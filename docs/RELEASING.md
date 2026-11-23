# Rilis sumber dan web

Tahap v0.2.0 belum menghasilkan installer desktop atau Android. GitHub Packages tidak diperlukan: distribusi berupa GitHub Release, bukan kontainer/npm/crate publik.

**Indikasi model, bukan peringatan resmi.** Gunakan informasi resmi BPBD/BNPB, BMKG dan pengelola sungai untuk keputusan keselamatan. Validasi Q2 masih pendahuluan.

## Versi dan batas paket

- Sumber versi: `[workspace.package].version` di `Cargo.toml`; crate anggota mewarisi versi. Sinkronkan tiga entri workspace dalam `Cargo.lock` serta `version` dalam `CITATION.cff`.
- Tag format `vMAJOR.MINOR.PATCH` harus sama dengan versi tersebut. Tidak ada publikasi otomatis dari push cabang atau pull request.
- Catatan tiap versi ada di `CHANGELOG.md`. Sebelum menandai rilis, isi tanggal aktual dan tinjau batasan, bukan hanya daftar fitur. `date-released` di `CITATION.cff` boleh ditambahkan saat tanggal publikasi sudah pasti.
- Rilis sumber berasal dari berkas yang dilacak Git pada tag; konfigurasi lokal `wrangler.toml`, token, `target/`, hasil tes dan `node_modules/` tidak boleh ikut.
- Paket web memuat hasil Trunk dan pemberitahuan lisensi; **tidak memuat backend Worker, data API terkini, kredensial, maupun fixture sintetis**. Hosting HTML/WASM saja tidak cukup: `/api/*` memerlukan backend kompatibel pada origin yang sama. Untuk self-host penuh gunakan arsip sumber dan konfigurasi Worker sendiri.
- Arsip wajib mempertahankan `LICENSE`, `DATA_LICENSES.md`, `THIRD_PARTY_NOTICES.md`, `CITATION.cff`, serta pemberitahuan MapLibre/font. Data GSIM CC BY-SA 3.0 tidak dilisensikan ulang menjadi MIT; lihat `DATA_LICENSES.md`.

## Verifikasi lokal

Prasyarat: Rust dengan target `wasm32-unknown-unknown`, Trunk 0.22.2, Node.js 22 atau lebih baru, Python 3.11 atau lebih baru. `Cargo.lock` dan `package-lock.json` disimpan agar versi dependensi konsisten.

```bash
cargo fmt --all -- --check
cargo test -p flood-shared -p flood-worker --locked
cargo clippy -p flood-shared -p flood-worker --target wasm32-unknown-unknown --locked -- -D warnings
cargo check -p flood-web --target wasm32-unknown-unknown --locked
cd crates/flood-web
trunk build --release --locked
cd ../..
npm ci
npx playwright install chromium
npm run test:e2e
```

Smoke test melayani `dist/` secara lokal dan menyediakan API sintetis deterministik. Pengujian ini tidak membuktikan ketepatan hidrologi atau konektivitas upstream produksi. Tidak ada pendaftaran topik ntfy, ingest produksi atau deploy dalam tes. Linux CI memasang dependensi browser melalui `npx playwright install --with-deps chromium`.

Jika memakai Chrome lokal, tetapkan `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` ke executable sendiri; jangan menyimpan path mesin pribadi dalam repositori. Artefak kegagalan/tangkapan layar disimpan sebagai hasil tes, tidak dalam `dist/`.

## Rilis GitHub

1. Selesaikan perubahan, perbarui versi/changelog/atribusi, dan pastikan CI lulus pada commit yang akan ditandai.
2. Buat tag pada commit yang ditinjau lalu dorong tag tersebut. Workflow rilis mengulang pemeriksaan sebelum menyiapkan artefak dan **draf** GitHub Release.
3. Unduh arsip, baca batasan, cocokkan checksum, dan tinjau isi ZIP (tidak ada konfigurasi pribadi atau fixture dalam paket web).
4. Terbitkan draf hanya sesudah peninjauan. Belum ada tanda tangan/attestation penerbit atau SBOM lengkap; jangan menyamakan checksum dengan autentikasi penerbit.

Contoh verifikasi checksum dari direktori unduhan:

```bash
sha256sum -c SHA256SUMS
```

Pemeriksaan anggaran WASM dan kesiapan paket sebelum commit:

```bash
node scripts/check_wasm_size.mjs --dist crates/flood-web/dist --budget-kib 450
python scripts/package_release.py --check-only --worktree --tag v0.2.0
```

Sesudah commit, buat paket dari revisi yang akan ditandai:

```bash
python scripts/package_release.py --tag v0.2.0 --rev HEAD --dist crates/flood-web/dist
```

Keluaran: `target/release-assets/flood-map-in-rust-v0.2.0-web.zip`, `flood-map-in-rust-v0.2.0-source.tar.gz`, `SHA256SUMS`; catatan rilis di `target/release-notes.md`. Ganti versi sesuai tag. Arsip sumber memakai revisi Git, bukan seluruh direktori kerja.

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

## Tahap berikutnya

PWA dan penanganan data basi/luring → uji awal Tauri Windows/Linux → Android TWA/APK → macOS bila toolchain, perangkat dan penandatanganan memungkinkan. ID aplikasi, kustodi keystore, akun toko dan biaya signing harus diputuskan sebelum paket native. Tidak ada janji dukungan OS sebelum artefak benar-benar dibangun dan diuji pada OS target.
