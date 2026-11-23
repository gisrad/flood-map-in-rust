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
  5. Ambang banjir (bila pos punya maksimum tahunan harian): indeks tahunan GSIM (Global Streamflow
     Indices and Metadata Archive; Do dkk. 2018, Gudmundsson dkk. 2018; PANGAEA, CC BY-SA 3.0), yang
     dihitung dari deret harian GRDC. `MAX` = debit harian tertinggi per tahun, setara dengan maksimum
     tahunan reanalisis GloFAS. Analisis utama memakai semua tahun dengan hari hilang
     ≤ MAX_MISSING_DAYS. Sensitivitas terpisah mengeluarkan tahun dengan rerata < MIN_MEAN_FRACTION
     × median rerata tahunan; nilai rendah tidak otomatis berarti alat rusak.
     Q2/Q5/Q20 dengan Gumbel L-moment yang sama seperti GloFAS bila ≥ MIN_FLOOD_YEARS tahun.
     Q2 pos diskalakan ke luas DAS katalog dengan Q ∝ A^b, b = 0,5–1,0 (rentang), nilai tengah b = 0,75.
     Berkas GSIM diambil per pos lewat HTTP range dari arsip zip PANGAEA dan disimpan di --cache-dir.
     Pos yang tidak ada di GSIM (Bojonegoro, Jabon: GRDC hanya menyimpan rekaman bulanan) memakai nilai
     terbit bila ada (PUBLISHED_FLOOD, `kind=published`): nilai sekunder tanpa tahun rekaman, sehingga
     lebih lemah daripada `gsim_annual_max`. Jabon belum punya sumber terbuka dan tetap tidak dinilai.
Keterbatasan: periode data GRDC (1951–2010) tidak sama dengan GloFAS (1997–2024); penskalaan luas
mengabaikan perbedaan hujan antara hulu dan hilir serta peredaman danau dan dataran banjir; Q2 pos dari
9–13 tahun data; homogenitas GSIM berstatus NSD (data tidak cukup), bukan terbukti homogen.

Keluaran: crates/flood-shared/data/gauge_validation.json

Pemakaian:
  python scripts/validate_gauges.py --catalogue <GRDC_Stations.xlsx> --cache-dir <dir>
