# Game flow and systems

How a game runs: the command and event layer, what happens in a turn and in what order,
and how movement, production and the treasury work. For how state is stored see
[`ecs-structure.md`](ecs-structure.md); for the map see [`map-layers.md`](map-layers.md).

Code: the game in `crates/f4x-master/src/game.rs` and `game/`; the world in `crates/f4x-core/src/` (`turn.rs`, `world/command.rs`, `unit/movement.rs`, `unit/pathing.rs`, `city/production.rs`, `player/finance.rs`, `map/vision.rs`).

## 1. The shape of the loop

```
client / AI / console       f4x-master                        f4x-core
─────────────────────       ───────────────────────           ───────────────────────────────
read state with queries ──▶ &Game (reads like a world) ─────▶ &World methods (no change)
send a Command          ──▶ Game::apply(player, command) ───▶ World::apply(command, rng)
let time pass           ──▶ a step of the turn order ───────▶ World::start_player_turn and others
                        ◀──────────────── Vec<Event>  (what happened, or why not)
draw, log, animate from events and queries
```

- **Everything a player does is a `Command`** sent to `Game::apply(player, command)`. The game refuses
  one that is not the sender's to give (`World::may_command`, `Rejection::NotYourCommand`), one that
  only sets a world up (`Command::is_setup`, `Rejection::SetupOnly`), on another player's turn one that
  is no reaction (`Rejection::NotYourTurn`, `turn-structure.md` 10.4) and any command after the end.
- **A command given to the world directly** (`World::apply`: the game itself, setup, tests) carries no
  sender, so **ownership is not checked** there: the caller is trusted.
- **Time is the game's.** The steps of the turn order (section 4) are no commands; no player sends
  them. Setup, tests and cheats also change the world through its setters.
- **`apply` returns events**: facts the caller can show or log. A refused command returns
  `Event::Rejected(reason)`; it does not panic and changes nothing.
- **Queries are read-only, not necessarily cheap**: methods taking `&self` do not
  intentionally change game state, but that says nothing about their computational cost.
  A query such as `find_path`, `visibility` or `city_economy` may do substantial work.
  The client should retain the information it has queried and refresh it when a relevant
  game-state change makes it stale, rather than rerunning every query every frame. Cheap
  checks can still be made on demand; expensive derived information should be cached and
  invalidated or refreshed when its dependencies change.
- **Determinism:** the same saved game, the same commands and the same steps in the same order give the
  same result. All randomness comes from the game's `GameRng`, which it passes to each world operation
  that draws (`turn-structure.md` 10.10); anything that visits entities does so in creation order (each
  unit and city carries a creation number, `Serial`; `hecs` entity ids are reused and are not used for
  ordering).
- **Saves are snapshots** (section 8): the whole game is written out and read back. A replay
  (`f4x_master::Replay`, `game/replay.rs`) is a saved game plus every command and step since; it names
  units and cities by creation number, so it plays back on the loaded game.

## 2. Commands and their events

The commands are the variants of `Command` (`world/command.rs`, each with its doc comment):

| Area | Commands |
|---|---|
| Units and movement | `MoveStack`, `GoTo`, `GiveOrder`, `ClearOrders`, `SetStandby`, `SetBridge`, `ShiftPlane`, `EarthGate`, `EnterSite`, `Meld`, `Purify`, `BuildRoad`, `DismissUnit`, `SettleOutpost` |
| Battles | `BeginBattle`, `AutoResolve`, `Defend`, `Battle` (an order in the battle being fought) |
| Cities | `SetProduction`, `BuyProduction`, `SellBuilding`, `SetFarmers`, `BalanceFarmers`, `SetAutoBuild`, `SetGrandVizier`, `RenameCity`, `RazeCity` |
| Treasury and shortages | `SetTaxRate`, `Alchemy`, `ReleaseTroops`, `ReleaseUpkeep` |
| Magic | `SetPowerSplit`, `SetResearch`, `CastSpell`, `ResolveCast`, `CancelCast`, `TurnOffSpell`, `TurnOffCitySpell`, `TurnOffUnitSpell` |
| Items and offers | `CraftItem`, `MoveItem`, `ScrapItem`, `SellItem`, `AcceptOffer`, `RejectOffer` |
| Other players and the end | `Diplomacy` (every proposal, answer, gift, treaty and war), `Resign` |
| Setup only, refused from a player | `CreateUnit`, `FoundCity`, `PlaceFortress` |
| Wrapper | `Anyway` (a command sent again after the player was told it would do nothing) |

