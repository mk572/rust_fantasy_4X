#!/usr/bin/env python3
"""Lists the tests that run on things made up by the test instead of the shipped game.

The rule ("Test on the actual game data and rules", `docs/rust_coding_rules.md`): a test runs on the
shipped content and the real engine; it does not hand-build units, spells, buildings or rule sets,
and it builds only situations the game can reach.

What it does:
  * Reads every `.rs` file under `crates/` (not `f4x-ai-ref`, a generated copy) and finds each
    `#[test]` function.
  * Flags a test whose body, or the body of a helper function of the same file it calls (followed
    through any number of helpers: `spearman()` -> `BattleUnit::simple`), contains one of the
    patterns in `PATTERNS`.
  * Writes the list, by crate and file with the line of each test, to `docs/tests-on-imaginary-units.md`
    (`--out` for another file).

It is a text search, not a compiler: it can flag a test that is fine (a pure function tested with
a made-up input) and it cannot see a made-up situation built by other means (a number written
straight onto a unit). Treat the list as the worksheet to read, not as the verdict.
"""

import argparse
import os
import re
import sys
from dataclasses import dataclass, field

CRATES = "crates"
SKIPPED_CRATES = ("f4x-ai-ref",)
DEFAULT_OUT = "docs/tests-on-imaginary-units.md"

# What each pattern means, as the list names it. Settings (`g.settings.rules.x = ..`) are not rule
# sets, so only `content.rules` assignments count. `MADE_UP` is the core of the rule (a unit, a
# battle, a spell's numbers, a rule set); `HAND_BUILT` is a game object or entity put together by
# hand, which is often only the set-up of a test of the engine's internals and needs a closer look.
MADE_UP = [
    (r"BattleUnit::simple\(", "hand-made battle unit (`BattleUnit::simple`)"),
    (r"\bBattle::new\(", "hand-made battle (`Battle::new`)"),
    (r"\bModifier::(?:new|amount)\(", "hand-made spell modifier (`Modifier::new`)"),
    (r"\bContent::default\(\)", "empty content (`Content::default`)"),
    (r"\bRegistry::new\(", "hand-built registry (`Registry::new`)"),
    (
        r"\b(?:UnitDef|SpellDef|AbilityDef|BuildingDef|RaceDef|ItemDef|HeroDef|RealmDef|RetortDef"
        r"|TerrainDef|FeatureDef|LairDef|ItemPowerDef)\s*\{",
        "hand-built definition (struct literal)",
    ),
    (r"\bcontent\.rules\.[a-z_]+\s*=[^=]", "assigns to `content.rules`"),
]
HAND_BUILT = [
    (
        r"\b(?:Item|Lair|Hero|City|Treasury)\s*\{\s*[a-z_]+\s*:",
        "hand-built game object (`Item`, `Lair`, `Hero`, `City`, `Treasury` literal)",
    ),
    (r"\.spawn\(\(\s*[^)\s]", "hand-built entity (`spawn` with components)"),
]
MADE_UP_LABELS = {label for _, label in MADE_UP}
COMPILED = [(re.compile(p), label) for p, label in MADE_UP + HAND_BUILT]

# Files `rust_coding_rules.md` names as allowed to build their own content, with its reason.
NAMED_EXCEPTIONS = {
    "crates/f4x-core/src/hook.rs": "validator test",
    "crates/f4x-core/src/game/save/tests.rs": "a save naming a unit the content lacks",
}

FN_RE = re.compile(r"(?:^|\n)[ \t]*((?:#\[[^\]]*\]\s*)*)(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?fn\s+(\w+)")


@dataclass
class Fn:
    name: str
    line: int
    is_test: bool
    body: str
    reasons: set = field(default_factory=set)
    via: list = field(default_factory=list)


def blank_comments_and_strings(src: str) -> str:
    """The source with comments and string literals blanked, so braces and names inside them
    are not read as code. Line numbers are kept."""
    out = []
    i, n = 0, len(src)
    while i < n:
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
            i = j
        elif src.startswith("/*", i):
            j = src.find("*/", i)
            j = n if j < 0 else j + 2
            out.append(re.sub(r"[^\n]", " ", src[i:j]))
            i = j
        elif src[i] == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            out.append(re.sub(r"[^\n]", " ", src[i : j + 1]))
            i = j + 1
        else:
            out.append(src[i])
            i += 1
    return "".join(out)


def parse_functions(path: str) -> list[Fn]:
    with open(path, encoding="utf8") as handle:
        src = blank_comments_and_strings(handle.read())
    fns = []
    for match in FN_RE.finditer(src):
        start = src.find("{", match.end())
        semicolon = src.find(";", match.end())
        if start < 0 or (0 <= semicolon < start):
            continue
        depth, end = 0, start
        while end < len(src):
            depth += {"{": 1, "}": -1}.get(src[end], 0)
            if depth == 0:
                break
            end += 1
        fns.append(
            Fn(
                name=match.group(2),
                line=src.count("\n", 0, match.start(2)) + 1,
                is_test="#[test]" in match.group(1).replace(" ", ""),
                body=src[start : end + 1],
            )
        )
    return fns


def calls(body: str, name: str) -> bool:
    """Whether `body` calls the function `name` of its own file: `name(..)` or `Self::name(..)`,
    not `Other::name(..)` (a function of another type that shares the name) and not `x.name(..)`
    (a method, which is not the file's own function)."""
    pattern = rf"(?:(?<![\w:.])|(?<=Self::)|(?<=self::)|(?<=super::)){re.escape(name)}\s*\("
    return re.search(pattern, body) is not None


