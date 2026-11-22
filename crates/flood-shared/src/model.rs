use serde::{Deserialize, Serialize};

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
            Self::FluvialBasin => "Banjir Kiriman Hulu",
            Self::UrbanPluvial => "Drainase & Genangan Kota",
            Self::CoastalRob => "Banjir Rob Pesisir",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    Normal,
    Waspada,
    Siaga,
    Awas,
}

impl AlertLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Waspada => "Waspada",
            Self::Siaga => "Siaga",
            Self::Awas => "Awas",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Waspada => 1,
            Self::Siaga => 2,
            Self::Awas => 3,
        }
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
    pub baseline_q75_hint: f64,
    /// Schematic upstream -> downstream polyline coordinates `[lon, lat]`.
    pub river_polyline: &'static [[f64; 2]],
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
    /// Subsidence & low-lying elevation risk multiplier (1.0 = normal, 1.45 = extreme Pantura subsidence).
    pub sensitivity: f64,
    pub trigger_msl_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrographPoint {
    pub date: String,
    pub is_forecast: bool,
    pub discharge_m3s: f64,
    pub discharge_mean_m3s: f64,
    pub discharge_max_m3s: f64,
    pub discharge_p75_m3s: f64,
    pub rain_mm: f64,
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
    pub river_polyline: Vec<[f64; 2]>,
    pub current_discharge_m3s: f64,
    pub p75_baseline_m3s: f64,
    pub q95_threshold_m3s: f64,
    pub peak_forecast_m3s: f64,
    pub peak_forecast_date: String,
    pub anomaly_ratio: f64,
    pub peak_ratio: f64,
    pub return_period_est: String,
    pub trend: Trend,
    pub rain_24h_mm: f64,
    pub rain_72h_mm: f64,
    pub soil_moisture_top: f64,
    pub soil_moisture_deep: f64,
    pub nearby_reports_24h: u32,
    pub mechanism: FloodMechanism,
    pub alert_level: AlertLevel,
    pub public_advice: String,
    pub gov_action: String,
    pub academic_note: String,
    pub hydrograph: Vec<HydrographPoint>,
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
    pub sensitivity: f64,
    pub trigger_msl_m: f64,
    pub current_msl_m: f64,
    pub peak_msl_48h_m: f64,
    pub peak_msl_time: String,
    pub wave_height_m: f64,
    pub rob_index: f64,
    pub alert_level: AlertLevel,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitizenReport {
    pub pkey: String,
    pub created_at: String,
    pub source: String,
    pub status: String,
    pub depth_cm: Option<u32>,
    pub title: String,
    pub text: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloodSummaryKpis {
    pub total_stations: usize,
    pub total_rob_corridors: usize,
    pub awas_count: usize,
    pub siaga_count: usize,
    pub waspada_count: usize,
    pub fluvial_elevated: usize,
    pub pluvial_elevated: usize,
    pub rob_elevated: usize,
    pub citizen_reports_24h: usize,
    pub max_anomaly_ratio: f64,
    pub top_station_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamHealth {
    pub glofas_ok: bool,
    pub weather_ok: bool,
    pub marine_ok: bool,
    pub petabencana_ok: bool,
    #[serde(default)]
    pub bmkg_nowcast_ok: bool,
    #[serde(default)]
    pub news_ok: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    pub link: String,
    pub source: String,
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
    pub min_level: AlertLevel,
    #[serde(default)]
    pub send_test: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloodSnapshot {
    pub generated_at_utc: String,
    pub is_historical_replay: bool,
    pub replay_id: Option<String>,
    pub replay_label: Option<String>,
    pub replay_citation: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayFrame {
    pub step_label: String,
    pub timestamp_wib: String,
    pub narrative: String,
    pub station_overrides: Vec<ReplayStationOverride>,
    pub rob_overrides: Vec<ReplayRobOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayStationOverride {
    pub station_id: String,
    pub discharge_m3s: f64,
    pub rain_24h_mm: f64,
    pub rain_72h_mm: f64,
    pub soil_top: f64,
    pub soil_deep: f64,
    pub mechanism: FloodMechanism,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayRobOverride {
    pub rob_id: String,
    pub msl_m: f64,
    pub wave_m: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplayScenarioMeta {
    pub id: &'static str,
    pub title: &'static str,
    pub period: &'static str,
    pub center_lon: f64,
    pub center_lat: f64,
    pub zoom: f64,
    pub citation: &'static str,
    pub frames: Vec<ReplayFrame>,
}
