# Rilis sumber dan web

Distribusi: arsip sumber, paket web, PWA (dari situs), klien tipis desktop Tauri 2 (Windows NSIS `.exe` tanpa tanda tangan dan Debian `.deb`), serta image Docker preview frontend di GHCR. Belum ada macOS, Android, npm atau crate publik. Tag dan aset rilis yang sudah ada tidak pernah ditimpa; perbaikan memakai versi baru. Membuat paket lokal tidak menerbitkan rilis.

**Indikasi model, bukan peringatan resmi.** Gunakan informasi resmi BPBD/BNPB, BMKG dan pengelola sungai untuk keputusan keselamatan. Validasi Q2 masih pendahuluan.

## Versi dan batas paket

- Sumber versi: `[workspace.package].version` di `Cargo.toml`; crate anggota mewarisi versi. Sinkronkan tiga entri workspace dalam `Cargo.lock`, `version` dalam `CITATION.cff` dan `package.json`/`package-lock.json`, serta `desktop/src-tauri/Cargo.toml`, `desktop/src-tauri/Cargo.lock` dan `desktop/src-tauri/tauri.conf.json`. `package_release.py --check-only` menolak versi yang tidak sama dan ID aplikasi selain `id.transbulent.hidrosentinel`.
- Tag format `vMAJOR.MINOR.PATCH` harus sama dengan versi tersebut. Tidak ada publikasi otomatis dari push cabang atau pull request.
- Catatan tiap versi ada di `CHANGELOG.md`. Sebelum menandai rilis, isi tanggal aktual dan tinjau batasan, bukan hanya daftar fitur. `date-released` di `CITATION.cff` boleh ditambahkan saat tanggal publikasi sudah pasti.
- Rilis sumber berasal dari berkas yang dilacak Git pada tag; konfigurasi lokal `wrangler.toml`, token, `target/`, hasil tes dan `node_modules/` tidak boleh ikut.
- Paket web memuat hasil Trunk dan pemberitahuan lisensi; **tidak memuat backend Worker, data API terkini, kredensial, maupun fixture sintetis**. Hosting HTML/WASM saja tidak cukup: `/api/*` memerlukan backend kompatibel pada origin yang sama. Untuk self-host penuh gunakan arsip sumber dan konfigurasi Worker sendiri.
- Arsip wajib mempertahankan `LICENSE`, `DATA_LICENSES.md`, `THIRD_PARTY_NOTICES.md`, `CITATION.cff`, pemberitahuan MapLibre/font, `licenses/`, `DEPENDENCY_LICENSES.json`, `BUILD_PROVENANCE.json`, `RELEASE_CONTENTS.json`, dan `Cargo.lock`. Data GSIM CC BY-SA 3.0 tidak dilisensikan ulang menjadi MIT; lihat `DATA_LICENSES.md`.

### Bukti lisensi dan asal build

`scripts/release_licenses.py` membaca semua paket eksternal dalam `Cargo.lock` melalui `cargo metadata --locked --offline`, lalu memeriksa SHA-256 arsip `.crate` cache Cargo terhadap checksum lockfile. Berkas lisensi/NOTICE/COPYRIGHT disalin verbatim, termasuk pemberitahuan bersarang. Author, deklarasi lisensi, repository, dan baris copyright asli disimpan sebagai bukti, bukan copyright holder/tahun hasil tebakan.

Sebagian crate terbit tidak menyertakan teks lisensi. Entri tersebut **tetap terlihat** sebagai `missing_upstream_notice: true` dan `standard_terms_used: true`; teks standar SPDX yang cocok dengan deklarasi Cargo disertakan dari revisi tetap SPDX license-list-data v3.27.0. Placeholder dalam teks standar bukan copyright upstream yang telah ditemukan. Inventaris mencakup dependensi seluruh workspace (termasuk backend, host/build, optional/target lain), **bukan daftar yang terbukti ter-link ke WASM, bukan SBOM, bukan pernyataan kepatuhan hukum lengkap**. Rust sysroot dan alat npm tes/deploy tidak diinventarisasi oleh collector ini. Pemberitahuan data, MapLibre, dan font terpisah tetap wajib. Tinjau semua fallback dan asal atribusi sebelum memutuskan publikasi; draf bukan persetujuan publikasi.

