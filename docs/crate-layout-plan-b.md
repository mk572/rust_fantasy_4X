# Crate layout: plan B (definitions in `f4x-static`, the game in `f4x-core`)

> **Superseded (6 November 2026):** the user chose neither plan as written. The static definitions and
> their loaders are in `f4x-def` (task 481), `f4x-data` is gone, and save/load is in `f4x-core`. This file
> stays as the record of the options.

Asked for by the user, 2 November 2026, as the alternative to plan A (`docs/crate-layout-plan.md`).
One of the two is to be chosen; until then no task changes. This file is meant to be executed without
reading plan A: everything a step needs is written out here. Numbers were measured on the code of that
day (non-test lines). "Estimate" or "not measured" marks a guess.

**Decisions so far (user, 2 November 2026):**
- Everything stored in a save sits in the game crate: the turn number, the map, the lairs, the cities,
  every player's units, the heroes' state, and so on. The plan is not to stay conservative; deeper
  changes are accepted.
- Every module is split into three layers, **definition, state, action** (section 2a). Definitions go to
  the new crate `f4x-static`; state and actions stay in `f4x-core`, in separate folders.
- **No rename.** `f4x-core` keeps its name (a rename touches 168 files and 34 documents for nothing). The
  new crate is `f4x-static`. There is no crate called `f4x-game`.
- New game is a module of `f4x-core`, not a crate. Undo and replay remain in the core action layer.
- Turn/round orchestration belongs in `f4x-master`, above `f4x-core` and below the client; AI and human controllers implement its interface (section 5).
- The battle is split: a small set of static rules goes to `f4x-static` (terrain values, combat spell
  definitions and the like); the battle engine stays in `f4x-core` (it creates the battle map, runs the
  combat turns, tracks unit health).
- Plan A versus B is not decided; the user has a few days.

## 1. The layout

```
f4x-static  the definitions: content types and loader (f4x-data merged in), registries, text, help,
            the effect/modifier engine, the static battle rules (terrain values, combat spell
            definitions, ...), formulas that need no state, shared primitives (Position, Plane, ids)
   ^
f4x-core    the state of one game and everything that changes it, in three folders:
            state/    components, Game, Map, Fog, Diplomacy, Events, Threat, the battle in progress;
                      the wiring that makes an instance from a definition; read-only queries; save and load
            action/   Command / Event / apply, moving, casting, producing, battle actions,
                      new game (module), undo, replay
   ^
f4x-master  round counter, player order, active controller, round/player lifecycle
   ^
f4x-ai      implements a master controller; decides commands and reads the game
   ^
f4x-console, f4x-client, f4x-learn, f4x-tools
```

`f4x-data` disappears into `f4x-static`. `f4x-ai-ref` stays beside `f4x-ai`. After the move,
`f4x_core::rules` (and `content`, `text`, `help`, `map::terrain`, `feature::def`) is
`pub use f4x_static::...`, so the several hundred existing `crate::rules::...` paths and the imports of the
other crates keep compiling. Remove the re-exports later, one module at a time, only if wanted.

## 2. What the code says about the cut

Measured per module of today's `f4x-core` (non-test lines; "touching state" = the file names `Game`,
`World` or `hecs::`):

| Module | Lines | Touching state | Reading |
|---|---|---|---|
| `rules` | 2,694 | 417 (`exec.rs`) | definitions; goes to `f4x-static` |
| `text`, `help`, `content` | 670 | 0 | definitions; goes to `f4x-static` |
| `battle` files free of the game | about 4,200 | 0 | `ai`, `attack`, `field`, `map`, `map/layout`, `special`, `vortex`. Engine code (it runs fights and builds the battle map), so it stays in core; being free of `Game` makes a later move easy. Only its static tables and values go to `f4x-static` |
| `battle` rest | about 7,300 | 7,300 | `mod.rs` (the `Battle`, with its units' health), `spell.rs`, `casting.rs`, `game.rs`, `spoils`, `save`, `brain`: engine and glue, stay in core |
| `map` | 2,985 | 2,183 | terrain definitions go to `f4x-static`; `Map`, `Grid`, fog, roads, mapgen stay |
| `feature` | 2,698 | 1,933 | `def.rs` goes to `f4x-static`; entities, placement stay |
| `city`, `unit`, `spell`, `wizard`, `player`, `diplomacy`, `hero`, `item`, `threat`, `world_event`, `game` | about 36,000 | about 35,000 | the blob: state and the rules acting on it, mixed in the same files. Section 2b |

So **today's `f4x-core` is mostly the game**. The files that are definitions already (rules, text, help,
terrain and feature definitions, the static battle tables) are an estimated 4,000 to 6,000 lines. The
other estimated 70,000 lines are state and actions.

