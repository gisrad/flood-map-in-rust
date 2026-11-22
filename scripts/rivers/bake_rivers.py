#!/usr/bin/env python3
"""Bake geometri sungai nyata (OpenStreetMap) untuk 50 stasiun, dua tingkat detail (LOD).

Keluaran:
  crates/flood-web/public/data/rivers.geojson           ikhtisar  (simplifikasi <= 0,0003 deg ~ 30 m; <= 1,5 MB)
  crates/flood-web/public/data/rivers/<station_id>.geojson   detail  (<= 0,00005 deg ~ 5 m; <= 400 KB per berkas)

Langkah:
  1. Baca stasiun dari crates/flood-shared/src/catalog.rs (id, sungai, lat/lon, luas DAS).
  2. Overpass API: way[waterway=river|canal][name~varian] dalam bbox sebanding luas DAS
     (satu kueri per sungai; respons dicache sehingga bake berikutnya offline).
  3. Rakit rantai way DENGAN MEMPERTAHANKAN ARAH way OSM (digitalisasi mengalir ke hilir;
     tidak pernah dibalik). Rantai hanya disambung ujung-akhir -> ujung-awal.
  4. Pilih komponen terhubung terdekat pos, potong ke lingkaran skala DAS (arah dipertahankan),
     sederhanakan dengan Douglas-Peucker (urutan titik tidak berubah), bulatkan.
  5. Stasiun tanpa geometri meyakinkan TIDAK mendapat fitur (tidak ada garis karangan).

Properti fitur: station_id, name, source, downstream=true (urutan koordinat = arah aliran hilir).

Pemakaian:
  python scripts/rivers/bake_rivers.py --cache <DIR> [--engine shapely|rustgis] [--report FILE]
Butuh: requests, shapely (rustgis opsional untuk simplifikasi).
Lisensi data: (c) OpenStreetMap contributors, ODbL.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sys
import time
from pathlib import Path

import requests
from shapely.geometry import LineString, Point, mapping, shape
from shapely.ops import unary_union

REPO = Path(__file__).resolve().parents[2]
CATALOG = REPO / "crates/flood-shared/src/catalog.rs"
DATA_DIR = REPO / "crates/flood-web/public/data"
SOURCE = "© OpenStreetMap contributors (ODbL)"
ENDPOINTS = [
    "https://overpass-api.de/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
]
UA = {"User-Agent": "flood-transbulent-bake/1.0 (https://flood.transbulent.id)"}
MIN_GAP_S = 5.0

TOL_OVERVIEW = 0.0003   # derajat (~33 m)
TOL_DETAIL = 0.00005    # derajat (~5,5 m)
DEC_OVERVIEW = 4        # ~11 m
DEC_DETAIL = 5          # ~1,1 m
MAX_OVERVIEW_BYTES = 1_500_000
MAX_DETAIL_BYTES = 400_000

PREFIX = r"(?:Sungai |Kali |Kali\. |Batang |Krueng |Way |Wae |Tukad |Wai |Bengawan |Salo |Ci |Sei |Sungei |Air |Kr\. |S\. )?"
NAME = {
    "ciliwung-katulampa": r"Ci ?Liwung",
    "ciliwung-manggarai": r"Ci ?Liwung",
    "cisadane-tangerang": r"Ci ?Sadane",
    "bekasi-pondokgede": r"Bekasi|Ci ?Leungsi|Ci ?Keas",
    "citarum-nanjung": r"Ci ?Tarum",
    "citarum-karawang": r"Ci ?Tarum",
    "cimanuk-jatigede": r"Ci ?Manuk",
    "citanduy-banjar": r"Ci ?Tanduy",
    "serayu-banyumas": r"Serayu",
    "progo-bantul": r"Progo",
    "wulan-demak": r"Wulan",
    "solo-sragen": r"Solo",
    "solo-bojonegoro": r"Solo",
    "brantas-mojokerto": r"Brantas",
    "sampean-situbondo": r"Sampean",
    "krueng-aceh": r"Aceh",
    "deli-medan": r"Deli",
    "asahan-tanjungbalai": r"Asahan",
    "rokan-bagansiapiapi": r"Rokan",
    "siak-pekanbaru": r"Siak",
    "kampar-pelalawan": r"Kampar",
    "indragiri-rengat": r"Indragiri|Kuantan",
    "batanghari-jambi": r"Batanghari|Hari",
    "musi-palembang": r"Musi",
    "ogan-baturaja": r"Komering|Ogan",
    "batang-anai-padang": r"Anai",
    "air-bengkulu": r"Bengkulu",
    "way-sekampung": r"Sekampung",
    "sambas-kalbar": r"Sambas",
    "kapuas-sintang": r"Kapuas",
    "kapuas-pontianak": r"Kapuas",
    "sampit-mentaya": r"Mentaya|Sampit",
    "kahayan-palangkaraya": r"Kahayan",
    "barito-buntok": r"Barito",
    "barito-banjarmasin": r"Barito",
    "mahakam-samarinda": r"Mahakam",
    "kayan-tanjungselor": r"Kayan",
    "tukad-unda": r"Unda",
    "dodokan-lombok": r"Dodokan",
    "benanain-malaka": r"Benanain|Benain",
    "saddang-pinrang": r"Sa?dd?ang",
    "jeneberang-bili": r"Je.?neberang?",
    "walanae-wajo": r"Wal+[ae]+n+ae|Walanae|Walennae",
    "konaweha-kendari": r"Konawe",
    "palu-sigi": r"Palu",
    "limboto-gorontalo": r"Bone|Bolango",
    "tondano-manado": r"Tondano",
    "batumerah-ambon": r"Batu ?Merah",
    "mamberamo-papua": r"Mamberamo",
    "digul-tanahmerah": r"Digul|Digoel",
}


def read_catalog():
    s = CATALOG.read_text(encoding="utf8")
    i = s.index("RIVER_STATIONS")
    j = s.find("COASTAL", i)
    blk = s[i : j if j > 0 else len(s)]
    out = []
    for m in re.finditer(r"StationStatic \{(.*?)\n    \},", blk, re.S):
        b = m.group(1)

        def g(k):
            return re.search(k + r": ([^\n]*),", b).group(1).strip('"')

        out.append(
            dict(id=g("id"), name=g("name"), river=g("river"), lat=float(g("lat")),
                 lon=float(g("lon")), area=float(g("basin_area_km2")))
        )
    return out


def radius_deg(area_km2: float) -> float:
    """Radius potong (derajat) sebanding dengan skala DAS: ~0,8*sqrt(A) km, 0,15..2,0 derajat."""
    return min(2.0, max(0.15, 0.8 * math.sqrt(area_km2) / 111.0))


def overpass(q: str, cache: Path, log) -> dict:
    cache.mkdir(parents=True, exist_ok=True)
    f = cache / (hashlib.sha1(q.encode()).hexdigest()[:16] + ".json")
    if f.exists():
        return json.loads(f.read_text(encoding="utf8"))
    wait = MIN_GAP_S
    for attempt in range(8):
        ep = ENDPOINTS[attempt % len(ENDPOINTS)] if attempt >= 3 else ENDPOINTS[0]
        time.sleep(MIN_GAP_S)
        try:
            r = requests.post(ep, data={"data": q}, headers=UA, timeout=150)
            if r.status_code == 200 and r.text.lstrip().startswith("{"):
                d = r.json()
                if "remark" in d and "runtime error" in d["remark"].lower():
                    raise RuntimeError(d["remark"])
                f.write_text(json.dumps(d), encoding="utf8")
                return d
            log(f"  {ep} -> HTTP {r.status_code}")
        except Exception as e:  # noqa: BLE001
            log(f"  {ep} -> {type(e).__name__}: {str(e)[:80]}")
        wait = min(wait * 2, 60)
        time.sleep(wait)
    raise RuntimeError("Overpass gagal: " + q[:80])


def name_ok(name: str, tok: str) -> bool:
    return re.match(rf"^{PREFIX}(?:{tok})\b", name, re.I) is not None


def key(c):
    return (round(c[0], 7), round(c[1], 7))


def components(ways):
    """Komponen terhubung (undirected) berdasarkan vertex bersama; hanya untuk pemilihan."""
    parent = list(range(len(ways)))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    owner = {}
    for i, w in enumerate(ways):
        for c in w:
            k = key(c)
            if k in owner:
                parent[find(i)] = find(owner[k])
            else:
                owner[k] = i
    groups = {}
    for i in range(len(ways)):
        groups.setdefault(find(i), []).append(i)
    return list(groups.values())


def chain_directed(ways):
    """Rangkai way menjadi rantai searah: way A disambung ke B hanya bila akhir(A) == awal(B).
    Arah way tidak pernah dibalik. Mengembalikan daftar daftar-koordinat."""
    starts = {}
    for i, w in enumerate(ways):
        starts.setdefault(key(w[0]), []).append(i)
    nxt, has_prev = {}, set()
    for i, w in enumerate(ways):
        for j in starts.get(key(w[-1]), []):
            if j != i and j not in has_prev:
                nxt[i] = j
                has_prev.add(j)
                break
    out, seen = [], set()
    for i in range(len(ways)):
        if i in has_prev:
            continue
        chain, j = list(ways[i]), i
        seen.add(i)
        while j in nxt and nxt[j] not in seen:
            j = nxt[j]
            seen.add(j)
            chain += ways[j][1:]
        out.append(chain)
    for i in range(len(ways)):  # siklus murni (jarang)
        if i not in seen:
            out.append(list(ways[i]))
            seen.add(i)
    return out


def clip_keep_direction(coords, circle):
    """Potong satu rantai ke lingkaran; setiap potongan diorientasikan sama dengan rantai asal."""
    ln = LineString(coords)
    inter = ln.intersection(circle)
    if inter.is_empty:
        return []
    parts = [g for g in (inter.geoms if hasattr(inter, "geoms") else [inter]) if g.geom_type == "LineString"]
    res = []
    for g in parts:
        c = list(g.coords)
        if ln.project(Point(c[0])) > ln.project(Point(c[-1])):
            c.reverse()
        res.append(c)
    return res


def simp_shapely(lines, tol):
    return [list(LineString(c).simplify(tol, preserve_topology=False).coords) for c in lines]


def simp_rustgis(lines, tol):
    import rustgis  # noqa: PLC0415

    s = rustgis.Session()
    fc = {"type": "FeatureCollection",
          "features": [{"type": "Feature", "properties": {"i": i},
                        "geometry": {"type": "LineString", "coordinates": c}} for i, c in enumerate(lines)]}
    layer = s.add_geojson(fc, name="in")
    km = tol * 111.0  # RustGIS: toleransi dalam km
    out = s.run_tool("simplify", input=layer, tolerance=km).to_geojson()
    res = {f["properties"]["i"]: f["geometry"]["coordinates"] for f in out["features"]}
    return [res[i] for i in range(len(lines))]


def finalize(lines, dec, min_len, simp, tol):
    lines = simp(lines, tol) if tol > 0 else lines
    out = []
    for c in lines:
        q = []
        for x, y in c:
            p = [round(x, dec), round(y, dec)]
            if not q or p != q[-1]:
                q.append(p)
        if len(q) >= 2 and LineString(q).length >= min_len:
            out.append(q)
    return out


def to_feature(st, lines):
    geometry = (
        {"type": "LineString", "coordinates": lines[0]}
        if len(lines) == 1
        else {"type": "MultiLineString", "coordinates": lines}
    )
    return {
        "type": "Feature",
        "properties": {"station_id": st["id"], "name": st["river"], "source": SOURCE, "downstream": True},
        "geometry": geometry,
    }


def length_km(lines, lat):
    c = math.cos(math.radians(lat))
    return sum(math.hypot((b[0] - a[0]) * c, b[1] - a[1]) * 111.0 for p in lines for a, b in zip(p, p[1:]))


def dumps(obj):
    return json.dumps(obj, separators=(",", ":"), ensure_ascii=False)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", required=True, help="folder cache respons Overpass")
    ap.add_argument("--data-dir", default=str(DATA_DIR))
    ap.add_argument("--engine", default="shapely", choices=["shapely", "rustgis"])
    ap.add_argument("--report", default=None, help="tulis laporan JSON per stasiun")
    ap.add_argument("--max-seed-km", type=float, default=9.0)
    args = ap.parse_args()
    log = lambda m: print(m, flush=True)  # noqa: E731
    cache, ddir = Path(args.cache), Path(args.data_dir)
    stations = read_catalog()
    assert len(stations) == 50, len(stations)
    simp = simp_rustgis if args.engine == "rustgis" else simp_shapely

    groups: dict[str, list[dict]] = {}
    for st in stations:
        groups.setdefault(NAME[st["id"]], []).append(st)
    data = {}
    for tok, sts in groups.items():
        rs = [radius_deg(s["area"]) for s in sts]
        S = min(s["lat"] - r for s, r in zip(sts, rs))
        N = max(s["lat"] + r for s, r in zip(sts, rs))
        W = min(s["lon"] - r for s, r in zip(sts, rs))
        E = max(s["lon"] + r for s, r in zip(sts, rs))
        q = (
            f'[out:json][timeout:120];way["waterway"~"^(river|canal)$"]["name"~"{tok}",i]'
            f"({S:.3f},{W:.3f},{N:.3f},{E:.3f});out geom;"
        )
        log(f"[overpass] {tok}  bbox=({S:.2f},{W:.2f},{N:.2f},{E:.2f})")
        data[tok] = overpass(q, cache, log)

    ov_feats, report = [], []
    (ddir / "rivers").mkdir(parents=True, exist_ok=True)
    for old in (ddir / "rivers").glob("*.geojson"):
        old.unlink()
    for st in stations:
        tok = NAME[st["id"]]
        r = radius_deg(st["area"])
        ways = []
        for el in data[tok]["elements"]:
            if el.get("type") != "way" or "geometry" not in el:
                continue
            if not name_ok(el.get("tags", {}).get("name", ""), tok):
                continue
            pts = [(p["lon"], p["lat"]) for p in el["geometry"]]
            if len(pts) >= 2:
                ways.append(pts)
        rep = dict(station_id=st["id"], radius_deg=round(r, 3), ways_in_bbox=len(ways))
        pt = Point(st["lon"], st["lat"])
        circle = pt.buffer(r, 64)
        ways = [w for w in ways if LineString(w).intersects(circle)]
        if not ways:
            rep.update(status="tidak ada way di sekitar pos")
            report.append(rep)
            log(f"  {st['id']}: tidak ada geometri")
            continue
        comps = components(ways)
        dists = [min(LineString(ways[i]).distance(pt) for i in c) for c in comps]
        k = min(range(len(comps)), key=lambda i: dists[i])
        seed_km = dists[k] * 111.0
        rep["seed_km"] = round(seed_km, 2)
        if seed_km > args.max_seed_km:
            rep.update(status=f"komponen terdekat {seed_km:.1f} km dari pos (> {args.max_seed_km} km): ditolak")
            report.append(rep)
            log(f"  {st['id']}: ditolak, jarak {seed_km:.1f} km")
            continue
        keep = list(comps[k])
        union = unary_union([LineString(ways[i]) for i in keep])
        rest = [c for i, c in enumerate(comps) if i != k]
        changed = True
        while changed:  # gabung komponen lain dalam celah <= 0,03 derajat (~3 km)
            changed = False
            for c in list(rest):
                if min(LineString(ways[i]).distance(union) for i in c) <= 0.03:
                    keep += c
                    union = unary_union([union] + [LineString(ways[i]) for i in c])
                    rest.remove(c)
                    changed = True
        sel = [ways[i] for i in keep]
        chains = chain_directed(sel)

        def build(radius):
            circ = pt.buffer(radius, 64)
            clipped = []
            for ch in chains:
                clipped += clip_keep_direction(ch, circ)
            return clipped

        # --- ikhtisar ---
        ov = finalize(build(r), DEC_OVERVIEW, 0.002, simp, TOL_OVERVIEW)
        if not ov:
            rep.update(status="kosong setelah pemotongan")
            report.append(rep)
            continue
        ov_feats.append(to_feature(st, ov))

        # --- detail: radius diperkecil bertahap bila berkas > 400 KB ---
        rd = r
        while True:
            dl = finalize(build(rd), DEC_DETAIL, 0.0005, simp, TOL_DETAIL)
            blob = dumps({"type": "FeatureCollection", "features": [to_feature(st, dl)]}) if dl else ""
            if len(blob.encode()) <= MAX_DETAIL_BYTES or rd < 0.06:
                break
            rd *= 0.85
        if dl:
            (ddir / "rivers" / f"{st['id']}.geojson").write_text(blob, encoding="utf8")
        rep.update(
            status="ok", overview_segments=len(ov), overview_points=sum(len(p) for p in ov),
            overview_km=round(length_km(ov, st["lat"]), 1),
            detail_radius_deg=round(rd, 3), detail_points=sum(len(p) for p in dl),
            detail_km=round(length_km(dl, st["lat"]), 1), detail_bytes=len(blob.encode()),
            ways_selected=len(sel), chains=len(chains),
        )
        report.append(rep)
        log(f"  {st['id']}: ok seed={seed_km:.1f} km | ikhtisar {rep['overview_points']} titik | "
            f"detail r={rd:.2f} {rep['detail_points']} titik {len(blob) / 1024:.0f} KB")

    ov_blob = dumps({"type": "FeatureCollection", "features": ov_feats})
    (ddir / "rivers.geojson").write_text(ov_blob, encoding="utf8")
    log(f"ikhtisar {len(ov_blob.encode()) / 1024:.0f} KB (batas {MAX_OVERVIEW_BYTES // 1024} KB), "
        f"{len(ov_feats)}/50 stasiun")
    if len(ov_blob.encode()) > MAX_OVERVIEW_BYTES:
        log("PERINGATAN: ikhtisar melebihi batas ukuran")
    if args.report:
        Path(args.report).write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf8")


if __name__ == "__main__":
    sys.exit(main())
