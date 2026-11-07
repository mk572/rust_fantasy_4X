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
| `Custom(name: "x", args: [..])` | Call a named Rust hook. The name is a plain tag in the `.ron` file; `args` (optional) tell the spells that share a hook apart, e.g. `Custom(name: "wall", args: ["fire"])`. `f4x_core::hook::CUSTOM_HOOKS` lists the hooks the code implements and the argument each takes, and `hook::problems` checks the content against it. A hook may also take one trailing number, e.g. `Custom(name: "counter_effect", args: ["dispel", "10"])`. There is no spell-number dispatch: the engine asks a spell for its hook and arguments. |

Damage kinds are `Physical`, `Fire`, `Cold`, `Lightning`, `Poison`, `Death`
and `Holy`. `DamageTraits` supports `armor_piercing`, `ignores_defense`, `area`,
`decaying` and `mana_per_point`. Slay effects can apply to `Any`, `Fantastic` or
`ChaosOrDeath` targets. `Custom(name: "unimplemented")` marks imported rules for which the
engine still lacks a native primitive.

### How effect timing actually works

`EffectRule.when` is a field in the content format, but it is **not** a subscription to a
general game-event system. The game does not broadcast events to every rule and automatically
run matching effects. Instead, each subsystem explicitly reads the rules it knows how to
interpret, and the turn itself is an ordered sequence of direct method calls. The authoritative
turn sequence is documented in [game-flow.md](game-flow.md) and implemented by
`game/mod.rs::end_turn` and `turn.rs::process_turn`.

The complete `Trigger` enum currently contains:

| Trigger | What the code currently does |
|---|---|
| `Passive` | Used for effects that are continuously in force. `stats::passive_modifiers` extracts unconditional `Modify` rules for races, abilities, buildings, spells and level paths. Other readers explicitly interpret passive rules: unit and city systems evaluate conditional modifiers; unit ability lookup reads passive `GrantAbility` rules from lasting spells and items. This is not one universal execution path. |
| `OnCast` | Used when a spell is cast. Overland spell resolution calls `run_rules(..., Trigger::OnCast, ...)` in `spell/cast.rs`. Battle spell handling also recognises OnCast rules and translates supported shapes into battle spell behaviour in `battle/spell.rs`; it is not simply the same overland executor. |
| `OnTurnStart` | Declared in the enum, but no caller dispatches it. |
| `OnAttack` | Declared in the enum, but no caller dispatches it. An executor unit test proves the generic executor can be asked to run it; it does not mean the game sends that trigger. |
| `OnDefend` | Declared in the enum, but no caller dispatches it. |
| `OnDeath` | Declared in the enum, but no caller dispatches it. |
| `OnBuilt` | Declared in the enum, but no caller dispatches it. |

`rules::exec::run_rules` is a small interpreter: a caller supplies one specific
`Trigger`, a `Context` and an `EffectHost`; it filters rules by that trigger, resolves their
targets, evaluates their conditions and applies the effect primitives. It does not schedule
itself, listen for events, or decide when to run. In current production code, the generic
executor's game call site is the overland spell-cast path. Other systems read the same rule
data directly where they need specialised behaviour.

#### What happens at the start of a turn?

The start-of-turn event is currently a notification, not an effect trigger:

1. `Game::end_turn` performs its pre-turn cleanup, increments `Game.turn`, processes stasis,
   refills unit movement and emits `Event::TurnStarted { turn }`.
2. It then calls `process_turn`, which directly calls the turn-step methods in a fixed order.
   Among those are treasury settlement, random events, city production and growth, timed-effect
   countdown, global and world enchantments, experience, wizard magic, healing, threats and
   offers.
3. After `process_turn`, `end_turn` records history, processes diplomacy, handles purifying,
   stranded ships and road work, then follows unit standing orders.

The precise order is maintained in [game-flow.md](game-flow.md). Nothing in this sequence
looks for `Trigger::OnTurnStart`. Likewise, `Event::TurnStarted` is returned to the caller
alongside other outcome/notification events; it is not sent back into the rules executor.
More generally, `Game::apply` returns `Event` values for the client, log and other callers.
They are not an internal event bus that automatically invokes `EffectRule`s.

No scripting language is used: rules are serialisable data. `Custom` and `Special` are
explicit calls into registered Rust hooks / spell-specific Rust code, not event listeners.
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
Reusable, named bundles of abilities and passive effects: flying, poison touch, regeneration, immunities, and stat bonuses. An `OnAttack` example would be misleading: the game currently has no generic attack-trigger dispatch.

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
Tunable numbers that are not content live in one file, `crates/f4x-def/content/rules.ron`: to-hit and damage formulas, growth rates, unrest thresholds, minimum city distance, spell casting limits. One place to balance from.

## 5. Systems built on the framework

- **City economy**: `city_outputs(city)` aggregates terrain around the city, buildings, enchantments and race into food, production, gold, mana and research, then the turn pipeline applies growth, production and unrest. All through stats and effects.
- **Unit production and upkeep**: production queue entries reference `UnitId`/`BuildingId`; upkeep is a stat summed per player.
- **Spellcasting and research**: wizard knows spells, researches from a pool, has mana and a casting skill; the cast command validates context, target and cost, then runs effects. Overland and battle share the same spell defs.
- **Combat resolution**: attack and defence behaviour is handled by the battle subsystem's explicit combat code. The `Trigger` enum is not a general combat-event callback system; battle spell definitions are interpreted by the battle spell code, while passive abilities and modifiers are read by the unit/battle views.
- **Experience and levelling**: level paths are data (thresholds plus modifiers per level).

## 6. Where it lives

```
f4x-core/src/rules/      defs.rs      definition types + typed ids
                         registry.rs  loaded definition tables
                         stats.rs     Stat enum, Modifier, compute()
                         effects.rs   Condition, Target, Effect, execute()
f4x-core/src/systems/    economy.rs, spells.rs, combat.rs, city/production.rs
f4x-def/src/load/        RON loading, cross-reference validation
crates/f4x-def/content/  races.ron, units.ron, buildings.ron, abilities.ron,
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
