#!/usr/bin/env python3
"""Darken the white glass of the notify boxes' picture frames in the packed art (task 511).

`resource.lbx` 41, 42, 44, 45, 47 and 48 (the plain and gem frames of the green, red and brown
event boxes) are outlined with glass blocks. Extracted as white tints they show as light grey
speckle over the box's black fill; they darken what is under them (as the selection box's frame
does, `to_rgba_stone`), so every translucent white pixel of the frames becomes black with the same
alpha. `f4x-extract art` now writes them that way (`to_rgba_notify_frame`); this does the same to
art already extracted, in `assets-local/art/resource.sheet4.png` only (no full unpack and pack).
It is safe to run again. Needs Pillow (`.venv`).
"""
from pathlib import Path

from PIL import Image, PngImagePlugin

SHEET = Path(__file__).resolve().parent.parent / "assets-local/art/resource.sheet4.png"


def main():
    src = Image.open(SHEET)
    # The sheet carries its picture list in a tEXt chunk (`f4x-sheet`); keep it.
    info = PngImagePlugin.PngInfo()
    for key, value in src.text.items():
        info.add_text(key, value)
    im = src.convert("RGBA")
    px = im.load()
    n = 0
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = px[x, y]
            if 0 < a < 255 and (r, g, b) == (255, 255, 255):
                px[x, y] = (0, 0, 0, a)
                n += 1
    im.save(SHEET, pnginfo=info)
    print(f"{n} pixels darkened in {SHEET.name}")


if __name__ == "__main__":
    main()
