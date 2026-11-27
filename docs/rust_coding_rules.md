# Rust Coding Rules

## Safe coding

Unsafe code is only allowed in the test environment.

Production code (the game) should avoid unsafe code wherever feasible.
Current unsafe code should be replaced.

Implement error handling according to good Rust coding practices.
Avoid unwrap. Avoid panics. (Tests have their own rules, see "Error handling in tests" below.)

Do not drop a result with `let _ =` or `.ok();`, and do not answer a failure with a `bool` or `Option` where
several things can go wrong: return an error that says which (`MoveError`, `MarchError`, `ItemError`, an event such
as `Event::OrderFailed`), and let the caller choose what to do with it (try the next, give up the plan, tell the
player, `Ai::note_failure`). A caller that wants only yes or no says so with `.is_ok()` and a comment. In the
engine, components are given with `World::put` / `put_bundle` and taken off with `World::strip` (`put_in`, `strip_in`
for a bare `hecs::World`): a component that is not there is fine, an entity that is gone stops a debug build. (Silent
error audit, tasks 546 to 559.)

Clippy's `unwrap_used` and `expect_used` lints keep unwraps out of production code; tests may allow
them with `#![cfg_attr(test, allow(clippy::unwrap_used))]`.

## Lists, tables and fixed-size arrays

No array type with a written length (`[T; 48]`) for a list or a table: the count must be edited
whenever an entry is added, and it says nothing about whether the right entries are there.

- A table of constants is a slice: `const NAMES: &[(&str, usize)] = &[...]`.
- One entry per variant of an enum is an exhaustive `match` with no wildcard arm, so a new variant
  does not compile until it has its entry (`stat_key` in `temp_effect_view.rs`). The same goes for
  moving from a variant to its place in a list, and from a name or a number back to a variant.
- To go over every variant, derive `strum::EnumIter` and use `iter()` (or a small `all()` on the
  enum, so the callers need no import). A test that must see all of them uses it too, so nothing
  can be forgotten.
- An array stays only where the length is the point, and a comment says why: a coordinate or a
  colour (`[f32; 2]`, `[u8; 4]`), the 8 neighbours of a square, a palette, a file format's fixed
  buffer, one value per plane or per side of a battle, a machine-learning input of a fixed shape.

## Resource efficiency

Avoid cloning, especially on large entities.

## Layers of calls

A function gets its information at the source. A function between a caller and the source needs a
purpose, which the doc comment states when it is not plain: it **wraps private data** (the caller may not see
the field), **converts** (a type, a unit), **binds** a default, a constant or the caller's own identity
(`PlayerView::income` binds `me`; `red_button` binds the face), **caches**, or **enforces a rule** (the
`PlayerView` forwarders are the boundary that keeps the AI from holding a `&World`: it reads only what a human
sees). A function that only passes its arguments on, renames the call, drops a result, or exists because it
once did something else is removed and its callers go to the source. `python3 scripts/find_wrappers.py`
lists the wrappers and `--chains` the wrappers of wrappers (it matches by name: read the hits).

# Basic Structure

The game is built on four major components: rules engine, game engine, client and ai.

The crates depend on each other in one direction: `f4x-def` is at the bottom (the static definitions,
no other game crate); `f4x-battle` depends on `f4x-def`; `f4x-core` on `f4x-battle` and `f4x-def`; `f4x-ai` on `f4x-def` and `f4x-core`. The line of the layers is client -> master -> core -> battle -> def, and nothing points up it;
`f4x-newgame` (the map generator, the setup of a new game) sits above core and uses only its public API,
so core never calls it; `f4x-master` (the game and its turns, undo and replays) on core, battle and def (no crate below it depends on it, for tests either: a test that needs turns to pass lives in it); `f4x-console` depends on def, core, newgame, ai and master; `f4x-learn` on def, core and
newgame; `f4x-client` and `f4x-tools` on all of these (`f4x-tools` also on `f4x-ai-ref`). A crate loads the data it holds: `f4x-def` loads the
definitions (`f4x_def::load`). `f4x-core` reads and writes save files only (`world/save/file.rs`); it is deterministic otherwise.

## Rules engine

The rules engine is static and implements the rules of the game.
It reads game rules and content and provides it to the game engine.
For example, unit stats and abilities, buildings, spells, terrain etc. are all provided via the rules engine.

The rules engine is split over two crates:

- **f4x-def** holds the types of the rules (`UnitDef`, `SpellDef`, `Effect`, `Stat`, the registries,
  the terrain and feature definitions, `Content`) and loads the files in `crates/f4x-def/content/` (`.ron`),
  validates them and builds the types from them (`f4x_def::load::load_content`). Core still
  offers `rules`, `map` and `feature` paths for the parts it shares with them.
- **f4x-core, `rules/exec.rs`** applies the effects.

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

### This is a game, not a banking system

