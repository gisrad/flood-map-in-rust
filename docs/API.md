# API Hidrosentinel Nusantara (schema v2)

Basis: `https://flood.transbulent.id`. Endpoint baca (`GET`/`HEAD`: `/api/flood`, `/api/stations.geojson`, `/api/station/{id}`, `/api/health`, `/api/openapi.json`, `/api/ai/*`, `/api/alerts.rss`) memuat `Access-Control-Allow-Origin: *`; `POST /api/alerts/subscribe` tidak. Isi API adalah **indikasi model, bukan peringatan resmi**. Metode: [METODOLOGI.md](METODOLOGI.md). Spesifikasi mesin: `/api/openapi.json`; ringkasan untuk agen AI: `/llms.txt`.

| Metode | Path | Isi | Cache |
|---|---|---|---|
| GET | `/api/flood` | `FloodSnapshot` nasional | 120 s |
| GET | `/api/stations.geojson` | FeatureCollection titik sungai, rob, laporan warga | 300 s |
| GET | `/api/station/{id}` | satu `StationTelemetry` atau `CoastalRobTelemetry` | – |
| GET | `/api/ai/briefing?region=&audience=public\|gov\|academia\|ai_agent&format=json\|md` | ringkasan deterministik | – |
| GET | `/api/ai/query?q=` | pencarian stasiun/rob + target `fly_to` | – |
| GET | `/api/alerts.rss?region=` | RSS 2.0 level Tinggi ke atas + nowcast BMKG | 180 s |
| POST | `/api/alerts/subscribe` | langganan ntfy.sh | – |
| GET | `/api/health` | umur snapshot dan waktu ambil tiap sumber | – |

## Level
`level`: `unknown` (Belum terkalibrasi) · `normal` · `high` (Tinggi) · `very_high` (Sangat tinggi) · `extreme` (Ekstrem).
`mechanism`: `fluvial_basin` · `urban_pluvial` · `coastal_rob`.

## `FloodSnapshot`
```text
schema_version = 2, methodology_version = "2026-11-22", generated_at_utc (ISO 8601 UTC),
is_historical_replay, replay_id?, replay_label?, replay_citation?, replay_note?,
kpis { river|pluvial|rob: { high, very_high, extreme, unknown }, citizen_flood_reports_24h,
       bmkg_nowcasts, max_ratio_q2?, max_ratio_station?, uncalibrated },
upstream { glofas_ok, weather_ok, marine_ok, petabencana_ok, bmkg_nowcast_ok, news_ok, notes[],
           glofas_fetched_at_utc?, weather_fetched_at_utc?, marine_fetched_at_utc?,
           petabencana_fetched_at_utc?, bmkg_fetched_at_utc?, news_fetched_at_utc? },
stations[], coastal_rob[], citizen_reports[], bmkg_nowcasts[], news[]
```
Field opsional (`?`) selalu ada dan bernilai `null` bila tidak tersedia.

### `stations[]`
`id, name, river, ws_code, province, island, lat, lon` (koordinat pos), `tz_offset_hours, basin_area_km2`,
`glofas_cell? { lat, lon, offset_km, mean_q_m3s }`,
`thresholds? { q2_m3s, q5_m3s, q20_m3s, p90_m3s, p99_m3s, period, n_years }`,
`current_discharge_m3s, ratio_q2?, peak_forecast_m3s, peak_forecast_date, peak_forecast_level`,
`exceed_prob_7d? { q2, q5, q20, members }` (fraksi 0..1),
`trend` (`naik_tajam|naik|stabil|surut`; kecenderungan relatif 1 hari, label "Naik cepat" untuk `naik_tajam`, lihat METODOLOGI §2.3), `rain_24h_mm, rain_72h_mm, rain_next_24h_mm, soil_moisture_top, soil_moisture_deep, nearby_reports_24h` (laporan warga `confirmed`/`verified` ≤ 10 km, 24 jam),
`fluvial_level, pluvial_level, level, mechanism, level_reason, public_advice, gov_action, academic_note`,
`hydrograph[] { date, is_forecast, discharge_m3s, discharge_min_m3s?, discharge_p25_m3s?, discharge_p75_m3s?, discharge_max_m3s?, rain_mm? }` — 14 hari lalu, hari ini, 7 hari prakiraan (GMT). Statistik ensemble hanya untuk hari prakiraan,
`grrr_check? { radius_km, n_outlets, verdict, q2_min_m3s?, q2_max_m3s?, closest? { id, distance_km, q2_m3s, q5_m3s, q20_m3s }, ratio_q2? }` — uji silang Q2 GloFAS terhadap Google GRRR (`verdict`: `within_range` | `glofas_above` | `glofas_below` | `no_outlets`; `null` untuk stasiun belum terkalibrasi; model terhadap model, lihat METODOLOGI §10.2),
`gauge_check? { grdc_no, gauge, distance_km?, gauge_area_km2, gauge_mean_m3s, gauge_years, area_ratio, scaled_mean_m3s, cell_mean_m3s, ratio, verdict }` — rerata debit sel GloFAS 1997–2024 dibanding rerata terukur pos duga air GRDC di sungai yang sama, diskalakan ke luas DAS katalog (`verdict`: `consistent` | `glofas_high` | `glofas_low`, toleransi 1,5×; hanya 5 stasiun dengan pos yang mewakili; `distance_km` `null` bila koordinat GRDC tidak andal; METODOLOGI §11),
`gauge_check.flood? { gsim_no, years[], excluded_low_mean_years[], screened_gauge_q2_m3s?, max_missing_days, status, gauge_q2_m3s, gauge_q5_m3s, gauge_q20_m3s, scaled_q2_low_m3s, scaled_q2_m3s, scaled_q2_high_m3s, glofas_q2_m3s, glofas_q20_m3s, ratio_q2, verdict }` — uji pendahuluan Q2 dari maksimum harian tahunan pos GSIM/GRDC (3 pasangan; `null` bila tidak tersedia). `status=preliminary`; `consistent` berarti dalam toleransi penyaring, bukan bukti validasi operasional. Tahun debit rendah hanya dikeluarkan untuk `screened_gauge_q2_m3s`, bukan hasil utama. METODOLOGI §12; statistik adaptasi GSIM CC BY-SA 3.0,
`inundation_history? { radius_km, buffer_km2, low_km2, medium_km2, high_km2 }` — luas dalam radius 5 km yang basah ≥ 0,5% / ≥ 1% / ≥ 5% waktu menurut citra satelit 1999–2020 (Google Inundation History; kelas bersarang; riwayat, bukan prakiraan, METODOLOGI §10.1).

