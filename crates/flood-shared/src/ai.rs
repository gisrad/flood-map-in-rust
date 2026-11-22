use crate::hydrology::{fmt_dec, fmt_num};
use crate::model::{
    AiBriefing, AiFlyTarget, AiQueryResponse, Audience, CoastalRobTelemetry, FloodMechanism,
    FloodSnapshot, IndicatorLevel, StationTelemetry, METHODOLOGY_VERSION,
};

const DISCLAIMER: &str =
    "Indikasi model, bukan peringatan resmi. Ikuti informasi BPBD, BMKG, dan BBWS setempat.";

fn region_match(rf_low: &str, fields: &[&str]) -> bool {
    rf_low.is_empty()
        || rf_low == "indonesia"
        || rf_low == "nasional"
        || fields.iter().any(|f| f.to_lowercase().contains(rf_low))
}

fn sort_stations(v: &mut [&StationTelemetry]) {
    v.sort_by(|a, b| {
        b.level.cmp(&a.level).then_with(|| {
            b.ratio_q2
                .unwrap_or(0.0)
                .total_cmp(&a.ratio_q2.unwrap_or(0.0))
        })
    });
}

pub fn generate_briefing(
    snapshot: &FloodSnapshot,
    region_filter: &str,
    audience: Audience,
) -> AiBriefing {
    let rf = region_filter.trim();
    let rf_low = rf.to_lowercase();

    let mut stations: Vec<&StationTelemetry> = snapshot
        .stations
        .iter()
        .filter(|s| region_match(&rf_low, &[&s.province, &s.island, &s.river]))
        .collect();
    sort_stations(&mut stations);

    let elevated: Vec<&StationTelemetry> = stations
        .iter()
        .copied()
        .filter(|s| s.level.is_elevated())
        .collect();
    let rob_elevated: Vec<&CoastalRobTelemetry> = snapshot
        .coastal_rob
        .iter()
        .filter(|r| region_match(&rf_low, &[&r.province, &r.island]) && r.level.is_elevated())
        .collect();
    let unknown_n = stations
        .iter()
        .filter(|s| s.level == IndicatorLevel::Unknown)
        .count();

    let scope_label = if rf.is_empty() {
        "Indonesia".to_string()
    } else {
        rf.to_string()
    };

    let headline = if snapshot.is_historical_replay {
        format!(
            "Replay historis: {} — {} stasiun dan {} titik rob berlevel tinggi ke atas",
            snapshot.replay_label.as_deref().unwrap_or("skenario"),
            elevated.len(),
            rob_elevated.len()
        )
    } else if let Some(t) = elevated.first() {
        format!(
            "{scope_label}: {} stasiun dan {} titik rob berlevel tinggi ke atas; tertinggi {} ({})",
            elevated.len(),
            rob_elevated.len(),
            t.name,
            t.level.label_id().to_lowercase()
        )
    } else if !rob_elevated.is_empty() {
        format!(
            "{scope_label}: {} titik rob berlevel tinggi ke atas; tidak ada stasiun sungai di atas Q2",
            rob_elevated.len()
        )
    } else {
        format!(
            "{scope_label}: tidak ada indikasi model di atas ambang pada {} stasiun yang dipantau",
            stations.len()
        )
    };

    let names: Vec<String> = elevated
        .iter()
        .take(3)
        .map(|s| format!("{} ({})", s.name, s.level.label_id().to_lowercase()))
        .collect();

    let sitrep_summary = match audience {
        Audience::Public => {
            if elevated.is_empty() && rob_elevated.is_empty() {
                format!("Saat ini model tidak menunjukkan debit sungai, hujan, atau muka laut di atas ambang untuk {scope_label}. {DISCLAIMER}")
            } else {
                format!(
                    "{} stasiun berlevel tinggi ke atas{}, serta {} titik rob. Warga di bantaran dan pesisir rendah sebaiknya memantau kondisi sekitar. {DISCLAIMER}",
                    elevated.len(),
                    if names.is_empty() { String::new() } else { format!(": {}", names.join(", ")) },
                    rob_elevated.len()
                )
            }
        }
        Audience::Gov => format!(
            "Ringkasan {scope_label}: {} stasiun sungai/drainase dan {} titik rob berlevel tinggi ke atas; {} stasiun belum terkalibrasi; {} nowcast BMKG; {} laporan banjir warga (24 jam). Pertimbangkan verifikasi ke pos duga air dan BPBD sebelum bertindak. {DISCLAIMER}",
            elevated.len(),
            rob_elevated.len(),
            unknown_n,
            snapshot.bmkg_nowcasts.len(),
            snapshot.citizen_reports.len()
        ),
        Audience::Academia => format!(
            "Metodologi {METHODOLOGY_VERSION}: debit simulasi GloFAS v4 dibandingkan dengan Q2/Q5/Q20 (Gumbel, L-moment, maksimum tahunan 1997–2024) pada sel terkalibrasi; drainase memakai kategori hujan BMKG; rob memakai persentil muka laut model. {} stasiun dievaluasi, {} belum terkalibrasi; rasio Q/Q2 maksimum {}.",
            stations.len(),
            unknown_n,
            snapshot.kpis.max_ratio_q2.map(|r| fmt_dec(r, 2)).unwrap_or_else(|| "–".into())
        ),
        Audience::AiAgent => format!(
            "context scope={scope_label} stations={} elevated={} rob_elevated={} unknown={} citizen_flood_reports_24h={} replay={} methodology={METHODOLOGY_VERSION} disclaimer=\"model indication, not an official warning\"",
            stations.len(),
            elevated.len(),
            rob_elevated.len(),
            unknown_n,
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
        "Kiriman hulu {fluvial_n} · Hujan lokal {pluvial_n} · Rob {}",
        rob_elevated.len()
    );

    let mut priority_actions = Vec::new();
    for st in elevated.iter().take(4) {
        priority_actions.push(match audience {
            Audience::Public => format!("{} — {}", st.name, st.public_advice),
            Audience::Gov => format!("{} — {}", st.name, st.gov_action),
            Audience::Academia => format!("{} — {}", st.name, st.academic_note),
            Audience::AiAgent => format!(
                "{} | level={} | mechanism={} | q={} | ratio_q2={} | rain24={}",
                st.id,
                st.level.slug(),
                st.mechanism.slug(),
                st.current_discharge_m3s,
                st.ratio_q2
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "null".into()),
                st.rain_24h_mm
            ),
        });
    }
    for r in rob_elevated.iter().take(2) {
        priority_actions.push(format!("Rob {} — {}", r.name, r.level_reason));
    }
    if priority_actions.is_empty() {
        if let Some(t) = stations.iter().find(|s| s.ratio_q2.is_some()) {
            priority_actions.push(format!(
                "Rasio Q/Q2 tertinggi: {} ({} m³/s, {}× Q2, tren {}).",
                t.name,
                fmt_num(t.current_discharge_m3s),
                fmt_dec(t.ratio_q2.unwrap_or(0.0), 2),
                t.trend.label().to_lowercase()
            ));
        }
    }

    let mut md = format!(
        "# Ringkasan banjir — {scope_label}\n\n**{headline}**\n\n- Waktu data (UTC): `{}`\n- Pembaca: {}\n- Mekanisme: {}\n\n{}\n\n## Poin utama\n",
        snapshot.generated_at_utc,
        audience.label(),
        mechanism_breakdown,
        sitrep_summary
    );
    for act in &priority_actions {
        md.push_str(&format!("- {act}\n"));
    }
    md.push_str(&format!("\n_{DISCLAIMER}_\n"));

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

fn wanted_level(ql: &str) -> Option<IndicatorLevel> {
    if ql.contains("ekstrem") || ql.contains("extreme") {
        Some(IndicatorLevel::Extreme)
    } else if ql.contains("sangat tinggi") || ql.contains("very_high") {
        Some(IndicatorLevel::VeryHigh)
    } else if ql.contains("tinggi") || ql.contains("high") {
        Some(IndicatorLevel::High)
    } else {
        None
    }
}

pub fn execute_ai_query(snapshot: &FloodSnapshot, query_str: &str) -> AiQueryResponse {
    let q = query_str.trim();
    let ql = q.to_lowercase();
    let level_req = wanted_level(&ql);

    let wanted_mech = if ql.contains("rob") || ql.contains("pasang") || ql.contains("pesisir") {
        Some(FloodMechanism::CoastalRob)
    } else if ql.contains("drainase")
        || ql.contains("kota")
        || ql.contains("hujan")
        || ql.contains("genangan")
    {
        Some(FloodMechanism::UrbanPluvial)
    } else if ql.contains("hulu") || ql.contains("kiriman") || ql.contains("debit") {
        Some(FloodMechanism::FluvialBasin)
    } else {
        None
    };

    let tokens: Vec<&str> = ql
        .split_whitespace()
        .filter(|w| w.chars().count() >= 3 && !is_stopword(w))
        .collect();

    let mut matched_robs: Vec<&CoastalRobTelemetry> = snapshot
        .coastal_rob
        .iter()
        .filter(|r| {
            let text = format!("{} {} {} {}", r.id, r.name, r.province, r.island).to_lowercase();
            let token_hit = tokens.iter().any(|t| text.contains(t));
            let level_ok = level_req.is_none_or(|l| r.level >= l);
            (token_hit || wanted_mech == Some(FloodMechanism::CoastalRob)) && level_ok
        })
        .collect();
    matched_robs.sort_by_key(|r| std::cmp::Reverse(r.level));

    let mut matched_stations: Vec<&StationTelemetry> = snapshot
        .stations
        .iter()
        .filter(|s| {
            let text = format!(
                "{} {} {} {} {} {}",
                s.id, s.name, s.river, s.ws_code, s.province, s.island
            )
            .to_lowercase();
            let text_hit = tokens.is_empty() || tokens.iter().any(|t| text.contains(t));
            let level_ok = level_req.is_none_or(|l| s.level >= l);
            let mech_ok = match wanted_mech {
                Some(FloodMechanism::CoastalRob) => false,
                Some(m) => s.mechanism == m || (!tokens.is_empty() && text_hit),
                None => true,
            };
            text_hit && level_ok && mech_ok
        })
        .collect();

    if matched_stations.is_empty()
        && (ql.contains("parah") || ql.contains("tertinggi") || ql.contains("kritis"))
    {
        matched_stations = snapshot.stations.iter().collect();
    }
    sort_stations(&mut matched_stations);

    let matched_station_ids: Vec<String> = matched_stations
        .iter()
        .take(8)
        .map(|s| s.id.clone())
        .collect();
    let matched_rob_ids: Vec<String> = matched_robs.iter().take(6).map(|r| r.id.clone()).collect();

    let rob_first = wanted_mech == Some(FloodMechanism::CoastalRob)
        || (matched_stations.is_empty() && !matched_robs.is_empty());
    let fly_to = if rob_first {
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

    let answer_id = if let (false, Some(top)) = (rob_first, matched_stations.first()) {
        format!(
            "{} stasiun cocok ({} titik rob). Teratas: {} — level {}. {}",
            matched_stations.len(),
            matched_robs.len(),
            top.name,
            top.level.label_id().to_lowercase(),
            top.level_reason
        )
    } else if let Some(r) = matched_robs.first() {
        format!(
            "{} titik rob cocok. Teratas: {} — level {}. {}",
            matched_robs.len(),
            r.name,
            r.level.label_id().to_lowercase(),
            r.level_reason
        )
    } else {
        format!(
            "Tidak ada stasiun atau titik rob yang cocok dengan \"{q}\". Coba nama sungai, provinsi, atau kata seperti \"tinggi jawa\" dan \"rob semarang\"."
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
            | "level"
            | "tinggi"
            | "sangat"
            | "ekstrem"
            | "rob"
            | "high"
            | "very_high"
            | "extreme"
    )
}

pub fn build_llms_txt(base_url: &str) -> String {
    format!(
        "# Hidrosentinel Nusantara — {base_url}\n\
\n\
> Indikasi model banjir sungai, genangan hujan lokal, dan rob untuk 50 stasiun sungai dan 12 titik pesisir di Indonesia. Indikasi model, BUKAN peringatan resmi; peringatan resmi berasal dari BNPB/BPBD, BMKG, dan BBWS.\n\
\n\
## Endpoint\n\
- `GET {base_url}/api/flood` — snapshot nasional (JSON, schema_version 2)\n\
- `GET {base_url}/api/stations.geojson` — titik stasiun, rob, dan laporan warga (GeoJSON, CORS terbuka)\n\
- `GET {base_url}/api/station/{{id}}` — satu stasiun atau titik rob\n\
- `GET {base_url}/api/ai/briefing?region=&audience=public|gov|academia|ai_agent&format=json|md`\n\
- `GET {base_url}/api/ai/query?q=...`\n\
- `GET {base_url}/api/alerts.rss?region=...` — RSS level tinggi ke atas\n\
- `POST {base_url}/api/alerts/subscribe` — notifikasi ntfy.sh (topik tanpa data pribadi)\n\
- `GET {base_url}/api/health` — kesegaran tiap sumber data\n\
- `GET {base_url}/api/openapi.json`\n\
\n\
## Level indikasi (metodologi {METHODOLOGY_VERSION})\n\
- `level`: unknown (belum terkalibrasi) | normal | high (tinggi) | very_high (sangat tinggi) | extreme (ekstrem).\n\
- Sungai: debit harian simulasi GloFAS v4 pada sel terkalibrasi; high ≥ Q2, very_high ≥ Q5, extreme ≥ Q20. Q2/Q5/Q20 = periode ulang 2/5/20 tahun dari distribusi Gumbel (L-moment) atas maksimum tahunan reanalisis GloFAS v4 1997–2024 pada sel yang sama.\n\
- Drainase: hujan model 24 jam terakhir atau 24 jam ke depan; high ≥ 50 mm (lebat, BMKG), very_high ≥ 100 mm (sangat lebat), extreme > 150 mm (ekstrem). Laporan banjir warga PetaBencana ≤ 10 km dalam 24 jam menaikkan minimal ke high.\n\
- Rob: puncak muka laut model 48 jam (Open-Meteo Marine) terhadap klimatologi puncak 48 jam setempat sejak 2022; high ≥ p99, very_high ≥ maksimum periode kalibrasi.\n\
- `level` stasiun = maksimum level sungai dan drainase; `mechanism` = komponen penentu.\n\
\n\
## Keterbatasan\n\
- Debit adalah simulasi model global 5 km, bukan pengukuran pos duga air. Sel model dapat jatuh di sungai lain; stasiun yang selnya tidak meyakinkan berlevel unknown.\n\
- Muka laut model tidak memuat amblesan tanah, tanggul, atau limpasan lokal.\n\
- Replay historis (`is_historical_replay: true`) memakai reanalisis GloFAS v4 dan hujan ERA5, bukan pengukuran.\n\
\n\
## Atribusi\n\
GloFAS (Copernicus Emergency Management Service) dan ERA5 (Copernicus Climate Change Service) melalui Open-Meteo (CC BY 4.0); Copernicus Marine Service melalui Open-Meteo; PetaBencana.id; BMKG.\n"
    )
}

pub fn build_openapi_spec(base_url: &str) -> serde_json::Value {
    let level_enum = serde_json::json!(["unknown", "normal", "high", "very_high", "extreme"]);
    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Hidrosentinel Nusantara API",
            "version": format!("2.0.0+{METHODOLOGY_VERSION}"),
            "description": "Indikasi model banjir sungai (GloFAS v4), genangan hujan lokal, dan rob. Indikasi model, bukan peringatan resmi."
        },
        "servers": [{ "url": base_url }],
        "components": {
            "schemas": {
                "IndicatorLevel": { "type": "string", "enum": level_enum },
                "FloodMechanism": { "type": "string", "enum": ["fluvial_basin", "urban_pluvial", "coastal_rob"] }
            }
        },
        "paths": {
            "/api/flood": { "get": {
                "operationId": "getFloodSnapshot",
                "summary": "Snapshot nasional (schema_version 2)",
                "responses": { "200": { "description": "FloodSnapshot JSON" } }
            }},
            "/api/stations.geojson": { "get": {
                "operationId": "getStationsGeoJson",
                "summary": "FeatureCollection titik: kind river|rob|report, level, level_label, mechanism, metrik kunci",
                "responses": { "200": { "description": "application/geo+json" } }
            }},
            "/api/station/{id}": { "get": {
                "operationId": "getStation",
                "summary": "Satu stasiun sungai (StationTelemetry) atau titik rob (CoastalRobTelemetry)",
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
                "responses": { "200": { "description": "JSON" }, "404": { "description": "Tidak ditemukan" } }
            }},
            "/api/ai/briefing": { "get": {
                "operationId": "getBriefing",
                "summary": "Ringkasan deterministik per pembaca",
                "parameters": [
                    { "name": "region", "in": "query", "required": false, "schema": { "type": "string" } },
                    { "name": "audience", "in": "query", "required": false, "schema": { "type": "string", "enum": ["public", "gov", "academia", "ai_agent"] } },
                    { "name": "format", "in": "query", "required": false, "schema": { "type": "string", "enum": ["json", "md"] } }
                ],
                "responses": { "200": { "description": "AiBriefing JSON atau Markdown" } }
            }},
            "/api/ai/query": { "get": {
                "operationId": "query",
                "summary": "Pencarian bahasa alami atas stasiun dan titik rob",
                "parameters": [{ "name": "q", "in": "query", "required": true, "schema": { "type": "string" } }],
                "responses": { "200": { "description": "AiQueryResponse" } }
            }},
            "/api/alerts.rss": { "get": {
                "operationId": "getAlertsRss",
                "summary": "RSS 2.0 stasiun dan titik rob berlevel tinggi ke atas, plus nowcast BMKG",
                "parameters": [{ "name": "region", "in": "query", "required": false, "schema": { "type": "string" } }],
                "responses": { "200": { "description": "application/rss+xml" } }
            }},
            "/api/alerts/subscribe": { "post": {
                "operationId": "subscribeNtfy",
                "summary": "Daftarkan topik ntfy.sh",
                "requestBody": { "required": true, "content": { "application/json": { "schema": {
                    "type": "object",
                    "required": ["topic"],
                    "properties": {
                        "topic": { "type": "string", "pattern": "^[A-Za-z0-9_-]{8,64}$" },
                        "region": { "type": "string", "maxLength": 48 },
                        "min_level": { "$ref": "#/components/schemas/IndicatorLevel" },
                        "send_test": { "type": "boolean" }
                    }
                }}}},
                "responses": {
                    "200": { "description": "Langganan tersimpan" },
                    "400": { "description": "Topik tidak valid" },
                    "429": { "description": "Batas permintaan per IP terlampaui" },
                    "503": { "description": "Kuota langganan penuh" }
                }
            }},
            "/api/health": { "get": {
                "operationId": "getHealth",
                "summary": "Kesegaran snapshot dan waktu ambil tiap sumber",
                "responses": { "200": { "description": "JSON" } }
            }}
        }
    })
}
