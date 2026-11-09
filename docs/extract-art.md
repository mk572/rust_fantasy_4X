# Extracting and using the original's art

How the original's pictures get from its `.lbx` files into this project, how palettes and fonts
work, and how to attach a picture to a screen or an object in the game. Code:
`crates/f4x-tools/src/{lbx,image,art,fonts,map_art}.rs` (extractor) and
`crates/f4x-client/src/{art,frame}.rs` (the client side). Where each picture is shown in the
original is read from the reverse-engineered source in `~/projects/ReMoM/MoM/src` (named below);
[`screen_list.md`](screen_list.md) lists the screens themselves and [`local-assets.md`](local-assets.md)
the IP rule.

**The art is protected by IP.** Everything under `assets-local/` is local only and gitignored.
Never commit a picture, an atlas, or a table that reproduces one; the game must run without them
(flat colours and the plain font).

## 1. Running it

```
cargo run --release -p f4x-extract -- art "reference/mom-classic/Master of Magic Official Release"
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
Settings, `items`, `wizards`, `cityscap`) stay in `art/`. A picture two screens use lives with its main screen. To move more, add rows and run
`f4x-extract art` again: it writes the picture in its new place and deletes the old copy in `art/`.
Unit art is filed by the unit's id from `crates/f4x-def/content/units.ron` (its `art` and hero `portrait`;
unit type numbers: `unit_type_number`), so the extractor must run from the project directory.
Spell art is filed by the table `crates/f4x-client/content/spell_art.ron` (spell id to `(lbx, entry, role)`): a picture
of one spell goes to `art/spells/<spell id>/<lbx>_NNN_FF.png`, one that several spells use or that
belongs to none (the realm pictures, the summon scene, the boxes, the masks) to `art/spells/_shared/`,
and the spell book's twelve pictures (`spells.lbx`) to `art/spells/_book/`. The 16x16 ability icons of the unit sheet (`SPECIAL.LBX` and the first 34 entries of `SPECIAL2.LBX`, the rest of that file is the small book and its marks) go to `art/unit_ability/NNN_FF.png`, numbered by the picture index of the original's `special_seg` (`SPECIAL2.LBX` continues at 111). The summon scene's creature pictures (`MONSTER.LBX`: the entry is the summoned unit's number minus the Magic Spirit's, 44 and 45 the heroes, 46 the sword) go to `art/summoning/NNN_FF.png`; entries 36, 37, 38, 40 and 47 are 1x1 stubs in the original's own file, which `f4x-extract art` skips, so Floating Island, Phantom Beast, Phantom Warriors and Air Elemental show no figure (no translucency or palette swap is involved). A picture no spell names
stays in `art/<lbx>/`. `f4x-extract art` writes this layout from the start. For art already extracted,
`f4x-extract refile [art-dir]` moves the files to where the current rules put them (screens, units, wizards
and spells alike; it touches only pictures whose place changed, takes their `_cycle` and `_banner`
masks along and rewrites `index.tsv`; it also files pictures a script left in `art/<lbx>/` that are not in
`manifest.tsv`, such as the new-colour gems) without reading the game files again. `manifest.tsv` stays
`lbx entry frames width height`; `index.tsv` is where the paths are. The client reads the table
through `Art::spell_picture(spell, role)` and `Art::shared_picture(role)`.
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
- **A screen's own colours for the pictures drawn over it:** the screen's background loads its
  colours (`Load_Palette_From_Animation`) and the buttons and other pictures drawn on it, which
  carry none, use them. Without them the colours from index 224 on come out as glass tints and
  speckle the picture (blue and white dots on the start menu's castle, a noisy Alchemy button).
  `screen_palette_entry` in `art.rs` names the entry that gives them, for `magic`, `load`,
  `mainscrn` and `armylist`; the other pictures of those files take it. Files with several
  screens or several palettes (`hire`, `reload`, `scroll`, `spellscr`, `unitview`, `wizlab`)
  are not covered yet: add them when a screen shows the speckles.
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
  that need another look may need their own copy. The learn-spell animation (`learn_spell.rs`) draws it with the base palette and looks right.
- **Palette cycling (task 146):** the original animates by changing one palette entry over
  time, so every pixel that uses that index changes colour at once, with no redrawing.
  `Cycle_Palette_Color(index, min r,g,b, max r,g,b, step)` (`Fonts.c`) moves the entry back and
  forth between two colours (6-bit values), one step per call. ReMoM only ever cycles index 198:
  from dark to bright red (40..63) on the main screen and menu (`LoadScr.c` starts it only when it returns to the main screen), from black to red
  (0..55) in combat, and between pale greys or blues in the spell book. `f4x-extract art` writes the
  pixels that use index 198 as a white `<picture>_<frame>_cycle.png` mask beside the picture (only
  where there are any: the intro, the wizard portraits, the menu and load screens, the end
  animations). The client (`cycle.rs`, `draw_pic_tinted`) draws the mask over the picture in the
  colour of the moment, for the files whose screen cycles (`cycle::for_file`: the menu); the other screens keep the base colour, as in the original, which only cycles
  while the cycling screens run. Text uses the same colour (a negative income on the main screen).
  The combat range is not wired up: no combat picture uses the index. The spell book ranges are
  (`cycle::SMALL_BOOK`, `BIG_BOOK`): the book's pictures hold no index 198 pixels; it is the text of the
  spell being cast or researched that is drawn in it, so the client tints that text.
- **The node box's colour slide (task 357):** `Lair_Confirm` slides palette entries 247 to 254 (`VGA_SlideColors`) while a node's question is up. For `reload.lbx` entries 9 to 19 `f4x-extract art` writes eight overlays, `<name>_<frame>_slide_<step>.png`, holding only those pixels in the colours of that step; the client (`site_prompt.rs`) draws one over the picture, step by time.
- **The summon scene's realm colours (task 439):** `Summon_Animation_Load` loads a palette (colours 224 to 255) from `SPELLSCR` entries 61 to 66 before the scene is drawn: grey (arcane creature, hero, champion, item), then nature, sorcery, chaos, life, death (46 plus the creature's race type; the entries 46 to 59 for the 14 races carry no palette, 60 holds colours 244 to 254 only, and no summon uses either). The frame (9), the two flames (10, 11; 99 percent of their pixels use these colours) and the caster figures (46 to 59; 65 pixels each) use them; the creature's figure (`MONSTER.LBX`) uses none. For those pictures `f4x-extract art` writes six more files per frame, `<name>_<frame>_summon_<61..66>.png`, with that palette; the client (`summon_scene.rs`) draws the one of the creature's realm. `MONSTER.LBX` entry 4, the Fire Elemental, is a flat block with the words "fire elemental" in the original too, so the scene draws our own placeholder picture instead (`crates/f4x-client/assets/summoning/fire_elemental.png`, 47x80, `own_figure` in `summon_scene.rs`).
- **Glass (translucent) pixels:** in a picture that brings no palette of its own, the indexes
  from 232 on are not colours but tints: each blends a colour into what is under it
  (`Create_Remap_Palette`; the 24 blocks, six-bit red, green, blue and a percent, are in the base
  palette's entry, 5376 bytes in; block 0 is the shadow, black at 50 percent). `f4x-extract art` writes
  them as translucent pixels (tree and figure shadows, the glow of spell pictures). A picture with
  an embedded palette (the battle panel, the Info window) uses those indexes for colours.
- **Also not reproduced:** fades, and the colour remapping for disabled or "ghost" drawing.

**If a picture looks wrong:** (1) find where ReMoM loads it (`grep -rn "NAME" ~/projects/ReMoM/MoM/src`);
(2) look for a `Load_Palette(n)` or `Load_Palette_From_Animation(...)` shortly before it;
(3) if it is a numbered palette, add the file to `palette_entry_for` with entry `n + 2`;
(4) re-run the extractor and add a line to the unit test in `art.rs`.

## 5. Fonts

`f4x-extract art` decodes `FONTS.LBX` entry 0 into **eight font styles** of 96 glyphs (ASCII 32 to
127), `fonts/style0.png` to `style7.png`, and `fonts/metrics.tsv`.

- Glyph data is stored by columns and uses **colour indexes 1 to 15**, not palette indexes. A
  glyph's colour index maps to a palette index through one of **16 colour sets** (entry 2 at offset
  `0x300`, 16 bytes a set). So each atlas stacks the glyph strip 16 times, once per set; picking
  a set picks the text colour.
- `metrics.tsv`: `S <style> <height> <hspacing> <vspacing>` per style, and
  `G <style> <ascii> <x in the strip> <width>` per glyph. Heights are 6, 6, 9, 9, 10, 16, 5 and 10
  pixels for styles 0 to 7.
- `fonts/bmfont/style0.fnt` to `style7.fnt` with `.png` are the same styles as AngelCode text BMFonts (for
  `macroquad_bitmap_font_engine`): 96 glyphs on a grid with a one pixel gap, `lineHeight` =
  height + vertical spacing, `xadvance` = width + horizontal spacing. The `.fnt` describes the first grid and the PNG
  stacks it 16 times, once per colour set (the engine's `Ink::Set`, `Nearest` and `Tint` pick one). Styles 0 to 5 also get the letters of other European languages, drawn by the extractor from the style's own ASCII
  letters (`accent.rs`; the rune styles 6 and 7 get none): accented letters with an acute, grave, circumflex, caron,
  breve, tilde, diaeresis, ring, dot, macron, double acute, cedilla or ogonek (a mark above leaves a clear row; a
  letter with no room grows upward and is drawn above the line, `yoffset` < 0; a mark below grows downward; the
  2-pixel marks of the 16 px style are 2 pixels per pixel), the tick of Czech and Slovak ď ť ľ, the strokes of
  ł Ł ø Ø đ Đ Ð, the ligatures æ œ Æ Œ, the dotless ı, ¿ ¡, « », typographic quotes „ “ ” ‚ ‘ ’, the dashes – —
  and the ellipsis …, the fraction ½ (`scripts/font_add_half.py`, a rough raster to fix by hand), and the eszett ß: a straight stem on the left, an upper half-circle and a slightly larger lower
  one, the lower bowl's bar left unjoined to the stem at the bottom. That covers French, Spanish, Portuguese,
  Italian, German, Dutch, Danish, Norwegian, Swedish, Finnish, Czech, Slovak, Polish, Hungarian, Romanian, Croatian
  and Turkish; not drawn: Icelandic þ ð, the capital ẞ, ĳ and the Greek and Cyrillic scripts (see the free fonts in
  `docs/fonts.md` for those). The extractor made the accented glyphs; they were then fixed by hand in the committed fonts, so the extractor's version is only the starting point. Check a new language by eye
  (draw its letters from the `.fnt` and `.png` with Pillow, enlarged 4 times). They are original art, so they stay local like
  the rest of `assets-local`.
- `fonts/colour_sets.ron` is the colour set table itself (set, colour index 1 to 15, RGB). It is only numbers, so a
  copy is committed as `crates/f4x-client/content/font_colour_sets.ron`; after `f4x-extract art` copy it over (`cp
  assets-local/art/fonts/colour_sets.ron crates/f4x-client/content/font_colour_sets.ron`). A test checks its shape.
- In the client (`art.rs`, `frame.rs`): `Art::text_width`, `Art::nearest_font_set(style, colour)`
  (the set whose average colour is closest to the colour wanted) and `Art::draw_bitmap_text`;
  the one function to use on a screen is `App::draw_styled(view, &Style, text, x, y)` (`font.rs`),
  where `view` is `(origin x, origin y, scale)` of the 320 by 200 layout and `x, y` are in the
  original's pixels. A `Style` is built with `Style::nearest(font, colour)`, `Style::set(font, n)`,
  `Style::flat(font, colour)` or `Style::outlined(font)` (the one-pixel black outline the original
  draws on gold and mana figures), then `.shadow()`, `.center()` or `.right()`. Without the fonts
  `Style::nearest` falls back to the plain font.
- The original picks a style with `Set_Font_Style(n)` (also `Set_Font_Style_Outline`, `_Shadow_Down`
  and the other variants, which add an outline or shadow) and colours with `Set_Font_Colors_15`;
  the style number in ReMoM is the style number here. Which style a label uses is read from the
  screen's draw function in ReMoM. The client mostly uses styles 1 and 3, and 5 for titles; task
  118 moves the rest over.
- **Colour ramps** (`scripts/font_ramps.py`, `crates/f4x-client/content/font_colour_ramps.ron`, `crates/f4x-client/src/font_ramp.rs`):
  the original builds most text colours at run time with `Set_Font_Colors_15(style, colours)`, not from the 16
  sets. The script lists each such ramp (`RAMPS`: a name, the screen whose palette applies, the palette
  indexes of colour 1 to n; entry 0 of the original's array is an alias and ignored) and the single palette
  colours (`FLATS`, `font_ramp::flat(index)`). The ramps become sets 16
  and up in the atlases. **The finished fonts are the committed `crates/f4x-client/assets/fonts/style*.fnt` and `.png`**:
  they were edited by hand after the one-time extraction (accents and other fixes), are compiled into the client, and
  are the only copy that ships. The extractor output (`fonts/bmfont/` in `assets-local`) is dev-side only and is not
  copied into the package; never copy it over the committed fonts. A new ramp needs its colour set added to the
  committed atlases: put it in `LATE` of the script (after the flats, so the sets keep their places) and run `scripts/font_ramp_sets.py`, which appends the sets to the atlases of styles 0 to 5. Use one with `Style::inked(font, Ink::Set(font_ramp::NAME))`, and `.effect(Effect::Shadow |
  Outline | OutlineHeavy)` with `.edge(colour)` for `Set_Font_Style_Shadow_Down / Outline / Outline_Heavy`
  and `Set_Outline_Color`. Outline colours from palette index 224 up belong to a screen's own palette: read
  them from that screen's palette in ReMoM, or approximate and say so in a comment. The atlas must stay under
  16384 pixels (`docs/KNOWLEDGE.md`).
- Not done: `Print_Paragraph` word wrap and justification (task 118), and the outline, shadow and
  colour-table variants of the font calls (only `Style::outlined` and `.shadow()`).

## 6. Banner colours

Player colour is painted in at run time by replacing five fixed palette indexes with the banner's
five shades; the extractor does this ahead of time or leaves a mask:

- **Cities and towers on the map** (`MAPBACK.LBX`): `map/locations/<name>_<banner>_<frame>.png` for the six
  banners in the game's order (blue, green, purple, red, yellow, neutral/brown), made with
  `COL_City_Banner`. Indexes 214 to 218 (and 224 to 228 on towers) are swapped for the banner's
  shades, as `Draw_Map_Cities` does. `map/banners.tsv` has each banner's five shades as r,g,b. The neutral banner is the original's brown; this game plays it grey: run `scripts/grey_neutral.py` after extracting.
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

1. **Find it.** `grep -rn "FILENAME" ~/projects/ReMoM/MoM/src/*.c`, or in `LOADER.c` for pictures
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
   `art: Some(("units1", 5))` in `crates/f4x-def/content/units.ron` (file, entry); `units1` entry `n` is unit type
   `n` and `units2` entry `n` is type `120 + n` (`unit_type_number`). Nothing else in `crates/f4x-def/content/`
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
| Wizard pictures | `wizards` (the picker, 109x104), `lilwiz` (41x43 gems), `moodwiz` (diplomacy mood), `diplomac` 24 + wizard (talking), `wizlab`, `halofam`, `spellscr` 46 + wizard (the caster's figure in the summon scene), `win`, `lose`, `spellose`, `conquest`, `splmastr` | all by `crates/f4x-client/content/wizard_art.ron`, in one folder for each wizard: `art/wizards/<id>/<role>` (`portrait`, `mirror`, `talk`, `lab`, `gem_0` to `gem_9`, `hall`, `summon`, `win`, `lose`, `sphere`, `sphere_figure`, `stand`, `pieces`, `seize`, `parade`, `parade_rival`). Gems 5 to 9 are the colours of `make_banner_colours.py` (`lilwiz` entries 100 and up). The hands of `win` are shared by several wizards and stay with the win screen |
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

- **Sound and music** are not pictures: `f4x-extract audio` writes them to `assets-local/audio` (task 362,
  `local-assets.md`, `sound.md`).
- **Data tables (names and numbers), once read to make the rules in `crates/f4x-def/content/`, now kept
  by hand:** `SPELLDAT`, `BUILDDAT`, `ITEMDATA`, `TERRSTAT`. `CITYNAME`, `EVENTMSG`, `DIPLOMSG` and
  `BUILDESC` are prose (IP): `scripts/import_mom_text.py` writes them to `content-local/` for
  comparison, and the game does not read them.
- **Data and text not read yet:** `HERODATA`, `ITEMPOW`, `LISTDAT`, `DESC`, `BUILDESC`,
  `DIPLOMSG`, `NAMES`, `CONFIG.MOM`.
- **Videos:** the opening and ending are frame sequences (`intro` has 13 scenes of full-screen
  pictures) with their own timing in `INTRO.c` and `CONQUEST.c`. The opening is played by
  `intro_screen.rs` from the extracted pictures (`assets-local/screens/intro/`) and the `SOUNDFX`
  entries, with the original's timing (55 ms ticks, three a frame, the fades); the ending is not
  wired (task 134). `f4x-client --shot FILE --intro SCENE` shows one scene.

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
- Unused effect pictures: `cmbtfx` 7 (Mind Twist), 12 (Petrify) and 32 (Zombie) are loaded by no code in the original, so no spell shows them. `cmbtfx` 27 to 31 belong to the undead rising scene (task 448) and 28 to the combat message panel.

## Pictures the game no longer draws (`assets-unused/`)

The pictures with lettering baked in have text-free copies in `crates/f4x-client/assets/` and the
client draws their labels, so the originals are no longer read. `crates/f4x-client/extract/unused.tsv`
lists them (lbx, first and last entry, the name); `f4x-extract art` and `refile` write them to the same
place under `assets-unused/` beside `assets-local/` (git-ignored like it), and `index.tsv` says so
(`../assets-unused/screens/...`; the client would still find one it asks for). They stay for
reference and as the sources of `scripts/button_text_free.py`, which looks there when a picture is no
longer in `assets-local/`. `scripts/unused_art.py` lists candidates from a trace of the pictures the
client reads (`F4X_TRACE_ART=FILE`). `make_app.sh` copies `assets-local` only, so the app leaves them out.
