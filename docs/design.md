# High-level design: a Master of Magic-style 4X in Rust with ECS

Status: draft. Scope: gameplay-faithful reimagining, not a port of the original code, data or art.

## 1. Goals and constraints
- Replicate MoM's *gameplay*: two planes (Arcanus/Myrror), wizard-led spell-casting, cities and unit production, hero units, tactical battles, and five magic realms.
- Don't copy its code, data or art. Use original assets and data files; treat the original's exact numbers as reference only.
- Targets are Mac and Linux, with low-res pixel graphics scaled by integer factors.
- The simulation is deterministic: the same seed plus the same command log gives the same game. That gives replays, regression tests, and an optional path to multiplayer.

## 2. Workspace layout
```
mom-core/     rules + simulation, no I/O or rendering
mom-data/     content schemas and loaders (units, spells, buildings, ...)
mom-ai/       AI players, depends on mom-core only
mom-client/   rendering, input, audio, UI
mom-tools/    content validators, map generator CLI, replay runner
content/      RON/TOML data files and assets
```
`mom-core` must never depend on the client. That keeps it testable and fast to iterate.

## 3. ECS choice
Use a standalone ECS (`hecs` or `bevy_ecs`) inside `mom-core`, not the full Bevy engine. The client can still use Bevy, SDL or macroquad for rendering. Keep a plain `World` wrapper so the ECS can be swapped later.

Note: MoM has a few hundred entities with well-defined types, and is turn-based. A plain ID-based `World` with typed arenas (e.g. `slotmap`) would also work; ECS is chosen here for composable abilities/effects and uniform handling of world and battle units. Revisit if it becomes friction.

## 4. Core entities and components

| Entity | Main components |
|---|---|
| **Wizard/Player** | `Wizard` (name, portrait, picks), `Mana`, `Gold`, `Food`, `ResearchQueue`, `KnownSpells`, `Relations`, `IsAI` |
| **Unit** | `UnitKind` (ref to data), `Owner`, `Position`, `Health`, `Stats`, `Movement`, `Experience`, `Abilities`, `Upkeep` |
| **Hero** | Unit components + `HeroClass`, `Equipment`, `Level` |
| **City** | `Owner`, `Position`, `Race`, `Population`, `Buildings`, `ProductionQueue`, `Garrison`, `Happiness` |
| **Map tile** | Stored in a grid resource (not per-entity), per plane: terrain, resources, feature, owner, visibility |
| **Enchantment** | `Target`, `Caster`, `EffectRef`, `Duration/UpkeepMana` |
| **Spell (data)** | Not an entity: defined in data, referenced by ID |
| **Battle** | Temporary world or sub-state: `BattleMap`, `Combatants`, `TurnOrder` |

Map tiles live in a dense grid because they are numerous and accessed by coordinates. Everything with identity and varied attributes is an entity.

### Map layers, features and cities
- **Base terrain** is one grid per plane (ocean, grassland, forest, hills, mountains, swamp, desert, tundra). Arcanus and Myrror share the same terrain classes; in the original the planes differ in art and in what spawns, not in terrain rules, so a terrain definition lists the planes it exists on and the client picks per-plane art.
- **Features** are a second, sparse layer on top of terrain, at most one per tile: minerals and specials, encounter sites (lairs, caves, temples, ruins, keeps, dungeons), towers and magic nodes. In the original, minerals and encounter sites and towers are overlays on ordinary terrain; nodes and volcanoes are dedicated terrain tile types there, but here nodes are also overlays so a node's underlying terrain is kept. (Based on how community clones model the original; not verified against the original's file format.)
- **Myrror-specific content** lives in data, not code: adamantium and crysx exist only there, its gold/mithril/quork weights are higher, minerals are denser by default, and in the original its nodes are stronger and encounters tougher (those last two are not implemented yet).
- **Towers** are placed at identical coordinates on every plane; water under a tower becomes grass. Nodes keep a distance from towers and from each other. Both distances are tunable constants.
- **Cities are ECS entities** (`City` + `Position`), not map tiles. Founding is a command checked against terrain (`can_found_city`), blocking features (towers, nodes and encounter sites block; minerals do not), and a minimum distance to other cities on the same plane (a tunable constant).
- Not yet modelled: node aura zones, rivers, lakes, shores, roads, volcanoes.

## 5. Data-driven content
- Races, units, buildings, spells, abilities, items and terrain are defined in RON files and validated at load time by `mom-tools`.
- Abilities and spell effects are composed from a small set of effect primitives (modify stat, grant ability, deal damage, summon, change terrain, ...). This avoids hardcoding hundreds of one-off spells.
- Side effect: new content needs no recompilation, which is mod-friendly.

