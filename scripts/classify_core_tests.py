#!/usr/bin/env python3
"""Sort the tests in crates/f4x-core/src by the rule in docs/rust_coding_rules.md ("Where a test lives").

A test is a candidate for crates/f4x-core/tests/it/ when it needs a Game or Content (directly, or
through a test helper of its file that does), names a shipped id, or is longer than LONG lines, AND
reaches no internals of the crate (`g.world`, `g.content`, `g.map`, `crate::...`, `put_*`, `super::*`
items): those stay unit tests on `testutil::shipped()`. "move" below counts the candidates.

    python3 scripts/classify_core_tests.py            # one line per file
    python3 scripts/classify_core_tests.py --tests    # one line per test that moves, with the reasons
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "crates/f4x-core/src"
LONG = 25
GAME = re.compile(
    r"\bGame\b|testutil|\bscenario_\w+|\bContent\b|\bcontent\(\)|\.apply\(|\bnew_game\b|\breal_\w+\(|\bcontent_with_\w+"
)


INTERNAL = re.compile(
    r"\bg(?:ame)?\.(?:world|content|map|settings|rng|events|threat|fog|turn|next_serial)\b|\.world\.|"
    r"\bcrate::(?!testdir|game::(?:Command|Event|Rejection))|\bput_(?:feature|lair|node)\w*\(|\bsuper::"
)


def shipped_ids():
    ids = set()
    for name in ("units", "spells", "buildings", "races", "items"):
        text = (ROOT / "content" / f"{name}.ron").read_text()
        ids |= set(re.findall(r'\bid:\s*"([a-z0-9_]+)"', text))
    return ids


def functions(text):
    """(name, is_test, start line, body) of every fn in the text, by brace matching."""
    lines = text.split("\n")
    out = []
    for i, line in enumerate(lines):
        m = re.match(r"\s*(?:pub(?:\([a-z]+\))?\s+)?fn (\w+)", line)
        if not m:
            continue
        is_test = any(lines[j].strip() == "#[test]" for j in range(max(0, i - 3), i))
        depth, started, j = 0, False, i
        while j < len(lines):
            depth += lines[j].count("{") - lines[j].count("}")
            started |= "{" in lines[j]
            if started and depth <= 0:
                break
            j += 1
        out.append((m.group(1), is_test, i + 1, "\n".join(lines[i : j + 1]), j - i + 1))
    return out


def test_part(path, text):
    if path.name in ("testutil.rs",):
        return ""
    if path.name == "tests.rs" or "tests" in path.parts[-2:-1]:
        return text
    i = text.find("#[cfg(test)]")
    return text[i:] if i >= 0 else ""


def classify(ids):
    result = {}
    for path in sorted(SRC.rglob("*.rs")):
        part = test_part(path, path.read_text())
        fns = functions(part)
        heavy = {n for n, t, _, b, _ in fns if not t and GAME.search(b)}
        changed = True
        while changed:
            changed = False
            for n, t, _, b, _ in fns:
                if not t and n not in heavy and any(re.search(rf"\b{h}\(", b) for h in heavy):
                    heavy.add(n)
                    changed = True
        for n, t, line, body, length in fns:
            if not t:
                continue
            why = []
            internal = INTERNAL.search(body) or any(
                re.search(rf"\b{h}\(", body) and INTERNAL.search(b2)
                for h, b2 in [(n2, b3) for n2, t2, _, b3, _ in fns if not t2]
            )
            if GAME.search(body) or any(re.search(rf"\b{h}\(", body) for h in heavy):
                why.append("game")
            lits = set(re.findall(r'"([a-z0-9_]+)"', body)) & ids
            if lits:
                why.append("ids " + ",".join(sorted(lits)[:3]))
            if length > LONG:
                why.append(f"{length} lines")
            if internal:
                why = []
            result.setdefault(path, []).append((n, line, why))
    return result


def main():
    per_test = "--tests" in sys.argv
    result = classify(shipped_ids())
    moved = stay = 0
    for path, tests in result.items():
        mv = [t for t in tests if t[2]]
        moved += len(mv)
        stay += len(tests) - len(mv)
        rel = path.relative_to(ROOT)
        if per_test:
            for n, line, why in mv:
                print(f"{rel}:{line} {n}: {'; '.join(why)}")
        else:
            print(f"{len(mv):4} move {len(tests) - len(mv):4} stay  {rel}")
    print(f"total: {moved} move, {stay} stay", file=sys.stderr)


main()
