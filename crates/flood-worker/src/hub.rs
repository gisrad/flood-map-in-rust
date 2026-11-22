use flood_shared::ai::{execute_ai_query, generate_briefing};
use flood_shared::calibration::{glofas_fetch_coords, marine_fetch_coords, station_calibration};
use flood_shared::catalog::{COASTAL_ROB_POINTS, RIVER_STATIONS};
use flood_shared::geo::build_stations_geojson;
use flood_shared::hydrology::{
    compute_kpis, evaluate_rob, evaluate_station, haversine_km, StationInputs, REPORT_RADIUS_KM,
};
use flood_shared::model::{
    AlertSubscriptionReq, Audience, BmkgNowcast, CitizenReport, CoastalRobTelemetry, ExceedProb,
    FloodSnapshot, HydrographPoint, IndicatorLevel, NewsItem, StationTelemetry, UpstreamHealth,
    METHODOLOGY_VERSION, SCHEMA_VERSION,
};
use flood_shared::reports::{parse_petabencana_geojson, PETABENCANA_URL};
use flood_shared::rss::{
    build_alerts_rss_xml, epoch_to_iso, merge_news, parse_bmkg_nowcast_rss, parse_flood_news,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use worker::*;

const SNAPSHOT_KEY: &str = "live-v2";
const STALE_MS: i64 = 20 * 60 * 1000;
const MANUAL_COOLDOWN_MS: i64 = 60 * 1000;
const PUSH_COOLDOWN_MS: i64 = 6 * 3600 * 1000;
const MIN_MS: i64 = 60 * 1000;

/// Jadwal per sumber. Cron `*/15` memicu ingest; sumber hanya diambil bila sudah jatuh tempo.
const GLOFAS_EVERY_MS: i64 = 6 * 3600 * 1000;
const WEATHER_EVERY_MS: i64 = 3600 * 1000;
const MARINE_EVERY_MS: i64 = 3600 * 1000;
const FAST_EVERY_MS: i64 = 15 * 60 * 1000;
/// Toleransi jitter cron.
const DUE_SLACK_MS: i64 = 2 * 60 * 1000;

const SUB_MAX_TOTAL: i64 = 1000;
const SUB_RATE_PER_HOUR: i64 = 5;
const USER_AGENT: &str = "HidrosentinelNusantara/0.2 (+https://flood.transbulent.id)";

const NEWS_FEEDS: &[(&str, &str)] = &[
    ("CNN Indonesia", "https://www.cnnindonesia.com/nasional/rss"),
    ("Tempo", "https://rss.tempo.co/nasional"),
    ("Republika", "https://www.republika.co.id/rss"),
    ("Antara", "https://www.antaranews.com/rss/terkini.xml"),
    ("detikNews", "https://news.detik.com/rss"),
];
const BMKG_NOWCAST_URL: &str = "https://www.bmkg.go.id/alerts/nowcast/id";

// ------------------------------------------------------------------ cache per sumber

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlofasSeries {
    dates: Vec<String>,
    q: Vec<Option<f64>>,
    qmin: Vec<Option<f64>>,
    p25: Vec<Option<f64>>,
    p75: Vec<Option<f64>>,
    qmax: Vec<Option<f64>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct GlofasCache {
    series: Vec<Option<GlofasSeries>>,
    exceed: Vec<Option<ExceedProb>>,
    ensemble_note: Option<String>,
}

/// JSON tidak punya NaN: serde_json menulis NaN sebagai `null`. Tanpa ini satu nilai
/// kosong (mis. gelombang di sel pesisir) menggagalkan seluruh cache sumber.
fn f64_or_nan<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::NAN))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WeatherPoint {
    #[serde(deserialize_with = "f64_or_nan")]
    rain_24h: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    rain_72h: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    rain_next_24h: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    soil_top: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    soil_deep: f64,
    daily: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MarinePoint {
    #[serde(deserialize_with = "f64_or_nan")]
    current: f64,
    #[serde(deserialize_with = "f64_or_nan")]
    peak: f64,
    peak_time: String,
    #[serde(deserialize_with = "f64_or_nan")]
    wave: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Glofas,
    Weather,
    Marine,
    PetaBencana,
    Bmkg,
    News,
}

impl Source {
    const ALL: [Source; 6] = [
        Source::Glofas,
        Source::Weather,
        Source::Marine,
        Source::PetaBencana,
        Source::Bmkg,
        Source::News,
    ];

    fn key(self) -> &'static str {
        match self {
            Source::Glofas => "glofas",
            Source::Weather => "weather",
            Source::Marine => "marine",
            Source::PetaBencana => "petabencana",
            Source::Bmkg => "bmkg",
            Source::News => "news",
        }
    }

    fn every_ms(self) -> i64 {
        match self {
            Source::Glofas => GLOFAS_EVERY_MS,
            Source::Weather => WEATHER_EVERY_MS,
            Source::Marine => MARINE_EVERY_MS,
            Source::PetaBencana | Source::Bmkg | Source::News => FAST_EVERY_MS,
        }
    }
}

struct Cached {
    fetched_at: String,
    fetched_ms: i64,
    attempted_ms: i64,
    ok: bool,
    payload: String,
}

#[durable_object]
pub struct FloodHub {
    state: State,
    #[allow(dead_code)]
    env: Env,
}

