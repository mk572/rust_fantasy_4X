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
| 68 | Meta | Random events |
| 71 | Meta | Advisors |
| 72 | Meta | Fame effects |
| 73 | Meta | Save-format migration |
| 74 | Meta | Stable command references for replays and multiplayer |
| 76 | Client | Background music |
| 78 | Content | Open conflict decisions (B9-B13, B15, B16, C3) |
| 81 | Housekeeping | Packaging for Mac |
| 82 | Meta | Wizard defeat aftermath and fortress display |
| 84 | Client | Analyse the original's screenshots and adjust the in-game graphics |
| 85 | Client | Import and play the original's opening movie |
| 87 | Magic | Remaining global enchantments (low priority) |
| 88 | Meta | Hall of fame |
| 89 | Meta | Game settings |
| 95 | Client | Tutorial |
| 97 | Client | Sound effects |
| 99 | Combat | Saving during a battle |
| 102 | Map | Road-building path selection in the client |
| 104 | Units | Ship tiers by building and ship movement estimates (low priority) |
| 106 | Combat | Battle views for the AI and for tools |
| 107 | Content | Check assumptions made while implementing |
| 108 | Magic | Magic nodes: warp spell, guardians' monsters and node visuals |
| 111 | Combat | Special battle sites: nodes, lairs, towers, ruins and fortress cities |
| 115 | Client | Cityscape: road masking |
| 116 | Client | Cityscape: animation, wards and rubble |
| 117 | City | Selling buildings from the city picture |
| 118 | Client | Bitmap fonts on every screen and a text layout helper |
| 119 | Client | City screen details |
| 120 | Client | Overland screen details |
| 121 | Map | Unexplored-edge pictures (corner-based exploration) |
| 123 | Map | Call the Void, Great Wasting and Armageddon corruption (low priority) |
| 124 | Map | Mines, lumber camps and hunter's lodges on the map (low priority) |
| 125 | Client | Unit stack backgrounds in banner colours |
| 126 | Client | Bitmap font for the remaining overland text |
| 127 | Map | Lair rewards: original spell, book and retort selection |
| 128 | Client | Site encounter dialogs and treasure popup |
| 130 | Client | Warped node and cleared site pictures (low priority) |
| 132 | Client | Battle screen: unit figures, animation and the original's frame |
| 133 | Client | Prompt before attacking, and battles between AI players |
| 134 | Client | Game-end cut-scenes and the original's score screen |
| 135 | Client | Run the AI wizards in the client |
| 136 | Client | Earth Gate and Astral Gate in the client and the AI (low priority) |
| 137 | Client | Teleport and merge animation in the battle screen (low priority) |
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
| 176 | Housekeeping | Mac installer: asks for the original files, extracts, builds a local app |
| 219 | Heroes | Hero roster and the hero component: per-wizard ability rolls, hire, die, six slots, save |
| 220 | Heroes | Turn-start offers: hero, mercenaries and merchant item |
| 221 | Heroes | Item loot and loss: fallen heroes' items, removal spells, lair items and prisoners |
| 222 | Heroes | AI and console for heroes and items |
| 223 | Heroes | Hero and item help conflicts: record H1 to H25 and settle the Check rows |
| 228 | Meta | Scripted AI steps 4 and 5: casting beyond summons, the win spell, special spells |
| 229 | Meta | Scripted AI steps 6 and 7: war targets, memory, defence, shared strength |
| 230 | Meta | Scripted AI step 8: ships and other land masses |
| 232 | Combat | Conventional battle AI steps 10 and 11: target values, focus fire, defence, fleeing |
| 233 | Combat | Sea battles: rules for fights on the ocean (ship combat, passengers, terrain) |
| 234 | Combat | Attacking from a ship: can land units attack a city or stack from a boat? |
| 235 | Units | A ship lost in battle or by a spell: what happens to its passengers |
| 236 | Units | The nine-unit stack limit and ships: passengers and capacity |
| 238 | AI | Wizard objectives steering the scripted AI (design: `ai-objectives.md`) |
| 240 | Content | Check the original's terrain tables (`TERRSTAT.LBX`, `TERRTYPE.LBX`) against `terrain.ron` |
| 241 | Content | Check the lair and monster data (`MONSTER.LBX`) and the event tables (`EVENTS.LBX`) against `lairs.rs` and the event code |
| 242 | Content | Check the other text and list tables the importer skips (`DESC`, `BUILDESC`, `LISTDAT`, `RESOURCE`, `EVENTMSG`, `DIPLOMSG`, `MESSAGE`) |
| 243 | Content | Find out what `WIZARDS.LBX`, `SPECIAL.LBX`, `SPLMASTR.LBX` and the other unread data files hold |
| 244 | Heroes | Work out what the "books" bytes of an item mean |
| 245 | Housekeeping | Delete the old `~/reference` folder (user) |
| 248 | Meta | Game settings, stage 3: the advanced screen and the easy custom settings |
| 249 | Meta | Game settings, stage 4: no Myrror, hardcore mode, larger stacks and other hard settings |
| 251 | Magic | Check where the AI mana bonus applies: fortress and volcano power |
| 252 | Magic | Check and implement the starting spells each wizard gets in a new game |
| 253 | Map | Lakes with an inflow and the original's rewriting of extra river inflows |
| 254 | Combat | Battlefield leftovers after the field generator and art (110, 112) |
| 255 | Combat | River terrain on the battlefield |
| 256 | Combat | Unit spell-like abilities: Fireball, Healing, Doom Bolt and Web spells |
| 258 | Combat | Summon Demons (Demon Lord) |
| 259 | Combat | Bug fix: path abilities and flight in battle movement |

## Descriptions

### Combat

**39. Auto-resolve.** Done: `Command::AutoResolve` (console `auto`) fights a battle to the end with the placeholder battle AI (`battle/ai.rs`) playing both sides, from an adjacent square, with the usual aftermath. Open: the strategic-combat-only option [685], a button for it on the battle screen (the Auto button is drawn greyed; task 133), and a better battle AI (46) so the result is not just whoever the placeholder favours.

### Heroes and items

**47. Hero abilities.** 13 hero abilities with levels and "super" versions [515-526]; data exists
as imported text, the effects mostly do not. Low priority: only needed for specific spells, buildings, units or abilities. Armsmaster itself is in the engine (task 27): give the heroes the `armsmaster` and `super_armsmaster` ability ids. Design: [`heroes-items-design.md`](heroes-items-design.md) section 2.3 (the formulas) and 3.6; step 5 of its section 9. The abilities are rolled per wizard and hero type (task 219) and carried by the hero, not by unit definitions.

**48. Hiring, summoning, recall, resurrection and fame.** Hero availability by fame, hiring cost,
the noble, charmed and lucky traits, Summon Hero and Summon Champion, Recall Hero, dead hero
tracking and resurrection [577-588, 240, 246]. Also: a Charismatic wizard pays half for hiring heroes and mercenaries and for buying items, and a Famous one has twice the chance of heroes and mercenaries offering themselves (task 53 left these here). Design: [`heroes-items-design.md`](heroes-items-design.md) sections 3.5 and 3.9, steps 4 and 6 (roster, hire, dismiss, Summon Hero, Summon Champion, Incarnation, Resurrection, Recall Hero). The offers each turn are task 220; the roster and the hero component are task 219.

**49. Item slots, powers and data schema.** Slots by hero type (sword, bow, staff, armor, misc),
item powers, charges, and how this is expressed in data (open question in `design.md` section 13).
Also: a hero's shield gives the hero a large shield in battle (the ability exists, `battle/game.rs`), and the items of a fallen hero are loot for the winner (the original lists them after the battle; `battle/spoils.rs` leaves them out). Design: [`heroes-items-design.md`](heroes-items-design.md) sections 3.1 to 3.4 and 5, steps 2 and 3: `items`, `item_powers`, `armory`, the six slot kinds and the item rules in the stat engine and the battle. The data import is task 218; the loot of fallen heroes is task 221.

**50. Enchant Item, Create Artifact, the vault and the anvil.** Crafting UI and rules (power budget
per book, mana cost, Artificer discount already in costs), the player's item vault, scrapping items
at the anvil. Design: [`heroes-items-design.md`](heroes-items-design.md) section 3.3, step 8 (cost formula, Enchant Item and Create Artifact, the pending item in `Magic`, random and prefab items, the anvil).

