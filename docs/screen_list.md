# Screen list

Every screen, window and pop-up of the original game, what it is for, when the game goes there,
and where this project stands. The original is described from the reverse-engineered source in
`reference/ReMoM/MoM/src` (function names in backquotes are from it; `MGC` is the start-up
program `MAGIC.EXE`, `WZD` the game `WIZARDS.EXE`) and the original help text. Where the source
has no comment I describe what the code does from its strings and calls and say so.
Task numbers refer to [`TASKS.md`](TASKS.md).

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
| **Main menu** (`Main_Menu_Screen`, `MainMenu.c`) | The title page: Continue, Load Game, New Game, Quit to DOS, Hall of Fame. Plays the credits when left alone. | At start (after the opening movie); from the Load screen's Quit (it saves to slot 9 first); after the Hall of Fame; after a cancelled new game; when a game ends (win or lose). | **Done** (`menu_screen.rs`, task 86), on the original's art; Hall of Fame is greyed (88) and there is no credits roll or opening movie (85). |
| **Continue** (`scr_Continue`, no picture) | Loads the last auto-save (slot 9) and goes to the main screen. | "Continue" on the main menu; when the new-game steps finish (the new game is created, then continued). | **Done:** the client auto-saves at every end of turn and Continue loads it. F9 loads the quick-save. Console: `load`. |
| **Load / Save** (`Load_Screen`, `LoadScr.c`) | One screen for both: the list of save slots, buttons Load, Save, Settings, Quit (saves, then to the main menu) and OK. | The **Game** button on the main screen; "Load Game" on the main menu. | **Done** (`menu_screen.rs`, task 145): eight slots on the original's art. A slot's name is not typed; it shows wizard, turn and save time. Settings is greyed (94). F5 and F9 still quick-save and load. |
| **New game** (`Newgame_Control`, `NewGame.c`) | Walks through the eight creation steps of section 3, then builds the world. | "New Game" on the main menu. | **Partial.** One creation screen covers wizard, picks, spells and race (section 3), with a Back button to the menu. |
| **Quit to DOS** (`scr_Quit_To_DOS`) | Ends the program (the source marks an auto-save here as to do). | Main menu Quit; the quit-game flag on the main screen. | **Done** (Esc quits; there is no confirmation or auto-save). |
| **Hall of fame** (`Hall_Of_Fame_Screen`, `SCORE.c`) | The table of best scores, with the winner's name, race and score, kept between games in `MAGIC.SET`. | "Hall of Fame" on the main menu; after the end-of-game score (`End_Of_Game_Score`). | **Missing.** Task 88. The score itself is computed (`endgame.rs`, `docs/game_end.md`). |
| **Settings** (`Settings_Screen`, `Settings.c`) | The game options as check boxes: music, sound effects and event music; animations of movement and spells; show enemy moves and spells; end-of-turn summary and wait; random events; strategic combat only; ask before razing a city; show node owners; show unit information automatically; spell book ordering; and what the hall of fame records (names, races, scores). | The Settings button on the Load / Save screen; it returns there. | **Missing.** Tasks 89 (rules for options) and 94 (screen). |
| **Main screen** (`Main_Screen`, `MainScr.c`) | The overland map, the minimap, the active stack's units and its orders (Done, Patrol, Wait, Build, Plane), the resource panel (gold, mana, food) and the buttons Game, Spells, Army, Cities, Magic, Info and Next Turn. Everything else is reached from here. | After loading or starting a game; when any other screen is left; after each turn is processed. | **Done** apart from the details in tasks 120, 126. Several buttons are keys here (see `F1` in the client). |
| **City screen** (`City_Screen__WIP`, `CityScr.c`) | One city: its population and farmers, resources, buildings, the worked area, the garrison and the production; change production, buy, sell. | Clicking an own city on the map; from the City list; from the turn summary when a city can no longer make what it was building. | **Done** (tasks 96, 117, 119 for the enchantments list, selling from the picture and small details). |
| **Army list** (`ArmyList_Screen`, `ArmyList.c`) | Every unit stack of the wizard, with its orders; select a unit to open its details or go to the Items screen. | The **Army** button on the main screen; back from the Items screen. | **Missing.** New task 138. |
| **City list** (`CityList_Screen`, `CityList.c`) | All cities of the wizard in a table (size, gold, production, what is being built), with a small map. Opens a city or the Production pop-up. | The **Cities** button on the main screen; back from the Production pop-up. | **Missing.** New task 139. |
| **Magic screen** (`Magic_Screen`, `MagicScr.c`) | The wizard overview: the power split (mana, research, skill), research, the alchemy pop-up, the spells and retorts, the other wizards' portraits and treaty icons. Click a portrait to talk. | The **Magic** button on the main screen; back from the Diplomacy screen. | **Partial.** Power split, research and the spell list exist (`magic_screen.rs`); no portraits (83), no wizard sheet (75), no research dialogs (92), no spell book view (93). |
| **Diplomacy** (`Diplomacy_Screen__WIP`, `DIPLOMAC.c`) | The talk with a rival wizard: greeting, treaty proposals, threats, tribute, spell exchange. | Clicking another wizard's portrait on the Magic screen (refused while banished); also when an AI wizard opens a talk with the human at the start of a turn (`Npc_Diplomacy_Screen`). | **Missing.** Task 91; the rules are planned in `docs/diplomacy.md` (tasks 63, 64). |
| **Spellbook** (`Spellbook_Screen`, `SBookScr.c`) | The spell book the wizard casts from: pages by realm, the cost and the casting time, choose a spell to cast overland. In combat the Spell button opens a second book, `Combat_Spellbook_Screen`. | The **Spells** button on the main screen. | **Partial.** Casting is done from a list in the Magic screen; the book view is task 93. |
| **Advisor** (`Advisor_Screen`, `AdvsrScr.c`) | A menu of nine information screens (section 4). | The **Info** button on the main screen; F1 to F9 keys. | **Missing.** Task 71 (the Mirror is also task 75). |
| **Item screen** (`Item_Screen`, `ItemScrn.c`) | Heroes' equipment: move magic items between hero slots and the vault of the wizard; destroy an item; look at it. | The Items button on the Army list; after a hero or lair reward is taken, to place the items found. | **Missing.** New task 142 (items themselves: tasks 49, 50). |
| **Road building** (`Road_Build_Screen`, `Roads.c`) | Pick the end of a road for the stack's engineers: shows the map, how many turns the road will take, and refuses a road across a large body of water. | The **Build** (special) button on the main screen when the active stack has engineers. | **Partial.** Engineers build with `B` and a path chosen on the map; the original's screen is task 102. |
| **Production** (`Production_Screen`, `ProdScr.c`) | A pop-up list of what a city can build (buildings and units, with costs and the building each needs); choosing sets the production. | The Change button of the City screen; the City list's production column. | **Done** as the production menu of the city screen (`city_screen.rs`), not as a separate pop-up. |
| **Next turn** (`scr_NextTurn`, `Next_Turn_Proc`) | Processes the turn for all wizards, then plays the summary, messages and offers (section 5). | The **Next Turn** button or Enter on the main screen, when no units are left to move or the player confirms. | **Done** in the core (`turn.rs`); the summary and messages are missing (task 144). |
| Test and proof-of-concept screens (`scr_Test_Screen`, `scr_PoC_Screen`) | Developer stubs. | Never in play. | Not applicable. |

