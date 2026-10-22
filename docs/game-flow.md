# Game flow and systems

How a game runs: the command and event layer, what happens in a turn and in what order,
and how movement, production and the treasury work. For how state is stored see
[`ecs-structure.md`](ecs-structure.md); for the map see [`map-layers.md`](map-layers.md).

Code: `crates/f4x-core/src/{lib,turn,movement,pathing,production,economy,units,vision}.rs`.

## 1. The shape of the loop

```
client / AI / test                         f4x-core
─────────────────                          ───────────────────────────────
read state with queries  ────────────────▶ &Game methods (no change)
send a Command           ────────────────▶ Game::apply(command)
                         ◀──────────────── Vec<Event>  (what happened, or why not)
draw, log, animate from events and queries
```

- **Everything that changes the game is a `Command`** passed to `Game::apply`. There is no
  other way to mutate state from outside the core.
- **`apply` returns events**: facts the caller can show or log. A refused command returns
  `Event::Rejected(reason)`; it does not panic and changes nothing.
- **Queries are free**: methods taking `&self` (`city_economy`, `find_path`, `visibility`,
  `treasury`, `can_build`, `can_found_city`, `buy_cost`, ...) change nothing, so the
  client calls them every frame to explain and to enable or disable controls.
- **Determinism:** the same starting `Game`, the same seed and the same commands in the same
  order give the same result. All randomness goes through `Game::roll`; anything that
  visits entities does so in creation order (each unit and city carries a creation number,
  `Serial`; `hecs` entity ids are reused and are not used for ordering).
- **Saves are snapshots** (section 8): the whole game is written out and read back. Within one
  run the command log still reproduces a game from its seed; commands carry entity ids, so a
  log cannot be replayed across a load (the loaded entities have new ids).

There are no per-player turns inside the core: all players issue commands as they like, and
`EndTurn` then processes everyone at once. A hot-seat or AI layer decides who acts when.
Commands do not carry an acting player, so **ownership is not checked**: the caller is
trusted. (Needed before any networked play.)

## 2. Commands and their events

| Command | What it does | Events on success | Typical rejections |
|---|---|---|---|
| `EndTurn` | Runs the turn (section 4) | `TurnStarted`, then everything the turn produced | none |
| `CreateUnit { owner, pos, unit }` | Makes a unit with full moves and health; explores around it | `UnitCreated` | none (not validated: for scenario setup and summoning) |
| `MoveStack { units, to }` | One step of a stack to an adjacent tile | `UnitsMoved`, or `AttackRequested` if hostile | `Move(...)` |
| `GoTo { units, to }` | Plans a route and starts walking it at once | `UnitsMoved`... (as far as moves allow), or `AttackRequested` | `Move(Impassable)` if there is no route |
| `ClearOrders { units }` | Removes standing orders | none | none |
| `SetStandby { units, standby }` | Puts units on Done (skipped for this turn) or Patrol (skipped until they move), or takes them off with `None` | none | none |
| `ShiftPlane { units }` | Same tile, other plane, costs one move | `UnitsMoved` | `Move(CannotShiftPlane)` and others |
| `FoundCity { owner, pos, name, race, population }` | New city (or outpost if population is 0) | `CityFounded` | `FoundCity(...)` |
| `SettleOutpost { unit, name }` | Settlers become an outpost where they stand | `CityFounded` | `FoundCity(...)`, `Move(BadSelection)` |
| `SetProduction { city, item }` | Choose what the city builds | none | `Production(...)` |
| `BuyProduction { city }` | Pay gold so the item completes at the next turn change | none | `Production(NotEnoughGold, NothingToBuy)` |
| `SellBuilding { city, building }` | Sell for its construction cost | none | `Production(NotBuilt, MissingBuilding)` |
| `SetFarmers { city, farmers }` | Citizens on farms, clamped to the minimum that feeds the city and the population | none | none |
| `SetTaxRate { player, rate }` | Tax rate, clamped to the table | none | none |
| `PlaceFortress { player, pos, name, race, population }` | Found the wizard's first city with the fortress and summoning circle in it (once per wizard) | `CityFounded`, `FortressPlaced` | `Rejected(Fortress(..))` |

