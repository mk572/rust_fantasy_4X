# Rust Coding Rules

## Safe coding

Unsafe code is only allowed in the test environment.

Production code (the game) should avoid unsafe code wherever feasible.
Current unsafe code should be replaced.

Implement error handling according to good Rust coding practices.
Avoid unwrap. Avoid panics. (Tests have their own rules, see "Error handling in tests" below.)

Clippy's `unwrap_used` and `expect_used` lints keep unwraps out of production code; tests may allow
them with `#![cfg_attr(test, allow(clippy::unwrap_used))]`.

## Resource efficiency

Avoid cloning, especially on large entities.

# Basic Structure

The game is built on four major components: rules engine, game engine, client and ai.

The crates depend on each other in one direction: `f4x-def` is at the bottom (the static definitions,
no other game crate); `f4x-core` depends on `f4x-def`; `f4x-data` and `f4x-ai` depend on `f4x-core`
(`f4x-ai` uses `f4x-data` only in its tests); `f4x-console` depends on core, data and ai; `f4x-learn`
on core and data; `f4x-client` and `f4x-tools` on all of these (`f4x-tools` also on `f4x-ai-ref`).
`f4x-data` is being folded into `f4x-def` (task 481): a crate loads the data it holds. `f4x-core`
reads and writes save files only (`game/save/file.rs`); it is deterministic otherwise.

## Rules engine

The rules engine is static and implements the rules of the game.
It reads game rules and content and provides it to the game engine.
For example, unit stats and abilities, buildings, spells, terrain etc. are all provided via the rules engine.

The rules engine is split over two crates:

- **f4x-def** holds the types of the rules (`UnitDef`, `SpellDef`, `Effect`, `Stat`, the registries,
  the terrain and feature definitions). Core re-exports them under their old paths (`rules`,
  `map::terrain`, `feature::def`) until task 481 stage 4 points every crate at `f4x-def`.
- **f4x-core, `rules/exec.rs`** applies the effects; `Content` is still in core (`content.rs`).
- **f4x-data** loads the files in `content/` (`.ron`), validates them and builds core's types from
  them (`load_content`). It holds no rules of its own.

## Game engine

The game engine is dynamic. It manages the flow of the game.
Starting a game, setting up a wizard, turn order, turn flow, combat, build orders, 
spellcasting, save and load etc. are all handled by the game engine.
The game engine sits in f4x-core.

## Client

The client handles the user interface.
It creates screens and menus, displays graphical assets, plays sounds, takes user input etc.
The client sits in f4x-client.

## AI

The AI runs the non-human players. It interacts directly with the game engine.
The AI follows the same rules as the human player, unless differences are defined 
in the rules engine (for example through difficulty settings).

The game engine or rules engine never hard-code AI decisions. (Unlike the original game)
For example, diplomacy decisions such as declaring war or accepting treaties are always 
made by the AI, not the rules or the game.

# Testing

Implement several test layers:

## Functional testing

Small tests within rs files themselves.

### Comments

Comments are brief and sit where the code cannot say it itself: why something is done, a rule of the
original game it follows, a trap. Documentation lives in `docs/`, not in the code.

- No task numbers and no history ("added for task 376", "first version", "will be replaced"). The task
  list and git say what was done and when; a comment that names a task goes stale when it is done.
- No general project information (what the game, a crate or the AI is for, how it works as a whole).
  A module doc is one or two lines on what the file holds, and a pointer to the doc if one exists.
- No comment that restates the next line, and no commented-out code.
- A comment that no longer matches the code is a bug: fix or delete it in the change that finds it.
- Test comments follow the same rule: name the situation a test builds only when the test name and the
  assertions do not already say it.

### Every file has tests

Every `.rs` file with logic has tests (a `#[cfg(test)] mod tests` in it, or a `tests` module beside it for a
big file). A file may go without only when someone has checked it, decided that tests add nothing, and
written that down: a line in the file's module doc, `//! Tests: none, because <reason>.` Typical
reasons: the file only declares modules or re-exports (`mod.rs`, `lib.rs`), only holds data types
without behaviour, or only draws and needs a window (then the deciding part is moved into a testable
function, see "UI test"). Code that is covered only by another file's tests is named there:
`//! Tests: in <file>`. A file with neither tests nor such a line is a gap, to be fixed in the
change that finds it. `docs/test-review-log.md` lists the files checked.

### Test on the actual game data and rules

**Always test on the shipped game data and the real rules, never on fixtures.** A test loads the content
the game ships with (`f4x_data::load_content`, the `content/` folder) and runs the real engine, so that
it behaves as close to actual gameplay as possible. Hand-built content, a second copy of a rule in the
test environment, or an empty `Content::default()` lets the test drift from the game and pass for the
wrong reason.

