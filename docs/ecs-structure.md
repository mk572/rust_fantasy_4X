# How game state is stored (and how that differs from the original)

**Version: 0.0.2** (`Cargo.toml`). The entity rework (tasks 307 to 339 in `TASKS.md`) changes the save format, so saves of another version will be refused (task 335).

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
| Combat | A separate temporary `Battle` record of battle units copied from the overland units at the start of a fight (not part of the `hecs` world); see `combat.md`. |
| Content | Tables of records (units, buildings, spells, races), some in the executable's data segment and some in the `.lbx` files; code refers to them by index and by compiled-in constants (`bt_Barracks`, `spl_Fireball`). |

Consequences of that model that matter here: hard limits (a 101st city is impossible),
meaning hidden in index ranges and flag bits, cached values that can go stale, and
behaviour that depends on slot order.

## 2. This project's model

```
Game                                   (lib.rs; the owner and the orchestrator)
├── world: hecs::World                 entities: units, cities, players and wizards (a player is an entity with
│                                      `PlayerId` and its components; a wizard one with `Name`, `Portrait`, `Banner`,
│                                      `CapitalRace`; heroes and battle units are not, yet)
├── player_index: HashMap              `PlayerId` to the player's `Entity` (`Game::player_entity`)
├── map: Option<Map>                   two dense grids (terrain, feature, road); not entities
├── fog: Fog                           what each player has explored
├── content: Content                   definitions loaded from data: terrains, features, rules
│   └── rules: Rules                   registries: races, units, abilities, buildings, realms, spells,
│                                      items, level paths
├── diplomacy: Diplomacy               contact, relations, treaties, proposals (diplomacy.rs)
├── battle: Option<ActiveBattle>       the fight in progress: a `Battle` plus the overland units it
│                                      was copied from (battle/); not saved
├── rng: GameRng                       the one source of randomness, with its draw count (game/rng.rs)
├── turn, magic_strength, difficulty   the game's counters and settings
├── human: Option<PlayerId>            the human wizard
├── game_over: Option<GameOver>        set once the game has ended; every later command is refused
├── next_serial                        the counter behind `Serial` (section 5)
├── override_log                       the cheat commands that changed the game
├── battle_brains                      the learned players of battles (task 170)
└── undo_stack                         snapshots for undoing a command (game/undo.rs)
```

This is the layout today. The rework of tasks 307 to 339 changes the left half of it: `players` goes,
and wizards, heroes, spell instances, terrain features and items become entities in `world`. Section 3
is the design; it is written first and each task replaces the text above it as it lands (task 337
finishes the pass).

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

### Who may read and change what

The fields of `Game` (`world`, `map`, `fog`, `content`, `players`) are private to `f4x-core`. The
other crates ask the game through its queries (`unit_*`, `city_*`, `player`, and `game/access.rs`:
`content()`, `map()`, `players()`, `is_explored`, `city_buildings`...), play through
`Game::apply`, and set a game up by hand (tests, scenarios, tools) through the setters of
`game/access.rs` and the `cheat_*` mutators of `game/cheat.rs`, which only the console's override commands
use. An AI wizard reads through a `PlayerView` (`player/view.rs`): its own things, the cities of
others on squares it has explored and the units on squares it sees, so it cannot know more than a
human player. Tests of other crates may read a component with the hidden
`Game::component::<T>`.

### Domains: how the code is organised

A **domain** is a part of the rules with state of its own and rules that mostly concern that
state: diplomacy, a wizard's magic, the battle engine, the map and fog, the modifier engine.
`Game` owns the state and orchestrates; it does not hold the rules. The base rule is in
[`design.md`](design.md) section 3, the audit and the plan are in
[`architecture-plan.md`](architecture-plan.md). In short:

1. **A new rule goes on the domain it belongs to**, as a method of that domain's struct or a
   function over the pieces it needs, not as another `impl Game` method. `Game` only gets
   orchestration: routing a `Command`, building the arguments, applying what comes back.
