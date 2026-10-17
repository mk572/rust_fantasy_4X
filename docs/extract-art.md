# Extracting and using the original's art

How the original's pictures get from its `.lbx` files into this project, how palettes and fonts
work, and how to attach a picture to a screen or an object in the game. Code:
`crates/f4x-tools/src/{lbx,image,art,fonts,map_art}.rs` (extractor) and
`crates/f4x-client/src/{art,frame}.rs` (the client side). Where each picture is shown in the
original is read from the reverse-engineered source in `reference/ReMoM/MoM/src` (named below);
[`screen_list.md`](screen_list.md) lists the screens themselves and [`local-assets.md`](local-assets.md)
the IP rule.

**The art is protected by IP.** Everything under `assets-local/` is local only and gitignored.
Never commit a picture, an atlas, or a table that reproduces one; the game must run without them
(flat colours and the plain font).

## 1. Running it

```
cargo run --release -p f4x-tools -- extract-art "reference/mom-classic/Master of Magic Official Release"
```

Reads the original game's `.lbx` files (case-insensitive names) and writes `assets-local/art/`
(about 24,500 files, 170 MB). It is safe to run again: files are overwritten, nothing is deleted.
The client reads the same directory (`--assets DIR` to point elsewhere). It needs `FONTS.LBX`
for the palette and stops without it.

## 2. The input format

**`.lbx` archive** (`lbx.rs`): a `u16` entry count, the magic `0xFEAD`, then offsets; entry `i`
spans `offsets[i]..offsets[i + 1]`. An entry can be a picture, text, a data table or sound; the
extractor tries to decode every entry as a picture and skips the ones that fail (that is also how
data entries are told apart).

**Picture entry** (`image.rs`): 16-byte header (width, height, frame count, optional palette
offset), the optional palette block, a table of frame offsets, then column-wise run-length data.
Pixels are 8-bit **palette indexes**, not colours. Index 0 is transparent. Frame type 0 means each
frame is a delta drawn over the one before (the decoder keeps the buffer); other types start from
a blank frame. A picture may carry a few palette colours of its own: `(first colour, count, 6-bit
RGB triples)`.

## 3. The output

```
assets-local/art/
  manifest.tsv            lbx <TAB> entry <TAB> frames <TAB> width <TAB> height  (one row a picture)
  <lbx>/NNN_FF.png        entry NNN (3 digits), frame FF (2 digits), RGBA, index 0 transparent
                          (pictures shared by several screens, spell effects, icons)
  battle/tiles/<terrain>/ the battlefield ground, trees and rocks: grassland, desert, mountains,
                          tundra; <lbx>_NNN_FF.png from `cmbgrass`..`cmbtundc`, `desert`, `tundra`
  units/<unit id>/        a unit's art: picture_FF.png (overland, `units1`/`units2`),
                          figure_D_FF.png and figure_D_FF_banner.png (combat figure, direction D,
                          section 6), portrait_FF.png (heroes, `portrait`)
  figure*/NNN_FF_banner.png   a mask beside every combat figure frame (section 6); a
                          picture_FF_banner.png also sits beside each overland unit picture
  cursors/                NN.png, the original's sixteen 16 by 16 mouse pointers, numbered as
                          `crsr_Finger` (1), `crsr_CastBase` (2), `crsr_RedCross` (3), `crsr_Ranged` (4),
                          `crsr_Melee` (5), `crsr_Hourglass` (6), `crsr_WingedBoot` (7)... They sit in
                          the base palette's entry in FONTS.LBX, 1280 bytes in, a column at a time
  fonts/                  style0..7.png, metrics.tsv (section 5)
  map/                    catalog.tsv, banners.tsv (section 6) and three folders:
    tiles/                <plane>/<kind>/tile_NNNN.png (arcanus/shore/ ...), types.tsv,
                          autotile.tsv, minimap.tsv (the minimap's colours and other palette colours
                          the client needs by name: the battle screen's text, damage bar and the
                          glow of enchanted units' figures), sheet.png
                          (section 7)
    locations/            cities, towers, encounter sites and magic nodes: <name>_FF.png and, for
                          cities and towers, <name>_<banner>_FF.png (sections 6, 8)
    overlays/             roads/, resources/ (minerals, mines, camps, wild game), fog/ (unexplored
                          masks), unit_backgrounds/, ground/ (corruption, mud, city work area):
                          `mapback`, named as in catalog.tsv
```

