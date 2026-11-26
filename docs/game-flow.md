# Game flow and systems

How a game runs: the command and event layer, what happens in a turn and in what order,
and how movement, production and the treasury work. For how state is stored see
[`ecs-structure.md`](ecs-structure.md); for the map see [`map-layers.md`](map-layers.md).

Code: `crates/f4x-core/src/{lib,turn,movement,pathing,production,economy,units,vision}.rs`.

## 1. The shape of the loop

```
client / AI / test                         f4x-core
─────────────────                          ───────────────────────────────
read state with queries  ────────────────▶ &World methods (no change)
send a Command           ────────────────▶ World::apply(command)
                         ◀──────────────── Vec<Event>  (what happened, or why not)
draw, log, animate from events and queries
```

- **Everything that changes the game is a `Command`** passed to `World::apply`. There is no
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
- **Determinism:** the same starting `World`, the same seed and the same commands in the same
  order give the same result. All randomness goes through `World::roll`; anything that
  visits entities does so in creation order (each unit and city carries a creation number,
  `Serial`; `hecs` entity ids are reused and are not used for ordering).
- **Saves are snapshots** (section 8): the whole game is written out and read back. Within one
  run the command log still reproduces a game from its seed; commands carry entity ids, so a
  log cannot be replayed across a load (the loaded entities have new ids).

The core has per-player lifecycle operations (section 4) but does not decide who acts when:
`f4x-master` runs the rounds and seats and checks a command against the active player
(`World::command_actor`). A command called directly on the core does not carry an acting
player, so **ownership is not checked** there: the caller is trusted. (Needed before any
networked play.)

## 2. Commands and their events

| Command | What it does | Events on success | Typical rejections |
|---|---|---|---|
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
| `BalanceFarmers { player }` | Just enough citizens on farms in all the player's cities to feed its citizens and army | none | none |
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
- **Moves are `HalfInt` amounts.** A unit's moves, its moves left and the cost of a step are
  counted in moves, with halves (1, 1.5, a road half a move): no doubled counts, in the rules, the
  battle map or the pathfinder (`path_grid.rs`). Moves are refilled at turn start.
- **A step is always allowed with any moves left**, even if it costs more than remains; the
  unit ends the turn with none. A unit with none cannot move (`NoMovesLeft`).
- **Cost of entering a tile** (`step_cost`, in moves):

| Mover | Cost |
|---|---|
| Non-corporeal (a ship made so too, which then also crosses land), onto any tile, water included | 0.5 |
| Flying, or in a stack carried by a wind-walker | 1 on any tile, water included |
| Sailing | 1 on ocean or on a city of its owner; otherwise impossible |
| Swimming | 1 on ocean; on land as walking |
| Walking | terrain `move_cost`; impossible on ocean or non-walkable terrain |
| Walking, between two road tiles | 0.5 |
| Pathfinding ability | at most 0.5 |
| A diagonal step (both x and y change), with the setting `diagonal_extra_cost` (off by default) | one and a half times the cost above, rounded up to a half (1.5 on open ground) |
| Forester ability, onto forest | at most 1 |
| Mountaineer ability, onto hills or mountains | at most 1 |

- **A stack** is units on one tile of one owner, at most nine per tile. All units in a move
  must be on the same tile and have moves left; each pays its own cost; the move fails if any
  unit cannot enter the tile.
- **Hostile tiles.** If the target holds units of another player, or another player's city,
  nothing moves and the core emits `AttackRequested { attackers, from, to, defenders }`.
  Combat resolution is not built yet; the client logs it.
- **Plane shift.** Allowed on a tower tile, or for a stack whose every unit has planar
  travel, or at an Astral Gate. A stack in a tower is on both planes at once (the original's
  `in_tower`, `Units_In_Tower`, `Player_Army_At_Square` / `Enemy_Army_At_Square`): the tower is a
  starting square on either plane, so a move from it to a tile of the other plane is an ordinary
  move with an ordinary cost (`World::start_from_tower_on`; the Plane button only changes which
  plane the stack is shown on, for nothing); a rival stepping onto the tower square from either
  plane meets the stack and attacks it (`Economy::tower_mirror`, `army_at`); the client draws and
  selects the stack on both planes. The room on the square is
  counted over both planes. Under a Planar Seal none of this holds: the tower is cut off. Planar
  travel and an Astral Gate still move the stack to the same tile of the other plane for one full
  move, which must be free of hostile units and have room.
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
offers four lifecycle operations, and the game (`f4x_master::Game`) makes them as **steps**:
no commands, no player sends them, but each is written into a replay and ends the undo history.
The master runs them in order and asks each seat (human, AI, neutrals) to act in between
(`docs/turn-structure.md` has the design and the cadence table, section 7.1):

| Phase | Step of the game | What it does |
|---|---|---|
| Round start | `start_round` | `World.turn` advances, `TurnStarted`, world events (`process_events`), threats (`process_threats`) |
| Turn start | `start_player_turn(player)` | what changes the land and the people, then the player's units become ready (4.1) |
| Turn end | `end_player_turn(player)` | the player's income, bills and production (4.2) |
| Round end | `end_round` | durations and world-wide processes, once (4.3) |

`Game::play_round` is the change of rounds at once, for tests and tools: every player ends their
turn, the round ends, the next begins, and every player starts theirs.

The order follows the original's turn change, which is why income, bills and production are the
turn **end** (a defender finished now defends against the rivals' turns, and a shortage costs its
units before the rivals can attack) and growth is the next turn **start**.

