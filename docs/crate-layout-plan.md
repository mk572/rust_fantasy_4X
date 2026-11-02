# Crate layout: plan to resort the crates

Asked for by the user, 2 November 2026. Covers tasks 455 (`f4x-rules`), 456 (merge `f4x-data`) and
458 (`f4x-newgame`), and adds `f4x-save` and `f4x-battle`. Numbers below were measured on the code
that day (non-test lines unless stated).

## 1. Where we are

```
f4x-core  (83k lines)  <-- depends on --> f4x-data (3.6k src, 1.3k tests)
                         (core: normal dependency of data; data: dev-dependency of core)
f4x-ai, f4x-ai-ref, f4x-learn, f4x-console, f4x-client (44k), f4x-tools: all depend on core + data
```

### Why the core / data split did not work

- The **types** of the content (`Rules`, `UnitDef`, `SpellDef`, `Terrains`, `Features`, `SettingsDef`,
  `Texts`, `Content`) live in `f4x-core`; the **loader and validator** live in `f4x-data`, which
  needs those types. So data depends on core.
- Core's tests need loaded content. Core cannot depend on data (cycle), and a dev-dependency cycle
  gives the unit tests in `src/` two copies of the core types, so `f4x_data::load_content` cannot
  be used there.
- The workaround is `game/testutil.rs::load_shipped()` (308 lines): a second, smaller loader that
  skips the validation, the messages, the help and the hero names. 59 core files use `shipped()`
  (170 calls). Task 430 recorded that it "can drift", and a guard test is impossible for the same
  reason.

So the split cut in the wrong place: it separated the types from the code that builds them. The fix
is to keep types and loader together, and to cut where the dependency really is one-way.

### What the code says about the seams (measured)

Non-test coupling between the modules of core (`a -> b(n)`: n references to `crate::b` from `a`):

| Module | Lines | Depends on |
|---|---|---|
| `rules` | 2,694 | map 8, game 4, battle 3, text 3, spell 2, player/item/city/hero 1 each |
| `map` | 2,985 | feature 10, rules 7, unit 5, player 4, game 3 |
| `battle` | 11,217 | rules 55, game 19, hero 12, unit 11, spell 9, map 6, player 6 |
| `spell` | 6,120 | rules 97, player 21, game 19, map 15, unit 14, hero 12, wizard 11 |
| `game` | 7,757 | nearly everything |
| `wizard`, `unit`, `player`, `city`, `feature`, `item`, `hero`, `diplomacy` | | each uses `game`, `rules`, `player`, and one another |

Two facts decide the plan:

1. **`rules` has almost no real dependencies on the simulation.** Its back-edges are a handful of
   constants and small types (section 3.1). It can go below core.
2. **The rest of core is a tangle** (`spell`, `unit`, `player`, `wizard`, `city`, `game` all use each
   other). Carving out a crate from that middle costs a lot and gains little, except `battle`
   (section 3.4).

## 2. Target layout

```
f4x-rules     content types, loader, validator, texts, help, settings definitions
   ^
f4x-core      the simulation (Game, commands, battle glue, ...)      [and f4x-battle below it, 3.4]
   ^
f4x-newgame   map generator, wizard setup, enemies, neutral cities, lairs
f4x-save      snapshot types, file reading and writing, version check
   ^
f4x-ai, f4x-console, f4x-learn, f4x-client, f4x-tools
```

### The question of direction

The request said "dependencies of f4x-core". That is true only for `f4x-rules`. `f4x-newgame` and
`f4x-save` have to build or read a `Game`, so they sit **above** core (core cannot depend on them
without a cycle, as task 458 already notes). This is fine because core's runtime code never calls
them: the only callers inside core are tests (`new_game(` appears 24 times in core, mostly tests, plus the snapshot round-trip tests in `game/replay`, `game/undo`, `unit/transport`,
`spell/hero`, `spell/temp_effect`). Section 5 says where those tests go.

### What goes where

**`f4x-rules`** (everything that is data about the rules, with its loader)