impl DurableObject for FloodHub {
    fn new(state: State, env: Env) -> Self {
        let sql = state.storage().sql();
        for ddl in [
            "CREATE TABLE IF NOT EXISTS snapshots (
                key TEXT PRIMARY KEY,
                updated_at TEXT NOT NULL,
                updated_epoch_ms INTEGER NOT NULL,
                payload_json TEXT NOT NULL
            );",
            "CREATE TABLE IF NOT EXISTS source_cache (
                source TEXT PRIMARY KEY,
                fetched_at TEXT NOT NULL,
                fetched_ms INTEGER NOT NULL,
                attempted_ms INTEGER NOT NULL,
                ok INTEGER NOT NULL,
                payload_json TEXT NOT NULL
            );",
            "CREATE TABLE IF NOT EXISTS ingest_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ran_at TEXT NOT NULL,
                stations_ok INTEGER NOT NULL,
                reports_ok INTEGER NOT NULL,
                rob_ok INTEGER NOT NULL,
                notes TEXT NOT NULL
            );",
            "CREATE TABLE IF NOT EXISTS subscriptions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                topic TEXT NOT NULL UNIQUE,
                region TEXT NOT NULL,
                min_level TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_sent_ms INTEGER NOT NULL DEFAULT 0
            );",
            "CREATE TABLE IF NOT EXISTS sub_rate (
                ip_hash TEXT NOT NULL,
                ts_ms INTEGER NOT NULL
            );",
            // Migrasi v1 → v2: level lama (Waspada/Siaga/Awas) dipetakan ke level indikasi baru.
            "UPDATE subscriptions SET min_level = 'high' WHERE min_level NOT IN ('high', 'very_high', 'extreme');",
            "DELETE FROM snapshots WHERE key <> 'live-v2';",
            "DELETE FROM ingest_log WHERE id NOT IN (SELECT id FROM ingest_log ORDER BY id DESC LIMIT 500);",
        ] {
            let _ = sql.exec(ddl, None);
        }
        Self { state, env }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let url = req.url()?;
        let path = url.path().to_string();

        match (req.method(), path.as_str()) {
            (Method::Post, "/internal/ingest") => {
                let snap = self.run_ingest().await?;
                self.ensure_alarm_scheduled().await;
                Response::from_json(&serde_json::json!({
                    "ok": true,
                    "generated_at_utc": snap.generated_at_utc,
                    "upstream": snap.upstream,
                }))
            }
            (Method::Post, "/api/ingest") => {
                let now_ms = now_ms();
                if let Some((_ts, last_ms, _)) = self.load_raw_snapshot()? {
                    if now_ms - last_ms < MANUAL_COOLDOWN_MS {
                        return Response::from_json(&serde_json::json!({
                            "ok": true,
                            "cached": true,
                            "message": "Ingest baru saja berjalan (<60 detik lalu)."
                        }));
                    }
                }
                let snap = self.run_ingest().await?;
                Response::from_json(&serde_json::json!({
                    "ok": true,
                    "cached": false,
                    "generated_at_utc": snap.generated_at_utc,
                    "upstream": snap.upstream,
                }))
            }
            (Method::Get, "/api/health") => self.health(),
            (Method::Get, "/api/flood") => {
                let snap = self.get_or_refresh_snapshot().await?;
                let mut resp = Response::from_json(&snap)?;
                resp.headers_mut()
                    .set("cache-control", "public, max-age=120")?;
                Ok(resp)
            }
            (Method::Get, "/api/stations.geojson") => {
                let snap = self.get_or_refresh_snapshot().await?;
                let body = serde_json::to_string(&build_stations_geojson(&snap))
                    .map_err(|e| Error::RustError(e.to_string()))?;
                let mut resp = Response::ok(body)?;
                let h = resp.headers_mut();
                h.set("content-type", "application/geo+json; charset=utf-8")?;
                h.set("cache-control", "public, max-age=300")?;
                h.set("access-control-allow-origin", "*")?;
                Ok(resp)
            }
            (Method::Get, p) if p.starts_with("/api/station/") => {
                let id = p.trim_start_matches("/api/station/").trim();
                let snap = self.get_or_refresh_snapshot().await?;
                if let Some(st) = snap.stations.iter().find(|s| s.id == id) {
                    Response::from_json(st)
                } else if let Some(rb) = snap.coastal_rob.iter().find(|r| r.id == id) {
                    Response::from_json(rb)
                } else {
                    Response::error("Stasiun tidak ditemukan", 404)
                }
            }
            (Method::Get, "/api/ai/briefing") => {
                let snap = self.get_or_refresh_snapshot().await?;
                let mut region = String::new();
                let mut aud = Audience::Public;
                let mut fmt = String::from("json");
                for (k, v) in url.query_pairs() {
                    match k.as_ref() {
                        "region" => region = v.chars().take(48).collect(),
                        "audience" => aud = Audience::from_str_lossy(&v),
                        "format" => fmt = v.into_owned(),
                        _ => {}
                    }
                }
                let briefing = generate_briefing(&snap, &region, aud);
                if fmt.eq_ignore_ascii_case("md") || fmt.eq_ignore_ascii_case("markdown") {
                    let mut resp = Response::ok(briefing.markdown)?;
                    resp.headers_mut()
                        .set("content-type", "text/markdown; charset=utf-8")?;
                    Ok(resp)
                } else {
                    Response::from_json(&briefing)
                }
            }
            (Method::Get, "/api/ai/query") => {
                let snap = self.get_or_refresh_snapshot().await?;
                let q: String = url
                    .query_pairs()
                    .find(|(k, _)| k == "q")
                    .map(|(_, v)| v.chars().take(200).collect())
                    .unwrap_or_default();
                Response::from_json(&execute_ai_query(&snap, &q))
            }
            (Method::Get, "/api/alerts.rss") => {
                let snap = self.get_or_refresh_snapshot().await?;
                let region: String = url
                    .query_pairs()
                    .find(|(k, _)| k == "region")
                    .map(|(_, v)| v.chars().take(48).collect())
                    .unwrap_or_default();
                let xml = build_alerts_rss_xml(&snap, &region, &base_url(&url));
                let mut resp = Response::ok(xml)?;
                let headers = resp.headers_mut();
                headers.set("content-type", "application/rss+xml; charset=utf-8")?;
                headers.set("cache-control", "public, max-age=180")?;
                Ok(resp)
            }
            (Method::Post, "/api/alerts/subscribe") => {
                let ip = req.headers().get("cf-connecting-ip")?.unwrap_or_default();
                let body: AlertSubscriptionReq = match req.json().await {
                    Ok(b) => b,
                    Err(_) => return Response::error("Payload JSON tidak valid", 400),
                };
                self.subscribe(body, &ip, &base_url(&url)).await
            }
            _ => Response::error("Not Found", 404),
        }
    }

    async fn alarm(&self) -> Result<Response> {
        let _ = self.run_ingest().await;
        self.ensure_alarm_scheduled().await;
        Response::ok("alarm-ok")
    }
}

