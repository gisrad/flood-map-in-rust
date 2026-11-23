# Metodologi Hidrosentinel Nusantara

Versi metodologi: **2026-11-22** (`methodology_version` di setiap snapshot API).

> **Indikasi model, bukan peringatan resmi.** Peringatan dini resmi banjir berasal dari BNPB/BPBD, BMKG, dan Balai (Besar) Wilayah Sungai. Status resmi Waspada, Siaga, dan Awas tidak dipakai di situs ini. Pusdalops BNPB: 117.

## 1. Sumber data

| Sumber | Isi | Akses | Lisensi/atribusi | Diperbarui di situs |
|---|---|---|---|---|
| GloFAS v4 (Copernicus Emergency Management Service) | Debit sungai harian simulasi, grid 0,05°; reanalisis 1997–2024 dan prakiraan ensemble 30 hari | Open-Meteo Flood API | CC BY 4.0; “Copernicus Emergency Management Service, GloFAS” + Open-Meteo | tiap 6 jam |
| Model cuaca Open-Meteo (`best_match`) | Hujan per jam, kelembapan tanah 0–7 cm dan 28–100 cm | Open-Meteo Forecast API | CC BY 4.0, Open-Meteo dan penyedia model | tiap 1 jam |
| ERA5 (Copernicus Climate Change Service) | Hujan harian dan kelembapan tanah untuk replay | Open-Meteo Archive API | CC BY 4.0 | dibakukan |
| MeteoFrance SMOC (Copernicus Marine Service) | Muka laut model per jam `sea_level_height_msl` (pasang surut, efek barometrik terbalik, tinggi muka laut), tinggi gelombang | Open-Meteo Marine API | Atribusi E.U. Copernicus Marine Service + Open-Meteo | tiap 1 jam |
| PetaBencana.id | Laporan banjir warga 24 jam (`disaster=flood`); hanya status `confirmed`/`verified` yang dihitung untuk menaikkan level | `data.petabencana.id/reports` | Atribusi PetaBencana.id (lihat info.petabencana.id/data) | tiap 15 menit |
| BMKG | Nowcast cuaca ekstrem (RSS CAP) | `bmkg.go.id/alerts/nowcast/id` | Atribusi BMKG | tiap 15 menit |
| Media daring | Judul berita banjir (CNN Indonesia, Tempo, Republika, Antara, detikNews) | RSS resmi tiap media | Hak cipta media; hanya judul dan tautan | tiap 15 menit |

Pemicu cron Worker berjalan tiap 15 menit. Setiap sumber hanya diambil bila jatuh tempo; waktu ambil terakhir dicatat di `upstream.*_fetched_at_utc` dan `/api/health`. Sumber yang gagal dicoba ulang paling cepat 15 menit kemudian; data lama dipakai selama masih dalam batas umur (GloFAS 18 jam, cuaca 4 jam, muka laut 6 jam, laporan warga 2 jam, nowcast 3 jam, berita 6 jam), setelah itu dianggap tidak tersedia.

Kuota Open-Meteo gratis dihitung berbobot: bobot per lokasi = max(1, V/10 × max(1, D/14)), V = jumlah variabel (dikali jumlah anggota ensemble), D = jumlah hari. Permintaan rutin Worker per hari ≈ 4 × (50 + 50 × 5,1) GloFAS + 24 × 50 cuaca + 24 × 12 muka laut ≈ 2.700 panggilan berbobot.

## 2. Indikator sungai (banjir kiriman hulu)

### 2.1 Pemilihan sel GloFAS
Sel 5 km GloFAS sering tidak jatuh di alur sungai yang dimaksud (dokumentasi Open-Meteo: “Due to the 5 km resolution the closest river might not be selected correctly”). Skrip `scripts/calibrate_glofas.py` memilih sel per stasiun:

