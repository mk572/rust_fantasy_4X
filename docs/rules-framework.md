# Game rules framework

Status: build steps 1-3 are implemented (registries, stats, conditions/targets/effects executor in `rules::exec`); steps 4-8 are not. It builds on [`design.md`](design.md) (overall architecture) and replaces its loose notes on spells, abilities and units with a concrete framework.

The aim is a rules layer that is:
- **Data-driven**: races, units, buildings, spells and abilities are content files, not code.
- **Composable**: a small set of effect primitives builds hundreds of spells and abilities, so new content rarely needs Rust.
- **Deterministic and testable**: integer math, seeded RNG, no hidden state; every rule runs headless.
- **Not tied to Master of Magic's numbers**: MoM is the starting point for the *shape* of the game, the rules and values are ours to change.

## 1. Definitions vs. instances

| | Definitions (static) | Instances (runtime) |
|---|---|---|
| Where | Content files loaded at startup into registries | ECS components in the world |
| Mutable | No | Yes |
| Examples | `RaceDef`, `UnitDef`, `BuildingDef`, `SpellDef`, `AbilityDef`, `RealmDef`, `ItemDef` | `Unit`, `City`, `Wizard`, `Enchantment` entities |

Definitions refer to each other by typed ids (`UnitId`, `SpellId`, ...) that index into the registries, the same pattern as `TerrainId` and `FeatureId` today. Instances hold a def id plus their own state (damage taken, experience, remaining duration). Loading validates every cross-reference once, so the simulation never has to handle a dangling id.

## 2. Stats and modifiers

Everything numeric about a unit, city or wizard is a **stat**, computed rather than stored:

```
final = clamp( (base + sum(adds)) * (100 + sum(percents)) / 100 , min, max )   // then any `Set` overrides
```

- **Stats are a closed Rust enum** (`Stat::Melee`, `Ranged`, `Defense`, `Resistance`, `MaxHp`, `Figures`, `Moves`, `Sight`, `Upkeep`, `CityFood`, `CityProduction`, ...). The engine knows what each means. Content cannot invent new stats, which keeps rules checkable. Adding a stat is a small Rust change.
- **Modifiers** are `{ stat, op: Add | Percent | Set | Min | Max, value: i32, source, condition? }`. All integer, applied in a fixed order, so results are deterministic and order of casting does not matter.
- **Sources** are tracked (race, ability, enchantment, item, building, terrain, experience level), so the UI can show "+2 from Iron Armor" and so removing a source removes exactly its modifiers.
- Derived values are computed on demand by `stats::compute(world, entity, stat)`. Nothing caches them until profiling says it matters.

## 3. Conditions, targets and effects

Abilities, spells, buildings and items all describe what they do with the same three building blocks.

**Condition** (a small enum, evaluated against a context):
`Always`, `HasAbility(id)`, `TerrainIs(class)`, `OnPlane(plane)`, `RaceIs(id)`, `RealmIs(id)`, `InBattle`, `InCity`, `All([..])`, `Any([..])`, `Not(..)`.

