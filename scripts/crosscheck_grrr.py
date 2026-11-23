#!/usr/bin/env python3
"""Uji kewajaran silang ambang periode ulang GloFAS terhadap Google Runoff Reanalysis & Reforecast (GRRR).

Sumber: gs://flood-forecasting/hydrologic_predictions/model_id_8583a5c2_v0 (bucket publik, CC BY 4.0,
Google Research). Ini GRRR yang tercantum di halaman sumber daya Google Flood Forecasting ("global river
discharge estimates from 1980 to 2023"); periode reanalisis dibaca dari sumbu waktu
`reanalysis/streamflow.zarr` dan dicatat di keluaran. Berkas yang dipakai: `return_periods.zarr` (debit
periode ulang per titik keluar DAS HydroBASINS) dan `hybas_outlet_locations_UNOFFICIAL.zarr` (koordinat
titik keluar). Folder `distributed_model_2026_11` di bucket yang sama adalah rilis lain (reanalisis
31 Des 1980–30 Des 2025) dan tidak dipakai.

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
Dibutuhkan: numpy, pandas, numcodecs (blosc). Unduhan disimpan di <dir>/grrr/<model>.
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

MODEL = "model_id_8583a5c2_v0"
BASE = f"https://storage.googleapis.com/flood-forecasting/hydrologic_predictions/{MODEL}/"
OUTLETS = "hybas_outlet_locations_UNOFFICIAL.zarr"
RETURN_PERIODS = "return_periods.zarr"
TIME = "reanalysis/streamflow.zarr/time"
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


def zarr_array(cache: Path, path: str) -> np.ndarray:
    """Larik 1-D zarr v2 (kompresor dari metadata; teks vlen-utf8), dipotong ke `shape`."""
    import numcodecs

    meta = json.loads(fetch(cache, f"{path}/.zarray"))
    (n,), (chunk,) = meta["shape"], meta["chunks"]
    codec = numcodecs.get_codec(meta["compressor"]) if meta.get("compressor") else None
    text = any(f.get("id") == "vlen-utf8" for f in meta.get("filters") or [])
    parts = []
    for i in range(math.ceil(n / chunk)):
        raw = fetch(cache, f"{path}/{i}")
        raw = codec.decode(raw) if codec else raw
        if text:
            parts.append(np.asarray(numcodecs.VLenUTF8().decode(raw), dtype=object))
        else:
            parts.append(np.frombuffer(raw, dtype=meta["dtype"]))
    return np.concatenate(parts)[:n]


def reanalysis_period(cache: Path) -> tuple[str, str]:
    """Tanggal pertama dan terakhir sumbu waktu reanalisis model."""
    units = json.loads(fetch(cache, f"{TIME}/.zattrs"))["units"]
    step, _, origin = units.partition(" since ")
    if step != "days":
        raise ValueError(f"satuan waktu tak dikenal: {units}")
    t0 = dt.date.fromisoformat(origin[:10])
    days = zarr_array(cache, TIME)
    return str(t0 + dt.timedelta(days=int(days.min()))), str(t0 + dt.timedelta(days=int(days.max())))


def load_outlets(cache: Path) -> pd.DataFrame:
    return pd.DataFrame(
        {
            "gauge_id": zarr_array(cache, f"{OUTLETS}/gauge_id"),
            "lat": zarr_array(cache, f"{OUTLETS}/latitude"),
            "lon": zarr_array(cache, f"{OUTLETS}/longitude"),
        }
    )


def load_return_periods(cache: Path) -> pd.DataFrame:
    df = pd.DataFrame({"gauge_id": zarr_array(cache, f"{RETURN_PERIODS}/gauge_id")})
    for t in (2, 5, 20):
        df[f"q{t}"] = zarr_array(cache, f"{RETURN_PERIODS}/return_period_{t}")
    return df


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache-dir", required=True, type=Path)
    args = ap.parse_args()
    cache = args.cache_dir / "grrr" / MODEL
    cache.mkdir(parents=True, exist_ok=True)

    start, end = reanalysis_period(cache)
    print(f"{MODEL}: reanalisis {start} s.d. {end}", flush=True)
    df = load_outlets(cache).merge(load_return_periods(cache), on="gauge_id", how="inner")
    df = df[(df.lon > BBOX[0]) & (df.lat > BBOX[1]) & (df.lon < BBOX[2]) & (df.lat < BBOX[3])]
    df = df[np.isfinite(df.q2) & (df.q2 > 0)].reset_index(drop=True)
    print(f"{len(df)} titik keluar GRRR di Indonesia dan sekitarnya", flush=True)

    glofas = {s["id"]: s for s in json.loads(GLOFAS_JSON.read_text(encoding="utf-8"))["stations"]}
    out, rows = {}, []
    for st in parse_catalog(CATALOG_RS):
        cal = glofas.get(st["id"])
        if not cal or not cal.get("accepted") or len(cal.get("annual_max_m3s", [])) < MIN_ANNUAL_MAXIMA:
            continue
        xi, alpha = gumbel_lmoments(np.array(cal["annual_max_m3s"], dtype=float))
        q2 = gumbel_q(xi, alpha, 2)
        d = haversine_vec(st["lat"], st["lon"], df.lat.values, df.lon.values)
        near = df[d <= RADIUS_KM].copy()
        near["d"] = d[d <= RADIUS_KM]
        if near.empty:
            out[st["id"]] = {"radius_km": RADIUS_KM, "n_outlets": 0, "verdict": "no_outlets"}
            rows.append((st["id"], q2, 0, None, None, "no_outlets"))
            continue
        lo, hi = float(near.q2.min()), float(near.q2.max())
        best = near.loc[(np.log(near.q2 / q2)).abs().idxmin()]
        verdict = "glofas_above" if q2 > ABOVE_FACTOR * hi else "glofas_below" if q2 < BELOW_FACTOR * lo else "within_range"
        out[st["id"]] = {
            "radius_km": RADIUS_KM,
            "n_outlets": int(len(near)),
            "q2_min_m3s": round(lo, 2),
            "q2_max_m3s": round(hi, 2),
            "closest": {
                "id": str(best["gauge_id"]),
                "distance_km": round(float(best["d"]), 1),
                "q2_m3s": round(float(best["q2"]), 2),
                "q5_m3s": round(float(best["q5"]), 2),
                "q20_m3s": round(float(best["q20"]), 2),
            },
            "ratio_q2": round(q2 / float(best["q2"]), 2),
            "verdict": verdict,
        }
        rows.append((st["id"], q2, len(near), lo, hi, verdict))

    counts = {v: sum(1 for r in rows if r[5] == v) for v in ("within_range", "glofas_above", "glofas_below", "no_outlets")}
    # Bentuk kurva hanya dari pasangan yang skalanya berdekatan (rasio Q2 0,5–2).
    pairs = [(st_id, v) for st_id, v in out.items() if v.get("closest") and 0.5 <= v["ratio_q2"] <= 2.0]
    glofas_growth, grrr_growth = [], []
    for st_id, v in pairs:
        xi, alpha = gumbel_lmoments(np.array(glofas[st_id]["annual_max_m3s"], dtype=float))
        glofas_growth.append(gumbel_q(xi, alpha, 20) / gumbel_q(xi, alpha, 2))
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
                "source": "Google Research, Google Runoff Reanalysis & Reforecast (GRRR), "
                f"{MODEL}/{RETURN_PERIODS} (CC BY 4.0)",
                "model": MODEL,
                "reanalysis_period": [start, end],
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
        lo = "–" if r[3] is None else round(r[3], 1)
        hi = "–" if r[4] is None else round(r[4], 1)
        print(f"{r[0]:<26} Q2 GloFAS {r[1]:9.1f}  n={r[2]:<3} GRRR {lo}–{hi}  {r[5]}")
    print(json.dumps(summary, ensure_ascii=False), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
