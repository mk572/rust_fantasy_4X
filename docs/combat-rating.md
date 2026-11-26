# The unit combat rating

What a unit is worth in a fight, as one number: **spearmen**. A recruit High Men spearman is 1;
a unit of 8 is worth about eight of them, and two units of 100 are worth one of 200, as a rough
guide. It is measured, not computed: `f4x-tools duel ladder` plays real battles on the shipped
rules ([`duel.md`](duel.md)) and the numbers are fitted to the results (task 655). It replaces the
eyeballed formula of `battle::power` as the thing the game and the AI weigh units by (the plan
below; tasks 720 to 724).

It is a rough guide. The weights move it a few percent either way, about 4 pairs in 100 are
rock-paper-scissors cases a single number cannot carry (immunities, special attacks, a missing
damage type), and spells, terrain and walls swing a real battle. It is still much closer to the
battles than an attack-and-defence formula: on pairs the fit had not seen it names the winner of
about 95 in 100.

## The data

The outcome tables of the runs are archived in `duel-out/` (local, not committed; see `duel.md`).

`crates/f4x-def/content/unit_ratings.ron` (`Rules::ratings`, optional):

```
(
    hero_factor: 1.0,
    units: [ (num: 104, id: "high_men_spearmen", levels: [1.00, 1.81, 2.41, 9.18]), ... ],
)
```

- One row per unit that fights (180 of 196; settlers, ships and units with no attack have none).
  `Rules::combat_rating(unit_def, level)` returns the row's number for the experience level (from
  0, as `UnitView::unit_level`), a level past the last number taking the last, and a hero's times
  `hero_factor`; `None` for a unit with no row (callers fall back to `battle::power`).
- `levels` holds levels 1 to 6 for a normal unit (Recruit to Champion) and 1 to 9 for a hero, one
  number for a creature, which does not level (the run of 20 November 2026, `duel-out/2026-11-20-ladder-2`,
  archived in `reference/ladder-2026-11-20.zip`; cross-level fights fit one scale over all levels).
- **Heroes** were fought with the abilities and spells the game rolls for a hero (rolled afresh before each
  fight, `hero::kind::roll_one`), with no items, so `hero_factor` is 1 (the field stays for rule sets
  that want one). Heroes show their levels most: a healer is worth 2.7 at level 1 and 114 at level 9; the
  fighters stay ahead of the casters (level 6: chosen 445, paladin 142, magician 71, healer 56). Wizard
  spells were off in the fights; unit spells on.
- Regenerate after a change of the units, abilities, spells or battle rules: run the ladder
  (`scripts/ladder_run.py`, or `duel ladder --levels 1-9`, `--cross 1-9`), then
  `duel ladder --combine 1-9 --ratings crates/f4x-def/content/unit_ratings.ron`, and
  `python3 scripts/ron_hashes.py --update`. The tables themselves (`target/ladder/`) are local and not
  committed. A full run takes about three hours on four threads (`scripts/cool_run.sh`).

- The header of the file lists the SHA-256 of the rule files the ratings were made for (units, abilities,
  spells, heroes, level paths, `rules.ron`), as `content.sha256` records them, written by `--combine`. It is a
  reference to see whether they changed since; **nothing checks it** (a changed unit is a reason to run the
  ladder again, not a failing test).

- **Efficiency** (`crates/f4x-def/content/unit_efficiency.ron`, `Rules::efficiency.row(num)`, optional):
  the rating per resource, for choosing garrisons, what to build and what to hire. The rating is
  averaged over the measured levels (a creature's is flat, a hero's has its factor), the cost is the
  base cost plus 100 turns of upkeep with gold, food and mana counting alike (the half-upkeep setting is
  not applied); `per_100` is rating per 100 resources and `relative` that over a High Men spearman's
  (1.71). Regenerate with `duel ladder --efficiency crates/f4x-def/content/unit_efficiency.ron` after the
  ratings (without `--levels`, or it fights the ladder again into `--out`). **Garrison caveat:** only normal units and heroes calm unrest (half a rebel each, as in the
  original); fantastic units defend but do not police, so a garrison chosen for unrest cannot use them.

