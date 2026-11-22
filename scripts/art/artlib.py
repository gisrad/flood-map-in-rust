"""Pustaka bersama pipeline karya visual (forge3D native viewer).

Alur: DEM Terrarium (fetch_dem.py) -> persegi -> penyiapan (haluskan laut, kompres batimetri)
      -> tekstur overlay (tint hipsometrik x hillshade multi-arah + sungai OSM) -> viewer forge3D -> snapshot.
Palet mengikuti token situs: kertas #F5F3EE, tinta #0E1C24, sungai #2F7FC1 / #5DB4E8.
"""
from __future__ import annotations

import json
import os
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter
from scipy import ndimage as ndi

PAPER = (0xF5, 0xF3, 0xEE)
INK = (0x0E, 0x1C, 0x24)
RIVER = (0x2F, 0x7F, 0xC1)
RIVER_GLOW = (0x5D, 0xB4, 0xE8)


def hex2rgb(h):
    h = h.lstrip("#")
    return np.array([int(h[i : i + 2], 16) for i in (0, 2, 4)], dtype=np.float32)


def ramp(t, stops):
    """t di [0,1] -> RGB dari daftar (posisi, hex)."""
    pos = np.array([p for p, _ in stops], dtype=np.float32)
    cols = np.stack([hex2rgb(c) for _, c in stops])
    t = np.clip(t, pos[0], pos[-1])
    out = np.empty(t.shape + (3,), np.float32)
    for k in range(3):
        out[..., k] = np.interp(t, pos, cols[:, k])
    return out


# laut: kedalaman (m, negatif) -> warna; paparan teal terang ke biru-tinta dalam
SEA_STOPS = [
    (0.00, "#A9D3D4"),  # 0 m
    (0.03, "#8FC2C8"),  # -60 m (paparan)
    (0.08, "#6EA8B8"),  # -200 m
    (0.22, "#3F7FA0"),  # -800 m
    (0.45, "#235A80"),  # -2000 m
    (0.75, "#133E5E"),  # -4000 m
    (1.00, "#0B2A41"),  # -6500 m
]
SEA_MAX_DEPTH = 6500.0

# darat: ketinggian (m) -> hipsometrik lembut di atas kertas
LAND_STOPS = [
    (0.00, "#C9D6B4"),  # pesisir/dataran banjir
    (0.04, "#D6DDB9"),  # 100 m
    (0.12, "#E4E1BE"),  # 300 m
    (0.25, "#E2D2A8"),  # 700 m
    (0.45, "#D1B98E"),  # 1300 m
    (0.70, "#BFA07A"),  # 2100 m
    (1.00, "#D8CFC4"),  # >= 3000 m
]
LAND_MAX = 3000.0


def hillshade(z, res, az_deg, alt_deg, exag):
    dzdy, dzdx = np.gradient(z, res)
    nx, ny, nz = -dzdx * exag, dzdy * exag, np.ones_like(z)
    inv = 1.0 / np.sqrt(nx * nx + ny * ny + nz * nz)
    az, alt = np.radians(az_deg), np.radians(alt_deg)
    lx, ly, lz = np.cos(alt) * np.sin(az), np.cos(alt) * np.cos(az), np.sin(alt)
    return np.clip((nx * lx + ny * ly + nz * lz) * inv, 0, 1)


def multi_hillshade(z, res, exag):
    hs = (0.40 * hillshade(z, res, 315, 38, exag) + 0.25 * hillshade(z, res, 270, 42, exag)
          + 0.20 * hillshade(z, res, 0, 40, exag) + 0.15 * hillshade(z, res, 225, 45, exag))
    return hs, float(np.sin(np.radians(40)))


def prepare_dem(z, res, sea_sigma=2.0, land_sigma=0.5, sea_factor=0.10):
    """Haluskan laut (batimetri Terrarium bergranul), kompres kedalaman ekstrem.
    Mengembalikan DEM yang dipakai untuk geometri 3D maupun hillshade."""
    z = z.astype(np.float32)
    sea = z < 0
    zs = ndi.gaussian_filter(z, sea_sigma)
    zl = ndi.gaussian_filter(z, land_sigma) if land_sigma > 0 else z
    # pantai: gunakan peta tepi-lembut agar tidak ada tangga
    w = ndi.gaussian_filter(sea.astype(np.float32), 1.0)
    out = zl * (1 - w) + zs * w
    neg = out < 0
    out[neg] = out[neg] * sea_factor  # batimetri jadi relief halus; kedalaman dibawa oleh warna
    return out


