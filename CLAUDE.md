# Fantasy 4X

**Version: 0.0.2** (the one number is `[workspace.package] version` in `Cargo.toml`; a test keeps this line,
the README and `docs/ecs-structure.md` equal to it). Saves and replays of another version are refused.

A Master of Magic-style 4X game in Rust (see `README.md`; design in `docs/design.md`, tasks in
`docs/TASKS.md`).

## Current work: the entity rework (tasks 307 to 342)

Tasks 307 to 310 are done (version 0.0.2, singular names, the layout by game concept, the design).
Before touching anything else read the section "The rework" at the top of `docs/TASKS.md` (rules,
phases, dependencies) and `docs/ecs-structure.md` section 3 (the entity model). Do the open tasks in
the order of their dependencies. Old code and docs describe the code as it was until a task replaces them.

## Conventions

Follow `~/projects/CODING_GUIDELINES.md`. Project-specific rules:

- **Game rules live in `docs/design.md`** and the topic docs it lists, not here. In particular, read
  design.md section 8 before touching anything the AI decides: the rules treat the AI like a human.
- **Tests only build situations the game can reach** (for example, a unit always has a real player as
  owner; see `docs/ecs-structure.md` section 3.3). A test of a situation the game cannot create in
  normal play is allowed only to test error handling (a rejected command, a refused save).
- **Names are singular** for modules, files and types of an entity or a concept (`unit`, `lair`,
  `item`, `UnitView`), also in the other crates. Plural stays for `Vec` fields, registries and `.ron`
  files (`Rules.units`, `units.ron`).
- **Player-facing text is never hardcoded**: it is a key in `content/text/en/*.ron` (`docs/text.md`),
  with count, plural and gender forms. A new message adds its key there in the same change.
- **Project lessons** (non-obvious findings of the rework) are in `docs/KNOWLEDGE.md`; add to it when you
  find something like that.
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

- **Test:** `cargo test --workspace` (skips the slow AI behaviour tests and never runs soaks; run those only when asked: `cargo test -p f4x-ai --test behaviour -- --ignored`, `cargo test -p f4x-learn --test mirror -- --ignored`, `f4x-tools soak`)
- **Lint/format:** `cargo fmt --all` and `cargo clippy --workspace --tests`
- **Scripts:** `scripts/` (table in `docs/scripts.md`)
