#!/usr/bin/env python3
"""Make a text-free copy of the city screen's background picture (task 511).

Reads `assets-local/screens/city_screen/background_00.png` (local-only extractor output, `backgrnd`
entry 6) and writes `crates/f4x-client/assets/city/background.png`: the same 320x200 picture with
the headings taken out (Resources, Enchantments, Buildings, Units and Producing), so the client draws them itself from the text keys
`city.heading_*` and they can be localised. Needs Pillow (`.venv`).

A heading is gold lettering (df961c) with a darker edge and shadow on the speckled brown bar. The
script paints over the box of each heading with the bar's own texture from a stretch of the same
rows to the right of the lettering that has none (`SOURCE_SHIFT` columns on), so the speckle looks
as it does elsewhere in the bar. `--check` only reports whether the picture is as written (the box
holds the lettering's colours and the source holds none), for when the extractor output changes.
"""
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets-local/screens/city_screen/background_00.png"
TARGET = ROOT / "crates/f4x-client/assets/city/background.png"

GOLD = (0xDF, 0x96, 0x1C)
# (name, x0, x1, y0, y1, columns to the right where the same rows have no lettering); the box
# is inclusive.
HEADINGS = [
    ("resources", 3, 52, 42, 48, 52),
    # Taken from the clean stretch of the Resources bar, the same rows (the bar is 85 columns
    # to the left).
    ("enchantments", 137, 201, 42, 48, -85),
    # The tail of the g reaches the row of the frame under the bar (101).
    ("buildings", 4, 43, 94, 101, 45),
    ("units", 213, 238, 94, 100, 25),
    # The tail of the g reaches the first row of the panel (146), its shadow the next.
    ("producing", 213, 260, 139, 147, 45),
]


def lettering_pixels(im, x0, x1, y0, y1):
    return sum(1 for y in range(y0, y1 + 1) for x in range(x0, x1 + 1) if im.getpixel((x, y)) == GOLD)


def main():
    check = "--check" in sys.argv
    im = Image.open(SOURCE).convert("RGB")
    if im.size != (320, 200):
        sys.exit(f"{SOURCE}: expected 320x200, got {im.size}")
    out = im.copy()
    for name, x0, x1, y0, y1, shift in HEADINGS:
        if lettering_pixels(im, x0, x1, y0, y1) == 0:
            sys.exit(f"{name}: no gold lettering in the box: the picture is not as written")
        if lettering_pixels(im, x0 + shift, x1 + shift, y0, y1) != 0:
            sys.exit(f"{name}: the source stretch has lettering")
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                out.putpixel((x, y), im.getpixel((x + shift, y)))
        if lettering_pixels(out, x0, x1, y0, y1) != 0:
            sys.exit(f"{name}: lettering left")
        print(f"{name}: covered {x1 - x0 + 1}x{y1 - y0 + 1}")
    if not check:
        TARGET.parent.mkdir(parents=True, exist_ok=True)
        out.save(TARGET)
        print(f"wrote {TARGET.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
