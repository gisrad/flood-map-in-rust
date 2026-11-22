"""Akses GEM-15 DTM (CC BY 4.0, \"This dataset is produced by Google\") lewat STAC publik, tanpa login.

Jalur: catalog.json -> collection.json -> katalog regional -> item sel 1 derajat -> aset `dtm`.
Nama sel tidak ditebak: daftar item dibaca dari katalog regional, bbox dan aset dari item JSON.
Jendela dibaca lewat rasterio (URL https), langsung diperhalus ke grid target (Resampling.average).
"""
from __future__ import annotations

import json
import re
from pathlib import Path
from urllib.parse import urljoin

import numpy as np
import requests
from concurrent.futures import ThreadPoolExecutor
from rasterio.crs import CRS
from rasterio.enums import Resampling
from rasterio.warp import reproject

ROOT = "https://storage.googleapis.com/climate-and-sustainability/gem-15/stac/catalog.json"
ATTRIBUTION = "This dataset is produced by Google (GEM-15, CC BY 4.0)"


def _get(url, cache: Path):
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


def _cell_key(name):
    m = re.fullmatch(r"w([mp]?)(\d+)s([mp]?)(\d+)", name)
    if not m:
        return None
    return (int(m.group(2)) * (-1 if m.group(1) == "m" else 1), int(m.group(4)) * (-1 if m.group(3) == "m" else 1))


class Index:
    def __init__(self, cache: Path):
        self.cache = Path(cache)
        self.cache.mkdir(parents=True, exist_ok=True)
        root = _get(ROOT, self.cache)
        cu = urljoin(ROOT, next(l["href"] for l in root["links"] if l["rel"] == "child"))
        coll = _get(cu, self.cache)
        regs = [urljoin(cu, l["href"]) for l in coll["links"] if l["rel"] == "child"]
        with ThreadPoolExecutor(8) as ex:
            cats = list(ex.map(lambda u: (u, _get(u, self.cache)), regs))
        self.cells = {}
        for u, c in cats:
            for l in (c or {}).get("links", []):
                if l["rel"] in ("item", "child"):
                    href = urljoin(u, l["href"])
                    k = _cell_key(Path(href).stem)
                    if k:
                        self.cells.setdefault(k, href)

    def items_for_bbox(self, bbox):
        """Item (id, bbox, epsg, dtm_url) untuk sel yang beririsan dengan bbox (w,s,e,n)."""
        w, s, e, n = bbox
        out = []
        for lon in range(int(np.floor(w)), int(np.ceil(e))):
            for lat in range(int(np.floor(s)), int(np.ceil(n))):
                href = self.cells.get((lon, lat))
                if not href:
                    continue
                it = _get(href, self.cache)
                if not it or "dtm" not in it.get("assets", {}):
                    continue
                out.append(dict(id=it["id"], bbox=it["bbox"], epsg=it["properties"].get("proj:epsg"),
                                dtm_url=urljoin(href, it["assets"]["dtm"]["href"])))
        return out


def read_into(items, dst_crs, dst_transform, shape):
    """Muat DTM semua item ke grid target; piksel tanpa data = NaN. Rata-rata saat turun-skala."""
    import rasterio

    out = np.full(shape, np.nan, np.float32)
    for it in items:
        with rasterio.open(it["dtm_url"]) as ds:
            tmp = np.full(shape, np.nan, np.float32)
            reproject(
                source=rasterio.band(ds, 1), destination=tmp,
                dst_transform=dst_transform, dst_crs=CRS.from_user_input(dst_crs),
                src_nodata=np.nan, dst_nodata=np.nan, resampling=Resampling.average,
            )
        m = np.isfinite(tmp)
        out[m] = tmp[m]
    return out
