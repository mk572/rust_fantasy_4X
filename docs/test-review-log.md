# Test review log

Review of every `.rs` file's tests, in alphabetical order by folder and file (`f4x-ai-ref` is skipped:
it is a generated copy). Checks: fixtures that should be the shipped game data, tests of outdated code,
slow or wasteful tests (cloning), `unwrap`/`expect`/`panic!` instead of error handling, files without tests
or a `//! Tests:` line, stale or wordy comments (task numbers, history, general project information). Rules:
`rust_coding_rules.md`, "Testing".

| File | Status | Notes |
|---|---|---|
| `crates/f4x-ai/src/diplomacy.rs` | done | Test `game()` now loads the shipped content (before: empty `Content::default()`, so the spell-swap check in `a_pact_is_refused...` was silently skipped; it now asserts). One seeded helper replaces three copies of the two-player setup. 9 `unwrap`/`expect` sites replaced, tests return `Result`. `testworld::content()` parses the shipped content once per process and hands out copies: the two seed-loop tests went from 0.37 s to under 0.06 s. |
| `crates/f4x-ai/src/economy.rs` | done | Already on the shipped content. Three tests return `Result`; one `capital(&g)` helper replaces the repeated city lookup with `unwrap`. |
| `crates/f4x-ai/src/expand.rs` | done | Had no tests. Added five on the shipped content: a settler walks to a site and founds a city; two settlers keep `SPACING` apart; a settler that did not move all turn gives its site up (`bad_sites`) and chooses another; a settler with no explored site scouts; the city name comes from the race's list. Not covered: targets of settlers that died are forgotten (no way to remove a unit in a test without an invented situation). |
| `crates/f4x-ai/src/lib.rs` | done | Only module declarations and `Ai`; the `#[cfg(test)]` is the `testworld` declaration. Covered by the module tests. Has the `//! Tests:` line the rule "Every file has tests" asks for. |
| `crates/f4x-ai/src/military.rs` | done | Clean: shipped content, no unwraps. Reuses `P0`/`P1` from `testworld`. |
| `crates/f4x-ai/src/monster.rs` | done | 2 unwraps replaced; the silent `if let` on the city owner is an assertion; the dead `units` variable is gone; `monsters_that_see_a_defended_city...` was satisfied by any event and now asserts the battle (monsters attack, win), the rampage and the dead defender. |
| comments, `crates/f4x-ai/src` (files above) | done | Removed task numbers (341, 376) and the stale `lib.rs` paragraph ("first version is deliberately simple", "tasks 64 to 67 add ..."; all four are done). Only task and history wording was searched in the other five files; `diplomacy.rs` was read in full (below). |
| comments, `crates/f4x-ai/src/diplomacy.rs` | done | Read all 60 comments. Section references in `docs/diplomacy.md` (4, 8) are right. Kept the doc comments that give the original's rule or formula. Shortened the module doc (it described the core/AI split), `approach_human` (listed what its body does, with the odds that are in the constants) and `negotiate`. Test comments are short and say why. |
| `crates/f4x-ai/src/naval.rs` | done | Shipped content, 13 tests under 25 ms. 4 `unwrap`/`expect` sites replaced (4 tests return `Result`); the island coordinate `28` in the ferry test is `ENEMY_CITY.0`. Module doc lost "task 230" and the retelling of the voyage. Other comments are short and say why. |
| `crates/f4x-ai/src/neutral.rs` | done | Shipped content, tests under 50 ms. 4 `unwrap` sites replaced (setup helpers and 6 tests return `Result`). `a_city_at_war_sends_all_but_its_garrison_against_a_site_in_range` asserted only "the site is cleared", the same as the peaceful test, so it never tested war; replaced by a test that compares both cases in the second turn (peace sends 4, war 7 of 9 paladins). Repeated "everyone is at home" checks are one `away()` helper. Module doc lost tasks 353, 354 and 376 and half its length; it keeps the trap that a host must call `play_neutrals`. The `Lair` in `raiding_world` is built by hand (an intact lair with one guard type and 300 gold) to control the fight; the war test sets `scenario_set_aggressive`. |
| `crates/f4x-ai/src/objective.rs` | done | Pure table logic, no game state needed. One `expect` replaced. `a_crueller_temper...` built a second copy of `TEMPERS` (and shadowed the `temper` fn); it uses the constant. `the_pragmatist_row_is_the_ai_as_it_was` (history in the name, bare numbers) is now `a_wizard_without_an_objective_has_the_pragmatist_row` and compares against the Pragmatist row; the numbers (7 cities, strike 4) stay pinned in the same test. Comments: "as it was before objectives steered anything" and task 148 removed. |
| `crates/f4x-ai/src/offer.rs` | done | Games come from `new_game` on the shipped content; 6 tests under 50 ms. 9 `unwrap`/`expect` sites replaced (`game()` and a `set_gold` helper return `Result`). The hand-built `Item { name: "Sword", .. }` is a copy of a shipped sword (`Item::from_def`), and the expected price is what the seller was paid, not a hard-coded 500. The turns 40 and 50 are `AI_FIRST_MERCENARY_TURN` (-10 and itself), and the test name says so. Comment "(task 294)" removed. Still pokes `g.turn` and uses `cheat_offer` to force offers: the game has no other way to make a merchant call on demand. |
| `crates/f4x-ai/src/race.rs` | done | Shipped content, 5 tests under 35 ms. 5 `unwrap`/`expect` sites replaced (`world()` and 3 tests return `Result`). Module doc cut from six lines to three. The two `rival_turns` tests are pure arithmetic with the numbers worked out in a comment. |
| `crates/f4x-ai/src/raze.rs` | done | Shipped content, 2 tests under 30 ms. 4 `unwrap` sites replaced. `a_peaceful_wizard_never_razes_and_a_maniacal_one...` only tested the maniacal case; renamed to what it tests, and the peaceful claim is now asserted in the temper test (`scores[3] < 0`). The "Second" city is founded with the high men race, not `race: None` (no real game founds a city without a race). Hard-coded turns: none left in `f4x-ai` (the `offer.rs` ones use `AI_FIRST_MERCENARY_TURN`). |
| `crates/f4x-ai/src/scout.rs` | done | Shipped content, 17 tests under 55 ms. 4 `unwrap`/`expect` sites replaced. New test for the stuck scout (no `EndTurn` between looks, so it never moves; after `STUCK_TURNS` its target is in `bad_scout_targets` and it picks another); checked by raising the threshold in the code, which makes it fail. Module doc lost the sentence on why scouting matters. |
| `crates/f4x-ai/src/shortfall.rs` | done | Shipped content, 3 tests under 35 ms. The food test no longer switches off the shipped half-upkeep rule (it also cloned the whole `Content` to do it); it passes under the real rule with the same 8 spearmen. 4 `unwrap` sites replaced (2 tests return `Result`). Module doc lost "tasks 62 and 292". **Open:** `heroes_and_loaded_ships_are_the_last_to_go` is still a synthetic test of `choose_troops` (hand-built `TroopLine`s on bare entities). A real-game version was tried and dropped: in a real shortfall a hero costs gold but no food, a ship costs no gold, and the plain soldiers cover the deficit, so the hero and the ship are never candidates and the ordering is not exercised (the inverted ordering passed the real-game test). It would need a gold deficit larger than the plain soldiers' upkeep, and `init_heroes` is crate-private to core. |
| `crates/f4x-ai/src/site.rs` | done | Shipped content, 4 tests under 50 ms. 2 `unwrap` sites replaced. The hand-made `Lair` (intact, 300 gold, given guards) was a copy of the one in `neutral.rs`; both now call `testworld::spawn_site`. Local `P0` constant replaced by the import; the repeated "everyone is home" check is `all_home`. Module doc lost task 356 and a third of its length. |
| `crates/f4x-ai/src/testworld.rs` | done | Test support (compiled only for tests), now says so in a `//! Tests:` line. Its 7 `expect`/`panic!` sites are setup that must succeed and name what is missing (`no unit high_men_swordsmen`), which is what `rust_coding_rules.md` asks for; making `game()`, `spawn()` and `build()` return `Result` would add `?` to about 100 call sites without better diagnostics, so they stay. Holds `spawn_site`, shared by `neutral.rs` and `site.rs`. **Decided (31 October 2026): they stay as they are.** |
| `crates/f4x-ai/src/view.rs` | done | Had no tests. Added six on the shipped content: strengths and stacks, the nine-unit cap of a fight, fighter versus ship versus settlers, coastal squares, the garrison (one fighter, two in the fortress city), the nearest unexplored square. `distance` is a one-line forward to `PlayerView::distance` and is not tested separately. |
| `crates/f4x-ai/src/wizardry.rs` | done | 14 tests, all under 35 ms (the sorting test was 0.1 s: `kind_of` reloaded the content from disk 19 times). All 46 `unwrap`/`expect`/`panic!` sites were in the tests and are gone (every test returns `Result`). `testworld` gained `shipped()` (the content parsed once, handed out by reference) and `fresh_game()` (`new_game` on it, seed 7), which replace three copies of the `new_game` boilerplate here and one in `offer.rs`, and two `g.content().clone()` calls made only to get past a borrow. The kind table is one `cases` array and the failure names the spell. `crusade` was the spell `awareness`. Module doc lost half its length and "the ladder as it was" is "the ladder". `what_the_ai_cannot_use_is_left_alone` says "a spell that is not done yet": not checked whether `earth_to_mud` is still unfinished. |
| `crates/f4x-ai/tests/it/behaviour.rs` | done (2 tests fail) | 5 slow tests, ignored by default (about 1 s each). 4 `unwrap`/`expect` sites replaced, every test returns `Result`, the helpers return `Result`. `testworld` is `cfg(test)` inside the library, so these keep their own `content()`/`game()`. Run with `--ignored`: **two fail, before and after my change**: Earth Lore (50 against 50 squares) and the island ship (195 of 300 squares). Task 429, to be checked later (decided 31 October 2026). |
| `crates/f4x-ai/tests/it/ships.rs` | done | The ship rules on the shipped content, under 100 ms. It includes `testworld.rs` by `#[path]`, so it shares that file's helpers. 8 `unwrap`/`expect` sites replaced (the can-build helpers return `Result<Result<..>, ..>`: the outer one is "the test named something that does not exist"). The squares `(11,6)`, `(27,5)`, `(28,5)`, `(12,6)`, `(10,5)` are derived from `COAST_CITY` and `ENEMY_CITY`. Task numbers 230, 233 to 236 removed from the doc and two tests. `a_loaded_ship_and_its_passengers_are_one_stack_for_the_nine_unit_limit` asserted only "6 units stand on the square"; the nine-unit limit is not what a tile holds any more (20), so it is `..._all_stand_on_the_ships_square`. The terrain check `!(a == b)` is a helper `at_sea`. |
| `crates/f4x-ai/tests/it/main.rs` | done | Two `mod` lines. |

