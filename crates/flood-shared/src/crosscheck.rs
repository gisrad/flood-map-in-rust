//! Data pembanding yang dibangkitkan skrip luring, ditanam lewat `include_str!`:
//! - `scripts/crosscheck_grrr.py` → uji kewajaran silang Q2 GloFAS terhadap GRRR (reanalisis 1980–2023);
//! - `scripts/bake_inundation.py` → luas genangan historis (Inundation History 1999–2020) per titik;
//! - `scripts/validate_gauges.py` → rerata debit sel GloFAS terhadap rerata terukur pos duga air GRDC.
//!
//! Semuanya statis: tidak diambil saat operasi dan tidak memengaruhi level indikasi.

use crate::model::{GaugeCheck, GrrrCheck, InundationExposure};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

const GRRR_JSON: &str = include_str!("../data/grrr_crosscheck.json");
const INUNDATION_JSON: &str = include_str!("../data/inundation_exposure.json");
const GAUGE_JSON: &str = include_str!("../data/gauge_validation.json");

#[derive(Debug, Clone, Deserialize)]
pub struct GrrrSummary {
    pub n_assessed: u32,
    pub verdicts: HashMap<String, u32>,
    pub n_similar_scale: u32,
    pub median_q20_over_q2_glofas: Option<f64>,
    pub median_q20_over_q2_grrr: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrrrFile {
    pub generated_at_utc: String,
    pub source: String,
    pub radius_km: f64,
    pub above_factor: f64,
    pub below_factor: f64,
    pub summary: GrrrSummary,
    pub stations: HashMap<String, GrrrCheck>,
}

#[derive(Debug, Clone, Deserialize)]
struct ExposureRecord {
    buffer_km2: f64,
    low_km2: f64,
    medium_km2: f64,
    high_km2: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InundationFile {
    pub generated_at_utc: String,
    pub source: String,
    pub radius_km: f64,
    points: HashMap<String, ExposureRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GaugeFile {
    pub generated_at_utc: String,
    pub source: String,
    pub flood_source: String,
    pub max_area_ratio: f64,
    pub tolerance: f64,
    pub area_exponents: Vec<f64>,
    pub stations: HashMap<String, GaugeCheck>,
}

static GRRR: LazyLock<GrrrFile> =
    LazyLock::new(|| serde_json::from_str(GRRR_JSON).expect("grrr_crosscheck.json valid"));
static INUNDATION: LazyLock<InundationFile> = LazyLock::new(|| {
    serde_json::from_str(INUNDATION_JSON).expect("inundation_exposure.json valid")
});
static GAUGE: LazyLock<GaugeFile> =
    LazyLock::new(|| serde_json::from_str(GAUGE_JSON).expect("gauge_validation.json valid"));

pub fn gauge_file() -> &'static GaugeFile {
    &GAUGE
}

/// Validasi rerata debit sel terhadap pos duga air GRDC yang mewakili stasiun ini.
pub fn gauge_check(station_id: &str) -> Option<&'static GaugeCheck> {
    GAUGE.stations.get(station_id)
}

pub fn grrr_file() -> &'static GrrrFile {
    &GRRR
}

pub fn inundation_file() -> &'static InundationFile {
    &INUNDATION
}

/// Hasil uji silang GRRR untuk stasiun sungai terkalibrasi.
pub fn grrr_check(station_id: &str) -> Option<&'static GrrrCheck> {
    GRRR.stations.get(station_id)
}

/// Luas genangan historis dalam radius sekitar stasiun atau titik rob.
pub fn inundation_exposure(point_id: &str) -> Option<InundationExposure> {
    let f = inundation_file();
    f.points.get(point_id).map(|r| InundationExposure {
        radius_km: f.radius_km,
        buffer_km2: r.buffer_km2,
        low_km2: r.low_km2,
        medium_km2: r.medium_km2,
        high_km2: r.high_km2,
    })
}
