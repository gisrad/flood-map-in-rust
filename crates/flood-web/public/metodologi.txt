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

Peramban meminta snapshot tiap 5 menit dan saat koneksi kembali. Bila permintaan gagal, snapshot terakhir dipertahankan **dengan banner peringatan dan waktu aslinya**; tombol coba ulang tersedia. Banner juga muncul saat luring, umur snapshot ≥30 menit (dua siklus cron), atau ada `upstream.*_ok = false`. Umur snapshot berbeda dari umur tiap sumber: snapshot baru tidak berarti semua sumber berhasil diperbarui. Tabel sumber menampilkan waktu data terakhir yang **berhasil** diambil, bukan waktu upaya gagal. Banner live disembunyikan dalam replay historis. Ini bukan cache/PWA luring: pemuatan pertama tetap membutuhkan koneksi.

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
python scripts/bake_inundation.py  --cache-dir <dir>
python scripts/crosscheck_grrr.py  --cache-dir <dir>
python scripts/validate_gauges.py  --catalogue <GRDC_Stations.xlsx> --cache-dir <dir>
cargo test -p flood-shared
```
Skrip memakai Python 3 + NumPy, menyimpan setiap respons API di `<dir>`, mencatat bobot panggilan di `<dir>/ledger.jsonl`, dan menahan diri di bawah 3.500 panggilan berbobot per jam (jeda ≥ 1,1 detik).

## 10. Data pembanding Google (statis; tidak memengaruhi level)
Dua dataset publik Google Research (CC BY 4.0) dibakukan luring. Keduanya tidak dipakai menghitung level indikasi dan tidak diambil saat operasi.

### 10.1 Riwayat genangan (Inundation History, 1999–2020)
Sumber: `gs://flood-forecasting/inundation_history/data`. Dibuat tim banjir Google dari citra satelit GLAD: seberapa sering tiap piksel 128 m tampak basah selama 1999–2020, setelah derau dibuang dan hasil dihaluskan, **tanpa badan air permanen**. Tiga kelas bersarang: basah ≥ 5% waktu (`high`, di situs "Sering basah"), ≥ 1% (`medium`, "Kadang basah"), ≥ 0,5% (`low`, "Jarang basah"). Di luar poligon, piksel basah < 0,5% waktu.

`scripts/bake_inundation.py` memotong 99 sel sumber berisi poligon (≈ 77 MB termasuk sel laut kosong) ke Indonesia dan sekitarnya (94,5–141,2°BT, 11,5°LS–6,5°LU), menyederhanakan poligon (toleransi 0,002–0,015° menurut kelas dan tingkat zoom), membuang bagian kecil, lalu menulis `public/data/inundation/overview.json` (seluruh Indonesia, zoom < 8,5) dan ubin 1° `t/<lat>_<lon>.json` (zoom ≥ 8,5, dimuat sesuai tampilan setelah pengguna menyalakan lapisan). Penyederhanaan hanya untuk tampilan peta; kelas ≥ 0,5% paling kasar.

Paparan per titik (`inundation_history` di API untuk setiap stasiun dan titik rob) dihitung dari geometri asli sebelum penyederhanaan: luas tiap kelas dalam radius 5 km (78,5 km²) dalam proyeksi LAEA setempat. Kelas bersarang sehingga luas `low ≥ medium ≥ high`; antarmuka memaksakan urutan itu bila pembulatan menggesernya.

Cara membaca: ini **frekuensi permukaan basah pada citra satelit**, bukan peta banjir dan bukan peta rawan banjir resmi. Dapat mencakup sawah tergenang, tambak, waduk, dan dataran pasang surut. Data sumber sudah dihaluskan sehingga kelas ≥ 0,5% dan ≥ 1% mencakup hamparan dataran rendah yang luas (di sekitar Jakarta, seluruh wilayah dalam radius 5 km Ciliwung–Manggarai masuk kelas ini, 93%), dan bentuk poligon kasar. Contoh paparan tinggi yang tidak mencerminkan banjir sungai biasa: radius 5 km Cimanuk–Jatigede (80% kelas `high`; ada waduk) dan Wulan–Demak (78%; pesisir dan tambak, dugaan). Di kota padat, seperti Ciliwung–Manggarai (68% `high`), nilai tinggi perlu dibaca hati-hati; penyebabnya tidak dikaji di sini.

