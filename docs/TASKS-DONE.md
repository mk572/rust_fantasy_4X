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
| 8 | Map | Encounter sites: guardians and loot (done; the open parts are tasks 127 to 130) |
| 9 | Map | City founding blocked only by intact encounter sites (B17) (done) |
| 11 | Map | ~~Roads built by engineers, and road enchantment~~ (done) |
| 12 | Map | Gates and the Planar Seal (done) |
| 13 | Map | Teleporting and merging movement (done) |
| 14 | Map | ~~Sight radius sources (Oracle, walls, Nature's Eye)~~ (done) |
| 15 | City | ~~Fortress and summoning circle, banishment~~ (done) |
| 16 | City | Autobuild (done) |
| 19 | City | ~~Forester's Guild raises the growth cap (B14)~~ (done) |
| 20 | City | Population growth edge rules (B19) | (done)
| 21 | City | Outpost growth: wild game and difficulty (B20) | (done)
| 22 | City | Nightshade dispel strength | (done)
| 23 | City | ~~Animist's Guild food per farmer~~ (done) |
| 26 | Units | ~~Unit abilities that need code~~ (done) |
| 28 | Units | ~~Transport: ships carrying units~~ (done) |
| 30 | Units | Temporary effects store (done) |
| 31 | Units | Neutral player and neutral cities (done) |
| 33 | Combat | ~~Battle data model in the ECS~~ (done) |
| 34 | Combat | ~~Attack resolution: to-hit, damage, defence, attack order~~ (done) |
| 40 | Combat | ~~Tactical battle map, walls and turn flow~~ (done) |
| 52 | Magic | ~~Timed spell durations~~ (done) |
| 54 | Magic | ~~Wizard creation screen and named wizards~~ (done) |
| 57 | Magic | ~~Global enchantments and wizard-level state~~ (done) |
| 59 | Magic | Dispel mathematics and the counter-magic family (done) |
| 70 | Meta | ~~Victory conditions and score~~ (done) |
| 86 | Client | Start menu: new game, load game, settings, quit (done) |
| 90 | Meta | ~~Undoing orders~~ (done) |
| 105 | Combat | ~~Wire overland attacks to battles~~ (done) |
| 113 | Client | ~~Original bitmap fonts~~ (done) |
| 114 | Client | ~~Cityscape: port the original's procedural city layout~~ (done) |
| 145 | Client | Save and load screen with slots (done) |

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

**8. Encounter sites: guardians and loot (done).** Core done (`lairs.rs`, `docs/lairs.md`): guards and hoards
for towers, nodes and encounter sites, assault by `Command::EnterSite`, treasure and clearing. Open
parts are split into tasks 127 to 130; the client has no confirm box yet and assaults a site at once (task 128).

**9. City founding blocked only by intact encounter sites (done).** A cleared encounter site now removes
its feature; the founding check (`city::check_site`) now follows the original's
`Map_Square_Survey`: a tower (on either plane) or a node always blocks, an encounter site only
while its lair record is intact. Tests in `city.rs`.

**10. New-game settings (done; the generator port is task 150).** A new `Options` screen (`options_screen.rs`, the original's `Newgame_Screen_0`) comes after New Game in the start menu and before the wizard: difficulty (Intro, Easy, Average, Hard, Impossible), opponents (1 to 4), land size (Small 15%, Medium 20%, Large 30% of each plane, the figures of ReMoM's `NEWG_CreateLands`, not the 15/20/25 the old task text said) and magic power (Weak, Normal, Powerful); Cancel returns to the menu, OK goes on. `setup::NewGame` takes `land_percent`, `difficulty`, `magic_strength` and a finished `build` for the human; the game keeps `difficulty` beside `magic_strength` and both are saved (older saves read as average). The client now starts games through `setup::new_game`, so every wizard gets a fortress and a starting force, spread round the map, and the camera opens on the human's fortress (before, only the human had a city and the rival none). The console's `new` takes `land=`, `difficulty=` and `magic=`. Open: the map seed is still the command-line seed; difficulty only affects lair guards and treasure so far, not the AI or the final score (`endgame::scale_for_difficulty` is not called yet).

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

**12. Gates and the Planar Seal (done).** `gates.rs`: Earth Gate teleports a stack between two of
its wizard's gate cities on the same plane for one move (`Command::EarthGate`, console `gate`);
Astral Gate lets a stack in the city change plane (`shift`), blocked when the other plane has a city
on the square or stationed enemies; the Planar Seal (`globals.rs`) and towers were already in. A gate
works only for the wizard who cast it. The client and AI do not use Earth Gates yet (task 136).

**13. Teleporting and merging movement (done).** In the original both are battle-only (`M_Teleport`,
`M_Merging` in `Move_Battle_Unit`; nothing reads them on the overland map). `battle/mod.rs`: a unit with
the `teleport` ability (unicorns, djinn) or `merging` (great wyrm) may move to any free, unblocked square
of the battle map for one move (two half-moves, or what is left), with no path walked. Tests in
`battle/tests.rs`. Open: an animation for it (task 137), and the merging power of magic items (task 49).

**14. Sight radius sources (done).** `Stat::CitySight` (base 2) with `Min` modifiers: an Oracle
makes a city see 4 squares, City Walls 3, Nature's Eye 4; the best one counts, they do not add
up. `Game::city_sight` feeds `visibility`, and each turn cities explore what they see. Units keep
their `Sight` stat (the importer reads it; scouting levels are not separate).

**15. Fortress and summoning circle (done).** Implemented in `fortress.rs`; see `magic.md`
section 12. What is left is split off as task 82.

**16. Autobuild (done).** The original's Grand Vizier is a per-player switch (`grand_vizier` in its save): when a city finishes something, the game chooses what it builds next. Here: `Player::grand_vizier` (saved), `Command::SetGrandVizier`, and `Game::vizier_choice`: the first building in `rules.config.autobuild_order` (data: `content/rules.ron`, written by the importer) that the city can build and whose gold upkeep the treasury's net income can bear; outposts are left alone. When a building completes the city takes that choice instead of housing; turning the Vizier on gives cities that are idle or on housing a building at once. Units and trade goods are never chosen. The list is ours: the original's rules for the Vizier are not documented in the reference material (only its call sites), so how it chooses differs. Client: F8 toggles it (a log line; the original's yes/no window is not drawn). Console: `vizier on|off`. A smarter chooser is item 7 in PLANNED.md.

