use crate::format::{fmt_day, fmt_day_tick, fmt_num, fmt_q};
use flood_shared::model::StationTelemetry;
use leptos::prelude::*;

const W: f64 = 360.0;
const H: f64 = 224.0;
const PAD_L: f64 = 44.0;
const PAD_R: f64 = 34.0;
const PAD_T: f64 = 22.0;
const PAD_B: f64 = 28.0;

/// Langkah sumbu "bagus" (1, 2, 2,5, 5 × 10^n) untuk ±`target` interval.
fn nice_step(max: f64, target: f64) -> f64 {
    let raw = (max / target).max(f64::MIN_POSITIVE);
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

fn tick_label(v: f64, step: f64) -> String {
    let dec = if step >= 1.0 {
        0
    } else if step >= 0.1 {
        1
    } else {
        2
    };
    fmt_num(v, dec)
}

fn path_from(pts: &[(f64, f64)]) -> String {
    pts.iter()
        .enumerate()
        .map(|(i, (x, y))| format!("{}{x:.1},{y:.1}", if i == 0 { "M" } else { "L" }))
        .collect::<Vec<_>>()
        .join(" ")
}

fn band_from(upper: &[(f64, f64)], lower: &[(f64, f64)]) -> String {
    upper
        .iter()
        .chain(lower.iter().rev())
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[component]
pub fn HydrographChart(station: StationTelemetry) -> impl IntoView {
    let pts = station.hydrograph.clone();
    if pts.len() < 2 {
        return view! { <p class="empty-msg">"Hidrograf belum tersedia untuk titik ini."</p> }
            .into_any();
    }

    let plot_w = W - PAD_L - PAD_R;
    let plot_h = H - PAD_T - PAD_B;
    let n = pts.len();
    let x_at = move |i: usize| PAD_L + (i as f64 / (n - 1) as f64) * plot_w;

    // Skala debit: data + ensemble; ambang hanya dimasukkan bila dekat dengan data.
    let data_max = pts
        .iter()
        .map(|p| {
            p.discharge_max_m3s
                .unwrap_or(p.discharge_m3s)
                .max(p.discharge_m3s)
        })
        .fold(0.0_f64, f64::max);
    let th = station.thresholds.clone();
    let mut scale_max = data_max;
    let mut lines: Vec<(&'static str, f64)> = Vec::new();
    if let Some(t) = &th {
        for (lbl, v) in [("Q2", t.q2_m3s), ("Q5", t.q5_m3s), ("Q20", t.q20_m3s)] {
            if v.is_finite() && v > 0.0 {
                lines.push((lbl, v));
                if lbl == "Q2" || v <= data_max * 1.6 {
                    scale_max = scale_max.max(v);
                }
            }
        }
    }
    let step = nice_step(scale_max.max(1e-3) * 1.05, 4.0);
    let y_top = (scale_max * 1.05 / step).ceil().max(1.0) * step;
    let y_at = move |q: f64| PAD_T + plot_h - (q.clamp(0.0, y_top) / y_top) * plot_h;
    let y_ticks: Vec<f64> = (0..=((y_top / step).round() as usize))
        .map(|k| k as f64 * step)
        .collect();

    // Hujan: batang terbalik dari atas, sumbu kanan, maks 32% tinggi plot.
    let rain_max = pts.iter().filter_map(|p| p.rain_mm).fold(0.0_f64, f64::max);
    let has_rain = pts.iter().any(|p| p.rain_mm.is_some());
    let rain_step = nice_step(rain_max.max(10.0), 2.0);
    let rain_top = (rain_max.max(10.0) / rain_step).ceil() * rain_step;
    let rain_h = plot_h * 0.32;
    let bar_w = (plot_w / n as f64 * 0.62).clamp(2.0, 12.0);
    let rain_bars: Vec<(f64, f64, f64, bool)> = pts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let r = p.rain_mm?;
            (r > 0.05).then(|| {
                (
                    x_at(i) - bar_w / 2.0,
                    (r / rain_top) * rain_h,
                    r,
                    p.is_forecast,
                )
            })
        })
        .collect();

    let first_fc = pts.iter().position(|p| p.is_forecast);
    // Hari ini = titik non-prakiraan terakhir (lihat hub.rs build_hydrograph).
    let today_idx = first_fc.map(|i| i.saturating_sub(1)).unwrap_or(n - 1);
    let sim: Vec<(f64, f64)> = pts
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.is_forecast)
        .map(|(i, p)| (x_at(i), y_at(p.discharge_m3s)))
        .collect();
    let fc_start = first_fc.map(|i| i.saturating_sub(1)).unwrap_or(n);
    let fc: Vec<(f64, f64)> = pts
        .iter()
        .enumerate()
        .skip(fc_start)
        .map(|(i, p)| (x_at(i), y_at(p.discharge_m3s)))
        .collect();

    let band = |lo: fn(&flood_shared::model::HydrographPoint) -> Option<f64>,
                hi: fn(&flood_shared::model::HydrographPoint) -> Option<f64>|
     -> Option<String> {
        let mut up = Vec::new();
        let mut dn = Vec::new();
        for (i, p) in pts.iter().enumerate().filter(|(_, p)| p.is_forecast) {
            if let (Some(l), Some(h)) = (lo(p), hi(p)) {
                up.push((x_at(i), y_at(h)));
                dn.push((x_at(i), y_at(l)));
            }
        }
        (up.len() >= 2).then(|| band_from(&up, &dn))
    };
    let band_minmax = band(|p| p.discharge_min_m3s, |p| p.discharge_max_m3s);
    let band_iqr = band(|p| p.discharge_p25_m3s, |p| p.discharge_p75_m3s);
    let has_minmax = band_minmax.is_some();
    let has_iqr = band_iqr.is_some();
    let has_fc = fc.len() >= 2;

    // Label tanggal: kira-kira tiap 4 hari + hari ini.
    let every = (n / 5).max(1);
    let x_labels: Vec<(f64, String, bool)> = pts
        .iter()
        .enumerate()
        .filter(|(i, _)| *i == today_idx || (i % every == 0 && i.abs_diff(today_idx) * 2 > every))
        .map(|(i, p)| (x_at(i), fmt_day_tick(&p.date), i == today_idx))
        .collect();

    let x_today = x_at(today_idx);
    let threshold_marks: Vec<(String, f64, bool, &'static str)> = lines
        .iter()
        .map(|(lbl, v)| {
            let inside = *v <= y_top;
            (
                format!("{lbl} {}", fmt_q(*v)),
                if inside { y_at(*v) } else { PAD_T },
                inside,
                match *lbl {
                    "Q2" => "hg-th hg-q2",
                    "Q5" => "hg-th hg-q5",
                    _ => "hg-th hg-q20",
                },
            )
        })
        .collect();
    let above: Vec<String> = threshold_marks
        .iter()
        .filter(|m| !m.2)
        .map(|m| m.0.clone())
        .collect();

    let desc = {
        let first = pts.first().map(|p| fmt_day(&p.date)).unwrap_or_default();
        let last = pts.last().map(|p| fmt_day(&p.date)).unwrap_or_default();
        let thr = th
            .as_ref()
            .map(|t| format!(" Ambang Q2 {} m³/s.", fmt_q(t.q2_m3s)))
            .unwrap_or_else(|| " Ambang belum terkalibrasi.".into());
        format!(
            "Debit {} dari {first} sampai {last}. Debit hari ini {} m³/s, puncak prakiraan {} m³/s pada {}.{thr}",
            station.name,
            fmt_q(station.current_discharge_m3s),
            fmt_q(station.peak_forecast_m3s),
            fmt_day(&station.peak_forecast_date)
        )
    };

    let y_axis = y_ticks
        .iter()
        .map(|v| {
            let y = y_at(*v);
            let lbl = tick_label(*v, step);
            view! {
                <line x1=PAD_L x2=W - PAD_R y1=format!("{y:.1}") y2=format!("{y:.1}") class="hg-grid"/>
                <text x=PAD_L - 5.0 y=format!("{:.1}", y + 3.5) text-anchor="end" class="hg-tick">{lbl}</text>
            }
        })
        .collect_view();

    view! {
        <figure class="hydro">
            <figcaption class="hydro-cap">
                <span>"Debit harian di sel model"</span>
                <span class="muted">"m³/s"</span>
            </figcaption>
            <svg class="hydro-svg" viewBox=format!("0 0 {W} {H}") role="img" aria-labelledby="hg-title hg-desc">
                <title id="hg-title">{format!("Hidrograf {}", station.name)}</title>
                <desc id="hg-desc">{desc}</desc>
                {y_axis}
                <text x=PAD_L - 5.0 y=PAD_T - 9.0 text-anchor="end" class="hg-unit">"m³/s"</text>

                {has_rain.then(|| view! {
                    <text x=W - PAD_R + 5.0 y=PAD_T - 9.0 text-anchor="start" class="hg-unit">"mm/hari"</text>
                    <text x=W - PAD_R + 5.0 y=format!("{:.1}", PAD_T + 4.0) class="hg-tick">"0"</text>
                    <text x=W - PAD_R + 5.0 y=format!("{:.1}", PAD_T + rain_h + 3.0) class="hg-tick">{fmt_num(rain_top, 0)}</text>
                    <line x1=W - PAD_R x2=W - PAD_R y1=PAD_T y2=PAD_T + rain_h class="hg-axis-r"/>
                })}
                {rain_bars.into_iter().map(|(x, h, r, f)| view! {
                    <rect x=format!("{x:.1}") y=PAD_T width=format!("{bar_w:.1}") height=format!("{h:.1}")
                        class=if f { "hg-rain is-fc" } else { "hg-rain" }>
                        <title>{format!("{} mm", fmt_num(r, 1))}</title>
                    </rect>
                }).collect_view()}

                <rect x=format!("{x_today:.1}") y=PAD_T width=format!("{:.1}", (W - PAD_R - x_today).max(0.0)) height=plot_h class="hg-fc-zone"/>

                {band_minmax.map(|b| view! { <polygon points=b class="hg-band-minmax"/> })}
                {band_iqr.map(|b| view! { <polygon points=b class="hg-band-iqr"/> })}

                {threshold_marks.into_iter().filter(|m| m.2).map(|(lbl, y, _, cls)| view! {
                    <line x1=PAD_L x2=W - PAD_R y1=format!("{y:.1}") y2=format!("{y:.1}") class=cls/>
                    <text x=PAD_L + 4.0 y=format!("{:.1}", y - 3.5) class=format!("{cls} hg-th-lbl")>{lbl}</text>
                }).collect_view()}

                <line x1=format!("{x_today:.1}") x2=format!("{x_today:.1}") y1=PAD_T y2=PAD_T + plot_h class="hg-today"/>
                <text x=format!("{:.1}", x_today + 4.0) y=PAD_T + plot_h - 5.0 class="hg-today-lbl">"Hari ini"</text>

                <path d=path_from(&sim) class="hg-sim"/>
                {has_fc.then(|| view! { <path d=path_from(&fc) class="hg-fc"/> })}

                <line x1=PAD_L x2=W - PAD_R y1=PAD_T + plot_h y2=PAD_T + plot_h class="hg-axis"/>
                {x_labels.into_iter().map(|(x, lbl, is_today)| view! {
                    <text x=format!("{x:.1}") y=H - 9.0 text-anchor="middle"
                        class=if is_today { "hg-tick is-today" } else { "hg-tick" }>{lbl}</text>
                }).collect_view()}
            </svg>
            {(!above.is_empty()).then(|| view! {
                <p class="hydro-note">{format!("Di atas skala: {}.", above.join(", "))}</p>
            })}
            <ul class="hydro-legend">
                <li><i class="lg lg-sim"></i>"Simulasi GloFAS"</li>
                {has_fc.then(|| view! { <li><i class="lg lg-fc"></i>"Prakiraan"</li> })}
                {has_iqr.then(|| view! { <li><i class="lg lg-iqr"></i>"Ensemble 25–75%"</li> })}
                {has_minmax.then(|| view! { <li><i class="lg lg-minmax"></i>"Ensemble min–maks"</li> })}
                {(!lines.is_empty()).then(|| view! { <li><i class="lg lg-th"></i>"Ambang periode ulang"</li> })}
                {has_rain.then(|| view! { <li><i class="lg lg-rain"></i>"Hujan"</li> })}
            </ul>
            <p class="hydro-foot">"Bukan pengukuran pos duga air. Simulasi model hidrologi GloFAS v4 pada sel 0,05°."</p>
        </figure>
    }
    .into_any()
}
