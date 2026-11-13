# Local-only original assets and data

Nothing protected by IP may be committed or shared (the repo is CC0): the original's art, music
and sound, help text and other prose, and code. The game's rules data (names, costs, stats,
effects and numbers) is not in that class and is committed in `crates/f4x-def/content/`. The two outputs below
are the local-only ones; both are gitignored and the game runs without them (flat colours, and the help of `crates/f4x-lang/content/text/en/help/help.ron` only).

## Where the originals live

`reference/` in the project directory (gitignored) holds the inputs:
- `reference/mom-classic/` the author's own copy of the original game (Steam "Master of Magic
  Classic", app 1146370, downloaded with steamcmd); the files are in
  `reference/mom-classic/Master of Magic Official Release/`;

The ReMoM clone is outside the project, in `~/projects/ReMoM` (a clone of jbalcomb/ReMoM, a
reverse-engineering of the original code), used for the unit, race and spell tables and as a
behaviour reference. A private full-history mirror is on GitHub as `ReMoM-reference`. All the
reference projects are listed in [`references.md`](references.md).

The scripts default to these paths. Nothing in `reference/`, `assets-local/` or
`content-local/` is committed.

## Art: `assets-local/art/`

```
cargo run --release -p f4x-extract -- art "reference/mom-classic/Master of Magic Official Release"
```

Decodes every picture in the original `.lbx` files to PNG (`<lbx>/<entry>_<frame>.png`,
index 0 transparent; the ones that belong to a single screen go to `assets-local/screens/<screen>/`
unit art to `assets-local/art/units/<unit id>/` and a spell's pictures to `assets-local/art/spells/<spell id>/`; see `extract-art.md` section 3) plus `manifest.tsv`, and the terrain tiles (`map/tiles/<plane>/<kind>/tile_NNNN.png`,
`map/tiles/types.tsv` mapping terrain type -> tile, `map/tiles/sheet.png` overview).
Format code: `crates/f4x-tools/src/{lbx,image,art}.rs`. Palettes, fonts, banner recolouring and where each file is used: [`extract-art.md`](extract-art.md).

### Sprite packs

`f4x-extract art` ends by packing the pictures under `art/` (`--loose` leaves them as PNG files).
A pack is one file for one folder: a table of picture names and rectangles, then one or more PNG
sheets of at most 2048 x 2048 pixels (`crates/f4x-client/extract/src/pack.rs` has the layout). A unit's
66 pictures are one sheet. Where they go:

- a unit folder `art/units/<unit>/` becomes `art/units/<race>/<unit>.pack`, so `art/units/high_elf/high_elf_spearmen.pack`
  holds every picture of the High Elf Spearmen. The race is the unit's race in `units.ron`; a unit without
  one goes to `hero/`, `ship/` (trireme, galley, warship) or `fantastic/` (summoned creatures, the catapult);
- any other folder `art/<folder>/` goes to `art/<folder>.pack` beside it (`art/spells/fire_bolt.pack`,
  `art/map/tiles/arcanus/shore.pack`);
- `packs.tsv` in `assets-local/` lists each folder and its pack. The client reads it at start and opens a
  pack when a picture of its folder is first asked for; a picture no pack has is read from the loose file, so
  a mixed tree works. The names the game asks for do not change.

Not packed: `screens/` (each picture is large already), `art/fonts/`, and `neutral/`, `palette-swap/`, `shots/`
(working files of scripts and the author, never read by the client).

```
f4x-extract pack [assets-local] [--keep]   pack every loose folder (a loose file replaces the same name in its pack)
f4x-extract unpack [assets-local]          every pack back to PNG files, for the scripts below
```

Scripts that make or change pictures (`make_banner_colours.py`, `make_spell_art.py`, `make_banner_flags.py`,
`palette_swap.py`, `grey_neutral.py`) and `f4x-extract refile` work on loose files: `unpack`, run the script,
`pack` again. Every pack is read back and compared pixel for pixel before a loose file is deleted. Measured
on 13 November 2026: `art/` went from 19,414 files (108 MB on disk) to 407 files (22 MB); the packs hold
21,809 pictures in 13 MB. `screens/` is still 3,815 loose files (35 MB).

## Sound and music: `assets-local/audio/`

```
cargo run --release -p f4x-extract -- audio "reference/mom-classic/Master of Magic Official Release"
```

Needs `oggenc` (`brew install vorbis-tools`; `ffmpeg` is the fallback). Writes the 154 effects of `SOUNDFX`, `CMBTSND`, `NEWSOUND` and `INTROSFX` as OGG
(`<archive>/<entry>_<name>.ogg`), the 118 songs of `MUSIC` and `INTROSND` unchanged as XMI (`music/NNN_name.xmi`; they
are not yet audio, task 363), the MT-32 timbre banks in `timbres/`, and `index.tsv`. What plays when: [`sound.md`](sound.md).
Format code: `crates/f4x-tools/src/audio.rs`.
The client plays them (`crates/f4x-client/src/audio.rs`, macroquad's audio): `--audio DIR` names another folder, `--mute`
plays nothing, `--hear soundfx:2` plays one sound and quits (a check that a file decodes). Without the folder the
game is silent. Which entry a spell or unit plays is `crates/f4x-client/content/sound.ron` (`scripts/sound_table.py`).

## Rules data: `crates/f4x-def/content/` (committed)

The game's rules (races, units, buildings, abilities, spells, items, realms, level paths,
`rules.ron`, terrain, features) are hand-kept names and numbers, committed in
`crates/f4x-def/content/`. No script writes them. Check them with:

```
cargo run -p f4x-tools -- validate content
```

The client reads the rules from there and its own tables, texts and settings from
`crates/f4x-client/content/`, and nothing overrides them (modding comes later). The only files read
from outside the repository are the pictures and sounds (`assets-local/art`, `assets-local/audio`,
`--assets` and `--audio`) and the player's own `saves/hotkeys.ron`. `f4x-tools` reads the rules from
`crates/f4x-def/content/` and takes `--dir` or `--content` for another folder.

The data loader's tests read `crates/f4x-def/content/` alone; there is no separate test rule set, so a
test cannot pass on data the game no longer has.

## The original's prose: `content-local/` (gitignored, developer only)

The game reads **no text of the original**: its texts, event sentences, diplomacy lines, city names and
help are its own files in `crates/f4x-lang/content/text/`. For comparing them with the original's,
a developer can write the original's prose to `content-local/`, which nothing reads:

```
python3 -I scripts/import_mom_text.py [--game DIR] [--remom DIR]
```

It writes `text/en/name/city.ron` (`CITYNAME.LBX`), `event_texts.ron` (`EVENTMSG.LBX`), `diplomsg.ron`
(`DIPLOMSG.LBX`), `building_texts.ron` (`BUILDESC.LBX`) and `IMPORT_REPORT.txt` (everything it could
not map). It cross-checks the building descriptions against the effects in `buildings.ron`; each
difference is a `[check]` line in the report. `scripts/import_mom.py` is only the shared reader of the
`.lbx` files (`read_lbx`, `read_records`, `cstr`) that the export scripts import. Never commit
`content-local/`.

## Local Mac app

`scripts/make_app.sh [--open]` builds `target/Fantasy4X.app`: the release client plus copies of
`crates/f4x-def/content/`, `content-local/` and `assets-local/art` and `screens`. A launcher script links them into
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
