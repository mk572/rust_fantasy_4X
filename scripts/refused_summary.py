#!/usr/bin/env python3
"""Summarise a refused-order log of `f4x-tools soak --refused-log` (task 668).

    python3 scripts/refused_summary.py soak-out/islands-refused.tsv [more.tsv ...]
    python3 scripts/refused_summary.py FILE --min-stood 10   # only orders asked again for 10+ turns

Groups the refusals by source line, reason and the kind of detail (the squares and counts of the
detail column are replaced by `N`, so "to (3, 4): water" and "to (9, 1): water" are one group).
For each group it prints the refusals, the games, the wizards and units' orders it came from, and
the longest run of one order refused turn after turn (`stood`); a run of `--loop` turns or more is
marked as a loop. The log has the columns seed, turn, player, source, reason, command, stood,
detail. Reads only; nothing is changed.
"""
import argparse
import csv
import re
import sys
from collections import defaultdict
from pathlib import Path

LOOP_TURNS = 10


def kind_of(detail: str) -> str:
    """The detail without its squares and counts: what kind of failure it is."""
    return re.sub(r"\d+", "N", detail)


def read(paths: list[Path]):
    for path in paths:
        with path.open(newline="") as f:
            reader = csv.DictReader(f, delimiter="\t")
            for row in reader:
                yield row


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], epilog=__doc__.split("\n\n", 1)[1],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("logs", nargs="+", type=Path, help="refused-order logs (tsv)")
    ap.add_argument("--min-stood", type=int, default=1, help="only refusals that stood this many turns")
    ap.add_argument("--loop", type=int, default=LOOP_TURNS, help="turns of one repeated order that count as a loop")
    ap.add_argument("--top", type=int, default=40, help="groups to print")
    args = ap.parse_args()

    groups = defaultdict(lambda: {"n": 0, "games": set(), "orders": set(), "stood": 0})
    for row in read(args.logs):
        stood = int(row["stood"])
        if stood < args.min_stood:
            continue
        key = (row["source"], row["reason"], kind_of(row.get("detail", "")))
        g = groups[key]
        g["n"] += 1
        g["games"].add((row["seed"], row["player"]))
        g["orders"].add((row["seed"], row["player"], row["command"]))
        g["stood"] = max(g["stood"], stood)
    if not groups:
        print("no refusals", file=sys.stderr)
        return 0
    print("| refusals | wizards (seed, player) | distinct orders | longest run | source | reason | detail |")
    print("|---|---|---|---|---|---|---|")
    ranked = sorted(groups.items(), key=lambda kv: -kv[1]["n"])[: args.top]
    for (source, reason, kind), g in ranked:
        loop = " LOOP" if g["stood"] >= args.loop else ""
        print(f"| {g['n']} | {len(g['games'])} | {len(g['orders'])} | {g['stood']}{loop} | {source} | {reason} | {kind or '-'} |")
    print(f"\n{sum(g['n'] for g in groups.values())} refusals in {len(groups)} groups", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
