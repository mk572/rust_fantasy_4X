#!/usr/bin/env python3
"""Check that the alias written beside a number in the Rust code is the alias of that number.

The code writes content by number (`BuildingId::new(35) /* city_walls */`, `Ability::new(28), // flying`).
The comment is the only help for a reader, so it must not drift from `content/buildings.ron` and
`content/abilities.ron`. A comment is checked when it is `/* alias */` right after the number, or a
`//` comment whose first word is an alias of some other number (so a free-text comment is left alone).
Exit status 1 and a list when one is wrong.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KINDS = {"BuildingId": "buildings.ron", "Ability": "abilities.ron"}


def aliases(file):
    text = (ROOT / "content" / file).read_text()
    return {int(n): a for n, a in re.findall(r'num: (\d+), id: "(\w+)"', text)}


def main():
    tables = {kind: aliases(file) for kind, file in KINDS.items()}
    bad = []
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        for no, line in enumerate(path.read_text().splitlines(), 1):
            for kind, table in tables.items():
                for m in re.finditer(rf"\b{kind}::new\((\d+)\)\s*(/\*\s*(\w+)\s*\*/|,?\s*//\s*(\w+))?", line):
                    num = int(m.group(1))
                    said = m.group(3) or m.group(4)
                    if said is None:
                        continue
                    if num not in table:
                        bad.append(f"{path.relative_to(ROOT)}:{no}: {kind} {num} does not exist")
                    elif said != table[num] and (m.group(3) or said in table.values()):
                        bad.append(f"{path.relative_to(ROOT)}:{no}: {kind} {num} is {table[num]}, not {said}")
    print("\n".join(bad) if bad else "all aliases beside numbers are right")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