**19. Forester's Guild raises the growth cap (done).** The city's maximum size already counts the
guild's +2 food (`pre_cap_adds` in `economy.rs`), as decided for B14; a test on barren land
(`the_foresters_guilds_food_raises_the_size_limit_too`) now pins it.

**20. Population growth edge rules (B19) (done).** The original (`City_Growth_Rate`) returns zero growth early for a city at its maximum size and for one at the largest city size, which also stopped such cities shrinking from starvation or from being over their limit (drake178's bugs 1 to 3). `economy::growth_rate` now only stops *growth* there: a starving city at its maximum shrinks, and a city over its limit (or at the 25 cap with a smaller limit) shrinks. Growth also never goes positive for a city over its limit. The original's discarding of surplus tens when a figure is gained stays replaced by carrying them over (design.md section 11). Neutral cities' half growth is not ported (design.md section 8). Tests in `economy.rs`.

**21. Outpost growth: wild game and difficulty (B20) (done).** `Game::outpost_growth_chance` (split out of `grow_outpost`): the race's base, the food the land supports, +5 or +10 for each mineral or special in reach, Gaia's Blessing +20 and Stream of Life +10. Wild game counts both in the food (it adds 2) and with its own +10 (the original ignored its food here); no difficulty modifier applies to anyone (design.md section 8; the original applied it to the human too). Not changed: the food term is the city's maximum size, capped at 25, where the original uses the uncapped terrain food. Tests in `turn.rs`.

**22. Nightshade dispel strength (done).** New `nightshade.rs`. A city with a shrine, animist's guild, temple, parthenon, cathedral, sage's guild, oracle, alchemists' guild or wizard's guild counts the nightshade plants in its area; each adds 100 dispel strength. When another wizard casts a spell aimed at the city (a bad city enchantment), it fizzles with chance `strength / (strength + the spell's dispel difficulty)`; the mana is spent and `Event::SpellFizzled` says why. The original's code lists eight buildings and checks the Shrine twice, where the Animist's Guild named by its help was evidently meant, so the animist's guild is included (nine in all). The imported help is extended for the other buildings (topics 731, 732, 734, 737, 738) and 766. The original's use of the plants in `CTY_NightshadeDispel` (removing the city's own bad enchantments each turn) is an empty stub there and is not modelled. The general dispel code (task 59) is unchanged.

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

**59. Dispel mathematics and the counter-magic family (done).** New `dispel.rs`. One formula: a dispel of strength `s` beats a spell of difficulty `d` with chance `s / (s + d)` (the original's roll of 1 to 250 at or under `250 s / (s + d)`, so even a huge strength fails one time in 250). A spell's difficulty is its cost times one plus one for its caster's mastery of its realm (nature, sorcery or chaos) and one for Archmage. Our spells have no extra mana, so a spell's dispel strength is its cost; the True spells count it three times. Built on it: Dispel Magic and Dispel Magic True (units; only enchantments cast by *other* wizards come off, and a unit under another wizard's Spell Lock can only have the lock attacked, at difficulty 150), Disenchant Area and True (new `Tile` target: the city, every unit and a warped node on the square, a warped node against Warp Node's cost), Disjunction, Disjunction True and Spell Binding (new `GlobalEnchantment` target, `Target::Global(owner, spell)`; Binding moves the spell to the caster, who pays its upkeep from then on), Spell Blast (new `EnemyWizard` target: the caster pays as much mana as the victim has put into the spell being cast, and that spell is lost), Suppress Magic (a standing strength-500 dispel against every overland spell another wizard casts) and nightshade (task 22, now with the caster's masteries). Counter Magic is state and a method on `Battle` (`cast_counter_magic`, `counter_magic_stops`: the reserve opposes the other side's spells and loses five per attempt) but nothing casts in a battle yet, so wiring it in is part of task 43. The importer sets the targets and strengths (a fix: Dispel Magic True was strength 10, not 30). Console: `global:<player>:<spell>` targets. Client: Spell Blast is aimed by clicking one of the rival's cities or units, Disenchant by clicking the square; the global-enchantment targets need a picker (task 151). The original's other code in `Cast_Disenchant` (`+ 500` in the roll and an enchantment test against the caster) looks like a disassembly artefact and was not followed; the help text's rules were. Open: dispel in battle (Dispel Magic on units in a fight and Disenchant's battle version, which also unwarps a node the caster owns) waits for 43 and 44.

**70. Victory conditions and score (done).** The game ends by conquest, by the human wizard's
defeat or by the Spell of Mastery; the game-over screen shows the score. See `docs/game_end.md`;
the cut-scenes and the hall of fame are tasks 134 and 88.

**86. Start menu (done).** `menu_screen.rs` shows the original's menu (its title picture and entries): Continue (loads the auto-save), Load Game (opens the slots of task 145), New Game (the wizard creation screen, which has a Back button), Hall of Fame (greyed until task 88) and Quit; hotkeys C, L, N, H, Q. It is the first screen unless `--quick`, `--load`, `--create` or `--shot` skip it (`--menu` and `--slots` show these screens in a screenshot). Esc on the map opens the game menu (the slots) instead of quitting. Open: Settings (89, 94), the opening movie (85), the map seed on the new-game options step (148); land size and opponents are there now (10).

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

**145. Save and load screen with slots (done).** `menu_screen.rs`: the original's Load / Save screen on its own art: eight slots (`saves/slot1.ron` to `slot8.ron`), each showing the wizard, the turn and the save time (UTC), with Quit (saves the auto-save and goes to the start menu), Load, Save (asks for a second click before replacing a filled slot), Settings (greyed, task 94) and OK. Reached by the Game button or Esc on the map, and from the start menu's Load Game. Not as in the original: a slot's name is not typed. F5 and F9 still quick-save and quick-load.

**30. Temporary effects store (done).** New `temp_effects.rs`: a `TempEffects` component on a unit or city holds stat modifiers, granted abilities and removed abilities, each with turns left (`None` lasts for good), counted down once per turn change in the magic step (`tick_temp_effects`). `unit_modifiers` and `city_modifiers` read the modifiers; `Game::unit_abilities` gives a unit's abilities now (own, plus granted, minus removed, in the order added, so a later grant undoes an earlier removal) and `unit_has_ability`, the abilities' own passive modifiers, the spell facts, global-spell facts, battle units, movement checks, ship need and noble upkeep all use it. A city only takes modifiers. `GameHost::add_modifier`, `grant_ability` and `remove_ability` now store (without an end, since a rule's effect carries no duration yet; `add_temp_modifier` and the other two take turns for callers that have one). Saved with units and cities (`effects`, older saves read as none). Tests in `temp_effects.rs`. Not done: no screen lists these effects, and an effect has no source for dispelling (they are not enchantments).

**31. Neutral player and neutral cities (done).** A new world now starts with independent cities, as the original's `Generate_Neutral_Cities` makes them: `neutral_cities.rs`, called from `setup::new_game` after the fortresses are down, up to 15 per plane (Arcanus and Myrror) on land clear of fortresses (range 5), other cities (the ordinary minimum) and nodes, towers and lairs (range 4, the original's `Range`: the longer side plus half the shorter, no wrap). Like the original, it gives up after 1000 placement attempts counting every ocean square it rolls, so a plane gets fewer than 15 when the sites leave no room (usually 4 to 13). Race: from the races of the plane (`RaceDef::myrror`, set by the importer for beastmen, dark elves, draconians, dwarves and trolls), high men in two of the slots, the second becoming high elves on forest, and three times in four the default race of the landmass (a flood fill of connected land). Population `1 + (difficulty + 1) / 3 + d4`, with a one-in-five roll of `(difficulty + 1) / 3 + d10` above normal difficulty, as the lair guards scale with difficulty (design.md section 8). Buildings by population (barracks, smithy, builders hall, armory, granary, stables, city walls, fighters guild, armorers guild, shrine at ever greater sizes), dropped where the race cannot build them or a requirement is missing. Garrison: up to a quarter of the population in the race's last melee soldier it has the buildings for, and the same again of its last ranged one (or melee again where it has none), as many as the city can feed and pay for (see below). **Every neutral city is a player of its own** (`Player::neutral`, numbers from 100, brown banner, Grand Vizier on, 100 starting gold) with its own treasury: its taxes pay its buildings and garrison like a wizard's. It is no wizard: no magic or victory (`is_wizard`, `living_wizards`), no fog, no AI, and the console and client lists leave it out (`Game::is_neutral_player`). As nobody runs it, each turn it puts enough citizens on the farms to feed its garrison (`balance_farmers`) and sets the lowest tax rate that leaves its treasury no worse off (`balance_taxes`); one that still cannot pay disbands units newest first, by the ordinary rules. Because the original gives neutral garrisons free upkeep and ours pay, the starting garrison is cut to what the city can feed and pay for, so neutral cities are somewhat weaker than the original's. The city then builds by the Vizier's order, grows and can be taken: `capture_city` only changes the owner, removes the city's player and any garrison left, and defeats nobody. Monsters and lair guards stay with the shared `units::NEUTRAL`. No half production, no population cap and no other neutral penalty (design.md section 8). Tests in `f4x-console` (placement, planes, seed, a player each, 60 turns without disbanding, an unpayable city, capture, save round trip). Saves carry `neutral` in each player. Not done: the client cannot open a neutral city's screen (task 141), raiders (task 154), and the cities of a defeated wizard turning neutral (task 82; they would need a player each too).

**129. Lair generation parity (done).** A world now has the original's 99 lairs instead of about 190: 6 towers on each plane (12 lairs, shared coordinates), one lair at each of the 30 nodes, and 25 strong plus 32 weak random sites (`featuregen::place_random_sites`; `GenParams::strong_sites` and `weak_sites` replace the per-plane `encounter_ratio` of one site per 10 land squares). Each random site rolls its plane, then a square from 2 inside the top and bottom edges, skipping ocean and anything within one square of a tower, node or other site on either plane; the kind comes from the terrain's feature weights. Exactly 25 of them, picked at random, get the strong budget (Arcanus `50 + 50 * d29`, Myrror `100 + 100 * d24`), the other 32 the weak one (`10 * d10`, `10 * d20`). Not copied: the original's three trailing empty slots, and its random square range (2 to 55 by 2 to 35 of 60 by 40) is scaled to the map's size. Test in `f4x-console`.

**32. Starting economy (done).** A new game's start now follows the original's `Generate_Home_Cities`, `Init_Players` and `Init_Runtime`, except gold (100 for everyone by decision). Each wizard's capital has 4 citizens with 3 on the farms and a smithy, barracks and builders hall; the army is one Spearmen and one Swordsmen of the race (two Swordsmen for dwarves, who have no spearmen) and no Settlers; the tax rate starts at index 2 (a gold a citizen, `player::START_TAX_RATE`) where it was 1. That balances exactly: 3 farmers feed 4 citizens and 2 units, and 4 gold of taxes pay 2 for buildings and 2 for units. A Myrran wizard starts on Myrror with one of the five Myrror races (`setup` refuses an Arcanus race for them, and a Myrror race for anyone else), the rest on Arcanus. Sites must be land clear of other fortresses (16 apart, relaxing to 10) and of nodes, towers and lairs (3, relaxing to 1), and able to feed 8 citizens (7 from the fourth wizard), by `Game::site_max_size`, the original's `City_Maximum_Size_NewGame` estimate; this matters because farmers' food beyond the land's capacity counts half, so a poor site cannot feed the army. The old start (3 citizens, 2 farmers, three units, tax index 1) disbanded two units on the first turn and lost about 4 gold a turn. Also changed: the turn now pays upkeep **before** cities produce and grow, as the original does (`Players_Apply_Upkeeps` before `Apply_City_Changes`): a city that grew used to disband units the same turn, before the player could move anyone to the farms. Tests in `f4x-console`: the start (6 seeds, 4 wizards), eight turns without disbanding, a Myrran dwarf wizard. What is left is task 155. `neutral_cities.rs`'s unused `grid` variable is gone too.

**35. Immunities and resistance rolls (done).** New `battle/immunity.rs`, following the original's `Battle_Unit_Attack_Immunities`, `Battle_Unit_Defense_Special`, `Combat_Effective_Resistance` and `Combat_Resistance_Check`; the rules and numbers are in `combat.md` section 2.1. Every attack now has a nature (a normal weapon, missile, fire, magic with a realm, or illusion) and every unit its immunities (the nine immunity abilities); `attack::process_attack` rolls the defence the pair gives: 50 against a non-melee attack the unit is immune to, at least 10 with weapon immunity against a normal weapon (the original's order, where this replaces a stop by missile immunity), 50 against ranged magic with magic immunity, and 0 against an illusion unless the unit sees through illusions. Magical weapons (heroes, fantastic and summoned creatures, the new `magical_weapon` ability from Flame Blade and Holy Weapon) ignore weapon immunity. Spells: `Effect::Damage` gained `immune` (read from the spell table's immunity byte, so Ice Bolt is stopped by cold and magic immunity), `realm` and `resist_modifier`; Righteousness protects outright against chaos and death spells; resistance checks use the original's roll (a ten-sided die above the resistance fails) with magic immunity +30, Righteousness +30 and Bless +3 against chaos and death, Elemental Armor +10 or Resist Elements +3 against chaos and nature, and Resist Magic +5 (`Game::unit_resistance_against`; Resist Elements and Resist Magic lost their flat resistance bonus). Lasting spells can grant abilities through a passive `GrantAbility` effect (Magic Immunity, Guardian Wind), read by `Game::unit_abilities`. The importer gives the immunity abilities accurate descriptions and corrects the Weapon Immunity help text; content validation lets a lasting spell have no effects. One ReMoM fix is applied (thrown weapons are stopped by weapon immunity). Tests in `immunity.rs`, `attack.rs`, `battle/tests.rs`, `exec.rs`, `temp_effects.rs` and `f4x-console`. Left to other tasks: defence bonuses of enchantments (156), gazes and touches (36), spell damage by dice (43), weapon quality (18).

**36. Special attacks (done).** `battle/special.rs` and `attack::process_attack` follow the original's `Battle_Unit_Process_Attack` (rules in `combat.md` section 2.2). New stat `Stat::SpecialLevel` (the unit table's special attack number, imported by `scripts/import_mom.py`, kept positive): poison strength, or the penalty of a gaze, touch or life steal. Poison (a resistance check per point of strength per attacking figure, each failure a point no defence blocks), stoning touch, death touch, destruction and dispel evil (a failed check kills a figure; stoning, death and magic immunity, and chaos and death creatures for dispel evil, -9 for the undead), life steal (the failure margin is damage no defence blocks and heals the thief, who can get dead figures back) and the stoning, death and multiple gazes (once against each figure of the target, in the thrown step; a defender with a gaze gazes back before the blows) are in. Chaos damage (`doom_damage`) is the attack strength with no rolls; armour piercing halves the defence before the immunity floor, and lightning breath pierces armour. `ranged_kind_of` counts gazes as short-range attacks. Tests in `special`-level form in `attack.rs`, `immunity.rs`, `battle/tests.rs` and `f4x-console`. Open parts are task 157.

**41. City sieges, walls and capture (done).** Battles at cities are city battles (`combat.md` section 7): the original's four by four city area with a ring of twelve wall sections for a city with City Walls, the gate facing the attackers, the original's deployment (defenders inside, the gate first), the original's movement rules (walkers outside cannot enter a wall except by the gate and nobody the corner towers; units inside leave only by the gate; fliers, teleporters and the non-corporeal pass over), wall crushers striking sections (50 percent in melee, 25 percent shooting, and also when they hit a unit standing on the wall), +3 defence on a wall and +1 elsewhere in the city against attackers outside, and city damage for each attacker inside at the start of a turn. Taking a city (`conquest.rs`): outposts are destroyed; otherwise fame (the loser loses the city's size, the winner gains the size less two), loot (the loser's gold by the city's share of citizens), citizens lost (10 percent plus twice the city damage, at most 50) and buildings destroyed (the city damage plus 10 percent, at most 75), the old owner's spells end, and the taker may raze it until their turn ends (`Command::RazeCity`; the size plus one more fame lost, a tenth of the buildings' cost looted). The battle AI walks round to the gate and sends wall crushers at the wall. The client draws walls, strikes them on a click and asks whether to raze; the console has `bwall`, `bshootwall` and `raze`. The rule that a wizard's capital cannot be destroyed (`destroy_city`) stays for the spells that destroy cities; the original's own raze after a conquest first banishes the wizard and then destroys the city, and that works the same here. Open parts are task 158.

