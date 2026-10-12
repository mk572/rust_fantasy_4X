# Local-only original assets and data

Nothing protected by IP may be committed or shared (the repo is CC0): the original's art, music
and sound, help text and other prose, and code. The game's rules data (names, costs, stats,
effects and numbers) is not in that class and is committed in `content/`. The two outputs below
are the local-only ones; both are gitignored and the game runs without them (flat colours, no help).

## Where the originals live

`reference/` in the project directory (gitignored) holds the inputs:
- `reference/mom-classic/` the author's own copy of the original game (Steam "Master of Magic
  Classic", app 1146370, downloaded with steamcmd); the files are in
  `reference/mom-classic/Master of Magic Official Release/`;
- `reference/ReMoM/` a single-commit clone of jbalcomb/ReMoM (a reverse-engineering of the
  original code), used for the unit and race tables and as a behaviour reference. A private
  full-history mirror is on GitHub as `ReMoM-reference`.

The scripts default to these paths. Nothing in `reference/`, `assets-local/` or
`content-local/` is committed.

## Art: `assets-local/art/`

```
cargo run --release -p f4x-tools -- extract-art "reference/mom-classic/Master of Magic Official Release"
```

Decodes every picture in the original `.lbx` files to PNG (`<lbx>/<entry>_<frame>.png`,
index 0 transparent) plus `manifest.tsv`, and the terrain tiles (`terrain/tile_NNNN.png`,
`terrain/types.tsv` mapping terrain type -> tile, `terrain/sheet.png` overview).
Format code: `crates/f4x-tools/src/{lbx,image,art}.rs`. Palettes, fonts, banner recolouring and where each file is used: [`extract-art.md`](extract-art.md).

## Rules data: `content/` (committed)

```
python3 -I scripts/import_mom.py [--game DIR] [--remom DIR]
cargo run -p f4x-tools -- validate content
```

Writes the game's rules to `content/`: races, units, buildings, abilities,
spells, items, realms, level paths and `rules.ron`, from the original `.lbx` files and the
unit/race tables of a local ReMoM clone. These are names and numbers, not IP-protected prose,
so they are committed (`terrain.ron` and `features.ron` there are hand-written). Rerun the
importer to regenerate them; edit the importer, not the files, for anything it produces.
Spells without a matching effect primitive use the `unimplemented` custom hook; the count of
those is the to-do list for new primitives. Spell rarity is inferred from table order and some
upkeep values are estimated (see the report).

## Help text: `content-local/` (gitignored)

The city names are IP too: `content/city_names.ron` holds invented names in each race's style (20 a race), and the same run writes the original's to `content-local/city_names.ron`, which replaces them per race when present. The run also writes the original help (`help.ron`: 807 topics; `help_screens.ron`: which screen
areas open which topic), the preset wizards (`wizards.ron`, built from help topics) and
`IMPORT_REPORT.txt` (everything it could not map) to `content-local/`. Old runs also wrote the
rules files there; the importer now removes those stale copies.

The client and `f4x-tools` read `content-local/` first and `content/` for every file it lacks
(`f4x_data::load_content_layers`), so the game runs without any local files, only without help
or preset wizards. The importer also cross-checks costs, upkeep and race build bans against the
help text (`[check]` lines in the report). What the help describes that is not implemented yet
is in `docs/mechanics-gaps.md`.

The small invented sample rule set (sunfolk, ironclans, spearman, fireball) that `content/` used
to hold is now `crates/f4x-data/testdata/`, the fixture for the data loader's tests.

## Local Mac app

`scripts/make_app.sh [--open]` builds `target/Fantasy4X.app`: the release client plus copies of
`content/`, `content-local/` and `assets-local/art`. A launcher script links them into
`~/Library/Application Support/Fantasy4X/` and runs the game from there, so saves land in
`~/Library/Application Support/Fantasy4X/saves/`. It contains the local-only original art and
text: for play on this machine, not for sharing. No icon, signing or universal binary yet (task 81).
