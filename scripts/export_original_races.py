#!/usr/bin/env python3
"""Write the original's race table (ReMoM `_race_type_table`, 14 rows) as a TSV the tests read.

One row per race in the original's order (the order of `content/races.ron`): name, the buildings the
race cannot build (record numbers of `BUILDDAT.LBX`, space separated), the outpost growth rate,
the growth modifier (tens of percent) and the house type. `crates/f4x-data/tests/it/original_content.rs`
compares every race of the content with it. Needs the ReMoM clone; the output is numbers, so it is committed.

    python3 -I scripts/export_original_races.py [--remom DIR] [--out FILE]
"""
import argparse
import re
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_REMOM = Path.home() / "projects/ReMoM"
DEFAULT_OUT = REPO / "crates/f4x-data/tests/original/races.tsv"
RACES = 14


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--remom", type=Path, default=DEFAULT_REMOM)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()
    src = (args.remom / "MoM/src/RACETYPE.c").read_text(encoding="utf-8", errors="replace")
    names = dict(re.findall(r'race_type_name_(\d+)\[\] = "([^"]*)"', src))
    # Each table row is preceded by its raw bytes in a comment: 12 little-endian words.
    comments = re.findall(r"/\*\s+((?:[0-9A-F]{2}\s+){23}[0-9A-F]{2})\s+\*/", src)
    if len(comments) != RACES:
        print(f"expected {RACES} races, found {len(comments)}", file=sys.stderr)
        return 1
    lines = ["index\tname\tcant_build\toutpost_growth\tgrowth_mod\thouse_type"]
    for i, text in enumerate(comments):
        w = struct.unpack("<12h", bytes.fromhex("".join(text.split())))
        cant = " ".join(str(b) for b in w[2 : 2 + w[1]])
        lines.append(f"{i}\t{names[f'{i:03d}']}\t{cant}\t{w[9]}\t{w[10]}\t{w[11]}")
    args.out.write_text("\n".join(lines) + "\n")
    print(f"wrote {RACES} races to {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