### 4.1 `World::start_player_turn(player)`

1. Reopen the player's city sales; process stasis.
2. Except in the first round: learn the research that finished and choose the next spell
   (`process_research`); Gaia's Blessing and Chaos Rift (terrain and buildings change before
   food is counted); outposts, population growth and pestilence of the player's cities
   (`grow_city`); unit experience and healing.
3. Refill each unit's moves (units in stasis, and units not owned by the Time Stop caster,
   receive zero); clear completed standby. A unit finished at the last turn end moves now.
4. Roll the player's offers; purify work; marooned ships sail and stranded units drown; road
   work; follow standing orders in unit creation order.

### 4.2 `World::end_player_turn(player)`

1. Close the player's raze windows.
2. Raise farmers to the minimum needed to feed each inhabited city.
3. Settle the treasury (section 5): gold and food income, then food and gold upkeep. A wizard
   under Time Stop is skipped.
4. `process_magic`: mana income, research points, mana upkeep, casting progress and the
   resolution of a finished spell.
5. Production: each city checks that its item is possible, refreshes autobuild and applies its
   production (a building or unit may complete).

The monsters' owner has units only and takes no part in 2 to 5.

### 4.3 `World::end_round()`

Temporary effects count down; global enchantments (Herb Mastery healing, vision) and world
enchantments (Great Wasting, Armageddon, Meteor Storm) run; volcanoes cool and corruption fades;
history is recorded; diplomacy timers advance. These run once per round however many players
there are.

Combat is not automatically invoked by the turn pipeline when movement encounters a hostile
tile: movement emits `AttackRequested`, and the caller/client must handle that request.
Likewise, `Event` values are returned to the caller for presentation or further orchestration;
they are not callbacks into the rules engine.

## 5. The treasury

`World::treasury(player)` previews what the next turn end will do; the turn end applies the same
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

Mana and research are handled by `process_magic` (section 4.2). The command `BalanceFarmers` sets farmers so the
cities also feed the army; the AI sends it every turn, and the client when food is short at the end of a turn
or when asked.

## 6. Healing

Each unit with damage heals once per turn, except Death creatures (born so or made undead, `unit/undead.rs`) and units under Black Channels. The base
is a fraction of its total hit points: one twentieth in the field, one tenth in a city of
its owner, one sixth in such a city if it has an animist's guild. The remainder is settled by
a seeded roll so the average is exact. Every unit in a stack that contains a unit with the
`healer` ability additionally heals a fifth of its total hit points. A unit heals at least
one point.

## 7. Production in detail

- **Items:** a building, a unit, trade goods (turn production into gold, two for one), or
  housing (faster growth). New cities start on housing.
- **A finished item ends the production** (rule: `design.md`, "Economy and movement"). The city
  then has no item (`None`) until its owner chooses again: nothing queues the same item. Trade
  goods and housing never finish; housing ends when the city's figures reach its maximum size
  (`ProductionCompleted` with the housing). A city with an auto-builder plan (the human's
  helper) gets the plan's next item at once. In the client, each of the human's cities that
  finished something has its city screen opened at the start of the turn, after the boxes, with
  the message "<city> has finished <item>" over it (`finished_city.rs`; the original shows the
  box over the map first, this game over the city screen). A city that has an item again by then
  (the auto-builder) does not open: a building is announced over the map, a unit only in the log. The switch "Open Finished Cities" on the Settings screen (`client_settings.ron`: `open_finished_cities`, default on) turns it off.
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

`World::snapshot()` captures a whole game as a `Snapshot`; `Snapshot::restore(content)` builds
the game again. `f4x_core::world::save::{save_game, load_game}` do the file work. Code:
`f4x-core/src/world/save.rs`, `world/save/file.rs`.

**What a save holds:** the seed and how many random numbers have been drawn (so the random
stream resumes exactly), the turn, the creation counter, the map (terrain, features, roads),
each player's explored area (bit-packed), the players, every unit (with moves, damage,
experience and any standing order) and every city (population, farmers, buildings,
enchantments, race, current item and progress).

**Where the game is in its round:** the optional `flow` (`World::flow`), written by `f4x-master` after every step and cleared by every step of the turn order, says which phase and seat come next; a save without it was made on the human's turn. The client's autosave is made at the start of a round. What each AI has in mind (`World::plans`) is saved with it.

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

**Files.** The text format is RON, readable and diff-able: about 78 KB for a new game of 3 wizards
on a 60x40 map, 420 KB at turn 400 of a 5-wizard game. The entities are written one field to a line,
what is inside a field on the same line, and numbers in a list on one line (`save.rs`, `to_text`); the explored
flags are one short text for each player and plane, hex bytes and runs like `00*300` (`Bits`); a unit's fields at their default are left out. Older
saves with the long form still load. `save_game` writes to `<file>.tmp` and renames it, so an interrupted save never
damages an existing one. Saves live in `saves/` (gitignored). What the player chose (preferences, default game settings, hotkeys, language) lives in `settings/` (gitignored), not in `saves/`; files an older version kept in `saves/` are moved over at the first start (`player_files.rs`).

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

- **A player action:** a `Command` variant, handled in `World::apply`, with its check as a
  `&self` method the client can also call, and `Event`s for what it changes. Add a test
  that applies the command to a hand-built world.
- **A turn step:** a method called from `process_turn` or `end_turn` at the right place in the
  order above. Keep it deterministic (sorted iteration, `World::roll`) and tested over several
  turns.
- **A new rejection reason:** extend the relevant error enum rather than inventing a string,
  so the client can match on it.
