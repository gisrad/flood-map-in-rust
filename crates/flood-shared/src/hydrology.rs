//! Logika indikator model: sungai (ambang klimatologis GloFAS), drainase (kategori
//! intensitas hujan BMKG), dan rob (persentil muka laut model setempat).

use crate::calibration::{rob_calibration, station_calibration};
use crate::model::{
    CoastalRobStatic, CoastalRobTelemetry, ExceedProb, FloodMechanism, FloodSummaryKpis,
    HydrographPoint, IndicatorLevel, LevelCounts, RiverThresholds, RobThresholds, StationStatic,
    StationTelemetry, Trend,
};

/// Batas kategori hujan harian BMKG (mm/24 jam): lebat 50–100, sangat lebat 100–150, ekstrem >150.
pub const BMKG_RAIN_HEAVY_MM: f64 = 50.0;
pub const BMKG_RAIN_VERY_HEAVY_MM: f64 = 100.0;
pub const BMKG_RAIN_EXTREME_MM: f64 = 150.0;
/// Radius laporan banjir warga yang menaikkan level drainase minimal ke `high`.
pub const REPORT_RADIUS_KM: f64 = 10.0;

const EULER_GAMMA: f64 = 0.577_215_664_901_532_9;

const DISCLAIMER_PUBLIC: &str =
    "Ini indikasi model, bukan peringatan resmi; ikuti informasi BPBD dan BMKG setempat.";

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// Parameter Gumbel (lokasi ξ, skala α) dengan metode L-moment (Hosking 1990).
/// Mengembalikan `None` bila sampel < 5 atau tidak bervariasi.
pub fn gumbel_lmoments(sample: &[f64]) -> Option<(f64, f64)> {
    let mut x: Vec<f64> = sample.iter().copied().filter(|v| v.is_finite()).collect();
    let n = x.len();
    if n < 5 {
        return None;
    }
    x.sort_by(|a, b| a.total_cmp(b));
    let nf = n as f64;
    let b0 = x.iter().sum::<f64>() / nf;
    let b1 = x
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64) / (nf - 1.0) * v)
        .sum::<f64>()
        / nf;
    let l1 = b0;
    let l2 = 2.0 * b1 - b0;
    if l2 <= 0.0 {
        return None;
    }
    let alpha = l2 / std::f64::consts::LN_2;
    let xi = l1 - EULER_GAMMA * alpha;
    Some((xi, alpha))
}

/// Kuantil Gumbel untuk periode ulang `t_years` (> 1).
pub fn gumbel_quantile(xi: f64, alpha: f64, t_years: f64) -> f64 {
    xi - alpha * (-(1.0 - 1.0 / t_years).ln()).ln()
}

/// Level sungai dari debit harian terhadap Q2/Q5/Q20.
pub fn river_level(q_m3s: f64, th: Option<&RiverThresholds>) -> IndicatorLevel {
    let Some(th) = th else {
        return IndicatorLevel::Unknown;
    };
    if !q_m3s.is_finite() {
        return IndicatorLevel::Unknown;
    }
    if q_m3s >= th.q20_m3s {
        IndicatorLevel::Extreme
    } else if q_m3s >= th.q5_m3s {
        IndicatorLevel::VeryHigh
    } else if q_m3s >= th.q2_m3s {
        IndicatorLevel::High
    } else {
        IndicatorLevel::Normal
    }
}

/// Level dari curah hujan 24 jam menurut kategori intensitas BMKG.
pub fn rain_level(mm_24h: f64) -> IndicatorLevel {
    if mm_24h > BMKG_RAIN_EXTREME_MM {
        IndicatorLevel::Extreme
    } else if mm_24h >= BMKG_RAIN_VERY_HEAVY_MM {
        IndicatorLevel::VeryHigh
    } else if mm_24h >= BMKG_RAIN_HEAVY_MM {
        IndicatorLevel::High
    } else {
        IndicatorLevel::Normal
    }
}

