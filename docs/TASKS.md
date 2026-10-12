# Open tasks

Everything not yet implemented, collected from the whole project history and the docs
(`mechanics-gaps.md`, `help-vs-implementation-conflicts.md`, `magic.md`, `map-layers.md`,
`design.md`). Keep the table in ascending order by number; new tasks go at their numeric place
(or take the next free number). Bracketed numbers like [430] point at topics in the original help
(`content-local/help.ron`); B/C numbers point at `help-vs-implementation-conflicts.md`.

Combat is under way: 33, 34 and 40 are done (see [`combat.md`](combat.md)).
Gaps found while working on the base game are added here as new tasks, not left in notes.
Finished tasks are moved to [`TASKS-DONE.md`](TASKS-DONE.md) with
`python3 scripts/archive_done_tasks.py` (a task is done when its row or title says "(done)").
Improvements for after the basic game is online are in [`PLANNED.md`](PLANNED.md) (replacement art, formerly task 79, is item 12 there).

## Table

| # | Area | Summary |
|---|---|---|
| 39 | Combat | Auto-resolve (low priority) |
| 46 | Combat | Battle AI |
| 47 | Heroes | Hero abilities (low priority) |
| 48 | Heroes | Hiring, summoning, recall, resurrection and fame |
| 49 | Heroes | Item slots, powers and data schema |
| 50 | Heroes | Enchant Item, Create Artifact, the vault and the anvil |
| 51 | Heroes | Heroes' casting contribution and sage heroes (low priority) |
| 56 | Magic | Implement the 37 "special" spells (low priority) |
| 58 | Magic | Unit enchantments needing non-stat effects (low priority) |
| 60 | Magic | Wizard-targeting, destruction and other special spell categories (low priority) |
| 61 | Magic | The Spell of Mastery's research cost and the AI casting it |
| 62 | Magic | Player-chosen losses when mana runs short |
| 64 | Meta | Wizard personalities and AI objectives |
| 65 | Meta | AI v1: expansion, production, defence |
| 66 | Meta | AI research and spell use |
| 67 | Meta | AI attack |
| 68 | Meta | Random events |
| 69 | Meta | Difficulty levels |
| 71 | Meta | Advisors |
| 72 | Meta | Fame effects |
| 73 | Meta | Save-format migration |
| 74 | Meta | Stable command references for replays and multiplayer |
| 75 | Client | Wizard overview screen |
| 76 | Client | Background music |
| 77 | Content | Content pass and balance with AI-vs-AI soak runs |
| 78 | Content | Open conflict decisions (B9-B13, B15, B16, C3) |
| 80 | Housekeeping | Push to GitHub and tidy the old reference folder |
| 81 | Housekeeping | Packaging for Mac |
| 82 | Meta | Wizard defeat aftermath and fortress display |
| 84 | Client | Analyse the original's screenshots and adjust the in-game graphics |
| 85 | Client | Import and play the original's opening movie |
| 87 | Magic | Remaining global enchantments (low priority) |
| 88 | Meta | Hall of fame |
| 89 | Meta | Game settings |
| 94 | Client | Settings screen |
| 95 | Client | Tutorial |
| 96 | Client | City screen: enchantments list |
| 97 | Client | Sound effects |
| 98 | Housekeeping | Packaging for Linux |
| 99 | Combat | Saving during a battle |
| 100 | Map | Draw the original's edge variants for mountains, hills and tundra |
| 102 | Map | Road-building path selection in the client |
| 103 | Units | Scouting levels and unit sight |
| 104 | Units | Ship tiers by building and ship movement estimates (low priority) |
| 106 | Combat | Battle views for the AI and for tools |
| 107 | Content | Check assumptions made while implementing |
| 108 | Magic | Magic nodes: warp spell, guardians' monsters and node visuals |
| 109 | Housekeeping | Importer clean-up |
| 110 | Combat | Battle map generation parity with the original |
| 111 | Combat | Special battle sites: nodes, lairs, towers, ruins and fortress cities |
| 112 | Client | Battle map art |
| 115 | Client | Cityscape: road masking |
| 116 | Client | Cityscape: animation, wards and rubble |
| 117 | City | Selling buildings from the city picture |
| 118 | Client | Bitmap fonts on every screen and a text layout helper |
| 119 | Client | City screen details |
| 120 | Client | Overland screen details |
| 121 | Map | Unexplored-edge pictures (corner-based exploration) |
| 122 | Map | River mouths and shore pieces with rivers |
| 123 | Map | Call the Void, Great Wasting and Armageddon corruption (low priority) |
| 124 | Map | Mines, lumber camps and hunter's lodges on the map (low priority) |
| 125 | Client | Unit stack backgrounds in banner colours |
| 126 | Client | Bitmap font for the remaining overland text |
| 127 | Map | Lair rewards: original spell, book and retort selection |
| 128 | Client | Site encounter dialogs and treasure popup |
| 130 | Client | Warped node and cleared site pictures (low priority) |
| 131 | Combat | Battle grid shape and deployment as in the original |
| 132 | Client | Battle screen: unit figures, animation and the original's frame |
| 133 | Client | Prompt before attacking, and battles between AI players |
| 134 | Client | Game-end cut-scenes and the original's score screen |
| 135 | Client | Run the AI wizards in the client |
| 136 | Client | Earth Gate and Astral Gate in the client and the AI (low priority) |
| 137 | Client | Teleport and merge animation in the battle screen (low priority) |
| 138 | Client | Army list screen |
| 139 | Client | City list screen |
| 140 | Client | Unit statistics window and the enemy stack list |
| 141 | Client | Outpost screen, enemy city screen and the naming boxes |
| 142 | Client | Item screen and the item pop-up |
| 143 | Client | Offer, hire and event pop-ups |
| 144 | Client | Turn summary and message pop-ups |
| 146 | Client | Palette cycling: the pulsing colour of index 198 |
| 147 | Tools | Find the use of the unreferenced art files `combat`, `desert`, `tundra`, `cmbtwall` |
| 148 | Core | Difficulty beyond lairs: the final score, AI strength and the seed field |
| 149 | Client | Show squares that are known but not currently seen (fog of war) |
| 150 | Map | Generator parity with the original |
| 151 | Client | Choose an enemy global enchantment as a target (Disjunction, Spell Binding) |
| 152 | Client | Show temporary effects on units and cities |
| 153 | Magic | Dispelling temporary effects |
| 154 | Meta | Raiders from neutral cities |
| 155 | Core | Starting setup: the original's random placement and AI race rules |
| 156 | Combat | Defence bonuses of enchantments and the undead's immunities |
| 157 | Combat | Special attack leftovers |
| 158 | Combat | Siege leftovers: magic walls, the AI's raze decision, diplomacy |
| 159 | Combat | Casting leftovers: the rest of the battle spells |
| 160 | Combat | Battlefield node counter and node auras |
| 161 | Combat | Dispelling battlefield enchantments |
| 162 | Client | Weapon quality icons and help |
| 163 | Combat | Wall of Stone in battle |
| 164 | Combat | Heavenly Light's True Light |
| 166 | Client | Book animations, the combat spell book and spell help |
| 167 | Meta | Diplomacy leftovers |
| 168 | Client | The original's diplomacy dialog as a local resource |
| 172 | Meta | Learning AI: battle AI follow-ups |
| 174 | Units | Level bonuses of the original (the level path has our simpler pattern) |
| 175 | Combat | Animate Dead: a list of the fallen to choose from in the client |
| 176 | Housekeeping | Mac installer: asks for the original files, extracts, builds a local app |
| 183 | Architecture | A test that limits the methods on `Game` per file |
| 184 | Architecture | The spell executor's host carries the `GameRng` |
| 185 | Architecture | Units follow the domain rules (unit queries and the stat engine over the world and the content) |
| 186 | Architecture | Economy follows the domain rules (the city's yields computed from what they need) |
| 187 | Architecture | Production follows the domain rules |
| 188 | Architecture | The turn code follows the domain rules (a sequence of domain steps) |
| 189 | Architecture | Movement and transport follow the domain rules |
| 190 | Architecture | The lairs (guarded sites) follow the domain rules |
| 191 | Architecture | Conquest follows the domain rules |
| 192 | Architecture | The fortress, banishment and defeat follow the domain rules |
| 193 | Architecture | Vision follows the domain rules |

## Descriptions

### Combat

**39. Auto-resolve.** Done: `Command::AutoResolve` (console `auto`) fights a battle to the end with the placeholder battle AI (`battle/ai.rs`) playing both sides, from an adjacent square, with the usual aftermath. Open: the strategic-combat-only option [685], a button for it on the battle screen (the Auto button is drawn greyed; task 133), and a better battle AI (46) so the result is not just whoever the placeholder favours.

**46. Battle AI.** Decision making for AI-controlled units in tactical battles and for the AI
wizard's combat spells. After 40 and 43.

### Heroes and items

**47. Hero abilities.** 13 hero abilities with levels and "super" versions [515-526]; data exists
as imported text, the effects mostly do not. Low priority: only needed for specific spells, buildings, units or abilities. Armsmaster itself is in the engine (task 27): give the heroes the `armsmaster` and `super_armsmaster` ability ids.

**48. Hiring, summoning, recall, resurrection and fame.** Hero availability by fame, hiring cost,
the noble, charmed and lucky traits, Summon Hero and Summon Champion, Recall Hero, dead hero
tracking and resurrection [577-588, 240, 246]. Also: a Charismatic wizard pays half for hiring heroes and mercenaries and for buying items, and a Famous one has twice the chance of heroes and mercenaries offering themselves (task 53 left these here).

**49. Item slots, powers and data schema.** Slots by hero type (sword, bow, staff, armor, misc),
item powers, charges, and how this is expressed in data (open question in `design.md` section 13).
Also: a hero's shield gives the hero a large shield in battle (the ability exists, `battle/game.rs`), and the items of a fallen hero are loot for the winner (the original lists them after the battle; `battle/spoils.rs` leaves them out).

**50. Enchant Item, Create Artifact, the vault and the anvil.** Crafting UI and rules (power budget
per book, mana cost, Artificer discount already in costs), the player's item vault, scrapping items
at the anvil.

**51. Heroes' casting contribution and sage heroes.** Heroes add to casting skill and sage heroes
add research (level times 3, or 9 for the greater ability). The hooks (`Skill_Left` and the
research income) exist; heroes do not feed them yet. A hero's mana in battle (`(skill + 1) * (level + 1) * 5 / 2`, `BattleUnit::mana`) and so its spell shots (3 mana a shot) and own spells come with this. Low priority: only needed for specific spells, buildings, units or abilities.

### Magic (first pass done; see `magic.md`)

**56. The 37 "special" spells.** Map reveals, terrain and mineral changes, healing a map square,
recalls, moving the fortress, resurrect and similar [mechanics-gaps section 4]. Add the missing
effect primitives (map reveal, teleport, grant movement type, building destruction, random-effect
tables) and implement them in groups. Low priority: only needed for specific spells, buildings, units or abilities.

**58. Unit enchantments needing non-stat effects.** 20 spells with movement types, immunities,
conditional bonuses and regeneration. Needs conditional modifiers (by attacker realm, ranged versus
melee, creature realm), immunity flags and movement grants; depends on 30 and the combat tasks. Low priority: only needed for specific spells, buildings, units or abilities.

**60. Wizard-targeting, destruction and other special categories.** About 20 spells aimed at
wizards, resistable kills, destruction of buildings and similar; needs wizard-state changes (mana,
skill, fame, diplomacy) and spell ownership transfer. Low priority: only needed for specific spells, buildings, units or abilities.

**61. The Spell of Mastery's research cost and the AI casting it.** Finishing the spell wins
the game and the Spell of Return works after banishment (tasks 70 and 15; `docs/game_end.md`).
Still open: the Spell of Mastery's special research cost (the data holds a placeholder of 1
because the original computes it), the announcement when a wizard starts casting it, and the AI
choosing to cast it.

