# How game state is stored (and how that differs from the original)

The original game keeps its world in fixed-size arrays of packed records and refers to
things by array index. This project keeps it as entities with components, typed ids into
definition tables, and values computed on demand. This document explains the layout of
`f4x-core`, why it differs, and the rules to follow when adding to it.

Facts about the original come from the ReMoM clone's headers (a faithful port of the
original code): `MoX/src/MOM_DAT.h` and `MOM_DEF.h`. Record sizes are for the original
16-bit layout.

## 1. The original's model in brief

| What | Original |
|---|---|
| Units | One array, 1009 slots of 32-byte records. A unit stores its type index, position, owner, level, experience, damage, remaining moves, and its enchantments and mutations as bit flags in 16/32-bit words. In the code a stack is not a stored object: it is whatever units share a square. |
| Cities | One array, 100 slots of 114-byte records. Buildings are a status byte per building (36 of them: unbuilt, replaced, built...), city enchantments are 26 bytes holding "0, or the casting player's index + 1", and the current product is one integer (below 100 a building, 100 and above a unit type). Computed yields (gold, food, mana, research, production, upkeep) are cached in the record and recalculated when something changes. |
| Players | A fixed array of 6 wizard records of 1224 bytes each: picks, known-spell table, global enchantments (24 flags), up to 6 heroes, mana, gold, research, tax rate. |
| Map (see `map-layers.md`) | Two planes of 60x40 terrain-type numbers (762 of them, including the shore and river variants that make autotiling work), plus separate per-square bitfields for specials, flags and exploration. |
| Sites | Fixed arrays: 102 lairs, 30 nodes, 6 towers, 138 magic items. |
| Combat | A separate temporary array of battle-unit records, copied from units at the start of a fight. |
| Content | Tables of records (units, buildings, spells, races), some in the executable's data segment and some in the `.lbx` files; code refers to them by index and by compiled-in constants (`bt_Barracks`, `spl_Fireball`). |

Consequences of that model that matter here: hard limits (a 101st city is impossible),
meaning hidden in index ranges and flag bits, cached values that can go stale, and
behaviour that depends on slot order.

## 2. This project's model

```
Game
├── world: hecs::World            entities: units, cities (later wizards' enchantments, heroes, battles)
├── map: Option<Map>              two dense grids (terrain, feature, road); not entities
├── fog: Fog                      what each player has explored
├── players: Vec<Player>          gold, mana, tax rate, capital race, fame
├── content: Content              definitions loaded from data: terrains, features, rules
│   └── rules: Rules              registries: races, units, abilities, buildings, realms, spells, items, level paths
├── turn: u32
└── rng: Pcg64Mcg                 the only source of randomness (Game::roll)
```

`Game::apply(Command) -> Vec<Event>` is the only way state changes. Commands name
entities by `hecs::Entity`; events report what happened. The client, the AI and tests all
use the same entry point.

### Three kinds of data

1. **Entities with components**: things that have identity and different attributes
   (units, cities). State that changes during play lives here.
2. **Definitions**: static content loaded once into registries (`Registry<UnitDef>` and so
   on). Never mutated at runtime. An entity points at its definition with a typed id.
3. **Resources owned by `Game`**: things that are numerous, dense or global (the map
   grids, fog, the player list, the RNG). They are plain fields, not entities.

The rule of thumb: if it has an identity that other things refer to and a varied set of
attributes, it is an entity; if it is addressed by coordinates or is one per player, it is
a field of `Game`.

## 3. Components today

### Unit entity
| Component | Meaning |
|---|---|
| `Serial(u64)` | Creation number from a counter in `Game`; never reused. The stable order of units and cities, and what a save records instead of the entity id. |
| `Unit(UnitId)` | Marker and definition: which `UnitDef` this is. Also what makes the entity a unit. |
| `Owner(PlayerId)` | Who controls it. `NEUTRAL` is the owner of monsters. |
| `Position` | Plane, x, y. Shared with cities. |
| `Hurt(u32)` | Hit points lost, over all figures. Current hit points are derived. |
| `Experience(u32)` | Experience points. The level is derived from the level path. |
| `MovesLeft(u32)` | Movement left this turn, in half-moves. Refilled at turn start. |
| `Order::GoTo(path)` | Optional. A standing order, removed when the path is finished or blocked. |