Events: `TurnStarted`, `CityFounded`, `ProductionCompleted`, `CityGrew`, `CityShrank`,
`UnitsDisbanded`, `UnitCreated`, `UnitsMoved`, `AttackRequested`, `Rejected`. Not every
state change has an event: units produced by a city appear through `ProductionCompleted`,
not `UnitCreated`; changes to production, farmers and tax have none because the caller
already knows.

Rejection reasons are typed: `FoundCity(FoundCityError)`, `Move(MoveError)`,
`Production(ProductionError)`.

## 3. Movement

### 3.1 Rules
- **Half-moves.** A unit's `Moves` stat is whole moves; the game counts half-moves (twice
  the stat), because roads and pathfinding cost half a move. Moves are refilled at turn start.
- **A step is always allowed with any moves left**, even if it costs more than remains; the
  unit ends the turn with none. A unit with none cannot move (`NoMovesLeft`).
- **Cost of entering a tile** (`step_cost`, in half-moves):

| Mover | Cost |
|---|---|
| Non-corporeal, onto land | 1 |
| Flying, or in a stack carried by a wind-walker | 2 on any tile, water included |
| Sailing | 2 on ocean or on a city of its owner; otherwise impossible |
| Swimming | 2 on ocean; on land as walking |
| Walking | terrain `move_cost` x 2; impossible on ocean or non-walkable terrain |
| Walking, between two road tiles | 1 |
| Pathfinding ability | at most 1 |
| Forester ability, onto forest | at most 2 |
| Mountaineer ability, onto hills or mountains | at most 2 |

- **A stack** is units on one tile of one owner, at most nine per tile. All units in a move
  must be on the same tile and have moves left; each pays its own cost; the move fails if any
  unit cannot enter the tile.
- **Hostile tiles.** If the target holds units of another player, or another player's city,
  nothing moves and the core emits `AttackRequested { attackers, from, to, defenders }`.
  Combat resolution is not built yet; the client logs it.
- **Plane shift.** Allowed on a tower tile, or for a stack whose every unit has planar
  travel. The destination (same x, y on the other plane) must be enterable by every unit,
  free of hostile units, and have room. It costs one full move.
- **Exploration.** After each move the owner explores around the new tile (unit `Sight`,
  at least 1) as far as it sees: a diagonal step counts 1.5 and a square is in sight up to the range plus 0.5
  (range 2 is a 5 by 5 block without its corners).
  The border of the known land is drawn on its outermost tiles. Every player, the AI's too,
  has an exploration of their own, and starts with what their capital sees, since a new city explores when it is founded.

### 3.2 Routes and standing orders
- `find_path` is A\* on the eight-neighbour grid (wrapping sideways), costing each step as
  the **slowest unit's** cost. Tiles holding hostile units are avoided unless they are the
  destination. `path_turns` estimates turns from the allowance of the units that move: land units riding a ship of the stack cost nothing and do not slow it, so the ship's speed decides.
- `GoTo` stores the path in an `Order::GoTo` component on every unit of the stack and
  immediately follows it as far as moves allow.
- At each turn change, units with an order continue (section 4, step 5). Units on the same
  tile with identical orders walk together. The order ends when the path is finished, when a
  step is refused for any reason other than running out of moves, or when it leads into
  hostile units (an `AttackRequested` is emitted and the order is cleared).

## 4. The turn

`EndTurn` runs these steps in this order:

1. **Turn counter** goes up; every unit's moves are refilled to its allowance.
2. **`TurnStarted`** is emitted.
3. **Treasury**, per player in the order players were added (section 5), on the cities as they
   stand: first every city's farmers are raised to the minimum that feeds its citizens. As in
   the original, upkeep is paid before the cities grow, so a city that grows this turn makes
   its new mouths felt next turn, when the player can still put them on the farms.
