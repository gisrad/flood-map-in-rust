use flood_shared::ai::{execute_ai_query, generate_briefing};
use flood_shared::catalog::{COASTAL_ROB_POINTS, RIVER_STATIONS};
use flood_shared::hydrology::{compute_kpis, evaluate_rob, evaluate_station, haversine_km};
use flood_shared::model::{
    AlertLevel, AlertSubscriptionReq, Audience, CitizenReport, CoastalRobTelemetry, FloodSnapshot,
    HydrographPoint, NewsItem, StationTelemetry, UpstreamHealth,
};
use flood_shared::rss::{
    build_alerts_rss_xml, parse_bmkg_nowcast_rss, parse_filtered_indonesian_news,
};
use serde_json::Value;
use worker::*;

const SNAPSHOT_KEY: &str = "live-v1";
const STALE_MS: i64 = 20 * 60 * 1000;
const MANUAL_COOLDOWN_MS: i64 = 60 * 1000;
const PUSH_COOLDOWN_MS: i64 = 6 * 3600 * 1000;

#[durable_object]
pub struct FloodHub {
    state: State,
    #[allow(dead_code)]
    env: Env,
}

impl DurableObject for FloodHub {
    fn new(state: State, env: Env) -> Self {
        let sql = state.storage().sql();
        let _ = sql.exec(
            "CREATE TABLE IF NOT EXISTS snapshots (
                key TEXT PRIMARY KEY,
                updated_at TEXT NOT NULL,
                updated_epoch_ms INTEGER NOT NULL,
                payload_json TEXT NOT NULL
            );",
            None,
        );
        let _ = sql.exec(
            "CREATE TABLE IF NOT EXISTS ingest_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ran_at TEXT NOT NULL,
                stations_ok INTEGER NOT NULL,
                reports_ok INTEGER NOT NULL,
                rob_ok INTEGER NOT NULL,
                notes TEXT NOT NULL
            );",
            None,
        );
        let _ = sql.exec(
            "CREATE TABLE IF NOT EXISTS subscriptions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                topic TEXT NOT NULL UNIQUE,
                region TEXT NOT NULL,
                min_level TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_sent_ms INTEGER NOT NULL DEFAULT 0
            );",
            None,
        );
        Self { state, env }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let url = req.url()?;
        let path = url.path();

        match (req.method(), path) {
            (Method::Post, "/internal/ingest") => {
                let snap = self.run_ingest().await?;
                self.ensure_alarm_scheduled().await;
                Response::from_json(&serde_json::json!({
                    "ok": true,
                    "generated_at_utc": snap.generated_at_utc,
                    "stations": snap.stations.len(),
                    "rob": snap.coastal_rob.len(),
                    "reports": snap.citizen_reports.len(),
                    "bmkg_nowcasts": snap.bmkg_nowcasts.len(),
                    "news": snap.news.len(),
                }))
            }
            (Method::Post, "/api/ingest") => {
                let now_ms = Date::now().as_millis() as i64;
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
                    "stations": snap.stations.len(),
                    "reports": snap.citizen_reports.len(),
                    "bmkg_nowcasts": snap.bmkg_nowcasts.len(),
                    "news": snap.news.len(),
                }))
            }
            (Method::Get, "/api/health") => {
                let row = self.load_raw_snapshot()?;
                let (updated_at, age_sec, counts) = if let Some((ts, ms, json)) = row {
                    let now_ms = Date::now().as_millis() as i64;
                    let snap: Option<FloodSnapshot> = serde_json::from_str(&json).ok();
                    let counts = snap.as_ref().map(|s| {
                        serde_json::json!({
                            "stations": s.stations.len(),
                            "coastal_rob": s.coastal_rob.len(),
                            "citizen_reports": s.citizen_reports.len(),
                            "bmkg_nowcasts": s.bmkg_nowcasts.len(),
                            "news": s.news.len(),
                            "awas": s.kpis.awas_count,
                            "siaga": s.kpis.siaga_count,
                            "waspada": s.kpis.waspada_count,
                            "upstream": s.upstream,
                        })
                    });
                    (Some(ts), Some((now_ms - ms) / 1000), counts)
                } else {
                    (None, None, None)
                };
                Response::from_json(&serde_json::json!({
                    "ok": true,
                    "engine": "rust-worker-durable-object-sqlite",
                    "durable_object": "FloodHub",
                    "updated_at_utc": updated_at,
                    "age_seconds": age_sec,
                    "telemetry": counts,
                }))
            }
            (Method::Get, "/api/flood") => {
                let snap = self.get_or_refresh_snapshot().await?;
                let mut resp = Response::from_json(&snap)?;
                resp.headers_mut()
                    .set("cache-control", "public, max-age=120")?;
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
                    Response::error(format!("Station '{}' not found", id), 404)
                }
            }
            (Method::Get, "/api/ai/briefing") => {
                let snap = self.get_or_refresh_snapshot().await?;
                let mut region = String::new();
                let mut aud = Audience::Public;
                let mut fmt = String::from("json");
                for (k, v) in url.query_pairs() {
                    match k.as_ref() {
                        "region" => region = v.into_owned(),
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
                let q = url
                    .query_pairs()
                    .find(|(k, _)| k == "q")
                    .map(|(_, v)| v.into_owned())
                    .unwrap_or_default();
                let res = execute_ai_query(&snap, &q);
                Response::from_json(&res)
            }
            (Method::Get, "/api/alerts.rss") => {
                let snap = self.get_or_refresh_snapshot().await?;
                let region = url
                    .query_pairs()
                    .find(|(k, _)| k == "region")
                    .map(|(_, v)| v.into_owned())
                    .unwrap_or_default();
                let base = format!(
                    "{}://{}",
                    url.scheme(),
                    url.host_str().unwrap_or("flood.transbulent.id")
                );
                let xml = build_alerts_rss_xml(&snap, &region, &base);
                let mut resp = Response::ok(xml)?;
                let headers = resp.headers_mut();
                headers.set("content-type", "application/rss+xml; charset=utf-8")?;
                headers.set("cache-control", "public, max-age=180")?;
                Ok(resp)
            }
            (Method::Post, "/api/alerts/subscribe") => {
                let body: AlertSubscriptionReq = match req.json().await {
                    Ok(b) => b,
                    Err(_) => return Response::error("Payload JSON tidak valid", 400),
                };
                let topic = body.topic.trim().to_string();
                if !is_valid_ntfy_topic(&topic) {
                    return Response::error(
                        "Topic harus 4-48 karakter (huruf, angka, '-' atau '_'), jangan masukkan nomor HP/PII.",
                        400,
                    );
                }
                let region = if body.region.trim().is_empty() {
                    "Nasional".to_string()
                } else {
                    body.region.trim().chars().take(48).collect()
                };
                let min_level = body.min_level.label().to_string();
                let now_iso = js_now_iso();

                let sql = self.state.storage().sql();
                sql.exec(
                    "INSERT INTO subscriptions (topic, region, min_level, created_at, last_sent_ms)
                     VALUES (?, ?, ?, ?, 0)
                     ON CONFLICT(topic) DO UPDATE SET
                       region = excluded.region,
                       min_level = excluded.min_level,
                       created_at = excluded.created_at;",
                    Some(vec![
                        topic.clone().into(),
                        region.clone().into(),
                        min_level.clone().into(),
                        now_iso.into(),
                    ]),
                )?;

                let mut test_pushed = false;
                if body.send_test {
                    let msg = format!(
                        "Langganan aktif untuk wilayah {region} (ambang >= {min_level}). Anda akan menerima notifikasi gratis saat debit DAS / pasang rob meningkat."
                    );
                    test_pushed = send_ntfy_push(
                        &topic,
                        "Hidrosentinel Nusantara: Notifikasi Aktif",
                        &msg,
                        "default",
                    )
                    .await
                    .is_ok();
                }

                let rss_region = if region.eq_ignore_ascii_case("Nasional") {
                    String::new()
                } else {
                    format!("?region={region}")
                };

                Response::from_json(&serde_json::json!({
                    "ok": true,
                    "topic": topic,
                    "region": region,
                    "min_level": min_level,
                    "test_pushed": test_pushed,
                    "ntfy_url": format!("https://ntfy.sh/{}", body.topic.trim()),
                    "rss_url": format!("https://flood.transbulent.id/api/alerts.rss{rss_region}"),
                }))
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
            let next = (Date::now().as_millis() as i64) + 15 * 60 * 1000;
            let _ = storage.set_alarm(next).await;
        }
    }

    fn load_raw_snapshot(&self) -> Result<Option<(String, i64, String)>> {
        #[derive(serde::Deserialize)]
        struct Row {
            updated_at: String,
            updated_epoch_ms: i64,
            payload_json: String,
        }
        let sql = self.state.storage().sql();
        let rows: Vec<Row> = sql
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

    async fn get_or_refresh_snapshot(&self) -> Result<FloodSnapshot> {
        let now_ms = Date::now().as_millis() as i64;
        if let Some((_ts, epoch_ms, json)) = self.load_raw_snapshot()? {
            if now_ms - epoch_ms < STALE_MS {
                if let Ok(snap) = serde_json::from_str::<FloodSnapshot>(&json) {
                    return Ok(snap);
                }
            }
        }
        self.run_ingest().await
    }

    async fn run_ingest(&self) -> Result<FloodSnapshot> {
        let now_ms = Date::now().as_millis() as i64;
        let now_iso = js_now_iso();
        let mut notes = Vec::new();

        // 1. Fetch GloFAS river discharge in 2 batches (25 + 25 = 50 stations)
        let mut glofas_items: Vec<Option<Value>> = vec![None; RIVER_STATIONS.len()];
        let mut glofas_ok = true;
        for chunk_idx in 0..2 {
            let start = chunk_idx * 25;
            let end = ((chunk_idx + 1) * 25).min(RIVER_STATIONS.len());
            let slice = &RIVER_STATIONS[start..end];
            let lats = slice
                .iter()
                .map(|s| format!("{:.4}", s.lat))
                .collect::<Vec<_>>()
                .join(",");
            let lons = slice
                .iter()
                .map(|s| format!("{:.4}", s.lon))
                .collect::<Vec<_>>()
                .join(",");
            let url = format!(
                "https://flood-api.open-meteo.com/v1/flood?latitude={lats}&longitude={lons}&daily=river_discharge,river_discharge_mean,river_discharge_max,river_discharge_p75&past_days=14&forecast_days=7"
            );
            match fetch_json_list(&url).await {
                Ok(items) => {
                    for (offset, item) in items.into_iter().enumerate() {
                        if start + offset < glofas_items.len() {
                            glofas_items[start + offset] = Some(item);
                        }
                    }
                }
                Err(e) => {
                    glofas_ok = false;
                    notes.push(format!("GloFAS batch {chunk_idx}: {e}"));
                }
            }
        }

        // 2. Fetch Open-Meteo Weather (rain + soil moisture) in 2 batches
        let mut weather_items: Vec<Option<Value>> = vec![None; RIVER_STATIONS.len()];
        let mut weather_ok = true;
        for chunk_idx in 0..2 {
            let start = chunk_idx * 25;
            let end = ((chunk_idx + 1) * 25).min(RIVER_STATIONS.len());
            let slice = &RIVER_STATIONS[start..end];
            let lats = slice
                .iter()
                .map(|s| format!("{:.4}", s.lat))
                .collect::<Vec<_>>()
                .join(",");
            let lons = slice
                .iter()
                .map(|s| format!("{:.4}", s.lon))
                .collect::<Vec<_>>()
                .join(",");
            let url = format!(
                "https://api.open-meteo.com/v1/forecast?latitude={lats}&longitude={lons}&daily=precipitation_sum&hourly=soil_moisture_0_to_7cm,soil_moisture_28_to_100cm&past_days=3&forecast_days=3&timezone=Asia%2FJakarta"
            );
            match fetch_json_list(&url).await {
                Ok(items) => {
                    for (offset, item) in items.into_iter().enumerate() {
                        if start + offset < weather_items.len() {
                            weather_items[start + offset] = Some(item);
                        }
                    }
                }
                Err(e) => {
                    weather_ok = false;
                    notes.push(format!("Weather batch {chunk_idx}: {e}"));
                }
            }
        }

        // 3. Fetch Open-Meteo Marine for 12 coastal Rob corridors
        let mut marine_ok = true;
        let rob_lats = COASTAL_ROB_POINTS
            .iter()
            .map(|r| format!("{:.4}", r.lat))
            .collect::<Vec<_>>()
            .join(",");
        let rob_lons = COASTAL_ROB_POINTS
            .iter()
            .map(|r| format!("{:.4}", r.lon))
            .collect::<Vec<_>>()
            .join(",");
        let marine_url = format!(
            "https://marine-api.open-meteo.com/v1/marine?latitude={rob_lats}&longitude={rob_lons}&hourly=sea_level_height_msl,wave_height&forecast_days=2&timezone=Asia%2FJakarta"
        );
        let marine_items = match fetch_json_list(&marine_url).await {
            Ok(v) => v,
            Err(e) => {
                marine_ok = false;
                notes.push(format!("Marine Rob: {e}"));
                Vec::new()
            }
        };

        // 4. Fetch PetaBencana 24h reports (graceful degradation)
        let (citizen_reports, petabencana_ok) = match fetch_petabencana().await {
            Ok(r) => (r, true),
            Err(e) => {
                notes.push(format!("PetaBencana fallback: {e}"));
                (Vec::new(), false)
            }
        };

        // 5. Fetch BMKG CAP Nowcast RSS & Google News Indonesia Flood RSS (non-fatal)
        let (bmkg_nowcasts, bmkg_nowcast_ok) = match fetch_text("https://www.bmkg.go.id/alerts/nowcast/id").await {
            Ok(xml) => (parse_bmkg_nowcast_rss(&xml), true),
            Err(e) => {
                notes.push(format!("BMKG Nowcast fallback: {e}"));
                (Vec::new(), false)
            }
        };

        let (news, news_ok) = fetch_indonesian_flood_news(&mut notes).await;

        // 6. Evaluate all 50 river stations
        let mut stations: Vec<StationTelemetry> = Vec::with_capacity(RIVER_STATIONS.len());
        for (idx, st) in RIVER_STATIONS.iter().enumerate() {
            let (rain_24h, rain_72h, sm_top, sm_deep, daily_rain_map) =
                parse_weather_item(weather_items.get(idx).and_then(|o| o.as_ref()));

            let (hydrograph, today_idx) = parse_glofas_item(
                glofas_items.get(idx).and_then(|o| o.as_ref()),
                &daily_rain_map,
                st.baseline_q75_hint,
            );

            let nearby_reports = citizen_reports
                .iter()
                .filter(|r| haversine_km(st.lat, st.lon, r.lat, r.lon) <= 28.0)
                .count() as u32;

            stations.push(evaluate_station(
                st,
                hydrograph,
                today_idx,
                rain_24h,
                rain_72h,
                sm_top,
                sm_deep,
                nearby_reports,
            ));
        }

        // 7. Evaluate all 12 coastal Rob corridors
        let mut coastal_rob: Vec<CoastalRobTelemetry> = Vec::with_capacity(COASTAL_ROB_POINTS.len());
        for (idx, rb) in COASTAL_ROB_POINTS.iter().enumerate() {
            let (cur_msl, peak_msl, peak_time, wave_h) = parse_marine_item(marine_items.get(idx));
            coastal_rob.push(evaluate_rob(rb, cur_msl, peak_msl, peak_time, wave_h));
        }

        let kpis = compute_kpis(&stations, &coastal_rob, citizen_reports.len());
        let snapshot = FloodSnapshot {
            generated_at_utc: now_iso.clone(),
            is_historical_replay: false,
            replay_id: None,
            replay_label: None,
            replay_citation: None,
            kpis,
            upstream: UpstreamHealth {
                glofas_ok,
                weather_ok,
                marine_ok,
                petabencana_ok,
                bmkg_nowcast_ok,
                news_ok,
                notes: notes.clone(),
            },
            stations,
            coastal_rob,
            citizen_reports,
            bmkg_nowcasts,
            news,
        };

        let payload_json = serde_json::to_string(&snapshot)
            .map_err(|e| Error::RustError(format!("serialize error: {e}")))?;

        let sql = self.state.storage().sql();
        sql.exec(
            "INSERT INTO snapshots (key, updated_at, updated_epoch_ms, payload_json)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET
               updated_at = excluded.updated_at,
               updated_epoch_ms = excluded.updated_epoch_ms,
               payload_json = excluded.payload_json;",
            Some(vec![
                SNAPSHOT_KEY.into(),
                now_iso.clone().into(),
                (now_ms as f64).into(),
                payload_json.into(),
            ]),
        )?;
        let _ = sql.exec(
            "INSERT INTO ingest_log (ran_at, stations_ok, reports_ok, rob_ok, notes)
             VALUES (?, ?, ?, ?, ?);",
            Some(vec![
                now_iso.into(),
                (snapshot.stations.len() as i32).into(),
                (snapshot.citizen_reports.len() as i32).into(),
                (snapshot.coastal_rob.len() as i32).into(),
                notes.join(" | ").into(),
            ]),
        );

        // 8. Dispatch deduplicated free ntfy.sh alerts (capped at 5 per ingest)
        self.dispatch_ntfy_alerts(&snapshot, now_ms).await;

        Ok(snapshot)
    }

    async fn dispatch_ntfy_alerts(&self, snapshot: &FloodSnapshot, now_ms: i64) {
        #[derive(serde::Deserialize)]
        struct SubRow {
            topic: String,
            region: String,
            min_level: String,
            last_sent_ms: i64,
        }
        let sql = self.state.storage().sql();
        let rows: Vec<SubRow> = match sql
            .exec(
                "SELECT topic, region, min_level, last_sent_ms FROM subscriptions LIMIT 30;",
                None,
            )
            .and_then(|c| c.to_array())
        {
            Ok(r) => r,
            Err(_) => return,
        };

        let mut sent_count = 0usize;
        for sub in rows {
            if sent_count >= 5 {
                break;
            }
            if now_ms - sub.last_sent_ms < PUSH_COOLDOWN_MS {
                continue;
            }
            let min_lvl = parse_alert_level(&sub.min_level);
            let rf = sub.region.trim().to_ascii_lowercase();
            let region_match = |prov: &str, isl: &str| {
                rf.is_empty()
                    || rf == "nasional"
                    || rf == "indonesia"
                    || prov.to_ascii_lowercase().contains(&rf)
                    || isl.to_ascii_lowercase().contains(&rf)
            };

            let mut hits: Vec<String> = snapshot
                .stations
                .iter()
                .filter(|s| s.alert_level >= min_lvl && region_match(&s.province, &s.island))
                .take(3)
                .map(|s| {
                    format!(
                        "[{}] {} ({:.1} m3/s · {:.1}x Q75)",
                        s.alert_level.label(),
                        s.name,
                        s.current_discharge_m3s,
                        s.anomaly_ratio
                    )
                })
                .collect();

            for r in snapshot
                .coastal_rob
                .iter()
                .filter(|r| r.alert_level >= min_lvl && region_match(&r.province, &r.island))
                .take(2)
            {
                hits.push(format!(
                    "[Rob {}] {} (Indeks {:.2}x)",
                    r.alert_level.label(),
                    r.name,
                    r.rob_index
                ));
            }

            if hits.is_empty() {
                continue;
            }

            let body = format!(
                "{}\nPantau detail 3D & hidrograf: https://flood.transbulent.id/",
                hits.join("\n")
            );
            let title = format!("Peringatan Banjir ({})", sub.region);
            if send_ntfy_push(&sub.topic, &title, &body, "high").await.is_ok() {
                let _ = sql.exec(
                    "UPDATE subscriptions SET last_sent_ms = ? WHERE topic = ?;",
                    Some(vec![(now_ms as f64).into(), sub.topic.into()]),
                );
                sent_count += 1;
            }
        }
    }
}

fn parse_alert_level(s: &str) -> AlertLevel {
    match s.trim().to_ascii_lowercase().as_str() {
        "awas" => AlertLevel::Awas,
        "siaga" => AlertLevel::Siaga,
        "waspada" => AlertLevel::Waspada,
        _ => AlertLevel::Waspada,
    }
}

fn is_valid_ntfy_topic(t: &str) -> bool {
    if !(4..=48).contains(&t.len()) {
        return false;
    }
    if t.chars().all(|c| c.is_ascii_digit() || c == '+' || c == '-') {
        // Reject phone-number-like topics to prevent PII storage
        return false;
    }
    t.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
    let _ = headers.set("tags", "ocean,warning");
    let _ = headers.set("click", "https://flood.transbulent.id/");
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(worker::wasm_bindgen::JsValue::from_str(body)));
    let url = format!("https://ntfy.sh/{topic}");
    let req = Request::new_with_init(&url, &init).map_err(|e| format!("{e:?}"))?;
    let resp = Fetch::Request(req).send().await.map_err(|e| format!("{e:?}"))?;
    if resp.status_code() >= 400 {
        Err(format!("ntfy HTTP {}", resp.status_code()))
    } else {
        Ok(())
    }
}

