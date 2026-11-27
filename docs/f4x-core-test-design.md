# f4x-core: test design

`f4x-core` is the game: the deterministic simulation every other crate sits on. All change goes through
`World::apply(Command) -> Vec<Event>` (53 commands); a refused command comes back as `Event::Rejected(Rejection)`,
where `Rejection` wraps the 22 per-feature error enums and gives each a text key. Around `apply` are the save and
load of a game (`Snapshot`), replay (a snapshot plus the commands since), undo, the turn processing (`turn`), and the
subsystems: map, units and movement, cities and the economy, wizards and spells, items and heroes, features and
lairs, diplomacy, world events, battle. It has no I/O and no rendering. The AI and the client drive it; the
newgame crate makes the world it starts from.

## Why a rebuild, and how

The old suite grew with the code: 1165 tests in 39 test files and 44 inline test modules, `battle/tests.rs` alone
held 138, written one commit at a time. It has no map of what it guards. Names such as
`rivers_pay_twenty_percent_and_river_mouths_thirty` or `the_worked_area_is_five_by_five_without_corners` state a rule
of the game as a fact of the test, which `docs/rust_coding_rules.md` forbids; others test one example where the rule
is "for every unit, every city, every seed". Nobody could say what was *not* tested.

So the method is a clean sweep, not a port:

1. All 1165 tests were **removed** in one commit (22 November 2026). The old files are kept for reference, git-ignored,
   in `reference/f4x-core-before-test-rebuild/` (the whole `src` and `tests/it`). The test helpers stay
   (`world/testutil.rs`, `feature/testutil.rs`, the `put_*` and `tr` methods): they show as unused until used again.
2. A test comes back **only for a need**: a way the crate can fail that this document names. The old names are the
   checklist of what the game once cared about, read for needs, not copied. Where several old tests say one thing
   they come back as one; where a test pins a rule it comes back reading that rule from the shipped content, or not
   at all.
3. Each subsystem is rebuilt and then **mutation-tested** (`cargo mutants -p f4x-core --file ...`, through
   `scripts/cool_run.sh`) to see which needs the tests still miss.

The rules of `docs/rust_coding_rules.md` hold: shipped content, no hand-built rule sets, no game rule or default
value in a test, a test performs an action and checks what the player sees.

## What core can and cannot use

- **No `new_game`.** The crate that makes worlds sits above core. Core tests load the four saved fresh games
  `testutil::fresh(wizards, seed)` (`tests/saves/fresh_w{2,3}_s*.save`), which `f4x-newgame`'s `fixtures` test keeps
  equal to what it makes. They carry a content fingerprint: any change of a `.ron` file means a refresh.
- **No game to hold the dice.** What draws takes `&mut GameRng`. `fresh` returns a `testutil::Table`: the
  world with the generator where the setup left it (`fresh_w*_s*.rng`), reading and changing like a `World`
  and with the drawing operations as methods; `Table::empty(seed)` is an empty one, and the macro
  `drawing_ops!` gives `f4x-master`'s `Game` the same methods for tests. A test that says two games are the
  same, or that nothing happened, compares the draws too (`support::state`).
- **No game, no turn order.** Core does not depend on `f4x-master`, for tests either. A core test calls the single
  world operations (`start_round`, `start_player_turn`, `end_player_turn`, `end_round`), the ones its need is
  about. A test that needs time to pass, replays or undo is a test of `f4x-master` (`tests/it`), in a module named
  after its need; no core helper repeats the turn order. The command sweep both use is
  `testutil::sweep`.
- **No AI.** The battle player of core's tests is `testutil::battle_policy`.
- **Long plays are not core's tests.** A game played for many turns with `new_game` and the AI uses several
  crates; those are the soaks of `f4x-tools` and tests of a crate that sees them all. Core's tests stay to what
  core can do alone: fixtures, short scripted plays and hand-made situations. The **checkers** `world_problems`
  (class 2) and the digest of a game (class 8) are public in core (behind the `testutil` feature), so tests of
  other crates call them without carrying a copy; today those are the tests of `f4x-master`, and no long play
  with the AI does.
- **Small worlds by hand** are made by the `put_*` helpers on a `World` loaded from a fresh save, never by
  editing the rule set.

## The failure classes (objectives)

### 1. A refused command changes the world

