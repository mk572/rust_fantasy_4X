# High-level design: a Master of Magic-style 4X in Rust with ECS

Status: living document. Scope: gameplay-faithful reimagining, not a port of the original code, data or art.
Related documents: [`rules-framework.md`](rules-framework.md) (the rules layer), [`map-layers.md`](map-layers.md) (map layers and generation), [`game-flow.md`](game-flow.md) (commands, turn, movement, production, treasury), [`ecs-structure.md`](ecs-structure.md) (how state is stored and how it differs from the original), [`architecture-plan.md`](archive/architecture-plan.md) (the audit of `World` and the plan for self-contained domains), [`mechanics-gaps.md`](archive/mechanics-gaps.md) (what the original does that we have not built), [`help-vs-implementation-conflicts.md`](archive/help-vs-implementation-conflicts.md) (resolved and open source conflicts), [`local-assets.md`](local-assets.md) (local-only original art and data), [`sound.md`](sound.md) (every music track and sound effect of the original and when it plays).

## 1. Goals and constraints
- Replicate MoM's *gameplay*: two planes (Arcanus/Myrror), wizard-led spell-casting, cities and unit production, hero units, tactical battles, and five magic realms.
- **Priorities, in this order:**
  1. **Playability and gameplay experience.** The game should be good to play.
  2. **Internal logic and consistency.** Rules that agree with each other, with no special cases that have no reason.
  3. **Balance.**
  4. **Adherence to the original's help text and documentation.** What the game tells the player is what it does.
  5. **Code parity.** Matching what the original's code does.

  So the original's code behaviour is followed when it gives better play, logic or balance, and not otherwise: a quirk or bug of the original is copied only if the result is better for one of the first three. Where a higher priority and the original disagree, we depart, and record it in [`divergences.md`](divergences.md). Where we are unsure, we follow the original until we know better.
- Don't copy anything protected by IP into the repository: its art, music and sound, help text (and other prose), and code. The repository is CC0 and says so. Those exist only in gitignored local folders (see section 9 and `local-assets.md`). The game's *rules* (names, costs, stats, effects and numbers of races, units, buildings, spells, items and the global rule values) are not treated as protected and are committed in `crates/f4x-def/content/`, as the importer makes them.
- Targets are Mac and Linux, with low-res pixel graphics scaled by integer factors.
- The simulation is deterministic: the same seed plus the same command log gives the same game. That gives replays, regression tests, and an optional path to multiplayer.

## 2. Workspace layout
```
crates/f4x-def/      the static definitions, their loaders and cross-reference validation (RON); `content/` is the shipped rules data
crates/f4x-battle/   the tactical battle (reads f4x-def): the rules and the field of a fight; core builds on it, and the elements of the battle move down from core into it step by step
crates/f4x-core/     the world (reads f4x-def and f4x-battle): what is in it and what can be done in it; no I/O but save files, no rendering, never depends on the client
crates/f4x-newgame/  what runs once per game: map generator, placement of nodes, lairs and neutral cities, the wizards and their start (uses f4x-core's public API)
crates/f4x-lang/     the words of the game: messages by key, names, name lists (`text.md`)
crates/f4x-ai/       the AI players: they see what a player sees and answer with `Command`s (`ai-design.md`)
crates/f4x-master/   the game: it owns the world and advances time (rounds, a turn per player, its end), saves, undo and replays (`turn-structure.md`)
crates/f4x-console/  the text console: every action as a line of text (`console.md`)
crates/f4x-client/   rendering, input, UI (macroquad); `extract/` reads the original's art
crates/f4x-setup/    the setup program: finds the player's copy of the original and extracts its pictures (later sound and music) into the data folder; depends on `f4x-extract` and `f4x-lang` only (`installer-design.md`)
crates/f4x-tools/    validator, map preview, soak runner, other tools
crates/f4x-ai-ref/   a frozen copy of the AI, outside the workspace, for AI battles (`ai-design.md` 8.1a)
scripts/             helper scripts (table in `scripts.md`)
docs/                design and reference documents
```

