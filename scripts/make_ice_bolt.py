#!/usr/bin/env python3
"""Make Ice Bolt's animation from Fire Bolt's: orange turned to light blue, and mirrored so the bolt
flies in from the left.

Reads assets-local/art/spells/fire_bolt/cmbtfx_000_NN.png (the player's own extracted art) and writes
assets-local/art/spells/ice_bolt/000_NN.png, where `content/spell_art.ron` finds it as
`(lbx: "spells/ice_bolt", entry: 0)`. The output is derived from the original's art, so it stays
local like the rest of assets-local (not committed). Needs Pillow (.venv).
"""
import argparse
import colorsys
import sys
from pathlib import Path

from PIL import Image, ImageOps

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets-local/art/spells/fire_bolt"
TARGET = ROOT / "assets-local/art/spells/ice_bolt"

# Fire's hues run from red through orange to yellow (0 to 60 degrees); they are squeezed into the
# blues from deep (210) to ice (190), the whites and greys staying what they are.
HUE_FROM, HUE_TO = (0.0, 60.0), (212.0, 188.0)


def to_ice(r, g, b):
    h, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
    deg = h * 360.0
    if deg > 180.0:
        deg -= 360.0
    deg = min(max(deg, HUE_FROM[0]), HUE_FROM[1])
    t = (deg - HUE_FROM[0]) / (HUE_FROM[1] - HUE_FROM[0])
    hue = (HUE_TO[0] + t * (HUE_TO[1] - HUE_TO[0])) / 360.0
    # Lighter and paler than the flame: light blue, not navy.
    s2 = s * 0.7
    v2 = min(1.0, v * 0.9 + 0.12)
    nr, ng, nb = colorsys.hsv_to_rgb(hue, s2, v2)
    return round(nr * 255), round(ng * 255), round(nb * 255)


def convert(img):
    img = img.convert("RGBA")
    px = img.load()
    for y in range(img.height):
        for x in range(img.width):
            r, g, b, a = px[x, y]
            if a:
                px[x, y] = (*to_ice(r, g, b), a)
    return ImageOps.mirror(img)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--source", type=Path, default=SOURCE, help="folder of Fire Bolt's frames")
    ap.add_argument("--target", type=Path, default=TARGET, help="folder to write Ice Bolt's frames to")
    args = ap.parse_args()
    frames = sorted(args.source.glob("cmbtfx_000_*.png"))
    if not frames:
        print(f"no Fire Bolt frames in {args.source} (run extract-art first)", file=sys.stderr)
        return 1
    args.target.mkdir(parents=True, exist_ok=True)
    for path in frames:
        n = path.stem.rsplit("_", 1)[1]
        convert(Image.open(path)).save(args.target / f"000_{n}.png")
    print(f"wrote {len(frames)} frames to {args.target}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
