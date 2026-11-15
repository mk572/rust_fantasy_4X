#!/usr/bin/env python3
"""Draw the game's app icon: `packaging/macos/icon.png` (1024 x 1024) and `Fantasy4X.icns`.

The picture is made here from a formula (a gold four-pointed star on a dark blue rounded square), so
it carries nothing of the original. Needs only the Python standard library, plus `sips` and
`iconutil` (macOS) for the `.icns`. Run it again only to change the picture.

    python3 scripts/make_icon.py [--out DIR]
"""

from __future__ import annotations

import argparse
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

SIZE = 1024
CORNER = 190.0
STAR_RADIUS = 360.0


def _clamp(value: float, low: float = 0.0, high: float = 1.0) -> float:
    return max(low, min(high, value))


def _rounded_square_coverage(x: float, y: float) -> float:
    """How much of pixel (x, y) is inside the rounded square (1 inside, 0 outside, soft edge)."""
    half = SIZE / 2 - 2
    dx = abs(x - SIZE / 2) - (half - CORNER)
    dy = abs(y - SIZE / 2) - (half - CORNER)
    outside = (max(dx, 0.0) ** 2 + max(dy, 0.0) ** 2) ** 0.5
    inside = min(max(dx, dy), 0.0)
    return _clamp(0.5 - (outside + inside - CORNER))


def _star_coverage(x: float, y: float) -> float:
    """A four-pointed star (an astroid) around the centre, soft edge."""
    u = abs(x - SIZE / 2)
    v = abs(y - SIZE / 2)
    f = (u ** (2 / 3) + v ** (2 / 3)) ** 1.5 - STAR_RADIUS
    return _clamp(0.5 - f / 2.5)


def pixel(x: int, y: int) -> tuple[int, int, int, int]:
    shape = _rounded_square_coverage(x + 0.5, y + 0.5)
    if shape <= 0.0:
        return (0, 0, 0, 0)
    t = y / SIZE
    background = (22 + 30 * t, 30 + 10 * t, 70 + 40 * t)
    star = _star_coverage(x + 0.5, y + 0.5)
    glow = _clamp(1.0 - (((x - SIZE / 2) ** 2 + (y - SIZE / 2) ** 2) ** 0.5) / 330.0) ** 2
    gold = (255 - 40 * t, 214 - 50 * t, 92 - 40 * t)
    colour = [
        (background[i] + glow * 40 * (i == 2)) * (1 - star) + gold[i] * star for i in range(3)
    ]
    return (*[int(_clamp(c / 255.0) * 255) for c in colour], int(shape * 255))


def write_png(path: Path) -> None:
    rows = bytearray()
    for y in range(SIZE):
        rows.append(0)
        for x in range(SIZE):
            rows.extend(pixel(x, y))

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(rows), 9))
        + chunk(b"IEND", b"")
    )


def write_icns(png: Path, icns: Path) -> None:
    sizes = [16, 32, 128, 256, 512]
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "Fantasy4X.iconset"
        iconset.mkdir()
        for size in sizes:
            for scale in (1, 2):
                name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
                subprocess.run(
                    ["sips", "-z", str(size * scale), str(size * scale), str(png), "--out", str(iconset / name)],
                    check=True,
                    capture_output=True,
                )
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(icns)], check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--out", type=Path, default=Path(__file__).resolve().parent.parent / "packaging/macos")
    args = parser.parse_args()
    if shutil.which("sips") is None or shutil.which("iconutil") is None:
        print("make_icon: sips and iconutil are needed (macOS)", file=sys.stderr)
        return 1
    args.out.mkdir(parents=True, exist_ok=True)
    png = args.out / "icon.png"
    write_png(png)
    write_icns(png, args.out / "Fantasy4X.icns")
    print(f"wrote {png} and {args.out / 'Fantasy4X.icns'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
