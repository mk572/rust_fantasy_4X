#!/usr/bin/env python3
"""Run an AI battle that needs the reference AI (`f4x-tools soak-versus`, or `soak` with a reference).

    python3 scripts/ai_battle.py soak-versus --seeds 1..8 --turns 400

Everything after the script name goes to `f4x-tools`. The reference (`crates/f4x-ai-ref`) is
outside the workspace and is never repaired when a change breaks it. Before the battle this
script forks it again from the live AI (`scripts/fork_ai.py`) if it is marked stale
(`crates/f4x-ai-ref/STALE`), or if it no longer builds; then it builds `f4x-tools` with the
`reference` feature and runs it. A fork makes the reference equal to the live AI, so a battle
right after one compares the AI with itself: freeze the AI before editing it (task 478).
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FORK = ROOT / "scripts" / "fork_ai.py"
BUILD = ["cargo", "build", "--release", "-p", "f4x-tools", "--features", "reference"]


def fork_again(reason: str) -> None:
    print(f"ai_battle: forking f4x-ai-ref again ({reason})", flush=True)
    subprocess.run([sys.executable, str(FORK)], cwd=ROOT, check=True)


def build() -> subprocess.CompletedProcess:
    return subprocess.run(BUILD, cwd=ROOT, capture_output=True, text=True)


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    if subprocess.run([sys.executable, str(FORK), "--is-stale"], cwd=ROOT).returncode:
        fork_again("marked stale")
    built = build()
    if built.returncode and "f4x-ai-ref" in built.stderr:
        fork_again("it no longer builds")
        built = build()
    sys.stderr.write(built.stderr)
    if built.returncode:
        return built.returncode
    binary = ROOT / "target" / "release" / "f4x-tools"
    return subprocess.run([str(binary), *args], cwd=ROOT).returncode


if __name__ == "__main__":
    sys.exit(main())
