#!/usr/bin/env python3
"""Write the original's unit table (ReMoM `_unit_type_table`, 198 rows) as a TSV the tests read.

The table is the numbers of `UNITTYPE.c` in the ReMoM clone: melee, ranged, ammo, to hit,
defence, resistance, moves (in half moves, as the file has them), cost, upkeep, hit points,
sight, transport, figures and the four flag words (movement, attributes 1 and 2, abilities), one
row per unit in the original's order (which is the order of `content/units.ron`). `crates/f4x-data/tests/it/original_content.rs` compares every unit of the
content with it. Needs the ReMoM clone; the output is numbers, so it is committed.

    python3 -I scripts/export_original_units.py [--remom DIR] [--out FILE]
"""
import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_REMOM = Path.home() / "projects/ReMoM"
DEFAULT_OUT = REPO / "crates/f4x-data/tests/original/units.tsv"
COLUMNS = ["index", "melee", "ranged", "ammo", "to_hit", "defense", "resist", "moves_half",
           "cost", "upkeep", "race", "hits", "sight", "transport", "figures", "move_flags", "attribs1", "attribs2", "abilities"]
# Field positions inside one row of the C initializer (after the name pointer).
FIELDS = {"melee": 0, "ranged": 1, "ammo": 3, "to_hit": 4, "defense": 5, "resist": 6,
          "moves_half": 7, "cost": 8, "upkeep": 9, "race": 10, "hits": 14, "sight": 15, "transport": 16,
          "figures": 17, "move_flags": 20, "attribs1": 21,
          "attribs2": 22, "abilities": 23}


def rows(text: str) -> list[list[int]]:
    start = text.index("struct s_UNIT_TYPE _unit_type_table[NUM_UNIT_TYPES]")
    body = text[start:]
    body = body[body.index("{"):]
    out = []
    for line in body.splitlines():
        m = re.search(r"\{&_unit_type_names\[\d+\],(.*)\},", line)
        if m:
            out.append([int(v, 0) for v in m.group(1).split(",")])
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--remom", type=Path, default=DEFAULT_REMOM)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()
    text = (args.remom / "MoM/src/UNITTYPE.c").read_text(encoding="latin1")
    table = rows(text)
    if len(table) != 198:
        print(f"expected 198 units, found {len(table)}", file=sys.stderr)
        return 1
    lines = ["\t".join(COLUMNS)]
    for i, r in enumerate(table):
        lines.append("\t".join([str(i)] + [str(r[FIELDS[c]]) for c in COLUMNS[1:]]))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text("\n".join(lines) + "\n")
    print(f"wrote {len(table)} units to {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