| From | To | Notes |
|---|---|---|
| `core/rules/{defs,ids,lair,refs,registry,stats,effects}.rs` | `rules` | `exec.rs` stays in core (below) |
| `core/map/terrain.rs` (`TerrainDef`, `Terrains`, `TerrainClass`) | `rules::terrain` | |
| `core/feature/def.rs` (`FeatureDef`, `Features`, `FeatureKind`) | `rules::feature` | |
| `core/game/setting.rs`, definition half (`SettingsDef`, `ClientSettingsDef`, difficulty levels, sizes, `SpellLibraryDef`) | `rules::setting` | the difficulty settings; the chosen `GameSettings` stays in core |
| `core/text.rs`, `core/text/` (`Texts`, `Gender`) and `core/help.rs` | `rules::text`, `rules::help` | texts are content |
| `core/content.rs` (`Content`) | `rules::content` | |
| all of `f4x-data/src` (`lib.rs` loaders, `rules.rs`, `sound.rs`, `spell_art.rs`) | `rules::load`, `rules::sound`, `rules::spell_art` | `sound` and `spell_art` move to the client in stage 0 (task 467) before this step |
| basic vocabulary the above name: `Plane`, `TerrainId`, `FeatureId`, `Position`, `PlayerId`, `Gender` | `rules::base` | see 3.4 for the later `f4x-base` |
| test support: `shipped()`, `content_dir()`, `num` constants, `TestDir` | `rules::testutil` (public, small) | one loader for tests |

**`f4x-newgame`**

| From | Notes |
|---|---|
| `core/map/mapgen.rs`, `mapgen/original.rs` (1,076 lines) | needs `feature::place`, `node::generate_nodes`, `lair::generate_lairs`; those placement generators (about 1,000 lines of `feature/`) move too, the entity components they create stay in core |
| `core/game/setup.rs`, `setup/original.rs` (894 lines): `new_game`, `NewGame`, `parse_retort` | builds the `Game` through `Command`s, so it only needs core's public API |
| `GenParams`, `scaled_count`, `neutral_city_count` | moved out of `setting.rs` and `city/neutral.rs`; `SettingsDef` keeps plain numbers |
| the client's `newgame_flow.rs` / `newgame_picks.rs` | stay in the client; they call this crate |

**`f4x-save`**

| From | Notes |
|---|---|
| `core/game/save.rs` (1,765 lines) and its tests (734), `game/serial.rs`, the `battle/save.rs` snapshot structs | the format (`SAVE_VERSION` 4) does not change |
| `f4x-data/src/save.rs` (221 lines): `save_game`, `load_game`, atomic write | |

Risk (section 6): `save.rs` reads 25 `pub(crate)` fields of `Game` and calls `pub(crate)` restore
methods (`restore_battle`, `Diplomacy::restore`). Moving it out means exposing a deliberate interface
from core. Do this last.

**Stays in `f4x-core`:** `game` (minus setup/save/settings definitions), `unit`, `city`, `wizard`,
`player`, `spell`, `hero`, `item`, `diplomacy`, `world_event`, `threat`, `turn`, `feature` entities
and `map` (the `Map`, `Grid`, fog, vision, roads, survey, corruption), `rules/exec.rs` (417 lines;
it runs effects and uses `battle::immunity`, `game::replay`, `player`, so it moves to `spell/exec.rs`).

## 3. Review: other structures and carve-outs

### 3.1 Back-edges that must be cut before `f4x-rules` can exist

| Back-edge | Fix |
|---|---|
| `rules/effects.rs` `CUSTOM_HOOKS` names constants of `spell::global_enchantment`, `item::crafting`, `city::gate`, `spell::city`, `battle::field` | define the hook names once in rules (`pub mod hook { pub const BATTLEFIELD: &str = ... }`); core's code uses them from there |
| `SPECIAL_SPELLS` (list of numbers) and `Effect::Special` | already data, moves unchanged |
| `rules/exec.rs` uses `battle`, `game::replay`, `player` | stays in core, see above |
| `rules/defs.rs` names `hero::HeroAbilities::ids` (doc link only), `text::Gender`, `map::TerrainClass` | doc link dropped; `Gender`, `TerrainClass` move with rules |
| `setting.rs` uses `mapgen::GenParams`, `city::neutral_city_count`, `player::{Bonus, Neutral, PlayerId}`, `wizard::magic::Retort`, `unit::MAX_STACK`, `MAX_TILE_LIMIT` | `GenParams` and the two counting functions go to newgame; `MAX_STACK` constants go to rules; `Bonus`, `Neutral`, `PlayerId` go to `rules::base` if they are plain data, otherwise `GameSettings` stays in core |
| `Retort` enum (`wizard::magic`) used by `SettingsDef` and `RetortDef` | move the enum to rules, or refer to retorts by number in settings (consistent with task 418). Related to task 449 (retorts as data); decide there |
| `registry.rs` uses `testdir` | `TestDir` moves to `rules::testutil` |

