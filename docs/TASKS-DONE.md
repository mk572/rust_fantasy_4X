# Finished tasks

Moved here from [`TASKS.md`](TASKS.md) by `scripts/archive_done_tasks.py`. Numbers are the
original task numbers. Newest work is merged in ascending order by number.

## Table

| # | Area | Summary |
|---|---|---|
| 1 | Map | ~~Rivers~~ (done) |
| 2 | Map | ~~Lakes~~ (done) |
| 3 | Map | ~~Volcanoes~~ (done) |
| 4 | Map | ~~Shore and coastline variants for drawing~~ (done) |
| 5 | Map | Land corruption and purification (done) |
| 6 | Map | ~~Magic node ownership, melding and power~~ (done) |
| 7 | Map | Node auras and warped nodes (done) |
| 11 | Map | ~~Roads built by engineers, and road enchantment~~ (done) |
| 14 | Map | ~~Sight radius sources (Oracle, walls, Nature's Eye)~~ (done) |
| 15 | City | ~~Fortress and summoning circle, banishment~~ (done) |
| 19 | City | ~~Forester's Guild raises the growth cap (B14)~~ (done) |
| 23 | City | ~~Animist's Guild food per farmer~~ (done) |
| 26 | Units | ~~Unit abilities that need code~~ (done) |
| 28 | Units | ~~Transport: ships carrying units~~ (done) |
| 33 | Combat | ~~Battle data model in the ECS~~ (done) |
| 34 | Combat | ~~Attack resolution: to-hit, damage, defence, attack order~~ (done) |
| 40 | Combat | ~~Tactical battle map, walls and turn flow~~ (done) |
| 52 | Magic | ~~Timed spell durations~~ (done) |
| 54 | Magic | ~~Wizard creation screen and named wizards~~ (done) |
| 57 | Magic | ~~Global enchantments and wizard-level state~~ (done) |
| 70 | Meta | ~~Victory conditions and score~~ (done) |
| 90 | Meta | ~~Undoing orders~~ (done) |
| 105 | Combat | ~~Wire overland attacks to battles~~ (done) |
| 113 | Client | ~~Original bitmap fonts~~ (done) |
| 114 | Client | ~~Cityscape: port the original's procedural city layout~~ (done) |

## Descriptions

**1. Rivers (done).** A river flag per tile in `Grid` (saved as `PlaneSave::rivers`), generated in
`mapgen::place_rivers` (from high ground downhill to the sea or another river, never on hills,
mountains or ocean). Effect as in the original (`Square_Gold_Bonus`): a city on a river gets +20
percent trade, on a river next to the sea (river mouth) +30, shore alone +10; capped with roads
and race at three times the citizens. The client draws blue strokes. The original also lets
rivers change movement art only, so no movement rule. Lakes are task 2.

**2. Lakes (done).** In the original a lake is one standalone water square (`_1Lake`) that is not part
of any sea, with outlet variants where a river leaves it. Here a lake is an ocean-class square
with a lake flag (`Grid::is_lake`, saved as `PlaneSave::lakes`), so ships can sail it, cities
beside it get the shore trade bonus (+10 percent, tested), and nothing can be built on it.
`mapgen` makes them two ways: a river that reaches a basin (nowhere lower to flow) ends in a
lake, and a few standalone lakes (about one per 450 land squares) go on quiet ground away from
rivers. Both keep clear of the sea (tested over 30 seeds). Not done: the original's drawing of
river outlets on lake squares, and checking whether a lake feeds the farmers of a city (ocean
food rules are the sample data's).

**3. Volcanoes (done).** `terraform.rs`: a volcano is a mountain tile with an owner (`Grid`
volcano layer, saved). Raise Volcano (hook `raise_volcano`, map square target) turns a non-hill,
non-mountain, non-site land tile into a volcano, destroys its minerals, gives the caster +1 power
per volcano (`power_base`), and collapses each building of a city on it with 15 percent. Change
Terrain (hook `change_terrain`) follows the help table: desert, forest, hills, swamp to grassland,
grassland to forest, mountains to hills, volcano to plain mountains. Not done: Armageddon's
volcano rain (needs timed global spells, 57) and Gaia's Blessing turning volcanoes into hills (24).

**4. Shore and coastline variants (done).** `f4x-tools extract-art` now also writes
`assets-local/art/terrain/autotile.tsv` from TERRTYPE.LBX (five 256-entry tables keyed by the
8-neighbour mask); the client picks the ocean tile for each water square from the land
neighbours. Only table 0 (ocean shores) is used; the tables for mountains, hills and tundra
edges are extracted but not yet drawn.

**6. Magic node ownership, melding and power (done).** `nodes.rs`: every node feature gets a `Node`
(on `Map`, saved in `MapSave::nodes`) with power (5-10 on Arcanus, 10-20 on Myrror: the number
of aura squares), an aura of squares around it, an owner, a guardian flag and a warped flag.
`Command::Meld` spends a unit with the `meld` ability on the node it stands on (a Guardian
Spirit preferred, leaving the node guarded); rivals' guarded nodes fall one time in four;
warped nodes cannot be melded. Owned nodes add `power * (strength + 1) / 2` to `power_base`
(strength 0 / 1 / 2 is `Game::magic_strength`, saved), doubled by the realm's mastery and again
by Node Mastery; a warped node costs five. The client tints aura squares by realm, outlines
owned nodes in the owner's colour and melds with U. Not done: node guardians (monsters) to
beat before melding (8), conjunction events (68), the warp spell (7), a setting screen for the
strength (10, 89).

**11. Roads and engineers (done).** `roads.rs`: units with the `Construction` stat (engineers: 1,
2 for the dwarven ones; the importer now reads it) take `Command::BuildRoad` with a path; a
square takes 3 turns on grassland, 4 desert, 6 forest, hills and tundra, 8 mountains and swamp,
4 or 5 on a node, divided by the stack's total construction strength (a minimum of one turn;
Endurance adds one per unit), then the stack steps to the next square of the path. The work is a
`RoadWork` component, saved, and cancelled when the stack is moved by hand. Enchant Road (hook
`enchant_road`, a map square) enchants every road within two squares; enchanted roads and all
roads on Myrror cost walking units no movement. The client's B key builds a road under
engineers (one square; path drawing is not done) and draws enchanted roads in gold. The trade
bonus now has roads to work with.

**14. Sight radius sources (done).** `Stat::CitySight` (base 2) with `Min` modifiers: an Oracle
makes a city see 4 squares, City Walls 3, Nature's Eye 4; the best one counts, they do not add
up. `Game::city_sight` feeds `visibility`, and each turn cities explore what they see. Units keep
their `Sight` stat (the importer reads it; scouting levels are not separate).

**15. Fortress and summoning circle (done).** Implemented in `fortress.rs`; see `magic.md`
section 12. What is left is split off as task 82.

**19. Forester's Guild raises the growth cap (done).** The city's maximum size already counts the
guild's +2 food (`pre_cap_adds` in `economy.rs`), as decided for B14; a test on barren land
(`the_foresters_guilds_food_raises_the_size_limit_too`) now pins it.

**23. Animist's Guild food per farmer (done).** The importer gives the guild `FarmerFood +1`, which
the economy adds to each farmer's harvest (not a flat bonus per city); the test
`the_animists_guild_adds_food_for_every_farmer` pins it. The stale note in
`help-vs-implementation-conflicts.md` section D was removed.

**26. Unit abilities that need code (done).** All 66 imported abilities were sorted by where
they act. *Overland, now implemented:* flying, wind walking, planar travel, non-corporeal,
pathfinding, forester and mountaineer (these three now share across the whole stack through
`StackTraits`, as the help says), healer (20 percent of total hit points a turn for the
stack), regeneration (full healing every turn), invisibility (`unit_visible_to`: hidden from
other wizards unless one of their illusion-immune units is within its sight; the client hides
such enemy units), meld, create outpost, and construction (roads, task 11). *In combat, so
listed under 35-37:* the immunities, armour piercing, first strike and negate first strike,
large shield, long range, lucky, holy bonus, illusion, cause fear, wall crusher, cavalry,
teleport, merging, gazes and bolts, poison, stoning and life steal touches, immolation,
spellcasters and the casting units. *Waiting for another system:* create undead and summon
demon (after-battle results, 42), purify and land corruption (5), undead rules (29).

**28. Transport (done).** `transport.rs`: ships have a `Transport` stat (Trireme 2, Galley 5,
Warship 3, Floating Island 8; imported). Land units cannot enter the sea alone: they board by
stepping onto a sea square where a friendly ship has room, or by sailing out with a ship they
share a square or port with (the `sea_lift` stack trait); passengers (`Aboard` component, saved
by creation number) ride free and follow their ship, ship capacity is enforced (`NoRoom`),
stepping ashore is an ordinary move that unloads, and land units left at sea without a ship
drown at the start of the next turn. Not done: ship tiers by building (which buildings allow
which ship is already in the unit data's requirements), and path turn estimates that mix ship
and passenger speeds.

**33. Battle data model in the ECS (done).** A battle is a separate `Battle` record, not part of the
`hecs` world: units are copied into `BattleUnit`s with their modifiers applied, and results are
written back when it ends (`battle/game.rs`, `Command::BeginBattle`, `Command::Battle`). See
`combat.md` section 1. Open: saving a battle in progress, the AI-facing views of a battle.

**34. Attack resolution (done).** `battle/attack.rs`: dice (30 percent, +10 per plus, a 10 always
hits), defence blocks, damage to the front figure with overflow and fresh defence rolls,
the melee exchange order (thrown or breath, first strike, the reply, the ordinary blows),
ranged attacks with distance penalties and ammunition. Seeded and tested for averages,
overflow, ordering and determinism. Immunities, special attacks and the ability rules are
tasks 35-37.

**40. Tactical battle map, walls and turn flow (done).** `battle/map.rs` and
`battle/mod.rs`: the 21 by 22 grid with open, rough and blocked squares generated from the
terrain, eight-direction movement with costs and obstacles, deployment on opposite edges, side
turns with per-unit moves and one attack each, win by elimination, flee, a 100-turn limit.
Walls and wall sections are not built yet: they belong to the siege task (41); the battle screen is 45.

**45. Battle screen in the client (done).** `battle_screen.rs`: the combat grid in the original's
isometric projection (32 by 16 cells, step 16 by 8) scaled to the window, ground from the
original's `cmbgrass`/`cmbdesrt`/`cmbmount`/`cmbtundr` sets (and the Myrror `c` variants) chosen by the
overland terrain, trees and rocks on blocked squares, units as their overland icon on a banner
base with health bar and figure count, reach and target markers, a side panel with the
selected or hovered unit's statistics, and orders by click (select, move, melee, shoot) and keys
(Space done, Tab next, Enter end turn) or buttons (Flee asks twice). A move into an enemy stack
or a guarded site starts the battle; the other side is played by a plain placeholder AI
(`battle/ai.rs`, walk at the nearest enemy and hit it) until task 46. Results go back through the core
(wounds, deaths, site loot logged). `--battle` with `--shot` opens a test fight. Remaining
work is split into tasks 131-133 and 112 (map art), 43/44 (casting).

**52. Timed spell durations (done).** `timers.rs`: a spell with `Duration::Turns(n)` gets a timer
(`Game::timers`, saved by creation number) when attached to a unit, a city or the wizard; each
turn's magic step counts it down and removes the enchantment with `Event::EnchantmentExpired`.
Timers whose enchantment was dispelled or whose bearer died are dropped silently. No imported
spell uses `Turns` (the original has none), so this is for mods and the community patch.

**54. Wizard creation screen and named wizards (done).** `wizard.rs` holds the pick rules of the
original's custom-wizard screen (eleven picks; books one each; retorts one, two for Warlord,
Channeler, Divine Power, Famous and Infernal Power, three for Myrran; at most six retorts;
prerequisites such as four books for a mastery, Archmage or Mana Focusing, two realms for Sage
Master, two books in three realms for Runemaster) and `Game::create_wizard`, which also takes
the starting spells: a realm with `n` books lets the wizard choose `n - 1` common spells, filled
from the realm's default list (`RealmDef::default_spells`). The 14 named wizards are data
(`wizards.ron`, written by the importer; Lo Pan is over budget in the original and is exempt).
The client's creation screen (`create_screen.rs`, shown at start unless `--quick`, `--load` or
`--shot`; `--create` forces it) offers the presets, a custom build, a name, starting spells and
a race. The rival gets another preset by seed. Not done: portraits and banner colours (83), the
Myrran start on Myrror and the race it controls (53), and eleven-book realms.