1. Pindai 25 titik kisi 0,05° dalam radius ±0,10° di sekitar koordinat pos; hapus duplikat sel.
2. Hitung rerata debit tiap sel pada jendela 1 Jan–20 Mei 2018 (140 hari).
3. Debit harapan Q_harapan = luas DAS katalog × debit spesifik asumsi per pulau: Jawa 30, Sumatera 40, Kalimantan 45, Sulawesi 35, Bali–Nusa Tenggara 15, Maluku 40, Papua 60 L/s/km² (orde besaran hujan tahunan dikurangi evapotranspirasi; 1 L/s/km² ≈ 31,5 mm/tahun). Asumsi ini hanya penyaring besaran.
4. Skor = |ln(Q_sel / Q_harapan)| + jarak_km / 50. Sel dengan skor terkecil dipilih.
5. Sel **diterima** bila rerata debit 1997–2024 pada sel itu berada dalam rentang Q_harapan/3 … 3 × Q_harapan dan tersedia ≥ 20 maksimum tahunan. Stasiun yang selnya tidak diterima diberi `thresholds = null`, `glofas_cell = null`, dan level sungai **`unknown`**; datanya tetap diambil di koordinat pos tetapi tidak dinilai.

Koordinat sel (`glofas_cell.lat/lon`) dipakai untuk mengambil data; koordinat pos tetap dipakai untuk peta dan jarak ke laporan warga.

### 2.2 Ambang klimatologis
Pada sel terpilih diunduh debit harian GloFAS v4 reanalisis (`models=consolidated_v4`) **1 Jan 1997 – 31 Des 2024**. Data GloFAS v4 di Open-Meteo dimulai 1 Januari 1997 (permintaan 1984–1996 mengembalikan nilai kosong), sehingga periode 1991–2020 tidak dapat dipakai.

- p90, p99: persentil debit harian seluruh periode.
- Maksimum tahunan: debit harian maksimum tiap tahun kalender dengan ≥ 330 hari valid.
- Distribusi Gumbel (nilai ekstrem tipe I), parameter dengan metode L-moment (Hosking, 1990):
  λ₁ = rerata, λ₂ = 2b₁ − b₀, α = λ₂ / ln 2, ξ = λ₁ − 0,5772 α.
- Debit periode ulang T tahun: Q_T = ξ − α ln(−ln(1 − 1/T)), untuk T = 2, 5, 20.

Q2/Q5/Q20 dihitung `flood-shared` (`calibration.rs`) dari deret maksimum tahunan di `crates/flood-shared/data/glofas_calibration.json`, sehingga angka di API dapat diulang dari data mentah.

### 2.3 Level sungai
Debit hari ini (hari GMT, `river_discharge` Open-Meteo) dibandingkan dengan ambang sel yang sama:

| Level | Syarat |
|---|---|
| Normal | Q < Q2 |
| Tinggi (`high`) | Q2 ≤ Q < Q5 |
| Sangat tinggi (`very_high`) | Q5 ≤ Q < Q20 |
| Ekstrem (`extreme`) | Q ≥ Q20 |
| Belum terkalibrasi (`unknown`) | sel tidak diterima atau data debit kosong |

`peak_forecast_level` memakai puncak prakiraan 7 hari dengan aturan yang sama. `exceed_prob_7d` = fraksi anggota ensemble GloFAS (Open-Meteo `ensemble=true`) yang puncak 7 harinya melampaui Q2, Q5, dan Q20. `ratio_q2` = Q hari ini / Q2.

`trend` adalah kecenderungan relatif debit simulasi satu hari, bukan ambang bahaya dan tidak ikut menentukan level. Dengan Q kemarin, hari ini, dan besok (dasar penyebut minimal 1 m³/s): `naik_tajam` (label tampilan "Naik cepat") bila kenaikan ke hari ini ≥ 25% atau ke besok ≥ 30%; `naik` bila ≥ 8% atau ≥ 10%; `surut` bila turun ≥ 10% ke hari ini dan ≥ 5% lagi ke besok; selain itu `stabil`. Batas-batas ini ditetapkan sebagai konvensi pelaporan, bukan hasil kalibrasi. Teks publik hanya menyebut tren bila Q ≥ 0,5 Q2, karena perubahan relatif pada debit kecil tidak berarti.

Stasiun `unknown` (sel belum terkalibrasi): debit mutlak sel tidak ditampilkan di antarmuka karena bukan debit sungai di pos itu; nilainya tetap ada di API (`current_discharge_m3s`) dengan `thresholds = null`. Bila jarak sel–pos `glofas_cell.offset_km` > 10 km, detail stasiun menyebutnya.

