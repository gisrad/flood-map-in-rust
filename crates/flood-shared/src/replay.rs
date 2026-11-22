use crate::catalog::{COASTAL_ROB_POINTS, RIVER_STATIONS};
use crate::hydrology::{compute_kpis, evaluate_rob, evaluate_station};
use crate::model::{
    CitizenReport, FloodMechanism, FloodSnapshot, HydrographPoint, ReplayFrame, ReplayRobOverride,
    ReplayScenarioMeta, ReplayStationOverride, UpstreamHealth,
};

pub fn list_replay_scenarios() -> Vec<ReplayScenarioMeta> {
    vec![
        ReplayScenarioMeta {
            id: "jabodetabek-2020",
            title: "Banjir Ekstrem Jabodetabek (1 Jan 2020)",
            period: "31 Des 2019 – 2 Jan 2020 · Rekor Hujan 377 mm/hari",
            center_lon: 106.86,
            center_lat: -6.32,
            zoom: 9.4,
            citation: "BMKG / Kemayoran 377 mm (1 Jan 2020) & BNPB SitRep Awal Tahun 2020",
            frames: vec![
                ReplayFrame {
                    step_label: "T-12j · Pra-Puncak".to_string(),
                    timestamp_wib: "31 Des 2019 18:00 WIB".to_string(),
                    narrative: "Hujan konvektif mulai jenuh di DAS Ciliwung–Cisadane–Bekasi; tanah lapisan atas mencapai kejenuhan.".to_string(),
                    station_overrides: vec![
                        ovr("ciliwung-katulampa", 48.0, 92.0, 140.0, 0.43, 0.39, FloodMechanism::FluvialBasin),
                        ovr("ciliwung-manggarai", 74.0, 118.0, 165.0, 0.44, 0.40, FloodMechanism::UrbanPluvial),
                        ovr("bekasi-pondokgede", 112.0, 130.0, 175.0, 0.45, 0.41, FloodMechanism::FluvialBasin),
                        ovr("cisadane-tangerang", 140.0, 88.0, 135.0, 0.41, 0.38, FloodMechanism::FluvialBasin),
                    ],
                    rob_overrides: vec![rob("rob-jakarta-muarabaru", 0.52, 0.85)],
                },
                ReplayFrame {
                    step_label: "T0 · Puncak Hujan 377 mm".to_string(),
                    timestamp_wib: "1 Jan 2020 07:00 WIB".to_string(),
                    narrative: "Kombinasi maut: hujan lokal >300 mm/hari melumpuhkan drainase kota bersamaan dengan gelombang kiriman hulu Ciliwung & Kali Bekasi.".to_string(),
                    station_overrides: vec![
                        ovr("ciliwung-katulampa", 86.0, 195.0, 290.0, 0.48, 0.45, FloodMechanism::FluvialBasin),
                        ovr("ciliwung-manggarai", 138.0, 342.0, 410.0, 0.49, 0.47, FloodMechanism::UrbanPluvial),
                        ovr("bekasi-pondokgede", 198.0, 310.0, 380.0, 0.49, 0.46, FloodMechanism::FluvialBasin),
                        ovr("cisadane-tangerang", 245.0, 220.0, 295.0, 0.47, 0.44, FloodMechanism::FluvialBasin),
                        ovr("citarum-karawang", 420.0, 180.0, 250.0, 0.46, 0.43, FloodMechanism::FluvialBasin),
                    ],
                    rob_overrides: vec![
                        rob("rob-jakarta-muarabaru", 1.38, 1.15),
                        rob("rob-muara-gembong", 1.32, 1.05),
                    ],
                },
                ReplayFrame {
                    step_label: "T+18j · Genangan Hilir".to_string(),
                    timestamp_wib: "2 Jan 2020 01:00 WIB".to_string(),
                    narrative: "Hujan mulai mereda di hulu, namun genangan persisten dan aliran balik hilir masih bertahan di dataran banjir Jakarta Timur, Utara & Bekasi.".to_string(),
                    station_overrides: vec![
                        ovr("ciliwung-katulampa", 36.0, 64.0, 260.0, 0.46, 0.45, FloodMechanism::FluvialBasin),
                        ovr("ciliwung-manggarai", 96.0, 95.0, 390.0, 0.48, 0.47, FloodMechanism::UrbanPluvial),
                        ovr("bekasi-pondokgede", 146.0, 82.0, 355.0, 0.48, 0.46, FloodMechanism::FluvialBasin),
                    ],
                    rob_overrides: vec![rob("rob-jakarta-muarabaru", 0.64, 0.90)],
                },
            ],
        },
        ReplayScenarioMeta {
            id: "barito-2021",
            title: "Banjir Besar DAS Barito Kalsel (Jan 2021)",
            period: "12–16 Jan 2021 · Luapan DAS Barito–Martapura",
            center_lon: 114.70,
            center_lat: -2.95,
            zoom: 7.4,
            citation: "Arsip Hidrometeorologi DAS Barito–Martapura Kalimantan Selatan Jan 2021",
            frames: vec![
                ReplayFrame {
                    step_label: "Hari-1 · Akumulasi Hulu".to_string(),
                    timestamp_wib: "12 Jan 2021 12:00 WITA".to_string(),
                    narrative: "Hujan persisten 3 hari di Pegunungan Meratus mengisi penuh DAS Barito Tengah & anak sungai Martapura.".to_string(),
                    station_overrides: vec![
                        ovr("barito-buntok", 5100.0, 98.0, 210.0, 0.46, 0.44, FloodMechanism::FluvialBasin),
                        ovr("barito-banjarmasin", 7200.0, 112.0, 235.0, 0.47, 0.45, FloodMechanism::FluvialBasin),
                        ovr("kahayan-palangkaraya", 1220.0, 74.0, 155.0, 0.43, 0.41, FloodMechanism::FluvialBasin),
                    ],
                    rob_overrides: vec![rob("rob-banjarmasin-tabanio", 0.78, 1.10)],
                },
                ReplayFrame {
                    step_label: "Hari-3 · Puncak Banjir Luas".to_string(),
                    timestamp_wib: "14 Jan 2021 15:00 WITA".to_string(),
                    narrative: "Debit kiriman DAS Barito–Martapura melampaui >2.3× Q75 dan tertahan pasang maksimum di muara.".to_string(),
                    station_overrides: vec![
                        ovr("barito-buntok", 6450.0, 145.0, 315.0, 0.49, 0.47, FloodMechanism::FluvialBasin),
                        ovr("barito-banjarmasin", 9850.0, 165.0, 340.0, 0.50, 0.48, FloodMechanism::FluvialBasin),
                        ovr("kahayan-palangkaraya", 1580.0, 92.0, 210.0, 0.45, 0.43, FloodMechanism::FluvialBasin),
                    ],
                    rob_overrides: vec![rob("rob-banjarmasin-tabanio", 1.48, 1.35)],
                },
                ReplayFrame {
                    step_label: "Hari-5 · Genangan Surut Lambat".to_string(),
                    timestamp_wib: "16 Jan 2021 12:00 WITA".to_string(),
                    narrative: "Aliran puncak bergerak ke estuari; surut berlangsung lambat akibat dataran aluvial rendah.".to_string(),
                    station_overrides: vec![
                        ovr("barito-buntok", 4750.0, 52.0, 190.0, 0.47, 0.46, FloodMechanism::FluvialBasin),
                        ovr("barito-banjarmasin", 8200.0, 68.0, 240.0, 0.48, 0.47, FloodMechanism::FluvialBasin),
                    ],
                    rob_overrides: vec![rob("rob-banjarmasin-tabanio", 0.76, 1.05)],
                },
            ],
        },
        ReplayScenarioMeta {
            id: "pantura-wulan-2024",
            title: "Tanggul Wulan & Rob Pantura Jateng (2024)",
            period: "Feb–Mar 2024 · Jebolnya Tanggul Wulan + Rob Sayung",
            center_lon: 110.55,
            center_lat: -6.88,
            zoom: 9.2,
            citation: "Dokumentasi Kejadian Banjir Demak–Kudus & Pesisir Sayung 2024",
            frames: vec![
                ReplayFrame {
                    step_label: "Fase 1 · Kenaikan Hulu".to_string(),
                    timestamp_wib: "8 Feb 2024 06:00 WIB".to_string(),
                    narrative: "Curah hujan tinggi di kawasan Muria–Grobogan mendorong lonjakan debit Sungai Wulan.".to_string(),
                    station_overrides: vec![
                        ovr("wulan-demak", 142.0, 118.0, 215.0, 0.46, 0.43, FloodMechanism::FluvialBasin),
                        ovr("solo-sragen", 265.0, 86.0, 165.0, 0.44, 0.40, FloodMechanism::FluvialBasin),
                    ],
                    rob_overrides: vec![
                        rob("rob-semarang-sayung", 0.62, 1.05),
                        rob("rob-pekalongan", 0.59, 0.95),
                    ],
                },
                ReplayFrame {
                    step_label: "Fase 2 · Limpasan & Rob Ganda".to_string(),
                    timestamp_wib: "17 Mar 2024 14:00 WIB".to_string(),
                    narrative: "Limpasan Tanggul Sungai Wulan bertemu dengan pasang rob tinggi di pesisir Demak–Semarang.".to_string(),
                    station_overrides: vec![
                        ovr("wulan-demak", 215.0, 172.0, 290.0, 0.49, 0.47, FloodMechanism::FluvialBasin),
                        ovr("solo-bojonegoro", 780.0, 95.0, 195.0, 0.45, 0.42, FloodMechanism::FluvialBasin),
                    ],
                    rob_overrides: vec![
                        rob("rob-semarang-sayung", 1.44, 1.35),
                        rob("rob-pekalongan", 1.36, 1.22),
                    ],
                },
            ],
        },
    ]
}