A command that answers `Rejected(_)` must leave the game exactly as it was: no gold spent, no unit moved, no random
number drawn, nothing recorded for undo or replay. The ways it fails: a check that comes after the first
side effect, a refusal that is raised after a loop that already changed part of the stack, a `?` in the middle of a
handler.

Guard: for every command, a refusal situation per `Rejection` reason the handler can give (wrong owner, no mana, not
enough gold, out of range, blocked square, game over), and the whole snapshot text, the random stream position and
the replay record compared before and after. Every `Rejection` has a text key that exists in the content
(`f4x-lang`), and every enum variant is reached by at least one refusal test or is named as unreachable.

### 2. An accepted command breaks what must always hold

The world has invariants that no single command is about: every unit and city belongs to a player that exists; a
unit stands on a square it can be on, and a square holds no more than the limit; a city's tallies (food, production,
gold, upkeep) equal the sum of their parts; mana, gold and population are not negative; a wizard's spell library,
books and retorts fit the sheet; no hero holds an item twice; battle units are in the cell they claim; the order
queues name things that exist.

Guard: an **independent checker** `world_problems(&World) -> Vec<Problem>`, written from the rules and not from the
code under test, with negative tests in core (each invariant broken on purpose is found). Core runs it on the fresh
saves and after short scripted plays; `f4x-master` runs it turn after turn over the same worlds and settings.

### 3. The same inputs give another game

Same snapshot and same commands give the same events and the same final text: on another run, in another order of
other games on the same thread, on another thread. The ways it fails: a hash-ordered container walked in order, a
float in a decision, a clock or a global, a random draw taken in an order that depends on memory addresses (entity
ids), a draw that is skipped in one branch.

Guard: two runs compared; a game played alone and one played interleaved with another; a loaded game against the one
it was saved from (class 4). The fixtures pin worlds to files (class 8).

### 4. Save, load, replay and undo lose something

The three paths that rebuild a game from text. The ways they fail: a field the snapshot forgets (so a loaded game
differs from the saved one on turn 40, not at once), an entity id used where a creation number must be, a content
change that loads without being noticed, a version or fingerprint check that lets a foreign file through, an undo
that restores everything but the random stream or the fog.

Guard:
- **Round trip over everything**: for each fresh save and for games played on for N turns, `snapshot -> text ->
  restore` gives an equal snapshot, and playing both on gives equal events (the saved game against the live one).
- **A field census**, as a compile-time guard rather than a test: `World::snapshot` destructures `World` with every
  field named and none left to `..`, so a new field breaks the build until it is put on a side (kept, or left out
  for a reason given there: a cache, the session, given again at the load). The census states what is left out;
  the round-trip tests below show that what is kept is kept whole.
- **Replay**: a recorded game replays to the same state; commands name units by creation number; a command naming
  nothing is reported.
- **Undo**: after each undoable command, `undo` gives the snapshot from before it; a command that draws, explores or
  starts a battle clears the history.

- **Attacks on the files**: truncated, edited, wrong version, wrong game version, wrong layout, unknown content id,
  wrong content fingerprint, settings out of range, a wizard with no player: each gives a typed `LoadError` and never
  a panic.

Replays and undo belong to the game, so their tests are `crates/f4x-master/tests/it/{replay,undo}.rs`, with
`refusal.rs` for what a refused command leaves in them.

### 5. A bad command panics, or is accepted

`apply` takes whatever a client, a script or an AI sends. The ways it fails: a stale entity (dead unit, a city that
was razed), a player id that does not exist, a square off the map or on the wrong plane, a number at the end of its
range (0, 255, `u32::MAX`), a command from a player who is not to move, any command after the game is over, a
battle command outside a battle and a world command during one.

Guard: a table of every command with bad arguments of each kind: the answer is a `Rejection` with a key, or a
documented no-op, never a panic (`catch_unwind` as in the newgame suite), and class 1 holds for it.

### 6. The code and the content disagree

Numbers the code names (a spell, a retort, a building, an ability) must agree with the shipped rows; a setting that
changes a rule must change what the player sees, under each of its values. Content rows no code reads (a hook no code
implements, an effect kind with no handler) are found by `hook::problems` for hooks; the same idea for the other
rows the code reads by number.

