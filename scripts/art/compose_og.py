#!/usr/bin/env python3
"""Susun og-flood.jpg (1200x630) dari render hero + wordmark.

  python scripts/art/compose_og.py --hero <hero_raw.png> --out <og-flood.jpg> --font-cache <DIR>

Font: SchibstedGrotesk-wght.woff2 (variabel) -> TTF via fontTools, lalu diinstansiasi pada wght 700 (wordmark) dan 450 (subjudul).
Teks dalam margin aman 64 px (kartu sosial memotong tepi). Kualitas JPEG diturunkan hingga <= 250 KB.
"""
from __future__ import annotations

import argparse
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from PIL import Image, ImageDraw, ImageFont

REPO = Path(__file__).resolve().parents[2]
WOFF2 = REPO / "crates/flood-web/assets/fonts/SchibstedGrotesk-wght.woff2"
PAPER = (245, 243, 238)
INK = (14, 28, 36)
RIVER = (47, 127, 193)
W, H = 1200, 630
MARGIN = 64
SHIFT = 300  # geser peta ke kanan agar Sumatra tidak tertutup blok teks


def instance(cache: Path, wght: int) -> Path:
    out = cache / f"SchibstedGrotesk-{wght}.ttf"
    if not out.exists():
        f = TTFont(str(WOFF2))
        f.flavor = None
        inst = instancer.instantiateVariableFont(f, {"wght": wght})
        inst.save(str(out))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--hero", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--font-cache", required=True)
    ap.add_argument("--title", default="Hidrosentinel Nusantara")
    ap.add_argument("--subtitle", default="Indikasi banjir sungai, hujan & rob")
    ap.add_argument("--max-kb", type=int, default=250)
    a = ap.parse_args()
    cache = Path(a.font_cache)
    cache.mkdir(parents=True, exist_ok=True)

    hero = Image.open(a.hero).convert("RGB")
    s = W / hero.width
    hero = hero.resize((W, round(hero.height * s)), Image.LANCZOS)
    top = max(0, (hero.height - H) // 2)
    crop = hero.crop((0, top, W, top + H))
    base = Image.new("RGB", (W, H), PAPER)
    base.paste(crop.crop((0, 0, W - SHIFT, H)), (SHIFT, 0))

    # gradasi kertas dari kiri agar teks terbaca, peta tetap terlihat di kanan
    px = base.load()
    for x in range(W):
        t = min(1.0, max(0.0, 1.0 - (x - SHIFT) / (W * 0.42))) ** 1.3
        for y in range(H):
            r, g, b = px[x, y]
            px[x, y] = (round(r + (PAPER[0] - r) * t), round(g + (PAPER[1] - g) * t), round(b + (PAPER[2] - b) * t))

    d = ImageDraw.Draw(base)
    f_title = ImageFont.truetype(str(instance(cache, 700)), 72)
    f_sub = ImageFont.truetype(str(instance(cache, 450)), 34)
    words = a.title.split(" ")
    lines = [" ".join(words[:-1]), words[-1]] if len(words) > 1 else [a.title]
    y = H // 2 - 100
    d.rectangle((MARGIN, y - 28, MARGIN + 96, y - 22), fill=RIVER)
    for ln in lines:
        d.text((MARGIN, y), ln, font=f_title, fill=INK)
        y += 84
    d.text((MARGIN, y + 20), a.subtitle, font=f_sub, fill=INK)

    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    for q in range(92, 40, -3):
        base.save(out, "JPEG", quality=q, optimize=True, progressive=True)
        if out.stat().st_size <= a.max_kb * 1024:
            break
    print(f"og q={q} {out.stat().st_size / 1024:.0f} KB {base.size}")


if __name__ == "__main__":
    main()
