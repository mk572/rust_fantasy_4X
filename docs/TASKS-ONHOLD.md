# Tasks on hold

Everything not yet implemented, to be addressed after TASKS.md is emptied. 
Keep the table in ascending order by number; new tasks go at their numeric place
(or take the next free number). Bracketed numbers like [430] point at topics in the original help
(`content-local/help.ron`); B/C numbers point at `help-vs-implementation-conflicts.md`.

Finished tasks are moved to [`TASKS-DONE.md`](TASKS-DONE.md) with
`python3 scripts/archive_done_tasks.py` (a task is done when its row or title says "(done)").

## Table

| \# | Area | Summary |
| --- | --- | --- |
| 81 | Housekeeping | Packaging for Mac |
| 95 | Client | Tutorial |
| 115 | Client | Cityscape: road masking |
| 172 | Meta | Learning AI: battle AI follow-ups |
| 176 | Housekeeping | Mac installer: asks for the original files, extracts, builds a local app |
| 223 | Heroes | Hero and item help conflicts: record H1 to H25 and settle the Check rows |
| 228 | Meta | Scripted AI steps 4 and 5: casting beyond summons, the win spell, special spells |
| 229 | Meta | Scripted AI steps 6 and 7: war targets, memory, defence, shared strength |
| 230 | Meta | Scripted AI step 8: ships and other land masses |
| 232 | Combat | Conventional battle AI steps 10 and 11: target values, focus fire, defence, fleeing |
| 256 | Combat | Unit spell-like abilities: Fireball, Healing, Doom Bolt and Web spells |
| 285 | Client | Spell interface: Stasis, mutations, vortices, Time Stop |
| 286 | Rules | Leftovers of the last ten spells (unrest, Time Stop, Call the Void) |
| 291 | Combat | Variant river pictures for the battlefield (low priority) |
| 299 | Client | Minimap: drag to scroll, and the other plane |
| 300 | Client | Bitmap fonts for the battle, settings, console and prompt screens |
| 303 | Core | Random events: what is left after task 68 |
| 304 | AI | Neutral city AI: goals and objectives, and units that move |
| 306 | Client | Overland spell animations (summons, city and global spells, map squares) |
| 351 | Text | Research the `MESSAGE.LBX` lines of task 348 that are not traced: 8, 10, 18, 19, 20, 45, 46, 61 |
| 357 | Client | Site discovery: the music and the colour slide of the node pictures |
| 358 | Text | Names in the rules files (units, spells, buildings, races, realms, abilities, item powers) per language |
| 359 | Client | Replace pictures with text baked in by text-free pictures plus font text, for localization |
| 360 | Client | Conquest, banishment and defeat scenes |
| 364 | Client | The scenes with music that the client lacks: the summon scene, the city-spell scroll and the Force of Nature scene |
| 365 | Client | Crossfade the music between tracks (ours; the original cuts) |
| 367 | AI | Overland spells, heroes and items the AI never uses |
| 368 | Diplomacy | Human-only branches in the diplomacy rules: decide each (move to the AI or keep as a rule) |
| 369 | AI | The AI hardly fights and stops expanding (wars that end, marches, the city target) |
| 370 | AI | Battle AI: retreat, sally, weigh a lair before attacking it |
| 371 | AI | The AI plays to win: more wars, cities, casting, items and heroes, measured by the soak |
| 373 | Battle | Ship graphics for sea battles (isometric decks and hulls in place of plain brown squares) |
| 375 | Client | Wire the five Settings switches that do nothing yet |
| 377 | Client | Unit statistics window: what is left |
| 378 | Client | Turn summary and message pop-ups: what is left |
| 379 | Meta | Settings: races banned for computer wizards and independent cities, by difficulty |
| 380 | Map | Check the original generator's port against real maps of the original |
| 382 | Meta | Code optimization and speedup: measure, then fix the hot spots |
| 383 | Client | Battle spell animations: the pictures the casting leftovers still lack |
| 384 | Client | Pop-up leftovers: Hero and Champion Summoned boxes, hire sheet checks |
| 385 | Client | Help pop-ups: the topics, areas and screens still open |
| 386 | Client | Magic screen: scroll the enemy wizard portraits |
| 388 | Magic | Spell Images |

## Descriptions

### Client, content, housekeeping

**81. Packaging for Mac.** (A local-play `.app` exists: `scripts/make_app.sh`, see `docs/local-assets.md`; the rest below is for distribution.) A macOS build: a `.app` bundle (icon, `Info.plist`), universal or
per-architecture binaries (Apple silicon and Intel), code signing and notarisation if the
author has a developer account (otherwise documented right-click-open steps), a disk image or
zip, and the local-assets workflow documented for players who supply their own original files
(`docs/local-assets.md`): where the game looks for `assets-local/` and `content-local/`, and how
to run the importer with the system Python. A release workflow on GitHub Actions if feasible.

