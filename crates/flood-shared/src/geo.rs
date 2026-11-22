//! GeoJSON titik untuk `/api/stations.geojson` (dipakai juga tombol "Buka di GeoLibre").

use crate::model::{FloodMechanism, FloodSnapshot};
use serde_json::{json, Value};

pub fn build_stations_geojson(snap: &FloodSnapshot) -> Value {
    let updated = &snap.generated_at_utc;
    let mut features = Vec::with_capacity(
        snap.stations.len() + snap.coastal_rob.len() + snap.citizen_reports.len(),
    );

    for s in &snap.stations {
        features.push(json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [s.lon, s.lat] },
            "properties": {
                "kind": "river",
                "id": s.id,
                "name": s.name,
                "river": s.river,
                "province": s.province,
                "level": s.level,
                "level_label": s.level.label_id(),
                "mechanism": s.mechanism,
                "fluvial_level": s.fluvial_level,
                "pluvial_level": s.pluvial_level,
                "discharge_m3s": s.current_discharge_m3s,
                "ratio_q2": s.ratio_q2,
                "q2_m3s": s.thresholds.as_ref().map(|t| t.q2_m3s),
                "peak_forecast_m3s": s.peak_forecast_m3s,
                "peak_forecast_date": s.peak_forecast_date,
                "rain_24h_mm": s.rain_24h_mm,
                "rain_next_24h_mm": s.rain_next_24h_mm,
                "calibrated": s.thresholds.is_some(),
                "level_reason": s.level_reason,
                "updated_at_utc": updated,
            }
        }));
    }
    for r in &snap.coastal_rob {
        features.push(json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [r.lon, r.lat] },
            "properties": {
                "kind": "rob",
                "id": r.id,
                "name": r.name,
                "province": r.province,
                "level": r.level,
                "level_label": r.level.label_id(),
                "mechanism": FloodMechanism::CoastalRob,
                "peak_msl_48h_m": r.peak_msl_48h_m,
                "p99_m": r.thresholds.as_ref().map(|t| t.p99_m),
                "wave_height_m": r.wave_height_m,
                "level_reason": r.level_reason,
                "updated_at_utc": updated,
            }
        }));
    }
    for c in &snap.citizen_reports {
        features.push(json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [c.lon, c.lat] },
            "properties": {
                "kind": "report",
                "id": c.pkey,
                "name": c.title,
                "level": null,
                "level_label": "Laporan warga",
                "mechanism": null,
                "depth_cm": c.depth_cm,
                "source": c.source,
                "status": c.status,
                "url": c.url,
                "created_at": c.created_at,
                "updated_at_utc": updated,
            }
        }));
    }

    json!({
        "type": "FeatureCollection",
        "name": "hidrosentinel-nusantara",
        "metadata": {
            "generated_at_utc": updated,
            "schema_version": snap.schema_version,
            "methodology_version": snap.methodology_version,
            "is_historical_replay": snap.is_historical_replay,
            "disclaimer": "Indikasi model, bukan peringatan resmi.",
            "attribution": "GloFAS/ERA5 (Copernicus) via Open-Meteo; Copernicus Marine via Open-Meteo; laporan warga PetaBencana.id",
        },
        "features": features,
    })
}
