# Fog and vision improvement plan

## Purpose

Keep fog-of-war and current visibility correct, cheap, and easy to reason about as maps and armies grow. The work covers:

- **Exploration:** what a player has discovered and remembers.
- **Current vision:** which tiles are visible now because of units, cities, or global effects.
- **Client queries:** avoiding a full visibility rebuild every rendered frame.
- **Save data:** preserving the distinction between runtime representation and the on-disk format.

The aim is a small, measured improvement, not a new visibility framework.

## Current implementation

### Exploration: `map/fog.rs`

`Fog` stores a separate explored layer for each player and plane:

```rust
pub struct Fog {
    width: u32,
    height: u32,
    explored: BTreeMap<PlayerId, [Vec<bool>; 2]>,
}
```

The layers are row-major, with `y * width + x` as the tile index. Exploration is persistent; current visibility is not stored here. `corners()` derives the four-corner mask used to draw the edge of explored land. Those four bits are a rendering result, not four separate exploration flags.

The map dimensions are already stored once in `Fog`. Maps are generated with their dimensions at runtime, so any packed runtime representation must be dynamically sized; it must not assume a fixed 60 × 40 board.

### Current vision: `map/vision.rs`

`Economy::vision_grid()` builds a `VisionGrid` by stamping each owned unit's and city's sight onto per-plane `u16` counters. A tile is visible when its counter is non-zero.

The counts are deliberate: when a unit moves, its old contribution can be removed without hiding a tile still seen by another unit or city. The grid also tracks each unit's previous position and sight so the AI's `VisionCache` can patch movement instead of rebuilding the whole grid.

The AI already keeps this cache for the duration of its turn. `VisionEffect::{None, Walk, Recount}` classifies commands, and `with_events()` makes the classification conservative when a command raises unexpected events. `F4X_CHECK_VISION=1` compares the kept result with a fresh calculation after commands.

### Client: repeated work

`f4x-client/src/view.rs::App::draw_map()` currently calls:

```rust
let visible = self.game.economy().visibility(HUMAN, self.plane);
```

That call builds a new grid and converts its counters into a fresh `Vec<bool>`. `draw_map()` runs every frame, although visibility normally changes only when relevant game state changes. The resulting flags are used by `draw_units()` to decide whether enemy stacks can be shown.

The Surveyor's `guard_names()` independently calls `visibility()` when it needs to know whether a lair is currently visible. This is another call site that should reuse the same client result.

This is a concrete instance of the broader client-cache task 476: cache queried game information and refresh it only after relevant state changes.

### Saves: do not mistake this for an unpacked save format

The runtime `Fog` uses `Vec<bool>`, but the save path already packs explored flags into bytes:

- `FogSave` stores two `Vec<u8>` layers per player.
- `pack()` packs eight flags into each byte.
- `restore_fog()` validates and unpacks those bytes when loading.

Therefore, replacing runtime `Vec<bool>` with a bitset is not automatically a save-file fix. The measured 127 KB fog share in task 485 should be investigated against the actual serialized `FogSave` field and map dimensions before changing the save representation. RON's textual representation of byte arrays may still be verbose, but the save is not writing one Boolean per tile.

## Proposed design

### 1. Keep the exploration and vision concepts separate

Do not merge exploration history with current visibility.

- `Fog` remains the authority for remembered exploration and explored-edge rendering.
- `VisionGrid` remains the authority for what can be seen right now.
- A visible tile should normally become explored through the existing exploration/update path; do not infer exploration merely from a client display cache.
- Keep the two planes and each player's state independent.

This avoids subtle bugs where a temporary visibility effect accidentally erases remembered terrain, or where an explored tile is treated as currently visible.

### 2. Core: retain overlap counts and incremental AI updates

**Recommendation: keep the current count-based `VisionGrid` for now. Do not replace it with an OR of per-source bitsets.**

An OR-of-source-masks design is valid for a full rebuild, but it has a trade-off: when a source moves or disappears, the aggregate OR cannot tell whether another source still covers the same tile. The implementation must either rebuild the aggregate from every source or retain the individual source masks. The current counter grid already solves that overlap problem while allowing one source to be lifted and restamped.

At the current board sizes, the `u16` counters are small and straightforward. Keep them unless profiling demonstrates that their memory footprint or stamping cost matters.

Core follow-ups:

