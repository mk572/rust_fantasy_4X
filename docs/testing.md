# Testing in this project

How the general rules of [`rust_coding_rules.md`](rust_coding_rules.md) apply here. The test and
lint commands are in `CLAUDE.md`; the design of the tests of single crates is in
`f4x-core-test-design.md`, `f4x-def-test-design.md` and `f4x-newgame-test-design.md`.

## The shipped content

- The content is in `crates/f4x-def/content/` (`.ron`) and is loaded with
  `f4x_def::load::load_content`, the same loader and checks as the game's. Tests get it once per test
  process from `f4x_def::testdir::shipped()`; unit tests in `f4x-core/src/` from
  `world::testutil::shipped()`; the integration tests of core and master from their
  `tests/it/support.rs`.
  The client's tests use `test_app.rs` (content, texts, settings and art of spells, parsed once).
- Do not hand-build units, spells, buildings or terrains. A limit of a game in play comes from the
  game (`w.max_tile()`, `w.max_battle_side()`), not from a setting's upper bound
  (`MAX_TILE_LIMIT`, `MAX_STACK`). `scripts/imaginary_tests.py` lists the tests that make things up.
- Call units, spells, realms, races, buildings, terrains and map features by their number (`num` in
  the content), as a save does, written where it is used with the alias as a comment:
  `104 /* high_men_spearmen */`. The number is the single source of truth: no constants, helpers or
  modules that only turn a name into a number. Look the number up in the content file.
- A unit always has a real player as owner (`ecs-structure.md` 3.3).
- `scripts/shadow_tests.py` lists the tests that assert a number and read the content.
- Whether the measure of a unit's strength (`battle::power`) matches the battles is the job of the
  duel harness (`duel.md`), not of a test.

## Content hashes

Each crate with `.ron` files has a `content.sha256` and a test that holds the files against it
(`f4x_def::content_hash`). A change made on purpose is recorded with
`python3 scripts/ron_hashes.py --update`, and the record goes in the same commit.

## Where a test lives

- `f4x-core` is the world and `f4x-master` the game: a test that needs turns to pass, replays or undo
  is a test of `f4x-master` (`tests/it/`). See `f4x-core-test-design.md`, "What core can and cannot
  use".
- Integration tests of `f4x-core` build situations with `Command`s, `cheat_*` and the `scenario_*`
  setters of `world/access.rs` (add a `scenario_*` setter when one is missing).
  `scripts/classify_core_tests.py` lists the unit tests of core that need only the public API.
- A unit test in `f4x-core/src/` that needs a whole new game (a real map, wizards, cities) cannot
  call `new_game` (its crate sits above core): it loads one with
  `world::testutil::fresh(wizards, seed)`, a saved game in
  `crates/f4x-core/tests/saves/fresh_w<wizards>_s<seed>.save` (its generator beside it, `.rng`). The
  test `fixtures` of `f4x-newgame` keeps the files equal to what `new_game` makes; when it fails,
  refresh them with `F4X_REFRESH_FIXTURES=1 cargo test -p f4x-newgame --test it fixtures` and run the
  tests of core again (a test that depends on a seed's map may need a look). A test of the generators
  or of `new_game` itself lives in `f4x-newgame`.
- A test that writes files takes its folder from `f4x_def::testdir::TestDir::new("label")`
  (`f4x_core::testdir` re-exports it).

## Regression tests

`crates/f4x-master/tests/it/regression.rs`, with the prepared saves and the targets in
`tests/regression/` (`<name>.save`, `<name>.target`): an early game, a game at war and a game with
global enchantments in force. Each is loaded, played on by a fixed command list and compared with a
target of a short summary plus the checksum of the whole save (the game's version left out, so a
release does not fail them). Change a target on purpose with
`F4X_REFRESH_REGRESSION=targets cargo test -p f4x-master --test it regression` and read the git diff
of `tests/regression/` before committing; after a save format change (`SAVE_VERSION`) or a change of a
set-up rebuild the saves with `F4X_REFRESH_REGRESSION=saves` (it rewrites the targets too).

## UI tests of the client

The seams: `App::for_test()` (`test_app.rs`: the shipped content and a new game, no art),
`Input::key(..)` and `Input::click(..)` build one frame by hand, `frame::window_size()` gives tests a
fixed window, and `App::modals` shows which prompts are open. To test a prompt, split its `frame` into
the part that draws and asks (`confirmation_box`, `notify2_box`, which need a window) and a method
that takes the answer, such as `QuitAsk::answered`; call it with `None`, `Some(false)` and
`Some(true)` and check what stays open and what the app did. A key or click is handed to the pure
answer function (`Input::dismisses`, `ui::hit`). Examples:
`the_human_who_loses_units_fleeing_gets_the_notice` (`fled_notice.rs`),
`yes_to_the_quit_question_quits_and_no_does_not` (`menu_screen.rs`), the answer tests of
`sell_prompt.rs`.

## Soaks and AI comparisons

Soaks are `f4x-tools soak`, started through `scripts/cool_run.sh` with at most 4 jobs. A change of the
AI is measured by playing the same seed and settings with the old AI, then again with the new AI in
some (not all) of the seats (`scripts/ai_battle.py`, `ai-design.md` 8.1a), and comparing games won,
cities founded and conquered, land explored, sites and nodes held, and spells researched and cast.

## Reviews

`test-review-log.md` lists the files whose tests were reviewed.
