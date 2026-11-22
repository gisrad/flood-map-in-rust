#!/usr/bin/env python3
"""Hero nusantara: relief + batimetri (Terrarium) -> forge3D viewer -> hero-indonesia.webp + og-flood.jpg.

Pemakaian (venv dengan forge3d, rasterio, scipy, pillow, requests, fonttools, brotli):
  python scripts/art/render_hero.py --work <DIR> --cache <DIR tiles> [--rivers rivers.geojson]
      [--out-dir <DIR>] [--zoom 7] [--phi 180 --theta 48 --radius R] [--stage prep|render|compose|all]
Keluaran antara di --work; hasil akhir di --out-dir (default crates/flood-web/public/art).
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import artlib as al  # noqa: E402
import fetch_dem as fd  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
CENTER = (118.0, -3.0)  # lon, lat pusat bingkai persegi
HALF_DEG = 31.0


def export_webp(src, dst, size=(2400, 1350), max_kb=400):
    """Simpan webp 2400x1350 pada kualitas tertinggi yang muat di batas ukuran."""
    img = Image.open(src).convert("RGB")
    if img.size != size:
        img = img.resize(size, Image.LANCZOS)
    dst.parent.mkdir(parents=True, exist_ok=True)
    for q in range(90, 30, -3):
        img.save(dst, "WEBP", quality=q, method=6)
        if dst.stat().st_size <= max_kb * 1024:
            break
    print(f"[export] {dst.name} q={q} {dst.stat().st_size / 1024:.0f} KB")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--work", required=True)
    ap.add_argument("--cache", required=True)
    ap.add_argument("--rivers", default=str(REPO / "crates/flood-web/public/data/rivers.geojson"))
    ap.add_argument("--zoom", type=int, default=7)
    ap.add_argument("--exag", type=float, default=3.0, help="eksagerasi hillshade darat")
    ap.add_argument("--zscale", type=float, default=8.0, help="z-scale viewer")
    ap.add_argument("--land-sigma", type=float, default=1.1)
    ap.add_argument("--stage", default="all", choices=["prep", "render", "all"])
    ap.add_argument("--shots", default="hero", help="nama bidikan, dipisah koma")
    ap.add_argument("--out-dir", default=None, help="jika diisi, simpan hero-indonesia.webp di sini")
    a = ap.parse_args()
    work = Path(a.work)
    work.mkdir(parents=True, exist_ok=True)

    scene, overlay = work / "hero_scene.tif", work / "hero_overlay.png"
    if a.stage in ("prep", "all"):
        dem, info = fd.fetch((CENTER[0] - HALF_DEG - 2, CENTER[1] - HALF_DEG - 3, CENTER[0] + HALF_DEG + 2, CENTER[1] + HALF_DEG + 3),
                             a.zoom, Path(a.cache))
        cx, cy = fd.lonlat_to_px(CENTER[0], CENTER[1], info)
        half = int(round(HALF_DEG * 111319.49 / info["res"]))
        x0, y0 = int(round(cx)) - half, int(round(cy)) - half
        x0, y0 = max(x0, 0), max(y0, 0)
        raw = dem[y0 : y0 + 2 * half, x0 : x0 + 2 * half]
        n = min(raw.shape)
        raw = raw[:n, :n]
        info2 = dict(info, left=info["left"] + x0 * info["res"], top=info["top"] - y0 * info["res"])
        res = info["res"]
        print(f"scene {raw.shape}, res {res:.0f} m, h {raw.min():.0f}..{raw.max():.0f}")
        z = al.prepare_dem(raw, res, land_sigma=a.land_sigma)
        rgb = al.make_tint(raw, z, res, land_exag=a.exag, sea_exag=18.0)
        feats = al.load_rivers(a.rivers)
        img = al.draw_rivers(rgb, lambda lo, la: fd.lonlat_to_px(lo, la, info2), feats, scale=1.0, width=2.2)
        # vinyet halus ke warna kertas di tepi (bingkai tanpa tepi keras)
        arr = np.asarray(img, np.float32)
        yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
        d = np.minimum(np.minimum(xx, n - 1 - xx), np.minimum(yy, n - 1 - yy)) / (n * 0.07)
        t = np.clip(d, 0, 1)
        t = (t * t * (3 - 2 * t))[..., None]
        arr = arr * t + np.array(al.PAPER, np.float32) * (1 - t)
        Image.fromarray(arr.astype(np.uint8), "RGB").save(overlay)
        al.write_scene_tif(z, res, scene)
        np.save(work / "hero_meta.npy", np.array([n, res]))
        print("prep selesai:", scene, overlay)

    if a.stage in ("render", "all"):
        n, res = np.load(work / "hero_meta.npy")
        n = int(n)
        shots = {
            "hero": dict(phi=90, theta=55, radius=n * 0.53, w=2400, h=1350),
            "wide": dict(phi=90, theta=45, radius=n * 0.75, w=2400, h=1350),
            "top": dict(phi=90, theta=70, radius=n * 0.5, w=2400, h=1350),
        }
        v = al.Viewer(scene, overlay, 1200, 675, a.zscale)
        try:
            for name in a.shots.split(","):
                s = shots[name]
                v.shot(work / f"hero_{name}.png", s["phi"], s["theta"], s["radius"], s["w"], s["h"], settle=1.5)
                print("bidikan", name)
                if name == "hero" and a.out_dir:
                    export_webp(work / "hero_hero.png", Path(a.out_dir) / "hero-indonesia.webp")
        finally:
            v.close()


if __name__ == "__main__":
    main()
