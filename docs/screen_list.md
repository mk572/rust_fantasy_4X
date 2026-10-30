# Screen list

Every screen, window and pop-up of the original game, what it is for, when the game goes there,
and where this project stands. The original is described from the reverse-engineered source in
`~/projects/ReMoM/MoM/src` (function names in backquotes are from it; `MGC` is the start-up
program `MAGIC.EXE`, `WZD` the game `WIZARDS.EXE`) and the original help text. Where the source
has no comment I describe what the code does from its strings and calls and say so.
Task numbers refer to [`TASKS.md`](TASKS.md) and [`TASKS-DONE.md`](TASKS-DONE.md). Where each screen's pictures come from and which palette they need: [`extract-art.md`](extract-art.md).

**Status:** *Done* = works as in the original (small gaps are named). *Partial* = something
usable exists. *Missing* = nothing yet. *Own* = no counterpart in the original.

## 1. How the original moves between screens

The game is a loop over one global, `current_screen` (`Screen_Control` in `MOM_SCR.c`, the values
are `e_SCREENS` in `MOM_SCR.h`). Each pass runs the function of the current screen; that function
runs its own input loop and, when it ends, sets `current_screen` to the next one. A **screen**
replaces the whole display and has its own entry in that loop. A **window, pop-up or prompt** is
called as a function from inside a screen (or from the turn processing), draws over it, waits for
the answer and returns; it never changes `current_screen`.

```
                  ┌────────────── Main menu ◀──── Hall of fame ◀──┐
                  │  Continue / Load / New game / Quit / Fame     │
   start ───▶     ▼                                               │
  (intro,      New game ─▶ Continue ─▶ MAIN SCREEN ◀───────────────┤ (win / lose:
  credits)      steps 0–7    (load)        │  ▲                    │  score, then fame)
                                           │  │ every screen below returns here
   ┌──────┬──────────┬────────────┬─────────┼──┴──┬───────────┬────────────┐
   ▼      ▼          ▼            ▼         ▼     ▼           ▼            ▼
 City   Armies ─▶ Items   Cities ─▶ City   Magic ─▶ Diplomacy  Spellbook   Advisors
 screen  list             list     (Production)      (talk)    (spells)    (F1–F9)
   │                                                                        │
   └─▶ Production pop-up          Road build   Game (Load/Save ─▶ Settings)  Next turn
                                                                            (turn processing,
                                    Combat (called from movement, not a screen)  then back to main)
```

Two things worth knowing before the tables:

- **Next turn is a "screen" only in name.** `scr_NextTurn` runs `Next_Turn_Proc`, which does the
  turn's calculations, then shows the Chancellor's summary, the building-complete messages and
  the offers, and returns to the main screen (or to the main menu if the game ended).
- **Combat is not in the list.** `Combat_Screen` is called from the movement code and the lair
  code (`Combat`) and returns the result to them; the overland screen stays loaded underneath.

## 2. Screens in the main loop (`e_SCREENS`)

