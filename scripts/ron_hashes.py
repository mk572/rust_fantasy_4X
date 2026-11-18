#!/usr/bin/env python3
"""Records and checks the SHA-256 of the `.ron` files of each crate (`content.sha256`).

Why: the content files are the game's settings. A value changed by hand, or by a script that
should not have touched it, must not pass unseen; the test `the_ron_files_are_the_recorded_ones`
of each crate (`f4x_def::content_hash`) fails when a file differs from the record. A change made
on purpose is recorded here and committed with the change, so the history shows it was meant.

What it does:
  * For every crate under `crates/` (not `f4x-ai-ref`) that has `.ron` files, hashes each `.ron`
    file under the crate's folder (not `target/`).
  * Default: compares with the crate's `content.sha256` and lists the files changed, added and
    removed; exit 1 when there is a difference or a crate has `.ron` files and no record.
  * `--update`: writes the records, one line per file, `<hash>  <path from the crate>`, sorted.
    A crate that gets its first record needs the test too (copy `tests/it/content_hash.rs` of
    `f4x-lang`).
"""

import argparse
import hashlib
import os
import sys

CRATES = "crates"
SKIPPED = ("f4x-ai-ref",)
RECORD = "content.sha256"


def hashes_of(crate_dir: str) -> dict[str, str]:
    found = {}
    for folder, dirs, names in os.walk(crate_dir):
        dirs[:] = [d for d in dirs if d != "target"]
        for name in names:
            if name.endswith(".ron"):
                path = os.path.join(folder, name)
                with open(path, "rb") as handle:
                    digest = hashlib.sha256(handle.read()).hexdigest()
                found[os.path.relpath(path, crate_dir).replace(os.sep, "/")] = digest
    return found


def recorded(crate_dir: str) -> dict[str, str] | None:
    path = os.path.join(crate_dir, RECORD)
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf8") as handle:
        rows = (line.rstrip("\n").split("  ", 1) for line in handle if line.strip())
        return {name: digest for digest, name in rows}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--update", action="store_true", help="write the records instead of checking")
    args = parser.parse_args()
    if not os.path.isdir(CRATES):
        sys.exit("run it from the repository root")
    bad = False
    for crate in sorted(os.listdir(CRATES)):
        crate_dir = os.path.join(CRATES, crate)
        if crate in SKIPPED or not os.path.isdir(crate_dir):
            continue
        now = hashes_of(crate_dir)
        if not now:
            continue
        if args.update:
            with open(os.path.join(crate_dir, RECORD), "w", encoding="utf8") as handle:
                handle.writelines(f"{digest}  {name}\n" for name, digest in sorted(now.items()))
            print(f"{crate}: {len(now)} files recorded")
            continue
        then = recorded(crate_dir)
        if then is None:
            print(f"{crate}: has .ron files and no {RECORD}")
            bad = True
            continue
        for name in sorted(now):
            if name not in then:
                print(f"{crate}: added {name}")
            elif then[name] != now[name]:
                print(f"{crate}: changed {name}")
        for name in sorted(set(then) - set(now)):
            print(f"{crate}: removed {name}")
        if then != now:
            bad = True
    if not args.update and not bad:
        print("every .ron file is as recorded")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