### `coastal_rob[]`
`id, name, province, island, lat, lon, tz_offset_hours, current_msl_m, peak_msl_48h_m, peak_msl_time` (ISO UTC), `wave_height_m` (maksimum 48 jam), `thresholds? { p95_m, p99_m, max_m, period }, level, level_reason, note, inundation_history?` (sama seperti pada stasiun).

### `citizen_reports[]`
`pkey, created_at, source, status, depth_cm?, title, text, lat, lon, disaster_type` (selalu `flood`), `url?`.

### `news[]`
`title, link` (selalu `https://`), `source, pub_date` (ISO 8601 UTC, ≤ 72 jam), `region_hint`.

## `/api/stations.geojson`
Titik `[lon, lat]`; `properties.kind` = `river` | `rob` | `report`; setiap fitur memuat `id, name, level, level_label, mechanism, updated_at_utc` dan metrik kunci (sungai: `discharge_m3s, ratio_q2, q2_m3s, peak_forecast_m3s, rain_24h_mm, calibrated`; rob: `peak_msl_48h_m, p99_m, wave_height_m`; laporan: `depth_cm, url, created_at`). Membuka di GeoLibre: `https://web.geolibre.app/?data=` + `encodeURIComponent("https://flood.transbulent.id/api/stations.geojson")`.

## Lapisan statis riwayat genangan (Google Inundation History 1999–2020)
Berkas GeoJSON statis (bukan bagian `/api/*`, tanpa CORS khusus, cache 1 hari): `/data/inundation/index.json` (metadata dan daftar ubin), `/data/inundation/overview.json` (seluruh Indonesia, disederhanakan), `/data/inundation/t/<lat>_<lon>.json` (ubin 1°, `<lat>_<lon>` = lantai lintang dan bujur sudut barat daya). Tiap fitur adalah MultiPolygon dengan `properties.c` = `low` (basah ≥ 0,5% waktu) | `medium` (≥ 1%) | `high` (≥ 5%); kelas bersarang. CC BY 4.0, Google Research, berdasarkan GLAD. Frekuensi permukaan basah pada citra satelit, bukan peta banjir; lihat METODOLOGI §10.1.

## `POST /api/alerts/subscribe`
```json
{ "topic": "banjir-semarang-utara", "region": "Jawa Tengah", "min_level": "high", "send_test": true }
```
- `Content-Type: application/json` wajib (`415` bila tidak).
- `topic`: `^[A-Za-z0-9_-]{8,64}$`, bukan deret angka (mencegah nomor telepon). Topik ntfy.sh bersifat publik; pakai nama yang sulit ditebak.
- `min_level`: `high` | `very_high` | `extreme` (bawaan `high`).
- Batas: 5 permintaan per jam per IP (disimpan sebagai hash SHA-256 IP + bulan, dihapus setelah 1 jam), 1.000 langganan total. Topik yang sudah terdaftar ditolak (`409`) karena langganan tidak berautentikasi dan tidak boleh ditimpa pihak lain.
- Respons: `200 { ok, topic, region, min_level, test_pushed, ntfy_url, rss_url }`, `400` topik tidak valid, `409` topik sudah terdaftar, `415` tipe konten, `429` batas IP, `503` kuota penuh.
- Notifikasi dikirim paling sering 6 jam sekali per topik, kecuali level tertinggi di wilayah naik sejak dorongan terakhir; paling banyak 5 dorongan per siklus 15 menit, level tertinggi dahulu. Setiap pesan memuat “Indikasi model, bukan peringatan resmi”.