## 3. The new-game steps (`Newgame_Control`)

Each step is a full screen with its own function in `NewGame.c`; the control function moves back
and forth between them (Cancel goes back one step).

| Step | Original | What it does | Status here |
|---|---|---|---|
| 0 Options | `Newgame_Screen_0` | Difficulty, opponents, land size, magic power (the game options). | **Missing.** The seed, land size and opponents are given at launch; task 10. |
| 1 Wizard | `Newgame_Screen_1__WIP` | Choose one of the 14 ready-made wizards, or "Custom". | **Partial** (the creation screen offers ready-made wizards). |
| 2 Portrait | `Newgame_Screen_2__WIP` | Custom only: pick the wizard's picture. | **Missing.** Task 83 (portraits and banners). |
| 3 Name | `Newgame_Screen_3__WIP` | Type the wizard's name. | **Missing** (the wizard has a fixed name). Task 83. |
| 4 Picks | `Newgame_Screen_4__WIP` | Custom only: spend eleven picks on spell books, retorts and extras. | **Done** in the creation screen (eleven picks, task 101 for the eleven-book realms). |
| 5 Spells | `Newgame_Screen_5` | Custom only: choose the starting spells for each realm. | **Done.** |
| 6 Race | `Newgame_Screen_6__WIP` | Choose the wizard's race (home city race). | **Done.** |
| 7 Banner | `Newgame_Screen_7__WIP` | Choose the banner colour. | **Missing.** Task 83. |

