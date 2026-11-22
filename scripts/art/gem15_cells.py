#!/usr/bin/env python3
"""Petakan stasiun -> sel GEM-15 (1 derajat) lewat STAC publik. Tanpa menebak nama sel.

Jalur: catalog.json -> collection.json -> katalog regional (child) -> item sel (link 'item').
Nama sel pada href hanya dipakai untuk menyaring kandidat; bbox dikonfirmasi dari item JSON,
dan aset `dtm` diambil dari item. Stasiun di luar cakupan: null.

Keluaran: crates/flood-web/public/data/gem15-cells.json
  { "<station_id>": {"cell_id","item_url","dtm_url","bbox","epsg"} | null, ... }
Atribusi (CC BY 4.0): "This dataset is produced by Google" (GEM-15).

Pemakaian: python scripts/art/gem15_cells.py --cache <DIR>
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from urllib.parse import urljoin

import requests

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts/rivers"))
from bake_rivers import read_catalog  # noqa: E402

ROOT = "https://storage.googleapis.com/climate-and-sustainability/gem-15/stac/catalog.json"
OUT = REPO / "crates/flood-web/public/data/gem15-cells.json"


def get_json(url: str, cache: Path):
    f = cache / (re.sub(r"[^A-Za-z0-9]+", "_", url)[-150:] + ".json")
    if f.exists():
        return json.loads(f.read_text(encoding="utf8"))
    for _ in range(4):
        try:
            r = requests.get(url, timeout=60)
            if r.status_code == 200:
                f.write_text(r.text, encoding="utf8")
                return r.json()
        except requests.RequestException:
            pass
    return None


def parse_cell(name: str):
    """w110sm8 -> (110, -8): tepi barat/selatan sel; 'm' = minus (hanya penyaring kandidat)."""
    m = re.fullmatch(r"w([mp]?)(\d+)s([mp]?)(\d+)", name)
    if not m:
        return None
    lon = int(m.group(2)) * (-1 if m.group(1) == "m" else 1)
    lat = int(m.group(4)) * (-1 if m.group(3) == "m" else 1)
    return lon, lat


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", required=True)
    ap.add_argument("--out", default=str(OUT))
    a = ap.parse_args()
    cache = Path(a.cache)
    cache.mkdir(parents=True, exist_ok=True)

    root = get_json(ROOT, cache)
    coll_url = urljoin(ROOT, next(l["href"] for l in root["links"] if l["rel"] == "child"))
    coll = get_json(coll_url, cache)
    regions = [urljoin(coll_url, l["href"]) for l in coll["links"] if l["rel"] == "child"]
    print(f"{len(regions)} katalog regional")
    with ThreadPoolExecutor(8) as ex:
        cats = list(ex.map(lambda u: (u, get_json(u, cache)), regions))
    cells = {}  # (lon,lat) -> list of item urls
    n_items = 0
    for u, c in cats:
        if not c:
            continue
        for l in c["links"]:
            if l["rel"] in ("item", "child"):
                href = urljoin(u, l["href"])
                name = Path(href).stem
                pc = parse_cell(name)
                if pc:
                    cells.setdefault(pc, []).append(href)
                    n_items += 1
    print(f"{n_items} item sel terdaftar di katalog regional")

    out = {}
    for st in read_catalog():
        key = (int(st["lon"] // 1), int(st["lat"] // 1))
        res = None
        for href in cells.get(key, []):
            item = get_json(href, cache)
            if not item or "bbox" not in item:
                continue
            w, s, e, n = item["bbox"]
            if not (w <= st["lon"] < e and s <= st["lat"] < n):
                continue
            dtm = item.get("assets", {}).get("dtm")
            if not dtm:
                continue
            res = {
                "cell_id": item["id"],
                "item_url": href,
                "dtm_url": urljoin(href, dtm["href"]),
                "bbox": [w, s, e, n],
                "epsg": item.get("properties", {}).get("proj:epsg"),
            }
            break
        out[st["id"]] = res
        print(f"  {st['id']}: {res['cell_id'] if res else 'tidak tercakup'}")
    Path(a.out).write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf8")
    print(f"{sum(v is not None for v in out.values())}/{len(out)} stasiun tercakup -> {a.out}")


if __name__ == "__main__":
    main()
