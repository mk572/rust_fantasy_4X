#!/usr/bin/env python3
"""Write the original's item tables (`ITEMDATA.LBX`, `ITEMPOW.LBX`) as TSVs the tests read.

`items.tsv`: the 250 prefab items in the original's order: type, cost, the five plain bonuses
(attack, to hit, defence, moves as the file stores them in whole moves, resistance), spell skill,
spell save, the charges of the embedded spell, the 32-bit power mask and the flag byte.
`item_powers.tsv`: the powers in the original's order: the mask of the item types that may carry
it, its mana cost, group, realm (0 nature, 1 sorcery, 2 chaos, 3 life, 4 death, 5 arcane), the
books it needs and its power mask. `crates/f4x-data/tests/original_content.rs` compares the
content with them. Needs the author's copy of the game; the output is numbers and names, so it
is committed.

    python3 -I scripts/export_original_items.py [--game DIR] [--out DIR]
"""
import argparse
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_GAME = REPO / "reference/mom-classic/Master of Magic Official Release"
DEFAULT_OUT = REPO / "crates/f4x-data/tests/original"
sys.path.insert(0, str(REPO / "scripts"))
from import_mom import cstr, read_lbx, read_records  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--game", type=Path, default=DEFAULT_GAME)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()

    items = read_records(read_lbx(args.game / "ITEMDATA.LBX")[0])
    lines = ["index\tname\ttype\tcost\tattack\tto_hit\tdefense\tmoves\tresist\tspell_skill\t"
             "spell_save\tcharges\tpowers\tflag"]
    for i, r in enumerate(items):
        kind = r[0x21]
        cost = struct.unpack_from("<h", r, 0x22)[0]
        attack, to_hit, defense, moves, resist, skill, save = struct.unpack_from("<7b", r, 0x24)
        charges = struct.unpack_from("<h", r, 0x2C)[0]
        powers = struct.unpack_from("<I", r, 0x2E)[0]
        flag = r[55]
        lines.append(f"{i}\t{cstr(r[:30])}\t{kind}\t{cost}\t{attack}\t{to_hit}\t{defense}\t{moves}\t"
                     f"{resist}\t{skill}\t{save}\t{charges}\t{powers}\t{flag}")
    (args.out / "items.tsv").write_text("\n".join(lines) + "\n")
    print(f"wrote {len(items)} items")

    powers = read_records(read_lbx(args.game / "ITEMPOW.LBX")[0])
    lines = ["index\tname\tkinds\tcost\tgroup\trealm\tbooks\tmask"]
    for i, r in enumerate(powers):
        name = cstr(r[:18]).strip()
        if not name:
            continue
        kinds, cost, group, realm, books, mask = struct.unpack_from("<HHBBHI", r, 18)
        lines.append(f"{i}\t{name}\t{kinds}\t{cost}\t{group}\t{realm}\t{books}\t{mask}")
    (args.out / "item_powers.tsv").write_text("\n".join(lines) + "\n")
    print(f"wrote {len(lines) - 1} item powers")
    return 0


if __name__ == "__main__":
    sys.exit(main())