| Screen (original function) | What it does | When the game goes there | Status here |
|---|---|---|---|
| **Main menu** (`Main_Menu_Screen`, `MainMenu.c`) | The title page: Continue, Load Game, New Game, Quit to DOS, Hall of Fame. Plays the credits when left alone. | At start (after the opening movie); from the Load screen's Quit (it saves to slot 9 first); after the Hall of Fame; after a cancelled new game; when a game ends (win or lose). | **Done** (`menu_screen.rs`, task 86), on the original's art; the Hall of Fame entry opens the table (88); there is no credits roll. The opening movie plays before it when the art is extracted and the Play Intro setting is on (`intro_screen.rs`, task 85; without its music). |
| **Continue** (`scr_Continue`, no picture) | Loads the last auto-save (slot 9) and goes to the main screen. | "Continue" on the main menu; when the new-game steps finish (the new game is created, then continued). | **Done:** the client auto-saves at every end of turn and Continue loads it. F9 loads the quick-save. Console: `load`. |
| **Load / Save** (`Load_Screen`, `LoadScr.c`) | One screen for both: the list of save slots, buttons Load, Save, Settings, Quit (saves, then to the main menu) and OK. | The **Game** button on the main screen; "Load Game" on the main menu. | **Done** (`menu_screen.rs`, task 145): eight slots on the original's art. A slot's name is not typed; it shows wizard, turn and save time. Settings opens the preferences page (89). F10 and Ctrl-L quick-save and load. |
| **New game** (`Newgame_Control`, `NewGame.c`) | Walks through the eight creation steps of section 3, then builds the world. | "New Game" on the main menu. | **Done** as the steps of section 3 (`newgame_flow.rs`, `newgame_picks.rs`), Esc going a step back. |
| **Quit to DOS** (`scr_Quit_To_DOS`) | Ends the program (the source marks an auto-save here as to do). | Main menu Quit; the quit-game flag on the main screen. | **Done** (Esc quits; there is no confirmation or auto-save). |
| **Hall of fame** (`Hall_Of_Fame_Screen`, `SCORE.c`) | The table of best scores, with the winner's name, race and score, kept between games in `MAGIC.SET`. | "Hall of Fame" on the main menu; after the end-of-game score (`End_Of_Game_Score`). | **Done** (`hall_of_fame.rs`, task 88): ten entries in `saves/hall_of_fame.txt`, from the main menu and after the score screen. |
| **Settings** (`Settings_Screen`, `Settings.c`) | The game options as check boxes: music, sound effects and event music; animations of movement and spells; show enemy moves and spells; end-of-turn summary and wait; random events; strategic combat only; ask before razing a city; show node owners; show unit information automatically; spell book ordering; and what the hall of fame records (names, races, scores). | The Settings button on the Load / Save screen; it returns there. | **Done as a page** (`settings_screen.rs`, `preferences.rs`, task 89): the original's check boxes in its four groups, less Movement Animations and Show Node Owners (removed 24 October 2026: movement is always animated and the node owners always shown), plus our Play Intro and Left Click Moves (Sound and Music, Messages, Combat, Display) on its art, saved to `saves/preferences.txt`; O, Enter and Escape leave. Strategic Combat Only and Play Intro do something; the rest are stored for tasks 266 to 283. A **Game...** button opens the game's own settings (task 94). The hall-of-fame data the original keeps in the same file is task 88. |
| **Main screen** (`Main_Screen`, `MainScr.c`) | The overland map, the minimap, the active stack's units and its orders (Done, Patrol, Wait, Build, Plane), the resource panel (gold, mana, food) and the buttons Game, Spells, Army, Cities, Magic, Info and Next Turn. Everything else is reached from here. | After loading or starting a game; when any other screen is left; after each turn is processed. | **Done** (tasks 126, 298 and 299 are finished). Several buttons are keys here (see `F1` in the client). |
| **City screen** (`City_Screen__WIP`, `CityScr.c`) | One city: its population and farmers, resources, buildings, the worked area, the garrison and the production; change production, buy, sell. | Clicking an own city on the map; from the City list; from the turn summary when a city can no longer make what it was building. | **Done** (tasks 96, 117, 119 for the enchantments list, selling from the picture and small details). |
| **Army list** (`ArmyList_Screen`, `ArmyList.c`) | Every unit stack of the wizard, with its orders; select a unit to open its details or go to the Items screen. | The **Army** button on the main screen; back from the Items screen. | **Done** (task 138, `army_screen.rs`), with the heroes (219) and the Items button (142). |
| **City list** (`CityList_Screen`, `CityList.c`) | All cities of the wizard in a table (size, gold, production, what is being built), with a small map. Opens a city or the Production pop-up. | The **Cities** button on the main screen; back from the Production pop-up. | **Done** (`city_list_screen.rs`, task 139); an outpost opens the outpost screen (141). |
| **Magic screen** (`Magic_Screen`, `MagicScr.c`) | The wizard overview: the power split (mana, research, skill), research, the alchemy pop-up, the spells and retorts, the other wizards' portraits and treaty icons. Click a portrait to talk. | The **Magic** button on the main screen; back from the Diplomacy screen. | **Done** in the original's layout (`magic_screen.rs`: staves with locks, text block, overland enchantments, alchemy window); portraits (83), diplomacy (63, 91) and the wizard sheet (75) are done. |
| **Diplomacy** (`Diplomacy_Screen__WIP`, `DIPLOMAC.c`) | The talk with a rival wizard: greeting, treaty proposals, threats, tribute, spell exchange. | Clicking another wizard's portrait on the Magic screen (refused while banished); also when an AI wizard opens a talk with the human at the start of a turn (`Npc_Diplomacy_Screen`). | **Done** (`diplomacy_screen.rs`, `diplomacy_text.rs`, task 91; the rules are in `docs/diplomacy.md`, tasks 63 and 64). A rival's proposal during the turn change comes as a question over the map (`diplomacy_prompt.rs`). |
| **Spellbook** (`Spellbook_Screen`, `SBookScr.c`) | The spell book the wizard casts from: pages by realm, the cost and the casting time, choose a spell to cast overland. In combat the Spell button opens a second book, `Combat_Spellbook_Screen`. | The **Spells** button on the main screen. | **Done** (`spellbook.rs`, tasks 93 and 166): the Spells button or V, with the page animations and the combat book. |
| **Advisor** (`Advisor_Screen`, `AdvsrScr.c`) | A menu of nine information screens (section 4). | The **Info** button on the main screen; F1 to F9 keys. | **Done** (task 71): `advisor.rs` (the list is a modal), `advisor_screen.rs`, `surveyor.rs`; the Mirror is `wizard_screen.rs` (75). |
| **Item screen** (`Item_Screen`, `ItemScrn.c`) | Heroes' equipment: move magic items between hero slots and the vault of the wizard; destroy an item; look at it. | The Items button on the Army list; after a hero or lair reward is taken, to place the items found. | **Done** (`item_screen.rs`, task 142). |
| **Road building** (`Road_Build_Screen`, `Roads.c`) | Pick the end of a road for the stack's engineers: shows the map, how many turns the road will take, and refuses a road across a large body of water. | The **Build** (special) button on the main screen when the active stack has engineers. | **Done** (task 102): the side panel of `main` art with the turns and OK/Cancel, the path drawn on the map. |
| **Production** (`Production_Screen`, `ProdScr.c`) | A pop-up list of what a city can build (buildings and units, with costs and the building each needs); choosing sets the production. | The Change button of the City screen; the City list's production column. | **Done** as the production menu of the city screen (`city_screen.rs`), not as a separate pop-up. |
| **Next turn** (`scr_NextTurn`, `Next_Turn_Proc`) | Processes the turn for all wizards, then plays the summary, messages and offers (section 5). | The **Next Turn** button or Enter on the main screen, when no units are left to move or the player confirms. | **Done** (the core's `turn.rs`; the client plays the Chancellor's scroll, the events, offers and building notices, tasks 143 and 144). |
| Test and proof-of-concept screens (`scr_Test_Screen`, `scr_PoC_Screen`) | Developer stubs. | Never in play. | Not applicable. |