impl FloodHub {
    async fn ensure_alarm_scheduled(&self) {
        let storage = self.state.storage();
        if let Ok(None) = storage.get_alarm().await {
            let _ = storage.set_alarm(now_ms() + FAST_EVERY_MS).await;
        }
    }

    fn load_raw_snapshot(&self) -> Result<Option<(String, i64, String)>> {
        #[derive(Deserialize)]
        struct Row {
            updated_at: String,
            updated_epoch_ms: i64,
            payload_json: String,
        }
        let rows: Vec<Row> = self
            .state
            .storage()
            .sql()
            .exec(
                "SELECT updated_at, updated_epoch_ms, payload_json FROM snapshots WHERE key = ? LIMIT 1;",
                Some(vec![SNAPSHOT_KEY.into()]),
            )?
            .to_array()?;
        Ok(rows
            .into_iter()
            .next()
            .map(|r| (r.updated_at, r.updated_epoch_ms, r.payload_json)))
    }

    fn load_source(&self, src: Source) -> Option<Cached> {
        #[derive(Deserialize)]
        struct Row {
            fetched_at: String,
            fetched_ms: i64,
            attempted_ms: i64,
            ok: i64,
            payload_json: String,
        }
        let rows: Vec<Row> = self
            .state
            .storage()
            .sql()
            .exec(
                "SELECT fetched_at, fetched_ms, attempted_ms, ok, payload_json FROM source_cache WHERE source = ? LIMIT 1;",
                Some(vec![src.key().into()]),
            )
            .and_then(|c| c.to_array())
            .ok()?;
        rows.into_iter().next().map(|r| Cached {
            fetched_at: r.fetched_at,
            fetched_ms: r.fetched_ms,
            attempted_ms: r.attempted_ms,
            ok: r.ok != 0,
            payload: r.payload_json,
        })
    }

    fn store_source(&self, src: Source, payload: Option<&str>, now: i64) {
        let sql = self.state.storage().sql();
        let _ = match payload {
            Some(p) => sql.exec(
                "INSERT INTO source_cache (source, fetched_at, fetched_ms, attempted_ms, ok, payload_json)
                 VALUES (?, ?, ?, ?, 1, ?)
                 ON CONFLICT(source) DO UPDATE SET fetched_at = excluded.fetched_at, fetched_ms = excluded.fetched_ms,
                   attempted_ms = excluded.attempted_ms, ok = 1, payload_json = excluded.payload_json;",
                Some(vec![
                    src.key().into(),
                    iso_from_ms(now).into(),
                    (now as f64).into(),
                    (now as f64).into(),
                    p.to_string().into(),
                ]),
            ),
            None => sql.exec(
                "UPDATE source_cache SET attempted_ms = ?, ok = 0 WHERE source = ?;",
                Some(vec![(now as f64).into(), src.key().into()]),
            ),
        };
    }

    async fn get_or_refresh_snapshot(&self) -> Result<FloodSnapshot> {
        if let Some((_ts, epoch_ms, json)) = self.load_raw_snapshot()? {
            if now_ms() - epoch_ms < STALE_MS {
                if let Ok(snap) = serde_json::from_str::<FloodSnapshot>(&json) {
                    return Ok(snap);
                }
            }
        }
        self.run_ingest().await
    }

    /// Ambil sumber yang jatuh tempo, simpan cache-nya, lalu bangun snapshot dari cache.
    async fn run_ingest(&self) -> Result<FloodSnapshot> {
        let now = now_ms();
        let mut notes: Vec<String> = Vec::new();

        for src in Source::ALL {
            let cached = self.load_source(src);
            let due = cached.as_ref().is_none_or(|c| {
                // Sumber yang gagal dicoba ulang paling cepat tiap 15 menit, bukan tiap permintaan.
                now - c.fetched_ms >= src.every_ms() - DUE_SLACK_MS
                    && now - c.attempted_ms >= src.every_ms().min(FAST_EVERY_MS) - DUE_SLACK_MS
            });
            if !due {
                continue;
            }
            match fetch_source(src, now).await {
                Ok(payload) => self.store_source(src, Some(&payload), now),
                Err(e) => {
                    notes.push(format!("{}: {e}", src.key()));
                    if cached.is_some() {
                        self.store_source(src, None, now);
                    } else {
                        // Catat percobaan agar tidak diulang tiap permintaan.
                        let _ = self.state.storage().sql().exec(
                            "INSERT OR IGNORE INTO source_cache (source, fetched_at, fetched_ms, attempted_ms, ok, payload_json)
                             VALUES (?, '', 0, ?, 0, 'null');",
                            Some(vec![src.key().into(), (now as f64).into()]),
                        );
                    }
                }
            }
        }

        let snapshot = self.build_snapshot(now, notes);
        let payload_json = serde_json::to_string(&snapshot)
            .map_err(|e| Error::RustError(format!("serialize error: {e}")))?;
        let sql = self.state.storage().sql();
        sql.exec(
            "INSERT INTO snapshots (key, updated_at, updated_epoch_ms, payload_json)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET updated_at = excluded.updated_at,
               updated_epoch_ms = excluded.updated_epoch_ms, payload_json = excluded.payload_json;",
            Some(vec![
                SNAPSHOT_KEY.into(),
                snapshot.generated_at_utc.clone().into(),
                (now as f64).into(),
                payload_json.into(),
            ]),
        )?;
        let _ = sql.exec(
            "INSERT INTO ingest_log (ran_at, stations_ok, reports_ok, rob_ok, notes) VALUES (?, ?, ?, ?, ?);",
            Some(vec![
                snapshot.generated_at_utc.clone().into(),
                (snapshot.stations.len() as i32).into(),
                (snapshot.citizen_reports.len() as i32).into(),
                (snapshot.coastal_rob.len() as i32).into(),
                snapshot.upstream.notes.join(" | ").into(),
            ]),
        );
        self.dispatch_ntfy_alerts(&snapshot, now).await;
        Ok(snapshot)
    }