pub fn rain_category_id(mm_24h: f64) -> &'static str {
    if mm_24h > BMKG_RAIN_EXTREME_MM {
        "ekstrem"
    } else if mm_24h >= BMKG_RAIN_VERY_HEAVY_MM {
        "sangat lebat"
    } else if mm_24h >= BMKG_RAIN_HEAVY_MM {
        "lebat"
    } else if mm_24h >= 20.0 {
        "sedang"
    } else if mm_24h >= 0.5 {
        "ringan"
    } else {
        "tidak hujan"
    }
}

/// Level drainase: maksimum dari hujan 24 jam terakhir dan 24 jam ke depan;
/// ≥1 laporan banjir warga dalam radius 10 km/24 jam menaikkan level minimal ke `high`.
pub fn pluvial_level(
    rain_24h_mm: f64,
    rain_next_24h_mm: f64,
    nearby_reports: u32,
) -> IndicatorLevel {
    let lvl = rain_level(rain_24h_mm).max(rain_level(rain_next_24h_mm));
    if nearby_reports > 0 {
        lvl.max(IndicatorLevel::High)
    } else {
        lvl
    }
}

/// Gabungan level stasiun. Level sungai yang belum terkalibrasi tidak boleh tampil sebagai
/// "Normal": bila drainase tidak tinggi, hasilnya `unknown`.
pub fn combine_levels(
    fluvial: IndicatorLevel,
    pluvial: IndicatorLevel,
) -> (IndicatorLevel, FloodMechanism) {
    if fluvial == IndicatorLevel::Unknown && pluvial < IndicatorLevel::High {
        return (IndicatorLevel::Unknown, FloodMechanism::FluvialBasin);
    }
    if pluvial > fluvial {
        (pluvial, FloodMechanism::UrbanPluvial)
    } else {
        (fluvial, FloodMechanism::FluvialBasin)
    }
}

/// Level rob dari puncak muka laut 48 jam terhadap klimatologi model setempat.
/// `high` ≥ p99, `very_high` ≥ maksimum periode kalibrasi. `extreme` tidak ditetapkan untuk rob.
pub fn rob_level(peak_msl_m: f64, th: Option<&RobThresholds>) -> IndicatorLevel {
    let Some(th) = th else {
        return IndicatorLevel::Unknown;
    };
    if !peak_msl_m.is_finite() {
        IndicatorLevel::Unknown
    } else if peak_msl_m >= th.max_m {
        IndicatorLevel::VeryHigh
    } else if peak_msl_m >= th.p99_m {
        IndicatorLevel::High
    } else {
        IndicatorLevel::Normal
    }
}

pub fn classify_trend(prev_q: f64, now_q: f64, next_q: f64) -> Trend {
    let base = prev_q.max(1.0);
    let delta_now = (now_q - prev_q) / base;
    let delta_next = (next_q - now_q) / now_q.max(1.0);
    if delta_now >= 0.25 || delta_next >= 0.30 {
        Trend::NaikTajam
    } else if delta_now >= 0.08 || delta_next >= 0.10 {
        Trend::Naik
    } else if delta_now <= -0.10 && delta_next <= -0.05 {
        Trend::Surut
    } else {
        Trend::Stabil
    }
}

/// Masukan satu stasiun untuk satu waktu evaluasi.
#[derive(Debug, Clone, Default)]
pub struct StationInputs {
    pub hydrograph: Vec<HydrographPoint>,
    /// Indeks titik hidrograf "hari ini" (hari evaluasi).
    pub today_idx: usize,
    pub rain_24h_mm: f64,
    pub rain_72h_mm: f64,
    pub rain_next_24h_mm: f64,
    pub soil_top: f64,
    pub soil_deep: f64,
    pub nearby_reports_24h: u32,
    pub exceed_prob_7d: Option<ExceedProb>,
}

