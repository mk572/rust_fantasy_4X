# Turn structure design

Status: design direction for task [477](TASKS.md), the player-turn and round lifecycle, which `f4x-master` and the four lifecycle operations of `f4x-core` carry out (section 7.1). The next step, the split of the game from the world (task 822), is in section 10; where it disagrees with an earlier section, section 10 is newer.

## 1. Goals

Use **sequential player turns**, not a shared phase in which all AI players plan against the same old board and then execute orders in a fixed movement phase.

- Every wizard, human or AI, takes a complete turn in a deterministic order.
- Each player's decisions and actions use the game state left by all earlier players in the round.
- Actions resolve before the next player starts. Movement, attacks and battles must not rely on stale plans made before another player's turn.
- Round-based processes run once at their explicitly assigned boundary, never once per player; world events are processed at round start.
- The new `f4x-master` crate owns round and player-turn orchestration. It sits above `f4x-core` and below the front ends; the client, AI and console provide player controllers to it rather than deciding independently whose turn it is.

This is a gameplay design choice, not an attempt to reproduce the original game's AI scheduling. In particular, do not copy the original pattern of planning for all AI wizards first and then executing all their movement orders.

## 2. Two levels of time

There are two distinct levels:

1. **Round start:** `f4x-master` advances/sets the round counter and runs once-per-round opening work, including world-event processing.
2. **Player turns:** each eligible player, human or AI, is called in deterministic sequence and takes a complete turn.
3. **Round end:** after all eligible players have acted, any processes explicitly assigned to the end-of-round boundary run; the master then begins the next round.

Conceptually:

```text
Round N begins (f4x-master)
  advance/set round counter
  process world events and other round-start work
  Player 1 turn
    player-turn start
    human input or AI decision-making
    execute actions, movement and combat
    player-turn end
  Player 2 turn
    player-turn start
    human input or AI decision-making
    execute actions, movement and combat
    player-turn end
  ...
  Round-end processing (only systems assigned to this boundary)
Round N+1 begins
  advance/set round counter
  process world events and other round-start work
  ...
```

The player sequence is stable and deterministic. It advances to the next eligible player; it does not restart the world pipeline for each player. The policy for the first player of a new round (fixed order, rotating first player, or another explicit rule) must be decided and tested, not left to incidental iteration order.

## 3. Player-turn responsibilities

A player turn is the time in which that player can act. It includes:

- strategic decisions (for an AI, made when its turn is active, using the current board state);
- issuing and resolving that player's commands;
- that player's unit movement and standing orders;
- resolving attacks and battles when actions trigger them;
- city-management decisions and other player-controlled choices;
- spellcasting, research/casting choices and diplomacy actions, subject to the rules for when those actions are available.

The AI must not plan every wizard's turn from a common pre-movement snapshot. When A starts its turn, it observes the state after B's preceding turn, including units that moved, battles that ended, cities that changed hands and other completed actions. If a target has moved or disappeared, A's action is validated and resolved against the current state; an old plan does not guarantee that the target is still there.

Human and AI turns use the same core lifecycle and command validation. The AI differs in how it chooses commands, not in when its turn takes place or which lifecycle rules apply.

## 4. Round-start and round-end responsibilities

Round-start processing runs once before the first player takes a turn in the round. World events belong here: select/advance the event and apply its opening effects before any player acts, so every player encounters the same already-resolved event state. Other once-per-round systems may belong at round start or round end, depending on their rules.

Round-end processing runs once after all eligible players have acted, but only for systems whose intended timing is specifically after the last player. Do not put world-event selection at round end.

A process can affect every player and still belong here. For example, a world enchantment's duration must not be decremented once for each wizard merely because it affects the whole world. Conversely, a process is not automatically round-based just because it currently appears in a global method.

These examples establish the distinction, not a final classification of every existing system. During implementation, examine each current turn-pipeline step and assign an intentional cadence: player turn, round start or round end. Do not assume without analysis that city growth, production completion, income, upkeep, healing, experience, magic generation, research, diplomacy timers, temporary effects or offers all belong to one category. Some may be player-specific, some world-wide, and some may need a more precise rule or placement.

Random event selection and the start-of-round application of world events happen once per round, before player turns. Ongoing event effects and durations must also advance at their intended cadence; do not assume that every event-related timer shares the event-draw boundary.