async fn fetch_text(url: &str) -> std::result::Result<String, String> {
    let headers = Headers::new();
    let _ = headers.set("accept", "application/rss+xml, application/xml, text/xml, */*");
    let _ = headers.set("user-agent", "HidrosentinelNusantara/0.1 (flood.transbulent.id)");
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let req = Request::new_with_init(url, &init).map_err(|e| format!("{e:?}"))?;
    let mut resp = Fetch::Request(req).send().await.map_err(|e| format!("{e:?}"))?;
    if resp.status_code() >= 400 {
        return Err(format!("HTTP {}", resp.status_code()));
    }
    resp.text().await.map_err(|e| format!("{e:?}"))
}

async fn fetch_json_list(url: &str) -> std::result::Result<Vec<Value>, String> {
    let headers = Headers::new();
    let _ = headers.set("accept", "application/json");
    let _ = headers.set("user-agent", "HidrosentinelNusantara/0.1 (flood.transbulent.id)");
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let req = Request::new_with_init(url, &init).map_err(|e| format!("{e:?}"))?;
    let mut resp = Fetch::Request(req).send().await.map_err(|e| format!("{e:?}"))?;
    if resp.status_code() >= 400 {
        return Err(format!("HTTP {}", resp.status_code()));
    }
    let val: Value = resp.json().await.map_err(|e| format!("{e:?}"))?;
    match val {
        Value::Array(arr) => Ok(arr),
        obj @ Value::Object(_) => Ok(vec![obj]),
        _ => Err("Unexpected JSON shape".to_string()),
    }
}

