# Local-only original assets and data

Nothing derived from the original game may be committed or shared (the repo is CC0).
Both outputs below are gitignored and the game runs without them (flat colours, the
sample `content/` set).

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
Format code: `crates/f4x-tools/src/{lbx,image,art}.rs`.

## Data: `content-local/`

```
python3 -I scripts/import_mom.py [--game DIR] [--remom DIR]
cargo run -p f4x-tools -- validate content-local
```

Builds races (with city names), units, buildings, abilities, spells, items, realms,
level paths and the original help (`help.ron`: 807 topics; `help_screens.ron`: which
screen areas open which topic) from the original `.lbx` files and the unit/race tables of a local
ReMoM clone. Everything it cannot map goes to `content-local/IMPORT_REPORT.txt`.
Spells without a matching effect primitive use the `unimplemented` custom hook; the
count of those is the to-do list for new primitives. Spell rarity is inferred from
table order and some upkeep values are estimated (see the report).

The client prefers `content-local/` when it exists, else `content/`.

The importer also cross-checks costs, upkeep and race build bans against the help text
(`[check]` lines in the report). What the help describes that is not implemented yet is in
`docs/mechanics-gaps.md`.