Use **re-exports to keep the diff small**: after the move, `f4x_core::rules` is `pub use f4x_rules::*;`
(and likewise `content`, `text`, `map::terrain`, `feature::def`), so the several hundred uses of
`crate::rules::...` and the other crates' imports keep compiling. Remove the re-exports later, one
module at a time, only if wanted.

### 3.2 `f4x-data` is merged into `f4x-rules`, not into core (decision for task 456)

Task 456 allows either. `f4x-rules` is better: the loader is only useful with the types, the types
are the lower layer, and a rules change then rebuilds only rules and what is above it. The tests
of `f4x-data` split as follows:

| Test file | Lines | Needs core? | Goes to |
|---|---|---|---|
| `original_content.rs` | 843 | `rules`, `wizard` (a retort enum) | `f4x-rules/tests/it` (retort moved with 3.1) |
| `text.rs`, `shipped.rs`, `stacking.rs` | 246 + 14 + 58 | `rules`, `text`, `content` | `f4x-rules/tests/it` |
| `autobuild.rs` | 119 | `city` | `f4x-core/tests/it` |

### 3.3 `f4x-newgame` and `f4x-save`

Covered above. Two extra points:

- The mapgen tests (`map/mapgen.rs` tests, `mapgen/original/tests.rs`) and the `setup` tests move
  with their code, which removes `new_game(` from most of core's unit tests only partly: the unit
  tests that need a ready game (`battle`, `turn`, `wizard/fortress`, `magic_game`, ...) use
  `game/testutil.rs` helpers, not `new_game`; check this when counting (step 3).
- `f4x-ai`, `f4x-console`, `f4x-learn`, `f4x-tools` and the client call `new_game` or the map generator in about 17 files. They
  add one dependency and change one import path each.

### 3.4 `f4x-battle` (added at the user's request)

**Status (2 November 2026): not planned for now. No task exists for it; stage D below is a record of the analysis only.**

Feasible, but as a second stage and only for the engine, not for the whole `battle/` folder.
`battle/` is 11.2k non-test lines (18.7k with tests), the biggest module of core, and it splits:

| Part | Files | Lines | Depends on |
|---|---|---|---|
| **Engine** (state and rules of a fight on its own) | `mod.rs` (the `Battle`, `BattleUnit`), `attack.rs`, `field.rs`, `special.rs`, `vortex.rs`, `ai.rs`, `immunity.rs`, `legal.rs`, `info.rs`, `map.rs`, `map/layout.rs`, and the pure half of `casting.rs`, `spell.rs`, `spell/unit_cast.rs` | about 7,000 | `rules` (55), and a few small things: `game::rng`, `game::replay`, `game::serial`, `game::setting`, `spell::dispel`, `spell::instance`, `spell::target_error`, `map::Position`, `PlayerId` |
| **Glue** (battle as part of the game) | `game.rs` (1,316: begin, end, leave, overland units, heroes, items, diplomacy), `spoils.rs`, `save.rs`, `brain.rs`, the `Game`-touching half of `spell.rs` | about 4,200 | `Game`, `unit`, `hero`, `item`, `diplomacy`, `wizard` |

What is needed first:

1. The small shared pieces the engine uses move down: `GameRng`, `Serial`, `Position`, `PlayerId`,
   `Plane`, the dispel and spell-instance helpers it reads. They are pure values. Collect them
   in a small **`f4x-base`** crate (ids and primitives: `Position`, `Plane`, `PlayerId`, `Serial`,
   `GameRng`, `Gender`). Until `f4x-battle` is started, keep them in `rules::base` and split
   `f4x-base` out then; rules, battle and core depend on it.
2. The engine must not name `unit::Hurt`, `hero::Hero` and similar. Check these in `battle/game.rs`
   (they are glue, which stays) and in `info.rs`/`immunity.rs`.
3. The four dependants of `battle` outside it (`rules` 3 constants, `unit` 5, `spell` 7, `game` 1)
   already point from core to the engine, which is the right direction.

Benefit: the fight (the largest, most self-contained rule set) builds, tests and is read on its own;
its 7,500 lines of tests no longer rebuild with every `wizard` change. Cost: the engine/glue split of
`spell.rs` (1,806 lines) and `casting.rs` is real work. Recommended order: after `f4x-save`, as its own
task, unless compile times of core become the pain first.

### 3.5 Looked at and not recommended now

