#!/usr/bin/env python3
"""Give the notify boxes' picture frames the colours of their strips in the packed art (task 511).

`resource.lbx` 41, 42, 44, 45, 47 and 48 (the plain and gem frames of the green, red and blue-grey
event boxes) have no palette of their own. They use the colours 224 to 243 that the strip's palette
(entries 40, 43 and 46, `Notify_Load` loads the strip first) set when it was loaded: the marble band
that runs around the picture, gold and blue for the green box. Extracted with the base palette those
indexes came out as translucent glass tints (light speckle). `f4x-extract art` now gives each frame
its strip's palette (`NOTIFY_FRAME` in `art.rs`); this script does the same to art already
extracted, in `assets-local/art/resource.sheet4.png` only (no full unpack and pack): it decodes the
frames from the game's `RESOURCE.LBX` and repaints every pixel of index 224 or more with the strip's
colour, opaque, keeping the sheet's picture list. Safe to run again.

    .venv/bin/python scripts/notify_frame_dark.py [RESOURCE.LBX]

The default LBX is `reference/mom-classic/Master of Magic Official Release/RESOURCE.LBX`. Needs
Pillow (`.venv`).
"""
import struct
import sys
from pathlib import Path

from PIL import Image, PngImagePlugin

ROOT = Path(__file__).resolve().parent.parent
SHEET = ROOT / "assets-local/art/resource.sheet4.png"
LBX = ROOT / "reference/mom-classic/Master of Magic Official Release/RESOURCE.LBX"
STRIP = {41: 40, 42: 40, 44: 43, 45: 43, 47: 46, 48: 46}


def entry(data, i):
    n = struct.unpack("<H", data[:2])[0]
    offs = struct.unpack(f"<{n + 1}I", data[8 : 8 + 4 * (n + 1)])
    return data[offs[i] : offs[i + 1]]


def palette(e):
    """(first index, [(r, g, b)]) of an entry's own palette, colours widened from 6 bits."""
    off = struct.unpack("<H", e[14:16])[0]
    data_off, start, n = struct.unpack("<HHH", e[off : off + 6])
    wide = lambda v: ((v & 63) << 2) | ((v & 63) >> 4)
    return start, [tuple(wide(c) for c in e[data_off + 3 * k : data_off + 3 * k + 3]) for k in range(n)]


def frame_indexes(e):
    """The first frame's palette indexes, row-major (the column RLE of `draw_frame` in `image.rs`)."""
    w, h = struct.unpack("<HH", e[:4])
    first = struct.unpack("<I", e[18:22])[0]
    d = e[first + 1 :]
    buf = [0] * (w * h)
    p = 0
    for x in range(w):
        op = d[p]
        p += 1
        if op == 0xFF:
            continue
        packet = d[p]
        p += 1
        y = 0
        while True:
            seq, skip = d[p], d[p + 1]
            p += 2
            y += skip
            packet -= seq + 2
            left = seq
            while left > 0:
                b = d[p]
                p += 1
                left -= 1
                if op == 0x80 and b >= 224:
                    run, v = b - 224 + 1, d[p]
                    p += 1
                    left -= 1
                    for _ in range(run):
                        buf[y * w + x] = v
                        y += 1
                else:
                    buf[y * w + x] = b
                    y += 1
            if packet < 1:
                break
    return w, h, buf


def main():
    lbx = Path(sys.argv[1]) if len(sys.argv) > 1 else LBX
    data = lbx.read_bytes()
    src = Image.open(SHEET)
    # The sheet carries its picture list in a tEXt chunk (`f4x-sheet`): "cell WxH" then one
    # "NNN_FF.png<TAB>w<TAB>h" line per picture, in cell order, left to right and top to bottom.
    info = PngImagePlugin.PngInfo()
    for key, value in src.text.items():
        info.add_text(key, value)
    lines = src.text["f4x-sheet"].splitlines()
    cw, ch = map(int, lines[0].split()[1].split("x"))
    entries = [int(l.split("_")[0]) for l in lines[1:]]
    im = src.convert("RGBA")
    px = im.load()
    per_row = im.width // cw
    n = 0
    for k, num in enumerate(entries):
        if num not in STRIP:
            continue
        start, colours = palette(entry(data, STRIP[num]))
        w, h, idx = frame_indexes(entry(data, num))
        assert (w, h) == (cw, ch)
        x0, y0 = (k % per_row) * cw, (k // per_row) * ch
        for y in range(h):
            for x in range(w):
                i = idx[y * w + x]
                if start <= i < start + len(colours):
                    px[x0 + x, y0 + y] = (*colours[i - start], 255)
                    n += 1
    im.save(SHEET, pnginfo=info)
    print(f"{n} pixels set in {SHEET.name}")


if __name__ == "__main__":
    main()
