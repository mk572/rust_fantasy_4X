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
| 8 | Map | Encounter sites: guardians and loot (core done; client prompt, items and prisoners open) |
| 9 | Map | City founding blocked only by intact encounter sites (B17) |
| 10 | Map | Generator parity with the original and new-game settings |
| 12 | Map | Gates and the Planar Seal |
| 13 | Map | Teleporting and merging movement |
| 16 | City | Autobuild |
| 17 | City | Selling buildings: prerequisites beyond one level |
| 18 | City | Weapon quality from mithril and adamantium |
| 20 | City | Population growth edge rules (B19) |
| 21 | City | Outpost growth: wild game and difficulty (B20) |
| 22 | City | Nightshade dispel strength |
| 24 | City | Stream of Life and Gaia's Blessing (B8) |
| 25 | City | City enchantments with non-stat effects |
| 27 | Units | Experience: Warlord, Crusade, Armsmaster, Heroism (B9) |
| 29 | Units | Undead and fantastic-creature rules (B10) |
| 30 | Units | Temporary effects store (instant modifiers, granted abilities) |
| 31 | Units | Neutral player and neutral cities |
| 32 | Units | Starting economy tuning |
| 35 | Combat | Immunities and resistance rolls |
| 36 | Combat | Special attacks |
| 37 | Combat | Combat ability rules |
| 38 | Combat | Ranged ammunition and spell shots |
| 39 | Combat | Auto-resolve |
| 41 | Combat | City sieges, walls and capture |
| 42 | Combat | Fleeing and after-battle results |
| 43 | Combat | Casting in battle |
| 44 | Combat | Battlefield-effect spells |
| 46 | Combat | Battle AI |
| 47 | Heroes | Hero abilities |
| 48 | Heroes | Hiring, summoning, recall, resurrection and fame |
| 49 | Heroes | Item slots, powers and data schema |
| 50 | Heroes | Enchant Item, Create Artifact, the vault and the anvil |
| 51 | Heroes | Heroes' casting contribution and sage heroes |
| 53 | Magic | Remaining retort effects |
| 55 | Magic | Importer: spell eligibility 2 and 3 |
| 56 | Magic | Implement the 37 "special" spells |
| 58 | Magic | Unit enchantments needing non-stat effects |
| 59 | Magic | Dispel mathematics and the counter-magic family |
| 60 | Magic | Wizard-targeting, destruction and other special spell categories |
| 61 | Magic | The Spell of Mastery's research cost and the AI casting it |
| 62 | Magic | Player-chosen losses when mana runs short |
| 63 | Meta | Diplomacy |
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
| 83 | Client | Wizard portraits and banner colours |
| 84 | Client | Analyse the original's screenshots and adjust the in-game graphics |
| 85 | Client | Import and play the original's opening movie |
| 86 | Client | Start menu: new game, load game, settings, quit |
| 87 | Magic | Remaining global enchantments |
| 88 | Meta | Hall of fame |
| 89 | Meta | Game settings |
| 91 | Client | Diplomacy screen |
| 92 | Client | Spell-research dialogs |
| 93 | Client | Spell-book view of the magic screen |
| 94 | Client | Settings screen |
| 95 | Client | Tutorial |
| 96 | Client | City screen: enchantments list |
| 97 | Client | Sound effects |
| 98 | Housekeeping | Packaging for Linux |
| 99 | Combat | Saving during a battle |
| 100 | Map | Draw the original's edge variants for mountains, hills and tundra |
| 101 | Magic | Eleven-book realms and the full starting-spell rule |
| 102 | Map | Road-building path selection in the client |
| 103 | Units | Scouting levels and unit sight |
| 104 | Units | Ship tiers by building and ship movement estimates |
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
| 123 | Map | Call the Void, Great Wasting and Armageddon corruption |
| 124 | Map | Mines, lumber camps and hunter's lodges on the map |
| 125 | Client | Unit stack backgrounds in banner colours |
| 126 | Client | Bitmap font for the remaining overland text |
| 127 | Map | Lair rewards: original spell, book and retort selection |
| 128 | Client | Site encounter dialogs and treasure popup |
| 129 | Map | Lair generation parity |
| 130 | Client | Warped node and cleared site pictures |
| 131 | Combat | Battle grid shape and deployment as in the original |
| 132 | Client | Battle screen: unit figures, animation and the original's frame |
| 133 | Client | Prompt before attacking, and battles between AI players |
| 134 | Client | Game-end cut-scenes and the original's score screen |
| 135 | Client | Run the AI wizards in the client |

