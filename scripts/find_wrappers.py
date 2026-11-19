#!/usr/bin/env python3
"""List the functions of the workspace that only pass their arguments to another function (task 654).

    python3 scripts/find_wrappers.py                 # every crate, trivial wrappers first
    python3 scripts/find_wrappers.py --crate f4x-ai  # one crate
    python3 scripts/find_wrappers.py --kind trivial  # only the pure forwarders

Reads the Rust sources under `crates/*/src` (not `f4x-ai-ref`, not `tests/`), finds each `fn` whose
body is one expression, and sorts it:

* `trivial`: the body is one call (a path or a method) and every argument is a parameter of the
  function, as written (`x`, `&x`, `&mut x`, `*x`) or `self`: nothing is checked, converted or defaulted.
* `std`: like `trivial` but the call is a standard method (`is_some`, `contains`, `len`, `new`):
  the name adds meaning (`is_defeated`), so these mostly stay.
* `task`: the body is one call but some argument is computed (a default, a conversion, a field, a
  clone), or the call is wrapped (`.ok()`, `?`, `.unwrap_or(..)`, `.map(..)`): a wrapper with a task.

Methods of trait impls are skipped (the trait requires them; `--trait-impls` lists them). The name
is shown as `Type::name`.

The caller count is a text search for `name(`, `.name(` and `name,`/`name)` (a function passed as a
value) over all of `crates/`; it over-counts a name used by several types. A text search misses
macros and a wrapper of two statements; read the hits (docs/KNOWLEDGE.md has what was decided).
Output is a table: kind, crate, file:line, name, target, callers. Nothing is changed.
"""
import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"
SKIP_CRATES = {"f4x-ai-ref"}

FN = re.compile(r"\bfn\s+([A-Za-z_]\w*)\s*(<[^{;]*?>)?\s*\(")
IMPL = re.compile(r"\bimpl\b[^{;]*\{")
# Methods of the standard library: a function that only calls one of these names a test or a conversion
# (`is_defeated` is `is_some`), and stays: the name adds the meaning.
STD = {"is_some", "is_none", "is_ok", "is_err", "is_empty", "len", "contains", "copied", "cloned", "as_ref",
       "as_deref", "ok", "iter", "flatten", "default", "new", "from", "get", "insert", "remove", "take",
       "sum", "Self", "into", "to_string", "unwrap_or_default", "hash", "serialize"}

PLAIN_ARG = re.compile(r"^(?:&\s*(?:mut\s+)?|\*)?\s*([A-Za-z_]\w*)$")


def match_close(text: str, start: int, open_ch: str, close_ch: str) -> int:
    """Index of the bracket closing the one at `start` (strings and chars are skipped)."""
    depth = 0
    i = start
    while i < len(text):
        c = text[i]
        if c == '"':
            i += 1
            while i < len(text) and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif c == open_ch:
            depth += 1
        elif c == close_ch:
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1


def split_top(text: str) -> list[str]:
    """Split on commas outside brackets."""
    parts, depth, cur = [], 0, ""
    for c in text:
        if c in "([{<":
            depth += 1
        elif c in ")]}>":
            depth -= 1
        if c == "," and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += c
    if cur.strip():
        parts.append(cur)
    return [p.strip() for p in parts if p.strip()]


def param_names(params: str) -> set[str]:
    names = set()
    for part in split_top(params):
        if re.match(r"^(&\s*('\w+\s+)?(mut\s+)?)?self\b", part) or part.startswith("mut self"):
            names.add("self")
            continue
        name = part.split(":")[0].strip()
        name = re.sub(r"^(mut\s+)", "", name)
        if re.match(r"^[A-Za-z_]\w*$", name):
            names.add(name)
    return names


def classify(body: str, params: set[str]):
    """(kind, target) for a one-expression body, or None when it is not a call."""
    body = body.strip().rstrip(";").strip()
    if not body or ";" in body or body.startswith(("let ", "if ", "match ", "for ", "while ", "loop")):
        return None
    if "\n\n" in body or "{" in body.replace("{}", ""):
        return None
    # peel a trailing `?`
    wrapped = body.endswith("?")
    core = body[:-1] if wrapped else body
    # find the outermost call: the last top-level `(...)` whose close paren ends the expression
    if not core.endswith(")"):
        return None
    depth, open_at = 0, -1
    for i in range(len(core) - 1, -1, -1):
        if core[i] == ")":
            depth += 1
        elif core[i] == "(":
            depth -= 1
            if depth == 0:
                open_at = i
                break
    if open_at <= 0:
        return None
    callee, args = core[:open_at].strip(), core[open_at + 1 : -1]
    if not re.match(r"^[A-Za-z_][\w:<>.,() &*]*$", callee):
        return None
    target = re.split(r"[.:]", callee)[-1].strip() or callee
    # a method chain `a.b().c(x)` is a wrapper of `c`; `a().b(x)` hides a lookup: `task`
    chain_calls = callee.count("(")
    plain = True
    for arg in split_top(args):
        m = PLAIN_ARG.match(arg)
        if not m or (m.group(1) not in params and m.group(1) != "self"):
            plain = False
    receiver_ok = True
    head = callee.split(".")[0]
    if "." in callee and head != "self" and head not in params and not re.match(r"^[A-Za-z_]\w*$", head):
        receiver_ok = False
    if "." in callee and head not in params and head != "self" and "(" not in head and "::" not in head:
        receiver_ok = False
    trivial = plain and not wrapped and chain_calls <= 1 and receiver_ok
    return ("trivial" if trivial else "task"), target


