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

/// Wilayah dari parameter pengguna sebelum dicetak ke JSON/Markdown: hanya huruf, angka, spasi,
/// titik, dan tanda hubung; paling banyak 48 karakter.
pub fn sanitize_region(raw: &str) -> String {
    let kept: String = raw
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '.'))
        .take(48)
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn generate_briefing(
    snapshot: &FloodSnapshot,
    region_filter: &str,
    audience: Audience,
) -> AiBriefing {
    let rf = sanitize_region(region_filter);
    let rf_low = rf.to_lowercase();
    let replay = snapshot.is_historical_replay;

    let mut stations: Vec<&StationTelemetry> = snapshot
        .stations
        .iter()
        .filter(|s| region_match(&rf_low, &[&s.province, &s.island, &s.river]))
        .collect();
    sort_stations(&mut stations);
    let robs: Vec<&CoastalRobTelemetry> = snapshot
        .coastal_rob
        .iter()
        .filter(|r| region_match(&rf_low, &[&r.province, &r.island]))
        .collect();

    let elevated: Vec<&StationTelemetry> = stations
        .iter()
        .copied()
        .filter(|s| s.level.is_elevated())
        .collect();
    let rob_elevated: Vec<&CoastalRobTelemetry> = robs
        .iter()
        .copied()
        .filter(|r| r.level.is_elevated())
        .collect();
    let unknown_n = stations
        .iter()
        .filter(|s| s.level == IndicatorLevel::Unknown)
        .count();
    // Stasiun tanpa ambang sungai; levelnya bisa tetap naik karena hujan lokal.
    let uncal_n = stations.iter().filter(|s| s.thresholds.is_none()).count();
    let rob_unknown_n = robs
        .iter()
        .filter(|r| r.level == IndicatorLevel::Unknown)
        .count();
    let assessed_n = stations.len() - unknown_n;
    let rob_assessed_n = robs.len() - rob_unknown_n;
    let no_points = stations.is_empty() && robs.is_empty();

    let scope_label = if rf.is_empty() {
        "Indonesia".to_string()
    } else {
        rf.clone()
    };
    // Titik yang tidak dapat dinilai selalu disebut, agar "tidak ada indikasi" tidak terbaca
    // sebagai "aman".
    let coverage = {
        let mut parts = Vec::new();
        if uncal_n > 0 {
            parts.push(format!(
                "{uncal_n} dari {} stasiun belum terkalibrasi untuk debit sungai",
                stations.len()
            ));
        }
        if rob_unknown_n > 0 {
            parts.push(format!(
                "{rob_unknown_n} dari {} titik rob belum dapat dinilai",
                robs.len()
            ));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("; {}", parts.join(", "))
        }
    };

    let headline = if no_points {
        format!("{scope_label}: tidak ada titik pantau Hidrosentinel di wilayah ini, sehingga model tidak dapat menilai")
    } else if replay {
        format!(
            "Replay historis: {} — {} stasiun dan {} titik rob berlevel tinggi ke atas{coverage}",
            snapshot.replay_label.as_deref().unwrap_or("skenario"),
            elevated.len(),
            rob_elevated.len()
        )
    } else if let Some(t) = elevated.first() {
        format!(
            "{scope_label}: {} stasiun dan {} titik rob berlevel tinggi ke atas; tertinggi {} ({}){coverage}",
            elevated.len(),
            rob_elevated.len(),
            t.name,
            t.level.label_id().to_lowercase()
        )
    } else if !rob_elevated.is_empty() {
        format!(
            "{scope_label}: {} titik rob berlevel tinggi ke atas; tidak ada stasiun berlevel tinggi{coverage}",
            rob_elevated.len()
        )
    } else if assessed_n + rob_assessed_n == 0 {
        format!(
            "{scope_label}: {} titik pantau belum dapat dinilai model{coverage}",
            stations.len() + robs.len()
        )
    } else {
        format!(
            "{scope_label}: tidak ada indikasi model di atas ambang pada {assessed_n} stasiun dan {rob_assessed_n} titik rob yang dapat dinilai{coverage}"
        )
    };

    let names: Vec<String> = elevated
        .iter()
        .take(3)
        .map(|s| format!("{} ({})", s.name, s.level.label_id().to_lowercase()))
        .collect();

    let sitrep_summary = match audience {
        _ if no_points && audience != Audience::AiAgent => format!(
            "Tidak ada titik pantau Hidrosentinel di {scope_label}, sehingga model tidak dapat menilai wilayah ini. {DISCLAIMER}"
        ),
        Audience::Public => {
            if elevated.is_empty() && rob_elevated.is_empty() {
                let lead = if replay {
                    "Pada tanggal replay ini, model"
                } else {
                    "Model"
                };
                format!(
                    "{lead} tidak menunjukkan debit sungai, hujan, atau muka laut di atas ambang pada {assessed_n} stasiun dan {rob_assessed_n} titik rob yang dapat dinilai di {scope_label}{coverage}. Ini bukan jaminan aman: banjir lokal dapat terjadi di luar titik pantau. {DISCLAIMER}"
                )
            } else {
                format!(
                    "{} stasiun berlevel tinggi ke atas{}, serta {} titik rob{coverage}. Warga di bantaran dan pesisir rendah sebaiknya memantau kondisi sekitar. {DISCLAIMER}",
                    elevated.len(),
                    if names.is_empty() { String::new() } else { format!(": {}", names.join(", ")) },
                    rob_elevated.len()
                )
            }
        }
        Audience::Gov => {
            // Nowcast BMKG dan laporan warga tidak dibakukan untuk replay.
            let live = if replay {
                String::new()
            } else {
                format!(
                    "; {} nowcast BMKG; {} laporan banjir warga (24 jam)",
                    snapshot.bmkg_nowcasts.len(),
                    snapshot.citizen_reports.len()
                )
            };
            format!(
                "Ringkasan {scope_label}: {} stasiun sungai/drainase dan {} titik rob berlevel tinggi ke atas; {} stasiun belum terkalibrasi{live}. Pertimbangkan verifikasi ke pos duga air dan BPBD sebelum bertindak. {DISCLAIMER}",
                elevated.len(),
                rob_elevated.len(),
                uncal_n
            )
        }
        Audience::Academia => format!(
            "Metodologi {METHODOLOGY_VERSION}: debit simulasi GloFAS v4 dibandingkan dengan Q2/Q5/Q20 (Gumbel, L-moment, maksimum tahunan 1997–2024) pada sel terkalibrasi; drainase memakai kategori hujan BMKG; rob memakai persentil muka laut model. {} stasiun dievaluasi, {} belum terkalibrasi; rasio Q/Q2 maksimum {}.",
            stations.len(),
            uncal_n,
            snapshot.kpis.max_ratio_q2.map(|r| fmt_dec(r, 2)).unwrap_or_else(|| "–".into())
        ),
        Audience::AiAgent => format!(
            "context scope={scope_label} stations={} assessed={assessed_n} elevated={} rob_points={} rob_elevated={} unknown={} uncalibrated={uncal_n} citizen_flood_reports_24h={} replay={replay} methodology={METHODOLOGY_VERSION} disclaimer=\"model indication, not an official warning; absence of indication is not a safety statement\"",
            stations.len(),
            elevated.len(),
            robs.len(),
            rob_elevated.len(),
            unknown_n,
            if replay {
                "n/a".to_string()
            } else {
                snapshot.citizen_reports.len().to_string()
            }
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

/// Kata utuh huruf/angka dalam huruf kecil.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

fn wanted_level(qwords: &[String]) -> Option<IndicatorLevel> {
    let has = |w: &str| qwords.iter().any(|x| x == w);
    if has("ekstrem") || has("extreme") {
        Some(IndicatorLevel::Extreme)
    } else if (has("sangat") && has("tinggi")) || has("very") {
        Some(IndicatorLevel::VeryHigh)
    } else if has("tinggi") || has("high") {
        Some(IndicatorLevel::High)
    } else {
        None
    }
}

/// Jumlah token kueri yang cocok dengan kata utuh di `fields`; token ≥ 5 huruf boleh cocok
/// sebagai awalan kata. Substring tidak dihitung, agar kata umum seperti "aman" tidak
/// mencocokkan nama sungai yang kebetulan memuatnya.
fn token_hits(tokens: &[String], fields: &[&str]) -> usize {
    let hay: Vec<String> = fields.iter().flat_map(|f| words(f)).collect();
    tokens
        .iter()
        .filter(|t| {
            hay.iter()
                .any(|w| w == *t || (t.chars().count() >= 5 && w.starts_with(t.as_str())))
        })
        .count()
}

pub fn execute_ai_query(snapshot: &FloodSnapshot, query_str: &str) -> AiQueryResponse {
    let q = query_str.trim();
    let qwords: Vec<String> = words(q).collect();
    let has = |w: &str| qwords.iter().any(|x| x == w);
    let level_req = wanted_level(&qwords);

    let wanted_mech = if has("rob") || has("pasang") || has("pesisir") {
        Some(FloodMechanism::CoastalRob)
    } else if has("drainase") || has("kota") || has("hujan") || has("genangan") {
        Some(FloodMechanism::UrbanPluvial)
    } else if has("hulu") || has("kiriman") || has("debit") {
        Some(FloodMechanism::FluvialBasin)
    } else {
        None
    };

    let tokens: Vec<String> = qwords
        .iter()
        .filter(|w| w.chars().count() >= 3 && !is_stopword(w))
        .cloned()
        .collect();

    // Hanya titik dengan skor tertinggi yang dipakai, agar satu kata yang kebetulan cocok tidak
    // mengalahkan nama tempat yang cocok lebih lengkap.
    let rob_scored: Vec<(usize, &CoastalRobTelemetry)> = snapshot
        .coastal_rob
        .iter()
        .map(|r| {
            (
                token_hits(&tokens, &[&r.id, &r.name, &r.province, &r.island]),
                r,
            )
        })
        .collect();
    let station_scored: Vec<(usize, &StationTelemetry)> = snapshot
        .stations
        .iter()
        .map(|s| {
            let fields = [
                s.id.as_str(),
                &s.name,
                &s.river,
                &s.ws_code,
                &s.province,
                &s.island,
            ];
            (token_hits(&tokens, &fields), s)
        })
        .collect();
    let best = rob_scored
        .iter()
        .map(|x| x.0)
        .chain(station_scored.iter().map(|x| x.0))
        .max()
        .unwrap_or(0);
    let text_ok = |score: usize| tokens.is_empty() || (best > 0 && score == best);

    let mut matched_robs: Vec<&CoastalRobTelemetry> = rob_scored
        .iter()
        .filter(|(score, r)| {
            let hit = if tokens.is_empty() {
                wanted_mech == Some(FloodMechanism::CoastalRob)
            } else {
                text_ok(*score)
            };
            hit && level_req.is_none_or(|l| r.level >= l)
        })
        .map(|(_, r)| *r)
        .collect();
    matched_robs.sort_by_key(|r| std::cmp::Reverse(r.level));

    let mut matched_stations: Vec<&StationTelemetry> = station_scored
        .iter()
        .filter(|(score, s)| {
            let mech_ok = match wanted_mech {
                Some(FloodMechanism::CoastalRob) => false,
                Some(m) => s.mechanism == m || !tokens.is_empty(),
                None => true,
            };
            text_ok(*score) && level_req.is_none_or(|l| s.level >= l) && mech_ok
        })
        .map(|(_, s)| *s)
        .collect();

    if matched_stations.is_empty()
        && matched_robs.is_empty()
        && (has("parah") || has("tertinggi") || has("kritis"))
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

    // Model tidak dapat menyatakan suatu tempat aman; jawab pertanyaan "aman?" dengan level saja.
    let safety = if has("aman") || has("safe") {
        "Model tidak dapat menyatakan suatu wilayah aman. "
    } else {
        ""
    };
    let answer_id = if let (false, Some(top)) = (rob_first, matched_stations.first()) {
        format!(
            "{safety}{} stasiun cocok ({} titik rob). Teratas: {} — level {}. {} {DISCLAIMER}",
            matched_stations.len(),
            matched_robs.len(),
            top.name,
            top.level.label_id().to_lowercase(),
            top.level_reason
        )
    } else if let Some(r) = matched_robs.first() {
        format!(
            "{safety}{} titik rob cocok. Teratas: {} — level {}. {} {DISCLAIMER}",
            matched_robs.len(),
            r.name,
            r.level.label_id().to_lowercase(),
            r.level_reason
        )
    } else {
        format!(
            "{safety}Tidak ada stasiun atau titik rob yang cocok dengan \"{q}\". Coba nama sungai, provinsi, atau kata seperti \"tinggi jawa\" dan \"rob semarang\"."
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
            | "itu"
            | "mana"
            | "apa"
            | "apakah"
            | "bagaimana"
            | "gimana"
            | "berapa"
            | "kah"
            | "ada"
            | "tidak"
            | "akan"
            | "sudah"
            | "masih"
            | "cek"
            | "lihat"
            | "info"
            | "informasi"
            | "kondisi"
            | "keadaan"
            | "hari"
            | "sekarang"
            | "kini"
            | "terkini"
            | "besok"
            | "nanti"
            | "aman"
            | "bahaya"
            | "potensi"
            | "risiko"
            | "dekat"
            | "sekitar"
            | "saya"
            | "kami"
            | "kita"
            | "rumah"
            | "wilayah"
            | "daerah"
            | "kota"
            | "kabupaten"
            | "kab"
            | "provinsi"
            | "prov"
            | "kecamatan"
            | "desa"
            | "banjir"
            | "air"
            | "status"
            | "sungai"
            | "level"
            | "tinggi"
            | "sangat"
            | "ekstrem"
            | "parah"
            | "tertinggi"
            | "kritis"
            | "rob"
            | "pasang"
            | "pesisir"
            | "laut"
            | "muka"
            | "hujan"
            | "lokal"
            | "genangan"
            | "drainase"
            | "hulu"
            | "kiriman"
            | "debit"
            | "naik"
            | "surut"
            | "high"
            | "very"
            | "extreme"
            | "the"
            | "flood"
            | "river"
            | "safe"
            | "near"
            | "now"
            | "today"
            | "how"
            | "what"
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