## 3. Indikator drainase (genangan hujan lokal)
Kategori intensitas hujan harian BMKG (Kepala BMKG, dikutip SINDOnews 28 Des 2022): lebat 50–100 mm, sangat lebat 100–150 mm, ekstrem > 150 mm per 24 jam.

| Level | Hujan model 24 jam terakhir **atau** 24 jam ke depan |
|---|---|
| Tinggi | ≥ 50 mm |
| Sangat tinggi | ≥ 100 mm |
| Ekstrem | > 150 mm |

Hujan 24 jam = jumlah presipitasi per jam model Open-Meteo di koordinat pos. Bila ada ≥ 1 laporan banjir warga PetaBencana berstatus `confirmed` atau `verified` dalam radius 10 km selama 24 jam terakhir, level drainase minimal Tinggi (`nearby_reports_24h` menghitung laporan semacam itu). Laporan belum dikonfirmasi tetap tampil di peta tetapi tidak menaikkan level.

## 4. Level stasiun dan mekanisme
`level = max(level sungai, level drainase)`, dengan satu pengecualian: bila level sungai `unknown` dan level drainase di bawah Tinggi, level stasiun tetap `unknown` (tidak ditampilkan sebagai Normal). `mechanism` = komponen penentu (`fluvial_basin` bila sungai ≥ drainase, selain itu `urban_pluvial`).

## 5. Indikator rob
Skrip `scripts/calibrate_rob.py` mengunduh muka laut model per jam (`sea_level_height_msl`, tersedia sejak Januari 2022) di sel laut terdekat tiap titik rob untuk **2022–2025** (26.808 jam), membentuk deret **puncak 48 jam** (maksimum jendela 48 jam yang bergeser per jam, setara dengan besaran yang dinilai saat operasi), lalu menghitung p95, p99, dan maksimum deret itu. Ambang per jam tidak dipakai karena puncak 48 jam ke depan melampaui p99 per jam pada 10–18% hari, yakni pasang purnama rutin. Bila sel terdekat tidak berdata, dicari sel laut berdata pada cincin 0,1° dan 0,2°.

| Level | Puncak muka laut model 48 jam ke depan |
|---|---|
| Normal | < p99 |
| Tinggi | ≥ p99 puncak 48 jam (±1% jendela pada periode kalibrasi) |
| Sangat tinggi | ≥ maksimum periode kalibrasi |
| Belum terkalibrasi | tidak ada ambang atau data |

Rob tidak diberi level Ekstrem: model global tidak memuat amblesan tanah, tanggul, atau limpasan lokal sehingga besaran genangan tidak dapat dinilai. Faktor kerentanan amblesan dan `trigger_msl_m` versi lama (diketik tangan) dihapus.

## 6. Replay kejadian
`scripts/bake_replays.py` membakukan tiga kejadian ke `crates/flood-shared/data/replays/*.json`: debit GloFAS v4 reanalisis pada sel terkalibrasi, hujan harian dan kelembapan tanah ERA5, serta muka laut model (hanya 2024). Satu bingkai = satu hari GMT; level dihitung dengan aturan yang sama seperti data langsung, tanpa prakiraan dan tanpa laporan warga. Puncak muka laut 48 jam pada replay = maksimum harian hari itu dan hari berikutnya. Narasi bingkai hanya kutipan bersumber (tautan di `citation`) atau ringkasan angka data; debit stasiun kunci yang selnya belum terkalibrasi tidak dikutip.

Setiap replay menampilkan blok **hasil model vs kejadian** yang dihitung `flood-shared/src/replay.rs` dari data yang sama: berapa stasiun kunci (`focus`) yang naik ke Tinggi atau lebih, level maksimum dan tanggalnya per stasiun, level stasiun kunci dalam ±1 hari dari tanggal kejadian bersumber (`observed`), dan pembanding hujan ERA5 terhadap hujan teramati bila sumber menyebutkannya. Pada banjir Jabodetabek 2020, hujan ERA5 harian tertinggi di stasiun kunci hanya sekitar 14% dari 377 mm/hari teramati di Halim, sehingga model tidak menaikkan Ciliwung–Manggarai. Replay menguji model, bukan merekonstruksi kejadian. Laporan warga, nowcast BMKG, dan berita tidak dibakukan untuk replay, sehingga panel tidak menampilkannya.