**51. Heroes' casting contribution and sage heroes.** Heroes add to casting skill and sage heroes
add research (level times 3, or 9 for the greater ability). The hooks (`Skill_Left` and the
research income) exist; heroes do not feed them yet. A hero's mana in battle (`(skill + 1) * (level + 1) * 5 / 2`, `BattleUnit::mana`) and so its spell shots (3 mana a shot) and own spells come with this. Low priority: only needed for specific spells, buildings, units or abilities. Design: [`heroes-items-design.md`](heroes-items-design.md) sections 3.6 and 5, step 10.

### Magic (first pass done; see `magic.md`)

**56. The 37 "special" spells.** Map reveals, terrain and mineral changes, healing a map square,
recalls, moving the fortress, resurrect and similar [mechanics-gaps section 4]. Add the missing
effect primitives (map reveal, teleport, grant movement type, building destruction, random-effect
tables) and implement them in groups. Low priority: only needed for specific spells, buildings, units or abilities.
First group done in `special_spells.rs`: Transmute (iron and gold, coal and gems, silver and mithril swap), Earth Lore
(explores a 12 by 10 block around the target tile; the original reveals the visible map window), Nature's Cures (heals the
whole stack on the unit's tile, except Death-realm and undead units), Word of Recall (to the summoning circle) and Plane
Shift (the whole stack to the other plane). Two known differences (recorded in `divergences.md` section 5): the original's Word of Recall evicts the weakest unit of
a full circle (`Evict_Weakest_Unit`), here a full circle makes the spell unavailable; and Plane Shift refuses a square
where any unit could not stand (the original has separate water rules, `Cast_Plane_Shift_Check`). Second group done in
`wizard_spells.rs`: Earthquake, Great Unsummoning, Death Wish, Drain Power, Cruel Unminding and Subversion (where help and code
disagree: `help-vs-implementation-conflicts.md` B30 to B33; Death Wish has no resistance penalty). Black Wind (a resistance roll at -1 for every figure of every unit
on the square, each failure costing one figure's hit points; the original's relations penalty for it is not applied: its call
is garbled) and Lycanthropy (the unit becomes Werewolves with no experience; ships are refused, the original does not) are in too.
The spells still to wire, with what each needs first, are in [`spells_to_implement.md`](spells_to_implement.md). Still open here: the AI casting any
of the new spells, and Fire Storm and Ice Storm.

**58. Unit enchantments needing non-stat effects.** 20 spells with movement types, immunities,
conditional bonuses and regeneration. **Done so far:** the movement spells grant the abilities the
movement code reads (`import_mom.py`, `UNIT_ENCHANT_MODS`): Wind Walking, Planar Travel, Water Walking
(new ability `water_walking`: the unit crosses water alone and needs no ship), Path Finding (new
ability `pathfinding`, which `movement.rs` already read) and Wraith Form (non-corporeal and weapon
immunity). **Left:** the spells still to do are in [`spells_to_implement.md`](spells_to_implement.md) (section B). Not checked: the AI's use of the five new spells. Needs conditional modifiers (by attacker realm, ranged versus
melee, creature realm), immunity flags and movement grants; depends on 30 and the combat tasks. Low priority: only needed for specific spells, buildings, units or abilities.

**60. Wizard-targeting, destruction and other special categories.** About 20 spells aimed at
wizards, resistable kills, destruction of buildings and similar; needs wizard-state changes (mana,
skill, fame, diplomacy) and spell ownership transfer. Most are done (task 56); the rest are in [`spells_to_implement.md`](spells_to_implement.md). Low priority: only needed for specific spells, buildings, units or abilities.

**61. The Spell of Mastery's research cost and the AI casting it (engine and AI done; announcement open).** Finishing the spell wins
the game and the Spell of Return works after banishment (tasks 70 and 15; `docs/game_end.md`).
Done (tasks 226 and 228): the research cost is the listed 60,000 less half the research cost of every spell known (the importer used to read it as a signed number and give 1), the spell is offered for research when nothing else is left, and the AI researches it when it is within reach, splits its power for it and casts it until it is paid (`ai-design.md` section 7).
Still open: the announcement when a wizard starts casting it (the player is told nothing today; `diplomacy.rs` already turns the other wizards against the caster).

**62. Player-chosen losses when mana runs short.** The engine drops the newest upkeep items
deterministically; the original asks the player. Add a prompt or a priority list, and the "mana
short" event interaction (68).

### Meta: diplomacy, AI, events, victory

**64. Wizard personalities and AI objectives.** Maniacal to peaceful personalities and the
pragmatist, militarist, theurgist, perfectionist and expansionist objectives that steer the AI
[271, 782-794]. See `docs/diplomacy.md`. **Done:** both are drawn at game start for every wizard
but the human, as the original's `Init_Magic_Personalities_Objectives` does (`objective.rs`: the
primary realm's weight row plus the retort bonuses, personality first, then objective; both stored in
`Diplomacy` and in saves; `Game::objective`, `Game::personality`). Personality already acted in
diplomacy (it was `Chaotic` for everyone before, so AI-against-AI games now differ). **Open:** what
the objective does. The original uses it for the AI's expansion rate (`AIMOVE.c`), building weights
(`AIBUILD.c`), magic investment (`AIDUDES.c`) and war declarations (`DIPLOMAC.c`); nothing reads it
here yet.

**68. Random events.** Plague, population boom, conjunctions, moons, mana short and others, each
ending with 5 percent chance per turn; switchable option [768-775, 683]. Follow intent for B18
(count from the real turn, not from game start or load).

**71. Advisors.** The original's advisors (surveyor, cartographer, historian, astrologer, chancellor,
tax collector, grand vizier, war and mage advisors) give information screens and messages about
the player's progress and surroundings [764, 216, 221]. Add an advisor menu and screens fed by
`Game` queries, plus the end-of-turn advice messages (a city starving, a spell learned, a hero
available).

**72. Fame effects.** Fame sources (heroes, Just Cause, Famous retort) and effects on unit upkeep
and hero availability [765]; follow the help for legendary heroes (+3 fame per level, B12). Design: [`heroes-items-design.md`](heroes-items-design.md) section 3.7: `fame::total` (earned fame, Legendary heroes, Just Cause) replaces the plain `Player.fame` in `finance.rs` and the offers.

**73. Save-format migration.** Version 1 is the only format; add a conversion step before
`restore` when the layout changes incompatibly. Only needed at the first breaking change.

**74. Stable command references.** Commands carry `hecs` entity ids, so a command log cannot
cross a load. If replays or multiplayer are wanted, switch commands to creation numbers (`Serial`).
Decide the multiplayer scope first (`design.md` section 13).

### Client, content, housekeeping

**76. Background music.** Play music in the client: a track per situation (title, map, city, battle,
victory and defeat, and the diplomacy screen's own tune), looping and crossfading, with a volume setting (94). The original's music
can be extracted locally for the author's own copy (like the art, under `assets-local/`; the
repo ships none); decide the audio crate, and keep the game silent and working without files.

**78. Open conflict decisions.** Items still marked "Check" or open in
`help-vs-implementation-conflicts.md`: B9 Heroism, B10 undead immunities, B11 Evil Omens cost,
B12 legendary fame (decided, not built), B13 Blur, B15 swamp food, B16 nature node food, C3
Invisibility. Decide each, apply to content and help, and add the "[Updated from game code]" note
where the help changes.

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

**87. Remaining global enchantments.** The global spells still without effect are listed in [`spells_to_implement.md`](spells_to_implement.md)
(section F), with what each needs. This task keeps the shared machinery: the engine reads them from the wizard's list (`globals.rs`); some need
battle code (Eternal Night, Zombie Mastery, Nature's Wrath, Time Stop's combat part: 33-46), the corruption layer, a condition on movement type
(Wind Mastery), the mutation of new units (Doom Mastery: temporary effects store 30) or the dispel mathematics (Life Force, Tranquility: 59).
Holy Arms' magic weapons are not done either. Low priority: only needed for specific spells, buildings, units or abilities.

**88. Hall of fame.** The list of best finished games with wizard, race, score, turns and
difficulty [628-641, 798], stored outside the saves (for example `saves/hall_of_fame.ron`),
shown from the start menu (86) and after a game ends (victory and score, 70). Needs 70 first.

**89. Game settings.** The options screens of the original [675-692]: automatic saving, event
switches, strategic-only combat, animation and message speeds, and so on. Store them as a
settings record, `serde(default)` and saved with the game where they change rules (events,
Spell of Return option from `PLANNED.md`), and in a client configuration file where they only
change presentation. Reachable from the start menu (86) and in game. The rules half is designed in [`game-settings.md`](game-settings.md) (draft); built in tasks 246 to 249.

