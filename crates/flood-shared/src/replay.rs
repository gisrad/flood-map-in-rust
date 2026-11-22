//! Replay kejadian historis dari data yang dibakukan `scripts/bake_replays.py`:
//! debit reanalisis GloFAS v4 pada sel terkalibrasi + hujan dan kelembapan tanah ERA5,
//! satu bingkai per hari. Tidak ada angka yang diketik tangan di sini; fakta kejadian
//! (`observed`) berasal dari sumber yang dikutip pada skenario.

use crate::catalog::{COASTAL_ROB_POINTS, RIVER_STATIONS};
use crate::hydrology::{
    compute_kpis, evaluate_rob, evaluate_station, fmt_dec, fmt_num, StationInputs,
};
use crate::model::{
    FloodSnapshot, HydrographPoint, IndicatorLevel, ReplayFrame, ReplayScenarioMeta, StationStatic,
    StationTelemetry, UpstreamHealth, METHODOLOGY_VERSION, SCHEMA_VERSION,
};
use serde::Deserialize;
use std::sync::OnceLock;

const REPLAY_SOURCES: &[&str] = &[
    include_str!("../data/replays/jabodetabek-2020.json"),
    include_str!("../data/replays/barito-2021.json"),
    include_str!("../data/replays/pantura-wulan-2024.json"),
];

#[derive(Debug, Clone, Deserialize)]
struct BakedScenario {
    id: String,
    title: String,
    period: String,
    center_lon: f64,
    center_lat: f64,
    zoom: f64,
    citation: String,
    provenance: String,
    #[serde(default)]
    note: String,
    /// Deret tanggal harian (termasuk hari-hari pengantar sebelum bingkai pertama).
    dates: Vec<String>,
    frames: Vec<ReplayFrame>,
    stations: Vec<BakedStation>,
    #[serde(default)]
    rob: Vec<BakedRob>,
    /// Stasiun kunci kejadian, urutan tampil.
    #[serde(default)]
    focus: Vec<String>,
    /// Fakta kejadian bersumber untuk pembanding hasil model.
    #[serde(default)]
    observed: Vec<Observation>,
}

#[derive(Debug, Clone, Deserialize)]
struct Observation {
    /// Tanggal ISO yang dilaporkan sumber.
    date: String,
    text: String,
    /// Hujan harian teramati (mm), bila sumber menyebutkannya.
    #[serde(default)]
    rain_mm: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct BakedStation {
    id: String,
    q: Vec<Option<f64>>,
    rain: Vec<Option<f64>>,
    #[serde(default)]
    soil_top: Vec<Option<f64>>,
    #[serde(default)]
    soil_deep: Vec<Option<f64>>,
}

#[derive(Debug, Clone, Deserialize)]
struct BakedRob {
    id: String,
    /// Maksimum harian muka laut model (m).
    msl_max: Vec<Option<f64>>,
    #[serde(default)]
    msl_max_time: Vec<Option<String>>,
    #[serde(default)]
    wave_max: Vec<Option<f64>>,
}

fn baked() -> &'static [BakedScenario] {
    static CELL: OnceLock<Vec<BakedScenario>> = OnceLock::new();
    CELL.get_or_init(|| {
        REPLAY_SOURCES
            .iter()
            .map(|s| serde_json::from_str::<BakedScenario>(s).expect("replay JSON tidak valid"))
            .filter(|s| !s.frames.is_empty())
            .collect()
    })
}

pub fn list_replay_scenarios() -> Vec<ReplayScenarioMeta> {
    baked()
        .iter()
        .map(|s| ReplayScenarioMeta {
            id: s.id.clone(),
            title: s.title.clone(),
            period: s.period.clone(),
            center_lon: s.center_lon,
            center_lat: s.center_lat,
            zoom: s.zoom,
            citation: s.citation.clone(),
            provenance: s.provenance.clone(),
            frames: s.frames.clone(),
            outcome: scenario_outcome(s),
        })
        .collect()
}