- **A test never builds its own rule set.** Do not copy a registry (`defs().to_vec()`), edit a
  definition and put it back with `Registry::new(..)`, and do not assign `content.rules.x = ..`: pick the
  unit, spell or setting of the shipped content that has the property the test needs, or fix the shipped
  content if its rules are wrong. A test of the content validator may break a copy on purpose. (Task 461
  lists the tests that still do.)
- Build only situations the game can reach (a unit always has a real player as owner; see
  `ecs-structure.md` section 3.3). A situation the game cannot create belongs in a test only to check
  error handling (a rejected command, a refused save).
- Look units, spells, buildings and races up by name in the shipped content; do not invent stats.
  Terrains and map features are called by their number (`num` in `content/`), as a save does, through
  the constants in `game::testutil::num` (`num::terrain::GRASSLAND`, `num::feature::TOWER`). The
  number is the single source of truth; the constant's name is only a note for the reader (the
  display name may change or be translated), and no test checks it against the content. Do not pick a spell, unit or other row by its position in the
  table (`defs()[0]`); name it, or call it by its number.
- Change the rules in a test only when the change is the point of the test (a unit given an
  ability, a spell made dearer, to see the outcome change). A change that only makes the sums easier
  belongs to the test that needs it, with the reason, not to a shared `world()`.
- A test whose subject needs content (spells, units) must fail when the content is missing. An empty
  registry that makes the test skip its assertions is a bug.
- A unit test inside `f4x-core/src/` cannot use `f4x_data::load_content`: `f4x-core` is below
  `f4x-data`, and with `f4x-data` as a dev-dependency the test build and `f4x-data` hold two copies of
  core's types (`expected content::Content, found f4x_core::content::Content`). Such a test calls
  `game::testutil::shipped()`, which reads the same `content/` files (the names from `text/en/name/`
  included, the messages and the checks of the loader not) and parses them once per test process.
  The integration tests in `f4x-core/tests/it/` use `f4x_data::load_content` itself (`tests/it/world.rs`).
  Do not hand-build units, spells, buildings or terrains in either.

### Where a test lives

Every test that needs rules or a game runs on the shipped content (see above). Where it lives depends on
what it reaches. `python3 scripts/classify_core_tests.py` counts the `f4x-core` tests that need a
game or content.

- **An integration test in `f4x-core/tests/it/`** when the situation can be built and the result read
  through the public API: `Command`s, `cheat_*` and the `scenario_*` setters of `game/access.rs` (add
  a `scenario_*` setter when one is missing; do not widen a field). This is the first choice for a
  rule that crosses modules (movement, gates, battles through `Command`s), and for new tests.
  `tests/it/world.rs` holds the shipped content and `flat_game`.
- **A unit test in `src/`** (`shipped()` for the rules) when the test needs a module's internals (a
  private system, a component, a field) or no game at all: a formula, a parser, an ordering, a
  state machine of one type. Keep it to the minimum the mechanic needs.