**95. Tutorial.** A first-game guide: highlighted steps for moving, founding a city, building,
researching and casting, with a way to skip or switch it off.

**97. Sound effects.** Short sounds for UI clicks, turn end, city growth, spell casting, combat
hits and deaths, and the wizard's spoken lines where the original has them (the diplomacy screen's greetings and answers among them), played through the
same audio layer as the music (76) with its own volume, from locally extracted files. Events
(`Event` values from `Game::apply`) are the trigger points, so the game core stays free of audio.

**99. Saving during a battle.** A battle in progress is not part of a save today (`Game::battle`
is skipped, so saving mid-fight would lose it, and the client should block saving). Make
`Battle` serialisable: units with their statistics, cells, turn, active side, and the random
generator state (seed plus draw count, as for the game), plus the list of overland units the
fighters were copied from (by creation number). Restore it on load, check it for consistency
(units on the map, sources still alive), keep old saves loading, and test that a save in the
middle of a battle continues exactly like the unsaved one.

**102. Road-building path selection in the client.** The B key builds a road only on the square under the engineers. Add the original's road-building mode (topics 510-512): click a path, show valid squares in blue and invalid in red with the turns it will take, OK and Cancel buttons, using `find_path`-style planning and `Command::BuildRoad` with the whole path.

**104. Ship tiers by building and ship movement estimates.** Which buildings allow which ship (ship wright's guild, shipyard, maritime guild: help 725-727) is only encoded as unit requirements in the data. Verify it for every ship, show the capacity in the unit and city screens, and make path-turn estimates (`turns_for_path`) use the ship's speed instead of the slowest passenger's. Low priority: only needed for specific spells, buildings, units or abilities.

**106. Battle views for the AI and for tools.** Task 33's model has no read-only summary an AI can search over: add cheap clone and undo of a `Battle`, a list of legal commands for a unit, and an evaluation helper (remaining hit points and strength per side), tested against random play. Prerequisite for the battle AI (46) and for auto-resolve (39).

**107. Check assumptions made while implementing.** Decisions taken without a source, to confirm against the original or the help: Evil Omens spares the wizard who holds it; the original's own Lo Pan spends 12 picks (presets skip the budget); the decompiled thrown and breath attack code looks broken, so thrown and breath attacks use plain strength with no range penalty; melee against fliers deals nothing from walkers; roads on nodes take 4 or 5 turns whatever the terrain; Enchant Road is permanent; melding spirits are used up even when the meld fails; the 100-turn battle limit is our own. Write each one into `help-vs-implementation-conflicts.md` with the decision.

**108. Magic nodes: warp spell, guardians' monsters and node visuals.** Warped nodes exist as a flag but nothing sets it (Warp Node, task 7); unowned nodes should be guarded by monsters before they can be melded (task 8); the client draws the aura as a tint only: add the original's node terrain art and a node info panel (power, owner, guardian, warped).

**111. Special battle sites: nodes, lairs, towers, ruins and fortress cities.** The original fights node, lair, tower, temple and ruin battles on their own battlefields (node type changes the ground and effects, `Combat_Node_Type`; lairs and keeps have structures; a wizard's fortress city has its own layout). Add battle map variants and their effects on movement and combat, fed from the overland feature under the defenders; needs guardians (8) and nodes (108).

**115. Cityscape: road masking.** The city picture draws the whole road grid; the original hides the road squares that no building, house or tree touches (`Cityscape_Roads_1` to `_5` in ReMoM `CITYSCAP.c`, using the road pictures and the two masks `cityscap.lbx` 109 and 110), so roads appear only where the city has grown. Port the masking rules.

**116. Cityscape: animation, wards and rubble.** Animate the city picture as the original does: building frames (a counter that cycles nine frames), the water overlay, the wall, and the build-in effect when something is newly built. Draw the ward and enchantment icons along the bottom (death, chaos, nature, life and sorcery wards, Nature's Eye, Inspirations, Prosperity, Consecration), Wall of Fire, Wall of Darkness and the wall sprites, Cloud of Shadow, Heavenly Light and Chaos Rift backdrops, Evil Presence over religious buildings, and rubble where a building was destroyed.

**117. Selling buildings from the city picture.** Clicking a building in the city picture offers to sell it (help topic 303): a confirmation with the gold it brings, then `Command::SellBuilding`. The command exists; the screen has no way to use it, and the prerequisite chain check is task 17.

**118. Bitmap fonts on every screen and a text layout helper.** Only the map frame and the city screen use the original's fonts. Move the magic, wizard creation, production menu, message log and unit panels to them, and add a bitmap-font word wrap, justify and column helper (`Print_Paragraph` style) so long help text and descriptions can be shown.

**119. City screen details.** What the original's city screen has and ours lacks: the production picture for buildings (the cityscape sprite) and a count of queued items, the unit list scrolling past nine units and clicking a unit to select or unload it, right-click help on every element (the original's help topics 295-303 are imported), the enchantment list scrolling and dispelling, the city name editing, and the worked-area view with the original's tile and mineral pictures and food and production figures for each square.

**120. Overland screen details.** What the original's main screen has and ours lacks: the unit panel in the right column with the stack's pictures and the Done, Patrol, Wait and Build buttons, the minimap's other plane switch and click-drag, the surveyor and the info menus behind the Info button, the Armies and Cities lists behind their buttons (tasks 71 and 91-96 cover the screens themselves), terrain and unit animation, and unit movement as a smooth slide.

**121. Unexplored-edge pictures.** The original keeps exploration per square corner and draws `mapback.lbx` 0 to 13 along the border of explored land so the edge is soft. We explore whole squares and draw black. Needs corner exploration in the fog data (with saves) and the 14 edge masks.

**123. Call the Void, Great Wasting and Armageddon corruption.** The corruption layer, Corruption, Purify, Land Corruption and Gaia's Blessing exist; the global spells that corrupt land each turn (and the Great Wasting and Armageddon looks) are still to wire to it (the spells themselves: [`spells_to_implement.md`](spells_to_implement.md)). Low priority: only needed for specific spells, buildings, units or abilities.

**124. Mines, lumber camps and hunter's lodges.** `mapback.lbx` 87 (mine), 88 (lumber camp), 90 (hunter's lodge) are extracted but nothing in the base game's data places them; check whether the original used them (they may be Caster of Magic's) and, if so, move to PLANNED item 4. Low priority: only needed for specific spells, buildings, units or abilities.

**125. Unit stack backgrounds.** Draw each stack's marker on the `mapback.lbx` 14 to 19 background of its owner's banner (blue, green, purple, red, yellow, neutral) instead of the current icon.

**126. Bitmap font for the remaining overland text.** The gold, mana, income panels and city names use the original fonts; still in the plain font are the turn/plane/race line, the selection and hover info (`Lines` helper), the message log, the "F1 keys" hint and key list, and the unit-count badge on stacks. Give `Lines` a bitmap-font mode (or replace it) and move these over, using the original's sizes and colours; see task 118 for the other screens.

**127. Lair rewards: original spell, book and retort selection.** We follow the intent of
`WIZ_RollSpellReward` and `EZ_SpecialTreasure` (ReMoM marks both as muddled): spell of the tier
from a realm without books, then a book (75%) or retort. Compare against the original's results over
many rolls and adopt its exact odds where they differ. Also: Famous gives +10 fame; Alchemy or
Warlord counting as "arcane" in the realm check (ReMoM bug note); prisoner heroes and magic items need
tasks 48 and 49 (the `Loot` already carries their values); the 12-book and 6-retort limits. Design: [`heroes-items-design.md`](heroes-items-design.md) step 7 (task 221 does the items and the prisoner hero).

**128. Site encounter dialogs and treasure popup.** The client should answer `SiteEncounter` with the
original's confirm box ("You have found a ruins.  Scouts have spotted 4 Skeletons within the ruins.
Do you wish to enter?", node pictures per realm, Y/N), send `EnterSite`, and after a win show
"Inside you find ..." with the gold, mana, items, prisoner and spell lines. Unguarded sites pay out
on arrival without a prompt. The fight itself uses the battle screen (done); the prompt is what is missing.

**130. Warped node and cleared site pictures.** A warped node is drawn with a purple tint placeholder;
use `mapback.lbx` 93 (the warped-node mask). Check the original for any picture left behind by a
cleared site (the original just stops drawing the lair) and for nodes that are guarded versus cleared. Low priority: only needed for specific spells, buildings, units or abilities.

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

**139. City list screen.** `CityList_Screen`: all cities in a table (size, gold, production, what is being built) with a small map; opens the city screen or the production menu. The Cities button of the main screen.

