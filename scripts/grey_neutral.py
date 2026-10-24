#!/usr/bin/env python3
"""Turn the neutral player's colour from brown to grey in the local art.

The neutral banner is the sixth of `map/banners.tsv`; the pictures that come already coloured for
it are the cities and towers on the map (`map/locations/*_neutral_*.png`), the unit backgrounds
(`map/overlays/unit_backgrounds/unit_background_neutral_*.png`) and the neutral colour of the
minimap (`map/tiles/minimap.tsv`). Only the banner's own pixels change (those that differ from the
blue version of the same picture), by luminance, so the shading stays. Running it again changes
nothing. Reads and writes assets-local/art; the output is derived from the original's art, so it
stays local. Needs Pillow (.venv).
"""
import argparse
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent


def grey_of(r, g, b):
    y = round(0.299 * r + 0.587 * g + 0.114 * b)
    return y, y, y


def grey_pixels(neutral, blue):
    """`neutral` with the pixels that differ from `blue` (all, if there is none) made grey."""
    out = neutral.convert("RGBA")
    px = out.load()
    other = blue.convert("RGBA").load() if blue is not None else None
    changed = 0
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, a = px[x, y]
            if not a:
                continue
            if other is not None and other[x, y][:3] == (r, g, b):
                continue
            new = grey_of(r, g, b)
            if new != (r, g, b):
                px[x, y] = (*new, a)
                changed += 1
    return out, changed


def tsv_lines(path, grey_tag):
    """The lines of a table with the neutral colour rows made grey; returns (text, changed)."""
    lines, changed = [], 0
    for line in path.read_text().splitlines():
        fields = line.split("\t")
        if grey_tag(fields):
            new = fields[:]
            if new[0] == "banner" or new[0] == "neutral":
                rgb = tuple(int(v) for v in new[-3:])
                new[-3:] = [str(v) for v in grey_of(*rgb)]
            else:
                shades = []
                for c in new[2:]:
                    shades.append(",".join(str(v) for v in grey_of(*[int(n) for n in c.split(",")])))
                new[2:] = shades
            if new != fields:
                changed += 1
            line = "\t".join(new)
        lines.append(line)
    return "\n".join(lines) + "\n", changed


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--art", type=Path, default=ROOT / "assets-local/art", help="the art folder")
    args = ap.parse_args()
    art = args.art
    if not (art / "map/banners.tsv").exists():
        print(f"{art}/map/banners.tsv is missing (run extract-art first)", file=sys.stderr)
        return 1
    total = 0
    for path in sorted((art / "map/locations").glob("*_neutral_*.png")):
        blue = path.with_name(path.name.replace("_neutral_", "_blue_"))
        grey, n = grey_pixels(Image.open(path), Image.open(blue) if blue.exists() else None)
        if n:
            grey.save(path)
            total += 1
    for path in sorted((art / "map/overlays/unit_backgrounds").glob("*_neutral_*.png")):
        grey, n = grey_pixels(Image.open(path), None)
        if n:
            grey.save(path)
            total += 1
    text, n = tsv_lines(art / "map/banners.tsv", lambda f: len(f) >= 3 and f[0] == "5")
    (art / "map/banners.tsv").write_text(text)
    total += n
    minimap = art / "map/tiles/minimap.tsv"
    if minimap.exists():
        text, n = tsv_lines(
            minimap, lambda f: (f[0] == "banner" and f[1] == "5") or f[0] == "neutral"
        )
        minimap.write_text(text)
        total += n
    print(f"{total} files or rows changed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
