#!/usr/bin/env python3
"""List the content ids (spells, units, buildings, abilities, ...) that no test names.

For each `content/*.ron` file that has a list of rows with `id: "..."`, it looks for the id as a
quoted string in the test code of the workspace: whole files named `tests.rs` or under a `tests/`
folder, and the part of any other source file from its `#[cfg(test)]` on. An id that is only
named in the content or in ordinary code is untested here. A loop over the whole content covers
an id without naming it; such an id still shows up, so read the list as "to look at", not as
"broken" (task 302, part 4). The kinds in `WALKED` are checked row by row against the original's
table (task 404) and count as covered.
"""
import argparse
import re
import sys
from pathlib import Path

FILES = [
    "spells", "units", "buildings", "abilities", "items", "item_powers", "heroes", "lairs",
    "features", "races", "terrain", "level_paths",
]
# Content kinds whose every row is compared with the original's table by a test that walks the
# whole file (`crates/f4x-data/tests/original_content.rs`, task 404): all ids are covered.
WALKED = {"units", "buildings", "items", "item_powers"}
ID = re.compile(r'\(\s*id:\s*"([^"]+)"')


def test_code(root: Path) -> str:
    """All test code of the workspace, as one text."""
    out = []
    for path in sorted(root.glob("crates/**/*.rs")):
        text = path.read_text(encoding="utf-8", errors="replace")
        parts = path.parts
        if path.name == "tests.rs" or "tests" in parts[parts.index("crates"):]:
            out.append(text)
            continue
        cut = text.find("#[cfg(test)]")
        if cut >= 0:
            out.append(text[cut:])
    return "\n".join(out)


def ids_of(path: Path) -> list[str]:
    ids = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.lstrip().startswith("//"):
            continue
        m = ID.search(line)
        if m:
            ids.append(m.group(1))
    return ids


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--root", default=".", help="the project folder (default: the current one)")
    ap.add_argument("--only", action="append", choices=FILES, help="a content file; repeatable")
    ap.add_argument("--names", action="store_true", help="list the ids, not only the counts")
    args = ap.parse_args()
    root = Path(args.root)
    tests = test_code(root)
    if not tests:
        print("no test code found; run it from the project folder", file=sys.stderr)
        return 2
    print(f"{'file':<12} {'ids':>5} {'untested':>9}")
    for name in args.only or FILES:
        path = root / "content" / f"{name}.ron"
        if not path.exists():
            continue
        ids = ids_of(path)
        missing = [] if name in WALKED else [i for i in ids if f'"{i}"' not in tests]
        print(f"{name:<12} {len(ids):>5} {len(missing):>9}")
        if args.names and missing:
            print("    " + ", ".join(missing))
    return 0


if __name__ == "__main__":
    sys.exit(main())
