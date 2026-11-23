use crate::ai::{execute_ai_query, generate_briefing};
use crate::calibration::{glofas_calibration, rob_calibration_file};
use crate::catalog::{COASTAL_ROB_POINTS, RIVER_STATIONS};
use crate::geo::build_stations_geojson;
use crate::hydrology::{
    combine_levels, compute_kpis, evaluate_rob, evaluate_station, fmt_dec, gumbel_lmoments,
    gumbel_quantile, haversine_km, pluvial_level, rain_level, river_level, rob_level,
    StationInputs,
};
use crate::model::{
    Audience, CitizenReport, FloodMechanism, FloodSnapshot, HydrographPoint, IndicatorLevel,
    RiverThresholds, RobThresholds, UpstreamHealth, METHODOLOGY_VERSION, SCHEMA_VERSION,
};
use crate::replay::{build_replay_snapshot, list_replay_scenarios};
use crate::reports::parse_petabencana_geojson;
use crate::rss::{
    build_alerts_rss_xml, epoch_to_iso, is_flood_headline, merge_news, parse_flood_news,
    parse_rss_date,
};
use IndicatorLevel::*;

fn th() -> RiverThresholds {
    RiverThresholds {
        q2_m3s: 100.0,
        q5_m3s: 150.0,
        q20_m3s: 220.0,
        p90_m3s: 60.0,
        p99_m3s: 120.0,
        period: "1997–2024".into(),
        n_years: 28,
    }
}

#[test]
fn catalog_inside_indonesia() {
    assert_eq!(RIVER_STATIONS.len(), 50);
    assert_eq!(COASTAL_ROB_POINTS.len(), 12);
    for st in RIVER_STATIONS {
        assert!(
            (-11.5..=6.5).contains(&st.lat) && (94.5..=141.5).contains(&st.lon),
            "{}",
            st.id
        );
        assert!((7..=9).contains(&st.tz_offset_hours));
    }
}

#[test]
fn haversine_jakarta_bogor() {
    let d = haversine_km(-6.208, 106.849, -6.633, 106.837);
    assert!((45.0..=50.0).contains(&d), "{d}");
}

#[test]
fn indicator_level_contract() {
    assert_eq!(serde_json::to_string(&VeryHigh).unwrap(), "\"very_high\"");
    assert_eq!(Unknown.rank(), 0);
    assert_eq!(Extreme.rank(), 4);
    assert_eq!(VeryHigh.label_id(), "Sangat tinggi");
    assert_eq!(Unknown.label_id(), "Belum terkalibrasi");
    assert!(Unknown < Normal && Normal < High && High < VeryHigh && VeryHigh < Extreme);
}

#[test]
fn gumbel_lmoments_known_sample() {
    // Sampel 1..5: l1 = 3, l2 = 1 → α = 1/ln2, ξ = 3 − γ/ln2.
    let (xi, alpha) = gumbel_lmoments(&[5.0, 1.0, 4.0, 2.0, 3.0]).unwrap();
    assert!((alpha - std::f64::consts::LOG2_E).abs() < 1e-12, "{alpha}");
    assert!((xi - 2.167_253_82).abs() < 1e-6, "{xi}");
    assert!((gumbel_quantile(xi, alpha, 2.0) - 2.696_020_2).abs() < 1e-6);
    assert!((gumbel_quantile(xi, alpha, 5.0) - 4.331_209_8).abs() < 1e-6);
    assert!((gumbel_quantile(xi, alpha, 20.0) - 6.452_339_8).abs() < 1e-6);
    assert!(
        gumbel_lmoments(&[1.0, 2.0, 3.0]).is_none(),
        "sampel < 5 ditolak"
    );
    assert!(
        gumbel_lmoments(&[4.0; 10]).is_none(),
        "sampel tanpa variasi ditolak"
    );
}

