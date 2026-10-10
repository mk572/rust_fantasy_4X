# Planned modifications

Changes and extensions to do **after the basic game is playable** (the core loop of
`docs/TASKS.md` working: map, cities, magic, combat, AI, victory). Unlike the task list, these
are not gaps against the original: they are improvements and options on top of it. Keep the
table in ascending order by number; new items take the next free number.

Related tasks in `TASKS.md` are given as `T<number>`.

## Table

| # | Area | Summary |
|---|---|---|
| 1 | Movement | Faster movement along rivers |
| 2 | Options | Spell of Return setting: off, player only, all wizards |
| 3 | Options | More NPC wizards (the base game has up to four opponents) |
| 4 | Content | Rules changes and new items from the community patch and the Caster of Magic expansion |
| 5 | Platform | Modding support |
| 6 | Client | Upscaled graphics |
| 7 | City | Smarter city autobuilder |
| 8 | Options | Hardcore mode |
| 9 | Client | Battle success hint |
| 10 | Client | Font and TrueType support |
| 11 | Client | Higher resolution modes |
| 12 | Content | Replacement art |
| 13 | Client | Different settlement icons by race |

## Descriptions

**1. Faster movement along rivers.** Rivers exist as a tile flag (T1, done) but do not affect
movement. Add a rule that a step between two river tiles (or from a river tile to the shore at
its mouth) costs less, like roads do (half a move; decide whether it stacks with roads, which
should not give a quarter move), for land units and perhaps cheaper still for boats-on-rivers if
ships ever sail them. Make it a rule in data (a movement cost modifier per river step) so it can
be switched off, and show river-assisted paths in the path preview. Needs a decision on
direction (up or down stream) before any asymmetric rule; the river layer currently has no flow
direction.

**2. Spell of Return options.** Today every banished wizard can cast the Spell of Return
(`fortress.rs`, `magic.md` section 12), and an AI wizard's resignation rule is task 82. Add a
new-game setting with three values: **off** (banishment is final: the wizard is defeated),
**player only** (the human may return, AI wizards are defeated on banishment) and **all
wizards** (as in the original). Store it in the game settings saved with the game (new field,
`serde(default)` = all wizards), check it in `capture_city` where banishment is decided, and
mention it in the new-game screen (T10, T75).

**3. More NPC wizards.** The base game allows one to four opponents (T10). Make the count a
setting beyond four: more wizard names, portraits and banner colours (the client's
`player_color` palette), larger default maps and more starting positions so wizards do not crowd
each other (spread by minimum distance rather than a fixed table), a check that `PlayerId` (a
`u8`) and the fog and save layouts cope, and a diplomacy screen that scales (T63, T75). Balance
note: AI cost grows with wizard count, so measure turn time first (T65).

**4. Community patch and Caster of Magic content.** Collect the rule changes of the community
patch (its files are in `reference/mom-classic/Master of Magic Community Patch/`) and of the
Caster of Magic expansion: list every changed number, ability, building, unit, spell and item;
decide per change whether it becomes the default, a rule-set option or a mod (see 5); and add
the new items, units and spells to `content-local/` through `scripts/import_mom.py` where they
come from original files, or hand-write CC0 equivalents for the repository. Details of the
expansion are not yet gathered here, so the first step is a survey document in `docs/` listing
what exists, before any code. The importer's conflict rules (`help-vs-implementation-conflicts.md`)
apply: patch behaviour that fixes a marked bug wins; anything else is an option.

**5. Modding support.** The rules are already data (`content/*.ron`, validated by
`f4x-tools validate`). Make this a supported feature: load a stack of content directories
(base, then mods in order, later ones override or extend by id); a mod manifest with name,
version and dependencies; validation that reports conflicts by id; a list of the engine hooks
(`CUSTOM_HOOKS`, effect primitives) as the documented extension surface, and a decision on
scripting (none, or a small embedded language) for effects the data cannot express; mod-aware
saves that record the active mods and refuse to load without them; a mod picker in the client.
Document the data formats in `docs/` for modders. Keep original-derived data out of the
repository, as for all content.