- **Events** (`world/event.rs`) report what happened. Not every state change has one: a unit a city
  builds appears through `ProductionCompleted`, not `UnitCreated`, and a change to farmers or tax has
  none because the sender already knows.
- **Rejections** are typed: `Rejection` (`world/event.rs`) wraps the error enum of each feature
  (`MoveError`, `ProductionError` and the others), and each has a text key for the player.

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
| Flying, or in a stack carried by a wind-walker; walking on water, onto water | 1 on any tile |
| Sailing | 1 on ocean or on a city of its owner; otherwise impossible |
| Swimming | 1 on ocean; on land as walking |
| A land unit onto water | nothing when a ship of the stack carries it; 1 to board a ship of its owner waiting there; otherwise impossible |
| Walking | terrain `move_cost`, at least 1; impossible on non-walkable terrain |
| Walking, between two road tiles | 0.5; nothing when both have enchanted roads (every road on Myrror is one) |
| Walking, between two river tiles, with the setting `river_roads` | at most 0.5 |
| Pathfinding ability | at most 0.5 |
| Forester ability, onto forest | at most 1 |
| Mountaineer ability, onto hills or mountains | at most 1 |
| A diagonal step (both x and y change), with the setting `diagonal_extra_cost` (off by default) | one and a half times the cost above, rounded up to a half (1.5 on open ground) |

- **A stack** is units of one owner on one tile. At most `max_tile` units (a setting; 9 as shipped)
  may walk onto a square, twice that onto a tower. All units in a move must be on the same tile and
  have moves left (passengers of a moving ship need none; a stack with a Wind Walker moves as far as
  the Wind Walker can); each pays its own cost; the move fails if any unit cannot enter the tile, or
  if a city's ward bars one of them.
- **Hostile tiles.** If the target holds units of another player, nothing moves and the core emits
  `AttackRequested { attackers, from, to, defenders }`. The sender then sends `BeginBattle` or
  `AutoResolve` (`combat.md`); an attack on the human is held until it answers (`turn-structure.md`
  10.4). Another player's city with no unit in it is walked into and taken (the monsters wreck it and
  stay outside). A move against a player one has a treaty with is refused (`MoveError::Treaty`).
- **Guarded sites.** A move onto a lair, node or tower with guards moves nothing and reports
  `SiteEncounter`; `EnterSite` assaults the guards.
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
  move, which must be free of hostile units and have room; an Astral Gate does not lead into a city.
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
- A **standing order** (`unit::Order`, `unit/order.rs`) is kept in its owner's army ledger
  (`ecs-structure.md` 3.12). `GoTo` gives every unit of the stack a walk; `GiveOrder` one of the
  errands below, to units that stand on one square; `BuildRoad` the road work. A new order takes
  the place of what the units were told, standby included.

