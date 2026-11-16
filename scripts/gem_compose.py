#!/usr/bin/env python3
"""Try the wizard gem of task 570 as a composition: a base diamond (the first five pictures of
`art/magic.sheet.png`, turned in hue for the five newer colours) with the wizard's small face (the
30 x 32 `halofam` picture, the `hall` role) put on it, the face's outer pixel blended with the
diamond and the diamond's pixel just outside the face tinted with the face's edge colour.

Writes a side-by-side picture (the original gem on the left, the composed one on the right, six times
larger) and prints, for each colour, how far the composed gems are from the original ones (mean
absolute difference per channel; the plain paste of the face is the baseline). The weights are
those measured on the original's gems (see task 570): the face's edge pixel keeps 0.68 of the face
(0.30 at the bottom) and the ring just outside it takes 0.32 of the face's edge colour (0.45 at
the bottom). Only the wizards whose `sheet2` has the standard layout (gems 0 to 9 and the face in
the first row, 132 pixels apart) are used. Needs Pillow (.venv) and the local art.

    .venv/bin/python scripts/gem_compose.py [--out FILE]
"""
import argparse
import colorsys
import glob
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from make_banner_colours import NEW, OLD, turn_rgb  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "assets-local" / "art"
GEM_W, GEM_H = 41, 43
FACE_W, FACE_H = 30, 32
FACE_AT = (6, 6)
CELL = 132
FACE_CELL = 10
# Weight of the face's edge pixel, and of the face's edge colour in the diamond's pixel outside the
# face, by side: top, bottom, left, right.
EDGE_WEIGHT = {"top": 0.68, "bottom": 0.30, "left": 0.69, "right": 0.66}
RING_WEIGHT = {"top": 0.30, "bottom": 0.45, "left": 0.32, "right": 0.33}
# The corners of the overlap are rounded: a face pixel this far from a corner (steps along the two
# sides) keeps at most this much of the face, and the diamond's pixels outside the face within
# CORNER_RING pixels of a corner are not tinted at all.
CORNER_WEIGHT = {0: 0.0, 1: 0.2, 2: 0.55, 3: 0.85}
CORNER_RING = 3
# What the overlap shows of the face: its colour with the hue of the diamond under it ("hue"), or
# with the hue and saturation of the diamond ("tint"), the lightness of the face kept.
OVERLAPS = ("hue", "tint")
# On the face's edge rows (top and bottom) the shift to the diamond's hue is reduced in the middle
# third of the width, down to this share, and comes back to the full shift over RAMP pixels.
MIDDLE_SHIFT = 0.35
RAMP = 3
# The pixel added above and below the face in the middle third takes the colour of the picture
# just inside the face's edge row (the edge row itself is the face's grey border), at this weight,
# and only this share of the shift to the diamond's hue.
CUTOUT_WEIGHT = 0.65
CUTOUT_SHIFT = 0.3


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


def mix(under, over, weight):
    return tuple(round(under[i] * (1 - weight) + over[i] * weight) for i in range(3)) + (255,)


def middle_share(fx):
    """How much of the shift to the diamond's hue the face's edge pixel at column `fx` gets."""
    third = FACE_W / 3
    outside = max(third - fx, fx - (FACE_W - 1 - third), 0)
    return MIDDLE_SHIFT + (1 - MIDDLE_SHIFT) * min(outside / RAMP, 1)


def edge_middle(fx):
    """1 in the middle third of the face's width, falling to 0 over RAMP pixels on both sides."""
    return (1 - middle_share(fx)) / (1 - MIDDLE_SHIFT)


def shifted(face_pixel, diamond_pixel, overlap, share=1.0):
    """The face's pixel turned to the hue (and for "tint" the saturation) of the diamond's."""
    fh, fs, fv = colorsys.rgb_to_hsv(*(c / 255 for c in face_pixel[:3]))
    dh, ds, _ = colorsys.rgb_to_hsv(*(c / 255 for c in diamond_pixel[:3]))
    r, g, b = colorsys.hsv_to_rgb(dh, ds if overlap == "tint" else fs, fv)
    turned = (round(r * 255), round(g * 255), round(b * 255), 255)
    return mix(face_pixel, turned, share)


def corner_steps(fx, fy):
    """How many steps a face pixel is from the nearest corner of the face, along the two sides."""
    return min(fx, FACE_W - 1 - fx) + min(fy, FACE_H - 1 - fy)


