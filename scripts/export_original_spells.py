#!/usr/bin/env python3
"""Write the original's spell table (`SPELLDAT.LBX`, entry 0) as a TSV the tests read.

One row per spell after the empty record 0, in the original's order (the order of
`content/spells.ron`): name, casting category (`type`: 0 is a summoning), realm (0 nature, 1 sorcery,
2 chaos, 3 life, 4 death, 5 arcane), casting cost, research cost, and the first parameter (the unit
a summoning creates). `crates/f4x-data/tests/it/original_content.rs` compares the content with it.
Needs the author's copy of the game; the output is numbers and names, so it is committed.

    python3 -I scripts/export_original_spells.py [--game DIR] [--out FILE]
"""
import argparse
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_GAME = REPO / "reference/mom-classic/Master of Magic Official Release"
DEFAULT_OUT = REPO / "crates/f4x-data/tests/original/spells.tsv"
sys.path.insert(0, str(REPO / "scripts"))
from import_mom import cstr, read_lbx, read_records  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--game", type=Path, default=DEFAULT_GAME)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()
    records = read_records(read_lbx(args.game / "SPELLDAT.LBX")[0])[1:]
    lines = ["index\tname\tcategory\trealm\tcost\tresearch\tparam"]
    for i, r in enumerate(records):
        category, _book, realm, _elig = struct.unpack_from("<bbbb", r, 21)
        cost, research = struct.unpack_from("<hH", r, 26)
        param = struct.unpack_from("<h", r, 32)[0]
        lines.append(f"{i}\t{cstr(r[:19])}\t{category}\t{realm}\t{cost}\t{research}\t{param}")
    args.out.write_text("\n".join(lines) + "\n")
    print(f"wrote {len(records)} spells to {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
