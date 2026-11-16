#!/usr/bin/env python3
"""Summarise `f4x-tools soak` logs: results, the full counters, and how races and realms do.

Each log is one soak run (one player count and rule setting). The report has, for every log, the
games and their results; the sum of every counter over all wizards; and the wizards grouped by
race, by their strongest realm and by retort, with how many won, how many were still alive at the
end and what they averaged. Needs the `profile` and `stats` lines the soak prints.

    scripts/soak_report.py soak-out/a-p2.log soak-out/a-p3.log > report.md
    scripts/soak_report.py -h
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path

RESULT = re.compile(
    r"^seed (?P<seed>\d+): (?:won by player (?P<winner>\d+) \((?P<how>[^)]+)\) on turn (?P<turn>\d+)"
    r"|no winner after (?P<limit>\d+) turns); (?P<body>.*)$"
)
WIZARD = re.compile(
    r"p(?P<id>\d+)\[(?P<objective>[^/\]]+)/(?P<temper>[^\]]+)\] "
    r"(?P<cities>\d+)c\((?P<outposts>\d+)o\)/(?P<units>\d+)u/(?P<gold>-?\d+)g(?P<defeated> \(defeated\))?"
)
PROFILE = re.compile(
    r"^seed (?P<seed>\d+): profile p(?P<id>\d+): race (?P<race>\S+); started on (?P<plane>\S+); "
    r"personality (?P<personality>\S+); objective (?P<objective>\S+); books \[(?P<books>[^\]]*)\]; "
    r"retorts \[(?P<retorts>[^\]]*)\]"
)
STATS = re.compile(r"^seed (?P<seed>\d+): stats p(?P<id>\d+): (?P<body>.*)$")

# The counters averaged in the race, realm and retort tables.
AVERAGED = [
    "cities captured",
    "cities lost",
    "battles won",
    "battles lost",
    "units built",
    "spells cast",
    "heroes joined",
    "mercenaries hired",
    "items made",
    "production rushed",
]


@dataclass
class Wizard:
    """One wizard of one game, from the result, profile and stats lines."""

    game: tuple[str, int]
    slot: int
    race: str = "?"
    realm: str = "?"
    retorts: list[str] = field(default_factory=list)
    cities: int = 0
    units: int = 0
    gold: int = 0
    defeated: bool = False
    won: bool = False
    stats: dict[str, int] = field(default_factory=dict)


@dataclass
class Game:
    """The result of one game."""

    log: str
    seed: int
    winner: int | None
    how: str
    turn: int


def strongest_realm(books: str) -> str:
    """The realm with the most books; two realms with the same count are written `a/b`."""
    counts: dict[str, int] = {}
    for part in books.split(","):
        part = part.strip()
        if part:
            name, number = part.rsplit(" ", 1)
            counts[name] = int(number)
    if not counts:
        return "none"
    top = max(counts.values())
    return "/".join(sorted(name for name, n in counts.items() if n == top))


def parse(path: Path) -> tuple[list[Game], list[Wizard]]:
    """Read one soak log."""
    games: list[Game] = []
    wizards: dict[tuple[int, int], Wizard] = {}
    log = path.name
    for line in path.read_text().splitlines():
        if m := RESULT.match(line):
            seed = int(m["seed"])
            winner = int(m["winner"]) if m["winner"] else None
            games.append(
                Game(log, seed, winner, m["how"] or "no winner", int(m["turn"] or m["limit"]))
            )
            for w in WIZARD.finditer(m["body"]):
                wizard = wizards.setdefault((seed, int(w["id"])), Wizard((log, seed), int(w["id"])))
                wizard.cities = int(w["cities"])
                wizard.units = int(w["units"])
                wizard.gold = int(w["gold"])
                wizard.defeated = bool(w["defeated"])
                wizard.won = winner == wizard.slot
        elif m := PROFILE.match(line):
            wizard = wizards.setdefault((int(m["seed"]), int(m["id"])), Wizard((log, int(m["seed"])), int(m["id"])))
            wizard.race = m["race"]
            wizard.realm = strongest_realm(m["books"])
            wizard.retorts = [r.strip() for r in m["retorts"].split(",") if r.strip()]
        elif m := STATS.match(line):
            wizard = wizards.setdefault((int(m["seed"]), int(m["id"])), Wizard((log, int(m["seed"])), int(m["id"])))
            for item in m["body"].split("; "):
                label, _, value = item.rpartition(" = ")
                wizard.stats[label] = int(value)
    return games, list(wizards.values())


def summary(log: str, games: list[Game]) -> str:
    """The results of one log as a table."""
    won = [g for g in games if g.winner is not None]
    by_how: dict[str, int] = defaultdict(int)
    for g in won:
        by_how[g.how] += 1
    kinds = ", ".join(f"{n} by {how}" for how, n in sorted(by_how.items())) or "none"
    lines = [
        f"### {log}",
        "",
        f"{len(won)} of {len(games)} games ended with a winner ({kinds}).",
        "",
        "| Seed | Result | Turn |",
        "|---|---|---|",
    ]
    for g in sorted(games, key=lambda g: g.seed):
        result = f"p{g.winner} {g.how}" if g.winner is not None else "**no winner**"
        lines.append(f"| {g.seed} | {result} | {g.turn} |")
    return "\n".join(lines)


def totals(wizards: list[Wizard]) -> str:
    """Every counter summed over all wizards of a log, and the mean per wizard."""
    sums: dict[str, int] = defaultdict(int)
    for w in wizards:
        for label, n in w.stats.items():
            sums[label] += n
    lines = ["| Counter | Total | Per wizard |", "|---|---|---|"]
    count = max(len(wizards), 1)
    for label, n in sums.items():
        lines.append(f"| {label} | {n} | {n / count:.1f} |")
    return "\n".join(lines)


def table(title: str, wizards: list[Wizard], key) -> str:
    """Wizards grouped by `key` (a function giving one or more group names for a wizard)."""
    groups: dict[str, list[Wizard]] = defaultdict(list)
    for w in wizards:
        for name in key(w):
            groups[name].append(w)
    head = ["wizards", "won", "alive", "cities", "units"] + AVERAGED
    lines = [
        f"| {title} | " + " | ".join(head) + " |",
        "|---|" + "---|" * len(head),
    ]
    for name, members in sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0])):
        n = len(members)
        won = sum(w.won for w in members)
        alive = sum(not w.defeated for w in members)
        cells = [
            str(n),
            f"{won} ({100 * won / n:.0f}%)",
            f"{100 * alive / n:.0f}%",
            f"{sum(w.cities for w in members) / n:.1f}",
            f"{sum(w.units for w in members) / n:.0f}",
        ]
        cells += [f"{sum(w.stats.get(c, 0) for w in members) / n:.1f}" for c in AVERAGED]
        lines.append(f"| {name} | " + " | ".join(cells) + " |")
    return "\n".join(lines)


def report(paths: list[Path]) -> str:
    """The whole report."""
    out = ["# Soak report", ""]
    every: list[Wizard] = []
    for path in paths:
        games, wizards = parse(path)
        every += wizards
        out += [summary(path.name, games), "", "Counters, all wizards:", "", totals(wizards), ""]
    out += [
        "## Races (all logs)",
        "",
        table("Race", every, lambda w: [w.race]),
        "",
        "## Realms (strongest realm; a tie counts in both)",
        "",
        table("Realm", every, lambda w: w.realm.split("/")),
        "",
        "## Retorts",
        "",
        table("Retort", every, lambda w: w.retorts or ["none"]),
        "",
    ]
    return "\n".join(out)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    parser.add_argument("logs", nargs="+", type=Path, help="soak logs (one run each)")
    args = parser.parse_args(argv)
    for path in args.logs:
        if not path.is_file():
            print(f"soak_report: no such file: {path}", file=sys.stderr)
            return 2
    print(report(args.logs))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