| Order | What the units do | Over when |
|---|---|---|
| Go to | walk to the square | they arrive |
| Scout | walk to the square that shows the most new ground (`unit/scouting.rs`), then to the next, keeping away from the patches the owner's other scouts walk to; a ship carries its passengers along, and their spent moves do not hold it up | nothing in reach shows anything new |
| Meld | walk to a magic node their owner knows, and meld | melded, or the node may no longer be melded with |
| Settle | walk to an explored square fit for a city, and found it (the name given, else one of the race's) | founded, or no city may be founded there |
| Reinforce | walk to a city of their owner | they arrive, or the city is no longer theirs |
| Escort | walk with the charge as one stack, at the pace of the slower; follow it when they are left behind; wait when they stand with it or its square is full | the charge is gone or changes sides |
| Build road | work along a path, at the start of each turn | the path is done |

- An order moves its units **as it is given**, as far as moves allow, and then at the **end** of
  their owner's turn, with the moves the turn left them (section 4.2, step 2), so the player can
  change an order before it moves anything; road work goes on at the turn start (4.1). Units on the
  same square with identical orders walk together, escorts after everyone else, and a group whose
  next square is full waits for the others to move first. A scout chooses a new patch for each step
  it makes at most, and a spent one chooses when it can walk again. A way that no longer starts
  where the units stand is found again.
- An order **never starts a fight**: a step that would attack units or a city, or enter a guarded
  site, is refused (`WouldFight`). The one exception is a Go to as it is given, which is the
  player's own move: it asks for the attack (`AttackRequested`) or the assault (`SiteEncounter`),
  and the order ends. An order ends when a step is refused for any reason other than running out of
  moves (`OrderFailed`), or because it cannot be done any more (`OrderOver`: target gone, no way,
  nothing left).
- **Enemies in sight.** Scout, Meld, Settle and Build road end (`OrderOver`) when one of the units
  walking together sees an enemy, a monster or a unit of a player at war with the owner, that was
  not in view when the order was given; giving the order again carries on. Go to, Reinforce and
  Escort walk on. Decided 27 November 2026. As with Patrol, only the unit's own sight counts, and
  the check follows every change of the world and every step of a walk.
- Moving a unit by hand (`MoveStack`, `ShiftPlane`, `EarthGate`) ends its order, except an
  escort's whose charge is moved with it; a ship carrying it off, a patrol, purifying, a bridge post
  and leaving or changing sides end it too. A refused move changes nothing. `ClearOrders` ends it.
- A player gives orders with the unit window's **Order** button (`f4x-client`, `order_menu.rs`),
  which opens a menu of the orders the selected units can take: **Go to**, **Scout**, **Meld** (a
  unit that melds), **Settle** (settlers), **Build road** (a builder; opens the road plan),
  **Escort** (when there is another unit to go with), **Reinforce** (when there is a city),
  **Patrol**, and **Cancel orders** when the units have been told something. Go to, Meld, Settle,
  Escort and Reinforce then take a click on the map (Escape or a right-click gives up); where
  several units could be escorted the client asks which. A click on the map (the preference `Left
  Click Moves Units`, else a right-click) sends a Go to as well.
- A click on a unit's picture in the unit window wakes it, if the click leaves it selected:
  the client ends its order (`ClearOrders`) and its standby, whether it has moves left or not.
  Picking the stack on the map wakes no one.
- A unit set aside with `SetStandby` is not asked for orders: `Done` (the button and its key)
  until its owner's next turn starts, which leaves a standing order and a patrol as they
  are; `Patrol` until it moves one step (`MoveStack`) or another player's unit, whoever that
  player is, comes into its sight. As with an order, the units in view when Patrol is set (to any
  of the units it is set for together) are ignored, and setting it again takes those in view then
  (decided 27 November 2026). Its own sight counts (`unit_sight`), not what its owner's other units
  and cities see, and an invisible unit only when the owner can make it out. The check follows every
  change of the world and every step of a walk.

## 4. The turn

A game runs in **rounds**; in each round every player takes one **turn**, in a fixed order (the
wizards, the human first, then each independent race player, then the monsters). The game
(`f4x_master::Game`) holds the round counter and the order and makes four kinds of **step**
(`game/step.rs`), each calling one operation of the world (`turn.rs`); each step is written into a
replay and ends the undo history. The master runs them in order and asks the controller of each player
(a human or an AI) to act in between (`docs/turn-structure.md` has the design and the cadence table,
section 7.1):

| Phase | Step of the game | What the world does |
|---|---|---|
| Round start | `Game::start_round` | the round counter goes up (the game's); `World::start_round(round)`: wars age, `TurnStarted`, world events (`process_events`), threats (`process_threats`) |
| Turn start | `Game::start_player_turn(player)` | what changes the land and the people, then the player's units become ready (4.1) |
| Turn end | `Game::end_player_turn(player)` | the player's income, bills and production (4.2) |
| Round end | `Game::end_round` | world-wide processes, once (4.3) |

`Game::play_round` is the change of rounds at once, for tests and tools: every player ends their
turn, the round ends, the next begins, and every player starts theirs.

The order follows the original's turn change, which is why income, bills and production are the
turn **end** (a defender finished now defends against the rivals' turns, and a shortage costs its
units before the rivals can attack) and growth is the next turn **start**.

### 4.1 `World::start_player_turn(player, round)`

1. Reopen the player's city sales; process stasis.
2. Except in the first round: learn the research that finished and choose the next spell
   (`process_research`); Gaia's Blessing and Chaos Rift (terrain and buildings change before
   food is counted); outposts, population growth and pestilence of the player's cities
   (`grow_city`); unit experience and healing. While Time Stop lasts, only experience and healing.
3. Refill each unit's moves (units in stasis, and units not owned by the Time Stop caster,
   receive zero); clear completed standby. A unit finished at the last turn end moves now.