async fn fetch_petabencana() -> std::result::Result<Vec<CitizenReport>, String> {
    let headers = Headers::new();
    let _ = headers.set("accept", "application/json");
    let _ = headers.set("user-agent", "HidrosentinelNusantara/0.1 (flood.transbulent.id)");
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let req = Request::new_with_init(
        "https://data.petabencana.id/reports?timeperiod=86400",
        &init,
    )
    .map_err(|e| format!("{e:?}"))?;
    let mut resp = Fetch::Request(req).send().await.map_err(|e| format!("{e:?}"))?;
    if resp.status_code() >= 400 {
        return Err(format!("HTTP {}", resp.status_code()));
    }
    let root: Value = resp.json().await.map_err(|e| format!("{e:?}"))?;
    let geoms = root
        .pointer("/result/objects/output/geometries")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Missing PetaBencana geometries".to_string())?;

    let mut out = Vec::new();
    for g in geoms {
        let coords = g.get("coordinates").and_then(|c| c.as_array());
        let (lon, lat) = match coords {
            Some(c) if c.len() >= 2 => match (c[0].as_f64(), c[1].as_f64()) {
                (Some(x), Some(y)) => (x, y),
                _ => continue,
            },
            _ => continue,
        };
        if !(-11.5..=6.5).contains(&lat) || !(94.5..=141.5).contains(&lon) {
            continue;
        }
        let props = match g.get("properties") {
            Some(p) => p,
            None => continue,
        };
        let pkey = props
            .get("pkey")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if pkey.is_empty() {
            continue;
        }
        let depth_cm = props
            .pointer("/report_data/flood_depth")
            .and_then(|v| v.as_u64())
            .map(|d| d as u32);
        out.push(CitizenReport {
            pkey,
            created_at: props
                .get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source: props
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("warga")
                .to_string(),
            status: props
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("confirmed")
                .to_string(),
            depth_cm,
            title: props
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Laporan Banjir Warga")
                .to_string(),
            text: props
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(220)
                .collect(),
            lat,
            lon,
        });
    }
    Ok(out)
}