def make_tint(z_raw, z_geom, res, land_exag, sea_exag=14.0, sea_shade=0.55, land_max=LAND_MAX, land_gamma=1.0):
    """RGB float32 [0..255]: tint hipsometrik x hillshade multi-arah."""
    sea = z_raw < 0
    # tint laut dari kedalaman asli (bukan terkompres)
    st = np.clip((-np.minimum(z_raw, 0)) / SEA_MAX_DEPTH, 0, 1)
    sea_col = ramp(st, SEA_STOPS)
    lt = np.clip(np.maximum(z_raw, 0) / land_max, 0, 1) ** land_gamma
    land_col = ramp(lt, LAND_STOPS)
    wsea = ndi.gaussian_filter(sea.astype(np.float32), 0.8)[..., None]
    col = land_col * (1 - wsea) + sea_col * wsea

    hs_l, flat = multi_hillshade(z_geom, res, land_exag)
    hs_s, _ = multi_hillshade(z_geom, res, sea_exag)
    shade_l = np.clip(0.82 + 1.15 * (hs_l - flat), 0.48, 1.25)
    shade_s = np.clip(1.0 + sea_shade * 1.15 * (hs_s - flat), 0.75, 1.18)
    shade = (shade_l * (1 - wsea[..., 0]) + shade_s * wsea[..., 0])[..., None]
    return np.clip(col * shade, 0, 255)


def draw_rivers(rgb, px_fn, features, scale=1.0, width=1.6, glow=True):
    """Gambar garis sungai (GeoJSON features) ke RGB uint8 PIL. px_fn(lon,lat)->(x,y) piksel."""
    im = Image.fromarray(np.clip(rgb, 0, 255).astype(np.uint8), "RGB")
    layer = Image.new("RGBA", im.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    for f in features:
        g = f["geometry"]
        parts = [g["coordinates"]] if g["type"] == "LineString" else g["coordinates"]
        for p in parts:
            xs, ys = px_fn(np.array([c[0] for c in p]), np.array([c[1] for c in p]))
            pts = list(zip(xs.tolist(), ys.tolist()))
            if len(pts) >= 2:
                d.line(pts, fill=RIVER + (255,), width=max(1, int(round(width * scale))), joint="curve")
    if glow:
        halo = layer.filter(ImageFilter.GaussianBlur(1.6 * scale))
        halo = Image.merge("RGBA", (*[Image.new("L", im.size, c) for c in RIVER_GLOW], halo.getchannel("A").point(lambda a: int(a * 0.55))))
        im = Image.alpha_composite(im.convert("RGBA"), halo)
    im = Image.alpha_composite(im.convert("RGBA"), layer).convert("RGB")
    return im


def load_rivers(path):
    p = Path(path)
    if not p.exists():
        return []
    return json.loads(p.read_text(encoding="utf8"))["features"]


class Viewer:
    """Pembungkus sesi viewer native forge3D: DEM persegi + overlay tekstur."""

    def __init__(self, scene_tif, overlay_png, width, height, zscale, sun=(300.0, 30.0), fov=38.0):
        from forge3d import viewer_ipc
        from forge3d.viewer import open_viewer_async

        self.v = open_viewer_async(width=width, height=height, terrain_path=str(Path(scene_tif).resolve()),
                                   fov_deg=fov, timeout=180)
        time.sleep(3)
        self.v.load_overlay("tint", str(Path(overlay_png).resolve()), opacity=1.0, z_order=1, preserve_colors=True)
        self.v.set_z_scale(zscale)
        self.v.set_sun(azimuth_deg=sun[0], elevation_deg=sun[1], intensity=3.0)
        try:
            viewer_ipc.set_terrain_pbr(self.v._socket, enabled=True, msaa=4, shadow_technique="none", exposure=1.0)
        except Exception as e:  # noqa: BLE001
            print("set_terrain_pbr dilewati:", e)
        time.sleep(2)
        self.fov = fov

    def shot(self, path, phi, theta, radius, w, h, settle=1.0):
        self.v.set_orbit_camera(phi_deg=phi, theta_deg=theta, radius=radius, fov_deg=self.fov)
        time.sleep(settle)
        self.v.snapshot(str(Path(path).resolve()), width=w, height=h)

    def close(self):
        self.v.close()


def write_scene_tif(z_m, res, path):
    """Tulis DEM skala-scene: tinggi dibagi res agar satuan piksel=1 (rasio vertikal benar)."""
    import rasterio
    from rasterio.transform import from_origin

    h, w = z_m.shape
    with rasterio.open(path, "w", driver="GTiff", height=h, width=w, count=1, dtype="float32",
                       crs="EPSG:32749", transform=from_origin(0, h, 1, 1), compress="deflate") as d:
        d.write((z_m / res).astype(np.float32), 1)
