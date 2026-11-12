#!/usr/bin/env python3
"""Late-game speed guard: time the soak from fixed late-game snapshots and fail when a game's
cost per turn goes over its limit (task 512).

    scripts/perf_guard.py                run the benchmarks and compare with the limits
    scripts/perf_guard.py --make-bench   (re)build the snapshots first (once, or after a rules change)
    scripts/perf_guard.py -h | --help

The benchmarks are snapshots of seed 25 at turn 400 (2 and 5 wizards, about 700 and 1,700
entities), kept in soak-out/bench/ (local, gitignored). The game is deterministic, so the same
code on the same snapshot always plays the same turns; only the clock varies. The window is turns
401 to 440. It runs through `cool_run.sh --timing`: it waits for a cool Mac, runs at normal priority (the plain
mode would put the process on the efficiency cores, about six times slower, which the limits do not
allow for) and warns when the Mac got hot during the run.

Exit status: 0 within the limits, 1 over a limit, 2 for a usage error or a missing snapshot.
"""
import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOOL = ROOT / "target" / "release" / "f4x-tools"
COOL_RUN = [str(ROOT / "scripts" / "cool_run.sh"), "--timing"]
BENCH_DIR = ROOT / "soak-out" / "bench"
SEED = 25
SNAPSHOT_TURN = 400
LAST_TURN = 440
EVERY = 20
# Limit in ms a turn (mean over the window), about 1.5 times what the code took on 13 November 2026
# (2 wizards 57 ms, 5 wizards 130 ms). Lower a limit when a speedup lands; raise it only with a reason.
LIMITS_MS = {2: 90, 5: 200}

LINE = re.compile(r"turn (\d+): (.*?) \(\d+ entities\) \[[0-9.]+s, (\d+) ms a turn\]")
PHASES = re.compile(r"ms a turn by phase: (.*)")
UNITS = re.compile(r"/(\d+)u")


def snapshot_path(players: int) -> Path:
    return BENCH_DIR / f"p{players}" / f"seed{SEED}" / f"t{SNAPSHOT_TURN:04d}.f4x"


def make_bench() -> None:
    for players in LIMITS_MS:
        out = BENCH_DIR / f"p{players}"
        cmd = [str(TOOL), "soak", "--seeds", f"{SEED}..{SEED}", "--players", str(players),
               "--turns", str(SNAPSHOT_TURN), "--snapshots-every", "200", "--out", str(out)]
        print("making", out, flush=True)
        subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL)


def run_bench(players: int) -> dict:
    """Play turns 401 to LAST_TURN from the snapshot; return the mean ms a turn, the phases and units."""
    cmd = [*COOL_RUN, str(TOOL), "soak", "--from", str(snapshot_path(players)), "--players", str(players),
           "--turns", str(LAST_TURN), "--every", str(EVERY)]
    done = subprocess.run(cmd, check=True, capture_output=True, text=True)
    if "WARNING" in done.stderr:
        print(done.stderr.strip(), file=sys.stderr)
    text = done.stdout
    totals, units, phases = [], [], {}
    for line in text.splitlines():
        if (m := LINE.search(line)):
            totals.append(int(m.group(3)))
            units.append(sum(int(n) for n in UNITS.findall(m.group(2))))
        elif (m := PHASES.search(line)):
            for part in m.group(1).split(", "):
                name, ms = part.rsplit(" ", 1)
                phases.setdefault(name, []).append(int(ms))
    if not totals:
        raise SystemExit(f"no timing lines in the soak output for {players} wizards:\n{text}")
    return {"ms": sum(totals) / len(totals), "units": units[-1],
            "phases": {n: sum(v) / len(v) for n, v in phases.items() if sum(v) > 0}}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--make-bench", action="store_true", help="build the snapshots first")
    args = parser.parse_args()
    if not TOOL.exists():
        print(f"{TOOL} is missing: cargo build --release -p f4x-tools", file=sys.stderr)
        return 2
    if args.make_bench:
        make_bench()
    missing = [p for p in LIMITS_MS if not snapshot_path(p).exists()]
    if missing:
        print(f"no snapshot for {missing} wizards: run scripts/perf_guard.py --make-bench",
              file=sys.stderr)
        return 2
    failed = False
    for players, limit in LIMITS_MS.items():
        r = run_bench(players)
        verdict = "ok" if r["ms"] <= limit else "OVER"
        failed |= verdict == "OVER"
        phases = ", ".join(f"{n} {ms:.0f}" for n, ms in r["phases"].items())
        print(f"{players} wizards, {r['units']} units at turn {LAST_TURN}: {r['ms']:.0f} ms a turn "
              f"(limit {limit}) {verdict}\n    by phase: {phases}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
