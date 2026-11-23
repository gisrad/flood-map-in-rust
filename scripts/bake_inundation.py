#!/usr/bin/env python3
"""Riwayat genangan Google (Inundation History, 1999–2020) untuk lapisan peta dan paparan per titik.

Sumber: gs://flood-forecasting/inundation_history/data (bucket publik, CC BY 4.0, Google Research;
dibuat dari citra satelit GLAD). Tiap berkas GeoJSON ≈ 83.000 km² memuat tiga lapisan bersarang:
`High_risk` (basah ≥ 5% waktu), `Medium_risk` (≥ 1%), `Low_risk` (≥ 0,5%), piksel 128 m, tanpa badan
air permanen. Daerah di luar semua poligon basah < 0,5% waktu. README: inundation_history/README.txt.

Keluaran:
  crates/flood-web/public/data/inundation/overview.json   gambaran seluruh Indonesia (zoom rendah)
  crates/flood-web/public/data/inundation/t/<lat>_<lon>.json   ubin 1° x 1° (zoom tinggi)
  crates/flood-web/public/data/inundation/index.json      daftar ubin + metadata
  crates/flood-shared/data/inundation_exposure.json       luas genangan historis dalam radius 5 km per titik

Pemakaian:
  python scripts/bake_inundation.py --cache-dir <dir>
Dibutuhkan: shapely>=2, pyproj, numpy. Unduhan (≈ 77 MB) disimpan di <dir>/inundation_raw.
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import datetime as dt
import json
import math
import re
import sys
import urllib.parse
import urllib.request
from pathlib import Path

import numpy as np
import shapely
from pyproj import Transformer
from shapely.geometry import MultiPolygon, Point, box, mapping, shape
from shapely.strtree import STRtree

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calibrate_glofas import parse_catalog  # noqa: E402
from calibrate_rob import parse_rob  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
CATALOG_RS = REPO / "crates/flood-shared/src/catalog.rs"
OUT_WEB = REPO / "crates/flood-web/public/data/inundation"
OUT_EXPOSURE = REPO / "crates/flood-shared/data/inundation_exposure.json"

BUCKET = "flood-forecasting"
PREFIX = "inundation_history/data/"
# lon_min, lat_min, lon_max, lat_max: Indonesia dan sekitarnya.
BBOX = (94.5, -11.5, 141.2, 6.5)
# (kelas, ambang waktu basah %, nama lapisan sumber)
CLASSES = [("low", 0.5, "Low_risk"), ("medium", 1.0, "Medium_risk"), ("high", 5.0, "High_risk")]
# toleransi penyederhanaan (derajat) dan luas minimum bagian (derajat²), per kelas.
OVERVIEW = {"low": (0.015, 1.2e-3), "medium": (0.012, 6e-4), "high": (0.008, 3e-4)}
DETAIL = {"low": (0.0060, 2e-4), "medium": (0.0040, 8e-5), "high": (0.0020, 2.5e-5)}
OVERVIEW_GRID = 1e-3  # presisi koordinat ≈ 110 m
DETAIL_GRID = 1e-4  # ≈ 11 m
TILE_DEG = 1
EXPOSURE_RADIUS_KM = 5.0


def list_cells() -> list[tuple[str, int, tuple[float, float, float, float]]]:
    pat = re.compile(r"inundation_history_(-?[\d.]+)_(-?[\d.]+)_(-?[\d.]+)_(-?[\d.]+)\.geojson$")
    out, token = [], None
    while True:
        url = (
            f"https://storage.googleapis.com/storage/v1/b/{BUCKET}/o?prefix={PREFIX}"
            "&maxResults=1000&fields=nextPageToken,items(name,size)"
        )
        if token:
            url += "&pageToken=" + urllib.parse.quote(token)
        d = json.loads(urllib.request.urlopen(url, timeout=60).read())
        for it in d.get("items", []):
            m = pat.search(it["name"])
            if not m:
                continue
            lat0, lon0, lat1, lon1 = map(float, m.groups())
            if lat1 >= BBOX[1] and lat0 <= BBOX[3] and lon1 >= BBOX[0] and lon0 <= BBOX[2]:
                out.append((it["name"], int(it["size"]), (lat0, lon0, lat1, lon1)))
        token = d.get("nextPageToken")
        if not token:
            return out


def download(cache: Path, cells) -> list[Path]:
    cache.mkdir(parents=True, exist_ok=True)

    def one(c):
        name, size, _ = c
        fn = cache / Path(name).name
        if fn.exists() and fn.stat().st_size == size:
            return fn
        url = f"https://storage.googleapis.com/{BUCKET}/" + urllib.parse.quote(name)
        fn.write_bytes(urllib.request.urlopen(url, timeout=180).read())
        return fn

    with cf.ThreadPoolExecutor(8) as ex:
        return list(ex.map(one, cells))


def load_cells(files: list[Path]) -> dict[str, list]:
    """kelas → daftar poligon (lon/lat, terpotong BBOX)."""
    bb = box(*BBOX)
    parts: dict[str, list] = {c: [] for c, _, _ in CLASSES}
    src_name = {n: c for c, _, n in CLASSES}
    for fn in files:
        if fn.stat().st_size < 1000:  # sel laut tanpa poligon
            continue
        for f in json.loads(fn.read_text(encoding="utf-8"))["features"]:
            cls = src_name.get(f["properties"].get("name"))
            if not cls:
                continue
            g = shape(f["geometry"]).intersection(bb)
            if g.is_empty:
                continue
            parts[cls].extend(p for p in shapely.get_parts(g) if p.geom_type == "Polygon")
    return parts


def reduce_polys(polys: list, tol: float, min_area: float, grid: float) -> list:
    """Sederhanakan, buang bagian kecil, snap ke grid."""
    out = []
    for p in polys:
        if p.area < min_area:
            continue
        q = p.simplify(tol, preserve_topology=True)
        q = shapely.set_precision(q, grid)
        if q.is_empty:
            continue
        out.extend(x for x in shapely.get_parts(q) if x.geom_type == "Polygon" and x.area >= min_area * 0.25)
    return out


def feature(cls: str, polys: list) -> dict:
    return {"type": "Feature", "properties": {"c": cls}, "geometry": mapping(MultiPolygon(polys))}


def write_json(path: Path, obj) -> int:
    path.parent.mkdir(parents=True, exist_ok=True)
    s = json.dumps(obj, separators=(",", ":"), ensure_ascii=False)
    path.write_text(s, encoding="utf-8")
    return len(s.encode("utf-8"))


def tiles(parts: dict[str, list]) -> tuple[dict[str, int], int]:
    """Ubin 1° detail. Mengembalikan {kunci ubin: ukuran byte}."""
    buckets: dict[tuple[int, int], dict[str, list]] = {}
    for cls, _, _ in CLASSES:
        tol, min_area = DETAIL[cls]
        for p in reduce_polys(parts[cls], tol, min_area, DETAIL_GRID):
            x0, y0, x1, y1 = p.bounds
            for ty in range(math.floor(y0 / TILE_DEG), math.floor(y1 / TILE_DEG) + 1):
                for tx in range(math.floor(x0 / TILE_DEG), math.floor(x1 / TILE_DEG) + 1):
                    if (tx + 1) * TILE_DEG <= x0 or tx * TILE_DEG >= x1 or (ty + 1) * TILE_DEG <= y0 or ty * TILE_DEG >= y1:
                        continue
                    c = shapely.clip_by_rect(p, tx * TILE_DEG, ty * TILE_DEG, (tx + 1) * TILE_DEG, (ty + 1) * TILE_DEG)
                    if c.is_empty:
                        continue
                    c = shapely.set_precision(c, DETAIL_GRID)
                    for x in shapely.get_parts(c):
                        if x.geom_type == "Polygon" and not x.is_empty:
                            buckets.setdefault((ty, tx), {}).setdefault(cls, []).append(x)
    out_dir = OUT_WEB / "t"
    if out_dir.exists():
        for f in out_dir.glob("*.json"):
            f.unlink()
    sizes = {}
    for (ty, tx), by_cls in sorted(buckets.items()):
        feats = [feature(cls, by_cls[cls]) for cls, _, _ in CLASSES if by_cls.get(cls)]
        if not feats:
            continue
        key = f"{ty}_{tx}"
        sizes[key] = write_json(out_dir / f"{key}.json", {"type": "FeatureCollection", "features": feats})
    return sizes, sum(sizes.values())


def overview(parts: dict[str, list]) -> int:
    feats = []
    for cls, _, _ in CLASSES:
        tol, min_area = OVERVIEW[cls]
        polys = reduce_polys(parts[cls], tol, min_area, OVERVIEW_GRID)
        if polys:
            feats.append(feature(cls, polys))
    return write_json(OUT_WEB / "overview.json", {"type": "FeatureCollection", "features": feats})


def exposure(parts: dict[str, list], points: list[dict]) -> dict:
    """Luas (km²) tiap kelas dalam radius EXPOSURE_RADIUS_KM; geometri asli, proyeksi LAEA setempat."""
    trees = {c: (STRtree(parts[c]), parts[c]) for c, _, _ in CLASSES}
    out = {}
    for p in points:
        laea = Transformer.from_crs(
            "EPSG:4326",
            f"+proj=laea +lat_0={p['lat']} +lon_0={p['lon']} +datum=WGS84 +units=m",
            always_xy=True,
        )
        circle = Point(0, 0).buffer(EXPOSURE_RADIUS_KM * 1000, 128)
        dlat = EXPOSURE_RADIUS_KM / 111.0 * 1.2
        dlon = dlat / max(math.cos(math.radians(p["lat"])), 0.2)
        window = box(p["lon"] - dlon, p["lat"] - dlat, p["lon"] + dlon, p["lat"] + dlat)
        rec = {"buffer_km2": round(circle.area / 1e6, 1)}
        for cls, _, _ in CLASSES:
            tree, geoms = trees[cls]
            area = 0.0
            for i in tree.query(window):
                g = shapely.transform(geoms[i], lambda a: np.column_stack(laea.transform(a[:, 0], a[:, 1])))
                area += g.intersection(circle).area
            rec[f"{cls}_km2"] = round(area / 1e6, 1)
        out[p["id"]] = rec
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache-dir", required=True, type=Path)
    args = ap.parse_args()

    cells = list_cells()
    files = download(args.cache_dir / "inundation_raw", cells)
    print(f"{len(cells)} sel sumber, {sum(c[1] for c in cells) / 1e6:.1f} MB", flush=True)
    parts = load_cells(files)
    for c, _, _ in CLASSES:
        print(f"  {c}: {len(parts[c])} bagian", flush=True)

    ov = overview(parts)
    sizes, detail_total = tiles(parts)
    now = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    write_json(
        OUT_WEB / "index.json",
        {
            "generated_at_utc": now,
            "source": "Google Research, Inundation History (gs://flood-forecasting/inundation_history), CC BY 4.0, "
            "berdasarkan GLAD (Global Land Analysis & Discovery)",
            "period": "1999–2020",
            "pixel_m": 128,
            "classes": [{"id": c, "min_wet_percent": t} for c, t, _ in CLASSES],
            "bbox": list(BBOX),
            "tile_deg": TILE_DEG,
            "overview_bytes": ov,
            "tiles": sorted(sizes),
        },
    )
    print(f"overview {ov / 1e6:.2f} MB, {len(sizes)} ubin, detail {detail_total / 1e6:.2f} MB", flush=True)

    points = [
        {"id": s["id"], "lat": float(s["lat"]), "lon": float(s["lon"])} for s in parse_catalog(CATALOG_RS)
    ] + [{"id": r["id"], "lat": float(r["lat"]), "lon": float(r["lon"])} for r in parse_rob(CATALOG_RS)]
    exp = exposure(parts, points)
    write_json(
        OUT_EXPOSURE,
        {
            "generated_at_utc": now,
            "source": "Google Research, Inundation History 1999–2020 (CC BY 4.0), berdasarkan GLAD",
            "radius_km": EXPOSURE_RADIUS_KM,
            "classes": {c: f"basah ≥ {t:g}% waktu" for c, t, _ in CLASSES},
            "points": exp,
        },
    )
    OUT_EXPOSURE.write_text(
        json.dumps(json.loads(OUT_EXPOSURE.read_text(encoding="utf-8")), ensure_ascii=False, indent=1) + "\n",
        encoding="utf-8",
    )
    n_hit = sum(1 for v in exp.values() if v["low_km2"] > 0)
    print(f"paparan: {len(exp)} titik, {n_hit} dengan genangan historis dalam {EXPOSURE_RADIUS_KM:g} km", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