4. Roll the player's offers; purify work; ships in port unload, marooned ships sail and stranded
   units drown; road work.

### 4.2 `World::end_player_turn(player)`

1. Proposals made in the turn and not answered lapse.
2. The units with a standing order walk on, in unit creation order (section 3.2).
3. Close the player's raze windows.
4. Raise farmers to the minimum needed to feed each inhabited city.
5. Settle the treasury (section 5): gold and food income, then food and gold upkeep.
6. `process_magic`: mana income, research points, mana upkeep, casting progress and the
   resolution of a finished spell.
7. Production: each city checks that its item is possible, refreshes autobuild and applies its
   production (a building or unit may complete).

The monsters' owner has units only and takes no part in 4 to 7; at its turn start its units only gain
experience and heal, and its lairs heal. While Time Stop lasts, the other players take no part in 4
to 7 (and their units have no moves to walk with), and its keeper pays no gold or food (5) and builds
nothing (7).

### 4.3 `World::end_round()`

Global enchantments (Herb Mastery healing, vision) and world enchantments (Great Wasting,
Armageddon, Meteor Storm) run; volcanoes cool and corruption fades; history is recorded; diplomacy
advances. These run once per round however many players there are.

Combat is not automatically invoked by the turn pipeline when movement encounters a hostile
tile: movement emits `AttackRequested`, and the caller/client must handle that request.
Likewise, `Event` values are returned to the caller for presentation or further orchestration;
they are not callbacks into the rules engine.

## 5. The treasury

`World::budget(player)` previews what the next turn end will do; the turn end applies the same
numbers.

**Income**, from the player's cities as they stand at the turn end, before production changes them:
- gold of each city (taxes, trade, mines, percentage buildings) minus its building upkeep;
- plus ten gold for each noble hero;
- plus **spare food sold at two for one**: each city's food beyond what its citizens eat, summed,
  less the army's food need, halved, never below zero.

**Then upkeep:**
- **Food.** A unit eats the food its definition names. If the army needs more than the cities have
  spare, units that eat are let go until it fits: the highest food first, then the cheapest to make,
  then the newest; heroes and loaded ships last.
- **Gold.** Units cost their gold upkeep (a noble hero costs none); the player's fame pays part of the
  total, never below zero. Gold cannot go below zero: if the upkeep exceeds the treasury (income
  already added), units that cost gold are let go until it fits, in the same order by gold upkeep,
  then the upkeep is paid.
- A wizard may choose beforehand whom to let go (`ReleaseTroops`); the order above is what happens
  otherwise.