def impl_blocks(text: str):
    """(start, end, header) of each `impl ... {}` block."""
    for m in IMPL.finditer(text):
        end = match_close(text, m.end() - 1, "{", "}")
        if end > 0:
            yield m.start(), end, " ".join(text[m.start() : m.end() - 1].split())


def functions(path: Path):
    text = path.read_text(errors="replace")
    blocks = list(impl_blocks(text))
    for m in FN.finditer(text):
        open_paren = m.end() - 1
        close_paren = match_close(text, open_paren, "(", ")")
        if close_paren < 0:
            continue
        i = close_paren + 1
        while i < len(text) and text[i] not in "{;":
            i += 1
        if i >= len(text) or text[i] == ";":
            continue  # a declaration (trait method without a body)
        end = match_close(text, i, "{", "}")
        if end < 0:
            continue
        body = text[i + 1 : end]
        line = text.count("\n", 0, m.start()) + 1
        in_test = "#[cfg(test)]" in text[: m.start()] or "#[test]" in text[max(0, m.start() - 80) : m.start()]
        owner = next((h for a, b, h in reversed(blocks) if a < m.start() < b), "")
        yield m.group(1), text[open_paren + 1 : close_paren], body, line, in_test, owner


def all_text() -> str:
    parts = []
    for p in CRATES.rglob("*.rs"):
        if SKIP_CRATES & set(p.parts) or "target" in p.parts:
            continue
        parts.append(p.read_text(errors="replace"))
    return "\n".join(parts)


def print_chains(rows) -> int:
    """Chains of wrappers: each wrapper points at the wrappers named like its target. A name that
    is shared by an ordinary function and a wrapper is a guess, so the list is a place to look, not a verdict.
    `std` wrappers are left out (the name adds the meaning)."""
    wrappers = [r for r in rows if r[0] != "std"]
    by_name: dict[str, list] = {}
    for r in wrappers:
        by_name.setdefault(r[4].split("::")[-1], []).append(r)
    memo: dict[tuple, list] = {}

    def longest(r, seen=()):
        key = (r[2], r[3])
        if key in memo:
            return memo[key]
        best = [r]
        for nxt in by_name.get(r[5], []):
            if (nxt[2], nxt[3]) in seen or nxt is r:
                continue
            c = [r] + longest(nxt, seen + (key,))
            if len(c) > len(best):
                best = c
        memo[key] = best
        return best

    chains = [longest(r) for r in wrappers]
    called = {(n[2], n[3]) for c in chains for n in c[1:]}
    chains = [c for c in chains if len(c) >= 2 and (c[0][2], c[0][3]) not in called]
    chains.sort(key=lambda c: -len(c))
    print("| layers | chain (each wrapper calls the next, the last calls the target) |\n|---|---|")
    for c in chains:
        path = " -> ".join(f"`{r[4]}` ({r[2].split('/src/')[-1]}:{r[3]})" for r in c)
        print(f"| {len(c) + 1} | {path} -> `{c[-1][5]}` |")
    print(f"\n{len(chains)} chains", file=sys.stderr)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0], epilog=__doc__.split("\n\n", 1)[1],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--crate", help="only this crate (directory name under crates/)")
    ap.add_argument("--kind", choices=["trivial", "std", "task"], help="only this kind")
    ap.add_argument("--trait-impls", action="store_true", help="include the methods of trait impls")
    ap.add_argument("--tests", action="store_true", help="include test code")
    ap.add_argument("--chains", action="store_true",
                    help="list the chains: a wrapper whose target is itself a wrapper (task 725), longest first")
    args = ap.parse_args()

    corpus = all_text()
    rows = []
    for crate in sorted(CRATES.iterdir()):
        if crate.name in SKIP_CRATES or (args.crate and crate.name != args.crate):
            continue
        for path in sorted((crate / "src").rglob("*.rs")) if (crate / "src").exists() else []:
            if path.name.startswith("tests") or path.name.endswith("_tests.rs"):
                if not args.tests:
                    continue
            for name, params, body, line, in_test, owner in functions(path):
                if in_test and not args.tests:
                    continue
                if re.search(r"\bimpl\b.*\bfor\b", owner) and not args.trait_impls:
                    continue  # a trait requires the method
                got = classify(body, param_names(params))
                if not got or (args.kind and got[0] != args.kind):
                    continue
                kind, target = got
                if kind == "trivial" and target in STD:
                    kind = "std"
                owner = re.sub(r"^impl(<[^>]*>)?\s*", "", owner)
                callers = len(re.findall(rf"(?<![\w]){re.escape(name)}\s*[(,)]", corpus)) - 1
                rows.append((kind, crate.name, str(path.relative_to(ROOT)), line, f"{owner}::{name}" if owner else name, target, max(callers, 0)))
    if args.chains:
        return print_chains(rows)
    rows.sort(key=lambda r: (["trivial", "std", "task"].index(r[0]), r[1], r[2], r[3]))
    print("| kind | crate | where | name | calls | callers |\n|---|---|---|---|---|---|")
    for kind, crate, where, line, name, target, callers in rows:
        print(f"| {kind} | {crate} | {where}:{line} | `{name}` | `{target}` | {callers} |")
    print(f"\n{len(rows)} functions", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