**95. Tutorial.** A first-game guide: highlighted steps for moving, founding a city, building,
researching and casting, with a way to skip or switch it off.

**115. Cityscape: road masking.** The city picture draws the whole road grid; the original hides the road squares that no building, house or tree touches (`Cityscape_Roads_1` to `_5` in ReMoM `CITYSCAP.c`, using the road pictures and the two masks `cityscap.lbx` 109 and 110), so roads appear only where the city has grown. Port the masking rules.

**172. Learning AI: battle AI follow-ups.** What tasks 170 and 171 left (`learning-ai.md` 7.7 and 7.8): (1) stronger yardsticks than the scripted AI, which random orders already beat: past checkpoints as opponents (the league of 6.4), Elo, and an evaluation as the defender; (2) the agent chooses **which unit** moves (a pointer over the units) and can cast later in the turn, not only at its start; (3) fleeing: it is found only with the help of a head that compares the value of fleeing with the critic's, and it still flees in fights it would win; (4) longer runs and a checkpoint chosen by its held-out score; (5) the AI wizards' real battles as training data (task 8.3), difficulty levels from checkpoints, and a client setting for `--battle-ai` (the client needs the `learned` feature and a checkpoint file); (6) the battle outcome model for the strategic AI (8.4); (7) a gap the first runs show: the odds classes were measured with the scripted AI, so a better player makes them easy. (8) The bank was made before the undead, experience and neutral-unit changes of 13 October, and no run has trained on **Animate Dead** (the agent can now aim it: the fallen have tokens); remake the bank and train again. Remaking the bank also picks up the 11 spells of eligibility 2 and 3 that are castable in battle now (task 55).

**176. Mac installer.** One program (a script, or a small installer app) that sets the game up on a Mac from the player's own copy of the original: it asks for the path to the original files, runs the extraction (the art with `f4x-tools extract-art`, and the importer `scripts/import_mom.py` for the help text, wizards and city names into `assets-local/` and `content-local/`; `docs/local-assets.md`), checks that the files are the right ones and says what is missing, and then builds the local app (`scripts/make_app.sh` makes `target/Fantasy4X.app`). It should also say what it needs first (the Rust toolchain through rustup, the system Python) and offer to stop there instead of installing anything itself, report progress and errors in plain words, and leave the app and its data where the player can find them (perhaps copying the app to Applications). Nothing from the original is ever uploaded or committed; the result is for play on that machine only. Builds on task 81 (packaging for distribution, which carries no original files).

**223. Hero and item help conflicts.** Section 7 of [`heroes-items-design.md`](heroes-items-design.md) lists 25 places where the help, the original's code and ReMoM disagree (Leadership per two or three levels, Constitution's defence, Agility without a help topic, Sage's super factor, Famous's cap, Lucky, item attack by kind, and more). Add them to `help-vs-implementation-conflicts.md` with the next B numbers, read the code again for each row marked Check (Leadership, Legendary's price cut, Sage, Lucky, Charismatic's order, Recall Hero overland, the number of powers in Enchant Item, hero 24's name, whether a slain hero can be resurrected, where offers fall in the turn), record the decision, and write `HELP_CORRECTIONS` notes where we deviate from the help. Do this before the steps that depend on each row. **Decided 24 October 2026 (user):** follow the code for the rows marked Check, except: Sage's super is `Level * 9 / 2` research (H5); Lucky is exactly +10% to hit, +10% to block and +1 resistance for the hero and -10% to hit for whoever attacks it (H8); Torin is not Noble by default but can roll it (H10), and costs 12 mana upkeep a turn, not gold; hero 24's name "Unknown" is right (Mystic X the Unknown, H25). Done in the code and in `heroes-items-design.md`. Still to do: the other Check rows (Leadership, Legendary's price, Charismatic's order, Recall Hero overland, Enchant Item's powers, slain heroes, where offers fall), the B numbers and the `HELP_CORRECTIONS` notes.

**228. Scripted AI steps 4 and 5: casting (partly done).** Done in `wizardry.rs`: the kinds the AI casts (Spell of Mastery, summons, unit and city buffs, good globals, Earth Lore, Nature's Cures), paid over as many turns as the casting skill needs, the Spell of Mastery never abandoned; tested with a wizard that knows it (`behaviour.rs`) and `soak --grant-mastery`. Done 24 October 2026: **curses on enemies at war**: city spells (Famine, Cursed Lands, Pestilence, Evil Presence, Earthquake, Chaos Rift, Call the Void) go on the most populous known city of a wizard the AI is at war with, and wizard spells (Drain Power, Subversion, Cruel Unminding) on a rival at war, from mana above the 200 reserve (`Kind::CityCurse`, `Kind::WizardCurse`; test in `wizardry.rs`). Still open: the spells with no target (Stasis, Chaos Channels, Magic Vortex, Time Stop, Meteor Storm, Great Wasting, Armageddon and the rest of task 286's list), Word of Recall, Transmute and Plane Shift, disenchant and disjunction (Spell Blast, Disjunction, Spell Binding), the original's weighted pick among kinds. Sections 5.3 and 7.3. The eleven category weights with our own pickers, the affordability check, the mana gate, the special spells of task 56 (Earth Lore, Word of Recall, Nature's Cures, Transmute, Plane Shift), and the Spell of Mastery: weight 500 when known, never abandoned, upkeep shed first.

