# Fonts: tools and candidates

The original's bitmap fonts (`FONTS.LBX`, `docs/extract-art.md` section 5) hold ASCII 32 to 127 only. For other
languages, or to ship without the original's art, the game needs other fonts. Candidates with open licences are in
`reference/fonts/` (local, see its `README.md` and `preview.png`).

## Installing the fonts

`python3 scripts/install_fonts.py` downloads the fonts of `scripts/fonts.json` into `reference/fonts/` (git-ignored;
`--list` shows them, `--only ID...` picks some). Each download has a pinned sha256, so a changed or tampered file
stops the script, and a font whose licence is not in the manifest's `allowed_licences` (OFL, CC0, public domain, MIT,
BSD, Apache) is refused: only fonts with no restrictions on use in the project belong there. To add one, add an entry
(id, name, licence as an SPDX id, role, downloads: `url` and `dest`, or an archive `url` with `extract` members)
and run `--pin` to fill in the hashes. Include the font's licence text as a download. Python standard library only.

## Tools (`f4x-tools`, `crates/f4x-tools/src/font_tool.rs`)

| Command | What it does |
|---|---|
| `font-chars [text-dir]` | prints the distinct characters of every `.ron` file under `content/text` (all languages) on one line; redirect it to a file for `--chars` |
| `font-coverage <font> [--chars FILE]` | lists the wanted characters the font has no glyph for. Wanted: Basic Latin, Latin-1 and Latin Extended-A, plus the characters of FILE |
| `font-bmfont <font> [<px>] [--out DIR] [--name NAME] [--chars FILE] [--smooth]` | rasterises the font at `<px>` into `DIR/NAME.fnt` and `.png` (default `assets-local/fonts`, name from the file and size): an AngelCode text BMFont with white glyphs, kerning pairs rounded to whole pixels, `lineHeight` and `base` from the font's line metrics. Without `--smooth` every pixel is on or off, which keeps pixel fonts crisp. A `.bdf` bitmap font (`bdf.rs`: ascent, descent, per glyph code point, size, offset, advance and bitmap) keeps its own pixels and size: leave `<px>` out (a different size is an error), and it has no kerning |

Example: check a font against the game's text, then make the BMFont:

    cargo run --release -p f4x-tools -- font-chars > /tmp/chars.txt
    cargo run --release -p f4x-tools -- font-coverage reference/fonts/jersey10/Jersey10-Regular.ttf --chars /tmp/chars.txt
    cargo run --release -p f4x-tools -- font-bmfont reference/fonts/jersey10/Jersey10-Regular.ttf 10

- `font-coverage` and `font-bmfont` take TrueType, OpenType and BDF files; e.g. `font-bmfont reference/fonts/spleen/spleen-8x16.bdf`
  writes `spleen_8x16_16.fnt` and `.png`.
- Use a pixel font at its design size (or a whole multiple): at any other size the glyphs blur or break.
- The BMFont has one colour set (white). Tint it with the engine's `Ink::Tint`; a font with the original's 16 sets
  only comes from `extract-art`.
- The tool rasterises with `fontdue` (already in the tree through macroquad); a variable font is drawn at its
  default instance.
- A font's licence goes with the generated files: copy its `OFL.txt` (or equivalent) next to them when they are
  committed.

## How close are the free fonts? (`scripts/font_match.py`, 31 October 2026)

The original's eight styles are: 0 to 3 small and medium sans pixel fonts (6, 6, 9 and 9 px; 3 is the bold one), 4 a
10 px blackletter, 5 a 16 px blackletter (the title style), 6 and 7 rune-like symbol fonts (not letters), so only
styles 0 to 5 can have a stand-in. The script scores each installed font against each style: the overlap of the
glyph pixels (intersection over union, mean over ASCII 33 to 126, after the best shift of the whole font; 1.0 is
identical). Best results, each with its size:

| Style | Best match | Score |
|---|---|---|
| 0 (6 px) | Micro 5 at 10 or 11 px, Jersey 10 at 8 px | 0.62, 0.60 |
| 1 (6 px) | Silkscreen at 8 px, Jersey 20 at 9 px | 0.57, 0.56 |
| 2 (9 px) | Jersey 20 at 10 px, DotGothic16 at 9 px | 0.56, 0.51 |
| 3 (9 px, bold) | Jersey 20 at 11 px, Jersey 15 at 12 px | 0.57, 0.55 |
| 4 (10 px blackletter) | Jersey 10 at 14 px, Jacquard 24 at 14 px | 0.39, 0.32 |
| 5 (16 px blackletter) | Jersey 25 at 17 px, Jacquard 24 at 21 px | 0.31, 0.30 |

**No free font is a pixel match.** The best score is 0.62, and the blackletter styles are at 0.3 to 0.4 (the
scores of Jacquard and the Bitcount family are in the same range). The original's fonts are drawn glyph by glyph,
and a different design differs in a pixel or two on most letters, which an overlap score punishes. Jersey 20 is the
closest in spirit for body text (styles 2 and 3). For the blackletter styles Jacquard 12 or 24 look right in a
preview but not in pixels. A real match would be a font drawn to the original's glyph shapes, which is a derivative
of the original's art: not something to commit.

Candidates that were looked at and left out (licence; install policy in the section above):

- Cloister Black (dafont "100% Free"): the font file reads "Copyright (c) Typographer Mediengestaltung, 2001. All
  rights reserved. Full character set added by Dieter Steffmann", with no licence text in the download, so the use
  is not clearly unrestricted. Not installed.
- Pixel Symtext: freeware; the listing says to contact the designer for commercial use. Not installed.
- Gridwright Story and tallreed: no page or licence found. Send a link and I will check them.
- Bitcount (all six non-ink families): installed, OFL; scores below Jersey for every style.

## Trying the engine and the free fonts on a screen

`F4X_BMFONT=1` makes `App::draw_styled` (every screen's bitmap text) draw through `macroquad_bitmap_font_engine` from
`assets-local/art/fonts/bmfont`; `F4X_BMFONT=DIR` takes the `style{N}.fnt` and `.png` files from DIR instead (a style
missing there falls back to the client's own drawing). A font with one colour set (any font made by `font-bmfont`)
is tinted: the colour asked for, or for `Style::set(n)` the average colour of the original's set n. The city screen
(`--shot FILE --city --shot-after 1`) with `F4X_BMFONT=1` is pixel-identical to the client's own drawing (31 October
2026, 2560 by 1600, zero differing pixels), so the engine reproduces the extracted fonts exactly. With free fonts
(Micro 5 for style 0, Silkscreen 8 for 1, Jersey 20 at 10 and 11 for 2 and 3, Jacquard 12 for 4, Jacquard 24 at 21 for
5) the title in Jacquard looks right; the narrow boxes overflow or clip because the free fonts' widths differ (Silkscreen
is capitals only and wide), so the layout would need the widths measured and the text fitted, as the localization task
plans.
