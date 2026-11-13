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
- **Queries are read-only, not necessarily cheap**: methods taking `&self` do not
  intentionally change game state, but that says nothing about their computational cost.
  A query such as `find_path`, `visibility` or `city_economy` may do substantial work.
  The client should retain the information it has queried and refresh it when a relevant
  game-state change makes it stale, rather than rerunning every query every frame. Cheap
  checks can still be made on demand; expensive derived information should be cached and
  invalidated or refreshed when its dependencies change.
- **Determinism:** the same starting `Game`, the same seed and the same commands in the same
  order give the same result. All randomness goes through `Game::roll`; anything that
  visits entities does so in creation order (each unit and city carries a creation number,
  `Serial`; `hecs` entity ids are reused and are not used for ordering).
- **Saves are snapshots** (section 8): the whole game is written out and read back. Within one
  run the command log still reproduces a game from its seed; commands carry entity ids, so a
  log cannot be replayed across a load (the loaded entities have new ids).

The core has per-player lifecycle phases (section 4) but does not decide who acts when:
`f4x-master` runs the rounds and seats and checks a command against the active player
(`Game::command_actor`). A command called directly on the core does not carry an acting
player, so **ownership is not checked** there: the caller is trusted. (Needed before any
networked play.)

## 2. Commands and their events

| Command | What it does | Events on success | Typical rejections |
|---|---|---|---|
| `StartRound`, `StartPlayerTurn`, `EndPlayerTurn`, `EndRound` | The lifecycle phases (section 4) | `TurnStarted` (round start), then everything the phase produced | none |
| `EndTurn` | All four phases for every player (compatibility, section 4) | as the phases | none |
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
| Non-corporeal (a ship made so too, which then also crosses land), onto any tile, water included | 1 |
| Flying, or in a stack carried by a wind-walker | 2 on any tile, water included |
| Sailing | 2 on ocean or on a city of its owner; otherwise impossible |
| Swimming | 2 on ocean; on land as walking |
| Walking | terrain `move_cost` x 2; impossible on ocean or non-walkable terrain |
| Walking, between two road tiles | 1 |
| Pathfinding ability | at most 1 |
| A diagonal step (both x and y change), with the setting `diagonal_extra_cost` (off by default) | one and a half times the cost above, rounded up (3 on open ground) |
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
- **Exploration.** After each move the owner explores around the new tile as far as the unit
  sees. `Sight` is a distance in squares counted like movement (a step along a row or column costs 1,
  a diagonal step 1.5 with the setting `diagonal_extra_cost` and 1 without it, the same setting as
  movement; a square is in sight when its cost is at most the sight): sight 1.5 is the tile and the
  eight round it, sight 2.5 a 5 by 5 block, without its corners when a diagonal step costs extra (the
  original's scouting levels 1 and 2). A unit has at least 1.5, a flier or wind walker at least 2.5; a city 2.5.
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

A game runs in **rounds**; in each round every player takes one **turn**, in a fixed order (seat
order, the human first, the neutral seat last: independent cities and monsters). The core
offers four lifecycle phases, recorded `Command`s like any other, and `f4x-master` runs them in
order and asks each seat (human, AI, neutrals) to act in between (`docs/turn-structure.md` has
the design and the cadence table, section 7.1):

| Phase | Command | What it does |
|---|---|---|
| Round start | `StartRound` | `Game.turn` advances, `TurnStarted`, world events (`process_events`), threats (`process_threats`) |
| Turn start | `StartPlayerTurn { player }` | what changes the land and the people, then the player's units become ready (4.1) |
| Turn end | `EndPlayerTurn { player }` | the player's income, bills and production (4.2) |
| Round end | `EndRound` | durations and world-wide processes, once (4.3) |

`Command::EndTurn` stays as a composition for tests and tools: `EndPlayerTurn` for every player,
`EndRound`, `StartRound`, `StartPlayerTurn` for every player.

The order follows the original's turn change, which is why income, bills and production are the
turn **end** (a defender finished now defends against the rivals' turns, and a shortage costs its
units before the rivals can attack) and growth is the next turn **start**.

### 4.1 `Game::start_player_turn(player)`

1. Reopen the player's city sales; process stasis.
2. Except in the first round: learn the research that finished and choose the next spell
   (`process_research`); Gaia's Blessing and Chaos Rift (terrain and buildings change before
   food is counted); outposts, population growth and pestilence of the player's cities
   (`grow_city`); unit experience and healing.
3. Refill each unit's moves (units in stasis, and units not owned by the Time Stop caster,
   receive zero); clear completed standby. A unit finished at the last turn end moves now.
4. Roll the player's offers; purify work; marooned ships sail and stranded units drown; road
   work; follow standing orders in unit creation order.

