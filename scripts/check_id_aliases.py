#!/usr/bin/env python3
"""Check that the alias written beside a number in the Rust code is the alias of that number.

The code writes content by number (`BuildingId::new(35) /* city_walls */`, `Ability::new(28), // flying`).
The comment is the only help for a reader, so it must not drift from `crates/f4x-def/content/buildings.ron` and
`crates/f4x-def/content/abilities.ron`. A comment is checked when it is `/* alias */` right after the number, or a
`//` comment whose first word is an alias of some other number (so a free-text comment is left alone).
Exit status 1 and a list when one is wrong.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# The call that writes a number, and the content file whose numbers they are. `id_numbered(N)` is how
# the code asks the unit registry for a number that may be missing from a mod.
KINDS = {
    r"BuildingId::new": "buildings.ron",
    r"Ability::new": "abilities.ron",
    r"UnitId::new": "units.ron",
    r"RealmId::new": "realms.ron",
    r"units\.id_numbered": "units.ron",
    r"SpellId::new": "spells.ron",
    r"spells\.id_numbered": "spells.ron",
    r"RaceId::new": "races.ron",
    r"races\.id_numbered": "races.ron",
    r"features\.id_numbered": "features.ron",
    r"terrains\.id_numbered": "terrain.ron",
    r"buildings\.id_numbered": "buildings.ron",
    r"ItemPowerId::new": "item_powers.ron",
    r"ItemId::new": "items.ron",
    r"Retort::new": "retorts.ron",
}


def aliases(file):
    text = (ROOT / "crates/f4x-def/content" / file).read_text()
    # `num: 3, id: "x"` or `id: "x", num: 3` (features and terrain write the id first).
    found = {int(n): a for n, a in re.findall(r'num: (\d+), id: "(\w+)"', text)}
    found.update({int(n): a for a, n in re.findall(r'id: "(\w+)", num: (\d+)', text)})
    return found


def main():
    tables = {kind: aliases(file) for kind, file in KINDS.items()}
    bad = []
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        # The whole file, because rustfmt may move the comment to the next line.
        text = path.read_text()
        for kind, table in tables.items():
            for m in re.finditer(rf"\b{kind}\((\d+)\)\s*(,?\s*/\*\s*(\w+)\s*\*/|,?[ \t]*//[ \t]*(\w+))?", text):
                num = int(m.group(1))
                said = m.group(3) or m.group(4)
                if said is None:
                    continue
                no = text.count("\n", 0, m.start()) + 1
                if num not in table:
                    bad.append(f"{path.relative_to(ROOT)}:{no}: {kind} {num} does not exist")
                elif said != table[num] and (m.group(3) or said in table.values()):
                    bad.append(f"{path.relative_to(ROOT)}:{no}: {kind} {num} is {table[num]}, not {said}")
    print("\n".join(bad) if bad else "all aliases beside numbers are right")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
