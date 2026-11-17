#!/usr/bin/env python3
"""Give the notify boxes' picture frames the marble of their strips in the packed art (task 511).

`resource.lbx` 41, 42, 44, 45, 47 and 48 (the plain and gem frames of the green, red and blue-grey
event boxes) are outlined with glass blocks. The original draws that outline as the marble band of
the strip around the picture (yellow and blue like the strip's left end, for the green box), not
as translucent tints. Every translucent pixel of the frames becomes an opaque step of the strip's
ramp (the same table as `NOTIFY_GREEN`, `NOTIFY_RED` and `NOTIFY_GREY` in
`crates/f4x-client/extract/src/image.rs`): dark for black blocks, light for white ones by their
alpha, the lightest for the blue ones (the colour slide). `f4x-extract art` writes them this way
(`to_rgba_notify_frame`); this does the same to art already extracted, in
`assets-local/art/resource.sheet4.png` only (no full unpack and pack), keeping the sheet's picture
list. Safe to run again: it only touches translucent pixels. Needs Pillow (`.venv`).
"""
from pathlib import Path

from PIL import Image, PngImagePlugin

SHEET = Path(__file__).resolve().parent.parent / "assets-local/art/resource.sheet4.png"

GREEN = [(20, 48, 56), (28, 60, 69), (36, 69, 77), (44, 81, 89), (56, 97, 105), (65, 113, 121), (73, 117, 125), (77, 125, 134), (81, 134, 142)]
RED = [(36, 0, 4), (60, 0, 4), (77, 0, 0), (105, 0, 0), (130, 0, 0), (146, 0, 0), (170, 0, 0), (186, 0, 0), (199, 0, 0)]
GREY = [(44, 44, 73), (52, 52, 81), (56, 56, 85), (60, 60, 89), (69, 69, 97), (77, 77, 105), (81, 81, 109), (89, 89, 117), (93, 93, 121)]
RAMPS = {41: GREEN, 42: GREEN, 44: RED, 45: RED, 47: GREY, 48: GREY}


def main():
    src = Image.open(SHEET)
    # The sheet carries its picture list in a tEXt chunk (`f4x-sheet`): "cell WxH" then one
    # "NNN_FF.png<TAB>w<TAB>h" line per picture, in cell order, left to right and top to bottom.
    info = PngImagePlugin.PngInfo()
    for key, value in src.text.items():
        info.add_text(key, value)
    lines = src.text["f4x-sheet"].splitlines()
    cw, ch = map(int, lines[0].split()[1].split("x"))
    entries = [int(l.split("_")[0]) for l in lines[1:]]
    im = src.convert("RGBA")
    px = im.load()
    per_row = im.width // cw
    n = 0
    for k, entry in enumerate(entries):
        ramp = RAMPS.get(entry)
        if ramp is None:
            continue
        x0, y0 = (k % per_row) * cw, (k // per_row) * ch
        for y in range(y0, y0 + ch):
            for x in range(x0, x0 + cw):
                r, g, b, a = px[x, y]
                if not 0 < a < 255:
                    continue
                alpha = a / 255
                t = 0.55 + 0.4 * alpha if (r, g, b) == (255, 255, 255) else 0.45 - 0.4 * alpha if (r, g, b) == (0, 0, 0) else 1.0
                px[x, y] = (*ramp[min(8, max(0, round(t * 8)))], 255)
                n += 1
    im.save(SHEET, pnginfo=info)
    print(f"{n} pixels changed in {SHEET.name}")


if __name__ == "__main__":
    main()
