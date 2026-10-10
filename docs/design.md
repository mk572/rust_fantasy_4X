# High-level design: a Master of Magic-style 4X in Rust with ECS

Status: living document. Scope: gameplay-faithful reimagining, not a port of the original code, data or art.
Related documents: [`rules-framework.md`](rules-framework.md) (the rules layer), [`ecs-structure.md`](ecs-structure.md) (how state is stored and how it differs from the original), [`mechanics-gaps.md`](mechanics-gaps.md) (what the original does that we have not built), [`help-vs-implementation-conflicts.md`](help-vs-implementation-conflicts.md) (resolved and open source conflicts), [`local-assets.md`](local-assets.md) (local-only original art and data).

## 1. Goals and constraints
- Replicate MoM's *gameplay*: two planes (Arcanus/Myrror), wizard-led spell-casting, cities and unit production, hero units, tactical battles, and five magic realms.
- **Parity first.** The first target is every system working as in the original; departures are planned only once that is reached. Where we are unsure, we follow the original.
- Don't copy its code, data or art into the repository. The repository is CC0 and says so. Original-derived material exists only in gitignored local folders (see section 9 and `local-assets.md`).
- Targets are Mac and Linux, with low-res pixel graphics scaled by integer factors.
- The simulation is deterministic: the same seed plus the same command log gives the same game. That gives replays, regression tests, and an optional path to multiplayer.