- Each unit let go is reported (`UnitLost`), with `Shortage` for what ran out and one
  `UnitsDisbanded { player, units }` for the count. A hero returns to the roster and its items to the
  armory; a ship with passengers aboard at sea turns neutral and sails for the shore.

Mana and research are handled by `process_magic` (section 4.2). The command `BalanceFarmers` sets farmers so the
cities also feed the army; the AI sends it every turn, and the client when food is short at the end of a turn
or when asked.

**A change of a city's citizens, up or down, readjusts its farmers** at once (the user, 28 November 2026, task
841): they become no more than the citizens and no fewer than it takes to feed them; a number within those
stays. It holds for growth, starvation and pestilence, a settler, world events, spells, fighting in the city,
rampaging monsters and cheats alike.

## 6. Healing

Each damaged unit of a player heals at the start of the player's turn (`unit/healing.rs`), except
Death creatures (born so or made undead, `unit/undead.rs`). A unit with regeneration, and every unit
in a city under Stream of Life, heals completely. Otherwise the base is a fraction of the total hit
points: one twentieth in the field, one tenth in a city of its owner, one sixth in such a city if it
has an animist's guild. The remainder is settled by a seeded roll so the average is exact. Every unit
in a stack that contains a unit with the `healer` ability heals a further fifth of its total hit
points. A unit that heals this way heals at least one point. While Time Stop lasts only its wizard's
units heal.

## 7. Production in detail

- **Items:** a building, a unit, trade goods (turn production into gold, two for one), or
  housing (faster growth). A new city starts on housing, a captured one on trade goods. A city
  whose item it can no longer build falls back to trade goods (`ProductionImpossible`).
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
  fantastic unit (boats, siege) is open to all; both need their required buildings. Ships, and
  buildings marked coastal, need a coastal city; a unit that costs a citizen (settlers, unless
  the setting says otherwise) needs two. Heroes and summoned units cannot be built.
- **Cost:** a building's cost; a unit's cost reduced by the city's mines (never below half).
- **Buying:** four times what remains if nothing is done yet; three times if less than half of
  the remaining amount is done; otherwise twice. Gold is paid at once; the item completes at
  the end of the owner's turn.
- **Selling:** a third of the construction cost (city walls a sixth, at least 1), one sale per city per turn; refused if another built building requires it; selling what the current production needs puts the city back on Housing.
- **Replaced buildings:** a building can replace another (an armory replaces barracks). The
  old one stays built and keeps its effects and its value as a requirement, but costs no
  upkeep.

## 8. Saving and loading

`World::snapshot()` captures a whole world as a `Snapshot`; `Snapshot::restore(content)` builds
the world again. A saved game (`f4x_master::SaveFile`, from `Game::save_file`) holds the game's
own state, that snapshot and the computer players' plans; `f4x_master::{save_game, load_game}` do
the file work. Code: `f4x-core/src/world/save.rs`, `world/save/file.rs`,
`f4x-master/src/game/save.rs`.

**What a save holds:** in the game's part, the seed and how many random numbers have been drawn (so
the random stream resumes exactly), the round, the order of the players, the place in the round, the
open reactions and held attacks, and the end of the game. In the world's part (`Snapshot`), the
settings, the creation counter, the map, each player's explored area (bit-packed), the players and
wizards, heroes, units (with moves, damage, experience and any standing order), cities, features,
items, spells, diplomacy, world events, threats and a battle in progress.

**Where the game is in its round:** the optional `phase` of the game, written by `f4x-master` as it runs and cleared by every step of the turn order, says what comes next and whose turn it is; a save without it was made on the human's turn. The client's autosave is made at the start of a round. What each AI has in mind (the `plans`) is saved beside the game.

**What it does not hold:** content definitions, events, the command log, or the undo history. A save is a game
state, not a content pack.