## Descriptions

### Map

**5. Land corruption and purification.** Corruption of tiles (by units, spells and Great Wasting)
and purification (three turns) [404, 615-617]. Needs a corruption layer in `Grid`, its effect on
the worked-area food and production of cities, and spells that set it.

**7. Node auras and warped nodes.** +2 attack and defence to matching creatures inside a node's
aura [653-656], and warping by spells. Depends on 6.

**8. Encounter sites: guardians and loot.** Core done (`lairs.rs`, `docs/lairs.md`): guards and hoards
for towers, nodes and encounter sites, assault by `Command::EnterSite`, treasure and clearing. Open
parts are split into tasks 127 to 130; the client has no confirm box yet and assaults a site at once (task 128).

**9. City founding blocked only by intact encounter sites.** A cleared encounter site now removes
its feature, so it no longer blocks; but any *intact* site still blocks, as it should. What is left:
check the rule against the original's `Square_Has_Lair` (towers and nodes block always, lairs only
while intact, B17) and update the founding check and its tests to read the lair record rather than
the feature kind.

**10. Generator parity and new-game settings.** Our generator is our own design. Optionally port
the original's landmass growth and shore fix-up, and add the new-game settings: land size 15/20/25
percent, 1-4 opponents, node strength, difficulty [648-652] (difficulty and node strength are now
inputs to `GenParams`, used for lairs and node power). Lair placement parity is task 129. Mark as a
parity task, low priority.

**12. Gates and the Planar Seal.** Earth Gate links two own cities for a one-move trip; Astral Gate
and towers link planes; Planar Seal closes planar travel [750, 752]. Needs gate state on cities and
movement rules.

**13. Teleporting and merging movement.** Teleport and merge movement types [253-261] are listed
but have no rules. Add them to `movement.rs`, including their stack and plane rules.

### City and economy

**16. Autobuild.** The production queue has no automatic mode. Add an autobuild order that picks
the next sensible building, as the original's city screen does.

**17. Selling buildings: prerequisites beyond one level.** Selling a building currently checks only
direct prerequisites. Block selling where later buildings depend on it through a chain.

**18. Weapon quality from mithril and adamantium.** Units trained in a city with mithril or
adamantium access get better weapons (+1 or +2 attack and to-hit bonuses in the original). Needs a
per-unit quality attribute set at creation and saved.

**20. Population growth edge rules (B19).** Excess above the next whole figure is discarded when a
figure is gained, growth is clamped so it cannot go negative at maximum size, and neutral cities
grow at half rate. Verify against `apply_growth` and add tests.

**21. Outpost growth: wild game and difficulty (B20).** Follow the intent: include wild game in
outpost growth and apply the AI difficulty modifier only to AI players (the original applied it to
the human too).

**22. Nightshade dispel strength.** Each nightshade plant in a city's area adds 100 dispel strength
against bad city enchantments [766]. Hook into the dispel code (task 59).

**24. Stream of Life and Gaia's Blessing (B8).** The original reads the wrong table, which looks
like a port error. Needs the original's disassembly; until then, implement the described behaviour
(unrest relief and growth) and note the decision.

**25. City enchantments with non-stat effects.** Nine spells (walls, gates, scouting, spell ward,
and so on) need effects that stat modifiers cannot express. Add city-state flags consumed by
movement, vision, combat and dispel code.

### Units

**27. Experience: Warlord, Crusade, Armsmaster, Heroism (B9).** Elite levels beyond 120 xp need
Warlord or Crusade; Armsmaster doubles hero xp use; Heroism should make a unit elite (decide the
combat-level exception from B9). Warlord retort needs task 53.

**29. Undead and fantastic-creature rules (B10).** No healing for undead, +50% upkeep for
fantastic undead, and the full set of death-creature immunities (the original forgot cold, poison
and illusion immunity for raised units; decision is to follow the help).

**30. Temporary effects store.** `GameHost::add_modifier`, `grant_ability` and `remove_ability` do
nothing, so instant or timed spells that grant a modifier or ability have nowhere to live. Add a
per-unit and per-city effects component (source, value, remaining turns) read by `unit_modifiers`
and `city_modifiers`, saved by name. Prerequisite for 52 and many spells.

**31. Neutral player and neutral cities.** Neutral cities (rebels, raider cities) and the neutral
player are not modelled; neutral cities grow at half rate (20).