**`crates/f4x-ai` is finished** (`f4x-ai-ref` skipped). 122 tests run in 0.26 s, plus 5 ignored (2 of them fail, task 429).

## crates/f4x-core (31 October 2026, done without the user present)

Done for the whole crate in four passes, by script and compiler where it was mechanical, so the rows below are by pass and area, not by file. Checked after every pass: `cargo nextest run -p f4x-core` (1,023 tests, 1.2 s), `cargo clippy --workspace --tests`, `cargo fmt`.

| Pass | Status | Notes |
|---|---|---|
| `unwrap` in tests, all of `crates/f4x-core/src` and `tests/it` | done | About 1,900 bare `.unwrap()` sites. Every `#[test]` that unwrapped now returns `TestResult` (new, in `f4x_core::testdir`) and uses `?`: a `Result` keeps its real error, an `Option` is `.ok_or("<call> is missing")?` (for `id_of("x")`: `x is not in the content`). Closures and test helpers (`testutil.rs`, `world()`-style builders) use `.expect("<call> succeeds")`. **The messages come from the call name by script and are plain; refine them where a failure turns out to be hard to read.** `panic!` checks became `return Err(..)`; `#[should_panic]` tests were left alone. No bare `unwrap()` is left in the crate (production code had none outside `#[cfg(test)]` helpers). |
| comments, all of `crates/f4x-core/src` | done | About 100 task references removed ("(task 348)", "(tasks 329 to 332)"); sentences that told history or the future rewritten or cut ("is task 49", "comes later", "for now", "thin methods with the old names", "Stage 1 holds ..."). The comments were found by searching for task and history wording, not read one by one, so comments that are merely wordy are probably still there. |
| `//! Tests:` lines, 72 files | done | Every file without a `#[test]` now says where its tests are or why it has none. The "in" targets were picked by matching function names against the test code (the test file that names most of the file's functions), not by reading each file; the 15 files where hardly any function is named are **not** marked and are listed in task 431. `rules/registry.rs` had no test: it has five now (lookup by alias and number, gaps, iteration order, `new_sorted`, both refusals). |
| tests on shipped data | partly | `f4x-data` is a dev-dependency of `f4x-core`; `tests/it/shipped_game.rs` runs a new game on `content/` (20 turns without a refusal, save and load give the same game, the same seed gives the same game). Since replaced by the 430 pass below. |
| hard-coded values | partly | The turn thresholds of marriage and meteor (`BIG_EVENTS_FROM_TURN`, was 150 in two places), of the monsters (`MONSTER_FIRST_TURN`) and of the mercenaries (`FIRST_MERCENARY_TURN`) are constants in the tests. Not searched for other literals in the tests. |
| slow tests | checked | All 1,023 tests under 0.35 s; the slowest are `a_guarded_node_resists_rivals_three_times_in_four` (0.33 s, a loop of rolls) and `the_battle_ai_takes_a_walled_city` (0.19 s). Needless cloning was not looked for beyond that. |


### f4x-core, task 430 (31 October 2026)

| Pass | Status | Notes |
|---|---|---|
| unit tests on the shipped content | done | `game::testutil::shipped()` reads `content/` once per process; all 1,041 unit tests run on it. `content_with_*`, `real_*` and `add_real_unit` are gone. 1.4 s, was 1.2 s (every test clones the `Content`). |
| fixtures | done | The item fixtures are gone; the item tests use the shipped items and powers. Terrains and features are called by number (`game::testutil::num`). |
| where a test lives | partly | Rule in `docs/rust_coding_rules.md`; `scripts/classify_core_tests.py` lists about 140 tests that need only the public API. One file moved (`city/gate.rs`, 6 tests to `tests/it/`). |
| tests that bend the content | reviewed | The blanket ones (in a shared `world()`) are gone; the rest change what the test is about. |

Open: the loader in `testutil.rs` is a second, smaller copy of the `f4x-data` loader and can drift; about 140 tests still to move; measure the run time again after that. The comment review of the crate was by search, not line by line.

## crates/f4x-data (31 October 2026)

Reviewed with the same checks as `f4x-core`. All 72 tests (0.15 s) were already on the shipped `content/`; the rest of the crate's tests either run `load_content` or edit a shipped file as text to provoke one error.

| File | Status | Notes |
|---|---|---|
| `src/lib.rs` | done | 28 tests. 55 `unwrap`/`panic!` sites in tests are `?` (the tests return `TestResult`; an `Option` is `.ok_or("..")?`), helpers and closures use `expect`. Two `panic!("expected invalid")` are `return Err(..)`. The 10 copies of the path `../../content` are `testutil::content_dir()`. `shipped_content_generates_a_map` built a `Content` of terrains and features only; it runs `Game::with_generated_map` on the whole shipped content. Dead helper `shipped_features` removed. Comment "(task 174)" removed. |
| `src/rules.rs` | done | 16 tests, 48 `unwrap` sites gone. `rules_with` read and parsed 11 files with 11 `unwrap`s; one generic `shipped` helper does it with a message that names the file. `find` keeps a `panic!` with the missing id (a helper for the tests that edit a definition). Test name lost "until local texts arrive" (future wording). `problems_after` copies every shipped definition list to change one of them; kept, nine tests, a few ms each. |
| `src/save.rs` | done | 4 tests, 22 `unwrap` sites gone. The saves went to a fixed folder `target/test-saves/<name>` that outlived the run; they use a `TestDir` now. `game()` returns a `Result`, loads the content once (it cloned it) and takes the map size from `GenParams` (was 40 and 60). |
| `src/sound.rs` | done | 2 tests on the shipped content, 5 `unwrap`s replaced. |
| `src/spell_art.rs` | done | 5 tests (3 on made-up tables, 2 on the shipped one), 6 `unwrap`s replaced. |
| `src/testutil.rs` | done | New: the folder of the shipped content for the unit tests (compiled for tests only; says so in a `//! Tests:` line). |
| `tests/it/autobuild.rs` | done | `panic!("{other:?}")` is an error that names the item; the other `unwrap` is `expect`. |
| `tests/it/original_content.rs` | done | 9 tests against the original's tables. 4 `unwrap`s are `expect` with the cell they parse; "(task 404)" removed from the module doc. **Open:** `original_retort_available` still has a `panic!` for a retort name the table does not know (a lookup in a helper, not a check). |
| `tests/it/shipped.rs` | done | New: `content()` and `content_dir()`, replacing three copies of the loader. |
| `tests/it/stacking.rs` | done | One `unwrap` is `expect`. |
| `tests/it/text.rs` | done | Three helpers return `Result`, four tests return `TestResult`; 10 `unwrap`s gone. |
| `tests/it/main.rs` | done | `mod` lines. |

**`crates/f4x-data` is finished.** Not looked at: whether the pinned counts (`35` heroes, `250` items, `66` powers, `41` help screens) are stated anywhere but these tests.

## crates/f4x-client (31 October 2026)

Reviewed with the same checks as the other crates. 169 tests became 224 (0.1 s). The crate has 88 files; 39 had no test and no `//! Tests:` line.

| Pass | Status | Notes |
|---|---|---|
| shipped content in tests | done | `test_app::shipped` (`content()`, `texts()`, `settings()`, `spell_art()`) parses each file once per run; eleven tests that each called `f4x_data::load_*` use it, and `App::for_test` clones from it. No test builds a unit, spell or terrain by hand. `craft_screen` built 24 invented item powers; its tests run on the shipped powers now (the column test on the powers a Misc item takes, 26 plain ones, so the split into two columns is exercised). |
| `unwrap`/`expect`/`panic!` in tests | done | About 100 sites. Tests that call `Result` code return `TestResult` and use `?`; `Option` setup uses `expect` with a reason. Fixed folders under `target/test-saves/` (preferences, save slots) are `TestDir`s. `race_summary_key(..).unwrap_or_else(panic!)` is an assertion with the race's id. |
| comments | done | 52 task references removed from 36 files; "Plain box for now", "until the win and lose animations exist" and "until the files' ship pictures are used" cut. Not read line by line: wordy comments are probably still there. |
| `//! Tests:` lines and new tests | partly | 20 files gained tests: `main.rs` (`parse_args` takes the argument list now), `autobuild_dialog`, `battle_screen/bar` (range and moves text), `console_screen` (`wrap_line` split into `wrap_by`, which takes the width test), `diplomacy_prompt`, `diplomacy_screen`, `river_art`, `frame`, `page_turn`, `upkeep_prompt`, `road_plan`, `unit_window`, `temp_effect_view`, `attack_prompt`, `city_screen`, `production_screen`, `create_screen`, `map_slide`, `combat_result`, `level_up`, `advisor`, `global_spell`, `square_anim`, `wizard_anim`, `view`. Five files say they only draw. **Nine files say "none yet"** because their decisions sit in the frame function that also draws (`battle_screen/pointer`, `event_popup`, `fighter_pick`, `learn_spell`, `offer_prompt`, `pointer`, `resurrect_prompt`, `sell_prompt`, `settings_screen`): task 432. |
| dead code found | done | `advisor.rs` indexed `ADVISORS[which]` in an arm no caller reaches, except `--advisor 9`, which panicked; the arm and the text key `advisor.not_yet` are gone and the flag says "0 to 8". `global_spell::any_input` was `Input::dismisses` under another name; five screens use `dismisses` now. Three empty `impl App {}` blocks removed. |
| slow tests | checked | All 224 run in 0.1 s together. |

Not looked at: the screenshot flags (`--shot` and its options) as a whole, and whether the client's string lookups of terrains, features and spells should be numbers (task 418).


## crates/f4x-console (31 October 2026)

96 tests (94 in `tests.rs`, 2 in `args.rs`), 1.4 s together; the slowest, at about 1 s each when run alone, play 60 turns or draw five wizards, which is what they test.

| Pass | Status | Notes |
|---|---|---|
| shipped content in tests | done | Three tests and `game()` each parsed the content; `content()` parses it once per run (`OnceLock`) and the tests take a reference. Only `rivals_without_ready_made_wizards_draw_a_name_from_their_list` clones it, because it empties the wizard list. No fixtures were hand-built. |
| `unwrap`/`expect`/`panic!` in tests | done | 281 `unwrap`s. 28 tests return `TestResult` and use `?` (`HeroError`, `MagicError` and `BattleError` have no `Error` impl, so they go through `map_err`); `Option` lookups are `expect` with a reason. The `panic!` in `arm_human` is an `expect`. |
| outdated test | done | `only_known_battle_spells_with_effects_can_be_cast` said Warp Wood is "not yet in the game" and asserted only `!ok`. The core has Warp Wood; it was refused because the target has no missile. The test (renamed `a_battle_spell_is_refused_for_the_reason_it_cannot_be_cast`) now asserts the reason of each of the three refusals. |
| comments | done | "(task 152)" and "(task 43)" removed; "taken out for now" in the crate doc says what is true. |
| `//! Tests:` lines | done | All eight source files have one; `setup.rs` only re-exports the core setup. |
| `args.rs` tests | done | Return `TestResult`; the error cases assert `ConsoleError::Parse`, not `is_err()`. |

Not looked at: `tests.rs` is one file of 3400 lines for the whole crate; splitting it per command module (query, legal, talk, meta) would make a failing test easier to find.

## crates/f4x-tools and tools/xmi-play (31 October 2026)

`f4x-tools`: 15 files, 36 tests became 41 (0.2 s). `xmi-play` (its own workspace, so `cargo test` there): 0 tests became 9.

| Pass | Status | Notes |
|---|---|---|
| shipped content in tests | done | `testutil.rs` (new, test-only) holds `content_dir()` and a cached `shipped()`; `metrics` (two tests) and `screens` used a relative `../../content` path and loaded it per test. |
| `unwrap` in tests | done | 30 sites in `refile`, `screens`, `audio`, `art`, `lbx`, `metrics`; tests that call `Result` code return `TestResult`. |
| `unwrap` in code | done | `soak.rs` locked its mutexes with `unwrap()`; a poisoned lock now gives the data back (`PoisonError::into_inner`), so one failed game thread does not hide its own error behind a second panic. |
| new tests | done | `soak`: option reading (range, single seed, bad value, missing value, unknown option), `jobs()`, `in_parallel` (order and first error). `xmi-play`: the XMI reader on songs built byte by byte (note-off from the duration, delays, tempo, loop controllers dropped, bad files with their reason), the DT1 checksum, the timbre and patch addresses, the timbre upload, and `wav_bytes`. |
| comments | done | "(task 146)" and "(task 363)" removed. |
| `//! Tests:` lines | done | `main`, `cache`, `contestant`, `console_cmd`, `soak`, `testutil` in `f4x-tools`; `terminal` and `main` in `xmi-play`. `cache`, `console_cmd` and whole soak games are run by hand. |

Not done: two clippy notes in `xmi-play` (`chunks_exact` with a constant size, which `as_chunks` replaces) in code that already worked; `console_cmd` and `cache` have no test, and `cache`'s fingerprint could be tested with a `TestDir`.

**The review order is finished** (f4x-ai, f4x-core first pass, f4x-data, f4x-client, f4x-console, f4x-tools, xmi-play), except `f4x-ai-ref` (frozen) and `f4x-learn` (a separate experiment).
