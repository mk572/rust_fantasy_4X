# Crate layout: plan C (runtime-loadable rulesets and mods)

Plan C extends plan B. It keeps the crate boundaries and the definition/state/action split of plan B, but makes the definitions in `f4x-static` genuinely replaceable at runtime.

**Goal:** a player can select a ruleset or install a mod by placing definition files in a local folder. The game loads those files at startup. Adding or changing a unit, building, spell, race, ability, item, hero, realm, terrain definition or other supported content must not require recompiling Rust.

This is a design and implementation plan, not a commitment to implement every content type in the first loader milestone. The first usable milestone must support new and overridden units, buildings and spells, plus every definition type they depend on.

## 1. Decisions inherited from plan B

Plan C preserves these plan-B decisions:

- `f4x-core` remains the game crate; it is not renamed and there is no `f4x-game` crate.
- `f4x-static` owns definition types, registries, validation, text/help definitions, effect primitives and the static battle rules.
- `f4x-core` owns game state, state construction, actions, save/load and the session. State and actions remain in separate `state/` and `action/` folders.
- Definitions know nothing about game state or actions. State reads definitions; actions are the only code that changes state.
- The tactical battle engine remains in `f4x-core`; only static battle values and definitions belong in `f4x-static`.
- The AI continues to depend on `f4x-core` and implement its `Seat` trait.
- The existing `HalfInt` representation remains an integer-backed half-step type. Content parsing must preserve exact half values and must not introduce floating-point game arithmetic.
- Art and other client-only presentation resources remain separate from rules definitions. A ruleset may refer to logical asset keys, but it must not make the simulation depend on image or sound files.

Plan C does not undo the module sorting or crate moves in plan B. It changes the responsibilities and interfaces of the definition loader as those steps are implemented.

## 2. Core architecture

The central object is an immutable, fully validated `RuleSet` loaded before a game starts.

```
local ruleset/mod folders
        |
        v
f4x-static::load
  read manifests and definition files
  parse RON
  apply ordered overlays
  resolve external string IDs to typed IDs
  validate schemas, references and constraints
        |
        v
f4x-static::RuleSet (immutable registries)
        |
        +--------------------+
        v                    v
f4x-core                f4x-ai
game state/actions      reads the same RuleSet through Game
        |
        v
f4x-client / console / tools
```

The loaded `RuleSet` is passed explicitly into new-game construction and retained by the game/session for its lifetime. No simulation system reads arbitrary definition files, scans directories, or performs lazy content loading during a turn.

Recommended API shape (names are indicative):

```rust
let ruleset = RuleSetLoader::new(content_root)
    .load(RuleSetRequest::new("classic"))
    .map_err(RuleSetLoadError::into_report)?;

let game = Game::new(ruleset.clone(), settings, seed)?;
```

The exact ownership mechanism (`Arc<RuleSet>` is the likely choice) can be decided during implementation. The important contract is one immutable ruleset snapshot per game. A running game never changes underneath the simulation because a file was edited on disk.

## 3. On-disk layout

Use RON, matching the existing content pipeline and `rules-framework.md`. Do not introduce a second serialization format just for mods.

Suggested layout:

```text
<game-data>/
  rulesets/
    classic/
      manifest.ron
      rules.ron
      races/
        highborn.ron
        klackon.ron
      units/
        spearman.ron
        archer.ron
      buildings/
        barracks.ron
      abilities/
        first_strike.ron
      realms/
        life.ron
      spells/
        fireball.ron
      heroes/
      items/
      enchantments/
      terrain/
      features/
      text/
  mods/
    example_mod/
      manifest.ron
      units/
        clockwork_guard.ron
      spells/
        lightning_lance.ron
      overrides/
        units/
          spearman.ron
```

The actual platform-specific root should be resolved by the application (for example, a user data directory), with an optional command-line/config override for development and portable installs. Keep the loader itself platform-neutral: it receives a root path and never guesses OS paths.

A ruleset directory is a complete playable base set. A mod directory is an optional overlay applied to a selected ruleset. The base game ships at least one ruleset in the normal game data installation; users can add more directories without changing the executable.

### 3.1 Manifests