**32. Starting economy tuning.** After eight turns the starting army costs about 3 gold more than
the single city earns. Tune the starting setup (units, tax, buildings) or document it as intended. A related finding: in a console game from the shared new-game setup (the same start as the client's), ending the first turn without changing farmers disbanded two of the three starting units (food upkeep, `UnitsDisbanded`), so the starting army and settlers do not fit the starting city's food either.

### Combat (paused)

**35. Immunities and resistance rolls.** All the immunities and their numbers: weapon immunity (10
defence against magic), magic immunity (50), missiles, fire, cold, poison, stoning, death,
illusion [412-420]; resistance rolls with spell penalties.

**36. Special attacks.** Armor piercing (halves defence), illusion (ignores armour), chaos damage
(ignores everything), life steal (turns victims undead), poison per level, stoning, cause death,
destruction, power drain, dispel evil [429-439].

**37. Combat ability rules.** Large shield (+2 versus ranged), wall crusher (50 percent melee, 25
percent ranged), healer, invisibility, long range, regeneration, luck, negate first strike, mana
leak [396-427, 491].

**38. Ranged ammunition and spell shots.** Quiver refill after combat, magical ranged "spell
shots" that use mana [542].

**39. Auto-resolve.** Done: `Command::AutoResolve` (console `auto`) fights a battle to the end with the placeholder battle AI (`battle/ai.rs`) playing both sides, from an adjacent square, with the usual aftermath. Open: the strategic-combat-only option [685], a button for it on the battle screen (the Auto button is drawn greyed; task 133), and a better battle AI (46) so the result is not just whoever the placeholder favours.

**41. City sieges, walls and capture.** Capture is done (a battle won at a city, or walking into an empty one, takes it; a fortress city banishes its wizard, `fortress.rs`). Open: razing a city instead of taking it, walls and gates in city battles, defenders' bonuses,
capture or raze of the city, and the fortress rule that a wizard's capital city cannot be
destroyed [CITYCALC notes in ReMoM].

**42. Fleeing and after-battle results.** Done: fleeing units are lost (50 percent, 25 percent of heroes; `overland_aftermath`), and dead units are removed. Open: experience awarded, healing of the healer ability, summoned creatures vanishing, loot and fame,
removal of destroyed units from the ECS.

**43. Casting in battle.** Battle spell lists, casting skill as the per-combat mana limit, the mana
range multiplier by distance from the wizard's fortress, and Counter Magic [610].

**44. Battlefield-effect spells.** 19 spells with combat-wide modifiers by realm, per-turn bolts,
mud, entangle and counter magic; implement on top of 30 and 43.

**46. Battle AI.** Decision making for AI-controlled units in tactical battles and for the AI
wizard's combat spells. After 40 and 43.

### Heroes and items

**47. Hero abilities.** 13 hero abilities with levels and "super" versions [515-526]; data exists
as imported text, the effects mostly do not.

**48. Hiring, summoning, recall, resurrection and fame.** Hero availability by fame, hiring cost,
the noble, charmed and lucky traits, Summon Hero and Summon Champion, Recall Hero, dead hero
tracking and resurrection [577-588, 240, 246].

**49. Item slots, powers and data schema.** Slots by hero type (sword, bow, staff, armor, misc),
item powers, charges, and how this is expressed in data (open question in `design.md` section 13).

**50. Enchant Item, Create Artifact, the vault and the anvil.** Crafting UI and rules (power budget
per book, mana cost, Artificer discount already in costs), the player's item vault, scrapping items
at the anvil.

**51. Heroes' casting contribution and sage heroes.** Heroes add to casting skill and sage heroes
add research (level times 3, or 9 for the greater ability). The hooks (`Skill_Left` and the
research income) exist; heroes do not feed them yet.

### Magic (first pass done; see `magic.md`)

**53. Remaining retort effects.** Retorts stored but without effect: Warlord, Channeler, Myrran
(Myrror start), Divine and Infernal Power (+50 percent religious power, B3), Famous, Charismatic,
Node Mastery and the realm masteries' power doubling for nodes. Alchemy, Archmage, Artificer,
Conjurer, Sage Master, Runemaster, Mana Focusing and the masteries' cost and research bonuses
already work.

**55. Importer: spell eligibility 2 and 3.** `scripts/import_mom.py` collapses eligibility 2 and 3
to overland-only, losing the overland ×5 cost for them; spells such as those cost less than in the
original. Preserve eligibility in `SpellDef` (or derive contexts exactly) and test spell costs
against the help text.

**56. The 37 "special" spells.** Map reveals, terrain and mineral changes, healing a map square,
recalls, moving the fortress, resurrect and similar [mechanics-gaps section 4]. Add the missing
effect primitives (map reveal, teleport, grant movement type, building destruction, random-effect
tables) and implement them in groups.

**58. Unit enchantments needing non-stat effects.** 20 spells with movement types, immunities,
conditional bonuses and regeneration. Needs conditional modifiers (by attacker realm, ranged versus
melee, creature realm), immunity flags and movement grants; depends on 30 and the combat tasks.

**59. Dispel mathematics and the counter-magic family.** Dispel strength versus spell cost with
node protection and Archmage or mastery doubling, Disenchant Area, Disjunction, Spell Blast, Counter
Magic and Spell Binding [610]. The current host dispel is a simple strength-over-difficulty roll.

**60. Wizard-targeting, destruction and other special categories.** About 20 spells aimed at
wizards, resistable kills, destruction of buildings and similar; needs wizard-state changes (mana,
skill, fame, diplomacy) and spell ownership transfer.

**61. The Spell of Mastery's research cost and the AI casting it.** Finishing the spell wins
the game and the Spell of Return works after banishment (tasks 70 and 15; `docs/game_end.md`).
Still open: the Spell of Mastery's special research cost (the data holds a placeholder of 1
because the original computes it), the announcement when a wizard starts casting it, and the AI
choosing to cast it.