A banking system has business rules the application must never stray from, and its tests pin them.
A game changes its gameplay as it evolves, and that is allowed and expected. So a test does **not
define the gameplay**: it never says "this spell does that" or "this immunity stops this" as a fact
of its own. The rules and their switches live in the content and in the settings (`settings.rs`,
`.ron`); a test reads them and checks that the game implements what they say. When the design
changes, the content or the setting changes, and the tests, which only follow it, stay green. A
test that has to be rewritten whenever the balance moves is carrying a rule it should not carry.

A good test performs an **action** and checks the **outcome the player sees**. Good: target a
death-immune unit with a spell of the death realm; the cast is refused and the player gets a message
that says why. Bad: check that zombies have death immunity (that is a fact of the content, and
changing the content must not break a test). Where the outcome depends on a setting, run the action
under each value of the setting and check the outcome the setting promises.

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
the game ships with (`f4x_def::load::load_content`, the `crates/f4x-def/content/` folder) and runs the real engine, so that
it behaves as close to actual gameplay as possible. Hand-built content, a second copy of a rule in the
test environment, or an empty `Content::default()` lets the test drift from the game and pass for the
wrong reason.

- **A test never builds its own rule set.** Do not copy a registry (`defs().to_vec()`), edit a
  definition and put it back with `Registry::new(..)`, and do not assign `content.rules.x = ..`: pick the
  unit, spell or setting of the shipped content that has the property the test needs, or fix the shipped
  content if its rules are wrong. A test of the content validator may break a copy on purpose, and so may a test of a
  function that takes the registry as its input and needs a case the shipped content lacks. These are
  the tests that still do, each with its reason (`docs/test-review-log.md`, tasks 461 and 484):
  `hook.rs` (validator), `world/save/tests.rs` (a save naming a unit the content lacks),
  `f4x-newgame/src/computer_wizard/tests.rs` (ready-made wizards with a composition the shipped ones lack: opposed realms,
  a set of Myrran ones) and `f4x-console` (no ready-made wizards, so the fallback names show).
  Where a result depends on a roll, cast or fight again until it happens (`cast_until`) or give the
  caster the means, rather than raising a number in the rules. Tests that only set a game setting or a
  config value (`g.settings.rules.x`, `rules.config.x`) are not rule sets.
- **A test never carries a number of the game's rules of its own.** How many units a square holds,
  how many fight in a battle, a price, a limit: read it from the game (`w.max_tile()`,
  `w.max_battle_side()`, the shipped content), never a literal (`0..12`) and never the setting's
  upper bound (`MAX_TILE_LIMIT`, `MAX_STACK`), which is not the limit of any game in play.
  `scripts/imaginary_tests.py` flags the constants.
- Build only situations the game can reach (a unit always has a real player as owner; see
  `ecs-structure.md` section 3.3). A situation the game cannot create belongs in a test only to check
  error handling (a rejected command, a refused save).
- Call units, spells, realms, races, buildings, terrains and map features by their number (`num` in
  `crates/f4x-def/content/`), as a save does, written where it is used with the alias as a comment:
  `104 /* high_men_spearmen */`. The number is the single source of truth. **No constants, helpers or
  modules that only turn a name into a number** (`const SPEARMEN: u16 = 104`, `num::terrain::GRASSLAND`):
  a name can lie (a constant once said spearmen over the number of the cavalry) and it gets copied
  from test to test. Look the number up in the content file before writing it; do not copy one from
  another test. Do not invent stats, and do not pick a row by its position in the table (`defs()[0]`).
- Change the rules in a test only when the change is the point of the test (a unit given an
  ability, a spell made dearer, to see the outcome change). A change that only makes the sums easier
  belongs to the test that needs it, with the reason, not to a shared `world()`.
- A test whose subject needs content (spells, units) must fail when the content is missing. An empty
  registry that makes the test skip its assertions is a bug.
- A unit test inside `f4x-core/src/` calls `world::testutil::shipped()`, which is
  `f4x_def::load::load_content` on the `crates/f4x-def/content/` folder (the same loader and checks as the game),
  run once per test process. The integration tests in `f4x-core/tests/it/` use
  `f4x_def::load::load_content` itself (`tests/it/world.rs`).
  Do not hand-build units, spells, buildings or terrains in either.