pub fn evaluate_station(st: &StationStatic, inp: StationInputs) -> StationTelemetry {
    let cal = station_calibration(st.id);
    let glofas_cell = cal.and_then(|c| c.cell.clone());
    let thresholds = cal.and_then(|c| c.thresholds());

    let hydro = &inp.hydrograph;
    let today_idx = inp.today_idx.min(hydro.len().saturating_sub(1));
    let current_q = hydro.get(today_idx).map(|p| p.discharge_m3s);
    let current = current_q.unwrap_or(f64::NAN);

    let mut peak_q = current;
    let mut peak_date = hydro
        .get(today_idx)
        .map(|p| p.date.clone())
        .unwrap_or_default();
    for pt in hydro.iter().skip(today_idx) {
        if pt.discharge_m3s > peak_q || !peak_q.is_finite() {
            peak_q = pt.discharge_m3s;
            peak_date = pt.date.clone();
        }
    }

    let prev_q = today_idx
        .checked_sub(1)
        .and_then(|i| hydro.get(i))
        .map(|p| p.discharge_m3s)
        .unwrap_or(current);
    let next_q = hydro
        .get(today_idx + 1)
        .map(|p| p.discharge_m3s)
        .unwrap_or(current);
    let trend = if current.is_finite() {
        classify_trend(prev_q, current, next_q)
    } else {
        Trend::Stabil
    };

    let fluvial_level = river_level(current, thresholds.as_ref());
    let peak_forecast_level = river_level(peak_q, thresholds.as_ref());
    let pluvial_level = pluvial_level(
        inp.rain_24h_mm,
        inp.rain_next_24h_mm,
        inp.nearby_reports_24h,
    );
    let (level, mechanism) = combine_levels(fluvial_level, pluvial_level);
    let ratio_q2 = thresholds
        .as_ref()
        .filter(|t| t.q2_m3s > 0.0 && current.is_finite())
        .map(|t| round2(current / t.q2_m3s));

    let level_reason = station_reason(
        st,
        current,
        thresholds.as_ref(),
        fluvial_level,
        &inp,
        pluvial_level,
        peak_q,
        &peak_date,
        peak_forecast_level,
        cal.map(|c| c.note.as_str()),
    );
    let public_advice = station_public_advice(st, level, mechanism);
    let gov_action = station_gov_action(st, level, mechanism, peak_forecast_level, &peak_date);
    let academic_note = station_academic_note(
        current,
        peak_q,
        &peak_date,
        glofas_cell.as_ref(),
        thresholds.as_ref(),
        &inp,
    );

    StationTelemetry {
        id: st.id.to_string(),
        name: st.name.to_string(),
        river: st.river.to_string(),
        ws_code: st.ws_code.to_string(),
        province: st.province.to_string(),
        island: st.island.to_string(),
        lat: st.lat,
        lon: st.lon,
        tz_offset_hours: st.tz_offset_hours,
        basin_area_km2: st.basin_area_km2,
        glofas_cell,
        thresholds,
        current_discharge_m3s: round2(current),
        ratio_q2,
        peak_forecast_m3s: round2(peak_q),
        peak_forecast_date: peak_date,
        peak_forecast_level,
        exceed_prob_7d: inp.exceed_prob_7d.clone(),
        trend,
        rain_24h_mm: round1(inp.rain_24h_mm),
        rain_72h_mm: round1(inp.rain_72h_mm),
        rain_next_24h_mm: round1(inp.rain_next_24h_mm),
        soil_moisture_top: round2(inp.soil_top),
        soil_moisture_deep: round2(inp.soil_deep),
        nearby_reports_24h: inp.nearby_reports_24h,
        fluvial_level,
        pluvial_level,
        level,
        mechanism,
        level_reason,
        public_advice,
        gov_action,
        academic_note,
        hydrograph: inp.hydrograph,
    }
}

