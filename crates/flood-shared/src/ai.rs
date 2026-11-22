use crate::model::{
    AiBriefing, AiFlyTarget, AiQueryResponse, AlertLevel, Audience, FloodMechanism, FloodSnapshot,
    StationTelemetry,
};

pub fn generate_briefing(
    snapshot: &FloodSnapshot,
    region_filter: &str,
    audience: Audience,
) -> AiBriefing {
    let rf = region_filter.trim();
    let rf_low = rf.to_ascii_lowercase();

    let mut stations: Vec<&StationTelemetry> = snapshot
        .stations
        .iter()
        .filter(|s| {
            rf_low.is_empty()
                || rf_low == "indonesia"
                || rf_low == "nasional"
                || s.province.to_ascii_lowercase().contains(&rf_low)
                || s.island.to_ascii_lowercase().contains(&rf_low)
                || s.river.to_ascii_lowercase().contains(&rf_low)
        })
        .collect();

    stations.sort_by(|a, b| {
        b.alert_level
            .rank()
            .cmp(&a.alert_level.rank())
            .then_with(|| {
                b.anomaly_ratio
                    .partial_cmp(&a.anomaly_ratio)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let elevated: Vec<&StationTelemetry> = stations
        .iter()
        .copied()
        .filter(|s| s.alert_level != AlertLevel::Normal)
        .collect();

    let rob_elevated: Vec<_> = snapshot
        .coastal_rob
        .iter()
        .filter(|r| {
            (rf_low.is_empty()
                || rf_low == "indonesia"
                || rf_low == "nasional"
                || r.province.to_ascii_lowercase().contains(&rf_low)
                || r.island.to_ascii_lowercase().contains(&rf_low))
                && r.alert_level != AlertLevel::Normal
        })
        .collect();

    let scope_label = if rf.is_empty() {
        "Nasional Indonesia".to_string()
    } else {
        rf.to_string()
    };

    let top = stations.first().copied();
    let headline = if snapshot.is_historical_replay {
        format!(
            "[REPLAY HISTORIS] {} — {} DAS & {} koridor rob berstatus waspada–awas",
            snapshot
                .replay_label
                .as_deref()
                .unwrap_or("Skenario Historis"),
            elevated.len(),
            rob_elevated.len()
        )
    } else if let Some(t) = top.filter(|s| s.alert_level != AlertLevel::Normal) {
        format!(
            "{}: {} stasiun DAS & {} pesisir meningkat · Puncak di {} ({:.2}× Q75)",
            scope_label,
            elevated.len(),
            rob_elevated.len(),
            t.name,
            t.anomaly_ratio
        )
    } else {
        format!(
            "{}: Seluruh {} stasiun sungai pantau berada dalam rentang Normal (< T2)",
            scope_label,
            stations.len()
        )
    };

    let sitrep_summary = match audience {
        Audience::Public => {
            if elevated.is_empty() && rob_elevated.is_empty() {
                format!(
                    "Pantauan saat ini ({scope_label}) relatif tenang. Tetap perhatikan prakiraan hujan lokal 24–72 jam dan kebersihan drainase lingkungan."
                )
            } else {
                let names: Vec<String> = elevated
                    .iter()
                    .take(3)
                    .map(|s| format!("{} ({})", s.river, s.alert_level.label()))
                    .collect();
                format!(
                    "Terdapat {} titik sungai berstatus Waspada–Awas ({}) serta {} zona pasang rob meningkat. Warga di bantaran rendah dan pesisir diimbau memantau pembaruan berkala.",
                    elevated.len(),
                    if names.is_empty() {
                        "pemantauan lokal".to_string()
                    } else {
                        names.join(", ")
                    },
                    rob_elevated.len()
                )
            }
        }
        Audience::Gov => format!(
            "SitRep BPBD/Pemda [{scope_label}]: {} stasiun DAS Waspada–Awas, {} koridor Rob Waspada–Awas, {} nowcast cuaca ekstrem BMKG, {} berita banjir terkini, {} laporan warga PetaBencana 24j. Prioritaskan kesiapan pompa drainase, pintu air hulu, dan posko kecamatan rawan.",
            elevated.len(),
            rob_elevated.len(),
            snapshot.bmkg_nowcasts.len(),
            snapshot.news.len(),
            snapshot.citizen_reports.len()
        ),
        Audience::Academia => format!(
            "Ringkasan Hidrologi [{scope_label}]: {} sel sungai GloFAS v4 dievaluasi terhadap baseline Q75 & ambang Q95; rasio anomali maksimum {:.2}×. Pemisahan mekanisme membedakan limpasan hulu (fluvial), jenuh tanah/hujan lokal (pluvial), dan faktor pasang-amblesan (rob).",
            stations.len(),
            top.map(|s| s.anomaly_ratio).unwrap_or(0.0)
        ),
        Audience::AiAgent => format!(
            "Deterministic Telemetry Context [{scope_label}]: stations_evaluated={}, elevated_fluvial_or_pluvial={}, elevated_rob={}, citizen_reports_24h={}, replay={}.",
            stations.len(),
            elevated.len(),
            rob_elevated.len(),
            snapshot.citizen_reports.len(),
            snapshot.is_historical_replay
        ),
    };

    let fluvial_n = elevated
        .iter()
        .filter(|s| s.mechanism == FloodMechanism::FluvialBasin)
        .count();
    let pluvial_n = elevated
        .iter()
        .filter(|s| s.mechanism == FloodMechanism::UrbanPluvial)
        .count();
    let mechanism_breakdown = format!(
        "Klasifikasi Mekanisme: Kiriman Hulu DAS = {} · Drainase/Hujan Lokal = {} · Pasang Rob Pesisir = {}",
        fluvial_n,
        pluvial_n,
        rob_elevated.len()
    );

    let mut priority_actions = Vec::new();
    for st in elevated.iter().take(4) {
        let msg = match audience {
            Audience::Public => format!("{} — {}", st.name, st.public_advice),
            Audience::Gov => format!("{} — {}", st.name, st.gov_action),
            Audience::Academia => format!("{} — {}", st.name, st.academic_note),
            Audience::AiAgent => format!(
                "{} | alert={} | mech={} | Q={:.1}m3/s | ratio={:.2}x | rain72h={:.1}mm",
                st.id,
                st.alert_level.label(),
                st.mechanism.slug(),
                st.current_discharge_m3s,
                st.anomaly_ratio,
                st.rain_72h_mm
            ),
        };
        priority_actions.push(msg);
    }
    for r in rob_elevated.iter().take(2) {
        priority_actions.push(format!(
            "Rob {}: {} (Indeks {:.2}× · {})",
            r.alert_level.label(),
            r.name,
            r.rob_index,
            r.note
        ));
    }
    if priority_actions.is_empty() {
        if let Some(t) = top {
            priority_actions.push(format!(
                "Debit tertinggi sementara: {} ({:.1} m³/s · {:.2}× Q75 · tren {}).",
                t.name,
                t.current_discharge_m3s,
                t.anomaly_ratio,
                t.trend.label()
            ));
        }
        priority_actions.push(
            "Gunakan Mode Replay (Jabodetabek 2020, Barito 2021, Wulan 2024) untuk simulasi respons ekstrem."
                .to_string(),
        );
    }

    let mut md = format!(
        "# Hidrosentinel SitRep — {scope_label}\n\n**{headline}**\n\n- **Waktu UTC:** `{}`\n- **Perspektif:** `{}`\n- **Mekanisme:** {}\n\n{}\n\n## Poin Prioritas\n",
        snapshot.generated_at_utc,
        audience.label(),
        mechanism_breakdown,
        sitrep_summary
    );
    for act in &priority_actions {
        md.push_str(&format!("- {act}\n"));
    }

    AiBriefing {
        generated_at_utc: snapshot.generated_at_utc.clone(),
        region_filter: scope_label,
        audience,
        headline,
        sitrep_summary,
        mechanism_breakdown,
        priority_actions,
        markdown: md,
    }
}

pub fn execute_ai_query(snapshot: &FloodSnapshot, query_str: &str) -> AiQueryResponse {
    let q = query_str.trim();
    let ql = q.to_ascii_lowercase();

    let wanted_alert: Option<AlertLevel> = if ql.contains("awas") {
        Some(AlertLevel::Awas)
    } else if ql.contains("siaga") {
        Some(AlertLevel::Siaga)
    } else if ql.contains("waspada") {
        Some(AlertLevel::Waspada)
    } else {
        None
    };

    let wanted_mech: Option<FloodMechanism> = if ql.contains("rob") || ql.contains("pasang") || ql.contains("pesisir") {
        Some(FloodMechanism::CoastalRob)
    } else if ql.contains("drainase") || ql.contains("kota") || ql.contains("hujan") || ql.contains("genangan") {
        Some(FloodMechanism::UrbanPluvial)
    } else if ql.contains("hulu") || ql.contains("kiriman") || ql.contains("debit") || ql.contains("sungai") {
        Some(FloodMechanism::FluvialBasin)
    } else {
        None
    };

    // Match coastal rob corridors
    let mut matched_robs: Vec<_> = snapshot
        .coastal_rob
        .iter()
        .filter(|r| {
            let text = format!("{} {} {}", r.name, r.province, r.island).to_ascii_lowercase();
            let token_hit = ql
                .split_whitespace()
                .filter(|w| w.len() >= 3 && !is_stopword(w))
                .any(|w| text.contains(w));
            let alert_ok = wanted_alert.map(|a| r.alert_level >= a).unwrap_or(true);
            (token_hit || wanted_mech == Some(FloodMechanism::CoastalRob)) && alert_ok
        })
        .collect();
    matched_robs.sort_by(|a, b| {
        b.rob_index
            .partial_cmp(&a.rob_index)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Match river stations
    let tokens: Vec<&str> = ql
        .split_whitespace()
        .filter(|w| w.len() >= 3 && !is_stopword(w))
        .collect();

    let mut matched_stations: Vec<&StationTelemetry> = snapshot
        .stations
        .iter()
        .filter(|s| {
            let text = format!(
                "{} {} {} {} {} {}",
                s.id, s.name, s.river, s.ws_code, s.province, s.island
            )
            .to_ascii_lowercase();
            let text_hit = if tokens.is_empty() {
                true
            } else {
                tokens.iter().any(|t| text.contains(t))
            };
            let alert_ok = wanted_alert
                .map(|a| s.alert_level >= a)
                .unwrap_or(true);
            let mech_ok = match wanted_mech {
                Some(FloodMechanism::CoastalRob) => false,
                Some(m) => s.mechanism == m || text_hit,
                None => true,
            };
            text_hit && alert_ok && mech_ok
        })
        .collect();

    // Fallback to highest-risk stations if query asked for "terparah"/"tertinggi"/"top"
    if matched_stations.is_empty()
        && (ql.contains("parah") || ql.contains("tinggi") || ql.contains("top") || ql.contains("kritis"))
    {
        matched_stations = snapshot.stations.iter().collect();
    }

    matched_stations.sort_by(|a, b| {
        b.alert_level
            .rank()
            .cmp(&a.alert_level.rank())
            .then_with(|| {
                b.anomaly_ratio
                    .partial_cmp(&a.anomaly_ratio)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let matched_station_ids: Vec<String> = matched_stations.iter().take(8).map(|s| s.id.clone()).collect();
    let matched_rob_ids: Vec<String> = matched_robs.iter().take(6).map(|r| r.id.clone()).collect();

    let fly_to = if wanted_mech == Some(FloodMechanism::CoastalRob) || (matched_stations.is_empty() && !matched_robs.is_empty()) {
        matched_robs.first().map(|r| AiFlyTarget {
            label: r.name.clone(),
            lon: r.lon,
            lat: r.lat,
            zoom: 9.2,
            station_id: Some(r.id.clone()),
        })
    } else {
        matched_stations.first().map(|s| AiFlyTarget {
            label: s.name.clone(),
            lon: s.lon,
            lat: s.lat,
            zoom: 8.8,
            station_id: Some(s.id.clone()),
        })
    };

    let answer_id = if let Some(top) = matched_stations.first() {
        format!(
            "Ditemukan {} stasiun sungai relevan (dan {} koridor rob). Prioritas utama: {} ({}, debit {:.1} m³/s · {:.2}× Q75 · tren {}). {}",
            matched_stations.len(),
            matched_robs.len(),
            top.name,
            top.alert_level.label(),
            top.current_discharge_m3s,
            top.anomaly_ratio,
            top.trend.label(),
            top.public_advice
        )
    } else if let Some(rob) = matched_robs.first() {
        format!(
            "Ditemukan {} koridor pasang rob relevan. Prioritas: {} (status {}, indeks {:.2}×). {}",
            matched_robs.len(),
            rob.name,
            rob.alert_level.label(),
            rob.rob_index,
            rob.note
        )
    } else {
        format!(
            "Tidak ada stasiun atau koridor rob yang cocok dengan \"{q}\". Coba nama sungai (Ciliwung, Citarum, Barito, Kapuas, Mahakam, Bengawan Solo), provinsi, atau kata kunci seperti \"siaga jawa\" / \"rob semarang\"."
        )
    };

    AiQueryResponse {
        query: q.to_string(),
        answer_id,
        recommended_mechanism: wanted_mech,
        matched_station_ids,
        matched_rob_ids,
        fly_to,
    }
}

fn is_stopword(w: &str) -> bool {
    matches!(
        w,
        "yang"
            | "dan"
            | "atau"
            | "di"
            | "ke"
            | "dari"
            | "untuk"
            | "pada"
            | "saat"
            | "ini"
            | "mana"
            | "apa"
            | "bagaimana"
            | "cek"
            | "lihat"
            | "banjir"
            | "status"
            | "sungai"
            | "siaga"
            | "waspada"
            | "awas"
            | "rob"
    )
}

pub fn build_llms_txt(base_url: &str) -> String {
    format!(
        "# Hidrosentinel Nusantara ({base_url})\n\
        \n\
        > Indonesian 3D Flood, River Discharge (Copernicus GloFAS v4), Urban Pluvial & Coastal Rob Observatory written 100% in Rust on Cloudflare Workers + SQLite Durable Objects.\n\
        \n\
        ## Machine-Readable Endpoints\n\
        - `GET {base_url}/api/openapi.json` — OpenAPI 3.1 specification for AI agents & tool callers\n\
        - `GET {base_url}/api/flood` — Live Indonesian national snapshot (50 river stations, 12 coastal rob corridors, PetaBencana citizen reports)\n\
        - `GET {base_url}/api/station/{{id}}` — 21-day observed + 7-day forecast GloFAS hydrograph & soil moisture for a station\n\
        - `GET {base_url}/api/ai/briefing?region=&audience=public|gov|academia|ai_agent&format=json|md` — Deterministic SitRep briefing\n\
        - `GET {base_url}/api/ai/query?q=...` — Natural-language hydrological query & map fly-to directive\n\
        - `GET {base_url}/api/alerts.rss?region=...` — Regional & national flood warning RSS 2.0 feed (for Telegram/n8n/RSS readers)\n\
        - `POST {base_url}/api/alerts/subscribe` — Register free ntfy.sh push notification topic (no phone number/PII required)\n\
        - `GET {base_url}/api/health` — Durable Object SQLite freshness & upstream health\n\
        \n\
        ## Scientific Caveats\n\
        - Discharge metrics are Copernicus GloFAS v4 5 km grid estimates compared against local Q75/Q95 thresholds, not direct physical staff-gauge telemetry.\n\
        - Replay scenarios (`jabodetabek-2020`, `barito-2021`, `pantura-wulan-2024`) are explicitly flagged `is_historical_replay: true`.\n"
    )
}

pub fn build_openapi_spec(base_url: &str) -> serde_json::Value {
    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Hidrosentinel Nusantara API",
            "version": "0.1.0",
            "description": "All-Rust Indonesian flood, river discharge (GloFAS v4), urban drainage, and coastal rob telemetry API backed by Cloudflare SQLite Durable Objects."
        },
        "servers": [{ "url": base_url }],
        "paths": {
            "/api/flood": {
                "get": {
                    "operationId": "getFloodSnapshot",
                    "summary": "Get live national flood telemetry snapshot",
                    "responses": { "200": { "description": "JSON FloodSnapshot" } }
                }
            },
            "/api/station/{id}": {
                "get": {
                    "operationId": "getStationHydrograph",
                    "summary": "Get 21-day hydrograph and meteorological details for one river station",
                    "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": { "200": { "description": "StationTelemetry JSON" }, "404": { "description": "Station not found" } }
                }
            },
            "/api/ai/briefing": {
                "get": {
                    "operationId": "getAiBriefing",
                    "summary": "Generate deterministic multi-audience SitRep briefing",
                    "parameters": [
                        { "name": "region", "in": "query", "required": false, "schema": { "type": "string" } },
                        { "name": "audience", "in": "query", "required": false, "schema": { "type": "string", "enum": ["public", "gov", "academia", "ai_agent"] } },
                        { "name": "format", "in": "query", "required": false, "schema": { "type": "string", "enum": ["json", "md"] } }
                    ],
                    "responses": { "200": { "description": "AiBriefing JSON or Markdown" } }
                }
            },
            "/api/ai/query": {
                "get": {
                    "operationId": "queryHydrologyNaturalLanguage",
                    "summary": "Query Indonesian rivers, provinces, alerts, or coastal rob in natural language",
                    "parameters": [{ "name": "q", "in": "query", "required": true, "schema": { "type": "string" } }],
                    "responses": { "200": { "description": "AiQueryResponse with matched IDs and map fly_to target" } }
                }
            },
            "/api/alerts.rss": {
                "get": {
                    "operationId": "getFloodAlertsRss",
                    "summary": "RSS 2.0 feed of elevated flood basins, coastal rob, and BMKG nowcasts",
                    "parameters": [{ "name": "region", "in": "query", "required": false, "schema": { "type": "string" } }],
                    "responses": { "200": { "description": "application/rss+xml feed" } }
                }
            },
            "/api/alerts/subscribe": {
                "post": {
                    "operationId": "subscribeNtfyPush",
                    "summary": "Register a free ntfy.sh push topic for automated flood alerts without phone number PII",
                    "responses": { "200": { "description": "Subscription status JSON" } }
                }
            },
            "/api/health": {
                "get": {
                    "operationId": "getHealth",
                    "summary": "Durable Object SQLite storage freshness and upstream health",
                    "responses": { "200": { "description": "Health JSON" } }
                }
            }
        }
    })
}
