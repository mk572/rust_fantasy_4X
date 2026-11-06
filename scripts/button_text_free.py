#!/usr/bin/env python3
"""Make text-free copies of the original's menu-bar buttons (the marbled gold ones with a label).

Reads `assets-local/screens/main_screen/button_<name>_00.png` (local-only extractor output)
and writes `crates/f4x-client/assets/button/<name>.png`: the same picture with the label
taken out, so the client draws the label itself with the font engine (and can localise it).

The label is style 4 in the font's colour set 1 (ink (8,4,4)) with the original's "shadow down":
copies of the ink one pixel right, down and diagonally, in a lighter gold. The script finds the ink
by its colour takes the three shifted copies of it too, and fills every pixel it took with the
marbling of the same row (the nearest pixel left or right that is not part of the label), because
the marble streaks run sideways. Needs Pillow (`.venv`).

The Next Turn button (67x16, gold lettering with a dark shadow on grey stone) is cleaned by colour:
the stone is grey and the lettering saturated.

The city screen's Buy, Change and OK (wooden, 30x10 and 36x10; style 1 lettering in lavender grey,
no shadow) are cleaned by colour: the lettering is the only blue-over-red pixels. They are written as
`city_<name>.png`; the pressed picture is the up one moved, as for the others.

The four action buttons of the unit window (Done, Patrol, Wait and Build, Meld or Purify) are
33x9 and lettered in style 0 (5 px), ink (8,4,4), at y=2, with the "shadow down" copies in a lighter gold. Only the rows of the lettering
count as ink, since the Wait and Build pictures have a dark speck below it. Only the up picture is
made: the original's pressed picture (frame 1) is the up one a pixel right and down under a dark
top row and left column, which the client draws itself. The locked pictures (`button_<name>_locked_00.png`, a dimmed copy with
the label in (121,85,36)) are cleaned at the places where the up picture has its ink.

The plank buttons of the load/save and settings screens (gold brown, lettering in mixed case) have a
blank picture already: `button_load_inactive_00.png` (38x13). `plank_<width>.png` is that picture
widened by repeating columns from its middle, so the Settings screen's Ok (40 wide) is the plank
plus a label from the font engine.

The army list's Items and Ok (41x15, dark blue marble with a gold border; gold blackletter, a black
shadow), the item screen's Alchemy and Ok (59x15, brown stone) and the magic screen's Alchemy and Ok
(58x15 and 26x15, blue stone) share one blank per family and width, made from the Ok picture (it has
the least lettering to overwrite): `panel_41.png`, `stone_59.png` and `blue_26.png`; the magic screen's
Ok is too small to widen, so `blue_58.png` is made from its Alchemy. The army list's lettering is gold (or the black of
its shadow) inside the border; the stone ones are lavender (four exact greys the stone does not have);
all are taken with their shadow down and to the right. The pressed picture is the up one moved.

The lavender lettering of those four buttons has a metallic sheen: its tone depends only on x - y, so
the light and dark bands run diagonally across the whole word. `sheen_<family>_<name>.png` holds that
pattern for one button (same size as the button, a colour for every pixel from the nearest diagonal that
has lettering on it); the client keeps the lettering's pixels and takes their colours from it.

The battle screen's six buttons (26x10, Spell, Wait, Info, Auto, Flee, Done; style 0 lettering in
(52,40,28) at rows 1 to 6 with a lighter shadow) share one blank made from the Info: `battle.png` and `battle_locked.png`.

The pictures this script reads that the game no longer draws live in `assets-unused/` (beside
`assets-local/`); `open_image` finds them there.

The question boxes' Yes and No slabs (66x17, `art/resource/003_00.png` and `004_00.png`, ink (4,69,36) and
(93,16,16)) lose their word: `yes.png` and `no.png`.

The crafting screen's ten tabs (31x13) share one blank, `tab.png`, made from the ten words' bare pixels.

Its OK is `craft_ok.png` (32x16); the x1 to x4 charge buttons read the same in every language and stay.

The alchemy popup's picture (`alchemy_frame.png`, the title cleared) and its stone Cancel and OK
(`alchemy_stone.png`, 49x16, from the OK) are text-free.

The options screen's picture `newgame` 1 loses the title "Game Options" and the four labels
(`options_frame.png`: they are cleared, the rest is transparent already). The new game screens' wooden OK and Cancel (64x16 pictures `newgame` 2 and 3, small OK 30x14) need no
cleaning: the screen has blank wooden plates of the same style (`newgame_004_00.png`, 64x15 opaque).
`wood_64.png` is that plate and `wood_30.png` its left and right 15 columns joined (for the small
OK). The plate is a pixel bigger than the OK picture to the left and up: the client draws it at the OK's
position less (1, 1), so the bottom right edges meet. The pressed look is the plate moved.
"""
import argparse
from pathlib import Path

