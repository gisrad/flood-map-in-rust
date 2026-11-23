# Changelog

Perubahan yang memengaruhi pengguna dicatat per versi mengikuti SemVer. Sebelum 1.0, versi minor dapat membawa perubahan kontrak API; `schema_version` snapshot tetap harus diperiksa konsumen.

## [0.3.0] - 2026-11-23

### Added
- **PWA**: manifest web, ikon 192/512/maskable/apple, dan service worker cangkang aplikasi. Situs dapat dipasang dari Edge/Chrome. Service worker hanya menyimpan HTML/JS/WASM/CSS/font/data statis; `/api/*` dan ubin peta tidak pernah disimpan, sehingga data banjir lama tidak disajikan sebagai data terkini. Saat dibuka tanpa koneksi, cangkang tampil dengan pesan "Data belum dapat dimuat".
- **Aplikasi desktop Tauri 2 (klien tipis, ID `id.transbulent.hidrosentinel`)**: installer Windows NSIS `.exe` (±1,5 MB, per pengguna, **tanpa tanda tangan kode**) dan paket Debian `.deb` (Debian 12+/Ubuntu 22.04+, memakai WebKitGTK sistem). Jendela membuka https://flood.transbulent.id; navigasi dibatasi ke domain itu, tautan lain dibuka di peramban bawaan. Tanpa koneksi, peluncur lokal menampilkan pesan jujur dan mencoba lagi otomatis. Bukti lisensi dependensi Rust desktop dibundel di `third-party/`.
- Workflow `desktop.yml`: build `.exe` dan `.deb`, uji pasang/jalankan/hapus senyap di Windows, dan uji pasang `apt` serta peluncuran di kontainer Debian 12 bersih.

### Fixed
- Pengumpul lisensi rilis kini membaca `cargo metadata` sebagai UTF-8 (sebelumnya gagal di Windows) dan mendukung ekspresi `WITH LLVM-exception` serta ID 0BSD/MIT-0/ISC/MPL-2.0 setelah ditinjau.

### Limitations
- `.exe` belum ditandatangani: Windows SmartScreen akan menampilkan peringatan "Windows protected your PC" (pilih *More info* → *Run anyway*). Verifikasi dengan `SHA256SUMS-desktop`.
- Aplikasi desktop memerlukan internet dan tidak membundel data atau frontend; pembaruan fitur mengikuti situs. Belum ada pembaruan otomatis cangkang aplikasi, paket macOS, atau Android.
- WebGL (peta 3D) di Linux bergantung pada driver GPU dan WebKitGTK; bila peta tidak tampil, buka situs di peramban.

## [0.2.1] - 2026-11-23

### Fixed
- Data terakhir tidak lagi tampak tanpa peringatan ketika koneksi terputus, refresh gagal, snapshot berumur ≥30 menit, atau sumber upstream gagal/kedaluwarsa. Data lama tetap tersedia dengan waktu aslinya; tersambung kembali memicu refresh dan tersedia tombol coba ulang. Replay tetap terpisah.
- Peta menyesuaikan tinggi banner koneksi agar titik/kanvas tidak tersembunyi; tombol Menu memiliki nama aksesibel di mobile.
- Tabel sumber menjelaskan waktu pengambilan data terakhir yang berhasil, bukan waktu upaya yang gagal.

### Added
- Manifest provenance dan bukti lisensi dependensi pada paket rilis. Teks upstream yang tidak tersedia ditandai eksplisit dengan deklarasi Cargo dan terms SPDX; bukan klaim audit legal lengkap atau SBOM lengkap.
- Regresi browser untuk putus koneksi setelah berhasil memuat, refresh gagal/pulih, sumber menurun, snapshot lama, dan titik mobile dengan banner.

### Limitations
- Arsip v0.2.0 tidak ditimpa; versi ini menggantikannya untuk perbaikan di atas. Publikasi draf tetap keputusan pemilik.
- Backend API tetap diperlukan; belum ada PWA, installer desktop/Android, penandatanganan paket, atau validasi Q2 operasional.

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
