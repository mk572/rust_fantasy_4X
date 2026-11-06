#!/usr/bin/env python3
"""Make text-free copies of the original's menu-bar buttons (the marbled gold ones with a label).

Reads `assets-local/screens/main_screen/button_<name>_00.png` (local-only extractor output)
and writes `crates/f4x-client/assets/button/<name>.png`: the same picture with the label
taken out, so the client draws the label itself with the font engine (and can localise it).

The label is style 4 in the font's colour set 1 (ink (8,4,4)) with the original's "shadow down":
copies of the ink one pixel right, down and diagonally, in a lighter gold. The script finds the ink
by its colour takes the three shifted copies of it too, and fills every pixel it took with the
marbling of the same row (the nearest pixel left or right that is not part of the label), because
the marble streaks run sideways. Needs Pillow (`.venv`).

The Next Turn button (67x16, gold lettering with a dark shadow on grey stone) is cleaned by colour:
the stone is grey and the lettering saturated.

The four action buttons of the unit window (Done, Patrol, Wait and Build, Meld or Purify) are
33x9 and lettered in style 0 (5 px), ink (8,4,4), at y=2, with the "shadow down" copies in a lighter gold. Only the rows of the lettering
count as ink, since the Wait and Build pictures have a dark speck below it. Only the up picture is
made: the original's pressed picture (frame 1) is the up one a pixel right and down under a dark
top row and left column, which the client draws itself. The locked pictures (`button_<name>_locked_00.png`, a dimmed copy with
the label in (121,85,36)) are cleaned at the places where the up picture has its ink.
"""
import argparse
from pathlib import Path

from PIL import Image

INK = (8, 4, 4)
NAMES = ["game", "spells", "armies", "cities", "magic", "info", "plane"]
ACTIONS = ["done", "patrol", "wait", "build", "meld", "purify"]
LOCKED = ["done", "patrol", "wait", "build"]


def ink_of(img: Image.Image, rows: range = None) -> set[tuple[int, int]]:
    """The pixels of the label's ink: the ink colour, not on the first row or column."""
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()
    return {
        (x, y)
        for y in (rows or range(1, h))
        for x in range(1, w if rows is None else w - 2)
        if px[x, y][:3] == INK
    }


def clean(
    img: Image.Image, ink: set[tuple[int, int]], shadow: bool = True
) -> tuple[Image.Image, int]:
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()
    taken = set(ink)
    for x, y in ink if shadow else ():
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


def clean_next_turn(img: Image.Image) -> tuple[Image.Image, int]:
    """The Next Turn button: gold lettering with a dark brown shadow on grey stone. The stone is
    grey (little colour), the lettering and its shadow are saturated, so those pixels are the
    label; they are filled from the stone of the same row."""
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()
    taken = {
        (x, y)
        for y in range(h)
        for x in range(w)
        if max(px[x, y][:3]) - min(px[x, y][:3]) > 30
    }
    # the shadow's darkest edge is nearly grey: dark pixels touching the label belong to it
    for x, y in list(taken):
        for dx, dy in ((1, 0), (0, 1), (1, 1)):
            q = (x + dx, y + dy)
            if 0 <= q[0] < w and 0 <= q[1] < h and max(px[q][:3]) < 110:
                taken.add(q)
    out = img.copy()
    po = out.load()
    for x, y in sorted(taken):
        for k in range(1, w):
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
        img = Image.open(Path(args.src) / f"button_{name}_00.png")
        img, n = clean(img, ink_of(img))
        img.save(out_dir / f"{name}.png")
        print(f"{name}: {n} label pixels replaced")
    img, n = clean_next_turn(Image.open(Path(args.src) / "button_next_turn_00.png"))
    img.save(out_dir / "next_turn.png")
    print(f"next_turn: {n} label pixels replaced")
    for name in ACTIONS:
        img = Image.open(Path(args.src) / f"button_{name}_00.png")
        ink = ink_of(img, range(2, 7))
        img, n = clean(img, ink)
        img.save(out_dir / f"{name}.png")
        print(f"{name}: {n} label pixels replaced")
        if name in LOCKED:
            img = Image.open(Path(args.src) / f"button_{name}_locked_00.png")
            img, n = clean(img, ink)
            img.save(out_dir / f"{name}_locked.png")
            print(f"{name} locked: {n} label pixels replaced")


if __name__ == "__main__":
    main()
