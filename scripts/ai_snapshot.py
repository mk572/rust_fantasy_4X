#!/usr/bin/env python3
"""Measure a change to the scripted AI against a snapshot of it, then drop the snapshot.

    python3 scripts/ai_snapshot.py begin [--from REF]   # freeze the AI (default: the last commit)
    # ... edit crates/f4x-ai, then play it against the snapshot:
    python3 scripts/ai_battle.py soak-versus --seeds 1..8 --turns 400
    python3 scripts/ai_snapshot.py end --keep new        # keep the edited AI
    python3 scripts/ai_snapshot.py end --keep old        # put the snapshot's AI back

`begin` forks `crates/f4x-ai` as it was at REF (`scripts/fork_ai.py --from`) into the local,
git-ignored `crates/f4x-ai-ref` and records REF. `end` removes the copy; with `--keep old` it first
restores `crates/f4x-ai/src` from the recorded REF. A fixed long-term baseline is not kept: tag a
commit and `begin --from TAG` to compare against it later.
"""
import argparse
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FORK = ROOT / "scripts" / "fork_ai.py"
REFERENCE = ROOT / "crates" / "f4x-ai-ref"
RECORD = REFERENCE / "SNAPSHOT_REF"


def begin(ref: str) -> int:
    sha = subprocess.run(
        ["git", "rev-parse", ref], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout.strip()
    subprocess.run([sys.executable, str(FORK), "--from", sha], cwd=ROOT, check=True)
    RECORD.write_text(sha + "\n")
    print(f"snapshot of f4x-ai at {sha[:10]}; edit crates/f4x-ai, then run scripts/ai_battle.py")
    return 0


def end(keep: str) -> int:
    if not RECORD.exists():
        print("no snapshot: run `ai_snapshot.py begin` first", file=sys.stderr)
        return 1
    sha = RECORD.read_text().strip()
    if keep == "old":
        subprocess.run(
            ["git", "checkout", sha, "--", "crates/f4x-ai/src"], cwd=ROOT, check=True
        )
        print(f"crates/f4x-ai/src restored from {sha[:10]}")
    shutil.rmtree(REFERENCE)
    print("snapshot removed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    start = commands.add_parser("begin")
    start.add_argument("--from", dest="ref", default="HEAD")
    finish = commands.add_parser("end")
    finish.add_argument("--keep", choices=["new", "old"], required=True)
    args = parser.parse_args()
    return begin(args.ref) if args.command == "begin" else end(args.keep)


if __name__ == "__main__":
    sys.exit(main())
