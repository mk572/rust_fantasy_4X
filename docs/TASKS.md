# Open tasks

Everything not yet implemented, collected from the whole project history and the docs
(`mechanics-gaps.md`, `help-vs-implementation-conflicts.md`, `magic.md`, `map-layers.md`,
`design.md`). Keep the table in ascending order by number; new tasks go at their numeric place
(or take the next free number). Bracketed numbers like [430] point at topics in the original help
(`content-local/help.ron`); B/C numbers point at `help-vs-implementation-conflicts.md`.

Combat (tasks 33-46) was paused and has been released to start (33, 34, 40 first).
Finished tasks are moved to [`TASKS-DONE.md`](TASKS-DONE.md) with
`python3 scripts/archive_done_tasks.py` (a task is done when its row or title says "(done)").
Improvements for after the basic game is online are in [`PLANNED.md`](PLANNED.md).

## Table

| # | Area | Summary |
|---|---|---|
| 2 | Map | Lakes |
| 5 | Map | Land corruption and purification |
| 7 | Map | Node auras and warped nodes |
| 8 | Map | Encounter sites: guardians and loot |
| 9 | Map | City founding blocked only by intact encounter sites (B17) |
| 10 | Map | Generator parity with the original and new-game settings |
| 11 | Map | Roads built by engineers, and road enchantment |
| 12 | Map | Gates and the Planar Seal |
| 13 | Map | Teleporting and merging movement |
| 14 | Map | Sight radius sources (Oracle, walls, Nature's Eye) |
| 16 | City | Autobuild |
| 17 | City | Selling buildings: prerequisites beyond one level |
| 18 | City | Weapon quality from mithril and adamantium |
| 19 | City | Forester's Guild raises the growth cap (B14) |
| 20 | City | Population growth edge rules (B19) |
| 21 | City | Outpost growth: wild game and difficulty (B20) |
| 22 | City | Nightshade dispel strength |
| 23 | City | Animist's Guild food per farmer |
| 24 | City | Stream of Life and Gaia's Blessing (B8) |
| 25 | City | City enchantments with non-stat effects |
| 26 | Units | Unit abilities that need code |
| 27 | Units | Experience: Warlord, Crusade, Armsmaster, Heroism (B9) |
| 28 | Units | Transport: ships carrying units |
| 29 | Units | Undead and fantastic-creature rules (B10) |
| 30 | Units | Temporary effects store (instant modifiers, granted abilities) |
| 31 | Units | Neutral player and neutral cities |
| 32 | Units | Starting economy tuning |
| 33 | Combat | Battle data model in the ECS |
| 34 | Combat | Attack resolution: to-hit, damage, defence, attack order |
| 35 | Combat | Immunities and resistance rolls |
| 36 | Combat | Special attacks |
| 37 | Combat | Combat ability rules |
| 38 | Combat | Ranged ammunition and spell shots |
| 39 | Combat | Auto-resolve |
| 40 | Combat | Tactical battle map, walls and turn flow |
| 41 | Combat | City sieges, walls and capture |
| 42 | Combat | Fleeing and after-battle results |
| 43 | Combat | Casting in battle |
| 44 | Combat | Battlefield-effect spells |
| 45 | Combat | Battle screen in the client |
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
| 61 | Magic | Spell of Mastery, Spell of Return and the Spell of Mastery research cost |
| 62 | Magic | Player-chosen losses when mana runs short |
| 63 | Meta | Diplomacy |
| 64 | Meta | Wizard personalities and AI objectives |
| 65 | Meta | AI v1: expansion, production, defence |
| 66 | Meta | AI research and spell use |
| 67 | Meta | AI attack |
| 68 | Meta | Random events |
| 69 | Meta | Difficulty levels |
| 70 | Meta | Victory conditions and score |
| 71 | Meta | Advisors |
| 72 | Meta | Fame effects |
| 73 | Meta | Save-format migration |
| 74 | Meta | Stable command references for replays and multiplayer |
| 75 | Client | Wizard overview screen |
| 76 | Client | Background music |
| 77 | Content | Content pass and balance with AI-vs-AI soak runs |
| 78 | Content | Open conflict decisions (B9-B13, B15, B16, C3) |
| 79 | Content | Final art sourcing |
| 80 | Housekeeping | Push to GitHub and tidy the old reference folder |
| 81 | Housekeeping | Packaging for Mac and Linux |
| 82 | Meta | Wizard defeat aftermath and fortress display |
| 83 | Client | Wizard portraits and banner colours |
| 84 | Client | Analyse the original's screenshots and adjust the in-game graphics |
| 85 | Client | Import and play the original's opening movie |
| 86 | Client | Start menu: new game, load game, settings, quit |
| 87 | Magic | Remaining global enchantments |
| 88 | Meta | Hall of fame |
| 89 | Meta | Game settings |
| 90 | Meta | Undoing orders |
| 91 | Client | Diplomacy screen |
| 92 | Client | Spell-research dialogs |
| 93 | Client | Spell-book view of the magic screen |
| 94 | Client | Settings screen |
| 95 | Client | Tutorial |
| 96 | Client | City screen: enchantments list |
| 97 | Client | Sound effects |

## Descriptions

### Map

**2. Lakes.** Same gap as rivers: lake variants are not modelled. Decide whether a lake is its own
terrain class or a flagged ocean tile that is not coastal for ships, generate them, and check
effects on food and city coast status.

**5. Land corruption and purification.** Corruption of tiles (by units, spells and Great Wasting)
and purification (three turns) [404, 615-617]. Needs a corruption layer in `Grid`, its effect on
the worked-area food and production of cities, and spells that set it.

**7. Node auras and warped nodes.** +2 attack and defence to matching creatures inside a node's
aura [653-656], and warping by spells. Depends on 6.

**8. Encounter sites: guardians and loot.** Lairs, caves, temples, ruins, keeps and dungeons are
only markers. Add guard generation (strength by plane and site type), loot (gold, mana, items,
spells, heroes), and what clearing a site does (it stops blocking cities). Depends on combat for
the fights themselves (task 33 onward).

**9. City founding blocked only by intact encounter sites.** Today any encounter feature blocks
founding. The original blocks only towers, nodes and *intact* lairs (B17). Needs a "cleared"
state for sites (see 8).

**10. Generator parity and new-game settings.** Our generator is our own design. Optionally port
the original's landmass growth and shore fix-up, and add the new-game settings: land size 15/20/25
percent, 1-4 opponents, node strength, difficulty [648-652]. Mark as a parity task, low priority.

**11. Roads and engineers.** Roads exist as a flag but nothing builds them. Add the engineer units'
build-road order (several turns per tile by terrain), road enchantment spells, and road display for
newly built roads. Also unlocks the trade bonus effect in play.

**12. Gates and the Planar Seal.** Earth Gate links two own cities for a one-move trip; Astral Gate
and towers link planes; Planar Seal closes planar travel [750, 752]. Needs gate state on cities and
movement rules.

**13. Teleporting and merging movement.** Teleport and merge movement types [253-261] are listed
but have no rules. Add them to `movement.rs`, including their stack and plane rules.

**14. Sight radius sources.** Oracle sees 4 squares, walls 3, Nature's Eye 4, scouting units more
[546, 731, 748]. Today a city sees radius 2 and units by their `Sight` stat only.

### City and economy

**16. Autobuild.** The production queue has no automatic mode. Add an autobuild order that picks
the next sensible building, as the original's city screen does.

**17. Selling buildings: prerequisites beyond one level.** Selling a building currently checks only
direct prerequisites. Block selling where later buildings depend on it through a chain.

**18. Weapon quality from mithril and adamantium.** Units trained in a city with mithril or
adamantium access get better weapons (+1 or +2 attack and to-hit bonuses in the original). Needs a
per-unit quality attribute set at creation and saved.

**19. Forester's Guild raises the growth cap (B14).** The original forgot to include the +2 food in
the maximum size; decision is to include it. Change the farmland-cap calculation in `economy.rs`.

**20. Population growth edge rules (B19).** Excess above the next whole figure is discarded when a
figure is gained, growth is clamped so it cannot go negative at maximum size, and neutral cities
grow at half rate. Verify against `apply_growth` and add tests.

**21. Outpost growth: wild game and difficulty (B20).** Follow the intent: include wild game in
outpost growth and apply the AI difficulty modifier only to AI players (the original applied it to
the human too).

**22. Nightshade dispel strength.** Each nightshade plant in a city's area adds 100 dispel strength
against bad city enchantments [766]. Hook into the dispel code (task 59).

**23. Animist's Guild food per farmer.** The guild adds food per farmer, not per city [723];
verify and fix the effect in the building data and the economy formula.

**24. Stream of Life and Gaia's Blessing (B8).** The original reads the wrong table, which looks
like a port error. Needs the original's disassembly; until then, implement the described behaviour
(unrest relief and growth) and note the decision.

**25. City enchantments with non-stat effects.** Nine spells (walls, gates, scouting, spell ward,
and so on) need effects that stat modifiers cannot express. Add city-state flags consumed by
movement, vision, combat and dispel code.

### Units

**26. Unit abilities that need code.** Many imported abilities are data only: movement and combat
abilities such as invisibility, regeneration, healer (B4: 20% of total hit points per turn for the
stack), long range, create and summon undead and demons, planar travel, non-corporeal, and so on
[396-427, 552-553]. Go through `content-local/abilities.ron`, list which are still no-ops, and
implement each where its system exists.

**27. Experience: Warlord, Crusade, Armsmaster, Heroism (B9).** Elite levels beyond 120 xp need
Warlord or Crusade; Armsmaster doubles hero xp use; Heroism should make a unit elite (decide the
combat-level exception from B9). Warlord retort needs task 53.

**28. Transport.** Ships carry units by building tier [725-727]. Needs a carried-by relation,
loading and unloading rules, and sea movement for carried stacks.

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
the single city earns. Tune the starting setup (units, tax, buildings) or document it as intended.

### Combat (paused)

**33. Battle data model in the ECS.** Define how a battle exists: battle entity or record, the two
sides, units' battle positions and figures, turn order, and how it hangs off `AttackRequested`.
Decide the relation to `ecs-structure.md`. Everything below builds on it.

**34. Attack resolution.** Base to-hit 30 percent, each plus +10 percent; defence blocks at 30
percent per point; damage by figure; attack order thrown, breath, gaze, then first strike, then
melee [430, 443-448, 543]. Implemented as pure functions with seeded RNG and thorough tests.

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

**39. Auto-resolve.** Resolve a battle without the tactical map, including the strategic-combat-only
option [685]. First playable combat; do after 34-37.

**40. Tactical battle map, walls and turn flow.** Combat grid, obstacles, wall sections, unit
initiative and movement points, melee, ranged and spell actions, per-unit turns [593-597].

**41. City sieges, walls and capture.** Walls and gates in city battles, defenders' bonuses,
capture or raze of the city, and the fortress rule that a wizard's capital city cannot be
destroyed [CITYCALC notes in ReMoM].

**42. Fleeing and after-battle results.** Fleeing (50 percent of units lost, 25 percent of heroes),
experience awarded, healing of the healer ability, summoned creatures vanishing, loot and fame,
removal of destroyed units from the ECS.

**43. Casting in battle.** Battle spell lists, casting skill as the per-combat mana limit, the mana
range multiplier by distance from the wizard's fortress, and Counter Magic [610].

**44. Battlefield-effect spells.** 19 spells with combat-wide modifiers by realm, per-turn bolts,
mud, entangle and counter magic; implement on top of 30 and 43.

**45. Battle screen in the client.** Draw the combat map, units, selection, moves, attacks and
spell casting, with the original battle art from `assets-local`.

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

**61. Spell of Mastery, Spell of Return and the Spell of Mastery research cost.** Victory by
casting the Spell of Mastery, Spell of Return after banishment (zero power while casting),
and the Spell of Mastery's special research cost (the data holds a placeholder of 1 because the
original computes it).

