#!/usr/bin/env python3
"""Add the original's item picture number to every row of content/items.ron.

Reads ITEMDATA.LBX (250 records of 56 bytes after a 4-byte header; the picture number of
ITEMS.LBX is the byte at offset 30 of a record) and writes `icon: N` after `cost: N` on each
row of the items file, in the file's order. Rows that already carry an icon are left alone, so
it can be run again. The importer no longer writes items.ron (task 244); this is the one
reading of the picture numbers (task 261).
"""
import argparse
import re
import struct
import sys

DEFAULT_LBX = "reference/mom-classic/Master of Magic Official Release/ITEMDATA.LBX"
DEFAULT_RON = "content/items.ron"


def read_icons(path):
    data = open(path, "rb").read()
    count, magic = struct.unpack_from("<HH", data, 0)
    if magic != 0xFEAD:
        sys.exit(f"{path}: not an LBX file")
    offsets = struct.unpack_from(f"<{count + 1}I", data, 8)
    entry = data[offsets[0]:offsets[1]]
    n, size = struct.unpack_from("<HH", entry, 0)
    rows = []
    for i in range(n):
        rec = entry[4 + i * size:4 + (i + 1) * size]
        name = rec[:30].split(b"\0")[0].decode("latin-1")
        rows.append((name, rec[30]))
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--lbx", default=DEFAULT_LBX, help="path of ITEMDATA.LBX")
    ap.add_argument("--ron", default=DEFAULT_RON, help="path of content/items.ron")
    ap.add_argument("--dry-run", action="store_true", help="report, change nothing")
    args = ap.parse_args()

    icons = read_icons(args.lbx)
    lines = open(args.ron, encoding="utf-8").read().split("\n")
    out, row, changed = [], 0, 0
    for line in lines:
        m = re.match(r'(\s*\(id: "[^"]*", name: ")([^"]*)(".*?cost: \d+,)(.*)$', line)
        if not m:
            out.append(line)
            continue
        name, icon = icons[row]
        if not m.group(2).startswith(name.rstrip(" ")) and not name.startswith(m.group(2)):
            sys.exit(f"row {row}: file says {name!r}, items.ron says {m.group(2)!r}")
        row += 1
        if " icon:" in line:
            out.append(line)
            continue
        out.append(f"{m.group(1)}{m.group(2)}{m.group(3)} icon: {icon},{m.group(4)}")
        changed += 1
    if row != len(icons):
        sys.exit(f"{row} rows in items.ron, {len(icons)} records in the file")
    print(f"{changed} rows get an icon, {row - changed} already had one")
    if not args.dry_run:
        open(args.ron, "w", encoding="utf-8").write("\n".join(out))


if __name__ == "__main__":
    main()