fn at<T: Copy>(v: &[Option<T>], i: usize) -> Option<T> {
    v.get(i).copied().flatten()
}

/// Indeks `dates` untuk setiap bingkai.
fn frame_days(sc: &BakedScenario) -> Vec<usize> {
    sc.frames
        .iter()
        .filter_map(|f| sc.dates.iter().position(|d| *d == f.date))
        .collect()
}

/// Satu stasiun pada hari `day` (indeks `dates`), dinilai dengan aturan level data live.
fn station_on_day(sc: &BakedScenario, st: &StationStatic, day: usize) -> Option<StationTelemetry> {
    let series = sc.stations.iter().find(|b| b.id == st.id)?;
    let hydrograph: Vec<HydrographPoint> = (0..=day)
        .filter_map(|j| {
            Some(HydrographPoint {
                date: sc.dates.get(j)?.clone(),
                is_forecast: false,
                discharge_m3s: at(&series.q, j)?,
                discharge_min_m3s: None,
                discharge_p25_m3s: None,
                discharge_p75_m3s: None,
                discharge_max_m3s: None,
                rain_mm: at(&series.rain, j),
            })
        })
        .collect();
    if hydrograph.is_empty() {
        return None;
    }
    let rain_72h: f64 = (day.saturating_sub(2)..=day)
        .filter_map(|j| at(&series.rain, j))
        .sum();
    let today_idx = hydrograph.len() - 1;
    Some(evaluate_station(
        st,
        StationInputs {
            hydrograph,
            today_idx,
            rain_24h_mm: at(&series.rain, day).unwrap_or(f64::NAN),
            rain_72h_mm: rain_72h,
            rain_next_24h_mm: f64::NAN,
            soil_top: at(&series.soil_top, day).unwrap_or(f64::NAN),
            soil_deep: at(&series.soil_deep, day).unwrap_or(f64::NAN),
            nearby_reports_24h: 0,
            exceed_prob_7d: None,
            hindcast: true,
        },
    ))
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
];

/// "2020-01-01" → "1 Jan 2020".
fn id_date(iso: &str) -> String {
    let mut it = iso.split('-');
    let (Some(y), Some(m), Some(d)) = (
        it.next(),
        it.next().and_then(|m| m.parse::<usize>().ok()),
        it.next().and_then(|d| d.parse::<u32>().ok()),
    ) else {
        return iso.to_string();
    };
    match MONTHS.get(m.wrapping_sub(1)) {
        Some(mon) => format!("{d} {mon} {y}"),
        None => iso.to_string(),
    }
}

/// Ringkasan satu stasiun kunci sepanjang bingkai replay.
struct KeyStation {
    name: String,
    /// Telemetri pada hari pertama level maksimum tercapai, dengan tanggalnya.
    peak: Option<(StationTelemetry, String)>,
    /// Level per hari bingkai (indeks `dates`).
    levels: Vec<(usize, IndicatorLevel)>,
    max_q: Option<f64>,
    max_rain: Option<(f64, String)>,
}

impl KeyStation {
    fn collect(sc: &BakedScenario, st: &StationStatic, days: &[usize]) -> Self {
        let series = sc.stations.iter().find(|b| b.id == st.id);
        let mut k = KeyStation {
            name: st.name.to_string(),
            peak: None,
            levels: Vec::new(),
            max_q: None,
            max_rain: None,
        };
        for &day in days {
            if let Some(s) = series {
                if let Some(q) = at(&s.q, day) {
                    k.max_q = Some(k.max_q.map_or(q, |m: f64| m.max(q)));
                }
                if let Some(r) = at(&s.rain, day) {
                    if k.max_rain.as_ref().is_none_or(|(m, _)| r > *m) {
                        k.max_rain = Some((r, sc.dates[day].clone()));
                    }
                }
            }
            let Some(t) = station_on_day(sc, st, day) else {
                continue;
            };
            k.levels.push((day, t.level));
            if k.peak.as_ref().is_none_or(|(p, _)| t.level > p.level) {
                k.peak = Some((t, sc.dates[day].clone()));
            }
        }
        k
    }

