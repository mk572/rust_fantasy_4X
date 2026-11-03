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

A stat is a numeric attribute with a fixed meaning in the engine. The authoritative list is
`f4x-core/src/rules/stats.rs` (`Stat`); this section is kept in sync with that enum.
Content may use these stats but cannot define new ones.

### Complete stat list

| Group | Stat | Meaning |
|---|---|---|
| Units and heroes | `Melee` | Melee attack strength. |
| | `Ranged` | Ranged attack strength. |
| | `Defense` | Defensive strength. |
| | `Resistance` | Resistance used for spell/effect checks. |
| | `MaxHp` | Maximum hit points. |
| | `Figures` | Number of figures in a unit. |
| | `Moves` | Movement allowance in whole moves. |
| | `HalfMoves` | Additional half-moves; items express movement bonuses in half-moves. |
| | `Sight` | Unit sight range. |
| | `ToHit` | General to-hit bonus; each point is ten percentage points and may be negative. |
| | `MeleeToHit` | To-hit bonus applied only to melee attacks. |
| | `RangedToHit` | To-hit bonus applied only to ranged attacks. |
| | `Ammo` | Ammunition allowance. |
| | `SpecialLevel` | Strength/level attached to a special attack or ability (e.g. poison, gaze, life steal, Holy Bonus); zero means absent. |
| | `Construction` | Road-building strength; stack strength determines turns needed to build a road square. |
| | `Transport` | Number of land units a ship carries. |
| Wizards | `ManaIncome` | Mana gained per turn. |
| | `ResearchIncome` | Research gained per turn. |
| | `CastingSkill` | Spell-casting skill. |
| | `SpellCostPercent` | Percentage change to spell mana costs. |
| Cities | `CityFood` | Food output before the farmland cap. |
| | `CityFoodFlat` | Food added after the farmland cap (e.g. granaries and markets). |
| | `FarmerFood` | Extra food produced by each farmer. |
| | `CityUnrestPercent` | Percentage change to unrest from sources such as famine. |
| | `StartingXp` | Experience points granted to newly produced units. |
| | `CityProduction` | Production output. |
| | `CityGold` | Gold output. |
| | `CityMana` | Mana output. |
| | `CityResearch` | Research output. |
| | `CityGrowthPercent` | Percentage change to city growth. |
| | `CityGrowth` | People gained per turn (growth is represented in people, not citizens). |
| | `CityUnrest` | Unrest points; negative values reduce unrest. |
| | `UnitCostPercent` | Percentage change to unit production cost in the city. |
| | `CitySight` | City sight range (base 2; some effects raise its minimum). |

The enum currently has 34 variants. Values that can legitimately be negative are
`ToHit`, `MeleeToHit`, `RangedToHit`, `CityUnrest`, `CityGrowth`,
`CityUnrestPercent`, `SpellCostPercent` and `UnitCostPercent`. Other stats have a
floor of zero. These are engine-level rules in `Stat::floor()`.

### Modifier calculation

A modifier is `{ stat, op, value, source }`, where `op` is one of:

- `Add`: sum with the base value.
- `Percent`: percentages are summed, then applied to base plus additions (`+50` means ×1.5).
- `Min`: the result cannot be below this value.
- `Max`: the result cannot exceed this value.
- `Set`: force this value; if several Set modifiers apply, the lowest wins.

The calculation order is: base plus all additions; apply the summed percentage (integer
division rounds down, including for negative results); apply Min/Max bounds; apply Set;
then apply the stat's zero floor where it has one. `Moves` is a deliberate exception:
its `Min` modifier raises the base movement allowance before additions, so a flying unit
with an additional movement bonus keeps both.

Modifier sources are tagged as `Race`, `Ability`, `Building`, `Spell`, `Item`
(slot 0–2), `Level` (experience level, zero-based), `Hero` (hero attributes), or
`Other`. This allows contributions to be explained and a source's modifiers to be
removed together. Modifiers and calculations use integers.

Stats are computed by the owning systems from a base value and applicable modifiers;
there is no single general-purpose `stats::compute(world, entity, stat)` entry point.
The shared calculation primitive is `rules::stats::compute(stat, base, modifiers)`.

## 3. Conditions, targets and effects

Abilities, spells, buildings and items all describe what they do with the same three building blocks.

**Conditions** are evaluated against facts about the selected subject. The complete vocabulary in
`rules/effects.rs` is:

