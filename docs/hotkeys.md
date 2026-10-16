# The original's hotkeys

What each key does in Master of Magic, read from ReMoM (`reference/ReMoM/MoM/src`, the field
setup of each screen: `Add_Hot_Key`, `Add_Button_Field`, `Add_Hidden_Field`). The client follows
this table; where it adds a key of its own, that is marked **extra**. Letter keys are not case
sensitive. "Not yet" means the screen or action does not exist in our game.

## Main screen (`MainScr.c`)

| Key | Original | Client |
|---|---|---|
| F1 | Advisor: Surveyor | not yet (screen missing) |
| F2 | Advisor: Cartographer | not yet |
| F3 | Advisor: Apprentice | done |
| F4 | Advisor: History | not yet |
| F5 | Advisor: Astrologer | not yet |
| F6 | Advisor: Chancellor | not yet |
| F7 | Advisor: Tax Collector | not yet |
| F8 | Advisor: Grand Vizier | toggles the Grand Vizier (the screen's one setting) |
| F9 | Advisor: Mirror (your wizard sheet) | done (a rival's sheet: right-click its gem on the Magic screen) |
| F10 | Quick-save (save slot 9) | done |
| N | Next turn | done (**extra:** Enter does the same) |
| G | Game button (save, load, quit menu) | done (also Esc) |
| S | Spells button (the spell book) | done |
| A | Armies button (army list) | done (task 138) |
| M | Magic button | done |
| I | Info button (the advisor menu) | not yet |
| P | Plane button: show the other plane | done (shifts the selected units first, if they can) |
| (none) | Cities button (no key) | not yet |
| D | Done: the selected stack is finished for this turn | clears the stack's orders only (no idle-stack cycling yet) |
| W | Wait: skip to the next idle stack | not yet |
| B | Build (outpost; with engineers a road: click the far end, a blue line (red if water blocks it) shows the path and the side panel the turns, then OK, O, Enter or B builds, Escape or right-click cancels); the Purify and Meld buttons have no key | done (Meld also on B for a spirit) |
| C | Centre the map on the selected stack | done |
| Q or U | Unselect the stack | done |
| Numpad 1 to 9 and the arrow keys (the source calls them the eight direction keys) | Move the selected stack one square | done |
| X | nothing | none |
| Z, T | Debug and test keys | none |
| Alt-K, Alt-A | Debug (commented out in the source) | none |

The original scrolls the map with the mouse at the screen edge; arrows with nothing selected scroll in the
client (**extra**, as is Shift plus an arrow with a stack selected).

## Extras of the client (no counterpart in the original)

| Key | Action |
|---|---|
| Ctrl-L | Quick-load |
| Ctrl-Z | Undo the last order |
| Ctrl-N | New random map (development) |
| Ctrl-R | Next race (development) |
| T | Balance farmers to feed the army |
| Tab | Show the other plane |
| `+`, `-` | Zoom |
| `/` | Show the key list |
| `` ` `` (backquote) | The text console |

## City screen (`CityScr.c`)

| Key | Original |
|---|---|
| B | Buy |
| C | Change (production) |
| O | OK (close) |
| Esc | Close |
| X | Debug |

Production screen (`ProdScr.c`): C Cancel, O OK, Esc.

## Lists and information screens

| Screen | Keys |
|---|---|
| City list (`CityList.c`), army list (`ArmyList.c`) | U scroll up, D scroll down, Esc leave |
| Unit view (`UnitStat.c`) | O OK, D Dismiss, U and D arrows (previous and next unit), Esc, X |
| Magic screen (`MagicScr.c`) | A Alchemy, Esc leave. Typed strings (RVL, PWR, DESTIN) are debug codes. |
| Spell book (`Spellbook.c`, `SBookScr.c`) | F next page, B previous page, Esc |
| Advisor screens (`AdvsrScr.c`) | N next, C close, Esc |
| Mirror (`WIZVIEW.c`), Surveyor, Settings, Score, Item screen | Esc leaves (the Mirror also on a click) |
| Hire dialog (`HIRE.c`) | H or A for the Hire button (the key depends on its text) |
| After taking a city (`Combat.c`) | R raze, N keep |
| Lair dialog (`Lair.c`) | Y yes, N no |
| Main menu (`MainMenu.c`) | C Continue, L Load, N New game, H Hall of fame, Q Quit, Esc |
| Load screen (`LoadScr.c`) | Esc |

## Battle (`Combat.c`)

| Key | Original |
|---|---|
| S | Spell |
| W | Wait |
| D or Space | Done with the unit |
| U | Info (the unit's sheet) |
| A | Auto (the computer plays the battle) |
| F | Flee |
| Esc | Cancel auto, or leave a sub-dialog |

The client keeps Tab (next unit) and E / Enter (end the turn) as **extras**.

## Source notes

- The key names in the source are `ST_KEY_*` (`MoX/src/MOX_KEYS.h`); the arrow keys and the numpad
  digits are the same eight direction keys.
- `Add_Multi_Hot_Key_Field` is a typed string; the only ones are the Alt debug keys and the Magic
  screen codes above.