#[test]
fn gumbel_recovers_parameters_from_theoretical_quantiles() {
    // Kuantil Gumbel(ξ=500, α=120) pada posisi plot (i−0,35)/n mendekati parameter asal.
    let n = 200;
    let sample: Vec<f64> = (1..=n)
        .map(|i| {
            let p = (i as f64 - 0.35) / n as f64;
            500.0 - 120.0 * (-p.ln()).ln()
        })
        .collect();
    let (xi, alpha) = gumbel_lmoments(&sample).unwrap();
    assert!((xi - 500.0).abs() < 5.0, "{xi}");
    assert!((alpha - 120.0).abs() < 5.0, "{alpha}");
}

#[test]
fn river_level_thresholds() {
    let t = th();
    assert_eq!(river_level(99.9, Some(&t)), Normal);
    assert_eq!(river_level(100.0, Some(&t)), High);
    assert_eq!(river_level(149.0, Some(&t)), High);
    assert_eq!(river_level(150.0, Some(&t)), VeryHigh);
    assert_eq!(river_level(220.0, Some(&t)), Extreme);
    assert_eq!(river_level(5000.0, None), Unknown);
    assert_eq!(river_level(f64::NAN, Some(&t)), Unknown);
}

#[test]
fn bmkg_rain_categories() {
    assert_eq!(rain_level(49.9), Normal);
    assert_eq!(rain_level(50.0), High);
    assert_eq!(rain_level(99.9), High);
    assert_eq!(rain_level(100.0), VeryHigh);
    assert_eq!(rain_level(150.0), VeryHigh, "ekstrem BMKG adalah > 150 mm");
    assert_eq!(rain_level(150.1), Extreme);
    assert_eq!(
        pluvial_level(10.0, 120.0, 0),
        VeryHigh,
        "hujan 24 jam ke depan ikut dinilai"
    );
    assert_eq!(
        pluvial_level(0.0, 0.0, 1),
        High,
        "laporan warga menaikkan minimal ke tinggi"
    );
    assert_eq!(pluvial_level(160.0, 0.0, 3), Extreme);
}

#[test]
fn combine_never_hides_uncalibrated_river() {
    assert_eq!(
        combine_levels(Unknown, Normal),
        (Unknown, FloodMechanism::FluvialBasin)
    );
    assert_eq!(
        combine_levels(Unknown, High),
        (High, FloodMechanism::UrbanPluvial)
    );
    assert_eq!(
        combine_levels(VeryHigh, High),
        (VeryHigh, FloodMechanism::FluvialBasin)
    );
    assert_eq!(
        combine_levels(High, High),
        (High, FloodMechanism::FluvialBasin)
    );
    assert_eq!(
        combine_levels(Normal, Extreme),
        (Extreme, FloodMechanism::UrbanPluvial)
    );
}

#[test]
fn rob_levels_against_local_percentiles() {
    let t = RobThresholds {
        p95_m: 1.0,
        p99_m: 1.2,
        max_m: 1.4,
        period: "2022–2025".into(),
    };
    assert_eq!(rob_level(1.19, Some(&t)), Normal);
    assert_eq!(rob_level(1.2, Some(&t)), High);
    assert_eq!(rob_level(1.45, Some(&t)), VeryHigh);
    assert_eq!(rob_level(1.0, None), Unknown);
    assert_eq!(rob_level(f64::NAN, Some(&t)), Unknown);
}