from PIL import Image

INK = (8, 4, 4)
NAMES = ["game", "spells", "armies", "cities", "magic", "info", "plane"]
ACTIONS = ["done", "patrol", "wait", "build", "meld", "purify"]
CITY = ["buy", "change", "ok"]
# (screen, output name, button, widths): the button's picture, cleaned, at each width. The Ok is the
# base where it is as wide as the others; the magic screen's (26) is too small to widen, so its
# Alchemy (58) is the base of the wide one.
OKS = [
    ("army_list", "panel", "ok", [41]),
    ("item_screen", "stone", "ok", [59]),
    ("magic_screen", "blue", "ok", [26]),
    ("magic_screen", "blue", "alchemy", [58]),
]
SHEENS = [
    ("item_screen", "stone", "alchemy"),
    ("item_screen", "stone", "ok"),
    ("magic_screen", "blue", "alchemy"),
    ("magic_screen", "blue", "ok"),
]
LOCKED = ["done", "patrol", "wait", "build"]
PLANK_WIDTHS = [38, 39, 40, 58]
TABS = ["sword", "mace", "axe", "bow", "staff", "wand", "misc", "shield", "chainmail", "platemail"]
# The battle screen's six 26x10 gold buttons (`combat/interface/button_<name>_00.png`).
BATTLE_INK = (52, 40, 28)


def open_image(path: Path) -> Image.Image:
    """Open an original picture: the pictures the game no longer draws moved from `assets-local/` to
    `assets-unused/` (`crates/f4x-client/extract/unused.tsv`), so a path that is gone is looked for
    there."""
    path = Path(path)
    if not path.exists():
        moved = Path(str(path).replace("assets-local", "assets-unused", 1))
        if moved.exists():
            path = moved
    return Image.open(path)


def ink_of(img: Image.Image, rows: range = None) -> set[tuple[int, int]]:
    """The pixels of the label's ink: the ink colour, not on the first row or column."""
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()
    return {
        (x, y)
        for y in (rows or range(1, h))
        for x in range(1, w if rows is None else w - 2)
        if px[x, y][:3] == INK
    }


def clean(
    img: Image.Image, ink: set[tuple[int, int]], shadow: bool = True
) -> tuple[Image.Image, int]:
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()
    taken = set(ink)
    for x, y in ink if shadow else ():
        for dx, dy in ((1, 0), (0, 1), (1, 1)):
            taken.add((x + dx, y + dy))
    taken = {p for p in taken if 0 <= p[0] < w and 0 <= p[1] < h}
    out = img.copy()
    po = out.load()
    for x, y in sorted(taken):
        for k in range(1, w):
            # alternate the side so neither edge of a word is always the source
            sides = (x + k, x - k) if (x + y) % 2 else (x - k, x + k)
            found = next(
                (px[s, y] for s in sides if 0 <= s < w and (s, y) not in taken), None
            )
            if found:
                po[x, y] = found
                break
    return out, len(taken)


