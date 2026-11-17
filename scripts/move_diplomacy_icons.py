#!/usr/bin/env python3
"""Move the diplomacy screen's small pictures into one folder, `screens/diplomacy/icons/` (task 489).

The folder is packed as `screens/diplomacy/icons.sheet.png`. It holds `diplomac` 2 to 23 and the
treaty symbols of `magic` 60 to 74 (the scroll, peace sign and crossed swords in the five banner
colours). `screens.tsv` of the extractor now places them there, so a fresh `f4x-extract art` puts them
right; this script does the same to art already extracted, touching only what moves:

1. `magic` 60 to 74 are cut out of `assets-local/art/magic.sheet.png` into loose files
   `icons/magic_060_00.png` ... and the sheet is written again without them (same grid, the
   pictures that stay in their order, `f4x-sheet` text chunk kept);
2. `screens/diplomacy/diplomac_002_00.png` ... `_023_00.png` move to `icons/`;
3. `assets-local/index.tsv` gets the new paths (the `diplomac` rows 2 to 23 change, `magic` rows 60
   to 74 are added after row 59).

Then pack the folder alone (no pass over every loose picture):

    .venv/bin/python scripts/move_diplomacy_icons.py
    target/debug/f4x-extract pack assets-local --only screens/diplomacy/icons

Safe to run again. Needs Pillow (`.venv`).
"""
from pathlib import Path

from PIL import Image, PngImagePlugin

ROOT = Path(__file__).resolve().parent.parent / "assets-local"
SHEET = ROOT / "art/magic.sheet.png"
FOLDER = ROOT / "screens/diplomacy/icons"
MAGIC = range(60, 75)
DIPLOMAC = range(2, 24)


def cut_magic():
    src = Image.open(SHEET)
    lines = src.text["f4x-sheet"].splitlines()
    cw, ch = map(int, lines[0].split()[1].split("x"))
    pictures = [(l.split("\t")[0], int(l.split("\t")[1]), int(l.split("\t")[2])) for l in lines[1:]]
    im = src.convert("RGBA")
    cols = im.width // cw
    FOLDER.mkdir(parents=True, exist_ok=True)
    keep, moved = [], 0
    for k, (name, w, h) in enumerate(pictures):
        x, y = (k % cols) * cw, (k // cols) * ch
        crop = im.crop((x, y, x + w, y + h))
        if int(name.split("_")[0]) in MAGIC:
            crop.save(FOLDER / f"magic_{name}")
            moved += 1
        else:
            keep.append((name, w, h, crop))
    if not moved:
        return 0
    rows = -(-len(keep) // cols)
    out = Image.new("RGBA", (cols * cw, rows * ch), (0, 0, 0, 0))
    for k, (_, _, _, crop) in enumerate(keep):
        out.paste(crop, ((k % cols) * cw, (k // cols) * ch))
    text = f"cell {cw}x{ch}\n" + "\n".join(f"{n}\t{w}\t{h}" for n, w, h, _ in keep)
    info = PngImagePlugin.PngInfo()
    info.add_text("f4x-sheet", text)
    out.save(SHEET, pnginfo=info)
    return moved


def move_diplomac():
    n = 0
    for e in DIPLOMAC:
        f = ROOT / f"screens/diplomacy/diplomac_{e:03d}_00.png"
        if f.exists():
            FOLDER.mkdir(parents=True, exist_ok=True)
            f.rename(FOLDER / f.name)
            n += 1
    return n


def index():
    path = ROOT / "index.tsv"
    out, added = [], False
    have = {l.split("\t")[1] for l in path.read_text().splitlines() if l.startswith("magic\t")}
    for line in path.read_text().splitlines():
        f = line.split("\t")
        if f[0] == "diplomac" and f[1].isdigit() and int(f[1]) in DIPLOMAC:
            line = f"diplomac\t{f[1]}\tscreens/diplomacy/icons/diplomac_{int(f[1]):03d}"
        out.append(line)
        if f[0] == "magic" and f[1] == "59" and not added:
            added = True
            out += [f"magic\t{e}\tscreens/diplomacy/icons/magic_{e:03d}" for e in MAGIC if str(e) not in have]
    path.write_text("\n".join(out) + "\n")


if __name__ == "__main__":
    print(f"{cut_magic()} magic symbols cut out, {move_diplomac()} diplomac pictures moved")
    index()
