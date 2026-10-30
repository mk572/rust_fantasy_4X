#!/usr/bin/env python3
"""Freeze the scripted strategic AI as the reference opponent of `f4x-tools soak-versus`.

Copies `crates/f4x-ai` to `crates/f4x-ai-ref` (replacing the previous reference), renames the
package, and turns off its own tests (the live crate's tests already cover the same code). Edit
`f4x-ai` afterwards; the reference keeps the old behaviour, and plays the neutral cities and
monsters in every versus game.

    python3 scripts/fork_ai.py [--from REF]

`--from REF` freezes the AI as it was at a git ref (a commit or tag) instead of the working tree.
"""
import argparse
import io
import re
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIVE = ROOT / "crates" / "f4x-ai"
REFERENCE = ROOT / "crates" / "f4x-ai-ref"


def copy_tree(ref: str | None) -> None:
    """Put a copy of the live crate's manifest and sources in the reference folder."""
    if REFERENCE.exists():
        shutil.rmtree(REFERENCE)
    REFERENCE.mkdir(parents=True)
    if ref is None:
        shutil.copy(LIVE / "Cargo.toml", REFERENCE / "Cargo.toml")
        shutil.copytree(LIVE / "src", REFERENCE / "src", copy_function=shutil.copy)  # new mtimes, or cargo keeps a stale build
        return
    archive = subprocess.run(
        ["git", "archive", ref, "crates/f4x-ai/Cargo.toml", "crates/f4x-ai/src"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        for member in tar.getmembers():
            member.name = member.name.removeprefix("crates/f4x-ai/")
            if member.name:
                tar.extract(member, REFERENCE, filter="data")


def rewrite_manifest() -> None:
    manifest = REFERENCE / "Cargo.toml"
    text = manifest.read_text()
    text = re.sub(r'name = "f4x-ai"', 'name = "f4x-ai-ref"', text)
    if "[lib]" not in text:
        text = text.replace(
            "\n[dependencies]",
            "\n[lib]\ntest = false\ndoctest = false\n\n[dependencies]",
        )
    manifest.write_text(text)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--from", dest="ref", help="a git ref to freeze instead of the working tree")
    args = parser.parse_args()
    copy_tree(args.ref)
    rewrite_manifest()
    source = f"git ref {args.ref}" if args.ref else "the working tree"
    print(f"crates/f4x-ai-ref is now a copy of f4x-ai from {source}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
