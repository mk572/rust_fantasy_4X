#!/usr/bin/env python3
"""Write the original's 256-colour base palette (FONTS.LBX entry 2) as a lookup table.

Usage: scripts/palette_table.py [FONTS.LBX] [OUT.tsv]
Default input: the Official Release in reference/mom-classic. Default output: docs/palette.tsv
Columns: index, r6 g6 b6 (as stored, 0-63), r g b (8-bit, as vga_to_rgb8), hex.
"""
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LBX = ROOT / "reference/mom-classic/Master of Magic Official Release/FONTS.LBX"
ENTRY = 2


def main() -> int:
    lbx = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_LBX
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "docs/palette.tsv"
    data = lbx.read_bytes()
    count, magic = struct.unpack_from("<HI", data, 0)
    if magic != 0x0000FEAD or ENTRY >= count:
        print(f"{lbx}: not an LBX with entry {ENTRY}", file=sys.stderr)
        return 1
    start, end = struct.unpack_from("<II", data, 8 + 4 * ENTRY)
    pal = data[start:end][:768]
    if len(pal) < 768:
        print("palette entry too short", file=sys.stderr)
        return 1
    rows = ["index\tr6\tg6\tb6\tr\tg\tb\thex"]
    for i in range(256):
        r6, g6, b6 = (v & 0x3F for v in pal[3 * i : 3 * i + 3])
        r, g, b = ((v << 2) | (v >> 4) for v in (r6, g6, b6))
        rows.append(f"{i}\t{r6}\t{g6}\t{b6}\t{r}\t{g}\t{b}\t#{r:02X}{g:02X}{b:02X}")
    out.write_text("\n".join(rows) + "\n")
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