`BUILD_PROVENANCE.json` dicatat pada mesin build segera setelah Trunk: commit, hash lockfile, target/perintah, versi rustc/Cargo/Trunk/Node, dan hash setiap aset. File ini catatan builder tanpa hostname/path/kredensial pribadi, **bukan attestation/tanda tangan**. `RELEASE_CONTENTS.json` mencatat hash isi setiap arsip, bukan dirinya sendiri. Checksum mendeteksi perubahan, bukan membuktikan identitas penerbit atau build bit-for-bit deterministik.

## Verifikasi lokal

Prasyarat yang sama dengan CI: Rust **1.97.1**, target `wasm32-unknown-unknown`, Trunk **0.22.2**, Node.js **22**, Python **3.11+**, Git, dan akses jaringan untuk fetch Cargo/alat Trunk/browser serta teks standar SPDX ketika packaging. `Cargo.lock` dan `package-lock.json` mengunci dependensi, bukan seluruh OS/toolchain eksternal. Jalankan dari checkout bersih commit yang akan dirilis; arsip sumber tanpa `.git` dapat dibangun, tetapi pembuatan paket/provenance memerlukan checkout Git.

```bash
rustup toolchain install 1.97.1 --profile minimal --component rustfmt,clippy --target wasm32-unknown-unknown --no-self-update
rustup default 1.97.1
cargo fetch --locked
cargo fmt --all -- --check
cargo test -p flood-shared -p flood-worker --locked
cargo clippy -p flood-shared -p flood-worker --target wasm32-unknown-unknown --locked -- -D warnings
cargo check -p flood-web --target wasm32-unknown-unknown --locked
cd crates/flood-web
trunk build --release --locked
cd ../..
python scripts/package_release.py --record-build --dist crates/flood-web/dist
npm ci
npx playwright install chromium
npm run test:e2e
```

Smoke test melayani `dist/` secara lokal dan menyediakan API sintetis deterministik. Pengujian ini tidak membuktikan ketepatan hidrologi atau konektivitas upstream produksi. Tidak ada pendaftaran topik ntfy, ingest produksi atau deploy dalam tes. Linux CI memasang dependensi browser melalui `npx playwright install --with-deps chromium`.

Jika memakai Chrome lokal, tetapkan `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` ke executable sendiri; jangan menyimpan path mesin pribadi dalam repositori. Artefak kegagalan/tangkapan layar disimpan sebagai hasil tes, tidak dalam `dist/`.

## Rilis GitHub

1. Selesaikan perubahan, perbarui versi/changelog/atribusi, dan pastikan CI lulus pada commit yang akan ditandai.
2. Buat tag pada commit yang ditinjau lalu dorong tag tersebut. Workflow rilis mengulang pemeriksaan sebelum menyiapkan artefak dan **draf** GitHub Release.
3. Unduh kedua arsip dan `SHA256SUMS` ke direktori baru, cocokkan checksum, lalu jalankan pemeriksa isi di bawah. Periksa entri fallback dalam `DEPENDENCY_LICENSES.json` dan batas web/backend; jangan menganggap CI menggantikan peninjauan atribusi.
4. Tetap simpan sebagai **draf** sampai pemilik memutuskan publikasi setelah peninjauan. Belum ada tanda tangan/attestation penerbit atau SBOM lengkap. Jangan memindah tag atau mengganti aset `v0.2.0` dengan commit patch; gunakan `v0.2.1`.

Contoh verifikasi checksum dari direktori unduhan:

```bash
sha256sum -c SHA256SUMS
python /path/ke/checkout/scripts/package_release.py --verify-assets /path/ke/unduhan --tag v0.2.1
```

Pemeriksa unduhan membaca arsip tanpa mengekstrak: checksum kedua unduhan, satu root yang aman/tanpa duplikat/link khusus, manifest isi, dokumen dan entrypoint crate, output JS/CSS/WASM, referensi HTML, hash seluruh aset builder, kesamaan commit sumber/web, kecocokan lockfile, serta satu catatan lisensi berteks untuk setiap paket eksternal terkunci. Penghapusan berkas wajib tetap ditolak walaupun manifest/checksum arsip dibuat ulang. Ini validasi konten untuk konsumen, bukan tes yang sekadar mencari string dalam source script. Paket lama `v0.2.0` belum memuat metadata/lisensi baru sehingga tidak lulus gate baru; kedua checksum unduhan lama tetap bisa cocok.