### 4.2 `Game::end_player_turn(player)`

1. Close the player's raze windows.
2. Raise farmers to the minimum needed to feed each inhabited city.
3. Settle the treasury (section 5): gold and food income, then food and gold upkeep. A wizard
   under Time Stop is skipped.
4. `process_magic`: mana income, research points, mana upkeep, casting progress and the
   resolution of a finished spell.
5. Production: each city checks that its item is possible, refreshes autobuild and applies its
   production (a building or unit may complete).

The monsters' owner has units only and takes no part in 2 to 5.

### 4.3 `Game::end_round()`

Temporary effects count down; global enchantments (Herb Mastery healing, vision) and world
enchantments (Great Wasting, Armageddon, Meteor Storm) run; volcanoes cool and corruption fades;
history is recorded; diplomacy timers advance. These run once per round however many players
there are.

Combat is not automatically invoked by the turn pipeline when movement encounters a hostile
tile: movement emits `AttackRequested`, and the caller/client must handle that request.
Likewise, `Event` values are returned to the caller for presentation or further orchestration;
they are not callbacks into the rules engine.

## 5. The treasury

`Game::treasury(player)` previews what the next turn end will do; the turn end applies the same
numbers.

**Income**, from the player's cities as they stand at the turn end, before production changes them:
- gold of each city (taxes, trade, mines, percentage buildings) minus its building upkeep;
- plus ten gold per noble hero (none exist yet);
- plus **spare food sold at two for one**: each city's food beyond what its citizens eat,
  halved, summed, minus half the army's food need, never below zero.

**Then upkeep:**
- **Food.** Each normal unit eats one food. If the army needs more than the cities have
  spare, units that eat are disbanded until it fits: the highest food first, then the cheapest
  to make, then the newest; heroes and loaded ships last.
- **Gold.** Normal units and heroes cost their gold upkeep (a noble hero costs none), reduced
  by the player's fame, never below zero. Gold cannot go below zero: if the upkeep exceeds the
  treasury (income already added), units that cost gold are disbanded until it fits, in the same
  order by gold upkeep, then the upkeep is paid.
- Disbanded units are removed and one `UnitsDisbanded { player, units }` is emitted.

Mana and research are handled by `process_magic` (section 4.2). `balance_farmers(player)` is a helper, used by
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
the game again. `f4x_core::game::save::{save_game, load_game}` do the file work. Code:
`f4x-core/src/game/save.rs`, `game/save/file.rs`.

**What a save holds:** the seed and how many random numbers have been drawn (so the random
stream resumes exactly), the turn, the creation counter, the map (terrain, features, roads),
each player's explored area (bit-packed), the players, every unit (with moves, damage,
experience and any standing order) and every city (population, farmers, buildings,
enchantments, race, current item and progress).

**Where the game is in its round:** the optional `flow` (`Game::flow`), written by `f4x-master` after every step and cleared by the lifecycle commands, says which phase and seat come next; a save without it was made on the human's turn. The client's autosave is made at the start of a round. What each AI has in mind (`Game::plans`) is saved with it.

**What it does not hold:** content definitions, events, the command log, or the AI's own memory (targets, campaigns). A save is a game
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

**Files.** The text format is RON, readable and diff-able: about 90 KB for a new game of 3 wizards
on a 60x40 map, 430 KB at turn 400 of a 5-wizard game. The entities are written one field to a line,
what is inside a field on the same line, and numbers in a list on one line (`save.rs`, `to_text`); the explored
flags are one hex string for each player and plane (`Bits`); a unit's fields at their default are left out. Older
saves with the long form still load. `save_game` writes to `<file>.tmp` and renames it, so an interrupted save never
damages an existing one. Saves live in `saves/` (gitignored).

**Versions.** `SAVE_VERSION` is bumped when the layout changes incompatibly. Adding optional
fields does not need a bump. Migrating older versions would be added as a conversion step
before `restore`. Version 3 saves (names for spells, items and powers) are refused with the version message; the converter is task 462.

**Game version.** The game has one version, the workspace `version` in `Cargo.toml` (now 0.0.6),
read as `save::GAME_VERSION`. Every save records it in `Snapshot.game_version` and in the header
comment (`// f4x save, version 4, game 0.0.6`). A load reads a save or replay when `SAVE_VERSION`
matches and its game version lies from `save::OLDEST_COMPATIBLE_VERSION` up to `GAME_VERSION`
(compared as numbers). Raise `OLDEST_COMPATIBLE_VERSION` when a change makes older files unreadable;
leave it when they still load. A newer or unparsable version is refused. An older save is read as it is, then each entry of `save::UPGRADES` newer than its game version converts the numbers whose meaning changed. Bump the game version in
`Cargo.toml` for each release.

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