`tools/f4x-learn/`, the learning environment of the battle AI (`learning-ai.md`), is a workspace of its own: a training tool, not a part of the game.
Gitignored, local only: `assets-local/` (extracted art, screenshots), `content-local/` (the help text and the screen-to-topic map imported from the author's own copy of the original game, plus the preset wizards written from them), `reference/` (the original's files and the ReMoM clone).

Dependencies point one way, `f4x-client` -> `f4x-master` -> `f4x-core` -> `f4x-battle` -> `f4x-def`, and nothing points up that line; `f4x-newgame` sits above core and uses only its public API. Each crate's `Cargo.toml` says what it uses, and a crate loads the data it holds. `f4x-core` must never depend on the client or on a crate above it; that keeps it testable and fast to iterate.

**The world and the game** (task 822, [`turn-structure.md`](turn-structure.md) section 10). `f4x-core`'s `World` says what is in the world and what can be done in it, one operation at a time (apply a command, start a player's turn, end a round); it knows no round counter, no order of play and no end of the game. `f4x-master`'s `Game` owns a `World` and advances time: the round, whose turn it is, when each operation runs, the end of the game, the open reactions, the random numbers, saves, undo and replays. No crate below `f4x-master` depends on it, for tests either: a test that needs turns to pass, replays or undo lives in `f4x-master`.

**What stays in `f4x-core/src/battle` and why** (the file check of task 741, 21 November 2026). A rule of the fight, a type of it or a function of the battle alone is `f4x-battle`'s (or `f4x-def`'s when the rules of the content need it too): the battle, its spells (`Battle::apply_spell`, unit casts, the demon), the spoils, who comes back as undead, where the units stand and the settings that change a fight. What stays in core is whatever reads or writes the overland world, and it only gathers the facts and hands them over as arguments (`AimWorld`, `CastWorld`, small structs and flags): building a battle unit from an overland one, the choice of who fights, the field of a square (terrain, roads, city, walls), the wizard's casts and mana, the save and load that tie a battle to the world's entities, and the write-back of a finished battle (wounds, deaths, items, spoils, relations, fleeing, the winner moving in, raised undead, conquest). A function that stays is thin: if it has a rule in it that needs no world, the rule moves.


## 3. ECS choice
Decided: `hecs` inside `f4x-core`, not a full engine. Only state with identity and varied attributes is an entity; everything else is plain data owned by `World`. The client renders with macroquad and only reads the core. The details, and how this differs from the original game's fixed arrays, are in [`ecs-structure.md`](ecs-structure.md).

### Base rule: domain logic is written over the pieces it needs, not over `World`
Write the rules of a domain (diplomacy, magic, economy, production, movement...) as functions
or methods that take the pieces they use, such as `(&mut hecs::World, &Content, &mut GameRng)` or a
domain struct (`&mut Diplomacy`) with a small context, **instead of `&mut World`**. `World` owns the
state, routes each `Command` to a domain, builds the arguments by borrowing its fields apart,
and applies what the domain returns; it does not hold the rules.

- A function that reads only takes `&hecs::World`, `&Content` and so on, and a function that changes one
  thing takes `&mut` of that thing only. The signature then says what the rule depends on.
- Where a domain needs a few facts from the rest of the game (a wizard's cities, whether a unit is
  visible), it asks through a small read-only trait, not through the whole game.
- A domain's state is private to its module. Other modules call its methods; they do not reach
  into its fields.
- A thin `World` method that calls the function is fine for the public interface (`World::apply`,
  queries the client uses); the logic itself is not in it.
- New code follows this from the start. The pure parts (`rules/`, `map`, the battle engine) are the model; older `impl World` code is moved over a group of methods at a time
  (the audit and the plan are in [`architecture-plan.md`](archive/architecture-plan.md)).

