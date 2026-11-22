#!/usr/bin/env python3
"""Ambang rob dari klimatologi muka laut model setempat.

Sumber: Open-Meteo Marine API, variabel `sea_level_height_msl` (MeteoFrance SMOC, Copernicus Marine;
tersedia sejak Januari 2022; pasang surut + efek barometrik terbalik + tinggi muka laut; datum = MSL
global, bukan LAT; tidak memuat amblesan tanah). Per titik rob di catalog.rs diunduh deret per jam
START..END pada sel laut terdekat (cell_selection=sea), lalu dihitung p95, p99, dan maksimum.

Level rob di flood-shared: high ≥ p99, very_high ≥ maksimum periode kalibrasi.
Titik dengan < MIN_HOURS jam valid tidak diberi ambang (level unknown).

Keluaran: crates/flood-shared/data/rob_calibration.json

Pemakaian:
  python scripts/calibrate_rob.py --cache-dir <dir> [--report <file.md>]
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys
import urllib.parse
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calibrate_glofas import Client, call_weight, n_days  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
CATALOG_RS = REPO / "crates/flood-shared/src/catalog.rs"
OUT_JSON = REPO / "crates/flood-shared/data/rob_calibration.json"
MARINE_API = "https://marine-api.open-meteo.com/v1/marine"
START, END = "2022-01-01", "2025-12-31"
MIN_HOURS = 2 * 365 * 24


def parse_rob(path: Path) -> list[dict]:
    src = path.read_text(encoding="utf-8")
    out = []
    for body in re.findall(r"CoastalRobStatic \{(.*?)\n    \}", src, re.S):
        d = {}
        for k, v in re.findall(r"^\s+(\w+): (.*),\s*$", body, re.M):
            v = v.strip()
            d[k] = v.strip('"') if v.startswith('"') else float(v)
        out.append(d)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache-dir", required=True, type=Path)
    ap.add_argument("--hourly-budget", type=float, default=3500.0)
    ap.add_argument("--report", type=Path)
    args = ap.parse_args()
    client = Client(args.cache_dir, args.hourly_budget)
    points = parse_rob(CATALOG_RS)
    days = n_days(START, END)
    period = f"{START[:4]}–{END[:4]}"

    out_points, rows = [], []
    for p in points:
        # Sel terdekat bisa tanpa data (masker darat model); cari sel laut berdata pada cincin 0,1° dan 0,2°.
        tries = [(0.0, 0.0)] + [
            (r * dy, r * dx) for r in (0.1, 0.2) for dy, dx in
            ((0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (-1, 1), (1, -1), (-1, -1))
        ]
        for dy, dx in tries:
            q = {
                "latitude": f"{p['lat'] + dy:.4f}",
                "longitude": f"{p['lon'] + dx:.4f}",
                "hourly": "sea_level_height_msl",
                "start_date": START,
                "end_date": END,
                "timezone": "GMT",
                "cell_selection": "sea",
            }
            url = MARINE_API + "?" + urllib.parse.urlencode(q)
            data = client.get_json(url, call_weight(1, 1, days))
            vals = np.array([np.nan if v is None else float(v) for v in data["hourly"]["sea_level_height_msl"]])
            if np.isfinite(vals).sum() >= MIN_HOURS:
                break
        valid = vals[np.isfinite(vals)]
        n = int(valid.size)
        th = None
        note = ""
        if n >= MIN_HOURS:
            # Level dibandingkan dengan puncak 48 jam ke depan, jadi ambang harus dari deret yang
            # setara: maksimum jendela 48 jam yang bergeser per jam. Ambang per jam membuat
            # pasang purnama rutin (±13% hari) terbaca "Tinggi".
            win = np.lib.stride_tricks.sliding_window_view(vals, 48)
            full = np.isfinite(win).all(axis=1)
            peak48 = win[full].max(axis=1)
            th = {
                "p95_m": round(float(np.percentile(peak48, 95)), 3),
                "p99_m": round(float(np.percentile(peak48, 99)), 3),
                "max_m": round(float(peak48.max()), 3),
                "period": period,
            }
        else:
            note = f"hanya {n} jam data muka laut model valid pada {period}"
        cell_lat, cell_lon = data.get("latitude"), data.get("longitude")
        out_points.append({
            "id": p["id"],
            "cell_lat": cell_lat,
            "cell_lon": cell_lon,
            "n_hours": n,
            "thresholds": th,
            "note": note,
        })
        rows.append((p["id"], cell_lat, cell_lon, n, th))
        print(f"{p['id']:<26} sel {cell_lat},{cell_lon}  n={n}  {th}", flush=True)

    out = {
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source": "Open-Meteo Marine API, sea_level_height_msl (MeteoFrance SMOC, E.U. Copernicus Marine Service)",
        "period": period,
        "method": "Persentil 95 dan 99 serta maksimum dari deret puncak muka laut model 48 jam (jendela bergeser per jam) pada sel laut terdekat; high ≥ p99, very_high ≥ maksimum. Setara dengan aturan level yang memakai puncak 48 jam ke depan.",
        "points": out_points,
    }
    OUT_JSON.write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"tulis {OUT_JSON}; bobot {client.spent:.1f}")
    if args.report:
        lines = ["| Titik | Sel laut | Jam valid | p95 m | p99 m | Maks m |", "|---|---|---:|---:|---:|---:|"]
        for pid, la, lo, n, th in rows:
            th = th or {}
            lines.append(f"| {pid} | {la}, {lo} | {n} | {th.get('p95_m', '–')} | {th.get('p99_m', '–')} | {th.get('max_m', '–')} |")
        args.report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