    fn cached_value<T: DeserializeOwned>(
        &self,
        src: Source,
        max_age_ms: i64,
        now: i64,
        notes: &mut Vec<String>,
    ) -> (Option<T>, bool, Option<String>) {
        let Some(c) = self.load_source(src) else {
            return (None, false, None);
        };
        if c.fetched_at.is_empty() {
            return (None, false, None);
        }
        let age = now - c.fetched_ms;
        if age > max_age_ms {
            notes.push(format!(
                "{}: data terakhir {} menit lalu, terlalu lama untuk dipakai",
                src.key(),
                age / MIN_MS
            ));
            return (None, false, Some(c.fetched_at));
        }
        let value = serde_json::from_str::<T>(&c.payload).ok();
        let ok = c.ok && value.is_some();
        (value, ok, Some(c.fetched_at))
    }

    fn build_snapshot(&self, now: i64, mut notes: Vec<String>) -> FloodSnapshot {
        let (glofas, glofas_ok, glofas_at) =
            self.cached_value::<GlofasCache>(Source::Glofas, 3 * GLOFAS_EVERY_MS, now, &mut notes);
        let (weather, weather_ok, weather_at) = self.cached_value::<Vec<Option<WeatherPoint>>>(
            Source::Weather,
            4 * WEATHER_EVERY_MS,
            now,
            &mut notes,
        );
        let (marine, marine_ok, marine_at) = self.cached_value::<Vec<Option<MarinePoint>>>(
            Source::Marine,
            6 * MARINE_EVERY_MS,
            now,
            &mut notes,
        );
        let (reports, petabencana_ok, petabencana_at) = self.cached_value::<Vec<CitizenReport>>(
            Source::PetaBencana,
            2 * 3600 * 1000,
            now,
            &mut notes,
        );
        let (bmkg, bmkg_ok, bmkg_at) =
            self.cached_value::<Vec<BmkgNowcast>>(Source::Bmkg, 3 * 3600 * 1000, now, &mut notes);
        let (news, news_ok, news_at) =
            self.cached_value::<Vec<NewsItem>>(Source::News, 6 * 3600 * 1000, now, &mut notes);

        let reports = reports.unwrap_or_default();
        if let Some(n) = glofas.as_ref().and_then(|g| g.ensemble_note.clone()) {
            notes.push(n);
        }
        let today = iso_from_ms(now)[..10].to_string();

        let mut stations: Vec<StationTelemetry> = Vec::with_capacity(RIVER_STATIONS.len());
        for (idx, st) in RIVER_STATIONS.iter().enumerate() {
            let w = weather.as_ref().and_then(|v| v.get(idx).cloned().flatten());
            let series = glofas
                .as_ref()
                .and_then(|g| g.series.get(idx).cloned().flatten());
            let exceed = glofas
                .as_ref()
                .and_then(|g| g.exceed.get(idx).cloned().flatten());
            let (hydrograph, today_idx) = build_hydrograph(series.as_ref(), w.as_ref(), &today);
            let nearby = reports
                .iter()
                .filter(|r| haversine_km(st.lat, st.lon, r.lat, r.lon) <= REPORT_RADIUS_KM)
                .count() as u32;
            stations.push(evaluate_station(
                st,
                StationInputs {
                    hydrograph,
                    today_idx,
                    rain_24h_mm: w.as_ref().map(|w| w.rain_24h).unwrap_or(f64::NAN),
                    rain_72h_mm: w.as_ref().map(|w| w.rain_72h).unwrap_or(f64::NAN),
                    rain_next_24h_mm: w.as_ref().map(|w| w.rain_next_24h).unwrap_or(f64::NAN),
                    soil_top: w.as_ref().map(|w| w.soil_top).unwrap_or(f64::NAN),
                    soil_deep: w.as_ref().map(|w| w.soil_deep).unwrap_or(f64::NAN),
                    nearby_reports_24h: nearby,
                    exceed_prob_7d: exceed,
                },
            ));
        }

        let coastal_rob: Vec<CoastalRobTelemetry> = COASTAL_ROB_POINTS
            .iter()
            .enumerate()
            .map(|(idx, rb)| {
                let m = marine.as_ref().and_then(|v| v.get(idx).cloned().flatten());
                match m {
                    Some(m) => evaluate_rob(rb, m.current, m.peak, m.peak_time, m.wave),
                    None => evaluate_rob(rb, f64::NAN, f64::NAN, String::new(), f64::NAN),
                }
            })
            .collect();

        let bmkg_nowcasts = bmkg.unwrap_or_default();
        let kpis = compute_kpis(&stations, &coastal_rob, reports.len(), bmkg_nowcasts.len());
        FloodSnapshot {
            schema_version: SCHEMA_VERSION,
            methodology_version: METHODOLOGY_VERSION.to_string(),
            generated_at_utc: iso_from_ms(now),
            is_historical_replay: false,
            replay_id: None,
            replay_label: None,
            replay_citation: None,
            replay_note: None,
            kpis,
            upstream: UpstreamHealth {
                glofas_ok,
                weather_ok,
                marine_ok,
                petabencana_ok,
                bmkg_nowcast_ok: bmkg_ok,
                news_ok,
                notes,
                glofas_fetched_at_utc: glofas_at,
                weather_fetched_at_utc: weather_at,
                marine_fetched_at_utc: marine_at,
                petabencana_fetched_at_utc: petabencana_at,
                bmkg_fetched_at_utc: bmkg_at,
                news_fetched_at_utc: news_at,
            },
            stations,
            coastal_rob,
            citizen_reports: reports,
            bmkg_nowcasts,
            news: news.unwrap_or_default(),
        }
    }