**140. Unit statistics window and the enemy stack list.** **Done:** the sheet (`unit_sheet.rs` builds it, `unit_screen.rs` draws it, following ReMoM `Unit_Statistics_Popup`/`Thing_View_Draw`): the unit's figures on their diamond (grass or water), the name (a race's unit with the race first), moves with the movement icon, upkeep icons or (in battle) the front figure's damage, five icon rows (melee by weapon quality, range by attack type, armour, resistance, hearts; gold icons for bonuses, darkened ones for curses and damage), and the abilities list, eight to a page with the up and down arrows (level with experience, quiver, to hit, construction, abilities with the original's icons, undead, lasting spells), Dismiss (with a yes/no, `Command::DismissUnit`; not heroes) and OK on the map. Opened by a right-click on a unit's line in the selected-stack text or on a unit in the battle; `--sheet N` for screenshots. **Left:** (1) `Unit_List_Window`, the read-only list of another wizard's stack. (2) The help pop-up of an ability or spell (right-click on its line) and the click that removes a lasting spell cast by you (`UV_Remove_Unit_Enchantment`, with its confirmation). (3) Heroes: portrait, items row and item powers, hero abilities (tasks 218 to 221, 47); Dismiss for heroes (task 48). (4) The sheet from the city garrison is done (a right-click on a garrison unit; `garrison_cells` in `city_screen.rs`); from the Army list and the item screen it comes with those screens (tasks 138 and 142, which do not exist yet); the production screen's version (cost, upkeep for a unit not yet built), and the expanding-box opening. (5) The battle's sheet is a snapshot taken on opening and sits on a plain background instead of over the field. (6) Scouting, Summon Demons use count, mana left (`Caster 12 (20) mp`) and Warp Creature's three icons, once those exist. (7) The hits row shows hit points of one figure; the original also draws it so (the figure count is the pictures on the diamond).

