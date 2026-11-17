#!/usr/bin/env python3
"""List the screen pictures that no run of the client read: candidates for `unused.tsv`.

Run the client with `F4X_TRACE_ART=FILE` (every picture file it reads is appended to FILE) over the
screenshot flags and a play session, then `scripts/unused_art.py FILE [FILE...]`. A picture of
`assets-local/index.tsv` placed under `screens/` is listed when no file of it was read and the
client's source has no `"lbx", entry` for it. The list is only a start: pictures chosen by a
computed index (icons, buildings, spell effects) show up as unread when no run reached them, so each
name is checked by hand before it goes into `crates/f4x-client/extract/unused.tsv`.
"""
import re
import sys
from pathlib import Path


import argparse

# Parsed before the heavy imports below, so `--help` and a wrong option never run the script.
if __name__ == "__main__":
    _parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for _name, _kw in (
    ("trace", dict(nargs="+", metavar="FILE", help="a file written with F4X_TRACE_ART=FILE by a client run")),
    ):
        _parser.add_argument(_name, **_kw)
    _parser.parse_args()

def main() -> None:
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    read = set()
    for trace in sys.argv[1:]:
        for line in Path(trace).read_text().splitlines():
            m = re.match(r"\.\./(.*)_\d\d\.png$", line.strip())
            if m:
                read.add(m.group(1))
    source = "".join(p.read_text() for p in Path("crates/f4x-client/src").rglob("*.rs"))
    for line in Path("assets-local/index.tsv").read_text().splitlines():
        lbx, entry, place = line.split("\t")
        if not place.startswith("screens/") or place in read:
            continue
        if re.search(rf'"{lbx}"\s*,\s*{entry}\b', source):
            continue
        print(f"{lbx}\t{entry}\t{entry}\t{place.split('/', 1)[1]}")


if __name__ == "__main__":
    main()