### 2b. The definition parts are carved out of the blob

The 36,000-line blob (`city`, `unit`, `spell`, `wizard`, `player`, `diplomacy`, `hero`, `item`, `threat`,
`world_event`, `game`) is not only state and actions. Definition code sits inside it, in the same files as
the systems. Stage 1 finds it and cuts it out, so `f4x-static` is bigger than the 4,000 to 6,000 lines
above. What to look for, in every file of the blob:

- **constants** that are rules, not state: `MAX_STACK`, `MAX_TILE_LIMIT`, caps, thresholds, costs, the hook
  names that `rules/effects.rs` lists (`CUSTOM_HOOKS`);
- **plain types that content names**: enums and structs used by definitions and by state alike (`Retort`,
  `Bonus`, `Neutral`, `PlayerId`, `Gender`, `TerrainClass`, `Standby` kinds if content refers to them);
- **formulas that take numbers or a definition and no world**: a cost, a table lookup, a rounding rule,
  a stat computed from a `UnitDef` alone (no components);
- **validation of content** that checks a definition against other definitions.

A function is a definition if its signature mentions no `Game`, `World`, `Map`, `Fog` or component type.
How many lines this is was **not measured**; the first domain sorted in stage 1 gives the ratio.

Example, the unit: `UnitDef` and its stat tables are already in `rules`. The cap on a stack
(`unit::MAX_STACK`) and any formula over a `UnitDef` alone are carved out of `unit/` into `f4x-static`.
What a spearman is *now* (figures left, xp, Bless) and how it moves stay.

About 60 distinct component types are read or written through the world (counted from `get`, `query` and
`insert` calls). They are state: `Population`, `Farmers`, `Standby` and the rest go to `f4x-core/state`.
Files that hold two layers are cut. This is the `architecture-plan.md` program carried to the end, and
the 1,300 methods of `impl Game` are what has to be sorted.

## 2a. Definition, state, action

Every module of today's core is cut into three layers. The example is the unit:

| Layer | What it is | Unit example | Lives in |
|---|---|---|---|
| **Definition** | static content, and formulas that need no state | a spearman has 8 figures, attack 1; the damage table | `f4x-static` |
| **State** | what exists in this game, how an instance is made, and read-only questions about it | this spearman has 5 figures left and 20 xp; `spawn` makes one under a player's control; its attack with Bless is 3; can it enter this tile | `f4x-core/state` |
| **Action** | what changes the state | move it one square, summon it, heal it at turn change, kill it in a battle | `f4x-core/action` |

The same for the battle: terrain values and combat spell definitions are definitions; the `Battle`, a
`BattleUnit` with its health and the battle map are state; running the combat turns, attacks and casting
are actions.

