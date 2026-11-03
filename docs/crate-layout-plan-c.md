# Crate layout: plan C (runtime-loadable rulesets and mods)

This is plan B with one architectural requirement added: **the game must load different rulesets and user-created content from local files at runtime**. A player must be able to add units, buildings, spells and other supported definitions by putting files in a local folder. Doing so must not require rebuilding the game or recompiling Rust code.

Plan C supersedes B only where this document says so. The crate split, the definition/state/action boundary, the session design, and the save ownership decisions from B remain in force. This document is intended to be executable on its own alongside the repository's existing architecture and content conventions.

## 1. Goals and non-goals

### Goals

- Load the shipped ruleset and user-installed rulesets from local directories.
- Support ordered mods which can add new definitions and replace existing definitions.
- Initially support at least units, buildings, spells, abilities, races, realms, items, item powers, level paths, hero templates, lair rules, settings and localized names/text, to the extent those concepts are already data-driven.
- Resolve references across the complete combined ruleset, including references from a mod to shipped definitions and between mods.
- Validate the complete result before a game can start.
- Keep definitions immutable during a game. A running game uses exactly the ruleset with which it was created.
- Keep the simulation, state/action split, and crate dependencies from plan B. Mod content is data, not dynamically compiled Rust.
- Keep shipped content as ordinary files in the repository; do not make the shipped ruleset a special code path.

### Non-goals for the first version

- No scripting language, embedded Lua/WASM, native-code plugins, or arbitrary executable mod code.
- No hot-reloading definitions into an active game.
- No promise that every conceivable new mechanic can be expressed by data. Mods can combine the effect types and hooks the engine already implements; a genuinely new effect, trigger, targeting rule, AI behavior, or simulation mechanic still requires an engine change.
- No mod browser, workshop integration, online distribution, or graphical mod editor. The first interface is a documented folder layout and a validator command.
- No change to the save format merely to accommodate the crate split. Save compatibility with modded games is addressed separately in section 8.

The important distinction is: **new content using existing game capabilities is data-only; new capabilities are code.** A new spell which summons a newly defined unit, for example, should be possible if the existing spell effect vocabulary supports it. A spell requiring a new kind of combat resolution is not.

## 2. Crate layout

Keep the Plan B crate graph. Change the description and responsibility of `f4x-static`: “static” means definitions which do not change as a game runs, not definitions compiled into the executable.

```
local data directories
       |
       v
f4x-static  definition types, RON parsing, ruleset/mod composition, reference resolution,
            validation, immutable Ruleset and registries, text/help, effect definitions
       ^
f4x-core    one game: state/, action/, session/; holds a shared Ruleset handle
       ^
f4x-ai      reads the game and its Ruleset; implements Seat
       ^
f4x-console, f4x-client, f4x-learn, f4x-tools
```

- `f4x-static` remains the bottom-level crate and absorbs `f4x-data`, as in B. It owns all parsing and validation of game definitions. The loader must not depend on `f4x-core`.
- `f4x-core` owns game state and actions, as in B. A game is constructed with a validated, immutable `Ruleset` (preferably shared as `Arc<Ruleset>` or an equivalent cheap-to-clone handle). State and action code read definitions through that handle; they do not read files.
- `f4x-ai` reads the same game/ruleset view as the human client. It must not load a second copy of the rules or silently fall back to shipped definitions.
- Front ends choose and load a ruleset before creating a new game. The console and client should use the same loader API.
- Saves remain in `f4x-core`, because they serialize a `Game`. The save records which ruleset it needs; it does not make the loader a dependency of core.

Suggested public shape (names are illustrative, not a demand to implement these exact signatures):

```rust
pub struct Ruleset {
    pub metadata: RulesetMetadata,
    pub rules: Rules,
    pub content_hash: ContentHash,
}

pub struct RulesetLoadRequest {
    pub base: RulesetSource,
    pub mods: Vec<ModSource>, // already in explicit load order
    pub language: String,
}

pub fn load_ruleset(request: &RulesetLoadRequest) -> Result<Ruleset, RulesetLoadError>;
```

The loaded `Ruleset` is a complete snapshot: definitions, registries, resolved references, validation results, metadata and a content fingerprint. It contains no mutable game state and no open file handles.

## 3. Local file layout and format

Use RON for the first version. The project already uses RON for definitions, so this avoids introducing a second serialization format and keeps files reasonably readable and commentable.

A suggested user data layout:

