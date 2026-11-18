# Constants in code: the review (task 483)

Every `const` and `static` of the crates, with the decision: **stay** (a technical limit or an invariant
of the engine, or tuning of code that is not a game rule), **content** (a rule of the game that a mod
might change: it moves into a `.ron` file, where the hash of `content.sha256` guards it), or
**setting** (a choice of the player or the difficulty). The list was made with
`python3` over the `const NAME: Type = value;` items outside the test code (about 380 in `f4x-def`,
`f4x-core` and `f4x-newgame`, about 430 in the AI, learning and tool crates).

A constant that moves keeps its test only if the test checks behaviour; a test that only pins the
number is deleted (the hash replaces it). The moves are the tasks named in the last column.

## Stay in code

| Where | What | Why |
|---|---|---|
| `f4x-core/battle/map.rs`, `map/layout.rs` | grid size (`GRID_W`, `GRID_H`, `W`, `H`), terrain codes, deck codes, step tables, `STRUCTURE`, city box, gate, `INF`, `SAIL_COST` | the shape of the battlefield and the encoding of its map, not a tunable rule |
| `path_grid.rs`, `unit/pathing/*`, `map/fog.rs` | `BLOCKED`, `NONE`, `AROUND`, fog bit masks and corner tables | engine encoding |
| `hook.rs`, `battle/field.rs`, `spell/*` (`*_EFFECT`, `RECALL`, `GATE`, ...), `item/crafting.rs` `CRAFT_ITEM` | names of hooks and effect kinds | the keys that tie a `.ron` row to the code that handles it |
| `game/save.rs`, `game/undo.rs`, `game/replay.rs`, `unit/kept.rs`, `unit/crowding.rs`, `finance.rs`, `vision.rs` | save versions, undo depth, thread-local caches, `OnceLock` switches | engine internals |
| `player/mod.rs` `MONSTERS`, `race_player.rs` `FIRST_RACE_PLAYER`, `game/cheat.rs`, `settings.rs` `HUMAN`/`RIVAL` | special player ids | identifiers, not rules |
| `city/mod.rs` `NAME_LEN`, `player/mod.rs` `PORTRAITS`, `BANNER_CHOICES`, `NEUTRAL_BANNER` | limits of the interface and of the art | follow the art that ships |
| `f4x-def/settings.rs` `MAP_*`, `MAX_TILE_LIMIT`, `MAX_STACK`, `PICKS`, `MAX_OPPONENTS`, `LEVELS`, `*_RANGE`, `DEFAULT_*`, `LAND_*`, `*_PER_*`, `COMMON`..`VERY_RARE` (the new-game defaults), `REFERENCE_AREA` | the defaults and bounds of the settings the player chooses in the new-game dialog; they are the setting, in its home | already settings |
| `f4x-def/settings.rs` `TRIREME_*`, `GALLEY_*`, `MAX_SHIP_CAPACITY`, `MONSTER_START_*`, `DEFAULT_FARMER_FOOD`, `DEFAULT_WORKER_PRODUCTION`, `FARMER_WORKER_MOST` | the defaults and bounds of rule settings the player can change (`trireme_capacity`, `monster_start_turn`, `farmer_food`, ...); the unit numbers of the two ships are the one thing that is not (the code names the units whose capacity a setting changes) | already settings |
| `f4x-def/{half_int,registry,content_hash,lib,testdir,text}.rs` | `ZERO`, `MAX`, `NO_ROW`, `RECORD`, paths, fallback language | technical |
| `f4x-newgame/{mapgen,island,place,neutral,setup/original}.rs` | the map generator's shares, belts, spacings, clearances, tries | tuning of the generator (the player-facing sizes are settings) |
| `f4x-newgame/computer_wizard.rs`, `temperament.rs` | the weights that make a computer wizard | the AI's character, not a rule; stays with the AI |
| `f4x-ai`, `f4x-learn`, `f4x-tools`, `f4x-console` (all) | scores, weights, horizons, limits, usage strings | tuning of the AI and tools; design.md section 8 forbids rules the AI has and a human lacks, and these are not rules |
| `battle/power.rs` `ABILITY_BONUS`, `city/autobuild.rs` weights (`PRIMARY_WEIGHT`, `SHARPNESS`, `ROLL_CUTOFF`, ..., `AFFINITY`) | strength estimates and the Grand Vizier's taste | an estimate and a taste, not a rule |
| Test-only constants (`P0`, `SEEDS`, `TRIES` of the tests, `SITE`) | scenario | tests |

