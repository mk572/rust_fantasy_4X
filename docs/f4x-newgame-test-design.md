# f4x-newgame: test design

`f4x-newgame` makes a new world and its wizards: the map generator (two generators: the noise one and the
original's), the placement of towers, nodes, encounter sites and minerals, the guards and treasure of each site,
the nodes' auras, the islands that are built out, the computer wizards, the start of every wizard (race, fortress,
army, view), the neutral cities, the temperaments and the heroes. It runs once, when a game begins, and every
other part of the game (the client, the console, the AI soaks, the tools, the tests of `f4x-core` through saved
fixtures) starts from what it makes. It reads rules and settings; it holds almost no state of its own.

A mistake here is not loud. A world with a tower on the sea, a wizard with eleven books in a realm, a neutral city
two squares from a fortress, or a seed that gives a different map on another machine still *starts*; the harm shows
fifty turns later in a soak, or never, as a game that is quietly unfair. So the tests guard the **properties of
the world that is made**, over many seeds and settings, and the **ways the call fails**, not a handful of maps
on chosen seeds.

The old suite (86 unit tests in the modules and 22 in `tests/it`, 108 in all) is kept for reference, git-ignored,
in `reference/f4x-newgame-before-test-rebuild/`. The suite was rebuilt from the objectives below (November 2026),
in the way of `docs/f4x-def-test-design.md`.

## What the old suite did, and what it left open

Good, and kept in some form: it ran on the shipped content, never on a hand-built rule set; most tests read the
count or the limit from the settings; there is a drift guard (`fixtures.rs`, `regression.rs`).

Weak:

- **Chosen seeds, one setting each.** Most tests make one or two worlds (`seed 7`, `seed 11`) at the standard size
  and say something about them. A rule that holds on seed 7 and fails on seed 8 is not seen. No test tries a seed at
  the end of its range, a small or a large map, an extreme land share, ten wizards, either generator against the
  other, or the settings that change the placement (`extend_tiny_islands`, `roads_at_start`, `mix_neutral_races`).
- **No input attack.** Nothing feeds `new_game` a bad preset, a race of the wrong plane, a wizard build over the
  limits, a settings record out of range through `NewGame` (one test does it for two values), or a content that
  lacks a terrain class, a tower, a race of a plane, or the wizard presets.
- **Examples instead of properties.** "the_start_points_are_spread_round_the_middle_and_inside_the_map",
  "a_myrran_wizard_starts_on_myrror_and_the_others_on_arcanus" look at one game. The rule is "for every wizard in
  every game", which is a loop over a matrix of games.
- **The numbers in the code are not checked against the content.** `Retort::new(9) /* myrran */`,
  `RealmId::new(1) /* sorcery */`, `Retort::new(15) /* conjurer */` and the `OPPOSED` table of realms are numbers
  the content must agree with. If a row is renumbered, the world is built with the wrong retort and no test fails.
- **The seed is used as a number.** A seed is added to a count in the race and preset choice without care for the
  end of the range (found when this design was written: see Findings).
- **Hard-coded rules inside tests** (`the_counts_follow_the_area_with_the_originals_for_60_by_40` and a few more)
  restate the figures of the original game; `scripts/imaginary_tests.py` flags the pattern.

## The failure classes (objectives)

### 1. The same inputs give another world

The generator is integer-only so that a seed gives the same map on every platform; the setup rolls from the game's
own seeded generator. The ways it fails: a float in a decision (`setup::anchor` takes the cosine of an `f32`: the
result can differ between platforms and libraries), a hash-ordered container walked in order, a call that reads the
clock, the thread or a global (the `with_fallback` of the settings is thread-local), a roll taken from the wrong
generator so that a change of one setting shifts the rest of the world, a random stream shared between steps so that
adding one roll moves everything after it.

