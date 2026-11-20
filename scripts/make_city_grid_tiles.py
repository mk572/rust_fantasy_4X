#!/usr/bin/env python3
"""Task 732: make the client's pack of the city grid's tiles, `crates/f4x-client/assets/combat/city_grid.sheet.png`.

The city's base picture of a battle (`cmbtcity` 0, 126 x 64) is a road lattice over 4 x 4 squares. This
cuts it into its 16 squares, each one 32 x 17 pixels (a 32 x 16 diamond whose middle row is row 8; the
pixels outside the diamond are transparent), as one sprite sheet in the format of `f4x_extract::pack`
(a plain PNG on a grid; the `f4x-sheet` text chunk names each picture). The tile `grid_<i>_<j>` is the
square in column `i` (towards the south-east) and row `j` (towards the south-west) of the original
picture. The block's middle is the edge between pixel columns 63 and 64 at the middle of pixel row 31:
there the roads are exactly 8 pixels wide and centred on the squares' edges (`docs/combat.md`,
`crates/f4x-client/src/battle_city_base.rs`).

The source is the original's picture that `f4x-extract` files in `assets-local/screens/combat/city/`
(the sheet in the repository is the original's art, kept there at the project owner's choice, 20 November
2026: `docs/local-assets.md`). Run it again only when the tiles change.

    .venv/bin/python scripts/make_city_grid_tiles.py
Needs Pillow (.venv).
"""
from pathlib import Path

import argparse

# Parsed before the heavy imports below, so `--help` and a wrong option never run the script.
if __name__ == "__main__":
    _parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    _parser.parse_args()

from PIL import Image
from PIL.PngImagePlugin import PngInfo

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets-local" / "screens" / "combat" / "city" / "cmbtcity_000_00.png"
TARGET = ROOT / "crates" / "f4x-client" / "assets" / "combat" / "city_grid.sheet.png"
SIDE = 4
PICTURE = (126, 64)
CENTRE_X, CENTRE_ROW = 64, 31
TILE_W, TILE_H = 32, 17
HALF_W, HALF_H = 16, 8


def square_at(x, y):
    """The square (column, row) of a source pixel, or None outside the block."""
    dx, dy = 2 * x + 1 - 2 * CENTRE_X, 2 * (y - CENTRE_ROW)
    u, v = dx + 2 * dy + 128, 2 * dy - dx + 128
    i, j = u // 64, v // 64
    return (i, j) if u >= 0 and v >= 0 and i < SIDE and j < SIDE else None


def centre_of(i, j):
    return CENTRE_X + (i - j) * HALF_W, CENTRE_ROW + (i + j - SIDE + 1) * HALF_H


def main():
    picture = Image.open(SOURCE).convert("RGBA")
    assert picture.size == PICTURE, picture.size
    tiles = [(i, j) for j in range(SIDE) for i in range(SIDE)]
    sheet = Image.new("RGBA", (TILE_W * SIDE, TILE_H * SIDE), (0, 0, 0, 0))
    lines = [f"cell {TILE_W}x{TILE_H}"]
    cut = 0
    for n, (i, j) in enumerate(tiles):
        cx, cy = centre_of(i, j)
        tile = Image.new("RGBA", (TILE_W, TILE_H), (0, 0, 0, 0))
        for ty in range(TILE_H):
            for tx in range(TILE_W):
                x, y = cx - HALF_W + tx, cy - HALF_H + ty
                if 0 <= x < PICTURE[0] and 0 <= y < PICTURE[1] and square_at(x, y) == (i, j):
                    pixel = picture.getpixel((x, y))
                    tile.putpixel((tx, ty), pixel)
                    cut += pixel[3] > 0
        sheet.paste(tile, ((n % SIDE) * TILE_W, (n // SIDE) * TILE_H))
        lines.append(f"grid_{i}_{j}.png\t{TILE_W}\t{TILE_H}")
    total = sum(1 for value in picture.getchannel("A").tobytes() if value > 0)
    assert cut == total, f"{total - cut} road pixels are outside the 16 squares"
    info = PngInfo()
    info.add_text("f4x-sheet", "\n".join(lines))
    TARGET.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(TARGET, pnginfo=info, optimize=True)
    print(f"wrote {TARGET} ({TARGET.stat().st_size} bytes, {cut} road pixels in 16 tiles)")


if __name__ == "__main__":
    main()