**Rules between the layers** (a test enforces them, like task 183's method count):
1. Definitions know nothing of state or actions (they are another crate, so the compiler enforces it).
2. State knows definitions. **State never calls an action.**
3. Actions know both, and are the only code that changes state. `Game::apply` is the one way in from outside.
4. The decision to create something is an action (production finishes, a spell summons); the wiring that
   makes the instance is state (`spawn`).

**Why state and action share a crate.** Both are in `f4x-core`, so state fields stay `pub(crate)`: actions
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
| `rules/{defs,ids,lair,refs,registry,stats,effects}.rs` | `f4x-static` | `exec.rs` is the next row |
| `rules/exec.rs` (effect executor, 417 lines) | `f4x-static` | it already runs over a trait (`EffectHost`, `rules/exec.rs:68`); core implements the host. It uses `battle::immunity`, `game::replay` and `player`: check that these go through the host. If not, leave it in core as `action/spell/exec.rs` |
| `map/terrain.rs` (`TerrainDef`, `Terrains`, `TerrainClass`) | `f4x-static::terrain` | |
| `feature/def.rs` (`FeatureDef`, `Features`, `FeatureKind`) | `f4x-static::feature` | |
| `game/setting.rs`, the definition half: `SettingsDef`, `ClientSettingsDef`, difficulty levels, sizes, `SpellLibraryDef` | `f4x-static::setting` | the chosen `GameSettings` is state and stays |
| `text.rs`, `text/` (`Texts`, `Gender`), `help.rs`, `content.rs` (`Content`) | `f4x-static` | texts are content |
| all of `f4x-data/src` (`lib.rs` loaders, `rules.rs`) | `f4x-static::load` | `sound.rs` and `spell_art.rs` go to the client first (stage 0); `save.rs` goes to core, see below |
| test support: `shipped()`, `content_dir()`, the `num` constants, `TestDir` | `f4x-static::testutil` (public, small) | one loader for all tests; `game/testutil.rs::load_shipped` (308 lines, 59 files, 170 calls; skips validation, messages, help, hero names; task 430 says it can drift) is deleted |
| static battle rules: terrain values for combat, combat spell definitions, ability and field tables, constants | `f4x-static` | not listed one by one yet; stage 1 sorts `battle/`. Much of it may already be content in `rules` |
| the battle engine: `Battle`, `BattleUnit` (health, position), battle map creation (`map/layout`), turn running, attacks, fields, specials, vortex, battle AI, spell casting in battle, glue (`battle/game.rs`, `spoils`, `save`, `brain`, the `Game` half of `spell.rs`) | `f4x-core` | state in `state/battle`, the running of it in `action/battle` |
| primitives: `Position`, `Plane`, `PlayerId`, `TerrainId`, `FeatureId`, `Gender`, `GameRng`, `Serial`, `MAX_STACK` | `f4x-static::base` | definitions and state both use them |
| components, `Game`, `Map`, `Fog`, `Diplomacy`, `Events`, `Threat`, `ActiveBattle`, `spawn`, the read views (`UnitView`, `Economy`), `view`, `access` | `f4x-core/state` | |
| `Command`, `Event`, `apply`, `undo`, `replay`, `endgame`, `cheat` | `f4x-core/action` | |
| `process_turn` and the turn steps | `f4x-core/action` | the steps that only read (what a city yields) are state |
| `mapgen` (1,076 lines in `mapgen/original.rs`), `setup` (`new_game`, 894 lines), the feature placement generators (about 1,000 lines of `feature/`) | `f4x-core/action/newgame` | a module, not a crate |
| `save.rs` (1,765 lines, 734 of tests), `serial.rs`, `battle/save.rs`, `f4x-data/src/save.rs` (221) | `f4x-core/state/save` | the format (`SAVE_VERSION` 4) does not change. The file IO (`save_game`, `load_game`, `summarize_save`, `SaveError`, `SaveSummary`) goes with it into core, although the rest of `f4x-data` goes to `f4x-static`: it takes and returns a `Game`, so it cannot live in a crate below core, and a separate save crate above core is plan A |
| `sound.rs`, `spell_art.rs`, art fields | client | stage 0, task 467 |

The tests of `f4x-data` split as follows:

| Test file | Lines | Goes to |
|---|---|---|
| `original_content.rs` (needs a retort enum) | 843 | `f4x-static/tests/it` (after `Retort` has moved, see 7, stage 1) |
| `text.rs`, `shipped.rs`, `stacking.rs` | 246 + 14 + 58 | `f4x-static/tests/it` |
| `autobuild.rs` (needs `city`) | 119 | `f4x-core/tests/it` |

## 4. Why this removes the hardest part of plan A

Plan A puts new game and save in crates above core, which can only see `Game`'s public API. `save.rs`
reads 25 `pub(crate)` fields and calls `pub(crate)` restore methods (`restore_battle`,
`Diplomacy::restore`), so plan A needs a designed `Game::to_parts()` / `from_parts()` interface and a
public API for the placement generators.

In plan B, `Game`, its save and its new-game code are in one crate. They keep using `pub(crate)` fields
as they do today. Nothing is exposed, no interface is designed. The format does not change. The price is
that the crate stays large and a save change rebuilds all of it, which is true today anyway.

## 5. The master and player controllers

Turn and round orchestration lives in a new `f4x-master` crate, between `f4x-core` and
`f4x-client`. It owns the `Game` session boundary, round counter, player order, active player
and lifecycle phase. It starts each round, invokes the eligible player controllers in
sequence, and runs the explicitly assigned round-boundary work. World events are processed
at **round start**, before the first player acts.

`f4x-master` depends on `f4x-core`; it must not depend on the client or a concrete AI crate.
It defines a player-controller/seat trait that the other crates implement:

- `f4x-ai` implements the interface for `Ai`; `f4x-learn` implements it for its agent.
- `f4x-client` implements the human controller. The master must support a controller that
  pauses while awaiting user input and resumes when the client submits commands; it must
  not block the client's event loop.
- The client and console construct the master/session with the game and ordered controllers.
  The master invokes each controller through the same lifecycle, regardless of whether it
  is human- or AI-controlled.

The dependency direction is `f4x-core <- f4x-master <- f4x-client`; AI/controller crates
depend on `f4x-core` and `f4x-master`, while `f4x-master` knows only the controller
interface. The exact public API (push-based commands versus a resumable `take_turn`
callback) is an implementation decision, but the human path must be non-blocking.

The core retains simulation rules and state-changing operations. The master decides when
those operations are called; it does not reimplement city, economy, movement, combat or
magic rules. Undo/replay remain with the game-state/action layer unless an operation
specifically needs to capture master lifecycle state. The save format must persist the
master's round counter, active player and phase/progress as well as the game state; decide
the clean serialization boundary when implementing the master.

## 6. No rename

The name `f4x_core` stays. The new crate is `f4x-static`; add it to `[workspace.dependencies]` and to the
manifests of core, and (where a crate names definition types directly) of the other crates. The
re-exports of section 1 keep the existing paths working.

## 7. Step by step

Each step leaves `cargo test --workspace`, `cargo clippy --workspace --tests`,
`scripts/check_id_aliases.py` and the soak and regression targets unchanged. One commit per step (per
domain in stage 1).

**Stage 0 (task 467): art out of core.** First, before any crate is created. Art, sound and animation data
belong to the client; done first, it makes every later move smaller.
1. List every art field and table in core, rules and `f4x-data` (known so far: `UnitDef.art`,
   `HeroDef.portrait`, `ItemDef.icon` and `Item.icon`, `Portrait`/`PORTRAITS`, `battle/info.rs`
   `InfoItem.icon`, `f4x-data/src/{sound,spell_art}.rs` with `content/{sound,spell_art}.ron`, the font
   colour tables, `hotkeys.ron`). Sort each into: only the client reads it; core logic reads it.
2. Move what only the client reads to `f4x-client` (loader, validation against `Rules`; the `.ron` file
   keeps its place under `content/`; validation that needs `Rules` becomes a client-side test).
3. Where core logic reads a number that is really art (a portrait number that picks a wizard), keep the
   number but name it a content number, not a picture.
4. Remove the dead fields from the rules types and the content files; the saves do not change.

**Stage 1: sort every module into definition, state, action, inside the crate, no new crate yet.**
Domain by domain, smallest and least depended on first (`threat`, `hero`, `item`, `world_event`,
`feature`, `diplomacy`, `player`, `city`, `wizard`, `unit`, `spell`, `battle`, `map`, `game`). For each:
1. Move its mutators into `action/<domain>`, its components, views and `spawn` into `state/<domain>`.
2. Cut out its definition parts (section 2b) into a clearly marked `definition` module of that domain, which
   stage 2 moves to `f4x-static` unchanged.
3. Make sure the test of rule 2 (state never calls an action) passes. The compiler lists every place
   where a read calls a mutator; those are the real work. A `&self` query that needs an `&mut` helper
   is split into the query and a returned plan that an action applies, as `Economy` does.
4. Suite green, commit.

Together with the domains, cut the back-edges that stop the definitions from standing alone:

| Back-edge | Fix |
|---|---|
| `rules/effects.rs` `CUSTOM_HOOKS` names constants of `spell::global_enchantment`, `item::crafting`, `city::gate`, `spell::city`, `battle::field` | define the hook names once in rules (`pub mod hook { pub const BATTLEFIELD: &str = ... }`); the game code uses them from there |
| `SPECIAL_SPELLS` (list of numbers) and `Effect::Special` | already data; moves unchanged |
| `rules/exec.rs` uses `battle`, `game::replay`, `player` | see section 3: through `EffectHost`, or it stays in core |
| `rules/defs.rs` names `hero::HeroAbilities::ids` (doc link only), `text::Gender`, `map::TerrainClass` | drop the doc link; `Gender` and `TerrainClass` move with the definitions |
| `game/setting.rs` uses `mapgen::GenParams`, `city::neutral_city_count`, `player::{Bonus, Neutral, PlayerId}`, `wizard::magic::Retort`, `unit::MAX_STACK`, `MAX_TILE_LIMIT` | `GenParams`, `scaled_count` and `neutral_city_count` go to `action/newgame` (`SettingsDef` keeps plain numbers); the `MAX_*` constants go to `base`; `Bonus`, `Neutral`, `PlayerId` go to `base` if they are plain data, otherwise `GameSettings` stays in core and only `SettingsDef` moves |
| the `Retort` enum (`wizard::magic`) used by `SettingsDef` and `RetortDef` | move the enum to `f4x-static`, or refer to retorts by number in settings (consistent with task 418; related to task 449, retorts as data) |
| `rules/registry.rs` uses `testdir` | `TestDir` moves to `f4x-static::testutil` |

Gate for stage 2: `grep -rn "crate::"` on the files that will move (`rules/`, `text*`, `help.rs`,
`content.rs`, `map/terrain.rs`, `feature/def.rs`, the `definition` modules) must name only each other
and `base`.

**Stage 2: create `f4x-static`** and merge `f4x-data` into it.
1. Move one small type end to end first (for example `Gender`) to see the shape of the errors.
2. `git mv` the files of section 3 (keeping history); add the re-exports to core (section 1); add the
   crate to `[workspace.dependencies]`.
3. Move the loader of `f4x-data` and its tests (table in section 3); replace `load_shipped` by
   `f4x_static::testutil::shipped()` and delete the 308-line copy; `save.rs` of `f4x-data` (the file IO) is not part of the merge: it goes to
   `f4x-core/state/save`, not to `f4x-static`. Remove `f4x-data` from the workspace; callers use `f4x_static::load_content`.
4. **The orphan rule.** Rust forbids an inherent `impl` on a type from another crate, so every
   `impl SomeDefinitionType { ... }` that lives in game code stops compiling. Find them first:
   `grep -rn "^impl \(UnitDef\|SpellDef\|...\)" crates/f4x-core/src` for each moved type. Fix each by
   moving the impl into `f4x-static` (if it needs no state) or turning it into an extension trait or a
   free function in core. Check the same for `serde` default functions and `Deserialize` impls that call
   core helpers.
5. Fix `scripts/export_original_*.py` (5 scripts), `scripts/untested_content.py` and
   `scripts/test_changed.py` (its `CORE_SRC` module graph, and a new rule: a change in `f4x-static` runs
   everything); update `docs/scripts.md`.
6. Docs: `CLAUDE.md`, `README.md`, `docs/design.md` (crate list at lines 22 to 27),
   `docs/ecs-structure.md`, `docs/rust_coding_rules.md` (8 mentions), `docs/text.md`,
   `docs/rules-framework.md`, and this file.
7. Measure compile time before and after (`cargo build --timings`, `cargo nextest run --workspace`, 3.6 s
   now): a change in `f4x-static` rebuilds the whole workspace, as a change in the types does today.

**Stage 3: the master and player controllers.**
1. Measure every loop that sends `Command::EndTurn` or runs `play_round`, and every `f4x_ai::` call in
   the client and console. Record the current human and AI turn entry points.
2. Create `f4x-master` depending on `f4x-core`. Move round/player orchestration out of core: round
   counter, player order, active controller, phase transitions and once-per-round calls belong to the
   master. Core keeps game rules and focused state-changing operations.
3. Define a controller interface in `f4x-master`. Implement it for `Ai` and the learned agent; implement
   the human controller in the client with a resumable, non-blocking input path. Do not add a dependency
   from master to client or AI.
4. Move world-event selection/application to round start, before the first player controller is invoked.
   Classify other existing turn-pipeline steps as player-turn, round-start or round-end work before
   migrating them; never run the entire old pipeline once per player.
5. Port the console and soak tools first, then `f4x-learn`, then the client. Keep controller order and
   lifecycle identical across hosts.
6. Test the master-driven game against a scripted sequence of core actions (same seed and commands),
   and test that round-start events happen exactly once before any player acts. Save/load must resume
   the master phase without repeating or skipping a controller or boundary operation.

**Stage 4 (ongoing):** a function in state or action that turns out to take no state is a definition;
move it to `f4x-static`.

**Where tests go.** A test that needs only definitions and content: `f4x-static/tests/it`, on
`shipped()`. A unit test that needs a module's private items: stays in the file. A test that builds a
whole game and runs commands: `f4x-core/tests/it`. No crate keeps its own loader for tests; if one is
needed, it is a bug in `f4x-static`.

## 8. A against B

| | A | B |
|---|---|---|
| Definitions in a crate of their own | `f4x-rules`; `f4x-core` stays the simulation | `f4x-static`; `f4x-core` stays the game |
| `f4x-data` | into `f4x-rules` | into `f4x-static` |
| Battle engine | stage D, own crate, not planned | stays in `f4x-core` as `state/battle`, `action/battle`; only the static values move |
| New game, save | two more crates, public interface to design | modules of `f4x-core`, nothing to expose |
| Turn/round orchestration | not covered | `f4x-master` between core and client; AI and human controllers implement its interface |
| Inside the big crate | unchanged | split into `state/` and `action/` by rule, per module |
| Renames | none | none |
| A change to a rule rebuilds less | yes | yes (`f4x-static` is small and at the bottom) |
| A change to game code rebuilds less | no | no: core stays one large crate |
| Crates added | `f4x-rules`, `f4x-newgame`, `f4x-save` | `f4x-static`, `f4x-master` |
| Tasks | 455, 456 (into rules), 458, a new save task | 455 becomes `f4x-static`, 456 (into `f4x-static`) stays, 458 drops out; new: sort the modules (stage 1), master/orchestration |

The two agree on the cut of the definitions and on stage 0. They differ in whether new game and save are
crates above the simulation (A) or modules in the same crate as `Game` (B), and in B's state/action
split and separate master crate.

## 9. Risks and open questions

| # | Risk or question | Answer |
|---|---|---|
| 1 | The AI/game cycle | handled by the controller interface in `f4x-master` (section 5); separate master crate chosen by the user, 4 November 2026 |
| 2 | `f4x-core` stays about 70,000 lines | accepted; the state/action split inside it keeps it readable, and it can become two crates later |
| 3 | Reads that call mutators (rule 2) | unknown until the first domain is sorted (stage 1, step 3) |
| 4 | Orphan rule: `impl` blocks on definition types in game code | stage 2, step 4; count with the compiler |
| 5 | Is the effect executor free of the game through `EffectHost`? | check before stage 2; `exec.rs` mentions `battle::immunity`, `game::replay`, `player`; fallback in section 3 |
| 6 | The battle split between static rules and engine | **Decided by the user, 2 November 2026:** static values in `f4x-static`, engine in core. The exact list is made in stage 1. If the engine later wants its own crate, it can go between `f4x-static` and `f4x-core` because it is free of `Game`; not planned |
| 7 | How big are the definition parts inside the blob? | not measured; the first domain of stage 1 gives the ratio |
| 8 | Does the client still need `f4x-console`? | it depends on it today; may become unneeded once both use `f4x-master`. Not checked |
| 9 | `Retort`, `Bonus`, `Neutral` in the settings | stage 1: move the plain ones to `f4x-static`, or refer to retorts by number (task 449 decides) |
