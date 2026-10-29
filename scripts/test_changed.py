#!/usr/bin/env python3
"""Runs the tests that the changes since a git ref can break, instead of the whole workspace.

What it does:
  * Changed files come from `git diff` against REF (default HEAD) plus untracked files.
  * A change in a crate runs that crate and the crates that depend on it.
  * A change in a f4x-core module runs that module's tests and the tests of the modules that
    mention it (`crate::module` in their source), not the other core modules.
  * The slow tests (SLOW below, seconds each in a debug build) run only when their area changed.
  * Content (`content/`), `Cargo.toml` and `Cargo.lock` run everything; docs and scripts alone run
    only the checks that read them (version and method-count tests).
  * `--all` is the whole `cargo test --workspace` plus the slow tests: run it before a commit.
The module graph of f4x-core is tightly connected (game, rules, map, unit, wizard, player and city
are used nearly everywhere), so a change there still selects most of the core tests. The saving is
mainly the slow tests of f4x-console and f4x-learn, and the crates the change cannot reach.
"""

import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CORE_SRC = "crates/f4x-core/src"

# Slow tests, by crate: (test name, the areas whose change makes it run). An area is a f4x-core
# module name, or a crate name for a change in that crate's own files.
SLOW = {
    "f4x-console": [
        ("a_neutral_city_whose_garrison_is_gone_trains_a_new_one", {"city", "feature", "unit", "f4x-ai"}),
        ("neutral_cities_keep_their_garrisons_for_sixty_turns", {"city", "feature", "unit", "f4x-ai"}),
        ("neutral_cities_play_by_the_ordinary_rules", {"city", "feature", "unit", "f4x-ai"}),
        ("the_legal_casts_are_the_casts_the_game_accepts", {"spell", "wizard", "battle"}),
        ("rivals_without_ready_made_wizards_draw_a_name_from_their_list", {"wizard", "player"}),
        ("more_city_damage_means_more_lost_citizens_and_buildings", {"spell", "city"}),
    ],
    "f4x-learn": [
        ("random_play_finishes_generated_battles_without_a_refused_command", {"battle", "spell", "unit", "hero", "item"}),
        ("a_random_agent_playing_by_the_masks_is_never_refused", {"battle", "spell", "unit", "hero", "item"}),
        ("fleeing_saves_only_the_units_that_survive_the_flight", {"battle", "unit"}),
    ],
}

EVERYTHING = ("content/", "Cargo.toml", "Cargo.lock", "content-local/")
DOC_CHECKS = ("docs/", "README.md", "CLAUDE.md")


def sh(cmd):
    return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, check=True).stdout


def changed_files(ref):
    files = set(sh(["git", "diff", "--name-only", ref]).split())
    files |= set(sh(["git", "ls-files", "--others", "--exclude-standard"]).split())
    return sorted(files)


def crate_graph():
    """{crate: set of workspace crates it depends on}, from cargo metadata."""
    meta = json.loads(sh(["cargo", "metadata", "--format-version", "1", "--no-deps"]))
    names = {p["name"] for p in meta["packages"]}
    return {
        p["name"]: {d["name"] for d in p["dependencies"] if d["name"] in names}
        for p in meta["packages"]
    }


def dependents(graph, crate):
    out, todo = {crate}, [crate]
    while todo:
        c = todo.pop()
        for name, deps in graph.items():
            if c in deps and name not in out:
                out.add(name)
                todo.append(name)
    return out


def core_modules():
    base = os.path.join(ROOT, CORE_SRC)
    mods = set()
    for name in os.listdir(base):
        if os.path.isdir(os.path.join(base, name)):
            mods.add(name)
        elif name.endswith(".rs") and name != "lib.rs":
            mods.add(name[:-3])
    return mods


