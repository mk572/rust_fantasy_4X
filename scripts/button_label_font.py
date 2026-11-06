#!/usr/bin/env python3
"""Make the fonts of two buttons' labels.

`label.{fnt,png}`: the font of the Build button's label: style 0 with the 3 px wide serif "I" the original's
BUILD picture is lettered with (the font's own "I" is a single pixel column; PURIFY uses that one).

Reads `crates/f4x-client/assets/fonts/style0.{fnt,png}` and writes
`crates/f4x-client/assets/button/label.{fnt,png}`: the atlas keeps only colour sets 0 (white, for
flat inks) and 1 (the ink (8,4,4) of the buttons), each 66 px tall, with the serif I at x=380, and
the .fnt points "I" at it (advance 4).

`wood.{fnt,png}`: the font of the city screen's wooden buttons (BUY, CHANGE, OK): style 1 with the original's own
letters for BUY, CHANGE and OK, cut from `assets-local/screens/city_screen`. Needs Pillow (`.venv`).
"""
import re
from pathlib import Path

from PIL import Image

FONTS = Path("crates/f4x-client/assets/fonts")
OUT = Path("crates/f4x-client/assets/button")
SET_H = 66
SERIF_I = ["###", ".#.", ".#.", ".#.", "###"]
X = 380


# The wooden buttons' lettering is style 1 with hand-set letters: the Y has a tail and the A and G
# of CHANGE have a darker top left pixel. The letters are cut from the original's pictures (4 by 5
# pixels, 5 apart from the label's first letter at the x given) and replace the font's own.
WOOD = [("buy", "BUY", 8), ("change", "CHANGE", 3), ("ok", "OK", 9)]
LAYOUT_W = 512


def make_wood(src: Path) -> None:
    from PIL import Image as I

    atlas = I.open(FONTS / "style1.png").convert("RGBA").crop((0, 0, 384, SET_H))
    out = I.new("RGBA", (LAYOUT_W, SET_H), (0, 0, 0, 0))
    out.paste(atlas, (0, 0))
    fnt = (FONTS / "style1.fnt").read_text()
    x = 384
    done = {}
    for name, label, left in WOOD:
        pic = I.open(src / "city_screen" / f"button_{name}_00.png").convert("RGB")
        for i, ch in enumerate(label):
            if ch in done:
                continue
            for j in range(5):
                for k in range(4):
                    p = pic.getpixel((left + 5 * i + k, 2 + j))
                    if p[2] > p[0] + 5:
                        # the lettering is the lavender grey (178,178,195) or a darker shade of
                        # it at the rounded corners; the font tints by the label's ink, so each
                        # channel is stored as its share of the ink's
                        r, g, b = (min(255, round(255 * c / t)) for c, t in zip(p, (178, 178, 195)))
                        out.putpixel((x + k, j), (r, g, b, 255))
            if ch == "A":
                # not in the original: the A's top right pixel is as dark as its top left one
                out.putpixel((x + 3, 0), out.getpixel((x, 0)))
            done[ch] = x
            x += 4
    for ch, gx in done.items():
        fnt, n = re.subn(
            rf"char id={ord(ch)} .*",
            f"char id={ord(ch)} x={gx} y=0 width=4 height=6 xoffset=0 yoffset=0 xadvance=5 page=0 chnl=15",
            fnt,
        )
        assert n == 1, ch
    fnt = fnt.replace("scaleW=384", f"scaleW={LAYOUT_W}").replace(
        'file="style1.png"', 'file="wood.png"'
    )
    assert x <= LAYOUT_W
    out.save(OUT / "wood.png")
    (OUT / "wood.fnt").write_text(fnt)


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
    make_wood(Path("assets-local/screens"))


if __name__ == "__main__":
    main()
