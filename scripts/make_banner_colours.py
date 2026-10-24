#!/usr/bin/env python3
"""Make the five more player colours (task 381) in the local art: pink, brown, turquoise, dark blue
and orange, as the original's five turned in hue (the same turns as the flags of the banner step,
`make_banner_flags.py`: green to pink, blue to brown, red to turquoise, purple to dark blue, yellow
to orange; the game's blue is bluer than the flag's, so brown is turned by 150 degrees here and by 180
there, and pink is turned past purple so that the two stay apart).

Writes, into assets-local/art:
- `map/banners.tsv` and `map/tiles/minimap.tsv`: the rows of banners 5 to 9 are the new colours
  and the neutral one moves to 10 (the game's `NEUTRAL_BANNER`);
- the pictures that come already coloured, one more for each new colour: cities, towers and node
  auras (`map/locations/*_<colour>_*.png`), the unit backgrounds (`map/overlays/unit_backgrounds/`
  and `mapback/100..104_00.png`, entries the client asks for) and the wizards' gems
  (`lilwiz/100..169_00.png`, 100 + portrait * 5 + the new colour's number from 0).
Only the pixels that differ between the original's five colours are turned, so the shading stays.
Running it again gives the same files. Run it after `extract-art` (and `grey_neutral.py`, in either
order). The output is derived from the original's art, so it stays local. Needs Pillow (.venv).
"""
import argparse
import colorsys
import shutil
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
OLD = ["blue", "green", "purple", "red", "yellow"]
# The new colours: name, the original colour it is made from, degrees of hue, brightness.
NEW = [
    ("pink", "green", 210, 1.0),
    ("brown", "blue", 150, 0.78),
    ("turquoise", "red", 180, 1.0),
    ("dark_blue", "purple", 285, 0.8),
    ("orange", "yellow", -18, 1.0),
]
PORTRAITS = 14
FIRST_NEW = 5
NEUTRAL = FIRST_NEW + len(NEW)


def turn_rgb(rgb, degrees, value):
    r, g, b = rgb
    h, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
    nr, ng, nb = colorsys.hsv_to_rgb((h + degrees / 360.0) % 1.0, s, min(1.0, v * value))
    return round(nr * 255), round(ng * 255), round(nb * 255)


def recolour(variants, base, degrees, value):
    """The picture `variants[base]` with the pixels that differ between the variants turned."""
    out = variants[base].convert("RGBA")
    px = out.load()
    others = [v.convert("RGBA").load() for v in variants]
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, a = px[x, y]
            if a and any(o[x, y] != px[x, y] for o in others):
                px[x, y] = (*turn_rgb((r, g, b), degrees, value), a)
    return out


def same_size(images):
    return len({i.size for i in images}) == 1


def pictures(art):
    """The pictures named with the colour: `<name>_blue_<frame>.png` and the other four."""
    count = 0
    for folder in (art / "map/locations", art / "map/overlays/unit_backgrounds"):
        for blue in sorted(folder.glob("*_blue_*.png")):
            paths = [blue.with_name(blue.name.replace("_blue_", f"_{c}_")) for c in OLD]
            if not all(p.exists() for p in paths):
                continue
            images = [Image.open(p) for p in paths]
            if not same_size(images):
                continue
            for name, source, degrees, value in NEW:
                out = recolour(images, OLD.index(source), degrees, value)
                out.save(blue.with_name(blue.name.replace("_blue_", f"_{name}_")))
                count += 1
    # The unit backgrounds are also asked for by entry: `mapback` 100 to 104.
    for i, (name, *_rest) in enumerate(NEW):
        source = art / f"map/overlays/unit_backgrounds/unit_background_{name}_00.png"
        if source.exists():
            target = art / "mapback" / f"{100 + i:03d}_00.png"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            count += 1
    return count


def gems(art):
    """The wizards' gems (`lilwiz`, portrait * 5 + the colour) in the new colours."""
    folder = art / "lilwiz"
    count = 0
    for portrait in range(PORTRAITS):
        paths = [folder / f"{portrait * 5 + b:03d}_00.png" for b in range(5)]
        if not all(p.exists() for p in paths):
            continue
        images = [Image.open(p) for p in paths]
        if not same_size(images):
            continue
        for i, (name, source, degrees, value) in enumerate(NEW):
            entry = 100 + portrait * 5 + i
            base = OLD.index(source)
            recolour(images, base, degrees, value).save(folder / f"{entry:03d}_00.png")
            cycle = paths[base].with_name(paths[base].stem + "_cycle.png")
            if cycle.exists():
                shutil.copyfile(cycle, folder / f"{entry:03d}_00_cycle.png")
            count += 1
    return count


def tsv_rows(art):
    """banners.tsv with the new colours and the neutral one last; returns the number of rows."""
    path = art / "map/banners.tsv"
    lines = path.read_text().splitlines()
    head = [l for l in lines if l.startswith("#")]
    rows = {}
    for line in lines:
        fields = line.split("\t")
        if len(fields) >= 7 and not line.startswith("#"):
            rows[fields[1]] = [tuple(int(v) for v in c.split(",")) for c in fields[2:7]]
    out = list(head)
    for i, name in enumerate(OLD):
        out.append("\t".join([str(i), name] + [",".join(map(str, c)) for c in rows[name]]))
    for i, (name, source, degrees, value) in enumerate(NEW):
        shades = [turn_rgb(c, degrees, value) for c in rows[source]]
        out.append("\t".join([str(FIRST_NEW + i), name] + [",".join(map(str, c)) for c in shades]))
    out.append("\t".join([str(NEUTRAL), "neutral"] + [",".join(map(str, c)) for c in rows["neutral"]]))
    path.write_text("\n".join(out) + "\n")
    return len(NEW) + 1


def minimap_rows(art):
    """minimap.tsv: `banner <n> r g b` rows, the new colours made from the original's."""
    path = art / "map/tiles/minimap.tsv"
    if not path.exists():
        return 0
    lines = path.read_text().splitlines()
    banners = {}
    others = []
    first = None
    for line in lines:
        f = line.split("\t")
        if f[0] == "banner" and len(f) >= 5:
            banners[int(f[1])] = tuple(int(v) for v in f[2:5])
            if first is None:
                first = len(others)
        else:
            others.append(line)
    if not banners or any(i not in banners for i in range(5)):
        return 0
    # The neutral colour is the highest-numbered of the rows there were.
    neutral = banners[max(banners)]
    rows = [f"banner\t{i}\t" + "\t".join(map(str, banners[i])) for i in range(5)]
    for i, (_name, source, degrees, value) in enumerate(NEW):
        rgb = turn_rgb(banners[OLD.index(source)], degrees, value)
        rows.append(f"banner\t{FIRST_NEW + i}\t" + "\t".join(map(str, rgb)))
    rows.append(f"banner\t{NEUTRAL}\t" + "\t".join(map(str, neutral)))
    at = first or 0
    path.write_text("\n".join(others[:at] + rows + others[at:]) + "\n")
    return len(rows)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--art", type=Path, default=ROOT / "assets-local/art", help="the art folder")
    args = ap.parse_args()
    art = args.art
    if not (art / "map/banners.tsv").exists():
        print(f"{art}/map/banners.tsv is missing (run extract-art first)", file=sys.stderr)
        return 1
    n = tsv_rows(art) + minimap_rows(art) + pictures(art) + gems(art)
    print(f"{n} files or rows written")
    return 0


if __name__ == "__main__":
    sys.exit(main())