Pictures that belong to one screen alone are written next to `art/`, not in it:

```
assets-local/
  index.tsv               lbx <TAB> entry <TAB> path from assets-local/ without _FF.png
                          (where each filed picture went: screens/..., art/units/..., art/map/...)
  screens/<screen>/<name>_FF.png   main_menu, load_save, main_screen, city_screen, combat, ...
                          (combat/ has folders: interface, city, walls, rivers, spell_effects and
                          cmbtwall_unknown, a file nothing loads)
```

`crates/f4x-tools/screens.tsv` is the table (lbx, first and last entry, screen, label; numbers
and names only). A labelled single entry is `<label>_FF.png` (`main_screen/button_done_00.png`),
anything else `<lbx>_NNN_FF.png`. Shared pictures (the grey gems, the OK button of Load and
Settings, `items`, `special`, `wizards`, `cityscap`) stay in `art/`. A picture two screens use lives with its main screen. To move more, add rows and run
`extract-art` again: it writes the picture in its new place and deletes the old copy in `art/`.
Unit art is filed by the unit's id from `content/units.ron` (its `art` and hero `portrait`;
unit type numbers: `unit_type_number`), so the extractor must run from the project directory.
The client finds every place through `Art::picture(lbx, entry, frame)` and `Art::figure`, which read
`index.tsv`, so the call sites keep the original's numbers.

Rules to remember:

- In `art/` the directory is the lower-case file name (`MAINSCRN.LBX` becomes `mainscrn/`); the entry number
  is the original's own, so the numbers in ReMoM's source (`LBX_Load(mainscrn_lbx_file, 5)`)
  are the numbers in the file names. An animation has one PNG per frame.
- A directory exists only for files with at least one decodable picture. `map/`, `fonts/` and the
  `units/` folders come from special passes; `FONTS.LBX`, `TERRAIN.LBX` and the terrain tables are not
  pictures in the normal sense.
- Pictures are not cropped or reordered, and positions are not stored: where a picture sits on
  the 320 by 200 screen is in the original's code (section 9).
- Terrain tiles are the one case where index 0 is a real colour (they are opaque).

## 4. Palettes

The original runs in a 256-colour VGA mode with one palette loaded at a time. The extractor must
pick the same palette the game had loaded when it showed a picture, or the colours come out wrong
(a garbled, noisy picture is the sign of the wrong palette).

- **Base palette:** `FONTS.LBX` entry 2, the first 768 bytes (256 triples of 6-bit RGB, values
  0 to 63; expanded to 8 bits by `vga_to_rgb8`). `Load_Palette(0)` is this one: the main game.
- **Palette in the picture:** a picture's own colours are laid over whichever palette is in use
  (`Load_Palette_From_Animation` in the original does the same from frame 0). Most screens with a
  special look work this way.
- **Other loaded palettes:** `Load_Palette(n)` reads `FONTS.LBX` entry `n + 2`. The calls in
  ReMoM, and what the extractor does about them (`palette_entry_for` in `art.rs`):

| `Load_Palette(n)` | `FONTS.LBX` entry | The original shows | Files extracted with it |
|---|---|---|---|
| 0 | 2 | the main game (base palette) | everything else |
| 1 | 3 | the Load / Save screen | `load` |
| 2 | 4 | the start menu ("Magic Castle View") | `mainscrn`, `vortex` |
| 3 | 5 | the conquest animation ("wizlab") | `conquest` |
| 4 | 6 | casting the Spell of Mastery | `splmastr` |
| 5 | 7 | the win animation | `win` |
| 6 | 8 | the lose animation | `lose` |

- **Checking:** `load/000` comes out the same whether it uses entry 3 or the base palette with
  its own colours; `lose` and `conquest` were visibly wrong without
  their palette (olive swirl, noisy figure). Compare a picture with a screenshot in
  `reference/screenshots/` when in doubt.