## 6. Game flow and systems
**Command layer.** All player and AI actions are `Command` values (`MoveUnit`, `CastSpell`, `SetProduction`, ...). The core validates and applies them, and emits `Event`s for the client to animate. The command log is the save, replay and network format.

**Turn pipeline** (each is a system or ordered group):
1. Start-of-turn: upkeep, mana income, research, food and growth
2. City production and building completion
3. Happiness and unrest
4. Enchantment duration and upkeep checks
5. Random events
6. Player/AI command phase
7. Movement and exploration updates (fog of war)
8. Combat triggers, either battle or auto-resolve
9. Victory/defeat checks

**Subsystems:**
- Pathfinding (A\* over the tile grid, with movement-type rules, plane shifts and portals)
- Visibility/fog of war per player
- Economy (gold, food, mana, with per-city outputs aggregated per player)
- Research (current research, spell books by realm)
- Diplomacy (basic treaties and attitude)

## 7. Tactical battle module
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

## 9. Client
- Fixed 320x200 or 640x400 logical framebuffer, integer-scaled.
- Renderer reads the core's state plus the event stream and never mutates the world directly.
- Screens: overworld, city, battle, spell book, research, wizard/diplomacy, settings.
- A small retained-mode UI layer for panels, lists and tooltips, since the game is menu-heavy.
- Audio via `rodio` or SDL_mixer.

### Placeholder art (local only)
- The original game's art is copyrighted and has never been released under a free licence, so it must never be committed or distributed with this project.
- For development, placeholder sprites are extracted locally from the author's own purchased copy (Steam or GOG) of the original game's `.lbx` files, using a small extractor in `f4x-tools` (reference implementations exist in other open source projects; study their file-format handling, don't copy art).
- The author's downloaded copy of the original (Steam "Master of Magic Classic", app 1146370) lives outside the repo at `~/reference/mom-classic/Master of Magic Official Release/`; the `.lbx` files there are the extractor's input. `*.lbx` is gitignored as a safeguard.
- Extracted files go in `assets-local/`, which is gitignored. The client loads from it if present and falls back to flat-colour rectangles otherwise, so the game runs and CI passes without it.
- Before any public release, all placeholders must be replaced with original art or art under a free licence (e.g. CC0 packs, commissioned work). Treat anything in `assets-local/` as temporary.

## 10. Cross-cutting
- **Determinism:** seeded RNG (`rand_pcg`), no `HashMap` iteration in logic (use `IndexMap`/sorted), no floats in rules.
- **Save/load:** serialize the command log plus periodic snapshots (`serde`).
- **Testing:** golden-seed scenario tests, property tests for pathfinding and combat, AI-vs-AI soak runs.
- **Performance:** not a concern except pathfinding and AI, which can be budgeted per turn.

## 11. Implementation order

Each stage should end with something runnable and tested.

1. **Foundation.** Workspace, CI, ECS setup, deterministic RNG, command/event framework, save/load of an empty world.
2. **Map and data pipeline.** Tile grid with two planes, terrain data in RON, procedural map generator, schema validation tool.
3. **Minimal client.** Framebuffer renderer, integer scaling, map scrolling, input. Placeholder art is fine.
4. **Units and movement.** Unit entities, movement rules, A\* pathfinding, fog of war, plane switching.
5. **Cities and economy.** City founding, population growth, buildings, production queue, gold and food, turn pipeline. *First playable milestone: expand and build.*
6. **Combat.** Auto-resolve first, then the tactical battle module with its own screen.
7. **Wizards, mana and spells.** Spell books by realm, research, casting from the overworld and in battle, the effect-primitive system, enchantments.
8. **Heroes, items and experience.** Unit levelling, equipment, hero hiring.
9. **AI v1.** Expansion, production and defence, then attacking, then spell use and battle AI.
10. **Diplomacy, events and victory conditions.**
11. **Content pass.** Fill out races, units, buildings and spells (the largest effort), then balance using AI-vs-AI soak runs.
12. **Polish.** Art and audio pass, UI quality, settings, tutorial, packaging for Mac and Linux, replays and optional multiplayer through the command log.

**Early risk check:** build steps 1-5 as a thin vertical slice before committing to the full content scope. If the command/event design and the world model feel right at that stage, the rest is mostly volume. If not, it is cheap to change.

## 12. Open questions
- Standalone ECS vs. plain ID-based arenas (see section 3).
- Rendering stack: SDL2/3 vs. macroquad vs. Bevy for the client.
- Multiplayer: in scope for v1, or only keep the command log compatible?
- Final art sourcing: commissioned, generated, or CC0 packs? (Placeholders come from the author's own copy of the original, see section 9.)