Pemeriksaan sebelum commit (tanpa dist/provenance baru):

```bash
python scripts/package_release.py --check-only --worktree --tag v0.2.1
```

Sesudah commit, bangun ulang dan catat provenance seperti di atas, jalankan tes, lalu:

```bash
node scripts/check_wasm_size.mjs --dist crates/flood-web/dist --budget-kib 450
python scripts/package_release.py --check-only --tag v0.2.1 --rev HEAD --dist crates/flood-web/dist
python scripts/package_release.py --tag v0.2.1 --rev HEAD --dist crates/flood-web/dist
python scripts/package_release.py --verify-assets target/release-assets/v0.2.1 --tag v0.2.1
```

Default lokal berdasarkan tag: `target/release-assets/v0.2.1/` berisi `flood-map-in-rust-v0.2.1-web.zip`, `flood-map-in-rust-v0.2.1-source.tar.gz`, `SHA256SUMS`; catatan di `target/release-notes-v0.2.1.md`. Output harus kosong: script menolak menimpa aset yang sudah ada. Gunakan `--out <direktori-baru>` / `--notes <file-baru>` untuk percobaan lain. CI memakai direktori runner baru dan nama output eksplisit. Arsip sumber memakai byte/mode Git ditambah bukti lisensi, provenance, dan manifest isi hasil generator; bukan salinan seluruh direktori kerja.

Windows PowerShell: `Get-FileHash <nama-arsip.zip> -Algorithm SHA256`, lalu bandingkan hasil dengan entri berkas yang sama dalam `SHA256SUMS`.

## Deploy produksi terpisah

Rilis sumber/web bukan deploy dan tidak mengubah Durable Object yang sedang berjalan. Workflow manual deploy memerlukan tag yang telah diterbitkan, konfigurasi tujuan yang eksplisit, serta otorisasi pemilik. Jangan menyimpan token/API key dalam kode, log, ZIP atau catatan rilis. Jangan menyalin `wrangler.toml` pribadi ke repositori.

Gunakan environment GitHub `production` dengan aturan persetujuan bila tersedia. Keberadaan deklarasi environment dalam YAML **tidak otomatis mengaktifkan required reviewers**; pemilik harus mengaturnya di GitHub. Token Cloudflare dibatasi ke akun/Worker dan zona yang dibutuhkan, bukan Global API Key. Penyiapan secret, izin akun dan environment bukan bagian dari tes lokal.

Workflow `Deploy published release (manual)` hanya menerima dispatch dari `main`, tag rilis non-draf/non-prerelease, dan paket web yang checksum serta commit tersematnya sesuai tag. `dry_run` bawaan **true**: build/bundle tanpa mengunggah ke Cloudflare. Untuk deploy nyata, pemilik harus sengaja mengubahnya menjadi false.

Atur pada environment `production`:

| Jenis | Nama | Isi |
|---|---|---|
| Variable | `CLOUDFLARE_ACCOUNT_ID` | ID akun tujuan (32 karakter heksadesimal) |
| Variable | `CLOUDFLARE_ZONE_NAME` | Zona DNS yang menaungi hostname |
| Variable | `CLOUDFLARE_WORKER_NAME` | Nama Worker tujuan |
| Variable | `FLOOD_HOSTNAME` | Hostname tanpa skema/path |
| Secret | `CLOUDFLARE_API_TOKEN` | Token deploy hak minimum; diperlukan hanya untuk unggahan nyata |

Konfigurasi sementara dibuat dari `wrangler.toml.example`; backend dibangun sebelum token diberikan kepada langkah unggahan. Workflow memeriksa rilis terbit lalu membangun dan menguji ulang revisi yang sama. Deklarasi workflow ini belum berarti credential/environment sudah dikonfigurasi atau deploy CI sudah diuji.

Sesudah deploy, periksa halaman sebenarnya (desktop/mobile, replay dan lensa Riset), header CSP, `/api/health` dan `/api/flood`. Data kalibrasi tersemat baru masuk snapshot Durable Object setelah ingest berikutnya (:00/:15/:30/:45 UTC, biasanya sekitar 90 detik tambahan). Jangan memanggil jalur ingest publik atau mengirim notifikasi uji untuk mempercepatnya. Jika pemeriksaan gagal, pulihkan versi Worker sebelumnya melalui Cloudflare; jangan menghapus database.

