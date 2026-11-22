//! Replay kejadian historis dari data yang dibakukan `scripts/bake_replays.py`:
//! debit reanalisis GloFAS v4 pada sel terkalibrasi + hujan dan kelembapan tanah ERA5,
//! satu bingkai per hari. Tidak ada angka yang diketik tangan di sini.

use crate::catalog::{COASTAL_ROB_POINTS, RIVER_STATIONS};
use crate::hydrology::{compute_kpis, evaluate_rob, evaluate_station, StationInputs};
use crate::model::{
    FloodSnapshot, HydrographPoint, ReplayFrame, ReplayScenarioMeta, UpstreamHealth,
    METHODOLOGY_VERSION, SCHEMA_VERSION,
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
        })
        .collect()
}

fn at<T: Copy>(v: &[Option<T>], i: usize) -> Option<T> {
    v.get(i).copied().flatten()
}

pub fn build_replay_snapshot(scenario_id: &str, frame_idx: usize) -> Option<FloodSnapshot> {
    let sc = baked().iter().find(|s| s.id == scenario_id)?;
    let frame = sc
        .frames
        .get(frame_idx.min(sc.frames.len().saturating_sub(1)))?;
    let day = sc.dates.iter().position(|d| *d == frame.date)?;

    let mut stations = Vec::with_capacity(RIVER_STATIONS.len());
    for st in RIVER_STATIONS {
        let Some(series) = sc.stations.iter().find(|b| b.id == st.id) else {
            continue;
        };
        let hydrograph: Vec<HydrographPoint> = (0..=day)
            .filter_map(|j| {
                Some(HydrographPoint {
                    date: sc.dates[j].clone(),
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
            continue;
        }
        let rain_24h = at(&series.rain, day).unwrap_or(0.0);
        let rain_72h: f64 = (day.saturating_sub(2)..=day)
            .filter_map(|j| at(&series.rain, j))
            .sum();
        let today_idx = hydrograph.len() - 1;
        stations.push(evaluate_station(
            st,
            StationInputs {
                hydrograph,
                today_idx,
                rain_24h_mm: rain_24h,
                rain_72h_mm: rain_72h,
                rain_next_24h_mm: 0.0,
                soil_top: at(&series.soil_top, day).unwrap_or(f64::NAN),
                soil_deep: at(&series.soil_deep, day).unwrap_or(f64::NAN),
                nearby_reports_24h: 0,
                exceed_prob_7d: None,
            },
        ));
    }

    let mut robs = Vec::with_capacity(COASTAL_ROB_POINTS.len());
    for rb in COASTAL_ROB_POINTS {
        let series = sc.rob.iter().find(|r| r.id == rb.id);
        let msl = series.and_then(|r| at(&r.msl_max, day)).unwrap_or(f64::NAN);
        let wave = series
            .and_then(|r| at(&r.wave_max, day))
            .unwrap_or(f64::NAN);
        let time = series
            .and_then(|r| r.msl_max_time.get(day).cloned().flatten())
            .unwrap_or_else(|| frame.date.clone());
        robs.push(evaluate_rob(rb, msl, msl, time, wave));
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
        replay_note: Some(if sc.note.is_empty() {
            sc.provenance.clone()
        } else {
            format!("{} {}", sc.provenance, sc.note)
        }),
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