## 3. The new-game steps (`Newgame_Control`)

Each step is a full screen with its own function in `NewGame.c`; the control function moves back
and forth between them (Cancel goes back one step).

| Step | Original | What it does | Status here |
|---|---|---|---|
| 0 Options | `Newgame_Screen_0` | Difficulty, opponents, land size, magic power (the game options). | **Done** (`options_screen.rs`, on `newgame.lbx`: four buttons cycling the choice, OK, Cancel). The map seed is still a launch option. |
| 1 Wizard | `Newgame_Screen_1__WIP` | Choose one of the 14 ready-made wizards, or "Custom". | **Done** (`newgame_flow.rs`: the two columns, hover shows portrait, books and retort; Custom). |
| 2 Portrait | `Newgame_Screen_2__WIP` | Custom only: pick the wizard's picture. | **Done** (`newgame_flow.rs`: the same lists as Select Picture). |
| 3 Name | `Newgame_Screen_3__WIP` | Type the wizard's name. | **Done** (the name box, ten characters, Enter keeps it). |
| 4 Picks | `Newgame_Screen_4__WIP` | Custom only: spend eleven picks on spell books, retorts and extras. | **Done** (`newgame_picks.rs`: drag in a realm's row for its books, click a retort; the refusals are the original's). |
| 5 Spells | `Newgame_Screen_5` | Custom only: choose the starting spells for each realm. | **Done** (`newgame_picks.rs`, a screen for each realm with two or more books). |
| 6 Race | `Newgame_Screen_6__WIP` | Choose the wizard's race (home city race). | **Done** (`newgame_flow.rs`; the Myrran races need the Myrran retort). |
| 7 Banner | `Newgame_Screen_7__WIP` | Choose the banner colour. | **Done** (`newgame_flow.rs`: five flags; a click starts the game). |