## Move into content

Each group is one task: the numbers become fields of the row they belong to (an ability, a spell, a unit,
a building, an item kind) or of `rules.ron`, the code reads them, the test that pins them is replaced by
one that reads them.

| Group | Constants | Task |
|---|---|---|
| Battle rules | `IMMOLATION_STRENGTH`, `LARGE_SHIELD`, `INVULNERABILITY_BLOCK`, `IMMUNE_DEFENSE`, `MANA_LEAK_DRAIN`, `CALL_LIGHTNING_STRENGTH`, `WALL_OF_FIRE_STRENGTH`, `NODE_COUNTER_STRENGTH`, `NODE_AURA_BONUS`, `SHOT_MANA`, `WEB_HITS`, `TURN_LIMIT`, the move costs (`JUMP_COST`, `DIAGONAL_EXTRA`, `OPEN_COST`, `ROUGH_COST`, `MUD_COST`), the vortex numbers, `FREEING_MODIFIER` | 675 |
| Experience, fame and spoils | `XP_PER_KILL`, `FAME_XP`, `RARE_FOE_COST`, `FAME_KILLS` (the four-dead-units rule), `JUST_CAUSE_FAME`, `FORTRESS_PRIZE_FAME`, `HEROISM_LEVEL`, `CHARMED_RESISTANCE`, the tiers of `hero.rs` | 676 (done: `renown` of `rules.ron`; the hero ability numbers of `hero.rs` are an encoding and stay) |
| Spell effects | `spell/city.rs` (chaos rift, wards, consecration, religious buildings, walls), `dispel.rs`, `terraform.rs`, `wizard.rs`, `world.rs`, `ARMAGEDDON_*`, `BLACK_WIND_MODIFIER`, `UNSUMMONING_MODIFIER`, `WEREWOLVES`, the corruption fade, `nightshade.rs` | 677 (done: `spell_effects` of `rules.ron`; `COUNTER_MAGIC_DRAIN` goes with 675, `JUST_CAUSE_FAME` with 676) |
| Abilities known by number | `mobility.rs` (the ability numbers and multipliers), `experience.rs` (`ARMSMASTER`), `mutation.rs`, `undead.rs` (`UNDEAD_IMMUNITIES`, `DEATH`), `hero/kind.rs` `MAGIC_RANGED`, `WEAPON_*`, `HEROISM`, `SPELL_CHARGES`, `monster.rs` `LIFE` | 678 (done: `abilities`, `targets` and `threat.spared_realms` of `rules.ron`, `holds_spell` of `item_powers.ron`; `WEAPON_*` is the encoding of the quality and stays) |
| Items | `base_cost` of the kinds, the Chain and Plate mail defence bonus, `MAX_POWERS`, `MAX_CHARGES`, `FREE_GROUP`, `VAULT_SLOTS`, `MAX_AWARDS`, `TRANSFER_MANA`, `item/make.rs` | 679 (done: `item` of `rules.ron`; `VAULT_SLOTS` is the art's four slots and stays in the item screen, `MAX_AWARDS` was unused and is gone) |
| Wizard | the hero mana formula, the five-fold cost of a spell away from a battle, the ten-percent book steps, `MASTERY_DISCOUNT*`, the starting figures (`CAPITAL_*`, `START_TAX_RATE`, army, gold), `RESIGN_BELOW_SKILL`, `MAX_RETORTS`, `RESEARCH_SLOTS` | 680 (done: `wizard` of `rules.ron`; `ALCHEMY_RETORT` and `MYRRAN_RETORT` name retorts by number and go with task 678) |
| Economy, diplomacy and threats | trade percentages, relation steps, `SUBVERSION_PENALTY`, `MAX_ROUNDS`, mercenary offers, outpost growth, `garrison_minimum`, `RAIDER_LIMIT`, `RAMPAGE_DAMAGE`, `CHEAPEST`, `BIG_EVENTS_FROM_TURN`, `ORES`, `STARTING_BUILDINGS`, `LIMITED_AI_RACES` | 681 (done: `trade`, `diplomacy`, `offer`, `outpost`, `threat`, `garrison_minimum`, `starting_buildings`, `limited_ai_races`, `event_ores` of `rules.ron`) |

`wizard/mod.rs` `MAX_BOOKS` and `BOOKS_BEFORE_LIMIT` already come from the settings defaults.
