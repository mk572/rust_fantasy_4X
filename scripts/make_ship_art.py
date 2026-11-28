#!/usr/bin/env python3
"""make_ship_art.py: draws the ships of the sea battles (task 373), our own pixel art for the
battle grid: a smooth hull and plank deck for each ship's deck size and a grassy island for the
Floating Island, each in the four views of the battle screen. Writes
crates/f4x-client/assets/ship/deck_<width>x<length>_<view>.png. With --preview it writes instead a
picture of two triremes on the water with units on them, in the four views."""
import argparse
import random
import re
from pathlib import Path

PROJECT = Path(__file__).resolve().parent.parent
HEIGHT = 7  # pixels of hull above the water
PAD = 40  # clear pixels round the deck's squares in a picture, as `ship_art.rs` has it
SHIP = {"ship": True, "top": (172, 146, 108), "side": (104, 66, 36), "rail": (186, 140, 82)}
ISLAND = {"ship": False, "top": (74, 122, 52), "side": (112, 84, 54), "rail": (104, 150, 66)}
UNITS = PROJECT / "crates/f4x-def/content/units.ron"


def ship_shape(width, length):
    """The deck squares, as `ship_shape` in f4x-battle lays them out."""
    taper, least = min(width // 2, 2), 2 if width % 2 == 0 else 1
    cells = []
    for row in range(length):
        inset = 2 * max(taper - min(row, length - 1 - row), 0)
        here = min(max(width - inset, least), width)
        x0 = (width - here) // 2
        cells += [(x, row) for x in range(x0, x0 + here)]
    return cells


def deck_sizes():
    """The deck of each unit of `units.ron` that has one: {unit id: (width, length)}."""
    found = {}
    for line in UNITS.read_text(encoding="utf-8").splitlines():
        unit = re.search(r'id: "(\w+)"', line)
        deck = re.search(r"deck: Some\(\((\d+), (\d+)\)\)", line)
        if unit and deck:
            found[unit[1]] = (int(deck[1]), int(deck[2]))
    if not found:
        raise SystemExit(f"no unit with a deck in {UNITS}")
    return found


def screen(u, v):
    return (u - v) * 16 + 16, (u + v) * 8


def grid(x, y):
    a, b = y / 8, (x - 16) / 16
    return (a + b) / 2, (a - b) / 2


def inside(poly, u, v):
    hit = False
    for (x1, y1), (x2, y2) in zip(poly, poly[1:] + poly[:1]):
        if (y1 > v) != (y2 > v) and u < x1 + (v - y1) / (y2 - y1) * (x2 - x1):
            hit = not hit
    return hit


def shade(c, f):
    return tuple(max(0, min(255, int(v * f))) for v in c) + (255,)


def rotate(p, quarters):
    for _ in range(quarters):
        p = (-p[1], p[0])
    return p


def rotate_cell(c, quarters):
    x, y = rotate((c[0] + 0.5, c[1] + 0.5), quarters)
    return (int((x - 0.5) // 1), int((y - 0.5) // 1))


def ship_outline(width, length):
    """The smooth hull round a ship's deck, bow towards row 0, as a polygon in grid coordinates."""
    middle, half, taper = width / 2, width / 2 + 0.2, min(width // 2, 2)
    full, aft = float(taper), length - 1.1 - (taper - 1)  # where the sides run straight
    bow = [(half * 0.5, -1.7 + (full + 1.7) * 0.75 / 2.7),
           (half * 0.853, -1.7 + (full + 1.7) * 1.65 / 2.7), (half, full)]
    stern = [(half, aft), (half * 0.735, aft + (length + 0.35 - aft) * 0.9 / 1.45),
             (half * 0.353, length + 0.35)]
    if half * 0.735 < 1.12:  # two wide: the corners of the last row stay inside the hull
        stern[1:] = [(1.12, length + 0.05), (0.75, length + 0.35)]
    right = bow + stern
    points = [(middle, -1.7)] + [(middle + x, y) for x, y in right]
    points += [(middle - x, y) for x, y in reversed(right)]
    return [(round(x, 2), round(y, 2)) for x, y in points]


def island_outline(width, length):
    """The shore round the Floating Island's deck, in grid coordinates."""
    ring = [(0, -1.2), (0.48, -0.88), (0.88, -0.48), (1.2, 0), (0.88, 0.48), (0.48, 0.88),
            (0, 1.2), (-0.48, 0.88), (-0.88, 0.48), (-1.2, 0), (-0.88, -0.48), (-0.48, -0.88)]
    return [(width / 2 * (1 + x), length / 2 * (1 + y)) for x, y in ring]


def deck_picture(width, length, quarters, style):
    """One deck in one view. The top left of the turned deck's square (0, 0) is at
    ((rows - 1) * 16 + PAD, PAD), where `rows` is the turned deck's extent along y."""
    from PIL import Image

    shape = ship_outline if style["ship"] else island_outline
    turned = [rotate_cell(c, quarters) for c in ship_shape(width, length)]
    low = (min(x for x, _ in turned), min(y for _, y in turned))
    cells = [(x - low[0], y - low[1]) for x, y in turned]
    outline = [(u - low[0], v - low[1])
               for u, v in (rotate(p, quarters) for p in shape(width, length))]
    columns, rows = max(x for x, _ in cells) + 1, max(y for _, y in cells) + 1
    size = ((columns + rows) * 16 + 2 * PAD, (columns + rows) * 8 + 2 * PAD + HEIGHT)
    canvas = Image.new("RGBA", size, (0, 0, 0, 0))
    draw_ship(canvas, ((rows - 1) * 16 + PAD, PAD), cells, outline, style)
    return canvas


def draw_ship(canvas, origin, cells, outline, style=SHIP):
    """A deck with grid corner (0, 0) at `origin`, drawn in `style`. `outline` is a smooth hull
    polygon in grid coordinates."""
    ox, oy = origin
    squares = set(cells)
    rng = random.Random(7)
    noise = {}

    def walk(u, v):
        return (int(u // 1), int(v // 1)) in squares

    def hull(x, y):
        u, v = grid(x + 0.5, y + 0.5)
        return walk(u, v) or inside(outline, u, v)

    xs, ys = [], []
    for p in outline + [(x + dx, y + dy) for x, y in cells for dx in (0, 1) for dy in (0, 1)]:
        sx, sy = screen(*p)
        xs.append(int(sx)); ys.append(int(sy))
    x0, x1, y0, y1 = min(xs) - 2, max(xs) + 3, min(ys) - 6, max(ys) + HEIGHT + 5
    px = canvas.load()

    def put(x, y, colour):
        cx, cy = ox + x, oy + y
        if 0 <= cx < canvas.width and 0 <= cy < canvas.height:
            px[cx, cy] = colour

    def blend(x, y, colour, a):
        cx, cy = ox + x, oy + y
        if 0 <= cx < canvas.width and 0 <= cy < canvas.height:
            was = px[cx, cy]
            if was[3] == 0:
                px[cx, cy] = tuple(colour[:3]) + (int(255 * a),)
            else:
                px[cx, cy] = tuple(int(was[i] * (1 - a) + colour[i] * a) for i in range(3)) + (255,)

    top_colour, side, rail = style["top"], style["side"], style["rail"]
    # Shadow and foam on the water.
    for y in range(y0, y1):
        for x in range(x0, x1):
            if hull(x, y):
                continue
            if any(hull(x, y - k) for k in range(HEIGHT + 1, HEIGHT + 3)) and not any(hull(x, y - k) for k in range(1, HEIGHT + 1)):
                blend(x, y, (215, 232, 250) if (x // 3 + y) % 3 else (150, 180, 230), 0.75)
            elif hull(x - 3, y - HEIGHT - 3):
                blend(x, y, (0, 0, 30), 0.35)
    # The sides below the near edges.
    for y in range(y0, y1):
        for x in range(x0, x1):
            if hull(x, y):
                continue
            k = next((k for k in range(1, HEIGHT + 1) if hull(x, y - k)), None)
            if k is None:
                continue
            # Which way the edge above runs decides how the face is lit.
            left = hull(x + 2, y - k) and not hull(x - 2, y - k)
            f = 1.05 if left else 0.80
            if k == 1:
                colour = shade(rail, 0.95 * f)
            elif k in (3, 5):
                colour = shade(side, 0.62 * f)
            elif k == HEIGHT:
                colour = shade(side, 0.45)
            else:
                colour = shade(side, f * (0.95 + 0.1 * rng.random()))
            if style["ship"] and k == 4 and (x % 8) in (3, 4):  # oar ports
                colour = (28, 18, 10, 255)
            put(x, y, colour)
    # The deck.
    for y in range(y0, y1):
        for x in range(x0, x1):
            if not hull(x, y):
                continue
            u, v = grid(x + 0.5, y + 0.5)
            plank = int((u * 4) // 1)
            seam = int((v * 4 + plank * 5) // 1)
            key = (plank, (seam + 2) // 9)
            tone = noise.setdefault(key, 0.95 + 0.09 * rng.random())
            f = tone * (0.98 + 0.04 * rng.random())
            if not style["ship"]:  # tufts of grass instead of planks
                f = 0.88 + 0.24 * rng.random()
            elif abs((u * 4) % 1) < 0.16:
                f = 0.76
            elif (seam + 2) % 9 == 0:
                f = 0.84
            colour = shade(top_colour, f)
            if not walk(u, v):  # the part of a smooth hull no unit can stand on
                colour = shade(top_colour, f * 0.62)
            edge = [not hull(x + dx, y + dy) for dx, dy in ((-2, 0), (2, 0), (0, -1), (0, 1))]
            if any(edge):
                colour = shade(rail, 1.0 if edge[2] or edge[0] else 0.85)
            elif any(not hull(x + dx, y + dy) for dx, dy in ((-4, 0), (4, 0), (0, -2), (0, 2))):
                colour = shade(side, 0.7)
            put(x, y, colour)
    # A low rail with posts along the far edges, so it never covers a unit's feet.
    for x in range(x0, x1) if style["ship"] else ():
        top = next((y for y in range(y0, y1) if hull(x, y)), None)
        if top is None:
            continue
        put(x, top - 3, shade(rail, 1.1))
        put(x, top - 2, shade(side, 0.7))
        if x % 6 == 0:
            put(x, top - 1, shade(rail, 0.8))
            put(x, top - 2, shade(rail, 0.9))


def figure(root, canvas, name, direction, at, banner):
    """A combat figure of the extracted art with its top left at `at` (screen pixels)."""
    from PIL import Image

    sheet = Image.open(root / f"art/units/{name}.sheet.png")
    rows = sheet.info["f4x-sheet"].split("\n")
    cw, ch = map(int, rows[0].split()[1].split("x"))
    names = [r.split("\t")[0] for r in rows[1:] if r]
    per_row = sheet.width // cw

    def cell(file):
        i = names.index(file)
        x, y = (i % per_row) * cw, (i // per_row) * ch
        return sheet.crop((x, y, x + cw, y + ch)).convert("RGBA")

    pic, mask = cell(f"figure_{direction}_01.png"), cell(f"figure_{direction}_01_banner.png")
    px, mk = pic.load(), mask.load()
    for y in range(ch):
        for x in range(cw):
            if mk[x, y][3] == 255 and 1 <= mk[x, y][0] <= 5:  # clothes in the owner's colour
                px[x, y] = shade(banner, 1.45 - 0.18 * mk[x, y][0])
    canvas.alpha_composite(pic, at)


# The two decks as the layout puts them, side by side: whose it is, and where its units stand
# (the square's place across the deck as a share of the width, and along it).
SHIPS = [
    ((40, 90, 220), [("high_men/high_men_swordsmen", (0.0, 1)), ("high_men/high_men_bowmen", (0.0, 4)),
                     ("high_men/high_men_swordsmen", (1.0, 3))]),
    ((200, 40, 40), [("gnoll/gnoll_spearmen", (0.0, 2)), ("gnoll/gnoll_spearmen", (1.0, 3)),
                     ("gnoll/gnoll_bowmen", (1.0, 5))]),
]
W, H = 300, 200


def scene(root, quarters):
    """One view of the preview: two triremes with units aboard, turned `quarters` quarter turns."""
    from PIL import Image, ImageDraw

    canvas = Image.new("RGBA", (W, H), (20, 40, 100, 255))
    water = [Image.open(root / f"screens/combat/city/cmbtcity_{n:03}_00.png").convert("RGBA") for n in (109, 110, 111)]
    pick = random.Random(3)
    for row in range(-2, H // 8 + 2):
        for col in range(-1, W // 32 + 2):
            canvas.paste(water[pick.randrange(3)], (col * 32 + (16 if row % 2 else 0), row * 8), water[0])
    width, length = deck_sizes()["trireme"]
    apart = width + 1  # one column of water between the decks
    mid = screen(*rotate(((apart + width) / 2, length / 2), quarters))
    origin = (W // 2 - int(mid[0]), H // 2 - int(mid[1]))
    # The farther ship first, so the nearer one lies over it.
    ships = [(n * apart, banner, units) for n, (banner, units) in enumerate(SHIPS)]
    for dx, banner, units in sorted(ships, key=lambda s: sum(rotate((s[0] + width / 2, length / 2), quarters))):
        cells = [rotate_cell((x + dx, y), quarters) for x, y in ship_shape(width, length)]
        hull = [rotate((u + dx, v), quarters) for u, v in ship_outline(width, length)]
        draw_ship(canvas, origin, cells, hull)
        placed = sorted(((rotate_cell((round(x * (width - 1)) + dx, y), quarters), name)
                         for name, (x, y) in units), key=lambda p: sum(p[0]))
        for (cx, cy), name in placed:
            sx, sy = screen(cx, cy)
            x, y = origin[0] + int(sx), origin[1] + int(sy)
            base = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
            ImageDraw.Draw(base).polygon([(x + 16, y), (x + 32, y + 8), (x + 16, y + 16), (x, y + 8)],
                                         fill=banner + (110,), outline=banner + (255,))
            canvas.alpha_composite(base)
            facing = ((2 if dx == 0 else 6) + 2 * quarters) % 8
            for fx, fy in ((1, 4), (-7, 8), (8, 8), (1, 12)):
                figure(root, canvas, name, facing, (x + 16 - 14 + fx - 1, y + 16 - 30 + fy - 8), banner)
    return canvas


def preview(target):
    """The two triremes in the four views, on the extracted art's water and with its figures."""
    from PIL import Image, ImageDraw

    root = PROJECT / "assets-local"
    if not root.is_dir():
        raise SystemExit(f"the preview needs the extracted art, there is no folder {root}")
    views = [scene(root, q) for q in range(4)]
    sheet = Image.new("RGBA", (W * 2 + 8, (H + 14) * 2), (16, 16, 16, 255))
    d = ImageDraw.Draw(sheet)
    for q, view in enumerate(views):
        x, y = (q % 2) * (W + 8), (q // 2) * (H + 14)
        sheet.alpha_composite(view, (x, y + 14))
        d.text((x + 4, y + 1), f"view {q + 1} of 4", fill=(255, 255, 255, 255))
    sheet.resize((sheet.width * 3, sheet.height * 3), Image.NEAREST).save(target)
    print(target)


def main():
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="Needs Pillow (.venv). The pictures are made from this script alone; only "
        "--preview reads the extracted art in assets-local/.",
    )
    parser.add_argument("--preview", metavar="FILE", type=Path,
                        help="write a picture of two triremes in the four views to FILE and "
                        "leave the client's pictures alone")
    args = parser.parse_args()
    if args.preview:
        preview(args.preview)
        return
    folder = PROJECT / "crates/f4x-client/assets/ship"
    folder.mkdir(parents=True, exist_ok=True)
    styles = {size: ISLAND if unit == "floating_island" else SHIP
              for unit, size in deck_sizes().items()}
    written = set()
    for (width, length), style in styles.items():
        for quarters in range(4):
            target = folder / f"deck_{width}x{length}_{quarters}.png"
            deck_picture(width, length, quarters, style).save(target, optimize=True)
            written.add(target)
            print(target.relative_to(PROJECT))
    for old in set(folder.glob("deck_*.png")) - written:
        old.unlink()
        print("removed", old.relative_to(PROJECT))


if __name__ == "__main__":
    main()
