use crate::model::{
    AlertLevel, CoastalRobStatic, CoastalRobTelemetry, FloodMechanism, FloodSummaryKpis,
    HydrographPoint, StationStatic, StationTelemetry, Trend,
};

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

pub fn estimate_return_period(anomaly_ratio: f64) -> &'static str {
    if anomaly_ratio >= 2.4 {
        "> T20 (Ekstrem)"
    } else if anomaly_ratio >= 1.7 {
        "T5 – T20"
    } else if anomaly_ratio >= 1.25 {
        "T2 – T5"
    } else {
        "< T2 (Normal)"
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

#[allow(clippy::too_many_arguments)]
pub fn evaluate_station(
    st: &StationStatic,
    hydrograph: Vec<HydrographPoint>,
    today_idx: usize,
    rain_24h_mm: f64,
    rain_72h_mm: f64,
    soil_top: f64,
    soil_deep: f64,
    nearby_reports_24h: u32,
) -> StationTelemetry {
    let now_pt = hydrograph.get(today_idx).or_else(|| hydrograph.last());
    let current_q = now_pt.map(|p| p.discharge_m3s).unwrap_or(st.baseline_q75_hint * 0.5);

    // Use GloFAS p75 when physically meaningful, fallback to curated station baseline
    let raw_p75 = now_pt.map(|p| p.discharge_p75_m3s).unwrap_or(0.0);
    let p75_baseline = if raw_p75 > 1.0 {
        // Blend slightly with curated hint to avoid tiny-grid artifact spikes
        (raw_p75 * 0.75 + st.baseline_q75_hint * 0.25).max(3.0)
    } else {
        st.baseline_q75_hint.max(3.0)
    };
    let q95_threshold = p75_baseline * 1.65;

    let mut peak_forecast = current_q;
    let mut peak_date = now_pt.map(|p| p.date.clone()).unwrap_or_default();
    for pt in hydrograph.iter().skip(today_idx) {
        let candidate = pt.discharge_mean_m3s.max(pt.discharge_m3s);
        if candidate > peak_forecast {
            peak_forecast = candidate;
            peak_date = pt.date.clone();
        }
    }

    let anomaly_ratio = (current_q / p75_baseline).max(0.0);
    let peak_ratio = (peak_forecast / p75_baseline).max(0.0);

    let prev_q = if today_idx > 0 {
        hydrograph
            .get(today_idx - 1)
            .map(|p| p.discharge_m3s)
            .unwrap_or(current_q)
    } else {
        current_q
    };
    let next_q = hydrograph
        .get(today_idx + 1)
        .map(|p| p.discharge_mean_m3s.max(p.discharge_m3s))
        .unwrap_or(current_q);
    let trend = classify_trend(prev_q, current_q, next_q);

    // Separate Fluvial Basin Flood vs Urban Pluvial Drainage Flood
    let fluvial_score = anomaly_ratio.max(peak_ratio * 0.90);
    let pluvial_score = (rain_24h_mm / 50.0)
        + (rain_72h_mm / 110.0) * 0.55
        + if soil_top >= 0.42 { 0.35 } else { 0.0 }
        + (nearby_reports_24h as f64) * 0.35;

    let mechanism = if fluvial_score >= 1.25 && fluvial_score >= pluvial_score * 0.95 {
        FloodMechanism::FluvialBasin
    } else if pluvial_score >= 1.05 || nearby_reports_24h > 0 {
        FloodMechanism::UrbanPluvial
    } else {
        FloodMechanism::FluvialBasin
    };

    let combined = fluvial_score.max(pluvial_score);
    let alert_level = if combined >= 2.20 || (anomaly_ratio >= 1.9 && nearby_reports_24h >= 2) {
        AlertLevel::Awas
    } else if combined >= 1.55 || nearby_reports_24h >= 2 {
        AlertLevel::Siaga
    } else if combined >= 1.15 || nearby_reports_24h >= 1 || (rain_24h_mm >= 45.0 && soil_top >= 0.38) {
        AlertLevel::Waspada
    } else {
        AlertLevel::Normal
    };

    let return_period_est = estimate_return_period(fluvial_score).to_string();

    let public_advice = match alert_level {
        AlertLevel::Awas => format!(
            "AWAS: {} berstatus sangat tinggi ({:.1}× normal Q75). Amankan dokumen penting, matikan listrik jika air masuk rumah, dan ikuti jalur evakuasi BPBD.",
            st.river, anomaly_ratio
        ),
        AlertLevel::Siaga => format!(
            "SIAGA: Debit {} dan hujan 72j ({:.0} mm) meningkat signifikan. Pantau tinggi muka air bantaran setiap 2 jam.",
            st.river, rain_72h_mm
        ),
        AlertLevel::Waspada => format!(
            "WASPADA: Terdeteksi peningkatan aliran/hujan lokal di {}. Bersihkan saluran drainase sekitar dan pantau prakiraan puncak {}.",
            st.province, peak_date
        ),
        AlertLevel::Normal => format!(
            "Normal: Debit {} berada dalam rentang aman ({:.1} m³/s vs Q75 {:.1} m³/s).",
            st.river, current_q, p75_baseline
        ),
    };

    let gov_action = match alert_level {
        AlertLevel::Awas | AlertLevel::Siaga => format!(
            "Aktivasi Posko {ws} ({prov}): siagakan pompa/perahu karet di hilir {river}, validasi pintu air & kirim peringatan dini kecamatan rawan.",
            ws = st.ws_code,
            prov = st.province,
            river = st.river
        ),
        AlertLevel::Waspada => format!(
            "Siaga Awal {ws}: cek operasi pintu pengendali banjir, bersihkan saringan sampah drainase, dan koordinasi dengan relawan lokal.",
            ws = st.ws_code
        ),
        AlertLevel::Normal => format!(
            "Pemantauan rutin {ws}: kalibrasi telemetri dan pemeliharaan tanggul reguler.",
            ws = st.ws_code
        ),
    };

    let academic_note = format!(
        "GloFAS v4 cell ({:.2}°, {:.2}°): Q_now={:.1} m³/s, Q75={:.1} m³/s, Q95_est={:.1} m³/s, 7d_peak={:.1} m³/s ({}), API_72h={:.1} mm, SM_0-7cm={:.2} m³/m³, SM_28-100cm={:.2} m³/m³.",
        st.lat,
        st.lon,
        current_q,
        p75_baseline,
        q95_threshold,
        peak_forecast,
        return_period_est,
        rain_72h_mm,
        soil_top,
        soil_deep
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
        river_polyline: st.river_polyline.to_vec(),
        current_discharge_m3s: round2(current_q),
        p75_baseline_m3s: round2(p75_baseline),
        q95_threshold_m3s: round2(q95_threshold),
        peak_forecast_m3s: round2(peak_forecast),
        peak_forecast_date: peak_date,
        anomaly_ratio: round2(anomaly_ratio),
        peak_ratio: round2(peak_ratio),
        return_period_est,
        trend,
        rain_24h_mm: round2(rain_24h_mm),
        rain_72h_mm: round2(rain_72h_mm),
        soil_moisture_top: round2(soil_top),
        soil_moisture_deep: round2(soil_deep),
        nearby_reports_24h,
        mechanism,
        alert_level,
        public_advice,
        gov_action,
        academic_note,
        hydrograph,
    }
}

pub fn evaluate_rob(
    rob: &CoastalRobStatic,
    current_msl_m: f64,
    peak_msl_48h_m: f64,
    peak_msl_time: String,
    wave_height_m: f64,
) -> CoastalRobTelemetry {
    let effective_peak = peak_msl_48h_m.max(current_msl_m);
    let baseline_high_tide = rob.trigger_msl_m + 0.55;
    let rob_index = (effective_peak / baseline_high_tide)
        * (0.78 + (rob.sensitivity - 1.0) * 0.45)
        + (wave_height_m - 0.6).max(0.0) * 0.18;
    let alert_level = if rob_index >= 1.35 {
        AlertLevel::Awas
    } else if rob_index >= 1.12 {
        AlertLevel::Siaga
    } else if rob_index >= 0.92 {
        AlertLevel::Waspada
    } else {
        AlertLevel::Normal
    };

    let note = format!(
        "MSL saat ini {:.2} m, puncak pasang 48j {:.2} m ({}), tinggi gelombang {:.2} m, faktor kerentanan amblesan {:.2}×.",
        current_msl_m, peak_msl_48h_m, peak_msl_time, wave_height_m, rob.sensitivity
    );

    CoastalRobTelemetry {
        id: rob.id.to_string(),
        name: rob.name.to_string(),
        province: rob.province.to_string(),
        island: rob.island.to_string(),
        lat: rob.lat,
        lon: rob.lon,
        tz_offset_hours: rob.tz_offset_hours,
        sensitivity: rob.sensitivity,
        trigger_msl_m: rob.trigger_msl_m,
        current_msl_m: round2(current_msl_m),
        peak_msl_48h_m: round2(peak_msl_48h_m),
        peak_msl_time,
        wave_height_m: round2(wave_height_m),
        rob_index: round2(rob_index),
        alert_level,
        note,
    }
}

pub fn compute_kpis(
    stations: &[StationTelemetry],
    rob: &[CoastalRobTelemetry],
    reports_count: usize,
) -> FloodSummaryKpis {
    let mut awas_count = 0usize;
    let mut siaga_count = 0usize;
    let mut waspada_count = 0usize;
    let mut fluvial_elevated = 0usize;
    let mut pluvial_elevated = 0usize;
    let mut rob_elevated = 0usize;
    let mut max_ratio = 0.0_f64;
    let mut top_name = String::from("Semua DAS Normal");

    for st in stations {
        match st.alert_level {
            AlertLevel::Awas => awas_count += 1,
            AlertLevel::Siaga => siaga_count += 1,
            AlertLevel::Waspada => waspada_count += 1,
            AlertLevel::Normal => {}
        }
        if st.alert_level != AlertLevel::Normal {
            match st.mechanism {
                FloodMechanism::FluvialBasin => fluvial_elevated += 1,
                FloodMechanism::UrbanPluvial => pluvial_elevated += 1,
                FloodMechanism::CoastalRob => rob_elevated += 1,
            }
        }
        if st.anomaly_ratio > max_ratio {
            max_ratio = st.anomaly_ratio;
            top_name = st.name.clone();
        }
    }

    for r in rob {
        match r.alert_level {
            AlertLevel::Awas => awas_count += 1,
            AlertLevel::Siaga => siaga_count += 1,
            AlertLevel::Waspada => waspada_count += 1,
            AlertLevel::Normal => {}
        }
        if r.alert_level != AlertLevel::Normal {
            rob_elevated += 1;
        }
    }

    FloodSummaryKpis {
        total_stations: stations.len(),
        total_rob_corridors: rob.len(),
        awas_count,
        siaga_count,
        waspada_count,
        fluvial_elevated,
        pluvial_elevated,
        rob_elevated,
        citizen_reports_24h: reports_count,
        max_anomaly_ratio: round2(max_ratio),
        top_station_name: top_name,
    }
}

pub fn round2(v: f64) -> f64 {
    if v.is_finite() {
        (v * 100.0).round() / 100.0
    } else {
        0.0
    }
}