Dibutuhkan: numpy, pandas, openpyxl.
"""
from __future__ import annotations

import argparse
import datetime as dt
import io
import json
import statistics
import sys
import urllib.request
import zipfile
from pathlib import Path

import numpy as np
import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calibrate_glofas import SPECIFIC_Q_LPS_KM2, gumbel_lmoments, gumbel_q, haversine_km, parse_catalog  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
CATALOG_RS = REPO / "crates/flood-shared/src/catalog.rs"
GLOFAS_JSON = REPO / "crates/flood-shared/data/glofas_calibration.json"
OUT_JSON = REPO / "crates/flood-shared/data/gauge_validation.json"

SOURCE = "The Global Runoff Data Centre, 56068 Koblenz, Germany (katalog stasiun GRDC, lta_discharge)"
MAX_AREA_RATIO = 3.0
TOLERANCE = 1.5
LARGE_BASIN_KM2 = 1000.0  # stasiun katalog umumnya DAS besar; pos hulu kecil cenderung lebih basah

GSIM_META_ZIP = "https://store.pangaea.de/Publications/DoH-etal_2018/GSIM_metadata.zip"
GSIM_INDEX_ZIP = "https://store.pangaea.de/Publications/GudmundssonL-etal_2018/GSIM_indices.zip"
GSIM_SOURCE = (
    "GSIM (Do dkk. 2018, doi:10.1594/PANGAEA.887477; Gudmundsson dkk. 2018, doi:10.1594/PANGAEA.887470), "
    "CC BY-SA 3.0, dari data GRDC"
)
MAX_MISSING_DAYS = 30
MIN_MEAN_FRACTION = 0.25
MIN_FLOOD_YEARS = 8
AREA_EXPONENTS = (0.5, 0.75, 1.0)

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


class HttpRange(io.RawIOBase):
    """Berkas jarak jauh yang dibaca per rentang byte, agar zip besar tidak diunduh utuh."""

    def __init__(self, url: str):
        self.url, self.pos = url, 0
        with urllib.request.urlopen(urllib.request.Request(url, method="HEAD"), timeout=60) as r:
            self.size = int(r.headers["Content-Length"])

    def readable(self) -> bool:
        return True

    def seekable(self) -> bool:
        return True

    def tell(self) -> int:
        return self.pos

    def seek(self, off: int, whence: int = 0) -> int:
        self.pos = off if whence == 0 else self.pos + off if whence == 1 else self.size + off
        return self.pos

    def readinto(self, buf) -> int:
        if self.pos >= self.size or len(buf) == 0:
            return 0
        end = min(self.size, self.pos + len(buf)) - 1
        req = urllib.request.Request(self.url, headers={"Range": f"bytes={self.pos}-{end}"})
        with urllib.request.urlopen(req, timeout=120) as r:
            if r.status != 206:
                raise ValueError("Server tidak mendukung HTTP range; unduhan dibatalkan")
            b = r.read()
        if len(b) != end - self.pos + 1:
            raise ValueError("Respons HTTP range tidak lengkap")
        buf[: len(b)] = b
        self.pos += len(b)
        return len(b)


def remote_member(cache: Path, url: str, member: str) -> Path:
    out = cache / member.rsplit("/", 1)[-1]
    if not out.exists():
        with zipfile.ZipFile(io.BufferedReader(HttpRange(url), buffer_size=1 << 20)) as z:
            out.write_bytes(z.read(member))
    return out


def gsim_yearly(cache: Path, grdc_no: int) -> tuple[str, pd.DataFrame] | None:
    meta = pd.read_csv(remote_member(cache, GSIM_META_ZIP, "GSIM_metadata/GSIM_catalog/GSIM_metadata.csv"), low_memory=False)
    row = meta[meta["grdb.no"] == grdc_no]
    if row.empty:
        return None
    gsim_no = str(row.iloc[0]["gsim.no"])
    f = remote_member(cache, GSIM_INDEX_ZIP, f"GSIM_indices/TIMESERIES/yearly/{gsim_no}.year")
    df = pd.read_csv(f, comment="#", sep=r",\s*", engine="python", na_values=["NA"])
    df.columns = [c.strip().strip('"') for c in df.columns]
    df["year"] = df["date"].str[:4].astype(int)
    return gsim_no, df[["year", "MEAN", "MAX", "n.missing"]].dropna(subset=["MAX", "MEAN"])


# Q2/Q5/Q20 terbit untuk pos yang tidak ada di GSIM (hanya rekaman bulanan di GRDC). Bukan data mentah:
# panjang rekaman tidak dilaporkan pada sumber.
PUBLISHED_FLOOD = {
    "solo-bojonegoro": {
        "q": {2: 1883.0, 5: 2316.0, 20: 2877.0},
        "citation": (
            "Sulaeman, A., Suhartanto, E., Sumiadi (2017), Jurnal Teknik Pengairan 8(2): 146–157, Tabel 3 "
            "(Gumbel pada debit tahunan maksimum pos Bojonegoro; data BBWS Bengawan Solo dan Perum Jasa Tirta I; "
            "tahun dan panjang rekaman tidak dilaporkan)"
        ),
    },
}


def _compare(
    kind: str,
    q2: float,
    q5: float,
    q20: float,
    area_ratio: float,
    glofas_amax: list[float],
    extra: dict,
) -> dict:
    gxi, galpha = gumbel_lmoments(np.array(glofas_amax, dtype=float))
    g2, g20 = (gumbel_q(gxi, galpha, t) for t in (2, 20))
    lo, mid, hi = (q2 * area_ratio**b for b in AREA_EXPONENTS)
    lo, hi = min(lo, hi), max(lo, hi)
    verdict = "glofas_high" if g2 > hi * TOLERANCE else "glofas_low" if g2 < lo / TOLERANCE else "consistent"
    return {
        "kind": kind,
        **extra,
        "status": "preliminary",
        "gauge_q2_m3s": round(q2, 2),
        "gauge_q5_m3s": round(q5, 2),
        "gauge_q20_m3s": round(q20, 2),
        "scaled_q2_low_m3s": round(lo, 2),
        "scaled_q2_m3s": round(mid, 2),
        "scaled_q2_high_m3s": round(hi, 2),
        "glofas_q2_m3s": round(g2, 2),
        "glofas_q20_m3s": round(g20, 2),
        "ratio_q2": round(g2 / mid, 2),
        "verdict": verdict,
    }


def flood_check(cache: Path, sid: str, grdc_no: int, area_ratio: float, glofas_amax: list[float]) -> dict | None:
    if len(glofas_amax) < MIN_FLOOD_YEARS:
        return None
    pub = PUBLISHED_FLOOD.get(sid)
    got = gsim_yearly(cache, grdc_no)
    if not got and pub:
        q = pub["q"]
        return _compare(
            "published",
            q[2],
            q[5],
            q[20],
            area_ratio,
            glofas_amax,
            {
                "gsim_no": None,
                "years": [],
                "excluded_low_mean_years": [],
                "screened_gauge_q2_m3s": None,
                "max_missing_days": None,
                "citation": pub["citation"],
            },
        )
    if not got:
        return None
    gsim_no, y = got
    y = y[y["n.missing"] <= MAX_MISSING_DAYS]
    if len(y) < MIN_FLOOD_YEARS:
        return None
    xi, alpha = gumbel_lmoments(y.MAX.to_numpy(dtype=float))
    q2, q5, q20 = (gumbel_q(xi, alpha, t) for t in (2, 5, 20))
    screened = y[y.MEAN >= MIN_MEAN_FRACTION * y.MEAN.median()]
    screened_q2 = None
    if len(screened) >= MIN_FLOOD_YEARS:
        sx, sa = gumbel_lmoments(screened.MAX.to_numpy(dtype=float))
        screened_q2 = round(gumbel_q(sx, sa, 2), 2)
    return _compare(
        "gsim_annual_max",
        q2,
        q5,
        q20,
        area_ratio,
        glofas_amax,
        {
            "gsim_no": gsim_no,
            "years": [int(v) for v in y.year],
            "excluded_low_mean_years": [int(v) for v in y.loc[~y.year.isin(screened.year), "year"]],
            "screened_gauge_q2_m3s": screened_q2,
            "max_missing_days": MAX_MISSING_DAYS,
            "citation": None,
        },
    )


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--catalogue", required=True, type=Path, help="GRDC_Stations.xlsx")
    ap.add_argument("--cache-dir", required=True, type=Path, help="tempat berkas GSIM yang diunduh")
    args = ap.parse_args()
    cache = args.cache_dir / "gsim"
    cache.mkdir(parents=True, exist_ok=True)

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
            "flood": flood_check(cache, sid, no, area_ratio, cal.get("annual_max_m3s", [])),
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
                "flood_source": GSIM_SOURCE,
                "max_area_ratio": MAX_AREA_RATIO,
                "tolerance": TOLERANCE,
                "area_exponents": list(AREA_EXPONENTS),
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
        f = v["flood"]
        if f:
            print(
                f"{'':<18} Q2 pos {f['gauge_q2_m3s']:7.1f} ({len(f['years'])} th) → {f['scaled_q2_low_m3s']:.0f}–{f['scaled_q2_high_m3s']:.0f}"
                f"  Q2 GloFAS {f['glofas_q2_m3s']:7.1f}  rasio {f['ratio_q2']:4.2f}  {f['verdict']}"
            )
    for s in skipped:
        print(f"{s['id']:<18} {s['gauge']:<30} A {s['area_ratio']:4.2f}×  tidak dinilai")
    print(json.dumps(islands, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