**43. Casting in battle (done).** A wizard casts in battles (`combat.md` section 8): `battle/casting.rs` (the `BattleCaster`: casting skill for the whole battle, the reserve paying the range multiplier of the original's `Combat_Casting_Cost_Multiplier`, one spell a turn, the summon squares, damage and healing on units, lasting spells with a saved base so a dispel can take them off) and `battle/spells.rs` (`Game::battle_spells`, `battle_spell_price`, `battle_cast`, `dispel_battle_unit`). Bolts take extra mana (Fire, Ice and Lightning Bolt one strength for each, Psionic Blast two, Fireball three), a dispel's strength and Counter Magic's reserve are the mana put in; the enemy's Counter Magic reserve tries to stop every spell. Spell damage is dice against the defence (`attack::spell_attack`: bolt, area for Fireball, chain for Warp Lightning), with the spell data's attributes (armour piercing, doom, area, warp lightning) now imported as `DamageTraits` on `Effect::Damage` and Counter Magic mapped to a `counter_magic` hook. Console: `bcast`, `bspells`, wizard lines in `battle`; client: the Spell button, a spell list, target prompt with the mana keys, the wizard's skill and mana on the panel. Left over: task 159.

**44. Battlefield-effect spells (done).** `battle/field.rs` and `combat.md` section 9: the fourteen battlefield enchantments (True Light, Darkness, Warp Reality, Black Prayer, Prayer, High Prayer, Metal Fires, Mass Invisibility, Blur, Terror, Entangle, Mana Leak, Call Lightning, Wrack) with their stat changes kept as a recomputed `FieldBonus` on each unit and their start-of-turn effects in the original's order, and the instant field spells Flame Strike, Mass Healing, Death Spell, Holy Word and Call Chaos. New data: `Effect::Slay` (figures that fail a resistance check are destroyed, or the unit if its resistance is low: Death Spell, Holy Word, Word of Death, Petrify, Dispel Evil, Banish, Disintegrate), `SpellDef::save` (a curse's resistance check: Weakness, Vertigo, Mind Storm), the `battlefield` and `call_chaos` hooks. Invisibility now gives -1 to hit (the ability, the spell and Mass Invisibility), Blur is in the attack roll, Mana Leak drains the wizard's reserve in the game. Help corrections for High Prayer, Entangle and Weakness. Open parts are in task 159.