pub fn build_replay_snapshot(scenario_id: &str, frame_idx: usize) -> Option<FloodSnapshot> {
    let scenarios = list_replay_scenarios();
    let scenario = scenarios.into_iter().find(|s| s.id == scenario_id)?;
    let clamped_idx = frame_idx.min(scenario.frames.len().saturating_sub(1));
    let frame = scenario.frames.get(clamped_idx)?;

    let mut stations = Vec::with_capacity(RIVER_STATIONS.len());
    for st in RIVER_STATIONS {
        let ovr_opt = frame
            .station_overrides
            .iter()
            .find(|o| o.station_id == st.id);
        let (q_now, r24, r72, sm_top, sm_deep) = if let Some(o) = ovr_opt {
            (o.discharge_m3s, o.rain_24h_mm, o.rain_72h_mm, o.soil_top, o.soil_deep)
        } else {
            (st.baseline_q75_hint * 0.55, 8.0, 22.0, 0.30, 0.28)
        };

        let mut hydro = Vec::with_capacity(14);
        for d in 0..14 {
            let factor = match d {
                0..=7 => 0.45 + (d as f64) * 0.05,
                8..=10 => 0.85 + ((d - 7) as f64) * 0.05,
                _ => 1.0 - ((d - 10) as f64) * 0.08,
            };
            let q = q_now * factor;
            hydro.push(HydrographPoint {
                date: format!("H{:+}", d as i32 - 10),
                is_forecast: d > 10,
                discharge_m3s: q,
                discharge_mean_m3s: q,
                discharge_max_m3s: q * 1.18,
                discharge_p75_m3s: st.baseline_q75_hint,
                rain_mm: if d == 10 { r24 } else { r24 * 0.25 },
            });
        }
        let mut tele = evaluate_station(
            st,
            hydro,
            10,
            r24,
            r72,
            sm_top,
            sm_deep,
            if ovr_opt.is_some() { 2 } else { 0 },
        );
        if let Some(o) = ovr_opt {
            tele.mechanism = o.mechanism;
        }
        stations.push(tele);
    }

    let mut robs = Vec::with_capacity(COASTAL_ROB_POINTS.len());
    for rb in COASTAL_ROB_POINTS {
        let ovr_opt = frame.rob_overrides.iter().find(|o| o.rob_id == rb.id);
        let (msl, wave) = if let Some(o) = ovr_opt {
            (o.msl_m, o.wave_m)
        } else {
            (rb.trigger_msl_m * 0.5, 0.55)
        };
        robs.push(evaluate_rob(
            rb,
            msl,
            msl * 1.05,
            frame.timestamp_wib.clone(),
            wave,
        ));
    }

    let reports = vec![CitizenReport {
        pkey: format!("replay-{}-{}", scenario.id, clamped_idx),
        created_at: frame.timestamp_wib.clone(),
        source: "arsip_historis".to_string(),
        status: "confirmed".to_string(),
        depth_cm: Some(120),
        title: frame.step_label.clone(),
        text: frame.narrative.clone(),
        lat: scenario.center_lat,
        lon: scenario.center_lon,
    }];

    let kpis = compute_kpis(&stations, &robs, reports.len());
    Some(FloodSnapshot {
        generated_at_utc: frame.timestamp_wib.clone(),
        is_historical_replay: true,
        replay_id: Some(scenario.id.to_string()),
        replay_label: Some(format!("{} · {}", scenario.title, frame.step_label)),
        replay_citation: Some(scenario.citation.to_string()),
        kpis,
        upstream: UpstreamHealth {
            glofas_ok: true,
            weather_ok: true,
            marine_ok: true,
            petabencana_ok: true,
            bmkg_nowcast_ok: true,
            news_ok: true,
            notes: vec![frame.narrative.clone()],
        },
        stations,
        coastal_rob: robs,
        citizen_reports: reports,
        bmkg_nowcasts: Vec::new(),
        news: Vec::new(),
    })
}

fn ovr(
    station_id: &str,
    discharge_m3s: f64,
    rain_24h_mm: f64,
    rain_72h_mm: f64,
    soil_top: f64,
    soil_deep: f64,
    mechanism: FloodMechanism,
) -> ReplayStationOverride {
    ReplayStationOverride {
        station_id: station_id.to_string(),
        discharge_m3s,
        rain_24h_mm,
        rain_72h_mm,
        soil_top,
        soil_deep,
        mechanism,
    }
}

fn rob(rob_id: &str, msl_m: f64, wave_m: f64) -> ReplayRobOverride {
    ReplayRobOverride {
        rob_id: rob_id.to_string(),
        msl_m,
        wave_m,
    }
}