What the numbers show that the old formula did not: the level jump. High Men spearmen are worth
1.0, 1.8, 2.4 and 9.2 at recruit, regular, veteran and elite, as the wiki says of Elites ("double the
staying power and four times the killing power"): the extra hit point and to-hit of Elite are worth
more than all the levels before. And the cost curve is nearly linear for the plain troops (about 0.1 to
0.25 spearmen a gold piece) with outliers worth looking at (the engineers and the Klackon
engineers, a fighter bought at 80 for a rating of 0.5).

## The combat forecast

The first user of the rating in the game (20 November 2026): with the game setting `combat_forecast`
(Custom screen, off by default) the question before an attack, and the question when a rival attacks the
human, say how the sides compare: the sum of the ratings of the units each side would send (the strongest
`max_battle_side`) against the other's, in seven steps from "You are completely outmatched" to "The enemy is
completely outmatched" (`World::combat_forecast`, `battle/forecast.rs`; the thresholds are `forecast.bands_pct`
in `rules.ron`, [`game-settings.md`](game-settings.md)). It is as rough as the rating: walls, terrain, spells
and the players' skill are not counted.

## The plan to wire it in

The AI weighs units in many places, nearly all through two functions: `battle::power::unit_power`
(a unit) and `def_power` (a definition), read through `PlayerView::unit_power`,
`ai::view::unit_strength` and `fighting_strength` (the strongest `max_battle_side` units summed).
So the first step swaps the measure behind those two and everything that reads it changes at once;
the rest tunes what the numbers are compared with. Task 718 (one measure, one fighter predicate, no
retyped formulas) comes first, so that the swap is one place.

### Step 1: the unit measure (task 720, needs 718)

**The rating is combat only.** It says nothing about mobility (speed, flying, pathfinding, the sea, how
long a unit takes to reach a fight) or about price and upkeep. Every user combines it with what it needs
and does not read it as the value of a unit: the builder already does (`raw_profiles`: offence is the
power with a bonus for speed, defence the power alone, both over the square root of the lifetime cost);
a raid weighs the speed of the party; a garrison never weighs speed (its job is to not move) but does weigh upkeep and
policing (`unit_efficiency.ron` is the rating over the cost, still blind to speed); a trade offer prices
a unit by the gold it costs to make as well as by the rating. Replacing the measure (task 720) changes
the combat factor in these combinations and leaves the others where they are.

