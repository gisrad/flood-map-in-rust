//! Data kalibrasi yang dibangkitkan skrip `scripts/calibrate_glofas.py` dan
//! `scripts/calibrate_rob.py`, ditanam ke biner lewat `include_str!`.

use crate::hydrology::{gumbel_lmoments, gumbel_quantile, round2};
use crate::model::{GlofasCell, RiverThresholds, RobThresholds};
use serde::Deserialize;
use std::sync::OnceLock;

const GLOFAS_JSON: &str = include_str!("../data/glofas_calibration.json");
const ROB_JSON: &str = include_str!("../data/rob_calibration.json");

/// Jumlah minimum maksimum tahunan untuk menetapkan ambang.
pub const MIN_ANNUAL_MAXIMA: usize = 20;

#[derive(Debug, Clone, Deserialize)]
pub struct GlofasCalibrationFile {
    pub generated_at_utc: String,
    pub source: String,
    pub model: String,
    pub period: String,
    pub method: String,
    pub stations: Vec<StationCalibration>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StationCalibration {
    pub id: String,
    /// `true` bila sel terpilih konsisten dengan besaran sungai.
    pub accepted: bool,
    /// Sel terpilih (hanya bila `accepted`).
    pub cell: Option<GlofasCell>,
    /// Debit harapan dari luas DAS × debit spesifik asumsi (m³/s).
    pub expected_q_m3s: f64,
    #[serde(default)]
    pub annual_max_m3s: Vec<f64>,
    #[serde(default)]
    pub p90_m3s: Option<f64>,
    #[serde(default)]
    pub p99_m3s: Option<f64>,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub note: String,
}

impl StationCalibration {
    /// Ambang Q2/Q5/Q20 (Gumbel L-moment) + p90/p99. `None` bila sel tidak diterima.
    pub fn thresholds(&self) -> Option<RiverThresholds> {
        if !self.accepted || self.cell.is_none() || self.annual_max_m3s.len() < MIN_ANNUAL_MAXIMA {
            return None;
        }
        let (xi, alpha) = gumbel_lmoments(&self.annual_max_m3s)?;
        Some(RiverThresholds {
            q2_m3s: round2(gumbel_quantile(xi, alpha, 2.0)),
            q5_m3s: round2(gumbel_quantile(xi, alpha, 5.0)),
            q20_m3s: round2(gumbel_quantile(xi, alpha, 20.0)),
            p90_m3s: round2(self.p90_m3s?),
            p99_m3s: round2(self.p99_m3s?),
            period: self.period.clone(),
            n_years: self.annual_max_m3s.len() as u32,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RobCalibrationFile {
    pub generated_at_utc: String,
    pub source: String,
    pub period: String,
    pub method: String,
    pub points: Vec<RobCalibration>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RobCalibration {
    pub id: String,
    pub cell_lat: Option<f64>,
    pub cell_lon: Option<f64>,
    pub n_hours: u32,
    pub thresholds: Option<RobThresholds>,
    #[serde(default)]
    pub note: String,
}

pub fn glofas_calibration() -> &'static GlofasCalibrationFile {
    static CELL: OnceLock<GlofasCalibrationFile> = OnceLock::new();
    CELL.get_or_init(|| {
        serde_json::from_str(GLOFAS_JSON).expect("glofas_calibration.json tidak valid")
    })
}

pub fn rob_calibration_file() -> &'static RobCalibrationFile {
    static CELL: OnceLock<RobCalibrationFile> = OnceLock::new();
    CELL.get_or_init(|| serde_json::from_str(ROB_JSON).expect("rob_calibration.json tidak valid"))
}

pub fn station_calibration(id: &str) -> Option<&'static StationCalibration> {
    glofas_calibration().stations.iter().find(|s| s.id == id)
}

pub fn rob_calibration(id: &str) -> Option<&'static RobCalibration> {
    rob_calibration_file().points.iter().find(|p| p.id == id)
}

/// Koordinat yang dipakai untuk mengambil data GloFAS: sel terkalibrasi bila ada,
/// selain itu koordinat pos.
pub fn glofas_fetch_coords(id: &str, station_lat: f64, station_lon: f64) -> (f64, f64) {
    station_calibration(id)
        .and_then(|c| c.cell.as_ref())
        .map(|c| (c.lat, c.lon))
        .unwrap_or((station_lat, station_lon))
}

/// Koordinat sel laut untuk titik rob (hasil kalibrasi) atau koordinat titik.
pub fn marine_fetch_coords(id: &str, lat: f64, lon: f64) -> (f64, f64) {
    rob_calibration(id)
        .and_then(|c| Some((c.cell_lat?, c.cell_lon?)))
        .unwrap_or((lat, lon))
}