- **A file shown under several palettes:** `wizlab` (used by the spell book, the conquest and the
  Spell of Mastery screens) is extracted with the base palette plus its own colours; the screens
  that need another look may need their own copy. Nothing in the client uses it yet.
- **Palette cycling (task 146):** the original animates by changing one palette entry over
  time, so every pixel that uses that index changes colour at once, with no redrawing.
  `Cycle_Palette_Color(index, min r,g,b, max r,g,b, step)` (`Fonts.c`) moves the entry back and
  forth between two colours (6-bit values), one step per call. ReMoM only ever cycles index 198:
  from dark to bright red (40..63) on the main screen, load screen and menu, from black to red
  (0..55) in combat, and between pale greys or blues in the spell book. `extract-art` writes the
  pixels that use index 198 as a white `<picture>_<frame>_cycle.png` mask beside the picture (only
  where there are any: the intro, the wizard portraits, the menu and load screens, the end
  animations). The client (`cycle.rs`, `draw_pic_tinted`) draws the mask over the picture in the
  colour of the moment, for the files whose screen cycles (`cycle::for_file`: the menu and the
  load screen); the other screens keep the base colour, as in the original, which only cycles
  while the cycling screens run. Text uses the same colour (a negative income on the main screen).
  The combat and spell book ranges are not wired up: no combat picture uses the index, and the
  book's glow belongs to task 166.
- **Glass (translucent) pixels:** in a picture that brings no palette of its own, the indexes
  from 232 on are not colours but tints: each blends a colour into what is under it
  (`Create_Remap_Palette`; the 24 blocks, six-bit red, green, blue and a percent, are in the base
  palette's entry, 5376 bytes in; block 0 is the shadow, black at 50 percent). `extract-art` writes
  them as translucent pixels (tree and figure shadows, the glow of spell pictures). A picture with
  an embedded palette (the battle panel, the Info window) uses those indexes for colours.
- **Also not reproduced:** fades, and the colour remapping for disabled or "ghost" drawing.

**If a picture looks wrong:** (1) find where ReMoM loads it (`grep -rn "NAME" reference/ReMoM/MoM/src`);
(2) look for a `Load_Palette(n)` or `Load_Palette_From_Animation(...)` shortly before it;
(3) if it is a numbered palette, add the file to `palette_entry_for` with entry `n + 2`;
(4) re-run the extractor and add a line to the unit test in `art.rs`.

## 5. Fonts

`extract-art` decodes `FONTS.LBX` entry 0 into **eight font styles** of 96 glyphs (ASCII 32 to
127), `fonts/style0.png` to `style7.png`, and `fonts/metrics.tsv`.

- Glyph data is stored by columns and uses **colour indexes 1 to 15**, not palette indexes. A
  glyph's colour index maps to a palette index through one of **16 colour sets** (entry 2 at offset
  `0x300`, 16 bytes a set). So each atlas stacks the glyph strip 16 times, once per set; picking
  a set picks the text colour.
- `metrics.tsv`: `S <style> <height> <hspacing> <vspacing>` per style, and
  `G <style> <ascii> <x in the strip> <width>` per glyph. Heights are 6, 6, 9, 9, 10, 16, 5 and 10
  pixels for styles 0 to 7.
- In the client (`art.rs`, `frame.rs`): `Art::text_width`, `Art::nearest_font_set(style, colour)`
  (the set whose average colour is closest to the colour wanted) and `Art::draw_bitmap_text`;
  the helper to use on a screen is `App::bm_text(view, style, colour, text, x, y, Align)`, where
  `view` is `(origin x, origin y, scale)` of the 320 by 200 layout and `x, y` are in the
  original's pixels. `bm_text_outlined` adds the one-pixel black outline the original draws on
  gold and mana figures. Without the fonts both fall back to the plain font.
- The original picks a style with `Set_Font_Style(n)` (also `Set_Font_Style_Outline`, `_Shadow_Down`
  and the other variants, which add an outline or shadow) and colours with `Set_Font_Colors_15`;
  the style number in ReMoM is the style number here. Which style a label uses is read from the
  screen's draw function in ReMoM. The client mostly uses styles 1 and 3, and 5 for titles; task
  118 moves the rest over.