## 2. Workspace layout
```
crates/f4x-core/    rules + simulation, no I/O or rendering
crates/f4x-data/    content loaders and cross-reference validation (RON)
crates/f4x-ai/      AI players, depends on f4x-core only (empty so far)
crates/f4x-client/  rendering, input, UI (macroquad)
crates/f4x-tools/   validator, map preview, original-art extractor
content/            shipped, original sample data (terrain, features, a small rules set)
scripts/            import_mom.py (original data -> content-local), list_ogbugs.py
docs/               design and reference documents
```
Gitignored, local only: `assets-local/` (extracted art, screenshots), `content-local/` (data imported from the author's own copy of the original game, including the help text).

`f4x-core` must never depend on the client. That keeps it testable and fast to iterate.

## 3. ECS choice
Decided: `hecs` inside `f4x-core`, not a full engine. Only state with identity and varied attributes is an entity; everything else is plain data owned by `Game`. The client renders with macroquad and only reads the core. The details, and how this differs from the original game's fixed arrays, are in [`ecs-structure.md`](ecs-structure.md).

## 4. Core entities and components

| Entity | Components today | Planned |
|---|---|---|
| **Player** (wizard) | Not an entity: a `Player` record in `Game::players` (gold, mana, tax rate, capital race, fame) | wizard picks, known spells, research, relations |
| **Unit** | `Unit` (definition id), `Owner`, `Position`, `Hurt`, `Experience`, `MovesLeft`, optional `Order` | `Enchantments`, `Equipment`, hero data |
| **City** | `City` (name, owner), `Position`, `Population`, `Farmers`, `Buildings`, `CityRace`, `Enchantments`, `Production` | garrison orders, walls state |
| **Map tile** | Dense grid per plane (terrain, feature, road); fog per player | node ownership, corruption |
| **Enchantment** | City list of `(spell, caster)` | entities for unit and global enchantments |
| **Spell / unit / building definitions** | Not entities: typed-id registries loaded from data | |
| **Battle** | not started | temporary world or sub-state |

Derived values are never stored: unit stats, city yields, rebels and growth are computed on demand from the definition plus modifiers.

### Map layers, features and cities
- **Base terrain** is one grid per plane (ocean, grassland, forest, hills, mountains, swamp, desert, tundra). Arcanus and Myrror share the same terrain classes; in the original the planes differ in art and in what spawns, not in terrain rules, so a terrain definition lists the planes it exists on and the client picks per-plane art. Shores and other transition tiles of the original are a rendering concern, not state.
- **Features** are a second, sparse layer on top of terrain, at most one per tile: minerals and specials, encounter sites (lairs, caves, temples, ruins, keeps, dungeons), towers and magic nodes. Nodes are overlays here (a node's underlying terrain is kept).
- **Roads** are a per-tile flag on the grid.
- **Myrror-specific content** lives in data, not code: adamantium and crysx exist only there, its gold/mithril/quork weights are higher, minerals are denser by default.
- **Towers** are placed at identical coordinates on every plane; water under a tower becomes grass. Nodes keep a distance from towers and from each other.
- **Cities are ECS entities**, not map tiles. Founding is a command checked against terrain (`can_found_city`), blocking features (towers, nodes and encounter sites block; minerals do not) and a minimum spacing of **four squares** to other cities on the same plane (settled from the original's code and help text).
- A city works a 5x5 area without its corners (21 tiles). A tile worked by two cities gives each half its food, production bonus and mine yield.
- Not yet modelled: node aura zones, rivers, lakes, volcanoes, corruption.

## 5. Data-driven content
- Races, units, buildings, spells, abilities, items, terrain and features are defined in RON files and validated at load time (`f4x-tools validate <dir>`): every reference must resolve, ids are `[a-z0-9_]`, spells must have effects, units need their required stats, and so on.
- Definitions refer to each other by string id in files; loading turns them into typed ids into registries, so the simulation never meets a dangling reference.
- Abilities and spell effects are composed from a small set of effect primitives and conditions (modify stat, grant ability, damage, heal, summon, change terrain, enchant, dispel, custom hook). Stats are a closed enum; content cannot invent stats.
- A rule the engine cannot express yet uses the `unimplemented` custom hook. Imported content uses it for 134 of 214 spells; the count of uses is the to-do list for new primitives.
- Two content sets: `content/` (original sample data, committed, used by tests and CI) and `content-local/` (imported from the author's own copy of the game; the client prefers it when present).
- New content needs no recompilation, which is mod-friendly.

## 6. Game flow and systems
**Command layer.** All player and AI actions are `Command` values (`MoveStack`, `GoTo`, `ShiftPlane`, `FoundCity`, `SettleOutpost`, `SetProduction`, `BuyProduction`, `SetFarmers`, `SellBuilding`, `SetTaxRate`, `CreateUnit`, `EndTurn`, ...). The core validates and applies them, and emits `Event`s for the client. Rejections are events, not panics. The command log is the save, replay and network format.

**Turn pipeline** (implemented in `Game::end_turn`, `turn.rs`):
1. Turn counter advances; every unit's moves are refilled.
2. City processing, in entity-id order: outposts roll to grow; farmers are raised to the minimum that feeds the city; production is applied (a finished building or unit appears); population grows or shrinks.
3. Treasury per player: city income minus building upkeep, plus spare food sold at two for one, minus unit gold upkeep; units that cannot be fed or paid are disbanded, newest first.
4. Healing: 5% of hit points a turn in the field, 10% in a city, about 17% in a city with an animist's guild, plus a fifth with a healer in the stack.
5. Standing go-to orders continue.

Planned for the same pipeline: enchantment upkeep and expiry, research and mana income, random events, combat triggers, victory checks.

**Movement** (`movement.rs`, `pathing.rs`):
- Movement points are counted in half-moves. A step always costs at least something, and a unit with any moves left may take a step it cannot fully afford (it ends with none).
- Terrain cost comes from the terrain definition; abilities override it (forester, mountaineer, pathfinding, non-corporeal); flying costs one move everywhere; sailing is for ocean and own cities; roads cost half a move between road tiles; wind walking lets one unit carry its whole stack.
- A stack is up to nine units on one tile. Moving onto hostile units or a hostile city starts a fight: the core emits `AttackRequested` and nothing moves.
- Shifting planes needs a tower or planar travel and costs one move.
- Pathfinding is A\* using the slowest unit's cost; hostile tiles other than the target are avoided.
- Visibility is derived each time from units and cities; exploration is remembered per player.

**City economy** (`economy.rs`, `production.rs`): see `rules-framework.md` and the code. Base amounts follow the original's formulas (farmers and workers, farmland cap, taxes, mines, trade, unrest); content supplies everything else as stat modifiers. Buying costs four times what is left from scratch, three times with under half of the remainder done, twice otherwise. Surplus production is lost on completion.

**Subsystems still to build:** research, diplomacy, victory and score, random events.

## 7. Tactical battle module
Not started. Intended design:
- Separate battle state seeded from the world: a small grid, unit placement, terrain and walls for sieges.
- Turn-based with unit initiative, movement points, melee, ranged and spell actions.
- Uses the same stat and ability components as the world, so there is no translation layer.
- Auto-resolve runs the same rules through a simple AI, so results stay consistent.
- Result is a set of events: casualties, XP, loot, and city capture or razing.

## 8. AI
- Layered: strategic (what to build, where to expand, when to attack), operational (unit group tasks), and tactical (battle moves).
- Implement as utility scoring or a behaviour tree over read-only views of the world, outputting `Command`s.
- Difficulty comes from bonuses and knowledge limits, not different rules.
- Start with a simple expansion-and-defend AI, then add aggression and spell use.

## 9. Client and art
- Rendering and input: **macroquad** (decided). Fixed logical layouts scaled by integer factors where the original's screens are reproduced (the city screen uses the original 320x200 layout).
- The renderer reads the core's state plus the event stream and never mutates the world directly; it sends commands.
- Screens today: overworld (scrolling, two planes, fog, units, orders, treasury) and city. Planned: battle, spell book, research, wizard/diplomacy, settings.
- `--shot FILE [--city] [--select] [--turns N]` renders one frame to a PNG and exits, so the client can be checked without a person.
- Audio is not started (`rodio` or SDL_mixer).

### Placeholder art and original data (local only)
- The original game's art and data are copyrighted and have never been released under a free licence, so they are never committed or distributed with this project.
- For development, art is extracted locally from the author's own purchased copy (Steam "Master of Magic Classic") by `f4x-tools extract-art` into `assets-local/art`. Data is imported by `scripts/import_mom.py` into `content-local/`. The original's `.lbx` files and the ReMoM reverse-engineering clone live outside the repo under `~/reference/`; `*.lbx` is gitignored as a safeguard.
- The client loads local art if present and falls back to flat colours otherwise, so the game runs and CI passes without it.
- Before any public release, all placeholders must be replaced with original art or art under a free licence. Treat anything in `assets-local/` and `content-local/` as temporary.

## 10. Cross-cutting
- **Determinism:** seeded RNG (`rand_pcg`) behind `Game::roll`; entity iteration in logic is sorted by entity id; no floats in rules; integer maths with a fixed order in the stats engine.
- **Save/load:** planned: serialize the command log plus periodic snapshots (`serde`). Not implemented.
- **Testing:** unit tests build synthetic content (`testutil`), so rules tests never depend on shipped data. Shipped data is validated in `f4x-data`. Planned: golden-seed scenario tests, property tests for pathfinding and combat, AI-vs-AI soak runs.
- **Performance:** not a concern except pathfinding and AI, which can be budgeted per turn.
- **Lint and format:** `cargo fmt`, `cargo clippy -D warnings` in CI.

## 11. Decisions made so far

Rules and data
- Number scale, magic structure (five realms plus arcane, spell books from picks, per-realm research), heroes and items from day one, and "deviation only after parity" are as in `rules-framework.md` section 8.
- Stats are a closed enum in code; modifiers are `Add`, `Percent`, `Min`, `Max`, `Set`, applied in a fixed order.
- Rate-style quantities have their own stats: growth in people per turn (`CityGrowth`), food after the farmland cap (`CityFoodFlat`), food per farmer, starting experience, unrest percent. Percentages that act on a base use `Percent`.
- Half-units are explicit where the original uses them (half-moves, half-food per terrain tile, half-production per worker).
- Population is kept in citizens (1000 people each) plus tens of people; growth is in tens. Surplus tens carry over instead of being discarded.

Resolving differences between sources (full list in `help-vs-implementation-conflicts.md`)
1. Anything the ReMoM clone marks as a bug in the original game: use ReMoM's fix.
2. Anything ReMoM marks as to-do or not implemented: use the original game's behaviour.
3. Where the in-game help disagrees with the original's code or data: the game files win; the imported help is rewritten with an appended `[Updated from game code: ...]` note.
4. The importer fails loudly when a source no longer matches, and writes everything it cannot map to `content-local/IMPORT_REPORT.txt`.

Economy and movement
- City spacing is four squares; a city works 21 tiles; shared tiles are halved.
- Spare food is sold for gold at two for one after units eat (one food each); units that cannot be fed or paid are disbanded.
- A finished building drops the city back to building housing; a finished unit repeats.
- Buildings a newer building replaces keep their effects and requirement value but cost no upkeep.
- A unit with any movement left may always take a step.

Client and tooling
- macroquad for the client; art and data stay local; screenshots go in `assets-local/shots`.

## 12. Implementation order

Each stage should end with something runnable and tested.

1. **Foundation.** Done: workspace, CI, ECS setup, deterministic RNG, command/event framework. Not done: save/load.
2. **Map and data pipeline.** Done: two-plane grid, RON data, procedural map generator, validation tool.
3. **Minimal client.** Done: macroquad renderer, scrolling, input, placeholder art from the local extractor.
4. **Units and movement.** Done: unit entities, movement rules, A\*, fog of war, plane switching.
5. **Cities and economy.** Done: founding, outposts, population, buildings, production queue, buying, gold and food, turn pipeline. *First playable milestone: expand and build.*
6. **Wizards, mana and spells.** Next. Spell books by realm, research, casting from the overworld, a real effect host over the ECS, enchantments with upkeep.
7. **Combat.** Paused on request. Auto-resolve first, then the tactical battle module with its own screen. Overland attacks already stop at `AttackRequested`.
8. **Heroes, items and experience.** Unit levelling (experience and level effects exist), equipment, hero hiring.
9. **AI v1.** Expansion, production and defence, then attacking, then spell use and battle AI.
10. **Diplomacy, events and victory conditions.**
11. **Content pass.** Fill out races, units, buildings and spells (the largest effort; imported data is the base), then balance using AI-vs-AI soak runs.
12. **Polish.** Art and audio pass, UI quality, settings, tutorial, packaging for Mac and Linux, replays and optional multiplayer through the command log.

**Early risk check (passed for steps 1-5):** the command/event design and the world model held up through units, movement and a full economy without rework. Revisit if wizards or combat strain it.

## 13. Open questions
- Multiplayer: in scope for v1, or only keep the command log compatible?
- Final art sourcing: commissioned, generated, or CC0 packs? (Placeholders come from the author's own copy of the original.)
- Stream of Life and Gaia's Blessing: confirm against the original's assembly (see the conflicts document).
- How heroes' item slots and powers are modelled in data (section 8 of the rules framework fixes the principle, not the schema).
- Save format: pure command log, or snapshots plus log. Entity ids appear in commands, so replays depend on deterministic entity allocation.
