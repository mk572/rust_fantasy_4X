# High-level design: a Master of Magic-style 4X in Rust with ECS

Status: living document. Scope: gameplay-faithful reimagining, not a port of the original code, data or art.
Related documents: [`rules-framework.md`](rules-framework.md) (the rules layer), [`map-layers.md`](map-layers.md) (map layers and generation), [`game-flow.md`](game-flow.md) (commands, turn, movement, production, treasury), [`ecs-structure.md`](ecs-structure.md) (how state is stored and how it differs from the original), [`architecture-plan.md`](architecture-plan.md) (the audit of `Game` and the plan for self-contained domains), [`mechanics-gaps.md`](mechanics-gaps.md) (what the original does that we have not built), [`help-vs-implementation-conflicts.md`](help-vs-implementation-conflicts.md) (resolved and open source conflicts), [`local-assets.md`](local-assets.md) (local-only original art and data).

## 1. Goals and constraints
- Replicate MoM's *gameplay*: two planes (Arcanus/Myrror), wizard-led spell-casting, cities and unit production, hero units, tactical battles, and five magic realms.
- **Priorities, in this order:**
  1. **Playability and gameplay experience.** The game should be good to play.
  2. **Internal logic and consistency.** Rules that agree with each other, with no special cases that have no reason.
  3. **Balance.**
  4. **Adherence to the original's help text and documentation.** What the game tells the player is what it does.
  5. **Code parity.** Matching what the original's code does.

  So the original's code behaviour is followed when it gives better play, logic or balance, and not otherwise: a quirk or bug of the original is copied only if the result is better for one of the first three. Where a higher priority and the original disagree, we depart, and record it in [`divergences.md`](divergences.md). Where we are unsure, we follow the original until we know better.
- Don't copy anything protected by IP into the repository: its art, music and sound, help text (and other prose), and code. The repository is CC0 and says so. Those exist only in gitignored local folders (see section 9 and `local-assets.md`). The game's *rules* (names, costs, stats, effects and numbers of races, units, buildings, spells, items and the global rule values) are not treated as protected and are committed in `content/`, as the importer makes them.
- Targets are Mac and Linux, with low-res pixel graphics scaled by integer factors.
- The simulation is deterministic: the same seed plus the same command log gives the same game. That gives replays, regression tests, and an optional path to multiplayer.