## 4. The advisors (`Advisor_Screen`, `AdvsrScr.c`)

A menu of nine entries, each a screen or window. All are **Missing** (task 71) unless said.

| Key | Name | What it shows |
|---|---|---|
| F1 | Surveyor (`Surveyor_Screen`, `Surveyor.c`) | The map with a cursor; for the square under it: the terrain's food, production and gold bonuses, and the maximum population and bonuses a city founded there would get; says why a city cannot be built there (water, lair, tower, magic node, closer than three squares to another city). |
| F2 | Cartographer (`Cartographer_Screen__WIP`) | The whole world map of one plane, zoomed out; N switches plane, clicking a spot moves the main map there. |
| F3 | Apprentice (`Apprentice_Screen`, `Spellbook.c`) | The "big book": the library of every spell the wizard knows and, on the first page, the spells being researched. |
| F4 | History (`Powergraph_Screen`) | A graph of each wizard's power over the turns ("History Of Wizards Power"). |
| F5 | Astrologer (`Status_Screen`) | "Current Status Of Wizards": a comparison of the wizards (including research). |
| F6 | Chancellor (`Chancellor_Screen__WIP`, `REPORT.c`) | The report scroll: what happened this turn (see section 5). |
| F7 | Tax Collector (`TaxCollector_Window`) | Tax rate table: gold per population and the unrest each rate causes. The tax rate is chosen here. |
| F8 | Grand Vizier (`Grand_Vizier_Window`) | Asks whether the Grand Vizier may choose what the cities build (auto-build). Needs autobuild: task 16. |
| F9 | Mirror (`Mirror_Screen`, `WIZVIEW.c`) | A wizard's sheet: name, spell books, fame and, for another wizard, the treaty with you, personality and objective. Also opened from the Magic screen and used when a spell targets a wizard (`Target_Wizard_Screen`). This is the wizard overview of task 75. |

## 5. Windows opened from inside a screen

Not part of the `e_SCREENS` loop. The "entered" column says what leads to each.

### 5.1 Overland map and turn