- Not done: `Print_Paragraph` word wrap and justification (task 118), and the outline, shadow and
  colour-table variants of the font calls (only the outline of `bm_text_outlined`).

## 6. Banner colours

Player colour is painted in at run time by replacing five fixed palette indexes with the banner's
five shades; the extractor does this ahead of time or leaves a mask:

- **Cities and towers on the map** (`MAPBACK.LBX`): `map/locations/<name>_<banner>_<frame>.png` for the six
  banners in the game's order (blue, green, purple, red, yellow, neutral/brown), made with
  `COL_City_Banner`. Indexes 214 to 218 (and 224 to 228 on towers) are swapped for the banner's
  shades, as `Draw_Map_Cities` does. `map/banners.tsv` has each banner's five shades as r,g,b.
- **Combat figures** (`figures1` to `figure16`): the figure pictures keep their palette indexes
  and a `_banner.png` mask marks the banner pixels (indexes 214 to 218; red channel 1 to 5, alpha
  255). The client paints the owner's shades at load (`paint_banner`, `Art::figure`).
- **Unit backgrounds on the map** (`mapback` 14 to 19) come already coloured, one per banner.
- Banner shade tables are `COL_Banners` (units) and `COL_City_Banner` (cities).

## 7. Terrain

`TERRAIN.LBX` entry 0 holds the 20 by 18 ground tiles (`map/tiles/<plane>/<kind>/tile_NNNN.png`, opaque, plus
`sheet.png` with all of them 40 to a row); entry 1 says which tile each of the 762 terrain *type
numbers* uses and whether it animates (`types.tsv`: type, first tile, animated flag, folder under `map/tiles/`, from the ranges in ReMoM's `TerrType.h`; an animated
tile's next three tiles are the other frames). `TERRTYPE.LBX` gives `autotile.tsv` (table, mask,
type): five tables of 256 entries indexed by a mask of the eight neighbours (NW 1, N 2, NE 4, E 8,
SE 16, S 32, SW 64, W 128), used to pick shore, mountain and desert edge pieces. The client
applies them in `terrain_pick.rs`; river tiles are laid over other ground with
`Art::river_overlay`.

## 8. Naming map sprites

`MAPBACK.LBX` holds nearly everything the overland map draws besides ground. `map/catalog.tsv`
gives each sprite a name (`name`, `lbx`, `entry`, `frames`, `width`, `height`), for example
`road_ne`, `enchanted_road_s`, `city_walled`, `tower_owned`, `site_cave`, `mineral_gold`,
`node_aura_blue`, `corruption`, `unexplored_mask_3`. The names come from `catalog_names()` in
`map_art.rs`; the entry numbers are the loads in ReMoM's `LOADER.c`. Use the name
when you can, the entry number when a lookup is by index (`map_draw.rs` does both).

## 9. Attaching a picture to a screen or an object

**Rule of thumb:** find the original's load call in ReMoM, take the file and entry number from
it, and place the picture where the draw call puts it on the 320 by 200 screen.

1. **Find it.** `grep -rn "FILENAME" reference/ReMoM/MoM/src/*.c`, or in `LOADER.c` for pictures
   shared by many screens (`Terrain_Init`, `Main_Screen_Load_Pictures`, `City_Screen_Load_Pictures`,
   `Reload_Combat_Graphics_Cache`, `Reload_Cityscape_Graphics_Cache`, `Reload_Item_Pictures`,
   `Cache_Graphics_Diplomacy`). A call like `LBX_Load(main_lbx_file, 8)` is `main/008_00.png`; `LBX_Reload_Next` is the
   same. The matching `FLIC_Draw(x, y, picture)` gives the position.
2. **Draw it** on the original's layout, scaled by whole numbers: the client's `frame::Layout`
   maps the 320 by 200 layout to the window (`lay.point(x, y)`, `lay.rect((x, y, w, h))`).
   `App::draw_pic(&lay, "main", 8, frame, x, y)` draws entry 8 of `main.lbx` at `(x, y)` and
   returns its rectangle (None when the art is missing, so draw a plain box then);
   `draw_pic_tinted` multiplies by a colour (faded for a disabled control). The raw call is
   `art.picture("main", 8, frame)`, a cached `Texture2D`.
