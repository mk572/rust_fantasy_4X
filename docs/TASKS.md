# Open Tasks

Everything not yet implemented.

Next open task number: 362 (increase when a task is created)
Keep the table in ascending order by number.
Add a summary to the table, then details under Descriptions.

Bracketed numbers like [430] point at topics in the original help
(`content-local/help.ron`)
B/C numbers point at `help-vs-implementation-conflicts.md`.

Gaps found while working on the base game are added here as new tasks, not left in notes.

Finished tasks are moved to [`TASKS-DONE.md`](TASKS-DONE.md) with
`python3 scripts/archive_done_tasks.py` (a task is done when its row or title says "(done)").

The older open tasks are parked in [`TASKS-ONHOLD.md`](TASKS-ONHOLD.md).

## The rework: concept entities (tasks 307 to 342)

Goal: the code is organised by the game's concepts, and the concepts are entities. Today only unit
and city are `hecs` entities. After the rework **player**, **wizard**, **hero**, **spell instance**,
**terrain feature** (nodes, lairs, towers, minerals, volcanoes) and **item** are too, names are
singular, and `PlayerId` lives with the player it identifies.

Rules for every task of the rework:
- **No change in the game's rules** (the one exception is task 338, which says so). Seeds do not
  need to stay consistent: the RNG draw order, the event order, the learned-AI observations and old
  saves may change. The tests (`cargo test --workspace`), `cargo fmt --all` and
  `cargo clippy --workspace --tests` pass after each task.
- **Do not keep a bad structure to avoid churn.** When a type, a name or a field is in the wrong place,
  move it and fix every use. No re-export shims or wrapper methods are left behind.
- **Docs first**: the design that the later tasks implement is in `docs/ecs-structure.md` section 3
  (written by task 310). Read it before starting task 311. Where a task and the design disagree, the
  design wins; fix the task text.
- **Saves in between.** Until task 335 the save format keeps its layout: each task adapts the mapping
  in `game/save.rs` so that the save round-trip tests stay green, and 335 redesigns the format. Old
  saves need not load after 335 (the game version is checked).
- **The rules treat the AI like a human.** A game rule never decides for the AI. If an AI wizard needs
  to do something (convert gold to mana, answer an offer), its script in `f4x-ai` does it with the same
  commands a human has.
- **One task at a time, then mark it done.** Run `cargo fmt --all`, `cargo clippy --workspace --tests`
  and `cargo test --workspace`; mark the task "(done)" in `TASKS.md` and run
  `python3 scripts/archive_done_tasks.py`. Update the docs the task names. Commits are made when the
  project owner asks.

Phases and dependencies (a task needs the tasks before it in its chain):

```
A prepare  307 > 308 > 309 > 310
B player   311 > 312 > 313 > 314 > 315 > 316 > 317     (needs A)
C spell    318 (needs 315) > 319, 320, 321, 322, 323, 324 (each needs 318) > 325
           340 (the art folders; no code dependency, placed last in C)
D hero     326 (needs 317) > 327 > 328                  (328 also needs 318)
E feature  329 (needs 313) > 330, 331, 332 (each needs 329)   (330 also needs 323 for warped nodes; 332 needs 313)
           338 (needs 329 and 330; it belongs to phase E but was added last)
F item     333 (needs 313, 315, 326) > 334 (needs 322, 327, 328, 331)
G close    335 (needs B to F and 338) > 336 > 337 > 339 (the AI, last)
H gaps     341 (needs 315, 317 and 325; its AI part is done with 339's AI work, after 339 if 339 comes first)
           342 (needs 337; clean-up of `content.rs` once the structure is final)
```

