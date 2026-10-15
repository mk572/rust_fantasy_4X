# Scripts

Every script in `scripts/`, with what it does. Run any of them with `--help` for its options. Add a
row when you add or change a script (see `CLAUDE.md`).

| Script | What it does |
|---|---|
| `archive_done_tasks.py` | Moves finished tasks (summary or title marked "(done)", or struck through) from `docs/TASKS.md` to `docs/TASKS-DONE.md`, keeping the numbers and the done list in ascending order. `--dry-run` lists what would move. |
| `cool_run.sh` | Runs a long command (training, soak runs) at low priority and pauses it while the Mac is under thermal pressure; `--state` prints the thermal state. Builds `thermal_state` when missing. |
| `fix_wrapper_calls.py` | After deleting `Game` wrapper methods, rewrites their callers to go through `game.units()` or `game.economy()`, driven by the compiler's "no method named" errors (task 198). |
| `import_mom.py` | Imports Master of Magic data from the author's own copy of the game and a ReMoM clone into `content/` (rules data, committed) and `content-local/` (help text and other copyrighted prose, never committed); writes an import report. It does not write the spells (`content/spells.ron` and `spell_abilities.ron` are kept by hand). |
| `list_ogbugs.py` | Indexes the `OGBUG` comments in the ReMoM clone (places where the original departs from its manual) into `content-local/`, as a worklist for `docs/help-vs-implementation-conflicts.md`. |
| `make_app.sh` | Builds the local macOS app `target/Fantasy4X.app` (release build of `f4x-client` plus a copy of `content/`, `content-local/` and `assets-local/art`). For this machine only: it carries the local-only art and text. |
| `thermal_state.swift` | Prints the macOS thermal pressure level (nominal, fair, serious, critical, or unknown); built to `thermal_state` and used by `cool_run.sh`. |
