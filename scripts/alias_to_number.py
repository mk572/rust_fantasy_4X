#!/usr/bin/env python3
"""Rewrites `<registry>.id_of("alias")` and `<registry>.by_name("alias")` in Rust files to the
number the alias stands for in `crates/f4x-def/content/*.ron` (`id_numbered(N)`, `by_num(N)`), so code and
tests refer to a definition by its number (task 467). Only a string literal is rewritten; a
call with a variable is listed. `--dry-run` writes nothing.
"""
import argparse, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FILES = {
    "units": "units", "spells": "spells", "races": "races", "buildings": "buildings",
    "realms": "realms", "retorts": "retorts", "abilities": "abilities", "items": "items",
    "item_powers": "item_powers", "wizards": "wizards", "features": "features",
    "terrains": "terrain",
}

def table(name):
    text = (ROOT / "crates/f4x-def/content" / f"{name}.ron").read_text()
    out = {}
    for m in re.finditer(r'\(\s*(?:num:\s*(\d+),\s*id:\s*"([^"]+)"|id:\s*"([^"]+)",\s*num:\s*(\d+))', text):
        num = m.group(1) or m.group(4)
        alias = m.group(2) or m.group(3)
        out.setdefault(alias, int(num))
    return out

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("paths", nargs="*", default=["crates"])
    a = ap.parse_args()
    tables = {k: table(v) for k, v in FILES.items()}
    pat = re.compile(r'\.(' + "|".join(FILES) + r')(\s*)\.(id_of|by_name)\((\s*)&?"([^"]+)"(\s*)\)')
    left = 0
    changed = 0
    for p in a.paths:
        for f in sorted((ROOT / p).rglob("*.rs")):
            src = f.read_text()
            def sub(m):
                nonlocal changed
                kind, ws, fn, _, alias, _ = m.groups()
                num = tables[kind].get(alias)
                if num is None:
                    print(f"{f}: no {kind} alias {alias}", file=sys.stderr)
                    return m.group(0)
                changed += 1
                return f".{kind}{ws}.{'id_numbered' if fn == 'id_of' else 'by_num'}({num})"
            new = pat.sub(sub, src)
            if new != src and not a.dry_run:
                f.write_text(new)
    print(f"{changed} calls rewritten")

main()
