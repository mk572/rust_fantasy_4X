#!/usr/bin/env python3
"""Make the small flags of the banner step: the five flags of the original's picture at half size,
and five more with the hue turned, for more player colours.

Reads assets-local/screens/new_game/newgame_046_00.png (the player's own extracted art, the five
flags on the screen's ground, which is cut away by comparing with background_00.png) and writes assets-local/art/banner_flags/NNN_00.png: entries 0 to 4
the flags of the original in the order of the screen (top to bottom), 5 to 9 the same turned.
`--turn` (default: pink, brown, turquoise, dark blue and orange) takes the angle in degrees, or one for each flag (comma separated); `90x0.7` also darkens the flag to 0.7 of its brightness. The output is
derived from the original's art, so it stays local like the rest of assets-local. Needs Pillow (.venv).
"""
import argparse
import colorsys
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets-local/screens/new_game/newgame_046_00.png"
GROUND = ROOT / "assets-local/screens/new_game/background_00.png"
# The picture is drawn at this column of the screen.
PICTURE_X = 158
TARGET = ROOT / "assets-local/art/banner_flags"
# The rows each flag of the picture stands in (the picture is drawn at the top of the screen).
# Green to pink, light blue to brown, red to turquoise, purple to dark blue, yellow to orange.
DEFAULT_TURN = "180,180x0.78,180,285x0.8,-18"
BANDS = [(21, 56), (56, 94), (94, 129), (129, 163), (163, 200)]


def cut_flags(picture, ground):
    """The picture with everything that is the screen's ground, and its shadows, made transparent."""
    out = picture.convert("RGBA")
    px, gp = out.load(), ground.convert("RGBA").load()
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, _ = px[x, y]
            if (r, g, b) == gp[PICTURE_X + x, y][:3]:
                px[x, y] = (0, 0, 0, 0)
                continue
            h, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
            # A shadow is the ground's brown made darker.
            if 10 / 360 < h < 50 / 360 and s < 0.65 and v < 0.5:
                px[x, y] = (0, 0, 0, 0)
    return out


def turn_hue(img, degrees, value=1.0):
    img = img.convert("RGBA")
    px = img.load()
    for y in range(img.height):
        for x in range(img.width):
            r, g, b, a = px[x, y]
            if a:
                h, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
                nr, ng, nb = colorsys.hsv_to_rgb((h + degrees / 360.0) % 1.0, s, v * value)
                px[x, y] = (round(nr * 255), round(ng * 255), round(nb * 255), a)
    return img


def half(img):
    """Half width and half height, the alpha averaged with the colour of what is there."""
    w, h = img.width // 2, img.height // 2
    out = Image.new("RGBA", (w, h))
    src, dst = img.load(), out.load()
    for y in range(h):
        for x in range(w):
            px = [src[2 * x + i, 2 * y + j] for i in (0, 1) for j in (0, 1)]
            solid = [p for p in px if p[3]]
            if len(solid) * 2 < 4:
                continue
            n = len(solid)
            dst[x, y] = (
                sum(p[0] for p in solid) // n,
                sum(p[1] for p in solid) // n,
                sum(p[2] for p in solid) // n,
                255,
            )
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--source", type=Path, default=SOURCE)
    ap.add_argument("--target", type=Path, default=TARGET)
    ap.add_argument("--turn", default=DEFAULT_TURN, help="degrees to turn the hue by, or five of them (see above)")
    args = ap.parse_args()
    angles = []
    for item in args.turn.split(","):
        degrees, _, value = item.partition("x")
        angles.append((float(degrees), float(value) if value else 1.0))
    if len(angles) == 1:
        angles *= 5
    if len(angles) != 5:
        print("--turn takes one angle or five", file=sys.stderr)
        return 2
    if not args.source.exists():
        print(f"{args.source} is missing (run extract-art first)", file=sys.stderr)
        return 1
    picture = cut_flags(Image.open(args.source), Image.open(GROUND))
    args.target.mkdir(parents=True, exist_ok=True)
    for i, (top, bottom) in enumerate(BANDS):
        band = picture.crop((0, top, picture.width, min(bottom, picture.height)))
        box = band.getbbox()
        if box is None:
            print(f"flag {i}: nothing in rows {top} to {bottom}", file=sys.stderr)
            return 1
        small = half(band.crop(box))
        small.save(args.target / f"{i:03d}_00.png")
        turn_hue(small, *angles[i]).save(args.target / f"{i + 5:03d}_00.png")
        print(f"flag {i}: {small.size}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