1. **Preserve the AI cache and its conservative invalidation rules.** Do not weaken `VisionEffect::Recount` defaults. A new command should continue to trigger a recount unless it is explicitly proven harmless or movement-only.
2. **Keep the diagnostic check.** Retain `F4X_CHECK_VISION=1` and expand its tests when the cache logic changes.
3. **Avoid unnecessary result allocations.** The public `Economy::visibility()` currently creates a `VisionGrid` and then a `Vec<bool>`. Keep this convenient on-demand API for callers that need a one-off answer, but let repeated callers retain a `VisionGrid` or a materialized flags result instead of rebuilding it.
4. **Check bounds and arithmetic.** Use checked dimension multiplication when allocating or restoring grids, and keep coordinate indexing consistent with the map's horizontal wrapping and vertical clipping rules.

A full rebuild remains the simple correctness baseline:

```rust
let fresh = economy.vision_grid(player, None);
assert_eq!(kept.flags(plane), fresh.flags(plane));
```

Do not add per-source bitsets or a generic visibility abstraction unless a measured use case requires them.

### 3. Core: consider a runtime bitset for exploration only if measurement justifies it

The explored layers are currently byte-per-flag at runtime. A dynamically sized bitset would reduce their memory footprint by roughly eight times and make bulk bit operations possible.

A minimal representation could be:

```rust
struct TileBits {
    words: Vec<u64>,
}

impl TileBits {
    fn new(tile_count: usize) -> Self {
        Self {
            words: vec![0; tile_count.div_ceil(64)],
        }
    }

    fn get(&self, index: usize) -> bool {
        self.words[index / 64] & (1_u64 << (index % 64)) != 0
    }

    fn set(&mut self, index: usize) {
        self.words[index / 64] |= 1_u64 << (index % 64);
    }
}
```

`Fog` would continue to own `width` and `height`; each player's two plane layers would own only their words. The tile index remains `y * width + x`. Validate the index at the public boundary, or keep the low-level bit operations private behind coordinate-checked `Fog` methods.

This is an optional memory optimization, not a prerequisite for fixing the client frame cost. Before doing it:

- Measure the runtime memory used by explored layers in a representative large game.
- Check all `Fog::to_parts()` / `Fog::from_parts()` and save/load call sites.
- Prefer keeping the existing `FogSave` byte-packed format. If the internal API changes, convert at the save boundary without changing the serialized shape.
- Test non-multiples of 64 tiles, empty/default fog, both planes, multiple players, horizontal wrapping, and malformed lengths.

If the measured memory saving is insignificant, keep `Vec<bool>`. Do not introduce a dependency for this; a small private type is enough if one is warranted.

### 4. Client: cache human visibility across frames

Make this part of task 476 rather than creating a separate cache mechanism.

The client needs the human player's visibility on both planes, but it does not need to reconstruct it every frame. Add a small cache owned by `App`, for example:

```rust
struct HumanVisibility {
    // Flags in row-major order, one layer per plane.
    flags: [Vec<bool>; 2],
    // Optional generation/revision used to decide whether the cache is stale.
    revision: u64,
}
```

The exact field placement can follow the existing client cache work. The important behavior is:

1. Build the two flag layers lazily when first requested.
2. Reuse them on subsequent frames and when the Surveyor asks about a tile.
3. Invalidate them whenever a game change can alter unit positions, unit sight, city positions/sight, ownership, or a global visibility effect.
4. Do not invalidate for unrelated changes such as a selection change, camera movement, animation frame, or city production choice.
5. Rebuild on load, new game, and after the AI/neutral turn processing has changed the world.

The preferred invalidation mechanism is the one established for task 476: make invalidation explicit at the client boundary where game changes are applied, and audit direct mutating calls as well as ordinary commands. If a reliable shared game-state revision already exists by then, key the cache to it. Do not add a broad revision counter that increments for every mutation if that makes unrelated changes invalidate visibility.

A small client helper should be the only way repeated client code requests the cached flags. For example (illustrative; adapt to the final cache type and borrow constraints):

```rust
fn refresh_human_visibility(&mut self) {
    if self.human_visibility.is_valid() {
        return;
    }

    let economy = self.game.economy();
    self.human_visibility.replace([
        economy.visibility(HUMAN, Plane::Arcanus),
        economy.visibility(HUMAN, Plane::Myrror),
    ]);
}
```

Then `draw_map()` reads the selected plane's cached layer, and `surveyor::guard_names()` reads the same cache rather than calling `Economy::visibility()` again. If borrowing `self` makes a helper awkward, refresh first and then borrow the cached slice in a short scope; do not clone the flags on every frame.

The client cache is derived data only: do not serialize it, and do not make it part of game rules or save compatibility.

### 5. Keep the public core query useful