4. **Cities**, one by one in creation order:
   - *Outposts* (no citizens) roll to grow: a chance each turn made of the race's base
     chance, the land's food capacity, bonuses from worked features (mines, game), and
     enchantments; if the roll succeeds the outpost gains a few tens of people, and a small
     chance makes it lose some. At 100 tens it becomes a one-citizen city (farmers are set,
     `CityGrew`). An outpost does nothing else.
   - *Cities*: the economy is computed **once**; the item in production gets this turn's production added and, if
     covered, completes (a building is added and the city falls back to housing; a unit is
     created in the city with the guilds' starting experience and the same unit is queued
     again; surplus production is lost); then population changes by the growth rate computed
     in the same economy (`CityGrew`/`CityShrank`). Because the economy was computed before
     the changes, a building finished this turn first affects the next one.
5. **Magic**, for all wizards (see [`magic.md`](magic.md) section 9): power applied, research
   checked, mana upkeep paid, casting progress, finished spells resolve.
6. **Healing** of wounded units (section 6).
7. **Standing orders** continue for units that have them, in creation order (section 3.2).

Planned additions and where they go: random events (`events.md`) where `Next_Turn_Calc` runs them;
combat resolution wherever an `AttackRequested` is acted on; victory checks last.

## 5. The treasury

`Game::treasury(player)` previews what the next `EndTurn` will do; the turn applies the same
numbers.

**Income**, from the player's cities as they stand before step 4 changes them:
- gold of each city (taxes, trade, mines, percentage buildings) minus its building upkeep;
- plus ten gold per noble hero (none exist yet);
- plus **spare food sold at two for one**: each city's food beyond what its citizens eat,
  halved, summed, minus half the army's food need, never below zero.

**Then upkeep:**
- **Food.** Each normal unit eats one food. If the army needs more than the cities have
  spare, units that eat are disbanded, newest first, until it fits.
- **Gold.** Normal units and heroes cost their gold upkeep (a noble hero costs none), reduced
  by the player's fame, never below zero. Gold cannot go below zero: if the upkeep exceeds the
  treasury (income already added), units that cost gold are disbanded newest first until it
  fits, then the upkeep is paid.
- Disbanded units are removed and one `UnitsDisbanded { player, units }` is emitted.

Mana and research are not handled here yet. `balance_farmers(player)` is a helper, used by
the client at game start, that sets farmers so the cities also feed the army.

## 6. Healing

Each unit with damage heals once per turn, except Death creatures (born so or made undead, `unit/undead.rs`) and units under Black Channels. The base
is a fraction of its total hit points: one twentieth in the field, one tenth in a city of
its owner, one sixth in such a city if it has an animist's guild. The remainder is settled by
a seeded roll so the average is exact. Every unit in a stack that contains a unit with the
`healer` ability additionally heals a fifth of its total hit points. A unit heals at least
one point.

## 7. Production in detail

- **Items:** a building, a unit, trade goods (turn production into gold, two for one), or
  housing (faster growth). A city always has one item; new cities start on housing.
- **Allowed** (`can_build`): a building must not already be built, must allow the city's
  race, and needs all its required buildings. A normal unit must be in the race's list; a
  fantastic unit (boats, siege) is open to all; both need their required buildings, and
  ships also need a coastal city. Heroes and summoned units cannot be built.
- **Cost:** a building's cost; a unit's cost reduced by the city's mines (never below half).
- **Buying:** four times what remains if nothing is done yet; three times if less than half of
  the remaining amount is done; otherwise twice. Gold is paid at once; the item completes at
  the next turn change.