```
data/
  rulesets/
    classic/
      ruleset.ron
      rules/
        units.ron
        buildings.ron
        spells.ron
        abilities.ron
        races.ron
        realms.ron
        items.ron
        ...other existing definition tables...
      text/
        en/
          name/
            unit.ron
            spell.ron
            ...
  mods/
    example_content/
      mod.ron
      rules/
        units.ron
        buildings.ron
        spells.ron
      text/
        en/
          name/
            unit.ron
  config.ron
```

The repository's current `content/` directory is the shipped classic ruleset during the transition. It can either remain the source directory for that ruleset or be moved into the same layout in a separate, mechanical change. Do not combine that file move with the first loader redesign.

- `config.ron` selects the active base ruleset and an explicit ordered list of enabled mods. A front end may later expose this in its UI.
- `ruleset.ron` describes the base ruleset: stable ID, display name, version, content schema version and supported engine-version range.
- `mod.ron` describes a mod: stable ID, display name, version, schema version, compatible ruleset/engine range, and optional declared dependencies/conflicts.
- Mods are folders, not compiled crates. A mod may contain only the tables it changes.
- All paths are relative to the selected data root. The loader must reject path traversal and must not follow arbitrary paths supplied by a definition file.
- Art, sound and other presentation-only assets stay client-owned as in B. A definition may refer to an asset by a logical content path/key; it must not make `f4x-static` load textures, audio or window resources.

Exact filenames can follow the current `f4x-data` tables. The first implementation should preserve existing names where practical rather than renaming every content file.

## 4. Composition rules: add, replace, resolve

The current loader already accepts multiple directories and uses the first directory containing a given file. That is useful for whole-file overrides, but it does **not** by itself let a mod append a new unit to `units.ron`: if a mod supplies that file, it currently shadows the lower-priority file. Plan C must replace this file-level fallback model with definition-level composition.

Use these rules:

1. Load the selected base ruleset.
2. Load enabled mods in the order specified by the user's configuration. Dependencies must be satisfied; either compute a deterministic topological order or reject a configuration whose explicit order violates dependencies.
3. For each definition table, parse each present file as a list of definitions.
4. Merge entries by stable definition ID, not by row position:
   - an ID not yet present adds a new definition;
   - an ID already present is an override and replaces that definition as a whole;
   - duplicate IDs within one source are an error;
   - an override must be explicit in the mod metadata or definition syntax, rather than silently winning because of directory traversal order;
   - a mod cannot override another mod's definition unless its dependency/order relationship makes that intentional.
5. After all sources have been merged, resolve every cross-reference against the final combined tables. This allows a new spell to refer to a unit added by the same mod, or to a unit supplied by another enabled mod.
6. Run validation against the final result. Do not validate only each mod in isolation.
7. Construct registries and publish the immutable `Ruleset` only if parsing, composition, reference resolution and validation all succeed.

For the first release, prefer whole-definition replacement over a general-purpose field patch language. It is simpler to reason about and test. If field-level patches are later needed, add them as an explicit, versioned feature; do not make implicit partial RON deserialization the merge contract.

### Stable IDs and numeric IDs

The existing definitions have string IDs and numeric `num` fields, and the current registries expose compact typed IDs. This is a key migration point for mod support:

- The stable string ID is the author-facing identity. New mod content should use namespaced IDs such as `example_content:clockwork_golem`, `example_content:iron_foundry` and `example_content:starfall`. Shipped IDs can remain unchanged for compatibility.
- Never identify mod content by its position in a file, vector index, or load order.
- Audit every use of `num`, typed numeric IDs, `id_numbered`, and serialized references before deciding how new definitions receive runtime handles. Do not let two mods accidentally claim the same numeric slot.
- Preferred direction: keep a stable string `ContentId` at the data/save boundary and resolve it to compact runtime handles inside a loaded `Ruleset`. If that is too broad for the first implementation, reserve and validate a deterministic numeric range/allocation scheme for mod-added definitions, while retaining the shipped numeric values. Document the limitation and keep the allocation independent of mod load order.
- References between definitions in RON should use stable string IDs. Runtime typed handles are resolved only after the final merge.

Do not ship mod support while a new definition can silently alias an existing definition because of a numeric collision.

## 5. Definition and behavior boundary

Plan B's definition/state/action split becomes more important with mods:

