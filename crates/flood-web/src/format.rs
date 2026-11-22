//! Format waktu (WIB/WITA/WIT) dan angka (EYD: titik ribuan, koma desimal).

use js_sys::Date;
use wasm_bindgen::JsValue;

const DAYS: [&str; 7] = [
    "Minggu", "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu",
];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
];

pub const MS_PER_HOUR: f64 = 3_600_000.0;
pub const MS_PER_DAY: f64 = 86_400_000.0;

pub fn now_ms() -> f64 {
    Date::now()
}

/// Mengurai ISO 8601 sebagai UTC. String tanpa zona waktu dianggap UTC,
/// tanggal saja (`YYYY-MM-DD`) dianggap 00.00 UTC.
pub fn parse_utc_ms(iso: &str) -> Option<f64> {
    let s = iso.trim();
    if s.len() < 10 {
        return None;
    }
    let has_zone = s.ends_with('Z')
        || s.get(10..)
            .map(|t| t.contains('+') || t.rfind('-').is_some_and(|i| i > 0))
            .unwrap_or(false);
    let normalized = if s.len() == 10 || has_zone {
        s.to_string()
    } else {
        format!("{s}Z")
    };
    let t = Date::new(&JsValue::from_str(&normalized)).get_time();
    (!t.is_nan()).then_some(t)
}

pub fn tz_label(offset_hours: i8) -> String {
    match offset_hours {
        7 => "WIB".into(),
        8 => "WITA".into(),
        9 => "WIT".into(),
        h => format!("UTC{h:+}"),
    }
}

struct Parts {
    weekday: usize,
    day: u32,
    month: usize,
    year: i32,
    hour: u32,
    minute: u32,
}

fn parts(ms_utc: f64, offset_hours: i8) -> Parts {
    let d = Date::new(&JsValue::from_f64(
        ms_utc + offset_hours as f64 * MS_PER_HOUR,
    ));
    Parts {
        weekday: d.get_utc_day() as usize % 7,
        day: d.get_utc_date(),
        month: d.get_utc_month() as usize % 12,
        year: d.get_utc_full_year() as i32,
        hour: d.get_utc_hours(),
        minute: d.get_utc_minutes(),
    }
}

/// "Minggu, 22 Nov 2026 · 17.00 WIB"
pub fn fmt_datetime_long(ms_utc: f64, offset_hours: i8) -> String {
    let p = parts(ms_utc, offset_hours);
    format!(
        "{}, {} {} {} · {:02}.{:02} {}",
        DAYS[p.weekday],
        p.day,
        MONTHS[p.month],
        p.year,
        p.hour,
        p.minute,
        tz_label(offset_hours)
    )
}

/// "22 Nov, 17.00 WIB"
pub fn fmt_datetime_short(ms_utc: f64, offset_hours: i8) -> String {
    let p = parts(ms_utc, offset_hours);
    format!(
        "{} {}, {:02}.{:02} {}",
        p.day,
        MONTHS[p.month],
        p.hour,
        p.minute,
        tz_label(offset_hours)
    )
}

/// Tanggal kalender `YYYY-MM-DD` → "Sen, 24 Nov".
pub fn fmt_day(date: &str) -> String {
    match parse_utc_ms(date.get(..10).unwrap_or(date)) {
        Some(ms) => {
            let p = parts(ms, 0);
            format!("{}, {} {}", &DAYS[p.weekday][..3], p.day, MONTHS[p.month])
        }
        None => date.to_string(),
    }
}

/// Tanggal kalender `YYYY-MM-DD` → "24/11".
pub fn fmt_day_tick(date: &str) -> String {
    match parse_utc_ms(date.get(..10).unwrap_or(date)) {
        Some(ms) => {
            let p = parts(ms, 0);
            format!("{}/{}", p.day, p.month + 1)
        }
        None => date.to_string(),
    }
}

/// Tanggal lokal (pada zona `offset_hours`) dari sebuah waktu UTC, sebagai `YYYY-MM-DD`.
pub fn local_date_iso(ms_utc: f64, offset_hours: i8) -> String {
    let p = parts(ms_utc, offset_hours);
    format!("{:04}-{:02}-{:02}", p.year, p.month + 1, p.day)
}

/// Selisih hari kalender `to - from` (keduanya `YYYY-MM-DD`).
pub fn days_between(from: &str, to: &str) -> Option<i64> {
    let a = parse_utc_ms(from.get(..10)?)?;
    let b = parse_utc_ms(to.get(..10)?)?;
    Some(((b - a) / MS_PER_DAY).round() as i64)
}

pub fn fmt_relative(ms_utc: f64, now: f64) -> String {
    let mins = ((now - ms_utc) / 60_000.0).floor();
    if mins < 1.0 {
        "baru saja".into()
    } else if mins < 60.0 {
        format!("{} menit lalu", mins as i64)
    } else if mins < 60.0 * 48.0 {
        format!("{} jam lalu", (mins / 60.0).floor() as i64)
    } else {
        format!("{} hari lalu", (mins / 1440.0).floor() as i64)
    }
}

/// Angka dengan pemisah ribuan titik dan desimal koma.
pub fn fmt_num(v: f64, decimals: usize) -> String {
    if !v.is_finite() {
        return "–".into();
    }
    let s = format!("{:.*}", decimals, v.abs());
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s.clone(), None),
    };
    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3);
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let sign = if v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') {
        "−"
    } else {
        ""
    };
    match frac_part {
        Some(f) => format!("{sign}{grouped},{f}"),
        None => format!("{sign}{grouped}"),
    }
}

/// Debit dengan presisi menurut besarnya.
pub fn fmt_q(v: f64) -> String {
    let a = v.abs();
    if a >= 100.0 {
        fmt_num(v, 0)
    } else if a >= 10.0 {
        fmt_num(v, 1)
    } else {
        fmt_num(v, 2)
    }
}

pub fn fmt_pct(frac: f64) -> String {
    format!("{}%", fmt_num((frac * 100.0).clamp(0.0, 100.0), 0))
}

pub fn fmt_ratio(r: f64) -> String {
    format!("{}×", fmt_num(r, 2))
}