#[test]
fn evaluate_station_uses_calibration_or_reports_unknown() {
    let hydro: Vec<HydrographPoint> = (0..5)
        .map(|i| HydrographPoint {
            date: format!("2026-11-{:02}", 18 + i),
            is_forecast: i > 2,
            discharge_m3s: 10.0 + i as f64,
            discharge_min_m3s: None,
            discharge_p25_m3s: None,
            discharge_p75_m3s: None,
            discharge_max_m3s: None,
            rain_mm: Some(1.0),
        })
        .collect();
    for st in RIVER_STATIONS {
        let tele = evaluate_station(
            st,
            StationInputs {
                hydrograph: hydro.clone(),
                today_idx: 2,
                ..Default::default()
            },
        );
        assert_eq!(tele.current_discharge_m3s, 12.0);
        assert_eq!(tele.peak_forecast_m3s, 14.0);
        if tele.thresholds.is_none() {
            assert_eq!(tele.level, Unknown, "{}", st.id);
            assert!(tele.level_reason.contains("belum ditetapkan"));
        } else {
            assert!(tele.glofas_cell.is_some());
        }
        assert!(tele.public_advice.contains("bukan peringatan resmi"));
        for banned in ["Waspada", "Siaga", "Awas", "WASPADA", "SIAGA", "AWAS"] {
            let all = format!(
                "{} {} {} {}",
                tele.level_reason, tele.public_advice, tele.gov_action, tele.academic_note
            );
            assert!(
                !all.contains(banned),
                "{} memuat istilah status resmi {banned}",
                st.id
            );
        }
    }
}

#[test]
fn calibration_files_are_consistent() {
    let g = glofas_calibration();
    assert_eq!(
        g.stations.len(),
        RIVER_STATIONS.len(),
        "kalibrasi mencakup semua stasiun"
    );
    let mut accepted = 0;
    for s in &g.stations {
        assert!(
            RIVER_STATIONS.iter().any(|r| r.id == s.id),
            "id {} tidak ada di katalog",
            s.id
        );
        if let Some(t) = s.thresholds() {
            accepted += 1;
            assert!(t.q2_m3s < t.q5_m3s && t.q5_m3s < t.q20_m3s, "{}", s.id);
            assert!(t.n_years >= 20);
            assert!(s.cell.as_ref().unwrap().offset_km <= 20.0);
        } else {
            assert!(!s.note.is_empty() || !s.accepted, "{}", s.id);
        }
    }
    assert!(accepted >= 25, "hanya {accepted} stasiun terkalibrasi");
    let r = rob_calibration_file();
    assert_eq!(r.points.len(), COASTAL_ROB_POINTS.len());
    for p in &r.points {
        if let Some(t) = &p.thresholds {
            assert!(t.p95_m <= t.p99_m && t.p99_m <= t.max_m, "{}", p.id);
        }
    }
}

#[test]
fn petabencana_keeps_only_floods() {
    let root: serde_json::Value = serde_json::from_str(
        r#"{"statusCode":200,"result":{"type":"FeatureCollection","features":[
        {"type":"Feature","geometry":{"type":"Point","coordinates":[112.92,0.86]},"properties":{"pkey":"400074","created_at":"2026-11-22T04:27:55.601Z","source":"grasp","status":"confirmed","url":"f9d78d84","image_url":"https://images.petabencana.id/f9d78d84.jpg","disaster_type":"flood","report_data":{"report_type":"flood","flood_depth":72},"tags":{"city":"KAPUAS HULU"},"title":null,"text":"Banjir di Kota Putusibau"}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[106.31,-6.19]},"properties":{"pkey":"400075","created_at":"2026-11-22T07:05:31Z","source":"grasp","status":"confirmed","disaster_type":"fire","report_data":{"report_type":"fire"},"title":null,"text":"Kebakaran pabrik"}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[107.1,-6.9]},"properties":{"pkey":"400076","created_at":"2026-11-22T08:00:00Z","source":"grasp","status":"confirmed","disaster_type":"earthquake","title":null,"text":"Gempa terasa"}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[150.0,-6.9]},"properties":{"pkey":"400077","disaster_type":"flood","text":"di luar Indonesia"}}
        ]}}"#,
    )
    .unwrap();
    let reports = parse_petabencana_geojson(&root).unwrap();
    assert_eq!(reports.len(), 1);
    let r = &reports[0];
    assert_eq!(r.pkey, "400074");
    assert_eq!(r.disaster_type, "flood");
    assert_eq!(r.depth_cm, Some(72));
    assert_eq!(r.title, "Laporan banjir warga · Kapuas Hulu");
    assert_eq!(
        r.url.as_deref(),
        Some("https://images.petabencana.id/f9d78d84.jpg")
    );
    assert!(parse_petabencana_geojson(&serde_json::json!({"result": {}})).is_err());
}

