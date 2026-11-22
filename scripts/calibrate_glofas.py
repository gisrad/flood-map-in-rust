#!/usr/bin/env python3
"""Kalibrasi sel GloFAS v4 per stasiun dan ambang klimatologis.

Keluaran: crates/flood-shared/data/glofas_calibration.json (dimuat flood-shared via include_str!).

Langkah
1. Pemilihan sel. Untuk tiap stasiun di crates/flood-shared/src/catalog.rs dipindai kisi 0,05°
   (radius ±0,10°, 25 titik) di sekitar koordinat pos. Rerata debit tiap sel dihitung pada jendela
   pemindaian (SCAN_START..SCAN_END, 140 hari = bobot 1 panggilan per lokasi). Skor sel =
   |ln(Q_sel / Q_harapan)| + jarak_km / 50; skor terkecil dipilih. Q_harapan = luas DAS katalog ×
   debit spesifik asumsi per pulau (SPECIFIC_Q_LPS_KM2).
2. Klimatologi. Pada sel terpilih diunduh debit harian GloFAS v4 reanalisis (models=consolidated_v4)
   CLIM_START..CLIM_END. Dihitung rerata, p90, p99 harian dan maksimum tahunan (tahun dengan
   ≥ 330 hari valid). Q2/Q5/Q20 dihitung flood-shared dari maksimum tahunan (Gumbel, L-moment);
   skrip ini menghitung angka yang sama untuk tabel laporan.
3. Penerimaan. Sel diterima bila rerata jangka panjang berada dalam [Q_harapan/ACCEPT_FACTOR,
   Q_harapan×ACCEPT_FACTOR]. Selain itu `accepted=false` dan stasiun berlevel `unknown`.

Kuota Open-Meteo gratis: bobot lokasi = max(1, V/10 × max(1, D/14)). Skrip mencatat bobot di
<cache>/ledger.jsonl, menahan diri di bawah --hourly-budget per 60 menit, dan memberi jeda ≥ 1,1 s.
Semua respons di-cache di --cache-dir sehingga eksekusi ulang tidak memanggil API lagi.

Pemakaian:
  python scripts/calibrate_glofas.py --cache-dir <dir> [--report <file.md>] [--hourly-budget 3500]
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import math
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

import numpy as np

REPO = Path(__file__).resolve().parents[1]
CATALOG_RS = REPO / "crates/flood-shared/src/catalog.rs"
OUT_JSON = REPO / "crates/flood-shared/data/glofas_calibration.json"

FLOOD_API = "https://flood-api.open-meteo.com/v1/flood"
USER_AGENT = "HidrosentinelNusantara-calibration/0.2 (+https://flood.transbulent.id)"

SCAN_START, SCAN_END = "2018-01-01", "2018-05-20"  # 140 hari
SCAN_STEP_DEG, SCAN_RADIUS_STEPS = 0.05, 2  # kisi 5×5
CLIM_MODEL = "consolidated_v4"
CLIM_START, CLIM_END = "1997-01-01", "2024-12-31"  # data GloFAS v4 di Open-Meteo dimulai 1997-01-01
MIN_VALID_DAYS_PER_YEAR = 330
ACCEPT_FACTOR = 3.0
DIST_PENALTY_KM = 50.0

# Debit spesifik rerata tahunan asumsi (L/s/km²) = (hujan tahunan − evapotranspirasi) orde besaran.
# 1 L/s/km² ≈ 31,5 mm/tahun. Nilai ini hanya penyaring besaran (toleransi ×3), bukan data.
SPECIFIC_Q_LPS_KM2 = {
    "Jawa": 30.0,
    "Sumatera": 40.0,
    "Kalimantan": 45.0,
    "Sulawesi": 35.0,
    "Bali-Nusra": 15.0,
    "Maluku": 40.0,
    "Papua": 60.0,
}
ISLAND_GROUP = {
    "Jawa": "Jawa",
    "Sumatera": "Sumatera",
    "Kalimantan": "Kalimantan",
    "Sulawesi": "Sulawesi",
    "Bali-Nustra": "Bali-Nusra",
    "Maluku": "Maluku",
    "Papua": "Papua",
}


# ---------------------------------------------------------------- utilitas
def parse_catalog(path: Path) -> list[dict]:
    src = path.read_text(encoding="utf-8")
    out = []
    for body in re.findall(r"StationStatic \{(.*?)\n    \}", src, re.S):
        d = {}
        for k, v in re.findall(r"^\s+(\w+): (.*),\s*$", body, re.M):
            v = v.strip()
            d[k] = v.strip('"') if v.startswith('"') else float(v)
        out.append(d)
    return out


def haversine_km(lat1, lon1, lat2, lon2):
    r = 6371.0
    p1, p2 = math.radians(lat1), math.radians(lat2)
    dp, dl = p2 - p1, math.radians(lon2 - lon1)
    a = math.sin(dp / 2) ** 2 + math.cos(p1) * math.cos(p2) * math.sin(dl / 2) ** 2
    return 2 * r * math.asin(math.sqrt(a))


def n_days(start: str, end: str) -> int:
    return (dt.date.fromisoformat(end) - dt.date.fromisoformat(start)).days + 1


def call_weight(n_locations: int, n_vars: int, days: int) -> float:
    return n_locations * max(1.0, (n_vars / 10.0) * max(1.0, days / 14.0))


class Client:
    """HTTP GET dengan cache berkas, ledger bobot per jam, dan jeda minimum."""

    def __init__(self, cache_dir: Path, hourly_budget: float, min_gap_s: float = 1.1):
        self.cache = cache_dir
        self.cache.mkdir(parents=True, exist_ok=True)
        self.ledger = self.cache / "ledger.jsonl"
        self.hourly_budget = hourly_budget
        self.min_gap_s = min_gap_s
        self.last = 0.0
        self.spent = 0.0

    def _window_sum(self) -> tuple[float, float]:
        if not self.ledger.exists():
            return 0.0, 0.0
        now = time.time()
        total, oldest = 0.0, now
        for line in self.ledger.read_text().splitlines():
            rec = json.loads(line)
            if now - rec["t"] < 3600:
                total += rec["w"]
                oldest = min(oldest, rec["t"])
        return total, oldest

    def get_json(self, url: str, weight: float):
        key = hashlib.sha1(url.encode()).hexdigest()[:20]
        f = self.cache / f"{key}.json"
        if f.exists():
            return json.loads(f.read_text(encoding="utf-8"))
        while True:
            used, oldest = self._window_sum()
            if used + weight <= self.hourly_budget:
                break
            wait = max(30.0, 3600 - (time.time() - oldest) + 5)
            print(f"  kuota jam: {used:.0f}+{weight:.0f} > {self.hourly_budget:.0f}; tunggu {wait:.0f} s", flush=True)
            time.sleep(min(wait, 600))
        gap = time.time() - self.last
        if gap < self.min_gap_s:
            time.sleep(self.min_gap_s - gap)
        for attempt in range(5):
            req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": "application/json"})
            try:
                with urllib.request.urlopen(req, timeout=180) as r:
                    body = r.read()
                    cost = r.headers.get("X-RateLimit-Cost")
                break
            except urllib.error.HTTPError as e:
                if e.code in (429, 503):
                    retry = float(e.headers.get("Retry-After") or 60)
                    print(f"  HTTP {e.code}; tunggu {retry:.0f} s", flush=True)
                    time.sleep(retry + 2)
                    continue
                raise
        else:
            raise RuntimeError(f"gagal: {url}")
        self.last = time.time()
        w = float(cost) if cost else weight
        self.spent += w
        with self.ledger.open("a") as lf:
            lf.write(json.dumps({"t": self.last, "w": w, "url": url[:200]}) + "\n")
        data = json.loads(body)
        f.write_text(json.dumps(data), encoding="utf-8")
        return data


def flood_url(lats, lons, start, end, model=None) -> str:
    q = {
        "latitude": ",".join(f"{x:.4f}" for x in lats),
        "longitude": ",".join(f"{x:.4f}" for x in lons),
        "daily": "river_discharge",
        "start_date": start,
        "end_date": end,
        "cell_selection": "nearest",
    }
    if model:
        q["models"] = model
    return FLOOD_API + "?" + urllib.parse.urlencode(q, safe=",")


def as_list(data):
    return data if isinstance(data, list) else [data]


def series(item) -> tuple[list[str], np.ndarray]:
    t = item["daily"]["time"]
    q = np.array([np.nan if v is None else float(v) for v in item["daily"]["river_discharge"]])
    return t, q


# ---------------------------------------------------------------- statistik
def gumbel_lmoments(x: np.ndarray) -> tuple[float, float]:
    x = np.sort(x[np.isfinite(x)])
    n = len(x)
    b0 = x.mean()
    b1 = np.sum(np.arange(n) / (n - 1) * x) / n
    l2 = 2 * b1 - b0
    alpha = l2 / math.log(2)
    xi = b0 - 0.5772156649015329 * alpha
    return xi, alpha


def gumbel_q(xi, alpha, t):
    return xi - alpha * math.log(-math.log(1 - 1 / t))


def annual_maxima(times: list[str], q: np.ndarray) -> list[float]:
    by_year: dict[int, list[float]] = {}
    for t, v in zip(times, q):
        if np.isfinite(v):
            by_year.setdefault(int(t[:4]), []).append(v)
    return [round(max(v), 3) for y, v in sorted(by_year.items()) if len(v) >= MIN_VALID_DAYS_PER_YEAR]


# ---------------------------------------------------------------- utama
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache-dir", required=True, type=Path)
    ap.add_argument("--hourly-budget", type=float, default=3500.0)
    ap.add_argument("--report", type=Path, help="tulis tabel Markdown")
    ap.add_argument("--only", nargs="*", help="id stasiun tertentu")
    args = ap.parse_args()

    client = Client(args.cache_dir, args.hourly_budget)
    stations = parse_catalog(CATALOG_RS)
    if args.only:
        stations = [s for s in stations if s["id"] in args.only]
    scan_days = n_days(SCAN_START, SCAN_END)
    clim_days = n_days(CLIM_START, CLIM_END)

    results = []
    def id_num(v: float, dec: int) -> str:
        """Angka gaya Indonesia: titik ribuan, koma desimal."""
        return f"{v:,.{dec}f}".replace(",", "_").replace(".", ",").replace("_", ".")

    for i, st in enumerate(stations, 1):
        group = ISLAND_GROUP.get(st["island"], st["island"])
        q_spec = SPECIFIC_Q_LPS_KM2[group]
        q_exp = st["basin_area_km2"] * q_spec / 1000.0

        offs = [k * SCAN_STEP_DEG for k in range(-SCAN_RADIUS_STEPS, SCAN_RADIUS_STEPS + 1)]
        lats = [st["lat"] + dy for dy in offs for _ in offs]
        lons = [st["lon"] + dx for _ in offs for dx in offs]
        data = as_list(client.get_json(flood_url(lats, lons, SCAN_START, SCAN_END), call_weight(len(lats), 1, scan_days)))
        cells = {}
        for item in data:
            key = (round(item["latitude"], 4), round(item["longitude"], 4))
            if key in cells:
                continue
            _, q = series(item)
            if np.isfinite(q).sum() < scan_days * 0.9:
                continue
            mean_q = float(np.nanmean(q))
            if mean_q <= 0:
                continue
            dist = haversine_km(st["lat"], st["lon"], key[0], key[1])
            score = abs(math.log(mean_q / q_exp)) + dist / DIST_PENALTY_KM
            cells[key] = {"lat": key[0], "lon": key[1], "scan_mean": mean_q, "dist": dist, "score": score}
        ranked = sorted(cells.values(), key=lambda c: c["score"])
        rec = {
            "id": st["id"],
            "river": st["river"],
            "name": st["name"],
            "basin_area_km2": int(st["basin_area_km2"]),
            "q_spec": q_spec,
            "expected_q_m3s": round(q_exp, 2),
            "n_candidates": len(cells),
            "scan_max_mean": round(max((c["scan_mean"] for c in cells.values()), default=0.0), 2),
        }
        if not ranked:
            rec.update(accepted=False, note="tidak ada sel GloFAS dengan data di sekitar pos")
            results.append(rec)
            continue
        best = ranked[0]
        url = flood_url([best["lat"]], [best["lon"]], CLIM_START, CLIM_END, CLIM_MODEL)
        item = as_list(client.get_json(url, call_weight(1, 1, clim_days)))[0]
        if (round(item["latitude"], 4), round(item["longitude"], 4)) != (best["lat"], best["lon"]):
            print(f"  peringatan: sel klimatologi {item['latitude']},{item['longitude']} ≠ sel pindai", flush=True)
        times, q = series(item)
        valid = q[np.isfinite(q)]
        amax = annual_maxima(times, q)
        mean_lt = float(valid.mean()) if len(valid) else float("nan")
        ratio = mean_lt / q_exp if q_exp > 0 else float("nan")
        accepted = (
            len(amax) >= 20
            and math.isfinite(ratio)
            and 1 / ACCEPT_FACTOR <= ratio <= ACCEPT_FACTOR
        )
        xi, alpha = gumbel_lmoments(np.array(amax)) if len(amax) >= 5 else (float("nan"), float("nan"))
        rec.update(
            cell={"lat": best["lat"], "lon": best["lon"], "offset_km": round(best["dist"], 2), "mean_q_m3s": round(mean_lt, 2)},
            scan_mean=round(best["scan_mean"], 2),
            ratio_to_expected=round(ratio, 3),
            annual_max_m3s=amax,
            p90_m3s=round(float(np.percentile(valid, 90)), 3) if len(valid) else None,
            p99_m3s=round(float(np.percentile(valid, 99)), 3) if len(valid) else None,
            q2=round(gumbel_q(xi, alpha, 2), 2) if math.isfinite(xi) else None,
            q5=round(gumbel_q(xi, alpha, 5), 2) if math.isfinite(xi) else None,
            q20=round(gumbel_q(xi, alpha, 20), 2) if math.isfinite(xi) else None,
            accepted=accepted,
            note="" if accepted else (
                f"rerata debit sel terbaik {id_num(mean_lt, 1)} m³/s tidak sebanding dengan perkiraan {id_num(q_exp, 0)} m³/s dari luas DAS (rasio {id_num(ratio, 2)})"
                if math.isfinite(ratio) else "data reanalisis tidak lengkap"
            ),
        )
        results.append(rec)
        print(f"[{i:02d}/{len(stations)}] {st['id']:<24} exp {q_exp:9.1f}  sel {mean_lt:9.1f}  r={ratio:5.2f}  {'OK' if accepted else 'TOLAK'}  ({client.spent:.0f} bobot)", flush=True)

    out = {
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source": "Copernicus Emergency Management Service, GloFAS v4 (reanalisis), melalui Open-Meteo Flood API (CC BY 4.0)",
        "model": CLIM_MODEL,
        "period": f"{CLIM_START[:4]}–{CLIM_END[:4]}",
        "method": (
            f"Sel: pindai kisi {SCAN_STEP_DEG}° radius ±{SCAN_STEP_DEG * SCAN_RADIUS_STEPS:.2f}°, rerata {SCAN_START}..{SCAN_END}, "
            f"skor |ln(Q/Q_harapan)| + jarak/{DIST_PENALTY_KM:.0f} km; diterima bila rerata {CLIM_START[:4]}–{CLIM_END[:4]} "
            f"dalam ×{ACCEPT_FACTOR:g} Q_harapan. Ambang: p90/p99 debit harian; Q2/Q5/Q20 Gumbel L-moment atas maksimum tahunan "
            f"(tahun ≥ {MIN_VALID_DAYS_PER_YEAR} hari valid)."
        ),
        "specific_discharge_assumption_lps_km2": SPECIFIC_Q_LPS_KM2,
        "stations": [
            {
                "id": r["id"],
                "accepted": r["accepted"],
                "cell": r.get("cell") if r["accepted"] else None,
                "expected_q_m3s": r["expected_q_m3s"],
                "annual_max_m3s": r.get("annual_max_m3s", []) if r["accepted"] else [],
                "p90_m3s": r.get("p90_m3s") if r["accepted"] else None,
                "p99_m3s": r.get("p99_m3s") if r["accepted"] else None,
                "period": f"{CLIM_START[:4]}–{CLIM_END[:4]}",
                "note": r.get("note", ""),
            }
            for r in results
        ],
    }
    if not args.only:
        OUT_JSON.write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
        print(f"tulis {OUT_JSON}")
    (args.cache_dir / "calibration_detail.json").write_text(json.dumps(results, ensure_ascii=False, indent=1), encoding="utf-8")
    if args.report:
        lines = [
            "| Stasiun | Sel (lat, lon) | Offset km | Q̄ sel m³/s | Q harapan m³/s | Rasio | Q2 | Q5 | Q20 | n | Status |",
            "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|",
        ]
        for r in results:
            c = r.get("cell") or {}
            lines.append(
                f"| {r['id']} | {c.get('lat', '–')}, {c.get('lon', '–')} | {c.get('offset_km', '–')} | {c.get('mean_q_m3s', '–')} | "
                f"{r['expected_q_m3s']} | {r.get('ratio_to_expected', '–')} | {r.get('q2', '–')} | {r.get('q5', '–')} | {r.get('q20', '–')} | "
                f"{len(r.get('annual_max_m3s', []))} | {'diterima' if r['accepted'] else 'ditolak'} |"
            )
        args.report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"bobot API terpakai sesi ini: {client.spent:.1f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
