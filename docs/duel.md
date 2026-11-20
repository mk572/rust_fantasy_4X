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
target/release/f4x-tools duel ladder [--levels 1-9] [--units N,N,..] [--seeds 10] [--prior DIR] [--jobs 4] [--out DIR]
target/release/f4x-tools duel heroes [--level 1-9] [--seeds 10] [--jobs 4] [--out DIR]
target/release/f4x-tools duel spells [--unit N] [--level 1-6 | --balance] [--spells N,.. | --all] [--against N,..] [--hurt PERCENT] [--count N] [--seeds 20] [--jobs 4] [--out DIR]
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

## The ladder (`duel ladder`, task 655)

Which unit beats which, as an exchange rate: "4 of A are worth 5 of B". Code:
`crates/f4x-tools/src/ladder.rs`. For every pair of units at one experience level:

1. 4 of A against 4 of B on a plain open battlefield, the units where the game puts them. The
   side with more wins takes the step (below). Equal wins is **even** (stops there as `4:4`);
   mostly fights with no winner after 100 turns is a **stalemate** (`draw`).
2. The loser gets one more unit; if it now wins or the step is even, the ladder stops with that
   ratio (`4:5`, a **flip**). Else the winner loses one and the loser gets one again: 4:5, 3:5, 3:6,
   2:6, 2:7, 1:7, 1:8, 1:9. The winner still winning at 1 against 9 is a **wipeout** (`1:9+`).
3. A step is `--seeds` fights (default 10, an even number), each side the attacker (first to move)
   half of them; the dice are seeded from the pair, the counts and the level, so a run repeats.

