#!/usr/bin/env python3
"""QA geometri sungai: ikhtisar semua stasiun + zoom dekat di atas tile basemap OSM.

  python scripts/rivers/qa_rivers.py --out <DIR> [--zoom-stations ciliwung-manggarai,barito-banjarmasin,krueng-aceh]
         [--tile-zoom 14] [--cache <DIR tile OSM>]

Tile OSM hanya beberapa lembar per zoom (pemakaian wajar; User-Agent jelas), dicache.
Keluaran: overview.png, zoom_<station_id>.png (detail LOD tergambar di atas basemap).
"""
from __future__ import annotations

import argparse
import json
import math
from io import BytesIO
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
import requests  # noqa: E402
from PIL import Image  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
DATA = REPO / "crates/flood-web/public/data"
UA = {"User-Agent": "flood-transbulent-qa/1.0 (https://flood.transbulent.id; riset hidrologi)"}


def tile_xy(lon, lat, z):
    n = 2**z
    return (lon + 180) / 360 * n, (1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n


def merc_to_lonlat(x, y, z):
    n = 2**z
    lon = x / n * 360 - 180
    lat = math.degrees(math.atan(math.sinh(math.pi * (1 - 2 * y / n))))
    return lon, lat


def fetch_tile(z, x, y, cache: Path):
    f = cache / f"{z}_{x}_{y}.png"
    if not f.exists():
        r = requests.get(f"https://tile.openstreetmap.org/{z}/{x}/{y}.png", headers=UA, timeout=30)
        r.raise_for_status()
        f.write_bytes(r.content)
    return Image.open(f).convert("RGB")


def lines_of(feature):
    g = feature["geometry"]
    return [g["coordinates"]] if g["type"] == "LineString" else g["coordinates"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--zoom-stations", default="ciliwung-manggarai,barito-banjarmasin,krueng-aceh")
    ap.add_argument("--tile-zoom", type=int, default=14)
    ap.add_argument("--cache", default=None)
    ap.add_argument("--span", type=int, default=3, help="jumlah tile per sisi")
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    cache = Path(a.cache) if a.cache else out / "osm_tiles"
    cache.mkdir(parents=True, exist_ok=True)

    ov = json.loads((DATA / "rivers.geojson").read_text(encoding="utf8"))
    fig, ax = plt.subplots(figsize=(16, 7), dpi=110)
    for f in ov["features"]:
        for ln in lines_of(f):
            xs, ys = zip(*ln)
            ax.plot(xs, ys, lw=0.8, color="#2F7FC1")
    ax.set_aspect("equal")
    ax.set_title(f"Ikhtisar: {len(ov['features'])} stasiun bergeometri")
    fig.savefig(out / "overview.png", bbox_inches="tight")
    plt.close(fig)

    for sid in a.zoom_stations.split(","):
        p = DATA / "rivers" / f"{sid}.geojson"
        if not p.exists():
            print("tidak ada detail", sid)
            continue
        f = json.loads(p.read_text(encoding="utf8"))["features"][0]
        # stasiun tengah: titik rantai terdekat ke titik tengah ikhtisar
        pts = np.array([c for ln in lines_of(f) for c in ln])
        meta = {s["id"]: s for s in json.loads((out / "stations.json").read_text(encoding="utf8"))} if (out / "stations.json").exists() else {}
        if sid in meta:
            lon0, lat0 = meta[sid]["lon"], meta[sid]["lat"]
        else:
            lon0, lat0 = np.median(pts[:, 0]), np.median(pts[:, 1])
        z = a.tile_zoom
        cx, cy = tile_xy(lon0, lat0, z)
        # pusatkan pada titik sungai terdekat ke pos
        k = int(np.argmin((pts[:, 0] - lon0) ** 2 + (pts[:, 1] - lat0) ** 2))
        cx, cy = tile_xy(pts[k, 0], pts[k, 1], z)
        x0, y0 = int(cx) - a.span // 2, int(cy) - a.span // 2
        img = Image.new("RGB", (256 * a.span, 256 * a.span))
        for i in range(a.span):
            for j in range(a.span):
                img.paste(fetch_tile(z, x0 + i, y0 + j, cache), (i * 256, j * 256))
        fig, ax = plt.subplots(figsize=(10, 10), dpi=100)
        ax.imshow(img)
        for ln in lines_of(f):
            xy = np.array([tile_xy(lo, la, z) for lo, la in ln])
            ax.plot((xy[:, 0] - x0) * 256, (xy[:, 1] - y0) * 256, color="#FF2D55", lw=1.4, alpha=0.9)
        ax.set_xlim(0, 256 * a.span)
        ax.set_ylim(256 * a.span, 0)
        ax.set_title(f"{sid}  (detail, z{z})")
        ax.axis("off")
        fig.savefig(out / f"zoom_{sid}.png", bbox_inches="tight")
        plt.close(fig)
        print("zoom", sid)


if __name__ == "__main__":
    main()