#[test]
fn news_filter_is_strict_recent_and_iso() {
    assert!(is_flood_headline(
        "Banjir di Sumut hingga Kalbar, Ratusan Keluarga Terdampak"
    ));
    assert!(is_flood_headline("Rob Rendam Pesisir Pekalongan"));
    assert!(is_flood_headline(
        "Tanggul Sungai Wulan Jebol, Warga Mengungsi"
    ));
    assert!(is_flood_headline("Sungai Citarum Meluap di Karawang"));
    assert!(!is_flood_headline("UMKM Banjir Pesanan Jelang Natal"));
    assert!(!is_flood_headline("Pabrik Permen di Serang Terbakar"));
    assert!(!is_flood_headline("Longsor Tutup Jalan Lintas Sumatera"));
    assert!(!is_flood_headline("Robot Pintar Dipamerkan di Jakarta"));

    let now = parse_rss_date("Sun, 22 Nov 2026 12:00:00 +0700").unwrap();
    assert_eq!(epoch_to_iso(now), "2026-11-22T05:00:00Z");
    let xml = r#"<rss><channel>
        <item><title>Banjir Rendam 300 Rumah di Bekasi</title><link>https://a/1</link><pubDate>Sun, 22 Nov 2026 10:00:00 +0700</pubDate></item>
        <item><title><![CDATA[Banjir Bandang Terjang Garut]]></title><link>https://a/2</link><pubDate>Thu, 15 Oct 2026 10:00:00 +0700</pubDate></item>
        <item><title>Harga Cabai Naik</title><link>https://a/3</link><pubDate>Sun, 22 Nov 2026 09:00:00 +0700</pubDate></item>
        <item><title>Banjir Kritik untuk Kebijakan Baru</title><link>https://a/4</link><pubDate>Sun, 22 Nov 2026 09:00:00 +0700</pubDate></item>
    </channel></rss>"#;
    let news = parse_flood_news(xml, "Uji", now);
    assert_eq!(
        news.len(),
        1,
        "berita 5 minggu lalu, non-banjir, dan kiasan ditolak"
    );
    assert_eq!(news[0].pub_date, "2026-11-22T03:00:00Z");
    assert_eq!(news[0].region_hint, "Jawa");
    assert!(
        parse_flood_news("<rss><channel></channel></rss>", "Uji", now).is_empty(),
        "tanpa fallback"
    );

    let mut dup = news.clone();
    dup.push(news[0].clone());
    assert_eq!(merge_news(dup, 10).len(), 1);
}

#[test]
fn rss_dates_parse() {
    assert_eq!(
        epoch_to_iso(parse_rss_date("Sun, 22 Nov 2026 08:15:00 GMT").unwrap()),
        "2026-11-22T08:15:00Z"
    );
    assert_eq!(
        epoch_to_iso(parse_rss_date("2026-11-22T10:32:16+07:00").unwrap()),
        "2026-11-22T03:32:16Z"
    );
    assert_eq!(epoch_to_iso(0), "1970-01-01T00:00:00Z");
    assert!(parse_rss_date("").is_none());
}

