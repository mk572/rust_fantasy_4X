# Fantasy 4X

A Master of Magic-style 4X game in Rust (see `README.md`; design in `docs/design.md`, tasks in
`docs/TASKS.md`).

## Conventions

Follow `~/projects/CODING_GUIDELINES.md`. Project-specific rules:

- **Always check what scripts exist** before writing a new one or doing a repetitive job by hand:
  read `docs/scripts.md` (the table of every script in `scripts/`) and list `scripts/`.
- **Log every script in `docs/scripts.md`**, a table with the script's name and what it does. Add the
  row in the same change that adds or changes the script.
- **Every script takes help arguments in the standard format of its language**: `-h` and `--help`
  (Python: `argparse`; shell: a `case` on `-h|--help` that prints `usage()`; Swift and others: the
  same two spellings), printed to stdout with exit 0, without running the script, and an unknown
  option fails with exit 2 (`~/projects/CODING_GUIDELINES.md`, "Scripts (any language): `--help`" and
  "unknown options fail"). A header comment alone is not `--help`. A new script has it from the
  start; a script you edit is brought into line in the same change.
- **Never move finished tasks by hand** between `docs/TASKS.md` and `docs/TASKS-DONE.md`. Mark the
  task "(done)" in `TASKS.md` (table row and description title), then run
  `python3 scripts/archive_done_tasks.py` (`--dry-run` first lists what would move). The script keeps
  `TASKS-DONE.md` in ascending order by number.

## Commands

- **Test:** `cargo test --workspace`
- **Lint/format:** `cargo fmt --all` and `cargo clippy --workspace --tests`
- **Scripts:** `scripts/` (table in `docs/scripts.md`)