**62. Player-chosen losses when mana runs short.** The engine drops the newest upkeep items
deterministically; the original asks the player. Add a prompt or a priority list, and the "mana
short" event interaction (68).

### Meta: diplomacy, AI, events, victory

**63. Diplomacy.** Relations on an 11-step scale, pacts, alliances with help obligations, war,
treaty proposals and wizard attitudes [271, 782-794]. Needs a diplomacy record per player pair. The plan
and the original's rules are in `docs/diplomacy.md`.

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

**69. Difficulty levels.** AI power, gold and mana modifiers by difficulty and the Hard-level
starting spells for the AI [difficulty tables in the original]; apply to AI players only. The
final score is also scaled by difficulty (`endgame::scale_for_difficulty` exists; the score is
not yet scaled because the game has no difficulty setting). See `docs/game_end.md`.

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
victory and defeat), looping and crossfading, with a volume setting (94). The original's music
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

**81. Packaging for Mac.** A macOS build: a `.app` bundle (icon, `Info.plist`), universal or
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

**83. Wizard portraits and banner colours.** The original lets the player choose a portrait for a
custom wizard and a banner colour (help topics 658 and 674); here colours come from the player
number and there are no portraits. Add both to the creation screen and the build, using the
extracted wizard pictures locally (`assets-local`).

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
Still open: the magic screen, the wizard creation screen, the unit and army lists, the battle
screen, the hero sheet and the diplomacy screen.

**85. Import and play the original's opening movie.** Extract the intro from the author's own
copy of the game (its animation frames and palette from the intro `.lbx` files, and the sound
and music if they can be decoded; `reference/ReMoM/MoM/src/INTRO.c` shows which entries play in
which order and with which timing) into `assets-local/` with a new `f4x-tools` step, and play it
in the client at start-up, skippable with any key and switchable off in the settings. The
movie is original-derived: it stays local, and the game runs without it. Depends on the audio tasks (76, 97)
for the sound.

**86. Start menu.** A title screen shown before anything else (after the opening movie, 85):
New game (opens the wizard creation screen, with map seed, land size and opponents once the
settings exist, T10), Load game (list the saves in `saves/` with date, turn and wizard; open
the chosen one), Settings, and Quit. The in-game Esc menu should offer the same entries plus
Save and Return to the game. Today the client starts straight in the creation screen
(`--quick` skips it, `--load` loads a file); both options stay for scripting.

**87. Remaining global enchantments.** The global spells still without effect, grouped by what
they need: *combat* (Eternal Night, Zombie Mastery, Life Force and Tranquility's battle part,
Nature's Wrath, Time Stop's combat part: 33-46); *corruption and map change* (Great Wasting,
Armageddon, Meteor Storm: land corruption 5, terraform volcano rain, timed damage); *dispel
mathematics* (Suppress Magic, Life Force, Tranquility: 59); *diplomacy and fame* (Aura of
Majesty: 63; Just Cause: 72); *movement* (Wind Mastery: doubles own ship and sky speeds and
halves everyone else's, which needs a condition on movement type); *experience* (Crusade: one
level more for normal units, 27); *abilities* (Holy Arms' magic weapons, Doom Mastery's
mutation of new units: temporary effects store 30); and Time Stop's turn flow.