**62. Player-chosen losses when mana runs short.** The engine drops the newest upkeep items
deterministically; the original asks the player. Add a prompt or a priority list, and the "mana
short" event interaction (68).

### Meta: diplomacy, AI, events, victory

**64. Wizard personalities and AI objectives.** Maniacal to peaceful personalities and the
pragmatist, militarist, theurgist, perfectionist and expansionist objectives that steer the AI
[271, 782-794]. See `docs/diplomacy.md`.

**65. AI v1.** First version in `crates/f4x-ai` (taxes, research, summoning spells, production
and buying, settlers, garrisons, attacks on weaker stacks, marching on explored enemy cities); it
uses commands only, like the human, and `f4x-tools soak` plays whole AI-against-AI games. It is
not hooked into the client yet (the rival still does nothing there). Known limits from the soak
runs: armies stay small (food upkeep disbands units), it rarely finds the rival to attack, and
outposts take very long to grow (`PLANNED.md` item 21), so no soak game has ended with a winner.

**66. AI research and spell use.** Research picking by personality, casting overland spells
(summoning, enchantments) and the spell weights from the original's AI tables.

**67. AI attack.** Target selection, army assembly and attacking cities and sites; depends on
combat (33-42).

**68. Random events.** Plague, population boom, conjunctions, moons, mana short and others, each
ending with 5 percent chance per turn; switchable option [768-775, 683]. Follow intent for B18
(count from the real turn, not from game start or load).

**69. Difficulty levels.** Difficulty is chosen in the options step (task 10) and scales lair guards
and treasure. The original's AI power, gold and mana modifiers and the Hard-level starting spells
for the AI are **not** ported: the AI gets no advantage or disadvantage (design.md section 8), so
difficulty can only change how well the AI plays. The final score is scaled by difficulty
(`endgame::scale_for_difficulty` exists but is not called yet). See `docs/game_end.md`.

**71. Advisors.** The original's advisors (surveyor, cartographer, historian, astrologer, chancellor,
tax collector, grand vizier, war and mage advisors) give information screens and messages about
the player's progress and surroundings [764, 216, 221]. Add an advisor menu and screens fed by
`Game` queries, plus the end-of-turn advice messages (a city starving, a spell learned, a hero
available).

**72. Fame effects.** Fame sources (heroes, Just Cause, Famous retort) and effects on unit upkeep
and hero availability [765]; follow the help for legendary heroes (+3 fame per level, B12).

**73. Save-format migration.** Version 1 is the only format; add a conversion step before
`restore` when the layout changes incompatibly. Only needed at the first breaking change.

**74. Stable command references.** Commands carry `hecs` entity ids, so a command log cannot
cross a load. If replays or multiplayer are wanted, switch commands to creation numbers (`Serial`).
Decide the multiplayer scope first (`design.md` section 13).

### Client, content, housekeeping

**75. Wizard overview screen.** A screen about one wizard (the player's own and, once met, the
rivals): portrait (83), name, books, retorts, fame, gold and mana, cities, heroes, and the spells
they are known to cast. The data exists (`Magic`, `Player`); the screen and its entry points
(menu key, clicking a wizard in lists) do not.

**76. Background music.** Play music in the client: a track per situation (title, map, city, battle,
victory and defeat, and the diplomacy screen's own tune), looping and crossfading, with a volume setting (94). The original's music
can be extracted locally for the author's own copy (like the art, under `assets-local/`; the
repo ships none); decide the audio crate, and keep the game silent and working without files.

**77. Content pass and balance.** Fill out races, units, buildings and spells from the imported
data, then balance with AI-vs-AI soak runs (needs AI and combat).

**78. Open conflict decisions.** Items still marked "Check" or open in
`help-vs-implementation-conflicts.md`: B9 Heroism, B10 undead immunities, B11 Evil Omens cost,
B12 legendary fame (decided, not built), B13 Blur, B15 swamp food, B16 nature node food, C3
Invisibility. Decide each, apply to content and help, and add the "[Updated from game code]" note
where the help changes.

**80. Push to GitHub and tidy.** Work since the last push is committed locally only. Push when the
user agrees. The user still needs to delete the old `~/reference` folder (the earlier ReMoM clone
and steamcmd home) themselves; the safety hook blocks it for Claude.

**81. Packaging for Mac.** (A local-play `.app` exists: `scripts/make_app.sh`, see `docs/local-assets.md`; the rest below is for distribution.) A macOS build: a `.app` bundle (icon, `Info.plist`), universal or
per-architecture binaries (Apple silicon and Intel), code signing and notarisation if the
author has a developer account (otherwise documented right-click-open steps), a disk image or
zip, and the local-assets workflow documented for players who supply their own original files
(`docs/local-assets.md`): where the game looks for `assets-local/` and `content-local/`, and how
to run the importer with the system Python. A release workflow on GitHub Actions if feasible.

**82. Wizard defeat aftermath and fortress display.** Done when a wizard is defeated
(`endgame::aftermath`): all their units are dismissed, their enchantments come off every city and
unit, their nodes are freed and the conqueror takes half their gold. Not done: when a *banished*
wizard with other cities resigns, the original turns their garrisoned normal units neutral and (for
AI) their remaining cities neutral. An AI wizard resigns instead of casting the Spell of Return if its skill is below 40
or it has no mana. Also draw the fortress and circle as buildings in the city screen's building
area.