| Candidate | Why not now |
|---|---|
| `f4x-map` (`Map`, `Grid`, fog, vision, roads) | depends on `feature` (10), `unit` (5), `player` (4), `city` (2), `spell` (2); the pure grid half is small and goes to `rules::base`/core anyway |
| `f4x-spell`, `f4x-unit`, `f4x-wizard`, `f4x-city`, `f4x-diplomacy`, `f4x-threat` | each uses `Game`, `player` and the others in both directions (`spell` -> `unit` 14, `unit` -> `spell` 13). Making them crates means designing the context interfaces of `docs/architecture-plan.md` first. That is a separate refactor, not a crate move |
| `f4x-text` | `text.rs` (438 lines) and `help.rs` (209) are content; they live in rules |
| `f4x-testkit` | `shipped()`, `num` and `TestDir` fit in `rules::testutil`. A later `f4x-testworld` (the `Game` builders: `flat_game`, `testworld.rs` of `f4x-ai`) could be shared by core, ai, console and learn; not needed yet |
| split of `f4x-client` (44k lines, 90 files) | possible later: art and asset loading (`art.rs`, `font*.rs`, `unit_sheet.rs`, `river_art.rs`) versus screens. Not measured here. The main gain would be compile time, so measure that first (task 445) |
| `f4x-ai-ref` | a frozen copy of the AI (7.4k lines). Whether it stays in the workspace is part of task 460 |

## 4. Step by step

Each step leaves `cargo test --workspace`, `cargo clippy --workspace --tests`, `scripts/check_id_aliases.py`
and the soak/regression targets unchanged. One commit per step.

**Stage 0: art out of core (task 467, first, before any crate is created)**

Art, sound and animation data does not belong in `f4x-core` or `f4x-rules`: it is the client's. Done
first, it makes every later move smaller (fewer fields on the rules types, no `sound` and `spell_art`
tables to carry). Steps:

1. Measure: list every art field and table in core, rules and `f4x-data` (known so far: `UnitDef.art`,
   `HeroDef.portrait`, `ItemDef.icon` and `Item.icon`, `Portrait`/`PORTRAITS`, `battle/info.rs`
   `InfoItem.icon`, `f4x-data/src/{sound,spell_art}.rs` with `content/{sound,spell_art}.ron`, the
   font colour tables, `hotkeys.ron`). Sort each into: only the client reads it; core logic reads it.
2. Move what only the client reads to `f4x-client` (loader, validation against `Rules`, the `.ron`
   file keeps its place under `content/`). Validation that needs `Rules` stays a client-side test.
3. Where core logic reads a number that is really art (a portrait number that picks a wizard), keep
   the number but name it a content number, not a picture.
4. Remove the dead fields from the rules types and the content files; the saves do not change.

**Stage A: `f4x-rules` + merge `f4x-data` (tasks 455 and 456, done together)**

1. *Inside core, no crate yet.* Cut the back-edges of 3.1: hook-name constants into `rules`, `exec.rs`
   to `spell/exec.rs`, `GenParams`/`scaled_count`/`neutral_city_count` out of `setting.rs`, split
   `setting.rs` into definitions and `GameSettings`, put `Plane`, `TerrainId`, `FeatureId`, `Position`,
   `PlayerId`, `Gender`, `MAX_STACK` in one `base` module. Decide Retort (3.1) here.
2. Run `grep -rn "crate::" crates/f4x-core/src/{rules,text*,help.rs,content.rs}` and
   `map/terrain.rs`, `feature/def.rs`: the result must name only each other and `base`. That is the
   gate for step 3.
3. Create `crates/f4x-rules`; move the files of the table (git mv, keeping history); add
   `pub use f4x_rules::...` re-exports in core. Add to `[workspace.dependencies]`.
4. Move the content of `f4x-data/src` into `f4x-rules` (`load`, `sound`, `spell_art`); `save.rs` waits
   in `f4x-core` as `game/file.rs` until stage C.
5. Replace `game/testutil.rs::load_shipped` by `f4x_rules::testutil::shipped()`. Delete the 200-line
   copy. The two loaders are now one, so task 430's drift problem is gone.
