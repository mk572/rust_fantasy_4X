#!/usr/bin/env python3
"""Try the wizard gem of task 570 as a composition: a base diamond (the first five pictures of
`art/magic.sheet.png`, turned in hue for the five newer colours) with the wizard's portrait (the
33 x 35 pictures of the same sheet, rounded corners included) put on it. The original blends the
portrait into the frame over its outer three rows and columns; here the outer pixels are shown with
the hue of the diamond under them, at the weight the original has, and the middle third of the top
and the bottom is shown as it is (the top of the head is not covered).

Writes a side-by-side picture (the original gem on the left, the composed one on the right, six times
larger) and prints, for each colour, how far the composed gems are from the original ones (mean
absolute difference per channel; a plain paste of the portrait is the baseline). Only the wizards
whose `sheet2` has the standard layout (gems 0 to 9 in the first row, 132 pixels apart) and whose
portrait cell is listed here are used. Needs Pillow (.venv) and the local art.

    .venv/bin/python scripts/gem_compose.py [--out FILE] [--overlap hue|tint]
"""
import argparse
import colorsys
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from make_banner_colours import NEW, OLD, turn_rgb  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "assets-local" / "art"
GEM_W, GEM_H = 41, 43
PORTRAIT_W, PORTRAIT_H = 33, 35
CELL = 132
# Where a wizard's portrait is in `magic.sheet.png` (row and column of the 41 x 43 cells) and where
# the original puts it in the gem (the artists placed each by hand: 4 to 6 across, 4 or 5 down).
PORTRAITS = {
    "ariel": ((2, 5), (4, 5)),
    "horus": ((2, 1), (5, 4)),
    "jafar": ((1, 3), (4, 5)),
    "kali": ((3, 1), (6, 5)),
    "oberic": ((1, 4), (5, 5)),
    "raven": ((1, 0), (4, 4)),
    "tauron": ((2, 4), (5, 5)),
}
DEFAULT_AT = (4, 5)
# How much of the portrait the gem shows at 0, 1 and 2 pixels from the portrait's edge (the original's
# values, measured over six wizards and five colours); from 3 pixels in it is all of it.
RAMP = {
    "top": (0.0, 0.6, 0.85),
    "bottom": (0.1, 0.4, 0.7),
    "left": (0.1, 0.45, 0.8),
    "right": (0.1, 0.45, 0.8),
}
# In the middle third of the top and the bottom the same three rows show this much of the portrait
# (the top of the head); the change comes in over EASE pixels on each side of the middle third.
MIDDLE = (0.5, 1.0, 1.0)
EASE = 3
# What the outer pixels show of the portrait: its colour with the hue of the diamond under it
# ("hue"), or with the hue and saturation of it ("tint"), the lightness of the portrait kept.
OVERLAPS = ("hue", "tint")


def base_diamonds():
    """The ten diamonds: the five of the original, then the five turned ones."""
    sheet = Image.open(ART / "magic.sheet.png").convert("RGBA")
    old = [sheet.crop((GEM_W * i, 0, GEM_W * (i + 1), GEM_H)) for i in range(len(OLD))]
    new = []
    for _, source, degrees, value in NEW:
        image = old[OLD.index(source)].copy()
        px = image.load()
        for y in range(GEM_H):
            for x in range(GEM_W):
                r, g, b, a = px[x, y]
                if a:
                    px[x, y] = (*turn_rgb((r, g, b), degrees, value), a)
        new.append(image)
    return old + new


def portrait(cell):
    """The portrait in the cell (row, column) of the magic sheet."""
    sheet = Image.open(ART / "magic.sheet.png").convert("RGBA")
    row, column = cell
    left, top = GEM_W * column, GEM_H * row
    return sheet.crop((left, top, left + PORTRAIT_W, top + PORTRAIT_H))


def mix(under, over, weight):
    return tuple(round(under[i] * (1 - weight) + over[i] * weight) for i in range(3)) + (255,)


def shifted(portrait_pixel, diamond_pixel, overlap):
    """The portrait's pixel turned to the hue (for "tint" also the saturation) of the diamond's."""
    _, saturation, value = colorsys.rgb_to_hsv(*(c / 255 for c in portrait_pixel[:3]))
    hue, diamond_saturation, _ = colorsys.rgb_to_hsv(*(c / 255 for c in diamond_pixel[:3]))
    r, g, b = colorsys.hsv_to_rgb(hue, diamond_saturation if overlap == "tint" else saturation, value)
    return (round(r * 255), round(g * 255), round(b * 255), 255)