fn sample_snapshot() -> FloodSnapshot {
    let st = &RIVER_STATIONS[0];
    let tele = evaluate_station(
        st,
        StationInputs {
            rain_24h_mm: 120.0,
            ..Default::default()
        },
    );
    let rob = evaluate_rob(
        &COASTAL_ROB_POINTS[0],
        0.5,
        0.6,
        "2026-11-22T10:00:00Z".into(),
        0.8,
    );
    let report = CitizenReport {
        pkey: "1".into(),
        created_at: "2026-11-22T04:00:00Z".into(),
        source: "grasp".into(),
        status: "confirmed".into(),
        depth_cm: Some(40),
        title: "Laporan banjir warga".into(),
        text: String::new(),
        lat: -6.2,
        lon: 106.8,
        disaster_type: "flood".into(),
        url: None,
    };
    let stations = vec![tele];
    let robs = vec![rob];
    FloodSnapshot {
        schema_version: SCHEMA_VERSION,
        methodology_version: METHODOLOGY_VERSION.into(),
        generated_at_utc: "2026-11-22T10:00:00Z".into(),
        is_historical_replay: false,
        replay_id: None,
        replay_label: None,
        replay_citation: None,
        replay_note: None,
        kpis: compute_kpis(&stations, &robs, 1, 0),
        upstream: UpstreamHealth::default(),
        stations,
        coastal_rob: robs,
        citizen_reports: vec![report],
        bmkg_nowcasts: Vec::new(),
        news: Vec::new(),
    }
}