6. Move the tests of `f4x-data` as in 3.2. Remove `f4x-data` from the workspace; users call
   `f4x_rules::load_content`. Fix `scripts/export_original_*.py` (5 scripts), `scripts/untested_content.py`,
   `scripts/test_changed.py` (its `CORE_SRC` module graph, and a rule "a change in `f4x-rules` runs
   everything"), `docs/scripts.md`.
7. Docs: `CLAUDE.md`, `README.md`, `docs/design.md` (crate list at lines 22 to 27), `docs/ecs-structure.md`,
   `docs/rust_coding_rules.md` (8 mentions), `docs/text.md`, `docs/rules-framework.md`, and this file.
   Task 456 is done with step 6; 455 with step 3.

**Stage B: `f4x-newgame` (task 458)**

1. Move the feature placement generators and `mapgen` (3.3), then `setup`. Core keeps the public API they
   use (`Game::apply`, `Command`, `Game::from_parts` or the existing constructors); list any `pub(crate)`
   items they touch and make them `pub` only if the use is legitimate.
2. Move the tests with the code. Core tests that called `new_game` use the helpers of
   `game/testutil.rs` or move to `f4x-newgame/tests/it`.
3. Update the callers in the other crates.

**Stage C: `f4x-save` (new task)**

1. First measure: list every `pub(crate)` field and method that `save.rs` and `battle/save.rs` use (25 field
   accesses plus the restore methods). Decide the interface: either `Game::to_parts()` / `Game::from_parts()`
   returning plain owned values (world, map, fog, rng, settings, serial, diplomacy, battle), or keep
   `Snapshot` in core and move only the file I/O. The second is cheap; the first matches the goal.
2. Move `Snapshot` and the format; keep `SAVE_VERSION` and the regression saves in `tests/regression`.
3. The round-trip tests inside core (`undo`, `replay`, `transport`, `hero`, `temp_effect`) either move to
   `f4x-save/tests/it` or use `Game::to_parts` directly. Undo (`game/undo.rs`) uses snapshots, so check
   whether it needs the format at all or only a clone.

**Stage D: `f4x-battle` (not planned for now, no task; 3.4)**

Pull the primitives into `f4x-base`, split `battle/spell.rs` and `casting.rs` into engine and glue, move
the engine, re-export `f4x_core::battle`.

**Possible future step E: entity definitions into `f4x-rules` as a chunk (not planned, no task)**

After stage A, `f4x-rules` holds the definition types and the loader, but the entity definitions
(spells, units, buildings, items, heroes, abilities, retorts and the like, `rules/defs.rs` with their
`.ron` files) may still be tangled with core through the back-edges of 3.1. A later step moves them
as one chunk: first make the whole definition set free of core, then `git mv` it in one commit
instead of type by type. Measure first what stage A leaves behind in core; if stage A already moves
`defs.rs` whole, this step is empty.

## 5. Where tests go (rule of thumb)

- A test that needs only rules and content: in `f4x-rules/tests/it`, on `shipped()`.
- A unit test that needs a module's private items: stays in the file in core, as now.
- A test that builds a whole game and runs commands: core's `tests/it` or the crate that owns the
  feature (`f4x-newgame`, `f4x-save`).
- No crate keeps its own loader for tests; if one is needed, it is a bug in `f4x-rules`.

## 6. Risks and open decisions

| # | Risk or decision | Suggested answer |
|---|---|---|
| 1 | Two-way direction wording of the request ("dependencies of core") | newgame and save sit above core (section 2); only rules is below |
| 2 | `save.rs` needs private fields of `Game` | do stage C last, with an explicit parts interface; measure first |
| 3 | `Retort` enum and `Bonus`, `Neutral` in the settings | move the plain ones to rules; or refer to retorts by number (needs task 449 to decide) |
| 4 | Build time: a change in `f4x-rules` rebuilds the whole workspace | already true today (types in core); measure before/after with `cargo build --timings` and `cargo nextest` (3.6 s now) |
| 5 | Orphan-rule problem: `impl SomeRulesType { ... }` blocks in core for types that moved to rules | Rust forbids inherent `impl` for a foreign type. Count them in step 2 (`grep -rn "^impl \(UnitDef\|SpellDef\|...\)" crates/f4x-core/src`); move the impl to rules, or turn it into an extension trait or a free function |
| 6 | `serde` default and `Deserialize` impls that call core helpers | same check as 5 |
| 7 | Where `sound.rs` and `spell_art.rs` belong | the client, in stage 0 (decided by the user, 2 November 2026) |
| 8 | The name `f4x-base` and whether to create it before stage D | keep `rules::base` until D |

Risk 5 is the one most likely to cost real time: it can only be counted with the compiler, so the
first thing step 1 does is move one small type (for example `Gender`) end to end to see the shape of
the errors.

## 7. Tasks

| Task | Now | Becomes |
|---|---|---|
| 467 | art out of core into `f4x-client` | Stage 0, first |
| 455 | new `f4x-rules` | Stage A steps 1 to 3, 5 to 7 |
| 456 | merge `f4x-data` into core | Stage A step 4 and 6: merge into `f4x-rules` instead |
| 458 | new `f4x-newgame` | Stage B |
| new | `f4x-save` | Stage C |
| none | `f4x-battle` and `f4x-base` | Stage D: not planned for now, no task |
