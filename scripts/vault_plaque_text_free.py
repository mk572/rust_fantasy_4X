#!/usr/bin/env python3
"""Make the item screen's picture without its "Fortress Vault" plaque title (task 511).

Reads `assets-local/screens/item_screen/hero_background_00.png` (local-only extractor output,
`armylist` entry 5, 285x197) and writes, to `crates/f4x-client/assets/item/`:

* `hero_background.png`: the same picture with the lettering taken out. The lettering is lavender
  (five colours the stone does not have) with an emboss: a grey light edge up and to the left and
  a black one down and to the right. Every such pixel is replaced by the stone of a clean stretch
  of the same rows to the right of the word, repeated as far as it is needed (the stone is fine
  grained, so the seam does not show).
* `sheen_vault.png`: the sheen of the lettering, as for the stone buttons (`button_text_free.py`):
  its tone depends only on x - y, so the light and dark bands run diagonally. The picture is the
  box of the lettering (`BOX`), a colour for every pixel from the nearest diagonal that has
  lettering on it; the client keeps the pixels of its own lettering and takes their colours from it.

Needs Pillow (`.venv`). `--check` only verifies the picture is as written.
"""
import sys
from collections import Counter
from pathlib import Path

import argparse

# Parsed before the heavy imports below, so `--help` and a wrong option never run the script.
if __name__ == "__main__":
    _parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for _name, _kw in (
    ("--check", dict(action="store_true", help="only report whether the picture is as written; change nothing")),
    ):
        _parser.add_argument(_name, **_kw)
    _parser.parse_args()

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets-local/screens/item_screen/hero_background_00.png"
TARGET = ROOT / "crates/f4x-client/assets/item"

LAVENDER = {
    (0x9E, 0xB2, 0xCF),
    (0xAA, 0xBA, 0xCF),
    (0xBA, 0xC3, 0xD7),
    (0x8A, 0xA6, 0xCF),
    (0x96, 0xAE, 0xCF),
}
# The emboss: colours the stone has no use for, and two it has a little of (only taken next to the
# lettering).
EMBOSS = {(0x8E, 0x86, 0x82), (0x18, 0x10, 0x0C), (0x08, 0x04, 0x04), (0x34, 0x28, 0x1C), (0x9E, 0x96, 0x92)}
SOMETIMES = {(0x7D, 0x75, 0x71), (0x69, 0x61, 0x5D)}
# The box of the lettering and its emboss: x0, y0, width, height (the sheen's size).
BOX = (49, 154, 90, 16)
# Clean stone to the right of the word on the same rows: first column and how many.
CLEAN = (140, 74)


def main():
    check = "--check" in sys.argv
    im = Image.open(SOURCE).convert("RGB")
    if im.size != (285, 197):
        sys.exit(f"{SOURCE}: expected 285x197, got {im.size}")
    x0, y0, w, h = BOX
    inside = [(x, y) for y in range(y0, y0 + h) for x in range(x0, x0 + w)]
    ink = {p for p in inside if im.getpixel(p) in LAVENDER}
    if len(ink) < 200:
        sys.exit(f"only {len(ink)} lavender pixels in the box: the picture is not as written")
    near = {(x + dx, y + dy) for x, y in ink for dx in (-1, 0, 1) for dy in (-1, 0, 1)}
    gone = {
        p
        for p in inside
        if p in ink or im.getpixel(p) in EMBOSS or (p in near and im.getpixel(p) in SOMETIMES)
    }
    cx, cn = CLEAN
    out = im.copy()
    for x, y in gone:
        out.putpixel((x, y), im.getpixel((cx + (x - x0) % cn, y)))
    left = [p for p in inside if out.getpixel(p) in LAVENDER | EMBOSS]
    if left:
        sys.exit(f"lettering left at {left[:5]}")
    # The sheen: the commonest lettering colour on each diagonal.
    by_diagonal = {}
    for x, y in ink:
        by_diagonal.setdefault((x - x0) - (y - y0), Counter())[im.getpixel((x, y))] += 1
    known = sorted(by_diagonal)
    sheen = Image.new("RGBA", (w, h))
    for y in range(h):
        for x in range(w):
            u = min(known, key=lambda k: (abs(k - (x - y)), k))
            sheen.putpixel((x, y), (*by_diagonal[u].most_common(1)[0][0], 255))
    print(f"lettering {len(ink)} pixels, {len(gone)} replaced")
    if not check:
        TARGET.mkdir(parents=True, exist_ok=True)
        out.save(TARGET / "hero_background.png")
        sheen.save(TARGET / "sheen_vault.png")
        print(f"wrote {TARGET.relative_to(ROOT)}/hero_background.png and sheen_vault.png")


if __name__ == "__main__":
    main()