3. **Buttons:** many of the original's buttons have two frames (up, pressed); some have one and
   are drawn a pixel down and right when pressed (the load screen does this). Check the picture's
   frame count in `manifest.tsv`. Hit areas are separate
   from the picture (`Add_Hidden_Field` in ReMoM), which is why the menu rows are wider than their
   words.
4. **Object art in the game's data:** the rules data carries only a reference, for units
   `art: Some(("units1", 5))` in `content/units.ron` (file, entry); `units1` entry `n` is unit type
   `n` and `units2` entry `n` is type `120 + n` (`unit_type_number`). Nothing else in `content/`
   names art; the rest is looked up by id or by the original's own numbering (below).
5. **Always allow the picture to be missing.** Every draw returns `None` or `false` without the
   art; give it a fallback.

### Where the entries are used

| What | File and entries | Notes |
|---|---|---|
| Overland frame, buttons, side panels | `main` 0 frame; 1 to 7 menu buttons (Game, Spells, Armies, Cities, Magic, Info, Plane); 8 to 15 Done, Patrol, Wait, Build and their locked versions; 18 to 23, 36 to 38 movement icons; 24 to 29 unit backgrounds; 34, 35 next-turn parts; 58 Next Turn button; 59 on little event icons | positions: `frame.rs` `spot` |
| Overland map sprites | `mapback` (named in `map/catalog.tsv`; now in `map/locations/` and `map/overlays/`) | section 8 |
| Terrain | `map/tiles/` | section 7 |
| Unit pictures (list and stack) | `units1`, `units2` | by unit type; now `art/units/<id>/picture` |
| Combat figures | `figures1` to `figure16`: 15 unit types to a file, 8 directions each, 4 frames | `figure_entry(unit_type, direction)`; now `art/units/<id>/figure_D` |
| City screen background and parts | `backgrnd` 6 background, 7 to 9 Buy, Change, OK; icon runs starting at 40 (small resources), 45 (workers), 59 (farmers), 74 (rebels), 88 (big resources), 94 and 95 (grey bread) | `city_screen.rs`; `LOADER.c` |
| City picture | `cityscap` (now `screens/city_screen/cityscape/`): 0 to 5, 8 to 10, 115, 116 are the backdrop layers of the Arcanus and Myrror city picture; the rest are buildings, houses and trees | `city_screen.rs`; layout: `cityscape.rs` (`Cityscape_Make_Buildings_Array`) |
| City walls, rivers in combat | `citywall`, `chriver`, `cmbtcity`, `wallrise` | combat city battles (task 41) |
| Battle ground | `cmbgrass`, `cmbdesrt`, `cmbmount`, `cmbtundr`; the `c` variants (`cmbgrasc` ...) are Myrror | entries 0 to 3 plain ground, 48 to 52 trees, 53 to 57 rocks; filed as `art/battle/tiles/<terrain>/<lbx>_NNN_FF.png` (grassland, desert, mountains, tundra) |
| Battle buttons | `compix` (frame 1 is pressed) | `battle_screen.rs` |
| Spell effects | `cmbmagic` (ranged attacks, 8 directions each), `cmbtfx`, `specfx` | combat and overland spell animations |
| Start menu | `mainscrn` 0 title (320x41), 5 background (320x159) at y = 41; `vortex` 1 Continue, 2 Hall of Fame, 3 Quit, 4 New Game, 5 Load Game (frame 0 hover, 1 idle) | `menu_screen.rs`; entries at x = 123, y = 141 plus 12 a row |
| Load / Save screen | `load` 0 background; 1 Load, 2 Quit, 3 Save, 4 OK, 6 and 7 the inactive Load and Save, 9 text field, 10 selection marker, 12 Settings | `menu_screen.rs` |
| Wizard portraits | `wizards` entry = wizard id (109x104); `lilwiz` entry = wizard id x 5 + banner (41x43 gems); `moodwiz` entry = wizard id (diplomacy mood pictures) | not used yet (task 83) |
| Diplomacy | `diplomac` (backgrounds, gargoyle eyes, treaty icons), `backgrnd` 18 the mirror | task 91 |
| New game | `newgame` | wizard creation (task 83) |
| Hero portraits | `portrait` (32x32, used by the Army list and the Item screen); `reload` holds screen backgrounds (advisors, city list, lair) and 41x43 pictures | tasks 48 to 50 |
| Items | `items` (17x17 item icons), `itemisc` (small icons of item powers and slots), `armylist` (Army and Item screens) | tasks 138, 142 |
| Unit sheet | `unitview` (large and small backgrounds), `special` and `special2` (16x16 ability icons) | task 140 |
| Spell book and library | `book`, `scroll`, `spellscr`, `spells`, `special2` (small book) | tasks 92, 93 |
| End of game | `win`, `lose`, `conquest`, `wizlab`, `splmastr`, `spellose`, `halofam`, `intro` | tasks 85, 88, 134 |
| Events and offers | `events` (event pictures, 41 by 43, entry = event number less one; drawn in the `Notify2` box: `resource` 40 strip, 42 picture frame, 51 to 55 the mascots chaos, death, life, nature, sorcery), `hire` (mercenary and hero offers), `resource` | tasks 68, 143; `event_popup.rs` |
| Help | `help` (two pictures) | |
| Files with no reference in ReMoM's loader | `combat`, `desert`, `tundra`, `cmbtwall` (`desert` and `tundra` are battlefield ground sets like `cmbdesrt`, filed under `art/battle/tiles/desert` and `tundra`) | they extract fine; their use in the original is not found, so treat as unknown |