**62. Player-chosen losses when mana runs short.** The engine drops the newest upkeep items
deterministically; the original asks the player. Add a prompt or a priority list, and the "mana
short" event interaction (68).

### Meta: diplomacy, AI, events, victory

**63. Diplomacy.** Relations on an 11-step scale, pacts, alliances with help obligations, war,
treaty proposals and wizard attitudes [271, 782-794]. Needs a diplomacy record per player pair.

**64. Wizard personalities and AI objectives.** Maniacal to peaceful personalities and the
pragmatist, militarist, theurgist, perfectionist and expansionist objectives that steer the AI
[271, 782-794].

**65. AI v1.** Expansion, city production and defence for the rival player; currently the rival
has no behaviour at all. Uses commands only, like the human.

**66. AI research and spell use.** Research picking by personality, casting overland spells
(summoning, enchantments) and the spell weights from the original's AI tables.

**67. AI attack.** Target selection, army assembly and attacking cities and sites; depends on
combat (33-42).

**68. Random events.** Plague, population boom, conjunctions, moons, mana short and others, each
ending with 5 percent chance per turn; switchable option [768-775, 683]. Follow intent for B18
(count from the real turn, not from game start or load).

**69. Difficulty levels.** AI power, gold and mana modifiers by difficulty and the Hard-level
starting spells for the AI [difficulty tables in the original]; apply to AI players only.