#[test]
fn geojson_builder_emits_all_kinds() {
    let snap = sample_snapshot();
    let gj = build_stations_geojson(&snap);
    assert_eq!(gj["type"], "FeatureCollection");
    let feats = gj["features"].as_array().unwrap();
    assert_eq!(feats.len(), 3);
    let kinds: Vec<&str> = feats
        .iter()
        .map(|f| f["properties"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["river", "rob", "report"]);
    let river = &feats[0];
    assert_eq!(river["geometry"]["coordinates"][0], RIVER_STATIONS[0].lon);
    assert_eq!(
        river["properties"]["level"], "very_high",
        "hujan 120 mm = sangat lebat"
    );
    assert_eq!(river["properties"]["level_label"], "Sangat tinggi");
    assert_eq!(river["properties"]["mechanism"], "urban_pluvial");
    assert_eq!(
        river["properties"]["updated_at_utc"],
        "2026-11-22T10:00:00Z"
    );
    assert_eq!(feats[2]["properties"]["depth_cm"], 40);
}

#[test]
fn snapshot_json_has_contract_fields() {
    let v = serde_json::to_value(sample_snapshot()).unwrap();
    assert_eq!(v["schema_version"], 2);
    assert_eq!(v["methodology_version"], "2026-11-22");
    for k in [
        "river",
        "pluvial",
        "rob",
        "citizen_flood_reports_24h",
        "bmkg_nowcasts",
        "max_ratio_q2",
        "max_ratio_station",
        "uncalibrated",
    ] {
        assert!(v["kpis"].get(k).is_some(), "kpis.{k}");
    }
    let st = &v["stations"][0];
    for k in [
        "glofas_cell",
        "thresholds",
        "ratio_q2",
        "peak_forecast_level",
        "exceed_prob_7d",
        "rain_next_24h_mm",
        "fluvial_level",
        "pluvial_level",
        "level",
        "level_reason",
    ] {
        assert!(st.get(k).is_some(), "stations[].{k}");
    }
    for k in [
        "river_polyline",
        "alert_level",
        "anomaly_ratio",
        "p75_baseline_m3s",
    ] {
        assert!(st.get(k).is_none(), "field lama {k} masih ada");
    }
    let rob = &v["coastal_rob"][0];
    assert!(rob.get("sensitivity").is_none() && rob.get("trigger_msl_m").is_none());
    assert!(rob.get("thresholds").is_some());
}

#[test]
fn alerts_rss_uses_model_levels_with_disclaimer() {
    let snap = sample_snapshot();
    let xml = build_alerts_rss_xml(&snap, "", "https://flood.transbulent.id");
    assert!(xml.contains("<rss version=\"2.0\">"));
    assert!(xml.contains("[Sangat tinggi]"));
    assert!(xml.contains("bukan peringatan resmi"));
    for banned in ["Waspada", "Siaga", "Awas"] {
        assert!(!xml.contains(banned));
    }
}

#[test]
fn replay_scenarios_are_baked_from_data() {
    let scenarios = list_replay_scenarios();
    assert_eq!(scenarios.len(), 3);
    for sc in &scenarios {
        assert!(sc.provenance.contains("Bukan pengukuran pos duga air"));
        assert!(!sc.citation.is_empty());
        assert!(sc.frames.len() >= 5, "{}", sc.id);
        let first = build_replay_snapshot(&sc.id, 0).unwrap();
        let last = build_replay_snapshot(&sc.id, sc.frames.len() - 1).unwrap();
        assert!(first.is_historical_replay);
        assert_eq!(first.stations.len(), RIVER_STATIONS.len());
        let h0 = first.stations[0].hydrograph.len();
        let h1 = last.stations[0].hydrograph.len();
        assert!(h1 > h0, "hidrograf replay bertambah sesuai bingkai");
        assert!(first
            .stations
            .iter()
            .all(|s| s.hydrograph.iter().all(|p| !p.is_forecast)));
    }
    let jkt = scenarios
        .iter()
        .find(|s| s.id == "jabodetabek-2020")
        .unwrap();
    assert!(jkt.citation.contains("Halim"));
    assert!(build_replay_snapshot("tidak-ada", 0).is_none());
}

#[test]
fn replay_discloses_model_versus_event_and_hides_uncalibrated_discharge() {
    for sc in list_replay_scenarios() {
        let snap = build_replay_snapshot(&sc.id, 0).unwrap();
        assert!(sc.outcome.len() >= 3, "{}: blok hasil kosong", sc.id);
        assert!(sc.outcome[0].starts_with("Model "), "{}", sc.id);
        for st in snap.stations.iter().filter(|s| s.thresholds.is_none()) {
            let quoted = format!("{}: debit model", st.name);
            assert!(
                sc.frames.iter().all(|f| !f.narrative.contains(&quoted)),
                "{}: narasi mengutip debit sel belum terkalibrasi {}",
                sc.id,
                st.id
            );
            for line in sc.outcome.iter().filter(|l| l.starts_with(&st.name)) {
                assert!(line.contains("tidak dinilai"), "{line}");
            }
        }
        // Hindcast: tanpa prakiraan dan tanpa saran yang terbaca sebagai kondisi saat ini.
        for st in &snap.stations {
            assert!(!st.level_reason.contains("ke depan"), "{}", st.level_reason);
            assert!(!st.academic_note.contains("puncak 7 hari"));
            assert!(st.public_advice.contains("replay") || st.public_advice.contains("Replay"));
        }
        assert!(snap
            .replay_note
            .as_deref()
            .is_none_or(|n| !n.contains(&sc.provenance)));
    }
    let jkt = list_replay_scenarios()
        .into_iter()
        .find(|s| s.id == "jabodetabek-2020")
        .unwrap();
    assert!(
        jkt.outcome.iter().any(|l| l.contains("377 mm teramati")),
        "pembanding hujan teramati hilang: {:?}",
        jkt.outcome
    );
}

#[test]
fn briefing_never_reassures_without_assessable_points() {
    let mut snap = build_replay_snapshot("barito-2021", 6).unwrap();
    snap.is_historical_replay = false;

    let b = generate_briefing(&snap, "<img src=x onerror=alert(1)>", Audience::Public);
    assert!(
        b.headline.contains("tidak ada titik pantau"),
        "{}",
        b.headline
    );
    assert!(!b.sitrep_summary.contains("tidak menunjukkan"));
    assert!(!b.markdown.contains("<img") && !b.markdown.contains("alert(1)"));
    assert_eq!(b.region_filter, "img srcx onerroralert1");

    let uncal_region = snap
        .stations
        .iter()
        .find(|s| s.thresholds.is_none())
        .map(|s| s.province.clone())
        .unwrap();
    let b = generate_briefing(&snap, &uncal_region, Audience::Public);
    assert!(
        b.sitrep_summary.contains("belum terkalibrasi"),
        "{}",
        b.sitrep_summary
    );
}

#[test]
fn ai_query_matches_whole_words_not_substrings() {
    let mut snap = build_replay_snapshot("barito-2021", 6).unwrap();
    snap.is_historical_replay = false;

    let r = execute_ai_query(&snap, "apakah aman di surabaya");
    assert!(
        r.matched_station_ids.is_empty(),
        "{:?}",
        r.matched_station_ids
    );
    assert_eq!(r.matched_rob_ids, vec!["rob-surabaya-gresik".to_string()]);
    assert!(r.answer_id.starts_with("Model tidak dapat menyatakan"));

    let r = execute_ai_query(&snap, "ciliwung manggarai");
    assert_eq!(
        r.matched_station_ids,
        vec!["ciliwung-manggarai".to_string()]
    );

    let r = execute_ai_query(&snap, "probolinggo");
    assert!(r.fly_to.is_none(), "{:?}", r.answer_id);
}

#[test]
fn indonesian_number_format() {
    assert_eq!(fmt_dec(1234.5, 1), "1.234,5");
    assert_eq!(fmt_dec(0.25, 2), "0,25");
    assert_eq!(fmt_dec(-12.0, 0), "-12");
    assert_eq!(fmt_dec(f64::NAN, 1), "–");
}

#[test]
fn news_filter_rejects_prevention_and_names_but_keeps_flood_impacts() {
    assert!(!is_flood_headline(
        "Aksi Bersih Sungai untuk Antisipasi Banjir Luapan"
    ));
    assert!(!is_flood_headline(
        "Simulasi Evakuasi Banjir di Jakarta Timur"
    ));
    assert!(!is_flood_headline("Rob Rock Gelar Konser di Bandung"));
    assert!(is_flood_headline(
        "Banjir Rendam Pejaten Timur, Warga Kesulitan Air Bersih"
    ));
    assert!(is_flood_headline("Rob Rendam Permukiman Pesisir Semarang"));
}

#[test]
fn news_with_non_https_link_is_dropped() {
    let now = parse_rss_date("Sun, 22 Nov 2026 12:00:00 +0700").unwrap();
    let item = |link: &str| {
        format!(
            "<rss><channel><item><title>Banjir Rendam Pejaten Timur</title><link>{link}</link><pubDate>Sun, 22 Nov 2026 09:00:00 +0700</pubDate></item></channel></rss>"
        )
    };
    assert!(parse_flood_news(&item("javascript:alert(1)"), "Uji", now).is_empty());
    assert!(parse_flood_news(&item("http://a/1"), "Uji", now).is_empty());
    let ok = parse_flood_news(&item("https://a/1"), "Uji", now);
    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0].region_hint, "Jawa");
}