def flag(fns: list[Fn]) -> dict[str, set]:
    """Mark each function with what it makes up, itself or through a helper of its file.
    Returns the helpers (not tests) that make something up, with their reasons."""
    for f in fns:
        f.reasons = {label for rx, label in COMPILED if rx.search(f.body)}
    helpers = {f.name: f for f in fns if not f.is_test}
    changed = True
    while changed:
        changed = False
        for f in fns:
            for name, helper in helpers.items():
                if name == f.name or not helper.reasons or not calls(f.body, name):
                    continue
                if not helper.reasons <= f.reasons:
                    f.reasons |= helper.reasons
                    changed = True
                if name not in f.via:
                    f.via.append(name)
    return {name: h.reasons for name, h in helpers.items() if h.reasons}


def rust_files() -> list[str]:
    found = []
    for folder, _, names in os.walk(CRATES):
        parts = folder.split(os.sep)
        if any(skip in parts for skip in SKIPPED_CRATES) or "target" in parts:
            continue
        found += [os.path.join(folder, n) for n in names if n.endswith(".rs")]
    return sorted(found)


Report = list[tuple[str, list[Fn]]]


def split_by_tier(report: list[tuple[str, list[Fn]]]) -> tuple[Report, Report]:
    """The tests that make up a unit, battle, spell or rule set, and (apart) those that only
    build a game object or entity by hand."""
    made_up: Report = []
    hand_built: Report = []
    for path, fns in report:
        for target, wanted in ((made_up, True), (hand_built, False)):
            tests = [
                f for f in fns
                if f.is_test and bool(f.reasons & MADE_UP_LABELS) == wanted and f.reasons
            ]
            if tests:
                helpers = {h for t in tests for h in t.via}
                target.append((path, tests + [f for f in fns if f.name in helpers]))
    return made_up, hand_built


def count(report: Report) -> int:
    return sum(f.is_test for _, fns in report for f in fns)


def section(report: Report, title: str) -> list[str]:
    lines = [f"# {title}", "", "| File | Tests |", "|---|---|"]
    for path, fns in sorted(report, key=lambda r: (-count([r]), r[0])):
        lines.append(f"| `{path}` | {count([(path, fns)])} |")
    for path, fns in report:
        tests = [f for f in fns if f.is_test]
        lines += ["", f"## `{path}` ({len(tests)})", ""]
        if path in NAMED_EXCEPTIONS:
            lines += [
                "`rust_coding_rules.md` names this file as allowed to build its own content: "
                f"{NAMED_EXCEPTIONS[path]}.",
                "",
            ]
        used = sorted({h for t in tests for h in t.via})
        if used:
            named = ", ".join(
                f"`{h}` (line {next(f.line for f in fns if f.name == h)})" for h in used
            )
            lines += [f"Helpers that make things up: {named}.", ""]
        lines += ["| Test | Line | What is made up | Through | Decision |", "|---|---|---|---|---|"]
        for t in tests:
            through = ", ".join(f"`{h}`" for h in t.via) or "itself"
            lines.append(f"| `{t.name}` | {t.line} | {'; '.join(sorted(t.reasons))} | {through} | |")
    return lines


def render(report: Report, total: int) -> str:
    made_up, hand_built = split_by_tier(report)
    lines = [
        "# Tests on imaginary units",
        "",
        "Generated by `scripts/imaginary_tests.py`; do not edit by hand, fix the tests and run it again.",
        "",
        "The rule (`docs/rust_coding_rules.md`, \"Test on the actual game data and rules\"): a test runs on",
        "the shipped content and the real engine. It does not hand-build units, spells, buildings or",
        "rule sets, it looks them up by name in the shipped content, and it builds only situations the",
        "game can reach.",
        "",
        "Part 1 lists the tests that make up a unit, a battle, a spell's numbers or a rule set. Part 2",
        "lists the tests that only put a game object or an entity together by hand (often the set-up of a",
        "test of the engine's internals), which needs a closer look. A test that reaches the made-up",
        "thing through a helper (`spearman()`, `duel()`) is listed with the helper in the `Through`",
        "column. It is a text search, so a row can be a false alarm (a pure function given a made-up",
        "input) and a made-up situation built another way (a number written onto a unit) is not found.",
        "The `Decision` column is for the walk through the list.",
        "",
        f"**Part 1: {count(made_up)} tests. Part 2: {count(hand_built)} tests. Of {total} in all.**",
        "",
    ]
    lines += section(made_up, "Part 1: made-up units, battles, spells and rule sets")
    lines += [""] + section(hand_built, "Part 2: hand-built game objects and entities")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", default=DEFAULT_OUT, help="the file to write (default: %(default)s)")
    args = parser.parse_args()
    if not os.path.isdir(CRATES):
        sys.exit("run it from the repository root")
    report, total = [], 0
    for path in rust_files():
        fns = parse_functions(path)
        total += sum(f.is_test for f in fns)
        flag(fns)
        if any(f.is_test and f.reasons for f in fns):
            report.append((path, fns))
    with open(args.out, "w", encoding="utf8") as handle:
        handle.write(render(report, total))
    made_up, hand_built = split_by_tier(report)
    print(f"{count(made_up)} + {count(hand_built)} of {total} tests listed in {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
