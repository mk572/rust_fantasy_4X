# Local-only original assets and data

Nothing protected by IP may be committed or shared (the repo is CC0): the original's art, music
and sound, help text and other prose, and code. The game's rules data (names, costs, stats,
effects and numbers) is not in that class and is committed in `content/`. The two outputs below
are the local-only ones; both are gitignored and the game runs without them (flat colours, and the help of `content/help.ron` only).

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
index 0 transparent; the ones that belong to a single screen go to `assets-local/screens/<screen>/`
unit art to `assets-local/art/units/<unit id>/` and a spell's pictures to `assets-local/art/spells/<spell id>/`; see `extract-art.md` section 3) plus `manifest.tsv`, and the terrain tiles (`map/tiles/<plane>/<kind>/tile_NNNN.png`,
`map/tiles/types.tsv` mapping terrain type -> tile, `map/tiles/sheet.png` overview).
Format code: `crates/f4x-tools/src/{lbx,image,art}.rs`. Palettes, fonts, banner recolouring and where each file is used: [`extract-art.md`](extract-art.md).

## Sound and music: `assets-local/audio/`

```
cargo run --release -p f4x-tools -- extract-audio "reference/mom-classic/Master of Magic Official Release"
```

Needs `oggenc` (`brew install vorbis-tools`; `ffmpeg` is the fallback). Writes the 154 effects of `SOUNDFX`, `CMBTSND`, `NEWSOUND` and `INTROSFX` as OGG
(`<archive>/<entry>_<name>.ogg`), the 118 songs of `MUSIC` and `INTROSND` unchanged as XMI (`music/NNN_name.xmi`; they
are not yet audio, task 363), the MT-32 timbre banks in `timbres/`, and `index.tsv`. What plays when: [`sound.md`](sound.md).
Format code: `crates/f4x-tools/src/audio.rs`.
The client plays them (`crates/f4x-client/src/audio.rs`, macroquad's audio): `--audio DIR` names another folder, `--mute`
plays nothing, `--hear soundfx:2` plays one sound and quits (a check that a file decodes). Without the folder the
game is silent. Which entry a spell or unit plays is `content/sound.ron` (`scripts/sound_table.py`).

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
areas open which topic), the preset wizards' names and descriptions (`wizard_texts.ron`, built from help topics; the random events' messages in `event_texts.ron`, from `EVENTMSG.LBX`; the building descriptions of `BUILDESC.LBX` in `building_texts.ron`, which the game does not show (it has its own in `content/text/en/building.ron`) but the importer compares with our building effects, each difference a `[check]` line in the report; their books and retorts are in the committed `content/wizards.ron`) and
`IMPORT_REPORT.txt` (everything it could not map) to `content-local/`. Old runs also wrote the
rules files there; the importer now removes those stale copies.

The client and `f4x-tools` read `content-local/` first and `content/` for every file it lacks
(`f4x_data::load_content_layers`), so the game runs without any local files, only without help
or preset wizards. The importer also cross-checks costs, upkeep and race build bans against the
help text (`[check]` lines in the report). What the help describes that is not implemented yet
is in `docs/mechanics-gaps.md`.

The data loader's tests read `content/` alone (never `content-local/`); there is no separate
test rule set, so a test cannot pass on data the game no longer has.

## Local Mac app

`scripts/make_app.sh [--open]` builds `target/Fantasy4X.app`: the release client plus copies of
`content/`, `content-local/` and `assets-local/art` and `screens`. A launcher script links them into
`~/Library/Application Support/Fantasy4X/` and runs the game from there, so saves land in
`~/Library/Application Support/Fantasy4X/saves/`. It contains the local-only original art and
text: for play on this machine, not for sharing. No icon, signing or universal binary yet (task 81).

## Exception: the battlefield river pictures

The six river pictures of a battlefield (`cmbtcity` 103 to 108, five frames each) are committed as
`crates/f4x-client/assets/rivers/river_<shape>_<frame>.png` and embedded in the client
(`river_art.rs`), so rivers show without the local art. Myrror's set
(`river_myrror_<shape>_<frame>.png`) was generated once from Arcanus's, by repainting the water
in the colour of Myrror's ocean and the banks in that of its ground, and kept as files. They are the original's art, kept in the
repository at the project owner's choice (17 October 2026); everything else stays local-only.