- **A test in `f4x-data`** when its subject is the content itself (counts, cross-references, the
  original's numbers) and no engine behaviour is involved.

### Error handling in tests

A test fails when it panics or when it returns `Err`; both are fine. Choose by what the test does:

- **Code under test returns `Result`** (loading, parsing, saving, a command that can be rejected):
  make the test `-> Result<(), Box<dyn std::error::Error>>` and use `?`. The failure then shows the
  real error instead of `called Result::unwrap() on an Err value`.
- **Setup that must succeed** (looking up a unit by name, a player that was just added): use
  `.expect("why this cannot be missing")`, never a bare `.unwrap()`. The message documents the
  precondition. Where the test returns `Result`, `.ok_or("what is missing")?` does the same.
- **Assertions stay `assert!`, `assert_eq!`, `assert_ne!`, `matches!`.** Do not turn them into `Err`
  returns: that loses the left/right diff and the line number.
- **No `panic!` as a check.** Use an assertion with a message. `#[should_panic(expected = "...")]` is
  only for code that is meant to panic; a returned error is asserted on as a value.
- **Never skip silently.** `if let Some(x) = ... { assert!(..) }` passes when the value is missing and
  checks nothing. Use `let Some(x) = ... else { return Err(..) }` or `expect`.
- **Test the error paths.** Assert the specific error variant (`matches!`, `assert_eq!`), not just
  `is_err()`, which can pass for the wrong reason.

### Quality of tests

- **Behaviour, not implementation.** Assert what a player or the AI can observe (events, state), so a
  refactor does not break the test.
- **Deterministic.** Seed every random generator; no wall-clock time; files only in a `TestDir`.
- **Fast.** A unit test runs in milliseconds. Build the world once per test (not once per loop
  iteration) and borrow rather than clone large entities (`Game`, `Content`, saves). A slow test is
  marked `#[ignore]` and run on request.
- **Every test asserts something that can fail.** Mutation testing (`cargo mutants -p <crate>`) finds
  tests that pass without protecting anything; task 408 tracks this.
- **Invariants and round trips** (save then load, a value that must never go negative) suit
  property-based tests (`proptest`); large structured output (event logs, saves) suits snapshot tests
  (`insta`). Neither is a dependency yet; add one with the first test that needs it.
- **Gates before a commit:** `cargo fmt --all`, `cargo clippy --workspace --tests`, and the test
  commands in `CLAUDE.md`.

A test that writes files takes its folder from `f4x_def::testdir::TestDir::new("label")` (`f4x_core::testdir` re-exports it), never a fixed
name in the temp folder: the name is unique per process and per call, so two `cargo test` runs at the same
time (or nextest) never share a folder, and the folder is removed when the value is dropped.

## Regression testing

Regression testing should be implemented on the overall game, not individual crates.

The goal of regression testing is to catch unintended divergences in specific gameplay.

Regression tests load a prepared test save and perform a number of defined game actions.
Then they save the game at a defined step. The save is then compared against either a 
predefined outcome save or a diff against the loaded save.

Regression testing is done without the client.

They live in `crates/f4x-core/tests/it/regression.rs`, with the prepared saves and the targets in
`tests/regression/` (`<name>.save`, `<name>.target`): an early game, a game at war and a game with three
global enchantments in force, each loaded, played on by a fixed command list, and compared with a
target of a short summary plus the checksum of the whole save (the game's version left out, so a
release does not fail them). A failing test prints both texts. Change a target on purpose with
`F4X_REFRESH_REGRESSION=targets cargo test -p f4x-core --test it regression` and read the git diff of
`tests/regression/` before committing; after a save format change (`SAVE_VERSION`) or a change of a
set-up rebuild the saves with `F4X_REFRESH_REGRESSION=saves` (it rewrites the targets too).

## UI test

The goal of this test is to catch divergence in the user interface.

Graphical tests load a predefined save, perform a defined set of game actions determined 
by a script, then take a screenshot and compare it to a target screenshot.

UI testing requires the client.

Click-flow tests (no window, no screenshot) are unit tests in the client crate. The seams:
`App::for_test()` (`test_app.rs`: the shipped content and a new game, no art), `Input::key(..)` and
`Input::click(..)` build one frame by hand, `frame::window_size()` gives tests a fixed window, and
`App::modals` shows which prompts are open. `test_app::shipped` holds the parsed `content/` (content, texts,
settings, spell art) once per test run; a test does not call `f4x_data::load_*` itself. Anything that draws needs a window, so put the part that
decides (what a key answers, whether a notice closes, which events open a prompt) in a function
that takes `&Input` and returns the new state (`resign_answer`, `Input::dismisses`), draw after it,
and test that function.

How to write one: split the modal's `frame` into the part that draws and asks (`confirmation_box`,
`notify2_box`, which need a window) and a method that takes the answer, for example
`QuitAsk::answered(self: Box<Self>, app, answer: Option<bool>) -> Option<Box<dyn Modal>>`. The test builds
`App::for_test()`, calls it with `None`, `Some(false)` and `Some(true)`, and checks what stays open
and what the app did (`app.quit`, `app.turn_pending`, `app.modals.has(..)`). A key or click is a
frame built with `Input::key` or `Input::click` handed to the pure answer function
(`Buttons::answer`, `Input::dismisses`, `ui::hit`). Examples: `the_human_who_loses_units_fleeing_gets_the_notice`
(`fled_notice.rs`), `yes_to_the_quit_question_quits_and_no_does_not` (`menu_screen.rs`), the answer tests of
`sell_prompt.rs`.

## Soak testing

The goal of soak testing is to catch divergence in overall gameplay.

Soak testing either loads a save or starts a new game with defined seed and settings.
It then runs an automated game with AI players, without interference or special instructions.
The test takes a save snapshot at predefined turn intervals (usually every 50 or 100 turns),
and compares it to a target save or a defined outcome specific to that test.
(number of spells cast, number of battles fought etc.)

## AI Improvement testing

These tests are similar to soak tests, except they are testing the outcome of AI changes.

An AI improvement test first runs a baseline game with specific settings and seed, 
using only AI players with the pre-change AI.

In the second step, the test runs the same game again, swapping one or several (but not all)
of the AI players for the updated AI.
The outcomes are then compared according to various success metrics, 
such as winning the game, number of cities funded, number of cities conquered,
explored terrain, conquered or controlled encounter zones and nodes, spells researched,
spells cast etc.

