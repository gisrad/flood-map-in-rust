#!/usr/bin/env python3
"""Unduh AWS Terrain Tiles (Terrarium) dan rakit menjadi GeoTIFF float32 (EPSG:3857, meter).

Terrarium: h = R*256 + G + B/256 - 32768 (meter; termasuk batimetri).
Sumber: https://registry.opendata.aws/terrain-tiles/  (atribusi: lihat docs/ART_PIPELINE.md)

Pemakaian:
  python scripts/art/fetch_dem.py --bbox 94 -12 142 8 --zoom 7 --cache <DIR> --out dem.tif
  (bbox = west south east north, derajat)
"""
from __future__ import annotations

import argparse
import math
from concurrent.futures import ThreadPoolExecutor
from io import BytesIO
from pathlib import Path

import numpy as np
import requests
from PIL import Image

URL = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png"
R_EARTH = 6378137.0
ORIGIN = math.pi * R_EARTH  # 20037508.34 m


def lonlat_to_tile(lon, lat, z):
    n = 2**z
    x = (lon + 180.0) / 360.0 * n
    lat = max(min(lat, 85.0511), -85.0511)
    y = (1.0 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2.0 * n
    return x, y


def get_tile(z, x, y, cache: Path) -> np.ndarray:
    f = cache / f"{z}_{x}_{y}.png"
    if not f.exists():
        for attempt in range(5):
            try:
                r = requests.get(URL.format(z=z, x=x, y=y), timeout=60,
                                 headers={"User-Agent": "flood-transbulent-art/1.0"})
                if r.status_code == 200:
                    f.write_bytes(r.content)
                    break
                if r.status_code == 404:  # tile kosong (laut terbuka jauh) -> 0 m
                    return np.zeros((256, 256), np.float32)
            except requests.RequestException:
                pass
        else:
            raise RuntimeError(f"gagal unduh tile {z}/{x}/{y}")
    a = np.asarray(Image.open(f).convert("RGB"), dtype=np.float32)
    return a[..., 0] * 256.0 + a[..., 1] + a[..., 2] / 256.0 - 32768.0


def fetch(bbox, zoom, cache: Path, workers=8):
    """Kembalikan (dem[H,W] float32, transform_info) untuk bbox (w,s,e,n), piksel mengikuti grid tile."""
    cache.mkdir(parents=True, exist_ok=True)
    w, s, e, n = bbox
    x0f, y1f = lonlat_to_tile(w, s, zoom)
    x1f, y0f = lonlat_to_tile(e, n, zoom)
    x0, x1 = int(math.floor(x0f)), int(math.floor(x1f))
    y0, y1 = int(math.floor(y0f)), int(math.floor(y1f))
    jobs = [(x, y) for y in range(y0, y1 + 1) for x in range(x0, x1 + 1)]
    with ThreadPoolExecutor(workers) as ex:
        tiles = dict(zip(jobs, ex.map(lambda t: get_tile(zoom, t[0], t[1], cache), jobs)))
    W = (x1 - x0 + 1) * 256
    H = (y1 - y0 + 1) * 256
    mos = np.zeros((H, W), np.float32)
    for (x, y), t in tiles.items():
        mos[(y - y0) * 256 : (y - y0 + 1) * 256, (x - x0) * 256 : (x - x0 + 1) * 256] = t
    # crop tepat ke bbox
    px0, px1 = int(round((x0f - x0) * 256)), int(round((x1f - x0) * 256))
    py0, py1 = int(round((y0f - y0) * 256)), int(round((y1f - y0) * 256))
    dem = mos[py0:py1, px0:px1]
    res = 2 * ORIGIN / (256 * 2**zoom)
    left = -ORIGIN + (x0 + px0 / 256.0) * 256 * res
    top = ORIGIN - (y0 + py0 / 256.0) * 256 * res
    return dem, dict(left=left, top=top, res=res, zoom=zoom, bbox=bbox)


def lonlat_to_px(lon, lat, info):
    """lon/lat (array) -> piksel (x, y) pada grid hasil fetch()."""
    lon = np.asarray(lon, float)
    lat = np.asarray(lat, float)
    mx = np.radians(lon) * R_EARTH
    my = np.log(np.tan(np.pi / 4 + np.radians(lat) / 2)) * R_EARTH
    return (mx - info["left"]) / info["res"], (info["top"] - my) / info["res"]


def write_tif(dem, info, path):
    import rasterio
    from rasterio.transform import from_origin

    with rasterio.open(
        path, "w", driver="GTiff", height=dem.shape[0], width=dem.shape[1], count=1,
        dtype="float32", crs="EPSG:3857", transform=from_origin(info["left"], info["top"], info["res"], info["res"]),
        nodata=-9999.0,
    ) as dst:
        dst.write(dem.astype(np.float32), 1)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--bbox", nargs=4, type=float, required=True, metavar=("W", "S", "E", "N"))
    ap.add_argument("--zoom", type=int, required=True)
    ap.add_argument("--cache", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    d, inf = fetch(a.bbox, a.zoom, Path(a.cache))
    write_tif(d, inf, a.out)
    print(f"{a.out}: {d.shape[1]}x{d.shape[0]} px, res {inf['res']:.0f} m, h {d.min():.0f}..{d.max():.0f} m")
