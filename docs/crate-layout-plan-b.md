# Crate layout: plan B (`f4x-game` holds the game, `f4x-core` is the rules)

Asked for by the user, 2 November 2026, as the alternative to plan A (`docs/crate-layout-plan.md`).
One of the two is to be chosen; until then no task changes. Numbers were measured on the code of that
day (non-test lines). "Estimate" or "not measured" marks a guess.

**Decisions so far (user, 2 November 2026):** everything stored in a save sits in `f4x-game`: the turn
number, the map, the lairs, the cities, every player's units, the heroes' state, and so on. The plan is
not to stay conservative; deeper changes are accepted. New game is a module, not a crate. Undo and
replay belong to the session. `f4x-ai` depends on `f4x-game` and implements a `Seat` trait defined there
(section 5). **The battle is split:** a small set of static rules in `f4x-core` (terrain values, combat
spell definitions and the like) and the battle engine in `f4x-game` (it creates the battle map, runs the
combat turns, tracks unit health). Plan A versus B is not decided; the user has a few days.
**Revision (2 November 2026):** every module is split into three layers, **definition, state, action**
(section 2a). Definitions go to `f4x-core`; state and actions both go to `f4x-game`, in separate
folders. So plan B cuts horizontally through the modules instead of moving whole modules.

## 1. The layout

```
f4x-core    the static rules: content types and loader (f4x-data merged in), registries, text, help,
            the effect/modifier engine, the static battle rules (terrain values, combat spell
            definitions, ...), shared primitives (Position, Plane, ids)
   ^
f4x-game    the state of one game and everything that changes it, in three folders:
            state/    components, Game, Map, Fog, Diplomacy, Events, Threat, the battle in progress;
                      the wiring that makes an instance from a definition; read-only queries; save and load
            action/   Command / Event / apply, moving, casting, producing, battle turns, the turn change,
                      new game (module), undo, replay
            session/  seats, turn order
   ^
f4x-ai      decides commands for a seat; reads the game
   ^
f4x-console, f4x-client, f4x-learn, f4x-tools
```

`f4x-data` disappears into `f4x-core`. `f4x-ai-ref` stays beside `f4x-ai`.

## 2. What the code says about the cut

Measured per module of today's `f4x-core` (non-test lines; "touching state" = the file names `Game`,
`World` or `hecs::`):