## 4. The advisors (`Advisor_Screen`, `AdvsrScr.c`)

A menu of nine entries, each a screen or window. All are **Done** (task 71): the list box `advisor.rs`, History, Astrologer and Cartographer in `advisor_screen.rs`, the Surveyor in `surveyor.rs`, the Chancellor in `chancellor.rs`, the Grand Vizier question in `advisor.rs`.

| Key | Name | What it shows |
|---|---|---|
| F1 | Surveyor (`Surveyor_Screen`, `Surveyor.c`) | The map with a cursor; for the square under it: the terrain's food, production and gold bonuses, and the maximum population and bonuses a city founded there would get; says why a city cannot be built there (water, lair, tower, magic node, closer than three squares to another city). |
| F2 | Cartographer (`Cartographer_Screen__WIP`) | The whole world map of one plane, zoomed out; N switches plane, clicking a spot moves the main map there. |
| F3 | Apprentice (`Apprentice_Screen`, `Spellbook.c`) | The "big book": the library of every spell the wizard knows and, on the first page, the spells being researched. **Done** (`bigbook.rs`, F3): the library then the research pages; the research choice and the learned-spell notice use the same book. |
| F4 | History (`Powergraph_Screen`) | A graph of each wizard's power over the turns ("History Of Wizards Power"). |
| F5 | Astrologer (`Status_Screen`) | "Current Status Of Wizards": a comparison of the wizards (including research). |
| F6 | Chancellor (`Chancellor_Screen__WIP`, `REPORT.c`) | The report scroll: what happened this turn (see section 5). |
| F7 | Tax Collector (`TaxCollector_Window`) | Tax rate table: gold per population and the unrest each rate causes. The tax rate is chosen here. |
| F8 | Grand Vizier (`Grand_Vizier_Window`) | Asks whether the Grand Vizier may choose what the cities build (auto-build). **Done:** F8 asks the question over the map (a modal) when switching it on and switches it off at once; the console has `vizier on|off`. |
| F9 | Mirror (`Mirror_Screen`, `WIZVIEW.c`) | A wizard's sheet: name, spell books, fame and, for another wizard, the treaty with you, personality and objective. Also opened from the Magic screen and used when a spell targets a wizard (`Target_Wizard_Screen`). **Done** (task 75, `wizard_screen.rs`): F9 and the right-click on a rival's gem on the Magic screen; a spell that targets a wizard is aimed by clicking one of their cities or units on the map (there is no portrait picker). |

## 5. Windows opened from inside a screen

Not part of the `e_SCREENS` loop. The "entered" column says what leads to each.

### 5.1 Overland map and turn

