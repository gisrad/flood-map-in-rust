use crate::bridge::{copy_text, download_text_file, js_fly_to_point, open_whatsapp_text};
use crate::components::about::AboutPane;
use crate::components::station_detail::{
    geolibre_url, lead_days, lead_label, RobDetail, StationDetail,
};
use crate::components::ui::{use_toast, Disclaimer, LevelBadge, OfficialRefs, Shape, ShapeGlyph};
use crate::format::{
    fmt_datetime_short, fmt_day, fmt_num, fmt_pct, fmt_q, fmt_ratio, fmt_relative, parse_utc_ms,
};
use crate::status::{status_body, status_stamp};
use flood_shared::ai::generate_briefing;
use flood_shared::model::{
    AiQueryResponse, AlertSubscriptionReq, Audience, FloodMechanism, FloodSnapshot, IndicatorLevel,
};
use gloo_net::http::Request;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Summary,
    News,
    Notify,
    About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetSnap {
    Peek,
    Half,
    Full,
}

impl SheetSnap {
    pub fn class_name(self) -> &'static str {
        match self {
            Self::Peek => "snap-peek",
            Self::Half => "snap-half",
            Self::Full => "snap-full",
        }
    }

    pub fn next_cycle(self) -> Self {
        match self {
            Self::Peek => Self::Half,
            Self::Half => Self::Full,
            Self::Full => Self::Peek,
        }
    }
}

pub fn lens_slug(a: Audience) -> &'static str {
    match a {
        Audience::Public => "publik",
        Audience::Gov => "bpbd",
        Audience::Academia => "riset",
        Audience::AiAgent => "ai",
    }
}

fn lens_label(a: Audience) -> &'static str {
    match a {
        Audience::Public => "Publik",
        Audience::Gov => "BPBD/Pemda",
        Audience::Academia => "Riset",
        Audience::AiAgent => "AI",
    }
}

/// Satu baris titik pantau (sungai atau rob) untuk daftar dan tabel.
#[derive(Clone)]
struct PointRow {
    id: String,
    name: String,
    sub: String,
    level: IndicatorLevel,
    shape: Shape,
    lon: f64,
    lat: f64,
    sort: f64,
}

fn point_rows(snap: &FloodSnapshot, filter: Option<FloodMechanism>) -> Vec<PointRow> {
    let mut rows = Vec::new();
    if filter != Some(FloodMechanism::CoastalRob) {
        for s in &snap.stations {
            let (level, shape) = match filter {
                Some(FloodMechanism::FluvialBasin) => (s.fluvial_level, Shape::Circle),
                Some(FloodMechanism::UrbanPluvial) => (s.pluvial_level, Shape::Square),
                _ => (
                    s.level,
                    if s.mechanism == FloodMechanism::UrbanPluvial {
                        Shape::Square
                    } else {
                        Shape::Circle
                    },
                ),
            };
            rows.push(PointRow {
                id: s.id.clone(),
                name: s.name.clone(),
                sub: format!("{} · {}", s.province, s.mechanism.label_id()),
                level,
                shape,
                lon: s.lon,
                lat: s.lat,
                sort: s.ratio_q2.unwrap_or(0.0),
            });
        }
    }
    if matches!(filter, None | Some(FloodMechanism::CoastalRob)) {
        for r in &snap.coastal_rob {
            rows.push(PointRow {
                id: r.id.clone(),
                name: r.name.clone(),
                sub: format!("{} · Rob pesisir", r.province),
                level: r.level,
                shape: Shape::Diamond,
                lon: r.lon,
                lat: r.lat,
                sort: 0.0,
            });
        }
    }
    rows.sort_by(|a, b| {
        b.level
            .rank()
            .cmp(&a.level.rank())
            .then(
                b.sort
                    .partial_cmp(&a.sort)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.name.cmp(&b.name))
    });
    rows
}

#[component]
fn PointList(rows: Vec<PointRow>, on_select: Callback<(String, f64, f64)>) -> impl IntoView {
    view! {
        <ul class="point-list">
            {rows.into_iter().map(|r| {
                let PointRow { id, name, sub, level, shape, lon, lat, .. } = r;
                view! {
                    <li>
                        <button type="button" class="point-row" on:click=move |_| on_select.run((id.clone(), lon, lat))>
                            <ShapeGlyph shape=shape level=level/>
                            <span class="pr-main">
                                <b>{name}</b>
                                <small>{sub}</small>
                            </span>
                            <LevelBadge level=level/>
                        </button>
                    </li>
                }
            }).collect_view()}
        </ul>
    }
}

