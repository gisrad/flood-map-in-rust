use flood_shared::model::StationTelemetry;
use leptos::prelude::*;

#[component]
pub fn HydrographChart(station: StationTelemetry) -> impl IntoView {
    let pts = station.hydrograph.clone();
    if pts.is_empty() {
        return view! { <div class="empty-msg">"Data hidrograf belum tersedia."</div> }.into_any();
    }

    let w = 356.0_f64;
    let h = 166.0_f64;
    let pad_l = 42.0_f64;
    let pad_r = 12.0_f64;
    let pad_t = 14.0_f64;
    let pad_b = 26.0_f64;
    let plot_w = w - pad_l - pad_r;
    let plot_h = h - pad_t - pad_b;

    let mut max_q = station.q95_threshold_m3s.max(station.p75_baseline_m3s * 1.15);
    let mut max_rain = 25.0_f64;
    for p in &pts {
        max_q = max_q.max(p.discharge_max_m3s).max(p.discharge_m3s);
        max_rain = max_rain.max(p.rain_mm);
    }
    max_q = (max_q * 1.12).max(5.0);

    let n = pts.len().max(2);
    let x_at = |i: usize| pad_l + (i as f64 / (n - 1) as f64) * plot_w;
    let y_at = |q: f64| pad_t + plot_h - (q.clamp(0.0, max_q) / max_q) * plot_h;

    let split_idx = pts
        .iter()
        .position(|p| p.is_forecast)
        .map(|i| i.saturating_sub(1))
        .unwrap_or(pts.len().saturating_sub(1));

    // Observed solid line (0..=split_idx)
    let obs_path: String = pts
        .iter()
        .enumerate()
        .take(split_idx + 1)
        .map(|(i, p)| {
            let cmd = if i == 0 { "M" } else { "L" };
            format!("{cmd}{:.1},{:.1}", x_at(i), y_at(p.discharge_m3s))
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Forecast dashed line (split_idx..n)
    let fc_path: String = pts
        .iter()
        .enumerate()
        .skip(split_idx)
        .enumerate()
        .map(|(k, (i, p))| {
            let cmd = if k == 0 { "M" } else { "L" };
            let q = if i == split_idx {
                p.discharge_m3s
            } else {
                p.discharge_mean_m3s
            };
            format!("{cmd}{:.1},{:.1}", x_at(i), y_at(q))
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Ensemble uncertainty band polygon for forecast window
    let mut band_pts = Vec::new();
    for (i, p) in pts.iter().enumerate().skip(split_idx) {
        let q_hi = if i == split_idx {
            p.discharge_m3s
        } else {
            p.discharge_max_m3s
        };
        band_pts.push(format!("{:.1},{:.1}", x_at(i), y_at(q_hi)));
    }
    for (i, p) in pts.iter().enumerate().skip(split_idx).rev() {
        let q_lo = if i == split_idx {
            p.discharge_m3s
        } else {
            (p.discharge_mean_m3s * 0.85).min(p.discharge_m3s)
        };
        band_pts.push(format!("{:.1},{:.1}", x_at(i), y_at(q_lo)));
    }
    let band_poly = band_pts.join(" ");

    // Rainfall bars at bottom (max 28% plot height)
    let bar_w = (plot_w / n as f64 * 0.58).max(3.0);
    let rain_bars: Vec<(String, String, String, String)> = pts
        .iter()
        .enumerate()
        .filter(|(_, p)| p.rain_mm > 0.2)
        .map(|(i, p)| {
            let bh = ((p.rain_mm / max_rain) * plot_h * 0.28).max(1.5);
            let x = x_at(i) - bar_w * 0.5;
            let y = pad_t + plot_h - bh;
            (
                format!("{x:.1}"),
                format!("{y:.1}"),
                format!("{bar_w:.1}"),
                format!("{bh:.1}"),
            )
        })
        .collect();

    let y_q75 = y_at(station.p75_baseline_m3s);
    let y_q95 = y_at(station.q95_threshold_m3s);
    let x_today = x_at(split_idx);
    let start_lbl = pts.first().map(|p| short_date(&p.date)).unwrap_or_default();
    let mid_lbl = pts.get(split_idx).map(|p| short_date(&p.date)).unwrap_or_default();
    let end_lbl = pts.last().map(|p| short_date(&p.date)).unwrap_or_default();

    let max_q_lbl = fmt_q(max_q);
    let mid_q_lbl = fmt_q(max_q * 0.5);
    let q75_lbl = format!("Q75 {}", fmt_q(station.p75_baseline_m3s));
    let q95_lbl = format!("Q95 {}", fmt_q(station.q95_threshold_m3s));

    view! {
        <div class="hydro-card">
            <div class="hydro-head">
                <span class="hydro-title">"Hidrograf Debit 14h + Prakiraan 7h (m³/s)"</span>
                <span class="hydro-unit">"GloFAS v4"</span>
            </div>
            <svg class="hydro-svg" viewBox="0 0 356 166" role="img" aria-label="Grafik hidrograf debit sungai dan prakiraan 7 hari">
                // Grid lines
                <line x1=pad_l y1=pad_t x2=w-pad_r y2=pad_t class="hg-grid"/>
                <line x1=pad_l y1=pad_t+plot_h*0.5 x2=w-pad_r y2=pad_t+plot_h*0.5 class="hg-grid"/>
                <line x1=pad_l y1=pad_t+plot_h x2=w-pad_r y2=pad_t+plot_h class="hg-axis"/>

                // Rainfall bars
                {rain_bars.into_iter().map(|(x, y, bw, bh)| view! {
                    <rect x=x y=y width=bw height=bh rx="1" class="hg-rain"/>
                }).collect_view()}

                // Q75 & Q95 baselines
                <line x1=pad_l y1=format!("{y_q75:.1}") x2=w-pad_r y2=format!("{y_q75:.1}") class="hg-q75"/>
                <text x=w-pad_r-2.0 y=format!("{:.1}", (y_q75 - 3.0).max(pad_t + 8.0)) text-anchor="end" class="hg-lbl-q75">{q75_lbl}</text>

                <line x1=pad_l y1=format!("{y_q95:.1}") x2=w-pad_r y2=format!("{y_q95:.1}") class="hg-q95"/>
                <text x=w-pad_r-2.0 y=format!("{:.1}", (y_q95 - 3.0).max(pad_t + 8.0)) text-anchor="end" class="hg-lbl-q95">{q95_lbl}</text>

                // Today divider
                <line x1=format!("{x_today:.1}") y1=pad_t x2=format!("{x_today:.1}") y2=pad_t+plot_h class="hg-today"/>

                // Ensemble uncertainty band + lines
                <polygon points=band_poly class="hg-band"/>
                <path d=obs_path class="hg-obs"/>
                <path d=fc_path class="hg-fc"/>

                // Today dot
                <circle
                    cx=format!("{x_today:.1}")
                    cy=format!("{:.1}", y_at(station.current_discharge_m3s))
                    r="4"
                    class="hg-dot"
                />

                // Y-axis ticks
                <text x=pad_l-4.0 y=pad_t+7.0 text-anchor="end" class="hg-tick">{max_q_lbl}</text>
                <text x=pad_l-4.0 y=pad_t+plot_h*0.5+3.0 text-anchor="end" class="hg-tick">{mid_q_lbl}</text>
                <text x=pad_l-4.0 y=pad_t+plot_h text-anchor="end" class="hg-tick">"0"</text>

                // X-axis labels
                <text x=pad_l y=h-7.0 text-anchor="start" class="hg-tick">{start_lbl}</text>
                <text x=format!("{x_today:.1}") y=h-7.0 text-anchor="middle" class="hg-tick-today">{mid_lbl}</text>
                <text x=w-pad_r y=h-7.0 text-anchor="end" class="hg-tick">{end_lbl}</text>
            </svg>
            <div class="hydro-legend">
                <span><i class="lg lg-obs"></i>"Observasi"</span>
                <span><i class="lg lg-fc"></i>"Prakiraan + Rentang Max"</span>
                <span><i class="lg lg-rain"></i>"Hujan Harian"</span>
            </div>
        </div>
    }.into_any()
}

fn short_date(s: &str) -> String {
    if s.len() >= 10 {
        s[5..10].to_string()
    } else {
        s.to_string()
    }
}

fn fmt_q(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.1}k", v / 1000.0)
    } else if v >= 100.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}
