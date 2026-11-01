#!/usr/bin/env python3
"""Write the original's 16 built-in font colour sets (FONTS.LBX palette entries) as RGB.

Each palette entry of FONTS.LBX holds, 768 bytes in, `font_colors`: 16 sets of 16 palette
indexes (`Set_Font_Style(style, set, ...)` picks a set; set 15 is the custom one that
`Set_Font_Colors_15` overwrites at run time). Entry 2 is the base palette, entries 3 to 8 the
alternate ones (load/save, menu, conquest, mastery, win, lose). Entry [0] of each set is the alias
colour, which `Set_Alias_Color` replaces at run time.
Usage: scripts/font_color_sets.py [FONTS.LBX] [OUT.tsv]
Default input: the Official Release in reference/mom-classic. Default output: docs/font_color_sets.tsv
Columns: palette (FONTS.LBX entry), set, entry (0-15), index (palette index), r g b, hex.
Indexes 224 and up are black in the base palette (entry 2): the screens fill them, see
palette_screens.tsv; their hex column says "screen" there.
"""
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LBX = ROOT / "reference/mom-classic/Master of Magic Official Release/FONTS.LBX"
ENTRIES = range(2, 9)
SETS, SET_SIZE, FONT_COLORS_AT = 16, 16, 768


def main() -> int:
    lbx = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_LBX
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "docs/font_color_sets.tsv"
    data = lbx.read_bytes()
    count, magic = struct.unpack_from("<HI", data, 0)
    if magic != 0x0000FEAD or max(ENTRIES) >= count:
        print(f"{lbx}: not the expected FONTS.LBX", file=sys.stderr)
        return 1
    offsets = struct.unpack_from(f"<{count + 1}I", data, 8)
    rows = ["palette\tset\tentry\tindex\tr\tg\tb\thex"]
    for entry in ENTRIES:
        block = data[offsets[entry] : offsets[entry + 1]]
        palette = block[:768]
        sets = block[FONT_COLORS_AT : FONT_COLORS_AT + SETS * SET_SIZE]
        for k, index in enumerate(sets):
            r6, g6, b6 = (v & 0x3F for v in palette[3 * index : 3 * index + 3])
            r, g, b = ((v << 2) | (v >> 4) for v in (r6, g6, b6))
            hexa = "screen" if entry == 2 and index >= 224 else f"#{r:02X}{g:02X}{b:02X}"
            rows.append(f"{entry}\t{k // SET_SIZE}\t{k % SET_SIZE}\t{index}\t{r}\t{g}\t{b}\t{hexa}")
    out.write_text("\n".join(rows) + "\n")
    print(f"wrote {out} ({len(rows) - 1} rows)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