Do not remove or silently change `Economy::visibility(player, plane) -> Vec<bool>` as part of this work. It is a reasonable one-off convenience method and is used by tests and other callers.

If the client later needs a reusable core cache, expose the smallest useful API (for example a retained `VisionGrid` plus `flags()`) rather than adding multiple overlapping methods. Keep client-specific invalidation policy in the client unless core has a clear, general state-change signal.

## Plan B: full fog and vision redesign

This is the alternative if the goal changes from a targeted performance fix to simplifying and unifying the whole fog/vision model. It is deliberately a larger change. Choose **Plan A** (the preceding incremental improvements) or **Plan B**; do not implement both as overlapping systems.

### Design goals

- Use one small, dynamically sized bitset representation for tile flags throughout core.
- Make the distinction between persistent exploration and transient visibility explicit in the types.
- Represent each visibility source independently, then combine sources with bitwise OR.
- Recompute derived visibility at clear synchronization points rather than maintaining several partially overlapping caches and patching rules.
- Keep client code as a consumer of a stable player-visibility snapshot, not as another visibility calculator.
- Preserve existing sight rules, save compatibility, and deterministic game behavior.

### Core data model

Introduce a private `TileBits` type in `f4x-core/src/map/`. It owns its bit storage; its caller supplies the map dimensions and plane. Do not make a generic bitset crate or add a dependency for this.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
struct TileBits {
    words: Vec<u64>,
}

impl TileBits {
    fn empty(tile_count: usize) -> Self {
        Self {
            words: vec![0; tile_count.div_ceil(64)],
        }
    }

    fn set(&mut self, index: usize) {
        self.words[index / 64] |= 1_u64 << (index % 64);
    }

    fn get(&self, index: usize) -> bool {
        self.words[index / 64] & (1_u64 << (index % 64)) != 0
    }

    fn clear(&mut self) {
        self.words.fill(0);
    }

    fn union_with(&mut self, other: &Self) {
        assert_eq!(self.words.len(), other.words.len());
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= *b;
        }
    }
}
```

The production implementation should keep indexing private and validate map dimensions and coordinates at the `Fog`/`Vision` boundary. It should also mask unused bits in the final word when complementing or counting bits. `width * height` must use checked arithmetic before allocation.

Keep the two planes as separate layers. The bit index is still row-major within a plane:

```rust
let index = y as usize * width as usize + x as usize;
let visible = plane_bits.get(index);
```

Do not bake a fixed map size into the type. Store dimensions once in the owning map/fog/vision object and require matching dimensions before combining bitsets.

Persistent exploration becomes:

```rust
struct PlayerExploration {
    explored: [TileBits; 2],
}

struct Fog {
    width: u32,
    height: u32,
    players: BTreeMap<PlayerId, PlayerExploration>,
}
```

Current visibility becomes a separate player-scoped object. The large-redesign option stores one mask per source and a combined mask:

```rust
struct VisionSource {
    // Unit, city, or another stable source identity.
    source: Entity,
    plane: Plane,
    tiles: TileBits,
}

struct PlayerVision {
    sources: HashMap<Entity, VisionSource>,
    visible: [TileBits; 2],
}
```

The exact source identity and storage can follow the existing ECS conventions. Global effects such as Nature Awareness need an explicit representation (for example an all-visible flag) rather than manufacturing a mask for every tile.

### Source updates and aggregation

When a source is added, moved, removed, or has its sight changed:

1. Recalculate that source's mask from its current position and sight using the existing sight-distance, diagonal-cost, wrapping, and clipping rules.
2. Replace or remove its entry in the source map.
3. Rebuild the aggregate visible masks by clearing them and OR-ing all remaining source masks.
4. Merge current visibility into persistent exploration.

Illustrative aggregation:

```rust
fn rebuild_visible(&mut self) {
    for plane in &mut self.visible {
        plane.clear();
    }

    for source in self.sources.values() {
        self.visible[source.plane.index()].union_with(&source.tiles);
    }

    for plane in 0..2 {
        self.explored[plane].union_with(&self.visible[plane]);
    }
}
```

This makes overlap behavior obvious: a tile remains visible if any source's mask contains it. Removing one source cannot incorrectly hide a tile still covered by another source.

The trade-off is explicit: the source masks consume more memory, and rebuilding the aggregate costs roughly `source_count × words_per_plane` OR operations. For the current small maps this may be entirely acceptable, but it must be measured with large armies and late-game source counts. If source-mask memory is too high, use the alternative full-rebuild variant below instead of retaining every mask.

**Lower-memory variant:** discard per-source masks and, whenever visibility is dirty, clear the aggregate and stamp every source directly into it. This stores only the aggregate bitsets and source descriptors/positions. It is conceptually simpler and uses less memory, but recalculates all source coverage after any relevant change. Benchmark both variants before choosing.

### Replace the current AI patching model

Under this plan, remove the special movement patch path (`VisionEffect::Walk`) and the per-tile `u16` overlap counters after the replacement is proven correct. Keep a conservative dirty/rebuild signal:

- `None`: the command cannot affect visibility.
- `Rebuild`: visibility-affecting state may have changed.

Initially, default commands and uncertain events to `Rebuild`. Do not try to recreate the current fine-grained invalidation matrix in the first pass. The source-mask model already makes rebuilding the aggregate relatively cheap and predictable.

The AI can still keep a player-vision snapshot for the duration of its turn. Its lifecycle should be explicit: build at turn start, mark dirty after relevant commands/events, rebuild before the next visibility query, and drop at turn end. Keep a debug mode that compares the cached result with an independently rebuilt result. Only reintroduce movement-only incremental optimization if profiling later proves it necessary.

### Core API

Expose a narrow read-only view for consumers, rather than making each caller reconstruct visibility:

```rust
pub struct PlayerVisibility<'a> {
    // Internally references the two aggregate plane masks.
    // Public methods answer tile visibility and expose iteration as needed.
}

