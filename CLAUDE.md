# Fantasy 4X

A Master of Magic-style 4X game in Rust (see `README.md`; design in `docs/design.md`, tasks in
`docs/TASKS.md`).

**Version: 0.0.3** (the one number is `[workspace.package] version` in `Cargo.toml`; a test keeps this line,
the README and `docs/ecs-structure.md` equal to it).


## Current work

For current priority tasks `docs/TASKS.md` 

For low-priority and deferred tasks, see `docs/TASKS-ONHOLD.md`

For long-term goals, see `docs/PLANNED.md`


## Commands

- **Test:** `cargo test --workspace --lib --bins --tests` (skips the examples and doctests, which only cost build time; skips the slow AI behaviour tests and never runs soaks; run those only when asked: `cargo test -p f4x-ai --test it behaviour -- --ignored`, `cargo test -p f4x-learn --test it mirror -- --ignored`, `f4x-tools soak`). The integration tests of a crate are one binary, `tests/it/main.rs` with a `mod` per file, so a crate links one test program, not one per file.
- **Test with nextest:** `cargo nextest run --workspace` runs every test as its own process in parallel (1451 tests in 3.6 s; the slow ones are skipped as above; nextest does not run doctests).
- **Test what changed:** `python3 scripts/test_changed.py` while working (`--dry-run` shows the plan); `cargo test --workspace` before a commit.
- **Lint/format:** `cargo fmt --all` and `cargo clippy --workspace --tests`
- **Scripts:** `scripts/` (table in `docs/scripts.md`)

## Conventions

For js, py and bash scripts, follow `docs/scripting_guidelines.md`

Refer to `docs/ecs-structure.md`

- **Game rules live in `docs/design.md`** and the topic docs it lists, not here. In particular, read
  design.md section 8 before touching anything the AI decides: the rules treat the AI like a human.

- **Tests run on the actual game rules** Avoid separate rules in the test environment to prevent drift.
  Tests should only build situations the game can reach (for example, a unit always has a real player as
  owner; see `docs/ecs-structure.md` section 3.3). A test of a situation the game cannot create in
  normal play should only be used to test error handling (a rejected command, a refused save).

- **Names are singular** for modules, files and types of an entity or a concept (`unit`, `lair`,
  `item`, `UnitView`), also in the other crates. Plural stays for `Vec` fields, registries and `.ron`
  files (`Rules.units`, `units.ron`).
  
- **Player-facing text is never hardcoded**: it is a key in `content/text/en/*.ron` (`docs/text.md`),
  with count, plural and gender forms. A new message adds its key there in the same change. The goal is to support localization.

- **Project lessons** (non-obvious findings of the rework) are in `docs/KNOWLEDGE.md`; add to it when you
  find something like that.

- **Always check what scripts exist** before writing a new one or doing a repetitive job by hand:
  read `docs/scripts.md` (the table of every script in `scripts/`) and list `scripts/`.

- **Log every script in `docs/scripts.md`**, a table with the script's name and what it does. Add the
  row in the same change that adds or changes the script.

- **Never move finished tasks by hand** between `docs/TASKS.md` and `docs/TASKS-DONE.md`. Follow the instructions in these documents.
