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
