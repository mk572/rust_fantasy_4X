#!/usr/bin/env python3
"""Make the animations of spells the original has no picture for, from pictures it has.

  ice_bolt  Fire Bolt's frames, orange turned to light blue and mirrored (the bolt flies in from the left).
  petrify   Change Terrain's sparkles (specfx 8), green turned to dark brown.

Reads the player's own extracted art (assets-local/art/spells/...) and writes
assets-local/art/spells/<spell>/000_NN.png, where `content/spell_art.ron` finds it as
`(lbx: "spells/<spell>", entry: 0)`. The output is derived from the original's art, so it stays
local like the rest of assets-local (not committed). Needs Pillow (.venv).
"""
import argparse
import colorsys
import sys
from pathlib import Path

from PIL import Image, ImageOps

ROOT = Path(__file__).resolve().parent.parent
SPELLS = ROOT / "assets-local/art/spells"


def squeeze_hue(r, g, b, source, target):
    """HSV of a pixel with its hue (degrees) moved from the range `source` into `target`."""
    h, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
    deg = h * 360.0
    if deg > 180.0 and source[0] < 180.0 and source[1] < 180.0:
        deg -= 360.0
    deg = min(max(deg, source[0]), source[1])
    t = (deg - source[0]) / (source[1] - source[0])
    return (target[0] + t * (target[1] - target[0])) / 360.0, s, v


def ice(r, g, b):
    # Fire's hues run from red through orange to yellow (0 to 60 degrees); they are squeezed into
    # the blues from deep (212) to ice (188), the whites and greys staying what they are.
    hue, s, v = squeeze_hue(r, g, b, (0.0, 60.0), (212.0, 188.0))
    # Lighter and paler than the flame: light blue, not navy.
    return colorsys.hsv_to_rgb(hue, s * 0.7, min(1.0, v * 0.9 + 0.12))


def stone(r, g, b):
    # Green sparkles (about 60 to 120 degrees) to a dark brown (hue 26 to 30): the brightest point
    # of a sparkle is a medium-dark brown, the rest nearly black brown.
    hue, s, v = squeeze_hue(r, g, b, (60.0, 120.0), (30.0, 26.0))
    return colorsys.hsv_to_rgb(hue, min(1.0, 0.5 + s * 0.3), 0.1 + v * 0.38)


RECIPES = {
    "ice_bolt": dict(
        source=SPELLS / "fire_bolt", glob="cmbtfx_000_*.png", colour=ice, mirror=True
    ),
    "petrify": dict(
        source=SPELLS / "_shared", glob="specfx_008_*.png", colour=stone, mirror=False
    ),
}


def convert(img, colour, mirror):
    img = img.convert("RGBA")
    px = img.load()
    for y in range(img.height):
        for x in range(img.width):
            r, g, b, a = px[x, y]
            if a:
                nr, ng, nb = colour(r, g, b)
                px[x, y] = (round(nr * 255), round(ng * 255), round(nb * 255), a)
    return ImageOps.mirror(img) if mirror else img


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("spell", nargs="?", choices=[*RECIPES, "all"], default="all",
                    help="the spell to make the animation of (default: all)")
    ap.add_argument("--target", type=Path, default=SPELLS, help="folder of the spells' folders")
    args = ap.parse_args()
    status = 0
    for name in RECIPES if args.spell == "all" else [args.spell]:
        recipe = RECIPES[name]
        frames = sorted(recipe["source"].glob(recipe["glob"]))
        if not frames:
            print(f"{name}: no frames in {recipe['source']} (run extract-art first)", file=sys.stderr)
            status = 1
            continue
        out = args.target / name
        out.mkdir(parents=True, exist_ok=True)
        for path in frames:
            n = path.stem.rsplit("_", 1)[1]
            convert(Image.open(path), recipe["colour"], recipe["mirror"]).save(out / f"000_{n}.png")
        print(f"{name}: wrote {len(frames)} frames to {out}")
    return status


if __name__ == "__main__":
    sys.exit(main())