def clean_next_turn(img: Image.Image) -> tuple[Image.Image, int]:
    """The Next Turn button: gold lettering with a dark brown shadow on grey stone. The stone is
    grey (little colour), the lettering and its shadow are saturated, so those pixels are the
    label; they are filled from the stone of the same row."""
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()
    taken = {
        (x, y)
        for y in range(h)
        for x in range(w)
        if max(px[x, y][:3]) - min(px[x, y][:3]) > 30
    }
    # the shadow's darkest edge is nearly grey: dark pixels touching the label belong to it
    for x, y in list(taken):
        for dx, dy in ((1, 0), (0, 1), (1, 1)):
            q = (x + dx, y + dy)
            if 0 <= q[0] < w and 0 <= q[1] < h and max(px[q][:3]) < 110:
                taken.add(q)
    out = img.copy()
    po = out.load()
    for x, y in sorted(taken):
        for k in range(1, w):
            sides = (x + k, x - k) if (x + y) % 2 else (x - k, x + k)
            found = next(
                (px[s, y] for s in sides if 0 <= s < w and (s, y) not in taken), None
            )
            if found:
                po[x, y] = found
                break
    return out, len(taken)


def city_ink(img: Image.Image) -> set[tuple[int, int]]:
    """The lettering of the city screen's wooden buttons: a cool lavender grey (blue above red) on
    brown wood (red above blue), in rows 2 to 6."""
    img = img.convert("RGBA")
    w, _ = img.size
    px = img.load()
    return {
        (x, y)
        for y in range(2, 7)
        for x in range(1, w - 2)
        if px[x, y][2] > px[x, y][0] + 5
    }


def panel_ink(img: Image.Image) -> set[tuple[int, int]]:
    """The lettering of the army list's blue buttons: gold, or the black of its shadow, inside the
    border (the bright gold line at row 1 and column 1, the darker one at the bottom and right, and the
    black frame)."""
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()
    return {
        (x, y)
        for y in range(2, h - 2)
        for x in range(2, w - 2)
        if px[x, y][0] > px[x, y][2] or px[x, y][:3] == INK
    }


LAVENDER = {(146, 146, 166), (178, 178, 195), (117, 117, 142), (174, 166, 162)}


def stone_ink(img: Image.Image) -> set[tuple[int, int]]:
    """The lavender lettering of the item and magic screens' buttons: the four greys the lettering
    is made of (none is in the stone), rows 3 to 11 inside the frame."""
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()
    return {
        (x, y)
        for y in range(3, 12)
        for x in range(4, w - 4)
        if px[x, y][:3] in LAVENDER
    }


def sheen(img: Image.Image) -> Image.Image:
    """The colour of the lettering along each diagonal x - y of the button, as a picture of the
    button's size: the commonest colour of the lettering on that diagonal, and for a diagonal with
    none the colour of the nearest one that has."""
    from collections import Counter

    img = img.convert("RGBA")
    w, h = img.size
    by_diagonal: dict[int, Counter] = {}
    for x, y in stone_ink(img):
        by_diagonal.setdefault(x - y, Counter())[img.getpixel((x, y))[:3]] += 1
    known = sorted(by_diagonal)
    out = Image.new("RGBA", (w, h))
    for y in range(h):
        for x in range(w):
            u = min(known, key=lambda k: (abs(k - (x - y)), k))
            out.putpixel((x, y), (*by_diagonal[u].most_common(1)[0][0], 255))
    return out


def widen(img: Image.Image, width: int, edge: int = 6) -> Image.Image:
    """`img` made `width` wide: its `edge` columns at each end kept, the middle filled by walking
    its middle columns back and forth. The same width returns the picture."""
    w, h = img.size
    if width == w:
        return img
    out = Image.new("RGBA", (width, h))
    middle = list(range(edge, w - edge))
    walk = middle + middle[-2:0:-1]
    for x in range(width):
        if x < edge:
            src = x
        elif x >= width - edge:
            src = w - (width - x)
        else:
            src = walk[(x - edge) % len(walk)]
        for y in range(h):
            out.putpixel((x, y), img.getpixel((src, y)))
    return out