**229. Scripted AI steps 6 and 7: war (partly done).** Done in `military.rs` and `economy.rs`: a march state, gathering within two squares, chunks of nine, unreachable targets dropped for 40 turns, remembered garrisons and a lightly-held target preferred, a strike force before buildings, units re-chosen every turn. Still open: defence of threatened cities, a shared strength function for the overland and battle AI, an army size from the war plan. Sections 5.2 and 5.4. Memory of enemy garrisons, target wizard then city, a stack that suits the target, defence of threatened cities, army size from the war plan, income-first buildings, a strength function shared with the battle AI.

**230. Scripted AI step 8: ships (not started).** What was learned is in `ai-design.md` 11.3: four of eight soak seeds put the wizards on separate land masses or planes.  Section 5.2 and 10. About half the soak seeds put the wizards on separate land masses or planes; without transports those games cannot end by conquest. Needs `transport.rs` and `movement.rs` queries on the view.

**232. Conventional battle AI steps 10 and 11 (partly done).** Done in `battle/ai.rs`: targets by value (`expected_damage`, `worth`), focus fire, shooters hold, defenders hold the walls; the old AI is `ai::baseline_command`. These unit tactics are within the dice's spread (123 to 117 in mirrored fights); only the wizard's spells showed a gain. Still open: sallying out, fleeing, kiting, not stepping into a worse fight. Sections 6.3 and 6.4. Target values after ReMoM's `Target_Unit_Value`, focus fire, no steps into worse fights, kiting, holding the walls, sallying out, fleeing when hopeless. Run the learn tests at each step; `learning-ai.md` section 13 gets a note.