#[allow(clippy::too_many_arguments)]
fn station_reason(
    st: &StationStatic,
    current: f64,
    th: Option<&RiverThresholds>,
    fluvial: IndicatorLevel,
    inp: &StationInputs,
    pluvial: IndicatorLevel,
    peak_q: f64,
    peak_date: &str,
    peak_level: IndicatorLevel,
    cal_note: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    match th {
        Some(t) if current.is_finite() => {
            let q = |v: f64| format!("{} m³/s", fmt_num(v));
            let band = match fluvial {
                IndicatorLevel::Extreme => format!("di atas Q20 ({})", q(t.q20_m3s)),
                IndicatorLevel::VeryHigh => {
                    format!("antara Q5 ({}) dan Q20 ({})", q(t.q5_m3s), q(t.q20_m3s))
                }
                IndicatorLevel::High => {
                    format!("antara Q2 ({}) dan Q5 ({})", q(t.q2_m3s), q(t.q5_m3s))
                }
                _ => format!("di bawah Q2 ({})", q(t.q2_m3s)),
            };
            parts.push(format!(
                "Debit model {} {} m³/s, {}.",
                st.river,
                fmt_num(current),
                band
            ));
            if peak_level > fluvial && peak_q.is_finite() {
                parts.push(format!(
                    "Prakiraan puncak {} m³/s pada {} (level {}).",
                    fmt_num(peak_q),
                    peak_date,
                    peak_level.label_id().to_lowercase()
                ));
            }
        }
        Some(_) => parts.push("Data debit GloFAS tidak tersedia saat ini.".to_string()),
        None => {
            let why = cal_note
                .filter(|n| !n.is_empty())
                .unwrap_or("sel GloFAS 5 km yang sesuai dengan besaran sungai ini belum ditemukan");
            parts.push(format!("Level sungai belum ditetapkan: {why}."));
        }
    }
    if pluvial.is_elevated() {
        let r = inp.rain_24h_mm.max(inp.rain_next_24h_mm);
        let when = if inp.rain_next_24h_mm > inp.rain_24h_mm {
            "24 jam ke depan"
        } else {
            "24 jam terakhir"
        };
        if rain_level(r).is_elevated() {
            parts.push(format!(
                "Hujan model {when} {} mm (kategori {} BMKG).",
                fmt_dec(r, 0),
                rain_category_id(r)
            ));
        }
        if inp.nearby_reports_24h > 0 {
            parts.push(format!(
                "{} laporan banjir warga dalam radius {} km pada 24 jam terakhir.",
                inp.nearby_reports_24h, REPORT_RADIUS_KM as u32
            ));
        }
    } else {
        parts.push(format!(
            "Hujan 24 jam terakhir {} mm, 24 jam ke depan {} mm.",
            fmt_dec(inp.rain_24h_mm, 0),
            fmt_dec(inp.rain_next_24h_mm, 0)
        ));
    }
    parts.join(" ")
}

fn station_public_advice(
    st: &StationStatic,
    level: IndicatorLevel,
    mech: FloodMechanism,
) -> String {
    let what = match mech {
        FloodMechanism::UrbanPluvial => "hujan lokal",
        _ => "debit sungai",
    };
    let body = match level {
        IndicatorLevel::Extreme => format!(
            "Model menunjukkan {what} di sekitar {} jauh di atas kebiasaan. Bila tinggal di bantaran atau dataran rendah, siapkan tas siaga dan rencana mengungsi.",
            st.river
        ),
        IndicatorLevel::VeryHigh => format!(
            "Model menunjukkan {what} di sekitar {} sangat tinggi. Amankan barang penting dan pantau kondisi air di lingkungan Anda.",
            st.river
        ),
        IndicatorLevel::High => format!(
            "Model menunjukkan {what} di sekitar {} di atas kebiasaan. Perhatikan saluran air dan kabar dari kelurahan.",
            st.river
        ),
        IndicatorLevel::Normal => format!(
            "Tidak ada indikasi banjir dari model untuk {} saat ini.",
            st.river
        ),
        IndicatorLevel::Unknown => format!(
            "Model belum dapat menilai debit {}. Andalkan informasi setempat.",
            st.river
        ),
    };
    format!("{body} {DISCLAIMER_PUBLIC}")
}

