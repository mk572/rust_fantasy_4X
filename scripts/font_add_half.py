#!/usr/bin/env python3
"""Add the glyph ½ (U+00BD, id 189) to the client's font styles 0 to 5.

The atlases in `crates/f4x-client/assets/fonts` hold 280 glyphs on a 48 x 6 grid, so each has free cells
at the end; the new glyph takes the next one. The picture is rough: a system font's ½ drawn without
anti-aliasing at the size where its height matches the style's digit `1`, in the colour of that digit,
copied into every stacked colour set through the colour mapping the atlas already has (as
`font_editor.py` does). Fix it by hand in `font_editor.py` where it looks odd. Idempotent: a style that
has the glyph is left alone. The runic styles 6 and 7 are full and get none.

    .venv/bin/python scripts/font_add_half.py [--font TTF]

Needs Pillow (`.venv`). The client embeds the fonts: rebuild it to see the glyph.
"""

import argparse
import re
import sys
from collections import Counter
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).resolve().parent))
from font_editor import CHAR_FIELDS, Font  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
FONTS = ROOT / "crates/f4x-client/assets/fonts"
DEFAULT_FONT = "/System/Library/Fonts/Supplemental/Arial.ttf"
HALF = 189


def ink_box(font: Font, glyph: dict) -> tuple[int, int, int, int]:
    """Bounding box (left, top, right, bottom) of the glyph's opaque pixels in the first set."""
    px = font.atlas.load()
    pts = [
        (dx, dy)
        for dy in range(glyph["height"])
        for dx in range(glyph["width"])
        if px[glyph["x"] + dx, glyph["y"] + dy][3]
    ]
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    return min(xs), min(ys), max(xs) + 1, max(ys) + 1


def rasterise(ttf: str, height: int) -> Image.Image:
    """The ½ as a 1-bit mask, cropped, at the font size whose ink is closest to `height` rows."""
    best = None
    for size in range(4, 80):
        face = ImageFont.truetype(ttf, size)
        mask = Image.new("L", (size * 2, size * 2), 0)
        ImageDraw.Draw(mask).text((0, 0), "½", font=face, fill=255)
        mask = mask.point(lambda v: 255 if v >= 128 else 0)
        box = mask.getbbox()
        if box is None:
            continue
        gap = abs((box[3] - box[1]) - height)
        if best is None or gap < best[0]:
            best = (gap, mask.crop(box))
    return best[1]


def add(font: Font, ttf: str) -> bool:
    if any(g["id"] == HALF for g in font.glyphs):
        return False
    one = next(g for g in font.glyphs if g["id"] == ord("1"))
    left, top, right, bottom = ink_box(font, one)
    mask = rasterise(ttf, bottom - top)
    width, height = mask.size
    cols = font.atlas.width // font._pitch("x")
    if len(font.glyphs) >= cols * (font.set_height // font._pitch("y")):
        raise SystemExit("no free cell in the glyph grid")
    n = len(font.glyphs)
    gx, gy = (n % cols) * font._pitch("x"), (n // cols) * font._pitch("y")
    if width > font.cell[0] or height > font.cell[1]:
        raise SystemExit(f"{width}x{height} does not fit the {font.cell} cell")
    px = font.atlas.load()
    colour = Counter(
        px[one["x"] + dx, one["y"] + dy]
        for dy in range(one["height"])
        for dx in range(one["width"])
        if px[one["x"] + dx, one["y"] + dy][3]
    ).most_common(1)[0][0]
    maps = font.colour_maps()
    for k in range(font.sets):
        out = maps[k].get(colour, colour)
        for y in range(height):
            for x in range(width):
                if mask.getpixel((x, y)):
                    font.atlas.putpixel((gx + x, gy + y + k * font.set_height), out)
    glyph = dict(
        id=HALF, x=gx, y=gy, width=width, height=height, xoffset=0,
        yoffset=one["yoffset"] + top, xadvance=width + 1, page=0, chnl=15,
    )
    lines = list(font.lines)
    last = max(i for i, line in enumerate(lines) if line.startswith("char "))
    lines.insert(last + 1, " ".join(["char"] + [f"{f}={glyph[f]}" for f in CHAR_FIELDS]))
    lines = [
        re.sub(r"count=\d+", f"count={len(font.glyphs) + 1}", line) if line.startswith("chars ") else line
        for line in lines
    ]
    font.fnt.write_text("\n".join(lines) + "\n", encoding="utf-8")
    font.atlas.save(font.png)
    return True


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--font", default=DEFAULT_FONT, help="TrueType font to rasterise the ½ from")
    args = parser.parse_args()
    for n in range(6):
        done = add(Font(FONTS, f"style{n}"), args.font)
        print(f"style{n}: {'added' if done else 'already there'}")


if __name__ == "__main__":
    main()