| Window | What it does | Entered | Status here |
|---|---|---|---|
| Unit statistics (`Main_Unit_Statistics_Window`, `Overland_Unit_Statistics_Window`, `Unit_Statistics_Popup`) | The sheet of one unit: pictures, attributes with the bonuses, abilities, enchantments, level, items, and the Dismiss and OK buttons. | Right-clicking a unit in the stack panel, the Army list, the City screen's garrison or the Item screen; in combat the same sheet (`Combat_Unit_Statistics_Window`). | **Done for map, city garrison and battle units** (right-click a unit in the selected-stack text, in the city's garrison or on the battlefield; `unit_screen.rs`); the enemy stack list and the heroes came with task 140. |
| Enemy stack list (`Unit_List_Window`) | The units of another player's stack on the map, read only. | Right-clicking a square holding another wizard's units. | **Done** (`unit_list_screen.rs`, `--unit-list`; a right-click on an explored square holding units that are not yours; a click or Esc closes it). |
| Outpost screen (`Outpost_Screen`, `Outpost.c`) | A small city sheet for an outpost: its surroundings and what is being built; names a new outpost (header "New Outpost Founded"). | Clicking an own outpost; right after settlers found one (to name it). | **Done** (`outpost_screen.rs`, task 141); the new-outpost music is part of task 76. |
| Enemy city screen (`Enemy_City_Screen`, `City_ovr55.c`) | What is known about another wizard's city: size, buildings seen, garrison seen. | Clicking another wizard's city. | **Done** (`enemy_city_screen.rs`, task 141). |
| Rename home city (`Change_Home_City_Name_Popup`) | Text box to rename the capital. | Once, on the first turn (`_turn == 0`). | **Done** (the "Name Starting City" box over the map on turn 1, `--capital-name`). |
| Rename city (`Change_City_Name_Popup`) | Text box to name a new outpost. | After founding an outpost. | **Done** (the naming box of the outpost screen, `Command::RenameCity`). |
| Chancellor's summary (`Chancellor_Screen__WIP(0)`) | A scroll of the turn's news in sections: random events, units disbanded (deserted for lack of food or gold, dispelled for lack of mana, deserted to make room, drowned), units killed, city growth, city deaths, outposts that grew or were deserted. Optional ("end of turn summary" setting). | After the turn is processed. | **Done** (`chancellor.rs`, tasks 272 and 144): a modal scroll with the sections of the news, also from the F6 advisor entry, which shows the last turn's news again and the shortfall forecast. The log still lists everything. |
| Building complete (`City_Built_Building_Message`) | A picture of the building with "X has completed the construction of Y". | After the turn is processed, one per finished building; then the City screen opens when a city can no longer make what it was building. | **Done** (`built_popup.rs`, task 144): the green box with the building's picture, one per finished building, then the build-in animation on the city screen. |
| Evict message (`Evict_Unit_With_Message`) | Tells that a unit was removed from a city that has no room. | When housing runs out. | **Partial:** the units that go are told in the log and the Chancellor's scroll; there is no box of its own. |
| Alchemy pop-up (`Alchemy_Popup`) | Converts gold to mana or back. | Button on the Magic screen and the Item screen. | **Done** in the Magic screen (`magic_screen.rs`); the Item screen's Alchemy button opens it. |
| Spell target selection (`Spell_Casting_Screen__WIP`, `SCastScr.c`) | Pick a square, city or unit on the map as a spell's target. | Casting a spell that needs a map target. | **Done:** the magic screen lets the player click the target on the map (`magic_screen.rs`). |
| Target wizard (`Target_Wizard_Screen`) / target enchantment (`Spell_Target_Global_Enchantment_Screen`) | Choose a wizard (portraits) or one of another wizard's global enchantments as a spell's target. | Wizard- or enchantment-targeting spells (Spell Binding, Subversion...). | **Partial:** the rules are done (tasks 60 and 62); the player aims at a wizard by clicking one of their cities or units, and picks a rival's global enchantment or the dead hero to raise from a list over the map (`resurrect_prompt.rs`, `view.rs`); there is no portrait screen. |
| Spell mana adder (`Spellbook_Mana_Adder_Screen`, `Combat_Spellbook_Mana_Adder_Screen`) | Choose how much extra mana to put into an infusable spell (its strength grows with the mana). | Choosing such a spell from the book, overland or in combat. | **Partial.** Combat casting has Up/Down keys for the extra mana in the battle screen's target prompt, no separate screen; overland has none. |
| Item-making (`Item_Make_Screen`, `ItemMake.c`) | Design a magic item or artifact: choose its type, powers and name, with the mana cost. | Casting Enchant Item or Create Artifact. | **Done** (`craft_screen.rs`, task 260; the rules are task 50, `Command::CraftItem`). |

