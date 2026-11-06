#!/usr/bin/env python3
"""Append the colour sets of new ramps to the finished font atlases (styles 0 to 5).

`scripts/font_ramps.py` writes the ramps (`crates/f4x-client/content/font_colour_ramps.ron`, numbered by
`crates/f4x-client/src/font_ramp.rs`: set 16 + position); the atlases `crates/f4x-client/assets/fonts/style<n>.png`
stack one copy of the glyphs per set, `scaleH` rows each. This adds the sets the atlas lacks (those
past its height), idempotent. A new set is the glyphs of the `hof_title` set, whose colours 1 to 11 are all
different and so tell which of the glyph's 15 colour indexes each pixel has, painted with the new ramp's colours.
The glyph pixels of indexes 12 to 15 (one colour in `hof_title`) all get index 12's colour.
Usage: scripts/font_ramp_sets.py   Needs Pillow (`.venv`).
"""
import re
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
FONTS = ROOT / "crates/f4x-client/assets/fonts"
RON = ROOT / "crates/f4x-client/content/font_colour_ramps.ron"
SOURCE = "hof_title"


def ramps() -> list[tuple[str, list[tuple[int, int, int]]]]:
    out = []
    for m in re.finditer(r'\(name: "(\w+)", colours: \[(.*?)\]\)', RON.read_text()):
        cols = [tuple(int(v) for v in c.split(",")) for c in re.findall(r"\(([\d, ]+)\)", m.group(2))]
        out.append((m.group(1), cols))
    return out


def main() -> None:
    all_ramps = ramps()
    names = [n for n, _ in all_ramps]
    source = dict(all_ramps)[SOURCE]
    source_at = 16 + names.index(SOURCE)
    index_of = {}
    for i, c in enumerate(source):
        index_of.setdefault(c, i)
    for n in range(6):
        path = FONTS / f"style{n}.png"
        grid = int(re.search(r"scaleH=(\d+)", (FONTS / f"style{n}.fnt").read_text()).group(1))
        atlas = Image.open(path).convert("RGBA")
        have = atlas.height // grid
        want = 16 + len(all_ramps)
        if have >= want:
            print(f"style {n}: {have} sets, nothing to add")
            continue
        out = Image.new("RGBA", (atlas.width, grid * want), (0, 0, 0, 0))
        out.paste(atlas, (0, 0))
        src = atlas.crop((0, source_at * grid, atlas.width, (source_at + 1) * grid))
        for s in range(have, want):
            colours = all_ramps[s - 16][1]
            block = Image.new("RGBA", src.size, (0, 0, 0, 0))
            sp, bp = src.load(), block.load()
            for y in range(src.height):
                for x in range(src.width):
                    r, g, b, a = sp[x, y]
                    if a:
                        bp[x, y] = (*colours[index_of[(r, g, b)]], a)
            out.paste(block, (0, s * grid))
        out.save(path, optimize=True)
        print(f"style {n}: sets {have} to {want - 1} added")


if __name__ == "__main__":
    main()
