#!/usr/bin/env python3
"""Make the font of the Build button's label: style 0 with the 3 px wide serif "I" the original's
BUILD picture is lettered with (the font's own "I" is a single pixel column; PURIFY uses that one).

Reads `crates/f4x-client/assets/fonts/style0.{fnt,png}` and writes
`crates/f4x-client/assets/button/label.{fnt,png}`: the atlas keeps only colour sets 0 (white, for
flat inks) and 1 (the ink (8,4,4) of the buttons), each 66 px tall, with the serif I at x=380, and
the .fnt points "I" at it (advance 4). Needs Pillow (`.venv`).
"""
import re
from pathlib import Path

from PIL import Image

FONTS = Path("crates/f4x-client/assets/fonts")
OUT = Path("crates/f4x-client/assets/button")
SET_H = 66
SERIF_I = ["###", ".#.", ".#.", ".#.", "###"]
X = 380


def main() -> None:
    atlas = Image.open(FONTS / "style0.png").convert("RGBA")
    out = atlas.crop((0, 0, atlas.width, 2 * SET_H))
    for set_no in (0, 1):
        ink = next(
            atlas.getpixel((x, y))
            for y in range(set_no * SET_H, (set_no + 1) * SET_H)
            for x in range(atlas.width)
            if atlas.getpixel((x, y))[3]
        )
        for j, row in enumerate(SERIF_I):
            for i, c in enumerate(row):
                if c == "#":
                    out.putpixel((X + i, set_no * SET_H + j), ink)
    out.save(OUT / "label.png")
    fnt = (FONTS / "style0.fnt").read_text()
    fnt, n = re.subn(
        r"char id=73 .*",
        f"char id=73 x={X} y=0 width=3 height=6 xoffset=0 yoffset=0 xadvance=4 page=0 chnl=15",
        fnt,
    )
    assert n == 1
    fnt = fnt.replace('file="style0.png"', 'file="label.png"')
    (OUT / "label.fnt").write_text(fnt)


if __name__ == "__main__":
    main()