Guard: for each of a set of seeds and settings, two calls give equal worlds (the whole snapshot text); two games
made interleaved, in another order, in another thread, give the same as made alone; the seed is the only source:
no test reads the clock. Steps are independent where the code says so (the map does not change when only the
wizards' names do; the nodes do not change when the number of wizards does). The drift guard (class 7) pins the
worlds to files, so a change that moves a seed is a visible, deliberate act. What cannot be tested on one machine
(another platform) is written down in the doc and kept out of decisions: floats are checked to give the same
integers over the whole range they are used for (`anchor`: every `k` of every `n` of every size).

### 2. A bad call panics, or is accepted

`new_game` is the entry of the client's screen, the console, the tools and a mod's content. Its input: the
`NewGame` (seed, preset, race, build, books, retorts, names, the settings), the content, and the settings of the
player's file. The map generator has its own call (`generate`) used by tools.

The ways it fails: a seed at the end of its range (overflow), a map size at either end or off the allowed list, a
land share of 0 or 100, a mineral share over 100, a count of towers or sites that the map cannot hold, a number of
wizards that the plane cannot hold, a preset number that does not exist, a race of the other plane, a build with
books over the limits or retorts the books do not allow, empty names, a content without a terrain class, a
tower feature, a race of a plane, a wizard preset, a hero template or a spell of a realm.

Guard: every bad input gives `Err` with a message that names the thing at fault, never a panic, never a hang
(a time limit on each attack). Every input the settings allow gives a game. The limits that the settings state
(`MAP_WIDTHS`, `MAX_OPPONENTS`, the ranges in `GameSettings::validate`) are the test's bounds, read from the code of
the settings, not typed again; both ends of every range are tried. Where `new_game` and `generate` take values the
settings would refuse (sizes 8 to 1024), the ends of *their* ranges are tried and the cost is bounded (a
1000 by 1000 map must not take longer than a stated bound).

### 3. The world breaks its own placement rules

The map and its features have rules that the code states in comments and constants: towers stand on land on both
planes at the same square, apart from each other, away from the poles; nodes keep their spacing, stand off the
poles and the towers, and take their node terrain; encounter sites keep off the margins and apart from everything;
minerals lie on land, never under a feature, and the minerals of Myrror stay on Myrror; every site has exactly one
lair (a tower one for both planes); a node's aura lies on the map, holds its own square and as many squares as its
power; the planes differ; the land share is what was asked within a stated slack; the poles are tundra or ocean;
the map wraps east and west.

Guard: a matrix of worlds (see *The matrix* below) and, for every one, an **independent checker** that reads the
finished `Map`, the `Placement` list and the content, through public types only, and re-derives each rule from the
terrain classes and the feature kinds. The checker is written from the rule, not from the code: it does not call
`candidates`, `clear_of_sites` or `growth`. It is one function that returns every violation, so one test can
run it on every world and a failure names the seed, the size and the rule.

Counts follow the settings: the number of towers, nodes per plane, sites (strong and weak), neutral cities are read
from the settings through the methods the game offers (`tower_count()`, `node_count(k)`, ...) and compared with
what is on the map, with the shortfall the generator may have (a plane without room) bounded and reported.

### 4. The start is not playable

After `new_game` each wizard must be able to play: a fortress on land of the right plane (Myrran wizards on
Myrror), fed (the land holds at least the start size), apart from the other fortresses (the passes relax this: the
relaxed cases are counted, and a standard map never needs them), a capital with its buildings, citizens, farmers
and army from the rules, a restricted view, the gold and mana of the settings, a banner and a portrait of its own,
a race free on its plane while one is, and the limits of the wizard rules: books per realm and in all, picks spent
not over the picks, retorts available for the books, at most the allowed number of retorts, no life with death
when banned, the Myrran share inside its range, no two computer wizards the same preset while a preset is free.
The human is set only when the game is not all-AI. The neutral cities: not over the target, on land, clear of
fortresses, nodes, towers and lairs by the stated distances, of the races of their plane, with a garrison and a
population the land holds; roads only when the setting says so. The temperaments and heroes exist for everyone.

Guard: the same matrix, with a second **independent checker** for a finished game (`start_problems(game)`), and the
property that a played game goes on: ten turns by the AI on every world of a small matrix without a refusal or
a panic, and a save made from the new game loads back equal.

### 5. A rule that is a defined algorithm

Some functions are small algorithms with a definition in the docs, not behaviour of the world: the weighted rolls
(`pick`, `pick_weighted`, `personality_weights`, `pick_personality`, `objective_weights`, `pick_objective`), the
allotment of races to sites (`assign_races`), the Myrran share (`myrran_range`), the scan order (`scan_rows`), the
range (`range`), the wizard refill and consolidation (`refill`, `balance_planes`), the island growth (`growth`,
`land_mass`).

Guard: properties of the definition, with the expected value from an independent restatement or a check of a
postcondition, not a hand-picked case: a weighted roll over the whole roll range lands on each item as often as its
weight (counted exactly, since the roll is a number, not a random draw); `assign_races` returns one race per site,
uses each rolled race once, gives a race with a favoured terrain the site with most of it; `myrran_range` is
monotone and its bounds hold the whole share for every `total`; `range` is symmetric, zero on itself, and the short
way round on a wrapping map; `growth` never changes a square that is land, never takes a square that is `taken`, a
lake or off the map, gives a mass of the minimum size whenever the frontier allows, and is deterministic. These
stay as inline tests next to the code (the functions are private) and are kept small.

### 6. Code and content disagree

The crate names rows of the content by number: `Retort::new(9) /* myrran */`, `Retort::new(15)`, `Retort::new(16)`,
`Retort::new(1)`, the five realms, the retorts of the masteries, the terrain class of a node realm, the
`REQUIRED_CLASSES`. And it carries tables the content could state (`OPPOSED`).

Guard: one generic test per coupling point (the way `f4x-def`'s `c5_coupling.rs` does it): every number the crate
names resolves to the row whose alias is the one in the comment; `OPPOSED` equals the opposed pairs of the realm
rows; every `TerrainClass` the generator asks for exists on both planes; every node realm has a terrain class; the
`Retort` and `RealmId` constants of `computer_wizard` and `setup` come from one place. Where a coupling can be
removed (the number replaced by a lookup of the alias) that is the better fix and the test then pins the lookup;
the memory rule "units by number" applies in the meantime.

### 7. The world drifts without anyone intending it

Whole games are pinned to files: the fresh games `f4x-core`'s tests load (`fixtures.rs`) and the three played
scenarios (`regression.rs`, `tests/regression/*.save` and `.target`). A change of the generator or the setup moves
them, and moving them must be deliberate (`F4X_REFRESH_FIXTURES`, `F4X_REFRESH_REGRESSION`). These two files are
kept as they are, moved to `c7_drift.rs`. The guard guards itself: a test makes a world with another seed and
shows the comparison fails (otherwise it could compare a file with itself).

## The matrix

A "world" is `(seed, wizards, all_ai, size, land, settings variant, generator)`. The matrix is built from:

- seeds: 0, 1, 2, a spread of small numbers, `u32::MAX`, `u64::MAX`, `u64::MAX - 1`, `1 << 63`, and a few from a
  fixed list of "random-looking" ones;
- sizes: the corners of `MAP_WIDTHS` by `MAP_HEIGHTS` (read from the settings) and the standard size;
- land: 1, the default, 99 (through the settings' own limits);
- wizards: 2, 3, 5, the most the settings allow;
- the variants of the settings that change the placement: each of `original_map_generator`, `extend_tiny_islands`,
  `roads_at_start`, `mix_neutral_races`, `limit_ai_races`, `ban_life_and_death`, `ai.wizards.randomise`, on and off,
  one at a time against the default (never the full product);
- both generators.

The normal run takes a **cut** of the matrix that runs in a few seconds with nextest (it is run by every
developer many times a day): the standard size over all seeds; every corner size over three seeds; every variant
over two seeds. `F4X_WORLDS=N cargo test -p f4x-newgame --test it` runs N extra random worlds with a seed printed
on failure (a minute or two for a few thousand), and the soak scripts may run it overnight. A failure always names
the world, as the arguments to repeat it.

## What a test here may not do

- Carry a game rule (a limit, a count, a price, a distance of the original): it reads it from the settings, the
  content, or the code's own public constant (`island::MIN_LAND`, the spacings if they are made public), and the
  checker for placement rules takes the spacings from one place (a `pub const` of `place`) so that the test and the
  code are held to the same rule *and* a test shows each rule fires (see the negative tests below).
- Hand-build a rule set (memory note "tests use shipped rules"). A negative test copies the shipped content into a
  test folder, changes one file through its type, loads it, and gives it to `new_game`. A test of a private helper
  uses the shipped content too.
- Restate the implementation. A checker that does the same computation as the code proves nothing; where the only
  way to say what is expected is to repeat the code, the test is dropped and a property of the result stays.
- Match the wording of an error. `new_game` returns a `NewGameError`; the tests match the variant and its values, as
  `f4x-def` asserts the parts of an issue. The English wording is `Display`, for logs only.

## Negative tests: the checkers must be able to fail

A checker that never fires is not a checker. For each rule of class 3 and 4 one test makes a finished world, **breaks
it in the world itself** (a tower moved onto the sea, a node onto a tower, a mineral under a feature, a fortress
beside another, a wizard given a twelfth book, a neutral city beside a fortress: through the public mutators of
the game and of `Map`, not by building a map by hand) and shows the checker returns that violation. It is the
mutation check for the checker.

## Attack catalogue

After the suite is built it is attacked, as `f4x-def` was. Two kinds:

1. **Attacks on the input** (part of the suite, class 2): extreme seeds; every settings value at both ends of its
   range, one at a time; every value just outside; `NewGame` fields out of agreement (a preset and a build, a race
   of the wrong plane, `all_ai` with a preset); the names bundle empty, with an empty list, with one name, with
   very long and unicode names; the content with a row removed (a race, a preset, a terrain class, a tower, a
   node kind, a hero template, a spell of a realm, the five retorts the crate names) and with a row's number
   changed; `generate` at 8 by 8, 1024 by 8, a land of 1 and 99 and a mineral share of 100.
2. **Mutations of the code**: a sample of the mutation sites of `src` (operator swaps, off-by-one literals, a
   dropped check, a `<` for `<=`), one at a time on a copy, the suite run on each: a mutant that is not caught is
   a hole, a test is added, or the mutant is shown dead or equivalent. `cargo-mutants` is not installed; the
   machine rule is to ask before installing it, so the first pass uses the throwaway script that `f4x-def`'s pass
   used (not in the repo). The result is written under **Findings**.

## The suite

`crates/f4x-newgame/tests/it/`, one test program, one module per class:

| Module | Class | Holds |
|---|---|---|
| `support.rs` | | the matrix (`worlds()`), `world(seed, ...)` with a cache so that a world is made once per run, the copy of the shipped content to break (as `f4x-def`'s `support.rs`), and the time limit helper |
| `c1_determinism.rs` | 1 | same call same world; interleaved and threaded; independence of steps; the floats (`anchor`) |
| `c2_input.rs` | 2 | extreme seeds, settings at the ends and over them, bad `NewGame` fields, damaged content, `generate` extremes |
| `c3_map.rs` | 3 | `map_problems(&Generated, &Content, &GenParams)`: the independent checker and a test per rule that it fires |
| `c4_start.rs` | 4 | `start_problems(&Game)`: the same for a started game; a played game goes on; save and load |
| `c5_algorithms.rs` | 5 | the algorithms reachable from outside (`neutral::range`, `neutral::neutral_city_count`, `temperament::*`, `island::growth` through `grow_site_islands`, `computer_wizard` through `new_game`); the private ones are inline tests |
| `c6_coupling.rs` | 6 | the numbers the crate names against the rows of the shipped content |
| `c7_drift.rs` | 7 | `fixtures` and `regression` of the old suite, and the proof that the comparison can fail |

In the modules of `src` only the inline tests of private algorithms stay (class 5), each short, each a property.
Every `mod tests` that only made a game and looked at it is dropped: `c3` and `c4` do it for every world.

## Order of work

1. This document; the reference copy of the old suite.
2. `support.rs` (the matrix, the world cache, the copy of the content), then `c3_map.rs` and `c4_start.rs` with their
   checkers, since every other class uses a checker or a world.
3. `c2_input.rs` and the first run of the attacks. **The holes it finds are fixed in the code** (with a test that
   fails without the fix) and listed below.
4. `c1`, `c5`, `c6`, `c7`.
5. The old tests are removed from `src` and `tests/it` once every one of them is covered by a property above or
   shown to be an example of one (a table in this document maps the old test to its successor; a test that
   restated a rule of the original is dropped with a line saying so).
6. The mutation pass; its findings.
7. The cost: the cut of the matrix runs in the time the doc states; the old suite ran in about one second, the new
   one must stay under ten with nextest.

## Findings

What the suite found (the six from reading the code, and those the matrix and the attacks added). Each is fixed with a
test that fails without the fix, unless "left" is in the last column (then see the next section).

| # | Hole | Class | Test | Outcome |
|---|------|-------|------|---------|
| 1 | A seed near `u64::MAX` made `new_game` panic (`seed as usize + k` overflow) | 2 | `c2_input` seeds at the range ends | fixed: `seed_offset` (wrapping) |
| 2 | `setup::anchor` used `f32` trig; `sin(PI)` is not 0, so the odd-height first start row (x.5) rounded by a half-ulp | 1 | inline `a_start_row_that_is_exactly_a_half_goes_up_on_every_odd_height`, `the_start_spot_is_stable_over_every_size` | fixed: `f64`, 0 and 1 snapped, half rounds up (the margin otherwise is at least 6.3e-4) |
| 3 | `race` was applied and `preset` ignored in an all-AI game | 2 | `c2_input` all-AI choices refused | fixed: `new_game` refuses the combination |
| 4 | 1000 by 1000 at 1 percent land took 10 to 21 s (site check scanned all placements) | 2 | `c2_input` 1000x1000 under 10 s | fixed: `Taken` occupancy grid (0.36 s) |
| 5 | `new_game` returned `String`: a bad input could not be told from an unfit content, and the client could not key the message | 2 | `c2_input` (variants asserted), `c6_coupling` (every error has its message, names only what it gives) | fixed: `NewGameError` (`error.rs`) with `text_key()` and `args()`, messages in `new_game_error.ron`, the client shows them keyed |
| 6 | Towers and sites fell short with free squares left (200 random tries, then quiet) | 3 | `c3_map` matrix at 1 percent land | fixed: a fallback scan over a shuffled list of the squares; maps without shortfall are unchanged |
| 7 | A river could be flagged on open-sea squares (a later river's path over water made by an earlier one); a river may cross a lake, so lakes stay allowed | 3 | `c3_map` rule `river` (open sea only) | fixed in `mapgen` |
| 8 | Content without a named retort or realm started a game that silently misbehaved | 6 | `c6_coupling`, `c2_input` damaged content | fixed: `coupling::problems` makes `new_game` refuse |
| 9 | The original generator reaches only 53 to 61 percent land whatever is asked | 3 | `c2_input` refusal above the limit, `...makes_the_land_it_may_be_asked_for_on_every_size` | fixed: `ORIGINAL_GENERATOR_MAX_LAND` (50) in `f4x-def`; the settings refuse more for that generator and the custom screen clamps |

Fixtures: none moved in the end (the river fix, narrowed to open sea, leaves the pinned games as they were). The three played
scenarios of `c7_drift` did not move either.

## What was done with the old tests

| Old | Successor |
|---|---|
| `shipped_content.rs` (placement rules, minerals of Myrror) | `c3_map` checker over the matrix, mineral tests |
| `shipped_game.rs`: turns pass, save loads, same seed | `c4_start`, `c1_determinism` |
| `shipped_game.rs`: counts, picks, book limits | kept as `c4_pick_rules.rs` |
| `world_settings.rs`: the default counts are the original's | dropped (restates numbers of the original); the rest is `c3_counts.rs` |
| `fixtures.rs`, `regression.rs` | `c7_drift/` |
| inline `anchor` test | three property tests in `setup.rs` |

## Left as they are, on purpose

- Desert on pole rows of the original generator stays as the original has it; the pole rule is for the noise generator.
- `GenParams::default()` carries tuning numbers; the suite uses `gen_params(&settings)`.
- No negative test for the hero count (no public way to change it).
- `f32` trig elsewhere cannot be tested across platforms.

## The pairs of realms that may be merged

`OPPOSED` (the realm pairs a computer wizard's books may be merged in) was a table of the code that the content could
state. It moved into `rules.ron` as `mergeable_realms` (22 November 2026), checked on load like the other references
(`f4x-def`: a realm that is not there, a realm with itself, a pair twice are refused), and the crate reads it. The
games made are unchanged (same pairs in the same order, so the same draws); only the content fingerprint, and with it
the four fresh-game fixtures of `f4x-core`, moved.

## No copied numbers

A review of the new tests for values copied from the game (22 November 2026) changed these:

- The ends of the settings are found by asking `GameSettings::validate` (`support::knobs`, `limits`): the lowest and highest
  value it takes, and the value just outside. No range is written in a test.
- The generator's own limits are constants of `mapgen` (`SIDE_RANGE`, `LAND_RANGE`, `MINERAL_MOST`) and of `node`
  (`AURA_REACH`) that the tests read.
- The pick tests read the picks, the books and the cost of Myrran from the settings and the content (the fewest picks a
  ready-made wizard spends, the ready-made wizard with the most books in a realm).
- The tests that restated a range (`a_number_out_of_range_is_refused`) or the default wizard are gone.

What stays: the numbers of the retorts and realms the code names (`c6_coupling` checks them against the content), the
seeds and sizes that are samples, not rules.

## Mutation pass

`cargo mutants -p f4x-newgame --shard 1/10` (155 of 1544 mutants, 22 November 2026): 140 caught, 6 timeouts (an endless
loop is a catch), 5 unviable, 4 missed. The misses:

- `error.rs`, the English `Display` of three reasons: not worth a test (the message of the player is keyed and tested).
- `island.rs` `land_near`, `+` to `-` on a symmetric loop: an equivalent mutant.
- `island.rs` `growth`, `<` to `==` on the row check: a gap, closed with `growth_may_use_the_top_row_of_the_map`.

A second pass over `growth` and `land_near` found more (that shard had missed none of them because they were not in it);
three tests close the real ones (`a_mass_too_small_is_built_out_even_when_there_is_ground_to_work_round_it`,
`ground_to_work_comes_first_and_nothing_more_than_needed_is_added`, `growth_may_use_the_top_row_of_the_map`). What is
left there: the mixing of the seed into the random generator (`^`, `<<`; any mixing is as good), the direction of the four
sides (`+` to `-` over a symmetric list), and three tie details of the first-the-ground-to-work rule (`||` for `&&` at the
start, `<=` for `<` where the ground is exactly enough, `||` for `&&` on the frontier of a truncated mass). None of them
changes what a player can see on a map more than the choice of one random square.

The other nine tenths of the mutants were not run. To run one: `scripts/cool_run.sh cargo mutants -p f4x-newgame --shard
K/10 --jobs 4 -o <dir>` (about half an hour).