Why: with every rule a method of one struct, any rule can read or write anything, tests need a
whole game, and the borrow checker forces `&mut self` everywhere. The audit and the plan are in
`archive/architecture-plan.md`.

## 4. Core entities and components

The entity model is written out in [`ecs-structure.md`](ecs-structure.md) section 3; that document is the one to keep current. In short, the entities of the game are:

| Entity | Components |
|---|---|
| **Player** | `PlayerId`, treasury (gold, mana), tax rate, offers, defeat, control (human or AI), link to its wizard; a `Neutral` marker for the independent cities and the monsters |
| **Wizard** | what the creation screen makes (name, portrait, banner, capital race, books, retorts) plus known spells, research, casting skill, fame, personality, objective, fortress, summoning circle, banishment |
| **Unit** | definition id, owner, position, damage, experience, moves, orders, temporary state |
| **Hero** | a hero's identity (abilities, casting skill, spells, state: free, serving one wizard, dead, gone); the unit components while serving. One hero entity of each of the 35 types in the whole game |
| **City** | name, owner, position, population, buildings, race, production |
| **Spell instance** | spell definition, caster (a wizard), target (global, wizard, city, unit, feature, square, battlefield), duration: global, city and unit enchantments, the cast in progress and battle spells |
| **Terrain feature** | feature kind, position; node, site (lair) or volcano data; owner: towers, nodes, encounter sites and volcanoes (minerals stay a map layer) |
| **Item** | kind, name, bonus, powers, charges, prefab, and who holds it (a hero, the armory or a lair's hoard) |
| **Definitions** (spells, units, buildings, features, items...) | not entities: typed-id registries loaded from data |
| **Battle** | a `Battle` outside the world, with a small world of its own for its spell instances |

Links between entities are `Entity` values; `PlayerId` identifies a player for what lives outside the
world (fog, diplomacy, saves, display).

Derived values are never stored: unit stats, city yields, rebels and growth are computed on demand from the definition plus modifiers.

### The map
The world is two wrapping planes of tiles with a base terrain, a road flag, rivers, lakes and
corruption per tile. What is placed on it (minerals, encounter sites, towers, nodes, volcanoes) is a
terrain feature entity, found by a position index; cities and units are entities too, and
exploration is remembered per player. The three node kinds also have a base terrain of their own
under the node (task 338). Cities are entities, not tiles, and must be at least four squares
apart. Layers, rules, generation and differences from the original are in
[`map-layers.md`](map-layers.md).

## 5. Data-driven content
- Races, units, buildings, spells, abilities, items, terrain and features are defined in RON files and validated at load time (`f4x-tools validate <dir>`): every reference must resolve, ids are `[a-z0-9_]`, spells must have effects, units need their required stats, and so on.
- Definitions refer to each other by string id in files; loading turns them into typed ids into registries, so the simulation never meets a dangling reference.
- Abilities and spell effects are composed from a small set of effect primitives and conditions (modify stat, grant ability, damage, heal, summon, change terrain, enchant, dispel, custom hook). Stats are a closed enum; content cannot invent stats.
- A rule the engine cannot express yet uses the `unimplemented` custom hook (`hook.rs`); no shipped spell uses it now, and a count of uses is the to-do list for new primitives.
- Two content directories, layered: `crates/f4x-def/content/` (the game's rules data, imported from the original's rules and committed) and `content-local/` (help text and preset wizards from the author's own copy; the client reads it first, then `crates/f4x-def/content/` for every file it lacks). The loader's tests read `crates/f4x-def/content/` alone (never `content-local/`), so they run on the data the game has.
- New content needs no recompilation, which is mod-friendly.

## 6. Game flow and systems
All player and AI actions are `Command` values sent to the game (`Game::apply(player, command)`, `f4x-master`), which refuses what is not the player's to give and passes the rest to `World::apply`; that validates them and returns `Event`s (refusals are events, not panics). Read-only queries take `&self`
and are used freely by the client. Time passes in rounds, and in each round every player takes a turn: the turn start grows outposts and cities, heals the wounded, refills moves; the turn end walks the units with a standing order on, settles the treasury (income, spare food sold, upkeep, disbanding the unpaid), runs the wizard's magic and completes production; the round end runs the world-wide work once.
Movement is counted in moves with halves (`HalfInt`; a road step costs 0.5), stacks hold nine, and moving onto hostile units emits `AttackRequested`, which a client turns into `Command::BeginBattle`. Production, buying and selling, the
treasury and healing rules are all written out in [`game-flow.md`](game-flow.md); the order of a round and of the players' turns is in [`turn-structure.md`](turn-structure.md).

## 7. Tactical battle module
A `Battle` is separate state seeded from the world, played with the same stat and ability components as the world (no translation layer); auto-resolve runs the same rules through the battle AI, and the result is a set of events (casualties, XP, loot, city capture or razing). The rules are in [`combat.md`](combat.md).

### Damage types and defence rules

Every attack is made of a set of damage types (`f4x-def/src/rules/damage_type.rs`): its form (`MundaneWeapon`, `Missile`, `Magic`), its element (`Fire`, `Cold`, `Lightning`, `Poison`, `Stoning`, `Death`, `Illusion`), its realm (`NatureRealm` to `ArcaneRealm`) and its delivery (`Melee` or `Ranged`, added where the attack is made). What a unit does against them is a list of defence rules on its abilities and on the lasting spells it carries (`defence` in `abilities.ron` and `spells.ron`): a rule matches an attack with any of its `against` types and all of its `with` types, and is an immunity (no damage), a seen-through illusion, a defence floor, or a bonus to defence or resistance. Tests read the shipped data; the tables below are generated from it (19 November 2026) and list what the data says, not the original's behaviour (differences: `divergences.md` 48). Open: magic immunity (task 711), doom damage and Black Sleep (714), the illusion set (712), death immunity (713).

**Attacks of unit abilities** (`attack`; an ability not listed adds no type):

| # | Ability | Delivery | Types | Armour piercing |
|---|---|---|---|---|
| 1 | armor_piercing | Any |  | yes |
| 3 | bow | Ranged | Missile, MundaneWeapon |  |
| 14 | death_bolt | Ranged | Magic, DeathRealm |  |
| 15 | death_gaze | Ranged | Magic, DeathRealm |  |
| 18 | drow_bolt | Ranged | Magic, ChaosRealm |  |
| 20 | fire_breath | Ranged | Fire, Magic, ChaosRealm |  |
| 22 | fireball_attack | Ranged | Fire, Magic, ChaosRealm |  |
| 29 | ice_bolt | Ranged | Cold, Magic, NatureRealm |  |
| 30 | illusion_attack | Any | Illusion |  |
| 36 | lightning_bolt | Ranged | Lightning, Magic, ChaosRealm |  |
| 37 | lightning_breath | Ranged | Lightning, Magic, ChaosRealm | yes |
| 46 | doom_gaze | Ranged | Magic, Doom, ChaosRealm |  |
| 47 | nature_bolt | Ranged | Magic, NatureRealm |  |
| 57 | rock_throwing | Ranged | Missile, MundaneWeapon |  |
| 58 | shaman_bolt | Ranged | Magic, NatureRealm |  |
| 59 | sling | Ranged | Missile, MundaneWeapon |  |
| 60 | sorcery_bolt | Ranged | Magic, SorceryRealm |  |
| 63 | sprite_bolt | Ranged | Magic, NatureRealm |  |
| 64 | stoning_gaze | Ranged | Magic, NatureRealm |  |
| 69 | thrown | Ranged | Missile, MundaneWeapon |  |

**Defence rules of abilities and spells** (`against`: any of; `with`: all of; empty `against`: every attack):

| Source | Against | With | Effect |
|---|---|---|---|
| ability cold_immunity | Cold |  | Immune |
| ability death_immunity | Death |  | Immune |
| ability fire_immunity | Fire |  | Immune |
| ability illusion_immunity | Illusion |  | SeeThrough |
| ability large_shield | any | Ranged | DefenceBonus(2) |
| ability magic_immunity | Magic |  | Immune |
| ability missile_immunity | Missile |  | Immune |
| ability poison_immunity | Poison |  | Immune |
| ability stoning_immunity | Stoning |  | Immune |
| ability weapon_immunity | MundaneWeapon |  | DefenceFloor(10) |
| spell resist_elements | ChaosRealm, NatureRealm |  | DefenceBonus(3) |
| spell resist_elements | ChaosRealm, NatureRealm |  | ResistanceBonus(3) |
| spell elemental_armor | ChaosRealm, NatureRealm |  | DefenceBonus(10) |
| spell elemental_armor | ChaosRealm, NatureRealm |  | ResistanceBonus(10) |
| spell resist_magic | NatureRealm, SorceryRealm, ChaosRealm, LifeRealm, DeathRealm, ArcaneRealm |  | ResistanceBonus(5) |
| spell bless | ChaosRealm, DeathRealm |  | DefenceBonus(3) |
| spell bless | ChaosRealm, DeathRealm |  | ResistanceBonus(3) |
| spell righteousness | ChaosRealm, DeathRealm |  | Immune |
| spell righteousness | ChaosRealm, DeathRealm |  | ResistanceBonus(30) |

**Damage of spells** (`Damage` in `spells.ron`; `Slay` and the `save` of a curse list the types an immunity refuses them by):

| # | Spell | Strength | Types |
|---|---|---|---|
| 13 | ice_bolt | 5 | Cold, Magic, NatureRealm |
| 25 | ice_storm | 6 | Cold, Magic, NatureRealm |
| 50 | psionic_blast | 5 | Illusion, Magic, SorceryRealm |
| 83 | fire_bolt | 5 | Fire, Magic, ChaosRealm |
| 91 | lightning_bolt | 5 | Lightning, Magic, ChaosRealm |
| 96 | fireball | 5 | Fire, Magic, ChaosRealm |
| 101 | warp_lightning | 10 | Lightning, Magic, ChaosRealm |
| 104 | doom_bolt | 10 | Magic, ChaosRealm |
| 107 | fire_storm | 8 | Fire, Magic, ChaosRealm |
| 109 | flame_strike | 15 | Fire, Magic, ChaosRealm |
| 122 | star_fires | 15 | Magic, LifeRealm |

| # | Spell | Refused by an immunity to |
|---|---|---|
| 22 | petrify | Stoning |
| 46 | confusion | Illusion |
| 53 | vertigo | Illusion |
| 67 | mind_storm | Illusion |
| 73 | creature_binding | Illusion |
| 162 | weakness | Death |
| 165 | black_sleep | Death |
| 172 | possession | Death |
| 194 | death_spell | Death |

**Dice of a part** (`types` on a `Modify` of `Melee` or `Ranged`, `bonus_types` of an item power): Flame Blade (spell 94) and the Flaming item power add fire dice; a defender immune to fire loses them.
**Positional defence** is not a type rule: a city wall gives `wall_defence` against any attack from outside the city area, the rest of the city `city_defence` (`rules.ron`, `battle/mod.rs` `city_defense_bonus`).


## 8. AI
- Layered: strategic (what to build, where to expand, when to attack), operational (unit group tasks), and tactical (battle moves).
- Implement as utility scoring or a behaviour tree over read-only views of the world, outputting `Command`s.
- Difficulty is the original's: one setting that fills in the bonuses and penalties the original gives each level (the computer wizards' city bonuses, upkeep cut, starting gold and picks, lair strength, score scale; see `game-settings.md`) **and** sets how well the AI plays. Reversed 16 October 2026: the earlier rule was no bonuses at all.

**Two rules for the AI (decided October 2026).**
1. **No hard-coded AI decisions in the game rules.** The core holds state, validates commands and applies them. Anything an AI player would *decide* (whether to accept a treaty, what to build, where to attack, how it answers a proposal) lives in `f4x-ai` and reaches the core only as ordinary commands, the same ones a human uses. Diplomacy already works this way (`diplomacy.md` section 9.2). So do the independent cities: the cities of one race are one neutral player (task 522), played by the standard AI like any wizard, diplomacy included (it talks, trades and makes treaties through the same commands; it has no spells, so no spell trades, gifts of spells or Mastery; `diplomacy.md` "Neutral players", `statecraft.rs`); `f4x_ai::NeutralAi` keeps an `Ai` for each race player and plays the monsters in their turn, which every host that ends turns calls (the client, the console's `end`, `play_round`).
2. **No information advantage for the AI, and no special rules for neutral cities and units.** The AI sees what a human would see and obeys the same commands. Neutral cities (rebels, raiders) and neutral units (monsters, lair guards) use the ordinary rules for growth, production, upkeep, stats and combat, with no bonus or penalty because they are neutral. **Changed 16 October 2026:** the difficulty bonuses and penalties of the *computer wizards* (food, production, gold, mana, research, growth and outpost growth up to 400%, a cut in maintenance and in unit upkeep, extra picks, the starting gold) are now **part of the game settings** (`game-settings.md`): the levels carry the original's figures, the advanced screen can change or switch them off, and a symmetric game (`GameSettings::flat`) stays for tests and the learning AI. We still skip AI-only branches inside rule code other than these, AI knowledge of hidden information, and the original's neutral-city growth halving. **Temporary exception (28 November 2026, until task 851):** the computer player judges an unseen lair by its stored apparent strength, which the client does not show the player yet.

**Human and AI follow the same rules (decided 16 October 2026).** An exception for the human or for the AI is allowed only for one of two reasons: an unavoidable limitation (the human plays through the client UI, so a pop-up or a named hero has to wait for an answer; the AI has no screen) or a specific difficulty setting (`game-settings.md`). Everything else, such as who meets the merchant, who is offered mercenaries or heroes, what an offer costs and what it can do, is the same for both. Where the original gives only one side something (its merchant calls on the human only), we give it to both and record the change in `divergences.md`. Where the original's AI bypasses a rule the human must obey, we do not copy that. New rules are written once, in the core, and the AI answers them through ordinary commands.

## 9. Client and art
- Rendering and input: **macroquad** (decided). Fixed logical layouts scaled by integer factors where the original's screens are reproduced (the city screen uses the original 320x200 layout).
- The renderer reads the core's state plus the event stream and never mutates the world directly; it sends commands. The screens and what is built of them are in `screen_list.md`; the catalogue of what plays when is `sound.md`.
- `--shot FILE [--city] [--select] [--turns N]` renders one frame to a PNG and exits, so the client can be checked without a person.

### Placeholder art and original data (local only)
- The original game's art, music, help text and code are copyrighted and have never been released under a free licence, so they are never committed or distributed with this project. Its rules data (races, units, buildings, spells, items, abilities, realms, level paths and rule values) is committed in `crates/f4x-def/content/`.
- For development, art is extracted locally from the author's own purchased copy (Steam "Master of Magic Classic") by `f4x-extract art` into `assets-local/art`. The rules data is kept by hand in `crates/f4x-def/content/` (committed), and the game's texts are its own (`crates/f4x-lang/content/text/`); `scripts/import_mom_text.py` can write the original's prose to `content-local/` for comparison. The original game files and the ReMoM reverse-engineering clone live in `reference/` inside the project directory, which is gitignored (and so is `*.lbx`, as a second safeguard).
- The client loads local art if present and falls back to flat colours otherwise, so the game runs and CI passes without it.
- Before any public release, all placeholders must be replaced with original art or art under a free licence. Treat anything in `assets-local/` and `content-local/` as temporary. The rules data in `crates/f4x-def/content/` stays: it is ours to change.

## 10. Cross-cutting
- **Determinism:** seeded RNG (`rand_pcg`), held by the `Game` and passed to each world operation that draws (`&mut GameRng`); entity iteration in logic is sorted by creation number (`Serial`), never by `hecs` id; no floats in rules; integer maths with a fixed order in the stats engine.
- **Save/load:** a save is a text snapshot (RON) of the whole game with content referenced by number, a version number and load-time checks; the client autosaves at the start of each round. Details in [`game-flow.md`](game-flow.md) section 8.
- **Replays:** commands name entities, and entity ids change on a load, so a command written down carries creation numbers (`Serial`) in their place (`f4x-core`, `world/stable_ref.rs`); the log and its playback are the game's (`f4x-master`, `game/replay.rs`).
- **Testing and where tests live:** the general rules are in [`rust_coding_rules.md`](rust_coding_rules.md), how they apply here in [`testing.md`](testing.md).
- **Lint and format:** `cargo fmt`, `cargo clippy` clean.

## 11. Decisions made so far

Deliberate departures from the original are listed in [`divergences.md`](divergences.md); add new ones there.

Code structure
- Domain logic takes the pieces it needs, not `&mut World` (section 3, base rule). `World` owns, routes and applies.

Rules and data
- Number scale, magic structure (five realms plus arcane, spell books from picks, per-realm research), heroes and items from day one, and "deviation only after parity" are as in `rules-framework.md` section 8.
- Stats are a closed enum in code; modifiers are `Add`, `Percent`, `Min`, `Max`, `Set`, applied in a fixed order.
- Rate-style quantities have their own stats: growth in people per turn (`CityGrowth`), food after the farmland cap (`CityFoodFlat`), food per farmer, starting experience, unrest percent. Percentages that act on a base use `Percent`.
- Half-units are explicit where the original uses them: every amount counted in halves is a `HalfInt` (food, production, gold, mana, moves, sight; `crates/f4x-def/src/half_int.rs`), written in files as the amount (`1`, `0.5`, `1.5`) and shown with the ½ glyph. A bare `i32` beside a `HalfInt` means whole units. The one finer count is the land's food of a city, a `QuarterInt` (`crates/f4x-def/src/quarter_int.rs`, steps of 0.25) because a shared tile gives half of a half; the quarters of the area are added up first and rounded down to a half (the city's food) or a whole number (the size the land allows) only at the end.
- Population is kept in citizens (1000 people each) plus tens of people; growth is in tens. Surplus tens carry over instead of being discarded.

Resolving differences between sources (full list in `archive/help-vs-implementation-conflicts.md`)
1. Anything the ReMoM clone marks as a bug in the original game: use ReMoM's fix.
2. Anything ReMoM marks as to-do or not implemented: use the original game's behaviour.
3. Where the in-game help disagrees with the original's code or data: decide by the priorities above. Where the game does not follow the help, the imported help is rewritten to say what the game does (the reason is in the importer's report, not in the text the player reads); the game's own help is `crates/f4x-lang/content/text/en/help/help.ron`.
4. The importer fails loudly when a source no longer matches, and writes everything it cannot map to `content-local/IMPORT_REPORT.txt`.

