# The original's data files

What each `.LBX` file of the original game holds, which of them carry rules or text, and where this
project reads them. Written 24 October 2026 (tasks 242 and 243) from the author's copy of the game
and the ReMoM clone: the counts are read from the files, the use from the source that loads each
(`~/projects/ReMoM/MoM/src`). The art is local only (`docs/local-assets.md`).

An `.lbx` is a list of entries (`docs/extract-art.md` section 2). A data entry starts with two
`u16`s, the number of records and the size of one, then the records.

## 1. Files with rules or text

| File | Entries | Holds | Read by this project |
|---|---|---|---|
| `SPELLDAT.LBX` | 1 | the spell table (cost, realm, rarity, research cost, cast sound) | `import_mom.py`, `sound_table.py` |
| `BUILDDAT.LBX` | 1 | the building table | `import_mom.py` |
| `ITEMDATA.LBX` | 1 | the item table (picture number, cost, powers) | `import_mom.py`, `item_icons.py` |
| `ITEMPOW.LBX` | 1 | 67 records of 30 bytes: the names of the item powers (" +1 Attack") | no: no screen of ours uses it |
| `HERODATA.LBX` | 1 | 150 records of 10 bytes: five copies of 30 hero types; a leftover of an earlier design, read by nothing (`heroes-items-design.md`, the table of sources) | no |
| `TERRSTAT.LBX` | 2 | entry 0: 770 records of 6 bytes, the half-move cost of each terrain type per movement mode; entry 1: the 14 by 14 racial unrest table | the unrest table is in `rules.ron` (`racial_unrest`), identical; the costs are compared in `divergences.md` section 34 |
| `TERRTYPE.LBX` | 1 | 2564 bytes: the generator's lookup from a square's neighbours to its terrain tile (the autotile table) | no (task 150, generator parity) |
| `NAMES.LBX` | 1 | five hero names per hero unit | `import_mom.py` (`hero_names.ron`, local) |
| `CITYNAME.LBX` | 1 | the city names by race | `import_mom.py` |
| `HELP.LBX`, `HLPENTRY.LBX` | 3, 41 | the help text and the table of help topics and the area of each | `import_mom.py` (`help.ron`, local) |
| `EVENTMSG.LBX` | 1 | 126 records of 200 bytes: the random events' messages | `import_mom.py` (`event_texts.ron`, local) |
| `DESC.LBX` | 1 | 241 records of 110 bytes: the one-line description of each spell, in the spell book order | no: no screen of ours uses it (section 3) |
| `BUILDESC.LBX` | 1 | 36 records of 200 bytes: the description of each building | no: no screen of ours uses it (section 3) |
| `LISTDAT.LBX` | 1 | 75 records of 200 bytes: the names of the city screen's lists (Farmers, Animist Guild, Granary, Wild Game) | no: no screen of ours uses it |
| `MESSAGE.LBX` | 1 | 100 records of 150 bytes: the short messages and prompts | no longer: the game has its own in `content/text` |
| `DIPLOMSG.LBX` | 2 | entry 0: 74 records of 6 bytes, an index; entry 1: the diplomacy speech, 222 KB of text | no: task 168 (the diplomacy dialog as a local resource) |

No record of `DESC`, `BUILDESC`, `LISTDAT`, `MESSAGE` or `EVENTMSG` has data after its text (checked:
every byte after the terminator is zero), so no rule hides beside the text.

The unit table, the race table, the wizard presets and the premade heroes are not in a `.LBX`: they are
in the program, which ReMoM transcribes (`UNITTYPE.c`, `NewGame.c`, `_wizard_presets_table`).

## 2. Files of pictures, animations and sound

| Group | Files | Notes |
|---|---|---|
| Map and overland | `TERRAIN`, `MAPBACK`, `CHRIVER`, `CITYSCAP`, `CITYWALL`, `UNITS1`, `UNITS2`, `UNITVIEW`, `MAIN`, `MAINSCRN`, `ITEMS`, `ITEMISC`, `BACKGRND`, `ARMYLIST` | tiles, the city picture, unit pictures, the main screen and lists |
| Combat | `COMBAT`, `COMPIX`, `CMBTCITY`, `CMBTWALL`, `CMBTFX`, `CMBMAGIC`, `CMBGRASS`, `CMBGRASC`, `CMBDESRT`, `CMBDESRC`, `CMBMOUNT`, `CMBMOUNC`, `CMBTUNDR`, `CMBTUNDC`, `DESERT`, `TUNDRA`, `WALLRISE`, `VORTEX`, `FIGURES1` to `FIGURES9`, `FIGURE10` to `FIGURE16`, `MONSTER` | battlefield ground, figures, effects |
| Magic and spells | `MAGIC`, `BOOK`, `SPELLS`, `SPELLSCR`, `SPECFX`, `SPLMASTR`, `SPELLOSE`, `WIZLAB`, `SPECIAL`, `SPECIAL2` | the spell book, cast animations, the Spell of Mastery and its loss |
| Wizards and heroes | `WIZARDS` (14 portraits), `LILWIZ`, `MOODWIZ`, `PORTRAIT`, `HIRE`, `NEWGAME`, `DIPLOMAC` | portraits, moods for the diplomacy screen, the new game screens |
| Screens and ends | `RESOURCE` (pictures of boxes and lairs), `SCROLL`, `LOAD`, `RELOAD`, `EVENTS`, `HALOFAM`, `CONQUEST`, `WIN`, `LOSE` | |
| Intro | `INTRO`, `INTROSFX`, `INTROSND` | `INTRO` is played by `intro_screen.rs` (task 85); the intro song waits for task 363 |
| Fonts | `FONTS` | `extract-art.md` section 5 |
| Sound and music | `SOUNDFX`, `NEWSOUND`, `CMBTSND`, `MUSIC`, `SNDDRV` | `f4x-tools extract-audio`; `SNDDRV` is the sound driver, not read |

`SPLMASTR.LBX` (32 entries, 1.5 MB) is the art of the Spell of Mastery's cast and loss animations
(`SPLMASTR.c`), not data; `SPECIAL` and `SPECIAL2` are interface art (`LOADER.c` loads them by entry);
`WIZARDS.LBX` holds the 14 wizard portraits (`gsa_WIZARDS_0to13`). None of the three carries rules or
text. The pictures that need a task are listed in `TASKS-ONHOLD.md` (147, 287).

## 3. What the unread text could still give

- `DESC.LBX`: a short spell description for a spell book that does not use the help text.
- `BUILDESC.LBX`: a building description for the production screen. Both are the original's prose and
  would stay local (`content-local/`, like the help text) if a screen wants them; none does today, so
  they are not imported.
- `DIPLOMSG.LBX`: the speech of the diplomacy screen (task 168).
