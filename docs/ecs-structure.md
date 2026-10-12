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
| Combat | A separate temporary `Battle` record of battle units copied from the overland units at the start of a fight (not part of the `hecs` world); see `combat.md`. |
| Content | Tables of records (units, buildings, spells, races), some in the executable's data segment and some in the `.lbx` files; code refers to them by index and by compiled-in constants (`bt_Barracks`, `spl_Fireball`). |

Consequences of that model that matter here: hard limits (a 101st city is impossible),
meaning hidden in index ranges and flag bits, cached values that can go stale, and
behaviour that depends on slot order.

## 2. This project's model

```
Game                                   (lib.rs; the owner and the orchestrator)
├── world: hecs::World                 entities: units and cities (a unit or city is an entity;
│                                      wizards, heroes and battle units are not, yet)
├── map: Option<Map>                   two dense grids (terrain, feature, road); not entities
├── fog: Fog                           what each player has explored
├── players: Vec<Player>               gold, mana, tax rate, capital race, fame, fortress, banner...
│   └── magic: Magic                   the wizard: books, retorts, skill, research, casting (magic.rs)
├── content: Content                   definitions loaded from data: terrains, features, rules
│   └── rules: Rules                   registries: races, units, abilities, buildings, realms, spells,
│                                      items, level paths
├── diplomacy: Diplomacy               contact, relations, treaties, proposals (diplomacy.rs)
├── battle: Option<ActiveBattle>       the fight in progress: a `Battle` plus the overland units it
│                                      was copied from (battle/); not saved
├── rng: GameRng                       the one source of randomness, with its draw count (rng.rs)
├── timers: Vec<Timer>                 countdowns of timed spells
├── turn, magic_strength, difficulty   the game's counters and settings
├── human: Option<PlayerId>            the human wizard
├── game_over: Option<GameOver>        set once the game has ended; every later command is refused
├── next_serial                        the counter behind `Serial` (section 5)
├── override_log                       the cheat commands that changed the game
├── battle_brains                      the learned players of battles (task 170)
└── undo_stack                         snapshots for undoing a command (undo.rs)
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

### Who may read and change what

The fields of `Game` (`world`, `map`, `fog`, `content`, `players`) are private to `f4x-core`. The
other crates ask the game through its queries (`unit_*`, `city_*`, `player`, and `access.rs`:
`content()`, `map()`, `players()`, `is_explored`, `city_buildings`...), play through
`Game::apply`, and set a game up by hand (tests, scenarios, tools) through the setters of
`access.rs` and the `cheat_*` mutators of `cheat.rs`, which only the console's override commands
use. An AI wizard reads through a `PlayerView` (`player_view.rs`): its own things, the cities of
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
   not reach into its fields. (`Diplomacy` is private in this sense; `Magic` lives in a
   public `Player::magic` field because the game borrows the wizard, the content and the
   generator apart, and the tests build it; its rules are still methods of `Magic`.)
3. **A domain takes what it needs, not `&mut Game`.** Its methods take `&Content`, `&mut GameRng`
   or numbers the game worked out (`Magic`), or a context value (`DiplomacyCtx`).
4. **The rest of the world is asked through `WorldView`** (`view.rs`): a small read-only trait
   with the turn, the human, the content, the players and a few questions with plain-data
   answers (what a wizard sees, a player's units, a city count). `Game` implements it and
   `testutil::FakeView` answers from lists, so a domain's rules can be tested with no world,
   map or fog. A question goes into the trait when two domains need it.
5. **What a domain wants changed outside itself comes back as outcomes**, not as writes:
   diplomacy leaves `Outcome::Gold` and `Outcome::Spell` in its context, and `Game` applies
   them. Events are returned the same way.
6. **Random numbers come from a `GameRng`** that the caller passes in (`rng.rs`). It is a field
   of `Game`, so a domain can take `&mut GameRng` next to other borrows.

Three ways to give a domain what it needs, all in use:

- **Borrowing the fields apart**, where the domain's state is a field of something `Game` can
  borrow beside the content and the generator (`Magic`: `Game::magic_parts`).
- **Lending the domain out for a call**, where its rules need the whole game as a read-only
  view (`Diplomacy`: `Game::with_diplomacy` takes the state and the generator out of the game,
  runs the rules over `&Game` as the `WorldView`, puts them back and applies the outcomes).
  The spell executor lends the generator to its host the same way (`GameHost`, task 184).
- **A read handle over a few fields** where the domain's state lives in the ECS world (units:
  `Units<'a>` holds `&World`, `&Content` and `&[Player]`, made by `Game::units()`; the city's yields likewise in `Economy<'a>` over the world, the map, the content and the players, made by `Game::economy()`; what changes
  units is a function over `&mut World`, such as `units::spawn`, `experience::gain_xp` and
  the `temp_effects` functions). A test builds a world and the content and no `Game`.

Where this stands: diplomacy, magic (income in `magic_income.rs` over a small view; the commands that need no world in `magic_commands.rs`), the units, the economy and production follow it (tasks 179,
180, 185, 186 and 187; production's checks are methods of `Economy`). Their `impl Game` methods are still there as thin wrappers with the old names
(`unit_stat`, `city_economy`, ...) that make a `Units` or an `Economy` and ask it; task 198 moves
the callers over and deletes the wrappers. The turn code does too (task 188: `process_turn` only sequences
the steps). What does not yet: movement, the lairs, conquest and the fortress (189 to 192; vision is done, task 193);
the battle engine, the rules layer, the map and the fog already did. Task 183 adds a test that
reports the number of `impl Game` methods per file, as a guideline.