**6. Upscaled graphics.** The client draws the original's 20x18 tiles and small unit pictures,
scaled with nearest-neighbour. Add an optional higher-resolution art path: an upscaling step in
`f4x-tools` (a documented, local-only pipeline, for example a pixel-art upscaler or a
diffusion-based one run on the user's own machine) writing to `assets-local/art-hd/`, and a
client setting that prefers it when present, with the same file naming so the existing loader
works. The output of upscaling original art is still original-derived and stays out of the
repository; the repository can only ship tools. Also check tile seams and shore blending
(the autotile table, T4) at the new size, and let the UI scale independently.

**7. Smarter city autobuilder.** The basic autobuild (T16) picks the next sensible building.
Improve it: weigh the city's needs (food deficit, unrest, gold, production, defence), the
buildings' upkeep against the city's income, the race's strengths, what the neighbours already
provide, the wizard's strategy (retorts, books) and the game stage; avoid buildings that are
about to be obsolete; let the player set priorities or a build profile (growth, production,
research, defence); and share the evaluation with the AI's city production (T65) so both use
one scoring function, tested on fixed city situations.

**8. Hardcore mode.** A new-game option for one-life play, bundling three rules: **no undo** (the
undo of orders, task 90, is off for the whole game); **permanent death** (a banished wizard is
defeated at once, whatever the Spell of Return setting of item 2 says, and the spell is not
available); and **one save file, constantly updated**: the game saves automatically after every
command that matters and at every turn end into a single file, cannot be saved elsewhere, and
cannot load an older state (loading the file continues the game, and a crash or quit never
rolls back). Store the flag in the saved game settings, show it in the new-game screen (86)
and on the hall of fame entry (88) so hardcore wins can be told apart, and make the client
refuse the quick-load key and the file picker while it is on. Closely related to the settings
work of tasks 89 and 90.

**9. Battle success hint.** Before the player commits to a fight (moving into an enemy stack,
a lair or a city), show an estimate of how it will go: a win chance, the expected losses on
each side and a plain verdict ("easy", "even", "risky", "hopeless"). Compute it by
simulating the battle many times with the real resolution code (`battle/attack.rs`, seeded
and thrown away, using the strategic auto-resolve of task 39 for speed) on copies of the
units as they stand, including walls, spells and heroes as they get implemented. Show it in a
tooltip over the target square and in the confirmation before an attack, and make its
accuracy and visibility an option (off in hardcore mode, item 8; "fog of war" versions that
estimate from only what the player has seen are an extension). Depends on the battle views
of task 106 and on auto-resolve (39).

**10. Font and TrueType support.** Today the client draws text with macroquad's built-in font and,
once task 113 is done, with the original's bitmap fonts. Add real font support: load TrueType
and OpenType files (and bitmap fonts in a simple data format) through a font setting, so a
player or a mod can choose the UI font, with a **font set** per role (title, heading, body,
small, numbers) and fallbacks when a glyph is missing. Needs: a text layout layer shared by
all screens (measure, wrap, centre, shadow and outline, colours, scaling that follows the
window), crisp rendering at any scale (rasterise at the target size, cache glyph atlases), full
Unicode and right-to-left and CJK coverage as an extension (the game's names and help are
English today but modded content may not be), and a licence check: only fonts under licences
the project can ship go into the repository (for example an open blackletter pixel-style face
to stand in for the original's display font), while the original's bitmap fonts stay local.
Modding support (item 5) should let a mod ship its own fonts and map them to the roles.

**11. Higher resolution modes.** The client keeps the original's 320 by 200 layout and scales it by a
whole factor (`frame::Layout`, the city screen's `Layout`), so a bigger window just shows a
bigger copy. Add real higher-resolution modes: layouts designed for 1920 by 1080 and beyond
and for wide and ultrawide windows, where the map view shows more tiles instead of larger
ones, side panels use the extra width (lists, message log, minimap, spell and research
panels), the city screen spreads out (a larger cityscape, more room for units and
enchantments), and UI scale is a separate setting from the map zoom. Needs the font work (item
10), art that holds up at the new sizes (items 6 and 12), a layout system with breakpoints
(compact, standard, wide) shared by all screens, high-DPI handling, and a screenshot test per
breakpoint. Keep the original layout as one of the modes.

**12. Replacement art.** The placeholder art comes from the author's own copy of the original and is
local only; the repository must stay CC0 and original-free. Before any public release, source
art that can be shipped: commissioned, generated or CC0 pieces for terrain tiles, units,
buildings, portraits, the interface frame, battle scenery and fonts, in the same sizes and
file layout as the extracted set so the client loads them unchanged (`assets/` next to the
code, `assets-local/` still taking precedence for a player who has the originals). Decide the
style (faithful pixel art, or a new look suited to the higher resolutions of item 11), list
every picture the client uses (the manifest of `extract-art` is the checklist), and give each
a source and licence in a credits file. Moved here from the task list (formerly task 79)
because it goes beyond the original game.

**13. Different settlement icons by race.** The original draws every city on the overland map with the
same walled or open picture in five sizes, recoloured by banner; only the city screen's
houses differ by race (standard, tree or mound houses). Give each race its own settlement
look on the map and wherever a settlement is shown: its own pictures for outposts, hamlets,
villages, towns and cities, with and without walls, in the owner's banner colour, so that an
orc town, a dark elf grove and a lizardman mound village can be told apart at a glance.
Needs art for fourteen races (item 12 for anything not drawn from the original), a settlement
sprite table in the content data (race, size, walls) with the original's pictures as the
default, a client lookup that falls back to the default, and the same set in the minimap's
city markers and the city screen's header.
