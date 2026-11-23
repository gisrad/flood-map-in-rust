# Third-Party Notices & Data Sources

## Software Libraries
- **Rust / Cargo Crates**: `leptos` (MIT), `worker` / `workers-rs` (Apache-2.0 / MIT), `serde` / `serde_json` (MIT / Apache-2.0), `wasm-bindgen` / `web-sys` / `js-sys` (MIT / Apache-2.0), `gloo-net` / `gloo-timers` (MIT / Apache-2.0).
- **MapLibre GL JS v5**: BSD-3-Clause (`crates/flood-web/vendor/LICENSE.txt`).
- **Schibsted Grotesk**: SIL Open Font License 1.1 (`crates/flood-web/assets/fonts/OFL.txt`).

## Open Data & Map Basemaps
- **Copernicus GloFAS v4 via Open-Meteo Flood API**: CC BY 4.0 (CEMS / ECMWF / Open-Meteo). Reanalisis `consolidated_v4` 1997–2024 dipakai untuk kalibrasi ambang dan replay.
- **Open-Meteo Weather & Marine APIs**: CC BY 4.0 (Open-Meteo / ECMWF IFS / ERA5 reanalysis, Copernicus Climate Change Service).
- **PetaBencana.id**: Open flood citizen-report GeoJSON/TopoJSON API.
- **BMKG Nowcast CAP RSS**: Badan Meteorologi, Klimatologi, dan Geofisika (open alerts).
- **Terrain Tiles (Terrarium DEM)**: AWS Open Data Registry / Mapzen. Sumber: Global ETOPO1 (NOAA); USGS 3DEP/NED, GMTED2010, SRTM; sumber regional sesuai https://github.com/tilezen/joerd/blob/master/docs/attribution.md.
- **Google GEM-15 DTM (15 m)**: CC BY 4.0. "This dataset is produced by Google." Dipakai untuk render relief replay dan tautan unduhan DTM per stasiun.
- **Google Inundation History (1999–2020)**: Google Research, CC BY 4.0, dibuat dari data GLAD (Global Land Analysis & Discovery, University of Maryland). Poligon disederhanakan dan dipotong ke Indonesia di `crates/flood-web/public/data/inundation/` (`scripts/bake_inundation.py`); angka paparan per titik di `crates/flood-shared/data/inundation_exposure.json`. Sumber: `gs://flood-forecasting/inundation_history`.
- **Google Runoff Reanalysis & Reforecast (GRRR), 1980–2023**: Google Research, CC BY 4.0. Hanya rentang Q2 dan debit periode ulang titik keluar sub-DAS dalam radius pos yang diringkas di `crates/flood-shared/data/grrr_crosscheck.json` (`scripts/crosscheck_grrr.py`). Sumber: `gs://flood-forecasting/hydrologic_predictions/distributed_model_2026_11`.
- **OpenStreetMap**: © OpenStreetMap contributors, ODbL 1.0. Geometri sungai di `crates/flood-web/public/data/rivers*.geojson` adalah turunan OSM (ODbL).
- **OpenFreeMap Vector Styles (Positron & Dark)**: OpenStreetMap contributors / OpenFreeMap.
- **ntfy.sh**: Free open-source pub-sub push notifications (Apache-2.0 / GPL-2.0).
- **Google Flood Hub**: hanya tautan keluar untuk perbandingan; tidak ada data Flood Hub yang disalin.