#[test]
fn rfc2822_dates_and_rss_items_are_well_formed() {
    use crate::rss::epoch_to_rfc2822;
    assert_eq!(epoch_to_rfc2822(0), "Thu, 01 Jan 1970 00:00:00 GMT");
    let ts = parse_rss_date("Sun, 22 Nov 2026 16:00:00 +0700").unwrap();
    assert_eq!(epoch_to_rfc2822(ts), "Sun, 22 Nov 2026 09:00:00 GMT");

    let xml = build_alerts_rss_xml(&sample_snapshot(), "", "https://flood.transbulent.id");
    let pub_dates: Vec<&str> = xml
        .split("<pubDate>")
        .skip(1)
        .map(|s| s.split("</pubDate>").next().unwrap())
        .collect();
    assert!(!pub_dates.is_empty());
    assert!(
        pub_dates
            .iter()
            .all(|d| d.ends_with(" GMT") && d.contains(", ")),
        "{pub_dates:?}"
    );
    assert!(xml.contains("/?s="));
    assert!(!xml.contains("?basin="));
}

#[test]
fn only_confirmed_or_verified_citizen_reports_count() {
    let report = |status: &str| -> crate::model::CitizenReport {
        serde_json::from_value(serde_json::json!({
            "pkey": "1", "created_at": "", "source": "petabencana", "status": status,
            "title": "t", "text": "", "lat": 0.0, "lon": 100.0, "disaster_type": "flood"
        }))
        .unwrap()
    };
    assert!(report("confirmed").is_verified());
    assert!(report("Verified").is_verified());
    assert!(!report("unconfirmed").is_verified());
    assert!(!report("").is_verified());
}

