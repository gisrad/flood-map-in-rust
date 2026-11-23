# Lisensi data dan aset

Lisensi MIT pada `LICENSE` berlaku untuk kode proyek, bukan untuk seluruh data, peta, font, dan pustaka pihak ketiga. Pemberitahuan ini harus ikut saat mendistribusikan sumber maupun paket web. Atribusi sumber lengkap: `THIRD_PARTY_NOTICES.md`.

| Berkas/komponen | Ketentuan |
|---|---|
| Statistik turunan GSIM pada `crates/flood-shared/data/gauge_validation.json` (`flood.kind=gsim_annual_max`, termasuk estimasi, penyaringan dan sensitivitas) | **CC BY-SA 3.0 Unported**, https://creativecommons.org/licenses/by-sa/3.0/ ; kode pengolah tetap MIT. |
| Statistik katalog GRDC dan pembanding terbit pada berkas yang sama | Bukan dilisensikan ulang sebagai MIT/GSIM. Pertahankan atribusi GRDC dan sitasi publikasi pada setiap rekaman. Deret harian asli GRDC tidak didistribusikan. |
| `crates/flood-web/public/data/rivers*.geojson` | © OpenStreetMap contributors, **ODbL 1.0**, https://opendatacommons.org/licenses/odbl/1-0/ ; patuhi ketentuan basis data turunan. |
| Kalibrasi/replay GloFAS, data Open-Meteo, ringkasan GRRR, Inundation History, render GEM-15 | **CC BY 4.0**, https://creativecommons.org/licenses/by/4.0/ ; masing-masing atribusi/sumber dan perubahan dijelaskan di `THIRD_PARTY_NOTICES.md` dan metodologi. |
| MapLibre GL JS/CSS | BSD-3-Clause; salinan pemberitahuan di `crates/flood-web/vendor/LICENSE.txt`. |
| Font Schibsted Grotesk | SIL OFL 1.1; salinan di `crates/flood-web/assets/fonts/OFL.txt`. |

## GSIM

- Do et al. (2018), metadata GSIM: https://doi.org/10.1594/PANGAEA.887477.
- Gudmundsson et al. (2018), indeks GSIM: https://doi.org/10.1594/PANGAEA.887470.
- Perubahan: seleksi maksimum tahunan berdasarkan kelengkapan, estimasi Gumbel L-moment, penskalaan luas DAS, dan analisis sensitivitas (`scripts/validate_gauges.py`).
- Statistik turunan tersebut ditawarkan dengan CC BY-SA 3.0; pertahankan kredit, tautan lisensi dan keterangan perubahan saat memakai atau mengadaptasinya. Penyertaan dalam paket bersama kode tidak mengganti lisensi kode MIT.
- Sumber observasi: **The Global Runoff Data Centre, 56068 Koblenz, Germany**. Tidak ada deret debit harian asli dalam paket.

Berkas JSON yang sama menggabungkan sumber berlainan; lisensi GSIM tidak boleh dianggap sebagai izin redistribusi data GRDC yang tidak disertakan. Keluaran API juga harus mempertahankan atribusi saat disalin. Informasi BMKG, PetaBencana, berita RSS dan layanan daring lain tetap mengikuti ketentuan sumbernya; paket web tidak menyertakan snapshot layanan tersebut.

**Indikasi model, bukan peringatan resmi.** Data dan statistik disediakan apa adanya, bukan pengganti informasi resmi BPBD/BNPB, BMKG atau pengelola sungai. Validasi Q2 masih pendahuluan dan tidak membuktikan kelayakan peringatan operasional.
