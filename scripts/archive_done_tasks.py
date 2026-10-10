#!/usr/bin/env python3
"""Move finished tasks from docs/TASKS.md to docs/TASKS-DONE.md.

A task counts as done when its table summary contains "(done)" or is struck through
(~~text~~), or when the bold title of its description contains "(done)". Both the table
row and the description move, and the numbers stay as they are. TASKS-DONE.md is kept in
ascending order by number.

    python3 scripts/archive_done_tasks.py [--dry-run] [--tasks FILE] [--done FILE]
"""
import argparse
import re
import sys
from pathlib import Path

ROW = re.compile(r"^\|\s*(\d+)\s*\|")
DESC = re.compile(r"^\*\*(\d+)\.\s")
DONE_HEADER = """# Finished tasks

Moved here from [`TASKS.md`](TASKS.md) by `scripts/archive_done_tasks.py`. Numbers are the
original task numbers. Newest work is merged in ascending order by number.

## Table

| # | Area | Summary |
|---|---|---|

## Descriptions
"""


def is_done(text: str) -> bool:
    return "(done)" in text.lower() or "~~" in text


def split_tasks(lines):
    """Return (kept lines, {number: row}, {number: description lines})."""
    kept, rows, descs = [], {}, {}
    i = 0
    while i < len(lines):
        line = lines[i]
        m = ROW.match(line)
        if m and is_done(line):
            rows[int(m.group(1))] = line
            i += 1
            continue
        m = DESC.match(line)
        if m:
            j = i + 1
            while j < len(lines) and not (DESC.match(lines[j]) or lines[j].startswith("#")):
                j += 1
            block = lines[i:j]
            if is_done(block[0]):
                descs[int(m.group(1))] = [b for b in block]
                while descs[int(m.group(1))] and not descs[int(m.group(1))][-1].strip():
                    descs[int(m.group(1))].pop()
                i = j
                continue
        kept.append(line)
        i += 1
    return kept, rows, descs


def tidy(lines):
    """Drop section headings left without any task, and collapse repeated blank lines."""
    out = []
    for idx, line in enumerate(lines):
        if line.startswith("### "):
            rest = []
            for nxt in lines[idx + 1:]:
                if nxt.startswith("#"):
                    break
                rest.append(nxt)
            if not any(DESC.match(r) for r in rest):
                continue
        if not line.strip() and out and not out[-1].strip():
            continue
        out.append(line)
    return out


def read_done(path: Path):
    if not path.exists():
        return {}, {}
    lines = path.read_text().splitlines()
    rows, descs = {}, {}
    i = 0
    while i < len(lines):
        m = ROW.match(lines[i])
        if m:
            rows[int(m.group(1))] = lines[i]
        m = DESC.match(lines[i])
        if m:
            j = i + 1
            while j < len(lines) and not DESC.match(lines[j]):
                j += 1
            block = lines[i:j]
            while block and not block[-1].strip():
                block.pop()
            descs[int(m.group(1))] = block
            i = j
            continue
        i += 1
    return rows, descs


def write_done(path: Path, rows, descs):
    head, _, _ = DONE_HEADER.partition("| # | Area | Summary |")
    out = [head.rstrip("\n"), "", "| # | Area | Summary |", "|---|---|---|"]
    out += [rows[n] for n in sorted(rows)]
    out += ["", "## Descriptions", ""]
    for n in sorted(descs):
        out += descs[n] + [""]
    path.write_text("\n".join(out).rstrip("\n") + "\n")


def main() -> int:
    docs = Path(__file__).resolve().parent.parent / "docs"
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--tasks", type=Path, default=docs / "TASKS.md")
    ap.add_argument("--done", type=Path, default=docs / "TASKS-DONE.md")
    ap.add_argument("--dry-run", action="store_true", help="only list what would move")
    args = ap.parse_args()

    kept, rows, descs = split_tasks(args.tasks.read_text().splitlines())
    moving = sorted(set(rows) | set(descs))
    if not moving:
        print("nothing to move")
        return 0
    print("moving tasks:", ", ".join(map(str, moving)))
    if set(rows) != set(descs):
        print("warning: table row and description do not both exist for",
              sorted(set(rows) ^ set(descs)), file=sys.stderr)
    if args.dry_run:
        return 0
    done_rows, done_descs = read_done(args.done)
    done_rows.update(rows)
    done_descs.update(descs)
    write_done(args.done, done_rows, done_descs)
    args.tasks.write_text("\n".join(tidy(kept)).rstrip("\n") + "\n")
    print(f"wrote {args.done} and {args.tasks}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