- Definition types and data-only formulas remain in `f4x-static`.
- Game state and instances remain in `f4x-core/state`.
- Mutations remain in `f4x-core/action`.
- A unit definition describes a unit type. A unit instance stores its owner, figures/health, experience, enchantments and other changing state.
- A building definition describes construction requirements, cost and effects. Whether a city owns or is building it is game state.
- A spell definition describes realm, cost, targeting/duration and a list of supported effects. Casting and ongoing enchantment state are core state/actions.
- A building, unit or spell added through RON must flow through the same generic systems as shipped content. Avoid adding `match id { ... }` branches for individual content IDs.
- The effect/modifier engine remains code in `f4x-static` where it is definition-only, with the game-facing host boundary as in B. Mod files may use existing effect variants and declared custom hooks only.
- `CUSTOM_HOOKS` and `Effect::Special` are not arbitrary extension points. A mod cannot invent a new Rust hook name and expect behavior to appear. Unknown hooks and unsupported special behavior must fail validation.
- Audit all enums used in definitions. An enum variant is a compiled capability, not a mod-extensible list. Where a concept is genuinely content-defined (for example a category or tag), prefer a string/typed content ID over adding a Rust enum variant for each new content item. Keep closed enums for engine concepts whose behavior is implemented in code.

A useful acceptance test is that a mod can add a unit, make it buildable by an existing race, add a building that unlocks it, and add a spell that summons it, using only supported existing fields/effects and files.

## 6. Loading API and lifecycle

The loader should expose one canonical path for production, tests, tools and front ends. Avoid a second test-only loader that assembles definitions differently.

Recommended flow:

1. Read user configuration (or accept an explicit ruleset selection from the caller).
2. Discover and read the selected base ruleset and enabled mod manifests.
3. Check schema and engine compatibility, dependencies, conflicts and duplicate mod IDs.
4. Parse the base and mod definition files.
5. Compose tables, resolve references, build registries and validate.
6. Compute a deterministic content hash from canonicalized metadata and all effective gameplay definitions (not filesystem timestamps or absolute paths).
7. Return `Ruleset` or a structured error containing source path, table, definition ID and the problem.
8. Pass the resulting `Ruleset` handle into new-game creation and the `Game`. Do not allow new-game code to call the filesystem.

Loading occurs before a new game starts. The ruleset is immutable for the lifetime of a game. Switching mods means starting a new game. A later developer tool may support reload-and-validate, but it must create a new `Ruleset` snapshot and must never mutate the definitions used by a live `Game`.

The loader should support:
- loading shipped content with no mods;
- loading a named local ruleset;
- loading a base ruleset plus an ordered list of mods;
- validating a ruleset without starting a game;
- listing installed rulesets and mods for a future selection UI.

The exact UI is out of scope. Both console and client should call the same `f4x_static` API.

## 7. Validation and diagnostics

Build on the existing `validate_rules` checks; do not create a weaker “mod validation” path.

Validation must cover at least:

- parse errors with file and line/column where RON provides them;
- unsupported schema versions and incompatible engine/ruleset versions;
- missing manifests, duplicate IDs, missing dependencies and dependency cycles;
- attempted overrides without permission, conflicting overrides and duplicate IDs within one source;
- numeric ID/handle collisions until the stable-ID migration is complete;
- unknown references after composition (unit, spell, building, realm, ability, item, race, etc.);
- invalid cross-table combinations, cycles and impossible requirements;
- unsupported effect variants, custom hooks or hard-coded special cases;
- references to unavailable logical assets, where that can be checked without loading client resources.

Errors should identify the source mod and file as well as the affected definition. Prefer collecting independent validation problems into one report rather than stopping at the first bad reference. A bad mod must not crash the game or leave a partially loaded ruleset active.

Add a command to `f4x-tools`, for example:

```sh
cargo run -p f4x-tools -- validate-ruleset --base classic --mod example_content
```

The command should print the effective load order, the ruleset hash, warnings and all validation errors, and return a non-zero exit code if invalid. It is the primary feedback loop for mod authors.

## 8. Saves and deterministic games

A save made with a modded ruleset must not silently load under a different set of definitions.

- Keep save serialization and file IO in `f4x-core`, as B specifies.
- Store the base ruleset ID/version, ordered mod IDs/versions, content schema version and effective gameplay content hash in save metadata.
- On load, the caller first loads the matching ruleset and passes it to the save loader. The save loader compares the required identity/hash before restoring the game.
- If the required mod or exact content is missing, changed, or incompatible, return a clear error. Do not silently substitute the shipped ruleset or the latest version of a mod.
- Decide explicitly whether saves store stable string IDs or compact runtime handles. Runtime handles must never be interpreted under a different ruleset. This decision is part of the numeric-ID audit in section 4.
- The initial policy should be strict matching. A future migration tool may migrate saves between ruleset versions, but migration is not part of the first mod-support implementation.
- Deterministic replay/soak tests must use the same ruleset hash as the original run. Include the hash in diagnostic output so a replay mismatch can be traced to content as well as code or RNG.