fn station_gov_action(
    st: &StationStatic,
    level: IndicatorLevel,
    mech: FloodMechanism,
    peak_level: IndicatorLevel,
    peak_date: &str,
) -> String {
    match (level, mech) {
        (IndicatorLevel::Extreme | IndicatorLevel::VeryHigh, FloodMechanism::FluvialBasin) => format!(
            "Pertimbangkan verifikasi segera ke pos duga air {ws} dan BBWS, kesiapan pompa serta perahu di hilir {river}, dan penyampaian informasi ke kecamatan bantaran.",
            ws = st.ws_code,
            river = st.river
        ),
        (IndicatorLevel::Extreme | IndicatorLevel::VeryHigh, _) => format!(
            "Pertimbangkan pengecekan pompa dan saluran utama {prov}, serta koordinasi dengan BMKG untuk peringatan dini cuaca.",
            prov = st.province
        ),
        (IndicatorLevel::High, FloodMechanism::FluvialBasin) => format!(
            "Pertimbangkan pemantauan pos duga air {ws} lebih rapat dan pengecekan pintu air.",
            ws = st.ws_code
        ),
        (IndicatorLevel::High, _) => format!(
            "Pertimbangkan pembersihan saringan sampah dan kesiapan pompa genangan di {prov}.",
            prov = st.province
        ),
        (IndicatorLevel::Normal, _) if peak_level.is_elevated() => format!(
            "Pertimbangkan kesiapan dini: prakiraan debit mencapai level {} pada {}.",
            peak_level.label_id().to_lowercase(),
            peak_date
        ),
        (IndicatorLevel::Normal, _) => "Pemantauan rutin.".to_string(),
        (IndicatorLevel::Unknown, _) => format!(
            "Pertimbangkan memakai data pos duga air {ws} secara langsung; sel model belum terkalibrasi.",
            ws = st.ws_code
        ),
    }
}

fn station_academic_note(
    current: f64,
    peak_q: f64,
    peak_date: &str,
    cell: Option<&crate::model::GlofasCell>,
    th: Option<&RiverThresholds>,
    inp: &StationInputs,
) -> String {
    let mut s = String::new();
    match cell {
        Some(c) => s.push_str(&format!(
            "GloFAS v4 (Open-Meteo Flood API), sel {:.3}°, {:.3}° ({} km dari pos; Q̄ 1997–2024 {} m³/s). ",
            c.lat,
            c.lon,
            fmt_dec(c.offset_km, 1),
            fmt_num(c.mean_q_m3s)
        )),
        None => s.push_str("GloFAS v4 (Open-Meteo Flood API), sel terdekat koordinat pos, tidak terkalibrasi. "),
    }
    if let Some(t) = th {
        s.push_str(&format!(
            "Ambang {} (n={} tahun): Q2 {}, Q5 {}, Q20 {} m³/s (Gumbel, L-moment, maksimum tahunan debit harian); p90 {}, p99 {} m³/s. ",
            t.period,
            t.n_years,
            fmt_num(t.q2_m3s),
            fmt_num(t.q5_m3s),
            fmt_num(t.q20_m3s),
            fmt_num(t.p90_m3s),
            fmt_num(t.p99_m3s)
        ));
    }
    if current.is_finite() {
        s.push_str(&format!(
            "Q hari ini {} m³/s; puncak 7 hari {} m³/s ({}). ",
            fmt_num(current),
            fmt_num(peak_q),
            peak_date
        ));
    }
    if let Some(e) = &inp.exceed_prob_7d {
        s.push_str(&format!(
            "Ensemble {} anggota: P(>Q2) {}, P(>Q5) {}, P(>Q20) {}. ",
            e.members,
            fmt_pct(e.q2),
            fmt_pct(e.q5),
            fmt_pct(e.q20)
        ));
    }
    s.push_str(&format!(
        "Hujan model 24/72 jam {}/{} mm, 24 jam ke depan {} mm; kelembapan tanah 0–7 cm {} m³/m³.",
        fmt_dec(inp.rain_24h_mm, 1),
        fmt_dec(inp.rain_72h_mm, 1),
        fmt_dec(inp.rain_next_24h_mm, 1),
        fmt_dec(inp.soil_top, 2)
    ));
    s
}

