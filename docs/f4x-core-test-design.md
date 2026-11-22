# f4x-core: test design

`f4x-core` is the game: the deterministic simulation every other crate sits on. All change goes through
`Game::apply(Command) -> Vec<Event>` (53 commands); a refused command comes back as `Event::Rejected(Rejection)`,
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
   (`game/testutil.rs`, `feature/testutil.rs`, the `put_*` and `tr` methods): they show as unused until used again.
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
- **No AI.** The battle player of core's tests is `testutil::battle_policy`.
- **Long plays are not core's tests.** A game played for many turns with `new_game` and the AI uses several
  crates, so it is an integration test of a crate that sees them all: `f4x-ai/tests/it` (which already depends on
  `f4x-newgame`) and the soaks of `f4x-tools`. Core's tests stay to what core can do alone: fixtures, short
  scripted plays and hand-made situations. What core gives those multi-crate tests is the **checkers**:
  `world_problems` (class 2) and the digest of a game (class 8) are public in core (behind the `testutil`
  feature), so the long plays call them after every command or turn without carrying a copy.
- **Small worlds by hand** are made by the `put_*` helpers on a `Game` loaded from a fresh save, never by
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

Guard: an **independent checker** `world_problems(&Game) -> Vec<Problem>`, written from the rules and not from the
code under test, with negative tests in core (each invariant broken on purpose is found). Core runs it after every
command of short scripted plays on the four fresh saves; the **long seeded plays** (many turns, the AI, a matrix of
worlds and rule settings, `F4X_WORLDS=N` for more) are integration tests in `f4x-ai/tests/it` that call the same
checker after every command.

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
- **A field census**: a test that fails when a new field of `Game` is neither saved nor named as derived, so a new
  field cannot be forgotten silently.
- **Replay**: a recorded game replays to the same state; commands name units by creation number; a command naming
  nothing is reported.
- **Undo**: after each undoable command, `undo` gives the snapshot from before it; a command that draws, explores or
  starts a battle clears the history.
- **Attacks on the files**: truncated, edited, wrong version, wrong game version, wrong layout, unknown content id,
  wrong content fingerprint, settings out of range, a wizard with no player: each gives a typed `LoadError` and never
  a panic.

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
record what the game does today. The digest function is core's; the plays that need `new_game` and the AI are
integration tests in `f4x-ai/tests/it`, and core pins only its own short scripted plays on the fresh saves.

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
`c2_invariants`, `c3_determinism`, `c4_persist`, `c5_input`, `c6_coupling`, `c7_arithmetic`, `c8_drift`, then one per
subsystem), and `support` holds the world loader, the matrix of fresh saves, a short scripted driver and the refusal
table. The checkers (`world_problems`, the digest) live in core itself behind the `testutil` feature. The long plays
with several crates are in `f4x-ai/tests/it`, a module of their own (`c2_core_invariants`, `c8_core_drift`) that
says in its header that it belongs to this design. Names follow the project's singular rule.

## Order of work

1. `support` and the checkers: the fresh-save matrix, `world_problems` with its negative tests, the digest, the
   short scripted driver.
2. Classes 4 and 1 first (the surfaces everything else leans on), then 5, 2, 3.
3. Classes 6, 7, 8.
4. The subsystems of class 9, biggest risk first (turn, city, unit, spell, battle), each with a mutation pass.
5. Mapping table of old tests to new ones, and the findings, added to this document as they come.

## Open questions

- Which invariants of class 2 the rules really demand: the list above is a start from reading names, to be checked
  against `docs/design.md` and the code as the checker is written.
- Whether the long plays belong in `f4x-ai/tests/it` or in a crate of their own that sees core, newgame and the AI
  (`f4x-tools` has the soaks but is a binary); `f4x-ai` is the start because it has the dependencies today.
- Whether the field census (class 4) can be done by a derive or a test over `Serialize` output.
- How much of class 9 survives the first pass: the rebuild decides by need, and a subsystem that already has its needs
  covered by classes 1 to 8 gets few tests of its own.