fn parse_weather_item(
    item: Option<&Value>,
) -> (f64, f64, f64, f64, std::collections::HashMap<String, f64>) {
    let mut rain_map = std::collections::HashMap::new();
    let Some(v) = item else {
        return (0.0, 0.0, 0.25, 0.25, rain_map);
    };
    let times = v
        .pointer("/daily/time")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let rains = v
        .pointer("/daily/precipitation_sum")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    let mut rain_vals = Vec::new();
    for (t, r) in times.iter().zip(rains.iter()) {
        if let (Some(ts), Some(rv)) = (t.as_str(), r.as_f64()) {
            rain_map.insert(ts.to_string(), rv);
            rain_vals.push(rv);
        }
    }

    let today_pos = 3.min(rain_vals.len().saturating_sub(1));
    let rain_24h = rain_vals.get(today_pos).copied().unwrap_or(0.0);
    let start_72 = today_pos.saturating_sub(2);
    let rain_72h: f64 = rain_vals[start_72..=today_pos].iter().sum();

    let sm_top = v
        .pointer("/hourly/soil_moisture_0_to_7cm")
        .and_then(|x| x.as_array())
        .and_then(|arr| {
            arr.get(72.min(arr.len().saturating_sub(1)))
                .and_then(|n| n.as_f64())
                .or_else(|| arr.iter().rev().find_map(|n| n.as_f64()))
        })
        .unwrap_or(0.28);

    let sm_deep = v
        .pointer("/hourly/soil_moisture_28_to_100cm")
        .and_then(|x| x.as_array())
        .and_then(|arr| {
            arr.get(72.min(arr.len().saturating_sub(1)))
                .and_then(|n| n.as_f64())
                .or_else(|| arr.iter().rev().find_map(|n| n.as_f64()))
        })
        .unwrap_or(0.28);

    (rain_24h, rain_72h, sm_top, sm_deep, rain_map)
}