**141. Outpost screen, enemy city screen and the naming boxes.** `Outpost_Screen` (a small city sheet for an outpost; also names a newly founded one), `Enemy_City_Screen` (what is known about another wizard's city) and the rename boxes for the capital (first turn) and new outposts.

**142. Item screen and the item pop-up.** `Item_Screen` (move magic items between hero slots and the wizard's vault, destroy one) and `Item_View_Popup`. Needs the item rules of tasks 49 and 50. Design: [`heroes-items-design.md`](heroes-items-design.md) section 3.4 (the `armory` API this screen drives).

**143. Offer, hire and event pop-ups.** `Merchant_Popup`, `Hire_Merc_Popup`, `Hire_Hero_Popup`, `Hero_LevelUp_Popup` and `Show_Event_Message`: the offers made at a turn's start (chance rising with fame, `Determine_Offer`), hero hiring, level-up announcements and the random-event announcements. Needs 48 (heroes), 68 (events) and 72 (fame). Design: [`heroes-items-design.md`](heroes-items-design.md) section 3.8 (task 220 makes the offers; this task shows them). Hero names are placeholders until the original's are imported (task 218): the hire box names the hero, with the unit's own name as the default.

**144. Turn summary and message pop-ups.** The Chancellor's scroll after a turn (random events, units disbanded, units killed, city growth, city deaths), the building-complete message, the evict message, the retreat-loss message, spell-cast banners, and a common yes/no confirmation box with the original's questions, including the warning before ending a turn that units will die for lack of food. Today events go to a log list.

**146. Palette cycling: the pulsing colour of index 198.** The original animates by cycling palette index 198 (`Cycle_Palette_Color` in ReMoM `Fonts.c`): between dark and bright red (40..63 in the red channel) on the main screen, load screen and start menu, black to red (0..55) in combat, and pale greys or blues in the spell book (`Spellbook.c`). `extract-art` bakes one fixed colour for that index into the PNGs, so the effect is missing. Find which pictures use index 198 (scan every picture's frames), write those pixels as a mask beside the picture (as the banner masks are written), and have the client tint the mask with a colour that moves between the original's two colours at the original's rate (the step values are 1 to 11 a call). Document the result in `docs/extract-art.md` section 4. Other palette effects (fades, the ghost remap) are separate.

**147. Find the use of the unreferenced art files.** ReMoM's loader has no reference to four LBX files that `extract-art` extracts fine: `combat`, `desert`, `tundra` and `cmbtwall`. `docs/extract-art.md` lists them as unknown rather than guessing. Find out what they are for: look at the pictures (frame counts, sizes, which terrain or battle element they resemble), search the original's disassembly and the other reverse-engineering sources in `reference/` for their names or for the loader slots they would occupy, and compare with the battlefield terrain and wall code (`cmbtwall` may be wall pieces beside `citywall`; `desert` and `tundra` may be terrain variants or battlefield backgrounds). Record the result in section 9 of `docs/extract-art.md` (replace the "unknown" row) and, if one is a real gap in the client, add a task for it. If nothing can be found, say so in the doc and leave the files unused.

**148. Difficulty: the AI's strength, and the seed field.** The original's difficulty effects on the world, the computer wizards' bonuses and the score are in (task 247, `game-settings.md`). Still open: let the difficulty set how well the new AI plays once it exists (the AI reads `settings.basic.difficulty`, not the bonuses), and add a map-seed field to the settings screen (the seed is a command-line option now).

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

**155. Starting setup: the original's random placement and AI race rules.** Task 32 brought the start in line with the original's `Generate_Home_Cities` (capital of four with three farmers, spearmen and swordsmen, smithy, barracks and builders hall, tax index 2, Myrran wizards on Myrror, sites that feed eight). Left out: (1) the original picks fortress sites at random (1000 tries per distance, fortress distance 16 relaxing to 10, node/tower/lair distance 7 relaxing to 1, halved in the check) while ours takes the nearest fitting site to a spread-out anchor, and measures every distance with `Range` where the original uses the wrapping delta for fortresses, nodes and lairs; (2) AI races: up to normal difficulty any of the nine Arcanus races, above it only Klackon, High Elf, Halfling, High Men and Nomad (a settings switch, `wizards.ai_race_limit`, in task 248), and a High Elf capital is moved off non-forest only by accident in the original; (3) starting gold: done in task 247 (the human gets `(5 - difficulty) * 25`, every AI 150, as settings); (4) fame 10 for the Famous retort (check it is set). Low priority.

**157. Special attack leftovers.** Task 36 did armour piercing, chaos (doom) damage, illusion, poison, life steal, stoning and death touch, destruction, dispel evil and the gazes. Done since: cause fear (`battle/attack.rs` `feared_figures`; row B34 in `help-vs-implementation-conflicts.md`); **extra hit points from life steal** (`DefenderState::heal_overflow`: what is left once every figure is whole becomes one more hit point per figure for each figure's worth, `BattleUnit::extra_hits`, which the damage written back ignores, and which end with the battle; the original's `Battle_Unit_Heal` also adds them when the front figure is still damaged by more than the figure count, which looks like a bug and is not copied); the **damage types** (`Damage::drained` and `gone`, `BattleUnit::fate`, `heal_points`, `heal_kinds`): life steal and create-undead damage leaves the victim to the winner as undead; what destruction, stoning (touch and gaze), dispel evil or a severing spell kills is gone for good, and healing (`BattleUnit::healable`) never brings back damage that destroys, only the ordinary damage first and then the drained, with the damage tally falling as it heals; death touch and gaze kills heal like any other. Left: (1) **Power drain** (the target resists or loses 2 to 20 spell points): in the original only an item's power has it (no unit uses it), so it waits for items, task 49. (2) Attacks that come from an **item or spell** rather than the unit: the original gives them an extra penalty (stoning touch -1, death touch and life steal -3) and halves doom damage; the unit's own count in full. Needs the source of an ability to be known (task 30's store, items task 49). (3) The **Life Drain spell** as the third damage type (drained, healing the caster), with task 159. (4) Fear from ranged and gaze attacks is not rolled (the original does not either).

**158. Siege leftovers: magic walls, the AI's raze decision, diplomacy.** Task 41 did city battles with walls and a gate, wall crushers, the defence bonus, city damage, loot, fame, losses and razing. Left: (1) the **battlefield spells** Wall of Stone (raises a wall in battle: task 163) and Disrupt (breaks a section); Wall of Fire, Wall of Darkness and Flying Fortress are done (task 25). (2) The **AI's decision to raze**: the original's `Raze_Check` scores the wizard's personality, how few troops it has left to garrison the city, and its units on the continent against the loser's; it needs an AI player (design.md section 8: the decision lives in `f4x-ai` and arrives as `RazeCity`). (3) *(Diplomacy, done with task 63: a city that holds costs the attacker -20, or -60 for the fortress city, and one taken -40 and a war.)* (4) **Monsters and raiders** taking a city: the neutral attackers' extra losses (50 percent more citizens, 50 more building chance) and the rampage that can leave ruins (with task 154). (5) The **city damage from spells** such as Magic Vortex (5 for each turn spent in the city). (6) The wizard whose fortress falls to a razed city: check the banishment and the summoning circle when the capital is razed (the original changes the owner first and then destroys the city, as here).

**159. Casting leftovers.** Tasks 43 and 44 did the wizard's casting in battle and the battlefield spells (`combat.md` sections 8 and 9), and part of this list: Word of Death, Petrify, Animate Dead (the wizard's own dead, not heroes or the destroyed; the client lists the fallen: task 175), Dispel Evil, Banish, Disintegrate, Holy Word, Death Spell, Flame Strike, Mass Healing, Call Chaos (one of its eight outcomes not yet: Chaos Channels), Raise Dead (a dead normal unit of the wizard's, not destroyed outright, returns with half its figures rounded up and without its enchantments; the client lists the fallen: task 175), Warp Wood, Shatter and Warp Creature (all three are enemy-unit spells here; normal units only; Warp Creature picks one of its three harms for the whole unit and lasts to the end of the battle, as the user confirmed), the curses Weakness, Vertigo and Mind Storm, and the Invisibility spell. Left: (1) the battle spells whose effects are not in the game, which are not offered yet: listed with what each needs in [`spells_to_implement.md`](spells_to_implement.md) (sections C and D), plus **Life Drain** (a resistance roll worth the extra mana; it heals the caster). Life Drain, Wraith Form and Heroism are imported with effects, so they are not on that list; their battle part has not been checked. Also the spells of eligibility 4, which the original offers in battle too: Word of Recall (takes a unit out of the fight), Disenchant True and Disenchant Area as battle dispels (the importer keeps them overland-only until then). Creature Binding (fantastic creatures, resistance -2) and Possession (normal units, not heroes; resistance penalty 0, which the help does not give) are in: the unit changes sides until the battle ends or, for the binding, a dispel, then (Possession) goes home; a creature still bound when the battle ends is destroyed (the original's wording, given by the user). The dispel path and the hero refusal have no test. (Black Sleep, Confusion, Web and Berserk are in; Web has no hit points of its own to break yet.) (2) The dispelling of battlefield enchantments is task 161, the node counter and auras task 160. (4) The **hero's own casting** and the units' spell-like abilities (Doom Bolt, Web, Fireball, Healing from a unit's `casts_...` abilities) with their mana (`BattleUnit::mana` exists now, for the spell shots of casters; units cast nothing yet), and the **items' spells** (task 49); the units' power drain (task 157). (5) Hit points from **Lionheart** (and Black Channels' hit points) are only counted during the fight; damage taken above the unit's real maximum is capped when the battle is written back. (6) The AI wizard casts nothing (task 46). (7) A spell animation and sound (art not extracted); the client shows the log line only. (8) **Undead** in the spells that treat them differently (Holy Word, Dispel Evil -9, Wraith Form), with task 29. (9) **True Sight** is in (it grants illusion immunity, which cancels invisibility); still open: "an invisible unit is seen when it is next to an enemy".

**160. Battlefield node counter and node auras.** A battle fought on a magic node (the battle map's centre square holds a node of the sorcery, chaos or nature realm; `Combat_Cast_Spell` reads `battlefield->center_square_structure`) works against spells of other realms: every spell the wizard casts that is not of the node's realm must first beat a dispel of strength 50 against the mana put in (`Combat_Spell_Dispel_Attempt(50, ...)`), unless the caster has Node Mastery; a stopped spell still costs its mana and the turn, like Counter Magic. The node also gives the auras (help 615 to 617): +2 attack and +2 defence to creatures of the node's realm (the original's `Calc_Battlefield_Bonuses`), which fits the `FieldBonus` of `battle/field.rs`. Needs the battle to know the node under it (`Game::start_battle` has the site; `nodes.rs` has the node's realm and the owner's melding) and a way to show it. Hard to say which realm: the original reads the structure type; ours would use the node's kind. See `combat.md` sections 8 and 9.

**161. Dispelling battlefield enchantments.** Battlefield enchantments (`battle/field.rs`) cannot be dispelled: Dispel Magic aims at a unit, while the original's `Combat_Cast_Dispel` takes a square and works on the enchantments of the battlefield too, and the combat Disenchant spells (`Combat_Cast_Disenchant`) go for the field's spells only. Add what the dispel can reach: each enchantment the caster's enemy has up is tried at its dispel difficulty (`Game::dispel_difficulty`, the caster's masteries doubling it) with the strength of the mana put in (Disenchant True three times), a success removes it and `Battle::refresh_fields` puts the bonuses right. Decide how the player picks it (the unit's square, as now, plus the field?) and whether Counter Magic should be dispellable (the original's Disenchant True can), and add an event. `combat.md` section 9 lists it under "Not here".

**162. Weapon quality icons and help.** Task 18 gave units a weapon quality but nothing shows it. The original draws a magic, mithril or adamantium weapon icon on the unit in the army and stack views (`MainScr.c`, the `main_lbx` icons 55 and on, animated) and in the battle (`Battle_Unit_Melee_Attack_Icon`: the melee icon changes with the weapon), and the unit screen's help topics 386 (Mithril Weapon) and 762 (Adamantium Weapon) are there to open. Also the console's unit line should name the weapons.

**163. Wall of Stone in battle.** The spell is a combat spell too: `Wall_Rise` in `Combat.c` lets the defender raise the walls of its city during the battle (the original refuses it for an attacker or a city that has walls). Task 25 did the overland casting only (`city_spells.rs`: it builds City Walls). Needs the `Wall::Standing` sites made mid-battle (`BattleMap::with_city`) and the spell in `battle/spells.rs`; Disrupt (breaks a section) belongs with it (task 159).

**164. Heavenly Light's True Light.** The original gives the defender True Light in every battle at a city with Heavenly Light, as Cloud of Shadow gives Darkness (`Calc_Battlefield_Bonuses`); here Heavenly Light is only its unrest modifier. Task 25 did Cloud of Shadow (`Game::start_battle`); this is the mirror: raise `Field::TrueLight` for the defender when the city has `heavenly_light`.

**166. Book animations, the combat spell book and spell help.** What the spell books of task 93 leave out: the page-turn animation (`SmlBook_PageTurn`, `BigBook_PageTurn`, `book.lbx`), the animation that shows a learned spell (`Learn_Spell_Animation`), the combat spell book drawn on the same book picture (the battle screen has its own list), the help that a right-click on a spell opens, and the glow of the book's palette-cycled pixels (task 146). Also the Magic screen's gems: a wizard's portrait, relations text, treaty icons and what it is casting once diplomacy (tasks 63, 91) and portraits (83) exist.

**167. Diplomacy leftovers.** What task 63 left out of `diplomacy.md`: (1) **Threaten to Attack** and the AI's reaction to it (section 4); (2) **trespass**, `DIPL_GetOffMyLawn` (section 8), the warnings for units next to another wizard's city and the treaty broken on the third; (3) the relation after **battle losses** (`-diplomatic_value`, type 8, `Combat.c` 14557: the formula has not been read); (4) **city curses between two wizards are cancelled when peace is signed** (`Cancel_Players_City_Enchantments`); (5) the **threat of an army** is the whole army of each wizard, not the strength on the other wizard's home continent (`Invader_Army_Strength_Comparison`), and the "too many cities" reaction assumes a medium map (`LAND_SIZE`): keep the map size in the game; (6) a treaty **clears the unit orders** of the wizards in it (`Player_Clear_All_Unit_Orders`); (7) **AI-to-AI** treaty negotiations, `G_DIPL_PickSides`, `G_DIPL_NeedForWar` and the AI's own greetings, demands and war or peace offers (tasks 64 and 65; the core takes them as commands); (8) the AI should **not attack a wizard it has a pact or alliance with** (movement treats every other wizard as hostile; the attack now breaks the treaty). (9) **Client leftovers of the diplomacy screen** (task 91 is done without them): the fade-in and fade-out of the room (`diplomac` entries 38+); a right-click on a wizard's gem on the Magic screen opens that wizard's sheet; the confirm box "You have a treaty with X. Do you still wish to attack?" before an attack on a pact or alliance partner; and the readable colour of disabled menu rows. The talk sounds and music are in tasks 76 and 97.

**168. The original's diplomacy dialog as a local resource.** The wizards of task 91 speak lines of the game's own (`diplomacy_text.rs`); the original's are in `DIPLOMSG.LBX` and are its prose, so they may only be used locally, like the help text (`docs/local-assets.md`). Extract them with `scripts/import_mom.py` (or `f4x-tools`) into `content-local/diplomsg.ron`: entry 0 holds, for each of the situations (`diplomsg_0_record_number`, the original's `do_*` message numbers), a 6-byte record with the count of variants; entry 1 holds the variants, 15 to a situation and 200 bytes each (`Get_Diplomacy_Statement`, `DIPLOMAC.c`). The text has control codes (bytes above 127, `code & 0x7F` up to 20) that stand for the player's name, the wizard's name, a spell, gold or a treaty: decode them into placeholders and read how each is filled in. Then the client (`diplomacy_text.rs`) uses the original's line for a situation when the file is there, picking a variant at random as the original does (one fewer in a two-player game), and falls back to its own otherwise. Map each situation to what the core does (greeting by personality, accept, refuse, counter, threat, war, peace offer, tribute thanks, get-off-my-lawn, treaty broken; `diplomacy.md` section 9.5). The situations the game cannot yet reach (threats, trespass, demands: task 167) can wait. Gitignored output, as for the help text.

**172. Learning AI: battle AI follow-ups.** What tasks 170 and 171 left (`learning-ai.md` 7.7 and 7.8): (1) stronger yardsticks than the scripted AI, which random orders already beat: past checkpoints as opponents (the league of 6.4), Elo, and an evaluation as the defender; (2) the agent chooses **which unit** moves (a pointer over the units) and can cast later in the turn, not only at its start; (3) fleeing: it is found only with the help of a head that compares the value of fleeing with the critic's, and it still flees in fights it would win; (4) longer runs and a checkpoint chosen by its held-out score; (5) the AI wizards' real battles as training data (task 8.3), difficulty levels from checkpoints, and a client setting for `--battle-ai` (the client needs the `learned` feature and a checkpoint file); (6) the battle outcome model for the strategic AI (8.4); (7) a gap the first runs show: the odds classes were measured with the scripted AI, so a better player makes them easy. (8) The bank was made before the undead, experience and neutral-unit changes of 13 October, and no run has trained on **Animate Dead** (the agent can now aim it: the fallen have tokens); remake the bank and train again. Remaking the bank also picks up the 11 spells of eligibility 2 and 3 that are castable in battle now (task 55).

**176. Mac installer.** One program (a script, or a small installer app) that sets the game up on a Mac from the player's own copy of the original: it asks for the path to the original files, runs the extraction (the art with `f4x-tools extract-art`, and the importer `scripts/import_mom.py` for the help text, wizards and city names into `assets-local/` and `content-local/`; `docs/local-assets.md`), checks that the files are the right ones and says what is missing, and then builds the local app (`scripts/make_app.sh` makes `target/Fantasy4X.app`). It should also say what it needs first (the Rust toolchain through rustup, the system Python) and offer to stop there instead of installing anything itself, report progress and errors in plain words, and leave the app and its data where the player can find them (perhaps copying the app to Applications). Nothing from the original is ever uploaded or committed; the result is for play on that machine only. Builds on task 81 (packaging for distribution, which carries no original files).

**219. Hero roster and the hero component.** Step 4 of [`heroes-items-design.md`](heroes-items-design.md) (sections 3.5 and 3.6). `roster` (one record per hero type per wizard on `Player`: availability Free, Serving, Dead or Gone, the abilities rolled at game start by `Init_Heroes` with the draw order kept, casting skill, innate spells, xp), the `Hero` component (kind, name, abilities, items), `hire_cost`, `pick` (`Pick_Random_Hero`: fame, the life and death book rules), six slots at most, Noble (no upkeep, +10 gold), Torin's upkeep, `on_dead` and `on_hired`; the save of all of it. First list every place a unit is removed (after a battle, the banishing spells, dismissal) and hook `on_dead` into each. The new draws at game creation change every seed once: regenerate the soak baseline and say so. Tests with a bare world and a fake roster. Also from task 218: the hero names are read from `content-local/hero_names.ron` (five a hero, `player * 35 + type` order in the original; invented defaults for a game without the local file), and the Knight / Elven Archer name question in task 218's text is settled first.

**220. Offers each turn.** Step 9 of [`heroes-items-design.md`](heroes-items-design.md) (section 3.8): `Determine_Offer` as a plan over plain data with an `OfferView` trait (fame, Famous, Charismatic, gold, hero count, roster, books) and a fake for tests. Hero offers (`(3 + fame / 25)` percent, doubled with Famous, capped at 10, plus 10 for computer wizards, divided by `(heroes + 1) / 2 + 1`), mercenaries (`1 + fame / 20`; level, count and price as in `Generate_Mercenaries`; the repeated "above 90" test is in the design doc) and merchant items (three times the cost, half for Charismatic). Commands `AcceptOffer` and `RejectOffer`; events; the computer wizards accept by the original's `AI_Accept_Hero`. Where in the turn change it runs against upkeep is to be checked in `NEXTTURN.c`. The pop-ups are task 143.

**221. Item loot and loss.** Step 7 of [`heroes-items-design.md`](heroes-items-design.md) (sections 4 and 5). `Spoils` gains the loser's fallen heroes' items and hero losses and the winner's armory takes them (`armory::place`: vault, then the award list of six, then lost); dismissed heroes' items are destroyed; Banish, Cracks Call, Disintegrate, Dispel Evil, Holy Word and Unsummon destroy the hero and its items, and Disintegrate and its kin make the hero Gone for good. Lair loot: `Loot.items` becomes real items made on the claim from the lair's value (`random_item`), and the prisoner a real hero offered for free (hire type 2). Needs tasks 49 and 219; the treasure dialog is task 128.

**222. The AI and the console for heroes and items.** Section 4 of [`heroes-items-design.md`](heroes-items-design.md). `f4x-ai`: accept hero offers (`AI_Accept_Hero`), equip items by fit (`AI_ProcessItems`: best attack in the weapon slot, armour in the armour slot), keep heroes with the army, craft nothing at first. `PlayerView` shows a wizard its own heroes and items only. `f4x-console`: commands and queries for the roster, the vault, equip, scrap, craft, accept and reject an offer, so the whole feature can be played and tested without the client. Soak: computer wizards must hire and equip heroes without refused orders.

**223. Hero and item help conflicts.** Section 7 of [`heroes-items-design.md`](heroes-items-design.md) lists 25 places where the help, the original's code and ReMoM disagree (Leadership per two or three levels, Constitution's defence, Agility without a help topic, Sage's super factor, Famous's cap, Lucky, item attack by kind, and more). Add them to `help-vs-implementation-conflicts.md` with the next B numbers, read the code again for each row marked Check (Leadership, Legendary's price cut, Sage, Lucky, Charismatic's order, Recall Hero overland, the number of powers in Enchant Item, hero 24's name, whether a slain hero can be resurrected, where offers fall in the turn), record the decision, and write `HELP_CORRECTIONS` notes where we deviate from the help. Do this before the steps that depend on each row.

**228. Scripted AI steps 4 and 5: casting (partly done).** Done in `wizardry.rs`: the kinds the AI casts (Spell of Mastery, summons, unit and city buffs, good globals, Earth Lore, Nature's Cures), paid over as many turns as the casting skill needs, the Spell of Mastery never abandoned; tested with a wizard that knows it (`behaviour.rs`) and `soak --grant-mastery`. Still open: Word of Recall, Transmute and Plane Shift, disenchant and disjunction, curses on the target wizard, the original's weighted pick among kinds. Sections 5.3 and 7.3. The eleven category weights with our own pickers, the affordability check, the mana gate, the special spells of task 56 (Earth Lore, Word of Recall, Nature's Cures, Transmute, Plane Shift), and the Spell of Mastery: weight 500 when known, never abandoned, upkeep shed first.

**229. Scripted AI steps 6 and 7: war (partly done).** Done in `military.rs` and `economy.rs`: a march state, gathering within two squares, chunks of nine, unreachable targets dropped for 40 turns, remembered garrisons and a lightly-held target preferred, a strike force before buildings, units re-chosen every turn. Still open: defence of threatened cities, a shared strength function for the overland and battle AI, an army size from the war plan. Sections 5.2 and 5.4. Memory of enemy garrisons, target wizard then city, a stack that suits the target, defence of threatened cities, army size from the war plan, income-first buildings, a strength function shared with the battle AI.

**230. Scripted AI step 8: ships (not started).** What was learned is in `ai-design.md` 11.3: four of eight soak seeds put the wizards on separate land masses or planes.  Section 5.2 and 10. About half the soak seeds put the wizards on separate land masses or planes; without transports those games cannot end by conquest. Needs `transport.rs` and `movement.rs` queries on the view.

**232. Conventional battle AI steps 10 and 11 (partly done).** Done in `battle/ai.rs`: targets by value (`expected_damage`, `worth`), focus fire, shooters hold, defenders hold the walls; the old AI is `ai::baseline_command`. These unit tactics are within the dice's spread (123 to 117 in mirrored fights); only the wizard's spells showed a gain. Still open: sallying out, fleeing, kiting, not stepping into a worse fight. Sections 6.3 and 6.4. Target values after ReMoM's `Target_Unit_Value`, focus fire, no steps into worse fights, kiting, holding the walls, sallying out, fleeing when hopeless. Run the learn tests at each step; `learning-ai.md` section 13 gets a note.

**233. Sea battles.** `battle/map.rs` treats an ocean fight as an open field (no obstacles, no rough ground) and the battle code has no notion of ships. Decide what the original does (and what the help says) for a fight at sea: whether ships fight as ordinary units, whether passengers fight or sit out, whether ranged attacks and spells differ over water, and what the fight map looks like. Record the answer in `combat.md` (and `divergences.md` where we differ), implement it, and test it with the real ships (`trireme`, `galley`, `warship`). Found while checking the ship mechanics before task 230.

**234. Attacking from a ship.** Moving a stack onto an enemy square starts a fight (`Entry::Hostile`) whatever the stack stands on; no rule bars land units aboard a ship from attacking a city or a stack on land, and none is tested. Find out what the original and the help say (a landing may have to happen first, or an attack from a ship may suffer a penalty), then enforce and test it, and tell the AI (`f4x-ai/naval.rs` lands its troops first and attacks from land, so it works either way today). Related: 233.

**235. A ship lost with passengers.** Passengers of a ship that is destroyed in battle or removed by a spell are only lost later, by `drown_stranded` at the next turn end (`transport.rs`). Decide the rule (lost at once, with the ship; or saved when land is adjacent), make the loss part of the battle result and the events, and test it: a ship sunk in a fight, a ship dismissed, a ship killed by a spell, passengers aboard each time. Check where in the turn order `drown_stranded` runs.

**236. The stack limit and ships.** A tile holds nine units (`MAX_STACK`); a galley carries five, so a loaded galley and its passengers take six places, and a port or sea square with several ships fills fast. Check how passengers count towards the limit when boarding, sailing into a city and landing, whether a landing stack can be refused for lack of room, and make the AI's gathering (chunks of nine in `military.rs`, loads of two to five in `naval.rs`) agree with it. Test with a full galley and a crowded square.

**238. Wizard objectives steering the AI.** Task 64 draws an objective for every AI wizard but nothing reads it. Design in [`ai-objectives.md`](ai-objectives.md): an objective table in `f4x-ai` read through the AI's own view; step 1 the plumbing (no change in play), step 2 the small hooks (cities wanted, strike size, research share, research score), step 3 weighted building and casting choices (a redesign, decided after step 2), step 4 war declarations by Militarist and Expansionist wizards. Each step that changes play gets a soak, only when asked.

**240. Terrain tables.** `TERRSTAT.LBX` (770 six-byte records and a 196-byte table) and `TERRTYPE.LBX` (five 512-byte tables of tile indexes) are not read by the importer. Find out what they hold (per-tile terrain type, food, movement cost, the tile-to-terrain map), compare with `content/terrain.ron` (14 hand-written rows) and the map generator, and note differences in `divergences.md` or fix the data. Nothing is known yet beyond the sizes.

**241. Lairs, monsters and events.** `MONSTER.LBX` (48 entries of 47 records of 80 bytes, not decoded; the lair guard and treasure tables are the likely content) and `EVENTS.LBX` (18 entries of 41 records of 43 bytes) are not read. Decode them, compare the guard sets, treasures and event chances with `lairs.rs` and the event code, and fix the data or record the difference.

**242. Other text and list tables.** `DESC.LBX` (241 descriptions of 110 bytes), `BUILDESC.LBX` (36 of 200), `LISTDAT.LBX` (75 of 200), `RESOURCE.LBX`, `EVENTMSG.LBX`, `DIPLOMSG.LBX` and `MESSAGE.LBX` are text; the original's prose stays in `content-local/` like the help text. Decide for each whether the game or the client needs it (screens, event and diplomacy messages), check that no rule hides in the numbers beside the text, and import what is used.

**243. Unread data files.** Find out what `WIZARDS.LBX` (14 entries of 109 records of 104 bytes), `SPECIAL.LBX` and `SPECIAL2.LBX`, `SPLMASTR.LBX` (32 entries of about 94 KB) and the remaining files the importer does not open hold (most of the 98 `.LBX` files are art, sound and music). List them in `docs/` with what each is, and add a task for each one that carries rules or text.

**244. What the "books" bytes of an item mean.** Every `ITEMDATA.LBX` record ends in five bytes (`spellranks[5]` in ReMoM's `s_ITEM_DATA`) and a flag. `Make_Item` (`ItemMake.c`) leaves an item out when any of the five is above the wizard's book count in that realm, so they decide which wizards a random or prefab item may go to. They are stored raw in `books` in `content/items.ron`, and the realm order and the way they come about are not known: a derivation from the item's powers (the largest book need in each power's realm, in the usual nature, sorcery, chaos, life, death order) matches only about 115 of the 250 items; Deathbringer (power Death, 3 books) has `[3, 0, 0, 0, 0]` and Sword of Stealth (True Sight, a life power needing 6) has `[0, 0, 6, 0, 0]`, so the order looks different. Find out (a brute-force fit over realm orders and over what counts, including the plain bonuses and the embedded spell's realm, and a read of `Make_Item` and `Create_Random_Item`), then write the rule and the realm order into `heroes-items-design.md` section 2.1, give `books` a named per-realm form in `ItemDef` and have `validate` check it. Needed before the item offers and rewards (tasks 220 and 221).

**245. Delete the old `~/reference` folder.** The folder outside the project, `~/reference` (the earlier ReMoM clone and the steamcmd home), is superseded by `reference/` inside the project and should go. The user deletes it themselves: the safety hook blocks Claude from deleting outside the project, and nothing in the project reads it. Check first that nothing there is still needed (the project's own `reference/ReMoM` and `reference/mom-classic` hold what the importer reads). This was part of task 80, which was closed without it.

**248. Game settings, stage 3: the advanced screen and the easy custom settings.** Stage 3 of `game-settings.md`. The advanced screen with the difficulty dropdown (five levels and Custom), the Advanced settings button on the base screen, human and AI values for starting gold, mana and picks, `ai.apply_to_human`, the AI wizard block, starting units (race units, settlers, scouts, ships, the first summoning spell's creature, and a free Magic Spirit as the scout: the starting-spell order is to be checked, see 4.2), switches (Spell of Mastery, Spell of Return off, human only, all, heroes, lairs, neutral cities, raiders, random events and frequency, fleeing), a lowered stack limit and city size cap, city distance, map size and counts, score scale and Needs 247. (Hardcore is stage 4, task 249.)

**249. Game settings, stage 4: no Myrror, hardcore mode, larger stacks and other hard settings.** Stage 4 of `game-settings.md`. No Myrror (section 4.9: one plane, no Myrran wizards, Planar Travel, Plane Shift or Planar Seal) and hardcore mode (4.10 and `PLANNED.md` item 8: no undo, permanent death, one constantly updated save file). Larger unit stacks are designed in [`large-stacks.md`](large-stacks.md); a raised city size cap needs its own check (growth tables, art for larger cities, balance); more than four opponents and per-event switches are listed in section 6. Each gets its own task before it starts. Needs 248.

**251. Check where the computer wizards' mana bonus applies.** Task 247 multiplies a computer wizard's city mana (at the end of the city calculation) and the power of its **nodes** (`magic_income.rs`, `node_percent`) by `ai.mana_percent`, as the original's `Do_City_Calculations` and the power-base phases (`CITYCALC.c`, "Apply AI Difficulty Modifiers", right after the node phase and before the city phase) do. It does **not** multiply the fortress's power (a point per book, five more on Myrror) or the volcanoes' power, because I could not tell from `CITYCALC.c` where the original adds them. Read the power-base function in ReMoM (`Player_Magic_Power` or the routine at the top of that file's power section) and find out whether the fortress and the volcanoes are added before or after the multiplier; if before, multiply them too (`base_from`), add a test, and record the result in `docs/game-settings.md` (3.2 notes) and `divergences.md`.

**252. Check and implement the starting spells each wizard gets in a new game.** What a wizard knows when the game begins is not settled against the original. Today `wizard.rs` gives the spells the books buy (the common spells chosen at creation, defaulted by `default_starting_spells_of`), and the ready-made wizards and the computer wizards go through the same path. Read the original's start (ReMoM `INITGAME.c` and the wizard-creation code) and write down what it gives each wizard: any spells every wizard starts with, the spells per book and realm (how many, which rarity, chosen or random), the starting research choice, and what the ready-made wizards and the computer wizards get beyond the player's own picks. Then compare with `wizard.rs` and fix the differences, with a test per rule. Points that depend on the answer: the order the starting spells are listed in, which `FirstSummon` in `game-settings.md` (4.2) needs fixed and tested; the Hard-level "knowable starting spell" for computer wizards, now left out because the original's code has bugs there (`divergences.md`); and the spells of a ready-made wizard with other than 11 picks (`game-settings.md`, open questions). Record any change in `divergences.md` and, where behaviour differs from the original help, in `HELP_CORRECTIONS` in `scripts/import_mom.py`.

**253. Lakes with an inflow and the original's rewriting of extra river inflows.** Left out of task 122. (1) A lake (a sea square with land all round) that a river runs into takes the `_1LakeRiv_N/E/S/W` piece for the side the river comes from, as block 1 of `River_Autotile` does in ReMoM `MAPGEN.c` (about 5029-5120); a lake with no inflow becomes desert (`_1Desert`) there. (2) Where more than one river runs into a lake or a shore square, the original keeps one outlet, turns the other inflow squares into grassland and goes back over the squares it changed. Our map keeps its rivers, so `terrain_pick::shore_with_rivers` falls back to fewer sides instead. Decide whether map generation should do the same rewrite (changes the maps of new games, so it is a rule-set question) or only the drawing should; then draw the lake pieces, check the look against the original, and test it.

**254. Battlefield leftovers after the field generator and art (110, 112).** (1) **Water battles** (a fight on a sea square): the original uses its own terrain set (ocean pictures, no land unit may deploy, ships move by the sailing cost map, `Undeployable_Battle_Units_On_Water`, floating islands for fights between ships); ours keeps open ground and draws the water pictures. (2) **Deployment order:** the original puts melee units in front and ranged ones behind, moving the strongest ranged unit forward when there are more than five (`Deploy_Battle_Units`). (3) **Effects drawn on the field:** the mud of Earth to Mud (`cmbtcity` 118, which also costs 12 to cross), the wall of fire and wall of darkness pictures, the wall-rise animation (`wallrise`), the cell effect (`cmbtcity` 120), the Flying Fortress cloud check, and the chaos node and nature node pictures (with task 111). (4) **A city with walls** and the Myrror wall bank were not looked at on a screenshot; compare them with the original's, including the draw order of the walls against units standing on them. (5) The original's other movement cost maps (`MoveCost_Ground2`, teleport and sailing) are not kept; only the walking one is. (6) The learning AI's networks and bank were made for the old fields (different deployment squares and costs): their checkpoints still load but should be retrained, and `docs/learning-ai.md` should say which runs were on which field.

**255. River terrain on the battlefield.** The original has the whole machinery and never uses it: `Build_Battlefield` passes `Generate_Combat_Map` a river array it has zeroed, so `Carve_River_Terrain` (a river walked across the field from the sides the overland river leaves by, only on plains) never runs. What exists to port: the river ground group (walking cost 4, sailing cost 2), the tile types 56 to 67 (`Set_Terrain_Tile_Types`: left-right, up-down and the four corners, two variants each), the animated river pictures (`cmbtcity` 97 to 108 on Arcanus, `chriver` 0 to 11 on Myrror, drawn over the ground in `Combat_Screen_Map_Draw`), and the rule that the ground under a river picture is the base square `(cgx + cgy) & 3`. Decide whether battles on a square with an overland river should show one (the original's intent, a rule-set choice because it changes the walking costs of those fights), then: fill the river flags from the square and its eight neighbours (`Grid::has_river`), port `Carve_River_Terrain` with its two cases, keep trees, rocks and roads off river squares, add `Layout` data and the cost, draw the pictures, and test that a river crosses the field and the deployment squares stay reachable. Related to the overland river pieces in task 253.

**256. Unit spell-like abilities: Fireball, Healing, Doom Bolt and Web spells.** Twelve non-hero units carry `casts_fireball` (Magicians of six races), `casts_healing` (the four races' Priests), `casts_doom_bolt` (Dark Elf Warlocks) or `casts_web` (Giant Spiders), and no code reads these abilities (`abilities.ron` only describes them); their ranged attacks (`fireball_attack`, `shaman_bolt`) work, but the spell does not exist. Help (entries 421 to 423 and 426): Healing heals a target unit by 5 hit points at most; Fireball is a strength 5 explosion hitting every figure of the target (the help adds one strength per 3 extra mana, which a unit cannot pay); Doom Bolt does 10 points ignoring defence; Web is a 12 hit point web (full melee and magic ranged damage breaks it; flying units stay grounded afterwards). ReMoM (`CMBTAI.c` 440 to 460 and 1100 to 1150, `Combat.c` 3659 to 3675) keeps each as a flag on the battle unit that is cleared when the spell is cast, so the spell can be cast once per battle, without mana, instead of the unit's attack, and the AI values Fireball and Doom Bolt by 16 minus the target's defence. Port: a once-per-battle cast command for the unit (reusing the Fireball, Healing, Doom Bolt and Web spells already in `battle/spells.rs`), the buttons or console command for it, the battle AI and learning AI's use of it (`legal_commands` and the observation grow, so retrain; ask first), a test per spell, the `auto` placeholder AI, and the unit window text. Check first whether the original lets a human cast Healing when the AI cannot (`CMBTAI.c` 462 notes the AI never executes it: an original bug, so do not copy it).

**258. Summon Demons.** `summon_demon` (Demon Lord) does nothing. Help (entry 552): instead of attacking, the unit conjures one demon into combat; the demon appears where the wizard's units began, stays if the summoner dies, and disappears after the combat. ReMoM (`Combat.c` `Summon_Demon` 11674 to 11712, `CMBMAGIC.c` 289, `UnitView.c` 1862 to 1880): the unit type carries two bits (1 or 2 uses; the unit window shows the count), each summon uses up one and ends the unit's movement, the demon is created for the controller, and a full set of squares loops forever (an original bug: do not copy). Port: a battle command for it, the `demon` unit (it exists in `units.ron`) added to the battle on a free square of the summoner's deployment area, the use count, the end-of-battle removal (see how the battle spell Summon handles it, `battle/spells.rs`), the cap on creatures in a battle (`casting.rs`), the AI's use and a test. Also note that `cavalry` (nine units) has no combat meaning in the original either: it only chooses the movement icon in the unit window (`UnitStat.c` 1645, the same picture as walking); show it there with task 140 and nothing else.

**259. Bug fix: path abilities and flight in battle movement.** (1) Forester, Mountaineer and Pathfinding (the abilities, the Path Finding spell, and a stack that shares them) should apply to movement in battle too: the forest, hills and mountains of the battlefield (trees and rocks add to the walking cost, `battle/map`) should cost such a unit what the overland rules give it (Pathfinding: every square at the cost of a road). Today the battle's cost map ignores them; check what the original does and decide per ability which squares they cancel. (2) The order of movement bonuses and Flight: the original (`COMBINIT.c`, after `Unit_Moves2`) first adds the moves of jewellery, Endurance and the like, then, when the unit has fewer than 3 moves, sets its moves to 3 for Flight, which throws the bonuses away (it also tests whole moves against half-moves, `OGBUG`). The rule should be the other way round: Flight sets the unit to 3 moves first, then the bonuses are added. Check the same order overland (`NEXTTURN.c` sets 6 half-moves for Flight, then adds Endurance and items, which is the right order there) and in our stat engine (`Modify Moves Set 3` in `content/spells.ron` is applied after `Add` modifiers by `compute`, so a bonus would be lost: this needs a `Min`/floor applied before the additions, or the order of operations changed for this stat). Test: a unit with Flight and Endurance has 3 + 1 moves in battle and overland; a Forester walks the battle's forest at the cheaper cost.