def core_referrers(mods):
    """{module: modules whose source mentions crate::module}, one level."""
    base = os.path.join(ROOT, CORE_SRC)
    uses = {m: set() for m in mods}
    for m in mods:
        paths = []
        if os.path.isdir(os.path.join(base, m)):
            for r, _, fs in os.walk(os.path.join(base, m)):
                paths += [os.path.join(r, f) for f in fs if f.endswith(".rs")]
        if os.path.exists(os.path.join(base, m + ".rs")):
            paths.append(os.path.join(base, m + ".rs"))
        for p in paths:
            text = open(p).read()
            found = set(re.findall(r"crate::(\w+)", text))
            for block in re.findall(r"crate::\{([^;]*?)\};", text, re.S):
                found |= set(re.findall(r"\b(\w+)(?:::|,|\s|$)", block))
            uses[m] |= (found & mods) - {m}
    refs = {m: set() for m in mods}
    for m, used in uses.items():
        for x in used:
            refs[x].add(m)
    return refs


def plan(files, run_all, with_slow):
    """The cargo commands to run, as (label, argv)."""
    graph = crate_graph()
    cmds = []
    if run_all or any(f.startswith(EVERYTHING) for f in files):
        return [("everything", ["cargo", "test", "--workspace"])]

    crates, areas, core_mods = set(), set(), set()
    unmapped = False
    mods = core_modules()
    for f in files:
        m = re.match(r"crates/([^/]+)/", f)
        if not m:
            continue
        crate = m.group(1)
        crates |= dependents(graph, crate)
        areas.add(crate)
        if f.startswith(CORE_SRC + "/"):
            top = f[len(CORE_SRC) + 1 :].split("/")[0].removesuffix(".rs")
            if top in mods:
                core_mods.add(top)
                areas.add(top)
            else:
                unmapped = True  # lib.rs and the like: no module to narrow to
        elif crate == "f4x-core":
            crates.add("f4x-core")  # tests/ files or Cargo changes in core

    if any(f.startswith(DOC_CHECKS) or f.startswith("scripts/") for f in files):
        crates.add("f4x-core")  # the version and method-count tests read docs and README

    if "f4x-core" in crates:
        refs = core_referrers(mods)
        wanted = set(core_mods)
        for m in core_mods:
            wanted |= refs[m]
        filters = [] if unmapped or not core_mods else sorted(f"{m}::" for m in wanted)
        if core_mods or unmapped or any(f.startswith("crates/f4x-core/") for f in files) or len(crates) > 1:
            cmds.append(
                ("f4x-core " + (" ".join(filters) if filters else "(all)"), ["cargo", "test", "-p", "f4x-core", "--lib", "--"] + filters)
            )
        cmds.append(("f4x-core tests/", ["cargo", "test", "-p", "f4x-core", "--test", "it"]))
        crates.discard("f4x-core")

    for crate in sorted(crates):
        skip = []
        for name, triggers in SLOW.get(crate, []):
            if with_slow or areas & triggers:
                continue
            skip += ["--skip", name]
        cmds.append((crate + (" (slow tests skipped)" if skip else ""), ["cargo", "test", "-p", crate, "--lib", "--bins", "--tests", "--"] + skip))
    return cmds


def main():
    ap = argparse.ArgumentParser(
        description="Run the tests that the changes since a git ref can break (see the file header).",
    )
    ap.add_argument("--since", default="HEAD", metavar="REF", help="compare with this git ref (default HEAD; the worktree and untracked files count)")
    ap.add_argument("--all", action="store_true", help="the whole workspace and the slow tests (before a commit)")
    ap.add_argument("--slow", action="store_true", help="also run the slow tests of the affected crates")
    ap.add_argument("--dry-run", action="store_true", help="print the plan, run nothing")
    args = ap.parse_args()

    files = changed_files(args.since)
    if not files and not args.all:
        print("no changes since", args.since)
        return 0
    cmds = plan(files, args.all, args.slow)
    if not cmds:
        print("changes need no tests:", ", ".join(files))
        return 0
    failed = []
    for label, argv in cmds:
        print(f"== {label}\n   {' '.join(argv)}", flush=True)
        if args.dry_run:
            continue
        if subprocess.run(argv, cwd=ROOT).returncode != 0:
            failed.append(label)
    if failed:
        print("FAILED:", "; ".join(failed))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