    fn health(&self) -> Result<Response> {
        let now = now_ms();
        let mut sources = serde_json::Map::new();
        for src in Source::ALL {
            let entry = match self.load_source(src) {
                Some(c) if !c.fetched_at.is_empty() => serde_json::json!({
                    "fetched_at_utc": c.fetched_at,
                    "age_seconds": (now - c.fetched_ms) / 1000,
                    "last_attempt_ok": c.ok,
                    "interval_seconds": src.every_ms() / 1000,
                }),
                _ => serde_json::json!({
                    "fetched_at_utc": null,
                    "age_seconds": null,
                    "last_attempt_ok": false,
                    "interval_seconds": src.every_ms() / 1000,
                }),
            };
            sources.insert(src.key().to_string(), entry);
        }
        let row = self.load_raw_snapshot()?;
        let (updated_at, age_sec, counts) = match row {
            Some((ts, ms, json)) => {
                let snap: Option<FloodSnapshot> = serde_json::from_str(&json).ok();
                let counts = snap.map(|s| {
                    serde_json::json!({
                        "stations": s.stations.len(),
                        "coastal_rob": s.coastal_rob.len(),
                        "citizen_flood_reports": s.citizen_reports.len(),
                        "bmkg_nowcasts": s.bmkg_nowcasts.len(),
                        "news": s.news.len(),
                        "kpis": s.kpis,
                        "notes": s.upstream.notes,
                    })
                });
                (Some(ts), Some((now - ms) / 1000), counts)
            }
            None => (None, None, None),
        };
        Response::from_json(&serde_json::json!({
            "ok": true,
            "schema_version": SCHEMA_VERSION,
            "methodology_version": METHODOLOGY_VERSION,
            "snapshot_updated_at_utc": updated_at,
            "snapshot_age_seconds": age_sec,
            "sources": sources,
            "telemetry": counts,
        }))
    }

    async fn subscribe(
        &self,
        body: AlertSubscriptionReq,
        ip: &str,
        base: &str,
    ) -> Result<Response> {
        let topic = body.topic.trim().to_string();
        if !is_valid_ntfy_topic(&topic) {
            return Response::error(
                "Topik harus 8–64 karakter huruf, angka, '-' atau '_', dan tidak boleh berupa nomor telepon.",
                400,
            );
        }
        let min_level = if body.min_level.is_elevated() {
            body.min_level
        } else {
            IndicatorLevel::High
        };
        let region: String = if body.region.trim().is_empty() {
            "Nasional".to_string()
        } else {
            body.region.trim().chars().take(48).collect()
        };

        let now = now_ms();
        let sql = self.state.storage().sql();
        let _ = sql.exec(
            "DELETE FROM sub_rate WHERE ts_ms < ?;",
            Some(vec![((now - 3600 * 1000) as f64).into()]),
        );
        let ip_hash = hash_ip(ip, &iso_from_ms(now)[..10]);
        let recent = count(
            &sql,
            "SELECT COUNT(*) AS n FROM sub_rate WHERE ip_hash = ?;",
            vec![ip_hash.clone().into()],
        );
        if recent >= SUB_RATE_PER_HOUR {
            let mut resp = Response::error(
                "Terlalu banyak permintaan langganan. Coba lagi dalam satu jam.",
                429,
            )?;
            resp.headers_mut().set("retry-after", "3600")?;
            return Ok(resp);
        }
        let _ = sql.exec(
            "INSERT INTO sub_rate (ip_hash, ts_ms) VALUES (?, ?);",
            Some(vec![ip_hash.into(), (now as f64).into()]),
        );
        let exists = count(
            &sql,
            "SELECT COUNT(*) AS n FROM subscriptions WHERE topic = ?;",
            vec![topic.clone().into()],
        ) > 0;
        if !exists
            && count(&sql, "SELECT COUNT(*) AS n FROM subscriptions;", vec![]) >= SUB_MAX_TOTAL
        {
            return Response::error(
                "Kuota langganan penuh. Gunakan feed RSS sebagai alternatif.",
                503,
            );
        }
        sql.exec(
            "INSERT INTO subscriptions (topic, region, min_level, created_at, last_sent_ms)
             VALUES (?, ?, ?, ?, 0)
             ON CONFLICT(topic) DO UPDATE SET region = excluded.region, min_level = excluded.min_level;",
            Some(vec![
                topic.clone().into(),
                region.clone().into(),
                min_level.slug().into(),
                iso_from_ms(now).into(),
            ]),
        )?;

        let mut test_pushed = false;
        if body.send_test && !exists {
            let msg = format!(
                "Langganan aktif untuk {region}, level {} ke atas. Pesan ini berisi indikasi model, bukan peringatan resmi.",
                min_level.label_id().to_lowercase()
            );
            test_pushed = send_ntfy_push(
                &topic,
                "Hidrosentinel Nusantara: langganan aktif",
                &msg,
                "default",
            )
            .await
            .is_ok();
        }
        let rss_region = if region.eq_ignore_ascii_case("Nasional") {
            String::new()
        } else {
            format!("?region={}", urlencode(&region))
        };
        Response::from_json(&serde_json::json!({
            "ok": true,
            "topic": topic,
            "region": region,
            "min_level": min_level,
            "updated": exists,
            "test_pushed": test_pushed,
            "ntfy_url": format!("https://ntfy.sh/{topic}"),
            "rss_url": format!("{base}/api/alerts.rss{rss_region}"),
        }))
    }

