# Pipeline karya visual dan data sungai

Semua berkas di `crates/flood-web/public/art/` dan `public/data/` dihasilkan oleh skrip di `scripts/art/` dan `scripts/rivers/`. Tidak ada langkah manual.

## Keluaran

| Berkas | Ukuran piksel | Batas | Sumber relief |
|---|---|---|---|
| `art/hero-indonesia.webp` | 2400×1350 | ≤ 400 KB | Terrarium (butuh batimetri) |
| `art/og-flood.jpg` | 1200×630 | ≤ 250 KB | turunan hero |
| `art/replay-jabodetabek-2020.webp` | 960×540 | ≤ 120 KB | GEM-15 di daratan, Terrarium di laut dan celah |
| `art/replay-barito-2021.webp` | 960×540 | ≤ 120 KB | idem |
| `art/replay-pantura-wulan-2024.webp` | 960×540 | ≤ 120 KB | idem |
| `data/rivers.geojson` | LOD ikhtisar | ≤ 1,5 MB | OSM |
| `data/rivers/<station_id>.geojson` | LOD detail | ≤ 400 KB per berkas | OSM |
| `data/gem15-cells.json` | station_id → sel | | STAC GEM-15 |
| `data/geolibre-style.json` | gaya GeoLibre | | token desain |

## Lingkungan

```
uv venv venv && uv pip install --python venv/Scripts/python.exe \
  shapely numpy pillow fonttools brotli requests matplotlib rasterio scipy imageio pyproj forge3d==1.48.0
```

Penulisan langsung ke `public/` dari beberapa venv ditolak di mesin pengembangan; tulis ke folder kerja lalu salin.

## 1. Sungai OSM

```
python scripts/rivers/bake_rivers.py --cache <cache_overpass> --data-dir <keluaran> --report <laporan.json> [--engine shapely|rustgis]
python scripts/rivers/qa_rivers.py --out <qa> [--zoom-stations id1,id2]
```

- Overpass: `way[waterway=river|canal][name~…]` per kelompok nama, bbox sebanding luas DAS. Jeda ≥ 5 s antarkueri; respons dicache menurut hash kueri.
- Arah way OSM (hilir) dipertahankan saat merakit; tidak ada pembalikan. Properti `downstream=true`.
- Simplifikasi Douglas–Peucker: ikhtisar 0,0003° (≈ 33 m), detail 0,00005° (≈ 5,5 m). Koordinat dibulatkan 4 dan 5 desimal.
- Pos stasiun yang jaraknya > 9 km dari komponen sungai terdekat ditolak, tidak digambar.
- `qa_rivers.py` menumpuk garis di atas tile OSM (zoom 14) untuk memeriksa kelokan.

## 2. Sel GEM-15

```
python scripts/art/gem15_cells.py --cache <stac_cache> --out public/data/gem15-cells.json
```

STAC root → koleksi → 297 katalog regional → item → aset `dtm`. Stasiun tanpa sel tercakup bernilai `null`.

## 3. Render hero

```
python scripts/art/render_hero.py --work <kerja> --cache <tiles> --rivers <rivers.geojson> --out-dir <art> --exag 3.0 --zscale 9.0
python scripts/art/compose_og.py --hero <kerja>/hero_hero.png --out <art>/og-flood.jpg --font-cache <fonts>
```

- Terrarium z7 (EPSG:3857) ke jendela persegi 5644 px, batimetri diratakan (`sea_factor=0,10`); kedalaman dibawa lewat warna, bukan geometri.
- Viewer forge3D: `phi=90`, `theta=55`, radius 0,53 sisi, `z_scale=9`, tint sebagai overlay.
- Wordmark: Schibsted Grotesk (`assets/fonts/SchibstedGrotesk-wght.woff2`) dikonversi ke TTF lalu diinstansiasi pada wght 700 dan 450 lewat fontTools.

## 4. Render replay

```
python scripts/art/render_replay.py --work <kerja> --cache <tiles> --stac-cache <stac> --out-dir <art>
```

1. Pilih sel GEM-15 yang menimpa jendela; baca dengan reproject rata-rata ke UTM setempat.
2. Isi nodata dengan Terrarium z10 setelah menggeser median selisih GEM − Terrarium pada piksel tumpang tindih (koreksi datum).
3. Laut: piksel tanpa GEM dengan Terrarium ≤ 0,5 m diperlakukan sebagai muka laut 0 m.
4. Haluskan Gaussian (σ per skenario) untuk menekan artefak penutup lahan yang tersisa pada DTM ML.
5. Tint, hillshade berganda, sungai dari `public/data/rivers/<id>.geojson`, vinyet ke warna kertas.
6. Render forge3D, turunkan ke 960×540, cari kualitas webp tertinggi yang muat ≤ 120 KB.

Catatan data GEM-15:

- DTM hasil model pembelajaran mesin (permukaan tanah, tanpa kanopi). Sisa artefak kanopi tetap mungkin muncul di hutan lebat.
- Datum vertikal EGM96; Terrarium memakai referensi lain, sehingga offset dikoreksi seperti langkah 2 (offset median: −1,13 m Jabodetabek, −0,73 m Barito, −0,66 m Wulan).
- Nodata NaN. Cakupan valid di jendela: Jabodetabek 94,6 %, Barito 76,4 %, Wulan 79,2 %.
- Dataran rendah dan pesisir sangat datar; relief dilebih-lebihkan hanya untuk ilustrasi dan tidak untuk analisis.

## Atribusi

- Relief nasional dan batimetri: Mapzen Terrain Tiles (AWS Open Data Registry). Global ETOPO1 (NOAA); data 3DEP (dahulu NED), GMTED2010, dan SRTM dari U.S. Geological Survey; sumber regional sesuai https://github.com/tilezen/joerd/blob/master/docs/attribution.md.
- Relief replay: Google GEM-15 DTM, CC BY 4.0. "This dataset is produced by Google."
- Sungai: © OpenStreetMap contributors, ODbL.