Do not serialize the entire `Ruleset` into every save in the first version. Keep the content files external and identify them precisely. A future self-contained save/export mode can be considered separately.

## 9. Implementation stages

Plan C preserves the Plan B order where possible, but introduces a small loader/composition design before the final crate move so the architecture does not freeze the current file-level overlay behavior into the new API.

### Stage 0 — inventory and boundaries

1. Keep B's art/sound/animation audit and move client-only assets out of game definitions.
2. Inventory every definition table, definition reference, `num` field, typed numeric ID, serialized reference and hard-coded content-ID branch.
3. Mark each content field as one of: gameplay definition, localized text, client asset reference, runtime state, or engine capability.
4. Identify enums and special hooks that currently prevent new content from being expressed as data.
5. Record the current shipped ruleset's load behavior and validation results as a baseline.

**Gate:** there is a written table of all definition types and all numeric-ID consumers. No new mod file format is designed around assumptions that have not been checked.

### Stage 1 — definition/state/action split (Plan B stage 1)

1. Sort each domain into definition, state and action as B describes.
2. Move mutators to `action/<domain>`, components/views/spawn to `state/<domain>`, and definition-only code to definition modules.
3. Remove state/action back-edges from definitions.
4. Ensure state never calls an action.
5. Keep the shipped rules loading and the full test suite green after each domain.

### Stage 2 — make the loader compose definitions

Do this while the existing `f4x-data` loader is still easy to compare with the current behavior.

1. Introduce a source/manifest model for one base ruleset and ordered mods.
2. Replace “first directory containing this file wins” with definition-level merge by stable ID. Preserve the current directory loader temporarily as a compatibility adapter if needed.
3. Implement explicit add/override rules and duplicate/override diagnostics.
4. Resolve all references only after the complete merged tables have been assembled.
5. Add namespaced IDs for mod-created definitions and complete the numeric-ID/handle decision before exposing the API as stable.
6. Return an immutable `Ruleset` with metadata and deterministic content hash.
7. Make the shipped ruleset load through this exact path, with no special test-only construction path.

**Gate:** shipped content loads identically to the baseline; a test mod adds one unit, one building and one spell without changing Rust code; a second test mod can refer to the first mod's unit; invalid references and collisions are rejected.

### Stage 3 — create `f4x-static` and merge `f4x-data` (Plan B stage 2)

1. Move the definition types, registries, effect/modifier engine, loader, manifests, composition and validation into `f4x-static`.
2. Keep the crate independent of `f4x-core`; use only shared base primitives and definition types.
3. Add the Plan B compatibility re-exports in `f4x-core`.
4. Move the tests to `f4x-static/tests/it`; delete duplicate test loaders such as the 308-line `game/testutil.rs::load_shipped`.
5. Handle the Rust orphan-rule issues in definition impls as B describes.
6. Update scripts that map changed source files to test targets. A change to `f4x-static` must run all ruleset/content tests and the relevant game integration tests.
7. Measure compile time as in B. Editing a RON mod file should not trigger a Rust rebuild at all; editing the loader or definition schema will.

### Stage 4 — pass Ruleset into the game

1. Change new-game construction to require a validated `Ruleset` handle.
2. Store/share that handle with `Game` (and expose read-only access to AI/front ends through the existing game API).
3. Remove all runtime reads of rules/content files from `f4x-core`, `f4x-ai` and the client. Only `f4x-static` loads definitions.
4. Ensure every action/query which needs a definition uses the current game's ruleset, not a global shipped singleton.
5. Keep the state/action boundary intact: actions may change game state, but neither state nor action may mutate the `Ruleset`.
6. Update new-game setup, tests, console, client, tools and AI construction to pass the same ruleset handle.

**Gate:** two games can be constructed in one process with different rulesets and run independently without definition leakage between them.

### Stage 5 — saves and sessions