## 2. Workspace layout
```
crates/f4x-core/    rules + simulation, no I/O or rendering
crates/f4x-data/    content loaders and cross-reference validation (RON)
crates/f4x-ai/      AI players, depends on f4x-core only (empty so far)
crates/f4x-console/ the text console: every action as a line of text (`console.md`)
crates/f4x-client/  rendering, input, UI (macroquad)
crates/f4x-tools/   validator, map preview, original-art extractor
content/            shipped, original sample data (terrain, features, a small rules set)
scripts/            import_mom.py (original rules data -> content, help text -> content-local), list_ogbugs.py
docs/               design and reference documents
```
Gitignored, local only: `assets-local/` (extracted art, screenshots), `content-local/` (the help text and the screen-to-topic map imported from the author's own copy of the original game, plus the preset wizards written from them).

`f4x-core` must never depend on the client. That keeps it testable and fast to iterate.

## 3. ECS choice
Decided: `hecs` inside `f4x-core`, not a full engine. Only state with identity and varied attributes is an entity; everything else is plain data owned by `Game`. The client renders with macroquad and only reads the core. The details, and how this differs from the original game's fixed arrays, are in [`ecs-structure.md`](ecs-structure.md).

### Base rule: domain logic is written over the pieces it needs, not over `Game`
Write the rules of a domain (diplomacy, magic, economy, production, movement...) as functions
or methods that take the pieces they use, such as `(&mut World, &Content, &mut GameRng)` or a
domain struct (`&mut Diplomacy`) with a small context, **instead of `&mut Game`**. `Game` owns the
state, routes each `Command` to a domain, builds the arguments by borrowing its fields apart,
and applies what the domain returns; it does not hold the rules.

- A function that reads only takes `&World`, `&Content` and so on, and a function that changes one
  thing takes `&mut` of that thing only. The signature then says what the rule depends on.
- Where a domain needs a few facts from the rest of the game (a wizard's cities, whether a unit is
  visible), it asks through a small read-only trait, not through the whole game.
- A domain's state is private to its module. Other modules call its methods; they do not reach
  into its fields.
- A thin `Game` method that calls the function is fine for the public interface (`Game::apply`,
  queries the client uses); the logic itself is not in it.
- New code follows this from the start. Existing `impl Game` code moves over in the order
  of [`architecture-plan.md`](architecture-plan.md) (tasks 177 to 183), one group of methods at a
  time. The pure parts (`rules/`, `map`, the battle engine) already follow it and are the model.

Why: with every rule a method of one struct, any rule can read or write anything, tests need a
whole game, and the borrow checker forces `&mut self` everywhere. The audit and the plan are in
`architecture-plan.md`.

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

### The map
The world is two wrapping planes of tiles with a base terrain, an optional feature (mineral,
encounter site, tower, node) and a road flag per tile; cities and units are entities on top,
and exploration is remembered per player. Cities are entities, not tiles, and must be at
least four squares apart. Layers, rules, generation and differences from the original are in
[`map-layers.md`](map-layers.md).

## 5. Data-driven content
- Races, units, buildings, spells, abilities, items, terrain and features are defined in RON files and validated at load time (`f4x-tools validate <dir>`): every reference must resolve, ids are `[a-z0-9_]`, spells must have effects, units need their required stats, and so on.
- Definitions refer to each other by string id in files; loading turns them into typed ids into registries, so the simulation never meets a dangling reference.
- Abilities and spell effects are composed from a small set of effect primitives and conditions (modify stat, grant ability, damage, heal, summon, change terrain, enchant, dispel, custom hook). Stats are a closed enum; content cannot invent stats.
- A rule the engine cannot express yet uses the `unimplemented` custom hook. Imported content uses it for 134 of 214 spells; the count of uses is the to-do list for new primitives.
- Two content directories, layered: `content/` (the game's rules data, imported from the original's rules and committed) and `content-local/` (help text and preset wizards from the author's own copy; the client reads it first, then `content/` for every file it lacks). The loader's tests use a small invented rule set in `crates/f4x-data/testdata/`.
- New content needs no recompilation, which is mod-friendly.

## 6. Game flow and systems
All player and AI actions are `Command` values applied with `Game::apply`, which validates
them and returns `Event`s (refusals are events, not panics). Read-only queries take `&self`
and are used freely by the client. A turn is `EndTurn`: refill moves, process cities (outposts
grow, production completes, population changes), settle each player's treasury (income, spare
food sold, upkeep, disbanding the unpaid), heal the wounded, then continue standing orders.
Movement is counted in half-moves, stacks hold nine, and moving onto hostile units emits
`AttackRequested` for the not-yet-built combat layer. Production, buying and selling, the
treasury and healing rules are all written out in [`game-flow.md`](game-flow.md).

Still to build in the pipeline: enchantment upkeep, research and mana income, random events,
combat, diplomacy, and victory and score.

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
- Difficulty is the original's: one setting that fills in the bonuses and penalties the original gives each level (the computer wizards' city bonuses, upkeep cut, starting gold and picks, lair strength, score scale; see `game-settings.md`) **and** sets how well the AI plays. Reversed 16 October 2026: the earlier rule was no bonuses at all.
- Start with a simple expansion-and-defend AI, then add aggression and spell use.

**The AI is being revamped (decided October 2026).** Two rules from now on:
1. **No hard-coded AI decisions in the game rules.** The core holds state, validates commands and applies them. Anything an AI player would *decide* (whether to accept a treaty, what to build, where to attack, how it answers a proposal) lives in `f4x-ai` and reaches the core only as ordinary commands, the same ones a human uses. Diplomacy already works this way (`diplomacy.md` section 9.2).
2. **No information advantage for the AI, and no special rules for neutral cities and units.** The AI sees what a human would see and obeys the same commands. Neutral cities (rebels, raiders) and neutral units (monsters, lair guards) use the ordinary rules for growth, production, upkeep, stats and combat, with no bonus or penalty because they are neutral. **Changed 16 October 2026:** the difficulty bonuses and penalties of the *computer wizards* (food, production, gold, mana, research, growth and outpost growth up to 400%, a cut in maintenance and in unit upkeep, extra picks, the starting gold) are now **part of the game settings** (`game-settings.md`): the levels carry the original's figures, the advanced screen can change or switch them off, and a symmetric game (`GameSettings::flat`) stays for tests and the learning AI. We still skip AI-only branches inside rule code other than these, AI knowledge of hidden information, and the original's neutral-city growth halving.