Each ruleset and mod has a manifest with a stable ID, display name, format/schema version and metadata. A mod manifest can declare dependencies, incompatibilities and a minimum/maximum supported ruleset API version.

Illustrative manifest:

```ron
(
    id: "classic",
    name: "Classic Rules",
    format_version: 1,
    rules_api_version: 1,
)
```

Illustrative mod manifest:

```ron
(
    id: "example_mod",
    name: "Example Mod",
    format_version: 1,
    rules_api_version: 1,
    requires: ["some_library_mod"],
    incompatible_with: [],
)
```

Do not let a mod manifest execute code. Dependency resolution and validation are data-only.

### 3.2 One definition per file

Use one definition per file, with the filename matching its external ID. This makes it easy to add a unit by dropping one file into `units/`, review diffs, and override one definition without copying an entire registry.

For example, `units/clockwork_guard.ron` contains one `UnitDef`. The loader verifies that the declared ID matches the expected path/filename. A future convenience format may allow grouped files, but it must normalize to the same internal definitions and must not be required for mods.

The loader must enumerate files deterministically (stable sort by normalized relative path). Never rely on filesystem enumeration order.

## 4. IDs and registries

The existing typed-ID/registry approach remains useful inside the engine, but file content needs stable symbolic IDs.

- Definitions use stable external string IDs (for example, `spearman`, `fireball`, `barracks`).
- References in RON use those strings, not array positions or registry insertion order.
- During loading, the loader resolves strings into the existing typed IDs (`UnitId`, `SpellId`, `BuildingId`, etc.).
- Runtime instances continue to store compact typed IDs, not strings.
- Registry allocation order is deterministic for a given fully composed ruleset.
- IDs are unique within a definition kind. A mod may add new IDs; replacing an existing ID is an explicit override, never an accidental duplicate.
- New mod-defined IDs should be namespaced (for example, `example_mod:clockwork_guard`) to avoid collisions. Existing base IDs can remain unchanged for compatibility.

A reference to an unknown ID is a load error with the source file and field path. Do not permit unresolved references to reach the simulation.

## 5. Composition: rulesets and mods

A ruleset is loaded first, then zero or more mods are applied in an explicit, deterministic order.

Composition rules:

1. Load the selected ruleset as the complete base definition set.
2. Resolve mod dependencies and reject missing, cyclic or incompatible dependencies.
3. Apply enabled mods in the user-selected order, subject to dependency ordering.
4. A new definition ID adds a definition.
5. An existing ID is unchanged unless the mod explicitly places a replacement in `overrides/<kind>/<id>.ron`.
6. A duplicate definition without an explicit override is an error, not last-writer-wins.
7. A mod may not silently remove a definition. If deletion is later needed, add an explicit, validated tombstone mechanism with clear reference-failure behaviour.
8. Validate the final composed ruleset as a whole, not only each mod in isolation.

The loader should report the provenance of each final definition: base ruleset or the mod that added/replaced it. This is important for diagnostics and a future in-game mod/content browser.

For the first implementation, do not support arbitrary patch/merge semantics within a definition. An override replaces the complete definition. This is predictable and easy to validate. Fine-grained patching can be considered later if real mod-authoring experience shows it is needed.

## 6. What is data and what still requires Rust

The existing rules framework already aims to express content with definitions, stats, conditions, targets, effects and triggers. Plan C makes that framework available to local files.

### Content that should be addable without recompilation

At minimum:

- Units, including their stats, costs, upkeep, movement type, abilities, requirements, race restrictions and level path.
- Buildings, including costs, requirements, race restrictions and effects.
- Spells, including realm, research/casting costs, context, target, duration, resistance and effects.
- Abilities and enchantments, using the existing effect/trigger primitives.
- Races, realms, heroes, items, terrain and feature definitions as their definition schemas are moved into `f4x-static`.
- Text, help entries and logical asset references, where those are part of the content model.

A mod adding a unit can refer to an existing ability or add a new ability in its own file. A mod adding a spell can use existing effects and targets or combine supported primitives in a new way.

### Content that still requires engine support

The engine remains deliberately closed over executable mechanics:

- New stat kinds (the `Stat` enum).
- New effect primitives, target selector kinds, condition kinds or trigger kinds.
- New movement/combat algorithms or a new damage/resistance model.
- New custom Rust hooks and special-case spell behaviour.
- New serialized runtime component types or changes to the save model.

If a mod asks for an unsupported stat/effect/target/hook, loading fails with a precise diagnostic. Never silently ignore it.

Do not add embedded Lua, Rhai, WASM or arbitrary executable plugins in this plan. The existing declarative effect system is the safe, deterministic extension point. If the primitive set proves insufficient, add a tested Rust primitive and increment the rules API version.

### Special cases must stop depending on numeric IDs

Audit `CUSTOM_HOOKS`, `SPECIAL_SPELLS` and any other code that branches on a definition's numeric ID or position. Special behaviour must use a stable semantic hook key or an explicit typed field that the validator checks. No code may assume that spell 133 is Resurrection or that a particular definition occupies a fixed registry index.

A custom hook key is still implemented in Rust and therefore is not user-inventable. But a mod can select an existing supported hook by its stable name, without depending on registry order or a magic number.

## 7. Validation and diagnostics

Loading is an all-or-nothing operation. The game must not start with a partially loaded ruleset.

Validation has several passes:

1. **Manifest and filesystem:** supported versions, valid IDs, readable files, expected directories, no path traversal, no duplicate normalized paths, and valid dependency graph.
2. **Parsing/schema:** every file parses into the expected definition type; required fields, ranges and enum variants are valid.
3. **Identity:** filename and declared ID agree; duplicate IDs require explicit override.
4. **References:** every race/unit/building/spell/ability/item/terrain/feature reference resolves in the final registry.
5. **Semantic rules:** requirement cycles, impossible or unreachable content, spells without effects, invalid target/effect combinations, units unavailable to any race (where that is disallowed), and other domain invariants.
6. **Engine compatibility:** every stat, effect, condition, target, trigger and hook is supported by this executable.
7. **Cross-content checks:** validate the final merged set, since one mod may add references to another mod's content.

Errors should include:
- ruleset and mod ID;
- relative file path;
- definition ID and field path where available;
- concise explanation and expected value/range.

Collect independent errors and report them together where practical. A failed mod should not be silently skipped and the game should not fall back to a different ruleset without telling the user.

The loader must treat content files as untrusted input: impose sensible file/count/size/depth limits, reject path traversal, avoid following paths outside the selected content root, and never deserialize arbitrary code. Do not allow content to choose arbitrary filesystem paths for assets or includes.

## 8. RuleSet lifetime, game state and saves

A game uses one immutable `RuleSet` snapshot from new-game creation until the game ends.

- New-game setup receives a `RuleSet` explicitly and builds state from its definitions.
- All systems that need definitions read through the game's ruleset reference.
- AI and front ends use the same ruleset as the simulation.
- Do not reload definitions while a game is running. Editing files affects the next game only.
- A ruleset selection and enabled mod order are part of the game/session setup and must be visible in the UI and logs.

Saves currently store game state and typed definition references, not a self-contained copy of all content. Preserve that model for the first implementation, but make compatibility explicit:

- Store the ruleset ID, rules API/schema version, enabled mod IDs in resolved order, and a deterministic content fingerprint in the save metadata.
- On load, compose the requested ruleset/mod set and compare its fingerprint with the save.
- If the exact content is unavailable or the fingerprint differs, fail with a useful message. Do not reinterpret saved typed IDs against a different registry.
- A future self-contained save or content snapshot can be designed separately; it is not required for this plan.
- A ruleset change that alters definition meaning is not a save migration. Save migration must be explicit and versioned.

The fingerprint should be computed from canonicalized parsed definitions and relevant manifests, not timestamps, absolute paths or filesystem order. Document whether text/localization and presentation-only references are included; simulation-affecting data must always be included.

## 9. Crate and module changes

Keep the plan-B crate names and dependency direction:

```
f4x-static
  definitions/       definition structs and external schema
  ids/               typed runtime IDs and external ID resolution
  registry/           immutable registries
  load/               filesystem input, RON parsing, composition, validation
  ruleset.rs          RuleSet and metadata/fingerprint
  stats/              closed Stat enum and modifier calculation
  effects/            conditions, targets, effects and executor
  terrain/
  feature/
  text/
  help/

f4x-core
  state/
    game.rs           owns/references the selected immutable RuleSet
    save/             save metadata includes ruleset identity/fingerprint
    ...
  action/
    newgame/          creates state using the selected RuleSet
    ...
  session/            keeps the game and its ruleset together
```

The loader belongs in `f4x-static` because it constructs the definitions and registries and has no dependency on game state. It must not depend on `f4x-core`, the client, AI or asset decoders.

Keep file access at the edge of `f4x-static::load`. Parsing and validation should be testable from in-memory RON strings/virtual file sets as well as real directories. This keeps tests fast and makes it possible to validate content in tools and CI.

## 10. Tools and user-facing workflow

Extend `f4x-tools` with:

- `validate`: validate a ruleset plus an ordered mod list; show all errors and definition provenance.
- `list-rulesets`: discover available ruleset directories and print IDs/names.
- `list-mods`: discover mods and show dependencies/compatibility.
- Optional `dump-ruleset`: print the resolved composition and fingerprint for bug reports.

The game should let the player select a ruleset and enable/disable mods before starting a new game. The first UI can be simple; a command-line/config selection is sufficient for the first end-to-end milestone.

Provide a sample local mod in the repository or test fixtures that adds one unit, one building and one spell using existing primitives. It must be possible to copy that sample folder into the user's mods directory and load it without rebuilding the executable.

## 11. Implementation sequence

The sequence is designed to avoid building a mod system on top of unstable definition types.

### Stage 0 — Preserve the Plan B refactor

Complete plan B's initial art separation and definition/state/action sorting. Keep the compiler-enforced crate boundary: definitions must not depend on state or actions. Do not start runtime mod loading while definition types are still being moved between modules.

### Stage 1 — Stabilize external definition schemas

1. Inventory every definition type and every place where content is still represented by a Rust constant, enum match or numeric ID.
2. Define stable external string IDs and the mapping to typed runtime IDs.
3. Move all supported definition structs and data-only validation into `f4x-static`.
4. Remove special-case logic based on registry position/numeric spell IDs; replace it with semantic hook keys or explicit fields.
5. Finalize RON schemas for units, buildings, spells, races, abilities, realms, heroes, items, enchantments, terrain and features.
6. Add schema/version compatibility tests and fixtures.

**Gate:** every definition that is promised as moddable can be represented entirely in the external schema, and no definition's meaning depends on its registry index.

### Stage 2 — Build the composed RuleSet loader

1. Introduce `RuleSet`, ruleset/mod manifests, metadata and content fingerprint.
2. Implement deterministic directory discovery and RON parsing.
3. Load a base ruleset from a directory.
4. Add ordered mod overlays, explicit full-definition overrides and duplicate detection.
5. Resolve external IDs to typed IDs only after composition is complete.
6. Validate all references and semantic constraints on the final set.
7. Return either a complete immutable `RuleSet` or structured diagnostics; never a partial registry.

**Gate:** integration tests load a ruleset and two ordered mods from temporary directories, and produce the same registries/fingerprint regardless of file creation order.

### Stage 3 — Wire the ruleset into the game

1. Pass `RuleSet` into new-game construction and retain it with `Game`/session.
2. Replace access to global/shipped registries with access through the selected ruleset.
3. Ensure state construction, actions, battle, spellcasting, economy, AI and front ends all use the same instance.
4. Ensure no simulation path reads content files or reloads definitions.
5. Keep the default shipped ruleset as the default selection so existing users have a straightforward path.

**Gate:** two games can be created in one process with different rulesets and run independently without registry cross-talk.

### Stage 4 — Make saves ruleset-aware

1. Add ruleset/mod identity and content fingerprint to save metadata.
2. Check the resolved fingerprint before restoring state.
3. Add tests for exact match, missing mod, changed content, changed mod order and unsupported rules API version.
4. Keep the existing save format migration/versioning explicit; do not silently load a save against different definitions.

**Gate:** a save cannot be loaded against a different effective ruleset without an explicit future migration path.

### Stage 5 — Tooling and sample mod