**256. Unit spell-like abilities: Fireball, Healing, Doom Bolt and Web spells.** Twelve non-hero units carry `casts_fireball` (Magicians of six races), `casts_healing` (the four races' Priests), `casts_doom_bolt` (Dark Elf Warlocks) or `casts_web` (Giant Spiders), and no code reads these abilities (`abilities.ron` only describes them); their ranged attacks (`fireball_attack`, `shaman_bolt`) work, but the spell does not exist. Help (entries 421 to 423 and 426): Healing heals a target unit by 5 hit points at most; Fireball is a strength 5 explosion hitting every figure of the target (the help adds one strength per 3 extra mana, which a unit cannot pay); Doom Bolt does 10 points ignoring defence; Web is a 12 hit point web (full melee and magic ranged damage breaks it; flying units stay grounded afterwards). ReMoM (`CMBTAI.c` 440 to 460 and 1100 to 1150, `Combat.c` 3659 to 3675) keeps each as a flag on the battle unit that is cleared when the spell is cast, so the spell can be cast once per battle, without mana, instead of the unit's attack, and the AI values Fireball and Doom Bolt by 16 minus the target's defence. Port: a once-per-battle cast command for the unit (reusing the Fireball, Healing, Doom Bolt and Web spells already in `battle/spells.rs`), the buttons or console command for it, the battle AI and learning AI's use of it (`legal_commands` and the observation grow, so retrain; ask first), a test per spell, the `auto` placeholder AI, and the unit window text. Check first whether the original lets a human cast Healing when the AI cannot (`CMBTAI.c` 462 notes the AI never executes it: an original bug, so do not copy it).

**285. Spell interface for the last ten spells.** **Done 30 October 2026:** the Stasis marker (a pale blue tint on the map, "Held in Stasis" on the unit sheet), the mutation named on the unit sheet (Demon Skin, Wings or Fire Breath, by a marker ability), Meteor Storm / Call the Void / Nature's Wrath in the Chancellor's scroll ("SPELLS THAT STRUCK"), and the Time Stop note over the Next Turn button. Left: the vortices drawn as the original's animation with a cursor for the steering move and the "Move Magic Vortex 1 Space" button. Was: the client lacks: a Stasis marker and text on the unit sheet and a pale tint on the map; the vortices drawn as the original's animation (now rings) with a cursor for the steering move and a "Move Magic Vortex 1 Space" button as in `Combat.c`; the mutation (armour, wings or breath) named on the unit sheet; messages for Meteor Storm, Call the Void and Nature's Wrath in the Chancellor's scroll and the original's spell animations and sounds; a note when Time Stop is on (the Next Turn button, the other wizards frozen).

**286. Leftovers of the last ten spells.** **Done 24 October 2026:** (1) Great Wasting's unrest, (2) Time Stop's other commands (founding is refused, the AI waits), (4) Stasis in a battle (the original never reads it; we keep the help's reading). (3) Call the Void's loss of citizens is in (about half, never the last; B40 follows the code). Left: (5) the AI's casting. Was: (1) Great Wasting's unrest in other wizards' cities (-1, `CITYCALC.c`, written there as an assignment that looks like a bug); Armageddon's own unrest effect is done in another form (no building calms any city). (2) Time Stop: block the other wizards' non-movement commands (founding a city, production and casting orders) and have the AI skip its turn. (3) Call the Void's loss of citizens, if wanted (conflicts B40). (4) Stasis in a battle is only "asleep"; check the original. (5) The AI casting and scoring the new spells (task 56) and steering a vortex.

**291. Variant river pictures for the battlefield (low priority).** Each river shape has two pictures in the original, a first variant (`cmbtcity` 97 to 102) and a second (103 to 108), chosen at random per square (`56 + 6 * variant + shape` in `Look::tiles`). The shipped files hold only the second variant, and Myrror's `chriver` 0 to 11 are empty, so every river square of a shape looks the same (`river_art.rs`, `crates/f4x-client/assets/rivers/`). Make a first variant by mirroring or rotating the existing six pictures (a left-right piece mirrored is up-down in the other diagonal, corners swap in pairs; check each against a screenshot so the banks still join), generate it once for both planes, keep the PNG files in `assets/rivers/` like the others, and have `river_art.png` pick the variant from the tile number. Until then the client takes the shape from `tile % 6` and ignores the variant. Asked for by the user, 17 October 2026; follows task 255, now archived.

**299. Minimap: drag to scroll, and the other plane.** The minimap centres the map on a click; the original also scrolls while the button is held and shows the other plane through the Plane button's behaviour on it. Check the original's `Main_Screen` input for the minimap field before building it. Split from task 120.

**300. Bitmap fonts for the battle, settings, console and prompt screens.** Tasks 118 and 126 moved the magic, wizard, production, unit and overland text to the original's fonts (`Bitmap_wrap` is `bm_wrap`). Still in the plain font: the battle screen's panels and buttons (`battle_screen.rs`), the settings, options and console screens, the attack prompt and autobuild dialog, the road planner, and the plain `ui::button` / `confirm_box` fallbacks. Move them over with the original's box and button art where it has some.

**303. Random events: what is left after task 68.** The events work (see `events.md`); these parts do not yet: (1) **help**: the world event's name under the minimap and the city's Plague and Pop. Boom marks should open help topics 768 to 775 on a right-click (`HLP_PLAGUE`...); (2) the **reports** (`REPORT.c`, the Chancellor's and the others' lines that name the running plague, boom and world events); (3) the **music** of the event box (`MUSIC_Events`, `MUSIC_Bad_Event`, by task 76); (4) **Depletion and New Minerals** pick the square by our own rule (a square in the area of one of the victim's cities, the mineral by its spawn weights), because ReMoM stubs both functions and the original's are said to be bugged, so any fix is fine; (5) decide the remaining quirks of `events.md` (a Population Boom triples where the original quadruples with Stream of Life, the Pirates formula, the rising end chance of the world events against the 5 percent in the help) and write the departures into `divergences.md`; (6) a **soak** that watches the events over 500 turns, when asked for.

**304. Neutral city AI: goals and objectives, and units that move.** The independent cities are played by `f4x-ai/src/neutral.rs` (`play_neutrals`, written with task 135): farmers, taxes, a Barracks first, then soldiers while the garrison is short and buildings after, from a fixed rule with no goals, and nothing ever moves. Make it a real AI, in the way of the wizards' (`ai-design.md`, `ai-objectives.md`). (1) **Goals and objectives.** Each city gets an objective it works towards and that steers its builds and spending, for instance Fortress (walls, guilds, a strong garrison), Trader (gold and growth), Breeder (population), Warband (units to send out), with the weights in one table as in `ai-objectives.md`; a city may change its objective when its situation changes (a rival wizard's army near, a lost garrison, a good treasury). The objective is state kept by the AI (`Ai` per city, or a small `Neutrals` holder in the host like the wizards' AIs, saved with the game or rebuilt from the cities). (2) **Moving units.** Garrison units may leave the city and return: step out to drive off a weaker stack beside the city, retake a square, escort a settler or caravan if there are any, and go home when a stronger enemy comes; never leave a city with fewer defenders than it wants. Units a city sends away must still be paid for by its own treasury. (3) **What the original does.** Read ReMoM's neutral turn before designing (`AI_Next_Turn` after the wizards: the neutral player's colony autobuild, farmers, destinations and movement, then the raiders and monsters: `AIBUILD.c`, `AIDATA.c`, `AIMOVE.c`, and `Make_Monsters` in `AIDATA.c` 167 to 182, which also needs checking against our monsters and raiders) and decide with the priorities in `design.md`: playability first, so neutral cities must stay places the player can take with an army and not threats that snowball. (4) **Hosts.** The client, the console and `play_round` already call `play_neutrals`; units that move need the same battle handling as the wizards' (an attack on the human is fought by the human, task 135). (5) Tests: an objective changes what a city builds; a garrison sallies against a weak stack and goes home; no city is ever left empty. A soak only when asked. Asked for by the user, 18 October 2026.

**306. Overland spell animations (summons, city and global spells, map squares).** **Progress 30 October 2026:** (1) the summon scene is done (`summon_scene.rs`, `--summon SPELL`: frame, caster, the creature rising, the rarity's music; the race recolouring of the frame is not done); (2) the city spell scroll is done (`city_scroll.rs`, `--city-scroll SPELL`: title, cityscape, "You cast", realm music, Earthquake's shake; the building appearing for Move Fortress and the gates is not done); (4) the square animation is done (`square_anim.rs`, `--square SPELL`: the map scrolls, the spell's own or the realm's picture plays on the square; Earth Lore and Enchant Road play their `specfx` pictures through it; Plane Shift's transition is left), (5) the target-wizard pictures are done (`wizard_anim.rs`, `--wizard-spell SPELL --global-by N`, from `Event::SpellCastOnWizard`: the portrait with the picture over it). The core now follows a cast with `Event::SpellCastAt` (where it landed). Gating by Spell Animations, Enemy Spells, Detect Magic, contact (summons) and the explored square is in (`spell_sight.rs`). Only the battle spell animations exist (`spell_fx.rs`, `battle_screen/spell_anim.rs`); the spells cast on the main map show a plain notice box or nothing. The original plays several kinds, listed spell by spell in [`overland_spell_animations.md`](overland_spell_animations.md) (152 spells, with the picture, the function and the conditions): (1) the **summon scene** for creature summons, Summon Hero and Champion, Enchant Item and Create Artifact (`Summon_Animation`, SPELLSCR backgrounds by race, `cmbtfx` 22); (2) the **city spell scroll** (SPELLSCR 73 with the cityscape, the building appearing, Earthquake's shake) for city enchantments, curses, Move Fortress, Wall of Stone, Spell Ward and Spell of Return; (3) the **global spell mirror** (**done, 18 October 2026**: `global_spell.rs`, for every wizard's cast; the swirl clearing the glass, the face dissolving into the spell's picture and the off-path still, with `--global SPELL [--global-by N] [--global-at SECONDS]` for screenshots; `Clipped_Copy_Mask` is read as clearing the picture where the mask is transparent, which ReMoM's own port does not do and notes as "IDGI"; the stage pace is the original's 110 ms; the music is task 76); (4) **square animations** (`Spell_Animation_Screen`: the map scrolls to the square and a `specfx` or `cmbtfx` picture plays) for unit enchantments, Change Terrain, Transmute, Raise Volcano, Corruption, Black Wind, Call the Void, Warp Node, Stasis, Nature's Cures, Chaos Channels, Lycanthropy, Word of Recall, Floating Island, Resurrection, Disenchant Area and True, Ice Storm and Fire Storm; (5) **Earth Lore and Enchant Road** (`specfx` 45 and 46, 22 steps), **Plane Shift**'s transition, and the **target-wizard** pictures (Spell Blast, Cruel Unminding, Drain Power, Subversion); (6) the **learn-spell animation** (**done, 18 October 2026**: `learn_spell.rs`, the wizard's lab then the library page decoded under the `specfx` 49 sparkle, and the "You have mastered the spell of X" box with the setting off; `--learn SPELL [--learn-at SECONDS]` for screenshots; the 110 ms lab loop is the original's, the 80 ms decode stage our own pace, the lab's fade to black and the fade into the book are not drawn). Wire each to the human's own casts first, then rivals' under the original's conditions (Enemy Spells setting, Detect Magic, contact, explored square; the table's rules section), and make all of them honour `Pref::SpellAnimations` with the original's off paths (task 280 only covers battle; `divergences.md` 30). The music of the summon, city and global scenes belongs to task 76. Doom Bolt, Star Fires and Warp Lightning have no overland cast in our content; decide whether they should (the original lets them hit an enemy stack). Task 287 (Spell of Mastery) stays separate. The pictures are filed per spell (`assets-local/art/spells/`, table `content/spell_art.ron`, task 340): look a spell's picture up by its id and role, and add a row to the table for any picture it lacks. Asked for by the user, 18 October 2026.

**351. Research the untraced `MESSAGE.LBX` lines.** Task 348, which is finished, could not say what rule stands behind these lines of the original: 8 ("All units retreat exhausted"), 10 (monsters rampage through the city), 18, 19, 20, 45, 46 (notices that other city, unit, Stasis or combat enchantments were dispelled) and 61 (Guise on a hero; no spell of that name in `spells.ron`). For each: find the original code that shows the message (ReMoM) and what triggers it, check whether our game has the rule, and write the answer in this task's "Done" paragraph as in the table of task 348: refused, no refusal, different on purpose, or no such rule. A missing rule goes to task 350 or a task of its own; the text is a key in `content/text/`.

**357. Site discovery: the music and the node colour slide.** **Checked 30 October 2026:** the music is done (`SiteNotice::music` plays `SITE_DISCOVERY`, entry 105); the node picture's colour slide is left. Was: The original plays a short discovery tune when a stack
finds a site and slides the colours of a node's picture (`docs/lairs.md`, "Missing"). Add both to the site prompt
(`f4x-client/src/site_prompt.rs`) where the client has the music and image support for it; text stays in
`content/text/en/site.ron`.

**358. Names in the rules files per language.** The names in `units.ron`, `spells.ron`, `buildings.ron`, `races.ron`, `realms.ron`, `abilities.ron` and `item_powers.ron` are shown as they are (`Item::display_name` and the screens read `def.name`). Decide and build how a language gets its own: a table of names by id in `content/text/<lang>/` that the display sites read first (falling back to the file's name), or a copy of each file per language. Then every place that prints a `def.name` goes through it, and the plural of a race (the original keeps it apart) needs its own form.

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

**360. Remaining Conquest, banishment and defeat scenes (client).** **Left:** the scene a rival’s Spell of Mastery brings (`Spell_Of_Mastery_Lose`, ReMoM `SPLMASTR.c`: `spellose.lbx`, the wizard's picture sheared and shrunk into the sphere of their colour, the familiar in the corner) before the defeat; the zap of a wizard banished by another wizard, which the original does not show (its early return) and which we could add as our own; and a hand check
of the scenes in a real game (the shots were taken from `--scene`).

**364. Scenes with their own music.** **Progress 30 October 2026:** (1) the summon scene and (2) the city-spell scroll are built with their music (`summon_scene.rs`, `city_scroll.rs`; the summon music follows the spell's rarity); only (3) the Force of Nature scene is left. Was: Three scenes of the original that the client does not have, each with the music
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

**365. Crossfade the music.** Ours; the original stops all sound and starts the next piece. Fade the old track out and
the new one in over about a second when the music changes (a screen, a battle, a short piece and back to the
background), so the changes are less abrupt; short pieces fade the background out and in again on their own end.
`Audio::music` in `crates/f4x-client/src/audio.rs` is the one place that changes tracks; macroquad's audio has
`set_sound_volume`, so the fade is volume steps per frame on the two sounds, and the old sound is kept loaded until it is
silent. A switch under Settings ("Fade Music") turns it off for those who want the original's cuts.

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
in the treasury while units are short). Measure with the soak (`ai-design.md` 12.2); tune the diplomacy numbers of `diplomacy.md` section 8 there too (treaties formed, wars started, who wins). *Needs:* 361.

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

**375. Wire the Settings switches that do nothing yet.** **Done 30 October 2026:** all six (tasks 269 to 271, 276 to 278). Left: the help topic and area of Left Click Moves Units and Play Intro. Was: City Spell Events, Overland Spell Events,
Summoning Events, Auto Unit Information and Enemy Moves are stored
(`preferences.rs`) and shown on the Settings screen but read by nothing. (Spell Book Ordering is gone: the
spell book has its own sort and filter lines.) Their help (topics 678 to 680, 686, 687) already says
what each should do, as the original does; make the client do it. Also give Left Click Moves Units and
Play Intro a help topic and a help area on the Settings screen (they have none, the area list comes
from the original). *Needs:* nothing.

**377. Unit statistics window: what is left.** The sheet itself is done (task 140: the unit's figures, moves, upkeep, the five icon rows, abilities and spells, Dismiss, turning off a lasting spell, right-click help on a line, the Unit List window). Still open:
1. The turn-off question and the Dismiss question use the original's `Confirmation_Box` art. (`--sheet N --sheet-spell ID [--sheet-ask]` puts a spell on the unit and opens the question for a screenshot.)
2. Heroes: the portrait, the items row and item powers, the hero abilities (tasks 218 to 221 and 47), and Dismiss for heroes (task 48).
3. The sheet from the Army list and the item screen (tasks 138 and 142 do not exist yet), the production screen's version (cost, and upkeep for a unit not yet built), and the original's expanding-box opening.
4. The battle's sheet is a snapshot taken on opening on a plain background; the original draws it over the field.
5. Scouting, the Summon Demons use count, mana left (`Caster 12 (20) mp`) and Warp Creature's three icons, once those exist in the core.
No action: the hit-points row shows one figure's hit points, and the original does the same.

**378. Turn summary and message pop-ups: what is left.** The Chancellor's scroll, the building-complete box, the combat results scroll, the fizzle and "has been dispelled" boxes (also in battle), the turn-limit heading and the rampage, ruins and raiders lines are done (task 144). Still open:
1. The evict message.
2. The raze prompt shows over the results scroll; the original asks before it.
3. The stasis "has been dispelled" notice (message 20).
4. The Tranquility and Life Force fizzle boxes (the core has no such counter).
5. The "can no longer produce" warning (the core has no event for an item that cannot be built any more). The unit-cap warning is dropped on purpose: we have no unit limit (`divergences.md`).
6. ~~The music `MUSIC_Bldng_Finished` and `MUSIC_Spell_Failure`.~~ Done (checked 30 October 2026: `BuiltNotice` and the fizzle notice play them).

**379. Settings: races banned for computer wizards and independent cities, by difficulty.** The original gives a computer wizard any of the nine Arcanus races up to Normal and only Klackon, High Elf, Halfling, High Men and Nomad above it (`Generate_Home_Cities`); ours picks any race. Add a setting instead of the fixed rule: in each difficulty level of `content/settings.ron`, a list of race ids that computer wizards may not have (`ai.wizards.banned_races`) and one for the independent cities (`world.neutral_banned_races`). Both lists are empty in the shipped file: they are for players and mods that want the original's rule or their own (the original's list can go in a comment as an example). Where to wire it: the race pick for computer wizards in `game/setup.rs` (the `races` loop) and `Game::neutral_races` in `city/neutral.rs`; if a ban leaves no race for a plane, fall back to the unbanned list and say so in `validate`. Check the ids against the races when the file is read (`game/setting.rs`, `f4x-tools validate`), add the two lists to `docs/game-settings.md` (they replace the `wizards.ai_race_limit` row), and test a game with a ban (no computer wizard and no neutral city of a banned race).

**380. Check the original generator's port against real maps of the original.** `map/mapgen/original.rs` (and `game/setup/original.rs`) follow the ReMoM notes in `~/projects/ReMoM/doc`, scaled to the map size; no output of the original game has been compared. When real maps are at hand (screenshots, a dump of the world map from ReMoM's `gd_dump_world_map`, or the author's own game), compare per map the tile counts by terrain, the landmass count and the largest landmass's share, the number of rivers, nodes by type, and how many neutral cities a game gets, with `f4x-tools mapgen-stats` and `start-stats`. Fix the port where it is off (for example the size of the polar caps, the hit-count thresholds, the climate walks), and note in `divergences.md` what stays different on purpose (the node counts, the extended islands' directions, the rivers).

**382. Code optimization and speedup.** A slow game is fine while the game is being built, but it has got slower (a 300-turn AI game of two wizards took 40 to 90 seconds, and 5 to 18 minutes once the armies grew with the half upkeep; the soak of 8 seeds takes hours), and the client's turn change waits on the same code. Measure first, then change what the measurements point at, one thing at a time, each with the soak line (`f4x-tools soak`) and the tests unchanged (the same game must come out: the soak output of a seed is the check that a speedup changed nothing).
1. **Measure.** Add per-phase timings to the soak (`f4x-tools soak --profile`: the turn change, each wizard's AI step, battles, upkeep, the map's queries) and run a profiler on one slow game (`samply` or `cargo flamegraph` on `f4x-tools soak --seeds 6..6 --turns 300`, release build with `debug = 1`). Write the table into this task. Check the client too: the frame time on a large map with many units, and the time from End Turn to the next turn.
2. **Build settings, cheap and without a code change.** `Cargo.toml` has no `[profile]` section: try `[profile.release] lto = "thin"`, `codegen-units = 1`, and for tests and the debug client `[profile.dev.package."*"] opt-level = 2` (and `opt-level = 1` for the workspace crates), and measure what each gives. Keep any that change the result of a seed (a float difference) out.
3. **Leads to check, not yet measured.** `Game::all_units` and `units_at` scan and sort the world on every call (`unit/mod.rs`), and callers call them per unit (a spatial index by square, and a per-player list kept up to date, would turn n squared into n); `economy().cities()`, `site_squares()` and `landmasses()` are rebuilt per call (cache them for the turn or make them an index); the AI's path finding per unit per turn (`find_path`, reuse one search for a stack, a cache of the movement cost map per movement type and turn); `Content` is cloned into every `Game` (share it with an `Arc`); every command saves an undo snapshot when undo is on (`undo.rs`: store only what changed, or leave undo off for the AI's own commands and in the soak); battles for the AI (`auto_resolve`) and the battle AI's `legal_commands` lists; `hecs` queries that build a `Vec` only to look at one item.
4. **Memory and allocation.** Count the entities and allocations per turn in a long game (`--every` prints the entities already); look for `clone()` of big values in the per-turn paths (`Game::snapshot` for the AI's what-ifs, `Content`), and `Vec<String>` built for names in hot loops.
5. **Done so far (measured with macOS `sample` on the running soak; the same games came out in every case).** The AI's `hostile_at` made a new `PlayerView` after every command, and each one worked out the sight of every unit on both planes: now one plane at a time and only the units on it. Items are looked up for heroes only (`ItemView::carried`), `Spells::has` stops at the first hit, `city_at` no longer builds and sorts the city list, and `find_path` and `reachable_squares` take the hostile squares in one pass (`hostile_squares`). Seed 6 of two wizards: 1093 s to 64 s; seed 8: 1329 s to 91 s; seed 1: 94 s to 8 s. **Second round (4 wizards, seeds 6 and 7, 150 turns: 26.5 s to 14.4 s, same games):** the army step made a new view (and so the whole sight computation) for every neighbouring square of every stack: now one view per stack; `legal_battle_casts` asked the price, the known-spell check and the wards for every aim of a spell (about 100 squares): now once per spell (`check_spell`); `unit_level` stops before the Heroism query when the unit has no level path and `unit_modifiers` works out the level once; "does any wizard keep spell X" is one query (`any_global`, `evil_omens_active`) instead of one per player. **Third round:** a path search works out the squares with a free place on one of the player's ships once (`ship_room_squares`, `Economy::with_ship_room`) instead of asking at every sea square (two wizards, seeds 6 and 7, 300 turns: 66 s to 54 s, same games). Still to do: sight (about 6% of a two-wizard game and 14% of a four-wizard one: `unit_sight` builds the whole modifier list of every unit for one stat; a cache between commands needs a trustworthy "nothing changed" signal, the core has none), the spell-instance queries `Spells::on`/`globals_of`, then the memory count in 4.
5. **Keep it honest.** A change that makes a seed play differently is not a speedup: the soak's winner and turn for the seeds 1 to 8 are the regression check; write the new baseline into `ai-design.md` section 12.2 once, after the neutral-city and upkeep changes of 24 October 2026.

**383. Battle spell animations: the pictures the casting leftovers still lack.** What is left of task 159 (6), checked against `content/spell_art.ron`, `spell_fx::fx_for` and `content/sound.ron` on 24 October 2026. **Sounds: none missing**, every spell of 159's list has a sound entry (Invisibility's id is spelled `invisiblity` in `spells.ron` and `sound.ron`, both consistent). **Animations:** a spell without a picture of its own plays the picture of its realm (`specfx` 0 to 5) on the target square, and a battlefield spell tints the screen; that is what the original does for spells it has no picture for, but which of these the original gives its own picture is not checked. (1) Check in ReMoM (`Spell_Animation_Load_Graphics`, `Combat_Spell_Animation`) whether these spells of 159's list have a picture of their own, and add the ones that do to `spell_art.ron` (role Cast) and `extract-art`: Animate Dead, Holy Word, Mass Healing, Raise Dead, Shatter, Warp Creature, Weakness, Invisibility, Word of Recall, Creature Binding, Chaos Channels, True Sight, Lionheart. Those with none keep the realm picture; say so in `docs/overland_spell_animations.md` or `combat.md`. (2) The cell effect on a targeted square (`cmbtcity` 120 and the spell's own animation) is not drawn at all (no use of entry 120 in `crates/f4x-client`). (3) The wall-rise animation (`wallrise`) when a wall spell (Wall of Stone) is cast in battle: **checked 30 October 2026, `battle_ground.rs` draws it; only a `--shot` check of a cast is left.** Tests: the client has none for art, so check by `--shot` of a battle where each is cast.

**384. Pop-up leftovers: summon boxes and the hire sheet.** What is left of task 264: (1) the hire box variants **Hero Summoned** and **Champion Summoned** of `GAME_HeroHireType` (title, the Accept button, the summon square for types 1 and 3): the Summon Hero and Summon Champion spells place the hero at once today, so they wait for a core offer that the box can answer; (2) the hero's level in the hire sheet is the level path's bonuses added to the base numbers: check them against `Hire_Hero_Popup` and `UV_Setup_HireScr`; the hire sheet also shows the template's gold upkeep, check what the original shows for a hero. Split from task 264, 25 October 2026.

**385. Help pop-ups: what is left.** What is left of task 289: (1) our own text for the topics not yet written (`Help::unwritten()` lists them; the conflicts to settle while writing are in `docs/help-conflicts-checklist.md`); (2) the areas the client draws differently from the original: check each wired screen; (3) F1; (4) the screens not yet wired. Split from task 289, 25 October 2026.

**386. Magic screen: scroll the enemy wizard portraits.** The Magic screen shows four rivals' gems and portraits only (`take(4)`), so with more than five wizards (ten colours are playable, task 381) the others cannot be reached. Let the player scroll through the enemy wizard portraits (arrows or the mouse wheel, in the original's style where it has one, else our own), so that every rival can be selected for diplomacy and its details. Split from task 381, 25 October 2026.

**388. Spell Images** Each spell has an image to be shown in Detect Magic etc., but they're not fully found and imported.