## Aplikasi desktop (Tauri 2)

Kode di `desktop/` (workspace Cargo terpisah agar dependensi native tidak masuk build web/worker). Peluncur lokal `desktop/launcher/` memeriksa `https://flood.transbulent.id/api/health`, lalu membuka situs; tanpa koneksi menampilkan pesan dan mencoba lagi. Navigasi jendela dibatasi ke `https://flood.transbulent.id` (diuji `cargo test` di `desktop/src-tauri`); tautan lain dan `target=_blank` dibuka di peramban sistem. Situs jarak jauh tidak mendapat izin IPC Tauri.

Build lokal Windows (Rust 1.97.1, Node 22; NSIS diunduh otomatis oleh Tauri CLI):

```bash
npm ci
cd desktop/src-tauri && cargo fetch --locked
python ../../scripts/release_licenses.py --manifest-dir . --out third-party   # direktori harus kosong/baru
cargo test --locked
cd .. && npx tauri build --bundles nsis
```

Linux `.deb` dibangun di Ubuntu 22.04 (glibc lebih tua dari Debian 12) dengan `libwebkit2gtk-4.1-dev`; lihat `.github/workflows/desktop.yml`. Workflow itu juga memasang `.exe` secara senyap, menjalankan dan menghapusnya, lalu memasang `.deb` lewat `apt` di kontainer Debian 12 bersih dan meluncurkannya di Xvfb (tangkapan layar sebagai artefak). Rilis tag menyertakan `.exe`, `.deb` dan `SHA256SUMS-desktop`.

Batasan: `.exe` belum ditandatangani (SmartScreen memperingatkan); tidak ada pembaruan otomatis cangkang; WebGL di Linux bergantung pada driver dan WebKitGTK. `third-party/DEPENDENCY_LICENSES.json` mencakup dependensi Rust desktop, bukan WebView2/WebKitGTK sistem.

## Image Docker preview (GHCR)

`ghcr.io/gisrad/hidrosentinel-web:0.3.0` dan `latest` menunjuk digest `sha256:118cb0e4ff9723550b8144ff47139c25209d9bc2e073407899f7e60e2e3b6533`. Image berisi frontend dan bukti lisensi dari arsip web v0.3.0 yang lolos `--verify-assets`, dilayani Nginx non-root. `/api/*` dan `/llms.txt` diproksikan dengan verifikasi TLS ke `https://flood.transbulent.id`. Image tidak memuat Worker, Durable Object, data API, token atau backend mandiri.

Build dan uji dari unduhan rilis (`SHA256SUMS`, arsip sumber dan web):

```bash
python containers/web/prepare.py --assets /path/ke/unduhan --tag v0.3.0 --out target/web-container
docker build --build-arg WEB_VERSION=0.3.0 --build-arg WEB_SOURCE_COMMIT=<commit-tag> --build-arg CONTAINER_COMMIT=$(git rev-parse HEAD) -t hidrosentinel-web:test target/web-container
docker run -d --name hidrosentinel-web-test --read-only --tmpfs /tmp:rw,noexec,nosuid,size=64m --cap-drop ALL --security-opt no-new-privileges -p 127.0.0.1:8080:8080 hidrosentinel-web:test
node scripts/check_web_container.mjs http://127.0.0.1:8080 target/web-container-proof
```

Pemeriksa browser membutuhkan 4 KPI dari API live, peta MapLibre, MIME manifest, `sw.js` no-cache, CSP tanpa pelanggaran, mobile Menu, dan 404 untuk aset yang hilang. Dari GitHub Actions, Cloudflare membalas `/api/flood` dengan 403 “Attention Required”, jadi v0.3.0 dibangun, diuji, lalu diunggah dari mesin Linux yang sama. Unggah hanya tag baru; jangan menimpa tag versi. GHCR membuat paket baru sebagai private; mengubahnya menjadi public di Package settings tidak dapat dibatalkan.

## Tahap berikutnya

Android TWA/APK dan macOS bila toolchain, perangkat dan penandatanganan memungkinkan. Kustodi keystore, akun toko dan biaya signing harus diputuskan sebelum paket tersebut. Tidak ada janji dukungan OS sebelum artefak benar-benar dibangun dan diuji pada OS target.
