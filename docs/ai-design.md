# The scripted AI: strategic and battle

Design for the hand-written AI players: the strategic AI (`crates/f4x-ai`) and the conventional
battle AI (`crates/f4x-ai/src/battle/order.rs`). The learned AI is a separate design
([`learning-ai.md`](learning-ai.md)) and is not touched here. Related tasks: 46 (battle AI), 61
(Spell of Mastery), 64 to 67 (personalities, AI v1, spells, attack), planned 24 (soak runs), 106
(battle views), 224 to 232 (this design's steps, section 9).

Status (30 October 2026): steps 0 to 7 and 9 to 11 are built and tested; step 8 (ships) works for one ship at a time (see 11.3).
Section 2 holds what was measured on the AI before the work, section 8.1 the results, section
11 what was built and what was learned. The numbers there are from 8 seeds, two wizards each.

## 1. Goals and rules

1. **Teach the AI the new access paths.** The core now has domain handles (`Units`, `Economy`,
   the magic and finance queries, `PlayerView`) and many spells that do something. The AI reads
   the game only through `PlayerView` and acts only through `Command`s (task 181); that stays.
   What changes is how much of the game `PlayerView` answers and what the AI does with it.
2. **Smarter means faster wins in AI-against-AI games.** The measure is the soak run
   (`f4x-tools soak`): how many games end, on which turn, and by what (conquest or the Spell of
   Mastery). Every step is kept only if it does not make that worse and the tests pass.
3. **The AI can cast the Spell of Mastery.** Today nobody can: see section 7.
4. **No information cheats** (`design.md` section 8): the AI sees what a player sees and obeys
   the same commands. (It has the difficulty bonuses of the game settings, task 247, at the Average level.) Where the original's AI knows more (it reads the whole map), we do not.
5. **Deterministic**: the AI draws no random numbers of its own; anything random goes through
   the game's generator (`EffectHost::rng`-style) or is a function of the seed and turn.
6. **Not touched:** the learned battle agent and `f4x-learn`. The scripted battle AI is also
   their baseline opponent and their expert, so a change to it moves their numbers; section 6.5.

## 2. What exists and what the soak shows

### 2.1 Strategic AI (`f4x-ai`, about 1,000 lines)

`Ai::play_turn` runs, in order: answer diplomatic proposals, balance farmers, taxes, research,
cast, settlers, army, city production (`lib.rs`). Each part:

| Part | What it does now |
|---|---|
| `diplomacy.rs` | Answers proposals with its own rating (`diplomacy::rate`, the original's `Diplomacy_Test`); a fierce militarist or expansionist declares war on a weaker neighbour (`quarrel`); a wizard may declare war on a Spell of Mastery caster (`mastery_scare`). Never proposes anything. |
| `raze` | After its attacks: razes a city just taken from a wizard when the points against keeping it pass a threshold, with no roll: the temper (a peaceful wizard never razes, a lawful one only a city of a wizard it fights), the racial unrest between its people and the city's, the worth of the land, the city's citizens and the cost of its buildings, how hard the city is to hold (own and enemy cities within 14 squares, walls) and the soldiers in or within 3 squares of it against the defence it wants. Weights: `AIK_RAZE_*`. Never a city of the independents. |
| `economy::taxes` | The lowest tax rate that leaves a small surplus. |
| `economy::research` | When idle, takes the first of the offered candidates (the core lists the fastest first). |
| `economy::cast` | Starts the dearest summoning spell it can pay for with a third of the mana left over. Nothing else is cast, so most spells are dead weight. |
| `expand` | Settlers pick the best explored site within nine squares by terrain value and walking distance. |
| `military` | Garrison two per city; hit adjacent weaker stacks (`AutoResolve`); pick the nearest known enemy city; gather three or more at the nearest own city; march when strong enough; spare units explore. |
| `economy::cities` | Housing in outposts, a fighter for an empty city, settlers up to seven cities, garrisons, buildings from a fixed list, an army up to `cities * 3 + 2`, else trade goods; buys production with spare gold. |

It never sets the power split (mana, research, skill stay at 33/34/33), never uses heroes,
items, nodes, lairs, ships, gates, plane travel or the new special spells, and does not use
`legal_battle_casts`. Battle commands never come from the AI crate: a battle is run by the core,
and its AI is `f4x_ai::battle::order::next_command`, which is given only the `Battle`.

### 2.2 Battle AI (`f4x-ai/src/battle/order.rs`, 178 lines)

Every unit goes for the nearest enemy and attacks, shoots when it cannot reach, and walks round
walls. It does not cast spells (it has no `World`), does not choose targets by value, does not
focus fire, does not hold the walls as a defender, never retreats and never flees. A wall
crusher goes for the wall.

### 2.3 Measured, soak seeds 1 to 4, two wizards, 200 turns

- **No game ends.** 0 of 4. Battles: 0, 0, 2, 0. Cities taken: 0.
- **They never meet.** Squares explored per wizard stay near 150 (of 2,400 on a plane) from turn
  100 to turn 200. In seeds 1, 3, 6 and 8 the two fortresses are joined by land. The explorers
  only look within 14 squares of where they stand and only two units explore a turn.
- **Magic is idle.** Mana income is 3 to 4 a turn and the reserve climbs to 800 unspent. Known
  spells go from 12 to 16 in 200 turns. Nobody casts after the first summoning spells.
- **Seed 2** has one wizard stuck on two cities and no mana (upkeep equals income).
- **The win spell cannot be had**: the Spell of Mastery is excluded from research, treasure and
  starting spells (`wizard/magic.rs` `NOT_RESEARCHABLE`, `feature/lair.rs`), and the importer reads its
  research cost, 60,000 as an unsigned 16-bit value, as the signed -5,536 and clamps it to 1.

These numbers are the baseline of section 8.

## 3. Access: what the AI can ask the game

The AI crate depends on `f4x-core` and uses `PlayerView` (`player/view.rs`) for every read. The
view keeps the rule that lists are limited to what the player knows. It wraps `World`'s own
queries and the domain handles `Units` (`world.units()`) and `Economy` (`world.economy()`), so a
new query is added once, in the domain, and exposed on the view if the AI may know it.

### 3.1 Exposed today

Own state (`player`, `magic`, `treasury`, `research_choices`, `casting_cost`), own units and
cities (stats, hits, abilities, orders, food upkeep, production, `can_build`, `buy_cost`,
`city_economy`, garrison), the map as the player knows it (`explored`, `sees`, terrain, rivers,
nodes, lairs, `can_found_city`, `shift`, `distance`), other wizards' cities on explored squares
(`known_cities`), hostile stacks on squares in sight (`hostile_at`), pathing (`can_reach`,
`reachable`), proposals.

### 3.2 Missing, and what each is for

| Wanted by the AI | Why | Source in the core |
|---|---|---|
| Income: mana, research, skill, upkeep (`income`), and the split | choose the split; know what can be afforded | `World::income`, `SetPowerSplit` |
| Research state: remaining points, the Spell of Mastery's cost | pick research; plan the win spell | `Magic`, new `som_research_cost` (section 7) |
| Spell facts: known set, castable now, target rules, upkeep | cast beyond summons | `check_target`, `spell_needs_target`, `SpellDef` |
| Spell categories | the original's weights are by category | derive from the `SpellDef` effects (section 5.3) |
| Enemy wizards: contact, relations, their visible globals, who is casting the Spell of Mastery | war target, disjunction, the race | `diplomacy`, `World::magic` of others (only what a player sees) |
| Remembered enemy garrisons | a city out of sight is not forgotten | new memory kept by the AI, or in the view |
| Unit levels, experience, mana, hero data | stack value | `Units` |
| Lairs and nodes: guard strength, reward | free growth for a strong stack | `feature/lair.rs`, `feature/node.rs` |
| Threat map: where enemy stacks are near my cities | defend, recall | derived in the AI from `hostile_at` |
| Ships and transports | reach other land masses | `unit/transport.rs`, `unit/movement.rs` |
| Battle: legal casts, spell costs, field state | the battle AI as a wizard | `World::legal_battle_casts`, `battle_spells` |

**Added so far** (all read-only methods on `PlayerView`, each a call into a domain function):
`income` (split, research, mana upkeep), `research_cost` (the Spell of Mastery's depends on the
spells known), `paid_spell` and `spell_needs_target` (a cast paid over turns waits for its
aim), `can_aim` (the game's own target check, so the AI never guesses), `unit_enchantments` and
`city_enchantments`. The AI crate reads nothing else of the game, and writes only `Command`s.
`World::explored_squares` and the `Fog::explore_block` the special spells use are in the core.

Each addition is a read-only method on `PlayerView` that calls a domain function. None gives
the AI a fact a human could not read off a screen; where a domain function answers more than the
screen shows (for example `reachable_squares` ignores the fog), the view limits it.

### 3.3 The battle AI needs the game, not only the battle

`next_command(&Battle)` cannot cast: spells need the wizard's mana, skill and known spells, which
live in `World`. `World::next_battle_command` already calls a brain with the whole `World`; the
scripted AI becomes a second brain of that kind (section 6.2).

## 4. What ReMoM's AI does (reference)

From `~/projects/ReMoM/doc/ComputerPlayer/` and the sources it points to. The ReMoM source is a
reconstruction, and several leaves are still stubs there; this is said below where it matters.
Not everything in these notes was read; the sections that were are named.

- **Turn pipeline** (`AI_Next_Turn`, read): per wizard, in this order: strategy and hostility,
  target values for cities and stacks, landmass values, war landmass choice, spell selection and
  casting, gold and mana reserves, colony autobuild, evaluation map, orders, cull; then movement
  (`AI_Execute_Orders`); then the neutral player (colony autobuild, farmers, destinations, movement; ours: a neutral race player is played by the standard AI, `NeutralSeat`, with its diplomacy and statecraft and without the wizard's spells and spell trades, and the sites' garrison code is `assault.rs`),
  then raiders and monsters.
- **Overland spell choice** (`AI_Spell_Select`, `AI_Select_Spell_Group`, read): eleven category
  weights, one picked by weighted random: nothing, summon, unit enchantment, city enchantment,
  disenchant, disjunction, summoning circle, curse a target wizard, suppression, global
  enchantment, **Spell of Mastery**. The weights: summon 100 plus ten per hero short of six,
  plus 30 for a strategy, times 1.5 for conjurer or channeler, times five if the summon point is
  not the fortress city; unit enchantments by turn (0 before 50, 100 before 200, else 200) for
  movement buffs, plus `(250 - turn) / 2` for attack and defence buffs; city enchantment 150 for
  some, else 50 or 5; disenchant +40 per curse on own cities and +100 per warped node;
  disjunction scored per enemy global; global enchantments about +100 each, capped back to 300
  when above 400, and **zero when the mana reserve is under 200**; **Spell of Mastery 500 if
  known**. Then an affordability check: skip a spell whose cost / 50 exceeds `reserve / 10 +
  income`. The picker leaves behind it (which spell inside a category) are stubs in ReMoM, so we
  design our own (section 5.3). The original has known bugs here (it reads the human's realms in
  the suppression weights, and the "contacted" test indexes player 0); we do not copy them.
- **Research choice** (`AI_Spell_Research_Select`, read): each of the up to eight candidates gets
  a weight from its cost, adjusted by research bonuses: x4 if the AI already has a spell of that
  category (and x3 again), /5 for a first combat summon, harm, curse or city enchantment, /2
  more for a militarist, /2 for Spell Ward, disenchant and disjunction, /5 for the True
  versions and Summon Hero, x2 for Summoning Circle, x2 and x5 if the cost is more than 20 or 50
  turns of research income. A weighted pick favours cheap, new-category, combat-useful spells.
  The Spell of Mastery is among the candidates with the player's own `som_research_cost`.
- **Spell of Mastery in the research list** (`Player_Research_Spells`, read): it is added to the
  candidates only when the research pool is empty (the wizard has researched everything their
  books allow), and not if already known. Its cost starts at the data value (60,000), is cut by
  3,000 for a wizard with all five books at rank 11, and is cut by half the research cost of
  every spell the wizard gets (`Player_Gets_Spell`).
- **Combat AI** (`Auto_Do_Combat_Turn`, `Choose_Target_And_Action`, `Target_Unit_Value`, read):
  - the side first casts (`Auto_Cast_Spell_And_Do_Combat_Turn`, `AITP_Combat_Spell`, not read in
    full), then each unit acts; heroes act in a second pass;
  - a defender behind walls stays in unless the attacker has more than 30 ranged strength and the
    defender none, or a magic vortex, Wrack, Call Lightning or Mana Leak is on the field;
  - melee units gather at a rally point (the median-speed melee unit's destination);
  - a unit's own abilities come first (Summon Demon, Doom Bolt, Fireball, Healing, Web, an item
    spell), then a spell caster unit with mana over 2 casts with chance `(ranged - 5) / 15`;
  - target value = attack strength minus the target's defence against that attack, +`ranged / 3 +
    2` for targets that shoot, -10 for confused targets, -10 when our defence is more than twice
    its attack (safe, so less urgent), +10 for the current target (stick to it), -200 for a
    flyer a unit cannot reach, -20 for targets behind walls that melee cannot get at, +20 for a
    sleeping target when shooting. Ranged units prefer the weakest of near-equal targets; melee
    units the healthiest;
  - ReMoM's own notes list the movement helpers and the healing action as unfinished.
- **Not read**: the continent and landmass evaluation, target assignment for stacks, ferries,
  the colony autobuild. Where we take an idea from them (section 5.2) it is re-derived.

## 5. The strategic AI: improvements

Each is a step with its own test and soak result (section 9).

### 5.1 Exploration (step 1)

The cause of "no wars" is that nobody finds the enemy. Replace `nearest_unexplored` (radius 14,
two units) with a frontier search: from every reachable square, the nearest square that is
explored with an unexplored neighbour, scored by how many unexplored squares lie within sight
radius, minus path distance. Scouts get distinct targets (a per-AI claim list, like
`settler_targets`). The first fighter built in a capital with nothing to attack becomes a scout;
a flying or fast unit is preferred. Units on a scouting trip are not counted as garrison. When
no frontier is reachable by land the AI stops scouting and notes that other land masses need
ships (step 8).

### 5.2 Choosing and reaching a war (steps 2 and 6)

- Target value of an enemy city: its size and buildings if seen, its distance, its garrison
  from memory (the AI remembers the last stack it saw, with the turn), and its owner's relation.
- Pick the owner first (the original picks a target wizard, `OVL_TargetWiz`, weighted by
  relation), then the city.
- Gather at the city nearest the target, as now, but assemble a stack that suits the target:
  ranged units against walls cannot help, so prefer units with `wall_crusher` or flyers when the
  target has walls.
- Do not march into a battle the battle AI would lose: estimate with the same strength function
  the battle AI uses (step 7) and include the defender's walls and enchantments.
- Defend: a city seen threatened (an enemy stack within three squares) pulls the nearest spare
  stack in and buys a unit.

### 5.3 Magic (steps 3 to 5)

- **Power split**: set once a turn from what is needed. Early: mostly research. With a spell
  in progress that is wanted: mana. With the Spell of Mastery known: skill and mana (section 7).
  The split is a command (`SetPowerSplit`), so it can be changed every turn.
- **Research**: port the original's weights (section 4) to our categories, which are derived from
  the `SpellDef` (summons, unit enchantments, city enchantments, bolts, globals, dispels). Add
  the Spell of Mastery when it is offered (section 7). Research cost per turn of income
  replaces the original's `cost / 20` and `cost / 50` tests.
- **Casting**: the eleven category weights of the original, with our own pickers:
  summon (the best strength for the mana among spells whose unit the cities can feed), unit
  enchantments (on the strongest own stack: haste, heroism, flight, regeneration), city
  enchantments (walls, ward against the strongest enemy realm), disenchant (own cities with
  enemy curses), disjunction (enemy globals scored as in the original table), the special
  spells that now exist (Transmute on a poor mine, Earth Lore when exploring is blocked, Word of
  Recall for a stranded stack, Nature's Cures on a hurt stack at home, Plane Shift to dodge),
  and the Spell of Mastery. Each picker returns a spell and a target the core accepts
  (`check_target`); the AI does not guess.
- **Reserve**: keep the original's mana gate (no globals under 200 in reserve) and upkeep margin.
  Unspent mana of 800 (section 2.3) means the cast rules are too timid, not that the AI is
  saving.

### 5.4 Economy (step 6)

- **The opening** (Build phase). Of the two units a wizard starts with, one is the garrison and one scouts from turn 1
  (`garrison::wanted_share` rounds the Build-phase share down to whole soldiers, so `reinforce.rs` does not call the scout
  back); a wizard with fewer than four cities wants two scouts (`scout::wanted`); the capital builds its first settler
  before the basics, with no army while nothing threatens; the site list is rebuilt as soon as a few new squares are explored.
- **Where the next cities go** (`city_site.rs`, tasks 664 to 666). The AI keeps one ranked list of the
  places on the explored map where a city can be founded (`SiteBook`, rebuilt when the number of known
  cities changes and every five turns). A site is scored by the land its city would work (terrain for
  food and growth, the production the land gives, mana, gold and food of special squares, cheaper
  soldiers, rivers), the squares another city already works count for less, and the score rises with
  nearness to the wizard's own cities and falls with nearness to a rival's (more at war) and with a
  crossing: a site over the sea or on the other plane ranks below any site on the home land. The weights
  are content (`crates/f4x-def/content/ai_sites.ron`, `SiteRules`), and `phase::width` leans wide while the
  list is not empty. Of two
  sites too close to be two cities only the better is kept. Settlers get their sites in rank order, the
  best site to the nearest settler (`assign`); a site is left out only for a reason the view shows (an
  enemy in sight on or next to it, a city founded, the ground no longer taking one), and there is no list
  of failed sites. A site over the sea is a site like any other with a crossing cost: its settler is tagged `Crossing::Sea` and waits for a ship, which the general rule for units waiting for a ship builds (`colony.rs` is gone).

- **What a city builds** (`builder.rs`, `economy::cities`). The AI does not use the auto-builder of the core
  (the human's helper): a finished item leaves a city with nothing in hand (`design.md`, the rule for a
  finished item). At the start of its turn the AI looks at the cities with nothing, Housing or Trade
  Goods, and only those (a standing order is left alone), and picks the next item: the garrison minimum,
  unrest, then the basics (gold when the net is low, food where the city is stuck, production, gold),
  then settlers where `plan_for` allows them (one for each place for a city that no settler has taken,
  from the ranked list of `city_site.rs`, no more; a wizard that wants more cities and knows no place
  yet sends one to look), then, in a military city (first focus Offense: the share of its cities that the wizard's
  objective gives to the army, and every city near a rival), the way to the strongest unit of the
  race (`forge::step`: the cheapest missing building the treasury carries, then the unit), then the
  city's specialty (the special tiles in its area,
  `PlayerView::city_specials`: gold and mana from ore and gems, wild game's food, coal and iron's
  cheaper units, land that gives production (forest, hills, mountains, desert, nature and chaos nodes: a specialty from four tiles' worth up; rivers: food, from four river squares), nightshade; the strongest is served first with the buildings that add to it, the
  miners guild for ore, and the buildings that let a city use nightshade), then the plan's mix (soldiers when the army is short,
  else buildings in the rules' `autobuild_order`), and last Housing (while the city can still grow) or
  Trade Goods. A basic that waits for another building (a miners guild needs a builders hall) makes the
  city build the one it waits for. Engineers and ships are not chosen there: `road.rs` (one engineer
  for every three cities, one at least, while there is road to build) and `naval.rs` order them, and the port, scout, colonist and forge items stay direct `SetProduction` overrides. The
  same item is never queued again by itself: the chooser picks it again only because it ranks first.
  Not done: the chooser has no weighting by focus beyond the plan's mix, and a treasury whose net is
  below a building's upkeep builds none (gold buildings included).
- Buildings by need: an income-first order until the treasury is positive, then research,
  then military; skip buildings whose upkeep the treasury cannot bear (already done) and prefer
  the ones that raise mana for a wizard with a long spell queued.
- Settlers: more cities earlier (seven today); stop when no site with value over a threshold
  is left, not at a fixed count.
- Army size from the war plan, not `cities * 3 + 2` (done: `military::force_wanted`).
- Food: the soak shows food limiting the army; build farmers markets, granaries and
  foresters' guilds when `spare_food` is the limit.

### 5.5 Lairs, nodes, heroes, items (later)

Clearing lairs gives treasure and removes a danger; nodes give mana; heroes and items arrive
with the tasks in `heroes-items-design.md`. The AI uses them after those exist (task 222).

### 5.6 Diplomacy (later)

The AI answers proposals but never makes any. Pacts with the strongest neighbour while it
fights the other, and a refusal to trade with a wizard casting the Spell of Mastery, come
after wars happen (task 64 covers personalities).

## 6. The conventional battle AI: improvements

### 6.1 Principles

Same inputs as a player: the `Battle` and, for spells, the `World`. No randomness of its own. A
step is kept if the new AI beats the old one in mirrored fights (section 8.2) and the battle
and learn tests still pass.

### 6.2 A brain with the game (step 9)

Built as `f4x_ai::battle::cast::next_command(&World)`, which `World::next_battle_command` calls for every
wizard that has no learned brain set (the game holds it as a function pointer, `battle::world::set_battle_ai`,
set by `f4x_ai::battle::install()`, which `Ai::new`, `play_round`, `NeutralSeat`, `Console::new` and the client's `main` call) (it is not itself a `BattleBrain` object: it needs no state).
It does: (1) if the wizard may cast this turn, choose a cast from `legal_battle_casts` (section
6.5); (2) otherwise the unit orders of `ai::next_command`. `ai::next_command(&Battle)` stays for
callers that have no game (the client's auto-battle, the console tests, the learn crate) and is
the same unit logic without spells. `ai::baseline_command` is the old plain AI, kept as the
yardstick of section 8.2.

### 6.3 Unit orders (step 10)

Port the shape of `Target_Unit_Value` (section 4) over our `BattleUnit`:

- target value = expected damage we deal minus what it would deal back, +threat for shooters,
  +stick to the current target, -10 when confused, +20 for sleeping targets when shooting;
- ranged units shoot the lowest-hit-point target among near-equal values (focus fire);
  melee units hit the one that is worth most;
- a unit does not step into a square where it would be hit by more than it can hit, unless it
  can get the first strike or nothing else is left;
- ranged units keep their distance when the enemy has no shooters and they have ammo;
- flyers and units with the walls' immunities go over or round walls first;
- a wall crusher still goes for the wall (kept).

### 6.4 Defending and fleeing (step 11)

A defender stays inside the walls (original rule, section 4) unless the original's exceptions
hold. If the attacker's strength is under a fraction of the defender's it also sallies out to
finish them. A side whose total strength is under a tenth of the enemy's flees when fleeing is
allowed and the battle is not in a city it must hold (the core's `Flee`); the strategic AI
treats a fled battle as a loss without counting its units dead.

### 6.5 Spells (step 9)

`AITP_Combat_Spell` (not read in full) picks a target per spell. Ours: score each legal cast as
the value it removes or adds (a bolt by its expected damage on the best target; a unit
enchantment by the bonus on the strongest unit; a curse by its penalty on the strongest enemy;
a summon by the unit's strength; dispel and counter magic only against enemy enchantments), per
mana, and cast the best affordable one once a turn with a floor on the score. The mana budget
(`set_battle_mana_budget`) stays the player's limit.

**Task 600 (14 November 2026): the wizard spends what it may.** The wizard casts the best cast it
can afford every turn, with no floor on the worth (only a cast worth nothing is left alone);
the bounds are outside: the casting skill of the battle, the mana reserve minus what the
overland upkeep needs, and the budget the player or the strategic AI sets (task 601). The tiers
and the value per mana (softened by `MANA_SOFTENING`) order the casts. What each kind is worth
(`f4x-ai/src/battle/cast.rs`, `docs/battle-spell-ai.md` lists every spell):

- a blow: the game's own odds (`preview_spell_damage`), a kill first, mana fed until it kills;
- a slay: 35% of the worth of each enemy it can touch (`slay_allows`), times the chance that the
  enemy fails its save (`Battle::failure_chance`);
- a curse with a save: its flat share times the chance the target fails the save; a cap on a stat
  (Shatter) is a curse; a spell that holds a unit is worth half of it, one that takes it over all;
- a lasting gift on our own unit: 25% of its worth;
- a battlefield spell: a share of the worth of the units it falls on, ours for a gift (Prayer,
  Metal Fires, Mass Invisibility, Blur against shooters), the enemy's for a harm (Black Prayer,
  Terror, Wrack, Call Lightning, Entangle), the realm creatures for light and darkness;
- a wall: 20% of the worth of the units behind it; Counter Magic: 20% of our dearest unit while
  the enemy wizard has mana; a dispel: 10% of a unit's worth for each lasting spell on an enemy
  unit; a recall: a unit that is nearly dead; Raise and Animate Dead: a share of the unit that
  returns; Life Drain: 20% of the unit.

- the ground: Earth to Mud holds back the enemy walkers near the square (15% of their worth, less ours); Cracks Call is a one in four chance on the walker over the fissure plus the wall it breaks; Disrupt opens a wall section for the side that attacks the city;
- a takeover (Possession, Creature Binding) is worth 1.5 times the unit: the enemy loses it and we gain it.

**Task 601 (17 November 2026): the strategic AI sets the budget** (`f4x-ai/src/budget.rs`, used by
`military::resolve` for every attack of a wizard: the army, the perimeter, the pickup, the raids on sites and
cities). The limit is the spare mana (the reserve, less ten turns of any shortfall of the income
against the upkeep) times *stake* times *need*; at 0.75 or more there is no limit. *Stake*: a
city 1.0 (taken, held or fought for), a lair 0.6, our own ground within three squares of a
city 0.7, the open field 0.3; a hero on either side lifts it to at least 0.6. *Need*: 1 for a
fight the attackers hold with a strength ratio of 1.2 or less, falling in a line to 0.2 at a
ratio of 3 or more (a fight that is won anyway). The defender of a battle the AI does not start
spends what it may (the command carries one budget, the attacker's). Monsters and neutrals have no
wizard and so no budget.

**Task 600 closed (17 November 2026).** Every battle spell has a value, and each spell that waits
for something in a plain duel has a test that proves it gains its worth when the need is met: a
hurt unit of ours (Healing, Mass Healing), a lasting spell on an enemy (the dispels), a unit that
is nearly dead (the recalls), a shooter (Blur), the creatures of a realm (True Light, Darkness,
Banish, Holy Word, Dispel Evil), an enemy walker on the square (Earth to Mud, Cracks Call, Magic
Vortex), an enemy wizard with mana (Counter Magic, Mana Leak), a missile unit with arrows (Warp
Wood). Disrupt waits for a walled city under attack. The units' own casts (a hero's spell, a
charge, an ability) keep a floor of 12% of the dearest enemy (`WORTH_FLOOR`) on purpose: a unit
that casts gives up its attack, so a cast of no account is not worth it, where the wizard's
turn is its own. The shares and the budget constants are first guesses; task 640 tunes them
against the soak.

**Tasks 509 and 598 (17 November 2026): intents read from the effect** (`f4x-ai/src/intent.rs`). The
world spells are sorted by the typed effect of the rules (`GlobalEffect`, `WorldKill`), not by a list
of ids: a boon for any caster (Nature Awareness, Just Cause), one for a wizard whose books lean to
its realm (Chaos Surge), a strike while at war (Time Stop, Armageddon, Death Wish), a strike at
the realms of a wizard at war (Nature's Wrath, Life Force, Evil Omens, unless it hurts the caster's
own books), and never (Planar Seal); Great Unsummoning waits while the wizard has creatures of its
own. A match over the enum has no catch-all, so a new effect gets a use before the code builds.
New kinds: a storm or the Black Wind on the strongest enemy stack by a city of the wizard (at least
half the garrison), Corruption beside the biggest enemy city and Warp Node on its node, a volcano
by an own city, and an Astral Gate in the city the army crosses from. A wall goes to the city with
an enemy stack by it first. The spells still left alone are listed with the reason in `LEFT_ALONE`
(`wizardry.rs`), and a test keeps the list true. Trades: a fighter over `garrison_wanted` (one
to spare) is for sale at peace. What needs a soak is task 641.

### 6.6 The learned agent

`f4x-learn`'s baseline and imitation target is `next_command`. After step 10 its numbers move.
The review point of `learning-ai.md` section 13 reads them again; a note goes there when step 10
lands, and the learn tests are run at every battle step.

## 7. The win spell

Three changes make the Spell of Mastery winnable by the AI and by anyone else.

1. **Data.** The importer reads the research cost as an unsigned 16-bit value: 60,000. (It is
   `-5536` signed today and `max(.., 1)` turns it into 1.) Spells whose cost is above 32,767 are
   only this one.
2. **Engine (task 61).** A per-wizard `som_research_cost`, set at the start to the spell's cost
   (minus 3,000 for a wizard with every realm at rank 11, which our books cannot yet give, so
   not modelled) and reduced by half the research cost of each spell the wizard learns, however
   they get it. The spell is added to the research candidates only when the wizard's research
   pool is empty and they do not know it. Researching it takes that wizard's remaining cost
   rather than the spell definition's. Casting stays as it is: 5,000 mana paid over turns by
   the casting skill, winning at once when paid.
3. **AI.** Research: when offered, take it unless a spell the AI badly needs is offered too.
   Power split: once known, raise skill and mana until the cast is under way. Casting: the
   category weight is the original's 500, and the cast is never abandoned for another spell;
   upkeep is shed first (the AI casts no new upkeep spell while it is casting the Spell of
   Mastery). Diplomacy: every other wizard's relation to the caster drops when it starts
   (`diplomacy.rs` already does this), and a defending AI hits the caster's cities first when
   the announcement arrives (step 6 target value).

4. **The race (task 61).** When a rival starts casting the Spell of Mastery everyone is told
   (`Event::MasteryStarted`), and the AI can read the mana the caster has left (`World::mastery_casters`;
   the original's AI only reads that the spell is being cast, so this is more than it knows). Each turn
   `race.rs` compares the turns the AI needs (casting at its skill and mana income; plus the research
   when the spell is only offered) with the rival's (its mana left over the pace seen since the last
   turn; a rival seen once is taken to pay as fast as the AI). If the AI is at least a quarter
   faster it races: it cancels other casting and takes the spell first in research. If not, it declares
   war on the caster whatever the treaty (the original also lets a treaty stand only when the
   other is not casting it: `AIDATA.c`, +50 hostility) and `military.rs` campaigns against that
   wizard's cities first. Not done: sabotage by spells, asking allies to join.

What to expect: 60,000 research points is far past 200 turns of the income measured in section
2.3 (about 10 a turn). Wins in the soak will come from conquest first; the Spell of Mastery
shows up only in long games or with the research spell split. A test gives an AI wizard the
spell and mana and checks the game ends by it, so the cast path is covered without a 2,000-turn
soak. A soak option (`--grant-mastery TURN`) does the same in a whole game.

## 8. Measuring

### 8.1 Soak

`f4x-tools soak --seeds 1..8 --turns 400 --every 50 --detail` prints for each seed whether the
game ended, how and when, the battles and cities taken, and (with `--detail`) each wizard's
mana, spells, research, casting, power split and explored squares. The table below is filled in
as steps land; "ended" is games with a winner out of 8. Every soak prints, per game and over all games (and, in `soak-versus`, per kind of seat),
how often each spell was cast and the map spells nobody cast, and the same for the battles (`battle casts:`, a wizard's or a unit's cast, counted from the battle events; `battle spells never cast:`).

Every game also prints, at its start, a `profile pN:` line per wizard (the race of its capital, the
plane it started on, the rolled personality and objective, books and retorts), and, at its end, a
`neutrals:` line with the cities and units each neutral race and the monsters hold, and what
happened to them over the game (cities founded, lost and taken, battles won and lost, units lost; per wizard, units lost in battle and outside it).

| Build | Ended of 8 (400 turns) | End turns | Battles (all seeds) | Note |
|---|---|---|---|---|
| baseline (before this work) | 0 | none | 0, 0, 8, 1, 5, 159, 4, 35 | wizards never meet in most seeds |
| steps 1 to 7, 9 to 11 | 3 (seeds 3, 6, 8) | 251, 252, 269 | 69, 6, 21, 5, 7, 7, 8, 10 | seeds 2, 4, 5, 7 are separate islands or planes |

The same run per seed, from the soak lines (cities `c`, units `u` at the end):

| Seed | Baseline | Now |
|---|---|---|
| 1 | no winner; p0 24c/80u, p1 5c/27u; 0 battles | no winner; p0 11c/39u, p1 6c/11u; 69 battles, 17 cities taken |
| 2 | no winner; 0 battles | no winner (p0 stuck on 2 cities on its own island); 6 battles |
| 3 | no winner; 8 battles, 2 cities taken | **p1 wins by conquest, turn 251**; 21 battles |
| 4 | no winner; 1 battle | no winner (p1 holds one city on another land mass); 5 battles |
| 5 | no winner; 5 battles | no winner; 7 battles, 12 cities taken |
| 6 | no winner; 159 battles, 7 cities taken | **p1 wins by conquest, turn 252** |
| 7 | no winner; 4 battles | no winner; 8 battles |
| 8 | no winner; 35 battles | **p0 wins by conquest, turn 269** |

`soak --mastery off` or `--return off` closes the spell to every wizard of a soak (no human plays there; `human` is the same as `off`); see task 584.

`soak --diplomacy` is gone (23 November 2026, task 779): every wizard plays diplomacy system B (`ai-diplomacy-b.md`); the "By diplomacy system" table of the summary went with it. What it said until then: the option gave the wizards system A, B or both in turn; the summary had a "By diplomacy system" table (wins, wizards alive at the end, every count per wizard and game for each system; task 761). While B is a copy of A the two columns differ only by seat and chance: that is the size of the noise to read the first real difference against.

`soak --grant-mastery 40` gives every wizard the Spell of Mastery and its mana at turn 40: the
AI starts it at once and finishes it (seed 1: turn 134; seed 2: turn 136), so the cast path works
in a whole game. Without the grant nobody gets it in 400 turns (60,000 research points).

Soak is deterministic; two runs of one build give identical lines (timings stripped).

### 8.1a New against old (`soak-versus`)

To measure a change to the scripted strategic AI: freeze the AI before editing it
(`python3 scripts/fork_ai.py`, or `--from REF` for an earlier commit), change `crates/f4x-ai`, then
`python3 scripts/ai_battle.py soak-versus --seeds 1..8 --turns 400`. The reference is a local copy, ignored
by git and outside the workspace (`ai_battle.py` builds it with `f4x-tools --features reference`;
`scripts/ai_snapshot.py begin` / `end` make and drop one around an experiment), so a change to the core never
breaks the workspace's build or tests through it. When a change breaks it, do not repair it: mark
it with `python3 scripts/fork_ai.py --mark-stale`. `ai_battle.py` never forks it: when the reference is
missing, stale or no longer builds it stops and says so, and the reference is forked by hand
(`python3 scripts/fork_ai.py --from REF`, with `--replace` over an existing one, which `fork_ai.py` refuses
otherwise: the reference is the baseline of the measurement, and a fork from the working tree is the live
AI, so a battle right after it compares the AI with itself; changed 18 November 2026 after the 14 November
baseline was replaced by an automatic fork twice). Per seed it plays four games with the reference's
neutral cities and monsters (so the neutrals stay the same whatever the live AI does; fork again
after a change to `assault.rs` to test it): reference only, new in the first half of the seats,
new in the second half, new only. The report gives per wizard and game the counts of
`crates/f4x-tools/src/metrics.rs` for the reference, for the new AI and for both in the mixed
games, who won the mixed games, and the first snapshot (every 50 turns, `soak-out/`) that differs
from the reference-only game. `f4x-tools soak-diff A B` compares two snapshots. A snapshot is an ordinary game save (`World::snapshot`), with the plans of the AIs that can write them in it (`World::plans`; not the frozen reference), so the client can load it. `soak --from SAVE` (also `soak-versus`) goes on from such a save instead of a new game: the game is loaded by the game's own `load_game`, the AIs take their plans back with `Ai::recall`, and the rounds run in the same master as ever; one game, played to `--turns`. The snapshots of a run that went on from a save equal the straight run's (a test pins it). The learned battle
brain is not part of this; only the strategic AI. There is
one reference at a time; fork again to replace it.

**The reference of 20 November 2026.** The fork of 19 November no longer built against the core (it broke on earlier core changes), so the project owner ordered a new one from `d26eeb9b` (the commit before the changes to unit strength: one soldier definition, one cost, the combat ratings). It is not repaired beyond three call sites that name `def_power` and now pass the rules (`builder.rs`, `spellhelper.rs`, `wizardry.rs`). It reads the core's `unit_power`, so both AIs weigh units by the same ratings: what a soak against it shows is the AI's own logic, not the measure. The old fork is kept in the session scratchpad only.

`--mixed-only` plays only the two mixed games of a seed (new in the first half of the seats, then in the
second), for a long run of many seeds (`soak-versus --seeds 1001..1004 --turns 1200 --players 4 --rules 1.31
--mixed-only`; the reference-only and new-only games are left out). `ai_battle.py` passes it on.

*One exception to "do not repair" (19 November 2026, by the project owner's order).* The reference of the
14 November AI (tag `ai-2026-11-14`) no longer built against the core, so it was repaired once by hand: the
constants it took from the core are frozen in a `frozen` module of the copy, the renamed calls follow the
new names, and its `military.rs` `march` sends the `MarchLaunched` event the soak counts (a stack sent at a
city, no effect on play). The 14 November AI has no unit roles, no war plan and no marches called off (all
later), so those counters (`units adrift`, `missions over`, `war plan ... changes`, `marches called off`,
`unit-turns waiting for a ship`) are zero for it because the thing does not exist, not because nothing
happened, and its `marches launched` counts every stack sent (many times the new AI's count for a
campaign): leave all of these out when comparing the two.

What a soak counts for each wizard (`metrics.rs`), besides the counts above: battles by who attacked and what
the other side was (`attacks won on` / `attacks lost on` / `defenses won against` / `defenses lost against`
a wizard, a neutral city, a lair; a neutral city is the player of a race, a lair the monsters: tower, node
and encounter site), units lost in battle (`BattleCasualties` and units lost fleeing) and outside battle
(food, gold, mana, wounds), and in the game report a `wars:` line (`p1>p3@88`: who declared on whom, on
which turn) and per wizard the three places where it lost the most attacks on a neutral city or a lair.
Results of the runs: `docs/soak-baseline.md`.

Speed: the games run `--jobs N` at a time (default one for each core; games that print as they
go, with `--every`, `--trace` or `--detail`, run one at a time), 5.7 times faster on ten cores
with the same results. The reference-only game of each seed is kept in `soak-out/cache/` and
reused while the rules, content, reference AI and soak code are unchanged (`--no-cache` plays
it again); editing only `crates/f4x-ai` keeps it valid. Seeds where the wizards live on separate
land masses stay in on purpose: ships and landings are what the AI has to learn.

### 8.2 Battle strength

A test fights the new AI against the old (`ai::baseline`) on both sides of the same random
armies from the content, 200 fights, and requires the new AI to win more than it loses and not
to lose more than 45% on the weaker side. The old logic stays in the tree as `baseline` for
this. The learn crate's evaluation (`f4x-learn`) runs after each battle step.

### 8.3 Tests per step

Unit tests for the new decision functions (pure functions of the view); the workspace tests,
clippy, the learn tests (`--features train`) and the soak diff; each step records its soak line
in section 8.1.

## 9. Order of work

| Step | Task | What | Needs |
|---|---|---|---|
| 0 | 224 | Soak detail output, baseline table, `--grant-mastery` | done |
| 1 | 225 | Exploration and scouts (5.1) | done |
| 2 | 226 | Spell of Mastery in the engine: data, research list, cost (7.1, 7.2) | done |
| 3 | 227 | Research choice and power split (5.3) | done |
| 4 | 228 | Casting beyond summons; the win spell cast (5.3, 7.3) | done (task 228) |
| 5 | 228 | The special spells in the AI (Earth Lore, Word of Recall, Nature's Cures...) | done, except Transmute (task 434, skipped) and Plane Shift (task 495) |
| 6 | 229 | War: target value, memory of garrisons, defence (5.2, 5.4) | partly: unreachable targets, committed marches, strike force; no garrison memory yet |
| 7 | 229 | Stack strength shared with the battle AI | not done (`worth` and `expected_damage` exist in `f4x-ai/src/battle/order.rs`) |
| 8 | 230 | Ships and other land masses | not done |
| 9 | 231 | Battle: wizard brain with spells (6.2, 6.5) | done (`f4x-ai/src/battle/cast.rs`) |
| 10 | 232 | Battle: target values, focus fire, shooters hold (6.3) | done |
| 11 | 232 | Battle: defence and fleeing (6.4) | walls held; no sally, no fleeing |

Steps 1 to 5 and 9 to 11 do not depend on each other and may be done in either order. A step
that does not improve the soak or the mirrored fights is reverted and noted in this file.

## 10. Risks and open questions

- **Soak time.** 200 turns take about 15 s a game; eight seeds at 400 turns take about four
  minutes. Steps that make games longer or busier (more battles) cost more.
- **Game balance, not AI quality, may stop wars.** If the AI meets and still never wins, the
  rules (city defence, growth, upkeep) are the cause and the fix is a balance task (planned 24).
- **Islands.** About half the seeds put the wizards on separate land masses, or on separate
  planes. Without ships (step 8) or plane travel those games cannot end by conquest.
- **Original's pickers are stubs in ReMoM**, so the category weights are the only part to port;
  what each category chooses is our design and untested against the original.
- **Not checked**: how the original's AI explores, and how often its AI wins by the Spell of
  Mastery. We have not looked; the 60,000 research cost suggests rarely.

## 11. What was built, and what it taught

Written after steps 0 to 7 and 9 to 11. Each finding below was found by looking at a soak game,
not guessed; the fixes are small and each has a test (`crates/f4x-ai/tests/it/behaviour.rs`, the
unit tests in `wizardry.rs` and `f4x-ai/src/battle/order.rs`, `crates/f4x-learn/tests/it/mirror.rs`).

### 11.1 Strategic AI

- **No city holds more than nine** (`garrison.rs`, 11 November 2026). More than one stack adds
  nothing to the defence. Each turn `disperse` sends the strongest of the excess to the nearest
  city with fewer defenders than `garrison_wanted`, else on a raid (`site::deploy`), else to
  scouting; the army's rally point takes no more than `city_cap` either. (The summoning circle's
  city once also kept a slot free and walked the weakest fighter out before a summoning: removed 22
  November 2026, a summoning is no longer refused for a full square, `divergences.md`.)
- **Cities built the same unit for ever.** A city that finishes a unit starts the same unit again
  (`city/production.rs`: the item stays, progress goes to zero). The AI only chose production for a
  city that was not building a unit, so the capital made settlers until the wizard held 15 or 25
  outposts, most of which stay outposts for hundreds of turns (the closed planned item 21, "faster outpost growth", not needed). Now units
  are looked at again every turn. The change also made the games much quicker to run (seed 1 took 40 s for 300
  turns; 150 turns now take about a second) and, with the other changes, put
  real armies in the field.
- **Exploration** (`scout.rs`). One scout (two with four or more cities) walks to the reachable
  square that shows the most unexplored ground within its own sight, less the walk. Two fixes
  came from watching games: the count of unexplored squares must use the unit's own sight
  radius (with a larger one the scouts shuttled between two squares for ever), and a scout that
  has not moved for three turns gives its target up. Squares explored in 200 turns rose from
  about 150 to 650 or more, and the wizards met.
- **Production order.** Garrison one fighter a city (two in the fortress city), then a strike
  force of four plus half a fighter per city, then buildings, then more army. The gold on hand,
  over a hundred turns, counts as income, so a full treasury is spent instead of hoarded.
- **War.** A march is a state, not a mood: units that set out are not sent home when they stop
  next to the target; a force attacks at 1.1 times the defence once marched; units within two
  squares of the rally city count as gathered (a full city, nine units, left every later unit
  outside, so no force ever formed); the gathered units march as one stack for each square they stand on, nine at most (an order
  to move needs units on one square, and the first version of the wider gathering broke this: the
  force never left); a target that no
  march could reach is dropped for 40 turns and the next nearest city is tried.
- **Magic** (`wizardry.rs`). The power split is set each turn: the smallest mana share that
  covers the upkeep, the rest research, a fifth skill; while the Spell of Mastery is being
  cast, half mana and half skill. Research takes the cheapest in turns, adjusted by the original's
  idea (cheaper for a first summon or combat spell, dearer for a second of a kind). Casting was
  limited to spells paid for in one turn, and the casting skill of 25 to 30 is far below what a
  summon costs, so the AI cast almost nothing and sat on 800 mana. A spell is now paid over as
  many turns as the skill needs, with a third of the reserve left over; spells with a lasting cost
  are held back while upkeep takes more than half the power.
- **The win spell.** See section 7. `soak --grant-mastery 40` shows the whole path.

### 11.2 Battle AI

- **Spells made the difference; unit tactics did not.** In 300 mirrored fights (150 random
  armies, the new AI on each side in turn against the old) the new AI won 173 and lost 127. With
  only the unit changes (target by value, shooters hold, defenders hold walls) it won 123 and lost
  117, which the dice explain. So the wizard casting is the part measured to work. The unit changes
  are kept because they are cheap and do not lose, but they are not shown to win.
- A cast is scored from the spell's data: damage by the share of an enemy unit it takes,
  healing by the share of a hurt unit it restores, a summon by the strength of what appears, a
  blessing or a curse by a quarter of the unit's strength. Spells the AI cannot judge are not cast
  (battlefield enchantments, dispels, Counter Magic, Call Chaos).
- Mana kept back for the overland upkeep, so a battle does not starve the creatures and spells
  the wizard keeps going.

### 11.3 What is still open

- **Ships** (step 8, task 230). Four of the eight seeds put the wizards on different land masses
  or planes; the soak cannot end those by conquest. A trireme (two land units, needs a Ship
  Wrights Guild) or a galley (five, needs a Ship Yard, which needs a Sawmill) is built in a
  coastal city; a stack of the ship and its passengers moves with one `GoTo`; passengers land with
  an ordinary move onto the land next to the target. What the AI would have to add: a port, the
  buildings, a gathering of nine or more units over several trips, and a landing area it holds
  until the force is whole. Built in `naval.rs` (port, ship, one landing at a time); a ship loaded for a rival's city declares war first; what stays open is a second ship, planes (a sea scout lands a fighter on empty land, a ship takes a party to a guarded tower, a scout changes plane on a cleared one; an army does not cross yet), and why seeds 2 and 8 make no landing on the other wizard (task 230).
- **Ship bridges** (21 November 2026, task 738). A computer player's walkers are never routed over a ship that merely stands at sea (the game's pathfinder stops them at its square: `World::ship_stops`), because the units a job sends over it would be passengers no job knows about. Where one sea square lies between the land of a waiting unit and the land of its target, `bridge.rs` sends a free ship there and parks it (`Command::SetBridge`, `Role::Bridge`, `Crossing::Bridge` for the units that wait for it): the walkers are then routed over the ship as over a land square, as many at a time as it has places, and step ashore on the other side; no pickup or voyage takes the ship, and it is released after 40 turns without a request. A longer crossing stays a ferry (`pickup.rs`). The human's walkers use any waiting ship, as before.
- **Settlers by sea** (31 October 2026, rebuilt 18 November 2026: `colony.rs` is gone, task 665). A site on another landmass that a ship from a coastal city can reach over explored water is on the ranked list of `city_site.rs` with a crossing cost, below the sites of the home land. When no site is left on the home land a settler is given the best of them and tagged `Crossing::Sea`; the port builds the ship by the general rule for units waiting for one, the pickup planner (`pickup.rs`) fetches the settler first of all the groups, and after landing it is an ordinary settler (`expand.rs`). The sea scout is called off for the ship (`pickup.rs`, `naval::release_scouts`), and a landing on a rival's city waits for it (`naval::run`).
- **Memory of enemy garrisons** and a **threat map** (section 5.2): a city out of sight is
  guessed at three average units.
- **The special spells in the AI** (step 5): Earth Lore, Word of Recall and Nature's Cures are
  cast; Transmute is left uncast on purpose (task 434, skipped), Plane Shift waits for task 495.
- **Sallying out and fleeing** (6.4), **a shared strength function** (step 7): `worth` and
  `expected_damage` in `f4x-ai/src/battle/order.rs` are the pieces.
- **Heroes and items** (`heroes-items-design.md`) once they exist.
- **The economy limits the war.** A wizard's net income is a few gold a turn at 200 turns; a unit
  costs one or two gold a turn, so armies stop at 15 to 40 units. Outposts take hundreds of turns
  to grow (the closed planned item 21, "faster outpost growth", not needed). A better AI cannot fix that; it is a rules and balance question
  (planned 24).
- **Speed.** 400 turns take 30 to 190 s a seed; the cost grows with the number of units (a path
  search for each idle unit each turn). Not yet profiled.

### 11.4 Differences from the original

- Our wizards make no random choices; the original weights its spell categories and picks one at
  random. Ours is a function of what the AI sees, so a game replays exactly.
- The Spell of Mastery's research cost counts the starting spells' half-cost too (section 7);
  the original takes half of each spell's cost off as spells are gained, starting spells not
  included, and takes 3,000 more off for a wizard with every book at rank 11.
- The AI never uses what a human player cannot see, but `PlayerView::reachable` answers over the
  real map, fog or no fog, as the game's own path check does (task 181).

## 12. State after the entity rework (24 October 2026)

The scripted AI (`f4x-ai`, the battle AI in `f4x-ai/src/battle/order.rs` and `cast.rs`) and the trained one
(`f4x-learn`) already ran on the new entities (task 336 moved them to the handles). This pass checked
them, fixed what the soak showed, and gave the objective and the personality something to do. All
numbers are from `f4x-tools soak` on the shipped content, release build.

### 12.1 What was done

- **Battle AI.** `examples/situations.rs` in `f4x-learn` (and `tests/situations.rs`) fights the scripted
  AI on both sides on every terrain class, on the sites of the map (tower, three nodes, the encounter
  sites) with and without a hero that carries an item: 1,840 fights, every one ended, none was refused a
  command, none stalled (a battle averages 40 to 80 commands and 5 to 9 rounds). Mirror test: the smarter
  AI wins 171 and loses 129 of 300 (it must beat the plain one by 20). The trained network loads and plays
  whole games.
- **A bug found by the soak: a battle left open.** A cast, a field refresh or an end-of-turn effect could
  settle a battle without the command's events saying so; the battle then stayed open and every later
  fight was refused for the rest of the game (4 players, seed 2, from turn 219). `World::finish_if_over`
  closes it (`docs/KNOWLEDGE.md`). The soak now reports "rounds ended with a battle open".
- **Refused orders cut from 100+ a game to 0 to 4.** The neutral cities' raiders sent `EnterSite` every
  turn to an unguarded site (they now walk onto it), and a raiding party that cannot find its way home
  gives the raid up (`site.rs`).
- **Speed.** A 300-turn game took up to 78 s (seed 2). `reachable_squares` and `find_path` asked the
  world for the unit's abilities and for hostile units and guarded sites at every one of eight
  neighbours of every square. They take the unit's `StepProfile` once and remember the squares that
  stop the walk. The same game now takes 25 s; eight seeds of 300 turns take about two minutes.
- **Objectives and personalities steer the AI** (`objective.rs`, `ai-objectives.md` section 0, on-hold
  task 64). Cities wanted (Expansionist 10, Pragmatist 7, Militarist 6, Theurgist 6, Perfectionist 5), the
  strike army (Militarist 6 beyond the guard, Pragmatist and Expansionist 4, Theurgist and Perfectionist
  3), the focus of a city, the power split (Militarist favours mana; Theurgist and Perfectionist run
  mana lean for research), the research order (a liked sort of spell costs two thirds), and wars: a
  Militarist or Expansionist whose temper is not Lawful or Peaceful declares war once in twenty turns on
  the weakest neighbour it has met, has no treaty with and that holds fewer cities. The temper moves the
  strike army (Maniacal +2 to Peaceful -1). Tests: `objective.rs`, `diplomacy.rs`, `economy.rs`,
  `wizardry.rs`.
- **AI decisions out of the core's diplomacy** (design.md section 8). The rating of a proposal
  (`Diplomacy_Test`) moved from `World::rate_proposal` to `f4x-ai` (`diplomacy::rate`; the asker's patience is
  worn down by the core when a proposal is answered, whoever answers); the 11 percent war on a caster of
  the Spell of Mastery, which the core rolled for the offended wizard, is now the wizard's own roll
  (`diplomacy::mastery_scare`). Section 12.3 lists what remains.
- **Soak.** The lines show each wizard's objective and temper, cities with outposts, wars declared, the
  entity count with `--every`, and rounds that ended with a battle open.

### 12.2 Baseline (seeds 1 to 8, two wizards, 300 turns; 4 wizards seeds 1 to 4)

The current baseline (2 to 5 wizards, seeds 21 to 28, 1000 turns, per-seed results) is in `docs/soak-baseline.md`.

| Run | Games won | Battles per game | Cities taken | Wars declared | Refused orders |
|---|---|---|---|---|---|
| 2 wizards, 8 seeds | 1 of 8 (conquest, turn 118) | 9 to 76 | 0 to 4 | 0 to 1 | 0 to 4 |
| 4 wizards, 4 seeds | 0 of 4 | 34 to 79 | 2 to 11 | 1 to 3 | 1 to 9 |
| 2 wizards, 4 seeds, Mastery granted on turn 40 | 4 of 4 (Mastery, turns 134 to 136) | 4 to 15 | 0 to 2 | 0 to 1 | 0 to 1 |

Before the objectives (same code otherwise): 0 of 8 won, 12 to 78 s a game. A wizard ends at 1 to 7 cities
(often 1 to 3 of them outposts) and 5 to 30 units; the treasury sits at 50 to 100 gold.

**Statecraft, 14 November 2026 (task 510, `docs/diplomacy.md` section 8).** Measured with the parts switched off
(`AIK_DIPLO_ON_*=0`) and on, on `small-2p`, `hard-3p` and `islands-3p` of `scripts/soak_variety.sh`, six rounds of seeds (24, 18 and 24
games, 66 each way; the numbers are before the later gift to a hostile coalition partner and the 25-turn peace cooldown):

| | Won | By conquest | Mean winning turn | Wars declared a game | Peace offers a game | Refused or lapsed proposals a game |
|---|---|---|---|---|---|---|
| off | 34 of 66 | 18 | 314 | 22.0 | 19.8 | 0.8 |
| on | 32 of 66 | 13 | 339 | 17.6 | 66.3 | 52.2 |

The parts fire (a game has 2.6 wars on a weaker neighbour, 5.8 appeasements, 34 peace offers by a wizard tired of a war;
a later run with the gift to a coalition partner had 0 to 13 coalition requests a game). Fewer wars and fewer conquests are what the task
asks for (no war on two fronts, peace for a lost war), not a loss of strength; the number of wins stays within the noise
(`small-2p` 13 of 24 off, 9 of 24 on). The refused proposals are the peace offers of the loser, which the winner refuses
while the war is young and has taken nothing (`peace_ok`). The soak prints four new counts after the diplomacy kinds: `wars on a
weaker neighbour`, `appeasements`, `coalition requests`, `war-weary peace offers`.

**Trades, items and coalitions, 14 November 2026 (tasks 504 and 597).** 42 games (`small-2p`, `crowded-5p`, `islands-3p`, `hard-3p`,
three rounds of seeds), the same games in each run, so the games differ only by what changed:

| Run | Games won | By conquest | Trades done a game | Alliances ended |
|---|---|---|---|---|
| `AIK_DIPLO_COALITION_END=0` (the asking stops) | 24 of 42 | 11 | 0.02 | 0 |
| `AIK_DIPLO_COALITION_END=1` (the alliance is broken) | 24 of 42 | 11 | 0.02 | 0 |
| trade margins 98, 95, 90 and the best swap pair | 24 of 42 | 11 | 0.02 | 0 |

The two coalition runs are the same game for game: in 42 games no alliance made against a leader lasted until the leader
stopped leading (an alliance forms about 1.4 times a game, a request for one 4.8 times), so breaking one never came up and the default stays
the quiet end. The trades are rare for two reasons seen with a debug line in one game: the wizards' books differ, so one side
usually has several spells the other can learn and the other has none (`swap gives 0 gets 3`), and a sale for gold needs a buyer with the
gold (the computer wizards spend it). The first margins (110, 103 and 95) gave the same count, so the margin is not what
holds them back; trades with the human are by the same rule (`diplomacy.md` section 8). The soak line now has `trades offered`,
`trades done`, `trades with gold`, `trades of items or units` and `items scrapped` for the next look.

**Trade negotiation and the gold reserve, 14 November 2026 (task 599).** 42 games (same four setups, three seed offsets): 23 won,
7 by conquest, 47 trades offered, 17 counters, 18 trades done, 0.43 a game (before: 0.02). Nearly all trades were counters that
added a category or gold; 17 of the 18 included an item, unit or hero. The reserve (40 + 3 times net gold a turn) and the lower
trade reserve (40) were not tested alone (a run with `AIK_GOLD_RESERVE_INCOME=0` would isolate them). Wins fell from 24 to 23
of 42, within the noise of this sample.

### 12.3 Open issues

1. **The AI hardly fights.** Most two-wizard games have 0 or 1 war and 0 to 4 cities taken in 300 turns.
   Armies march only when they are well ahead (`military.rs` thresholds), nobody proposes peace or a
   pact (task 361), and a war once declared never ends. Wins by conquest are rare (one in eight).
2. **Expansion stops at 5 to 7 cities, and outposts are slow.** **Fixed 24 October 2026, not
   yet re-measured:** an outpost gains about 0.4 tens a turn and needed 100, ten times the original's 10
   (and started at 0 instead of 3), so the baseline above is too slow on outposts; run the soak again
   (the planned item 21 was dropped). With `WANTED_CITIES` met, a wizard stops settling. Land is often
   left free. Housing does nothing in an outpost (it makes no production).
3. **Overland magic is thin** (as of the first baseline; since then the AI casts curses and attacks on
   cities and stacks, Spell of Return, item crafting and the enchanting spells, `docs/soak-results-2026-11-17.md`).
   Still open: city protection (`spell_ward`, task 641) and research chosen by what its enemies cast
   (task 367); the 17 November soak left 145 known spells never cast on the map (tasks 641 and 642).
4. **Heroes and items are used little.** The AI hires the heroes it can pay for, but heroes march with
   the army as ordinary units, are not kept out of lair fights, and the AI never crafts items.
   Items (task 504, `offer.rs`): the merchant's item is bought when it is worth its price to the wizard
   (the worth of `f4x_core::diplomacy::trade::item_worth`: more with a hero that has a free slot for it, half with none,
   against the price less the merchant's markup of three); a hero takes the item of most use to it (a caster likes spell
   skill and charges, a fighter attack and defence) and gives its item up for one worth 130 percent of it, the old one
   returning to the vault; more than two items no hero could carry are broken up for mana, the cheapest first
   (`items scrapped` in the soak); a wizard sells items and spells to the others by the priced trade of
   `diplomacy.md` section 8.
5. **Battle AI.** It does not retreat or flee, does not sally from a city, and its attackers do badly at
   the three magic nodes (12 to 27 percent wins in the situations table: the guards there are strong, as
   they should be, but the AI does not weigh a lair before it attacks it with a small party; `site.rs`
   uses a strength guess). Unit casts (hero spells, item charges) are scored like the wizard's but
   happen about 0.5 times a fight.
6. **Human-only branches in the diplomacy rules** (task 368, decided 31 October 2026, `divergences.md`): the growth penalties for a big army and many cities, the peace duration after a war and the peace patience of a war now hold for every wizard pair; the record of the strongest action (a remark to the human) and Subversion's sparing of the human stay.
7. **Small refusals left:** `shortfall.rs` sells a building the city lacks (once or twice a game),
   neutral raiders and parties are refused a `GoTo` to a square they cannot reach (two or three a game),
   an `EnterSite` on an unguarded site is refused once before the fallback walk-in.
8. **Trained AI.** Not retrained. The battle network `run-20261016-v3/best.bpk` still loads and plays;
   `models-local/battle-v1.bpk` has no shape file and does not. The strategic learned AI is still only
   design.

## 13. Unit roles

Every unit has exactly one role, and none has an empty one. The roles live in one place, the
registry `Ai::roles` (`roles.rs`); the whole subsystem (data, groups, claim rules, the audit, how to
add a role) is in [`ai-unit-registry.md`](ai-unit-registry.md).

In short: a map from a unit's serial to one assignment (role, since, target square, party, tied
unit, standstill count). A claim of a role is refused for a unit that holds one of equal or higher
priority, so a unit cannot be raiding and marching at once. A unit with nothing to do has
`AwaitingOrders`. At the end of each turn `roles::audit` looks at every unit (soldiers, ships,
settlers, engineers, spirits): the units awaiting orders are counted, soldiers as *adrift* and
ships and the others as `ships and other units with no role`, and stay where they stand (since
task 645: no walk home, no wait on a shore; the role `Return` is gone). The soak shows
`units adrift (no role)`, and `unit-turns waiting for a ship` (units tagged for a ship that has not
taken them aboard, summed over turns; task 647). `troops landed` and `cities by sea` count from the
unit moves and captures, so they read the same under the tag-based landings. The bridges (task 738) show as `ship-turns held as a bridge` (ships that hold the post of a bridge, on their way or parked, summed over turns) and `unit-turns waiting for a bridge` (units tagged `Crossing::Bridge` that no walk leads over yet); both zero means no bridge was ever asked for.

**Why a settler failed** (`settler_fate.rs`, the soak line `settlers that failed`). The clock's count of dropped settler
plans says only that a plan ended. Each place that ends one says why: never picked up by a ship (the settler was tagged for
a ship and the clock ran out), did not arrive in time, site settled by someone else, site not fit for a city, way to the
site blocked, and (per turn) no suitable site. The soak adds what the game took from the AI: settlers lost in battle,
let go for lack of food, let go for lack of gold (`metrics.rs`, from the game's events). The refused-order log
(`--refused-log`; written by default to `soak-out/refused-TIME.tsv`, `--no-refused-log` turns it off) has the columns `seed turn player source reason command stood detail`. `stood` is for how many turns in a row
the same wizard has sent the same command from the same line and had it refused (1 for the first time). `detail` holds the
facts of the failure when the game knows them: why a go-to found no way (`NoPath`: the target is water, land for a ship,
full, an enemy, or the walk stopped at a square with a blocker beyond it, `unit/pathing/failure.rs`) and which square stopped a
standing order and what holds it. `python3 scripts/refused_summary.py FILE` groups the log by source, reason and kind of
detail and marks an order asked again for ten turns as a loop. A panic in a soak names the seed, turn and player and the last
order the AI sent.

The escort role is built (`escort.rs`: a settler leaving a city takes a spare soldier along). The new roles perimeter, road and meld are tasks 625, 627 and 628.

## 14. Job board (tasks 645 to 648)

Decided on 17 November 2026, after the overnight soak: the unit-first logic ("I have a unit, what do I
do with it") lets every module take units and produces uncoordinated orders. The new order of thought is
job-first. A unit with nothing to do awaits orders where it stands. At the end of the turn the
dispatcher counts the waiting units, goes through the open jobs in a fixed order (rally at war, the army
against an enemy city, a scout, a raiding party, an under-defended city, a settler, an engineer) and
hands each the units it needs; what is left stays waiting. A unit awaiting a ship carries the
destination it wants, and ships take units that go to the same landmass, whatever they are. Tasks 645
(the pool, the dispatcher and the first jobs; a city's base defenders keep `Role::Garrison` and are not in the pool, only the soldiers beyond the defence strength a city needs are; the need is a strength read from the enemies' nearness and the game phase, met first by slow defensive units, while cavalry, fliers and heroes stay free for scouting and war), 646 (the war jobs) and 647 (transport) in `TASKS.md`.

## 15. War plan (task 648)

Asked for on 17 November 2026. What is there (`war::run`): diplomacy (`statecraft.rs`,
`diplomacy.rs`) decides *whether* to fight. Then one campaign target is chosen, the enemy city with the
lowest "distance plus remembered garrison". The units that happen to be free gather at the own city
nearest it and march when `force_wanted` is met. Everything else (a stack next to a weaker stack, the
perimeter, raids) is a reaction. Nothing sits between "we are at war with X" and "these units go
there". So the AI attacks the nearest city. It cannot take the weak city behind the front, make a
pincer, go for the production bases or the fortress by a chosen path, or look before it commits.

**Where it sits.** Diplomacy, then the **war plan**, then the dispatcher jobs and the builder, then
the unit orders.

    statecraft: war with X, peace with X
        |
    war plan (one per enemy): analysis, targets, recon, phases
        |  demands: recon squares, groups (rally, strength, target, start turn), strength missing
    dispatcher jobs (646) and the builder (economy.rs)     transport (647) for the sea and the planes
        |
    unit orders

The plan is the only place that chooses a target. The dispatcher never decides *what* to attack, only
fills a demand from the pool. If the pool cannot, the demand stays open and the builder reads the
missing strength.

**Analysis (what a human can see).** For the enemy at war:
- the cities known, each with the remembered defence and its age (`city_memory`), the size and
  buildings seen, and whether it is the fortress city;
- the distance from our cities and from their other cities (depth: a city behind others is "in the
  back");
- the route in (land path, open tower, Plane Shift, sea) and what is known of the squares on it;
- the army scores.

What the AI has not seen is *unknown*, not guessed, and the age of a picture counts against it.

**The town list and two priorities** (decided with the user, 17 November 2026). The strategist lists
every town of the enemy at war (known or remembered, with the age of what is known) and rates each on
two separate scales:

1. **The war goal**: the prize the war is for. A fortress city, or the main production site. Rated by
   importance (the wizard's fortress or capital, size, buildings seen), the closeness to our land,
   and whether it is reachable at all. The goal is the long-term aim and changes rarely. It is not
   always the highest value: a good target on the same plane is taken before a fortress on the other
   plane, because the first is reachable now and the second needs a tower, a gate or a ship.
2. **The next target**: the town to attack now, on the way to the goal. Rated by strategic location (it
   lies on the route to the goal, it cuts the enemy's roads, it protects our border), closeness to
   our units, a landing bridge or foothold (a coastal or planar-tower town that lets the army cross),
   and a low defence (the strength to beat, with the age of the picture). The next target is
   re-chosen when it falls or the picture changes.

The goal sets the direction and the next target sets the step. A weakly held town in the back is a
next target when taking it is cheap and does not pull the army off the route, and a pincer is a second
next target on another axis, started so that both groups arrive in the same turn. A front that is only
held is a *screen*: no attack, but our cities near it are covered while the army is away.

**Recon before commitment.** A target whose picture is older than the limit, or whose route has unseen
squares, becomes a *recon demand*: a fast unit goes (`scout.rs`, job 3). No group is committed to a
target it has not seen recently, unless it is the only one in reach and the plan has waited the limit.
A town whose picture is old also counts against its rating on both scales until it has been seen again.

**Phases per group.** Recon, mass (at the rally point the plan names, on the side of the target the
route comes from), strike, exploit. The groups of a pincer start together: a group's start turn is the
arrival turn of the slowest group minus its own trip. A group that has made no progress for a limit of
turns, a destroyed stack, or a target that is no longer worth it makes the plan revise. A better target
must beat the one in progress by a margin, so the plan does not swing between two.

**What it replaces.** `ai.campaign`, `best_enemy_city`, `planar_target`, `march_called_off`, the rally
choice and the target selection in `war::run`; the builder's `war_force` becomes the plan's
missing strength. Step 1 of the army (a stack hits a weaker stack next to it) and the perimeter stay as
reactions. They take their units from the pool but may not take a group's reserved units.

**Order of work.**
1. 648: analysis and targets, with the recon demand and one group.
2. 646: the dispatcher fills the plan's demands.
3. Pincers and pickoffs (several groups at once).
4. 647: transport as a route type.

The soak against `crates/f4x-ai-ref` is the measure at each step: conquest wins, the turn of the win,
cities taken per war, turns from the declaration to the first city taken, armies lost.

**Built (17 November 2026, first step).** `war_plan.rs`: the town list with the city report and the
walk; `goal_score` and `next_score` on plain data (tests); the plan kept in `Ai::war_plan` and revised
every turn by `war::run`, which sets its campaign from it; the recon list read by
`scout::best_target_with`. `expected_defence` reads the garrison from the city report for a city the AI
has explored, as the human's enemy city screen does, so the remembered strength (`city_memory`) is only
the fallback. Not built: several groups, the route recon, the demands to the builder and the dispatcher.
The numbers (`FORTRESS_WORTH`, `GOAL_ACROSS`, `NEXT_DEFENCE` and the others in `war_plan.rs`) are first
guesses, to be set from soaks with more than two wizards.

## 16. Enchant Road (task 642)

`enchant_road.rs`. The spell enchants every road within two squares of the square it is cast on, and
a step between two enchanted road squares costs no movement, so a road is worth enchanting from end
to end, a cast for every five squares. The places to join to the fortress, in order: a harbour city
(when the fortress is inland), the other large cities (8 citizens or more), the border towns (an own
city within `BORDER_RANGE` of a wizard's city) and the open towers. A place needs to be joined to the
enchanted network of its continent once: the road is searched from the place to the nearest square of
that network, not to the fortress, and a continent without the fortress has its own network around
its largest city. The cast square is the third square from the enchanted end of the road between
them. Myrror needs none. Cast rank 2 (before the summons), research at half the turns when the AI
has no spell of the kind. It enchants only the roads that exist; building them is task 650.

**Scope (17 November 2026).** The basics come first (task 648): one army, one target at a time, one stack
that attacks from one square, the next target from where the army stands, the campaign's units
reserved, the plan visible in the soak. The pincer, the pickoff, the screen, several groups and the
route recon are task 651 and come after task 646 (the dispatcher fills the plan's demands) and task 649.

**Party, not stack (17 November 2026).** A square holds `max_tile` units (12) but a battle holds
`max_battle_side` (9) a side, and nobody joins a battle once it has begun. Every plan that attacks,
gathers, marches, crosses or lands therefore counts and moves the *party* (`view::party`: the
strongest nine of the units, strongest first), not the stack: the sorties of `strike.rs`, the
gathering and the march (a muster is full at nine, `force_wanted` is at most nine), the crossing
by a tower, a landing (judged square by square), the perimeter, the stranded groups of `pickup.rs`
and the neutral stacks. The soldiers beyond the party stay where they are, available. A stack of
twelve walking about only crowds the squares it stands on and blocks the next stack.

**One measure of a unit's strength (17 November 2026).** `f4x_core::battle::power`
(`unit_power`, `def_power`, `PlayerView::unit_power`): what a unit adds to a battle, with no side
to it (a battle is a series of blows both ways): attack (the better of melee and ranged, a quarter
of the other), defence with a tenth of resistance, hit points over all figures at *full* health, and
the ability bonuses (one table, `ABILITY_BONUS`). The strategic AI (`view::unit_strength`), the
spell helper, the autobuilder (its offence profile is the strength with a little for speed, its
defence profile the strength) and the engine's fighter preset use it; the battle AI weighs a
unit in the fight by the same kernel with the hit points it has left. Hurt units are not weighed
less: the army builders leave out units that need rest (`recover::needs_rest`). The numbers are a
first draft, eyeballed. *Mobility* (how fast a unit gets to a square: moves, movement type,
terrain abilities, the slowest of a party unless a spell lifts it) is the second measure and is
not built yet: a garrison does not need it, a march is as slow as its slowest unit, a scout is all
about it.

**Mobility, the second measure (17 November 2026).** `f4x_core::unit::mobility` (`unit_mobility`,
`def_mobility`, `party_mobility`, `PlayerView::unit_mobility`): the movement rate times a flat
multiplier for the kind of movement, the best one that applies and never several multiplied
(non-corporeal 3, wind walking 3, flying 2, pathfinding 2, swimming 1.5, mountaineer, forester and
water walking 1.2; cavalry adds nothing by itself). Wind walking and pathfinding carry the whole stack, so they rate above what only helps the
unit; no unit has pathfinding by itself (the spell and an item give it), the wind mage and the djinn
have wind walking. No walk over the terrain: a flier with two
moves gets where a walker with four is held up. A party goes at the pace of its slowest unit, unless
a unit of it carries the whole stack (pathfinding, wind walking), which doubles each unit's own rate.
A garrison does not ask for it, a march or a raid asks for it only when a fast strike party is
wanted, a scout is chosen by it first (the autobuilder's exploration profile and its offence
profile read it). The war jobs of task 646 use `party_mobility` to pick a strike party.


## Quick strikes (17 November 2026)

`strike.rs` runs first in the dispatcher's turn, before any unit has used a move or received an order. A stack next to an enemy square it can clearly beat (the party of nine against what is seen there, with the ratios of `military::attack_ratio`, `committed_ratio` and, out of a city, `sortie_ratio`) fights now. Targets: units of wizards and neutral cities we are at war with, and roaming monster bands; never a lair's guards, and never a town that is a war target (the plan's goal and next target, `ai.campaign`, `ai.march_target`). The strike changes nothing in the registry; the units that fought stand on the enemy's square and the dispatcher's repair sends them on to their targets. A city's garrison may join when no other enemy is near the city; its soldiers walk home at once. The engine needs all attackers on one square, so gathering units from several squares first is a later step.