| Window | What it does | Entered | Status here |
|---|---|---|---|
| Unit statistics (`Main_Unit_Statistics_Window`, `Overland_Unit_Statistics_Window`, `Unit_Statistics_Popup`) | The sheet of one unit: pictures, attributes with the bonuses, abilities, enchantments, level, items, and the Dismiss and OK buttons. | Right-clicking a unit in the stack panel, the Army list, the City screen's garrison or the Item screen; in combat the same sheet (`Combat_Unit_Statistics_Window`). | **Missing.** New task 140. (The battle screen has a text panel with the same numbers.) |
| Enemy stack list (`Unit_List_Window`) | The units of another player's stack on the map, read only. | Right-clicking a square holding another wizard's units. | **Missing.** Task 140. |
| Outpost screen (`Outpost_Screen`, `Outpost.c`) | A small city sheet for an outpost: its surroundings and what is being built; names a new outpost (header "New Outpost Founded"). | Clicking an own outpost; right after settlers found one (to name it). | **Missing.** New task 141. |
| Enemy city screen (`Enemy_City_Screen`, `City_ovr55.c`) | What is known about another wizard's city: size, buildings seen, garrison seen. | Clicking another wizard's city. | **Missing.** Task 141. |
| Rename home city (`Change_Home_City_Name_Popup`) | Text box to rename the capital. | Once, on the first turn (`_turn == 0`). | **Missing** (task 83 covers naming). |
| Rename city (`Change_City_Name_Popup`) | Text box to name a new outpost. | After founding an outpost. | **Missing.** Task 141. |
| Chancellor's summary (`Chancellor_Screen__WIP(0)`) | A scroll of the turn's news in sections: random events, units disbanded (deserted for lack of food or gold, dispelled for lack of mana, deserted to make room, drowned), units killed, city growth, city deaths, outposts that grew or were deserted. Optional ("end of turn summary" setting). | After the turn is processed. | **Partial:** the client's message log (`log`) shows the same events in a list; the scroll is task 144. |
| Building complete (`City_Built_Building_Message`) | A picture of the building with "X has completed the construction of Y". | After the turn is processed, one per finished building; then the City screen opens when a city can no longer make what it was building. | **Partial** (log lines only). Task 144. |
| Evict message (`Evict_Unit_With_Message`) | Tells that a unit was removed from a city that has no room. | When housing runs out. | **Missing.** Task 144. |
| Alchemy pop-up (`Alchemy_Popup`) | Converts gold to mana or back. | Button on the Magic screen and the Item screen. | **Partial** (core `alchemy`; the console has the command; the client has no pop-up). |
| Spell target selection (`Spell_Casting_Screen__WIP`, `SCastScr.c`) | Pick a square, city or unit on the map as a spell's target. | Casting a spell that needs a map target. | **Done:** the magic screen lets the player click the target on the map (`magic_screen.rs`). |
| Target wizard (`Target_Wizard_Screen`) / target enchantment (`Spell_Target_Global_Enchantment_Screen`) | Choose a wizard (portraits) or one of another wizard's global enchantments as a spell's target. | Wizard- or enchantment-targeting spells (Spell Binding, Subversion...). | **Missing.** Tasks 60 and 62 (the spells). |
| Spell mana adder (`Spellbook_Mana_Adder_Screen`, `Combat_Spellbook_Mana_Adder_Screen`) | Choose how much extra mana to put into an infusable spell (its strength grows with the mana). | Choosing such a spell from the book, overland or in combat. | **Missing** (task 43 for combat casting). |
| Item-making (`Item_Make_Screen`, `ItemMake.c`) | Design a magic item or artifact: choose its type, powers and name, with the mana cost. | Casting Enchant Item or Create Artifact. | **Missing.** Task 50. |

### 5.2 Offers, rewards and events

| Window | What it does | Entered | Status here |
|---|---|---|---|
| Merchant (`Merchant_Popup`) | A merchant offers a magic item for gold. | At turn start, by chance rising with fame (`Determine_Offer`). | **Missing.** New task 143 (events and offers: 68, 48, 72). |
| Mercenaries (`Hire_Merc_Popup`) | Offers a group of mercenary units for gold. | `Determine_Offer`. | **Missing.** Task 143. |
| Hero offer (`Hire_Hero_Popup`) | A hero offers service for gold; the player names the hero. | `Determine_Offer`; hero spells (Summon Hero, Incarnation); lair rewards. | **Missing.** Tasks 143, 48. |
| Hero level-up (`Hero_LevelUp_Popup`) | Announces a hero's new level and its gains. | When a hero levels up in `Do_All_Units_XP_Check`. | **Missing.** Task 143. |
| Event message (`Show_Event_Message`, `EVENTS.c`) | The random events (earthquake, plague, conjunction and so on) with a picture and text. | When a random event starts. | **Missing.** Tasks 68 (the events) and 143. |
| Lair treasure (`Lair_Treasure_Popup`, `Lair.c`) | Lists the gold, mana, items, spells and heroes won from a cleared site. | After winning a fight in a lair, tower or node. | **Missing.** Task 128. |
| Site encounter prompt (`Lair.c`) | A box naming the site and its kind, "... Do you wish to enter?". | Moving onto a guarded site. | **Missing.** Task 128. |

### 5.3 Combat