**57. Global enchantments and wizard-level state (done).** `globals.rs` reads the wizard's
global spells where they matter: Evil Omens (life and nature spells cost half as much again for
every wizard but the holder; one cost function, B11 decided as "follow the help"), Herb Mastery
(heals every unit each turn), Planar Seal (blocks every plane shift, towers and planar travel
included), Awareness (explores all city squares), Nature's Awareness (explores and shows the
whole map), Detect Magic (`detect_magic_view`, shown on the magic screen), and the stat
bonuses Holy Arms (+1 melee and ranged for normal units), Chaos Surge (+2 for chaos creatures)
and Charm of Life (a quarter of the hit points, at least one). Unit rules can now carry
conditions on a unit's kind (`Condition::KindIs`). The data marks these spells with the
`global_effect` hook so that casting does not report "no effect yet".

**70. Victory conditions and score (done).** The game ends by conquest, by the human wizard's
defeat or by the Spell of Mastery; the game-over screen shows the score. See `docs/game_end.md`;
the cut-scenes and the hall of fame are tasks 134 and 88.

**90. Undoing orders (done).** `undo.rs`: with `Game::set_undo(true)` (the client switches it on) every
command saves a snapshot first and `Game::undo` goes back one command, up to 32 deep. Fairness
rules: a command that explored new squares, drew random numbers or started a battle cannot be
undone, and clears the history before it; ending the turn clears it too, and refused commands
leave no entry. An undo rebuilds the units, so `hecs` entities change: find them again by
creation number (`entity_of_serial`, now public); the client does this for its selection and
binds undo to Ctrl or Cmd with Z. Hardcore mode (`PLANNED.md` item 8) will switch undo off.