/// "2026-11-24T01:00:00Z" + offset 7 → "24/11 08.00 WIB"; string asli bila tidak terbaca.
fn local_time_label(iso_utc: &str, tz_offset_hours: i64) -> String {
    let Some(ts) = crate::rss::parse_iso8601(iso_utc) else {
        return iso_utc.to_string();
    };
    let local = crate::rss::epoch_to_iso(ts + tz_offset_hours * 3600);
    let zone = match tz_offset_hours {
        8 => "WITA",
        9 => "WIT",
        _ => "WIB",
    };
    format!(
        "{}/{} {}.{} {zone}",
        &local[8..10],
        &local[5..7],
        &local[11..13],
        &local[14..16]
    )
}

pub fn evaluate_rob(
    rob: &CoastalRobStatic,
    current_msl_m: f64,
    peak_msl_48h_m: f64,
    peak_msl_time: String,
    wave_height_m: f64,
) -> CoastalRobTelemetry {
    let cal = rob_calibration(rob.id);
    let thresholds = cal.and_then(|c| c.thresholds.clone());
    let peak = peak_msl_48h_m.max(current_msl_m);
    let level = rob_level(peak, thresholds.as_ref());

    let level_reason = match (&thresholds, level) {
        (None, _) => "Ambang muka laut model belum tersedia untuk titik ini.".to_string(),
        (Some(_), IndicatorLevel::Unknown) => {
            "Data muka laut model tidak tersedia saat ini.".to_string()
        }
        (Some(t), IndicatorLevel::VeryHigh) => format!(
            "Puncak muka laut model 48 jam {} m menyamai atau melampaui maksimum {} ({} m).",
            fmt_dec(peak, 2),
            t.period,
            fmt_dec(t.max_m, 2)
        ),
        (Some(t), IndicatorLevel::High) => format!(
            "Puncak muka laut model 48 jam {} m di atas persentil 99 ({} m, {}).",
            fmt_dec(peak, 2),
            fmt_dec(t.p99_m, 2),
            t.period
        ),
        (Some(t), _) => format!(
            "Puncak muka laut model 48 jam {} m di bawah persentil 99 ({} m, {}).",
            fmt_dec(peak, 2),
            fmt_dec(t.p99_m, 2),
            t.period
        ),
    };
    let note = if current_msl_m.is_finite() && peak_msl_48h_m.is_finite() {
        let wave = if wave_height_m.is_finite() {
            format!(
                ", tinggi gelombang maksimum {} m",
                fmt_dec(wave_height_m, 1)
            )
        } else {
            String::new()
        };
        format!(
            "Muka laut model (relatif MSL global; pasang surut + efek barometrik, tanpa amblesan tanah) saat ini {} m, puncak 48 jam {} m ({}){wave}. Akurasi model di pesisir terbatas.",
            fmt_dec(current_msl_m, 2),
            fmt_dec(peak_msl_48h_m, 2),
            local_time_label(&peak_msl_time, rob.tz_offset_hours as i64),
        )
    } else {
        "Muka laut model (relatif MSL global; pasang surut + efek barometrik, tanpa amblesan tanah) belum tersedia. Akurasi model di pesisir terbatas.".to_string()
    };

    CoastalRobTelemetry {
        id: rob.id.to_string(),
        name: rob.name.to_string(),
        province: rob.province.to_string(),
        island: rob.island.to_string(),
        lat: rob.lat,
        lon: rob.lon,
        tz_offset_hours: rob.tz_offset_hours,
        current_msl_m: round2(current_msl_m),
        peak_msl_48h_m: round2(peak_msl_48h_m),
        peak_msl_time,
        wave_height_m: round2(wave_height_m),
        thresholds,
        level,
        level_reason,
        note,
    }
}

pub fn compute_kpis(
    stations: &[StationTelemetry],
    rob: &[CoastalRobTelemetry],
    citizen_flood_reports_24h: usize,
    bmkg_nowcasts: usize,
) -> FloodSummaryKpis {
    let mut river = LevelCounts::default();
    let mut pluvial = LevelCounts::default();
    let mut rob_counts = LevelCounts::default();
    let mut max_ratio: Option<(f64, &str)> = None;
    let mut uncalibrated = 0usize;

    for st in stations {
        river.add(st.fluvial_level);
        pluvial.add(st.pluvial_level);
        if st.thresholds.is_none() {
            uncalibrated += 1;
        }
        if let Some(r) = st.ratio_q2 {
            if max_ratio.is_none_or(|(m, _)| r > m) {
                max_ratio = Some((r, st.name.as_str()));
            }
        }
    }
    for r in rob {
        rob_counts.add(r.level);
    }

    FloodSummaryKpis {
        river,
        pluvial,
        rob: rob_counts,
        citizen_flood_reports_24h,
        bmkg_nowcasts,
        max_ratio_q2: max_ratio.map(|(r, _)| r),
        max_ratio_station: max_ratio.map(|(_, n)| n.to_string()),
        uncalibrated,
    }
}