### 5.2 Offers, rewards and events

| Window | What it does | Entered | Status here |
|---|---|---|---|
| Merchant (`Merchant_Popup`) | A merchant offers a magic item for gold. | At turn start, by chance rising with fame (`Determine_Offer`). | **Done** (`offer_prompt.rs`, task 143): a modal that waits over the map until answered; the offers live in the game (`Player::offers`). |
| Mercenaries (`Hire_Merc_Popup`) | Offers a group of mercenary units for gold. | `Determine_Offer`. | **Done** (`offer_prompt.rs`, task 143). |
| Hero offer (`Hire_Hero_Popup`) | A hero offers service for gold; the player names the hero. | `Determine_Offer`; hero spells (Summon Hero, Incarnation); lair rewards. | **Done** (`offer_prompt.rs`, tasks 143 and 48), with the name box. |
| Hero level-up (`Hero_LevelUp_Popup`) | Announces a hero's new level and its gains. | When a hero levels up in `Do_All_Units_XP_Check`. | **Done** (`level_up.rs`, task 143): a modal with the hero's gains and the fanfare. |
| Event message (`Show_Event_Message`, `EVENTS.c`) | The random events (earthquake, plague, conjunction and so on) with a picture and text. | When a random event starts. | **Done** (`event_popup.rs`, tasks 68 and 143): the `Notify2` box, with good or bad event music. |
| Lair treasure (`Lair_Treasure_Popup`, `Lair.c`) | Lists the gold, mana, items, spells and heroes won from a cleared site. | After winning a fight in a lair, tower or node. | **Done** (`site_prompt.rs`, task 128). |
| Site encounter prompt (`Lair.c`) | A box naming the site and its kind, "... Do you wish to enter?". | Moving onto a guarded site. | **Done** (`site_prompt.rs`, task 128). |

### 5.3 Combat