- **Done (task 720, 20 November 2026).** `UnitView::unit_power(unit)` is `Rules::combat_power(def,
  level)` (the rating times `power_scale`) of the unit's experience level, times what it carries
  beyond that level: the old formula on its stats and abilities now over the old formula on the stats
  and abilities of its level alone (an item, a spell or a rolled ability raises it, a curse lowers it).
  It is still the strength **at full health** (callers that care about wounds, such as the battle
  AI's `worth`, count the hit points left themselves). `def_power(def, rules)` is the rating at the
  first level; a unit with no row keeps the old formula (`def_formula`).
- **The scale.** The AI's thresholds were tuned to the old numbers, and mostly compare a strength with a
  strength, so `power_scale` (105 in `unit_ratings.ron` after the run of 20 November, 69.2 before it, written by the `--combine` run: the median of
  the old formula over the level 1 rating of the units that are not heroes) puts a typical unit where
  it was and the thresholds (`attack_ratio`, `sortie_ratio`, `GUESS_DEFENDERS`, `strength_wanted`, the
  raid yardsticks) stay as they are. The ratings change the *ratios between* units, which is the point
  (an elite spearman is nine times a recruit; a great drake is worth 250 spearmen, the old formula
  gave about a tenth of that gap). Not soaked: the real test is whether the AI beats the reference AI.

### Step 2: groups (task 721)

- **Measured (task 721, 20 November 2026; `duel teams --battles 20000`, groups of 1 to 4 recruits):** the side
  with the larger sum of ratings wins 92.7% of the battles, the old formula 80.3%. By how much larger the
  sum is: below 1.2 times 60%, 1.2 to 1.5 74%, 1.5 to 2 87%, 2 to 3 95%, 3 to 5 99%, above 5 99.9%. These
  are the bands of the combat forecast.

- A group is the sum of the best `max_battle_side` ratings (as `fighting_strength` does), and the
  harness already tests it: `duel teams` plays random groups, and the share of battles the larger
  sum wins is the number to report (with the old formula beside it): a report of the harness, run by
  hand after a change of the rules or of the file. **No test enforces the ratings or their order**: a
  change of the game's rules is allowed to move them, and the rating file is regenerated, not defended.
- Context factors for a group, each measured the same way before it is used: the side that
  defends a walled city (the harness can place walls: `BattleMap::with_city`), ranged units
  against a wall, fliers, a ranged group against a melee one on open ground. A first estimate
  is a multiplier on the group sum, with the measured ones replacing it.
- Counters: units with a rock-paper-scissors flag (the misfit list of each table,
  `ladder_L_misfits.csv`) are not special-cased yet; once the AI knows what it faces (a group of
  missile-immune units, no ranged defence) a correction by ability tag, from the misfits, is a
  later refinement.

### Step 3: who reads it (done by step 1; checks and exceptions)

| Place | Reads the measure for | Change |
|---|---|---|
| Raid composition: `raid.rs`, `threat/raider.rs` | how strong a raid or a monster party is against a target, the paladin and drake yardsticks | yardsticks read ratings; party strength is the group sum |
| Attack plans: `war_plan.rs` `strength_wanted`, `military.rs` `expected_defence`, `attack_ratio`, `sortie_ratio`, `reinforce.rs` | the force wanted for a target and whether to march | thresholds rescaled (step 1), the defence of a walled city with the wall factor (step 2) |
| Garrisons: `military.rs`, `crowd.rs`, `escort.rs` (the weak hero), `assault.rs` `guard_parts`, `city/neutral.rs` | who stays, who marches, the neutral and lair garrisons | ranking by rating; `neutral.rs` keeps the original's melee and ranged slots and chooses within them by rating; **a garrison also has to calm the city (a strong preference, not a ban; raids and conquest take any fighter): only normal units and heroes count as police (half a rebel each, up to one battle side, and only while the buildings and spells leave rebels), fantastic units never do, so the garrison logic keeps enough policing units in a city with unrest and picks the rest by efficiency (`unit_efficiency.ron`), where the fantastic units may fill the remaining slots** |
| Production: `city/autobuild.rs` (`best_defender`, `raw_profiles`), `ai/builder.rs` `best_soldier` | the best soldier a city can build for its gold | rating over the square root of the lifetime cost as now, the mobility bonus kept; the ratings expose the cost-inefficient units |
| Diplomacy: `diplomacy/trade.rs` `unit_worth`, `hero_worth`; `ai/statecraft.rs` `Power { army }`, `leader_of`, `is_straggler` | what a unit is worth in a trade offer, how strong a rival's army is | a unit's price is its rating times a price per spearman read off the cost curve, at least its cost-based price; the army strength is the sum of ratings, the thresholds rescaled (task 722) |
| Battle AI: `ai/battle/order.rs` `worth`, `cast.rs` (the worth of a summon) | which enemy to attack first, whether a spell is worth casting | `worth` = rating with the figures and hit points left; a summon is its rating |
| Spells: `ai/spellhelper.rs`, `wizardry.rs` | what a summon or an enchantment adds | the rating; an enchantment as the modifier of step 1 |
| Client: `fighter_pick.rs` | the order of the fighters in the pick screen | the same measure (the rating could also be shown to the player as an estimate) |
| Lairs: `feature/lair.rs` | the strength of a guard against the treasure | the group sum of the guards' ratings |

### Step 4: the data gets better (tasks 723 and 724)

- **Heroes (task 724)**: rolled abilities, spells and levels 1 to 9 are measured (20 November 2026,
  `hero_factor` 1, a hero's rolled abilities are in its rating and not counted again by `unit_power`);
  what items add is not. Measure a typical loadout by level against the same heroes without, and wire
  the uplift into `unit_power`. Until then a hero is assumed to carry nothing.
- **Levels 5 and 6 of the normal units** (Ultra-Elite and Champion, task 723): `duel ladder --normal
  --cross 1-6`, needing the Warlord retort and the Crusade spell in the harness, which it has
  (`World::scenario_level_bonus`).
- **A stale rating** (task 723): the file does not know that a unit changed. A check that the units,
  abilities and spells the rating was made for are the shipped ones (a hash in the header, tested)
  would flag it.

### Not to expect

A rating does not say who wins a given battle: a group of 100 spearmen of ratings does not beat a
drake of 100 on open ground against the engine every time, the wall and the spells matter, and the
one against one cases the table fits are the plainest. It is a starting point for the AI, better than
attack and defence alone, and it gets better with each correction above.