**Human and AI follow the same rules (decided 16 October 2026).** An exception for the human or for the AI is allowed only for one of two reasons: an unavoidable limitation (the human plays through the client UI, so a pop-up or a named hero has to wait for an answer; the AI has no screen) or a specific difficulty setting (`game-settings.md`). Everything else, such as who meets the merchant, who is offered mercenaries or heroes, what an offer costs and what it can do, is the same for both. Where the original gives only one side something (its merchant calls on the human only), we give it to both and record the change in `divergences.md`. Where the original's AI bypasses a rule the human must obey, we do not copy that. New rules are written once, in the core, and the AI answers them through ordinary commands.

## 9. Client and art
- Rendering and input: **macroquad** (decided). Fixed logical layouts scaled by integer factors where the original's screens are reproduced (the city screen uses the original 320x200 layout).
- The renderer reads the core's state plus the event stream and never mutates the world directly; it sends commands.
- Screens today: overworld (scrolling, two planes, fog, units, orders, treasury) and city. Planned: battle, spell book, research, wizard/diplomacy, settings.
- `--shot FILE [--city] [--select] [--turns N]` renders one frame to a PNG and exits, so the client can be checked without a person.
- Audio is not started (`rodio` or SDL_mixer).

### Placeholder art and original data (local only)
- The original game's art, music, help text and code are copyrighted and have never been released under a free licence, so they are never committed or distributed with this project. Its rules data (races, units, buildings, spells, items, abilities, realms, level paths and rule values) is committed in `content/`.
- For development, art is extracted locally from the author's own purchased copy (Steam "Master of Magic Classic") by `f4x-tools extract-art` into `assets-local/art`. Rules data is imported by `scripts/import_mom.py` into `content/` (committed), and the help text into `content-local/`. The original game files and the ReMoM reverse-engineering clone live in `reference/` inside the project directory, which is gitignored (and so is `*.lbx`, as a second safeguard).
- The client loads local art if present and falls back to flat colours otherwise, so the game runs and CI passes without it.
- Before any public release, all placeholders must be replaced with original art or art under a free licence. Treat anything in `assets-local/` and `content-local/` as temporary. The rules data in `content/` stays: it is ours to change.

## 10. Cross-cutting
- **Determinism:** seeded RNG (`rand_pcg`) behind `Game::roll`; entity iteration in logic is sorted by creation number (`Serial`), never by `hecs` id; no floats in rules; integer maths with a fixed order in the stats engine.
- **Save/load:** done. A save is a text snapshot (RON) of the whole game with content referenced by name, a version number and load-time checks; autosave runs each turn. Details in [`game-flow.md`](game-flow.md) section 8.
- **Testing:** unit tests build synthetic content (`testutil`), so rules tests never depend on shipped data. Shipped data is validated in `f4x-data`. Planned: golden-seed scenario tests, property tests for pathfinding and combat, AI-vs-AI soak runs.
- **Where tests live:** unit tests stay in the file they test as a child module (`#[cfg(test)] mod tests`), so they can reach private items and are left out of release builds. When a file's test module goes over 250 lines (the size of the test module alone; the file's total size does not matter), the tests move to their own file next to it, still as a child module: `#[cfg(test)] mod tests;` in `foo.rs` and the tests in `foo/tests.rs` (`tests.rs` beside `mod.rs` or `lib.rs`). Tests that use only the public API go in the crate's `tests/` directory. Do not make items `pub` just to test them from another file.
- **Performance:** not a concern except pathfinding and AI, which can be budgeted per turn.
- **Lint and format:** `cargo fmt`, `cargo clippy -D warnings` in CI.

## 11. Decisions made so far

Deliberate departures from the original are listed in [`divergences.md`](divergences.md); add new ones there.

Code structure
- Domain logic takes the pieces it needs, not `&mut Game` (section 3, base rule). `Game` owns, routes and applies.

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

Saving
- Snapshots, not command logs: a save restores the state directly. Content is referenced by string id, so saves survive content being added or reordered and fail clearly when content is missing. Units and cities carry a permanent creation number used for ordering and saved, so a loaded game continues exactly like the original.

Client and tooling
- macroquad for the client; art and data stay local; screenshots go in `assets-local/shots`.

## 12. Implementation order

Each stage should end with something runnable and tested.