**84. Analyse the original's screenshots and adjust the graphics.** Study the screenshots of the
original game in `reference/screenshots/` (local only) and bring the client's look closer to
them: screen layouts and proportions (map view, side panel, city screen, magic screen, creation
screen), the use of the original's frames, buttons, fonts and palette, unit and city
presentation on the map, terrain and shore drawing, and the information shown in the bars.
First write a short comparison list (what differs, per screen), then change the client in
steps, checking each with `--shot` screenshots. Original-derived art stays under
`assets-local/`.

*Progress.* Done: the overland screen now uses the original's frame (menu pictures, minimap,
gold and mana boxes, resource panels, Next Turn) and the city screen follows the original
layout with its fonts, citizen icons, resource icon rows, unit pictures, production panel,
picture buttons and the cityscape. The screenshots in `reference/screenshots/` include
Caster of Magic screens (the Game Options, Score Modifiers, hero sheet and building-choice
screens): those belong with the expansion content (`PLANNED.md` item 4), not the base game.
The magic screen, the spell book and the big book follow the original's layouts (`MagicScr.c`, `SBookScr.c`, `Spellbook.c`; checked against a screenshot of the real Magic screen). Still open: the wizard creation screen, the unit and army lists, the battle
screen, the hero sheet and the diplomacy screen.

**85. Import and play the original's opening movie.** Extract the intro from the author's own
copy of the game (its animation frames and palette from the intro `.lbx` files, and the sound
and music if they can be decoded; `reference/ReMoM/MoM/src/INTRO.c` shows which entries play in
which order and with which timing) into `assets-local/` with a new `f4x-tools` step, and play it
in the client at start-up, skippable with any key and switchable off in the settings. The
movie is original-derived: it stays local, and the game runs without it. Depends on the audio tasks (76, 97)
for the sound.

**87. Remaining global enchantments.** The global spells still without effect, grouped by what
they need: *combat* (Eternal Night, Zombie Mastery, Life Force and Tranquility's battle part,
Nature's Wrath, Time Stop's combat part: 33-46); *corruption and map change* (Great Wasting,
Armageddon, Meteor Storm: land corruption 5, terraform volcano rain, timed damage); *dispel
mathematics* (Suppress Magic, Life Force, Tranquility: 59); *diplomacy and fame* (Aura of
Majesty: 63; Just Cause: 72); *movement* (Wind Mastery: doubles own ship and sky speeds and
halves everyone else's, which needs a condition on movement type); *abilities* (Holy Arms' magic weapons, Doom Mastery's
mutation of new units: temporary effects store 30); and Time Stop's turn flow. Low priority: only needed for specific spells, buildings, units or abilities.

**88. Hall of fame.** The list of best finished games with wizard, race, score, turns and
difficulty [628-641, 798], stored outside the saves (for example `saves/hall_of_fame.ron`),
shown from the start menu (86) and after a game ends (victory and score, 70). Needs 70 first.

**89. Game settings.** The options screens of the original [675-692]: automatic saving, event
switches, strategic-only combat, animation and message speeds, and so on. Store them as a
settings record, `serde(default)` and saved with the game where they change rules (events,
Spell of Return option from `PLANNED.md`), and in a client configuration file where they only
change presentation. Reachable from the start menu (86) and in game.

**94. Settings screen.** The client screen for the options of task 89: gameplay options and
presentation options (volume, speeds, zoom), reachable from the start menu (86).

**95. Tutorial.** A first-game guide: highlighted steps for moving, founding a city, building,
researching and casting, with a way to skip or switch it off.

**96. City screen: enchantments list.** List the spells on a city (own and enemy, with the caster's
colour and upkeep) as the original's city screen does [301], and allow dispel attempts from it.

**97. Sound effects.** Short sounds for UI clicks, turn end, city growth, spell casting, combat
hits and deaths, and the wizard's spoken lines where the original has them (the diplomacy screen's greetings and answers among them), played through the
same audio layer as the music (76) with its own volume, from locally extracted files. Events
(`Event` values from `Game::apply`) are the trigger points, so the game core stays free of audio.

**98. Packaging for Linux.** A Linux build: static or well-documented dynamic binaries (the
macroquad dependencies: X11 or Wayland, OpenGL, ALSA once audio exists), an AppImage or tarball,
a `.desktop` file and icon, testing on a current Debian/Ubuntu and Fedora, and the same
local-assets workflow as the Mac build (81). A release workflow on GitHub Actions.

**99. Saving during a battle.** A battle in progress is not part of a save today (`Game::battle`
is skipped, so saving mid-fight would lose it, and the client should block saving). Make
`Battle` serialisable: units with their statistics, cells, turn, active side, and the random
generator state (seed plus draw count, as for the game), plus the list of overland units the
fighters were copied from (by creation number). Restore it on load, check it for consistency
(units on the map, sources still alive), keep old saves loading, and test that a save in the
middle of a battle continues exactly like the unsaved one.

**100. Draw the original's edge variants for mountains, hills and tundra.** The extractor already writes all five tables of `autotile.tsv` (neighbour mask to terrain type), but the client only uses table 0 (ocean shores). Use the other tables so mountain, hill and tundra squares blend with their neighbours as in the original, and check the result against the screenshots (84).

**102. Road-building path selection in the client.** The B key builds a road only on the square under the engineers. Add the original's road-building mode (topics 510-512): click a path, show valid squares in blue and invalid in red with the turns it will take, OK and Cancel buttons, using `find_path`-style planning and `Command::BuildRoad` with the whole path.

**103. Scouting levels and unit sight.** Units see by their `Sight` stat only; the original's scouting ability adds a level to sight [546] and Wind Walking gives its stack speed 3. Import scouting as its own ability or stat modifier, apply it in `visibility`, and give wind-walking stacks their speed.