Not stored: attack, defence, resistance, hit points per figure, number of figures, moves
per turn, sight. All come from `Game::unit_stat(unit, stat)`: the definition's base value
plus modifiers from its abilities and its level.

### City entity
| Component | Meaning |
|---|---|
| `Serial(u64)` | Creation number, as for units. |
| `City { name, owner }` | Marker, name and owner. |
| `Position` | Where it stands. Cities are entities, not map tiles. |
| `Population { figures, tens }` | Citizens (1000 people each) and tens of people toward the next. Zero figures is an outpost. |
| `Farmers(u32)` | How many citizens farm. The rest work, minus rebels. |
| `Buildings(Vec<BuildingId>)` | What is built, in build order. "Replaced" is not stored: it follows from a building's `replaces` field. |
| `CityRace(RaceId)` | Race of the citizens; picks what the city can build and its traits. |
| `Enchantments(Vec<(SpellId, PlayerId)>)` | City enchantments with the caster. |
| `Production { item, progress }` | What is being built and how much is done. `item` is a typed `ProductionItem`, not an index range. |

Not stored: food, production, gold, mana, research, rebels, growth, maximum size. All come
from `Game::city_economy(city)`, which recomputes them from the land the city works, its
people, race, buildings and enchantments. Nothing is cached, so nothing can go stale.

### Planned
- Unit enchantments as modifiers with a caster and a duration (the city list is the
  template), equipment and hero data on units, spell books and research on players (a
  `Wizard` component once players become entities), battles as a temporary world with its
  own unit entities that use the same stat engine.

## 4. Where the original's data went

