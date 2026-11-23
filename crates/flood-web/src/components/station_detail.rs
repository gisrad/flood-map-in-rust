use crate::bridge::{copy_text, download_text_file, open_whatsapp_text, station_url};
use crate::components::hydrograph::HydrographChart;
use crate::components::ui::{use_toast, Disclaimer, LevelBadge, OfficialRefs, Shape, ShapeGlyph};
use crate::format::{
    days_between, fmt_datetime_long, fmt_day, fmt_num, fmt_pct, fmt_q, fmt_ratio, local_date_iso,
    parse_utc_ms, tz_label,
};
use flood_shared::model::{
    Audience, CoastalRobTelemetry, FloodMechanism, GrrrCheck, GrrrVerdict, InundationExposure,
    StationTelemetry,
};
use leptos::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

/// Entri `public/data/gem15-cells.json` (milik worker aset): station_id → sel GEM-15.
#[derive(Debug, Clone, Deserialize)]
pub struct Gem15Cell {
    #[serde(default)]
    pub cell_id: Option<String>,
    #[serde(default)]
    pub item_url: Option<String>,
    #[serde(default)]
    pub dtm_url: Option<String>,
}

/// Peta sel GEM-15 per stasiun; kosong bila berkas tidak tersedia.
#[derive(Clone, Copy)]
pub struct Gem15Ctx(pub RwSignal<HashMap<String, Option<Gem15Cell>>>);