- **Selling:** a third of the construction cost (city walls a sixth, at least 1), one sale per city per turn; refused if another built building requires it; selling what the current production needs puts the city back on Housing.
- **Replaced buildings:** a building can replace another (an armory replaces barracks). The
  old one stays built and keeps its effects and its value as a requirement, but costs no
  upkeep.

## 8. Saving and loading

`Game::snapshot()` captures a whole game as a `Snapshot`; `Snapshot::restore(content)` builds
the game again. `f4x_data::save::{save_game, load_game}` do the file work. Code:
`f4x-core/src/game/save.rs`, `f4x-data/src/save.rs`.

**What a save holds:** the seed and how many random numbers have been drawn (so the random
stream resumes exactly), the turn, the creation counter, the map (terrain, features, roads),
each player's explored area (bit-packed), the players, every unit (with moves, damage,
experience and any standing order) and every city (population, farmers, buildings,
enchantments, race, current item and progress).

**What it does not hold:** content definitions, events, or the command log. A save is a game
state, not a content pack.

**References by name.** Everything that points at content (a unit's definition, a building,
a terrain, a feature, a race) is saved as its string id and looked up again on load. Adding
content, or reordering the data files, does not break old saves (a test restores a save into
content with reordered tables and gets the identical game). A save that needs content that
is no longer loaded is refused with a message naming it (`LoadError::UnknownId`), never
loaded wrongly.

**Same game after loading.** Units and cities are written in creation order with their
creation numbers, so a loaded game orders everything as the original did. A test saves a game
in which entity ids had been reused, loads it, plays twelve turns on both copies and compares
the snapshots after every turn: they never differ.

**Checks on load:** the format version (an unknown version is refused); every id resolves;
positions are on the map; creation numbers are unique and below the counter; farmers do not
exceed citizens; the explored area matches the map; a player appears once. Failures are
`LoadError::{Parse, Version, UnknownId, Inconsistent}` and leave the running game untouched.

**Files.** The text format is RON, about 130 KB for a full 60x40 map, readable and
diff-able. `save_game` writes to `<file>.tmp` and renames it, so an interrupted save never
damages an existing one. Saves live in `saves/` (gitignored).

**Versions.** `SAVE_VERSION` is bumped when the layout changes incompatibly. Adding optional
fields does not need a bump. Migrating older versions would be added as a conversion step
before `restore`. No migration is planned while there are no old saves (task 73 was dropped).

**Game version.** The game has one version, the workspace `version` in `Cargo.toml` (now 0.0.1),
read as `save::GAME_VERSION`. Every save records it in `Snapshot.game_version` and in the header
comment (`// f4x save, version 1, game 0.0.1`). A load does not compare it: only `SAVE_VERSION`
decides whether a file is readable. The game version says which build wrote a save, for bug reports
and for a later migration step. Bump it in `Cargo.toml` for each release.

## 9. What the client does with this

The macroquad client (`f4x-client`) runs this loop each frame: read input, turn it into a
`Command`, `apply` it, add any events to its message log, then draw from queries. It never
touches the `hecs::World` directly except to read. The `--shot` mode runs a few commands
(`--turns N`) and renders a single frame, which is how the flow is checked without a person.

Keys: F10 saves to `saves/quick.ron`, Ctrl-L loads it, and the game autosaves to
`saves/autosave.ron` at every turn end. `--save FILE` and `--load FILE` do the same from the
command line. Before each turn end the client puts more citizens on farms if the army would
otherwise go hungry, so the starting troops do not starve unnoticed.

## 10. Adding to the flow

- **A player action:** a `Command` variant, handled in `Game::apply`, with its check as a
  `&self` method the client can also call, and `Event`s for what it changes. Add a test
  that applies the command to a hand-built world.
- **A turn step:** a method called from `process_turn` or `end_turn` at the right place in the
  order above. Keep it deterministic (sorted iteration, `Game::roll`) and tested over several
  turns.
- **A new rejection reason:** extend the relevant error enum rather than inventing a string,
  so the client can match on it.