### 10.2 Uji silang ambang terhadap GRRR (Google Runoff Reanalysis & Reforecast)
Sumber: `gs://flood-forecasting/hydrologic_predictions/model_id_8583a5c2_v0`, yaitu GRRR yang tercantum di halaman sumber daya Google Flood Forecasting (debit global 1980–2023; sumbu waktu reanalisisnya 1 Jan 1980–23 Des 2023, dibaca dan dicatat skrip). Berkas: `return_periods.zarr` (debit periode ulang 2–200 tahun per titik keluar DAS HydroBASINS) dan `hybas_outlet_locations_UNOFFICIAL.zarr` (koordinat titik keluar). Folder `distributed_model_2026_11` di bucket yang sama adalah rilis lain (reanalisis 31 Des 1980–30 Des 2025) dan tidak dipakai. `scripts/crosscheck_grrr.py` untuk tiap stasiun terkalibrasi (44 stasiun):

1. Menghitung Q2/Q5/Q20 GloFAS dengan rumus yang sama seperti `flood-shared`.
2. Mengumpulkan titik keluar GRRR dalam radius 12 km dari koordinat pos.
3. Memberi **putusan** dari seluruh kandidat: `glofas_above` bila Q2 GloFAS > 2 × Q2 GRRR terbesar; `glofas_below` bila < 0,5 × Q2 GRRR terkecil; `within_range` selainnya; `no_outlets` bila tidak ada kandidat.
4. Mencatat kandidat dengan Q2 terdekat ke Q2 GloFAS (`closest`) beserta Q5, Q20, jarak, dan rasio Q2 GloFAS/GRRR. Berkas GRRR tidak memuat luas DAS sehingga pencocokan ini condong ke kecocokan; hanya dipakai sebagai gambaran bentuk kurva, bukan bukti kecocokan.

Hasil (23 Nov 2026): 34 `within_range`, 8 `glofas_above` (Kali Bekasi, Cimanuk–Jatigede, Krueng Aceh, Deli, Batang Anai, Way Sekampung, Walanae, Palu–Sigi), 1 `glofas_below` (Tukad Unda), 1 `no_outlets` (Batumerah Ambon). `within_range` adalah uji kasar karena DAS di sekitar pos berukuran sangat beragam; yang informatif adalah `glofas_above` dan `glofas_below`, yang ditandai di panel stasiun dan berarti ambang perlu diverifikasi dengan pos duga air. Pada 33 pasangan berskala mirip (rasio Q2 0,5–2), median Q20/Q2 adalah 1,50 untuk GloFAS dan 1,33 untuk GRRR: kurva Gumbel GloFAS lebih curam, sehingga level Ekstrem lebih sulit tercapai dibanding bila memakai bentuk kurva GRRR.

Ini perbandingan **model terhadap model**; GRRR bukan pengukuran. Hasil memberi tahu stasiun mana yang paling perlu divalidasi dengan data pos duga air (BBWS/PUPR), bukan stasiun mana yang benar.

## 11. Validasi sel terhadap pos duga air (GRDC)
Sumber: katalog stasiun Global Runoff Data Centre (`GRDC_Stations.xlsx` dari `ftp://ftp.bafg.de/pub/REFERATE/GRDC/catalogue/grdc_stations.zip`, diunduh 23 Nov 2026). Katalog memuat 26 pos duga air Indonesia dengan luas DAS dan rerata debit jangka panjang terukur (`lta_discharge`), semuanya di Sumatera, Jawa, dan Sulawesi; data dari dinas hidrologi nasional, periode 1951–2010. Rujukan: The Global Runoff Data Centre, 56068 Koblenz, Germany.