Guard: a coupling list like `f4x-newgame`'s `coupling.rs` for the numbers core names, checked against the content; a
test per rule setting that runs the action under each value and checks the outcome (rule: "where the outcome depends
on a setting, run the action under each value"). Every spell, ability and building of the content is used by at
least one play in the matrix (`scripts/untested_content.py` shows what no test names).

### 7. The turn and the numbers

The turn processing and the economy do arithmetic over everything: food, gold, mana, upkeep, growth, healing,
research, unrest. The ways it fails: an overflow or a wrap at the extremes (a city of the size cap, a treasury at the
limit, a stack of the most units, a 2000-turn game), a rounding that differs between the tally and the sum of its
parts, an order dependence (the result changes with which player or city goes first), a value that is applied twice
(on turn start and on round start).

Guard: for each tally, an independent recomputation from the parts, run over the matrix (class 2 covers the
fresh worlds; here the extremes, built with the `put_*` helpers); the turn run with the players in another order;
the game played on past the usual end.

### 8. A change moves the game without being seen

Guard: seeded plays whose digest (not the whole text) is pinned in a file, so a rule change that moves a game is a
visible, deliberate act: the file is regenerated and the diff reviewed. The pinned digests are not a rule; they
record what the game does today. The digest function is core's; core pins what a run of commands does to the fresh
saves, `f4x-master` the plays with turns. No play with the AI is pinned.

### 9. Subsystem behaviour

What a player sees from one subsystem: a city grows and starves, a unit crosses water with a ship, a spell
cast is resolved, a hero levels, an item is crafted, a treaty is made. These are the old suite's bulk, and the part
the rebuild reads for needs. A test here is **one action and the outcome the player sees**, for the cases the
content *allows* and *refuses*, with the numbers read from the content. Grouped as one `tests/it` module per
subsystem; unit tests stay inline only for pure private logic that cannot be reached from outside.

The subsystems, with the old suite's size (tests), as the order to work in once classes 1 to 8 stand:

| Subsystem | Old | Subsystem | Old |
|---|---|---|---|
| battle | 169 | player (treasury, offers) | 59 |
| spell | 156 | item | 44 |
| wizard | 131 | feature (lairs, nodes) | 43 |
| unit (movement, transport) | 112 | map | 39 |
| game (save, replay, actor) | 81 | world_event | 37 |
| turn | 75 | threat | 27 |
| diplomacy | 71 | hero, rules, path_grid, hook | 43 |
| city | 62 | | |

## Structure

`crates/f4x-core/tests/it/`, one binary: `main.rs` lists a module per class and subsystem (`c1_refusal`,
`c2_invariant`, `c4_persist`, `c5_input`, `c6_setting`, `c6_tile`, `c8_drift`, then a `c9_` module per subsystem), and
`support` holds the world loader and the matrix of fresh saves. The checkers (`world_problems`, the digest) and the
command sweep live in core itself behind the `testutil` feature.

What of a class needs turns to pass is in `crates/f4x-master/tests/it`:

| Class | Modules there |
|---|---|
| 1 refusal | `refusal` |
| 2 invariants, turn after turn | `invariant` |
| 3 determinism (all of it) | `determinism` |
| 4 save, replay, undo | `persist`, `replay`, `undo` |
| 6 settings, over the turns | `matrix`, `setting`, `tile` |
| 7 the turn and the numbers (all of it) | `arithmetic`, `treasury` |
| 8 drift of the turns | `drift` |

Names follow the project's singular rule.

## Order of work

1. `support` and the checkers: the fresh-save matrix, `world_problems` with its negative tests, the digest, the
   short scripted driver.
2. Classes 4 and 1 first (the surfaces everything else leans on), then 5, 2, 3.
3. Classes 6, 7, 8.
4. The subsystems of class 9, biggest risk first (turn, city, unit, spell, battle), each with a mutation pass.
5. Mapping table of old tests to new ones, and the findings, added to this document as they come.

## Findings

Found by the rebuilt tests, each fixed in the code with the test that found it:

1. `CreateUnit` for a player the game does not have panicked (class 1 sweep). Every command now goes through a
   gate: a command that acts for a player the game lacks is refused with `Rejection::NoSuchPlayer`.
2. A command naming a square off the map (`GoTo`, `CreateUnit`, `FoundCity`, `PlaceFortress`, `EnterSite`,
   `EarthGate`, `BuildRoad`) panicked in the path grid or the map (class 1 sweep). Refused with `MoveError::OffMap`.
3. A refused lifecycle command (`StartPlayerTurn` for no player, `EndPlayerTurn` and so on) wiped the undo history
   although it changed nothing (class 1 with undo on).
4. A cleared site with nothing left to give for its book or retort, and a full hoard of items, told the winner of
   1000 mana alone although the treasure's own mana was paid too (class 9, feature): the message now holds both.

Found by the rebuilt tests, each made a task for the user to decide (28 November 2026):

5. Raiders and monsters compared landmass numbers without the plane, so a player on one plane counted on the other
   (class 9, threat): fixed (task 842).
6. A node or tower cleared by assault keeps its guard list, so it never counts as empty (class 9, threat): left to
   task 829, with an ignored test.
7. The documents and the code disagreed about the heroes of a defeated wizard and of a city that turns neutral
   (class 9, hero): they are free (task 844).

Seen, harmless in play and left alone: `World::cheat_add_building` accepts any entity that has an owner, a unit too
(the console resolves a city first); the hook `disintegrate` is listed in `hook::CUSTOM_HOOKS` although no content
carries it and no code acts on it.

## Open questions

- A command that is refused in part (looked for, not found): `forget_if_rejected` drops a whole command from the
  replay when any event is a `Rejected`, which would lose a command that changed the game and also reported a
  refusal. The code does not seem to do that: a standing order that cannot go on reports `OrderFailed`, and the only
  two places that build a `Rejected` after other work (diplomacy, moving a building by spell) report it for an
  `Err` alone. The refusal sweep, which now includes every diplomacy command, found no refusal that changed the
  game on the four worlds, at the start (core) and after 4, 12 and 25 rounds of play (the slow variant in
  `f4x-master`, `refusal.rs`, `--ignored`).
  If a command ever mixes the two, this sweep is where it shows.
- Which invariants of class 2 the rules really demand: the list above is a start from reading names, to be checked
  against `docs/design.md` and the code as the checker is written.
- Long plays with the AI that call the checkers do not exist. They would belong in `f4x-ai/tests/it`, which has the
  dependencies, or in a crate of their own (`f4x-tools` has the soaks but is a binary).
- How much of class 9 survives the first pass: the rebuild decides by need, and a subsystem that already has its needs
  covered by classes 1 to 8 gets few tests of its own.

## Normal and deep runs

The suite is run after every change, so it has to be short (about eleven seconds with nextest). A test that
sweeps a range does not run every value there: two sizes of the same test.

- **Normal:** the lowest value, the highest and the default of a range (`f4x_def::testdir::spread`), one world of
  the four, a sample of the damages, a stride through the commands (`sample`).
- **Deep:** every value, every world, every damage (`F4X_DEEP=1`, `scripts/test_deep.sh`, about four minutes). Run
  it when a normal run breaks and the breaking point is wanted, and now and then before a release. A sweep's failure
  message names the value, and the matrix's says to rerun deep.
- **Mutation tests** (`scripts/test_deep.sh --mutants CRATE FILE...`) are not part of either: they take hours,
  run under `cool_run.sh`, and are run on the files a class of tests was written for, to see what the tests
  miss. The missed mutants that matter become tests; the equivalent ones are left.
- A new sweep: take the values from `spread(range, &default)` and its breadth from `deep()` / `sample(all, normal)`;
  never `for v in range` over a range of more than a handful of values.

`F4X_ATTACKS=n` still sets the number of numbers attacked in the content loading tests, for any count.

## Class 8 in use: the pinned digests

`check::digest(&world)` cuts the saved text at the fields of the snapshot and hashes each (the versions left out);
the pins add the seed and the number of draws as they are, and in `f4x-master` the round and the end.
Short scripts are played on the four fresh saves, digested at the checkpoints and compared with pins
(`f4x_core::world::testutil::pin`). There are two sets:

| Plays | Test | Pins | Regenerate |
|---|---|---|---|
| every command of the sweep on a world | `f4x-core`, `tests/it/c8_drift.rs` | `crates/f4x-core/tests/pins/core_drift.txt` | `F4X_REFRESH_PINS=1 cargo test -p f4x-core --features testutil --test it c8_` |
| the same sweep then three turns, rounds of the scripted walk, bare turns | `f4x-master`, `tests/it/drift.rs` | `crates/f4x-master/tests/pins/drift.txt` | `F4X_REFRESH_PINS=1 cargo test -p f4x-master --test it drift` |

A failure names each checkpoint and the fields that moved. Regenerate on purpose and review the diff: the
fields that moved should be the ones the change is about. A content change moves the pins too (after
`F4X_REFRESH_FIXTURES=1 ...`, refresh the pins). The pins record what the game does today; they are not a rule.