| Window | What it does | Entered | Status here |
|---|---|---|---|
| **Combat screen** (`Combat_Screen`, `Combat.c`) | The tactical battle: the 21 by 22 map, units, spell book, Wait, Done, Flee and Auto buttons; the result goes back to the caller. | A stack moves into an enemy unit, city or guarded site (`Move_Units`, `Lair_Combat`); the computer fights its own battles without showing the screen. | **Partial.** `battle_screen.rs`: the grid, figures, animation, controls (task 132). No spell casting (43), no Auto button or attack prompt (133), no walls (41). |
| Combat spell book (`Combat_Spellbook_Screen`) | Choose a combat spell. | The Spell button in combat. | **Missing.** Task 43. |
| Combat spell target (`Combat_Spell_Target_Screen`) | Pick the target square or unit. | After choosing a combat spell. | **Missing.** Task 43. |
| Combat information (`Combat_Information_Window`) | Battle-wide info: enchantments in force and so on. | The Info button in combat. | **Missing.** Task 132. |
| Vortex move (`Vortex_Move_Screen`) | The player moves a magic vortex on the battlefield. | Each combat round with a vortex cast by the player. | **Missing.** Task 44. |
| Retreat losses (`Build_Flee_Loss_Message`) | Tells which units were lost while fleeing. | After a flight from combat. | **Missing** in the client (core: `Event::UnitsLostFleeing`). Task 144. |
| Raze or keep (`Raze_City_Prompt`) | After taking a city: "Do you wish to completely destroy this city?" (skipped when the "raze city" option is off). | End of a combat won at a city. | **Missing.** Task 41. |
| Spell cast messages (`Combat_Cast_Spell_Message`, `Combat_Spell_Counter_Message`) | Banner with the spell's name when cast or countered. | Casting in combat. | **Missing.** Task 43. |
| Confirmations (`Confirmation_Box`, `Warn`) | Yes/No boxes: sell a building; turn off a spell ("Do you wish to turn off the ... spell?"); "Do you wish to flee?"; "... Do you still wish to attack?" (a wizard you have a treaty with); "Do you wish to completely destroy this city?"; and, when ending a turn, "Some units do not have enough food and will die unless you allocate more farmers in a city. Do you wish to allow them to die?" | Many places; the food warning when Next Turn is pressed. | **Partial:** the client asks twice for Flee; no common box. The food warning is missing (see task 32). Task 144. |

## 6. The end of the game

| What | Original | Status here |
|---|---|---|
| Win or lose animation | `Win_Animation`, `Lose_Animation` (`CONQUEST.c`): a short full-screen picture sequence. | **Missing.** Task 134. |
| Score screen | `End_Of_Game_Score` (`SCORE.c`): the score and its parts. | **Partial:** a plain game-over screen with the result and score (`game_over_screen.rs`). Task 134. |
| Hall of fame | After the score, then the main menu. | **Missing.** Task 88. |
| Triggered by | A wizard finishing the Spell of Mastery (checked each turn, `Get_Winner`); the last wizard standing; the human wizard banished. | **Done** in the core (`endgame.rs`). |

## 7. Screens this project has that the original lacks

| Screen | What it does | Entered |
|---|---|---|
| **Console** (`console_screen.rs`, `f4x-console`) | The text interface to the game: every action as a command, with legal and override modes. See [`console.md`](console.md). | F2 in the client; the `f4x-tools` binary runs it without the client. |
| **Game over** (`game_over_screen.rs`) | Plain result and score; stands in for sections 6's screens until task 134. | When the core reports the game ended. |
| **Wizard creation** (`create_screen.rs`) | One screen for steps 1, 4, 5 and 6 of section 3. | At start. |

## 8. What is missing, by task

Existing tasks: 10 (new-game options), 41, 43, 44, 48, 49, 50, 60, 62, 68, 71 (advisors), 75,
83, 85, 86, 88, 89, 91, 92, 93, 94, 96, 102, 117, 119, 120, 126, 128, 132, 133, 134. New tasks
added with this list:

| # | Screen or window |
|---|---|
| 138 | Army list screen |
| 139 | City list screen |
| 140 | Unit statistics window and the enemy stack list |
| 141 | Outpost screen, enemy city screen and the naming boxes |
| 142 | Item screen and the item pop-up |
| 143 | Offer, hire and event pop-ups |
| 144 | Turn summary and message pop-ups |
| 145 | Save and load screen with slots (done) |