Pertanyaan yang diuji: apakah sel GloFAS terpilih (§1) mewakili sungai dan ukuran DAS yang benar? Pemilihan sel memakai debit spesifik asumsi per pulau; di sini asumsi diganti debit terukur pos di sungai yang sama. `scripts/validate_gauges.py`:

1. Memasangkan stasiun katalog dengan pos GRDC di sungai yang sama (kurasi manual; nama sungai dan letak dicek). Pos di sungai lain tidak dipasangkan walau dekat, misalnya Ci Sadane–Legokmuncang 3 km dari Katulampa.
2. Menilai pasangan bila rasio luas DAS katalog/GRDC dalam 1/3–3, lalu menskalakan rerata terukur ke luas DAS katalog: Q_skala = Q_GRDC × A_katalog / A_GRDC.
3. Membandingkan rerata sel GloFAS 1997–2024 dengan Q_skala: `consistent` bila rasio 1/1,5–1,5, `glofas_high` bila lebih tinggi, `glofas_low` bila lebih rendah.

| Stasiun | Pos GRDC (periode) | A katalog / A GRDC | Q_GRDC → Q_skala (m³/s) | Rerata sel (m³/s) | Rasio | Putusan |
|---|---|---|---|---|---|---|
| Bengawan Solo · Bojonegoro | Bojonegoro (1983–1991, bulanan), 8 km | 1,23 | 361 → 446 | 460 | 1,03 | consistent |
| Bengawan Solo · Sragen | Jurug (1975–2009), hulu | 1,89 | 82 → 156 | 199 | 1,27 | consistent |
| Brantas · Mojokerto | Jabon (1951–1983, bulanan), hulu | 1,36 | 184 → 251 | 404 | 1,61 | glofas_high |
| Walanae · Wajo | Sempajeruk (1990–2009), hulu | 2,59 | 61 → 158 | 163 | 1,03 | consistent |
| Tondano · Manado | Kairagi (1991–2009) | 1,28 | 12 → 15 | 24 | 1,62 | glofas_high |

Tidak dinilai (pos GRDC di hulu dengan DAS jauh lebih kecil, rasio luas 4,3–9,7): Citarum–Nanjung (Majalaya), Serayu–Banyumas (Banjarnegara), Progo–Bantul (Badran), Way Sekampung (Kunyir), Kampar–Pelalawan (Kampar Kiri, Lipat Kain). Koordinat GRDC Kairagi (124,483°BT) jatuh di laut barat Manado; pos dipasangkan menurut nama, sungai, dan luas DAS, dan jaraknya tidak dipakai.

Cara membaca:
- Tiga dari lima sel sesuai besaran sungai terukur. Walanae ditandai `glofas_above` oleh uji GRRR (§10.2), tetapi rerata selnya sesuai pos Sempajeruk; selisih Q2 dengan GRRR lebih mungkin berasal dari pencocokan titik keluar GRRR atau bentuk kurva banjir daripada salah sungai. Ini belum menguji Q2.
- Brantas–Mojokerto dan Tondano–Manado: rerata sel ±1,6× rerata terukur. Pada Brantas, data Jabon berasal dari 1951–1983 dan sungainya banyak dimanfaatkan untuk irigasi; pada Tondano, sel berjarak 12,9 km dari pos. Kedua stasiun ditandai di panel stasiun. Level tetap dihitung terhadap ambang model yang sama sehingga bias rerata sebagian saling meniadakan.
- Debit spesifik pos GRDC dengan DAS ≥ 1.000 km²: Jawa median 25,7 L/s/km² (3 pos; asumsi pemilihan sel 30), Sulawesi 21,5 (1 pos; asumsi 35), Sumatera 61,1 (1 pos; asumsi 40). Pos hulu kecil jauh lebih basah (median Jawa seluruh pos 58,0), sehingga asumsi per pulau tidak berlaku untuk DAS kecil.

Keterbatasan: periode GRDC tidak sama dengan GloFAS; penskalaan luas mengabaikan beda hujan hulu–hilir; analisis rerata di atas tidak memvalidasi ambang banjir. Uji pendahuluan Q2 dengan indeks tahunan terbuka dijelaskan di §12.

