# Turn structure design

Status: design direction for task [477](TASKS.md). This describes the intended player-turn and round lifecycle; it is not yet implemented. The current pipeline remains as documented in [game-flow.md](game-flow.md).

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

The current `Game::end_turn` / `process_turn` pipeline mixes player-specific work with world-wide periodic work. It cannot simply be called in full once for each player: that would multiply global events, durations and other round-level effects by the number of players. `f4x-master` orchestrates the lifecycle, but it should call focused `f4x-core` operations for the work assigned to each boundary rather than move the simulation rules out of core.

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
work runs once per round; the order of players is fixed (seat order, the human first) and the
**neutral seat** (independent cities, the monsters, raiders) acts last, before round-end work. The
core keeps `Game.turn` as the round counter; `Command::EndTurn` stays as a compatibility
composition of the four phases for tests and tools (all players, in order).

**Revision (7 November 2026): the economy moves to the turn start.** A player's economy
(magic, Gaia's Blessing and Chaos Rift, farmers, treasury, cities, experience, healing) runs at
the start of their own turn so they act on the state they will see, and a unit built is ready
to move at once. The order inside the start phase matters: stasis, then **magic** (a spell that
finishes this turn counts), **Gaia's Blessing and Chaos Rift** (they change terrain and
buildings), then min farmers, treasury, cities, experience and healing, then the moves refill
(new units get their moves), offers, workers, orders. The first round has no economy (the game
begins with the first turn), so the number of economy ticks per turn stays as before. The end
phase keeps only what closes out the turn: raze windows and land corruption (which needs the
moves the turn left unspent). In the table below, the rows marked `start_player_turn(p)` for
these steps are the revised ones.

Phases (core operations, orchestrated by `f4x-master`): `start_round`, `start_player_turn(p)`,
`end_player_turn(p)`, `end_round`.

| Step (today, in order) | New cadence | Phase | Scope / note |
|---|---|---|---|
| `close_raze_windows` | player | `end_player_turn(p)` | only cities `p` took (marker is per taker) |
| `reopen_sales` | player | `start_player_turn(p)` | `p`'s cities only |
| `process_land_corruption` | player | `end_player_turn(p)` | `p`'s units that stood still all turn (needs unrefilled moves) |
| `turn += 1` | round | `start_round` | the round counter |
| `process_stasis` | player | `start_player_turn(p)` | `p`'s units, before the moves refill |
| moves refill, `Standby::Done` cleared | player | `start_player_turn(p)` | Time Stop: units of others get 0 |
| `TurnStarted` event | round | `start_round` | once per round |
| min farmers | player | `start_player_turn(p)` | `p`'s cities |
| `settle_treasury` | player | `start_player_turn(p)` | skipped for the Time Stop wizard as today |
| `process_events` (world events) | round | `start_round` | before any player acts (turn-structure section 4) |
| outposts, production, growth, pestilence | player | `start_player_turn(p)` | `p`'s cities |
| `tick_temp_effects`, `process_globals` | round | `end_round` | durations must not be multiplied by player count |
| `process_world_enchantments` | round | `end_round` | |
| `cool_volcanoes`, `fade_corruption` | round | `end_round` | |
| `process_experience` | player | `start_player_turn(p)` | `p`'s units (already takes an owner filter) |
| `process_magic` | player | `start_player_turn(p)` | power, research, upkeep, casting of `p` |
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
Loading: the master state (round, phase, active seat) is saved with the game (stage 5).

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