def middle(x):
    """1 in the middle third of the portrait's width, falling to 0 over EASE pixels either side."""
    third = PORTRAIT_W / 3
    outside = max(third - x, x - (PORTRAIT_W - 1 - third), 0)
    return 1 - min(outside / EASE, 1)


def ramp(side, distance, x=None):
    """How much of the portrait shows `distance` pixels from the edge `side`."""
    if distance >= len(RAMP[side]):
        return 1.0
    weight = RAMP[side][distance]
    if x is not None and side in ("top", "bottom"):
        weight += (MIDDLE[distance] - weight) * middle(x)
    return weight


def weight_at(x, y):
    """How much of the portrait shows at (x, y) of the portrait: the least of the four edges."""
    return min(
        ramp("left", x),
        ramp("right", PORTRAIT_W - 1 - x),
        ramp("top", y, x),
        ramp("bottom", PORTRAIT_H - 1 - y, x),
    )


def compose(diamond, face, at=DEFAULT_AT, overlap="hue"):
    """The diamond with the portrait at `at`. With overlap `None` the portrait is pasted as it is."""
    gem = diamond.copy()
    for y in range(PORTRAIT_H):
        for x in range(PORTRAIT_W):
            pixel = face.getpixel((x, y))
            if pixel[3] == 0:
                continue
            spot = (x + at[0], y + at[1])
            under = gem.getpixel(spot)
            if overlap is None:
                gem.putpixel(spot, pixel)
                continue
            # Only the overlap is turned to the diamond's hue: the less of the pixel shows, the more
            # of it is turned; a pixel shown whole is not touched.
            weight = weight_at(x, y)
            turned = mix(pixel, shifted(pixel, under, overlap), 1 - weight)
            gem.putpixel(spot, mix(under, turned, weight))
    return gem


def distance(a, b):
    total = sum(
        abs(a.getpixel((x, y))[i] - b.getpixel((x, y))[i])
        for x in range(GEM_W)
        for y in range(GEM_H)
        for i in range(3)
    )
    return total / (GEM_W * GEM_H * 3)


def wizards():
    """(name, original gems, portrait, where it is) of the wizards listed in PORTRAITS."""
    found = []
    for name, (cell, at) in PORTRAITS.items():
        sheet = Image.open(ART / "wizards" / f"{name}.sheet2.png").convert("RGBA")
        gems = [sheet.crop((CELL * c, 0, CELL * c + GEM_W, GEM_H)) for c in range(10)]
        found.append((name, gems, portrait(cell), at))
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", default="/tmp/gem_compose.png", help="the comparison picture")
    parser.add_argument("--overlap", choices=OVERLAPS, default="hue", help="what the outer pixels show")
    args = parser.parse_args()
    diamonds = base_diamonds()
    chosen = wizards()
    names = OLD + [n for n, *_ in NEW]
    print(f"{len(chosen)} wizards: {', '.join(n for n, *_ in chosen)}")
    scale = 6
    sheet = Image.new(
        "RGBA", (len(chosen) * 2 * GEM_W * scale + 20 * len(chosen), 10 * GEM_H * scale), (255, 255, 255, 255)
    )
    for colour in range(10):
        pasted, blended = [], []
        for column, (_, gems, face, at) in enumerate(chosen):
            original = gems[colour]
            pasted.append(distance(original, compose(diamonds[colour], face, at, None)))
            mine = compose(diamonds[colour], face, at, args.overlap)
            blended.append(distance(original, mine))
            left = column * (2 * GEM_W * scale + 20)
            top = colour * GEM_H * scale
            sheet.paste(original.resize((GEM_W * scale, GEM_H * scale), Image.NEAREST), (left, top))
            sheet.paste(mine.resize((GEM_W * scale, GEM_H * scale), Image.NEAREST), (left + GEM_W * scale, top))
        print(
            f"{names[colour]:>10}: plain paste {sum(pasted) / len(pasted):5.2f}, "
            f"composed {sum(blended) / len(blended):5.2f}"
        )
    sheet.save(args.out)
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
