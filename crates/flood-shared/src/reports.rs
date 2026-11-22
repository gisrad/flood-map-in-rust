//! Laporan warga PetaBencana.id (`/reports?disaster=flood&geoformat=geojson`).
//! Lisensi data: lihat https://info.petabencana.id/data (atribusi PetaBencana.id).

use crate::model::CitizenReport;
use serde_json::Value;

pub const PETABENCANA_URL: &str =
    "https://data.petabencana.id/reports?disaster=flood&geoformat=geojson&timeperiod=86400";

/// Ambil laporan banjir dari GeoJSON PetaBencana. Jenis bencana lain (kebakaran, gempa,
/// angin, kabut asap, gunung api) ditolak walau parameter `disaster=flood` tidak dihormati.
pub fn parse_petabencana_geojson(root: &Value) -> Result<Vec<CitizenReport>, String> {
    let features = root
        .pointer("/result/features")
        .and_then(Value::as_array)
        .ok_or_else(|| "struktur GeoJSON PetaBencana tidak dikenal".to_string())?;

    let mut out = Vec::new();
    for f in features {
        let props = match f.get("properties") {
            Some(p) => p,
            None => continue,
        };
        let disaster_type = props
            .get("disaster_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if disaster_type != "flood" {
            continue;
        }
        let Some(coords) = f.pointer("/geometry/coordinates").and_then(Value::as_array) else {
            continue;
        };
        let (Some(lon), Some(lat)) = (
            coords.first().and_then(Value::as_f64),
            coords.get(1).and_then(Value::as_f64),
        ) else {
            continue;
        };
        if !(-11.5..=6.5).contains(&lat) || !(94.5..=141.5).contains(&lon) {
            continue;
        }
        let str_of = |k: &str| {
            props
                .get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
        };
        let pkey = str_of("pkey");
        if pkey.is_empty() {
            continue;
        }
        let depth_cm = props
            .pointer("/report_data/flood_depth")
            .and_then(Value::as_f64)
            .filter(|d| d.is_finite() && *d >= 0.0)
            .map(|d| d.round() as u32);
        let url = [str_of("url"), str_of("image_url")]
            .into_iter()
            .find(|u| u.starts_with("https://"))
            .map(str::to_string);
        let city = props
            .pointer("/tags/city")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let title = match str_of("title") {
            "" if city.is_empty() => "Laporan banjir warga".to_string(),
            "" => format!("Laporan banjir warga · {}", title_case(city)),
            t => t.to_string(),
        };
        out.push(CitizenReport {
            pkey: pkey.to_string(),
            created_at: str_of("created_at").to_string(),
            source: if str_of("source").is_empty() {
                "petabencana".to_string()
            } else {
                str_of("source").to_string()
            },
            status: str_of("status").to_string(),
            depth_cm,
            title,
            text: str_of("text").chars().take(280).collect(),
            lat,
            lon,
            disaster_type: disaster_type.to_string(),
            url,
        });
    }
    Ok(out)
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let lower = w.to_lowercase();
            let mut c = lower.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