fn kpi_strip(snap: &FloodSnapshot) -> impl IntoView {
    let k = &snap.kpis;
    // Laporan warga tidak dibakukan untuk replay; kartu nol akan terbaca sebagai fakta.
    let replay = snap.is_historical_replay;
    view! {
        <dl class="kpi-strip" class:is-3=replay>
            <div><dt>"Sungai ≥ Tinggi"</dt><dd class="num">{k.river.elevated()}</dd></div>
            <div><dt>"Hujan lokal ≥ Tinggi"</dt><dd class="num">{k.pluvial.elevated()}</dd></div>
            <div><dt>"Rob ≥ Tinggi"</dt><dd class="num">{k.rob.elevated()}</dd></div>
            {(!replay).then(|| view! {
                <div><dt>"Laporan warga 24 jam"</dt><dd class="num">{k.citizen_flood_reports_24h}</dd></div>
            })}
        </dl>
    }
}

fn csv_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn national_csv(snap: &FloodSnapshot) -> String {
    let o = |v: Option<f64>| v.map(|x| format!("{x:.3}")).unwrap_or_default();
    let mut csv = String::from(
        "kind,id,name,province,lat,lon,level,fluvial_level,pluvial_level,discharge_m3s,q2_m3s,q5_m3s,q20_m3s,ratio_q2,peak_m3s,peak_date,p_exceed_q2_7d,rain_24h_mm,rain_72h_mm,generated_at_utc\n",
    );
    for s in &snap.stations {
        let t = s.thresholds.as_ref();
        csv.push_str(&format!(
            "river,{},{},{},{:.5},{:.5},{},{},{},{:.3},{},{},{},{},{:.3},{},{},{:.1},{:.1},{}\n",
            s.id,
            csv_escape(&s.name),
            csv_escape(&s.province),
            s.lat,
            s.lon,
            s.level.slug(),
            s.fluvial_level.slug(),
            s.pluvial_level.slug(),
            s.current_discharge_m3s,
            o(t.map(|t| t.q2_m3s)),
            o(t.map(|t| t.q5_m3s)),
            o(t.map(|t| t.q20_m3s)),
            o(s.ratio_q2),
            s.peak_forecast_m3s,
            s.peak_forecast_date,
            o(s.exceed_prob_7d.as_ref().map(|e| e.q2)),
            s.rain_24h_mm,
            s.rain_72h_mm,
            snap.generated_at_utc
        ));
    }
    for r in &snap.coastal_rob {
        csv.push_str(&format!(
            "rob,{},{},{},{:.5},{:.5},{},,,,,,,,{:.3},{},,,,{}\n",
            r.id,
            csv_escape(&r.name),
            csv_escape(&r.province),
            r.lat,
            r.lon,
            r.level.slug(),
            r.peak_msl_48h_m,
            r.peak_msl_time,
            snap.generated_at_utc
        ));
    }
    csv
}