**104. Ship tiers by building and ship movement estimates.** Which buildings allow which ship (ship wright's guild, shipyard, maritime guild: help 725-727) is only encoded as unit requirements in the data. Verify it for every ship, show the capacity in the unit and city screens, and make path-turn estimates (`turns_for_path`) use the ship's speed instead of the slowest passenger's. Low priority: only needed for specific spells, buildings, units or abilities.

**106. Battle views for the AI and for tools.** Task 33's model has no read-only summary an AI can search over: add cheap clone and undo of a `Battle`, a list of legal commands for a unit, and an evaluation helper (remaining hit points and strength per side), tested against random play. Prerequisite for the battle AI (46) and for auto-resolve (39).

**107. Check assumptions made while implementing.** Decisions taken without a source, to confirm against the original or the help: Evil Omens spares the wizard who holds it; the original's own Lo Pan spends 12 picks (presets skip the budget); the decompiled thrown and breath attack code looks broken, so thrown and breath attacks use plain strength with no range penalty; melee against fliers deals nothing from walkers; roads on nodes take 4 or 5 turns whatever the terrain; Enchant Road is permanent; melding spirits are used up even when the meld fails; the 100-turn battle limit is our own. Write each one into `help-vs-implementation-conflicts.md` with the decision.

**108. Magic nodes: warp spell, guardians' monsters and node visuals.** Warped nodes exist as a flag but nothing sets it (Warp Node, task 7); unowned nodes should be guarded by monsters before they can be melded (task 8); the client draws the aura as a tint only: add the original's node terrain art and a node info panel (power, owner, guardian, warped).

**109. Importer clean-up.** `scripts/import_mom.py` has duplicate entries (`Natures Eye` and `Nature's Eye` in the city-enchantment table), several maps keyed by display names that differ between sources, and spell-effect tables that grew by hand (global, tile, fortress hooks). Tidy them into one table per kind with a check that every key matches a real spell, building or unit, reported in `IMPORT_REPORT.txt`.

**110. Battle map generation parity with the original.** Our battle map only sprinkles rough and blocked squares by terrain class. The original (`Generate_Combat_Map` in ReMoM `Combat.c`) builds the map from the overland square and its surroundings: terrain sets and patches, scattered trees and rocks by terrain, roads crossing the battlefield where overland roads run, rivers carved through, rough ground removed around cities, and per-square movement-cost maps. Port it, keep it seeded and deterministic, and test the shapes (connectivity of the deployment edges, roads and rivers on the right sides).

**111. Special battle sites: nodes, lairs, towers, ruins and fortress cities.** The original fights node, lair, tower, temple and ruin battles on their own battlefields (node type changes the ground and effects, `Combat_Node_Type`; lairs and keeps have structures; a wizard's fortress city has its own layout). Add battle map variants and their effects on movement and combat, fed from the overland feature under the defenders; needs guardians (8) and nodes (108).

**112. Battle map art.** Draw the battle map with the original's combat terrain art, extracted locally (`cmbgrass*`, `cmbdesrt*`, `citywall`, `cityscap` and the other combat `.lbx` files already extracted under `assets-local/art`): terrain sets per overland terrain and plane, tree and rock scenery, roads, rivers, city buildings and walls, the isometric grid and its coordinate mapping (`Combat_Grid_Screen_Coordinates`). Part of the battle screen (45) but large enough to track separately.

**115. Cityscape: road masking.** The city picture draws the whole road grid; the original hides the road squares that no building, house or tree touches (`Cityscape_Roads_1` to `_5` in ReMoM `CITYSCAP.c`, using the road pictures and the two masks `cityscap.lbx` 109 and 110), so roads appear only where the city has grown. Port the masking rules.

**116. Cityscape: animation, wards and rubble.** Animate the city picture as the original does: building frames (a counter that cycles nine frames), the water overlay, the wall, and the build-in effect when something is newly built. Draw the ward and enchantment icons along the bottom (death, chaos, nature, life and sorcery wards, Nature's Eye, Inspirations, Prosperity, Consecration), Wall of Fire, Wall of Darkness and the wall sprites, Cloud of Shadow, Heavenly Light and Chaos Rift backdrops, Evil Presence over religious buildings, and rubble where a building was destroyed.

**117. Selling buildings from the city picture.** Clicking a building in the city picture offers to sell it (help topic 303): a confirmation with the gold it brings, then `Command::SellBuilding`. The command exists; the screen has no way to use it, and the prerequisite chain check is task 17.

**118. Bitmap fonts on every screen and a text layout helper.** Only the map frame and the city screen use the original's fonts. Move the magic, wizard creation, production menu, message log and unit panels to them, and add a bitmap-font word wrap, justify and column helper (`Print_Paragraph` style) so long help text and descriptions can be shown.

**119. City screen details.** What the original's city screen has and ours lacks: the production picture for buildings (the cityscape sprite) and a count of queued items, the unit list scrolling past nine units and clicking a unit to select or unload it, right-click help on every element (the original's help topics 295-303 are imported), the enchantment list scrolling and dispelling, the city name editing, and the worked-area view with the original's tile and mineral pictures and food and production figures for each square.

**120. Overland screen details.** What the original's main screen has and ours lacks: the unit panel in the right column with the stack's pictures and the Done, Patrol, Wait and Build buttons, the minimap's other plane switch and click-drag, the surveyor and the info menus behind the Info button, the Armies and Cities lists behind their buttons (tasks 71 and 91-96 cover the screens themselves), terrain and unit animation, and unit movement as a smooth slide.

**121. Unexplored-edge pictures.** The original keeps exploration per square corner and draws `mapback.lbx` 0 to 13 along the border of explored land so the edge is soft. We explore whole squares and draw black. Needs corner exploration in the fog data (with saves) and the 14 edge masks.

**122. River mouths and shore pieces with rivers.** The original has terrain types (`_Shore..R..`) for a river reaching the sea; we lay the plain river piece over the shore. Pick the river-mouth shore pieces and check against the original's look.

**123. Call the Void, Great Wasting and Armageddon corruption.** The corruption layer, Corruption, Purify, Land Corruption and Gaia's Blessing exist; the global spells that corrupt land each turn (and the Great Wasting and Armageddon looks) are still to wire to it. Low priority: only needed for specific spells, buildings, units or abilities.

**124. Mines, lumber camps and hunter's lodges.** `mapback.lbx` 87 (mine), 88 (lumber camp), 90 (hunter's lodge) are extracted but nothing in the base game's data places them; check whether the original used them (they may be Caster of Magic's) and, if so, move to PLANNED item 4. Low priority: only needed for specific spells, buildings, units or abilities.

**125. Unit stack backgrounds.** Draw each stack's marker on the `mapback.lbx` 14 to 19 background of its owner's banner (blue, green, purple, red, yellow, neutral) instead of the current icon.

**126. Bitmap font for the remaining overland text.** The gold, mana, income panels and city names use the original fonts; still in the plain font are the turn/plane/race line, the selection and hover info (`Lines` helper), the message log, the "F1 keys" hint and key list, and the unit-count badge on stacks. Give `Lines` a bitmap-font mode (or replace it) and move these over, using the original's sizes and colours; see task 118 for the other screens.

**127. Lair rewards: original spell, book and retort selection.** We follow the intent of
`WIZ_RollSpellReward` and `EZ_SpecialTreasure` (ReMoM marks both as muddled): spell of the tier
from a realm without books, then a book (75%) or retort. Compare against the original's results over
many rolls and adopt its exact odds where they differ. Also: Famous gives +10 fame; Alchemy or
Warlord counting as "arcane" in the realm check (ReMoM bug note); prisoner heroes and magic items need
tasks 48 and 49 (the `Loot` already carries their values); the 12-book and 6-retort limits.

**128. Site encounter dialogs and treasure popup.** The client should answer `SiteEncounter` with the
original's confirm box ("You have found a ruins.  Scouts have spotted 4 Skeletons within the ruins.
Do you wish to enter?", node pictures per realm, Y/N), send `EnterSite`, and after a win show
"Inside you find ..." with the gold, mana, items, prisoner and spell lines. Unguarded sites pay out
on arrival without a prompt. The fight itself uses the battle screen (done); the prompt is what is missing.

**130. Warped node and cleared site pictures.** A warped node is drawn with a purple tint placeholder;
use `mapback.lbx` 93 (the warped-node mask). Check the original for any picture left behind by a
cleared site (the original just stops drawing the lair) and for nodes that are guarded versus cleared. Low priority: only needed for specific spells, buildings, units or abilities.

**131. Battle grid shape and deployment as in the original.** The original draws 22 staggered rows
of 11 cells, which covers only the band of the 21 by 22 grid with about -9 <= x - y <= 11
(`Combat_Screen_Map_Draw`); the core lets units stand on all squares and deploys in columns at the
west and east edges, so the client draws a full diamond. Port the original's playable squares,
its deployment squares per side, the lair/structure square (6, 11) and `Combat_Grid_Screen_Coordinates`
placement at 320 by 200, then restrict the core map and movement to them.

**132. Battle screen: unit figures, animation and the original's frame.** Done: each unit is drawn
as its figures (`figures1`-`figure16`, positions and facing as in the original, painted in the
owner's banner), and orders play as animations (walking along the path, a lunge and hurt flash,
a missile in flight, fading death). Still missing: the original's projectile and effect pictures
(`cmbmagic`, `cmbtfx`; arrows and bolts are drawn as simple shapes), blood and gibs, enchantment
outlines and figure effects, the Spell, Auto and Info buttons' functions (the pictures are drawn,
greyed, and Wait, Done and Flee work), the unit and info windows, enchantment icons, mouse
pointers by action and sound. Terrain autotile edges, dirt patches, roads, rivers and the city
(task 112) are also missing.

**133. Prompt before attacking, and battles between AI players.** A human move into an enemy
stack or a city starts the battle at once; the original asks first (attack, or cancel). AI
attacks (`AttackRequested` for a non-human attacker) are ignored by the client, and nothing
resolves a fight between two AI players in the client. The AI crate attacks with `Command::AutoResolve` (39) and `f4x-tools soak` runs it, but the client does not run the AI (task 135). Still open here: the attack prompt, the Auto button on the battle screen (greyed) and the option to auto-resolve a battle the human does not want to fight by hand.

**134. Game-end cut-scenes and the original's score screen.** The game-over screen is plain text
and buttons. The original plays a victory animation, a defeat ("limbo fall") animation and, for
a conquest, a conquest animation; announces when a wizard *starts* casting the Spell of Mastery
(full-screen, with a two-line caption) and, when it completes, parades each surviving wizard's
portrait; and draws the score as icon rows (book, townsfolk, defeated wizards' portraits, fame,
time, Spell of Mastery) with a running bar of `min(score / 40, 200)`. Needs the local art and
music (task 93). See `docs/game_end.md`.

