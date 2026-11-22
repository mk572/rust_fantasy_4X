# f4x-def: test design

`f4x-def` reads setting files from disk (`content/*.ron`, a mod's layer, the player's settings) and offers
static values to the rest of the game: typed tables, ids, fixed-point numbers (`HalfInt`, `QuarterInt`), stat
modifiers, ratings, bit sets. Every other crate stands on it. So its tests guard against the ways such a
module fails; they do not restate what the content says.

The old suite (about 93 tests) is kept for reference, git-ignored, in
`reference/f4x-def-before-test-rebuild/`. The suite was rebuilt from the five objectives below (task: test
rebuild, November 2026).

## The five failure classes (objectives)

### 1. Bad content reaches the game unnoticed

The content is hand-edited and large. Mistakes: a reference to something that does not exist (a unit names a
missing ability, spell, building, race), a repeated or missing number, a value out of range (negative cost,
percent over 100), a contradiction inside the content (a building replaces nothing, a race cannot build what
its list needs, a level path that does not lead anywhere).

Guard: (a) the validator runs over the **shipped** content and finds nothing; (b) for each *rule class* of the
validator one negative test shows that the rule fires (a rule that never fires is a rule that is not there).
A negative test starts from the shipped content and breaks exactly one thing; it never builds a rule set by hand.

### 2. Loading itself fails, or fails badly

A missing or unreadable file, malformed RON, an unknown field, a wrong type, a truncated or empty file, a
file that is not UTF-8, a local override that silently wins or silently loses, an error that does not say
which file / row / field is at fault.

Guard: one test per kind of failure. Assertions are on what the error *identifies* (the file, the key, the
number), never on its exact wording. Loading never panics on any input: every failure is a returned error.

### 3. Content drifts without anyone intending it

The hash guard (`content.sha256`, `content_hash`) turns every change of a `.ron` file into a visible,
deliberate act (`scripts/ron_hashes.py --update`). Guard: the shipped files equal the record; and the guard
itself reports a changed, an added and a removed file (otherwise it could silently guard nothing).

### 4. The derived values are wrong

The computed values: `HalfInt` and `QuarterInt` arithmetic and rounding, stat modifiers (adds before percent,
floors, bounds), veteran stats, ratings, forecasts, bit sets, id and number tables.

Guard: properties rather than hand-picked cases: round trip (write then read gives the value back), exactness
(what is representable stays exact), monotonicity, order independence, no overflow or panic at the extremes of
the type (`i32::MIN`, `i32::MAX`), rounding direction for negatives. The expected values come from an
independent restatement of the *definition* in the doc (for example "halves are twice the integer"), never from
a copy of the implementation, and never from a tuning number of the content.

### 5. Code and content disagree

Rust enums, constants and ids that must match rows of the files: a variant with no row, a row with no
variant, a constant that points at the wrong row, a unit effect no spell gives.