def compose(diamond, face, overlap="hue"):
    """The diamond with the face on it. With overlap `None` the face is pasted as it is."""
    gem = diamond.copy()
    at_x, at_y = FACE_AT
    blend = overlap is not None
    if blend:
        for x in range(at_x - 1, at_x + FACE_W + 1):
            if min(x - (at_x - 1), at_x + FACE_W - x) < CORNER_RING:
                continue
            for y, side, fy in ((at_y - 1, "top", 0), (at_y + FACE_H, "bottom", FACE_H - 1)):
                under = gem.getpixel((x, y))
                bump = edge_middle(x - at_x)
                column = min(max(x - at_x, 0), FACE_W - 1)
                inside = face.getpixel((column, 1 if side == "top" else FACE_H - 2))
                source = mix(face.getpixel((column, fy)), inside, bump)
                share = CUTOUT_SHIFT * bump + middle_share(x - at_x) * (1 - bump)
                over = shifted(source, under, overlap, share)
                weight = RING_WEIGHT[side] + (CUTOUT_WEIGHT - RING_WEIGHT[side]) * bump
                gem.putpixel((x, y), mix(under, over, weight))
        for y in range(at_y, at_y + FACE_H):
            if min(y - at_y, at_y + FACE_H - 1 - y) < CORNER_RING - 1:
                continue
            for x, side, fx in ((at_x - 1, "left", 0), (at_x + FACE_W, "right", FACE_W - 1)):
                under = gem.getpixel((x, y))
                over = shifted(face.getpixel((fx, y - at_y)), under, overlap)
                gem.putpixel((x, y), mix(under, over, RING_WEIGHT[side]))
    for fy in range(FACE_H):
        for fx in range(FACE_W):
            at = (fx + at_x, fy + at_y)
            under = gem.getpixel(at)
            weight, colour = 1.0, face.getpixel((fx, fy))
            if blend:
                side = {0: "top", FACE_H - 1: "bottom"}.get(fy) or {0: "left", FACE_W - 1: "right"}.get(fx)
                if side:
                    weight = EDGE_WEIGHT[side]
                    if side in ("top", "bottom"):
                        weight += (1 - weight) * edge_middle(fx)
                    share = 1.0
                    if side in ("top", "bottom"):
                        bump = edge_middle(fx)
                        inside = face.getpixel((fx, 1 if side == "top" else FACE_H - 2))
                        colour = mix(colour, inside, bump)
                        share = CUTOUT_SHIFT * bump + middle_share(fx) * (1 - bump)
                    colour = shifted(colour, under, overlap, share)
                weight = min(weight, CORNER_WEIGHT.get(corner_steps(fx, fy), 1.0))
            gem.putpixel(at, mix(under, colour, weight))
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
    """(name, original gems, face) of every wizard whose sheet has the standard layout."""
    found = []
    for path in sorted(glob.glob(str(ART / "wizards" / "*.sheet2.png"))):
        sheet = Image.open(path).convert("RGBA")
        cell = sheet.crop((CELL * FACE_CELL, 0, CELL * (FACE_CELL + 1), 60))
        if cell.getbbox() != (0, 0, FACE_W, FACE_H):
            continue
        gems = [sheet.crop((CELL * c, 0, CELL * c + GEM_W, GEM_H)) for c in range(10)]
        found.append((Path(path).name.split(".")[0], gems, cell.crop((0, 0, FACE_W, FACE_H))))
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", default="/tmp/gem_compose.png", help="the comparison picture")
    parser.add_argument("--overlap", choices=OVERLAPS, default="hue", help="what the overlap shows")
    args = parser.parse_args()
    diamonds = base_diamonds()
    chosen = wizards()
    names = OLD + [n for n, *_ in NEW]
    print(f"{len(chosen)} wizards with the standard layout: {', '.join(n for n, *_ in chosen)}")
    scale = 6
    sheet = Image.new(
        "RGBA", (len(chosen) * 2 * GEM_W * scale + 20 * len(chosen), 10 * GEM_H * scale), (255, 255, 255, 255)
    )
    for colour in range(10):
        pasted, blended = [], []
        for column, (_, gems, face) in enumerate(chosen):
            original = gems[colour]
            pasted.append(distance(original, compose(diamonds[colour], face, None)))
            mine = compose(diamonds[colour], face, args.overlap)
            blended.append(distance(original, mine))
            left = column * (2 * GEM_W * scale + 20)
            top = colour * GEM_H * scale
            sheet.paste(original.resize((GEM_W * scale, GEM_H * scale), Image.NEAREST), (left, top))
            sheet.paste(mine.resize((GEM_W * scale, GEM_H * scale), Image.NEAREST), (left + GEM_W * scale, top))
        print(
            f"{names[colour]:>10}: plain paste {sum(pasted) / len(pasted):5.2f}, "
            f"blended {sum(blended) / len(blended):5.2f}"
        )
    sheet.save(args.out)
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
