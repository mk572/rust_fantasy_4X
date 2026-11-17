#!/usr/bin/env python3
"""Task 570: make the client's pack of gem frames, `crates/f4x-client/assets/gem/frame.sheet.png`: the
original's five diamonds (blue, green, purple, red, yellow; 41 x 43, with no portrait in them) from
`magic` 1 to 5, as one sprite sheet in the format of `f4x_extract::pack` (a plain PNG on a grid; the
`f4x-sheet` text chunk names each picture). The five newer colours are made from these at run time
(`crates/f4x-client/src/gem.rs`). Run it again only when the frames change; the sources are the
originals' pictures that `f4x-extract` files in `assets-unused/art/magic/` (the sheet in the
repository is the original's art, kept there at the project owner's choice, 17 November 2026:
`docs/local-assets.md`).

    .venv/bin/python scripts/make_gem_frames.py
Needs Pillow (.venv).
"""
from pathlib import Path

import argparse

# Parsed before the heavy imports below, so `--help` and a wrong option never run the script.
if __name__ == "__main__":
    _parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for _name, _kw in ():
        _parser.add_argument(_name, **_kw)
    _parser.parse_args()

from PIL import Image
from PIL.PngImagePlugin import PngInfo

ROOT = Path(__file__).resolve().parent.parent
SOURCES = ROOT / "assets-unused" / "art" / "magic"
TARGET = ROOT / "crates" / "f4x-client" / "assets" / "gem" / "frame.sheet.png"
FRAMES = ["blue", "green", "purple", "red", "yellow"]
WIDTH, HEIGHT = 41, 43


def main():
    sheet = Image.new("RGBA", (WIDTH * len(FRAMES), HEIGHT), (0, 0, 0, 0))
    lines = [f"cell {WIDTH}x{HEIGHT}"]
    for i, name in enumerate(FRAMES):
        picture = Image.open(SOURCES / f"{i + 1:03d}_00.png").convert("RGBA")
        assert picture.size == (WIDTH, HEIGHT), picture.size
        sheet.paste(picture, (i * WIDTH, 0))
        lines.append(f"{name}.png\t{WIDTH}\t{HEIGHT}")
    info = PngInfo()
    info.add_text("f4x-sheet", "\n".join(lines))
    TARGET.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(TARGET, pnginfo=info, optimize=True)
    print(f"wrote {TARGET} ({TARGET.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