| Skenario | Fakta bersumber |
|---|---|
| Jabodetabek, 1 Jan 2020 | Kepala BNPB: TNI AU Halim 377 mm/hari, Taman Mini 335 mm/hari, Jatiasih 259 mm/hari (DW, 2 Jan 2020). Versi lama menyebut Kemayoran; itu keliru. |
| Kalimantan Selatan, Jan 2021 | Banjir mulai 12 Jan; status tanggap darurat provinsi 14 Jan; 10 kabupaten/kota terdampak (BNPB, 17 Jan 2021). |
| Demak, Feb–Mar 2024 | Tanggul Sungai Wulan jebol, jalur pantura Demak–Kudus terputus (Portal Kabupaten Demak, 9 Feb 2024); tanggul jebol lagi 17 Mar (BNPB, 18 Mar 2024). |

## 7. Laporan warga dan berita
- PetaBencana: hanya `disaster_type = flood` (parameter `disaster=flood` dan disaring ulang di kode). Laporan kebakaran, gempa, angin, kabut asap, dan gunung api ditolak. Kedalaman genangan (`report_data.flood_depth`, cm) disimpan bila ada; `url` berisi tautan foto laporan.
- Berita: judul harus memuat banjir (bukan kiasan seperti “banjir pesanan”), banjir bandang, rob, genangan/tergenang, luapan/meluap, atau tanggul jebol; umur ≤ 72 jam; deduplikasi judul dan tautan; `pub_date` ISO 8601 UTC. Tanpa cadangan: daftar kosong berarti tidak ada berita banjir yang memenuhi syarat.

## 8. Keterbatasan
1. Debit adalah **simulasi** model hidrologi global (LISFLOOD dalam GloFAS), bukan pengukuran pos duga air. Bias lokal dapat besar, terutama untuk DAS kecil (< 1.000 km²), sungai berbendungan, delta bercabang, dan wilayah pasang surut.
2. Ambang Q2/Q5/Q20 berasal dari model yang sama sehingga bias sistematis sebagian saling meniadakan, tetapi periode ulang model tidak sama dengan periode ulang debit terukur. Prakiraan debit diambil dari produk prakiraan GloFAS, sedangkan ambang dari reanalisis; perbedaan pemaksa iklim dapat menggeser level.
3. Debit spesifik asumsi dan toleransi ×3 adalah keputusan penyaring, bukan kalibrasi terhadap pengukuran.
4. Hujan model 24 jam untuk satu titik tidak mewakili hujan konvektif lokal; kategori BMKG ditetapkan untuk pengamatan pos hujan.
5. Muka laut model hanya 4 tahun (2022–2025); maksimum periode itu bukan periode ulang. Datum adalah MSL global, bukan datum pasang surut lokal; akurasi di pesisir terbatas.
6. Waktu harian GloFAS mengikuti GMT (07.00 WIB).

## 9. Reproduksi
```
python scripts/calibrate_glofas.py --cache-dir <dir> --report kalibrasi.md
python scripts/calibrate_rob.py    --cache-dir <dir> --report rob.md
python scripts/bake_replays.py     --cache-dir <dir>
cargo test -p flood-shared
```
Skrip memakai Python 3 + NumPy, menyimpan setiap respons API di `<dir>`, mencatat bobot panggilan di `<dir>/ledger.jsonl`, dan menahan diri di bawah 3.500 panggilan berbobot per jam (jeda ≥ 1,1 detik).

## Rujukan
- Hosking, J. R. M. (1990). L-moments: analysis and estimation of distributions using linear combinations of order statistics. *Journal of the Royal Statistical Society B*, 52(1), 105–124.
- Open-Meteo Flood API: https://open-meteo.com/en/docs/flood-api
- Open-Meteo Marine API: https://open-meteo.com/en/docs/marine-weather-api
- Open-Meteo, batas kuota: https://github.com/open-meteo/open-meteo/blob/main/docs/api-rate-limiting.md
- PetaBencana API: https://docs.petabencana.id/master-1/routes/crowdsourced-reports
- Kategori hujan BMKG: https://nasional.sindonews.com/read/981399/15/ini-penjelasan-bmkg-soal-perbedaan-hujan-lebat-sangat-lebat-dan-ekstrem-1672215134