1. Extend `f4x-tools validate` to accept a ruleset path and ordered mod paths.
2. Add ruleset/mod discovery commands.
3. Add a sample mod folder with a new unit, building and spell.
4. Document how to install, enable, override and troubleshoot a mod.
5. Add CI fixtures that validate the shipped ruleset and sample mod.

**Gate:** copy the sample mod into a local mods folder, run the validator and start a game with it, without compiling Rust.

### Stage 6 — Game UI and authoring quality

1. Add pre-game ruleset selection.
2. Add mod enable/disable and ordering, with dependency and incompatibility feedback.
3. Show the final active mod list and ruleset fingerprint in game/session diagnostics.
4. Add clearer error presentation for invalid content.
5. Consider hot reload only for editor/tool workflows; never mutate the ruleset snapshot of an active game.

## 12. Testing requirements

In addition to plan B's existing workspace tests, clippy, ID-alias checks, soak and regression targets, add:

- Parse tests for every definition type and schema version.
- Unknown ID, duplicate ID, missing reference and invalid override tests.
- Dependency missing/cycle/incompatibility and deterministic ordering tests.
- Full composition tests where one mod adds content and another overrides it.
- Invalid effect/stat/target/hook tests.
- Path traversal, oversized input and malformed-file tests.
- Fingerprint stability tests across machines, directory order and timestamps.
- Two simultaneous games with different rulesets.
- Save/load tests for matching and mismatching content fingerprints.
- A no-recompile acceptance test: alter/add a RON definition in a local folder and run the already-built executable; the new definition appears in the loaded ruleset.

Every stage keeps `cargo test --workspace`, `cargo clippy --workspace --tests`, `scripts/check_id_aliases.py`, and the existing soak/regression targets green.

## 13. Risks and explicit non-goals

| Risk | Mitigation / decision |
|---|---|
| Numeric IDs or array positions are baked into special-case code | Audit and replace with stable semantic hook keys before external loading. |
| A mod changes a definition referenced by existing saves | Save fingerprint prevents silent reinterpretation. |
| Overlay order changes gameplay | Explicit order, deterministic composition, order included in save metadata/fingerprint. |
| Content schema changes over time | Version manifests and rules API; reject unsupported versions with actionable errors. |
| Mods need mechanics the engine cannot express | Add a tested Rust primitive; do not execute arbitrary scripts. |
| Definition loader grows into a general asset system | Keep art/audio decoding and client asset resolution outside this crate. |
| Invalid content crashes or corrupts a running game | Validate completely before game construction; immutable ruleset snapshot. |

Not included in Plan C:

- Arbitrary user code, scripting languages, native dynamic libraries or WASM mods.
- Runtime hot reload of an active game.
- Automatic conversion of old saves to changed rulesets.
- Fine-grained field-level patch/merge semantics in the first version.
- A mod marketplace, download manager or network distribution system.
- Making every UI layout, image, sound or animation moddable through the rules loader.

## 14. Plan B versus Plan C

Plan C is Plan B plus a runtime content contract:

| Area | Plan B | Plan C |
|---|---|---|
| Definitions | Moved to `f4x-static`; loaded content | Immutable `RuleSet` assembled from local files |
| Loader | Existing shipped-content loader merged into `f4x-static` | Versioned ruleset loader with ordered mod overlays |
| IDs | Typed IDs in registries | Stable external string IDs resolved to typed IDs |
| Overrides | Not specified | Explicit whole-definition overrides |
| Validation | Shipped content validation | Full validation of the composed ruleset |
| Game lifetime | Uses static definitions | Holds one immutable selected ruleset snapshot |
| Saves | Existing state serialization | Store and verify ruleset/mod fingerprint |
| Tools | Existing content validation | Validate/list rulesets and mods; sample mod |
| Adding a unit/building/spell | Change content files in the shipped set | Drop definition files into a local folder; no rebuild |

**Recommended relationship:** treat Plan C as the target plan and execute the structural work from Plan B, incorporating the additional requirements at the stages above. In particular, do not finish Plan B by hard-wiring a single shipped registry and then bolt mods on later: stable external IDs, ruleset ownership, semantic hooks and save fingerprints must be considered while the definition layer is being extracted.