2. **A domain's fields are private to its module.** Other modules call its methods; they do
   not reach into its fields. (`Diplomacy` is private in this sense; the wizard's rules are
   methods of the wizard handles in `wizard/sheet.rs`, over components of the wizard entity.)
3. **A domain takes what it needs, not `&mut Game`.** Its methods take `&Content`, `&mut GameRng`
   or numbers the game worked out (`Magic`), or a context value (`DiplomacyCtx`).
4. **The rest of the world is asked through `WorldView`** (`game/view.rs`): a small read-only trait
   with the turn, the human, the content, the players and a few questions with plain-data
   answers (what a wizard sees, a player's units, a city count). `Game` implements it and
   `testutil::FakeView` answers from lists, so a domain's rules can be tested with no world,
   map or fog. A question goes into the trait when two domains need it.
5. **What a domain wants changed outside itself comes back as outcomes**, not as writes:
   diplomacy leaves `Outcome::Gold` and `Outcome::Spell` in its context, and `Game` applies
   them. Events are returned the same way.
6. **Random numbers come from a `GameRng`** that the caller passes in (`game/rng.rs`). It is a field
   of `Game`, so a domain can take `&mut GameRng` next to other borrows.

Three ways to give a domain what it needs, all in use:

- **Borrowing the fields apart**, where the domain's state is a field of something `Game` can
  borrow beside the content and the generator (the wizard handles: `Game::wizard`, `Game::wizard_mut`).
- **Lending the domain out for a call**, where its rules need the whole game as a read-only
  view (`Diplomacy`: `Game::with_diplomacy` takes the state and the generator out of the game,
  runs the rules over `&Game` as the `WorldView`, puts them back and applies the outcomes).
  The spell executor lends the generator to its host the same way (`GameHost`, task 184).
- **A read handle over a few fields** where the domain's state lives in the ECS world (units:
  `UnitView<'a>` holds `&World` and `&Content`, made by `Game::units()`; the city's yields likewise in `Economy<'a>` over the world, the map and the content, made by `Game::economy()`; what changes
  units is a function over `&mut World`, such as `unit::spawn`, `experience::gain_xp` and
  the `spell/temp_effects.rs` functions). A test builds a world and the content and no `Game`.

Where this stands: every domain of the game follows it (tasks 179, 180, 185 to 193 and 196, 197):
diplomacy, magic (income in `wizard/magic_income.rs` over a small view; the commands that need no world in
`wizard/magic_commands.rs`), the units, the economy and production (the checks are methods of `Economy`,
which is the world over the map), the turn (`process_turn` only sequences the steps), movement and
transport (plans on `Economy`; the moves themselves stay on `Game` because they change the fog), the
lairs (clearing and loot as functions over the map and the wizard's magic), conquest (the loss, the
loot and the razing as plain functions), the fortress (its rules as functions over the player) and
vision. The battle engine, the rules layer, the map and the fog already did. The old thin
`impl Game` wrappers (`unit_stat`, `city_economy`, `plan_step`, ...) are gone: callers ask
`game.units()` or `game.economy()` (task 198). Task 183's test reports the number of `impl Game`
methods per file, as a guideline.


## 3. The entity model of the rework (tasks 307 to 339)

Written by task 310. It is the design that tasks 311 to 339 build; where it and the code differ,
the code is behind, not wrong, until the task named next to the item has landed. Rules of the rework:
the game's rules do not change (seeds, draw order and old saves may), a wrong structure is moved
and not worked around, and names are singular.

### 3.1 The entity kinds

| Entity | Module | Components | Built in |
|---|---|---|---|
| **Player** | `player/` | `PlayerId`, `Treasury {gold, mana}`, `Tax`, `Bonus`, `History`, `GrandVizier`, `Offers`, `Defeat`, `Control`, `Neutral` (marker), `WizardLink`, `Armory` and `Roster` (until tasks 328 and 327 replace them), `Serial` | 311, 313, 314, 317 |
| **Wizard** | `wizard/` | `Name`, `Portrait`, `Banner`, `CapitalRace`, `Books`, `Retorts`, `Skill`, `PowerSplit`, `Research`, `KnownSpells`, `Candidates`, `Fame`, `Personality`, `Objective`, `Fortress`, `SummonAt`, `Banishment`, `PlayerLink`, `Serial` | 312, 315, 316 |
| **Unit** | `unit/` | `Unit`, `Owner`, `Position`, `Hurt`, `Experience`, `WeaponQuality`, `MovesLeft`, `Order`, `Standby`, `Serial`, and the other components it has today | exists |
| **Hero** | `hero/` | `Hero` (name, abilities, casting skill, spells), `HeroState`, `Owner`, `Experience`, `HeroSlots` of items, `Serial`; the unit components while serving | 326 to 328 |
| **City** | `city/` | `City`, `Position`, `Owner`, `Population`, `Farmers`, `Buildings`, `CityRace`, `Production`, `Serial` | exists |
| **Spell instance** | `spell/` | `SpellCast {def}`, `Caster`, `SpellTarget`, `Duration`, `Serial` | 318 to 325 |
| **Terrain feature** | `feature/` | `TerrainFeature(FeatureId)`, `Position`, `Serial`; `Node`, `Site`, `Volcano` by kind; `Owner` where owned | 329 to 332, 338 |
| **Item** | `item/` | `ItemKind`, `Name`, `Bonus`, `Powers`, `Charges`, `Prefab`, `HeldBy`, `Serial` | 333, 334 |

Everything with a `Serial` can be named in a save and in a command log (section 3.9). Definitions
(`SpellDef`, `UnitDef`, `FeatureDef`, `ItemDef`...) stay registry data and are never entities.

### 3.2 Player and wizard

Two entities, because they answer different questions.

- The **player** is who plays. It has what it owns and does: gold and mana, the tax rate, its offers,
  its turn (human or AI `Control`), whether it is defeated, and through `Owner` its units, cities and
  heroes. Mana is the player's resource (the reserve pays the upkeep); casting skill is the wizard's.
- The **wizard** is what the wizard creation screen makes, and what follows from it: name, portrait,
  banner colour, the capital race, spell books, retorts, the spells known and on offer, research,
  casting skill and the split of power between mana, research and skill, fame, personality and objective, and the fortress and summoning circle (a wizard's
  home) with the banishment that follows from losing it.
- A player may have no wizard. The independent (neutral) cities and the monsters' owner are players
  with `Neutral` and no `WizardLink`. Code that reads a wizard for "every player" (today
  `player.magic`) must handle the missing wizard explicitly.
- The two refer to each other (`WizardLink`, `PlayerLink`) and are made together by game setup and
  by the AI wizard builder. `Game::player(id)` and `Game::wizard(id)` return read handles.

Where a thing goes when it is not obvious: **fame** is the wizard's (it is on the wizard sheet);
**offers** are the player's (a pop-up a turn brings); **the armory** is the player's (vault slots hold
item entities held by the player); **defeat** is the player's, **banishment** the wizard's.

### 3.3 Links and ids

- **Inside the world, links are `Entity` values**: `Owner(Entity)` on a unit, city, hero or feature
  points at a player; `Caster(Entity)` on a spell instance at a wizard; `SpellTarget`, `HeldBy`,
  `WizardLink` and `PlayerLink` hold entities. A link to something that has gone is a bug; the lifecycle
  rules in 3.8 say who despawns what so that none dangles.
- **`PlayerId` is the key for what lives outside the world**: the fog, the diplomacy pairs, the map's
  owner fields, saves, console commands and display ("wizard 3"). It is defined in `player/`, is a
  component of the player entity and is never reused. One index `PlayerId` to `Entity` on `Game`
  turns one into the other. No owner is ever a raw `u8`.
- **A unit, hero or city is always owned by a real player**: a human wizard, an AI wizard, an
  independent city's player, or the monsters' player. Nothing is ownerless and nothing has a
  placeholder owner. Tests build only such situations; a test of a situation the game cannot
  create in normal play exists only to test error handling (a rejected command, a refused save).
- Independent cities' players get ids after the real players, in order of creation. The one player
  that owns the monsters is made with the game (`Game::monsters`, id `player::MONSTERS`, kept at
  255 until the save format changes in task 335). It is an entity with `PlayerId` and `Neutral` like
  the cities' players but is in no list of players (`Game::has_player` is false for it). Whether a player is
  neutral is the `Neutral` component (`Game::is_neutral_player`), never an id test.
- `Game::set_owner(thing, player)` hands a unit or city to a player; `units().unit_owner` and
  `economy().city_owner` give the `PlayerId` back, `unit_owner_entity` and `city_owner_entity` the
  entity. `economy().cities()` returns `CityInfo` (name and owner id) for display code.
- A **position index** (square to entity, for terrain features) is the one other index. It is rebuilt
  on load and is never saved.

### 3.4 Spell instance

A spell cast is an entity only while it lasts or while it is being paid for. An instant spell does its
work and leaves no entity.

- `SpellCast { def: SpellId }`: which definition. `Caster(Entity)`: the wizard. `SpellTarget`:
  `Global`, `Wizard(Entity)`, `City(Entity)`, `Unit(Entity)`, `Feature(Entity)`, `Square(plane, x, y)`
  or `Battlefield`. `Duration`: turns left, or none (lasts until dispelled or the caster loses it).
- A **cast in progress** is a spell instance in a casting state, with the mana still owed, owned by
  the wizard; finishing it either does the instant effect and despawns, or turns the instance into
  the lasting one. Crafting an item and raising a hero are casts of the same kind.
- One query covers what is now eight containers: "the enchantments on this city", "this wizard's global
  spells", "the upkeep this wizard pays", "what dispel can reach". Dispel, upkeep and the removal when
  a caster loses them are queries over spell instances (task 325).
- **Battlefield enchantments** live in a small `hecs::World` of the battle, because a battle is not in
  `Game::world` and is not saved. When battles are saved (task 99) that world is saved with them.
- Not spell instances: corruption (a map layer), volcanoes (terrain features), temporary effects that no
  spell causes (kept as a component if there is such a source; task 321 settles it).

### 3.5 Hero

A hero is an entity that outlives its body. The roster of 35 records goes; a hero type nobody has
hired is a definition, and a hero that has been hired is an entity.

- `HeroState`: `Serving`, `Dead` (can be resurrected or recalled), `Gone` (lost for good).
- While serving, the hero carries the unit components (`Unit`, `Position`, `MovesLeft`...) and is
  found by any unit query. When the body dies the unit components are removed and the state becomes
  `Dead`; abilities, experience, name and spells stay on the entity. `hero::game::remove_unit`
  stays the one exit path.
- A wizard has at most six heroes; the count is a query over the player's hero entities.

### 3.6 Terrain feature

The map keeps the per-tile layers: terrain, roads, rivers, lakes, corruption. Everything placed on it
that has identity and state is an entity.

- Kinds (`FeatureKind` in `content/features.ron`): minerals and other markers (no extra component),
  encounter sites, towers, nodes; and volcanoes, made and cooled by spells.
- `Node` (realm, power, aura squares, guardian, `Owner`) and `Site` (the lair: intact, guards, gold,
  mana, loot values, prisoner, special) are components on the feature entity. A cleared tower or
  node stays; a cleared encounter site is despawned, as its marker is removed today. A tower is one
  entity at one `(x, y)` on both planes; if its two lairs differ between planes, `Site` keeps a guard
  list per plane.
- **Node base terrain.** A node square also has the node's base terrain (`sorcery_node`,
  `nature_node`, `chaos_node`: their own food, production and movement values), as in the original.
  The aura squares keep their terrain (task 338; the one change of rules in the rework).
- Lair guards are made as neutral units when the site is entered and removed after the fight, as
  today. Where loot items are made (at map creation as item entities held as `Loot`, or at clearing)
  is decided in task 331.

### 3.7 Item

An item instance is an entity with its kind, name, bonus, powers, charges, optional prefab id and
a `HeldBy`: `Hero(entity, slot)`, `Vault(player, slot)`, `Award(player)`, `Crafting(wizard)`,
`Offered(player)` or `Loot(feature)`. Moving an item changes `HeldBy`; an item with no holder is
despawned. The prefabs already made stay a registry on `Game`.

### 3.8 Lifecycles: who is created and removed when

| Event | What is spawned | What is despawned | What stays |
|---|---|---|---|
| Game setup | players and wizards (together), the monsters' neutral player, features, neutral cities with their players | | |
| Unit created | the unit entity | | |
| Unit dies or is disbanded | | the unit; spell instances targeting it | a hero's entity (state `Dead`); its items pass to the heir or winner by changing `HeldBy`, or are despawned when lost |
| City founded | the city | | |
| City conquered | | | the city, with its new `Owner`; its enchantments as today (`conquest.rs` decides) |
| City destroyed | | the city; spell instances targeting it | |
| Neutral city taken or destroyed | | its neutral player once nothing refers to it | its `PlayerId` is not reused |
| Wizard banished | the Spell of Return, as a cast in progress | the fortress and summoning circle (components of the wizard) | player, wizard, `Banishment`, the wizard's other spell instances |
| Wizard defeated | | all the spell instances the wizard cast (global, on cities, on units, the cast in progress), as `aftermath` does today; the wizard's units; its nodes are freed (`Owner` removed) | player, wizard and `Defeat`; its cities become neutral cities with a neutral player each |
| Spell cast (lasting) | a spell instance | | |
| Spell expires, is dispelled or cancelled | | the spell instance | |
| Feature cleared (encounter site) | | the feature | |
| Raise Volcano / cooled | a volcano feature / | / the volcano feature | |
| Item made, bought, crafted, won | the item | | |
| Item scrapped or lost | | the item | |
| Battle starts / ends | the battle's own world with its battlefield spell instances | the battle world at the end | |

A player that has a wizard, and that wizard, are never despawned during a game: the score, the
history and the diplomacy log refer to them. The players without a wizard (the monsters' owner and
the independent cities' players) are the exception: a neutral city's player is despawned once
nothing refers to it, as the table says, and its `PlayerId` is not reused.

### 3.9 Saving and replaying

- The save writes each entity kind as its own list, in `Serial` order; a link is written as the
  linked entity's serial (the `stable_ref` of the replay code already does this for commands).
  `PlayerId`s are written as values. The map layers are written as now; the node, lair, tower and
  volcano tables and `Grid.features` are gone, replaced by the feature list.
- `SAVE_VERSION` becomes 2 with game version 0.0.2. A save or replay of another game version is
  refused with a plain message (task 335).
- A command that names an entity of a new kind (a hero, an item, a feature) names it by serial in the
  log, as units and cities are named today.

### 3.10 What stays outside the world

The map layers and the fog (dense grids), the content registries, `Diplomacy` (pairs of players, keyed
by `PlayerId`), the world events table, the RNG, the undo stack, and the battle in progress (a
`Battle` with its own world for its spell instances). Battle units are not entities in `Game::world`.

### 3.11 How the code reads and writes the new entities

The pattern is the one of section 2: a **read handle** over the parts it needs
(`UnitView<'a>`, `Economy<'a>`, and for the new kinds `PlayerView`-style handles from
`Game::player(id)`, `Game::wizard(id)`), and functions over `&mut World` for changes. Domain state
is private to its module; a domain returns outcomes for what it cannot change itself. A wizard
handle borrows the world and the content, so the borrow of the RNG beside it stays possible.
Queries over a `hecs` world have no fixed order: wherever the order is visible (events, the AI's
choices), sort by `Serial`.