**88. Hall of fame.** The list of best finished games with wizard, race, score, turns and
difficulty [628-641, 798], stored outside the saves (for example `saves/hall_of_fame.ron`),
shown from the start menu (86) and after a game ends (victory and score, 70). Needs 70 first.

**89. Game settings.** The options screens of the original [675-692]: automatic saving, event
switches, strategic-only combat, animation and message speeds, and so on. Store them as a
settings record, `serde(default)` and saved with the game where they change rules (events,
Spell of Return option from `PLANNED.md`), and in a client configuration file where they only
change presentation. Reachable from the start menu (86) and in game.

**91. Diplomacy screen.** Talk to a rival wizard: attitude, treaties on offer, proposals and
answers, shown with the wizard's portrait and mood. Needs the diplomacy records of task 63. See `docs/diplomacy.md`.

**92. Spell-research dialogs.** The dialogs of the original's research flow: the choice of the
next spell to research when one is learned (a list of the researchable spells with costs and
turns left), and the "spell learned" announcement. Today the magic screen holds the list and
learned spells only show in the message log.

**93. Spell-book view of the magic screen.** A page-turning spell book, as in the original,
beside or instead of the list: spells by realm and rarity with their costs, casting buttons
and the research queue, using the original's book art locally.

**94. Settings screen.** The client screen for the options of task 89: gameplay options and
presentation options (volume, speeds, zoom), reachable from the start menu (86).

**95. Tutorial.** A first-game guide: highlighted steps for moving, founding a city, building,
researching and casting, with a way to skip or switch it off.

**96. City screen: enchantments list.** List the spells on a city (own and enemy, with the caster's
colour and upkeep) as the original's city screen does [301], and allow dispel attempts from it.

**97. Sound effects.** Short sounds for UI clicks, turn end, city growth, spell casting, combat
hits and deaths, and the wizard's spoken lines where the original has them, played through the
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

**101. Eleven-book realms and the full starting-spell rule.** The original lets one realm hold 11 books (with 10 common choices, 2 uncommon and 1 rare starting spell, `m_select_count_*`); the creation screen and `init_wizard` stop at 10 books and only choose common spells. Raise the limit, add the uncommon and rare starting picks, and extend the creation screen's spell lists.

**102. Road-building path selection in the client.** The B key builds a road only on the square under the engineers. Add the original's road-building mode (topics 510-512): click a path, show valid squares in blue and invalid in red with the turns it will take, OK and Cancel buttons, using `find_path`-style planning and `Command::BuildRoad` with the whole path.

**103. Scouting levels and unit sight.** Units see by their `Sight` stat only; the original's scouting ability adds a level to sight [546] and Wind Walking gives its stack speed 3. Import scouting as its own ability or stat modifier, apply it in `visibility`, and give wind-walking stacks their speed.

**104. Ship tiers by building and ship movement estimates.** Which buildings allow which ship (ship wright's guild, shipyard, maritime guild: help 725-727) is only encoded as unit requirements in the data. Verify it for every ship, show the capacity in the unit and city screens, and make path-turn estimates (`turns_for_path`) use the ship's speed instead of the slowest passenger's.

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

**123. Call the Void, Great Wasting and Armageddon corruption.** The corruption layer, Corruption, Purify, Land Corruption and Gaia's Blessing exist; the global spells that corrupt land each turn (and the Great Wasting and Armageddon looks) are still to wire to it.

**124. Mines, lumber camps and hunter's lodges.** `mapback.lbx` 87 (mine), 88 (lumber camp), 90 (hunter's lodge) are extracted but nothing in the base game's data places them; check whether the original used them (they may be Caster of Magic's) and, if so, move to PLANNED item 4.

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

**129. Lair generation parity.** The original places 6 towers, 30 nodes, 25 strong and 32 weak random
sites (102 slots, three left switched off), each at least two squares from any other lair, on land,
2 squares from the map edge, with a seven-way random site type (cave, dungeon, temple, keep, lair, ruins,
fallen temple). We give every generated feature a lair and split strong/weak 25:32 at random. Port the
exact placement, counts and type roll once the generator parity task (10) is taken.

**130. Warped node and cleared site pictures.** A warped node is drawn with a purple tint placeholder;
use `mapback.lbx` 93 (the warped-node mask). Check the original for any picture left behind by a
cleared site (the original just stops drawing the lair) and for nodes that are guarded versus cleared.

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
