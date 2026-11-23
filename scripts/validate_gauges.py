#!/usr/bin/env python3
"""Validasi sel GloFAS terhadap pos duga air: rerata debit terukur dari katalog GRDC.

Sumber: katalog stasiun Global Runoff Data Centre (GRDC), `GRDC_Stations.xlsx` di
ftp://ftp.bafg.de/pub/REFERATE/GRDC/catalogue/grdc_stations.zip (lembar `station_catalogue`). Kolom yang
dipakai: luas DAS (`area`, km²), rerata debit jangka panjang terukur (`lta_discharge`, m³/s), dan
periode data harian/bulanan. Statistik ini diturunkan GRDC dari data dinas hidrologi nasional.
Rujukan wajib: "The Global Runoff Data Centre, 56068 Koblenz, Germany". Deret harian GRDC tidak
dipakai di sini (permintaan lewat portal GRDC; redistribusi tidak diizinkan).

Pertanyaan yang diuji: apakah sel GloFAS terpilih (calibrate_glofas.py) mewakili sungai dan ukuran DAS
yang benar? Pemilihan sel memakai debit spesifik asumsi per pulau; di sini asumsi itu diganti debit
terukur pos duga air di sungai yang sama.

Metode:
  1. PASANGAN dikurasi manual: stasiun katalog dan pos GRDC pada sungai yang sama (nama sungai dan
     letak dicek). Pos di sungai lain tidak dipasangkan walau dekat (mis. Ci Sadane di dekat
     Katulampa).
  2. Pasangan dinilai bila rasio luas DAS katalog/GRDC dalam [1/MAX_AREA_RATIO, MAX_AREA_RATIO].
     Rerata terukur diskalakan ke luas DAS katalog (debit spesifik pos dianggap berlaku di pos
     katalog): Q_skala = Q_GRDC × A_katalog / A_GRDC.
  3. Rasio = rerata sel GloFAS 1997–2024 / Q_skala. Putusan: `consistent` bila
     1/TOLERANCE ≤ rasio ≤ TOLERANCE, `glofas_high` bila di atas, `glofas_low` bila di bawah.
  4. Ringkasan per pulau: median debit spesifik seluruh pos GRDC Indonesia dibanding asumsi
     pemilihan sel.
Keterbatasan: periode data GRDC (1951–2010) tidak sama dengan GloFAS (1997–2024); penskalaan luas
mengabaikan perbedaan hujan antara hulu dan hilir. Ini menguji rerata debit, bukan ambang banjir Q2.

Keluaran: crates/flood-shared/data/gauge_validation.json

Pemakaian:
  python scripts/validate_gauges.py --catalogue <GRDC_Stations.xlsx>
Dibutuhkan: pandas, openpyxl.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import statistics
import sys
from pathlib import Path

import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calibrate_glofas import SPECIFIC_Q_LPS_KM2, haversine_km, parse_catalog  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
CATALOG_RS = REPO / "crates/flood-shared/src/catalog.rs"
GLOFAS_JSON = REPO / "crates/flood-shared/data/glofas_calibration.json"
OUT_JSON = REPO / "crates/flood-shared/data/gauge_validation.json"

SOURCE = "The Global Runoff Data Centre, 56068 Koblenz, Germany (katalog stasiun GRDC, lta_discharge)"
MAX_AREA_RATIO = 3.0
TOLERANCE = 1.5
LARGE_BASIN_KM2 = 1000.0  # stasiun katalog umumnya DAS besar; pos hulu kecil cenderung lebih basah

# Stasiun katalog → nomor GRDC pada sungai yang sama. Catatan: koordinat GRDC Kairagi (124,483°BT)
# jatuh di laut barat Manado; nama pos (Kairagi, Manado), sungai, dan luas DAS cocok dengan Tondano
# hilir, sehingga dipasangkan menurut nama, dan jaraknya tidak bermakna.
PAIRS = {
    "solo-bojonegoro": 5141200,  # Bengawan Solo · Bojonegoro
    "solo-sragen": 5141201,  # Bengawan Solo · Jurug (hulu Sragen)
    "brantas-mojokerto": 5141100,  # Kali Brantas · Jabon (hulu Mojokerto)
    "walanae-wajo": 5148800,  # Salo Walanae · Sempajeruk (hulu Wajo)
    "tondano-manado": 5148250,  # Sungai Tondano · Kairagi
    "citarum-nanjung": 5141150,  # Ci Tarum · Majalaya (hulu)
    "serayu-banyumas": 5142700,  # Kali Serayu · Banjarnegara (hulu)
    "progo-bantul": 5142550,  # Kali Progo · Badran (hulu)
    "way-sekampung": 5137600,  # Wai Sekampung · Kunyir (hulu)
    "kampar-pelalawan": 5135100,  # Sungai Kampar Kiri · Lipat Kain (anak sungai hulu)
}
COORD_UNRELIABLE = {5148250}


# Pulau pos GRDC menurut koordinat; pos GRDC Indonesia hanya ada di Sumatera, Jawa, dan Sulawesi.
def island_of(lat: float, lon: float) -> str:
    if lon < 106.0:
        return "Sumatera"
    if lon < 115.0 and lat < -5.8:
        return "Jawa"
    if 118.5 < lon < 125.5:
        return "Sulawesi"
    return "lainnya"


def years(row: pd.Series) -> str:
    for a, b in (("d_start", "d_end"), ("m_start", "m_end"), ("t_start", "t_end")):
        if pd.notna(row.get(a)) and pd.notna(row.get(b)):
            return f"{int(row[a])}–{int(row[b])}"
    return ""


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--catalogue", required=True, type=Path, help="GRDC_Stations.xlsx")
    args = ap.parse_args()

    cat = pd.read_excel(args.catalogue, sheet_name="station_catalogue")
    idn = cat[cat.country == "ID"].copy()
    for col in ("lat", "long", "area", "lta_discharge"):
        idn[col] = pd.to_numeric(idn[col], errors="coerce")
    idn = idn[(idn.area > 0) & (idn.lta_discharge > 0)]
    by_no = {int(r.grdc_no): r for _, r in idn.iterrows()}
    print(f"{len(idn)} pos GRDC Indonesia dengan luas DAS dan rerata debit", flush=True)

    stations = {s["id"]: s for s in parse_catalog(CATALOG_RS)}
    glofas = {s["id"]: s for s in json.loads(GLOFAS_JSON.read_text(encoding="utf-8"))["stations"]}

    out, skipped = {}, []
    for sid, no in PAIRS.items():
        st, g, cal = stations[sid], by_no[no], glofas.get(sid) or {}
        cell = cal.get("cell") if cal.get("accepted") else None
        area_ratio = st["basin_area_km2"] / float(g.area)
        dist = None if no in COORD_UNRELIABLE else round(haversine_km(st["lat"], st["lon"], float(g.lat), float(g.long)), 1)
        label = f"{str(g.river).title()} · {str(g.station).title()}"
        if not cell or not (1 / MAX_AREA_RATIO <= area_ratio <= MAX_AREA_RATIO):
            skipped.append({"id": sid, "gauge": label, "area_ratio": round(area_ratio, 2), "calibrated": bool(cell)})
            continue
        scaled = float(g.lta_discharge) * area_ratio
        ratio = cell["mean_q_m3s"] / scaled
        verdict = "glofas_high" if ratio > TOLERANCE else "glofas_low" if ratio < 1 / TOLERANCE else "consistent"
        out[sid] = {
            "grdc_no": no,
            "gauge": label,
            "distance_km": dist,
            "gauge_area_km2": float(g.area),
            "gauge_mean_m3s": round(float(g.lta_discharge), 2),
            "gauge_years": years(g),
            "area_ratio": round(area_ratio, 2),
            "scaled_mean_m3s": round(scaled, 2),
            "cell_mean_m3s": cell["mean_q_m3s"],
            "ratio": round(ratio, 2),
            "verdict": verdict,
        }

    specific = {}
    for _, r in idn.iterrows():
        q = 1000 * float(r.lta_discharge) / float(r.area)
        specific.setdefault(island_of(float(r.lat), float(r.long)), []).append((float(r.area), q))
    islands = {}
    for k, v in sorted(specific.items()):
        if k == "lainnya":
            continue
        large = [q for a, q in v if a >= LARGE_BASIN_KM2]
        islands[k] = {
            "n_gauges": len(v),
            "median_lps_km2": round(statistics.median(q for _, q in v), 1),
            "n_large": len(large),
            "median_large_lps_km2": round(statistics.median(large), 1) if large else None,
            "assumption_lps_km2": SPECIFIC_Q_LPS_KM2.get(k),
        }
    counts = {v: sum(1 for x in out.values() if x["verdict"] == v) for v in ("consistent", "glofas_high", "glofas_low")}
    OUT_JSON.write_text(
        json.dumps(
            {
                "generated_at_utc": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                "source": SOURCE,
                "max_area_ratio": MAX_AREA_RATIO,
                "tolerance": TOLERANCE,
                "summary": {"n_pairs": len(PAIRS), "n_assessed": len(out), "verdicts": counts, "islands": islands},
                "not_assessed": skipped,
                "stations": out,
            },
            ensure_ascii=False,
            indent=1,
        )
        + "\n",
        encoding="utf-8",
    )
    for sid, v in out.items():
        print(
            f"{sid:<18} {v['gauge']:<30} A {v['area_ratio']:4.2f}×  Q_GRDC {v['gauge_mean_m3s']:7.1f} → {v['scaled_mean_m3s']:7.1f}"
            f"  sel {v['cell_mean_m3s']:7.1f}  rasio {v['ratio']:4.2f}  {v['verdict']}"
        )
    for s in skipped:
        print(f"{s['id']:<18} {s['gauge']:<30} A {s['area_ratio']:4.2f}×  tidak dinilai")
    print(json.dumps(islands, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
