"""Index the original-game bugs the ReMoM clone marks with `OGBUG` comments.

Each marker is a place where the original code departs from what the manual or the
author's intent describes, so the index is a worklist for deciding which behaviour
the new game follows (see docs/help-vs-implementation-conflicts.md). The index
repeats ReMoM's comment text, so it is written to the gitignored `content-local/`.

Usage:
    python3 -I scripts/list_ogbugs.py [--remom DIR] [--out FILE]
"""

from __future__ import annotations

import argparse
import logging
import re
import sys
from collections import Counter
from pathlib import Path

LOG = logging.getLogger("list_ogbugs")
REPO = Path(__file__).resolve().parent.parent
DEFAULT_REMOM = Path.home() / "projects/ReMoM"
SOURCE_DIRS = ("MoM/src", "MoX/src")
AI_FILES = ("AIMOVE", "AISPELL", "AIDUDES", "AIDATA", "AIBUILD", "CMBTAI")
# Files that are about presentation, not rules.
UI_FILES = ("FLIC_Draw", "Fonts", "LOADER", "LoadScr", "Settings", "ALLOC")


def area_of(stem: str) -> str:
    if stem in AI_FILES:
        return "AI"
    if stem in UI_FILES:
        return "presentation/loading"
    if stem.startswith(("Combat", "CMBMAGIC", "COMBINIT")):
        return "combat"
    if stem.startswith(("Spells", "Spellbook", "SPLMASTR", "OverSpel")):
        return "spells"
    if stem.startswith(("MAPGEN", "Terrain", "SETTLE", "MainScr_Maps")):
        return "map and terrain"
    if stem.startswith(("CITYCALC", "NEXTTURN", "ProdScr", "CONQUEST")):
        return "cities and turn processing"
    return "other"


def collect(remom: Path) -> list[tuple[str, int, str]]:
    found = []
    for sub in SOURCE_DIRS:
        for path in sorted((remom / sub).glob("*.[ch]")):
            for number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
                if "OGBUG" in line:
                    text = re.sub(r"\s+", " ", re.sub(r"/\*|\*/|//", "", line)).strip()
                    found.append((path.name, number, text))
    return found


def render(found: list[tuple[str, int, str]]) -> str:
    by_area = Counter(area_of(Path(name).stem) for name, _, _ in found)
    lines = [
        "# OGBUG index (generated; do not commit)",
        "",
        f"{len(found)} markers. By area: " + ", ".join(f"{a} {n}" for a, n in by_area.most_common()),
        "",
    ]
    for area in sorted(by_area, key=lambda a: (a in ("AI", "presentation/loading"), a)):
        lines += [f"## {area}", ""]
        lines += [f"- `{n}:{ln}` {t}" for n, ln, t in found if area_of(Path(n).stem) == area]
        lines.append("")
    return "\n".join(lines)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--remom", type=Path, default=DEFAULT_REMOM)
    parser.add_argument("--out", type=Path, default=REPO / "content-local/OGBUG_INDEX.md")
    args = parser.parse_args(argv)
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    found = collect(args.remom)
    if not found:
        LOG.error("no OGBUG markers found under %s", args.remom)
        return 1
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(render(found), encoding="utf-8")
    LOG.info("wrote %d markers to %s", len(found), args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
