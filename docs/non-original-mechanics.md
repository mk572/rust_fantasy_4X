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
  `Custom(..)` and `Special` effects of `content/spells.ron` (99; all are spells of the original).
- A claim "the original has no such thing" was checked in ReMoM (`~/projects/ReMoM/MoM/src`) by a search
  for the original's own name of the thing; where the search found nothing but I did not read the whole
  code around it, the row says so.
- **Not covered:** the inside of `battle/` (attack, immunity, special, spell), `unit/movement`, `map/mapgen`,
  the `Effect` variants of `rules/effects.rs`, the settings of `game/setting.rs`, and the crates
  `f4x-ai`, `f4x-ai-ref`, `f4x-learn`, `f4x-tools`, `f4x-console` (they are our own by design: the original's
  AI is not a port; see `design.md` section 8). Task 409 walks the core's tests and will meet more of these;
  add rows here when it does.

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

## What was found and is **not** ours

These looked invented and are in the original, so they stay without a note: the Nomad's trade bonus
(`RaceDef.trade_percent`, `CITYCALC.c` and the Nomad test there), Nightshade (`city/nightshade.rs`,
`CITYCALC.c`), the random events and their table (`world_event/`, `EVENTS.c`), the raiders and
roaming monsters (`threat/`, `Make_Raiders` and `Make_Monsters` in `AIDATA.c`), the Surveyor
(`map/survey.rs`, `Surveyor.c`), the offers (`player/offer.rs`, `Determine_Offer`), the Astrologer and the
history screens (`wizard/astrology.rs`, `Record_History`), the wizards' personality and objective
(`wizard/objective.rs`), and the 99 `Custom` and `Special` spell effects.

## Left for the next pass

- The `Effect` variants of `rules/effects.rs` against what the 214 spells, the abilities and the buildings
  actually use: a variant no row of `content/` names is dead code (`scripts/untested_content.py` does not
  list effects).
- `UnitDef.requires`, `BuildingDef.shape` and `coastal`, `ItemDef.flag` and `charges`, `RulesConfig.racial_unrest`:
  the shape of the field was read, the original's counterpart was not.
- The settings of `game/setting.rs` one by one: which have a counterpart in `Settings.c` and which are ours.
