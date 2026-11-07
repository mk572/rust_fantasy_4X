# Mechanics and fields the original does not have

Task 407 (part 1 of the old task 302). Every row is a mechanic, a data field or a rule of the core or
the data crates that no part of the original needs, with where it lives, what uses it, the evidence
that the original has no such thing, and a **proposed** decision. Nothing was deleted. The decisions
are for the user to confirm; a removal becomes a task of its own.

Deliberate departures from a rule the original *has* (a different number, a different order) are in
[`divergences.md`](divergences.md), not here.

## How it was checked

- The first paragraph of every module of `f4x-core/src` was read (about 100 files), and the schema of
  the rules (`rules/defs.rs`: race, unit, building, spell, item, power, level, config fields) and the
  `Custom(..)` and `Special` effects of `crates/f4x-def/content/spells.ron` (99; all are spells of the original).
- A claim "the original has no such thing" was checked in ReMoM (`~/projects/ReMoM/MoM/src`) by a search
  for the original's own name of the thing; where the search found nothing but I did not read the whole
  code around it, the row says so.
- **Walked in task 409:** the first paragraph of every file of `battle/`, `unit/movement.rs` and `map/mapgen*`, the settings (`f4x-def/src/settings.rs`, `game/setting.rs`, `game-settings.md`) and the vocabulary of `rules/effects.rs`. Rows 15 to 24 are what it found (23 and 24 are dead vocabulary of the rules data, found by searching the shipped content for each name). **Still not read:** the bodies of `battle/spell.rs` and `battle/attack.rs` beyond their headers, and the crates `f4x-ai`, `f4x-ai-ref`, `f4x-learn`, `f4x-tools`, `f4x-console` (our own by design: the original's AI is not a port; see `design.md` section 8).

## The table

| # | What | Where | What uses it | Evidence | Proposed decision |
|---|------|-------|--------------|----------|-------------------|
| 1 | **Undo** of orders within a turn | `game/undo.rs` (`Game::set_undo`, `Game::undo`); the client's Undo (`view.rs`) | the client while a turn is played | ReMoM has no undo: the word occurs only in two comments (`Combat.c`, `UNITSTK.c`) | Keep: a convenience that gives no hidden information (a command that explored, drew a random number or began a battle cannot be undone). Say so in `design.md` if it is not there |
| 2 | **City auto-builder** (a build plan per city) | `city/autobuild.rs`, `rules.autobuild_order`, the client's dialog, `docs/auto-builder.md` | the city screen and the production screen | The original's `autobuild` code is the computer's (`AIBUILD.c`); a human city has none | Keep: asked for as PLANNED item 7 |
| 3 | **Replay / command log** | `game/replay.rs`, `game/replay/` | `f4x-tools`, the soaks, the learning crates | Tooling, not a rule | Keep |
| 4 | **Override (cheat) mutators** and the override log in the save | `game/cheat.rs`, `Snapshot.override_log` | the client's test hooks, the screenshots, tests; the console once more under task 415 | Not a rule | Keep |
| 5 | **Ships are decks** in battle: a ship has a deck of squares, land units fight on it, a walker may jump one square of water | `unit.deck` in `units.ron`, `battle/`, `docs/combat.md` section "Ships" | battles at sea | `combat.md` says it is our own rule; the original keeps land units out of a sea battle | Keep, as the user's design; already written down |
| 6 | **One player per neutral city**, run by a caretaker (farmers and tax balanced each turn) | `city/neutral.rs`, `player/mod.rs`, `divergences.md` section 2 | every game with neutral cities | The original has one shared neutral owner and no treasury | Keep: decided with task 31 |
| 7 | **Game settings and the levels' difficulty extras as data** | `game/setting.rs`, `game-settings.md` | new game, options | The original hard-codes the difficulty extras | Keep: decided with task 247 |
| 8 | **The AI**: wizard objectives acted on, battle AI, the neutral AI, the learned AI | `f4x-ai`, `f4x-learn`, `wizard/ai`, `f4x-ai/src/battle/order.rs`, `f4x-ai/src/battle/cast.rs` | the computer wizards | Not a port; the original's own AI code is not used | Keep: this is the project's own part |
| 9 | **Our map generator** | `map/mapgen/` | new game | `map-layers.md` section "generator": our own design, not a port | Keep |
| 10 | **Dispellable enchanted roads** | `spell/world.rs` (Enchant Road) | Enchant Road, Disenchant Area | `help-conflicts-checklist.md` item 55: the original's cannot be dispelled | Keep: divergence 27 |
| 11 | **Depletion and New Minerals** by our own rule | `spell/terraform.rs` | those two spells | ReMoM stubs both (`divergences.md` section on mineral spells) | Keep |
| 12 | **Sounds and tracks the original has not** (life and death node music and others) | `docs/sound.md` section 5.3 | the client | Not a rule | Keep |
| 13 | **Spell fields `not_with`, `already_able`, `dispel_difficulty`, `save`** | `rules/defs.rs` `SpellDef` | the casting and dispel checks | The original has these rules, but in code (a spell's legality in `Spells*.c`); ours is data. Not an invention | Keep |
| 14 | **A battle limit of 100 turns** (the original: 50) | `battle/mod.rs` `TURN_LIMIT` | long battles | `divergences.md` section 29 | Pointer only: a different number, not a missing rule |
| 15 | **Overland diagonal cost** as a setting (a diagonal step costs half as much again, and so does sight along a diagonal) | `BasicSettings.diagonal_extra_cost` (`f4x-def/src/settings.rs`), `unit/movement.rs` | the options step; off by default | The original charges a diagonal like a straight step; the doc comment of the field says so | Keep: an option, off by default, so the original's rule is the default |
| 16 | **Melee against fliers** as a setting (nothing, half strength, full strength) | `RuleSettings.melee_vs_fliers`, `MeleeVsFliers` | battles with a flier | The original's rule is "nothing" (divergence 26 in `divergences.md`); the other two are ours | Keep: the original's rule is the default; already written down |
| 17 | **Most units on one square** from 9 up to a ceiling (a reserve above nine on a defended square) | `RuleSettings.max_tile`, `MAX_TILE_LIMIT` | stacks, battles (a battle side stays at nine) | The original has the limit of nine and no reserve | Keep: default nine; see `game-settings.md` 4.3 |
| 18 | **Settings the original has no switch for**: Spell of Mastery on or off, Spell of Return off, human only or all, random events switch and frequency, Hardcore, No Myrror, scoring percent, banned races per level, raider and monster switches, the start reveal radius | `GameSettings` and its parts (`f4x-def/src/settings.rs`), `game-settings.md` | new game, options | Not in the original's options (only difficulty, opponents, land size and magic power are) | Keep: decided with task 247; every extra defaults to the original's behaviour and never changes the difficulty dropdown (`game-settings.md` 3.2) |
| 19 | **A battle in the save** (a game saved and loaded in the middle of a fight) | `battle/save.rs` | the save and load of the client and console | The original cannot save during a battle | Keep: a convenience, no hidden information |
| 20 | **The list of legal battle commands** | `battle/legal.rs` | the console's battle queries and the learning environment (`learning-ai.md` 4) | Tooling, not a rule; a test checks it against `Battle::apply` | Keep |
| 21 | **Rivers on the battlefield** | `battle/map/layout.rs` (task 255), `combat.md` | battles on plains with a river on the overland square | The original has `Carve_River_Terrain` and never calls it | Keep: the user's choice (task 255); already written down in `combat.md` |
| 22 | **Our noise generator's rivers and lakes**, also run over the original generator's land | `map/mapgen.rs`, `map/mapgen/original.rs` | new game | The original's generator is only the land; rivers and lakes are ours | Keep: part of row 9 |
| 23 | **Five `Effect` variants no shipped content uses**: `RemoveAbility`, `AddResource`, `ChangeTerrain`, `SetFeature`, `ApplyEnchantment` | `rules/effects.rs`, `rules/exec.rs`, the checks in `f4x-def/src/load/rules.rs`, tests in `rules/exec/tests.rs` (hand-built rules) | nothing in `crates/f4x-def/content/` (checked by a search for the name); only the executor's own tests | Not a mechanic of the original; the terrain, feature and enchantment spells all run as `Custom` hooks | **Removed** with task 490 (7 November 2026): no spell or ability of the original needs them |
| 24 | **Triggers never fired and conditions never used**: triggers `OnTurnStart`, `OnAttack`, `OnDefend`, `OnDeath`, `OnBuilt` (no code outside `effects.rs` names them; content uses `OnCast` only); conditions `HasAbility`, `TerrainIs`, `OnPlane`, `RaceIs`, `InBattle`, `InCity`, `All`, `Any` (evaluated in code, but no shipped rule uses them); targets `UnitsInTile`, `UnitsInArea`, `City` (content uses none) | `rules/effects.rs`, the evaluators in `rules/` | nothing shipped | Not a mechanic of the original; a generic vocabulary made ahead of need | **Removed**: the five triggers with task 491 and the eight conditions and three targets with task 492 (7 November 2026). Added back with the rule that needs one |

## What was found and is **not** ours

These looked invented and are in the original, so they stay without a note: the Nomad's trade bonus
(`RaceDef.trade_percent`, `CITYCALC.c` and the Nomad test there), Nightshade (`city/nightshade.rs`,
`CITYCALC.c`), the random events and their table (`world_event/`, `EVENTS.c`), the raiders and
roaming monsters (`threat/`, `Make_Raiders` and `Make_Monsters` in `AIDATA.c`), the Surveyor
(`map/survey.rs`, `Surveyor.c`), the offers (`player/offer.rs`, `Determine_Offer`), the Astrologer and the
history screens (`wizard/astrology.rs`, `Record_History`), the wizards' personality and objective
(`wizard/objective.rs`), and the 99 `Custom` and `Special` spell effects.

## Checked at the end of task 409

- The five fields whose counterpart was open have one in the original (ReMoM): `UnitDef.requires` is
  `reqd_bldg_1` and `reqd_bldg_2` of `s_UNIT_TYPE`; `BuildingDef.shape` is `bldg_data_table[].shape`
  (2x2, 2x3, 3x2, 3x3, `CITYSCAP.c`); `RulesConfig.racial_unrest` is `TBL_Unrest` (`CITYCALC.c`);
  `ItemDef.charges` are the spell charges of an item (`ItemMake.c`); `ItemDef.flag` is the original's
  flag byte (its doc comment says so). `BuildingDef.coastal` (Shipyard and the like) was not traced to a
  line of ReMoM's code.
- The functions of `battle/attack.rs` and `battle/spell.rs` map to the original's (`process_attack`, raising
  and animating the dead, Warp Creature, Call Chaos, the battle dispel); the one rule that differs, the
  refusal of stacking pairs in battle, is in `divergences.md`. The test helper `legal_battle_casts` is
  tooling (row 20). No new row.
