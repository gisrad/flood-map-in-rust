pub mod ai;
pub mod catalog;
pub mod hydrology;
pub mod model;
pub mod replay;
pub mod rss;

#[cfg(test)]
mod tests {
    use crate::ai::{execute_ai_query, generate_briefing};
    use crate::catalog::{COASTAL_ROB_POINTS, RIVER_STATIONS};
    use crate::hydrology::{evaluate_station, haversine_km};
    use crate::model::{AlertLevel, Audience, FloodMechanism, HydrographPoint};
    use crate::replay::{build_replay_snapshot, list_replay_scenarios};
    use crate::rss::{
        build_alerts_rss_xml, parse_bmkg_nowcast_rss, parse_filtered_indonesian_news,
        parse_flood_news_rss,
    };

    #[test]
    fn all_stations_and_rob_inside_indonesia_bbox() {
        assert_eq!(RIVER_STATIONS.len(), 50);
        assert_eq!(COASTAL_ROB_POINTS.len(), 12);
        for st in RIVER_STATIONS {
            assert!(
                (-11.5..=6.5).contains(&st.lat) && (94.5..=141.5).contains(&st.lon),
                "Station {} out of Indonesia bounds",
                st.id
            );
            assert!(st.river_polyline.len() >= 2, "Polyline missing for {}", st.id);
            assert!((7..=9).contains(&st.tz_offset_hours));
        }
        for rb in COASTAL_ROB_POINTS {
            assert!(
                (-11.5..=6.5).contains(&rb.lat) && (94.5..=141.5).contains(&rb.lon),
                "Rob {} out of Indonesia bounds",
                rb.id
            );
        }
    }

    #[test]
    fn haversine_jakarta_bogor_reasonable() {
        let d = haversine_km(-6.208, 106.849, -6.633, 106.837);
        assert!((45.0..=50.0).contains(&d), "Expected ~47 km, got {d}");
    }

    #[test]
    fn evaluate_station_distinguishes_fluvial_and_awas() {
        let st = &RIVER_STATIONS[0];
        let mut hydro = Vec::new();
        for i in 0..14 {
            hydro.push(HydrographPoint {
                date: format!("2026-11-{:02}", i + 1),
                is_forecast: i >= 10,
                discharge_m3s: if i >= 9 { 65.0 } else { 10.0 },
                discharge_mean_m3s: if i >= 9 { 65.0 } else { 10.0 },
                discharge_max_m3s: 80.0,
                discharge_p75_m3s: 20.0,
                rain_mm: 20.0,
            });
        }
        let tele = evaluate_station(st, hydro, 10, 40.0, 90.0, 0.35, 0.32, 0);
        assert_eq!(tele.alert_level, AlertLevel::Awas);
        assert_eq!(tele.mechanism, FloodMechanism::FluvialBasin);
    }

    #[test]
    fn replay_and_ai_query_work_end_to_end() {
        let scenarios = list_replay_scenarios();
        assert_eq!(scenarios.len(), 3);
        let snap = build_replay_snapshot("jabodetabek-2020", 1).expect("replay exists");
        assert!(snap.is_historical_replay);
        assert!(snap.kpis.awas_count > 0);

        let resp = execute_ai_query(&snap, "ciliwung");
        assert!(!resp.matched_station_ids.is_empty());
        assert!(resp.fly_to.is_some());

        let briefing = generate_briefing(&snap, "Jawa", Audience::Gov);
        assert!(briefing.markdown.contains("SitRep"));
        assert!(!briefing.priority_actions.is_empty());

        let rss_xml = build_alerts_rss_xml(&snap, "Jawa", "https://flood.transbulent.id");
        assert!(rss_xml.contains("<rss version=\"2.0\">"));
        assert!(rss_xml.contains("Ciliwung"));
    }

    #[test]
    fn parses_bmkg_cap_and_google_news_rss() {
        let bmkg_xml = r#"<rss version="2.0"><channel><item>
            <title>Hujan Lebat disertai Petir di Jawa Tengah</title>
            <link>https://www.bmkg.go.id/alerts/nowcast/id/CJH1.xml</link>
            <description>Hujan lebat &amp; banjir lokal di Demak.</description>
            <pubDate>Sun, 22 Nov 2026 16:00:00 +0700</pubDate>
        </item></channel></rss>"#;
        let nowcasts = parse_bmkg_nowcast_rss(bmkg_xml);
        assert_eq!(nowcasts.len(), 1);
        assert_eq!(nowcasts[0].province, "Jawa Tengah");
        assert!(nowcasts[0].summary.contains("& banjir lokal"));

        let gnews_xml = r#"<rss version="2.0"><channel><item>
            <title><![CDATA[Banjir Rob Melanda Pesisir Semarang Utara - Kompas.com]]></title>
            <link>https://news.google.com/rss/articles/123</link>
            <pubDate>Sun, 22 Nov 2026 08:15:00 GMT</pubDate>
            <source url="https://www.kompas.com">Kompas.com</source>
        </item></channel></rss>"#;
        let news = parse_flood_news_rss(gnews_xml);
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].source, "Kompas.com");
        assert_eq!(news[0].title, "Banjir Rob Melanda Pesisir Semarang Utara");
        assert_eq!(news[0].region_hint, "Jawa");

        let indo_xml = r#"<rss version="2.0"><channel>
            <item><title>Sungai Citarum Meluap di Karawang, BPBD Evakuasi Warga</title><link>https://www.cnnindonesia.com/1</link><description>Banjir merendam permukiman.</description><pubDate>Sun, 22 Nov 2026 10:00:00 +0700</pubDate></item>
            <item><title>Berita Ekonomi Pasar Modal</title><link>https://www.cnnindonesia.com/2</link><description>IHSG menguat tipis.</description><pubDate>Sun, 22 Nov 2026 09:00:00 +0700</pubDate></item>
        </channel></rss>"#;
        let (matched, fallback) = parse_filtered_indonesian_news(indo_xml, "CNN Indonesia");
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].region_hint, "Jawa");
        assert_eq!(fallback.len(), 1);
    }
}