| Window | What it does | Entered | Status here |
|---|---|---|---|
| **Combat screen** (`Combat_Screen`, `Combat.c`) | The tactical battle: the 21 by 22 map, units, spell book, Wait, Done, Flee and Auto buttons; the result goes back to the caller. | A stack moves into an enemy unit, city or guarded site (`Move_Units`, `Lair_Combat`); the computer fights its own battles without showing the screen. | **Done** (`battle_screen.rs` and its folder, tasks 132, 133 and 166): the grid, figures, animation, the original's panel and buttons, the spell list, the attack question with Fight, Auto and Cancel (`attack_prompt.rs`). The result scroll opens over the battle's last picture (not yet seen in a played battle). |
| Combat spell book (`Combat_Spellbook_Screen`) | Choose a combat spell. | The Spell button in combat. | **Done** (task 166). |
| Combat spell target (`Combat_Spell_Target_Screen`) | Pick the target square or unit. | After choosing a combat spell. | **Done** (clicking a square; the extra-mana keys). |
| Combat information (`Combat_Information_Window`) | Battle-wide info: enchantments in force and so on. | The Info button in combat. | **Done** (`battle_info.rs`). |
| Vortex move (`Vortex_Move_Screen`) | The player moves a magic vortex on the battlefield. | Each combat round with a vortex cast by the player. | **Done:** click the vortex, then a square next to it (`battle_screen.rs`). |
| Retreat losses (`Build_Flee_Loss_Message`) | Tells which units were lost while fleeing. | After a flight from combat. | Implemented as a notice (`fled_notice.rs`) with the count of lost units; the original names them. |
| Raze or keep (`Raze_City_Prompt`) | After taking a city: "Do you wish to completely destroy this city?" (skipped when the "raze city" option is off). | End of a combat won at a city. | **Done** (`combat_result.rs`): the question comes on the result scroll, with the Raze and No buttons. |
| Spell cast messages (`Combat_Cast_Spell_Message`, `Combat_Spell_Counter_Message`) | Banner with the spell's name when cast or countered. | Casting in combat. | **Partial:** the spell notices of a dispel or a fizzle are boxes over the battle (`event_popup.rs`); the banner with the name of a spell cast has no box. |
| Confirmations (`Confirmation_Box`, `Warn`) | Yes/No boxes: sell a building; turn off a spell ("Do you wish to turn off the ... spell?"); "Do you wish to flee?"; "... Do you still wish to attack?" (a wizard you have a treaty with); "Do you wish to completely destroy this city?"; and, when ending a turn, "Some units do not have enough food and will die unless you allocate more farmers in a city. Do you wish to allow them to die?" | Many places; the food warning when Next Turn is pressed. | **Partial:** the client asks twice for Flee; no common box (the sale of a building uses the original's box, `confirmation_box` in `sell_prompt.rs`). The upkeep prompt before Next Turn (`upkeep_prompt.rs`) lists what the turn costs; the food warning in the original's words is missing. |

## 6. The end of the game

| What | Original | Status here |
|---|---|---|
| Win or lose animation | `Win_Animation`, `Lose_Animation` (`CONQUEST.c`): a short full-screen picture sequence. | **Done** (`end_scene.rs`, task 134): the win, lose, conquest, return and parade scenes. |
| Score screen | `End_Of_Game_Score` (`SCORE.c`): the score and its parts. | **Done** (`game_over_screen.rs`, on the original's art: icon rows, bar). |
| Hall of fame | After the score, then the main menu. | **Done** (task 88). |
| Triggered by | A wizard finishing the Spell of Mastery (checked each turn, `Get_Winner`); the last wizard standing; the human wizard banished. | **Done** in the core (`game/endgame.rs`). |

## 7. Screens this project has that the original lacks

| Screen | What it does | Entered |
|---|---|---|
| **Console** (`console_screen.rs`, `f4x-console`) | The text interface to the game: every action as a command, with legal and override modes. See [`console.md`](console.md). | The backquote key in the client (F2 is the Cartographer); the `f4x-tools` binary runs it without the client. |
| **Game over** (`game_over_screen.rs`) | The score, after the end scenes of section 6. | When the core reports the game ended. |
| **Wizard creation** (`create_screen.rs`) | Only the state of the steps; the steps are the original's, section 3. | At start. |

## 8. What is still missing

Every screen and window above is done except these (each is a task in `TASKS.md`: 396 to 402 except 398, done; the evict box is part of task 378):

- the credits roll of the main menu;
- a confirmation before Quit and an auto-save on it;
- the food warning when Next Turn is pressed, in the original's words;
- a box of its own for the units evicted from a full city;
- the extra-mana screen for an overland spell (combat has the Up and Down keys);
- the portrait picker for a spell that targets a wizard (the player clicks a city or unit now);
- the banner with the name of a spell cast in combat.

## 9. How the client shows a window

The original calls each window as a function from inside a screen. This client has one stack of
modals instead (`modal.rs`, task 389, done): a window is a struct that implements `Modal`, opened
on top of the stack for a question the player asked for (`open`) or queued behind the others for
a notice the turn produced (`enqueue`, first in first out, lower `priority` first). The screen
below is drawn first with an empty `Input` and dimmed (the Dim Behind Boxes setting), and only
the top modal takes input. A modal gives its own music (`Modal::music`), says whether the turn
change has to wait for it (`blocks_turn`) and may cover the whole screen (`covers_screen`: the
help pop-up). The kinds: the questions (upkeep, attack and its mana ask, resign, Spell Ward realm,
no-target, Resurrection, a global enchantment to pick, unit list, capital name, Grand Vizier, the
advisor and tax list, help); the notices (mastery, proposal, site, learned spell, level-up,
event, global-cast mirror, spell notice, offer, building finished, Chancellor); and the combat
result, which opens when the last blow of the battle has played and is drawn over the battle's
last picture. A battle under a modal only draws and does not go on. The end scenes are still a
queue of their own and become a screen with task 394.
