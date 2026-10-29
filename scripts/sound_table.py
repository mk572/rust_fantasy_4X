#!/usr/bin/env python3
"""Writes `content/sound.ron`: which of the original's sounds each spell and unit plays.

Reads the spell table (`SPELLDAT.LBX`, byte 30 of a record: the cast sound, positive is a `SOUNDFX.LBX`
entry and negative a `NEWSOUND.LBX` entry, and byte 32: the music of a global enchantment) from the
author's own copy of the game, and the units' sound bytes (high nibble the move sound, low nibble the
melee sound) from the ReMoM clone (`NEXTTURN.c`, which sets them at start-up). Ids come from the
committed `content/spells.ron` and `content/units.ron`. What plays when is `docs/sound.md`.

Usage:
    python3 -I scripts/sound_table.py [--game DIR] [--remom DIR] [--out FILE]
"""

import argparse
import re
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_GAME = REPO / "reference/mom-classic/Master of Magic Official Release"
DEFAULT_REMOM = Path.home() / "projects/ReMoM"

# Global enchantments whose music is entry 19 + the spell row's byte 32 (`WIZ_GlobalSpellAnim`).
GLOBAL_SPELLS = [
    "eternal_night", "evil_omens", "zombie_mastery", "aura_of_majesty", "wind_mastery", "suppress_magic",
    "time_stop", "nature_awareness", "herb_mastery", "chaos_surge", "doom_mastery",
    "great_wasting", "meteor_storm", "armageddon", "tranquility", "life_force", "crusade", "just_cause",
    "holy_arms",
]
# Spells the original gives another spell's piece.
BORROWED = {"death_wish": 23, "great_unsummoning": 24}

HEADER = """\
// Which of the original's sounds each spell and unit plays (`docs/sound.md`, section 4). Written by
// `scripts/sound_table.py`; the numbers are entries of the original's sound files, which the
// client finds under `assets-local/audio` (`f4x-tools extract-audio`). Without the files the game
// is silent.
//   spells:       the cast sound of a spell: positive is a SOUNDFX entry, negative a NEWSOUND entry.
//   units:        the move sound (a SOUNDFX entry; 0 silent, 13 is NEWSOUND 0) and the melee sound
//                 (a CMBTSND entry; 8 silent) of a unit.
//   global_music: the MUSIC entry of a global enchantment's scene.
"""


def read_lbx(path):
    data = path.read_bytes()
    count, magic = struct.unpack_from("<HH", data, 0)
    if magic != 0xFEAD:
        raise ValueError(f"not an LBX file: {path}")
    offsets = struct.unpack_from(f"<{count + 1}I", data, 8)
    return [data[offsets[i]:offsets[i + 1]] for i in range(count)]


def ids_of(path):
    return re.findall(r'\(id: "([^"]+)"', path.read_text(encoding="utf-8"))


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0], formatter_class=argparse.RawDescriptionHelpFormatter, epilog=__doc__.split("\n", 2)[2])
    ap.add_argument("--game", type=Path, default=DEFAULT_GAME, help="the original game folder")
    ap.add_argument("--remom", type=Path, default=DEFAULT_REMOM, help="the ReMoM clone")
    ap.add_argument("--out", type=Path, default=REPO / "content/sound.ron", help="file to write")
    args = ap.parse_args(argv)

    spell_ids = ids_of(REPO / "content/spells.ron")
    unit_ids = ids_of(REPO / "content/units.ron")

    entry = read_lbx(args.game / "SPELLDAT.LBX")[0]
    count, size = struct.unpack_from("<HH", entry, 0)
    rows = [entry[4 + i * size:4 + (i + 1) * size] for i in range(count)]
    if count != len(spell_ids) + 1:  # row 0 is "None"
        sys.exit(f"SPELLDAT has {count} rows, spells.ron {len(spell_ids)} spells")
    spells, music = {}, {}
    for i, row in enumerate(rows[1:]):
        sound = struct.unpack_from("<b", row, 30)[0]
        if sound:
            spells[spell_ids[i]] = sound
        if spell_ids[i] in GLOBAL_SPELLS:
            music[spell_ids[i]] = 19 + row[32]
    for spell, entry_no in BORROWED.items():
        if spell not in spell_ids:
            sys.exit(f"{spell}: not in spells.ron")
        music[spell] = entry_no
    missing = [s for s in GLOBAL_SPELLS if s not in music]
    if missing:
        sys.exit(f"not in spells.ron: {missing}")

    text = (args.remom / "MoM/src/NEXTTURN.c").read_text(encoding="latin1")
    units = {}
    for off, name, value in re.findall(
        r"_unit_type_table\[(?:\(0x([0-9A-Fa-f]+) / sizeof\(struct s_UNIT_TYPE\)\)|(ut_\w+))\]\.Sound = (\d+);", text
    ):
        index = int(off, 16) // 36 if off else {"ut_Dwarf": 0, "ut_Nagas": len(unit_ids) - 1}[name]
        units[unit_ids[index]] = (int(value) >> 4, int(value) & 15)
    if len(units) != len(unit_ids):
        sys.exit(f"{len(units)} unit sounds for {len(unit_ids)} units")

    out = [HEADER, "(\n    spells: {\n"]
    out += [f'        "{k}": {v},\n' for k, v in spells.items()]
    out += ["    },\n    units: {\n"]
    out += [f'        "{k}": (move: {m}, melee: {ml}),\n' for k, (m, ml) in units.items()]
    out += ["    },\n    global_music: {\n"]
    out += [f'        "{k}": {v},\n' for k, v in music.items()]
    out += ["    },\n)\n"]
    args.out.write_text("".join(out), encoding="utf-8")
    print(f"{args.out}: {len(spells)} spells, {len(units)} units, {len(music)} global spells")


if __name__ == "__main__":
    main(sys.argv[1:])