impl PlayerVisibility<'_> {
    pub fn sees(&self, at: Position) -> bool;
}
```

Keep `Economy::visibility(player, plane) -> Vec<bool>` temporarily as a compatibility/convenience wrapper implemented from the new representation. Migrate internal callers to the read-only view, then decide whether the allocating wrapper remains useful.

Do not expose mutable bitset words to the client. Core owns the rules and visibility state; the client gets queries or a borrowed read-only snapshot.

### Client architecture

Replace the client-specific repeated `Economy::visibility()` calls with one cached human-player visibility snapshot covering both planes.

- `draw_map()`, `draw_units()`, and the Surveyor all read the same snapshot.
- The snapshot is refreshed only when core reports a relevant change or the client applies a mutation known to affect visibility.
- New game, load, and turn transitions invalidate it.
- Camera movement, selection, animation, and unrelated UI state do not invalidate it.
- Do not copy the bitset into a new `Vec<bool>` per frame. Provide a cheap `sees(Position)` query or iterate the visible tiles directly.

If the client and core share a reliable game-state revision, use a visibility-specific revision rather than a global revision that changes for every game mutation. The client cache is derived data and is never saved.

### Save and migration strategy

Keep the external save representation independent from the new runtime representation:

- Continue writing explored flags as the existing per-player, per-plane packed `Vec<u8>` values in `FogSave`.
- Convert between `TileBits` and the existing byte representation in one place.
- Do not save current visibility or source masks; reconstruct them from the loaded world when needed.
- Do not bump `SAVE_VERSION` unless the serialized `FogSave` shape or meaning actually changes.
- Retain the existing load validation for dimensions and packed-layer lengths.

This redesign should not be bundled with RON compression, map-save packing changes, or other task 485 work. Those are separate measurements and changes.

### Migration sequence

1. Add `TileBits` with exhaustive unit tests for empty layers, boundaries, lengths not divisible by 64, union, and dimension mismatch.
2. Convert `Fog` exploration layers to `TileBits`, keeping `FogSave` byte packing and all public fog behavior unchanged.
3. Implement the new source-mask `PlayerVision` behind the existing visibility query; initially run old and new calculations side by side in tests/debug builds.
4. Add differential tests for units, cities, overlapping sources, movement, removal, modified sight, both planes, horizontal wrapping, diagonal extra cost, and global effects.
5. Switch AI turn caching to the new dirty/rebuild lifecycle. Keep the old implementation until soak tests and differential checks pass.
6. Switch client map drawing and Surveyor queries to the shared read-only player-visibility view; remove the per-frame allocation.
7. Remove old counters and movement patching only after all callers and tests use the new implementation.
8. Profile memory and CPU on representative early-, mid-, and late-game saves. Compare source-mask and lower-memory full-rebuild variants; choose the simpler one that meets the performance target.
9. Run save/load round-trips, deterministic replay/soak comparisons, and `F4X_CHECK_VISION`-equivalent differential checks before deleting the old path.

### Plan B acceptance criteria

- The new and old visibility implementations agree for all differential test scenarios before the old implementation is removed.
- Overlapping sources remain visible until the final covering source is removed.
- A move or sight change updates the source mask and aggregate correctly on both the old and new positions.
- Global visibility effects compose correctly with ordinary sources and can be removed cleanly.
- Both planes and arbitrary runtime map dimensions work, including dimensions whose tile count is not a multiple of 64.
- Exploration survives save/load unchanged; current visibility is reconstructed and is not serialized.
- AI visibility queries do not trigger a rebuild for every repeated read within an unchanged state.
- Client map drawing and Surveyor queries share one result and allocate no visibility grid per rendered frame.
- Soak/replay output remains deterministic.
- Benchmarks document CPU and memory costs against the current implementation.

### Plan B risks and reasons not to choose it

- Per-source masks increase memory use; large maps and many sources make this the main cost to watch.
- Rebuilding the aggregate after each source change may be more work than the current counter-based movement patch.
- Replacing a mature AI cache risks subtle stale-visibility bugs if any command/event is misclassified.
- A unified bitset representation is cleaner, but it is not by itself a guarantee of faster gameplay.

Choose Plan B only if simplifying the overall model and removing incremental bookkeeping is worth a larger migration and its testing burden. If the immediate goal is simply to eliminate repeated client work, Plan A is substantially lower risk.

## Implementation order

1. **Baseline and correctness tests**
   - Record a representative map size, player count, unit/city count, and the cost of one `visibility()` call.
   - Add or retain tests for overlapping sources: moving or removing one source must not hide a tile still seen by another.
   - Cover city sight, modified unit sight, both planes, horizontal wrap, and Nature Awareness.
2. **Client cache (task 476)**
   - Add the cached human flags.
   - Audit every client path that mutates the game, including command application, turn advancement, loading, and new-game setup.
   - Invalidate at those points and reuse the result in `draw_map()` and the Surveyor.
   - Test that a visibility-affecting mutation refreshes the cache and a harmless UI change does not.
3. **Core AI cache audit**
   - Keep `VisionEffect` conservative.
   - Add a test for each newly classified command/event path.
   - Run a representative soak with `F4X_CHECK_VISION=1` to verify the cache against a fresh grid.
4. **Measure before changing representation**
   - Profile the client before and after the cache.
   - Measure runtime explored-layer memory.
   - Only then decide whether to replace `Fog`'s `Vec<bool>` with a private dynamic bitset.
5. **Save-size follow-up (task 485)**
   - Inspect the actual `FogSave` RON field and map dimensions from representative saves.
   - Compare compact/binary or compressed save output separately from runtime fog changes.
   - Preserve old-save loading, the readable save header, checksums, and `soak-diff` behavior.

## Tests and acceptance criteria

- A tile covered by two sources remains visible when either one moves away.
- Removing the final source makes the tile invisible.
- Moving a unit updates the old and new areas without changing unrelated tiles.
- A city and a unit overlapping in sight behave the same as two units.
- Nature Awareness makes every tile visible and returns to normal visibility when removed.
- The AI's kept grid matches a fresh `vision_grid()` after every command when `F4X_CHECK_VISION=1` is enabled.
- The client computes human visibility once per invalidation, not once per rendered frame.
- The map and Surveyor show the same current-visibility result.
- Loading and starting a game cannot reuse flags from the previous game.
- If runtime `Fog` is converted to a bitset, save/load round-trips preserve every explored tile and reject malformed lengths.
- No save-format change is made merely to implement the client cache.

## Explicit non-goals

- No per-source visibility bitsets in addition to the current counters.
- No visibility worker thread, async job system, or generic cache framework.
- No fixed map dimensions or fixed-size bitset.
- No changes to sight-distance rules, diagonal cost, horizontal wrapping, or fog-edge rendering.
- No save compression or save-format redesign bundled into the runtime/client work.
- No claim that fog is the main late-game performance bottleneck without profiling; pathfinding, economy, and unit counts have separate performance tasks.

## Relevant code and tasks

- `crates/f4x-core/src/map/fog.rs` — explored state and fog-edge rendering.
- `crates/f4x-core/src/map/vision.rs` — current visibility, overlap counts, AI cache and invalidation effects.
- `crates/f4x-core/src/game/save.rs` — `FogSave`, byte packing and restoration.
- `crates/f4x-client/src/view.rs` — map drawing and unit visibility.
- `crates/f4x-client/src/surveyor.rs` — current visibility query for lair guards.
- Task **476** — client query caching and explicit invalidation.
- Task **485** — smaller save files; investigate the already byte-packed fog field as part of the measured save-size work.
- Task **507** / **512** — broader AI and late-game profiling; use these to avoid attributing unrelated slowdown to fog.