The rules are 1.3 with death and magic immunity exceptions on (the original's; the shipped default
has them off). No wizard casts, no items, no walls, no terrain; a unit with no attack (settlers, engineers) is fought too and loses to anything that can hurt it; the units cast their own spells
(the fights go through the game with `f4x_ai::battle::cast::next_command`, so a hero's or a
creature's spell is cast as in a real game). The 1.50 patch's gaze at strength 0 stays on
(`divergences.md`). A stalemate is a fight that reaches the turn limit with both sides standing
(the engine gives the field to the defender; the ladder counts it as nobody's).

**Levels.** `--levels 1-9` runs one table per level: 1 is a recruit (a hero at Hero), 9 a Demi-God.
A hero has the nine levels of its path, a normal unit six (Recruit to Champion; levels 7 to 9 are
Champion again) and a fantastic or summoned creature none (it enters with its base stats). Experience
gives a normal unit up to Elite; Ultra-Elite and Champion are given the way the game gives them, by
the Warlord retort and the Crusade spell on the player (`Game::scenario_level_bonus`).

**Output** in `--out` (default `target/ladder`), per level L: `ladder_L.csv` (every pair: who won at
4:4, the result, the counts), `ladder_L_rank.csv` (the strength fitted over all the pairs: `worth`
is how many of the weakest unit one of this is worth), `ladder_L.html` (the table of who beats whom,
strongest first, green where the row wins), and with several levels `ladder_ranking.csv` (the worth
of every unit at every level). A pair is fought once; the other half of the table is its mirror.

**Reading the table.** The strength is a robust fit (Huber, so a counter does not drag a unit's rank)
of the log of the exchange rates; the pairs it misses by more than a factor of two are the
rock-paper-scissors cases (`ladder_L_misfits.csv`, and per unit in `ladder_L_rank.csv` the opponents
it beats far above, and loses to far below, its rank). On pairs held out of the fit it names the
winner of about 96% of them (level 1), so a single number carries most of the table and the rest is
counters (immunities, ranged against walls of shields, a missing damage type). The number is given in
**spearmen**: the worth of a unit as a multiple of the High Men spearmen (unit 104), like a pawn in
chess, and as a coarse `bracket` (1, 1.5, 2, 3, 4, 6, 8, ...), because the fit is not more exact than
that (a few percent either way with other weights). Two units of 100 spearmen are worth one of 200,
as a rough guide; a wipeout only says "at least nine to one", so very high values come from chains of
pairs. Spells, terrain and walls can swing a real battle beyond all this. `--report` rebuilds these
files from the `ladder_L.csv` of a run.

**All levels in one table.** A ladder compares units of the same level only. To put every unit at
every level on one scale (in spearmen of a recruit High Men spearman) the normal units of levels 1
to 4 are also fought across levels: `--normal` (normal units only: no heroes, no creatures, no
settlers), `--cross 1-4` (every unit at a lower level against every unit, itself included, at a
higher one; one `cross_A_B.csv` per pair of levels, kept if it is there so that a stopped run goes
on), `--combine 1-4` (one fit over `ladder_L.csv` and the `cross_A_B.csv` files into
`ladder_levels.csv`: the worth and the bracket of each unit at each level). Levels 5 and 6 need a
retort or a spell and the creatures do not level, so they stay out; heroes are left for a run with
rolled heroes.

**Heroes are rolled for every fight.** Each hero of a side is rolled anew the way the game rolls it
(`hero::kind::roll_one`: the template's abilities, the random picks and the spells it knows), from the
seed of the fight, so four heroes of one type are four different heroes and a run repeats. No items.
The heroes' tables are therefore of an average hero, not a floor. Heroes have the nine levels, so
`--levels 1-9` and `--cross` over 1-9 include them; the normal units stop at 6 (the pairs of two units
that are not heroes at levels 7 to 9 are those of level 6, copied from `ladder_6.csv`, which must be
done first). `--cross` fights the units that gain levels (normal units, settlers and engineers, heroes).

**Starting from an earlier run (`--prior DIR`).** A pair's ladder starts at the cell where the old
run ended it (a wipeout at 1:9, a flip at its ratio) and walks up while the winner still wins and
down while it does not, so a result that held costs one step, or two with the cell before it. The
first step of a wipeout or a stalemate ends after two fights (each side attacking once) that both
end as the old result says; a changed result plays the whole step. A pair that changed order (the
old winner does not win at 4:4) is laddered anew. Levels above 6 start from level 6.

**`scripts/ladder_run.py`** runs the whole ladder as brackets (one table each: a level, or a pair
of levels across) at once, each through `cool_run.sh --boost` (the thermal state is read once,
before a bracket starts), skips the brackets whose file is already in `--out`, and ends with the
fit over all of it (`ladder_levels.csv`, no rating file written).

**Archived runs.** The tables of a run are kept in `duel-out/DATE-NAME/` (ignored by git, so local; a
`README.txt` in each says how it was made and what each file is). The first, `2026-11-20-ladder/`,
holds the levels 1 to 4 ladders, the cross-level files, the fitted `ladder_levels.csv`, the heroes at
level 5, the rating file and the Discord posts. Copy a run there when it is the reference for a
rating file, with the commit it was made on.

**Heroes one against one (`duel heroes`).** A hero is one individual, so the heroes are not fought in
multiples: every hero type against every other, one against one, at one experience level (default 5,
Champion; `--level`). The heroes are rolled the way the game rolls them (`hero::kind::roll_one`: the
template's abilities, then the random picks, and the spells it knows), anew for each of `--seeds`
seeds (default 10); the pair fights a best of three with those two heroes and fresh dice, and the
hero that starts (the attacker moves first) alternates by seed, five each. A seed goes to the hero
with two fights won (nobody if the turn limit ends them); the pair's score is the share of seeds the
first hero wins, a drawn seed half. `heroes_L.csv` has every pair, `heroes_L_rank.csv` the heroes by
their mean score and how many opponents each beats, loses to and ties. No items and no wizard
casts. A different seed set gives different rolls, so the table is one sample of the randomizer.

Units with no attack, and those that cannot enter a plain battle, are left out and listed. About
16,000 pairs a level take a few minutes on four threads: run it through `scripts/cool_run.sh`.
Reading it for the unit strength measure (`battle::power`): compare `worth` with `unit_power`; a
wipeout or a stalemate bounds the ratio, it does not measure it.

## Spell effects (`duel spells`, task 705)

What a spell on one unit does to a fight. One reference unit (`--unit`, default 47, the Beastmen
swordsmen) fights every other unit (or `--against`) one against one on open ground, `--seeds` fights
each, with the same dice in three variants: `base` (nothing cast), `ours` (the reference side gains
from the spell) and `theirs` (the opponent's side does). A spell for one's own unit that lasts is on
the unit before the battle (as an overland cast would be); a spell for an enemy unit is cast on the
first turn by the side that gains from it; a healing spell (instant, own unit) is known by the wizard
of that side, who has 1000 mana, and the battle AI casts it when it sees fit. The CSV of a spell
(`spell_N_ID.csv`) has a row per opponent with its features and the three win counts. We look for the
outliers (opponents against which the spell moves the win rate a lot), not the average.

- `--level 1-6`: the reference unit's experience level (5 and 6 get the Warlord bonus, as the ladder
  does); `--balance` picks per opponent the level whose base fight is nearest to even, so that a
  spell has a fight to change (a one against one against a strong monster is 0 or 100 wins otherwise).
- `--hurt PERCENT`: both units start with that share of their hit points lost (for healing).
- `--count N`: N units a side; the spell goes to or from the first. Needed for the spells that need
  allies (Confusion lends the unit to the enemy for a turn).
- `--trace OPPONENT`: the orders and events of the first fight against that opponent.

What it showed (20 November 2026, reference units 47 and 101, the soak-out analysis in task 705):

- **The first unit decides.** Haste, Confusion and Warp Wood do little for the swordsmen and a lot for
  the longbowmen: a hasted archer shoots twice (win rate +13 to +16 points), Warp Wood on an archer
  takes its only weapon (the longbowmen win 0.62 without it, 0.15 with it), a confused archer loses
  most of its shots (+7 for us, -14 against). A melee unit's Haste is a second swing in the same exchange (after the counter-attack, which is not doubled); the figures below for a melee unit were measured before that change (a second exchange, the enemy striking first: a hasted swordsman won 30 of 100 against an equal one) and must be measured again.
- **Confusion:** a confused unit does nothing, wanders, is lent to the enemy for a turn or acts as usual, a quarter of the turns each. When every unit a side has left is lent at once, that side loses on the spot and the units are destroyed (wiki "Confusion"; `Battle::check_winner`). Before this rule one against one measured nothing (a swordsman 0.427 base, 0.422 with the enemy confused); now 0.77 against a lone unit, and 0.58 with three a side (task 705 run of 20 November 2026, reference unit 47).
- **Healing** shows nothing in a fight between whole units; with `--hurt` the side with the healer
  wins nearly always (unlimited mana).

## Extending it (task 655 and others)

- **A new invariant**: add it to `check` (it sees the battle after the event) and to the test
  `a_small_set_plays_through_without_a_violation`.
- **Groups of one kind against groups of another** (task 655): `make_battle` already takes lists; add
  a mode that takes two unit numbers and a count for each (the counts from the measure,
  `battle::power::unit_power` in `f4x-core`, so that the two sides are worth the same) and reports the
  win rate of each side.
- **The measure against the engine**: the CSV is the engine's ordering of the units; compare it with
  the order of `unit_power` (and the AI's `worth`, `f4x-ai/src/battle/order.rs`) to find where the
  measure is wrong, then fit its weights. (No test enforces the ratings: they are a guide that changes with the rules,
  `combat-rating.md`.)
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