**Target selector**: `This`, `Owner`, `AllOwnUnits`, `AllEnemyUnits`, `UnitsInTile`, `UnitsInArea(radius)`, `City`, `Tile`, `Chosen` (the target picked when casting), `UnitsOnChosenSquare` (every unit on the square of the chosen target, whoever owns it and a city's garrison too: Ice Storm and Fire Storm; such a spell needs an enemy unit there, `Game::is_square_spell`). A `Damage` effect with `traits: (area: true)` strikes every figure of the unit with the dice of a battle (`EffectHost::strike`); without it the damage is flat.

**Effect** (the primitives; each is a Rust function with tests):

| Effect | Does |
|---|---|
| `Modify { stat, op, value }` | adds a modifier while the source exists |
| `GrantAbility(id)` / `RemoveAbility(id)` | |
| `Damage { amount, kind, resist }` | with a resistance/save check |
| `Heal(amount)` | |
| `AddResource { gold, food, mana, research }` | per turn or once |
| `Summon { unit, count }` | |
| `ChangeTerrain(class)` / `SetFeature(id)` | |
| `ApplyEnchantment(id)` / `Dispel { strength }` | |
| `Custom(name)` | named hook implemented in Rust, for the rare effect no primitive covers |

**Triggers** say when an effect fires: `Passive` (modifiers while present), `OnTurnStart`, `OnCast`, `OnAttack`, `OnDefend`, `OnDeath`, `OnBuilt`. No scripting language: effects are plain data, which keeps content validatable, serialisable in saves, and safe to mod. `Custom` is the deliberate escape hatch, and we count how often it is used as a signal that a primitive is missing.

## 4. The definitions

Sketches in RON; field lists are indicative, not final.

### Race
Determines which units and buildings a city can produce and its baseline economy.
```
(id: "highborn", name: "Highborn",
 units: ["spearman", "archer", ...], buildings: ["barracks", ...],
 modifiers: [ (stat: CityFood, op: Add, value: 1) ],
 terrain_affinity: [ (Grassland, 100), (Desert, 40) ])
```

### Unit
```
(id: "spearman", name: "Spearman", race: Some("highborn"), kind: Normal,   // Normal | Hero | Summoned | Fantastic
 cost: 40, upkeep: 1,
 stats: { Melee: 3, Defense: 3, Resistance: 4, MaxHp: 1, Figures: 6, Moves: 2, Sight: 1 },
 movement: Walking,                 // Walking | Flying | Swimming | ... (interacts with terrain `walkable`/`move_cost`)
 abilities: ["first_strike"], requires: ["barracks"],
 level_path: "standard_xp")
```
Runtime: `Unit { def }`, `Owner`, `Position`, `Damage`, `Experience`, plus enchantments and equipment attached as modifiers.

### Ability
Reusable, named bundles of effects: flying, first strike, poison touch, regeneration, immunities, and so on.
```
(id: "first_strike", name: "First Strike", effects: [ (when: OnAttack, ...) ])
```

### Building
```
(id: "barracks", name: "Barracks", cost: 100, upkeep: 1, requires: ["city_hall"],
 races: [], effects: [ (when: Passive, target: City, do: Modify(UnitCostPercent, -10)) ])
```

### Realm (magic school)
Data, not hard-coded, so the number and names of realms can change: `(id: "nature", name: "Nature", colour: ..)`. Spells, nodes, wizards and some units carry a realm.

### Spell
```
(id: "fireball", name: "Fireball", realm: "chaos", rarity: Common,
 cost: 30, research_cost: 120, contexts: [Battle], target: EnemyUnit,
 duration: Instant, resist: Some(Resistance),
 effects: [ Damage(amount: 15, kind: Fire, resist: true) ])
```
Enchantments (lasting spells) are the same def with a duration and optional per-turn upkeep.

### Enchantment, Item
- **Enchantment** is a runtime entity `{ def, target, caster, remaining, upkeep }`. It is an entity, not a list on the target, so it can be queried, dispelled and cleaned up when the target dies.
- **Item** defs grant modifiers and abilities to the carrier, same machinery as buildings.

### Global rules config
Tunable numbers that are not content live in one file, `content/rules.ron`: to-hit and damage formulas, growth rates, unrest thresholds, minimum city distance, spell casting limits. One place to balance from.

## 5. Systems built on the framework

- **City economy**: `city_outputs(city)` aggregates terrain around the city, buildings, enchantments and race into food, production, gold, mana and research, then the turn pipeline applies growth, production and unrest. All through stats and effects.
- **Unit production and upkeep**: production queue entries reference `UnitId`/`BuildingId`; upkeep is a stat summed per player.
- **Spellcasting and research**: wizard knows spells, researches from a pool, has mana and a casting skill; the cast command validates context, target and cost, then runs effects. Overland and battle share the same spell defs.
- **Combat resolution**: attack and defence rolls are driven by stats and the config formulas; abilities and enchantments hook in through triggers. The tactical battle and auto-resolve call the same functions.
- **Experience and levelling**: level paths are data (thresholds plus modifiers per level).

## 6. Where it lives

```
f4x-core/src/rules/      defs.rs      definition types + typed ids
                         registry.rs  loaded definition tables
                         stats.rs     Stat enum, Modifier, compute()
                         effects.rs   Condition, Target, Effect, execute()
f4x-core/src/systems/    economy.rs, spells.rs, combat.rs, city/production.rs
f4x-data/                RON loading, cross-reference validation
content/                 races.ron, units.ron, buildings.ron, abilities.ron,
                         realms.ron, spells.ron, items.ron, rules.ron
```
`f4x-tools validate` checks the whole set: unknown ids, requirement cycles, spells with no effects, units unavailable to every race, and similar.

## 7. Build order for the framework

1. **Registries and typed ids** for all definition kinds, loading and cross-reference validation (empty-ish content, real schema).
2. **Stats and modifiers**: `Stat`, `Modifier`, `compute()`, with unit tests for ordering, clamping and source removal.
3. **Conditions, targets and effects**: the primitives and a small executor, tested with synthetic worlds.
4. **Races, units, abilities**: components, a first small original content set, and `FoundCity`-style commands to create and move units.
5. **Buildings and the city economy**: production, growth, upkeep through the turn pipeline.
6. **Realms, spells and enchantments**: casting, duration, upkeep, dispel; research and the wizard's mana.
7. **Combat resolution**: formulas, abilities in combat, auto-resolve; then hand off to the tactical battle screen.
8. **Content pass**: grow the sample content into a playable set, and balance with AI-vs-AI runs.

Each step ships with tests and validator checks before the next starts.

## 8. Decisions (confirmed)

1. **Number scale: as in Master of Magic.** Small numbers, units of roughly 1 to 10 per stat with a figure count per unit.
2. **Magic structure: as in Master of Magic.** Five realms, spell books built from picks, per-realm research. Realms stay data so this is easy to change later.
3. **Stats: fixed in code.** A closed `Stat` enum; content cannot invent stats.
4. **Heroes and items: from day one.** Hero units, item slots and item effects are part of the base schema and of steps 4 to 7 below, not a later layer.
5. **Deviation from MoM follows the priorities of `design.md` section 1** (play, logic, balance, the original's help text, then its code). Where those rank above the original's behaviour we depart; when in doubt, follow MoM. Each departure is recorded in `divergences.md`.

Consequence of 4 and 5: the sample content and the schema are shaped around MoM's mechanics, but with original names and values for the placeholder data.