    fn elevated(&self) -> bool {
        self.peak
            .as_ref()
            .is_some_and(|(p, _)| p.level.is_elevated())
    }

    /// Level tertinggi dalam ±1 hari dari `day` (laporan sering berselisih sehari dari hari GMT).
    fn level_near(&self, day: usize) -> Option<IndicatorLevel> {
        self.levels
            .iter()
            .filter(|(d, _)| d.abs_diff(day) <= 1)
            .map(|(_, l)| *l)
            .max()
    }

    fn sentence(&self) -> String {
        let rain = self
            .max_rain
            .as_ref()
            .map(|(r, d)| format!("hujan ERA5 maks {} mm ({})", fmt_dec(*r, 0), id_date(d)))
            .unwrap_or_else(|| "hujan ERA5 tidak tersedia".into());
        let Some((p, date)) = &self.peak else {
            return format!("{}: tanpa data model; {rain}.", self.name);
        };
        match &p.thresholds {
            _ if p.level.is_elevated() => {
                let cause = match &p.thresholds {
                    Some(t) if p.fluvial_level == p.level => format!(
                        "debit model {} m³/s, Q2 {} m³/s",
                        fmt_num(p.current_discharge_m3s),
                        fmt_num(t.q2_m3s)
                    ),
                    Some(_) => format!("hujan ERA5 {} mm", fmt_dec(p.rain_24h_mm, 0)),
                    None => format!(
                        "hujan ERA5 {} mm; debit tidak dinilai, sel model belum terkalibrasi",
                        fmt_dec(p.rain_24h_mm, 0)
                    ),
                };
                format!(
                    "{}: maksimum {} pada {} ({cause}).",
                    self.name,
                    p.level.label_id(),
                    id_date(date)
                )
            }
            Some(t) => format!(
                "{}: tetap Normal; debit model maks {} m³/s (Q2 {} m³/s), {rain}.",
                self.name,
                self.max_q.map(fmt_num).unwrap_or_else(|| "–".into()),
                fmt_num(t.q2_m3s)
            ),
            None => format!(
                "{}: debit tidak dinilai (sel model belum terkalibrasi); {rain}.",
                self.name
            ),
        }
    }
}

/// Hasil model terhadap kejadian, dihitung dari data replay: stasiun kunci yang naik, level
/// stasiun kunci di sekitar tanggal kejadian bersumber, dan pembanding hujan teramati.
fn scenario_outcome(sc: &BakedScenario) -> Vec<String> {
    let days = frame_days(sc);
    let keys: Vec<KeyStation> = sc
        .focus
        .iter()
        .filter_map(|id| RIVER_STATIONS.iter().find(|s| s.id == id.as_str()))
        .map(|st| KeyStation::collect(sc, st, &days))
        .collect();
    if keys.is_empty() {
        return Vec::new();
    }
    let raised = keys.iter().filter(|k| k.elevated()).count();
    let mut out = vec![if raised == 0 {
        format!(
            "Model tidak menaikkan satu pun dari {} stasiun kunci ke level Tinggi atau lebih selama periode replay.",
            keys.len()
        )
    } else {
        format!(
            "Model menaikkan {raised} dari {} stasiun kunci ke level Tinggi atau lebih.",
            keys.len()
        )
    }];
    out.extend(keys.iter().map(KeyStation::sentence));

    for ob in &sc.observed {
        let near: Vec<String> = match sc.dates.iter().position(|d| *d == ob.date) {
            Some(day) => keys
                .iter()
                .map(|k| {
                    let lv = k
                        .level_near(day)
                        .map(|l| l.label_id().to_lowercase())
                        .unwrap_or_else(|| "tanpa data".into());
                    format!("{} {lv}", k.name)
                })
                .collect(),
            None => Vec::new(),
        };
        if near.is_empty() {
            out.push(format!("{}: {}.", id_date(&ob.date), ob.text));
        } else {
            out.push(format!(
                "{}: {}. Level stasiun kunci ±1 hari: {}.",
                id_date(&ob.date),
                ob.text,
                near.join("; ")
            ));
        }
        if let Some(obs_mm) = ob.rain_mm.filter(|v| *v > 0.0) {
            let top = keys
                .iter()
                .filter_map(|k| k.max_rain.as_ref().map(|(r, d)| (*r, d, &k.name)))
                .max_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((r, d, name)) = top {
                out.push(format!(
                    "Hujan ERA5 harian tertinggi di stasiun kunci {} mm ({name}, {}), sekitar {}% dari {} mm teramati.",
                    fmt_dec(r, 0),
                    id_date(d),
                    fmt_dec(100.0 * r / obs_mm, 0),
                    fmt_dec(obs_mm, 0)
                ));
            }
        }
    }
    out.push(
        "ERA5 (grid 0,25°) meratakan hujan lokal yang ekstrem, dan GloFAS tidak memodelkan tanggul jebol atau pintu air kota. Replay menguji model, bukan merekonstruksi kejadian."
            .to_string(),
    );
    out
}