fn safe_http(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

const GEOJSON_URL: &str = "https://flood.transbulent.id/api/stations.geojson";

pub fn geolibre_url() -> String {
    let enc: String = js_sys::encode_uri_component(GEOJSON_URL).into();
    format!("https://web.geolibre.app/?data={enc}")
}

/// Hari menuju puncak prakiraan, dihitung pada tanggal lokal stasiun.
pub fn lead_days(st: &StationTelemetry, generated_at_utc: &str) -> Option<i64> {
    let gen = parse_utc_ms(generated_at_utc)?;
    days_between(
        &local_date_iso(gen, st.tz_offset_hours),
        &st.peak_forecast_date,
    )
}

pub fn lead_label(days: Option<i64>) -> String {
    match days {
        Some(d) if d <= 0 => "hari ini".into(),
        Some(1) => "besok".into(),
        Some(d) => format!("{d} hari lagi"),
        None => "–".into(),
    }
}

fn media_quote(st: &StationTelemetry, generated_at_utc: &str) -> String {
    let when = parse_utc_ms(generated_at_utc)
        .map(|ms| fmt_datetime_long(ms, 7))
        .unwrap_or_default();
    let thr = st
        .thresholds
        .as_ref()
        .map(|t| {
            format!(
                "; ambang periode ulang 2 tahun (Q2) {} m³/s",
                fmt_q(t.q2_m3s)
            )
        })
        .unwrap_or_else(|| "; ambang belum terkalibrasi".into());
    format!(
        "{} ({}): level {} — indikasi model, bukan peringatan resmi. Debit simulasi {} m³/s, puncak prakiraan {} m³/s pada {}{}. Data per {}. Sumber: GloFAS v4 via Open-Meteo (CC BY 4.0), diolah Hidrosentinel Nusantara. {}",
        st.name,
        st.province,
        st.level.label_id(),
        fmt_q(st.current_discharge_m3s),
        fmt_q(st.peak_forecast_m3s),
        fmt_day(&st.peak_forecast_date),
        thr,
        when,
        station_url(&st.id)
    )
}

#[component]
fn ShareRow(id: String, whatsapp: String, quote: String) -> impl IntoView {
    let toast = use_toast();
    let id_c = id.clone();
    view! {
        <div class="share-row">
            <button type="button" class="btn" on:click=move |_| {
                copy_text(&station_url(&id_c));
                toast.show("Tautan disalin");
            }>"Salin tautan"</button>
            <button type="button" class="btn" on:click=move |_| open_whatsapp_text(&whatsapp)>"WhatsApp"</button>
            <button type="button" class="btn" on:click=move |_| {
                copy_text(&quote);
                toast.show("Kutipan untuk media disalin");
            }>"Salin kutipan media"</button>
        </div>
    }
}

/// Riwayat genangan Google (Inundation History 1999–2020) dalam radius sekitar titik.
#[component]
fn InundationBlock(exposure: InundationExposure, what: &'static str) -> impl IntoView {
    let (low, medium, high) = exposure.fractions();
    let pct = |f: f64| fmt_num(f * 100.0, 0);
    let rows = [
        ("high", "Sering basah (≥ 5% waktu)", exposure.high_km2, high),
        (
            "medium",
            "Kadang basah (≥ 1% waktu)",
            exposure.medium_km2,
            medium,
        ),
        ("low", "Jarang basah (≥ 0,5% waktu)", exposure.low_km2, low),
    ];
    let aria = format!(
        "Dalam radius {} km: {} persen sering basah, {} persen kadang basah, {} persen jarang basah",
        fmt_num(exposure.radius_km, 0),
        pct(high),
        pct(medium),
        pct(low)
    );
    view! {
        <section class="block inund">
            <h3 class="h-sub">"Riwayat genangan sekitar titik"</h3>
            <div class="inund-bar" role="img" aria-label=aria>
                <span class="inund-seg inund-high" style=format!("width:{:.1}%", high * 100.0)></span>
                <span class="inund-seg inund-medium" style=format!("width:{:.1}%", (medium - high) * 100.0)></span>
                <span class="inund-seg inund-low" style=format!("width:{:.1}%", (low - medium) * 100.0)></span>
            </div>
            <ul class="inund-legend">
                {rows.into_iter().map(|(cls, lbl, km2, f)| view! {
                    <li><span class=format!("inund-sw inund-{cls}")></span><span>{lbl}</span>
                        <b class="num">{format!("{} km² · {}%", fmt_num(km2, 1), pct(f))}</b></li>
                }).collect_view()}
            </ul>
            <p class="fine">{format!(
                "Dalam radius {} km dari {} ({} km²), citra satelit 1999–2020 menunjukkan permukaan basah pada persentase waktu di atas. Dapat mencakup sawah tergenang, tambak, waduk, dan dataran pasang surut, bukan hanya banjir. Ini riwayat, bukan prakiraan, dan bukan peta rawan banjir resmi. Sumber: Google Research, Inundation History (CC BY 4.0), berdasarkan GLAD.",
                fmt_num(exposure.radius_km, 0),
                what,
                fmt_num(exposure.buffer_km2, 1)
            )}</p>
        </section>
    }
}

/// Baris lensa Riset: uji kewajaran silang Q2 GloFAS terhadap GRRR (Google).
fn grrr_facts(g: &GrrrCheck) -> impl IntoView {
    let radius = fmt_num(g.radius_km, 0);
    let range = match (g.q2_min_m3s, g.q2_max_m3s) {
        (Some(lo), Some(hi)) => format!(
            "{} titik · Q2 {}–{} m³/s",
            g.n_outlets,
            fmt_q(lo),
            fmt_q(hi)
        ),
        _ => "tidak ada titik".to_string(),
    };
    let verdict = match g.verdict {
        GrrrVerdict::WithinRange => "Q2 GloFAS dalam rentang Q2 GRRR di sekitar pos (uji kasar, model terhadap model)",
        GrrrVerdict::GlofasAbove => "Q2 GloFAS lebih dari 2× Q2 GRRR terbesar di sekitar pos: verifikasi dengan pos duga air",
        GrrrVerdict::GlofasBelow => "Q2 GloFAS kurang dari 0,5× Q2 GRRR terkecil di sekitar pos: verifikasi dengan pos duga air",
        GrrrVerdict::NoOutlets => "Tidak ada titik keluar sub-DAS GRRR dalam radius pos",
    };
    let closest = g.closest.clone().map(|c| {
        view! {
            <div><dt>"GRRR dengan Q2 terdekat"</dt><dd class="num">{format!(
                "Q2 {} · Q5 {} · Q20 {} m³/s, {} km dari pos (GloFAS/GRRR {})",
                fmt_q(c.q2_m3s), fmt_q(c.q5_m3s), fmt_q(c.q20_m3s), fmt_num(c.distance_km, 1),
                fmt_ratio(g.ratio_q2.unwrap_or(f64::NAN))
            )}</dd></div>
        }
    });
    view! {
        <div><dt>{format!("GRRR Google ≤ {radius} km")}</dt><dd class="num">{range}</dd></div>
        {closest}
        <div><dt>"Uji silang Q2"</dt><dd>{verdict}</dd></div>
    }
}

#[component]
pub fn StationDetail(
    station: StationTelemetry,
    generated_at_utc: String,
    lens: Audience,
    /// Replay historis: tanpa prakiraan, hujan ERA5, dan tanpa tombol berbagi.
    replay: bool,
    on_back: Callback<()>,
) -> impl IntoView {
    let st = station;
    let shape = Shape::for_mechanism(st.mechanism);
    let lead = lead_days(&st, &generated_at_utc);
    let quote = media_quote(&st, &generated_at_utc);
    let wa = format!(
        "{}\n\nIndikasi model, bukan peringatan resmi. Ikuti BPBD setempat.",
        quote
    );
    let st_csv = st.clone();
    let meaning = match lens {
        Audience::Public => st.public_advice.clone(),
        Audience::Gov => st.gov_action.clone(),
        Audience::Academia | Audience::AiAgent => st.academic_note.clone(),
    };
    let tz = tz_label(st.tz_offset_hours);

    let thresholds_view = match st.thresholds.clone() {
        Some(t) => view! {
            <div class="thr-grid">
                {[("Q2", t.q2_m3s, "high"), ("Q5", t.q5_m3s, "very_high"), ("Q20", t.q20_m3s, "extreme")]
                    .into_iter()
                    .map(|(lbl, v, lv)| {
                        let passed = st.current_discharge_m3s >= v;
                        view! {
                            <div class=format!("thr-cell thr-{lv}") class:is-passed=passed>
                                <span>{lbl}</span>
                                <b>{fmt_q(v)}</b>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
            <p class="fine">{format!("Ambang periode ulang 2, 5, dan 20 tahun (m³/s), reanalisis GloFAS v4 {} · {} tahun.", t.period, t.n_years)}</p>
        }
        .into_any(),
        None => view! {
            <p class="note-unknown">"Belum terkalibrasi: ambang periode ulang belum tersedia untuk sel model ini, sehingga level sungai tidak dinilai."</p>
        }
        .into_any(),
    };

    let exceed_view = st.exceed_prob_7d.clone().map(|e| {
        view! {
            <section class="block">
                <h3 class="h-sub">"Peluang melampaui ambang (7 hari)"</h3>
                <div class="prob-list">
                    {[("Q2", e.q2, "high"), ("Q5", e.q5, "very_high"), ("Q20", e.q20, "extreme")]
                        .into_iter()
                        .map(|(lbl, p, lv)| view! {
                            <div class="prob-row">
                                <span class="prob-lbl">{lbl}</span>
                                <span class="prob-track"><span class=format!("prob-fill lvl-bg-{lv}") style=format!("width:{:.1}%", (p * 100.0).clamp(0.0, 100.0))></span></span>
                                <b class="num">{fmt_pct(p)}</b>
                            </div>
                        })
                        .collect_view()}
                </div>
                <p class="fine">{format!("Fraksi dari {} anggota ensemble GloFAS.", e.members)}</p>
            </section>
        }
    });

    let lens_extra = match lens {
        Audience::Gov => view! {
            <section class="block">
                <h3 class="h-sub">"Untuk BPBD / Pemda"</h3>
                <dl class="facts">
                    {(!replay).then(|| view! {
                        <div><dt>"Puncak prakiraan"</dt><dd>{format!("{} ({})", fmt_day(&st.peak_forecast_date), lead_label(lead))}</dd></div>
                        <div><dt>"Level puncak"</dt><dd><LevelBadge level=st.peak_forecast_level/></dd></div>
                        <div><dt>"Laporan warga ≤10 km, 24 jam"</dt><dd class="num">{st.nearby_reports_24h}</dd></div>
                    })}
                    <div><dt>"Zona waktu"</dt><dd>{tz.clone()}</dd></div>
                </dl>
                <button type="button" class="btn" on:click=move |_| {
                    let mut csv = String::from("date,is_forecast,discharge_m3s,min_m3s,p25_m3s,p75_m3s,max_m3s,rain_mm\n");
                    let o = |v: Option<f64>| v.map(|x| format!("{x:.3}")).unwrap_or_default();
                    for p in &st_csv.hydrograph {
                        csv.push_str(&format!(
                            "{},{},{:.3},{},{},{},{},{}\n",
                            p.date, p.is_forecast, p.discharge_m3s,
                            o(p.discharge_min_m3s), o(p.discharge_p25_m3s), o(p.discharge_p75_m3s),
                            o(p.discharge_max_m3s), o(p.rain_mm)
                        ));
                    }
                    download_text_file(&format!("hidrosentinel-{}-hidrograf.csv", st_csv.id), "text/csv", &csv);
                }>"Unduh hidrograf (CSV)"</button>
            </section>
        }
        .into_any(),
        Audience::Academia => {
            let cell = st.glofas_cell.clone();
            let th = st.thresholds.clone();
            let sid = st.id.clone();
            let gem_ctx = use_context::<Gem15Ctx>();
            let gem_view = move || {
                let g = gem_ctx?.0.with(|m| m.get(&sid).cloned().flatten())?;
                let dtm = g.dtm_url.filter(|u| safe_http(u));
                let item = g.item_url.filter(|u| safe_http(u));
                (dtm.is_some() || item.is_some()).then(|| view! {
                    <div class="gem15">
                        <span class="h-sub">"DEM 15 m GEM-15 (Google, CC BY 4.0)"</span>
                        <div class="btn-row">
                            {dtm.map(|u| view! { <a class="btn" href=u target="_blank" rel="noopener">"Unduh DTM"</a> })}
                            {item.map(|u| view! { <a class="btn" href=u target="_blank" rel="noopener">"Item STAC"</a> })}
                        </div>
                        {g.cell_id.map(|c| view! { <p class="fine">{format!("Sel {c}")}</p> })}
                    </div>
                })
            };
            view! {
                <section class="block">
                    <h3 class="h-sub">"Untuk riset"</h3>
                    <dl class="facts">
                        <div><dt>"Pos (lintang, bujur)"</dt><dd class="num">{format!("{}, {}", fmt_num(st.lat, 4), fmt_num(st.lon, 4))}</dd></div>
                        {match cell {
                            Some(c) => view! {
                                <div><dt>"Sel GloFAS"</dt><dd class="num">{format!("{}, {}", fmt_num(c.lat, 3), fmt_num(c.lon, 3))}</dd></div>
                                <div><dt>"Jarak sel–pos"</dt><dd class="num">{format!("{} km", fmt_num(c.offset_km, 1))}</dd></div>
                                <div><dt>"Debit rerata sel"</dt><dd class="num">{format!("{} m³/s", fmt_q(c.mean_q_m3s))}</dd></div>
                            }.into_any(),
                            None => view! { <div><dt>"Sel GloFAS"</dt><dd>"belum dikalibrasi"</dd></div> }.into_any(),
                        }}
                        <div><dt>"Luas DAS (katalog)"</dt><dd class="num">{format!("{} km²", fmt_num(st.basin_area_km2 as f64, 0))}</dd></div>
                        {th.map(|t| view! {
                            <div><dt>"P90 / P99 harian"</dt><dd class="num">{format!("{} / {} m³/s", fmt_q(t.p90_m3s), fmt_q(t.p99_m3s))}</dd></div>
                            <div><dt>"Periode klimatologi"</dt><dd>{format!("{} ({} tahun)", t.period, t.n_years)}</dd></div>
                        })}
                        {st.ratio_q2.map(|r| view! { <div><dt>"Q / Q2"</dt><dd class="num">{fmt_ratio(r)}</dd></div> })}
                        {st.grrr_check.clone().map(|g| grrr_facts(&g))}
                        <div><dt>"Lengas tanah 0–7 / 28–100 cm"</dt><dd class="num">{format!("{} / {} m³/m³", fmt_num(st.soil_moisture_top, 2), fmt_num(st.soil_moisture_deep, 2))}</dd></div>
                    </dl>
                    <div class="btn-row">
                        <a class="btn" href="/api/stations.geojson" download="hidrosentinel-stations.geojson">"Unduh GeoJSON"</a>
                        <a class="btn" href=geolibre_url() target="_blank" rel="noopener">"Buka di GeoLibre"</a>
                    </div>
                    {gem_view}
                </section>
            }
            .into_any()
        }
        Audience::AiAgent => {
            let id = st.id.clone();
            view! {
                <section class="block">
                    <h3 class="h-sub">"Untuk agen AI"</h3>
                    <pre class="code">{format!("GET /api/flood\n.stations[] | select(.id == \"{id}\")")}</pre>
                </section>
            }
            .into_any()
        }
        Audience::Public => ().into_any(),
    };

    let id_share = st.id.clone();
    // Debit sel yang belum terkalibrasi bukan debit sungai di pos ini: tidak ditampilkan.
    let calibrated = st.thresholds.is_some();
    let q_unit = if calibrated { " m³/s" } else { "" };
    let q_now = if calibrated {
        fmt_q(st.current_discharge_m3s)
    } else {
        "–".to_string()
    };
    let q_peak = if calibrated {
        fmt_q(st.peak_forecast_m3s)
    } else {
        "–".to_string()
    };
    let offset_note = st
        .glofas_cell
        .as_ref()
        .filter(|c| c.offset_km > 10.0)
        .map(|c| {
            format!(
                "Sel model GloFAS berjarak {} km dari pos; indikasi sungai dapat berbeda dari kondisi di pos.",
                fmt_num(c.offset_km, 0)
            )
        });
    let grrr_note = st.grrr_check.as_ref().and_then(|g| match g.verdict {
        GrrrVerdict::GlofasAbove => Some("Pembanding model Google (GRRR) memberi debit banjir 2 tahunan di sekitar pos yang jauh lebih rendah; level sungai di sini perlu diverifikasi dengan pos duga air."),
        GrrrVerdict::GlofasBelow => Some("Pembanding model Google (GRRR) memberi debit banjir 2 tahunan di sekitar pos yang jauh lebih tinggi; level sungai di sini perlu diverifikasi dengan pos duga air."),
        _ => None,
    });
    let floodhub = format!(
        "https://sites.research.google/floods/l/{:.4}/{:.4}/11",
        st.lat, st.lon
    );
    view! {
        <article class="detail">
            <button type="button" class="link-back" on:click=move |_| on_back.run(())>"← Semua titik"</button>
            <header class="detail-head">
                <h2>{st.name.clone()}</h2>
                <p class="muted">{format!("{} · {} · DAS {} km²", st.river, st.province, fmt_num(st.basin_area_km2 as f64, 0))}</p>
            </header>
            <div class="level-line">
                <ShapeGlyph shape=shape level=st.level/>
                <LevelBadge level=st.level/>
                <span class="muted">{st.mechanism.label_id()}</span>
            </div>
            <p class="reason">{st.level_reason.clone()}</p>
            {offset_note.map(|n| view! { <p class="muted">{n}</p> })}
            {grrr_note.map(|n| view! { <p class="muted">{n}</p> })}

            {if replay {
                view! {
                    <div class="metrics">
                        <div class="metric">
                            <span class="m-lbl">"Debit simulasi"</span>
                            <b class="m-val">{q_now.clone()}<small>{q_unit}</small></b>
                            <span class="m-sub">{match (st.ratio_q2, st.thresholds.is_some()) {
                                (Some(r), _) => format!("{} Q2", fmt_ratio(r)),
                                (None, false) => "Sel belum terkalibrasi".into(),
                                (None, true) => "Reanalisis GloFAS v4".into(),
                            }}</span>
                        </div>
                        <div class="metric">
                            <span class="m-lbl">"Hujan ERA5 24 jam"</span>
                            <b class="m-val">{fmt_num(st.rain_24h_mm, 0)}<small>" mm"</small></b>
                            <span class="m-sub">{format!("72 jam {} mm", fmt_num(st.rain_72h_mm, 0))}</span>
                        </div>
                    </div>
                }
                .into_any()
            } else {
                view! {
                    <div class="metrics">
                        <div class="metric">
                            <span class="m-lbl">"Debit simulasi hari ini"</span>
                            <b class="m-val">{q_now.clone()}<small>{q_unit}</small></b>
                            <span class="m-sub">{match (st.ratio_q2, st.thresholds.is_some()) {
                                (Some(r), _) => format!("{} Q2", fmt_ratio(r)),
                                (None, false) => "Sel belum terkalibrasi".into(),
                                (None, true) => format!("Tren {}", st.trend.label().to_lowercase()),
                            }}</span>
                        </div>
                        <div class="metric">
                            <span class="m-lbl">"Puncak prakiraan 7 hari"</span>
                            <b class="m-val">{q_peak.clone()}<small>{q_unit}</small></b>
                            <span class="m-sub">{format!("{} · {}", fmt_day(&st.peak_forecast_date), lead_label(lead))}</span>
                        </div>
                        <div class="metric">
                            <span class="m-lbl">"Hujan 24 jam"</span>
                            <b class="m-val">{fmt_num(st.rain_24h_mm, 0)}<small>" mm"</small></b>
                            <span class="m-sub">{format!("72 jam {} mm", fmt_num(st.rain_72h_mm, 0))}</span>
                        </div>
                        <div class="metric">
                            <span class="m-lbl">"Prakiraan hujan 24 jam"</span>
                            <b class="m-val">{fmt_num(st.rain_next_24h_mm, 0)}<small>" mm"</small></b>
                            <span class="m-sub">{format!("Drainase: {}", st.pluvial_level.label_id())}</span>
                        </div>
                    </div>
                }
                .into_any()
            }}

            <section class="block">
                <h3 class="h-sub">"Ambang periode ulang"</h3>
                {thresholds_view}
            </section>

            {(!replay).then(|| st.inundation_history.clone().map(|e| view! { <InundationBlock exposure=e what="pos"/> }))}

            {exceed_view}

            <HydrographChart station=st.clone()/>

            <section class="meaning">
                <h3 class="h-sub">"Apa artinya?"</h3>
                <p>{meaning}</p>
            </section>

            <a class="btn" href=floodhub target="_blank" rel="noopener">"Bandingkan di Google Flood Hub"</a>

            {lens_extra}

            {(!replay).then(|| view! { <ShareRow id=id_share.clone() whatsapp=wa.clone() quote=quote.clone()/> })}
            <Disclaimer/>
            <OfficialRefs/>
        </article>
    }
}

#[component]
pub fn RobDetail(
    rob: CoastalRobTelemetry,
    generated_at_utc: String,
    lens: Audience,
    /// Replay historis: tanpa nilai "kini" dan tanpa tombol berbagi.
    replay: bool,
    on_back: Callback<()>,
) -> impl IntoView {
    let rb = rob;
    let peak_time = parse_utc_ms(&rb.peak_msl_time)
        .map(|ms| crate::format::fmt_datetime_short(ms, rb.tz_offset_hours))
        .unwrap_or_else(|| rb.peak_msl_time.clone());
    let when = parse_utc_ms(&generated_at_utc)
        .map(|ms| fmt_datetime_long(ms, 7))
        .unwrap_or_default();
    let quote = format!(
        "{} ({}): rob level {} — indikasi model, bukan peringatan resmi. Puncak muka laut model 48 jam {} m (relatif MSL) pada {}; tinggi gelombang {} m. Data per {}. Sumber: Open-Meteo Marine, diolah Hidrosentinel Nusantara. {}",
        rb.name,
        rb.province,
        rb.level.label_id(),
        fmt_num(rb.peak_msl_48h_m, 2),
        peak_time,
        fmt_num(rb.wave_height_m, 1),
        when,
        station_url(&rb.id)
    );
    let wa = format!("{quote}\n\nIkuti BPBD setempat.");
    let thr = rb.thresholds.clone();
    let id_share = rb.id.clone();
    let id_ai = rb.id.clone();
    view! {
        <article class="detail">
            <button type="button" class="link-back" on:click=move |_| on_back.run(())>"← Semua titik"</button>
            <header class="detail-head">
                <h2>{rb.name.clone()}</h2>
                <p class="muted">{format!("Pesisir · {} · {}", rb.province, rb.island)}</p>
            </header>
            <div class="level-line">
                <ShapeGlyph shape=Shape::Diamond level=rb.level/>
                <LevelBadge level=rb.level/>
                <span class="muted">{FloodMechanism::CoastalRob.label_id()}</span>
            </div>
            <p class="reason">{rb.level_reason.clone()}</p>
            <div class="metrics">
                {(!replay).then(|| view! {
                    <div class="metric">
                        <span class="m-lbl">"Muka laut model kini"</span>
                        <b class="m-val">{fmt_num(rb.current_msl_m, 2)}<small>" m"</small></b>
                        <span class="m-sub">"relatif MSL"</span>
                    </div>
                })}
                <div class="metric">
                    <span class="m-lbl">"Puncak 48 jam"</span>
                    <b class="m-val">{fmt_num(rb.peak_msl_48h_m, 2)}<small>" m"</small></b>
                    <span class="m-sub">{peak_time}</span>
                </div>
                <div class="metric">
                    <span class="m-lbl">"Tinggi gelombang"</span>
                    <b class="m-val">{fmt_num(rb.wave_height_m, 1)}<small>" m"</small></b>
                    <span class="m-sub">"model"</span>
                </div>
            </div>
            <section class="block">
                <h3 class="h-sub">"Ambang muka laut setempat"</h3>
                {match thr {
                    Some(t) => view! {
                        <div class="thr-grid">
                            <div class="thr-cell"><span>"P95"</span><b>{format!("{} m", fmt_num(t.p95_m, 2))}</b></div>
                            <div class="thr-cell thr-high"><span>"P99"</span><b>{format!("{} m", fmt_num(t.p99_m, 2))}</b></div>
                            <div class="thr-cell thr-very_high"><span>"Maks"</span><b>{format!("{} m", fmt_num(t.max_m, 2))}</b></div>
                        </div>
                        <p class="fine">{format!("Persentil puncak muka laut model 48 jam, periode {}.", t.period)}</p>
                    }.into_any(),
                    None => view! { <p class="note-unknown">"Belum terkalibrasi: klimatologi muka laut belum tersedia untuk titik ini."</p> }.into_any(),
                }}
            </section>
            {(!replay).then(|| rb.inundation_history.clone().map(|e| view! { <InundationBlock exposure=e what="titik ini"/> }))}
            <section class="meaning">
                <h3 class="h-sub">"Apa artinya?"</h3>
                <p>{rb.note.clone()}</p>
            </section>
            {(lens == Audience::AiAgent).then(|| view! {
                <pre class="code">{format!("GET /api/flood\n.coastal_rob[] | select(.id == \"{id_ai}\")")}</pre>
            })}
            {(!replay).then(|| view! { <ShareRow id=id_share.clone() whatsapp=wa.clone() quote=quote.clone()/> })}
            <Disclaimer/>
            <OfficialRefs/>
        </article>
    }
}
