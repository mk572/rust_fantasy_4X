#!/usr/bin/env python3
"""Install the neutral players' pictures into the local art (task 523).

`assets-local/neutral/<race id>/` holds, for each race, `portrait_109x109.png` (the diplomacy
screen), `gem_41x43.png` (lists and the magic screen) and `hall_30x32.png` (the hall of fame),
cropped by hand from the new art. This copies them to `assets-local/art/neutral/NNN_FF.png`, where
NNN is the race's number in `crates/f4x-def/content/races.ron` and FF is 00 (portrait), 01 (gem) or
02 (hall); the client reads them as the pictures of the `neutral` file (`Art::picture`). Running it
again changes nothing. A race with no folder is listed and the exit code is 1.
"""
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RACES = ROOT / "crates/f4x-def/content/races.ron"
SOURCE = ROOT / "assets-local/neutral"
TARGET = ROOT / "assets-local/art/neutral"
PICTURES = ["portrait_109x109.png", "gem_41x43.png", "hall_30x32.png"]


def races():
    """The (number, id) of each race, in the order of races.ron."""
    text = RACES.read_text()
    return [(int(n), i) for n, i in re.findall(r'\(num: (\d+), id: "([a-z_]+)"', text)]


def main():
    TARGET.mkdir(parents=True, exist_ok=True)
    missing = []
    for num, race in races():
        folder = SOURCE / race
        files = [folder / name for name in PICTURES]
        if not all(f.is_file() for f in files):
            missing.append(race)
            continue
        for frame, source in enumerate(files):
            shutil.copyfile(source, TARGET / f"{num:03}_{frame:02}.png")
    if missing:
        print("no pictures for: " + ", ".join(missing), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