    async fn dispatch_ntfy_alerts(&self, snapshot: &FloodSnapshot, now: i64) {
        #[derive(Deserialize)]
        struct SubRow {
            topic: String,
            region: String,
            min_level: String,
            last_sent_ms: i64,
        }
        let sql = self.state.storage().sql();
        let rows: Vec<SubRow> = match sql
            .exec(
                "SELECT topic, region, min_level, last_sent_ms FROM subscriptions WHERE last_sent_ms < ? ORDER BY last_sent_ms LIMIT 30;",
                Some(vec![((now - PUSH_COOLDOWN_MS) as f64).into()]),
            )
            .and_then(|c| c.to_array())
        {
            Ok(r) => r,
            Err(_) => return,
        };

        let mut sent = 0usize;
        for sub in rows {
            if sent >= 5 {
                break;
            }
            if now - sub.last_sent_ms < PUSH_COOLDOWN_MS {
                continue;
            }
            let min_lvl = IndicatorLevel::from_slug(&sub.min_level)
                .filter(|l| l.is_elevated())
                .unwrap_or(IndicatorLevel::High);
            let rf = sub.region.trim().to_lowercase();
            let region_match = |prov: &str, isl: &str| {
                rf.is_empty()
                    || rf == "nasional"
                    || rf == "indonesia"
                    || prov.to_lowercase().contains(&rf)
                    || isl.to_lowercase().contains(&rf)
            };
            let mut hits: Vec<String> = snapshot
                .stations
                .iter()
                .filter(|s| s.level >= min_lvl && region_match(&s.province, &s.island))
                .take(3)
                .map(|s| format!("[{}] {} — {}", s.level.label_id(), s.name, s.level_reason))
                .collect();
            hits.extend(
                snapshot
                    .coastal_rob
                    .iter()
                    .filter(|r| r.level >= min_lvl && region_match(&r.province, &r.island))
                    .take(2)
                    .map(|r| format!("[Rob · {}] {}", r.level.label_id(), r.name)),
            );
            if hits.is_empty() {
                continue;
            }
            let body = format!(
                "{}\nIndikasi model, bukan peringatan resmi. Ikuti BPBD dan BMKG setempat.\nhttps://flood.transbulent.id/",
                hits.join("\n")
            );
            let title = format!("Indikasi banjir ({})", sub.region);
            if send_ntfy_push(&sub.topic, &title, &body, "default")
                .await
                .is_ok()
            {
                let _ = sql.exec(
                    "UPDATE subscriptions SET last_sent_ms = ? WHERE topic = ?;",
                    Some(vec![(now as f64).into(), sub.topic.into()]),
                );
                sent += 1;
            }
        }
    }
}

// ------------------------------------------------------------------ pengambilan sumber

async fn fetch_source(src: Source, now: i64) -> std::result::Result<String, String> {
    match src {
        Source::Glofas => to_json(&fetch_glofas().await?),
        Source::Weather => to_json(&fetch_weather(now).await?),
        Source::Marine => to_json(&fetch_marine(now).await?),
        Source::PetaBencana => {
            let root: Value =
                serde_json::from_str(&fetch_text(PETABENCANA_URL, "application/json").await?)
                    .map_err(|e| format!("JSON: {e}"))?;
            to_json(&parse_petabencana_geojson(&root)?)
        }
        Source::Bmkg => {
            let xml = fetch_text(BMKG_NOWCAST_URL, "application/rss+xml, application/xml").await?;
            to_json(&parse_bmkg_nowcast_rss(&xml))
        }
        Source::News => {
            let mut all = Vec::new();
            let mut ok_any = false;
            let mut errs = Vec::new();
            for (label, url) in NEWS_FEEDS {
                match fetch_text(url, "application/rss+xml, application/xml, text/xml").await {
                    Ok(xml) => {
                        ok_any = true;
                        all.extend(parse_flood_news(&xml, label, now / 1000));
                    }
                    Err(e) => errs.push(format!("{label} {e}")),
                }
            }
            if !ok_any {
                return Err(errs.join("; "));
            }
            to_json(&merge_news(all, 16))
        }
    }
}

fn to_json<T: Serialize>(v: &T) -> std::result::Result<String, String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}

fn coord_list(coords: &[(f64, f64)]) -> (String, String) {
    let lats = coords
        .iter()
        .map(|c| format!("{:.4}", c.0))
        .collect::<Vec<_>>()
        .join(",");
    let lons = coords
        .iter()
        .map(|c| format!("{:.4}", c.1))
        .collect::<Vec<_>>()
        .join(",");
    (lats, lons)
}

fn arr(v: &Value, ptr: &str) -> Vec<Option<f64>> {
    v.pointer(ptr)
        .and_then(Value::as_array)
        .map(|a| a.iter().map(Value::as_f64).collect())
        .unwrap_or_default()
}

async fn fetch_glofas() -> std::result::Result<GlofasCache, String> {
    let coords: Vec<(f64, f64)> = RIVER_STATIONS
        .iter()
        .map(|s| glofas_fetch_coords(s.id, s.lat, s.lon))
        .collect();
    let (lats, lons) = coord_list(&coords);
    let url = format!(
        "https://flood-api.open-meteo.com/v1/flood?latitude={lats}&longitude={lons}&daily=river_discharge,river_discharge_min,river_discharge_p25,river_discharge_p75,river_discharge_max&past_days=14&forecast_days=7&cell_selection=nearest"
    );
    let items = fetch_json_list(&url).await?;
    if items.len() != RIVER_STATIONS.len() {
        return Err(format!("GloFAS mengembalikan {} lokasi", items.len()));
    }
    let series = items
        .iter()
        .map(|v| {
            let dates: Vec<String> = v
                .pointer("/daily/time")?
                .as_array()?
                .iter()
                .map(|t| t.as_str().unwrap_or_default().to_string())
                .collect();
            Some(GlofasSeries {
                q: arr(v, "/daily/river_discharge"),
                qmin: arr(v, "/daily/river_discharge_min"),
                p25: arr(v, "/daily/river_discharge_p25"),
                p75: arr(v, "/daily/river_discharge_p75"),
                qmax: arr(v, "/daily/river_discharge_max"),
                dates,
            })
        })
        .collect();

    // Probabilitas pelampauan dari seluruh anggota ensemble (bobot API ≈ 5 per lokasi; tiap 6 jam).
    let ens_url = format!(
        "https://flood-api.open-meteo.com/v1/flood?latitude={lats}&longitude={lons}&daily=river_discharge&forecast_days=7&ensemble=true&cell_selection=nearest"
    );
    let (exceed, ensemble_note) = match fetch_json_list(&ens_url).await {
        Ok(ens) if ens.len() == RIVER_STATIONS.len() => (
            RIVER_STATIONS
                .iter()
                .zip(ens.iter())
                .map(|(st, v)| exceed_probability(st.id, v))
                .collect(),
            None,
        ),
        Ok(ens) => (
            Vec::new(),
            Some(format!("ensemble GloFAS: {} lokasi", ens.len())),
        ),
        Err(e) => (Vec::new(), Some(format!("ensemble GloFAS: {e}"))),
    };
    Ok(GlofasCache {
        series,
        exceed,
        ensemble_note,
    })
}

