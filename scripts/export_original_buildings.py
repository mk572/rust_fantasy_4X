#!/usr/bin/env python3
"""Write the original's building table (`BUILDDAT.LBX`, entry 0) as a TSV the tests read.

One row per record, in the original's order: name, cost, upkeep (gold a turn), the two
required buildings (a record number, 0 for none, 100 and up for a terrain), the building it
replaces and the experience it gives to new units (20 or 60). `crates/f4x-data/tests/
original_content.rs` compares the content with it. Needs the author's copy of the game; the
output is numbers and names, so it is committed.

    python3 -I scripts/export_original_buildings.py [--game DIR] [--out FILE]
"""
import argparse
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_GAME = REPO / "reference/mom-classic/Master of Magic Official Release"
DEFAULT_OUT = REPO / "crates/f4x-data/tests/original/buildings.tsv"
sys.path.insert(0, str(REPO / "scripts"))
from import_mom import cstr, read_lbx, read_records  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--game", type=Path, default=DEFAULT_GAME)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()
    records = read_records(read_lbx(args.game / "BUILDDAT.LBX")[0])
    lines = ["index\tname\tcost\tupkeep\treq1\treq2\treplaces\txp"]
    for i, r in enumerate(records):
        req1, req2, replace = struct.unpack_from("<hhh", r, 20)
        grants_20, grants_60 = struct.unpack_from("<hh", r, 26)
        upkeep = -struct.unpack_from("<h", r, 32)[0]
        cost = struct.unpack_from("<h", r, 44)[0]
        xp = 60 if grants_60 else 20 if grants_20 else 0
        lines.append(f"{i}\t{cstr(r[:20])}\t{cost}\t{max(upkeep, 0)}\t{req1}\t{req2}\t{max(replace, 0)}\t{xp}")
    args.out.write_text("\n".join(lines) + "\n")
    print(f"wrote {len(records)} buildings to {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
