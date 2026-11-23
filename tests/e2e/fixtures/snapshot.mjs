// Test-only synthetic telemetry, never copied into dist. No live feeds are fetched.
// Static catalog/calibration/comparison files are READ from the repository, not duplicated.
// Their original attribution and licenses remain in DATA_LICENSES.md and THIRD_PARTY_NOTICES.md
// (notably GSIM-derived statistics: CC BY-SA 3.0; GRDC catalog attribution is retained).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
export const MARKER = 'DATA SINTETIS / DEMONSTRASI — uji otomatis E2E';
export const FIXTURE_TIME = '2026-11-22T10:00:00Z';
export const repoText = (path) => readFileSync(new URL(`../../../${path}`, import.meta.url), 'utf8');
export const repoJson = (path) => JSON.parse(repoText(path));
const shared = 'crates/flood-shared/';
const catalog = repoText(`${shared}src/catalog.rs`);
const calibration = repoJson(`${shared}data/glofas_calibration.json`);
const robCalibration = repoJson(`${shared}data/rob_calibration.json`);
export const gauges = repoJson(`${shared}data/gauge_validation.json`);
const grrr = repoJson(`${shared}data/grrr_crosscheck.json`);
const exposure = repoJson(`${shared}data/inundation_exposure.json`);
export const replays = ['jabodetabek-2020', 'barito-2021', 'pantura-wulan-2024']
  .map((id) => repoJson(`${shared}data/replays/${id}.json`));

// Read the flat catalog literals so test data follows the repository's station locations.
// Production WASM deserialization is the schema gate; do not reimplement Rust/serde here.
function catalogRecords(type) {
  const records = [...catalog.matchAll(new RegExp(`\\b${type}\\s*\\{([^{}]*)\\}`, 'g'))].map((match) => {
    const record = {};
    for (const field of match[1].matchAll(/(\w+)\s*:\s*("(?:[^"\\]|\\.)*"|-?\d[\d_]*(?:\.\d+)?)/g)) {
      record[field[1]] = field[2].startsWith('"') ? JSON.parse(field[2]) : Number(field[2].replaceAll('_', ''));
    }
    return record;
  });
  assert.ok(records.length > 0, `No ${type} entries parsed from catalog.rs`);
  assert.equal(new Set(records.map((record) => record.id)).size, records.length);
  return records;
}


export const stationsCatalog = catalogRecords('StationStatic');
export const coastalCatalog = catalogRecords('CoastalRobStatic');
const round = (value) => Math.round(value * 100) / 100;
const day = (offset) => new Date(Date.parse('2026-11-22T00:00:00Z') + offset * 86_400_000).toISOString().slice(0, 10);
const ago = (minutes) => new Date(Date.parse(FIXTURE_TIME) - minutes * 60_000).toISOString();
const levels = ['unknown', 'normal', 'high', 'very_high', 'extreme'];
const rank = (level) => levels.indexOf(level);
const higher = (a, b) => rank(a) >= rank(b) ? a : b;
const rainLevel = (mm) => mm > 150 ? 'extreme' : mm >= 100 ? 'very_high' : mm >= 50 ? 'high' : 'normal';
const riverLevel = (q, threshold) => !threshold ? 'unknown' : q >= threshold.q20_m3s ? 'extreme'
  : q >= threshold.q5_m3s ? 'very_high' : q >= threshold.q2_m3s ? 'high' : 'normal';

// Same estimator as calibration.rs / hydrology.rs: Gumbel L-moments, not arbitrary thresholds.
export function thresholdsFor(entry) {
  if (!entry?.accepted || !entry.cell || entry.annual_max_m3s.length < 20) return null;
  const sample = [...entry.annual_max_m3s].sort((a, b) => a - b);
  const n = sample.length;
  const b0 = sample.reduce((sum, value) => sum + value, 0) / n;
  const b1 = sample.reduce((sum, value, index) => sum + index / (n - 1) * value, 0) / n;
  const alpha = (2 * b1 - b0) / Math.LN2;
  assert.ok(alpha > 0, `Invalid calibration for ${entry.id}`);
  const xi = b0 - 0.5772156649015329 * alpha;
  const quantile = (years) => round(xi - alpha * Math.log(-Math.log(1 - 1 / years)));
  return {
    q2_m3s: quantile(2), q5_m3s: quantile(5), q20_m3s: quantile(20),
    p90_m3s: round(entry.p90_m3s), p99_m3s: round(entry.p99_m3s),
    period: entry.period, n_years: n,
  };
}
const riverCal = new Map(calibration.stations.map((entry) => [entry.id, entry]));
const robCal = new Map(robCalibration.points.map((entry) => [entry.id, entry]));
const history = (id) => exposure.points[id] ? { radius_km: exposure.radius_km, ...exposure.points[id] } : null;

function distance(a, b) {
  const radians = (value) => value * Math.PI / 180;
  const dlat = radians(b.lat - a.lat);
  const dlon = radians(b.lon - a.lon);
  return 12742 * Math.asin(Math.sqrt(Math.sin(dlat / 2) ** 2
    + Math.cos(radians(a.lat)) * Math.cos(radians(b.lat)) * Math.sin(dlon / 2) ** 2));
}

