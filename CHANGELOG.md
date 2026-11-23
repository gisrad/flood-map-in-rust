# Changelog

Perubahan yang memengaruhi pengguna dicatat per versi mengikuti SemVer. Sebelum 1.0, versi minor dapat membawa perubahan kontrak API; `schema_version` snapshot tetap harus diperiksa konsumen.

## [0.2.0] - 2026-11-23

Fondasi rilis sumber dan web. Bagian ini menjadi catatan rilis ketika tag `v0.2.0` diterbitkan; keberadaan berkas ini sendiri bukan bukti rilis telah dipublikasikan.

### Added
- Workflow rilis berbasis tag dengan pemeriksaan versi, paket sumber/web, checksum SHA-256 dan rilis draf untuk ditinjau sebelum publikasi.
- CI build frontend Trunk, uji native worker/shared, anggaran ukuran WASM Brotli, dan smoke test Playwright portabel dengan data sintetis terpisah dari aset produksi.
- Prosedur deploy manual terpisah dari rilis; lisensi data dan atribusi dibawa bersama artefak.
- Riwayat genangan Google 1999–2020 opsional dan ringkasan paparan sekitar titik; uji silang ambang GloFAS terhadap GRRR di lensa Riset.
- Pembanding rerata debit GRDC serta uji Q2 pendahuluan GSIM untuk Jurug, Sempajeruk dan Kairagi; pembanding terbit Bojonegoro ditandai lebih lemah. Jabon tetap belum dinilai.
- Tema mengikuti sistem dan tur fitur enam langkah.

### Fixed
- Panel replay desktop/mobile diringkas; kamera memperhitungkan panel dan lembar informasi agar titik tidak tersembunyi.
- Replay dipisahkan dari data terkini, prakiraan, berita dan notifikasi; hasil model dibandingkan dengan catatan kejadian, tanpa menyebut debit sel yang belum terkalibrasi.
- Penanganan nilai muka laut kosong, kategori ambang dan zona waktu lokal; catatan publik Brantas/Tondano menjelaskan perbandingan dengan pos duga air.
- Kebijakan CSP berbasis hash, tautan luar HTTPS, CORS endpoint baca, pembatasan registrasi topik, dan pengiriman notifikasi saat kenaikan level.

### Limitations
- **Indikasi model, bukan peringatan resmi.** Empat pembanding Q2 masih pendahuluan; enam dari 50 sel sungai belum terkalibrasi. Tidak ada perubahan ambang operasional dari uji silang tersebut.
- Paket web memerlukan backend API Cloudflare Worker; bukan aplikasi luring atau installer Linux/Windows/macOS/Android. Belum ada PWA, auto-update atau penandatanganan aplikasi.
- Belum ada SBOM lengkap maupun penandatanganan artefak; checksum hanya memeriksa integritas, bukan identitas penerbit.

## [0.1.0] - 2026-11-22

### Added
- Implementasi awal observatorium Indonesia: Leptos/WASM, Cloudflare Worker dan SQLite Durable Object.
- Pemantauan 50 titik sungai, 12 koridor rob, ensemble GloFAS, hujan/lengas tanah, laporan PetaBencana, BMKG dan berita.
- Peta MapLibre, replay historis Jabodetabek/Barito/Wulan, lensa pengguna, API, RSS dan notifikasi ntfy.

Versi 0.1.0 adalah versi workspace historis, bukan klaim pernah tersedia sebagai GitHub Release.