**135. Run the AI wizards in the client.** The client's rival has no behaviour: only the human gets
a fortress and units (`start_game`), and ending the turn does nothing for the rival. Connect
`f4x-ai`: start every wizard with `f4x_core::setup::new_game` (the client keeps its own copy of the
setup, so it should call the shared one, which also places each wizard's fortress and units), run
`f4x_ai::play_round`-style turns for the AIs before the turn ends, show what they did (new
cities, battles against the human, captured cities) as notes, and handle an AI attacking the
human: the human defends in the battle screen or auto-resolves, instead of the attack being
ignored. Needs the attack prompt and Auto button (133) and the AI tuning in 65 to be worth playing
against. See `docs/console.md` section 6 for the matching console gaps.

**136. Earth Gate and Astral Gate in the client and the AI.** The core has both (task 12). The client
should offer the teleport when a stack in an Earth Gate city moves onto another gate city (a button
or a prompt) and enable the plane-shift button in an Astral Gate city; the AI should use them to
reinforce threatened cities and to cross planes. Low priority: only needed for specific spells, buildings, units or abilities.

**137. Teleport and merge animation in the battle screen.** The core reports a teleport or merge as a
`Moved` event with a one-square path to the destination, so the battle screen now glides the unit across
the map. Play the original's fade out at the origin and fade in at the destination (teleport) or a sinking
and rising figure (merge, `Battle_Unit_Teleport`, `Battle_Unit_Tunnel`), with the sounds. Low priority: only needed for specific spells, buildings, units or abilities.

**138. Army list screen.** The original's `ArmyList_Screen`: every stack of the wizard with its units and orders; opens a unit's details (140) and the Item screen (142). Reached from the Army button of the main screen. See [`screen_list.md`](screen_list.md).

**139. City list screen.** `CityList_Screen`: all cities in a table (size, gold, production, what is being built) with a small map; opens the city screen or the production menu. The Cities button of the main screen.

**140. Unit statistics window and the enemy stack list.** `Unit_Statistics_Popup` (the sheet of one unit: attributes with their bonuses, abilities, enchantments, level, items, Dismiss) opened by right-clicking a unit in the stack panel, the Army list, the city garrison, the Item screen and in combat; and `Unit_List_Window`, the read-only list of another wizard's stack. The battle screen's text panel is a stopgap.

**141. Outpost screen, enemy city screen and the naming boxes.** `Outpost_Screen` (a small city sheet for an outpost; also names a newly founded one), `Enemy_City_Screen` (what is known about another wizard's city) and the rename boxes for the capital (first turn) and new outposts.

**142. Item screen and the item pop-up.** `Item_Screen` (move magic items between hero slots and the wizard's vault, destroy one) and `Item_View_Popup`. Needs the item rules of tasks 49 and 50.

**143. Offer, hire and event pop-ups.** `Merchant_Popup`, `Hire_Merc_Popup`, `Hire_Hero_Popup`, `Hero_LevelUp_Popup` and `Show_Event_Message`: the offers made at a turn's start (chance rising with fame, `Determine_Offer`), hero hiring, level-up announcements and the random-event announcements. Needs 48 (heroes), 68 (events) and 72 (fame).

**144. Turn summary and message pop-ups.** The Chancellor's scroll after a turn (random events, units disbanded, units killed, city growth, city deaths), the building-complete message, the evict message, the retreat-loss message, spell-cast banners, and a common yes/no confirmation box with the original's questions, including the warning before ending a turn that units will die for lack of food. Today events go to a log list.

**146. Palette cycling: the pulsing colour of index 198.** The original animates by cycling palette index 198 (`Cycle_Palette_Color` in ReMoM `Fonts.c`): between dark and bright red (40..63 in the red channel) on the main screen, load screen and start menu, black to red (0..55) in combat, and pale greys or blues in the spell book (`Spellbook.c`). `extract-art` bakes one fixed colour for that index into the PNGs, so the effect is missing. Find which pictures use index 198 (scan every picture's frames), write those pixels as a mask beside the picture (as the banner masks are written), and have the client tint the mask with a colour that moves between the original's two colours at the original's rate (the step values are 1 to 11 a call). Document the result in `docs/extract-art.md` section 4. Other palette effects (fades, the ghost remap) are separate.

**147. Find the use of the unreferenced art files.** ReMoM's loader has no reference to four LBX files that `extract-art` extracts fine: `combat`, `desert`, `tundra` and `cmbtwall`. `docs/extract-art.md` lists them as unknown rather than guessing. Find out what they are for: look at the pictures (frame counts, sizes, which terrain or battle element they resemble), search the original's disassembly and the other reverse-engineering sources in `reference/` for their names or for the loader slots they would occupy, and compare with the battlefield terrain and wall code (`cmbtwall` may be wall pieces beside `citywall`; `desert` and `tundra` may be terrain variants or battlefield backgrounds). Record the result in section 9 of `docs/extract-art.md` (replace the "unknown" row) and, if one is a real gap in the client, add a task for it. If nothing can be found, say so in the doc and leave the files unused.

**148. Difficulty beyond lairs, and the seed field.** The difficulty chosen in the options step (task 10) only scales lair guards and treasure. Call `endgame::scale_for_difficulty` where the final score is made, leave out the original's AI bonuses (design.md section 8: no advantage or disadvantage for the AI), let the difficulty set only how well the new AI plays once it exists, and add a map-seed field to the options screen (the seed is a command-line option now).

**149. Fog of war: known but unseen squares.** The overworld draws two states only: never explored (black) and explored (drawn in full). The game already tracks what each wizard sees now (`Game::visibility`); the client uses it only to hide enemy units. Draw explored squares that no unit or city currently sees dimmed, as the original does (check how it does it in ReMoM's map drawing before copying; it may use a darkening palette remap), keep what was last seen of cities there (a remembered city, not its current state, if the rules need that), and make the minimap match. Soft edges along the explored border are task 121.

**150. Generator parity with the original.** Our generator is our own design (noise-based
elevation with a land percentage). Optionally port the original's `NEWG_CreateLands`: random walkers
from origins spread over map sections (3 to 5 arms of 6-15, 11-20 or 21-30 steps by land size, with
a bias against straight coastlines, 2-tile and 4-tile borders) and its shore fix-up, as described in
`reference/ReMoM/doc/MoX-MapGen-NEWG_CreateLands__WIP.md`. The new-game settings (difficulty,
opponents, land size, node strength) are done (task 10 in TASKS-DONE). Lair placement parity is done (task 129 in TASKS-DONE).
Parity task, low priority.

**151. Choose an enemy global enchantment as a target.** Disjunction, Disjunction True and Spell Binding take `Target::Global(owner, spell)`. The client cannot pick one yet: casting them does nothing. Add a picker (the original's list of each rival's global enchantments by wizard, with the spell's name and cost) that opens when the spell needs it, then sends the cast with the chosen target. The console already takes `global:<player>:<spell>`.

**152. Show temporary effects on units and cities.** The temporary effects store (task 30, `temp_effects.rs`) changes stats and abilities, but nothing says so. Show them on the unit info screen and the city screen (what, how much, turns left or "permanent"), and list them in the console's unit and city views.

**153. Dispelling temporary effects.** Temporary effects (task 30) are not enchantments, so Dispel Magic, Disenchant and Disjunction cannot remove them. Decide which ones a dispel should reach (the original makes instant grants permanent; timed ones may be the exception), give those a source spell and caster so dispel difficulty and the enemy-only rule apply (task 59), and have `dispel_target` include them.

**155. Starting setup: the original's random placement and AI race rules.** Task 32 brought the start in line with the original's `Generate_Home_Cities` (capital of four with three farmers, spearmen and swordsmen, smithy, barracks and builders hall, tax index 2, Myrran wizards on Myrror, sites that feed eight). Left out: (1) the original picks fortress sites at random (1000 tries per distance, fortress distance 16 relaxing to 10, node/tower/lair distance 7 relaxing to 1, halved in the check) while ours takes the nearest fitting site to a spread-out anchor, and measures every distance with `Range` where the original uses the wrapping delta for fortresses, nodes and lairs; (2) AI races: up to normal difficulty any of the nine Arcanus races, above it only Klackon, High Elf, Halfling, High Men and Nomad (a bias against the AI that design.md section 8 may rule out; decide), and a High Elf capital is moved off non-forest only by accident in the original; (3) starting gold: the original gives the human `(5 - difficulty) * 25` and every AI 150, ours gives everyone 100 by decision, and the AI's flat 150 would be an AI bonus; (4) fame 10 for the Famous retort (check it is set). Low priority.

**156. Defence bonuses of enchantments and the undead's immunities.** Task 35 did immunities and resistance. Still open from the same original code: the unit enchantments that add *defence* against ranged magic of a realm (Elemental Armor +10 against chaos and nature, Resist Elements +3, Bless +3 against chaos and death), and the spells themselves, which the importer still leaves as `unimplemented`: Elemental Armor, Bless, Righteousness (their resistance part is done), Resist All and Invulnerability (-2 damage per hit). Undead should also gain cold, poison and illusion immunity (decision B10 in `help-vs-implementation-conflicts.md`), which belongs with task 29. Give `effective_defense` a realm bonus parameter and apply the spell tables in `immunity.rs`.

**157. Special attack leftovers.** Task 36 did armour piercing, chaos (doom) damage, illusion, poison, life steal, stoning and death touch, destruction, dispel evil and the gazes. Left: (1) **Power drain** (the target resists or loses 2 to 20 spell points): needs battle mana, with task 43. (2) Attacks that come from an **item or spell** rather than the unit: the original gives them an extra penalty (stoning touch -1, death touch and life steal -3) and halves doom damage; the unit's own count in full. Needs the source of an ability to be known (task 30's store, items task 49). (3) **Hit points above full strength** from life steal (`Extra_Hits`). (4) The **damage types**: done for what returns the dead (`Damage::drained` and `gone`, `BattleUnit::fate`): damage from life steal or a create-undead attacker (Ghouls) leaves the victim to be taken by the winner as an undead unit, and what destruction or a severing spell kills is gone for good; left: the healing of the dead (stoning, death and destruction kills are not healed afterwards), the original's Life Drain spell as a third type, and the damage a heal takes off a unit's tally. (5) Cause fear.

**158. Siege leftovers: magic walls, the AI's raze decision, diplomacy.** Task 41 did city battles with walls and a gate, wall crushers, the defence bonus, city damage, loot, fame, losses and razing. Left: (1) the **battlefield spells** Wall of Stone (raises a wall in battle: task 163) and Disrupt (breaks a section); Wall of Fire, Wall of Darkness and Flying Fortress are done (task 25). (2) The **AI's decision to raze**: the original's `Raze_Check` scores the wizard's personality, how few troops it has left to garrison the city, and its units on the continent against the loser's; it needs an AI player (design.md section 8: the decision lives in `f4x-ai` and arrives as `RazeCity`). (3) *(Diplomacy, done with task 63: a city that holds costs the attacker -20, or -60 for the fortress city, and one taken -40 and a war.)* (4) **Monsters and raiders** taking a city: the neutral attackers' extra losses (50 percent more citizens, 50 more building chance) and the rampage that can leave ruins (with task 154). (5) The **city damage from spells** such as Magic Vortex (5 for each turn spent in the city). (6) The wizard whose fortress falls to a razed city: check the banishment and the summoning circle when the capital is razed (the original changes the owner first and then destroys the city, as here).

**159. Casting leftovers.** Tasks 43 and 44 did the wizard's casting in battle and the battlefield spells (`combat.md` sections 8 and 9), and part of this list: Word of Death, Petrify, Animate Dead (the wizard's own dead, not heroes or the destroyed; the client raises the strongest: task 175), Dispel Evil, Banish, Disintegrate, Holy Word, Death Spell, Flame Strike, Mass Healing, Call Chaos (two of its eight outcomes not yet), the curses Weakness, Vertigo and Mind Storm, and the Invisibility spell. Left: (1) the battle spells whose effects are not in the game, which are not offered yet: Earth to Mud, Web, Cracks Call, Confusion, Spell Lock, Warp Wood, Disrupt, Eldritch Weapon, Shatter, Warp Creature, Immolation, Magic Vortex, Creature Binding, Cloak of Fear, Black Sleep, **Life Drain** (a resistance roll worth the extra mana; it heals the caster), Possession, Berserk, Wraith Form, Raise Dead, Heroism, Invulnerability, Recall Hero and Chaos Channels (needed by Call Chaos). Most need unit mechanics of task 37 (abilities) or the map of task 158 (walls). Also the spells of eligibility 4, which the original offers in battle too: Word of Recall (takes a unit out of the fight), Disenchant True and Disenchant Area as battle dispels (the importer keeps them overland-only until then). Black Sleep, Confusion and Creature Binding also need a per-unit combat state (cannot act, random actions, changes sides). (2) The dispelling of battlefield enchantments is task 161, the node counter and auras task 160. (4) The **hero's own casting** and the units' spell-like abilities (Doom Bolt, Web, Fireball, Healing from a unit's `casts_...` abilities) with their mana (`BattleUnit::mana` exists now, for the spell shots of casters; units cast nothing yet), and the **items' spells** (task 49); the units' power drain (task 157). (5) Hit points from **Lionheart** (and Black Channels' hit points) are only counted during the fight; damage taken above the unit's real maximum is capped when the battle is written back. (6) The AI wizard casts nothing (task 46). (7) A spell animation and sound (art not extracted); the client shows the log line only. (8) **Undead** in the spells that treat them differently (Holy Word, Dispel Evil -9, Wraith Form), with task 29. (9) **True Sight** (cancels invisibility; the Invisibility spell is in, True Sight is not) and "an invisible unit is seen when it is next to an enemy".

**160. Battlefield node counter and node auras.** A battle fought on a magic node (the battle map's centre square holds a node of the sorcery, chaos or nature realm; `Combat_Cast_Spell` reads `battlefield->center_square_structure`) works against spells of other realms: every spell the wizard casts that is not of the node's realm must first beat a dispel of strength 50 against the mana put in (`Combat_Spell_Dispel_Attempt(50, ...)`), unless the caster has Node Mastery; a stopped spell still costs its mana and the turn, like Counter Magic. The node also gives the auras (help 615 to 617): +2 attack and +2 defence to creatures of the node's realm (the original's `Calc_Battlefield_Bonuses`), which fits the `FieldBonus` of `battle/field.rs`. Needs the battle to know the node under it (`Game::start_battle` has the site; `nodes.rs` has the node's realm and the owner's melding) and a way to show it. Hard to say which realm: the original reads the structure type; ours would use the node's kind. See `combat.md` sections 8 and 9.

**161. Dispelling battlefield enchantments.** Battlefield enchantments (`battle/field.rs`) cannot be dispelled: Dispel Magic aims at a unit, while the original's `Combat_Cast_Dispel` takes a square and works on the enchantments of the battlefield too, and the combat Disenchant spells (`Combat_Cast_Disenchant`) go for the field's spells only. Add what the dispel can reach: each enchantment the caster's enemy has up is tried at its dispel difficulty (`Game::dispel_difficulty`, the caster's masteries doubling it) with the strength of the mana put in (Disenchant True three times), a success removes it and `Battle::refresh_fields` puts the bonuses right. Decide how the player picks it (the unit's square, as now, plus the field?) and whether Counter Magic should be dispellable (the original's Disenchant True can), and add an event. `combat.md` section 9 lists it under "Not here".

**162. Weapon quality icons and help.** Task 18 gave units a weapon quality but nothing shows it. The original draws a magic, mithril or adamantium weapon icon on the unit in the army and stack views (`MainScr.c`, the `main_lbx` icons 55 and on, animated) and in the battle (`Battle_Unit_Melee_Attack_Icon`: the melee icon changes with the weapon), and the unit screen's help topics 386 (Mithril Weapon) and 762 (Adamantium Weapon) are there to open. Also the console's unit line should name the weapons.

**163. Wall of Stone in battle.** The spell is a combat spell too: `Wall_Rise` in `Combat.c` lets the defender raise the walls of its city during the battle (the original refuses it for an attacker or a city that has walls). Task 25 did the overland casting only (`city_spells.rs`: it builds City Walls). Needs the `Wall::Standing` sites made mid-battle (`BattleMap::with_city`) and the spell in `battle/spells.rs`; Disrupt (breaks a section) belongs with it (task 159).

**164. Heavenly Light's True Light.** The original gives the defender True Light in every battle at a city with Heavenly Light, as Cloud of Shadow gives Darkness (`Calc_Battlefield_Bonuses`); here Heavenly Light is only its unrest modifier. Task 25 did Cloud of Shadow (`Game::start_battle`); this is the mirror: raise `Field::TrueLight` for the defender when the city has `heavenly_light`.

**166. Book animations, the combat spell book and spell help.** What the spell books of task 93 leave out: the page-turn animation (`SmlBook_PageTurn`, `BigBook_PageTurn`, `book.lbx`), the animation that shows a learned spell (`Learn_Spell_Animation`), the combat spell book drawn on the same book picture (the battle screen has its own list), the help that a right-click on a spell opens, and the glow of the book's palette-cycled pixels (task 146). Also the Magic screen's gems: a wizard's portrait, relations text, treaty icons and what it is casting once diplomacy (tasks 63, 91) and portraits (83) exist.

**167. Diplomacy leftovers.** What task 63 left out of `diplomacy.md`: (1) **Threaten to Attack** and the AI's reaction to it (section 4); (2) **trespass**, `DIPL_GetOffMyLawn` (section 8), the warnings for units next to another wizard's city and the treaty broken on the third; (3) the relation after **battle losses** (`-diplomatic_value`, type 8, `Combat.c` 14557: the formula has not been read); (4) **city curses between two wizards are cancelled when peace is signed** (`Cancel_Players_City_Enchantments`); (5) the **threat of an army** is the whole army of each wizard, not the strength on the other wizard's home continent (`Invader_Army_Strength_Comparison`), and the "too many cities" reaction assumes a medium map (`LAND_SIZE`): keep the map size in the game; (6) a treaty **clears the unit orders** of the wizards in it (`Player_Clear_All_Unit_Orders`); (7) **AI-to-AI** treaty negotiations, `G_DIPL_PickSides`, `G_DIPL_NeedForWar` and the AI's own greetings, demands and war or peace offers (tasks 64 and 65; the core takes them as commands); (8) the AI should **not attack a wizard it has a pact or alliance with** (movement treats every other wizard as hostile; the attack now breaks the treaty). (9) **Client leftovers of the diplomacy screen** (task 91 is done without them): the fade-in and fade-out of the room (`diplomac` entries 38+); a right-click on a wizard's gem on the Magic screen opens that wizard's sheet; the confirm box "You have a treaty with X. Do you still wish to attack?" before an attack on a pact or alliance partner; and the readable colour of disabled menu rows. The talk sounds and music are in tasks 76 and 97.

**168. The original's diplomacy dialog as a local resource.** The wizards of task 91 speak lines of the game's own (`diplomacy_text.rs`); the original's are in `DIPLOMSG.LBX` and are its prose, so they may only be used locally, like the help text (`docs/local-assets.md`). Extract them with `scripts/import_mom.py` (or `f4x-tools`) into `content-local/diplomsg.ron`: entry 0 holds, for each of the situations (`diplomsg_0_record_number`, the original's `do_*` message numbers), a 6-byte record with the count of variants; entry 1 holds the variants, 15 to a situation and 200 bytes each (`Get_Diplomacy_Statement`, `DIPLOMAC.c`). The text has control codes (bytes above 127, `code & 0x7F` up to 20) that stand for the player's name, the wizard's name, a spell, gold or a treaty: decode them into placeholders and read how each is filled in. Then the client (`diplomacy_text.rs`) uses the original's line for a situation when the file is there, picking a variant at random as the original does (one fewer in a two-player game), and falls back to its own otherwise. Map each situation to what the core does (greeting by personality, accept, refuse, counter, threat, war, peace offer, tribute thanks, get-off-my-lawn, treaty broken; `diplomacy.md` section 9.5). The situations the game cannot yet reach (threats, trespass, demands: task 167) can wait. Gitignored output, as for the help text.

**172. Learning AI: battle AI follow-ups.** What tasks 170 and 171 left (`learning-ai.md` 7.7 and 7.8): (1) stronger yardsticks than the scripted AI, which random orders already beat: past checkpoints as opponents (the league of 6.4), Elo, and an evaluation as the defender; (2) the agent chooses **which unit** moves (a pointer over the units) and can cast later in the turn, not only at its start; (3) fleeing: it is found only with the help of a head that compares the value of fleeing with the critic's, and it still flees in fights it would win; (4) longer runs and a checkpoint chosen by its held-out score; (5) the AI wizards' real battles as training data (task 8.3), difficulty levels from checkpoints, and a client setting for `--battle-ai` (the client needs the `learned` feature and a checkpoint file); (6) the battle outcome model for the strategic AI (8.4); (7) a gap the first runs show: the odds classes were measured with the scripted AI, so a better player makes them easy. (8) The bank was made before the undead, experience and neutral-unit changes of 13 October, and no run has trained on **Animate Dead** (the agent can now aim it: the fallen have tokens); remake the bank and train again. Remaking the bank also picks up the 11 spells of eligibility 2 and 3 that are castable in battle now (task 55).

**174. Level bonuses of the original.** The `standard_xp` and `hero_xp` level paths give every level +1 resistance and, from the third, +10 to hit (`level_path_ron` in `import_mom.py`): a simple stand-in. The original's battle code (`Battle_Unit_Level_Stats`, `COMBINIT.c`) gives normal units: level 1 resistance +1 and melee/ranged +1; level 2 resistance +1 and defence +1; level 3 to hit +1, resistance +1, attacks +1; level 4 to hit, resistance, defence; level 5 to hit, resistance, attacks (attacks only when the unit has them). Heroes differ. Port them, including Ultra-Elite and Champion, and check the figures against the help text.

**175. Animate Dead: choosing the unit.** The original lists the wizard's fallen units and lets the player pick one (`Cast_Animate_Dead`); the client raises the strongest at once, since the fallen are not on the field to click. Add a list popup (name, figures, how it fell) opened by the spell. The original also leaves out units that fled or were never in the fight; we offer only dead units of the wizard's army, not heroes, summoned creatures or the destroyed (`Fate::Gone`).

**176. Mac installer.** One program (a script, or a small installer app) that sets the game up on a Mac from the player's own copy of the original: it asks for the path to the original files, runs the extraction (the art with `f4x-tools extract-art`, and the importer `scripts/import_mom.py` for the help text, wizards and city names into `assets-local/` and `content-local/`; `docs/local-assets.md`), checks that the files are the right ones and says what is missing, and then builds the local app (`scripts/make_app.sh` makes `target/Fantasy4X.app`). It should also say what it needs first (the Rust toolchain through rustup, the system Python) and offer to stop there instead of installing anything itself, report progress and errors in plain words, and leave the app and its data where the player can find them (perhaps copying the app to Applications). Nothing from the original is ever uploaded or committed; the result is for play on that machine only. Builds on task 81 (packaging for distribution, which carries no original files).

**183. A test that limits the methods on `Game` per file.** A test (or a script in `f4x-tools`) that counts the `impl Game` methods of each file in `f4x-core` and fails when a file goes above its recorded limit or a new file adds more than a few, with the limits kept in a small file in the repository. Lower the limits as 179 and 180 move methods out. Growth then shows up in review instead of by drift.

**184. The spell executor's host carries the `GameRng`.** A spell is resolved with a copy of the game's generator which the game takes back afterwards (`cast.rs`, `resolve_cast`), because the executor and its host (`GameHost`, which holds `&mut Game`) both need the game. A game method that rolls from the game's own generator while a spell resolves is rewound by the write-back (no hook is known to do it; see task 177 in `TASKS-DONE.md`). Make the host carry the `GameRng` (taken out of the game for the resolution, as diplomacy's is, task 179), so that the executor and the hooks draw from one stream and nothing is rewound. Check `dispel_target`, `raise_volcano` and the city-spell hooks, which take an `rng` argument today.

**185. Units follow the domain rules.** `units.rs` (17 methods), `temp_effects.rs` (13) and `experience.rs` (3) are the unit queries and the stat engine's entry points: `unit_def`, `unit_owner`, `unit_position`, `unit_abilities`, `unit_level`, `unit_modifiers`, `unit_stat`, `unit_hits`, `units_at`, `all_units`, `spawn_unit`, the temporary effects and the experience. They read only the world and the content (12 and 4 uses of those fields in `units.rs`; the other calls are to each other, to `serial` and to `undead`), but they are called from everywhere (movement 23 times, the turn 16, the battle copy and the AI), so this is the base the other domains stand on and goes first. Give them a read handle that borrows only what they use, such as `Units<'a> { world: &'a World, content: &'a Content }` with the queries as its methods (`Game::units()` makes one), and a small set of functions that change units (`spawn_unit`, `gain_xp`, the temporary effects) over `&mut World` and `&Content`. The `impl Game` queries remain as thin wrappers with the old names so the callers need not change at once; move callers over by area afterwards. What needs the owner's magic (Crusade, Warlord in `unit_level`) takes it as an argument or through `WorldView`. Done when the unit rules have no `impl Game` block beyond the wrappers and a unit's stats can be computed in a test with a world and content only. Rules and structure: `ecs-structure.md` section "Domains".

**186. Economy follows the domain rules.** `economy.rs` (12 methods, 657 lines) works out a city's yields (`city_economy`, `city_economy_with`, `city_modifiers`, `city_area`, `road_trade`, `site_max_size`, `city_min_farmers`, `city_building_upkeep`) from the land it works, its people, race, buildings and enchantments. It uses the world (17), the content (10) and the map (4), and calls the unit queries (3), city spells (1), temporary effects (1) and magic (1: the owner's Divine and Infernal Power). It is a pure query over a snapshot of the world, so the shape is a function or a small handle over `(&World, &Map, &Content)` with the owner's retorts as an input (`CityEconomy::compute`), not a method of the whole game. Do it after 185 (it asks the unit queries for the garrison and the troops that keep order). `city_economy` is the hot path of the turn, the AI and the client: keep it as fast as it is, or measure. Done when the yields can be computed in a test from a hand-made world without a `Game`, and the economy tests that need a whole game are only those that need units and magic.

**187. Production follows the domain rules.** `production.rs` (17 methods, 413 lines): what a city can build (`can_build`, `requirements_met`, `city_is_coastal`), what it costs (`item_cost`, `buy_cost`), how far it is (`production_remaining`, `city_production`), the commands (`set_production`, `buy_production`, `sell_building`), the finished item (`apply_production`, `city_weapon_quality`, `city_starting_xp`) and the Grand Vizier (`vizier_choice`, `vizier_fills_idle_cities`). It uses the world (13), the content (8) and the map (2), and asks the economy 7 times (the unit cost reduction, whether the city has people, the yields), so it follows 186. The checks ("can this be built?") are queries over the city and the content; the commands change the city's `Production` and `Buildings` components and the owner's gold, which comes back as an outcome (as diplomacy's gold does, task 179) or is done by the one caller that owns the treasury. Done when the checks and the costs run in a test from a world and content only, and `production.rs` has no `impl Game` block beyond wrappers.

**188. The turn code follows the domain rules.** `turn.rs` (1,589 lines, 15 methods, most of it tests) is the orchestrator by nature and should stay one: `process_turn` runs the cities, then each player's treasury, then healing, then the standing orders. What is not orchestration should move to its domain: unit food and gold upkeep to the units (185), city growth and outposts (`apply_growth`, `outpost_growth_chance`, `grow_outpost`) and `spawn_city` to the cities, taxes, farmers and the treasury (`balance_taxes`, `balance_farmers`, `treasury`, `settle_treasury`) to a finance domain over the players and the economy, and healing (`heal_units`) to the units. `Game::roll` is the old wrapper of the generator (task 177) and moves to `lib.rs` next to `next_u64`. Done when `turn.rs` only sequences the steps, each step is a call to a domain with the arguments it needs, and the order of the turn and the draws from the generator are unchanged (compare a soak before and after, as for tasks 177 to 180). Do it after 185 to 187. The other groups of `impl Game` methods are tasks 189 to 193 (movement with transport, the lairs, conquest, the fortress, vision); task 183's limits show which grow.

**189. Movement and transport follow the domain rules.** `movement.rs` (10 methods: `plan_step`, `move_stack`, `step_cost`, `check_stack`, `stack_traits`, `stack_flies`, `hostile_at`, `claim_tower`, `can_shift_plane`, `shift_plane`) and `transport.rs` (9: ships, passengers, room, `drown_stranded`). They use the world (9), the map (5), the content (3) and the fog (1), and lean on the unit queries (23 calls from movement, 9 from transport), so they follow 185. Movement is a stack's way over the map: what a step costs, whether it may be taken, what it meets (an enemy, a guarded site, a city), and the move itself. The plan: the checks (`plan_step`, `check_stack`, `step_cost`, `stack_traits`) become functions or a read handle over `(&World, &Map, &Content)` with the unit queries of 185, so a step can be planned in a test with a hand-made world; the move itself (`move_stack`, `shift_plane`, boarding and unloading) changes positions and moves left over `&mut World` and the fog's explored squares, and keeps reporting what it meets as an `Entry` (free, hostile units or city, a guarded site) for `Game` to act on (start a battle, enter a site, take a tower); check that nothing in it calls into the lairs or the battle directly (it calls the lairs once). `claim_tower` and the tower and site entries move with the lairs (190). Done when `plan_step` and `step_cost` run without a `Game`, `movement.rs` and `transport.rs` have no `impl Game` block beyond wrappers, and the pathfinding (`pathing.rs`) and the AI still get their paths from the same queries.

**190. The lairs follow the domain rules.** `lairs.rs` (1,382 lines, 704 of them not tests) has two halves. The generation (`generate_lairs`, `make_lair`, `pick_guards`, `fill_hoard` and the helpers) is already free functions over `(seed, params, &mut Map, &Content)`, which is the model. The play half is 7 methods on `Game`: `lair_at`, `enter_site` (guards made for a fight, the battle started), `collect_site`, `claim_lair`, `award_spell`, `award_special` and `finish_site_battle` (clearing or keeping the site, the loot, the guards removed, the undead). They use the world (3), the map (3), the content (3) and the battle (2), and call the magic code 7 times, the turn 5 times and the unit queries 4 times. The plan: the rules of what a site holds, what the loot is and what clearing does become functions over the site record and the content that return the loot and the changes as plain data (`Loot` already is), and `Game` applies them (a spell learned through `Magic::learn`, gold, items, specials). The battle start stays with `Game`, which owns the battle. Follows 189 for the entry; the magic side is task 180, which is finished. Done when the loot and the clearing can be tested without a `Game` and the lair tests that need a battle are the only ones that do.

**191. Conquest follows the domain rules.** `conquest.rs` (236 lines, 6 methods: `citizens`, `conquest_loot`, `conquer_city`, `destroy_building`, `raze_city`, `close_raze_windows`): what happens to a city that falls (fame and gold, the citizens and buildings the fighting costs, the choice to raze). It uses the world (11) and the content (1) and calls the economy 4 times, the fortress 4 times, the turn 2, diplomacy 1 and production 1. It is mostly arithmetic on a city's citizens, buildings and the two players' fame and gold, which makes it a good small domain: the rules take the city's figures and the two treasuries as inputs and return the changes (citizens lost, buildings lost, fame, gold looted, the owner change) as outcomes that `Game` applies, with the diplomacy reaction (`city_taken_relations`) as an event it raises. Follows 186 and 187 (the buildings), and goes with 192 (the fortress city). Done when the loss and the loot can be worked out in a test from a city's figures and the treasuries, and `conquest.rs` has no `impl Game` block beyond wrappers.

**192. The fortress, banishment and defeat follow the domain rules.** `fortress.rs` (704 lines, 274 not tests, 13 methods: `fortress_city`, `is_fortress_city`, `summon_point`, `place_fortress`, `move_fortress`, `move_summoning_circle`, `return_to_fortress`, `capture_city`, `banish`, `defeat`, `destroy_city`, and two small helpers). The fortress is a position on the `Player`; the rules decide where it stands, what a fallen fortress city does (banish the wizard, who casts the Spell of Return), and when a wizard is defeated (units dismissed, enchantments off, nodes freed, half the gold to the conqueror: task 82). It uses the world (8) and calls the game's player and city accessors in `lib.rs` 20 times, the economy 5 times, the turn 2, diplomacy 2 and the end of the game 2. Wizard defeat touches nearly everything (units, cities, nodes, diplomacy, the game over check), so it is an orchestration (`Game`) over steps that belong to other domains: the unit and city removal (185 to 187), the nodes, the diplomacy reaction (`conquest_relations`, task 179) and the end-of-game test. The plan: the decisions (is this the fortress city, where may it be placed, may the circle move there, is the wizard out of cities) become queries over the player and the view; the changes become a list of steps the game carries out in the original's order. Follows 185 to 187 and 191; do it with 82, which adds to the aftermath. Done when the decisions can be tested without a `Game` and the order of the aftermath is unchanged.

**193. Vision follows the domain rules.** `vision.rs` (217 lines, 81 not tests, 3 methods: `unit_visible_to`, `city_sight`, `visibility`): what a player sees now, as opposed to what it has explored (`fog.rs`, which is already free of `Game`). It uses the map (2) and the unit queries (8). `WorldView` (task 178) already asks `foreign_sightings` of it, and the contact rules of diplomacy run on that. It is small and a pure query, so it is the easiest of this group: a function over `(&World, &Map, &Content)` that returns the visible squares of a player and whether a unit is visible to a viewer, with the sight radius of a city (walls, Oracle, Nature's Eye) and Nature Awareness as inputs. Follows 185. Done when `visibility` runs from a hand-made world without a `Game` and the `WorldView` implementation calls it.