| Original | Here | Why it differs |
|---|---|---|
| `_UNITS[1009]`, index = unit id | Unit entities, `hecs::Entity` | No limit on count; a stale handle is detected (the entity's generation) instead of pointing at a reused slot. |
| Unit type index into a table | `Unit(UnitId)`: an index wrapped in a type that only fits `UnitDef` | You cannot pass a building id where a unit id is expected. |
| Unit enchantment bit flags | Planned: modifiers (stat, op, value, source) with caster and duration | Flags need code per flag; modifiers are data, show where a bonus came from, and can be removed by source. |
| Stack: units sharing a square | The same: `units_at(position)` | Unchanged, but capped at nine by `MAX_STACK` in `units.rs`. |
| `_CITIES[100]`, cached yields | City entities, yields computed | Cache invalidation bugs cannot happen; the cost (recomputing per query) is irrelevant at this scale. |
| `bldg_status[36]` (unbuilt / replaced / built) | `Buildings` list plus `BuildingDef::replaces` | One source of truth; the status is derived. |
| `enchantments[26]`, player + 1 | `Enchantments` list of `(spell, caster)` | Any number of enchantments; the spell is an id into the same table as spells cast any other way. |
| `construction` integer, < 100 building, ≥ 100 unit | `ProductionItem` enum | Meaning is in the type, not in a numeric range. |
| `_players[6]` big record | `Player` struct (gold, mana, tax, capital race, fame) now; wizard data later | Only what exists is stored; more players are possible. |
| `_world_maps`: 762 terrain-type numbers incl. shores | Two grids: base terrain class + feature overlay + road flag (see `map-layers.md`) | Shore and river variants are a rendering decision, not game state. Minerals, nodes, lairs and towers are features instead of separate arrays and flag bits. |
| `_LAIRS`, `_NODES`, `_TOWERS` arrays | Features on tiles (kind: encounter, node, tower) | One mechanism; lairs' guards and loot will become components of the site entity when encounters are built. |
| Compiled-in constants (`bt_Barracks`) | String ids in data files, resolved to typed ids at load | New content needs no code; every reference is validated once at load. |
| Per-unit and per-city fixed record order | Entities visited in creation order (`Serial`) | Deterministic, independent of slot reuse, and the same after a save and load. |

## 5. Identity, references and ordering

- **Entities** are referenced by `hecs::Entity`. Commands and events carry them. Because an
  entity can be despawned (a settler founding an outpost, a disbanded unit), code that
  holds one must tolerate `world.get` failing; the code does this with `.ok()` and
  `map_or`.
- **Definitions** are referenced by typed ids (`Id<T>`). In data files they are strings;
  `f4x-data` checks that every string resolves before the game starts.
- **Order**: ECS iteration order is not a game rule. Anything in game logic that iterates
  entities sorts by creation number first (`all_units`, `cities`, `units_at`, `sort_by_serial`). Hash maps are never
  iterated in logic. This is what keeps a seed plus a command log reproducible.
- **Creation numbers.** `hecs` reuses the id of a despawned entity, so ids do not give creation
  order and differ after a load. Every unit and city gets a `Serial` at spawn; sorting,
  "newest first" rules and saves use it. Spawning code must call `take_serial`.
- **Randomness** comes only from `Game::roll` / `next_u64`; the seed plus a count of draws is
  enough to resume the stream after a load.

## 6. Derived values and the modifier engine

Every number about a unit or city is computed:

```
base value (from the definition, or from a formula for cities)
  + modifiers from: race, abilities, buildings, enchantments, level effects, items (later)
  = result      (Add, then Percent, then Min/Max, then Set, then the stat's own floor)
```
`unit_stat`, `unit_modifiers` and `city_economy` are the entry points. A building does not
add 2 to a stored research number; it declares `Modify(CityResearch, Add, 2)` and the
economy sums whatever modifiers apply. Removing a building removes exactly its effect.

The effect executor (`rules::exec`) runs spell and ability effects against an
`EffectHost` trait, so the same rules run on the real world or a synthetic test world.
The real host over `hecs` is part of the wizards and spells step.

## 7. Systems

The full command list, the order of a turn and the movement and treasury rules are in
[`game-flow.md`](game-flow.md); this section is only about how they relate to the ECS.

There is no scheduler. A "system" is a method on `Game` called from a fixed place:

- `apply(Command)` validates and changes state, returning events.
- `end_turn` refills moves, runs `process_turn` (cities, then each player's treasury, then
  healing), then follows standing orders.
- Pure queries (`city_economy`, `visibility`, `find_path`, `treasury`, `can_build`) take
  `&self` and change nothing; the client calls them freely each frame.

This is deliberate. The game is turn-based, so there is no parallelism to gain, and an
explicit call order is easier to read, test and keep deterministic than a scheduler.

## 8. Working with `hecs` here

- `world.query::<(&A, &B)>()` borrows the world; collect what you need into a `Vec`
  before calling anything that mutates (the code does this everywhere).
- `world.insert_one(entity, component)` **replaces** an existing component of that type,
  which is how updates are done. It is not an error.
- `world.get::<&T>(entity)` returns a guard; drop it before mutating the same entity.
- Spawn with a tuple of components; add optional ones with `insert_one`.

### Saving
A save is a snapshot written by name (see `game-flow.md` section 8): it records units and
cities with their components, not `hecs` entities, and refers to content by string id. The
component tables above are therefore also the list of what a save must carry: **a new
component on units or cities needs a field in the snapshot and a test that it survives a
round trip.**

## 9. What this buys and costs

Gains: no hard limits except the game's own (nine per stack, 25 citizens per city);
modifiers instead of flags; no cache invalidation; typed ids; tests that build a
hand-made world in a few lines (`testutil.rs`) without any shipped data.

Costs: more indirection than a flat array; a recomputed economy per query (cheap here);
a loaded game gets new `hecs` entity ids, so entity ids never appear in saves and logic never
relies on them (see the creation number below).

## 10. Adding to the model

1. **A new per-entity fact** (say a hero's equipment): a component struct in the module
   that owns the concept, inserted where the entity is created, with a test that builds a
   synthetic world.
2. **A new number that rules can change**: add a variant to `Stat`, give it a floor in
   `Stat::floor` if it cannot go negative, and read it through `compute` where it is used.
   Content then changes it with `Modify` effects, no code needed.
3. **A new kind of content**: a definition struct in `rules/defs.rs`, a registry on
   `Rules`, a loader line and cross-reference checks in `f4x-data`, with a test that a
   broken reference is reported.
4. **A new mechanic**: add a `Command` and the `Event`s it produces, apply it in
   `Game::apply`, and keep the check ("can this happen?") as a `&self` method the client
   can also call.
5. Do not store anything that can be computed from existing state.