fn parse_glofas_item(
    item: Option<&Value>,
    rain_map: &std::collections::HashMap<String, f64>,
    q75_hint: f64,
) -> (Vec<HydrographPoint>, usize) {
    let Some(v) = item else {
        return (Vec::new(), 0);
    };
    let times = v
        .pointer("/daily/time")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let q_arr = v
        .pointer("/daily/river_discharge")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let q_mean = v
        .pointer("/daily/river_discharge_mean")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let q_max = v
        .pointer("/daily/river_discharge_max")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let q_p75 = v
        .pointer("/daily/river_discharge_p75")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    let today_idx = 14.min(times.len().saturating_sub(1));
    let mut points = Vec::with_capacity(times.len());
    for (i, t) in times.iter().enumerate() {
        let date = t.as_str().unwrap_or("").to_string();
        let q = q_arr.get(i).and_then(|n| n.as_f64()).unwrap_or(q75_hint * 0.5);
        let qm = q_mean.get(i).and_then(|n| n.as_f64()).unwrap_or(q);
        let qx = q_max.get(i).and_then(|n| n.as_f64()).unwrap_or(qm * 1.15);
        let qp = q_p75.get(i).and_then(|n| n.as_f64()).unwrap_or(q75_hint);
        let rain_mm = rain_map.get(&date).copied().unwrap_or(0.0);
        points.push(HydrographPoint {
            date,
            is_forecast: i > today_idx,
            discharge_m3s: q,
            discharge_mean_m3s: qm,
            discharge_max_m3s: qx,
            discharge_p75_m3s: qp,
            rain_mm,
        });
    }
    (points, today_idx)
}