## 5. Trigger vocabulary and timing

The rules framework should distinguish player-turn timing from round timing. The intended trigger vocabulary includes:

- `OnPlayerTurnStart`
- `OnPlayerTurnEnd`
- `OnRoundStart`
- `OnRoundEnd`

These names describe lifecycle points; they do not imply that the current rules executor already dispatches them. The current trigger system is caller-driven, as documented in [rules-framework.md](rules-framework.md). Implementation must add explicit call sites and context (especially the player whose turn is starting or ending), not turn notifications into an implicit event bus.

Not every subsystem needs generic effect rules. Existing specialised systems may continue to perform their work directly at the appropriate lifecycle point.

## 6. Core ownership and state

`f4x-master` should be authoritative for:

- the round counter and current lifecycle phase;
- the ordered set of player controllers/seats participating in the game;
- which player is currently active;
- whether a player is eligible to take a turn, including elimination or other inactive states;
- starting a round and running round-start processing exactly once;
- invoking each eligible player's turn and advancing to the next;
- entering and completing round-end processing exactly once.

The master mediates player commands: they must be associated with the active player and rejected when submitted for another player. It then delegates the actual game-state changes to `f4x-core`. System-owned round-boundary work is not a player command and must not be blocked by active-player validation.

The dependency direction is one-way: `f4x-master` depends on `f4x-core`; the client depends on `f4x-master`; AI/controller crates may depend on both core and master to implement the controller interface. `f4x-master` must not depend on the client or a concrete AI crate. It defines the controller/seat interface and invokes it. A human controller is implemented by the client and must be able to pause while waiting for user input; the master should expose a resumable/waiting state rather than block the UI event loop. An AI controller can complete its turn through the same interface without client-specific code.

The master's round counter, active player, phase and any progress needed to resume a partially completed round must be represented in saved games so loading cannot repeat a player turn or run round-start processing twice. The exact division of serialized data between the existing save code and a future save/session wrapper is an implementation decision, but the state must be sufficient to resume deterministically.

Elimination during a round must be handled explicitly. An eliminated player who has not yet acted is skipped; one eliminated during or after its turn must not receive another turn. If no eligible players remain, the game reaches a defined terminal state rather than looping. Defeat, surrender and neutral factions must be handled consistently with their game rules.

## 7. Relationship to the existing turn pipeline

The current `World::end_turn` / `process_turn` pipeline mixes player-specific work with world-wide periodic work. It cannot simply be called in full once for each player: that would multiply global events, durations and other round-level effects by the number of players. `f4x-master` orchestrates the lifecycle, but it should call focused `f4x-core` operations for the work assigned to each boundary rather than move the simulation rules out of core.

Task 475 separates the existing pre- and post-counter portions of current turn advancement. That is useful groundwork, but the final design must move orchestration to `f4x-master` and establish the lifecycle described here. `f4x-core` retains the game rules and focused state-changing operations; `f4x-master` owns the round counter, phase transitions and calls to player controllers. Use names that make player-turn, round-start and round-end boundaries unambiguous.

Before moving each existing operation, make a system-by-system inventory from [game-flow.md](game-flow.md). For each operation, record:

- whether it runs per player, per round, or on a more specific condition;
- which player or world state it reads and changes;
- whether it should happen before or after player actions;
- whether it must run exactly once, and how that is enforced;
- how it behaves under elimination, stasis, loading and game end.

Update `game-flow.md` after implementation so it describes the new lifecycle rather than the old single-player end-turn sequence.

### 7.1 Inventory of the current pipeline (decided 6 November 2026)

