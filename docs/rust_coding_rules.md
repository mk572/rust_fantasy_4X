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

## Rules engine

The rules engine is static and implements the rules of the game.
It reads game rules and content and provides it to the game engine.
For example, unit stats and abilities, buildings, spells, terrain etc. are all provided via the rules engine.
The rules engine sits in f4x-data.

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

### Test on the actual game data and rules

**Always test on the shipped game data and the real rules, never on fixtures.** A test loads the content
the game ships with (`f4x_data::load_content`, the `content/` folder) and runs the real engine, so that
it behaves as close to actual gameplay as possible. Hand-built content, a second copy of a rule in the
test environment, or an empty `Content::default()` lets the test drift from the game and pass for the
wrong reason.

- Build only situations the game can reach (a unit always has a real player as owner; see
  `ecs-structure.md` section 3.3). A situation the game cannot create belongs in a test only to check
  error handling (a rejected command, a refused save).
- Look units, spells, buildings and races up by name in the shipped content; do not invent stats.
- A test whose subject needs content (spells, units) must fail when the content is missing. An empty
  registry that makes the test skip its assertions is a bug.
- The one exception is a crate that cannot depend on `f4x-data` (`f4x-core` is below it). Keep any
  hand-built content there to the minimum, and cover the same behaviour again in a test that uses the
  shipped data (`f4x-data/tests/` or a higher crate).

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

A test that writes files takes its folder from `f4x_core::testdir::TestDir::new("label")`, never a fixed
name in the temp folder: the name is unique per process and per call, so two `cargo test` runs at the same
time (or nextest) never share a folder, and the folder is removed when the value is dropped.

## Regression testing

Regression testing should be implemented on the overall game, not individual crates.

The goal of regression testing is to catch unintended divergences in specific gameplay.

Regression tests load a prepared test save and perform a number of defined game actions.
Then they save the game at a defined step. The save is then compared against either a 
predefined outcome save or a diff against the loaded save.

Regression testing is done without the client.

## UI test

The goal of this test is to catch divergence in the user interface.

Graphical tests load a predefined save, perform a defined set of game actions determined 
by a script, then take a screenshot and compare it to a target screenshot.

UI testing requires the client.

Click-flow tests (no window, no screenshot) are unit tests in the client crate. The seams:
`App::for_test()` (`test_app.rs`: the shipped content and a new game, no art), `Input::key(..)` and
`Input::click(..)` build one frame by hand, `frame::window_size()` gives tests a fixed window, and
`App::modals` shows which prompts are open. Anything that draws needs a window, so put the part that
decides (what a key answers, whether a notice closes, which events open a prompt) in a function
that takes `&Input` and returns the new state (`resign_answer`, `Input::dismisses`), draw after it,
and test that function.

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