fn bmkg_block(snap: &FloodSnapshot, limit: usize) -> AnyView {
    if snap.is_historical_replay {
        // Nowcast BMKG historis tidak dibakukan untuk replay.
        return ().into_any();
    }
    let items: Vec<_> = snap.bmkg_nowcasts.iter().take(limit).cloned().collect();
    let total = snap.bmkg_nowcasts.len();
    view! {
        <section class="block">
            <h3 class="h-sub">{format!("Peringatan dini cuaca BMKG ({total})")}</h3>
            {if items.is_empty() {
                view! { <p class="empty-msg">"Tidak ada peringatan dini cuaca BMKG yang aktif."</p> }.into_any()
            } else {
                view! {
                    <div class="chip-wrap">
                        {items.into_iter().map(|nc| view! {
                            <a class="bmkg-chip" href="https://www.bmkg.go.id/peringatan-dini" target="_blank" rel="noopener" title=nc.summary>{nc.province}</a>
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </section>
    }
    .into_any()
}

#[component]
pub fn InspectorDrawer(
    snapshot: Signal<Option<FloodSnapshot>>,
    loading: RwSignal<bool>,
    selected_id: RwSignal<Option<String>>,
    mech_filter: RwSignal<Option<FloodMechanism>>,
    lens: RwSignal<Audience>,
    active_tab: RwSignal<InspectorTab>,
    drawer_open: RwSignal<bool>,
    mobile_snap: RwSignal<SheetSnap>,
    now: RwSignal<f64>,
    on_select: Callback<(String, f64, f64)>,
) -> impl IntoView {
    let toast = use_toast();
    let news_region = RwSignal::new("Semua".to_string());

    // Notifikasi ntfy
    let ntfy_topic = RwSignal::new(String::new());
    let ntfy_region = RwSignal::new("Nasional".to_string());
    let ntfy_level = RwSignal::new(IndicatorLevel::High);
    let ntfy_test = RwSignal::new(true);
    let ntfy_busy = RwSignal::new(false);
    let ntfy_feedback = RwSignal::new(Option::<String>::None);

    // Tanya data
    let ai_query_text = RwSignal::new(String::new());
    let ai_busy = RwSignal::new(false);
    let ai_result = RwSignal::new(Option::<AiQueryResponse>::None);

    let run_ai_search = move |q_override: Option<String>| {
        let q = q_override.unwrap_or_else(|| ai_query_text.get_untracked());
        if q.trim().is_empty() {
            return;
        }
        ai_query_text.set(q.clone());
        ai_busy.set(true);
        spawn_local(async move {
            let encoded: String = js_sys::encode_uri_component(&q).into();
            let url = format!("/api/ai/query?q={encoded}");
            let remote = match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => resp.json::<AiQueryResponse>().await.ok(),
                _ => None,
            };
            let parsed = remote.or_else(|| {
                snapshot
                    .get_untracked()
                    .map(|snap| flood_shared::ai::execute_ai_query(&snap, &q))
            });
            if let Some(parsed) = parsed {
                if let Some(m) = parsed.recommended_mechanism {
                    mech_filter.set(Some(m));
                }
                if let Some(fly) = &parsed.fly_to {
                    js_fly_to_point(fly.lon, fly.lat, fly.zoom);
                }
                ai_result.set(Some(parsed));
            }
            ai_busy.set(false);
        });
    };

    let submit_ntfy = move |_| {
        let topic = ntfy_topic.get_untracked().trim().to_string();
        if topic.len() < 4 {
            ntfy_feedback.set(Some(
                "Nama topik minimal 4 karakter (huruf, angka, - atau _).".into(),
            ));
            return;
        }
        let payload = AlertSubscriptionReq {
            topic: topic.clone(),
            region: ntfy_region.get_untracked(),
            min_level: ntfy_level.get_untracked(),
            send_test: ntfy_test.get_untracked(),
        };
        ntfy_busy.set(true);
        ntfy_feedback.set(None);
        spawn_local(async move {
            let msg = match Request::post("/api/alerts/subscribe").json(&payload) {
                Ok(req) => match req.send().await {
                    Ok(resp) if resp.ok() => {
                        let txt = resp.text().await.unwrap_or_default();
                        if txt.contains("\"test_pushed\":true") {
                            format!("Tersimpan. Notifikasi uji dikirim ke ntfy.sh/{topic}.")
                        } else {
                            format!("Tersimpan. Berlangganan topik {topic} di aplikasi ntfy.")
                        }
                    }
                    Ok(resp) if resp.status() == 429 => {
                        "Terlalu banyak permintaan. Coba lagi nanti.".into()
                    }
                    Ok(resp) => {
                        let err = resp.text().await.unwrap_or_default();
                        format!("Ditolak ({}): {err}", resp.status())
                    }
                    Err(_) => "Gagal menghubungi server.".into(),
                },
                Err(_) => "Gagal menyiapkan permintaan.".into(),
            };
            ntfy_feedback.set(Some(msg));
            ntfy_busy.set(false);
        });
    };

    let selected_view = move || {
        let id = selected_id.get()?;
        let snap = snapshot.get()?;
        let gen = snap.generated_at_utc.clone();
        let replay = snap.is_historical_replay;
        let back = Callback::new(move |_| selected_id.set(None));
        let lens_v = lens.get();
        if let Some(st) = snap.stations.iter().find(|s| s.id == id).cloned() {
            return Some(
                view! { <StationDetail station=st generated_at_utc=gen lens=lens_v replay=replay on_back=back/> }
                    .into_any(),
            );
        }
        snap.coastal_rob
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .map(|rb| {
                view! { <RobDetail rob=rb generated_at_utc=gen lens=lens_v replay=replay on_back=back/> }
                    .into_any()
            })
    };
    let has_selection = Memo::new(move |_| {
        let Some(id) = selected_id.get() else {
            return false;
        };
        snapshot.with(|s| {
            s.as_ref().is_some_and(|s| {
                s.stations.iter().any(|x| x.id == id) || s.coastal_rob.iter().any(|x| x.id == id)
            })
        })
    });
    // Berita, laporan warga, dan notifikasi hanya bermakna untuk data terkini.
    let is_replay =
        Memo::new(move |_| snapshot.with(|s| s.as_ref().is_some_and(|s| s.is_historical_replay)));

    let tab_btn = move |tab: InspectorTab, label: &'static str| {
        view! {
            <button
                type="button"
                role="tab"
                class="tab"
                aria-selected=move || (active_tab.get() == tab).to_string()
                on:click=move |_| {
                    active_tab.set(tab);
                    if mobile_snap.get_untracked() == SheetSnap::Peek {
                        mobile_snap.set(SheetSnap::Half);
                    }
                }
            >{label}</button>
        }
    };

    let summary_pane = move |snap: FloodSnapshot| {
        let filter = mech_filter.get();
        let rows = point_rows(&snap, filter);
        let replay = snap.is_historical_replay;
        let none_elevated = if replay {
            "Tidak ada titik pada level Tinggi atau lebih pada hari ini di replay."
        } else {
            "Tidak ada titik pada level Tinggi atau lebih saat ini."
        };
        match lens.get() {
            Audience::Public => {
                let elevated: Vec<_> = rows
                    .iter()
                    .filter(|r| r.level.is_elevated())
                    .cloned()
                    .collect();
                let n_el = elevated.len();
                view! {
                    <div class="pane">
                        {kpi_strip(&snap)}
                        <section class="block">
                            <h3 class="h-sub">"Perlu diperhatikan"</h3>
                            {if n_el == 0 {
                                view! { <p class="empty-msg">{none_elevated}</p> }.into_any()
                            } else {
                                view! { <PointList rows=elevated on_select=on_select/> }.into_any()
                            }}
                        </section>
                        {bmkg_block(&snap, 12)}
                        <Disclaimer/>
                        <OfficialRefs/>
                        <section class="block">
                            <h3 class="h-sub">{format!("Semua titik pantau ({})", rows.len())}</h3>
                            <PointList rows=rows on_select=on_select/>
                        </section>
                    </div>
                }
                .into_any()
            }
            Audience::Gov => {
                let gen = snap.generated_at_utc.clone();
                let mut table: Vec<_> = Vec::new();
                for s in &snap.stations {
                    let lv = match filter {
                        Some(FloodMechanism::FluvialBasin) => s.fluvial_level,
                        Some(FloodMechanism::UrbanPluvial) => s.pluvial_level,
                        Some(FloodMechanism::CoastalRob) => continue,
                        // Replay tidak punya prakiraan: hanya level pada tanggal itu.
                        None if replay => s.level,
                        None => s.level.max(s.peak_forecast_level),
                    };
                    if lv.is_elevated() || (!replay && s.peak_forecast_level.is_elevated()) {
                        table.push((
                            s.id.clone(),
                            s.name.clone(),
                            s.province.clone(),
                            s.level,
                            s.peak_forecast_level,
                            if replay {
                                String::new()
                            } else {
                                format!(
                                    "{} · {}",
                                    fmt_day(&s.peak_forecast_date),
                                    lead_label(lead_days(s, &gen))
                                )
                            },
                            s.exceed_prob_7d
                                .as_ref()
                                .map(|e| fmt_pct(e.q2))
                                .unwrap_or_else(|| "–".into()),
                            s.lon,
                            s.lat,
                        ));
                    }
                }
                if matches!(filter, None | Some(FloodMechanism::CoastalRob)) {
                    for r in snap.coastal_rob.iter().filter(|r| r.level.is_elevated()) {
                        let when = parse_utc_ms(&r.peak_msl_time)
                            .map(|ms| fmt_datetime_short(ms, r.tz_offset_hours))
                            .unwrap_or_else(|| r.peak_msl_time.clone());
                        table.push((
                            r.id.clone(),
                            r.name.clone(),
                            r.province.clone(),
                            r.level,
                            r.level,
                            when,
                            "–".into(),
                            r.lon,
                            r.lat,
                        ));
                    }
                }
                if replay {
                    table.sort_by_key(|row| std::cmp::Reverse(row.3));
                } else {
                    table.sort_by(|a, b| b.4.cmp(&a.4).then(b.3.cmp(&a.3)));
                }
                let (heading, empty) = if replay {
                    (
                        format!("Titik ≥ Tinggi pada hari replay ({})", table.len()),
                        "Tidak ada titik pada level Tinggi atau lebih pada hari ini di replay.",
                    )
                } else {
                    (
                        format!("Titik ≥ Tinggi, kini atau prakiraan ({})", table.len()),
                        "Tidak ada titik pada level Tinggi atau lebih, kini maupun dalam prakiraan 7 hari.",
                    )
                };
                let snap_csv = snap.clone();
                let n = table.len();
                view! {
                    <div class="pane">
                        {kpi_strip(&snap)}
                        <section class="block">
                            <div class="block-head">
                                <h3 class="h-sub">{heading}</h3>
                                <button type="button" class="btn btn-sm" on:click=move |_| {
                                    download_text_file("hidrosentinel-indonesia.csv", "text/csv", &national_csv(&snap_csv));
                                }>"Unduh CSV"</button>
                            </div>
                            {if n == 0 {
                                view! { <p class="empty-msg">{empty}</p> }.into_any()
                            } else {
                                view! {
                                    <div class="table-wrap">
                                        <table class="data-table">
                                            <thead><tr>
                                                <th scope="col">{if replay { "Titik" } else { "Titik · waktu puncak" }}</th>
                                                <th scope="col">{if replay { "Level" } else { "Kini → puncak" }}</th>
                                                {(!replay).then(|| view! { <th scope="col" class="num">"P(>Q2)"</th> })}
                                            </tr></thead>
                                            <tbody>
                                                {table.into_iter().map(|(id, name, prov, now_l, peak_l, when, p, lon, lat)| view! {
                                                    <tr>
                                                        <th scope="row">
                                                            <button type="button" class="cell-link" on:click=move |_| on_select.run((id.clone(), lon, lat))>{name}</button>
                                                            <small>{if when.is_empty() { prov } else { format!("{prov} · {when}") }}</small>
                                                        </th>
                                                        <td class="lv-pair"><LevelBadge level=now_l/>{(!replay).then(|| view! { <LevelBadge level=peak_l/> })}</td>
                                                        {(!replay).then(|| view! { <td class="num">{p}</td> })}
                                                    </tr>
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                }.into_any()
                            }}
                        </section>
                        {bmkg_block(&snap, 40)}
                        <Disclaimer/>
                        <OfficialRefs/>
                    </div>
                }
                .into_any()
            }
            Audience::Academia => {
                let k = snap.kpis.clone();
                let period = snap
                    .stations
                    .iter()
                    .find_map(|s| s.thresholds.as_ref().map(|t| t.period.clone()))
                    .unwrap_or_else(|| "–".into());
                let mut st: Vec<_> = snap.stations.clone();
                st.sort_by(|a, b| {
                    b.ratio_q2
                        .unwrap_or(-1.0)
                        .partial_cmp(&a.ratio_q2.unwrap_or(-1.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let max_station = k.max_ratio_station.as_ref().and_then(|id| {
                    snap.stations
                        .iter()
                        .find(|s| &s.id == id)
                        .map(|s| s.name.clone())
                });
                view! {
                    <div class="pane">
                        <dl class="facts">
                            <div><dt>"Versi metodologi"</dt><dd>{snap.methodology_version.clone()}</dd></div>
                            <div><dt>"Klimatologi ambang"</dt><dd>{format!("Reanalisis GloFAS v4, {period}")}</dd></div>
                            <div><dt>"Belum terkalibrasi"</dt><dd class="num">{format!("{} dari {} stasiun", k.uncalibrated, snap.stations.len())}</dd></div>
                            {k.max_ratio_q2.map(|r| view! {
                                <div><dt>"Q/Q2 tertinggi"</dt><dd class="num">{format!("{}{}", fmt_ratio(r), max_station.map(|n| format!(" · {n}")).unwrap_or_default())}</dd></div>
                            })}
                        </dl>
                        <div class="btn-row">
                            <a class="btn" href="/api/stations.geojson" download="hidrosentinel-stations.geojson">"Unduh GeoJSON"</a>
                            <a class="btn" href=geolibre_url() target="_blank" rel="noopener">"Buka di GeoLibre"</a>
                            <a class="btn" href="/api/flood" target="_blank" rel="noopener">"JSON lengkap"</a>
                        </div>
                        <div class="table-wrap">
                            <table class="data-table">
                                <caption>{if replay {
                                    "Debit simulasi pada tanggal replay terhadap ambang periode ulang (m³/s)"
                                } else {
                                    "Debit simulasi hari ini terhadap ambang periode ulang (m³/s)"
                                }}</caption>
                                <thead><tr><th scope="col">"Stasiun"</th><th scope="col" class="num">"Q"</th><th scope="col" class="num">"Q2"</th><th scope="col" class="num">"Q20"</th><th scope="col" class="num">"Q/Q2"</th><th scope="col" class="num">"Sel–pos"</th></tr></thead>
                                <tbody>
                                    {st.into_iter().map(|s| {
                                        let id = s.id.clone();
                                        let (lon, lat) = (s.lon, s.lat);
                                        let t = s.thresholds.clone();
                                        view! {
                                            <tr>
                                                <th scope="row"><button type="button" class="cell-link" on:click=move |_| on_select.run((id.clone(), lon, lat))>{s.name.clone()}</button></th>
                                                <td class="num">{fmt_q(s.current_discharge_m3s)}</td>
                                                <td class="num">{t.as_ref().map(|t| fmt_q(t.q2_m3s)).unwrap_or_else(|| "–".into())}</td>
                                                <td class="num">{t.as_ref().map(|t| fmt_q(t.q20_m3s)).unwrap_or_else(|| "–".into())}</td>
                                                <td class="num">{s.ratio_q2.map(fmt_ratio).unwrap_or_else(|| "–".into())}</td>
                                                <td class="num">{s.glofas_cell.as_ref().map(|c| format!("{} km", fmt_num(c.offset_km, 1))).unwrap_or_else(|| "–".into())}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                        <Disclaimer/>
                    </div>
                }
                .into_any()
            }
            Audience::AiAgent => {
                let briefing = generate_briefing(&snap, "", Audience::AiAgent);
                let md = briefing.markdown.clone();
                view! {
                    <div class="pane">
                        <section class="block">
                            <h3 class="h-sub">"Endpoint terbuka"</h3>
                            <ul class="endpoint-list">
                                <li><a href="/api/flood" target="_blank" rel="noopener"><code>"GET /api/flood"</code></a><span>"Snapshot lengkap (JSON, skema v2)"</span></li>
                                <li><a href="/api/stations.geojson" target="_blank" rel="noopener"><code>"GET /api/stations.geojson"</code></a><span>"Titik dan level (GeoJSON, CORS terbuka)"</span></li>
                                <li><a href="/api/ai/query?q=ciliwung" target="_blank" rel="noopener"><code>"GET /api/ai/query?q="</code></a><span>"Pencarian bahasa alami"</span></li>
                                <li><a href="/api/openapi.json" target="_blank" rel="noopener"><code>"/api/openapi.json"</code></a><span>"Spesifikasi OpenAPI"</span></li>
                                <li><a href="/llms.txt" target="_blank" rel="noopener"><code>"/llms.txt"</code></a><span>"Panduan untuk model bahasa"</span></li>
                                <li><a href="/api/alerts.rss" target="_blank" rel="noopener"><code>"/api/alerts.rss"</code></a><span>"RSS titik ≥ Tinggi"</span></li>
                            </ul>
                        </section>
                        <section class="block">
                            <h3 class="h-sub">"Tanya data"</h3>
                            <form class="ask-form" on:submit=move |ev| { ev.prevent_default(); run_ai_search(None); }>
                                <label class="sr-only" for="ask-input">"Pertanyaan"</label>
                                <input id="ask-input" type="search" class="input" placeholder="Contoh: ciliwung, rob semarang"
                                    prop:value=move || ai_query_text.get()
                                    on:input=move |ev| ai_query_text.set(event_target_value(&ev))/>
                                <button type="submit" class="btn btn-solid" disabled=move || ai_busy.get()>
                                    {move || if ai_busy.get() { "Mencari…" } else { "Tanya" }}
                                </button>
                            </form>
                            {move || ai_result.get().map(|res| {
                                let target = res.fly_to.clone().and_then(|f| f.station_id.map(|id| (id, f.lon, f.lat)));
                                view! {
                                    <div class="answer">
                                        <p>{res.answer_id}</p>
                                        {target.map(|t| view! {
                                            <button type="button" class="btn btn-sm" on:click=move |_| on_select.run(t.clone())>"Buka titik ini"</button>
                                        })}
                                    </div>
                                }
                            })}
                        </section>
                        <section class="block">
                            <div class="block-head">
                                <h3 class="h-sub">"Ringkasan situasi (Markdown)"</h3>
                                <button type="button" class="btn btn-sm" on:click=move |_| { copy_text(&md); toast.show("Markdown disalin"); }>"Salin"</button>
                            </div>
                            <pre class="code code-scroll">{briefing.markdown}</pre>
                        </section>
                    </div>
                }
                .into_any()
            }
        }
    };

    let news_pane = move |snap: FloodSnapshot| {
        let reg = news_region.get();
        let now_v = now.get();
        let news: Vec<_> = snap
            .news
            .iter()
            .filter(|n| reg == "Semua" || n.region_hint.eq_ignore_ascii_case(&reg))
            .cloned()
            .collect();
        let reports = snap.citizen_reports.clone();
        view! {
            <div class="pane">
                <section class="block">
                    <h3 class="h-sub">"Berita banjir 72 jam terakhir"</h3>
                    <div class="chip-wrap" role="group" aria-label="Wilayah berita">
                        {["Semua", "Jawa", "Sumatera", "Kalimantan", "Sulawesi", "Nasional"].into_iter().map(|r| view! {
                            <button type="button" class="chip" aria-pressed=move || (news_region.get() == r).to_string()
                                on:click=move |_| news_region.set(r.to_string())>{r}</button>
                        }).collect_view()}
                    </div>
                    {if news.is_empty() {
                        view! { <p class="empty-msg">"Tidak ada berita banjir 72 jam terakhir."</p> }.into_any()
                    } else {
                        view! {
                            <ul class="news-list">
                                {news.into_iter().map(|item| {
                                    let when = parse_utc_ms(&item.pub_date)
                                        .map(|ms| format!("{} · {}", fmt_datetime_short(ms, 7), fmt_relative(ms, now_v)))
                                        .unwrap_or_else(|| item.pub_date.clone());
                                    view! {
                                        <li>
                                            <a class="news-item" href=item.link target="_blank" rel="noopener">
                                                <b>{item.title}</b>
                                                <small>{format!("{} · {}", item.source, when)}</small>
                                            </a>
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        }.into_any()
                    }}
                </section>
                <section class="block">
                    <h3 class="h-sub">{format!("Laporan banjir warga 24 jam ({})", reports.len())}</h3>
                    {if reports.is_empty() {
                        view! { <p class="empty-msg">"Tidak ada laporan banjir warga di PetaBencana.id dalam 24 jam terakhir."</p> }.into_any()
                    } else {
                        view! {
                            <ul class="news-list">
                                {reports.into_iter().map(|r| {
                                    let when = parse_utc_ms(&r.created_at)
                                        .map(|ms| fmt_datetime_short(ms, 7))
                                        .unwrap_or_else(|| r.created_at.clone());
                                    let depth = r.depth_cm.map(|d| format!(" · ±{d} cm")).unwrap_or_default();
                                    let title = if r.title.trim().is_empty() { "Laporan banjir".to_string() } else { r.title.clone() };
                                    let body = view! {
                                        <b>{title}</b>
                                        <small>{format!("{} · {when}{depth}", r.source)}</small>
                                        {(!r.text.trim().is_empty()).then(|| view! { <span class="news-text">{r.text.clone()}</span> })}
                                    };
                                    match r.url.filter(|u| u.starts_with("https://")) {
                                        Some(u) => view! { <li><a class="news-item" href=u target="_blank" rel="noopener">{body}</a></li> }.into_any(),
                                        None => view! { <li><div class="news-item">{body}</div></li> }.into_any(),
                                    }
                                }).collect_view()}
                            </ul>
                        }.into_any()
                    }}
                </section>
                {bmkg_block(&snap, 40)}
            </div>
        }
        .into_any()
    };

    let notify_pane = move |snap: FloodSnapshot| {
        let reg_val = ntfy_region.get();
        let rss_url = if reg_val == "Nasional" {
            "https://flood.transbulent.id/api/alerts.rss".to_string()
        } else {
            let enc: String = js_sys::encode_uri_component(&reg_val).into();
            format!("https://flood.transbulent.id/api/alerts.rss?region={enc}")
        };
        let rss_copy = rss_url.clone();
        view! {
            <div class="pane">
                <Disclaimer/>
                <section class="block">
                    <h3 class="h-sub">"Notifikasi ke ponsel (ntfy)"</h3>
                    <p class="fine">"Pasang aplikasi ntfy, lalu langgani nama topik yang Anda pilih. Tanpa nomor telepon."</p>
                    <label class="field">"Nama topik"
                        <input type="text" class="input" placeholder="mis. banjir-rt05-bekasi" maxlength="48"
                            prop:value=move || ntfy_topic.get()
                            on:input=move |ev| ntfy_topic.set(event_target_value(&ev))/>
                    </label>
                    <div class="field-row">
                        <label class="field">"Wilayah"
                            <select class="input" on:change=move |ev| ntfy_region.set(event_target_value(&ev))>
                                {["Nasional", "Jawa", "Sumatera", "Kalimantan", "Sulawesi", "DKI Jakarta", "Jawa Barat", "Jawa Tengah", "Jawa Timur"]
                                    .into_iter()
                                    .map(|r| view! { <option value=r selected=move || ntfy_region.get() == r>{r}</option> })
                                    .collect_view()}
                            </select>
                        </label>
                        <label class="field">"Level minimum"
                            <select class="input" on:change=move |ev| {
                                if let Some(l) = IndicatorLevel::from_slug(&event_target_value(&ev)) { ntfy_level.set(l); }
                            }>
                                {[IndicatorLevel::High, IndicatorLevel::VeryHigh, IndicatorLevel::Extreme]
                                    .into_iter()
                                    .map(|l| view! { <option value=l.slug() selected=move || ntfy_level.get() == l>{l.label_id()}</option> })
                                    .collect_view()}
                            </select>
                        </label>
                    </div>
                    <label class="check">
                        <input type="checkbox" prop:checked=move || ntfy_test.get()
                            on:change=move |ev| ntfy_test.set(event_target_checked(&ev))/>
                        <span>"Kirim notifikasi uji"</span>
                    </label>
                    <button type="button" class="btn btn-solid" disabled=move || ntfy_busy.get() on:click=submit_ntfy>
                        {move || if ntfy_busy.get() { "Menyimpan…" } else { "Aktifkan notifikasi" }}
                    </button>
                    {move || ntfy_feedback.get().map(|m| view! { <p class="feedback" role="status">{m}</p> })}
                </section>
                <section class="block">
                    <h3 class="h-sub">"Bagikan ringkasan ke WhatsApp"</h3>
                    <button type="button" class="btn" on:click=move |_| {
                        let msg = format!(
                            "Hidrosentinel Nusantara, {}\n{}\n\nIndikasi model, bukan peringatan resmi. Ikuti BPBD setempat.\nhttps://flood.transbulent.id/",
                            status_stamp(&snap), status_body(&snap)
                        );
                        open_whatsapp_text(&msg);
                    }>"Buka WhatsApp"</button>
                </section>
                <section class="block">
                    <h3 class="h-sub">"RSS"</h3>
                    <div class="copy-row">
                        <code class="code-inline">{rss_url}</code>
                        <button type="button" class="btn btn-sm" on:click=move |_| { copy_text(&rss_copy); toast.show("Alamat RSS disalin"); }>"Salin"</button>
                    </div>
                </section>
            </div>
        }
        .into_any()
    };

    view! {
        <>
            <Show when=move || !drawer_open.get()>
                <button type="button" class="drawer-reopen" on:click=move |_| drawer_open.set(true)>"Buka panel"</button>
            </Show>

            <aside
                class=move || format!(
                    "inspector {} {}",
                    if drawer_open.get() { "is-open" } else { "is-closed" },
                    mobile_snap.get().class_name()
                )
                aria-label="Panel informasi"
                id="panel"
                tabindex="-1"
            >
                <div class="sheet-bar">
                    <button type="button" class="sheet-handle"
                        aria-label=move || match mobile_snap.get() {
                            SheetSnap::Full => "Perkecil panel",
                            _ => "Perbesar panel",
                        }
                        on:click=move |_| mobile_snap.update(|s| *s = s.next_cycle())>
                        <span class="sheet-pill" aria-hidden="true"></span>
                    </button>
                    <div class="lens" role="group" aria-label="Lensa">
                        {[Audience::Public, Audience::Gov, Audience::Academia, Audience::AiAgent].into_iter().map(|a| view! {
                            <button type="button" class="lens-btn" aria-pressed=move || (lens.get() == a).to_string()
                                on:click=move |_| lens.set(a)>{lens_label(a)}</button>
                        }).collect_view()}
                    </div>
                    <button type="button" class="icon-btn drawer-close" aria-label="Sembunyikan panel" title="Sembunyikan panel"
                        on:click=move |_| drawer_open.set(false)>
                        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.6" fill="none"/></svg>
                    </button>
                </div>

                <div class="peek-status" on:click=move |_| if mobile_snap.get_untracked() == SheetSnap::Peek { mobile_snap.set(SheetSnap::Half) }>
                    {move || match snapshot.get() {
                        Some(s) if s.is_historical_replay => view! {
                            <p class="peek-line"><b>"Replay historis"</b>{format!(" · {}", s.replay_label.clone().unwrap_or_default())}</p>
                        }.into_any(),
                        Some(s) => view! {
                            <p class="peek-line"><b>{status_stamp(&s)}</b><br/>{status_body(&s)}</p>
                        }.into_any(),
                        None if loading.get() => view! { <p class="peek-line muted">"Memuat data…"</p> }.into_any(),
                        None => view! { <p class="peek-line muted">"Data belum tersedia."</p> }.into_any(),
                    }}
                </div>

                <Show when=move || has_selection.get()>
                    <div class="scroll">{selected_view}</div>
                </Show>

                <Show when=move || !has_selection.get()>
                    <div class="tabs" role="tablist" aria-label="Bagian panel">
                        {tab_btn(InspectorTab::Summary, "Ringkasan")}
                        <Show when=move || !is_replay.get()>
                            {tab_btn(InspectorTab::News, "Berita")}
                            {tab_btn(InspectorTab::Notify, "Notifikasi")}
                        </Show>
                        {tab_btn(InspectorTab::About, "Tentang data")}
                    </div>
                    <div class="scroll" role="tabpanel">
                        <Show when=move || loading.get() && snapshot.with(|s| s.is_none())>
                            <div class="skeleton" aria-busy="true" aria-label="Memuat data">
                                <span></span><span></span><span></span><span></span>
                            </div>
                        </Show>
                        {move || {
                            let tab = active_tab.get();
                            if tab == InspectorTab::About {
                                return Some(view! { <AboutPane snapshot=snapshot now=now/> }.into_any());
                            }
                            let snap = snapshot.get()?;
                            Some(match tab {
                                InspectorTab::News | InspectorTab::Notify if snap.is_historical_replay => {
                                    summary_pane(snap)
                                }
                                InspectorTab::Summary => summary_pane(snap),
                                InspectorTab::News => news_pane(snap),
                                InspectorTab::Notify => notify_pane(snap),
                                InspectorTab::About => unreachable!(),
                            })
                        }}
                    </div>
                </Show>
            </aside>
        </>
    }
}

pub fn lens_from_slug(s: &str) -> Audience {
    match s {
        "bpbd" => Audience::Gov,
        "riset" => Audience::Academia,
        "ai" => Audience::AiAgent,
        other => Audience::from_str_lossy(other),
    }
}