| Condition | Meaning |
|---|---|
| `Always` | Always passes. |
| `HasAbility(id)` | Subject has the named ability. |
| `TerrainIs(class)` | Subject is on the named terrain class. |
| `OnPlane(plane)` | Subject is on the named plane. |
| `RaceIs(id)` | Subject has the named race. |
| `RealmIs(id)` | Subject has the named realm. |
| `KindIs(kind)` | Subject is the named unit kind (normal, hero, creature, etc.). |
| `InBattle` | Subject is in a battle. |
| `InCity` | Subject is in a city. |
| `All([..])` | Every nested condition passes. |
| `Any([..])` | At least one nested condition passes. |
| `Not(..)` | The nested condition does not pass. |

**Target selectors** choose the subjects to which an effect is applied:

| Selector | Meaning |
|---|---|
| `This` | The entity/source represented by the rule context. |
| `Owner` | The player who owns the source. |
| `AllOwnUnits` | Every unit owned by the source owner. |
| `AllEnemyUnits` | Every unit not owned by the source owner. |
| `UnitsInTile` | Every unit on the source's tile. |
| `UnitsInArea(radius)` | Every unit within Chebyshev distance radius on the same plane. |
| `City` | The city represented by the source, or the city on its tile. |
| `Tile` | The source's map tile. |
| `Chosen` | The target selected by the caster. |
| `UnitsOnChosenSquare` | Every unit on the chosen target's tile, regardless of owner; includes a city's garrison. |

Selectors resolve in a stable order. A selector that needs a position yields no targets if the
source has no position; `Chosen` yields none if no target was selected.

**Effects** are the complete primitive vocabulary in `rules/effects.rs`:

| Effect | Meaning |
|---|---|
| `Modify { stat, op, value }` | Add a stat modifier tagged with the rule's source. |
| `GrantAbility(id)` / `RemoveAbility(id)` | Add or remove an ability. |
| `Damage { ... }` | Deal damage of a `DamageKind`, optionally resistible, with explicit immunity abilities, realm, resistance modifier and battle damage traits. |
| `Heal(amount)` | Restore hit points. |
| `Slay { ... }` | Battle-only destruction effect with realm, resistance modifier, immunity list and optional restrictions/thresholds. |
| `AddResource { gold, food, mana, research }` | Add the specified resources to the target. |
| `Summon { unit, count }` | Summon a number of units at the target's tile. |
| `ChangeTerrain(class)` | Change the target tile's terrain class. |
| `SetFeature(id)` | Put a map feature on the target tile. |
| `ApplyEnchantment(id)` | Attach a lasting spell to the target. |
| `Dispel { strength }` | Remove eligible enchantments up to the given strength. |
| `Custom(name)` | Call a named Rust hook; the name must be in `CUSTOM_HOOKS`. |
| `Special` | Call the Rust implementation for the spell number; the number must be in `SPECIAL_SPELLS`. |

Damage kinds are `Physical`, `Fire`, `Cold`, `Lightning`, `Poison`, `Death`
and `Holy`. `DamageTraits` supports `armor_piercing`, `ignores_defense`, `area`,
`decaying` and `mana_per_point`. Slay effects can apply to `Any`, `Fantastic` or
`ChaosOrDeath` targets. `Custom("unimplemented")` marks imported rules for which the
engine still lacks a native primitive.

### Trigger vocabulary and dispatch status

The complete `Trigger` enum currently contains:

| Trigger | Intended meaning | Current dispatch |
|---|---|---|
| `Passive` | Continuously applies while its source exists. | Supported for passive stat modifiers and passive ability grants; also read by spell/enchantment systems. |
| `OnTurnStart` | Fires at the start of a turn. | Declared, but no game-system dispatch found in the current code. |
| `OnCast` | Fires when the spell is cast. | Dispatched by spell casting (`spell/cast.rs`). |
| `OnAttack` | Fires when the subject attacks. | Declared; currently covered by executor tests, but no game-system dispatch found. |
| `OnDefend` | Fires when the subject defends. | Declared, but no game-system dispatch found. |
| `OnDeath` | Fires when the subject dies. | Declared, but no game-system dispatch found. |
| `OnBuilt` | Fires when the subject is built/produced. | Declared, but no game-system dispatch found. |

This distinction matters: a trigger being representable in content does not mean the game
currently emits that event. The executor can run any trigger when a caller supplies it, but
only the dispatch points listed above are present in the current game code. Add a dispatch
point before relying on an otherwise-unused trigger in content. Passive rules are not all
handled by the generic executor: some systems collect or interpret them directly.

No scripting language is used: rules are plain serialisable data. `Custom` and `Special`
are deliberate Rust escape hatches, validated against the registries in `effects.rs`.

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