fn parse_marine_item(item: Option<&Value>) -> (f64, f64, String, f64) {
    let Some(v) = item else {
        return (0.20, 0.35, String::from("-"), 0.50);
    };
    let times = v
        .pointer("/hourly/time")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let msls = v
        .pointer("/hourly/sea_level_height_msl")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let waves = v
        .pointer("/hourly/wave_height")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    let cur_msl = msls.first().and_then(|n| n.as_f64()).unwrap_or(0.20);
    let cur_wave = waves
        .iter()
        .filter_map(|n| n.as_f64())
        .fold(0.0_f64, f64::max)
        .max(0.2);

    let mut peak_msl = cur_msl;
    let mut peak_time = times
        .first()
        .and_then(|t| t.as_str())
        .unwrap_or("-")
        .to_string();
    for (t, m) in times.iter().zip(msls.iter()) {
        if let Some(val) = m.as_f64() {
            if val > peak_msl {
                peak_msl = val;
                peak_time = t.as_str().unwrap_or("-").to_string();
            }
        }
    }
    (cur_msl, peak_msl, peak_time, cur_wave)
}

async fn fetch_indonesian_flood_news(notes: &mut Vec<String>) -> (Vec<NewsItem>, bool) {
    let feeds = [
        ("CNN Indonesia", "https://www.cnnindonesia.com/nasional/rss"),
        ("Tempo.co", "https://rss.tempo.co/nasional"),
        ("Republika", "https://www.republika.co.id/rss"),
    ];
    let mut matched = Vec::new();
    let mut fallback = Vec::new();
    let mut seen_titles = std::collections::HashSet::new();
    let mut any_ok = false;

    for (label, url) in feeds {
        match fetch_text(url).await {
            Ok(xml) => {
                any_ok = true;
                let (m, fb) = parse_filtered_indonesian_news(&xml, label);
                for item in m {
                    let key = item.title.to_ascii_lowercase();
                    if seen_titles.insert(key) {
                        matched.push(item);
                    }
                }
                for item in fb {
                    let key = item.title.to_ascii_lowercase();
                    if seen_titles.insert(key) {
                        fallback.push(item);
                    }
                }
            }
            Err(e) => notes.push(format!("{label} RSS: {e}")),
        }
    }

    if matched.is_empty() {
        matched = fallback;
    }
    matched.truncate(16);
    (matched, any_ok)
}

fn js_now_iso() -> String {
    worker::js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string())
}