## 12. Uji pendahuluan ambang Q2 terhadap maksimum tahunan pos (GSIM)
GSIM menyediakan indeks tahunan yang dihitung dari debit harian terukur GRDC, termasuk `MAX` (maksimum debit harian per tahun), sehingga tidak perlu meminta ulang deret harian untuk menghitung estimasi Q2. Sumber: Do dkk. (2018), metadata, https://doi.org/10.1594/PANGAEA.887477; Gudmundsson dkk. (2018), indeks, https://doi.org/10.1594/PANGAEA.887470. Lisensi CC BY-SA 3.0. Statistik adaptasi GSIM dalam `gauge_validation.json` mengikuti lisensi tersebut; bukan perubahan lisensi kode program.

Metode (`validate_gauges.py`):
1. Identitas pos dicocokkan berdasarkan nomor GRDC dalam metadata GSIM, bukan koordinat terdekat. Bojonegoro dan Jabon tidak ada di GSIM (GRDC hanya menyimpan rekaman bulanan; maksimum bulanan rerata bukan maksimum harian dan tidak dipakai sebagai pengganti). Bojonegoro dinilai dengan nilai terbit (lihat di bawah); Jabon tidak punya sumber terbuka dan tetap **tidak dinilai**.
2. Analisis utama memakai tahun dengan maksimum tersedia dan ≤30 hari hilang/meragukan menurut GSIM. Hari hilang masih dapat mencakup puncak banjir: ini bukan jaminan maksimum lengkap. Minimal 8 tahun adalah batas hitung eksploratif, bukan syarat kecukupan validasi.
3. Estimasi Gumbel L-moment Q2/Q5/Q20 memakai rumus yang sama dengan reanalisis GloFAS. Periode tidak disamakan karena rekaman pendek dan terputus; hasil menggabungkan perbedaan periode, model, serta lokasi.
4. Untuk membandingkan DAS berbeda, Q2 pos diskalakan menurut Q ∝ A^b, b=0,5–1,0; b=0,75 untuk rasio ringkasan. Eksponen adalah asumsi sensitivitas, bukan hasil kalibrasi lokal; rentang bukan interval kepercayaan. `consistent` hanya berarti Q2 GloFAS berada dalam rentang skala yang diperlebar toleransi 1,5× di kedua sisi.
5. Sensitivitas terpisah mengeluarkan tahun dengan rerata <25% median rerata tahunan. Ini bukan penghapusan dalam analisis utama dan bukan bukti kerusakan alat: Jurug 1998 dan Sempajeruk 2009 tetap disertakan di hasil utama.

| Stasiun / pos | Tahun dipakai | Estimasi Q2 pos (m³/s) | Q2 skala DAS katalog (m³/s) | Q2 GloFAS (m³/s) |
|---|---:|---:|---:|---:|
| Solo–Sragen / Jurug | 13 | 544 | 748–1.029 | 716 |
| Walanae–Wajo / Sempajeruk | 13 | 665 | 1.071–1.724 | 753 |
| Tondano–Manado / Kairagi | 9 | 43 | 49–56 | 71 |
| Solo–Bojonegoro / Bojonegoro (makalah) | – | 1.883 | 2.092–2.324 | 1.663 |

Ketiga hasil GSIM masuk toleransi penyaring lebar, **bukan berarti ambang tervalidasi akurat**. Tanpa tahun debit sangat rendah, Q2 Jurug menjadi 602 dan Sempajeruk 733 m³/s; pada Sempajeruk, putusan berubah menjadi `glofas_low`. Kesimpulan Walanae sensitif terhadap mutu rekaman dan asumsi pemindahan debit antarlokasi. Danau, waduk, dataran banjir, serta perubahan hujan dapat membuat penskalaan luas tidak berlaku.