**70. Victory conditions and score.** Win by Spell of Mastery or conquest, game-over screen, and
the score with its multipliers [642-647, 799-805].

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

**79. Final art sourcing.** Placeholders come from the author's own copy of the original and are
local only. The repo must stay CC0 and original-free; decide on commissioned, generated or CC0
art before any public release.

**80. Push to GitHub and tidy.** Work since the last push is committed locally only. Push when the
user agrees. The user still needs to delete the old `~/reference` folder (the earlier ReMoM clone
and steamcmd home) themselves; the safety hook blocks it for Claude.

**81. Packaging.** Mac and Linux builds, with the local-assets workflow documented for players who
supply their own original files.

**82. Wizard defeat aftermath and fortress display.** When a wizard is defeated (task 15's
`WizardDefeated`), the original turns their garrisoned normal units neutral, dismisses their heroes
and fantastic units, removes their city enchantments everywhere, and (for AI) turns their remaining
cities neutral. An AI wizard resigns instead of casting the Spell of Return if its skill is below 40
or it has no mana. Also draw the fortress and circle as buildings in the city screen's building
area. Needed before victory conditions (70) work properly.

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

**90. Undoing orders.** The original lets the player undo some orders [216, 221]. Decide the
scope (the last unit move, the last city change, none after end of turn) and implement it with
a command history in the client or a `Game` snapshot per order, keeping the stable command
references of task 74 in mind.

**91. Diplomacy screen.** Talk to a rival wizard: attitude, treaties on offer, proposals and
answers, shown with the wizard's portrait and mood. Needs the diplomacy records of task 63.

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