fn exceed_probability(station_id: &str, v: &Value) -> Option<ExceedProb> {
    let th = station_calibration(station_id)?.thresholds()?;
    let daily = v.get("daily")?.as_object()?;
    let mut members = 0u32;
    let (mut n2, mut n5, mut n20) = (0u32, 0u32, 0u32);
    for (k, series) in daily {
        if !k.starts_with("river_discharge") {
            continue;
        }
        let peak = series
            .as_array()?
            .iter()
            .filter_map(Value::as_f64)
            .fold(f64::NAN, f64::max);
        if !peak.is_finite() {
            continue;
        }
        members += 1;
        n2 += u32::from(peak >= th.q2_m3s);
        n5 += u32::from(peak >= th.q5_m3s);
        n20 += u32::from(peak >= th.q20_m3s);
    }
    if members < 10 {
        return None;
    }
    let m = f64::from(members);
    let r = |n: u32| (f64::from(n) / m * 100.0).round() / 100.0;
    Some(ExceedProb {
        q2: r(n2),
        q5: r(n5),
        q20: r(n20),
        members,
    })
}

async fn fetch_weather(now: i64) -> std::result::Result<Vec<Option<WeatherPoint>>, String> {
    let coords: Vec<(f64, f64)> = RIVER_STATIONS.iter().map(|s| (s.lat, s.lon)).collect();
    let (lats, lons) = coord_list(&coords);
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lats}&longitude={lons}&hourly=precipitation,soil_moisture_0_to_7cm,soil_moisture_28_to_100cm&daily=precipitation_sum&past_days=14&forecast_days=7&timeformat=unixtime"
    );
    let items = fetch_json_list(&url).await?;
    if items.len() != RIVER_STATIONS.len() {
        return Err(format!("cuaca mengembalikan {} lokasi", items.len()));
    }
    let now_s = now / 1000;
    Ok(items.iter().map(|v| parse_weather(v, now_s)).collect())
}

fn parse_weather(v: &Value, now_s: i64) -> Option<WeatherPoint> {
    let times: Vec<i64> = v
        .pointer("/hourly/time")?
        .as_array()?
        .iter()
        .filter_map(Value::as_i64)
        .collect();
    let precip = arr(v, "/hourly/precipitation");
    let sm_top = arr(v, "/hourly/soil_moisture_0_to_7cm");
    let sm_deep = arr(v, "/hourly/soil_moisture_28_to_100cm");
    // Presipitasi per jam = jumlah satu jam sebelum stempel waktu.
    let sum_between = |from: i64, to: i64| -> f64 {
        times
            .iter()
            .zip(precip.iter())
            .filter(|(t, _)| **t > from && **t <= to)
            .filter_map(|(_, p)| *p)
            .sum()
    };
    let cur = times.iter().rposition(|t| *t <= now_s)?;
    let latest = |s: &[Option<f64>]| {
        s[..=cur.min(s.len().saturating_sub(1))]
            .iter()
            .rev()
            .find_map(|x| *x)
    };
    let daily_t = v
        .pointer("/daily/time")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let daily_p = arr(v, "/daily/precipitation_sum");
    let daily = daily_t
        .iter()
        .zip(daily_p.iter())
        .filter_map(|(t, p)| Some((epoch_to_iso(t.as_i64()?)[..10].to_string(), (*p)?)))
        .collect();
    Some(WeatherPoint {
        rain_24h: sum_between(now_s - 86_400, now_s),
        rain_72h: sum_between(now_s - 3 * 86_400, now_s),
        rain_next_24h: sum_between(now_s, now_s + 86_400),
        soil_top: latest(&sm_top).unwrap_or(f64::NAN),
        soil_deep: latest(&sm_deep).unwrap_or(f64::NAN),
        daily,
    })
}

async fn fetch_marine(now: i64) -> std::result::Result<Vec<Option<MarinePoint>>, String> {
    let coords: Vec<(f64, f64)> = COASTAL_ROB_POINTS
        .iter()
        .map(|r| marine_fetch_coords(r.id, r.lat, r.lon))
        .collect();
    let (lats, lons) = coord_list(&coords);
    let url = format!(
        "https://marine-api.open-meteo.com/v1/marine?latitude={lats}&longitude={lons}&hourly=sea_level_height_msl,wave_height&past_days=1&forecast_days=3&timeformat=unixtime&cell_selection=sea"
    );
    let items = fetch_json_list(&url).await?;
    if items.len() != COASTAL_ROB_POINTS.len() {
        return Err(format!("marine mengembalikan {} lokasi", items.len()));
    }
    let now_s = now / 1000;
    Ok(items.iter().map(|v| parse_marine(v, now_s)).collect())
}

