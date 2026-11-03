# Turn structure design

Status: design direction for task [477](TASKS.md). This describes the intended player-turn and round lifecycle; it is not yet implemented. The current pipeline remains as documented in [game-flow.md](game-flow.md).

## 1. Goals

Use **sequential player turns**, not a shared phase in which all AI players plan against the same old board and then execute orders in a fixed movement phase.

- Every wizard, human or AI, takes a complete turn in a deterministic order.
- Each player's decisions and actions use the game state left by all earlier players in the round.
- Actions resolve before the next player starts. Movement, attacks and battles must not rely on stale plans made before another player's turn.
- Processes that advance the shared world by a round run once after all eligible players have acted, not once per player.
- The core owns turn order and lifecycle rules; the client, AI and console participate in that lifecycle rather than deciding independently whose turn it is.

This is a gameplay design choice, not an attempt to reproduce the original game's AI scheduling. In particular, do not copy the original pattern of planning for all AI wizards first and then executing all their movement orders.

## 2. Two levels of time

There are two distinct levels:

1. **Player turn:** one player is active and may make that player's decisions and actions.
2. **Round resolution:** after every eligible player has completed a turn, the game runs the processes that advance the shared world by one round.

Conceptually:

```text
Round N
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
  Round resolution
    world events and other once-per-round processes
    advance round
Round N+1
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

## 4. Round-resolution responsibilities

Round resolution runs once after all eligible players have acted. It is for processes whose cadence is global or explicitly round-based, for example:

- drawing or advancing random world events;
- global environmental changes;
- ticking durations of effects measured in rounds;
- other shared periodic processes that should not happen once for every player.

A process can affect every player and still belong here. For example, a world enchantment's duration must not be decremented once for each wizard merely because it affects the whole world. Conversely, a process is not automatically round-based just because it currently appears in a global method.

These examples establish the distinction, not a final classification of every existing system. During implementation, examine each current turn-pipeline step and assign an intentional cadence. Do not assume without analysis that city growth, production completion, income, upkeep, healing, experience, magic generation, research, diplomacy timers, temporary effects or offers all belong to one side. Some may be player-specific, some world-wide, and some may need a more precise rule or placement.

Random event selection is clearly once per round: it must not be rolled separately for each player. Ongoing event effects and durations must also advance at their intended cadence, which may differ from the decision to draw a new event.

## 5. Trigger vocabulary and timing

The rules framework should distinguish player-turn timing from round timing. The intended trigger vocabulary includes:

- `OnPlayerTurnStart`
- `OnPlayerTurnEnd`
- `OnRoundStart`
- `OnRoundEnd`

These names describe lifecycle points; they do not imply that the current rules executor already dispatches them. The current trigger system is caller-driven, as documented in [rules-framework.md](rules-framework.md). Implementation must add explicit call sites and context (especially the player whose turn is starting or ending), not turn notifications into an implicit event bus.

Not every subsystem needs generic effect rules. Existing specialised systems may continue to perform their work directly at the appropriate lifecycle point.

## 6. Core ownership and state

The core should be authoritative for:

- the ordered set of players participating in the game;
- the active player;
- whether a player is eligible to take a turn, including elimination or other inactive states;
- whether the game is in a player turn or round-resolution phase;
- advancing to the next eligible player;
- entering and completing round resolution exactly once.

Commands representing player actions must identify or otherwise be associated with the acting player and be rejected when that player is not active. System-owned round-resolution work is not a player command and must not be blocked by active-player validation.

The lifecycle and phase must be represented in saved games so loading cannot repeat a player turn or run round resolution twice. The exact save-version migration is an implementation decision, but the state must be sufficient to resume deterministically.

Elimination during a round must be handled explicitly. An eliminated player who has not yet acted is skipped; one eliminated during or after its turn must not receive another turn. If no eligible players remain, the game reaches a defined terminal state rather than looping. Defeat, surrender and neutral factions must be handled consistently with their game rules.

## 7. Relationship to the existing turn pipeline

The current `Game::end_turn` / `process_turn` pipeline mixes player-specific work with world-wide periodic work. It cannot simply be called in full once for each player: that would multiply global events, durations and other round-level effects by the number of players.

Task 475 separates the existing pre- and post-counter portions of current turn advancement. That is useful groundwork, but the final player-turn design must establish the lifecycle described here and place each operation at its proper cadence. Use `end_turn` and `start_turn` consistently for the resulting player and round boundaries; avoid ambiguous methods whose meaning depends on whether "turn" means a player turn or a round.

Before moving each existing operation, make a system-by-system inventory from [game-flow.md](game-flow.md). For each operation, record:

- whether it runs per player, per round, or on a more specific condition;
- which player or world state it reads and changes;
- whether it should happen before or after player actions;
- whether it must run exactly once, and how that is enforced;
- how it behaves under elimination, stasis, loading and game end.

Update `game-flow.md` after implementation so it describes the new lifecycle rather than the old single-player end-turn sequence.

## 8. Determinism and tests

The lifecycle must be deterministic and testable without the client. At minimum, tests should cover:

- the expected player sequence across multiple rounds;
- a human and an AI using the same active-player validation;
- an AI turn observing changes made by the immediately preceding player's turn;
- a unit or city target moving or changing hands before a later player's turn, with actions validated against the current state;
- eliminated players being skipped and never receiving a later turn;
- round resolution running once after all eligible players, never once per player;
- round-based event rolls and duration ticks not being multiplied by player count;
- saving and loading at player-turn and round boundaries without repeating or skipping work;
- deterministic results for the same initial state and command sequence.

## 9. Implementation boundary

This document defines the lifecycle and design constraints, not a demand to rewrite every subsystem at once. Implement the core turn-order state and transitions first, then migrate existing turn-pipeline operations according to the cadence inventory. Keep each migration small, tested and explicit. Record any deliberate exception to the player-turn/round distinction here or in the relevant system documentation.
