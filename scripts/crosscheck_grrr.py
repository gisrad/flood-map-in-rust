#!/usr/bin/env python3
"""Uji kewajaran silang ambang periode ulang GloFAS terhadap Google Runoff Reanalysis & Reforecast (GRRR).

Sumber: gs://flood-forecasting/hydrologic_predictions/distributed_model_2026_11 (bucket publik, CC BY 4.0,
Google Research): `return_periods.parquet` (debit periode ulang 1,01–100 tahun per sub-DAS HydroBASINS,
reanalisis 1980–2023) dan `subhybas_outlet_locations_UNOFFICIAL.zarr` (koordinat titik keluar sub-DAS).

Metode (untuk tiap stasiun dengan ambang GloFAS terkalibrasi):
  1. Ambang GloFAS Q2/Q5/Q20 dihitung dari `annual_max_m3s` dengan rumus yang sama seperti flood-shared.
  2. Kandidat = titik keluar GRRR dalam RADIUS_KM dari koordinat pos.
  3. `closest` = kandidat dengan Q2 terdekat (skala log) ke Q2 GloFAS; ini pencocokan yang bias menuju
     kecocokan (data GRRR tidak memuat luas DAS), sehingga hanya dipakai sebagai gambaran bentuk kurva.
  4. Putusan memakai seluruh kandidat dan tidak bergantung pada pemilihan `closest`:
       glofas_above : Q2 GloFAS > ABOVE_FACTOR × Q2 terbesar kandidat
       glofas_below : Q2 GloFAS < BELOW_FACTOR × Q2 terkecil kandidat
       within_range : selainnya;  no_outlets : tanpa kandidat.
Ini model terhadap model, bukan validasi terhadap pos duga air.

Keluaran: crates/flood-shared/data/grrr_crosscheck.json

Pemakaian:
  python scripts/crosscheck_grrr.py --cache-dir <dir>
Dibutuhkan: numpy, pandas, pyarrow, numcodecs (blosc).
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import statistics
import sys
import urllib.request
from pathlib import Path

import numpy as np
import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calibrate_glofas import gumbel_lmoments, gumbel_q, parse_catalog  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
CATALOG_RS = REPO / "crates/flood-shared/src/catalog.rs"
GLOFAS_JSON = REPO / "crates/flood-shared/data/glofas_calibration.json"
OUT_JSON = REPO / "crates/flood-shared/data/grrr_crosscheck.json"

BASE = "https://storage.googleapis.com/flood-forecasting/hydrologic_predictions/distributed_model_2026_11/"
MODEL = "distributed_model_2026_11"
BBOX = (94.5, -11.5, 141.2, 6.5)  # lon_min, lat_min, lon_max, lat_max
RADIUS_KM = 12.0
ABOVE_FACTOR = 2.0
BELOW_FACTOR = 0.5
MIN_ANNUAL_MAXIMA = 20


def haversine_vec(lat1: float, lon1: float, lat2: np.ndarray, lon2: np.ndarray) -> np.ndarray:
    """Jarak (km) satu titik ke larik titik."""
    p = np.pi / 180
    a = np.sin((lat2 - lat1) * p / 2) ** 2 + math.cos(lat1 * p) * np.cos(lat2 * p) * np.sin((lon2 - lon1) * p / 2) ** 2
    return 12742.0 * np.arcsin(np.sqrt(a))


def fetch(cache: Path, rel: str) -> bytes:
    fn = cache / rel.replace("/", "__")
    if not fn.exists():
        fn.write_bytes(urllib.request.urlopen(BASE + rel, timeout=300).read())
    return fn.read_bytes()


def load_outlets(cache: Path) -> pd.DataFrame:
    import numcodecs

    zarr = "subhybas_outlet_locations_UNOFFICIAL.zarr/"
    blosc, vlen = numcodecs.Blosc(), numcodecs.VLenUTF8()

    def meta(var):
        z = json.loads(fetch(cache, zarr + f"{var}/.zarray"))
        return z["shape"][0], z["chunks"][0]

    n, ch = meta("gauge_id")
    ids: list[str] = []
    for i in range(math.ceil(n / ch)):
        ids += list(vlen.decode(blosc.decode(fetch(cache, zarr + f"gauge_id/{i}"))))

    def floats(var):
        n_, ch_ = meta(var)
        a = [np.frombuffer(blosc.decode(fetch(cache, zarr + f"{var}/{i}")), dtype="<f8") for i in range(math.ceil(n_ / ch_))]
        return np.concatenate(a)[:n_]

    return pd.DataFrame({"gauge_id": ids[:n], "lat": floats("latitude")[:n], "lon": floats("longitude")[:n]})


def load_return_periods(cache: Path) -> pd.DataFrame:
    fn = cache / "return_periods.parquet"
    if not fn.exists():
        fn.write_bytes(urllib.request.urlopen(BASE + "return_periods.parquet", timeout=600).read())
    rp = pd.read_parquet(fn)
    rp.columns = [str(c) for c in rp.columns]
    return rp.reset_index() if "gauge_id" not in rp.columns else rp


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache-dir", required=True, type=Path)
    args = ap.parse_args()
    args.cache_dir.mkdir(parents=True, exist_ok=True)
    cache = args.cache_dir / "grrr"
    cache.mkdir(exist_ok=True)

    outlets = load_outlets(cache)
    rp = load_return_periods(cache)
    df = outlets.merge(rp, on="gauge_id", how="inner")
    df = df[(df.lon > BBOX[0]) & (df.lat > BBOX[1]) & (df.lon < BBOX[2]) & (df.lat < BBOX[3])].reset_index(drop=True)
    print(f"{len(df)} titik keluar GRRR di Indonesia dan sekitarnya", flush=True)

    glofas = {s["id"]: s for s in json.loads(GLOFAS_JSON.read_text(encoding="utf-8"))["stations"]}
    out, rows = {}, []
    for st in parse_catalog(CATALOG_RS):
        cal = glofas.get(st["id"])
        if not cal or not cal.get("accepted") or len(cal.get("annual_max_m3s", [])) < MIN_ANNUAL_MAXIMA:
            continue
        xi, alpha = gumbel_lmoments(np.array(cal["annual_max_m3s"], dtype=float))
        q2, q5, q20 = (gumbel_q(xi, alpha, t) for t in (2, 5, 20))
        d = haversine_vec(st["lat"], st["lon"], df.lat.values, df.lon.values)
        near = df[d <= RADIUS_KM].copy()
        near["d"] = d[d <= RADIUS_KM]
        if near.empty:
            out[st["id"]] = {"radius_km": RADIUS_KM, "n_outlets": 0, "verdict": "no_outlets"}
            rows.append((st["id"], q2, 0, None, None, "no_outlets"))
            continue
        lo, hi = float(near["2.0"].min()), float(near["2.0"].max())
        best = near.loc[(np.log(near["2.0"] / q2)).abs().idxmin()]
        verdict = "glofas_above" if q2 > ABOVE_FACTOR * hi else "glofas_below" if q2 < BELOW_FACTOR * lo else "within_range"
        out[st["id"]] = {
            "radius_km": RADIUS_KM,
            "n_outlets": int(len(near)),
            "q2_min_m3s": round(lo, 2),
            "q2_max_m3s": round(hi, 2),
            "closest": {
                "id": str(best["gauge_id"]),
                "distance_km": round(float(best["d"]), 1),
                "q2_m3s": round(float(best["2.0"]), 2),
                "q5_m3s": round(float(best["5.0"]), 2),
                "q20_m3s": round(float(best["20.0"]), 2),
            },
            "ratio_q2": round(q2 / float(best["2.0"]), 2),
            "verdict": verdict,
        }
        rows.append((st["id"], q2, len(near), lo, hi, verdict))

    counts = {v: sum(1 for r in rows if r[5] == v) for v in ("within_range", "glofas_above", "glofas_below", "no_outlets")}
    # Bentuk kurva hanya dari pasangan yang skalanya berdekatan (rasio Q2 0,5–2).
    pairs = [
        (st_id, v)
        for st_id, v in out.items()
        if v.get("closest") and 0.5 <= v["ratio_q2"] <= 2.0
    ]
    glofas_growth, grrr_growth = [], []
    for st_id, v in pairs:
        cal = glofas[st_id]
        xi, alpha = gumbel_lmoments(np.array(cal["annual_max_m3s"], dtype=float))
        g2 = gumbel_q(xi, alpha, 2)
        glofas_growth.append(gumbel_q(xi, alpha, 20) / g2)
        grrr_growth.append(v["closest"]["q20_m3s"] / v["closest"]["q2_m3s"])
    summary = {
        "n_assessed": len(rows),
        "verdicts": counts,
        "n_similar_scale": len(pairs),
        "median_q20_over_q2_glofas": round(statistics.median(glofas_growth), 2) if pairs else None,
        "median_q20_over_q2_grrr": round(statistics.median(grrr_growth), 2) if pairs else None,
    }
    OUT_JSON.write_text(
        json.dumps(
            {
                "generated_at_utc": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                "source": "Google Research, Google Runoff Reanalysis & Reforecast (GRRR) 1980–2023, "
                f"{MODEL}/return_periods.parquet (CC BY 4.0)",
                "radius_km": RADIUS_KM,
                "above_factor": ABOVE_FACTOR,
                "below_factor": BELOW_FACTOR,
                "summary": summary,
                "stations": out,
            },
            ensure_ascii=False,
            indent=1,
        )
        + "\n",
        encoding="utf-8",
    )
    for r in rows:
        print(f"{r[0]:<26} Q2 GloFAS {r[1]:9.1f}  n={r[2]:<3} GRRR {r[3] if r[3] is None else round(r[3], 1)}–{r[4] if r[4] is None else round(r[4], 1)}  {r[5]}")
    print(json.dumps(summary, ensure_ascii=False), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