Keempat uji homogenitas `MAX` GSIM untuk ketiga pos GSIM berstatus **NSD** (kurang data untuk pengujian), bukan homogen. Q20 dari hanya 9–13 tahun adalah ekstrapolasi yang lebih tidak pasti. Karena itu `flood.status` tetap `preliminary`, hasil ditampilkan di lensa Riset, tidak mengganti catatan GRRR, tidak mengubah Q2 operasional, level, atau notifikasi. Validasi operasional masih memerlukan rekaman lebih panjang, keterhubungan DAS yang terverifikasi, periode tumpang tindih, dan data/QC BBWS/PUPR atau penyedia pos.

**Bojonegoro (`kind=published`).** Sulaeman, Suhartanto & Sumiadi (2017), *Jurnal Teknik Pengairan* 8(2): 146–157, Tabel 3, menerbitkan Gumbel debit tahunan maksimum pos Bojonegoro (data BBWS Bengawan Solo dan Perum Jasa Tirta I): Q2 1.883, Q5 2.316, Q20 2.877 m³/s. Tahun dan panjang rekaman tidak dilaporkan, nilai tidak dapat dihitung ulang, dan makalah itu sendiri hanya menurunkannya untuk simulasi genangan. Karena itu bobotnya lebih rendah daripada hasil GSIM, ditandai `published` di API dan antarmuka, dan tetap `preliminary`. Q2 GloFAS (1.663) sekitar 0,88× nilai terbit di pos yang sama dan 0,76× nilai tengah setelah penskalaan luas DAS katalog (1,23×); di bawah seluruh rentang skala (2.092–2.324) tetapi dalam toleransi penyaring 1,5×, sehingga `consistent` di sini sangat lemah. Q20/Q2 terbit 1,53 vs GloFAS 1,31. Banjir Des 2007 tercatat 2.508 m³/s di pos yang sama, dengan periode ulang Gumbel makalah 8,4 tahun; maksimum tahunan reanalisis GloFAS 1997–2024 pada selnya 2.387 m³/s, lebih rendah. Catatan tambahan: GloFAS cenderung lebih rendah pada puncak banjir di Bengawan Solo hilir, tetapi satu kejadian dan satu makalah tidak cukup untuk menyimpulkan bias ambang.

## Rujukan
- Sulaeman, A., Suhartanto, E., Sumiadi (2017). Analisis genangan banjir akibat luapan Bengawan Solo untuk mendukung peta risiko bencana banjir di Kabupaten Bojonegoro. *Jurnal Teknik Pengairan* 8(2): 146–157: https://jurnalpengairan.ub.ac.id/index.php/jtp/article/view/327
- Hosking, J. R. M. (1990). L-moments: analysis and estimation of distributions using linear combinations of order statistics. *Journal of the Royal Statistical Society B*, 52(1), 105–124.
- Open-Meteo Flood API: https://open-meteo.com/en/docs/flood-api
- Open-Meteo Marine API: https://open-meteo.com/en/docs/marine-weather-api
- Open-Meteo, batas kuota: https://github.com/open-meteo/open-meteo/blob/main/docs/api-rate-limiting.md
- PetaBencana API: https://docs.petabencana.id/master-1/routes/crowdsourced-reports
- Kategori hujan BMKG: https://nasional.sindonews.com/read/981399/15/ini-penjelasan-bmkg-soal-perbedaan-hujan-lebat-sangat-lebat-dan-ekstrem-1672215134
- Google Research, Flood Forecasting resources (Inundation History, GRRR): https://sites.research.google/gr/floodforecasting/resources
- Cohen, D. dkk. (2024). An improved flood forecasting AI model, trained and evaluated globally: https://research.google/blog/a-flood-forecasting-ai-model-trained-and-evaluated-globally
- Global Runoff Data Centre (GRDC), katalog stasiun dan portal data: https://grdc.bafg.de/data/data_portal/ (The Global Runoff Data Centre, 56068 Koblenz, Germany)
- Do, H. X. dkk. (2018), GSIM metadata, https://doi.org/10.1594/PANGAEA.887477 (CC BY-SA 3.0).
- Gudmundsson, L. dkk. (2018), GSIM indeks tahunan dan homogenitas, https://doi.org/10.1594/PANGAEA.887470 (CC BY-SA 3.0).