**References by number.** Everything that points at content (a unit's definition, a building, a
terrain, a feature, a race, a spell) is saved as its number in the content and looked up again on
load (`ecs-structure.md` 3.9). A save that needs a number the content does not have is refused with a
message naming it (`LoadError::UnknownId`), never loaded wrongly. The save also carries a fingerprint
of the content files; other content is refused unless the player chooses to load it.

**Same game after loading.** Units and cities are written in creation order with their creation
numbers, so a loaded game orders everything as the saved one did and plays on as it would have
(`f4x-master/tests/it/persist.rs`).

**Checks on load:** the format and game version; settings in range; every content number resolves;
positions are on the map; creation numbers are unique; owners exist; population figures are possible.
Failures are `LoadError::{Parse, Version, GameVersion, UnknownId, Inconsistent, Content}` and leave
the running game untouched.

**Files.** The text format is RON, readable and diff-able: about 80 KB for a new game of 3 wizards
on a 60x40 map, growing with the game. The entities are written one field to a line,
what is inside a field on the same line, and numbers in a list on one line (`save.rs`, `to_text`); the explored
flags are one short text for each player and plane, hex bytes and runs like `00*300` (`Bits`); a unit's fields at their default are left out. Older
saves with the long form still load. `save_game` writes to `<file>.tmp` and renames it, so an interrupted save never
damages an existing one. Saves live in `saves/` (gitignored). What the player chose (preferences, default game settings, hotkeys, language) lives in `settings/` (gitignored), not in `saves/`; files an older version kept in `saves/` are moved over at the first start (`player_files.rs`).

**Versions.** `SAVE_VERSION` is bumped when the layout changes incompatibly. Adding optional
fields does not need a bump. Version 4 saves are converted to version 5 when they are read
(`f4x-master/src/convert.rs`, to be retired by task 823). Version 3 saves (names for spells, items
and powers) are refused with the version message.

**Game version.** The game has one version, the workspace `version` in `Cargo.toml` (now 0.0.8),
read as `save::GAME_VERSION`. Every save records it in `game_version` and in the header
comment (`// f4x save, version 5, game 0.0.8`). A load reads a save or replay when `SAVE_VERSION`
matches and its game version lies from `save::OLDEST_COMPATIBLE_VERSION` up to `GAME_VERSION`
(compared as numbers); the converter reads the older ones, replays excepted. Raise
`OLDEST_COMPATIBLE_VERSION` when a change makes older files unreadable; leave it when they still
load. A newer or unparsable version is refused. Bump the game version in `Cargo.toml` for each
release.

## 9. What the client does with this

The macroquad client (`f4x-client`) runs this loop each frame: read input, turn it into a
`Command`, send it to the game (`Game::apply`), add any events to its message log, then draw from queries. It never
touches the `hecs::World` directly except to read. `--shot FILE` renders a single frame, after
`--turns N` turns have been ended if asked, which is how the flow is checked without a person.

Keys: F10 saves to `saves/quick.ron`, Ctrl-L loads it (both can be changed, `hotkeys.ron`). The game
autosaves to `saves/autosave.ron` at the start of every round and when the player leaves it for the
menu. `--save FILE` and `--load FILE` do the same from the
command line. Before each turn end the client puts more citizens on farms if the army would
otherwise go hungry, so the starting troops do not starve unnoticed.

## 10. Adding to the flow

- **A player action:** a `Command` variant, handled in `World::apply`, with its check as a
  `&self` method the client can also call, and `Event`s for what it changes. Say who may send it
  (`World::command_actor`, and `game/reaction.rs` if it answers another player). Add a test in core
  that applies the command to a fresh world.
- **Turn work:** a world method called from one of the turn operations (`turn.rs`) at the right
  place in the order above. Keep it deterministic (sorted iteration, the `GameRng` passed in). Test
  the operation in core, and what needs several turns in `f4x-master`.
- **A new rejection reason:** extend the relevant error enum rather than inventing a string,
  so the client can match on it.
