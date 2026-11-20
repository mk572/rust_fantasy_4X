#!/usr/bin/env python3
"""Run the whole unit ladder (`f4x-tools duel ladder`) as brackets, several at once.

A bracket is one table: the ladder of one experience level (ladder_L.csv) or the cross-level
fights of one pair of levels (cross_A_B.csv). Each runs as its own `f4x-tools` process through
`cool_run.sh --boost` (full power, the thermal state read only before the start). The brackets of
levels above 6 wait for ladder_6.csv, whose pairs they reuse. A bracket whose file is already in
the output folder is skipped, so a stopped run goes on where it left off. At the end the fit over
all of it is written (`--combine`), without the rating file.
"""

import argparse
import logging
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
COOL_RUN = ROOT / "scripts" / "cool_run.sh"
TOP_LEVEL = 9
LEVEL_6 = 6
DEFAULT_PAUSE_AT = "serious"
POLL_S = 5

EPILOG = """\
Input: a built release binary (cargo build --release -p f4x-tools) and, with --prior, the folder of
an earlier run (its ladder_L.csv and cross_A_B.csv files start each pair's ladder where it ended).
Output: --out gets the files of `f4x-tools duel ladder` (docs/duel.md), one log per bracket in
--out/logs/, and ladder_levels.csv from the final fit. Environment: COOL_PAUSE_AT (default
serious): a bracket waits to start while the thermal state is this or worse.
Example: scripts/ladder_run.py --out duel-out/2026-11-21-ladder --prior duel-out/2026-11-20-ladder-1-6
"""


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog=EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--out", required=True, type=Path, help="output folder (created)")
    parser.add_argument("--prior", type=Path, help="folder of an earlier run to start from")
    parser.add_argument("--brackets", type=int, default=4, help="brackets at once (default 4)")
    parser.add_argument("--jobs", type=int, default=2, help="threads per bracket (default 2)")
    parser.add_argument("--seeds", type=int, default=10, help="fights a step (even, default 10)")
    parser.add_argument(
        "--max-level", type=int, default=TOP_LEVEL, help="highest level (default 9)"
    )
    parser.add_argument(
        "--binary",
        type=Path,
        default=ROOT / "target" / "release" / "f4x-tools",
        help="the f4x-tools binary (default target/release/f4x-tools)",
    )
    parser.add_argument("--dry-run", action="store_true", help="print the brackets and stop")
    args = parser.parse_args(argv)
    if args.seeds < 2 or args.seeds % 2:
        parser.error("--seeds must be even, so that each side moves first as often")
    if not 1 <= args.max_level <= TOP_LEVEL:
        parser.error(f"--max-level is 1 to {TOP_LEVEL}")
    if args.brackets < 1 or args.jobs < 1:
        parser.error("--brackets and --jobs must be at least 1")
    return args


class Bracket:
    """One table of the ladder: its tool arguments, its output file and what it waits for."""

    def __init__(self, name: str, tool_args: list[str], output: str, needs: str | None):
        self.name = name
        self.tool_args = tool_args
        self.output = output
        self.needs = needs


def brackets_for(max_level: int) -> list[Bracket]:
    """Levels up to 6, then the cross-level pairs, then the levels above 6 (they need level 6)."""
    ladder = lambda level: Bracket(
        f"ladder_{level}",
        ["--levels", str(level)],
        f"ladder_{level}.csv",
        f"ladder_{LEVEL_6}.csv" if level > LEVEL_6 else None,
    )
    cross = [
        Bracket(
            f"cross_{a}_{b}",
            ["--cross", f"{a},{b}"],
            f"cross_{a}_{b}.csv",
            None,
        )
        for a in range(1, max_level)
        for b in range(a + 1, max_level + 1)
    ]
    low = [ladder(level) for level in range(1, min(max_level, LEVEL_6) + 1)]
    high = [ladder(level) for level in range(LEVEL_6 + 1, max_level + 1)]
    return low + cross + high


def command_for(bracket: Bracket, args: argparse.Namespace) -> list[str]:
    command = [
        str(COOL_RUN),
        "--boost",
        str(args.binary),
        "duel",
        "ladder",
        *bracket.tool_args,
        "--seeds",
        str(args.seeds),
        "--jobs",
        str(args.jobs),
        "--out",
        str(args.out),
    ]
    if args.prior:
        command += ["--prior", str(args.prior)]
    return command


def pick_runnable(todo: list[Bracket], out: Path) -> Bracket | None:
    """The first bracket whose input file is there."""
    for bracket in todo:
        if bracket.needs is None or (out / bracket.needs).exists():
            return bracket
    return None


def run_all(todo: list[Bracket], args: argparse.Namespace) -> list[str]:
    """Run the brackets, `args.brackets` at a time. Returns the names of those that failed."""
    logs = args.out / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    env = {"COOL_PAUSE_AT": DEFAULT_PAUSE_AT, **os.environ}
    running: dict[str, tuple[subprocess.Popen, object]] = {}
    failed: list[str] = []
    while todo or running:
        while todo and len(running) < args.brackets:
            bracket = pick_runnable(todo, args.out)
            if bracket is None:
                break
            todo.remove(bracket)
            log_file = open(logs / f"{bracket.name}.log", "w")
            process = subprocess.Popen(
                command_for(bracket, args),
                stdout=log_file,
                stderr=subprocess.STDOUT,
                env=env,
            )
            running[bracket.name] = (process, log_file)
            logging.info("started %s", bracket.name)
        time.sleep(POLL_S)
        for name, (process, log_file) in list(running.items()):
            if process.poll() is None:
                continue
            log_file.close()
            del running[name]
            if process.returncode:
                failed.append(name)
            logging.info("%s %s", "FAILED" if process.returncode else "done", name)
        if todo and not running and pick_runnable(todo, args.out) is None:
            raise SystemExit(f"nothing can run, waiting for: {[b.needs for b in todo]}")
    return failed


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    logging.basicConfig(format="%(asctime)s %(message)s", level=logging.INFO)
    todo = [b for b in brackets_for(args.max_level) if not (args.out / b.output).exists()]
    logging.info("%d brackets to run", len(todo))
    if args.dry_run:
        for bracket in todo:
            print(" ".join(command_for(bracket, args)))
        return 0
    if not args.binary.exists():
        raise SystemExit(f"the binary is missing: {args.binary} (cargo build --release -p f4x-tools)")
    args.out.mkdir(parents=True, exist_ok=True)
    failed = run_all(todo, args)
    combine = [str(args.binary), "duel", "ladder", "--combine", f"1-{args.max_level}"]
    combine += ["--out", str(args.out)]
    if subprocess.run(combine, check=False).returncode:
        failed.append("combine")
    if failed:
        logging.error("failed: %s", ", ".join(failed))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
