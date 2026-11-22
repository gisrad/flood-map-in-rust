#!/usr/bin/env python3
"""Tiga kartu replay (960x540 webp): relief oblik wilayah kejadian dari GEM-15 DTM (daratan)
+ Terrarium (celah, laut) lewat forge3D viewer, sungai utama disorot.

  python scripts/art/render_replay.py --work <DIR> --cache <tiles> --stac-cache <DIR> [--only jabodetabek-2020]
          [--rivers-dir crates/flood-web/public/data/rivers] [--out-dir crates/flood-web/public/art]

Catatan data: GEM-15 DTM = prediksi bare-earth dari stereo optik (bukan LiDAR); datum EGM96;
kanopi rapat dan masker nodata bisa menimbulkan celah -> diisi Terrarium.
Atribusi wajib (CC BY 4.0): "This dataset is produced by Google" (GEM-15).
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter
from pyproj import Transformer
from rasterio.crs import CRS
from rasterio.enums import Resampling
from rasterio.transform import from_origin
from rasterio.warp import reproject
from scipy import ndimage as ndi

sys.path.insert(0, str(Path(__file__).resolve().parent))
import artlib as al  # noqa: E402
import fetch_dem as fd  # noqa: E402
import gem15  # noqa: E402

REPO = Path(__file__).resolve().parents[2]

# pusat (lon, lat), setengah lebar (km), station_id sungai yang disorot, kamera
SCENARIOS = {
    "jabodetabek-2020": dict(
        center=(106.86, -6.40), half_km=44.0, rivers=["ciliwung-katulampa", "ciliwung-manggarai", "cisadane-tangerang", "bekasi-pondokgede"],
        exag=6.0, zscale=2.5, land_max=500.0, gamma=0.5, smooth=1.2, theta=55, phi=90, radius=0.8, label="Ciliwung, Cisadane, Bekasi"),
    "barito-2021": dict(
        center=(114.60, -3.42), half_km=48.0, rivers=["barito-banjarmasin"],
        exag=40.0, zscale=6.0, land_max=250.0, gamma=0.5, smooth=2.0, theta=55, phi=90, radius=0.8, label="Barito, Martapura"),
    "pantura-wulan-2024": dict(
        center=(110.78, -6.80), half_km=40.0, rivers=["wulan-demak"],
        exag=14.0, zscale=3.6, land_max=600.0, gamma=0.55, smooth=1.2, theta=55, phi=90, radius=0.7, label="Wulan, Serang"),
}
FILES = {"jabodetabek-2020": "replay-jabodetabek-2020", "barito-2021": "replay-barito-2021",
         "pantura-wulan-2024": "replay-pantura-wulan-2024"}


def build_scene(key, sc, a, idx):
    lon0, lat0 = sc["center"]
    zone = int((lon0 + 180) // 6) + 1
    epsg = 32700 + zone if lat0 < 0 else 32600 + zone
    to_utm = Transformer.from_crs(4326, epsg, always_xy=True)
    cx, cy = to_utm.transform(lon0, lat0)
    half = sc["half_km"] * 1000.0
    res = 2 * half / a.size
    n = a.size
    tr = from_origin(cx - half, cy + half, res, res)
    # bbox derajat jendela (+ sedikit margin) untuk memilih sel
    lons, lats = [], []
    inv = Transformer.from_crs(epsg, 4326, always_xy=True)
    for x, y in [(cx - half, cy - half), (cx + half, cy - half), (cx - half, cy + half), (cx + half, cy + half)]:
        lo, la = inv.transform(x, y)
        lons.append(lo)
        lats.append(la)
    bbox = (min(lons) - 0.01, min(lats) - 0.01, max(lons) + 0.01, max(lats) + 0.01)
    items = idx.items_for_bbox(bbox)
    print(f"[{key}] sel GEM-15: {[i['id'] for i in items]}, res {res:.0f} m")
    gem = gem15.read_into(items, epsg, tr, (n, n))
    print(f"[{key}] GEM-15 valid: {np.isfinite(gem).mean():.1%}, h {np.nanmin(gem):.1f}..{np.nanmax(gem):.1f}")

    # Terrarium z10 untuk celah dan laut
    dem, info = fd.fetch(bbox, 10, Path(a.cache))
    ter = np.full((n, n), np.nan, np.float32)
    reproject(source=dem, destination=ter,
              src_transform=from_origin(info["left"], info["top"], info["res"], info["res"]), src_crs=CRS.from_epsg(3857),
              dst_transform=tr, dst_crs=CRS.from_epsg(epsg), resampling=Resampling.bilinear,
              src_nodata=None, dst_nodata=np.nan)
    valid = np.isfinite(gem)
    # laut: Terrarium <= 0 dan GEM tanpa data -> permukaan laut 0 m (laut ditandai terpisah)
    sea = (~valid) & (np.nan_to_num(ter, nan=0.0) <= 0.5)
    fill = (~valid) & ~sea
    # offset datum lokal antara GEM dan Terrarium pada piksel tumpang tindih (median), agar tambalan tidak melompat
    both = valid & np.isfinite(ter)
    off = float(np.median(gem[both] - ter[both])) if both.sum() > 100 else 0.0
    print(f"[{key}] offset median GEM - Terrarium: {off:+.2f} m; laut {sea.mean():.1%}, tambalan Terrarium {fill.mean():.1%}")
    z = np.where(valid, gem, np.where(fill, ter + off, 0.0)).astype(np.float32)
    # transisi lembut pada tambalan
    w = ndi.gaussian_filter((~valid).astype(np.float32), 1.5)
    zs = ndi.gaussian_filter(z, 1.5)
    z = np.where(valid, z * (1 - w) + zs * w, z)
    z = np.maximum(z, 0.0)
    seaf = ndi.gaussian_filter(sea.astype(np.float32), 0.8)

    # tint
    lt = (np.clip(z / sc["land_max"], 0, 1)) ** sc["gamma"]
    land_col = al.ramp(lt, al.LAND_STOPS)
    sea_col = np.broadcast_to(al.hex2rgb("#7FB8C6"), land_col.shape)
    col = land_col * (1 - seaf[..., None]) + sea_col * seaf[..., None]
    zg = ndi.gaussian_filter(z, sc["smooth"])
    hs, flat = al.multi_hillshade(zg, res, sc["exag"])
    shade = np.clip(0.84 + 1.2 * (hs - flat), 0.55, 1.25)[..., None]
    shade = shade * (1 - seaf[..., None]) + 1.0 * seaf[..., None]
    rgb = np.clip(col * shade, 0, 255)

    # sungai
    def px(lo, la):
        x, y = to_utm.transform(lo, la)
        return (np.asarray(x) - (cx - half)) / res, ((cy + half) - np.asarray(y)) / res

    feats = []
    for sid in sc["rivers"]:
        p = Path(a.rivers_dir) / f"{sid}.geojson"
        if p.exists():
            feats += al.load_rivers(p)
        else:
            print(f"[{key}] PERINGATAN: {p.name} belum ada (tanpa sungai)")
    img = al.draw_rivers(rgb, px, feats, scale=max(1.0, a.size / 1500.0), width=2.4)
    arr = np.asarray(img, np.float32)
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    d = np.minimum(np.minimum(xx, n - 1 - xx), np.minimum(yy, n - 1 - yy)) / (n * 0.06)
    t = np.clip(d, 0, 1)
    t = (t * t * (3 - 2 * t))[..., None]
    arr = arr * t + np.array(al.PAPER, np.float32) * (1 - t)
    work = Path(a.work)
    Image.fromarray(arr.astype(np.uint8), "RGB").save(work / f"{key}_overlay.png")
    al.write_scene_tif(ndi.gaussian_filter(z, sc["smooth"] * 0.7), res, work / f"{key}_scene.tif")
    json.dump(dict(res=res, n=n, gem_valid=float(valid.mean()), offset_m=off, cells=[i["id"] for i in items],
                   rivers=[f["properties"]["station_id"] for f in feats]),
              open(work / f"{key}_meta.json", "w"))
    return n, res


def export_webp(src, dst, size=(960, 540), max_kb=120):
    """Turunkan ke 960x540 dan cari kualitas webp tertinggi yang muat di batas ukuran."""
    img = Image.open(src).convert("RGB").resize(size, Image.LANCZOS)
    dst.parent.mkdir(parents=True, exist_ok=True)
    for q in range(92, 30, -4):
        img.save(dst, "WEBP", quality=q, method=6)
        if dst.stat().st_size <= max_kb * 1024:
            break
    print(f"[export] {dst.name} q={q} {dst.stat().st_size / 1024:.0f} KB")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--work", required=True)
    ap.add_argument("--cache", required=True, help="cache tile Terrarium")
    ap.add_argument("--stac-cache", required=True)
    ap.add_argument("--rivers-dir", default=str(REPO / "crates/flood-web/public/data/rivers"))
    ap.add_argument("--out-dir", default=None, help="jika diisi, simpan webp final di sini")
    ap.add_argument("--only", default=None)
    ap.add_argument("--size", type=int, default=1600)
    ap.add_argument("--stage", default="all", choices=["prep", "render", "all"])
    a = ap.parse_args()
    Path(a.work).mkdir(parents=True, exist_ok=True)
    idx = gem15.Index(Path(a.stac_cache)) if a.stage in ("prep", "all") else None
    keys = [a.only] if a.only else list(SCENARIOS)
    for key in keys:
        sc = SCENARIOS[key]
        if a.stage in ("prep", "all"):
            build_scene(key, sc, a, idx)
        if a.stage in ("render", "all"):
            meta = json.load(open(Path(a.work) / f"{key}_meta.json"))
            n = meta["n"]
            v = al.Viewer(Path(a.work) / f"{key}_scene.tif", Path(a.work) / f"{key}_overlay.png", 960, 540, sc["zscale"])
            try:
                v.shot(Path(a.work) / f"{key}_raw.png", sc["phi"], sc["theta"], n * sc["radius"], 1920, 1080, settle=1.5)
            finally:
                v.close()
            print(f"[{key}] render ok")
            if a.out_dir:
                export_webp(Path(a.work) / f"{key}_raw.png", Path(a.out_dir) / f"{FILES[key]}.webp")


if __name__ == "__main__":
    main()