**105. Wire overland attacks to battles (done).** Done: the winner of an overland battle moves onto the square, takes a city on it (`capture_city`) and an empty enemy city is taken by walking in (`Game::hostile_entry`); the human's attack opens the battle screen (task 45) and AIs use `Command::AutoResolve` (39). Experience and loot are task 42, walls and razing task 41. Original text: Moving into a hostile square only emits `AttackRequested`. Connect it: the client opens a battle (`Command::BeginBattle`) for the human, the AI does so for itself, and the winner takes the square (attackers advance when the defenders are gone), with the defenders' city handled by the siege task (41). Needs the after-battle results of 42 for experience and loot.

**113. Original bitmap fonts (done).** `f4x-tools extract-art` now decodes `FONTS.LBX` (`fonts.rs`):
eight styles of 96 glyphs, stored by columns with skip and run bytes, drawn through 16 colour
sets; it writes `assets-local/art/fonts/style{N}.png` (the glyph strip once per colour set) and
`metrics.tsv`. The client (`art.rs`: `text_width`, `draw_bitmap_text`, `nearest_font_set`;
`frame.rs`: `App::bm_text`) draws text with them and falls back to the plain font without
them. Style 5 is the blackletter display face (titles), 3 the chunky label face, 1 and 2 the
small text, 4 a medium face, 7 a runic one. Used by the city screen and the map frame.
Still to do elsewhere: the other screens (magic, creation) and a text-wrapping helper for the
bitmap fonts.

**114. Cityscape: the original's procedural city layout (done).** `cityscape.rs` ports
`Cityscape_Make_Buildings_Array`: a 15 by 23 grid with reserved roads and corners, buildings
placed by footprint (shapes now imported into `BuildingDef::shape`, house types into
`RaceDef::house_type`), the fortress, port and magic buildings in fixed cells, houses, trees and
rocks by terrain, seeded like the original (`0x2895 + x * y`) so a city looks the same every
visit. `city_screen.rs` draws it with the `cityscap.lbx` sprites in the original's sheared
cell positions: ground by plane and enchantment, river or ocean overlay, hills or mountains,
the road grid, every cell back to front, the walls; hovering shows a building's name. Not done:
the original's road masking (the whole grid is drawn), animation of buildings and water,
the ward and enchantment icons along the bottom, rubble, and selling by clicking.
