# Fantasy 4X

A Master of Magic-style 4X game in Rust (see `README.md`; design in `docs/design.md`, tasks in
`docs/TASKS.md`).

**Version: 0.0.8** (the one number is `[workspace.package] version` in `Cargo.toml`; a test keeps this line,
the README and `docs/ecs-structure.md` equal to it).


## Current work

For the open tasks and their priorities (High, Medium, Low) see `docs/TASKS.md`

Long-term ideas are Low-priority tasks in the same file.


## Where things are

The crates and who may depend on whom: `docs/design.md` section 2 and `docs/rust_coding_rules.md`. The world (`f4x-core`) says what is in the game and what can be done; the game (`f4x-master`) advances time (`docs/turn-structure.md` section 10). The state model: `docs/ecs-structure.md`. Lessons: `docs/KNOWLEDGE.md`.

## Commands

- **Test:** `cargo test --workspace --exclude f4x-learn --lib --bins --tests` (skips `f4x-learn`, which is stale, and the examples and doctests, which only cost build time; skips the slow AI behaviour tests and never runs soaks; run those only when asked: `cargo test -p f4x-ai --test it behaviour -- --ignored`, `cargo test -p f4x-learn --test it mirror -- --ignored`, `f4x-tools soak`). The integration tests of a crate are one binary, `tests/it/main.rs` with a `mod` per file, so a crate links one test program, not one per file.
- **Deep tests:** the normal run tries the ends of a range and its default; `scripts/test_deep.sh` (`F4X_DEEP=1`) tries every value, every world, every damage, to find where something breaks (minutes), and `scripts/test_deep.sh --mutants CRATE FILE...` runs cargo-mutants (hours). A test that sweeps takes its values from `f4x_def::testdir::{deep, spread, sample}` (`docs/f4x-core-test-design.md`, "Normal and deep runs"); a failure message of such a test says to rerun deep.
- **Test with nextest:** `cargo nextest run --workspace --exclude f4x-learn` runs every test as its own process in parallel (about 2150 tests in a few seconds; the slow ones are skipped as above; nextest does not run doctests).
- **Test what changed:** `python3 scripts/test_changed.py` while working (`--dry-run` shows the plan); `cargo test --workspace --exclude f4x-learn` before a commit.
- **Soaks and other long runs** go through `scripts/cool_run.sh COMMAND...` (low priority, paused while the Mac is hot, `docs/learning-ai.md` section 3) with at most 4 jobs: `scripts/cool_run.sh target/release/f4x-tools soak --jobs 4 ...`. `scripts/ai_battle.py` does it by itself.
- **Duels:** `target/release/f4x-tools duel pairs` (every unit with an attack or defence rule against every other, one against one) and `duel teams` (random groups): real battles played by the battle AI, with invariants checked and the win rate of each unit (`--csv FILE`); run it after changing battle rules (about a second); see `docs/duel.md`.
- **Lint/format:** `cargo fmt --all` and `cargo clippy --workspace --tests`
- **Scripts:** `scripts/` (table in `docs/scripts.md`)

## Conventions

For js, py and bash scripts, follow [`docs/scripting_guidelines.md`](docs/scripting_guidelines.md); read it before writing, changing or running a script (every script has `--help`, and `--help` never runs it).

Refer to `docs/ecs-structure.md`

- **Game rules live in `docs/design.md`** and the topic docs it lists, not here. In particular, read
  design.md section 8 before touching anything the AI decides: the rules treat the AI like a human.

- **Rust and testing rules** (safe coding, error handling, testing on the actual game data and rules,
  error handling and quality of tests) are in `docs/rust_coding_rules.md`; follow them when writing or
  changing code and tests. A test never carries a game rule of its own (a limit, a count, a price): it reads
  it from the game (`w.max_tile()`, the shipped content); `scripts/imaginary_tests.py` flags the offenders.

- **The `.ron` content is hash-guarded**: each crate with `.ron` files has a `content.sha256` and a test
  that fails when a file differs. After a change of content made on purpose, run
  `python3 scripts/ron_hashes.py --update` and commit the record with it. Never update the record to
  silence the test without knowing who changed the file.
  A change of game behaviour (content or code) also moves the pinned digests of the scripted plays
  (`crates/f4x-core/tests/pins/core_drift.txt`, `crates/f4x-master/tests/pins/drift.txt`): the failure names
  the fields that moved and the command that refreshes that file; if the move is meant, run it
  (`F4X_REFRESH_PINS=1 cargo test ...`, `docs/f4x-core-test-design.md` class 8) and review the diff.

- **Names are singular** for modules, files and types of an entity or a concept (`unit`, `lair`,
  `item`, `UnitView`), also in the other crates. Plural stays for `Vec` fields, registries and `.ron`
  files (`Rules.units`, `units.ron`).
  
- **Player-facing text is never hardcoded**: it is a key in `crates/f4x-client/content/text/en/*.ron` (`docs/text.md`),
  with count, plural and gender forms. A new message adds its key there in the same change. The goal is to support localization.

- **Project lessons** (non-obvious findings of the rework) are in `docs/KNOWLEDGE.md`; add to it when you
  find something like that.

- **`crates/f4x-ai-ref` is never repaired** (one exception, 19 November 2026, ordered by the project owner: `docs/ai-design.md` 8.1a). It is outside the workspace; if a change breaks it, run
  `python3 scripts/fork_ai.py --mark-stale` and go on. `scripts/ai_battle.py` never forks it itself: it stops and asks for a fork by hand (`python3 scripts/fork_ai.py --from REF --replace`) before the
  next AI battle (`docs/ai-design.md` 8.1a).

- **Always check what scripts exist** before writing a new one or doing a repetitive job by hand:
  read `docs/scripts.md` (the table of every script in `scripts/`) and list `scripts/`.

- **Log every script in `docs/scripts.md`**, a table with the script's name and what it does. Add the
  row in the same change that adds or changes the script.

- **Never move finished tasks by hand** between `docs/TASKS.md` and `docs/TASKS-DONE.md`. Follow the instructions in these documents.
