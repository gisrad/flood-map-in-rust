use crate::bridge::{
    copy_text, download_text_file, js_fly_to_point, open_whatsapp_text,
};
use crate::components::hydrograph::HydrographChart;
use flood_shared::ai::generate_briefing;
use flood_shared::model::{
    AiQueryResponse, AlertLevel, AlertSubscriptionReq, Audience, CoastalRobTelemetry,
    FloodMechanism, FloodSnapshot, StationTelemetry,
};
use gloo_net::http::Request;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Summary,
    News,
    Alerts,
    AiData,
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

#[component]
pub fn InspectorDrawer(
    snapshot: Signal<Option<FloodSnapshot>>,
    loading: RwSignal<bool>,
    error_msg: RwSignal<Option<String>>,
    selected_id: RwSignal<Option<String>>,
    mech_filter: RwSignal<Option<FloodMechanism>>,
    audience: RwSignal<Audience>,
    active_tab: RwSignal<InspectorTab>,
    drawer_open: RwSignal<bool>,
    mobile_snap: RwSignal<SheetSnap>,
    ai_focus_tick: RwSignal<u32>,
) -> impl IntoView {
    let news_region = RwSignal::new("Semua".to_string());

    // Free ntfy.sh subscription state
    let ntfy_topic = RwSignal::new("hidrosentinel-id-waspada".to_string());
    let ntfy_region = RwSignal::new("Nasional".to_string());
    let ntfy_level = RwSignal::new("Waspada".to_string());
    let ntfy_test = RwSignal::new(true);
    let ntfy_busy = RwSignal::new(false);
    let ntfy_feedback = RwSignal::new(Option::<String>::None);
    let rss_copied = RwSignal::new(false);

    // AI Query state
    let ai_query_text = RwSignal::new(String::new());
    let ai_busy = RwSignal::new(false);
    let ai_result = RwSignal::new(Option::<AiQueryResponse>::None);
    let md_copied = RwSignal::new(false);

    // Jump to AI tab when requested from top bar
    Effect::new(move |_| {
        let tick = ai_focus_tick.get();
        if tick > 0 {
            selected_id.set(None);
            active_tab.set(InspectorTab::AiData);
            drawer_open.set(true);
            mobile_snap.set(SheetSnap::Half);
        }
    });

    let run_ai_search = move |q_override: Option<String>| {
        let q = q_override.unwrap_or_else(|| ai_query_text.get());
        if q.trim().is_empty() {
            return;
        }
        ai_query_text.set(q.clone());
        ai_busy.set(true);
        spawn_local(async move {
            let encoded: String = js_sys::encode_uri_component(&q).into();
            let url = format!("/api/ai/query?q={encoded}");
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => {
                    if let Ok(parsed) = resp.json::<AiQueryResponse>().await {
                        if let Some(m) = parsed.recommended_mechanism {
                            mech_filter.set(Some(m));
                        }
                        if let Some(fly) = &parsed.fly_to {
                            js_fly_to_point(fly.lon, fly.lat, fly.zoom);
                        }
                        ai_result.set(Some(parsed));
                    }
                }
                _ => {
                    // Fallback to client-side Rust query when offline/replay
                    if let Some(snap) = snapshot.get_untracked() {
                        let parsed = flood_shared::ai::execute_ai_query(&snap, &q);
                        if let Some(fly) = &parsed.fly_to {
                            js_fly_to_point(fly.lon, fly.lat, fly.zoom);
                        }
                        ai_result.set(Some(parsed));
                    }
                }
            }
            ai_busy.set(false);
        });
    };

    let submit_ntfy = move |_| {
        let topic = ntfy_topic.get();
        let region = ntfy_region.get();
        let min_level = match ntfy_level.get().as_str() {
            "Awas" => AlertLevel::Awas,
            "Siaga" => AlertLevel::Siaga,
            _ => AlertLevel::Waspada,
        };
        let send_test = ntfy_test.get();
        ntfy_busy.set(true);
        ntfy_feedback.set(None);
        spawn_local(async move {
            let payload = AlertSubscriptionReq {
                topic: topic.clone(),
                region,
                min_level,
                send_test,
            };
            match Request::post("/api/alerts/subscribe").json(&payload) {
                Ok(req) => match req.send().await {
                    Ok(resp) if resp.ok() => {
                        let txt = resp.text().await.unwrap_or_default();
                        let pushed = txt.contains("\"test_pushed\":true");
                        let msg = if pushed {
                            format!("Aktif! Tes push sudah dikirim ke https://ntfy.sh/{topic}")
                        } else {
                            format!("Tersimpan di SQLite Durable Object. Buka aplikasi/web ntfy.sh dan subscribe topik: {topic}")
                        };
                        ntfy_feedback.set(Some(msg));
                    }
                    Ok(resp) => {
                        let err = resp.text().await.unwrap_or_else(|_| "Gagal menyimpan".into());
                        ntfy_feedback.set(Some(format!("Ditolak: {err}")));
                    }
                    Err(e) => {
                        ntfy_feedback.set(Some(format!("Gagal menghubungi worker: {e}")));
                    }
                },
                Err(e) => {
                    ntfy_feedback.set(Some(format!("Error JSON: {e}")));
                }
            }
            ntfy_busy.set(false);
        });
    };

    let selected_station = Signal::derive(move || -> Option<StationTelemetry> {
        let id = selected_id.get()?;
        let snap = snapshot.get()?;
        snap.stations.into_iter().find(|s| s.id == id)
    });

    let selected_rob = Signal::derive(move || -> Option<CoastalRobTelemetry> {
        let id = selected_id.get()?;
        let snap = snapshot.get()?;
        snap.coastal_rob.into_iter().find(|r| r.id == id)
    });

    let select_station = move |id: String, lon: f64, lat: f64| {
        selected_id.set(Some(id));
        drawer_open.set(true);
        mobile_snap.set(SheetSnap::Half);
        js_fly_to_point(lon, lat, 9.0);
    };

    view! {
        <>
            <Show when=move || !drawer_open.get()>
                <button
                    type="button"
                    class="drawer-reopen"
                    on:click=move |_| drawer_open.set(true)
                >"Panel Hidrologi"</button>
            </Show>

            <aside
                class=move || {
                    let open_cls = if drawer_open.get() { "is-open" } else { "is-closed" };
                    let snap_cls = mobile_snap.get().class_name();
                    format!("inspector {open_cls} {snap_cls}")
                }
                aria-label="Panel Inspeksi Hidrometeorologi"
            >
                <div class="sheet-handle-bar">
                    <button
                        type="button"
                        class="sheet-handle-btn"
                        aria-label="Ubah tinggi panel bawah"
                        on:click=move |_| mobile_snap.update(|s| *s = s.next_cycle())
                    >
                        <span class="sheet-pill"></span>
                        <span class="sheet-hint">
                            {move || match mobile_snap.get() {
                                SheetSnap::Peek => "Ringkasan · Tarik / Ketuk",
                                SheetSnap::Half => "Panel · Ketuk Perluas",
                                SheetSnap::Full => "Panel Penuh · Perkecil",
                            }}
                        </span>
                    </button>
                    <button
                        type="button"
                        class="drawer-collapse-btn"
                        on:click=move |_| drawer_open.set(false)
                        title="Sembunyikan panel di desktop"
                    >"Tutup"</button>
                </div>

                <Show when=move || selected_station.get().is_some()>
                    {move || selected_station.get().map(|st| {
                        let aud = audience.get();
                        let advice_txt = match aud {
                            Audience::Public => st.public_advice.clone(),
                            Audience::Gov => st.gov_action.clone(),
                            Audience::Academia | Audience::AiAgent => st.academic_note.clone(),
                        };
                        let st_csv = st.clone();
                        let st_wa = st.clone();
                        let soil_top_pct = (st.soil_moisture_top * 200.0).clamp(5.0, 100.0);
                        let soil_deep_pct = (st.soil_moisture_deep * 200.0).clamp(5.0, 100.0);

                        view! {
                            <div class="detail-view">
                                <div class="detail-nav">
                                    <button type="button" class="back-btn" on:click=move |_| selected_id.set(None)>
                                        "← Kembali ke Ringkasan"
                                    </button>
                                    <span class=format!("badge badge-{}", st.alert_level.label().to_ascii_lowercase())>
                                        {st.alert_level.label()}
                                    </span>
                                </div>

                                <h2 class="detail-title">{st.name.clone()}</h2>
                                <p class="detail-sub">
                                    {format!("{} · {} ({}) · DAS {} km²", st.ws_code, st.province, st.island, st.basin_area_km2)}
                                </p>

                                <div class="mech-tag-row">
                                    <span class=format!("mech-pill mech-{}", st.mechanism.slug())>
                                        {st.mechanism.label_id()}
                                    </span>
                                    <span class="trend-pill">{format!("Tren: {}", st.trend.label())}</span>
                                    <span class="trend-pill">{st.return_period_est.clone()}</span>
                                </div>

                                <div class="kpi-grid-4">
                                    <div class="kpi-cell">
                                        <small>"Debit Kini"</small>
                                        <strong>{format!("{:.1}", st.current_discharge_m3s)}</strong>
                                        <span>"m³/s"</span>
                                    </div>
                                    <div class="kpi-cell">
                                        <small>"Puncak 7h"</small>
                                        <strong>{format!("{:.1}", st.peak_forecast_m3s)}</strong>
                                        <span>{st.peak_forecast_date.clone()}</span>
                                    </div>
                                    <div class="kpi-cell">
                                        <small>"Rasio Q75"</small>
                                        <strong>{format!("{:.2}×", st.anomaly_ratio)}</strong>
                                        <span>{format!("Q75 {:.0}", st.p75_baseline_m3s)}</span>
                                    </div>
                                    <div class="kpi-cell">
                                        <small>"Hujan 72j"</small>
                                        <strong>{format!("{:.0}", st.rain_72h_mm)}</strong>
                                        <span>{format!("24j: {:.0} mm", st.rain_24h_mm)}</span>
                                    </div>
                                </div>

                                <HydrographChart station=st.clone()/>

                                <div class="soil-card">
                                    <div class="soil-row">
                                        <span>"Kejenuhan Tanah 0–7 cm"</span>
                                        <strong>{format!("{:.2} m³/m³", st.soil_moisture_top)}</strong>
                                    </div>
                                    <div class="meter-track">
                                        <div class="meter-fill" style=format!("width:{soil_top_pct:.0}%")></div>
                                    </div>
                                    <div class="soil-row">
                                        <span>"Kejenuhan Tanah 28–100 cm"</span>
                                        <strong>{format!("{:.2} m³/m³", st.soil_moisture_deep)}</strong>
                                    </div>
                                    <div class="meter-track">
                                        <div class="meter-fill deep" style=format!("width:{soil_deep_pct:.0}%")></div>
                                    </div>
                                </div>

                                <div class="advice-box">
                                    <div class="advice-head">{format!("Panduan {}", aud.label())}</div>
                                    <p>{advice_txt}</p>
                                </div>

                                <div class="detail-actions">
                                    <button
                                        type="button"
                                        class="btn-primary"
                                        on:click=move |_| {
                                            let msg = format!(
                                                "[Hidrosentinel] {} ({}) — Status {} · Debit {:.1} m3/s ({:.2}x Q75) · Hujan 72j {:.0} mm. {}",
                                                st_wa.name, st_wa.province, st_wa.alert_level.label(),
                                                st_wa.current_discharge_m3s, st_wa.anomaly_ratio, st_wa.rain_72h_mm,
                                                st_wa.public_advice
                                            );
                                            open_whatsapp_text(&msg);
                                        }
                                    >"Bagikan WhatsApp"</button>
                                    <button
                                        type="button"
                                        class="btn-secondary"
                                        on:click=move |_| {
                                            let mut csv = String::from("date,is_forecast,discharge_m3s,mean_m3s,max_m3s,p75_m3s,rain_mm\n");
                                            for p in &st_csv.hydrograph {
                                                csv.push_str(&format!(
                                                    "{},{},{:.2},{:.2},{:.2},{:.2},{:.2}\n",
                                                    p.date, p.is_forecast, p.discharge_m3s,
                                                    p.discharge_mean_m3s, p.discharge_max_m3s,
                                                    p.discharge_p75_m3s, p.rain_mm
                                                ));
                                            }
                                            download_text_file(&format!("{}-hydrograph.csv", st_csv.id), "text/csv", &csv);
                                        }
                                    >"Unduh CSV"</button>
                                </div>
                            </div>
                        }
                    })}
                </Show>

                <Show when=move || selected_station.get().is_none() && selected_rob.get().is_some()>
                    {move || selected_rob.get().map(|rb| {
                        let rb_wa = rb.clone();
                        view! {
                            <div class="detail-view">
                                <div class="detail-nav">
                                    <button type="button" class="back-btn" on:click=move |_| selected_id.set(None)>
                                        "← Kembali ke Ringkasan"
                                    </button>
                                    <span class=format!("badge badge-{}", rb.alert_level.label().to_ascii_lowercase())>
                                        {format!("Rob {}", rb.alert_level.label())}
                                    </span>
                                </div>
                                <h2 class="detail-title">{rb.name.clone()}</h2>
                                <p class="detail-sub">{format!("{} ({}) · Faktor Amblesan {:.2}×", rb.province, rb.island, rb.sensitivity)}</p>
                                <div class="kpi-grid-4">
                                    <div class="kpi-cell">
                                        <small>"MSL Kini"</small>
                                        <strong>{format!("{:.2}", rb.current_msl_m)}</strong>
                                        <span>"meter"</span>
                                    </div>
                                    <div class="kpi-cell">
                                        <small>"Puncak 48j"</small>
                                        <strong>{format!("{:.2}", rb.peak_msl_48h_m)}</strong>
                                        <span>"meter"</span>
                                    </div>
                                    <div class="kpi-cell">
                                        <small>"Gelombang"</small>
                                        <strong>{format!("{:.2}", rb.wave_height_m)}</strong>
                                        <span>"meter"</span>
                                    </div>
                                    <div class="kpi-cell">
                                        <small>"Indeks Rob"</small>
                                        <strong>{format!("{:.2}×", rb.rob_index)}</strong>
                                        <span>{format!("Picu {:.2}m", rb.trigger_msl_m)}</span>
                                    </div>
                                </div>
                                <div class="advice-box">
                                    <div class="advice-head">"Analisis Pasang & Amblesan Pesisir"</div>
                                    <p>{rb.note.clone()}</p>
                                </div>
                                <div class="detail-actions">
                                    <button
                                        type="button"
                                        class="btn-primary"
                                        on:click=move |_| {
                                            let msg = format!(
                                                "[Hidrosentinel Rob] {} ({}) — Status {} · Puncak MSL 48j {:.2} m · Gelombang {:.2} m. {}",
                                                rb_wa.name, rb_wa.province, rb_wa.alert_level.label(),
                                                rb_wa.peak_msl_48h_m, rb_wa.wave_height_m, rb_wa.note
                                            );
                                            open_whatsapp_text(&msg);
                                        }
                                    >"Bagikan WhatsApp"</button>
                                </div>
                            </div>
                        }
                    })}
                </Show>

                <Show when=move || selected_station.get().is_none() && selected_rob.get().is_none()>
                    <div class="drawer-tabs" role="tablist">
                        <button
                            type="button"
                            role="tab"
                            class="tab-btn"
                            aria-selected=move || (active_tab.get() == InspectorTab::Summary).to_string()
                            on:click=move |_| { active_tab.set(InspectorTab::Summary); mobile_snap.set(SheetSnap::Half); }
                        >"Ringkasan"</button>
                        <button
                            type="button"
                            role="tab"
                            class="tab-btn"
                            aria-selected=move || (active_tab.get() == InspectorTab::News).to_string()
                            on:click=move |_| { active_tab.set(InspectorTab::News); mobile_snap.set(SheetSnap::Half); }
                        >"Berita"</button>
                        <button
                            type="button"
                            role="tab"
                            class="tab-btn"
                            aria-selected=move || (active_tab.get() == InspectorTab::Alerts).to_string()
                            on:click=move |_| { active_tab.set(InspectorTab::Alerts); mobile_snap.set(SheetSnap::Half); }
                        >"Peringatan"</button>
                        <button
                            type="button"
                            role="tab"
                            class="tab-btn"
                            aria-selected=move || (active_tab.get() == InspectorTab::AiData).to_string()
                            on:click=move |_| { active_tab.set(InspectorTab::AiData); mobile_snap.set(SheetSnap::Half); }
                        >"AI & Data"</button>
                    </div>

                    <div class="drawer-scroll">
                        <Show when=move || loading.get() && snapshot.get().is_none()>
                            <div class="loading-box">"Memuat telemetri 50 DAS GloFAS, 12 koridor rob, BMKG Nowcast & berita…"</div>
                        </Show>
                        <Show when=move || error_msg.get().is_some()>
                            <div class="error-box">{move || error_msg.get().unwrap_or_default()}</div>
                        </Show>

                        {move || snapshot.get().map(|snap| {
                            let tab = active_tab.get();
                            match tab {
                                InspectorTab::Summary => {
                                    let k = &snap.kpis;
                                    let elevated_total = k.awas_count + k.siaga_count + k.waspada_count;

                                    // 7-day lead-time watch (forecast peak significantly higher than current)
                                    let mut surge_watch: Vec<_> = snap.stations.iter()
                                        .filter(|s| s.peak_ratio >= 1.20 && s.peak_forecast_m3s > s.current_discharge_m3s * 1.15)
                                        .cloned()
                                        .collect();
                                    surge_watch.sort_by(|a, b| b.peak_ratio.partial_cmp(&a.peak_ratio).unwrap_or(std::cmp::Ordering::Equal));

                                    let filter_m = mech_filter.get();
                                    let mut ranked_stations: Vec<_> = snap.stations.iter()
                                        .filter(|s| filter_m.map(|m| m != FloodMechanism::CoastalRob && s.mechanism == m).unwrap_or(true))
                                        .cloned()
                                        .collect();
                                    ranked_stations.sort_by(|a, b| {
                                        b.alert_level.rank().cmp(&a.alert_level.rank())
                                            .then_with(|| b.anomaly_ratio.partial_cmp(&a.anomaly_ratio).unwrap_or(std::cmp::Ordering::Equal))
                                    });

                                    let mut ranked_robs: Vec<_> = snap.coastal_rob.clone();
                                    ranked_robs.sort_by(|a, b| b.rob_index.partial_cmp(&a.rob_index).unwrap_or(std::cmp::Ordering::Equal));

                                    view! {
                                        <div class="tab-pane">
                                            <div class="kpi-grid-4">
                                                <div class="kpi-cell">
                                                    <small>"Waspada–Awas"</small>
                                                    <strong>{elevated_total}</strong>
                                                    <span>{format!("{} Awas · {} Siaga", k.awas_count, k.siaga_count)}</span>
                                                </div>
                                                <div class="kpi-cell">
                                                    <small>"Anomali Maks"</small>
                                                    <strong>{format!("{:.2}×", k.max_anomaly_ratio)}</strong>
                                                    <span>"vs baseline Q75"</span>
                                                </div>
                                                <div class="kpi-cell">
                                                    <small>"Nowcast BMKG"</small>
                                                    <strong>{snap.bmkg_nowcasts.len()}</strong>
                                                    <span>"peringatan aktif"</span>
                                                </div>
                                                <div class="kpi-cell">
                                                    <small>"Laporan Warga"</small>
                                                    <strong>{k.citizen_reports_24h}</strong>
                                                    <span>"PetaBencana 24j"</span>
                                                </div>
                                            </div>

                                            {(!snap.bmkg_nowcasts.is_empty()).then(|| view! {
                                                <div class="section-block">
                                                    <div class="sec-head">
                                                        <h3>"Peringatan Dini Cuaca Ekstrem (BMKG)"</h3>
                                                        <span class="sec-meta">"CAP RSS"</span>
                                                    </div>
                                                    <div class="pill-wrap">
                                                        {snap.bmkg_nowcasts.iter().take(6).map(|nc| {
                                                            view! {
                                                                <a
                                                                    class="bmkg-chip"
                                                                    href=nc.link.clone()
                                                                    target="_blank"
                                                                    rel="noopener noreferrer"
                                                                    title=nc.summary.clone()
                                                                >{nc.province.clone()}</a>
                                                            }
                                                        }).collect_view()}
                                                    </div>
                                                </div>
                                            })}

                                            {(!surge_watch.is_empty()).then(|| view! {
                                                <div class="section-block">
                                                    <div class="sec-head">
                                                        <h3>"Pantauan Lonjakan 7 Hari (Lead-Time)"</h3>
                                                        <span class="sec-meta">"GloFAS Forecast"</span>
                                                    </div>
                                                    <div class="list-stack">
                                                        {surge_watch.iter().take(4).map(|st| {
                                                            let id = st.id.clone();
                                                            let lon = st.lon;
                                                            let lat = st.lat;
                                                            view! {
                                                                <button
                                                                    type="button"
                                                                    class="station-row surge-row"
                                                                    on:click=move |_| select_station(id.clone(), lon, lat)
                                                                >
                                                                    <div class="st-main">
                                                                        <strong>{st.name.clone()}</strong>
                                                                        <span>{format!("{} · Puncak {}", st.province, st.peak_forecast_date)}</span>
                                                                    </div>
                                                                    <div class="st-metric">
                                                                        <b>{format!("{:.2}×", st.peak_ratio)}</b>
                                                                        <small>{format!("{:.0} m³/s", st.peak_forecast_m3s)}</small>
                                                                    </div>
                                                                </button>
                                                            }
                                                        }).collect_view()}
                                                    </div>
                                                </div>
                                            })}

                                            {if filter_m == Some(FloodMechanism::CoastalRob) {
                                                view! {
                                                    <div class="section-block">
                                                        <div class="sec-head">
                                                            <h3>"Koridor Pasang Rob Pesisir"</h3>
                                                            <span class="sec-meta">{format!("{} koridor", ranked_robs.len())}</span>
                                                        </div>
                                                        <div class="list-stack">
                                                            {ranked_robs.into_iter().map(|rb| {
                                                                let id = rb.id.clone();
                                                                let lon = rb.lon;
                                                                let lat = rb.lat;
                                                                let lvl = rb.alert_level.label().to_ascii_lowercase();
                                                                view! {
                                                                    <button type="button" class="station-row" on:click=move |_| select_station(id.clone(), lon, lat)>
                                                                        <div class="st-main">
                                                                            <strong>{rb.name.clone()}</strong>
                                                                            <span>{format!("{} · Gelombang {:.2} m · Amblesan {:.2}×", rb.province, rb.wave_height_m, rb.sensitivity)}</span>
                                                                        </div>
                                                                        <div class="st-metric">
                                                                            <span class=format!("badge badge-{lvl}")>{rb.alert_level.label()}</span>
                                                                            <small>{format!("{:.2}×", rb.rob_index)}</small>
                                                                        </div>
                                                                    </button>
                                                                }
                                                            }).collect_view()}
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="section-block">
                                                        <div class="sec-head">
                                                            <h3>"Prioritas Stasiun DAS Indonesia"</h3>
                                                            <span class="sec-meta">{format!("{} titik", ranked_stations.len())}</span>
                                                        </div>
                                                        <div class="list-stack">
                                                            {ranked_stations.into_iter().take(18).map(|st| {
                                                                let id = st.id.clone();
                                                                let lon = st.lon;
                                                                let lat = st.lat;
                                                                let lvl = st.alert_level.label().to_ascii_lowercase();
                                                                view! {
                                                                    <button type="button" class="station-row" on:click=move |_| select_station(id.clone(), lon, lat)>
                                                                        <div class="st-main">
                                                                            <strong>{st.name.clone()}</strong>
                                                                            <span>{format!("{} · {} · Hujan 72j {:.0} mm", st.province, st.mechanism.label_id(), st.rain_72h_mm)}</span>
                                                                        </div>
                                                                        <div class="st-metric">
                                                                            <span class=format!("badge badge-{lvl}")>{st.alert_level.label()}</span>
                                                                            <small>{format!("{:.2}× Q75", st.anomaly_ratio)}</small>
                                                                        </div>
                                                                    </button>
                                                                }
                                                            }).collect_view()}
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            }}
                                        </div>
                                    }.into_any()
                                }

                                InspectorTab::News => {
                                    let reg = news_region.get();
                                    let filtered_news: Vec<_> = snap.news.iter()
                                        .filter(|n| reg == "Semua" || n.region_hint.eq_ignore_ascii_case(&reg))
                                        .cloned()
                                        .collect();

                                    view! {
                                        <div class="tab-pane">
                                            <div class="section-block">
                                                <div class="sec-head">
                                                    <h3>"Kanal Berita Banjir Nasional & Regional"</h3>
                                                    <span class="sec-meta">"RSS Live"</span>
                                                </div>
                                                <div class="filter-pills-wrap">
                                                    {["Semua", "Jawa", "Sumatera", "Kalimantan", "Sulawesi", "Nasional"].into_iter().map(|r| {
                                                        let r_str = r.to_string();
                                                        view! {
                                                            <button
                                                                type="button"
                                                                class="mini-chip"
                                                                aria-pressed=move || (news_region.get() == r_str).to_string()
                                                                on:click=move |_| news_region.set(r.to_string())
                                                            >{r}</button>
                                                        }
                                                    }).collect_view()}
                                                </div>

                                                {filtered_news.is_empty().then(|| view! {
                                                    <p class="empty-msg">"Belum ada item berita untuk filter wilayah ini."</p>
                                                })}

                                                <div class="news-list">
                                                    {filtered_news.into_iter().map(|item| {
                                                        view! {
                                                            <a class="news-card" href=item.link.clone() target="_blank" rel="noopener noreferrer">
                                                                <div class="news-meta">
                                                                    <span class="news-pub">{item.source.clone()}</span>
                                                                    <span class="news-reg">{item.region_hint.clone()}</span>
                                                                </div>
                                                                <strong>{item.title.clone()}</strong>
                                                                <small>{item.pub_date.clone()}</small>
                                                            </a>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            </div>

                                            {(!snap.bmkg_nowcasts.is_empty()).then(|| view! {
                                                <div class="section-block">
                                                    <div class="sec-head">
                                                        <h3>"Detail Nowcast Cuaca Ekstrem BMKG"</h3>
                                                        <span class="sec-meta">{snap.bmkg_nowcasts.len()}</span>
                                                    </div>
                                                    <div class="news-list">
                                                        {snap.bmkg_nowcasts.iter().take(6).map(|nc| {
                                                            view! {
                                                                <a class="news-card bmkg-item" href=nc.link.clone() target="_blank" rel="noopener noreferrer">
                                                                    <div class="news-meta">
                                                                        <span class="news-pub">"BMKG CAP"</span>
                                                                        <span class="news-reg">{nc.province.clone()}</span>
                                                                    </div>
                                                                    <strong>{nc.title.clone()}</strong>
                                                                    <p>{nc.summary.clone()}</p>
                                                                </a>
                                                            }
                                                        }).collect_view()}
                                                    </div>
                                                </div>
                                            })}
                                        </div>
                                    }.into_any()
                                }

                                InspectorTab::Alerts => {
                                    let snap_wa = snap.clone();
                                    let reg_val = ntfy_region.get();
                                    let rss_url = if reg_val == "Nasional" {
                                        "https://flood.transbulent.id/api/alerts.rss".to_string()
                                    } else {
                                        format!("https://flood.transbulent.id/api/alerts.rss?region={reg_val}")
                                    };
                                    let rss_url_copy = rss_url.clone();

                                    view! {
                                        <div class="tab-pane">
                                            <div class="info-callout">
                                                <strong>"Notifikasi Gratis Tanpa Biaya SMS / WA Gateway"</strong>
                                                <p>"Nomor HP/SMS Gateway komersial sengaja dilewati agar sistem tetap $0/bulan & bebas menyimpan data pribadi (PII). Gunakan 3 jalur gratis real-time di bawah ini:"</p>
                                            </div>

                                            <div class="section-block card-surface">
                                                <h3>"1. Push HP Gratis via ntfy.sh (Tersimpan di Durable Object)"</h3>
                                                <p class="field-hint">"Install aplikasi ntfy (Android/iOS) atau buka ntfy.sh lalu subscribe nama topik unik Anda:"</p>
                                                <label class="field-lbl">"Nama Topik Unik (4–48 huruf/angka, bukan nomor HP)"
                                                    <input
                                                        type="text"
                                                        class="text-input"
                                                        prop:value=move || ntfy_topic.get()
                                                        on:input=move |ev| ntfy_topic.set(event_target_value(&ev))
                                                    />
                                                </label>
                                                <div class="form-row-2">
                                                    <label class="field-lbl">"Wilayah Pantau"
                                                        <select
                                                            class="text-input"
                                                            on:change=move |ev| ntfy_region.set(event_target_value(&ev))
                                                        >
                                                            <option value="Nasional" selected>"Nasional"</option>
                                                            <option value="Jawa">"Jawa"</option>
                                                            <option value="Sumatera">"Sumatera"</option>
                                                            <option value="Kalimantan">"Kalimantan"</option>
                                                            <option value="Sulawesi">"Sulawesi"</option>
                                                            <option value="DKI Jakarta">"DKI Jakarta"</option>
                                                            <option value="Jawa Barat">"Jawa Barat"</option>
                                                            <option value="Jawa Tengah">"Jawa Tengah"</option>
                                                            <option value="Jawa Timur">"Jawa Timur"</option>
                                                        </select>
                                                    </label>
                                                    <label class="field-lbl">"Ambang Minimal"
                                                        <select
                                                            class="text-input"
                                                            on:change=move |ev| ntfy_level.set(event_target_value(&ev))
                                                        >
                                                            <option value="Waspada" selected>"Waspada"</option>
                                                            <option value="Siaga">"Siaga"</option>
                                                            <option value="Awas">"Awas"</option>
                                                        </select>
                                                    </label>
                                                </div>
                                                <label class="check-row">
                                                    <input
                                                        type="checkbox"
                                                        prop:checked=move || ntfy_test.get()
                                                        on:change=move |ev| ntfy_test.set(event_target_checked(&ev))
                                                    />
                                                    <span>"Kirim tes notifikasi push sekarang"</span>
                                                </label>
                                                <button
                                                    type="button"
                                                    class="btn-primary full-w"
                                                    disabled=move || ntfy_busy.get()
                                                    on:click=submit_ntfy
                                                >
                                                    {move || if ntfy_busy.get() { "Menyimpan ke SQLite..." } else { "Aktifkan Push Gratis" }}
                                                </button>
                                                <Show when=move || ntfy_feedback.get().is_some()>
                                                    <p class="feedback-msg">{move || ntfy_feedback.get().unwrap_or_default()}</p>
                                                </Show>
                                            </div>

                                            <div class="section-block card-surface">
                                                <h3>"2. Bagikan Peringatan ke Grup WhatsApp (Gratis 1 Klik)"</h3>
                                                <p class="field-hint">"Kirim ringkasan status DAS & rob terkini langsung ke WhatsApp keluarga, grup RT, atau posko BPBD:"</p>
                                                <button
                                                    type="button"
                                                    class="btn-secondary full-w"
                                                    on:click=move |_| {
                                                        let b = generate_briefing(&snap_wa, "", Audience::Public);
                                                        let msg = format!("{}\n\n{}\nCek peta 3D: https://flood.transbulent.id/", b.headline, b.sitrep_summary);
                                                        open_whatsapp_text(&msg);
                                                    }
                                                >"Buka WhatsApp & Bagikan SitRep Sekarang"</button>
                                            </div>

                                            <div class="section-block card-surface">
                                                <h3>"3. RSS Feed Peringatan Wilayah (Untuk Bot Telegram / n8n)"</h3>
                                                <div class="copy-row">
                                                    <code class="rss-code">{rss_url}</code>
                                                    <button
                                                        type="button"
                                                        class="mini-chip"
                                                        on:click=move |_| {
                                                            copy_text(&rss_url_copy);
                                                            rss_copied.set(true);
                                                        }
                                                    >{move || if rss_copied.get() { "Tersalin" } else { "Salin RSS" }}</button>
                                                </div>
                                            </div>
                                        </div>
                                    }.into_any()
                                }

                                InspectorTab::AiData => {
                                    let aud = audience.get();
                                    let briefing = generate_briefing(&snap, "", aud);
                                    let md_text = briefing.markdown.clone();
                                    let snap_csv = snap.clone();
                                    let snap_gj = snap.clone();

                                    view! {
                                        <div class="tab-pane">
                                            <div class="section-block card-surface">
                                                <div class="sec-head">
                                                    <h3>"Tanya Data & Kendali Peta AI"</h3>
                                                    <span class="sec-meta">"Rust Engine"</span>
                                                </div>
                                                <form
                                                    class="ai-form"
                                                    on:submit=move |ev| {
                                                        ev.prevent_default();
                                                        run_ai_search(None);
                                                    }
                                                >
                                                    <input
                                                        type="search"
                                                        class="text-input"
                                                        placeholder="Cth: ciliwung, rob semarang, siaga jawa..."
                                                        prop:value=move || ai_query_text.get()
                                                        on:input=move |ev| ai_query_text.set(event_target_value(&ev))
                                                    />
                                                    <button type="submit" class="btn-primary" disabled=move || ai_busy.get()>
                                                        {move || if ai_busy.get() { "..." } else { "Tanya" }}
                                                    </button>
                                                </form>
                                                <div class="filter-pills-wrap">
                                                    {["ciliwung", "rob semarang", "barito", "banjir terparah"].into_iter().map(|preset| {
                                                        view! {
                                                            <button
                                                                type="button"
                                                                class="mini-chip"
                                                                on:click=move |_| run_ai_search(Some(preset.to_string()))
                                                            >{preset}</button>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                                <Show when=move || ai_result.get().is_some()>
                                                    {move || ai_result.get().map(|res| {
                                                        let fly_id = res.fly_to.as_ref().and_then(|f| f.station_id.clone());
                                                        let fly_lon = res.fly_to.as_ref().map(|f| f.lon).unwrap_or(118.0);
                                                        let fly_lat = res.fly_to.as_ref().map(|f| f.lat).unwrap_or(-2.5);
                                                        view! {
                                                            <div class="ai-answer">
                                                                <p>{res.answer_id}</p>
                                                                {fly_id.map(|fid| view! {
                                                                    <button
                                                                        type="button"
                                                                        class="mini-chip"
                                                                        on:click=move |_| select_station(fid.clone(), fly_lon, fly_lat)
                                                                    >"Buka Hidrograf Stasiun Ini →"</button>
                                                                })}
                                                            </div>
                                                        }
                                                    })}
                                                </Show>
                                            </div>

                                            <div class="section-block card-surface">
                                                <div class="sec-head">
                                                    <h3>{format!("AI SitRep ({})", aud.label())}</h3>
                                                    <button
                                                        type="button"
                                                        class="mini-chip"
                                                        on:click=move |_| {
                                                            copy_text(&md_text);
                                                            md_copied.set(true);
                                                        }
                                                    >{move || if md_copied.get() { "Tersalin MD" } else { "Salin Markdown" }}</button>
                                                </div>
                                                <div class="filter-pills-wrap">
                                                    {[(Audience::Public, "Publik"), (Audience::Gov, "BPBD"), (Audience::Academia, "Riset"), (Audience::AiAgent, "AI Agent")].into_iter().map(|(a_val, a_lbl)| {
                                                        view! {
                                                            <button
                                                                type="button"
                                                                class="mini-chip"
                                                                aria-pressed=move || (audience.get() == a_val).to_string()
                                                                on:click=move |_| audience.set(a_val)
                                                            >{a_lbl}</button>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                                <strong class="sitrep-head">{briefing.headline}</strong>
                                                <p class="sitrep-body">{briefing.sitrep_summary}</p>
                                                <p class="sitrep-mech">{briefing.mechanism_breakdown}</p>
                                                <ul class="sitrep-list">
                                                    {briefing.priority_actions.into_iter().map(|act| view! { <li>{act}</li> }).collect_view()}
                                                </ul>
                                            </div>

                                            <div class="section-block card-surface">
                                                <h3>"Ekspor Terbuka & AI Agent Discovery"</h3>
                                                <div class="detail-actions">
                                                    <button
                                                        type="button"
                                                        class="btn-secondary"
                                                        on:click=move |_| {
                                                            let mut csv = String::from("id,name,river,province,island,lat,lon,alert,mechanism,discharge_m3s,p75_m3s,anomaly_ratio,peak_7d_m3s,rain_72h_mm\n");
                                                            for s in &snap_csv.stations {
                                                                csv.push_str(&format!(
                                                                    "{},\"{}\",\"{}\",\"{}\",{},{:.4},{:.4},{},{},{:.2},{:.2},{:.2},{:.2},{:.2}\n",
                                                                    s.id, s.name, s.river, s.province, s.island, s.lat, s.lon,
                                                                    s.alert_level.label(), s.mechanism.slug(),
                                                                    s.current_discharge_m3s, s.p75_baseline_m3s, s.anomaly_ratio,
                                                                    s.peak_forecast_m3s, s.rain_72h_mm
                                                                ));
                                                            }
                                                            download_text_file("hidrosentinel-indonesia.csv", "text/csv", &csv);
                                                        }
                                                    >"Unduh CSV Nasional"</button>
                                                    <button
                                                        type="button"
                                                        class="btn-secondary"
                                                        on:click=move |_| {
                                                            let (_, st_gj, _, _) = crate::bridge::build_map_geojsons(&snap_gj, None);
                                                            download_text_file("hidrosentinel-stations.geojson", "application/geo+json", &st_gj);
                                                        }
                                                    >"Unduh GeoJSON"</button>
                                                </div>
                                                <div class="api-links-row">
                                                    <a href="/llms.txt" target="_blank">"/llms.txt"</a>
                                                    <a href="/api/openapi.json" target="_blank">"/api/openapi.json"</a>
                                                    <a href="/api/alerts.rss" target="_blank">"/api/alerts.rss"</a>
                                                    <a href="/api/health" target="_blank">"/api/health"</a>
                                                </div>
                                            </div>
                                        </div>
                                    }.into_any()
                                }
                            }
                        })}
                    </div>
                </Show>
            </aside>
        </>
    }
}