fn parse_marine(v: &Value, now_s: i64) -> Option<MarinePoint> {
    let times: Vec<i64> = v
        .pointer("/hourly/time")?
        .as_array()?
        .iter()
        .filter_map(Value::as_i64)
        .collect();
    let msl = arr(v, "/hourly/sea_level_height_msl");
    let wave = arr(v, "/hourly/wave_height");
    let cur = times.iter().rposition(|t| *t <= now_s)?;
    let current = msl.get(cur).copied().flatten()?;
    let mut peak = current;
    let mut peak_t = times[cur];
    let mut wave_max = f64::NAN;
    for (i, t) in times.iter().enumerate() {
        if *t < times[cur] || *t > now_s + 48 * 3600 {
            continue;
        }
        if let Some(m) = msl.get(i).copied().flatten() {
            if m > peak {
                peak = m;
                peak_t = *t;
            }
        }
        if let Some(w) = wave.get(i).copied().flatten() {
            wave_max = if wave_max.is_finite() {
                wave_max.max(w)
            } else {
                w
            };
        }
    }
    Some(MarinePoint {
        current,
        peak,
        peak_time: epoch_to_iso(peak_t),
        wave: wave_max,
    })
}

fn build_hydrograph(
    series: Option<&GlofasSeries>,
    weather: Option<&WeatherPoint>,
    today: &str,
) -> (Vec<HydrographPoint>, usize) {
    let Some(s) = series else {
        return (Vec::new(), 0);
    };
    let rain_for =
        |d: &str| weather.and_then(|w| w.daily.iter().find(|(dd, _)| dd == d).map(|(_, r)| *r));
    let today_pos = s
        .dates
        .iter()
        .position(|d| d.as_str() == today)
        .or_else(|| s.dates.iter().rposition(|d| d.as_str() < today));
    let mut points = Vec::with_capacity(s.dates.len());
    let mut today_idx = 0usize;
    for (i, d) in s.dates.iter().enumerate() {
        let Some(q) = s.q.get(i).copied().flatten() else {
            continue;
        };
        let is_forecast = today_pos.is_some_and(|t| i > t);
        if today_pos == Some(i) {
            today_idx = points.len();
        }
        let ens = |v: &Vec<Option<f64>>| {
            if is_forecast {
                v.get(i).copied().flatten()
            } else {
                None
            }
        };
        points.push(HydrographPoint {
            date: d.clone(),
            is_forecast,
            discharge_m3s: q,
            discharge_min_m3s: ens(&s.qmin),
            discharge_p25_m3s: ens(&s.p25),
            discharge_p75_m3s: ens(&s.p75),
            discharge_max_m3s: ens(&s.qmax),
            rain_mm: rain_for(d),
        });
    }
    if today_pos.is_none() {
        today_idx = points.len().saturating_sub(1);
    }
    (points, today_idx)
}

// ------------------------------------------------------------------ utilitas

fn now_ms() -> i64 {
    Date::now().as_millis() as i64
}

fn iso_from_ms(ms: i64) -> String {
    epoch_to_iso(ms / 1000)
}

fn base_url(url: &Url) -> String {
    format!(
        "{}://{}",
        url.scheme(),
        url.host_str().unwrap_or("flood.transbulent.id")
    )
}

fn count(sql: &SqlStorage, q: &str, params: Vec<SqlStorageValue>) -> i64 {
    #[derive(Deserialize)]
    struct N {
        n: i64,
    }
    sql.exec(
        q,
        if params.is_empty() {
            None
        } else {
            Some(params)
        },
    )
    .and_then(|c| c.to_array::<N>())
    .ok()
    .and_then(|v| v.into_iter().next())
    .map(|r| r.n)
    .unwrap_or(0)
}

/// Hash IP + tanggal UTC. IP mentah tidak pernah disimpan; baris dihapus setelah 1 jam.
fn hash_ip(ip: &str, day: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"hidrosentinel-sub-rate|");
    h.update(day.as_bytes());
    h.update(b"|");
    h.update(ip.as_bytes());
    h.finalize()
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `^[A-Za-z0-9_-]{8,64}$`, dan bukan deret angka (mencegah nomor telepon sebagai topik).
fn is_valid_ntfy_topic(t: &str) -> bool {
    (8..=64).contains(&t.len())
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !t
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == '_')
}

async fn send_ntfy_push(
    topic: &str,
    title: &str,
    body: &str,
    priority: &str,
) -> std::result::Result<(), String> {
    let headers = Headers::new();
    let _ = headers.set("title", title);
    let _ = headers.set("priority", priority);
    let _ = headers.set("tags", "ocean");
    let _ = headers.set("click", "https://flood.transbulent.id/");
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(worker::wasm_bindgen::JsValue::from_str(body)));
    let url = format!("https://ntfy.sh/{topic}");
    let req = Request::new_with_init(&url, &init).map_err(|e| format!("{e:?}"))?;
    let resp = Fetch::Request(req)
        .send()
        .await
        .map_err(|e| format!("{e:?}"))?;
    if resp.status_code() >= 400 {
        Err(format!("ntfy HTTP {}", resp.status_code()))
    } else {
        Ok(())
    }
}

async fn fetch_text(url: &str, accept: &str) -> std::result::Result<String, String> {
    let headers = Headers::new();
    let _ = headers.set("accept", accept);
    let _ = headers.set("user-agent", USER_AGENT);
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let req = Request::new_with_init(url, &init).map_err(|e| format!("{e:?}"))?;
    let mut resp = Fetch::Request(req)
        .send()
        .await
        .map_err(|e| format!("{e:?}"))?;
    if resp.status_code() >= 400 {
        return Err(format!("HTTP {}", resp.status_code()));
    }
    resp.text().await.map_err(|e| format!("{e:?}"))
}

async fn fetch_json_list(url: &str) -> std::result::Result<Vec<Value>, String> {
    let text = fetch_text(url, "application/json").await?;
    match serde_json::from_str::<Value>(&text).map_err(|e| format!("JSON: {e}"))? {
        Value::Array(arr) => Ok(arr),
        obj @ Value::Object(_) => {
            if obj.get("error").and_then(Value::as_bool) == Some(true) {
                return Err(obj
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("error")
                    .to_string());
            }
            Ok(vec![obj])
        }
        _ => Err("bentuk JSON tidak dikenal".to_string()),
    }
}