// Profiles deliberately exercise independent river/rain/report/rob signals. The series below
// are analytic curves, not observations or downloaded gauge time series.
const profile = {
  'citarum-karawang': 'extreme', 'ciliwung-manggarai': 'very_high',
  'bekasi-pondokgede': 'high', 'solo-bojonegoro': 'high', 'ciliwung-katulampa': 'rising',
};
const rainfall = { 'ciliwung-manggarai': 110, 'bekasi-pondokgede': 65, 'deli-medan': 55, 'barito-banjarmasin': 75 };

function stationTelemetry(station, index, empty, reports) {
  const cal = riverCal.get(station.id);
  assert.ok(cal, `Catalog station lacks calibration entry: ${station.id}`);
  const threshold = thresholdsFor(cal);
  const base = threshold ? threshold.q2_m3s * (0.30 + (index % 4) * 0.035) : Math.max(1, cal.expected_q_m3s * 0.25);
  const kind = empty ? 'normal' : profile[station.id] || 'normal';
  const target = !threshold ? base : ({
    extreme: threshold.q20_m3s * 1.12, very_high: (threshold.q5_m3s + threshold.q20_m3s) / 2,
    high: (threshold.q2_m3s + threshold.q5_m3s) / 2,
    rising: threshold.q2_m3s * 0.60, normal: base,
  })[kind];
  const peak = !threshold ? target * 1.10 : kind === 'rising' ? threshold.q5_m3s * 1.05
    : kind === 'normal' ? base * 1.15 : target * 1.12;
  const rain24 = empty ? 4 : rainfall[station.id] || 8 + index % 12;
  const nextRain = empty ? 3 : rain24 * 0.7;
  const hydrograph = Array.from({ length: 22 }, (_, i) => {
    const offset = i - 14;
    const q = offset <= 0 ? base + (target - base) * Math.exp(offset / 3)
      : offset <= 2 ? target + (peak - target) * offset / 2 : base + (peak - base) * Math.exp(-(offset - 2) / 3);
    const forecast = offset > 0;
    const spread = 0.12 + Math.max(0, offset) * 0.025;
    return {
      date: day(offset), is_forecast: forecast, discharge_m3s: round(q),
      discharge_min_m3s: forecast ? round(q * (1 - spread)) : null,
      discharge_p25_m3s: forecast ? round(q * (1 - spread / 3)) : null,
      discharge_p75_m3s: forecast ? round(q * (1 + spread / 3)) : null,
      discharge_max_m3s: forecast ? round(q * (1 + spread)) : null,
      rain_mm: round(offset === 0 ? rain24 : 3 + (index + i) % 17),
    };
  });
  const current = hydrograph[14].discharge_m3s;
  const forecastPeak = hydrograph.slice(14).reduce((a, b) => b.discharge_m3s > a.discharge_m3s ? b : a);
  const nearby = reports.filter((report) => ['confirmed', 'verified'].includes(report.status) && distance(station, report) <= 10).length;
  const fluvial = riverLevel(current, threshold);
  const pluvial = higher(higher(rainLevel(rain24), rainLevel(nextRain)), nearby > 0 ? 'high' : 'normal');
  const level = fluvial === 'unknown' && rank(pluvial) < 2 ? 'unknown' : higher(fluvial, pluvial);
  const exceed = (q) => Math.round(Math.min(1, Math.max(0, (forecastPeak.discharge_m3s / q - 0.7) / 0.6)) * 51) / 51;
  return {
    ...station, glofas_cell: cal.cell, thresholds: threshold, current_discharge_m3s: current,
    ratio_q2: threshold ? round(current / threshold.q2_m3s) : null,
    peak_forecast_m3s: forecastPeak.discharge_m3s, peak_forecast_date: forecastPeak.date,
    peak_forecast_level: riverLevel(forecastPeak.discharge_m3s, threshold),
    exceed_prob_7d: threshold ? { q2: exceed(threshold.q2_m3s), q5: exceed(threshold.q5_m3s), q20: exceed(threshold.q20_m3s), members: 51 } : null,
    trend: 'naik', rain_24h_mm: rain24, rain_72h_mm: rain24 * 2, rain_next_24h_mm: round(nextRain),
    soil_moisture_top: 0.38, soil_moisture_deep: 0.41, nearby_reports_24h: nearby,
    fluvial_level: fluvial, pluvial_level: pluvial, level,
    mechanism: rank(pluvial) > rank(fluvial) ? 'urban_pluvial' : 'fluvial_basin',
    level_reason: `${MARKER}. Debit dan hujan buatan untuk menguji tampilan; bukan kondisi sungai.`,
    public_advice: `${MARKER}. Ikuti informasi resmi BPBD dan BMKG, bukan angka contoh ini.`,
    gov_action: `${MARKER}. Contoh tampilan untuk BPBD/Pemda, bukan instruksi penanganan.`,
    academic_note: `${MARKER}. Deret debit, ensemble dan hujan dibuat secara analitik; kalibrasi/pembanding statis mengikuti atribusi repositori.`,
    hydrograph,
    grrr_check: threshold ? grrr.stations[station.id] || null : null,
    gauge_check: threshold ? gauges.stations[station.id] || null : null,
    inundation_history: history(station.id),
  };
}