pub fn build_replay_snapshot(scenario_id: &str, frame_idx: usize) -> Option<FloodSnapshot> {
    let sc = baked().iter().find(|s| s.id == scenario_id)?;
    let frame = sc
        .frames
        .get(frame_idx.min(sc.frames.len().saturating_sub(1)))?;
    let day = sc.dates.iter().position(|d| *d == frame.date)?;

    let stations: Vec<StationTelemetry> = RIVER_STATIONS
        .iter()
        .filter_map(|st| station_on_day(sc, st, day))
        .collect();

    // Puncak 48 jam dari awal hari GMT = maksimum harian hari ini dan besok, setara dengan
    // besaran yang dinilai pada data live dan pada ambang `rob_calibration.json`.
    let mut robs = Vec::with_capacity(COASTAL_ROB_POINTS.len());
    for rb in COASTAL_ROB_POINTS {
        let series = sc.rob.iter().find(|r| r.id == rb.id);
        let peak = series.and_then(|r| {
            (day..=day + 1)
                .filter_map(|d| at(&r.msl_max, d).map(|v| (v, d)))
                .max_by(|a, b| a.0.total_cmp(&b.0))
        });
        let wave = series
            .and_then(|r| {
                (day..=day + 1)
                    .filter_map(|d| at(&r.wave_max, d))
                    .max_by(f64::total_cmp)
            })
            .unwrap_or(f64::NAN);
        let time = peak
            .and_then(|(_, d)| series?.msl_max_time.get(d).cloned().flatten())
            .unwrap_or_else(|| frame.date.clone());
        let peak_m = peak.map_or(f64::NAN, |(v, _)| v);
        robs.push(evaluate_rob(rb, f64::NAN, peak_m, time, wave));
    }

    let kpis = compute_kpis(&stations, &robs, 0, 0);
    let has_rob = !sc.rob.is_empty();
    Some(FloodSnapshot {
        schema_version: SCHEMA_VERSION,
        methodology_version: METHODOLOGY_VERSION.to_string(),
        generated_at_utc: format!("{}T00:00:00Z", frame.date),
        is_historical_replay: true,
        replay_id: Some(sc.id.clone()),
        replay_label: Some(format!("{} · {}", sc.title, frame.step_label)),
        replay_citation: Some(sc.citation.clone()),
        // Hanya catatan skenario; asal data (`provenance`) ada di metadata skenario dan
        // `upstream.notes`, jadi tidak diulang di sini.
        replay_note: (!sc.note.is_empty()).then(|| sc.note.clone()),
        kpis,
        upstream: UpstreamHealth {
            glofas_ok: true,
            weather_ok: true,
            marine_ok: has_rob,
            notes: vec![sc.provenance.clone()],
            ..UpstreamHealth::default()
        },
        stations,
        coastal_rob: robs,
        citizen_reports: Vec::new(),
        bmkg_nowcasts: Vec::new(),
        news: Vec::new(),
    })
}