- A unit test in `f4x-core/src/` that needs a whole new game (a real map, wizards, cities) cannot call
  `new_game` (the crate that has it sits above core): it loads one with `world::testutil::fresh(wizards, seed)`,
  a saved game in `crates/f4x-core/tests/saves/fresh_w<wizards>_s<seed>.save` (its generator beside it,
  `.rng`). The test `fixtures` of
  `f4x-newgame` keeps the files equal to what `new_game` makes; when it fails, refresh them with
  `F4X_REFRESH_FIXTURES=1 cargo test -p f4x-newgame --test it fixtures` and run the tests of core again
  (a test that depends on a seed's map may need a look). A test of the generators or of `new_game` itself
  lives in `f4x-newgame`.

### Test the behaviour against the setting, not a copy of it

**The content is guarded by its hashes, not by tests.** Each crate with `.ron` files (`f4x-def`,
`f4x-lang`, `f4x-client`) has a `content.sha256` with the SHA-256 of each file, and a test that
holds the files against it (`f4x_def::content_hash`). A setting changed by hand or by a script that
should not have touched it fails that test; a change made on purpose is recorded with
`python3 scripts/ron_hashes.py --update`, and the record goes in the same commit, so the history
shows it was meant. This is why no other test needs to write down what the content says: such a
test is deleted, or rewritten to read the setting as below.

**A test never writes down what a setting says.** An expected value typed into a test (a cost, a stat,
a list of races, the spell that has an effect, the number of units a building gives) is a *shadow
setting*: a second copy of the content that has to be edited together with it, that fails when the
content is changed on purpose, and that shows nothing about the game. The test reads the setting and
checks that **the game does what it says**.

- **The expectation comes from the rules the game was built from.** Read the value from the shipped
  content (`shipped()`, the registry, the settings) in the test, and drive the real game: the command,
  the city builder, the battle, the turn change. Then compare the outcome with what was read.
- **Example.** A test reads `buildings.ron`, finds a race that cannot build a building, opens the city
  builder (or asks `can_build`) for a city of that race and checks the building is not offered and that
  `SetProduction` for it is refused; it finds a race that can, and checks that it is offered and gets
  built. Another: read a unit's `cost`, put it in production in a city with a known surplus and check
  that it is done after the turns that cost needs, not after a number the test wrote down.
- **A test names the setting it reads** (a comment or the helper's name: "the building's `races`
  list"), so that a reader sees what the behaviour is checked against.
- **Fixed numbers as input are no shadow settings.** The unit number to spawn, a position, a seed, a
  number of turns to run are the scenario (and units are called by number with the alias, see above).
  What counts is a number or a name *asserted* as the outcome that the content alone decides.
- **Pick the case from the content, not from memory.** Do not write "high elves cannot build X": search the
  content for a race and a building where the property holds (see the first rule of this section: pick
  the row that has the property the test needs) and fail the test when there is none.
- **A test that can only restate a field**, because no game code reads it, is deleted, and the field is
  reported as dead.
- **No unproven gameplay assumption decides a test.** A test must not rely on a claim nobody has
  shown, such as "four paladins lose to two great drakes" or "a spearman beats a scout": the test does
  not know. When it needs a stronger or a weaker side, it builds it from the game's own measure
  (as many of one unit as it takes to be worth three times the other, by `battle::power`), so that it
  tests the decision that follows from the measure, not the measure. Whether the measure itself
  matches the battles is the job of the duel harness (task 655).
- **Check yourself:** change a content value on purpose (a cost, a race in a list). A test may fail
  only when the behaviour then differs from the setting. If it fails because the number it wrote down
  no longer matches, it is a shadow setting. (Task 653 replaces the existing ones.)

### Where a test lives

Every test that needs rules or a game runs on the shipped content (see above). Where it lives depends on
what it reaches. `python3 scripts/classify_core_tests.py` counts the `f4x-core` tests that need a
game or content.

- **An integration test in `f4x-core/tests/it/`** when the situation can be built and the result read
  through the public API: `Command`s, `cheat_*` and the `scenario_*` setters of `world/access.rs` (add
  a `scenario_*` setter when one is missing; do not widen a field). This is the first choice for a
  rule that crosses modules (movement, gates, battles through `Command`s), and for new tests.
  `tests/it/world.rs` holds the shipped content and `flat_game`.
- **A unit test in `src/`** (`shipped()` for the rules) when the test needs a module's internals (a
  private system, a component, a field) or no game at all: a formula, a parser, an ordering, a
  state machine of one type. Keep it to the minimum the mechanic needs.
- **A test in `f4x-def` (`load`) or `f4x-core/tests/it`** when its subject is the content itself (counts, cross-references, the
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
  iteration) and borrow rather than clone large entities (`World`, `Content`, saves). A slow test is
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

They live in `crates/f4x-master/tests/it/regression.rs`, with the prepared saves and the targets in
`tests/regression/` (`<name>.save`, `<name>.target`): an early game, a game at war and a game with three
global enchantments in force, each loaded, played on by a fixed command list, and compared with a
target of a short summary plus the checksum of the whole save (the game's version left out, so a
release does not fail them). A failing test prints both texts. Change a target on purpose with
`F4X_REFRESH_REGRESSION=targets cargo test -p f4x-master --test it regression` and read the git diff of
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
`App::modals` shows which prompts are open. `test_app::shipped` holds the parsed `crates/f4x-def/content/` (content, texts,
settings, spell art) once per test run; a test does not call `f4x_def::load::load_*` itself. Anything that draws needs a window, so put the part that
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