Guard: one test per coupling point, written generically (every variant of the enum has its row, the row's
alias is the variant's name), so a new variant is covered without a new test.

## What a test here may not do

- Carry a game rule (a limit, a count, a price, a tuning default): it reads it from the shipped content or
  from the settings it is checking (`docs/rust_coding_rules.md`).
- Hand-build or edit a rule set (the memory note "tests use shipped rules"). A negative test copies the
  shipped content into a test folder of its own (`testdir`) and changes one file there.
- Restate the implementation: if the only way to say what is expected is to repeat the code, the test is
  deleted and the thing is covered by a property or by the validator.

## Attack catalogue (mutation checks)

After the suite was rebuilt it was attacked on purpose, to find what it does not catch. Two kinds:

1. **Mutations of the code**: small wrong changes to the source (`+` to `-`, `<` to `<=`, a dropped check, a
   saturating op to a wrapping one, a returned default instead of an error), one at a time; the suite must fail
   on each. A mutant that survives is a hole: either a test is added or the mutated code is shown to be dead
   or equivalent (and perhaps removed).
2. **Attacks on the input**: the loader is fed missing, faulty, mistyped, corrupted and extreme input:
   missing file / folder / key; wrong type; negative and out-of-range numbers; `i64`/`u32` extremes; empty,
   truncated, binary and non-UTF-8 files; a BOM; a very long string, deep nesting, a huge number of rows;
   duplicate keys; unicode and control characters, path separators, `..`, NUL and format characters in ids and
   names (injection into paths, into text keys, into formulas); a symlink loop; an unreadable file.

The results are in the section **Findings** below.

## The suite

`crates/f4x-def/tests/it/`, one test program, one module per class: `c1_content.rs` (the validator on the
shipped content, one negative test per rule), `c2_loading.rs` (missing, damaged, mistyped, unknown, layered,
extreme input), `c3_drift.rs` (the hash guard), `c4_values.rs` (fixed-point numbers, stats, bit sets, settings
arithmetic), `c5_coupling.rs` (code against content). `support.rs` has the copy of the shipped content that a
test breaks one file of (`edit::<Vec<UnitDef>>(dir, "units.ron", |rows| ...)` changes the rows through their
type and writes the file again) and `dangling`, an independent walk over loaded content that finds every
reference that leads to no row.

The attacks of class 2 are part of the suite: every number of every file is made negative, too big, fractional,
not a number and not a number-shaped thing (`attack_numbers`), a key no type has is put in every struct of every
file, and every file is damaged in 30 ways. A normal run tries a few hundred numbers; `F4X_ATTACKS=5000 cargo
test -p f4x-def --test it c2` tries thousands (a minute or two).

## Findings

What the first run of the attacks found, and what was done. Every item has a test that fails without the fix.

| # | Hole | Class | Fix |
|---|------|-------|-----|
| 1 | A number twice or out of order in a content file made `Registry::new` **panic** (1400 of 5000 attacks) | 2 | `Registry::try_new`; the loader reports the file |
| 2 | The lair check **overflowed** adding weights near `u32::MAX` | 2 | sums in 64 bits |
| 3 | `terrain.ron` and `features.ron` errors did not say which file | 2 | `ContentError::in_file` |
| 4 | References accepted that lead nowhere: `rules.ron` (`autobuild_order`, `starting_buildings`, `limited_ai_races`, `event_ores`) and `features.ron` (`realm`, `reward`, `transmute`) | 1 | `check_config`, `check_feature_references` |
| 5 | A misspelled key was **dropped silently** (every struct, also inside effects) | 2 | `deny_unknown_fields` on the content types; `rules_settings.ron` (types shared with saves) is checked by `unknown_keys` |
| 6 | An optional file that was a link to nothing counted as missing | 2 | `is_there` looks at the link, not what it leads to |
| 7 | `HalfInt` written as its smallest value could not be read back; `scaled` and `percent_nearest` wrapped to a wrong value; `ceil`, `half`, `plus_half`, `abs` overflowed at the extremes; `QuarterInt::percent` and `ceil` too | 4 | 128/64-bit arithmetic, saturation, range check that includes `i32::MIN` |
| 8 | `scaled_count` and `squares_per_player` overflowed for a big map size (a save or the player's file is not validated) | 4 | 128/64-bit |
| 9 | Ids of land and map sizes (the end of a text key) took any character, a repeat was not reported | 1 | `id_problem` and a repeat check |
| 10 | A help topic could include itself; a picture could be a path (`../x`) | 1 | `Help::problems` |
| 11 | The hash guard followed a link back up the tree (a loop) | 3 | links are not followed |
| 12 | `GameSettings::normalised` left out of range: mineral percent, city size, city distance, event start turn, units per side and per square, nodes, sites; `DefaultGame::load` did not normalise | 4 | clamps added, `load` normalises; a property test holds `normalised` and `validate` to the same ranges |
| 13 | A `DefaultGame::save` error did not say which path | 2 | the path is in the message |

Left as they are, on purpose (the test says so):

- An emptied `rules.ron` (`()`) loads the built-in defaults: the type is all defaults. A missing one is an error.
- Operators on `HalfInt` and `QuarterInt` (`+`, `-`, `*`, `/`, negation) panic on overflow in a debug build and wrap in
  a release build, as integers do; the functions that can reach the largest value (a search starts from
  `HalfInt::MAX`) saturate.
- `Custom` hook names in the content are strings: a misspelled hook is not an error here (the code that runs the
  hook lives in `f4x-core`).
- Costs and similar `u32` values have no upper bound.

## Mutation pass (22 November 2026)

A sample of 240 of the 1353 mutation sites of `f4x-def/src` (operator swaps, literals plus one), one at a time on a
copy, the suite (`tests/it`, `F4X_ATTACKS=60`) run on each. Same sample (seed 1) three times:

| Run | Caught | Not caught | Did not compile |
|---|---|---|---|
| 1 (first suite) | 89 | 143 | 8 |
| 2 (after the first additions) | 136 | 96 | 8 |
| 3 (after `c6_behaviour.rs`) | 151 | 81 | 8 |

Of the 81 not caught in run 3, 78 are in the **Rust copies of game values** (`rules/defs.rs` 64, `settings.rs` 13,
`rules/lair.rs` 1): built-in default tables and limits that the shipped files override, so no test sees a changed
default. They go with the defaults themselves (`docs/content-policy.md`, task 766); do not write tests for them.
The other three:

- `hero.rs` (a zero modifier is dropped): a test was added after run 3.
- `load/rules.rs` 724: the `2` of the "done" mark of the include-cycle walk; any value other than 1 does the same, so
  the mutant is equivalent.
- `rules/retort_stat.rs` 25: only a passive retort effect counts. No shipped retort has another trigger, so the
  mutant is equivalent on the shipped content; a mod could differ. Left without a test until a rule uses it.

`cargo-mutants` was not used (not installed; the machine rule is to ask before installing). The script of this pass
is a throwaway one, not in the repo.