Economy and movement
- City spacing is four squares; a city works 21 tiles; shared tiles are halved.
- Gold is an amount in halves everywhere (`HalfInt`): a citizen's tax is half a gold per tax rate step, the miners' 3/2 and the trade goods' half gold per production point are kept, building and unit upkeep (with the AI difficulty percent) are exact, and the treasury holds halves; the screens show `12½`. Spare food is sold for gold at two for one after units eat, from the empire's total; units that cannot be fed or paid are disbanded. What a unit eats is the `upkeep.food` of its row in `units.ron`, **as an amount** (1 is one food, 0.5 half a food, 1.5 one and a half; every normal unit has 1, heroes and summoned creatures 0, as in the original). An army's halves are added first and the total is rounded up, so three units of half a food eat 2 food; the undead, the unpaid and the free half of a basic troop under `half_upkeep` eat nothing.
- Production (a city's yield, the progress on its item and the buy cost) and mana (a wizard's power base, the mana share, the stock and the upkeep) are `HalfInt`s too: a mine's mana and a worker's half production are kept, and a half point of mana left in the stock stays. The power base splits into the mana share (in halves), the skill share (whole points; skill and mana trade one for one when a spell is paid) and the rest to research (rounded down); a spell's upkeep percent (Channeler 50) gives halves; the fortress prize takes the exact half of the loser's mana. The AI decides on the whole part (`floor`).
- Movement is a `HalfInt` of moves: a unit's moves left, a step's cost (grass 1, forest 2, a road 0.5, a diagonal step 1.5 under `diagonal_extra_cost`), Wind Mastery's x1.5 and the battle's moves and square costs. Items give whole `Moves`. Saves keep the half-move count they always had. Sight is a `HalfInt` of squares: a unit sees 1.5 (the tile and the eight round it), 2.5 for a flier or a Wind Walker, a city 2.5 (3.5 with walls, 4.5 with an Oracle or Nature's Eye); `Sight` in `units.ron` is the amount.
- **A finished item ends the production.** When a building or a unit is finished the city has nothing in hand (its production reads "none") and nothing queues the same item again: whoever wants another one chooses it. A computer wizard checks its cities for an empty production at the start of its turn and sets the next goal from its own logic (`ai-design.md`; a city with a standing order is not looked at). A human player gets a "finished" message and the city screen at the start of the turn, and chooses there (task 575).
  - Trade Goods and Housing never finish, so they do not revert to none. Housing ends, and reads none, when the city has reached the population it can sustain (its figures are at its maximum size).
  - The auto-builder (`auto-builder.md`, the human's helper, never the computer wizards') is the exception: a city with a plan gets the plan's next item at once.
  - Not built yet: a queue of the next items (a later task); until then a city has one item in hand.
- Buildings a newer building replaces keep their effects and requirement value but cost no upkeep.
- **Which races can build a building is its `races` list in `buildings.ron`, complete.** A race not in the list cannot build it; an empty list is a building no race builds (one that only a spell or an event gives), not one every race builds. The list already accounts for what the building `requires`: a race that cannot build the armorers' guild is not in the war college's list. `requires` says only what must stand first, and the loader refuses a list with a race that cannot build one of the buildings it requires. A mod that changes who can build one building lists the races of the buildings that need it too. A unit is tied to its race by `UnitDef.race` and the race's unit list and to the building it needs, which the race must be able to build (also checked by the loader).
- A unit with any movement left may always take a step.

Saving
- Snapshots, not command logs: a save restores the state directly. Content is referenced by the definition's number (`num` in the content file; terrains and map features too), so saves survive content being added or reordered and fail clearly when content is missing. Units and cities carry a permanent creation number used for ordering and saved, so a loaded game continues exactly like the original.

Client and tooling
- macroquad for the client; art and data stay local; screenshots go in `assets-local/shots`.
## 12. Open questions
- Multiplayer: in scope for v1, or only keep the command log compatible? The scope question is task 811.
- Final art sourcing: commissioned, generated, or CC0 packs? (Placeholders come from the author's own copy of the original.)
- Stream of Life and Gaia's Blessing: confirm against the original's assembly (see the conflicts document).

The plan of the work is `TASKS.md` (open tasks) (long-term ideas are tasks of low priority there); the self-learning AI is `learning-ai.md`.