pub fn round2(v: f64) -> f64 {
    if v.is_finite() {
        (v * 100.0).round() / 100.0
    } else {
        0.0
    }
}

pub fn round1(v: f64) -> f64 {
    if v.is_finite() {
        (v * 10.0).round() / 10.0
    } else {
        0.0
    }
}

/// Angka gaya Indonesia: pemisah ribuan titik, desimal koma.
pub fn fmt_dec(v: f64, decimals: usize) -> String {
    if !v.is_finite() {
        return "–".to_string();
    }
    let s = format!("{:.*}", decimals, v.abs());
    let (int_part, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s.clone(), None),
    };
    let mut grouped = String::new();
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let sign = if v < 0.0 && s.chars().any(|c| c != '0' && c != '.') {
        "-"
    } else {
        ""
    };
    match frac {
        Some(f) => format!("{sign}{grouped},{f}"),
        None => format!("{sign}{grouped}"),
    }
}

/// Debit dengan presisi yang wajar terhadap besarnya.
pub fn fmt_num(v: f64) -> String {
    let a = v.abs();
    if a >= 100.0 {
        fmt_dec(v, 0)
    } else if a >= 10.0 {
        fmt_dec(v, 1)
    } else {
        fmt_dec(v, 2)
    }
}

pub fn fmt_pct(frac: f64) -> String {
    format!("{}%", fmt_dec(frac * 100.0, 0))
}