Phases B and C meet only at 315 (the wizard's spell lists) and 322 (the cast in progress). Phase E
needs only the player entity, so it can run in parallel with C and D; phase F needs both its
neighbours.

## Table

| # | Area | Summary |
|---|---|---|
| 358 | Text | Names in the rules files (units, spells, buildings, races, realms, abilities, item powers) per language |
| 350 | Text | Refusals and notices the original has and ours lacks (continue-casting prompts, unit limits, Spell Blast mana, nodes, and more) |
| 351 | Text | Research the `MESSAGE.LBX` lines of task 348 that are not traced: 8, 10, 18, 19, 20, 45, 46, 61 |
| 357 | Client | Site discovery: the music and the colour slide of the node pictures |
| 363 | Tools | Render the original's XMI music as the Roland MT-32 played it, with `mt32-rs` and the user's ROMs |
| 364 | Client | The scenes with music that the client lacks: the summon scene, the city-spell scroll and the Force of Nature scene |
| 365 | Client | Crossfade the music between tracks (ours; the original cuts) |
| 359 | Client | Replace pictures with text baked in by text-free pictures plus font text, for localization |
| 360 | Client | Conquest, banishment and defeat scenes |
| 366 | AI | Objectives, step 3: building weights by category and casting as weighted categories |
| 367 | AI | Overland spells, heroes and items the AI never uses |
| 368 | Diplomacy | Human-only branches in the diplomacy rules: decide each (move to the AI or keep as a rule) |
| 369 | AI | The AI hardly fights and stops expanding (wars that end, marches, the city target) |
| 370 | AI | Battle AI: retreat, sally, weigh a lair before attacking it |
| 371 | AI | The AI plays to win: more wars, cities, casting, items and heroes, measured by the soak |
| 373 | Battle | Ship graphics for sea battles (isometric decks and hulls in place of plain brown squares) |
| 375 | Client | Wire the five Settings switches that do nothing yet |
| 384 | AI | Diplomacy leftovers: threats, demands, greetings, get off my lawn, superiority wars, tuning |
| 385 | Meta | Diplomacy leftovers of the rules and the client: Threaten to Attack, trespass, map size, picking sides, attack-on-partner box |

## Descriptions

Today `Game.map` (outside the world) holds a `FeatureId` per tile, `Map.nodes`, `Map.lairs`,
`tower_owners` and an owner byte per volcano. Nodes and lairs are found by `Vec` index
(`Entry::Site(usize)`, `lair_at`, `node_at`), which breaks when one is removed.

### H. Gaps found during the rework

**358. Names in the rules files per language.** The names in `units.ron`, `spells.ron`, `buildings.ron`, `races.ron`,
`realms.ron`, `abilities.ron` and `item_powers.ron` are shown as they are (`Item::display_name` and the screens read
`def.name`). Decide and build how a language gets its own: a table of names by id in `content/text/<lang>/` that the
display sites read first (falling back to the file's name), or a copy of each file per language. Then every place that
prints a `def.name` goes through it, and the plural of a race (the original keeps it apart) needs its own form.

*Needs:* 347.

**360. Conquest, banishment and defeat scenes (client).** From task 341: the original's conquest
animation, the "begins casting the Spell of Return" scene for a banished computer wizard, the defeat
and victory scenes and the banished wizard's portrait. Read `Conquest_Animation`, `Return_Animation`,
`Lose_Animation` and `Win_Animation` (ReMoM `CONQUEST.c`) first. *Needs:* 341 (done).
**Done in task 134** (`end_scene.rs`, `docs/game_end.md`): the victory, the defeat, the conquest scene with the raiders,
the Spell of Return scene and the parade of portraits for the human's Spell of Mastery. **Left:** the scene a rival's
Spell of Mastery brings (`Spell_Of_Mastery_Lose`, ReMoM `SPLMASTR.c`: `spellose.lbx`, the wizard's picture sheared and
shrunk into the sphere of their colour, the familiar in the corner) before the defeat; the zap of a wizard banished by
another wizard, which the original does not show (its early return) and which we could add as our own; and a hand check
of the scenes in a real game (the shots were taken from `--scene`).

**350. Refusals and notices the original has and ours lacks.** From the table of task 348: the "continue casting?" prompts (lines 12 to 16, including the second cast of a global enchantment, task 349 leaves it here); the settler refusal for a fortress's last citizens (11); too many units on the square when a spell summons or a city builds (21, 50, 66); Disenchant Area with nothing to dispel (28); Spell Blast without the mana (68); Change Terrain on a node with line 34's own words (ours refuses it as a fixed terrain; Corruption on a node, 41, is done); Volcano on a river (86; check first whether the map has rivers); Web and Sorcery immunity refused at the aim, where ours casts and the unit resists (71, 73); Lycanthropy on a mutated unit; the mass battle spells with nothing to hit (79, 80, 81, 83); wizard targets without contact (27); the notices of dispelled enchantments (18, 19, 20, 45, 46) and the monsters' rampage (10), whose triggers must be found first. Decide for each: add it, or record in `docs/divergences.md` that it is not wanted. The new texts are keys in `content/text/`.

*Needs:* 348 (done).

**351. Research the untraced `MESSAGE.LBX` lines.** Task 348, which is finished, could not say what rule stands behind these lines of the original: 8 ("All units retreat exhausted"), 10 (monsters rampage through the city), 18, 19, 20, 45, 46 (notices that other city, unit, Stasis or combat enchantments were dispelled) and 61 (Guise on a hero; no spell of that name in `spells.ron`). For each: find the original code that shows the message (ReMoM) and what triggers it, check whether our game has the rule, and write the answer in this task's "Done" paragraph as in the table of task 348: refused, no refusal, different on purpose, or no such rule. A missing rule goes to task 350 or a task of its own; the text is a key in `content/text/`.

*Needs:* 348 (done).

**357. Site discovery: the music and the node colour slide.** The original plays a short discovery tune when a stack
finds a site and slides the colours of a node's picture (`docs/lairs.md`, "Missing"). Add both to the site prompt
(`f4x-client/src/site_prompt.rs`) where the client has the music and image support for it; text stays in
`content/text/en/site.ron`.

*Needs:* nothing.

**359. Pictures with text baked in become plain pictures plus font text.** Many of the original's pictures carry
words in the bitmap itself: button faces ("Done", "Patrol", "Buy", "OK"), screen titles and headings, the labels on the
magic, city, army, diplomacy and load screens, and the unit and spell names on cards. The client draws these as they
are, so a language other than English would still show English words on them (and the original's font cannot draw
every script). Make every such picture text-free and draw its words from `content/text/<lang>/` in a font, as the rest
of the player text already is (`docs/text.md`):
1. **Find them.** List the pictures that contain text, per screen: start from `crates/f4x-tools/screens.tsv` and the
   labelled files in `assets-local/art` (`main_screen/button_done_00.png` and the like), look at each, and write the
   list (file, screen, the words, where the client draws it) into a table in `docs/extract-art.md`. Say which of them
   the original itself draws as text over a plain picture (those need nothing) and which are baked.
2. **Make them plain.** For each baked picture, a text-free version: a tool in `f4x-tools` that blanks the text area
   of the extracted picture (the plain button face, banner or plaque), or a redraw where a blank cannot be cut out.
   The extractor writes the plain version; the baked one is not used by the client any more. Log the tool in
   `docs/scripts.md` with `--help` as the project rules say.
3. **Draw the words.** The client draws the text on the plain picture, centred in the area the old words filled, with
   a key in `content/text/en/*.ron` for each (count, plural and gender forms where the language needs them). Button
   labels shrink or wrap to fit; a test fails when a key is missing, as for other text.
4. **Fonts.** The original's bitmap fonts (`FONTS.LBX`, `docs/extract-art.md` section 5) hold only ASCII 32 to 127.
   Decide and build the font the localized text uses: a vector font loaded by the client (a file in `assets/` with
   its licence noted) for languages with accents or other scripts, and the bitmap font kept for English where it
   keeps the look. A language names its font in `content/text/<lang>/`'s settings.
5. **Check.** A screenshot of each changed screen in English matches the old look closely (position and size of the
   words); a second language, even a test one with long words, shows no overflow.

*Needs:* nothing for 1 to 3; 358 is the same job for the names in the rules files and can share the fonts of step 4.

Already tracked elsewhere (not repeated): spell and book treasure that matches the wizard's books is on-hold task 305;
checking the lair data against `MONSTER.LBX` and `EVENTS.LBX` is on-hold task 241; where loot items are made (at
clearing today) belongs to task 334; a freed prisoner needs no square because the hire is free, so nothing is open there.

**363. Render the original's music as the Roland MT-32 played it.** The songs are XMI (AIL Extended MIDI), written by
task 362. Turn them into audio the client can play, using `https://github.com/CopperlineHQ/mt32-rs` (a safe-Rust port
of Munt; LGPL-2.1-or-later; the notes are in `docs/sound.md`, "mt32-rs") with the control and PCM ROMs of a real
MT-32 that the author supplies (`assets-local/`, never committed; the revision to use, new or old, is the author's
pick). Steps: (1) a spike: convert one song (`104_main_menu.xmi`) and render it, to see whether the timbres of
`timbres/fat.mt` are needed (the DOS drivers uploaded them to the unit by SysEx before a song) and whether the
result sounds right; (2) the XMI to standard MIDI converter: try the `cnc-formats` crate first (`xmi` feature: `XmiFile`, `XmiSequence`,
`XmiTimbre` and `to_mid()`; MIT or Apache-2.0, 0.1.0-alpha.5 of 2026-03-26, github.com/iron-curtain-engine/cnc-formats;
its docs do not say how it treats `FOR`/`NEXT` loops and branches, or several sequences in one file, so test it on
all 118 songs and write our own if it falls short: delta times are the XMI's delay bytes, a note carries its
duration, loops become a loop point). Its `XmiTimbre` list of a song's `TIMB` chunk also answers the question of
step (1): which timbres of `fat.mt` each song needs;
(3) a `f4x-tools render-music` step that renders every song to OGG with a loop point, in `assets-local/audio/music/`;
(4) keep the dependency out of the shipped game (the extractor only), because of the LGPL; pin a git commit, since
the crate has no release. The ROM path is an argument and the step fails with a message if a ROM is missing.

The opening movie (task 85, done) plays without its song: `music/112_intro.xmi` (also `introsnd/000.xmi`) starts
with it in `Draw_Logos`; wire it when this task can play XMI.

*Needs:* 362 (done), the ROMs from the author.

**364. Scenes with their own music.** Three scenes of the original that the client does not have, each with the music
`sound.md` section 3.3 lists (the audio layer of tasks 76 and 97 is built; each scene only names its track and cast
sound in `audio.rs`): (1) the **summon scene** (`Summon_Animation`, `Spells137.c`; the `SPELLSCR` summon background of the
unit's race, 46 + race, the caster's figure and the flame frame `cmbtfx` 22, the music of the unit's rarity, 0 weak to 3
mighty: `Weak/Avg/Powerful/Mighty_Summon`, entries 13 to 16), shown when a summoning spell completes, for the human's casts and
for a rival's with Summoning Events on; the rarity comes from the spell's rarity or the unit's cost, decide and
write it in `sound.md`. (2) The **city-spell scroll** (`Cast_Spell_City_Enchantment_Animation_1` and `_2`; `SPELLSCR` 73
with the city's cityscape, "You cast X" or "<wizard> casts X", with the realm's good or bad piece, entries 0 to 11:
`Good_<Realm>` 0 to 5, `Bad_<Realm>` 6 to 11, bad for a curse), shown for the city spells with City Spell Events on.
(3) The **Force of Nature scene** (`Spells129.c`, music entry 27): no spell of that name is in `spells.ron`, so find
which spell or effect the original plays it for in ReMoM and whether we have it; if it is Nature's Wrath or an
effect we do not have, say so in `sound.md` and drop the scene. Each scene follows the Spell Animations and the
matching Events preferences, and plays its piece through `Track::piece`.

*Needs:* 76, 97 (done).

**365. Crossfade the music.** Ours; the original stops all sound and starts the next piece. Fade the old track out and
the new one in over about a second when the music changes (a screen, a battle, a short piece and back to the
background), so the changes are less abrupt; short pieces fade the background out and in again on their own end.
`Audio::music` in `crates/f4x-client/src/audio.rs` is the one place that changes tracks; macroquad's audio has
`set_sound_volume`, so the fade is volume steps per frame on the two sounds, and the old sound is kept loaded until it is
silent. A switch under Settings ("Fade Music") turns it off for those who want the original's cuts.

*Needs:* 76 (done).

### I. Found by the AI check of 24 October 2026

The numbers and the reasons are in `ai-design.md` section 12.

**366. Objectives, step 3.** The original weights the buildings of a city by category and objective
(`AIBUILD.c`) and casts overland spells as weighted categories (`AIDUDES.c`); here the plan of a city
takes only a focus pair (`objective.rs`) and `wizardry::cast` tries kinds in a fixed order. Design
(`ai-objectives.md` step 3), build, soak. *Needs:* nothing.

**367. Overland spells, heroes and items the AI never uses.** Curses and attacks on enemy cities and
stacks, city protection and wards against what an enemy casts, Spell of Return and the banished wizard's
casts, Enchant Item and Create Artifact, the special spells (Word of Recall, Spell Blast and the like),
and research by what the enemy casts. Heroes: keep them out of lair fights they cannot win, spend
their experience, raise them with Resurrection; items: trade and craft. Each as a function of what the
AI sees; each gets a test and a soak line. *Needs:* nothing.

**368. Human-only branches in the diplomacy rules.** List in `ai-design.md` section 12.3 item 6. For each:
keep as a rule of the game with a reason (the human has no feelings to track; a UI hint), move it to the AI
(the AI's mood towards a big human army is the AI's), or make it apply to every pair of wizards. Record in
`diplomacy.md` and `divergences.md`. *Needs:* nothing.

**369. The AI hardly fights and stops expanding.** The outpost rate was a bug (10 tens to a city, not 100, fixed 24 October 2026; re-run the soak first). After task 361
(proposals): the march thresholds in `military.rs`, wars that end (peace offers, a war lasting longer than
its reason), the city target by objective once land is free, settlers by sea, using the gold (50 to 100
in the treasury while units are short). Measure with the soak (`ai-design.md` 12.2). *Needs:* 361.

**370. Battle AI: retreat, sally, weigh a lair.** The battle AI never flees, does not sally from a walled
city, and `site.rs` attacks sites by a strength guess. Add a retreat when a fight is lost, a sally when the
attacker is weak at the gate, and a better guess of a lair's guards (what the wizard has seen of them).
Check with `examples/situations.rs` and the mirror test. *Needs:* nothing.

**371. The AI plays to win.** The umbrella over tasks 361, 366, 367, 369 and 370 and planned item 21: today
a two-wizard game of 300 turns ends in a win once in eight (`ai-design.md` section 12.2), wizards stop at 5
to 7 cities, the treasury and the mana sit unspent and wars are rare. The aim is games that end, by
conquest or by the Spell of Mastery, because the AI goes for it. The parts, each a function of what the AI
sees (design.md section 8), each with a test and a soak line:
- **Wars and fights** (369, 361): march when ahead and at a measured edge, keep a war going until it is
  won or ended by an offer, attack the weakest neighbour first, take lairs and nodes for their treasure,
  defend what is worth defending (370).
- **Expansion** (369, planned 21): settle until the land is full, outposts grow (the original's rate since the 24 October 2026 fix; Housing does nothing in one),
  settlers by sea and to the other plane, found cities on good ground first (food, minerals, nodes).
- **Magic cast** (367, 366): spend the mana each turn (the reserve should not climb past a few turns of
  upkeep), cast curses and attacks on enemy cities and stacks, protect the own cities, keep a spell in
  the making at all times, use the nodes; research toward the Spell of Mastery once the army is safe.
- **Item crafting** (367): Enchant Item and Create Artifact when the mana allows, items to the hero that
  uses them best, scrap what no hero can use.
- **Heroes** (367): hire every hero offered that the gold allows, cast Summon Hero and Summon Champion,
  raise the dead, give them the best items, send them to lairs and into battles they win, level them.
- **Gold** (369): spend it (buy production, hire, buy items) instead of holding 50 to 100.

**373. Ship graphics for sea battles.** Task 233 draws each ship's deck as plain brown diamonds
(`battle_ground.rs`, `SHIP_DECK`) with the deck shapes from `ship_shape` (pointed ends; trireme 3 by 7,
galley 4 by 8, warship 5 by 9, Floating Island 5 by 5, in `units.ron`). Give them real pictures: a
hull and deck drawn for the isometric diamond grid (the field's cells are 32 by 16 pixels; a deck is
a rectangle in cell axes, so it looks diagonal on screen), a bow and stern for the pointed ends, a
rail where the gap is jumped, a sunk hull when a ship dies, and a picture for the Floating Island's
deck. The LPC ship pack ("[LPC] Ship" by bluecarrot16, OGA-BY 3.0+ or CC-BY 3.0+ or GPL 2.0+, in
`reference/lpc-ship/`, notice in `reference/CREDITS.md`) was tried and does not fit: it is flat top-down 32 by 32 tiles, not isometric. Options to
settle: draw our own pixel art, reproject the pack's tiles to the diamond grid, or find an isometric
set (check its licence and credit it in a `CREDITS.md` that is in the repository once art is used). Keep the deck cells as the rules' truth; only
the drawing changes. *Needs:* nothing.

**375. Wire the five Settings switches that do nothing yet.** City Spell Events, Overland Spell Events,
Summoning Events, Auto Unit Information and Enemy Moves are stored
(`preferences.rs`) and shown on the Settings screen but read by nothing. (Spell Book Ordering is gone: the
spell book has its own sort and filter lines.) Their help (topics 678 to 680, 686, 687) already says
what each should do, as the original does; make the client do it. Also give Left Click Moves Units and
Play Intro a help topic and a help area on the Settings screen (they have none, the area list comes
from the original). *Needs:* nothing.

**384. Diplomacy leftovers of the AI.** Task 361 made the AI propose (to the human at the start of its turn, to the other AI wizards with the chance and score of `NPC_To_NPC_Treaty_Negotiations`) with basic rules. What it leaves out, from `docs/diplomacy.md` sections 4, 6 and 8: (1) the war-or-peace decision with warnings (`DIPL_HumanWarOrPeace`: a counter of warnings, a roll to declare war or break the treaty; today a relation of -75 declares war and -50 breaks a treaty at once); (2) demands for tribute and the AI's reaction to the human's threats (`Threaten to Attack` is not a command yet); (3) the greeting of a wizard newly met, by temper; (4) `DIPL_GetOffMyLawn` (units next to a city, three warnings); (5) the need-for-war rolls (`G_DIPL_NeedForWar`: Chaotic wizards, superiority wars, joining an ally's war against the human, the `Random(15) <= -rel / 10` war); (6) the AI's gifts of gold and spells (`GiveGold`, `GiveSpell`) as tribute or thanks; (7) tuning the chances and weights with the soak (`ai-design.md` 12.2: treaties formed, wars started, who wins), and whether AI wizards in a pact or alliance stay out of each other's way in `race::war_target`.

**385. Diplomacy leftovers of the rules and the client.** What is left of task 167 after the checks of 24 October 2026 (the AI's own talks are task 384; AI-to-AI treaties, the AI's offers to the human and an AI that does not march on a partner are done). (1) **Threaten to Attack** (`docs/diplomacy.md` section 4): the Threaten menu of `diplomacy_screen.rs` has only the two break-treaty rows; add the threat as a `DiplomacyCommand`, the human's loss of `Random(15)` relation, the treaty patience of -120 and the wizard's reaction (below 0 war, 0 to 89 a refusal, 90 to 199 a payment of `25 * (2..9)` gold and a peace of `Random(15)` turns, 200 or more a spell), as AI code in `f4x-ai` over a command the AI answers. (2) **Trespass** (`DIPL_GetOffMyLawn`, section 8): `Action::Trespass` exists and nothing raises it; units of a wizard within two squares of another's city give a mild relation loss and a warning, and on the third step the treaty is broken with -5 (not between wizards at war; the settlers do not count). (3) **The map size** in the "too many cities" reaction: `LAND_SIZE` in `diplomacy/mod.rs` assumes a medium map; keep the game's land and map size and use them. (4) **`G_DIPL_PickSides`**: an alliance breaks when one ally is at war with the other's ally. (5) **Client:** the fade-in and fade-out of the diplomacy room (`diplomac` entries 38 and up); the confirm box "You have a treaty with X. Do you still wish to attack?" before an attack on a pact or alliance partner; the readable colour of disabled menu rows. (The right-click on a wizard's gem already opens the sheet.) *Needs:* nothing; (1) shares the reaction code with 384.