#[test]
fn briefing_markdown_shows_wib_time() {
    let b = generate_briefing(&sample_snapshot(), "", Audience::Public);
    assert!(b.markdown.contains(" WIB"), "{}", b.markdown);
    assert!(b.markdown.contains("UTC"), "{}", b.markdown);
}

#[test]
fn grrr_crosscheck_is_consistent_with_current_calibration() {
    use crate::crosscheck::{grrr_check, grrr_file};
    use crate::model::GrrrVerdict;
    let f = grrr_file();
    let mut n = 0;
    for st in RIVER_STATIONS {
        let th = crate::calibration::station_calibration(st.id).and_then(|c| c.thresholds());
        let chk = grrr_check(st.id);
        // Hanya stasiun terkalibrasi yang diuji, dan semuanya diuji.
        assert_eq!(th.is_some(), chk.is_some(), "{}", st.id);
        let (Some(th), Some(chk)) = (th, chk) else {
            continue;
        };
        n += 1;
        let expected = match (chk.q2_min_m3s, chk.q2_max_m3s) {
            (Some(_), Some(hi)) if th.q2_m3s > f.above_factor * hi => GrrrVerdict::GlofasAbove,
            (Some(lo), Some(_)) if th.q2_m3s < f.below_factor * lo => GrrrVerdict::GlofasBelow,
            (Some(_), Some(_)) => GrrrVerdict::WithinRange,
            _ => GrrrVerdict::NoOutlets,
        };
        assert_eq!(chk.verdict, expected, "{}: Q2 {}", st.id, th.q2_m3s);
        if let Some(c) = &chk.closest {
            let ratio = th.q2_m3s / c.q2_m3s;
            assert!(
                // Rasio dibulatkan ke 2 desimal oleh skrip; Q2 Rust dibulatkan ke 2 desimal.
                (chk.ratio_q2.unwrap() - ratio).abs() <= 0.006 + 0.01 * ratio,
                "{}: rasio {} vs {}",
                st.id,
                chk.ratio_q2.unwrap(),
                ratio
            );
            assert!(c.q2_m3s <= c.q5_m3s && c.q5_m3s <= c.q20_m3s, "{}", st.id);
        }
    }
    assert_eq!(n as u32, f.summary.n_assessed);
}

#[test]
fn inundation_exposure_covers_every_point_and_is_sane() {
    use crate::crosscheck::inundation_exposure;
    let ids = RIVER_STATIONS
        .iter()
        .map(|s| s.id)
        .chain(COASTAL_ROB_POINTS.iter().map(|r| r.id));
    for id in ids {
        let e = inundation_exposure(id).unwrap_or_else(|| panic!("tanpa paparan: {id}"));
        assert!((e.buffer_km2 - 78.5).abs() < 0.2, "{id}: {}", e.buffer_km2);
        let (low, medium, high) = e.fractions();
        assert!(
            (0.0..=1.0).contains(&high) && high <= medium && medium <= low && low <= 1.0,
            "{id}"
        );
    }
    // Muncul di API untuk stasiun dan titik rob.
    let st = RIVER_STATIONS
        .iter()
        .find(|s| s.id == "ciliwung-manggarai")
        .unwrap();
    let t = evaluate_station(st, StationInputs::default());
    assert!(t.inundation_history.is_some() && t.grrr_check.is_some());
}