/*
===============================================================================
PART 1: VERSES ABOUT THE GREAT FLOOD OF PROPHET NOAH (AS)
===============================================================================

1. Surah Hud (11:40 & 42) — The Start of the Flood & Waves Like Mountains
   * Verse 40:
     حَتّٰىٓ اِذَا جَاۤءَ اَمْرُنَا وَفَارَ التَّنُّوْرُۙ قُلْنَا احْمِلْ فِيْهَا مِنْ كُلٍّ زَوْجَيْنِ اثْنَيْنِ وَاَهْلَكَ اِلَّا مَنْ سَبَقَ عَلَيْهِ الْقَوْلُ وَمَنْ اٰمَنَ ۗوَمَآ اٰمَنَ مَعَهٗٓ اِلَّا قَلِيْلٌ
     Translation: "So when Our command came and the oven overflowed, We said,
     'Load upon the ship of each [creature] two mates and your family, except
     those about whom the word has preceded, and [include] whoever has believed.'
     But none had believed with him, except a few."

   * Verse 42:
     وَهِيَ تَجْرِيْ بِهِمْ فِيْ مَوْجٍ كَالْجِبَالِۗ وَنَادٰى نُوْحُ اࣙبْنَهٗ وَكَانَ فِيْ مَعْزِلٍ يّٰبُنَيَّ ارْكَبْ مَّعَنَا وَلَا تَكُنْ مَّعَ الْكٰفِرِيْنَ
     Translation: "And it sailed with them through waves like mountains, and
     Noah called to his son who was apart [from them], 'O my son, come aboard
     with us and be not with the disbelievers.'"

2. Surah Al-Qamar (54:11–12) — Water Pouring from the Sky and Gushing from the Earth
   فَفَتَحْنَآ اَبْوَابَ السَّمَاۤءِ بِمَاۤءٍ مُّنْهَمِرٍ ۖ وَّفَجَّرْنَا الْاَرْضَ عُيُوْنًا فَالْتَقَى الْمَاۤءُ عَلٰٓى اَمْرٍ قَدْ قُدِرَ ۚ
   Translation: "Then We opened the gates of the heaven with rain pouring down,
   and caused the earth to burst with springs, and the waters met for a matter
   already predestined."

3. Surah Al-Mu'minun (23:27) — The Command to Build the Ark
   فَاَوْحَيْنَآ اِلَيْهِ اَنِ اصْنَعِ الْفُلْكَ بِاَعْيُنِنَا وَوَحْيِنَا فَاِذَا جَاۤءَ اَمْرُنَا وَفَارَ التَّنُّوْرُۙ فَاسْلُكْ فِيْهَا مِنْ كُلٍّ زَوْجَيْنِ اثْنَيْنِ وَاَهْلَكَ اِلَّا مَنْ سَبَقَ عَلَيْهِ الْقَوْلُ مِنْهُمْ ۚوَلَا تُخَاطِبْنِيْ فِى الَّذِيْنَ ظَلَمُوْاۚ اِنَّهُمْ مُّغْرَقُوْنَ
   Translation: "So We inspired to him, 'Construct the ship under Our
   observation and Our inspiration, and when Our command comes and the oven
   overflows, put into it from each [creature] two mates and your family,
   except those for whom the decree [of destruction] has proceeded. And do not
   address Me concerning those who have wronged; indeed, they are to be drowned.'"

4. Surah Nuh (71:25) — The Drowning of the Disbelievers
   مِمَّا خَطِيْۤـٰٔتِهِمْ اُغْرِقُوْا فَاُدْخِلُوْا نَارًا ۙفَلَمْ يَجِدُوْا لَهُمْ مِّنْ دُوْنِ اللّٰهِ اَنْصَارًا
   Translation: "Because of their sins they were drowned and put into the Fire,
   and they found not for themselves besides Allah [any] helpers."

5. Surah Al-A'raf (7:64) — The Salvation of Noah and His Followers
   فَكَذَّبُوْهُ فَاَنْجَيْنٰهُ وَالَّذِيْنَ مَعَهٗ فِى الْفُلْكِ وَاَغْرَقْنَا الَّذِيْنَ كَذَّبُوْا بِاٰيٰتِنَاۗ اِنَّهُمْ كَانُوْا قَوْمًا عَمِيْنَ ࣖ
   Translation: "But they denied him, so We saved him and those who were with
   him in the ship. And We drowned those who denied Our signs. Indeed, they
   were a blind people."

===============================================================================
PART 2: VERSES ABOUT CALAMITIES CAUSED BY HUMAN ACTIONS
===============================================================================

1. Surah Ar-Rum (30:41) — Corruption on Land and Sea Due to Human Deeds
   ظَهَرَ الْفَسَادُ فِى الْبَرِّ وَالْبَحْرِ بِمَا كَسَبَتْ اَيْدِى النَّاسِ لِيُذِيْقَهُمْ بَعْضَ الَّذِيْ عَمِلُوْا لَعَلَّهُمْ يَرْجِعُوْنَ
   Translation: "Corruption has appeared throughout the land and sea by [reason
   of] what the hands of people have earned so He may let them taste part of
   [the consequence of] what they have done that perhaps they will return [to
   righteousness]."

2. Surah Ash-Shuraa (42:30) — Misfortune Strikes Because of What People Earn
   وَمَآ اَصَابَكُمْ مِّنْ مُّصِيْبَةٍ فَبِمَا كَسَبَتْ اَيْدِيْكُمْ وَيَعْفُوْا عَنْ كَثِيْرٍۗ
   Translation: "And whatever strikes you of disaster—it is for what your
   hands have earned; but He pardons much."

3. Surah An-Nisa (4:79) — Good Comes from Allah, Harm Comes from Oneself
   مَآ اَصَابَكَ مِنْ حَسَنَةٍ فَمِنَ اللّٰهِ ۖ وَمَآ اَصَابَكَ مِنْ سَيِّئَةٍ فَمِنْ نَّفْسِكَ ۗ
   Translation: "What comes to you of good is from Allah, but what comes to
   you of evil, [O man], is from yourself..."
*/
