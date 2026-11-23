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

/// Peringatan data terakhir: waktu snapshot bukan jaminan setiap sumber masih mutakhir.
pub fn freshness_notice(
    snapshot: Option<&FloodSnapshot>,
    failure: Option<&str>,
    online: bool,
    now: f64,
) -> Option<(&'static str, String)> {
    if snapshot.is_some_and(|s| s.is_historical_replay) {
        return None;
    }
    let (title, mut detail) = if !online {
        (
            "Tidak ada koneksi internet.",
            "Pembaruan data tidak dapat diperiksa.".to_string(),
        )
    } else if let Some(error) = failure {
        (
            if snapshot.is_some() {
                "Pembaruan data gagal."
            } else {
                "Data belum dapat dimuat."
            },
            error.to_string(),
        )
    } else if let Some(snap) = snapshot {
        let age = parse_utc_ms(&snap.generated_at_utc).map(|ms| now - ms);
        if age.is_none() {
            (
                "Waktu data tidak diketahui.",
                "Waktu snapshot dari server tidak dapat dikenali.".to_string(),
            )
        } else if age.is_some_and(|ms| ms >= 30.0 * 60_000.0) {
            (
                "Data belum diperbarui.",
                "Snapshot sudah berumur 30 menit atau lebih.".to_string(),
            )
        } else {
            let u = &snap.upstream;
            let mut failed = String::new();
            for (ok, name) in [
                (u.glofas_ok, "GloFAS"),
                (u.weather_ok, "hujan"),
                (u.marine_ok, "laut"),
                (u.petabencana_ok, "laporan warga"),
                (u.bmkg_nowcast_ok, "BMKG"),
                (u.news_ok, "berita"),
            ] {
                if !ok {
                    if !failed.is_empty() {
                        failed.push_str(", ");
                    }
                    failed.push_str(name);
                }
            }
            if failed.is_empty() {
                return None;
            }
            (
                "Sebagian sumber belum mutakhir.",
                format!("Gagal diperbarui atau kedaluwarsa: {failed}. Lihat Tentang data."),
            )
        }
    } else {
        return None;
    };
    if let Some(snap) = snapshot {
        detail.push_str(" Data terakhir per ");
        detail.push_str(&status_stamp(snap));
        detail.push('.');
    }
    Some((title, detail))
}