def plank(blank: Image.Image, width: int) -> Image.Image:
    """The blank plank widened to `width` by repeating columns of its middle."""
    w, h = blank.size
    mid = w // 2
    out = Image.new("RGBA", (width, h))
    for x in range(width):
        src = x if x < mid else (mid if x < mid + width - w else x - (width - w))
        for y in range(h):
            out.putpixel((x, y), blank.convert("RGBA").getpixel((src, y)))
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--src", default="assets-local/screens/main_screen")
    ap.add_argument("--out", default="crates/f4x-client/assets/button")
    args = ap.parse_args()
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    for name in NAMES:
        img = open_image(Path(args.src) / f"button_{name}_00.png")
        img, n = clean(img, ink_of(img))
        img.save(out_dir / f"{name}.png")
        print(f"{name}: {n} label pixels replaced")
    img, n = clean_next_turn(open_image(Path(args.src) / "button_next_turn_00.png"))
    img.save(out_dir / "next_turn.png")
    print(f"next_turn: {n} label pixels replaced")
    for name in CITY:
        img = open_image(Path(args.src).parent / "city_screen" / f"button_{name}_00.png")
        img, n = clean(img, city_ink(img), shadow=False)
        img.save(out_dir / f"city_{name}.png")
        print(f"city {name}: {n} label pixels replaced")
    for screen, prefix, button, widths in OKS:
        img = open_image(Path(args.src).parent / screen / f"button_{button}_00.png")
        ink = panel_ink(img) if prefix == "panel" else stone_ink(img)
        img, n = clean(img, ink, shadow=prefix != "panel")
        for width in widths:
            widen(img, width).save(out_dir / f"{prefix}_{width}.png")
            print(f"{prefix} {width}: {n} label pixels replaced")
    plate = open_image(Path(args.src).parent / "new_game" / "newgame_004_00.png").convert("RGBA")
    plate = plate.crop(plate.getbbox())
    plate.save(out_dir / "wood_64.png")
    small = Image.new("RGBA", (30, plate.height))
    small.paste(plate.crop((0, 0, 15, plate.height)), (0, 0))
    small.paste(plate.crop((plate.width - 15, 0, plate.width, plate.height)), (15, 0))
    small.save(out_dir / "wood_30.png")
    print("wood_64, wood_30: from the blank plate")
    frame = open_image(Path(args.src).parent / "new_game" / "newgame_001_00.png").convert("RGBA")
    for box in [(0, 0, frame.width, 18), (0, 37, 83, 140)]:
        frame.paste((0, 0, 0, 0), box)
    frame.save(out_dir / "options_frame.png")
    print("options_frame: title and labels cleared")
    for screen, prefix, button in SHEENS:
        img = open_image(Path(args.src).parent / screen / f"button_{button}_00.png")
        sheen(img).save(out_dir / f"sheen_{prefix}_{button}.png")
        print(f"sheen {prefix} {button}")
    blank = open_image(Path(args.src).parent / "load_save" / "button_load_inactive_00.png")
    for width in PLANK_WIDTHS:
        plank(blank, width).save(out_dir / f"plank_{width}.png")
        print(f"plank {width}: widened from the blank picture")
    for name in ACTIONS:
        img = open_image(Path(args.src) / f"button_{name}_00.png")
        ink = ink_of(img, range(2, 7))
        img, n = clean(img, ink)
        img.save(out_dir / f"{name}.png")
        print(f"{name}: {n} label pixels replaced")
        if name in LOCKED:
            img = open_image(Path(args.src) / f"button_{name}_locked_00.png")
            img, n = clean(img, ink)
            img.save(out_dir / f"{name}_locked.png")
            print(f"{name} locked: {n} label pixels replaced")

    # one blank for all six: the Info picture cleans best (Spell and Auto keep dark patches)
    folder = Path(args.src).parent / "combat" / "interface"
    img = open_image(folder / "button_info_00.png").convert("RGBA")
    px = img.load()
    ink = {
        (x, y) for y in range(1, 7) for x in range(3, 23) if px[x, y][:3] == BATTLE_INK
    }
    cleaned, n = clean(img, ink)
    cleaned.save(out_dir / "battle.png")
    locked, _ = clean(open_image(folder / "button_info_locked_00.png"), ink)
    locked.save(out_dir / "battle_locked.png")
    print(f"battle: {n} label pixels replaced")
    # the Yes and No stone slabs of the question boxes (`resource` 3 and 4): one ink colour, no shadow
    for name, entry, ink_colour in [("yes", "003", (4, 69, 36)), ("no", "004", (93, 16, 16))]:
        img = open_image(Path(args.src).parent.parent / "art" / "resource" / f"{entry}_00.png")
        px = img.convert("RGBA").load()
        ink = {
            (x, y)
            for y in range(img.height)
            for x in range(img.width)
            if px[x, y][:3] == ink_colour
        }
        img, n = clean(img, ink, shadow=False)
        img.save(out_dir / f"{name}.png")
        print(f"{name}: {n} label pixels replaced")
    # the alchemy popup: its title "Transmute Gold to Power" is cleared (the ink and two pixels round
    # it, filled with the marbling of the row), and its stone Cancel and OK share the OK's blank
    folder = Path(args.src).parent / "alchemy"
    frame = open_image(folder / "background_00.png").convert("RGBA")
    px = frame.load()
    ink = {
        (x, y) for y in range(3, 22) for x in range(8, 158) if px[x, y][:3] == (8, 4, 4)
    }
    take = {
        (x + dx, y + dy)
        for x, y in ink
        for dx in range(-2, 3)
        for dy in range(-2, 3)
        if 9 <= y + dy <= 20 and 11 <= x + dx <= 154
    }
    clean(frame, take, shadow=False)[0].save(out_dir / "alchemy_frame.png")
    stone = open_image(folder / "button_ok_00.png").convert("RGBA")
    px = stone.load()
    ink = {
        (x, y) for y in range(2, 13) for x in range(3, 46) if px[x, y][:3] == (52, 40, 28)
    }
    clean(stone, ink)[0].save(out_dir / "alchemy_stone.png")
    print("alchemy_frame, alchemy_stone: title and labels cleared")
    # the item crafting screen's ten weapon and armour tabs (31x13, up and chosen): the lettering is
    # the pixels where the two pictures differ (grey and gold), with its shadow one pixel right and
    # down. One blank for all of them: each pixel is the most common one among the words that do
    # not cover it, the few that all cover take the marbling of their row.
    crafting = Path(args.src).parent / "item_crafting"
    seen: dict[tuple[int, int], list] = {}
    for word in TABS:
        up = open_image(crafting / f"button_{word}_00.png").convert("RGB")
        chosen = open_image(crafting / f"button_{word}_chosen_00.png").convert("RGB")
        pu, pc = up.load(), chosen.load()
        ink = {
            (x, y) for y in range(13) for x in range(31) if pu[x, y] != pc[x, y]
        }
        covered = {(x + dx, y + dy) for x, y in ink for dx in (0, 1) for dy in (0, 1)}
        for y in range(13):
            for x in range(31):
                if (x, y) not in covered:
                    seen.setdefault((x, y), []).append(pu[x, y])
    tab = Image.new("RGBA", (31, 13))
    pt = tab.load()
    for (x, y), colours in seen.items():
        pt[x, y] = (*max(set(colours), key=colours.count), 255)
    gaps = {(x, y) for y in range(13) for x in range(31) if (x, y) not in seen}
    tab, n = clean(tab, gaps, shadow=False)
    tab.save(out_dir / "tab.png")
    print(f"tab: {n} pixels no word leaves bare, filled from their rows")
    # the crafting screen's OK (the pixels in its text box that the body does not have)
    ok = open_image(crafting / "button_ok_00.png").convert("RGBA")
    px = ok.load()
    body = {px[x, y] for y in range(3, 13) for x in [*range(3, 9), *range(24, 29)]}
    ink = {(x, y) for y in range(3, 13) for x in range(9, 24) if px[x, y] not in body}
    clean(ok, ink, shadow=False)[0].save(out_dir / "craft_ok.png")
    print("craft_ok: lettering cleared")


if __name__ == "__main__":
    main()
