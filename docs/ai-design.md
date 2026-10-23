# The scripted AI: strategic and battle

Design for the hand-written AI players: the strategic AI (`crates/f4x-ai`) and the conventional
battle AI (`crates/f4x-core/src/battle/ai.rs`). The learned AI is a separate design
([`learning-ai.md`](learning-ai.md)) and is not touched here. Related tasks: 46 (battle AI), 61
(Spell of Mastery), 64 to 67 (personalities, AI v1, spells, attack), planned 24 (soak runs), 106
(battle views), 224 to 232 (this design's steps, section 9).

Status (13 October 2026): steps 0 to 7 and 9 to 11 are built and tested; step 8 (ships) is not.
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
   the same commands. (It has the difficulty bonuses of the game settings, task 247, unless the
   game is the symmetric `flat` one.) Where the original's AI knows more (it reads the whole map), we do not.
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
| `economy::taxes` | The lowest tax rate that leaves a small surplus. |
| `economy::research` | When idle, takes the first of the offered candidates (the core lists the fastest first). |
| `economy::cast` | Starts the dearest summoning spell it can pay for with a third of the mana left over. Nothing else is cast, so most spells are dead weight. |
| `expand` | Settlers pick the best explored site within nine squares by terrain value and walking distance. |
| `military` | Garrison two per city; hit adjacent weaker stacks (`AutoResolve`); pick the nearest known enemy city; gather three or more at the nearest own city; march when strong enough; spare units explore. |
| `economy::cities` | Housing in outposts, a fighter for an empty city, settlers up to seven cities, garrisons, buildings from a fixed list, an army up to `cities * 3 + 2`, else trade goods; buys production with spare gold. |

It never sets the power split (mana, research, skill stay at 33/34/33), never uses heroes,
items, nodes, lairs, ships, gates, plane travel or the new special spells, and does not use
`legal_battle_casts`. Battle commands never come from the AI crate: a battle is run by the core,
and its AI is `battle::ai::next_command`, which is given only the `Battle`.

### 2.2 Battle AI (`battle/ai.rs`, 178 lines)

Every unit goes for the nearest enemy and attacks, shoots when it cannot reach, and walks round
walls. It does not cast spells (it has no `Game`), does not choose targets by value, does not
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
view keeps the rule that lists are limited to what the player knows. It wraps `Game`'s own
queries and the domain handles `Units` (`game.units()`) and `Economy` (`game.economy()`), so a
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
| Income: mana, research, skill, upkeep (`income`), and the split | choose the split; know what can be afforded | `Game::income`, `SetPowerSplit` |
| Research state: remaining points, the Spell of Mastery's cost | pick research; plan the win spell | `Magic`, new `som_research_cost` (section 7) |
| Spell facts: known set, castable now, target rules, upkeep | cast beyond summons | `check_target`, `spell_needs_target`, `SpellDef` |
| Spell categories | the original's weights are by category | derive from the `SpellDef` effects (section 5.3) |
| Enemy wizards: contact, relations, their visible globals, who is casting the Spell of Mastery | war target, disjunction, the race | `diplomacy`, `Game::magic` of others (only what a player sees) |
| Remembered enemy garrisons | a city out of sight is not forgotten | new memory kept by the AI, or in the view |
| Unit levels, experience, mana, hero data | stack value | `Units` |
| Lairs and nodes: guard strength, reward | free growth for a strong stack | `feature/lair.rs`, `feature/node.rs` |
| Threat map: where enemy stacks are near my cities | defend, recall | derived in the AI from `hostile_at` |
| Ships and transports | reach other land masses | `unit/transport.rs`, `unit/movement.rs` |
| Battle: legal casts, spell costs, field state | the battle AI as a wizard | `Game::legal_battle_casts`, `battle_spells` |

**Added so far** (all read-only methods on `PlayerView`, each a call into a domain function):
`income` (split, research, mana upkeep), `research_cost` (the Spell of Mastery's depends on the
spells known), `paid_spell` and `spell_needs_target` (a cast paid over turns waits for its
aim), `can_aim` (the game's own target check, so the AI never guesses), `unit_enchantments` and
`city_enchantments`. The AI crate reads nothing else of the game, and writes only `Command`s.
`Game::explored_squares` and the `Fog::explore_block` the special spells use are in the core.

Each addition is a read-only method on `PlayerView` that calls a domain function. None gives
the AI a fact a human could not read off a screen; where a domain function answers more than the
screen shows (for example `reachable_squares` ignores the fog), the view limits it.

### 3.3 The battle AI needs the game, not only the battle

`next_command(&Battle)` cannot cast: spells need the wizard's mana, skill and known spells, which
live in `Game`. `Game::next_battle_command` already calls a brain with the whole `Game`; the
scripted AI becomes a second brain of that kind (section 6.2).

## 4. What ReMoM's AI does (reference)

From `reference/ReMoM/doc/ComputerPlayer/` and the sources it points to. The ReMoM source is a
reconstruction, and several leaves are still stubs there; this is said below where it matters.
Not everything in these notes was read; the sections that were are named.

- **Turn pipeline** (`AI_Next_Turn`, read): per wizard, in this order: strategy and hostility,
  target values for cities and stacks, landmass values, war landmass choice, spell selection and
  casting, gold and mana reserves, colony autobuild, evaluation map, orders, cull; then movement
  (`AI_Execute_Orders`); then the neutral player (colony autobuild, farmers, destinations, movement; ours is `neutral.rs`: farmers, taxes, a Barracks first, then soldiers while the garrison is short and buildings after, never settlers; it never moves),
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

- Buildings by need: an income-first order until the treasury is positive, then research,
  then military; skip buildings whose upkeep the treasury cannot bear (already done) and prefer
  the ones that raise mana for a wizard with a long spell queued.
- Settlers: more cities earlier (seven today); stop when no site with value over a threshold
  is left, not at a fixed count.
- Army size from the war plan, not `cities * 3 + 2`.
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

Same inputs as a player: the `Battle` and, for spells, the `Game`. No randomness of its own. A
step is kept if the new AI beats the old one in mirrored fights (section 8.2) and the battle
and learn tests still pass.

### 6.2 A brain with the game (step 9)

Built as `battle::brain::next_command(&Game)`, which `Game::next_battle_command` calls for every
wizard that has no learned brain set (it is not itself a `BattleBrain` object: it needs no state).
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
   (`Event::MasteryStarted`), and the AI can read the mana the caster has left (`Game::mastery_casters`;
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
as steps land; "ended" is games with a winner out of 8.

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

`soak --grant-mastery 40` gives every wizard the Spell of Mastery and its mana at turn 40: the
AI starts it at once and finishes it (seed 1: turn 134; seed 2: turn 136), so the cast path works
in a whole game. Without the grant nobody gets it in 400 turns (60,000 research points).

Soak is deterministic; two runs of one build give identical lines (timings stripped).

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
| 4 | 228 | Casting beyond summons; the win spell cast (5.3, 7.3) | done, except the special spells |
| 5 | 228 | The special spells in the AI (Earth Lore, Word of Recall, Nature's Cures...) | not done |
| 6 | 229 | War: target value, memory of garrisons, defence (5.2, 5.4) | partly: unreachable targets, committed marches, strike force; no garrison memory yet |
| 7 | 229 | Stack strength shared with the battle AI | not done (`worth` and `expected_damage` exist in `battle/ai.rs`) |
| 8 | 230 | Ships and other land masses | not done |
| 9 | 231 | Battle: wizard brain with spells (6.2, 6.5) | done (`battle/brain.rs`) |
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
not guessed; the fixes are small and each has a test (`crates/f4x-ai/tests/behaviour.rs`, the
unit tests in `wizardry.rs` and `battle/ai.rs`, `crates/f4x-learn/tests/mirror.rs`).

### 11.1 Strategic AI

- **Cities built the same unit for ever.** A city that finishes a unit starts the same unit again
  (`city/production.rs`: the item stays, progress goes to zero). The AI only chose production for a
  city that was not building a unit, so the capital made settlers until the wizard held 15 or 25
  outposts, most of which stay outposts for hundreds of turns (`PLANNED.md` item 21). Now units
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
  until the force is whole. Nothing of that is built.
- **Memory of enemy garrisons** and a **threat map** (section 5.2): a city out of sight is
  guessed at three average units.
- **The special spells in the AI** (step 5): Earth Lore, Word of Recall, Nature's Cures,
  Transmute, Plane Shift exist in the core; the AI casts none.
- **Sallying out and fleeing** (6.4), **a shared strength function** (step 7): `worth` and
  `expected_damage` in `battle/ai.rs` are the pieces.
- **Heroes and items** (`heroes-items-design.md`) once they exist.
- **The economy limits the war.** A wizard's net income is a few gold a turn at 200 turns; a unit
  costs one or two gold a turn, so armies stop at 15 to 40 units. Outposts take hundreds of turns
  to grow (`PLANNED.md` item 21). A better AI cannot fix that; it is a rules and balance question
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

The scripted AI (`f4x-ai`, the battle AI in `battle/ai.rs` and `brain.rs`) and the trained one
(`f4x-learn`) already ran on the new entities (task 336 moved them to the handles). This pass checked
them, fixed what the soak showed, and gave the objective and the personality something to do. All
numbers are from `f4x-tools soak` on the shipped content plus `content-local`, release build.

### 12.1 What was done

- **Battle AI.** `examples/situations.rs` in `f4x-learn` (and `tests/situations.rs`) fights the scripted
  AI on both sides on every terrain class, on the sites of the map (tower, three nodes, the encounter
  sites) with and without a hero that carries an item: 1,840 fights, every one ended, none was refused a
  command, none stalled (a battle averages 40 to 80 commands and 5 to 9 rounds). Mirror test: the smarter
  AI wins 171 and loses 129 of 300 (it must beat the plain one by 20). The trained network loads and plays
  whole games.
- **A bug found by the soak: a battle left open.** A cast, a field refresh or an end-of-turn effect could
  settle a battle without the command's events saying so; the battle then stayed open and every later
  fight was refused for the rest of the game (4 players, seed 2, from turn 219). `Game::finish_if_over`
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
  (`Diplomacy_Test`) moved from `Game::rate_proposal` to `f4x-ai` (`diplomacy::rate`; the asker's patience is
  worn down by the core when a proposal is answered, whoever answers); the 11 percent war on a caster of
  the Spell of Mastery, which the core rolled for the offended wizard, is now the wizard's own roll
  (`diplomacy::mastery_scare`). Section 12.3 lists what remains.
- **Soak.** The lines show each wizard's objective and temper, cities with outposts, wars declared, the
  entity count with `--every`, and rounds that ended with a battle open.

### 12.2 Baseline (seeds 1 to 8, two wizards, 300 turns; 4 wizards seeds 1 to 4)

| Run | Games won | Battles per game | Cities taken | Wars declared | Refused orders |
|---|---|---|---|---|---|
| 2 wizards, 8 seeds | 1 of 8 (conquest, turn 118) | 9 to 76 | 0 to 4 | 0 to 1 | 0 to 4 |
| 4 wizards, 4 seeds | 0 of 4 | 34 to 79 | 2 to 11 | 1 to 3 | 1 to 9 |
| 2 wizards, 4 seeds, Mastery granted on turn 40 | 4 of 4 (Mastery, turns 134 to 136) | 4 to 15 | 0 to 2 | 0 to 1 | 0 to 1 |

Before the objectives (same code otherwise): 0 of 8 won, 12 to 78 s a game. A wizard ends at 1 to 7 cities
(often 1 to 3 of them outposts) and 5 to 30 units; the treasury sits at 50 to 100 gold.

### 12.3 Open issues

1. **The AI hardly fights.** Most two-wizard games have 0 or 1 war and 0 to 4 cities taken in 300 turns.
   Armies march only when they are well ahead (`military.rs` thresholds), nobody proposes peace or a
   pact (task 361), and a war once declared never ends. Wins by conquest are rare (one in eight).
2. **Expansion stops at 5 to 7 cities, and outposts are slow.** An outpost gains about 0.4 tens a turn
   and needs 100 (planned item 21); with `WANTED_CITIES` met, a wizard stops settling. Land is often
   left free. Housing does nothing in an outpost (it makes no production).
3. **Overland magic is thin.** The AI casts summons, unit and city buffs, a few global buffs, Earth Lore,
   Nature's Cures, Resurrection and the Spell of Mastery. It never casts a curse or an attack on a city
   or stack, never uses city protection, Spell of Return (task 339 note), item crafting or the
   enchanting spells, and does not choose research by what its enemies do. Mana sits unspent in
   long games.
4. **Heroes and items are used little.** The AI hires the heroes it can pay for and equips them by
   worth (`offer.rs`), but heroes march with the army as ordinary units, are not kept out of lair
   fights, and the AI never trades or crafts items.
5. **Battle AI.** It does not retreat or flee, does not sally from a city, and its attackers do badly at
   the three magic nodes (12 to 27 percent wins in the situations table: the guards there are strong, as
   they should be, but the AI does not weigh a lair before it attacks it with a small party; `site.rs`
   uses a strength guess). Unit casts (hero spells, item charges) are scored like the wizard's but
   happen about 0.5 times a fight.
6. **Human-only branches remain in the diplomacy rules** (core): relation changes towards the human
   army size and city count (`diplomacy_growth`), the human's peace duration after a war
   (`declare_peace`), `peace_patience` moves only for a human attacker, `last_action` is kept only for a
   human attacker, subversion spares the human. They are the original's AI-only rules, not decisions,
   but they are exceptions between human and AI; task 368 decides each.
7. **Small refusals left:** `shortfall.rs` sells a building the city lacks (once or twice a game),
   neutral raiders and parties are refused a `GoTo` to a square they cannot reach (two or three a game),
   an `EnterSite` on an unguarded site is refused once before the fallback walk-in.
8. **Trained AI.** Not retrained. The battle network `run-20261016-v3/best.bpk` still loads and plays;
   `models-local/battle-v1.bpk` has no shape file and does not. The strategic learned AI is still only
   design.