1. Add ruleset identity, mod list and content hash to save metadata.
2. Require the matching `Ruleset` when loading a save; reject missing or mismatched content clearly.
3. Keep save IO and game restoration in `f4x-core`.
4. Complete the Plan B session/Seat work and ensure AI seats use the game's ruleset.
5. Verify undo/replay and deterministic tests with both shipped and modded rulesets.

### Stage 6 — tools, docs and author workflow

1. Add `validate-ruleset` to `f4x-tools`.
2. Document the folder layout, manifest fields, ID naming, load order, override rules, supported effect vocabulary, validation command and save compatibility.
3. Include a minimal example mod which adds a unit, a building and a spell using existing mechanics.
4. Update `README.md`, `CLAUDE.md`, `docs/design.md`, `docs/rules-framework.md`, `docs/text.md`, `docs/scripts.md` and the crate layout docs.
5. Add tests proving that changing only a local RON file changes the loaded ruleset hash and behavior without compiling Rust.

## 10. Required tests and acceptance criteria

The first mod-support milestone is complete only when all of these pass:

1. **No-mod parity:** loading the shipped ruleset through the new path produces the same effective definitions and gameplay behavior as the current loader.
2. **Additive content:** a local mod adds a unit, building and spell using only files. The game can build the unit, the building can unlock it, and the spell can summon it where those mechanics are supported.
3. **Overrides:** a mod can replace a shipped definition by stable ID; the override is explicit and appears in diagnostics.
4. **Multiple mods:** two mods can add definitions and cross-reference one another under a deterministic load order.
5. **Isolation:** two games in one process can use different rulesets without sharing or mutating definitions.
6. **Validation:** duplicate IDs, invalid overrides, missing references, bad effects, schema mismatch and numeric-ID collisions fail before game creation with useful diagnostics.
7. **Save safety:** a save refuses to load if its required mod or effective content hash is unavailable or different.
8. **No recompilation:** editing or adding a local RON file is picked up by the next ruleset load and requires no Rust build.
9. **Regression:** `cargo test --workspace`, `cargo clippy --workspace --tests`, `scripts/check_id_aliases.py`, soak tests and deterministic replay targets remain green.

## 11. Risks and decisions to settle

| Risk / question | Plan C treatment |
|---|---|
| Existing numeric IDs are assumed throughout the code and possibly saves | Audit before promising arbitrary additions; use stable string IDs at the content boundary and a collision-safe runtime handle strategy. |
| Current loader replaces whole files by directory priority | Replace with definition-level merge by stable ID; this is a required feature, not an optional refinement. |
| Existing code has hard-coded special cases for specific content numbers/IDs | Inventory them. Keep shipped compatibility where needed, but new content must use generic systems. Some special content may remain non-extensible until those branches are redesigned. |
| New effects cannot be invented in data | State clearly: data can compose implemented effects; new mechanics require Rust code. |
| Mods may depend on each other or override the same definition | Manifests, explicit order/dependencies, conflict detection and clear diagnostics. |
| A local mod changes after a save was created | Content hash mismatch; fail safely rather than silently changing the meaning of the save. |
| `f4x-static` name suggests compile-time constants | Document that “static” means immutable during a game; consider a crate rename only in a separate future decision. |
| Mod loading could become a security boundary | Treat files as untrusted input; bounded file sizes/counts, safe relative paths, no executable code, no arbitrary asset path traversal. |

## 12. What Plan C changes compared with Plan B

| Area | Plan B | Plan C |
|---|---|---|
| Definitions crate | `f4x-static` holds shipped definitions and loader | Same crate loads any selected local ruleset and composes mods at runtime |
| Loader | Merged `f4x-data` loader; directory fallback | Manifest-driven base + ordered mods; definition-level merge by stable ID |
| Content additions | Primarily shipped tables | Mods can add units, buildings, spells and other supported definitions without recompiling |
| Overrides | Whole-file directory priority | Explicit per-definition overrides |
| Definition identity | Existing string IDs and numeric IDs | Namespaced stable IDs for mod content; numeric handle collision strategy is a prerequisite |
| Game construction | Uses shipped definitions | Receives an immutable `Ruleset` snapshot |
| Saves | Existing game save | Save records exact ruleset/mod identity and content hash |
| Validation | Shipped content validation | Full validation of the composed ruleset before a game starts |
| New mechanics | Rust implementation | Still require Rust; mod files only use implemented capabilities |

**Plan C's central architectural rule:** the executable contains the engine; local files contain the ruleset and content. The engine loads, composes and validates those files once before a game begins. A game then runs against an immutable snapshot of that exact ruleset.
