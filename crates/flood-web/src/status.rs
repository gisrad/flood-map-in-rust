//! Kalimat status nasional dari KPI snapshot.

use crate::format::{fmt_datetime_long, parse_utc_ms};
use flood_shared::model::{FloodSnapshot, LevelCounts};

fn counts_phrase(c: &LevelCounts) -> Option<String> {
    let mut parts = Vec::new();
    if c.extreme > 0 {
        parts.push(format!("{} Ekstrem", c.extreme));
    }
    if c.very_high > 0 {
        parts.push(format!("{} Sangat tinggi", c.very_high));
    }
    if c.high > 0 {
        parts.push(format!("{} Tinggi", c.high));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Isi kalimat status tanpa stempel waktu.
pub fn status_body(snap: &FloodSnapshot) -> String {
    let k = &snap.kpis;
    let mut segs = Vec::new();
    segs.push(match counts_phrase(&k.river) {
        Some(p) => format!("Sungai: {p}"),
        None => "Tidak ada sungai di atas ambang".to_string(),
    });
    if let Some(p) = counts_phrase(&k.pluvial) {
        segs.push(format!("Hujan lokal: {p}"));
    }
    if let Some(p) = counts_phrase(&k.rob) {
        segs.push(format!("Rob: {p}"));
    }
    segs.push(format!("{} peringatan cuaca BMKG", k.bmkg_nowcasts));
    segs.push(format!(
        "{} laporan banjir warga",
        k.citizen_flood_reports_24h
    ));
    segs.join(" · ")
}

/// Stempel waktu snapshot dalam WIB.
pub fn status_stamp(snap: &FloodSnapshot) -> String {
    parse_utc_ms(&snap.generated_at_utc)
        .map(|ms| fmt_datetime_long(ms, 7))
        .unwrap_or_else(|| snap.generated_at_utc.clone())
}
