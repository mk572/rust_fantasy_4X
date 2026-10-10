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
  visits entities does so in entity-id order.
- **The command log is the save format** (planned): replay the log from a seed to rebuild a
  game. Commands carry entity ids, so replays depend on entities being allocated in the same
  order, which the core guarantees as long as it runs the same commands.

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
| `ShiftPlane { units }` | Same tile, other plane, costs one move | `UnitsMoved` | `Move(CannotShiftPlane)` and others |
| `FoundCity { owner, pos, name, race, population }` | New city (or outpost if population is 0) | `CityFounded` | `FoundCity(...)` |
| `SettleOutpost { unit, name }` | Settlers become an outpost where they stand | `CityFounded` | `FoundCity(...)`, `Move(BadSelection)` |
| `SetProduction { city, item }` | Choose what the city builds | none | `Production(...)` |
| `BuyProduction { city }` | Pay gold so the item completes at the next turn change | none | `Production(NotEnoughGold, NothingToBuy)` |
| `SellBuilding { city, building }` | Sell for its construction cost | none | `Production(NotBuilt, MissingBuilding)` |
| `SetFarmers { city, farmers }` | Citizens on farms, clamped to the minimum that feeds the city and the population | none | none |
| `SetTaxRate { player, rate }` | Tax rate, clamped to the table | none | none |

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
  at least 1).

### 3.2 Routes and standing orders
- `find_path` is A\* on the eight-neighbour grid (wrapping sideways), costing each step as
  the **slowest unit's** cost. Tiles holding hostile units are avoided unless they are the
  destination. `path_turns` estimates turns from the stack's allowance.
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
3. **Cities**, one by one in entity-id order:
   - *Outposts* (no citizens) roll to grow: a chance each turn made of the race's base
     chance, the land's food capacity, bonuses from worked features (mines, game), and
     enchantments; if the roll succeeds the outpost gains a few tens of people, and a small
     chance makes it lose some. At 100 tens it becomes a one-citizen city (farmers are set,
     `CityGrew`). An outpost does nothing else.
   - *Cities*: farmers are raised to the minimum that feeds the citizens; the economy is
     computed **once**; the item in production gets this turn's production added and, if
     covered, completes (a building is added and the city falls back to housing; a unit is
     created in the city with the guilds' starting experience and the same unit is queued
     again; surplus production is lost); then population changes by the growth rate computed
     in the same economy (`CityGrew`/`CityShrank`). Because the economy was computed before
     the changes, a building finished this turn first affects the next one.
4. **Treasury**, per player in the order players were added (section 5).
5. **Healing** of wounded units (section 6).
6. **Standing orders** continue for units that have them, in entity-id order (section 3.2).

Planned additions and where they go: enchantment upkeep and expiry and wizard mana and
research income belong with the treasury step; random events before the cities;
combat resolution wherever an `AttackRequested` is acted on; victory checks last.

## 5. The treasury

`Game::treasury(player)` previews what the next `EndTurn` will do; the turn applies the same
numbers.

**Income**, from the player's cities as they stand after step 3:
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

Each unit with damage heals once per turn, except units with the `undead` ability. The base
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
- **Selling:** refund of the construction cost; refused if another built building requires it.
- **Replaced buildings:** a building can replace another (an armory replaces barracks). The
  old one stays built and keeps its effects and its value as a requirement, but costs no
  upkeep.

## 8. What the client does with this

The macroquad client (`f4x-client`) runs this loop each frame: read input, turn it into a
`Command`, `apply` it, add any events to its message log, then draw from queries. It never
touches the `hecs::World` directly except to read. The `--shot` mode runs a few commands
(`--turns N`) and renders a single frame, which is how the flow is checked without a person.

## 9. Adding to the flow

- **A player action:** a `Command` variant, handled in `Game::apply`, with its check as a
  `&self` method the client can also call, and `Event`s for what it changes. Add a test
  that applies the command to a hand-built world.
- **A turn step:** a method called from `process_turn` or `end_turn` at the right place in the
  order above. Keep it deterministic (sorted iteration, `Game::roll`) and tested over several
  turns.
- **A new rejection reason:** extend the relevant error enum rather than inventing a string,
  so the client can match on it.