const counts = (records, key) => records.reduce((out, record) => {
  if (Object.hasOwn(out, record[key])) out[record[key]]++;
  return out;
}, { high: 0, very_high: 0, extreme: 0, unknown: 0 });

export function buildSnapshot(scenario = 'mixed') {
  assert.ok(['mixed', 'empty'].includes(scenario), `Unknown synthetic scenario ${scenario}`);
  const empty = scenario === 'empty';
  const reports = empty ? [] : [
    { pkey: 'synthetic-confirmed', created_at: ago(90), source: 'Sintetis (bukan PetaBencana)', status: 'confirmed', depth_cm: 40, title: `${MARKER}: laporan contoh 1`, text: 'Tidak ada kejadian nyata yang dilaporkan.', lat: -6.208, lon: 106.849, disaster_type: 'flood', url: null },
    { pkey: 'synthetic-verified', created_at: ago(180), source: 'Sintetis (bukan PetaBencana)', status: 'verified', depth_cm: null, title: `${MARKER}: laporan contoh 2`, text: '', lat: -6.215, lon: 106.855, disaster_type: 'flood', url: null },
    { pkey: 'synthetic-unconfirmed', created_at: ago(240), source: 'Sintetis (bukan PetaBencana)', status: 'unconfirmed', depth_cm: 10, title: `${MARKER}: belum terkonfirmasi`, text: 'Tidak dihitung untuk menaikkan level drainase.', lat: 3.588, lon: 98.678, disaster_type: 'flood', url: null },
  ];
  const stations = stationsCatalog.map((station, index) => stationTelemetry(station, index, empty, reports));
  const robs = coastalCatalog.map((rob, index) => {
    const threshold = robCal.get(rob.id)?.thresholds || null;
    const peak = !threshold ? 0.2 : !empty && index === 3 ? threshold.max_m + 0.08
      : !empty && index < 2 ? (threshold.p99_m + threshold.max_m) / 2 : threshold.p95_m * 0.65;
    return {
      ...rob, current_msl_m: round(peak - 0.15), peak_msl_48h_m: round(peak),
      peak_msl_time: '2026-11-23T06:00:00Z', wave_height_m: 0.8, thresholds: threshold,
      level: !threshold ? 'unknown' : peak >= threshold.max_m ? 'very_high' : peak >= threshold.p99_m ? 'high' : 'normal',
      level_reason: `${MARKER}. Muka laut buatan, bukan observasi.`,
      note: `${MARKER}. Untuk uji antarmuka saja.`, inundation_history: history(rob.id),
    };
  });
  const nowcasts = empty ? [] : ['DKI Jakarta', 'Jawa Barat'].map((province) => ({
    title: `${MARKER}: cuaca contoh ${province}`, province, summary: `${MARKER}, bukan peringatan BMKG.`,
    link: 'https://example.org/synthetic-weather', pub_date: ago(30),
  }));
  const max = stations.filter((station) => station.ratio_q2 !== null)
    .reduce((a, b) => !a || b.ratio_q2 > a.ratio_q2 ? b : a, null);
  const snapshot = {
    schema_version: 2, methodology_version: '2026-11-22',
    generated_at_utc: FIXTURE_TIME, is_historical_replay: false,
    replay_id: null, replay_label: null, replay_citation: null, replay_note: null,
    kpis: {
      river: counts(stations, 'fluvial_level'), pluvial: counts(stations, 'pluvial_level'), rob: counts(robs, 'level'),
      citizen_flood_reports_24h: reports.length, bmkg_nowcasts: nowcasts.length,
      max_ratio_q2: max?.ratio_q2 ?? null,
      // Mirrors hydrology.rs::compute_kpis (currently serializes the station name).
      max_ratio_station: max?.name ?? null, uncalibrated: stations.filter((station) => !station.thresholds).length,
    },
    upstream: {
      glofas_ok: true, weather_ok: true, marine_ok: true, petabencana_ok: true, bmkg_nowcast_ok: true, news_ok: true,
      notes: [`${MARKER}. Tidak ada ingest/akses API produksi; statistik pembanding statis memakai lisensi aslinya.`],
      glofas_fetched_at_utc: ago(180), weather_fetched_at_utc: ago(20), marine_fetched_at_utc: ago(40),
      petabencana_fetched_at_utc: ago(5), bmkg_fetched_at_utc: ago(5), news_fetched_at_utc: ago(5),
    },
    stations, coastal_rob: robs, citizen_reports: reports, bmkg_nowcasts: nowcasts,
    news: empty ? [] : [{ title: `${MARKER}: berita banjir contoh`, link: 'https://example.org/synthetic-news', source: 'Contoh sintetis', pub_date: ago(120), region_hint: 'Jawa' }],
  };
  return snapshot;
}
