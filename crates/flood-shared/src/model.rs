use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 2;
pub const METHODOLOGY_VERSION: &str = "2026-11-22";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloodMechanism {
    FluvialBasin,
    UrbanPluvial,
    CoastalRob,
}

impl FloodMechanism {
    pub fn label_id(self) -> &'static str {
        match self {
            Self::FluvialBasin => "Banjir kiriman hulu",
            Self::UrbanPluvial => "Genangan hujan lokal",
            Self::CoastalRob => "Rob pesisir",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::FluvialBasin => "fluvial",
            Self::UrbanPluvial => "pluvial",
            Self::CoastalRob => "rob",
        }
    }
}

/// Level indikasi model. Bukan status peringatan resmi (BNPB/BPBD/BMKG/BBWS).
/// Urutan varian = urutan keparahan, sehingga `Ord` dapat dipakai untuk `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorLevel {
    Unknown,
    Normal,
    High,
    VeryHigh,
    Extreme,
}

impl IndicatorLevel {
    pub fn label_id(self) -> &'static str {
        match self {
            Self::Unknown => "Belum terkalibrasi",
            Self::Normal => "Normal",
            Self::High => "Tinggi",
            Self::VeryHigh => "Sangat tinggi",
            Self::Extreme => "Ekstrem",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Normal => 1,
            Self::High => 2,
            Self::VeryHigh => 3,
            Self::Extreme => 4,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Normal => "normal",
            Self::High => "high",
            Self::VeryHigh => "very_high",
            Self::Extreme => "extreme",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "unknown" => Some(Self::Unknown),
            "normal" => Some(Self::Normal),
            "high" => Some(Self::High),
            "very_high" => Some(Self::VeryHigh),
            "extreme" => Some(Self::Extreme),
            _ => None,
        }
    }

    /// `true` untuk level Tinggi ke atas.
    pub fn is_elevated(self) -> bool {
        self >= Self::High
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Public,
    Gov,
    Academia,
    AiAgent,
}

impl Audience {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "gov" | "bpbd" | "pemda" | "bnpb" => Self::Gov,
            "academia" | "riset" | "research" | "academic" => Self::Academia,
            "ai" | "ai_agent" | "agent" => Self::AiAgent,
            _ => Self::Public,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Public => "Publik",
            Self::Gov => "BPBD / Pemda",
            Self::Academia => "Akademisi / Riset",
            Self::AiAgent => "AI Agent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    NaikTajam,
    Naik,
    Stabil,
    Surut,
}

