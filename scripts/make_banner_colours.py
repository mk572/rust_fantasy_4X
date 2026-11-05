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
  (`wizards/<id>/gem_5` to `gem_9`, entries 100 + the wizard's first gem + the new colour's number
  from 0 of `lilwiz`; in `lilwiz/100..169_00.png` for art that is not filed under the wizards yet).
Only the pixels that differ between the original's five colours are turned, so the shading stays.
Running it again gives the same files. Run it after `f4x-extract art` (and `grey_neutral.py`, in either
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


def read_index(art):
    """`index.tsv` beside the art folder: (lbx, entry) -> where `f4x-extract` filed the picture."""
    path = art.parent / "index.tsv"
    index = {}
    if path.exists():
        for line in path.read_text().splitlines():
            f = line.split("\t")
            if len(f) >= 3 and f[1].isdigit():
                index[(f[0], int(f[1]))] = f[2]
    return index


def write_index(art, index):
    rows = sorted(index.items())
    text = "".join(f"{lbx}\t{entry}\t{place}\n" for (lbx, entry), place in rows)
    (art.parent / "index.tsv").write_text(text)


def gems(art):
    """The wizards' gems (`lilwiz`, the wizard's first gem + the colour) in the new colours.

    The originals are filed under their wizard (`wizards/<id>/gem_<colour>`, `f4x-extract art` and
    `refile`); the new ones go beside them as `gem_5` to `gem_9` and into `index.tsv`. Art that
    is not filed yet keeps them in `lilwiz/`, which `f4x-extract refile` files later."""
    index = read_index(art)
    folder = art / "lilwiz"
    count = 0
    for portrait in range(PORTRAITS):
        entries = [portrait * 5 + b for b in range(5)]
        places = [index.get(("lilwiz", e)) for e in entries]
        if all(places):
            paths = [art.parent / f"{p}_00.png" for p in places]
            new_paths = [paths[0].with_name(f"gem_{FIRST_NEW + i}_00.png") for i in range(len(NEW))]
            new_places = [places[0].rsplit("/", 1)[0] + f"/gem_{FIRST_NEW + i}" for i in range(len(NEW))]
        else:
            paths = [folder / f"{e:03d}_00.png" for e in entries]
            new_paths = [folder / f"{100 + portrait * 5 + i:03d}_00.png" for i in range(len(NEW))]
            new_places = [None] * len(NEW)
        if not all(p.exists() for p in paths):
            continue
        images = [Image.open(p) for p in paths]
        if not same_size(images):
            continue
        for i, (name, source, degrees, value) in enumerate(NEW):
            base = OLD.index(source)
            recolour(images, base, degrees, value).save(new_paths[i])
            cycle = paths[base].with_name(paths[base].stem + "_cycle.png")
            if cycle.exists():
                shutil.copyfile(cycle, new_paths[i].with_name(new_paths[i].stem + "_cycle.png"))
            if new_places[i]:
                index[("lilwiz", 100 + portrait * 5 + i)] = new_places[i]
            count += 1
    write_index(art, index)
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
        print(f"{art}/map/banners.tsv is missing (run f4x-extract art first)", file=sys.stderr)
        return 1
    n = tsv_rows(art) + minimap_rows(art) + pictures(art) + gems(art)
    print(f"{n} files or rows written")
    return 0


if __name__ == "__main__":
    sys.exit(main())
