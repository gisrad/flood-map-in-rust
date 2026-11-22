#!/usr/bin/env python3
"""Bakukan data replay kejadian historis ke crates/flood-shared/data/replays/<id>.json.

Sumber per skenario (hari kalender GMT):
- Debit: GloFAS v4 reanalisis (Open-Meteo Flood API, models=consolidated_v4) pada sel terkalibrasi
  dari crates/flood-shared/data/glofas_calibration.json; stasiun tanpa sel diterima memakai koordinat pos.
- Hujan harian dan kelembapan tanah: ERA5 (Open-Meteo Archive API, models=era5) di koordinat pos.
- Muka laut (hanya bila periode ≥ 2022-01-01): Open-Meteo Marine `sea_level_height_msl` + `wave_height`
  pada sel laut dari rob_calibration.json; maksimum harian.
Narasi bingkai hanya fakta bersumber (EVENT_FACTS, diverifikasi manual dengan tautan) atau ringkasan
angka dari data di atas.

Pemakaian:
  python scripts/bake_replays.py --cache-dir <dir>
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import sys
import urllib.parse
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calibrate_glofas import CATALOG_RS, Client, call_weight, n_days, parse_catalog  # noqa: E402
from calibrate_rob import parse_rob  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
DATA = REPO / "crates/flood-shared/data"
OUT_DIR = DATA / "replays"

PROVENANCE = "Debit: simulasi reanalisis GloFAS v4 via Open-Meteo; hujan: ERA5. Bukan pengukuran pos duga air."

BULAN = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"]

SCENARIOS = [
    {
        "id": "jabodetabek-2020",
        "title": "Banjir Jabodetabek, 1 Januari 2020",
        "period": "30 Des 2019 – 4 Jan 2020",
        "center_lon": 106.86,
        "center_lat": -6.30,
        "zoom": 9.4,
        "start": "2019-12-20",
        "frames": ("2019-12-30", "2020-01-04"),
        "focus": ["ciliwung-manggarai", "bekasi-pondokgede", "cisadane-tangerang"],
        "citation": (
            "DW Indonesia, 2 Jan 2020 (Kepala BNPB: TNI AU Halim 377 mm/hari, Taman Mini 335 mm/hari, Jatiasih 259 mm/hari) "
            "https://www.dw.com/id/banjir-jakarta-disebabkan-hujan-sangat-ekstrem-20-ribu-orang-mengungsi/a-51851463; "
            "Mongabay, 3 Jan 2020 (BMKG: hujan ekstrem 31/12/2019 sore hingga 1/1/2020) "
            "https://mongabay.co.id/2020/01/03/bmkg-curah-hujan-paling-ekstrim-dalam-sejarah-jadi-pemicu-banjir-jakarta"
        ),
        "facts": {
            "2019-12-31": "BMKG mencatat hujan ekstrem berdurasi panjang sejak Selasa (31/12/2019) sore hingga Rabu (1/1/2020), merata di DKI Jakarta (Mongabay, 3/1/2020).",
            "2020-01-01": "Kepala BNPB: curah hujan di TNI AU Halim 377 mm/hari, Taman Mini 335 mm/hari, Jatiasih 259 mm/hari (DW, 2/1/2020).",
        },
    },
    {
        "id": "barito-2021",
        "title": "Banjir Kalimantan Selatan, Januari 2021",
        "period": "8 – 24 Jan 2021",
        "center_lon": 115.0,
        "center_lat": -2.9,
        "zoom": 7.2,
        "start": "2020-12-29",
        "frames": ("2021-01-08", "2021-01-24"),
        "focus": ["barito-banjarmasin", "barito-buntok"],
        "citation": (
            "BNPB, 17 Jan 2021, [Update] 10 Kabupaten/Kota Terdampak Banjir di Kalimantan Selatan "
            "https://bnpb.go.id/berita/-update-10-kabupaten-kota-terdampak-banjir-di-kalimantan-selatan"
        ),
        "facts": {
            "2021-01-12": "Banjir mulai terjadi pada Selasa (12/1) di Kalimantan Selatan (BNPB).",
            "2021-01-14": "Pemerintah Provinsi Kalimantan Selatan menetapkan status tanggap darurat bencana banjir (BNPB).",
            "2021-01-17": "BNPB (data 17/1 pukul 14.00 WIB): 10 kabupaten/kota terdampak, 24.379 rumah terendam, 39.549 warga mengungsi, 15 orang meninggal.",
        },
    },
    {
        "id": "pantura-wulan-2024",
        "title": "Tanggul Sungai Wulan jebol, Demak 2024",
        "period": "5 Feb – 22 Mar 2024",
        "center_lon": 110.70,
        "center_lat": -6.85,
        "zoom": 9.2,
        "start": "2024-01-26",
        "frames": ("2024-02-05", "2024-03-22"),
        "focus": ["wulan-demak", "solo-sragen"],
        "citation": (
            "Portal Kabupaten Demak, 9 Feb 2024 https://demakkab.go.id/news/banjir-di-karanganyar-demak-memutus-jalur-pantura-ketinggian-hingga-dua-meter; "
            "Kompas.id, 19 Feb 2024 https://www.kompas.id/artikel/banjir-demak-berangsur-surut-warga-yang-mengungsi-mulai-kembali-ke-rumah; "
            "BNPB, 18 Mar 2024 https://bnpb.go.id/berita/enam-tanggul-jebol-11-kecamatan-di-kabupaten-demak-terendam-banjir"
        ),
        "facts": {
            "2024-02-09": "Banjir akibat jebolnya tanggul Sungai Wulan di Karanganyar memutus jalur pantura Demak–Kudus (Portal Kabupaten Demak, 9/2/2024).",
            "2024-02-19": "Banjir yang merendam Demak sejak 9/2 berangsur surut; sebagian pengungsi kembali ke rumah (Kompas, 19/2/2024).",
            "2024-03-13": "Hujan intensitas tinggi di Jawa Tengah; enam tanggul jebol memicu banjir di Demak (BNPB, 18/3/2024).",
            "2024-03-17": "Tanggul Sungai Wulan di perbatasan Demak–Kudus kembali jebol (BNPB, 18/3/2024).",
            "2024-03-18": "BPBD Demak: 89 desa di 11 kecamatan terendam 30–80 cm; 93.149 jiwa terdampak, 22.725 mengungsi (BNPB, 18/3/2024).",
        },
    },
]


def daterange(a: str, b: str) -> list[str]:
    d0, d1 = dt.date.fromisoformat(a), dt.date.fromisoformat(b)
    return [(d0 + dt.timedelta(days=i)).isoformat() for i in range((d1 - d0).days + 1)]


def label(d: str) -> str:
    x = dt.date.fromisoformat(d)
    return f"{x.day} {BULAN[x.month - 1]} {x.year}"


def as_list(data):
    return data if isinstance(data, list) else [data]


def rnd(v, nd=2):
    return None if v is None else round(float(v), nd)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache-dir", required=True, type=Path)
    ap.add_argument("--hourly-budget", type=float, default=3500.0)
    args = ap.parse_args()
    client = Client(args.cache_dir, args.hourly_budget)

    stations = parse_catalog(CATALOG_RS)
    rob_points = parse_rob(CATALOG_RS)
    cal = {s["id"]: s for s in json.loads((DATA / "glofas_calibration.json").read_text(encoding="utf-8"))["stations"]}
    rob_cal = {p["id"]: p for p in json.loads((DATA / "rob_calibration.json").read_text(encoding="utf-8"))["points"]}
    if not cal:
        print("glofas_calibration.json kosong; jalankan calibrate_glofas.py dulu", file=sys.stderr)
        return 1
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    for sc in SCENARIOS:
        end = sc["frames"][1]
        dates = daterange(sc["start"], end)
        nd = n_days(sc["start"], end)

        cells = []
        for st in stations:
            c = cal.get(st["id"], {}).get("cell")
            cells.append((c["lat"], c["lon"]) if c else (st["lat"], st["lon"]))
        q = {
            "latitude": ",".join(f"{a:.4f}" for a, _ in cells),
            "longitude": ",".join(f"{b:.4f}" for _, b in cells),
            "daily": "river_discharge",
            "start_date": sc["start"],
            "end_date": end,
            "models": "consolidated_v4",
            "cell_selection": "nearest",
        }
        flood = as_list(client.get_json(
            "https://flood-api.open-meteo.com/v1/flood?" + urllib.parse.urlencode(q, safe=","),
            call_weight(len(cells), 1, nd),
        ))

        q = {
            "latitude": ",".join(f"{s['lat']:.4f}" for s in stations),
            "longitude": ",".join(f"{s['lon']:.4f}" for s in stations),
            "daily": "precipitation_sum",
            "hourly": "soil_moisture_0_to_7cm,soil_moisture_28_to_100cm",
            "start_date": sc["start"],
            "end_date": end,
            "models": "era5",
            "timezone": "GMT",
        }
        era5 = as_list(client.get_json(
            "https://archive-api.open-meteo.com/v1/archive?" + urllib.parse.urlencode(q, safe=","),
            call_weight(len(stations), 3, nd),
        ))

        out_stations = []
        for st, fl, er in zip(stations, flood, era5):
            fmap = dict(zip(fl["daily"]["time"], fl["daily"]["river_discharge"]))
            rmap = dict(zip(er["daily"]["time"], er["daily"]["precipitation_sum"]))
            soil = {}
            for t, a, b in zip(er["hourly"]["time"], er["hourly"]["soil_moisture_0_to_7cm"], er["hourly"]["soil_moisture_28_to_100cm"]):
                if a is not None and b is not None:
                    soil.setdefault(t[:10], []).append((a, b))
            out_stations.append({
                "id": st["id"],
                "q": [rnd(fmap.get(d), 2) for d in dates],
                "rain": [rnd(rmap.get(d), 1) for d in dates],
                "soil_top": [rnd(sum(x[0] for x in soil[d]) / len(soil[d]), 3) if d in soil else None for d in dates],
                "soil_deep": [rnd(sum(x[1] for x in soil[d]) / len(soil[d]), 3) if d in soil else None for d in dates],
            })

        rob_out = []
        if sc["start"] >= "2022-01-01":
            coords = [
                (rob_cal[p["id"]]["cell_lat"], rob_cal[p["id"]]["cell_lon"]) if p["id"] in rob_cal else (p["lat"], p["lon"])
                for p in rob_points
            ]
            q = {
                "latitude": ",".join(f"{a:.4f}" for a, _ in coords),
                "longitude": ",".join(f"{b:.4f}" for _, b in coords),
                "hourly": "sea_level_height_msl,wave_height",
                "start_date": sc["start"],
                "end_date": end,
                "timezone": "GMT",
                "cell_selection": "sea",
            }
            marine = as_list(client.get_json(
                "https://marine-api.open-meteo.com/v1/marine?" + urllib.parse.urlencode(q, safe=","),
                call_weight(len(coords), 2, nd),
            ))
            for p, m in zip(rob_points, marine):
                best: dict[str, tuple[float, str]] = {}
                wave: dict[str, float] = {}
                for t, s, w in zip(m["hourly"]["time"], m["hourly"]["sea_level_height_msl"], m["hourly"]["wave_height"]):
                    d = t[:10]
                    if s is not None and (d not in best or s > best[d][0]):
                        best[d] = (s, t + ":00Z")
                    if w is not None:
                        wave[d] = max(wave.get(d, w), w)
                if not best:
                    continue
                rob_out.append({
                    "id": p["id"],
                    "msl_max": [rnd(best[d][0], 3) if d in best else None for d in dates],
                    "msl_max_time": [best[d][1] if d in best else None for d in dates],
                    "wave_max": [rnd(wave.get(d), 2) for d in dates],
                })

        by_id = {s["id"]: s for s in out_stations}
        names = {s["id"]: s for s in stations}
        frames = []
        for d in daterange(*sc["frames"]):
            i = dates.index(d)
            fact = sc["facts"].get(d)
            if fact:
                narrative = fact
            else:
                parts = []
                for fid in sc["focus"]:
                    s = by_id[fid]
                    qv, rv = s["q"][i], s["rain"][i]
                    if qv is None:
                        continue
                    parts.append(f"{names[fid]['name']}: debit model {qv:,.0f} m³/s".replace(",", ".") + f", hujan ERA5 {rv or 0:.0f} mm")
                narrative = "; ".join(parts) + "."
            frames.append({
                "date": d,
                "step_label": label(d),
                "timestamp_wib": f"{label(d)}, 07.00 WIB",
                "narrative": narrative,
            })

        out = {
            "id": sc["id"],
            "title": sc["title"],
            "period": sc["period"],
            "center_lon": sc["center_lon"],
            "center_lat": sc["center_lat"],
            "zoom": sc["zoom"],
            "citation": sc["citation"],
            "provenance": PROVENANCE,
            "note": "Satu bingkai = satu hari (GMT). Rob hanya tersedia untuk periode sejak 2022." if not rob_out else "Satu bingkai = satu hari (GMT). Muka laut: Open-Meteo Marine (Copernicus Marine), maksimum harian.",
            "generated_at_utc": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "dates": dates,
            "frames": frames,
            "stations": out_stations,
            "rob": rob_out,
        }
        path = OUT_DIR / f"{sc['id']}.json"
        path.write_text(json.dumps(out, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
        print(f"tulis {path} ({path.stat().st_size // 1024} KB, {len(frames)} bingkai)")
    print(f"bobot API sesi ini: {client.spent:.1f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
