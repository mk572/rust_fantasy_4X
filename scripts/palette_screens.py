#!/usr/bin/env python3
"""List the colours the original's pictures load into palette indexes 224-255, per file.

The base palette (palette_table.py) has no colours from 224 on: a screen's pictures carry them
(`Load_Palette_From_Animation`) and FONTS.LBX entries 3 to 8 are whole alternate palettes.
Usage: scripts/palette_screens.py [GAME_DIR] [OUT.tsv]
Default input: the Official Release in reference/mom-classic. Default output: docs/palette_screens.tsv
Columns: source (file stem, or fonts:<entry> for a whole palette), entries (picture entries in
the file that carry exactly this colour set, "all" for a whole palette), index, r6 g6 b6, r g b, hex.
Only indexes 224-255 are listed; a picture's set may start lower.
"""
import struct
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_DIR = ROOT / "reference/mom-classic/Master of Magic Official Release"
FIRST, LAST = 224, 255
FONTS_PALETTES = range(3, 9)  # Load_Palette(1..6): load/save, menu, conquest, mastery, win, lose


def lbx_entries(data: bytes):
    count, magic = struct.unpack_from("<HI", data, 0)
    if magic != 0x0000FEAD:
        return
    offsets = struct.unpack_from(f"<{count + 1}I", data, 8)
    for i in range(count):
        yield i, data[offsets[i] : offsets[i + 1]]


def picture_palette(entry: bytes):
    """(first colour, 6-bit RGB bytes) of a picture entry's own palette, or None."""
    if len(entry) < 18:
        return None
    width, height, _, count, _, _, _, pal_off = struct.unpack_from("<8H", entry, 0)
    if not (1 <= width <= 640 and 1 <= height <= 480 and 1 <= count <= 256) or pal_off == 0:
        return None
    if pal_off + 6 > len(entry):
        return None
    data_off, start, n = struct.unpack_from("<HHH", entry, pal_off)
    if start + n > 256 or data_off + 3 * n > len(entry):
        return None
    return start, entry[data_off : data_off + 3 * n]


def window(start: int, rgb: bytes):
    """The colours of a set that fall in 224-255, as {index: (r6, g6, b6)}."""
    return {
        start + k: tuple(v & 0x3F for v in rgb[3 * k : 3 * k + 3])
        for k in range(len(rgb) // 3)
        if FIRST <= start + k <= LAST
    }


def rows_for(source: str, entries: str, colours: dict):
    for index in sorted(colours):
        r6, g6, b6 = colours[index]
        r, g, b = ((v << 2) | (v >> 4) for v in (r6, g6, b6))
        yield f"{source}\t{entries}\t{index}\t{r6}\t{g6}\t{b6}\t{r}\t{g}\t{b}\t#{r:02X}{g:02X}{b:02X}"


def main() -> int:
    game = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_DIR
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "docs/palette_screens.tsv"
    rows = ["source\tentries\tindex\tr6\tg6\tb6\tr\tg\tb\thex"]
    fonts = next((p for p in game.iterdir() if p.name.upper() == "FONTS.LBX"), None)
    if fonts is None:
        print(f"{game}: no FONTS.LBX", file=sys.stderr)
        return 1
    for number, entry in lbx_entries(fonts.read_bytes()):
        if number in FONTS_PALETTES:
            rows.extend(rows_for(f"fonts:{number}", "all", window(0, entry[:768])))
    for path in sorted(game.iterdir()):
        if path.suffix.upper() != ".LBX" or path.name.upper() == "FONTS.LBX":
            continue
        sets = defaultdict(list)  # colour set -> picture entries
        for number, entry in lbx_entries(path.read_bytes()):
            pal = picture_palette(entry)
            colours = window(*pal) if pal else {}
            if colours:
                sets[tuple(sorted(colours.items()))].append(number)
        for key, numbers in sets.items():
            rows.extend(rows_for(path.stem.lower(), ",".join(map(str, numbers)), dict(key)))
    out.write_text("\n".join(rows) + "\n")
    print(f"wrote {out} ({len(rows) - 1} rows)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