The table is a guide. For anything with an index not listed here read the loader or the screen's
`Load` function in ReMoM; entries often depend on the player's wizard, banner or race.

## 10. What is not extracted

- **Sound and music:** `MUSIC.LBX`, `SOUNDFX.LBX`, `CMBTSND.LBX`, `INTROSND.LBX`, `INTROSFX.LBX`,
  `NEWSOUND.LBX`, `SNDDRV.LBX` and the `FAT*` instrument banks. Tasks 76, 97.
- **Data tables, read by `scripts/import_mom.py` into `content/` (names and numbers):**
  `SPELLDAT`, `BUILDDAT`, `ITEMDATA`, `TERRSTAT`, and `HELP`/`HLPENTRY` and `CITYNAME` into
  `content-local/` (they are IP).
- **Data and text not read yet:** `HERODATA`, `ITEMPOW`, `LISTDAT`, `DESC`, `BUILDESC`,
  `DIPLOMSG`, `NAMES`, `CONFIG.MOM`.
- **Videos:** the opening and ending are frame sequences (`intro` has 13 full-screen pictures) with
  their own timing in `INTRO.c` and `CONQUEST.c`; neither is wired (tasks 85, 134).

## 11. Adding to the extractor

1. A new kind of data gets its own pass in `art.rs` (like `extract_terrain`) writing under its own
   subdirectory, plus a `TSV` file for anything numeric. Keep the format plain text so the client
   reads it without a dependency.
2. A new palette: section 4. A new recoloured sprite: `map_art.rs` (a name in `catalog_names()`,
   the five indexes in the banner table).
3. Add a unit test next to the code (the tests in `art.rs`, `fonts.rs`, `image.rs` use tiny
   hand-made inputs, never the original's data).
4. Update this document and, for a new screen, [`screen_list.md`](screen_list.md).

## 12. Known gaps

- Palette cycling and fades are not reproduced (section 4).
- `wizlab` and a few other files are shown under more than one palette in the original; only the
  base one is extracted.
- Entries that do not decode are skipped without a message, so a missing picture looks the same
  as a data entry; `manifest.tsv` lists what was written, and the gaps in its entry numbers are
  data, text or sound, or a picture the decoder does not understand.
- Animation timing, the order frames play in, and which frames are "idle" or "pressed" come from
  the original's code and are not stored with the pictures.
