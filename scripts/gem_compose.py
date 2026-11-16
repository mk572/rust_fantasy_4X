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


def compose(diamond, face, blend=True):
    """The diamond with the face on it. Without `blend` the face is pasted as it is."""
    gem = diamond.copy()
    edge_colour = face.getpixel((0, FACE_H // 3))
    at_x, at_y = FACE_AT
    if blend:
        for x in range(at_x - 1, at_x + FACE_W + 1):
            for y, side in ((at_y - 1, "top"), (at_y + FACE_H, "bottom")):
                gem.putpixel((x, y), mix(gem.getpixel((x, y)), edge_colour, RING_WEIGHT[side]))
        for y in range(at_y, at_y + FACE_H):
            for x, side in ((at_x - 1, "left"), (at_x + FACE_W, "right")):
                gem.putpixel((x, y), mix(gem.getpixel((x, y)), edge_colour, RING_WEIGHT[side]))
    for fy in range(FACE_H):
        for fx in range(FACE_W):
            side = None
            if blend:
                side = {0: "top", FACE_H - 1: "bottom"}.get(fy) or {0: "left", FACE_W - 1: "right"}.get(fx)
            weight = EDGE_WEIGHT[side] if side else 1.0
            at = (fx + at_x, fy + at_y)
            gem.putpixel(at, mix(gem.getpixel(at), face.getpixel((fx, fy)), weight))
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
            pasted.append(distance(original, compose(diamonds[colour], face, blend=False)))
            mine = compose(diamonds[colour], face)
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