impl Trend {
    pub fn label(self) -> &'static str {
        match self {
            Self::NaikTajam => "Naik Tajam",
            Self::Naik => "Naik",
            Self::Stabil => "Stabil",
            Self::Surut => "Surut",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StationStatic {
    pub id: &'static str,
    pub name: &'static str,
    pub river: &'static str,
    pub ws_code: &'static str,
    pub province: &'static str,
    pub island: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub tz_offset_hours: i8,
    pub basin_area_km2: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoastalRobStatic {
    pub id: &'static str,
    pub name: &'static str,
    pub province: &'static str,
    pub island: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub tz_offset_hours: i8,
}

/// Sel GloFAS v4 (0,05°) terpilih untuk stasiun.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlofasCell {
    pub lat: f64,
    pub lon: f64,
    pub offset_km: f64,
    /// Debit rerata harian 1997–2024 pada sel ini.
    pub mean_q_m3s: f64,
}

/// Ambang klimatologis dari reanalisis GloFAS v4 pada sel yang sama.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiverThresholds {
    pub q2_m3s: f64,
    pub q5_m3s: f64,
    pub q20_m3s: f64,
    pub p90_m3s: f64,
    pub p99_m3s: f64,
    pub period: String,
    pub n_years: u32,
}

/// Fraksi (0..1) anggota ensemble yang melampaui ambang dalam horizon prakiraan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExceedProb {
    pub q2: f64,
    pub q5: f64,
    pub q20: f64,
    pub members: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrographPoint {
    pub date: String,
    pub is_forecast: bool,
    pub discharge_m3s: f64,
    #[serde(default)]
    pub discharge_min_m3s: Option<f64>,
    #[serde(default)]
    pub discharge_p25_m3s: Option<f64>,
    #[serde(default)]
    pub discharge_p75_m3s: Option<f64>,
    #[serde(default)]
    pub discharge_max_m3s: Option<f64>,
    #[serde(default)]
    pub rain_mm: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationTelemetry {
    pub id: String,
    pub name: String,
    pub river: String,
    pub ws_code: String,
    pub province: String,
    pub island: String,
    pub lat: f64,
    pub lon: f64,
    pub tz_offset_hours: i8,
    pub basin_area_km2: u32,
    #[serde(default)]
    pub glofas_cell: Option<GlofasCell>,
    #[serde(default)]
    pub thresholds: Option<RiverThresholds>,
    pub current_discharge_m3s: f64,
    #[serde(default)]
    pub ratio_q2: Option<f64>,
    pub peak_forecast_m3s: f64,
    pub peak_forecast_date: String,
    pub peak_forecast_level: IndicatorLevel,
    #[serde(default)]
    pub exceed_prob_7d: Option<ExceedProb>,
    pub trend: Trend,
    pub rain_24h_mm: f64,
    pub rain_72h_mm: f64,
    pub rain_next_24h_mm: f64,
    pub soil_moisture_top: f64,
    pub soil_moisture_deep: f64,
    pub nearby_reports_24h: u32,
    pub fluvial_level: IndicatorLevel,
    pub pluvial_level: IndicatorLevel,
    pub level: IndicatorLevel,
    pub mechanism: FloodMechanism,
    pub level_reason: String,
    pub public_advice: String,
    pub gov_action: String,
    pub academic_note: String,
    pub hydrograph: Vec<HydrographPoint>,
}

/// Ambang muka laut model setempat (Open-Meteo Marine `sea_level_height_msl`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RobThresholds {
    pub p95_m: f64,
    pub p99_m: f64,
    pub max_m: f64,
    pub period: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoastalRobTelemetry {
    pub id: String,
    pub name: String,
    pub province: String,
    pub island: String,
    pub lat: f64,
    pub lon: f64,
    pub tz_offset_hours: i8,
    pub current_msl_m: f64,
    pub peak_msl_48h_m: f64,
    pub peak_msl_time: String,
    pub wave_height_m: f64,
    #[serde(default)]
    pub thresholds: Option<RobThresholds>,
    pub level: IndicatorLevel,
    pub level_reason: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitizenReport {
    pub pkey: String,
    pub created_at: String,
    pub source: String,
    pub status: String,
    #[serde(default)]
    pub depth_cm: Option<u32>,
    pub title: String,
    pub text: String,
    pub lat: f64,
    pub lon: f64,
    pub disaster_type: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelCounts {
    pub high: usize,
    pub very_high: usize,
    pub extreme: usize,
    pub unknown: usize,
}

impl LevelCounts {
    pub fn add(&mut self, level: IndicatorLevel) {
        match level {
            IndicatorLevel::High => self.high += 1,
            IndicatorLevel::VeryHigh => self.very_high += 1,
            IndicatorLevel::Extreme => self.extreme += 1,
            IndicatorLevel::Unknown => self.unknown += 1,
            IndicatorLevel::Normal => {}
        }
    }

    /// Jumlah titik berlevel Tinggi ke atas.
    pub fn elevated(&self) -> usize {
        self.high + self.very_high + self.extreme
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloodSummaryKpis {
    pub river: LevelCounts,
    pub pluvial: LevelCounts,
    pub rob: LevelCounts,
    pub citizen_flood_reports_24h: usize,
    pub bmkg_nowcasts: usize,
    #[serde(default)]
    pub max_ratio_q2: Option<f64>,
    #[serde(default)]
    pub max_ratio_station: Option<String>,
    /// Jumlah stasiun sungai tanpa ambang terkalibrasi (`thresholds = null`).
    pub uncalibrated: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpstreamHealth {
    pub glofas_ok: bool,
    pub weather_ok: bool,
    pub marine_ok: bool,
    pub petabencana_ok: bool,
    pub bmkg_nowcast_ok: bool,
    pub news_ok: bool,
    pub notes: Vec<String>,
    #[serde(default)]
    pub glofas_fetched_at_utc: Option<String>,
    #[serde(default)]
    pub weather_fetched_at_utc: Option<String>,
    #[serde(default)]
    pub marine_fetched_at_utc: Option<String>,
    #[serde(default)]
    pub petabencana_fetched_at_utc: Option<String>,
    #[serde(default)]
    pub bmkg_fetched_at_utc: Option<String>,
    #[serde(default)]
    pub news_fetched_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    pub link: String,
    pub source: String,
    /// ISO 8601 UTC, mis. `2026-11-22T03:15:00Z`.
    pub pub_date: String,
    pub region_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmkgNowcast {
    pub title: String,
    pub province: String,
    pub summary: String,
    pub link: String,
    pub pub_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSubscriptionReq {
    pub topic: String,
    #[serde(default)]
    pub region: String,
    #[serde(default = "default_min_level")]
    pub min_level: IndicatorLevel,
    #[serde(default)]
    pub send_test: bool,
}

fn default_min_level() -> IndicatorLevel {
    IndicatorLevel::High
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloodSnapshot {
    pub schema_version: u32,
    pub methodology_version: String,
    pub generated_at_utc: String,
    pub is_historical_replay: bool,
    #[serde(default)]
    pub replay_id: Option<String>,
    #[serde(default)]
    pub replay_label: Option<String>,
    #[serde(default)]
    pub replay_citation: Option<String>,
    #[serde(default)]
    pub replay_note: Option<String>,
    pub kpis: FloodSummaryKpis,
    pub upstream: UpstreamHealth,
    pub stations: Vec<StationTelemetry>,
    pub coastal_rob: Vec<CoastalRobTelemetry>,
    pub citizen_reports: Vec<CitizenReport>,
    #[serde(default)]
    pub bmkg_nowcasts: Vec<BmkgNowcast>,
    #[serde(default)]
    pub news: Vec<NewsItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiBriefing {
    pub generated_at_utc: String,
    pub region_filter: String,
    pub audience: Audience,
    pub headline: String,
    pub sitrep_summary: String,
    pub mechanism_breakdown: String,
    pub priority_actions: Vec<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiFlyTarget {
    pub label: String,
    pub lon: f64,
    pub lat: f64,
    pub zoom: f64,
    pub station_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiQueryResponse {
    pub query: String,
    pub answer_id: String,
    pub recommended_mechanism: Option<FloodMechanism>,
    pub matched_station_ids: Vec<String>,
    pub matched_rob_ids: Vec<String>,
    pub fly_to: Option<AiFlyTarget>,
}

/// Satu bingkai replay = satu hari kalender (data harian reanalisis).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayFrame {
    /// Tanggal ISO `YYYY-MM-DD`.
    pub date: String,
    pub step_label: String,
    pub timestamp_wib: String,
    pub narrative: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayScenarioMeta {
    pub id: String,
    pub title: String,
    pub period: String,
    pub center_lon: f64,
    pub center_lat: f64,
    pub zoom: f64,
    pub citation: String,
    pub provenance: String,
    pub frames: Vec<ReplayFrame>,
}
