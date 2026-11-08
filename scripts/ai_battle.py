#!/usr/bin/env python3
"""Run an AI battle that needs the reference AI (`f4x-tools soak-versus`, or `soak` with a reference).

    python3 scripts/ai_battle.py soak-versus --seeds 1..8 --turns 400

Everything after the script name goes to `f4x-tools`. The reference (`crates/f4x-ai-ref`) is
outside the workspace and is never repaired when a change breaks it. Before the battle this
script forks it again from the live AI (`scripts/fork_ai.py`) if it is marked stale
(`crates/f4x-ai-ref/STALE`), or if it no longer builds; then it builds `f4x-tools` with the
`reference` feature (adding the reference to the `f4x-tools` manifest for the build, then
restoring the manifest and `Cargo.lock`) and runs it through `scripts/cool_run.sh` (low priority,
paused while the Mac is under thermal pressure; `AI_BATTLE_NO_COOL=1` runs it plainly) with four games at
once unless `--jobs` says otherwise. A fork makes the reference equal to the live AI, so a battle
right after one compares the AI with itself: freeze the AI before editing it (task 478).
"""
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
COOL_RUN = ROOT / "scripts" / "cool_run.sh"
# Games played at once unless --jobs is given: the machine has ten cores and heats up (docs/learning-ai.md
# section 3 keeps long runs to four).
DEFAULT_JOBS = "4"
REFERENCE = ROOT / "crates" / "f4x-ai-ref"
FORK = ROOT / "scripts" / "fork_ai.py"
MANIFEST = ROOT / "crates" / "f4x-tools" / "Cargo.toml"
LOCK = ROOT / "Cargo.lock"
DEPENDENCY = 'f4x-ai-ref = { path = "../f4x-ai-ref" }'
BUILD = ["cargo", "build", "--release", "-p", "f4x-tools", "--features", "reference"]


def fork_again(reason: str) -> None:
    print(f"ai_battle: forking f4x-ai-ref again ({reason})", flush=True)
    subprocess.run([sys.executable, str(FORK)], cwd=ROOT, check=True)


def build() -> subprocess.CompletedProcess:
    """Build with the reference's dependency line added to `f4x-tools`; the files come back as they were."""
    saved = {path: path.read_bytes() for path in (MANIFEST, LOCK)}
    try:
        text = MANIFEST.read_text().replace("[dependencies]\n", f"[dependencies]\n{DEPENDENCY}\n", 1)
        MANIFEST.write_text(text)
        return subprocess.run(BUILD, cwd=ROOT, capture_output=True, text=True)
    finally:
        for path, content in saved.items():
            path.write_bytes(content)


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    if not (REFERENCE / "Cargo.toml").exists():
        fork_again("there is none yet")
    elif subprocess.run([sys.executable, str(FORK), "--is-stale"], cwd=ROOT).returncode:
        fork_again("marked stale")
    built = build()
    if built.returncode and "f4x-ai-ref" in built.stderr:
        fork_again("it no longer builds")
        built = build()
    sys.stderr.write(built.stderr)
    if built.returncode:
        return built.returncode
    binary = ROOT / "target" / "release" / "f4x-tools"
    if "--jobs" not in args:
        args = [*args, "--jobs", DEFAULT_JOBS]
    # Like every long run: at low priority, paused while the Mac is hot (scripts/cool_run.sh).
    # AI_BATTLE_NO_COOL=1 runs it plainly.
    cool = [] if os.environ.get("AI_BATTLE_NO_COOL") else [str(COOL_RUN)]
    return subprocess.run([*cool, str(binary), *args], cwd=ROOT).returncode


if __name__ == "__main__":
    sys.exit(main())
