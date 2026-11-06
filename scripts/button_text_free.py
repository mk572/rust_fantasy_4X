#!/usr/bin/env python3
"""Make text-free copies of the original's menu-bar buttons (the marbled gold ones with a label).

Reads `assets-local/screens/main_screen/button_<name>_0<frame>.png` (local-only extractor output)
and writes `crates/f4x-client/assets/button/<name>_<frame>.png`: the same picture with the label
taken out, so the client draws the label itself with the font engine (and can localise it).

The label is style 4 in the font's colour set 1 (ink (8,4,4)) with the original's "shadow down":
copies of the ink one pixel right, down and diagonally, in a lighter gold. The script finds the ink
by its colour (not the pressed frame's dark outline on the top row and left column), takes the three shifted copies of it too, and fills every pixel it took with the
marbling of the same row (the nearest pixel left or right that is not part of the label), because
the marble streaks run sideways. Needs Pillow (`.venv`).
"""
import argparse
from pathlib import Path

from PIL import Image

INK = (8, 4, 4)
NAMES = ["game", "spells", "armies", "cities", "magic", "info", "plane"]


def clean(img: Image.Image) -> tuple[Image.Image, int]:
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()
    ink = {(x, y) for y in range(1, h) for x in range(1, w) if px[x, y][:3] == INK}
    taken = set(ink)
    for x, y in ink:
        for dx, dy in ((1, 0), (0, 1), (1, 1)):
            taken.add((x + dx, y + dy))
    taken = {p for p in taken if 0 <= p[0] < w and 0 <= p[1] < h}
    out = img.copy()
    po = out.load()
    for x, y in sorted(taken):
        for k in range(1, w):
            # alternate the side so neither edge of a word is always the source
            sides = (x + k, x - k) if (x + y) % 2 else (x - k, x + k)
            found = next(
                (px[s, y] for s in sides if 0 <= s < w and (s, y) not in taken), None
            )
            if found:
                po[x, y] = found
                break
    return out, len(taken)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--src", default="assets-local/screens/main_screen")
    ap.add_argument("--out", default="crates/f4x-client/assets/button")
    args = ap.parse_args()
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    for name in NAMES:
        for frame in (0, 1):
            src = Path(args.src) / f"button_{name}_0{frame}.png"
            img, n = clean(Image.open(src))
            img.save(out_dir / f"{name}_{frame}.png")
            print(f"{name} {frame}: {n} label pixels replaced")


if __name__ == "__main__":
    main()