| Module | Lines | Touching state | Reading |
|---|---|---|---|
| `rules` | 2,694 | 417 (`exec.rs`) | rules; goes to `f4x-core` |
| `text`, `help`, `content` | 670 | 0 | goes to `f4x-core` |
| `battle` files free of the game | about 4,200 | 0 | `ai`, `attack`, `field`, `map`, `map/layout`, `special`, `vortex`. Engine code (it runs fights and builds the battle map), so it goes to game as a module; being free of `Game` makes the move easy. Only its static tables and values go to core |
| `battle` rest | about 7,300 | 7,300 | `mod.rs` (the `Battle`, with its units' health), `spell.rs`, `casting.rs`, `game.rs`, `spoils`, `save`, `brain`: engine and glue, all to game |
| `map` | 2,985 | 2,183 | terrain definitions to core; `Map`, `Grid`, fog, roads, mapgen to game |
| `feature` | 2,698 | 1,933 | `def.rs` to core; entities, placement to game |
| `city`, `unit`, `spell`, `wizard`, `player`, `diplomacy`, `hero`, `item`, `threat`, `world_event`, `game` | about 36,000 | about 35,000 | the systems: all of this is "state plus the rules acting on it" and goes to game as it is |

So **today's `f4x-core` is mostly the game**. The static rules are a small part: estimate 4,000 to
6,000 lines (rules, text, help, terrain and feature definitions, the static battle tables) against about
70,000 lines that stay with the state. Plan B therefore means: carve a small `f4x-core` out of the
bottom of the present crate, and call the rest `f4x-game`.

About 60 distinct component types are read or written through the world (counted from `get`, `query` and
`insert` calls). They are state: `Population`, `Farmers`, `Standby` and the rest go to `f4x-game/state`.
The splitting per file is now part of the plan (section 2a): each file of the 36,000 lines is sorted into
definition, state or action, and files that hold two layers are cut. This is the `architecture-plan.md`
program carried to the end, and the 1,300 methods of `impl Game` are what has to be sorted.

## 2a. Definition, state, action

Every module of today's core is cut into three layers. The example is the unit:

| Layer | What it is | Unit example | Lives in |
|---|---|---|---|
| **Definition** | static content, and formulas that need no state | a spearman has 8 figures, attack 1; the damage table | `f4x-core` |
| **State** | what exists in this game, how an instance is made, and read-only questions about it | this spearman has 5 figures left and 20 xp; `spawn` makes one under a player's control; its attack with Bless is 3; can it enter this tile | `f4x-game/state` |
| **Action** | what changes the state | move it one square, summon it, heal it at turn change, kill it in a battle | `f4x-game/action` |

The same for the battle: terrain values and combat spell definitions are definitions; the `Battle`, a
`BattleUnit` with its health and the battle map are state; running the combat turns, attacks and casting
are actions.

**Rules between the layers** (a test enforces them, like task 183's method count):
1. Definitions know nothing of state or actions.
2. State knows definitions. **State never calls an action.**
3. Actions know both, and are the only code that changes state. `Game::apply` is the one way in from outside.
4. The decision to create something is an action (production finishes, a spell summons); the wiring that
   makes the instance is state (`spawn`).

**Why state and action share a crate.** Both are in `f4x-game`, so state fields stay `pub(crate)`: actions
see them, the front ends do not. Save and new game need those fields and sit in the same crate; nothing is
exposed. Splitting state and action into two crates later is a `git mv` per folder (the fields would then
have to become `pub`); not planned.

**Folders: layer first.** `state/unit/`, `action/unit/` rather than `unit/state.rs`, `unit/action.rs`.
Rule 2 is then a check on directory names, and a later crate split is a move of two folders.

**Size.** In a signature count of the 36,000-line blob, about 360 of 1,500 functions take `&mut` of the
game, the world, the map or the fog: roughly a quarter. The rest read. Lines were not counted; the
mutators are probably the longer functions. The count misses multi-line signatures.

## 3. Where things go

| What | To | Notes |
|---|---|---|
| `rules/{defs,ids,lair,refs,registry,stats,effects}.rs` | core | as plan A |
| `rules/exec.rs` (effect executor) | core | it already runs over a trait (`EffectHost`, `rules/exec.rs:68`); game implements the host. Plan A had to move it up; check that its uses of `battle::immunity`, `game::replay` and `player` go through the host |
| `map/terrain.rs`, `feature/def.rs`, definition half of `game/setting.rs`, `text`, `help`, `content` | core | as plan A |
| `f4x-data` loaders, validators, `shipped()` test loader | core | `game/testutil.rs::load_shipped` (308 lines) is deleted; game's tests call the real loader |
| static battle rules: terrain values for combat, combat spell definitions, ability and field tables, constants | core | not yet listed one by one; stage 2 sorts `battle/` into static values (core) and engine (game). Much of it may already be content in `rules` |
| the battle engine: `Battle`, `BattleUnit` (health, position), battle map creation (`map/layout`), turn running, attacks, fields, specials, vortex, battle AI, spell casting in battle | game | state and the rules that run on it |
| primitives: `Position`, `Plane`, `PlayerId`, `TerrainId`, `FeatureId`, `Gender`, `GameRng`, `Serial` | core `base` module | game and the content types both use them; no `f4x-base` crate is needed because core is the bottom |
| components, `Game`, `Map`, `Fog`, `Diplomacy`, `Events`, `Threat`, `ActiveBattle`, `spawn`, the read views (`UnitView`, `Economy`) | game/state | |
| `Command`, `Event`, `apply`, `undo`, `replay`, `endgame`, `cheat` | game/action | `view` and `access` (queries) are state |
| `process_turn` and the turn steps | game/action | the turn steps that only read (what a city yields) are state |
| battle glue (`battle/game.rs`, `spoils`, `save`, `brain`, the `Game` half of `spell.rs`) | game | |
| `mapgen`, `setup` (`new_game`) | game, as modules | see 4 |
| `save.rs`, `serial.rs`, `battle/save.rs`, `f4x-data/src/save.rs` | game, as modules | see 4 |
| `sound.rs`, `spell_art.rs` | client | stage 0, task 467 |

## 4. Why this removes the hardest part of plan A

Plan A puts `f4x-newgame` and `f4x-save` above core, so they can only see the public API of `Game`.
`save.rs` reads 25 `pub(crate)` fields and calls `pub(crate)` restore methods; the generators need the
feature placement code. Plan A therefore needs a designed `to_parts` / `from_parts` interface (its risk 2)
and a public API for placement.

In plan B, `Game`, its save and its new-game code are in one crate. They keep using `pub(crate)` fields
as they do today. Nothing is exposed, no interface is designed, and A's stages B and C are not needed.
The format (`SAVE_VERSION` 4) does not change. The price is that the crate stays large and a save
change rebuilds all of it, which is true today anyway.

## 5. The session, and a change to my earlier answer

The session lives in `f4x-game`: `Session` owns the `Game`, the seats and the turn state;
`submit(seat, Command) -> Vec<Event>` for a human; `end_turn()` lets every AI seat play, then runs the turn
change; the AI's proposals to the human (`Ai::approach_human`, `answer_proposals`, `answer_resales`)
come out as events or a queue, so no screen calls `f4x_ai` for them. Undo and replay (`game/undo.rs`,
`game/replay.rs`) move into it.

**This changes the answer to question 1.** I suggested `f4x-game` depends on `f4x-ai` directly. With the
state in `f4x-game` that is impossible: `f4x-ai` reads the game (its 130 or so `f4x_core::` references
are mostly `rules`, `player`, `game`, `map`, `wizard`, `diplomacy`), so `f4x-ai` must depend on
`f4x-game`, and a cycle is not allowed. The fix is the trait variant:

- `f4x-game` defines `trait Seat { fn play_turn(&mut self, game: &mut Game) -> Vec<Event>; ... }`.
- `f4x-ai` implements it for `Ai`; `f4x-learn` implements it for its agent.
- The front ends build the session: `Session::new(game, vec![Seat::Human, Box::new(Ai::new(id)), ...])`.

The battle brain is the same pattern and already exists (`BattleBrain`, `battle/brain.rs`). "The AI
directly" still holds: an AI seat sends commands straight through `Game::apply`; only the crate
dependency points the other way.

## 6. Names and the rename

The old crate becomes `f4x-game` and a new, small crate takes the name `f4x-core`. The name `f4x_core` is
used in 168 files outside core with 1,468 lines, and in 34 documents, scripts and manifests. Done in
this order it is two mechanical steps and no confusion:

1. Rename `f4x-core` to `f4x-game` everywhere (a script with `sed -i ''`, logged in `docs/scripts.md`).
   One commit; tests unchanged.
2. Create the new `f4x-core` and `git mv` the static parts into it from `f4x-game`, leaving
   `pub use f4x_core::...` re-exports in game so the existing paths keep compiling; remove them later
   module by module if wanted.

## 7. Step by step

Each step leaves `cargo test --workspace`, `cargo clippy --workspace --tests`,
`scripts/check_id_aliases.py` and the soak and regression targets unchanged. One commit per step.

**Stage 0 (task 467):** art out of core, as plan A. First.

**Stage 1: rename** `f4x-core` to `f4x-game` (section 6, step 1).

**Stage 2: sort every module into definition, state, action, inside the crate, no new crate yet.**
Domain by domain, smallest and least depended on first (`threat`, `hero`, `item`, `world_event`,
`feature`, `diplomacy`, `player`, `city`, `wizard`, `unit`, `spell`, `battle`, `map`, `game`). For each:
move its mutators into `action/<domain>`, its components, views and `spawn` into `state/<domain>`, its
static values and formulas into a `definition` part that stage 3 moves to core. After each domain the
suite is green and the test of rule 2 (state never calls an action) passes. The compiler lists every
place where a read calls a mutator; those are the real work, and the first domain shows how many there are.
Together with this, the back-edges: As plan A 3.1: hook-name constants
into `rules`, `GenParams` and the counting functions out of `setting.rs`, the primitives into one `base`
module, the definition half of `setting.rs` split from `GameSettings`, `Retort` decided. Plus the
battle: list every table, constant and formula in `battle/` that is a static rule (terrain values,
spell and ability tables) and sort it into core or game. Gate: `grep -rn "crate::" ` on the files that will move names only each
other and `base`.

**Stage 3: create `f4x-core`** and merge `f4x-data` into it: move the static parts (section 3), the
loader and its tests, replace `load_shipped`, fix the scripts that name `f4x-data` (the five
`scripts/export_original_*.py`, `untested_content.py`, `test_changed.py`) and the docs. The battle engine
does not move; only its static values do. The orphan-rule problem (plan A, risk 5: an `impl SomeRulesType`
in game code) appears here; start with one small type to see the shape of the errors.

**Stage 4: the session.**
1. Measure: every loop that sends `Command::EndTurn` or runs `play_round`, and every `f4x_ai::` call in
   the client and console. (`EndTurn` appears in the client, the console, `f4x-ai` and `f4x-ai-ref`.)
2. `Session` and `Seat` in `f4x-game`; `Ai` and the learned agent implement `Seat`.
3. Port the console first (text only, easiest to test), then `f4x-tools soak` and `f4x-learn`, then the
   client.
4. Test: one scripted game through `Session` equals the same game through raw `Game::apply` (same seed,
   same events).

**Stage 5 (ongoing):** a function in state or action that turns out to take no state is a definition;
move it to core.

## 8. A against B

| | A | B |
|---|---|---|
| Static rules in a crate of their own | `f4x-rules`, then `f4x-core` is the simulation | `f4x-core`, the simulation is `f4x-game` |
| `f4x-data` | into `f4x-rules` | into `f4x-core` |
| Battle engine | stage D, own crate, not planned | a module of `f4x-game`; only the static values go to core |
| New game, save | two more crates, public interface to design | modules of `f4x-game`, nothing to expose |
| Session and seats | not covered | in `f4x-game`, the AI implements `Seat` |
| Inside the big crate | unchanged | split into `state/` and `action/` by rule, per module |
| Renames | none | `f4x-core` to `f4x-game` (168 files), then the new core |
| A change to a rule rebuilds less | yes | yes (core is small and at the bottom) |
| A change to game code rebuilds less | no | no: game stays one large crate |
| Crates added | `f4x-rules`, `f4x-newgame`, `f4x-save` | `f4x-game` and the new `f4x-core` |
| Tasks | 455, 456 (into rules), 458, a new save task | 455 and 458 drop out, 456 (into the new core) stays, new: rename, session, sort the battle's static rules |

The two agree on the static-rules cut and on stage 0. They differ in whether newgame and save are
crates above the simulation (A) or modules in the same crate as `Game` (B), and in B's session.

## 9. Risks and open questions

| # | Risk or question | Answer |
|---|---|---|
| 1 | The AI/game cycle | solved by `Seat` (section 5). **Confirmed by the user, 2 November 2026** |
| 2 | `f4x-game` stays about 70,000 lines | accepted; the state/action split inside it keeps it readable, and it can become two crates later |
| 8 | Reads that call mutators (rule 2) | unknown until the first domain is sorted; a `&self` query that needs an `&mut` helper is split into the query and a returned plan, as `Economy` does |
| 3 | Orphan rule: `impl` blocks on rules types in game code | count in stage 2 with the compiler; move to core, or make an extension trait |
| 4 | Is the effect executor free of the game through `EffectHost`? | check in stage 2; `exec.rs` mentions `battle::immunity`, `game::replay`, `player` |
| 5 | The battle split between static rules and engine | **Decided by the user, 2 November 2026:** static values in core, engine in game. The exact list is made in stage 2. If the engine later wants its own crate, it can go between core and game because it is free of `Game`; not planned |
| 6 | Does the client still need `f4x-console`? | it depends on it today; may become unneeded once both use the session. Not checked |
| 7 | Do undo and replay work through the public API? | now irrelevant: they are in game, which owns the fields |