1. **Foundation.** Done: workspace, CI, ECS setup, deterministic RNG, command/event framework, save/load.
2. **Map and data pipeline.** Done: two-plane grid, RON data, procedural map generator, validation tool.
3. **Minimal client.** Done: macroquad renderer, scrolling, input, placeholder art from the local extractor.
4. **Units and movement.** Done: unit entities, movement rules, A\*, fog of war, plane switching.
5. **Cities and economy.** Done: founding, outposts, population, buildings, production queue, buying, gold and food, turn pipeline. *First playable milestone: expand and build.*
6. **Wizards, mana and spells.** Done (first pass, see [`magic.md`](magic.md)): books and retorts, power split, spell library, research, overland casting through a real effect host over the ECS, enchantments with mana upkeep, magic screen. Open: fortress, nodes, heroes' casting, most retort effects, timed durations, the many spells still on the `unimplemented` hook.
7. **Combat.** Under way: the battle model, attack resolution and tactical map are in `f4x-core` (see `combat.md`); immunities, special attacks, sieges, casting, the battle screen, auto-resolve and the battle AI follow. Overland attacks stop at `AttackRequested`, which a client turns into `Command::BeginBattle`.
8. **Heroes, items and experience.** Unit levelling (experience and level effects exist), equipment, hero hiring.
9. **AI v1.** Expansion, production and defence, then attacking, then spell use and battle AI.
10. **Diplomacy, events and victory conditions.**
11. **Content pass.** Fill out races, units, buildings and spells (the largest effort; imported data is the base), then balance using AI-vs-AI soak runs.
12. **Polish.** Art and audio pass, UI quality, settings, tutorial, packaging for Mac and Linux, replays and optional multiplayer through the command log.

**Early risk check (passed for steps 1-5):** the command/event design and the world model held up through units, movement and a full economy without rework. Revisit if wizards or combat strain it.

## 13. Open questions
- Multiplayer: in scope for v1, or only keep the command log compatible? The command log now exists for local play (task 74); the scope question is `PLANNED.md` item 36.
- Final art sourcing: commissioned, generated, or CC0 packs? (Placeholders come from the author's own copy of the original.)
- Stream of Life and Gaia's Blessing: confirm against the original's assembly (see the conflicts document).
- How heroes' item slots and powers are modelled in data (section 8 of the rules framework fixes the principle, not the schema): proposed in [`heroes-items-design.md`](heroes-items-design.md).
- Replays: commands name entities, and entity ids change on a load, so the log writes creation numbers (`Serial`) in their place (`replay.rs`, task 74, built for local play: replays and saving a game with its history).

## 14. Later considerations

### A self-learning AI
Not planned for v1. Assessment from the time of writing, for when it is revisited.

**What already helps:** the engine is deterministic and headless (`Command` in, `Event` out,
seeded RNG), so a bot can play thousands of games without the client; saves allow branching a
game from any state; the engine rejects illegal commands, so a bot cannot cheat or crash it.

**What makes it hard:**
1. Combat does not exist yet (tasks 33-42), and an AI that cannot fight cannot learn to play.
2. The action space is large (hundreds of commands per turn: moves, production, research, spells
   with targets, tax) and games run for hundreds of turns, so the reward comes very late. Plain
   reinforcement learning from scratch struggles with this.
3. Fog of war and several opponents make the problem noisy.
4. Learning needs millions of games; a full game on a 60x40 two-plane map is much slower than a
   board game. Measure the engine's turns per second before committing.
5. Every rule or content change makes a trained bot partly obsolete, so the content pass should
   settle first.

**Options, easiest first:**

| Approach | Effort | Result |
|---|---|---|
| Scripted AI with weights (expand, build, research, attack) | weeks | acceptable, predictable play, like the original |
| The same, weights tuned by self-play (evolutionary search over a few dozen parameters) | plus weeks | cheap, robust "self-learning"; most of the benefit |
| Monte Carlo tree search for tactical battles, on top of the scripted AI | plus weeks | strong battle play; battles are small and self-contained |
| Neural-network policy trained by self-play (AlphaZero style) | months plus GPU time | possible in principle, a research project for a 4X game |

**Suggested path:**
1. Finish combat and a scripted AI first (tasks 33-46 and 65-67); it is the baseline and the
   training opponent.
2. Build an AI-vs-AI soak runner (also wanted for balancing, planned 24); it is the harness any
   learning would use.
3. Tune the scripted AI's weights by self-play.
4. Consider learning only for the battle AI (task 46), where search or a small model can work.

A fully learned end-to-end bot is realistic only as its own long-term project.
