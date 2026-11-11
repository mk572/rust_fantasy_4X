#!/usr/bin/env python3
"""Install the neutral players' pictures into the client (task 523).

`assets-local/neutral/<race id>/` holds, for each race, the new art as `original.jpg` and the crops
made from it by hand: `portrait_109x109.png` (the diplomacy screen), `gem_41x43.png` (lists and
the magic screen) and `hall_30x32.png` (the hall of fame). This copies the crops to
`crates/f4x-client/assets/neutral/<race id>/{portrait,gem,hall}.png`, which `neutral_art.rs` embeds
in the client (the pictures are new art, so they ship with the repository). Running it again
changes nothing. A race with no folder is listed and the exit code is 1.
"""
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RACES = ROOT / "crates/f4x-def/content/races.ron"
SOURCE = ROOT / "assets-local/neutral"
TARGET = ROOT / "crates/f4x-client/assets/neutral"
PICTURES = {
    "portrait_109x109.png": "portrait.png",
    "gem_41x43.png": "gem.png",
    "hall_30x32.png": "hall.png",
}


def races():
    """The (number, id) of each race, in the order of races.ron."""
    text = RACES.read_text()
    return [(int(n), i) for n, i in re.findall(r'\(num: (\d+), id: "([a-z_]+)"', text)]


def main():
    missing = []
    for _, race in races():
        folder = SOURCE / race
        if not all((folder / name).is_file() for name in PICTURES):
            missing.append(race)
            continue
        (TARGET / race).mkdir(parents=True, exist_ok=True)
        for name, installed in PICTURES.items():
            shutil.copyfile(folder / name, TARGET / race / installed)
    if missing:
        print("no pictures for: " + ", ".join(missing), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