Decisions: the economy of a player runs **per player** (at that player's turn end); the world-wide
work runs once per round; the order of players is fixed (the wizards, the human first), and the
players without a wizard (independent race players, then the monsters) act after them, each in a
turn of its own (10.11), before round-end work. The
core keeps `World.turn` as the round counter; `Command::EndTurn` stays as a compatibility
composition of the four phases for tests and tools (all players, in order).

**Revision (7 November 2026, replaces the table's cadence for the steps below).** The original's
turn change (`NEXTTURN.c`) is: gold income, mana income and research and casting, upkeep (food,
gold, mana), then per city growth and production. What a unit or building needs to defend must
exist before the rivals act, and a shortage must cost its units before the rivals act. So the
**income, the bills and the production are the player's turn end**, and what *changes the land and
the people* is the next turn start:

| Turn end (`end_player_turn`) | Turn start (`start_player_turn`) |
|---|---|
| raze windows | reopen sales, stasis |
| minimum farmers | research learned, next spell chosen (`process_research`) |
| `settle_treasury`: gold and food income, then food and gold upkeep; a shortage releases units now | Gaia's Blessing, Chaos Rift (terrain and buildings change before food is counted) |
| `process_magic`: mana income, research points, mana upkeep, casting (a summon or ward finished now defends) | outposts, population growth, pestilence (`grow_city`) |
| city production: units and buildings finish, so they defend (`produce_city`) | experience, healing |
| | moves refill (units finished at the end move now), offers, workers, orders |

The first round skips the start-of-turn work (the game begins with the first turn), and the end
work runs from the first turn on, so the number of ticks stays what it was. The research points
arrive with the mana income at the end; learning the spell is the start's, because it has nothing
to do with defence and the player sees it, and picks the next one, when they begin to act. Mana
upkeep stays before casting, so the shortage preview is exact.

**Shortages (decided 7 November 2026).** Every wizard pays, as today. The original drops the
newest unit (it walks the unit table from the end) and only for the human, and for mana only the
summons (the enchantment branches are stubs); ours releases the items that cost most of the
missing resource, since those are the likely cause of the shortage:
- *Default release, for gold, food and mana alike:* the **highest upkeep first** in the resource
  that is short (gold upkeep for gold, food for food, mana for mana; summons and lasting spells
  rank together for mana), until the books balance. Ties: the **lowest production cost first**
  (the cheapest to replace; a spell's cost is its casting mana), then the newest. Heroes and loaded
  ships stay last. The rule is `finance::release_order` and `magic::upkeep_release_order`; it
  pre-marks the human's dialog and is the game's fallback.
- *The AI's choice sits in `f4x-ai`* (`shortfall.rs`): it applies the same rule itself and
  releases through `ReleaseTroops` and `ReleaseUpkeep` before it ends its turn; the game's
  default is only the fallback for a wizard who did not choose (the human who closes the dialog,
  a script).

Phases (core operations, orchestrated by `f4x-master`): `start_round`, `start_player_turn(p)`,
`end_player_turn(p)`, `end_round`.

| Step (today, in order) | New cadence | Phase | Scope / note |
|---|---|---|---|
| `close_raze_windows` | player | `end_player_turn(p)` | only cities `p` took (marker is per taker) |
| `reopen_sales` | player | `start_player_turn(p)` | `p`'s cities only |
| `turn += 1` | round | `start_round` | the round counter |
| `process_stasis` | player | `start_player_turn(p)` | `p`'s units, before the moves refill |
| moves refill, `Standby::Done` cleared | player | `start_player_turn(p)` | Time Stop: units of others get 0 |
| `TurnStarted` event | round | `start_round` | once per round |
| min farmers | player | `end_player_turn(p)` | `p`'s cities |
| `settle_treasury` | player | `end_player_turn(p)` | skipped for the Time Stop wizard as today |
| `process_events` (world events) | round | `start_round` | before any player acts (turn-structure section 4) |
| outposts, production, growth, pestilence | player | production: `end_player_turn(p)`; outposts, growth, pestilence: `start_player_turn(p)` | `p`'s cities |
| `tick_temp_effects`, `process_globals` | round | `end_round` | durations must not be multiplied by player count |
| `process_world_enchantments` | round | `end_round` | |
| `cool_volcanoes`, `fade_corruption` | round | `end_round` | |
| `process_experience` | player | `start_player_turn(p)` | `p`'s units (already takes an owner filter) |
| `process_magic` | player | `end_player_turn(p)` | mana income, research points, mana upkeep, casting of `p`; `process_research` (the learning) at `start_player_turn(p)` |
| `process_gaias_blessing` | player | `start_player_turn(p)` | `p`'s cities |
| `heal_units` | player | `start_player_turn(p)` | `p`'s units |
| `process_chaos_rifts` | player | `start_player_turn(p)` | `p`'s cities hit what stands on them |
| `process_threats` (raiders, monsters) | round | `start_round` | the clocks tick once |
| `determine_offers` | player | `start_player_turn(p)` | offers lapse and are rolled for `p` |
| `record_history` | round | `end_round` | |
| `process_diplomacy` | round | `end_round` | timers once; proposals are answered in the AI turns |
| `process_purify`, `process_road_work` | player | `start_player_turn(p)` | `p`'s workers |
| `sail_marooned`, `drown_stranded` | player | `start_player_turn(p)` | `p`'s ships and units |
| `follow_orders` | player | `start_player_turn(p)` | `p`'s units with standing orders, last |

Elimination: a defeated player gets no `start_player_turn`/`end_player_turn`; its cities and units
are already gone or neutral. Time Stop: while it lasts, other wizards' turns still run their
economy except where the table says "skipped"; the stopper's units are the only ones that move.
Loading: the round and where the master was in it (the phase, with the player whose turn it is) are the game's state, saved with it (`Game::phase`, written by the master after every step; a save without a phase was made on the human's turn). The client's autosave is written when the master reaches the start of a round (`Master::checkpoints`, `Stopped::RoundStart`), the one moment no seat is half way through its turn; loading it plays on from there, so the round start and the human's turn start run again with the same random stream and the news is told as after any turn. The computer players' plans go with it, beside the game in the save file (`Plans`): one opaque text per AI (`Ai::to_text`, RON on one line, maps in key order so the text and the checksum are stable). The master collects them from the controllers (`Master::plans`, `Controller::plans`) for the autosave, the client's manual save asks its rivals and its AI of the other players (`Ai::plans_of`, `NeutralAi::plans`), and a load gives them back (`Ai::recall`, `NeutralAi::recall`, which makes the AI of each race player that has plans); a save without plans, or plans that do not parse, gives an AI that starts afresh. A loaded game then plays on exactly as the saved one would have (pinned by a test in `f4x-ai`'s `rounds.rs`). About 1 to 2 KB per AI. A save made while a computer player waits for the human to fight its attack goes on in that player's turn: the turn is not started again, but the player gives its orders again.

## 8. Determinism and tests

The lifecycle must be deterministic and testable without the client. At minimum, tests should cover:

- the expected player sequence across multiple rounds, including round-start world events before the first player acts;
- the master invoking both human and AI controllers through the same interface, with the human path pausing and resuming for input;
- a human and an AI using the same active-player validation;
- an AI turn observing changes made by the immediately preceding player's turn;
- a unit or city target moving or changing hands before a later player's turn, with actions validated against the current state;
- eliminated players being skipped and never receiving a later turn;
- round-start processing, including world events, running once before any player acts;
- round-end processing running once after all eligible players, never once per player;
- round-based event rolls and duration ticks not being multiplied by player count;
- saving and loading at player-turn and round boundaries without repeating or skipping work;
- deterministic results for the same initial state and command sequence.

## 9. Implementation boundary

This document defines the lifecycle and design constraints, not a demand to rewrite every subsystem at once. Implement `f4x-master` and its controller interface first, then migrate existing turn-pipeline operations into focused core calls orchestrated at the appropriate master boundary according to the cadence inventory. Keep each migration small, tested and explicit. Record any deliberate exception to the player-turn/round distinction here or in the relevant system documentation.

## 10. Splitting the game from the world (task 822)

**Built so far:** `f4x_master::Game` owns the world; the master drives it, and the AI, the console, the client and the tools hold it. It reads like a world, and a command goes through `Game::apply`; setup and cheats use `Game::world_mut` and `Game::world_and_rng`. Undo and the replay log are the game's (`game/undo.rs`, `game/replay.rs`). So is the turn order (`game/step.rs`): the lifecycle commands and `EndTurn` are gone, the game makes the steps (`Game::start_round` and the others, `Game::play_round`) through `World::advance`, `turn_players` is master's, and the replay log holds commands and steps. A command in the log carries its sender. The round counter, the phase and the end of the game are the game's (`GameState`, `game/end.rs`); the plans are the controllers'; the save file has the parts of 10.8, without the seats and the reactions, and older saves are converted (`convert.rs`, game version 0.0.8). Every player has a turn of its own (10.11). The world operations that need the round take it as an argument (`start_round`, `start_player_turn`, `score`, `cheat_offer`), and everyone outside the world asks `Game::round`. The random numbers are the game's too (10.10). Every command has a sender (`Game::apply(player, command)`, 10.4). Not yet built: the game's own check of whose turn it is, and reactions (10.4).

The **world** (`f4x-core`'s `World`, named `Game` until 26 November 2026) says what is in the world and what can be done in it. The **game** (`Game` in `f4x-master`) owns a `World` and advances time.

**Decided (26 November 2026):** the game owns the round counter and hands it to the world operations that need it. The world keeps no copy. This replaces "the core keeps `World.turn`" in section 7.1.

### 10.1 Inventory

| Item, today on `World` | Goes to | Note |
|---|---|---|
| `ecs`, `map`, `fog`, `content`, `names`, `player_index`, `monsters`, `next_serial`, `settings` | world | |
| `rng` (seed and draws) | game | passed to the world operations (10.10) |
| `diplomacy`, `events`, `threat`, `battle`, `battle_brains`, `item_awards`, `prefab_taken`, `locations` | world | the clocks of the events and threats are state of the world |
| `vision`, `net_gold_memo`, `budget_parts`, `tile_watch`, `confirmed` | world | caches and command state |
| `undo_on`, `undo_stack` | game | beside the replay log (10.9) |
| `recording` | game | replays move to the game (10.9) |
| `override_log` | world | the caller writes the round into the line (10.6) |
| `turn` | game | decided |
| `flow` | game | becomes typed state of the game (round, phase, active seat); the opaque text goes |
| `plans` | the controllers | the game never reads them; the save holds them beside the game (10.5) |
| `game_over` | game | the world keeps the facts, the game decides (10.7) |
| `turn_players()` | game | the order of the turns, which `Master` follows |
| `start_round`, `start_player_turn`, `end_player_turn`, `end_round` (`turn.rs`) | split | the order and the counter to the game; each step stays a world operation |
| `end_turn` (`Command::EndTurn`) | game | becomes `Game::play_round` |
| `Command::StartRound`, `StartPlayerTurn`, `EndPlayerTurn`, `EndRound` | leave the command set | |
| `endgame.rs`: `finish`, `win_by_mastery`, `check_conquest` | split | the facts in the world, the check and the verdict in the game (10.7) |
| `endgame.rs`: `living_wizards`, `banished_wizards`, `score`, `aftermath` | world | `score` takes the round; it reads the Spell of Mastery from the world |

Users of the moving state outside core: `f4x-ai` reads the round in most of its modules (directly and through `PlayerView::turn` and `game_over`) and its tests set it; the client reads `turn`, `game_over` and `flow`; the console reads `turn` and `game_over` and its tests send `EndTurn`; `f4x-tools` (soak, metrics, contestants) reads `turn`, `game_over` and sets `plans`. `f4x-learn` is stale.

### 10.2 Readers of the round inside the world

| Reader | Runs | How it can get the round |
|---|---|---|
| events (`world_event/world.rs`), raiders, monsters, sea raids (`threat/`) | round start | argument |
| growth is skipped in round 1 (`start_player_turn`), first offers (`player/offer.rs`) | turn start | argument |
| `score` (`world/endgame.rs`) | query | argument |
| `war_since` (`diplomacy`): stamped by a war declaration, read by `war_age` | **inside a player's command** | 10.6 |
| `GameOver.turn` (`finish`) | **inside a player's command** | the game stamps it |
| the cheat log's prefix (`world/cheat.rs`) | **inside a cheat command** | 10.6 |

The last three run while a command is applied, so "pass it in" does not reach them by itself (decided in 10.6).

### 10.3 Further decisions (26 November 2026)

| Point | Decision |
|---|---|
| The battle in progress | the world's: it holds unit entities and is saved with them; the game does not advance while one is open |
| The clocks of the events and threats | their state stays in the world; the game calls their step |
| Undo and reactions | a command that opens or answers a reaction (10.4) ends the undo history: the other player has seen it |
| Order of play (no neutral seat, 10.11) | wizards in today's fixed order, then the independent race players by player number, the monster player last |

### 10.4 Commands and reactions (decided 26 November 2026)

The human (through the client) and the AI players send every command through `Game::apply(player, command)`, and the game refuses the illegal ones. Setup, cheats and tests keep a separate way into the world; the commands that make something out of nothing (`CreateUnit`, `FoundCity`, `PlaceFortress`: `Command::is_setup`), which only setup, cheats and tests send, are refused from a player. The AI gets the game to send commands and a read-only view of the world.

**The rule:** on its own turn a player may do anything the world allows; on another player's turn it may only react to something that happened to it. After the end of the game everything is refused.

The game does not take a sender's word that something is a reaction. An event that affects another player opens a **reaction** for that player, which allows these commands and no others:

| What happened to the player | What it may do |
|---|---|
| A request, demand or threat | accept, refuse, counter, pay, declare war on the asker |
| First contact | greet |
| An attack | choose fighters, fight or auto-resolve, give battle orders to its own side |
| A city taken | declare war on the taker |
| A spell cast on it (which spells: task 785) | declare war on the caster |
| Items won | place, sell or scrap those items |
| Banishment | resign |

- A reaction stays open until the turn that caused it ends. A request and an attack hold that turn until they are answered; the others may be left, and lapse with it.
- Open reactions are part of the saved game.
- An attacker sends the attack on its own turn; the client does not start a battle on an AI's behalf.
- A rival's unprompted offers and demands are no reactions: they are made in the rival's own turn (today `approach_human` runs at the start of the human's turn).
- A reaction being allowed does not mean a front end or the AI uses it. A war question after a lost city, or an AI that strikes back at once, is new behaviour and not part of task 822.
- `World::command_actor` names a player for every command: the speaker of a talk, the owner of the attacking stack of a fight, the side to play of a battle order. `World::may_command` also refuses a stack with another player's unit in it.

**Until the reactions are built** (27 November 2026) a command is sometimes given in the name of a player other than the one whose code sends it: the battle AI of an attacking AI gives the orders of both sides, the client gives the orders of the other side in the human's battle and starts a rival's attack on the human, and the console in override mode sends a command in the name of whoever it acts for.

### 10.5 Game, master and the save (decided 26 November 2026)

| `Game` (game state, saved) | `Master` (runs the game, rebuilt at runtime) |
|---|---|
| the world | the controllers: the code that plays each seat |
| round, phase, active seat | round-start checkpoints |
| seats: which players, in what order | saving: asks each controller for its plans |
| game over | loading: gives each controller its plans back |
| open reactions (10.4) | |
| the random numbers (10.10) | |

- `Master` drives a `&mut Game`: it asks the active seat's controller to play, and another seat's controller when a reaction waits for it. The `Game` does not own the controllers: they borrow the front end's state, cannot be saved, and need to change the game to send commands.
- The save holds two parts side by side: the `Game`, and one block of plans per computer player. The `Game` does not know that AIs exist; a human or remote player has no block.

### 10.6 Who knows the round (decided 26 November 2026)

No command takes a round.

- **At a round or turn start** (and in queries) the world operation takes the round as an argument.
- **War age:** the world stores the age of each war instead of the round it began, and `start_round` adds one, the moment the round advances, so the values stay as they are today. The saved diplomacy field changes, and with it its pinned digest.
- **Game over:** the `Game` stamps the winning turn.
- **Cheat log:** the caller, which has the `Game`, writes "turn N:" into the line it records.
- **Client, console and tools** ask `game.round()`.
- **AI:** reads `game.round()` where it has the game and hands the round to the functions that need it; it stores none (changed 27 November 2026 from a noted copy, which could go stale). `PlayerView::turn` is gone, and `PlayerView::new` stays as it is.

### 10.7 Game over (decided 26 November 2026)

- The world keeps facts only: who is defeated and by whom, and who has completed the Spell of Mastery (a new saved fact; today only `game_over` records it).
- The `Game` asks the world after each command and after each turn or round step whether an end condition is met, and keeps the winner, the kind and the turn. The conditions are today's: the human wizard has fallen, at most one wizard is left, or the Spell of Mastery is complete.
- `Game::apply` refuses commands after the end; the world no longer refuses them.
- Of two conditions met at once the first of these counts: the human wizard has fallen, the Spell of Mastery is complete, at most one wizard is left. The `GameOver` event closes the events of the command or step.
- A cheat that wins the game sets the fact in the world and has the `Game` check (`Game::check_end`, which whoever changes the world by hand calls).

### 10.8 The save file, old saves and the pins (decided 26 November 2026)

| Part | Holds |
|---|---|
| header | the save and game version |
| game | seed and draws, round, phase, active seat, seats, game over, open reactions |
| world | everything else; it still loads on its own, for core's tests |
| plans | one block per computer player |

- **Old saves are converted** by a converter in `f4x-master`: one module that nothing else depends on, so that it can be retired later (task 823). The change bumps the version to 0.0.8.
- **Pinned digests** move: `seed`, `draws`, `turn`, `game_over`, `flow` and `plans` leave the world, the war age changes the diplomacy field, and the Spell of Mastery fact is new.
- **Proof that play is unchanged:** for the same seeds, a converted save from the build before the change is byte-identical to a save from the build after it. Run this on the layout change alone, before any change of behaviour; then refresh the pins and check that no other field moved.

### 10.9 Replays, undo and tests (decided 26 November 2026)

- **Replays belong to the game**, since a log with turn and round steps cannot be played on a world alone. The log is the starting game followed by entries: a player's command with its sender, or a turn or round step. Playback goes through the `Game` and needs no controllers or AI. Writing units and cities as creation numbers stays in core. Cheats are not recorded, as today.
- **Old replay files are refused**; the converter (10.8) is for saves. A replay written by a 0.0.8 build from before the commands carried their sender is not read either.
- **Undo belongs to the game** too: it restores the whole `Game`, open reactions included, and takes the command out of the replay log. What cannot be taken back stays as today (a command that explored, drew a random number or started a battle, and everything before it; the end of a turn); the game sees the draws itself, and the world tells it the other facts.
- **Turn-order, replay and undo tests** move to `f4x-master`.
- **Core tests that only need time to pass** call `Game::play_round`, with `f4x-master` as a dev-dependency of `f4x-core`, while the move is under way. Only the tests under `tests/` can use it, and `docs/design.md` section 2 records the exception. No test helper in core repeats the turn order.
- **The dev-dependency is temporary** (27 November 2026). Once the move is done, a test that needs the core alone sits in core and a test that needs the game sits in master, and core no longer depends on master, for tests either (the "Sort the tests" step of task 822).

### 10.10 Random numbers (decided 26 November 2026, built 27 November 2026)

- The seed and the number of draws belong to the `Game` and are saved in its part of the file.
- The `Game` passes `&mut GameRng` to each world operation that draws, as its last argument; the world keeps none. Whoever changes the world by hand in a way that draws takes both from `Game::world_and_rng`. The order of draws is the same as when the world kept the generator: the saves of the same seeds were compared (10.8).
- `f4x_newgame::setup::new_game` returns the world with the generator that made it, and the game goes on drawing from it.
- A test of core has no game to hold the generator: `world::testutil::Table` is a world with one, and the fresh games it loads have theirs beside the save (`fresh_w*_s*.rng`).
- A save written by a 0.0.8 build from before this step (seed and draws in the world) is not read: the converter takes older versions only.
- The AI draws no random numbers from the game (its one draw, whether to raze a captured city, is gone: task 824, 26 November 2026).

### 10.11 Players and seats (26 November 2026, built 27 November 2026)

There is no neutral seat. The players are the wizards, the independent race players (no wizard, one for each race, each played by an AI) and the monster player, and each has a turn of its own, with its own start, play, end and (once built) reactions.

- Every race has its player from the start of the game to its end (decided 27 November 2026); the monster player exists from the start too. None is made or removed during a game.
- The `Game` keeps the list of the players in the order of their turns and saves it (`GameState::players`, `Game::turn_players`): the wizards, the race players by player number, the monster player last. The list is made when the game begins and does not change.
- A player is skipped, with no start, play or end of a turn, when it is defeated, or is a race with no city and no unit when its place comes (`Game::takes_turn`, `World::is_dormant`). A race that gets one later plays the next time its place comes.
- A dormant race keeps what it owns (decided 27 November 2026: no reset): treasury, vault and its AI's memory. Its treaties and wars end when it loses its last city, and nobody deals with it until it returns (`diplomacy.md` "Neutral players").
- The saved phase names the player whose turn it is, not a position in a list.
- A seat of the master (`Seat`) says who plays a turn: `Seat::new` for one wizard, `Seat::non_wizards` for the controller that is asked in the turn of every other player (`f4x_ai::NeutralAi`, one AI for each race player and the roaming monsters).
- The converter of old saves (10.8) turns a seat number into a player; a restored old save gets the race players it lacks and its list of players. A save made while the old shared seat was playing goes on in the monsters' turn, the last of the round, so that no turn is started twice.
- One turn at a time changed the order of operations and of the random draws, so it came after the comparison of 10.8, as a change of its own.
