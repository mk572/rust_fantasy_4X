# The duel harness (`f4x-tools duel`)

Real battles between shipped units, played by the battle AI with the real engine on the shipped
rules, many of them in seconds, with the rules' invariants checked on every event and the win rate
of every unit counted. Code: `crates/f4x-tools/src/duel.rs` (tests in the same file). It was made
for the damage type rework (task 708, to find bugs of the framework: 17,000 fights found none) and
is the base of task 655 (the unit strength measure).

## Use

```
cargo build --release -p f4x-tools
target/release/f4x-tools duel pairs  [--units N,N,..|--all] [--seeds N] [--max-commands N] [--csv FILE]
target/release/f4x-tools duel teams  [--units N,N,..|--all] [--battles N] [--max-team N] [--seed N] [--csv FILE]
```

- `pairs`: every unit of the set against every unit of the set, one against one, `--seeds` fights
  each (default 1). The unit on the west side is the attacker. About 16,000 fights take a second.
- `teams`: `--battles` (default 6000) random groups of 1 to `--max-team` (default 4) units against
  each other, drawn from the set with a small generator seeded by `--seed`.
- The set: `--units` (content numbers, comma separated), `--all`, or by default the units that have
  an ability with an `attack` or a `defence` rule in `abilities.ron` (read from the data, no ability
  numbers in the harness). Units that cannot fight in a plain battle (the galley, the warship and the
  floating island) are left out and listed.
- `--max-commands` (default 3000): the orders a fight may take before it counts as never ended.
- `--csv FILE`: the win rate of every unit (`unit,id,fights,wins,win_rate`, best first). A fight
  counts for every unit in it; a draw (no winner) is a win for nobody.
- It is short enough to run after any change of the battle rules, and it fails (exit code 1) when a
  fight never ends, the engine refuses an order the AI gave, or an invariant breaks. Long runs
  (`--seeds 20`, `--battles 200000`) go through `scripts/cool_run.sh` like the soaks.

## How a battle is made

`duel::make_battle(content, ours, theirs, seed)`: a small game (two islands, a city of each of two
players), the units spawned for the two players, `Command::BeginBattle` so that the game itself sets
the battle up (the units have their real numbers, stats, abilities and owners), then the battle map
is replaced by open ground (`BattleMap::open()`: no walls, no terrain) and each unit put on the
square given (`ours` at x 3, `theirs` at x 6 for a pair, x 3 and x 14 for teams; the unit list order
is the unit id order). The dice come from `seed` through the game. Nothing else is set: no wizard
casts, no hero items, no enchantments, no walls, no experience levels (units are as shipped). The
battle then runs with `f4x_ai::battle::order::next_command` until `Battle.winner` is set.

## What it checks (`check`)

- an `Unharmed` event names a damage type the defender has an immunity (or a floor) against;
- a `Shot` that dealt damage was not stopped by an immunity on every damage type the attacker's
  ranged abilities carry (an immunity means no damage, 708).

A violation is printed as `VIOLATION label: ...` with the units and the seed, so the fight can be
replayed: `make_battle` with the same numbers and squares, and the seed in the label.

## Extending it (task 655 and others)

- **A new invariant**: add it to `check` (it sees the battle after the event) and to the test
  `a_small_set_plays_through_without_a_violation`.
- **Groups of one kind against groups of another** (task 655): `make_battle` already takes lists; add
  a mode that takes two unit numbers and a count for each (the counts from the measure,
  `battle::power::unit_power` in `f4x-core`, so that the two sides are worth the same) and reports the
  win rate of each side.
- **The measure against the engine**: the CSV is the engine's ordering of the units; compare it with
  the order of `unit_power` (and the AI's `worth`, `f4x-ai/src/battle/order.rs`) to find where the
  measure is wrong, then fit its weights; then keep a test of the ordering for a sample of pairs
  (task 655, point 3).
- **Spells, hero items, walls, terrain, wizards**: not in the battles yet. A spell needs a wizard
  with mana and the cast orders of `f4x_ai::battle::cast::next_command(&Game)` (it works on a `Game`,
  not on a `Battle` alone); walls need `BattleMap::with_city(true)` and units placed inside.
- **Speed**: single threaded; a `--jobs` as in `soak` would need the fights split by unit.

## Known limits

- A one against one duel on open ground says little about units that need a group (healers, summoners,
  wall crushers, a hero's leadership), and ranged units get no cover. A unit's win rate also depends
  on which units are in the set (the default set has many strong monsters).
- The battle AI decides everything (it may flee, wait or charge), so a win rate is the unit as the AI
  plays it, not the best play.
- Units are spawned as the game spawns them, with no experience, items or enchantments added; what a hero gets from `spawn_unit` (level, abilities) has not been checked.
