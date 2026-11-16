#!/usr/bin/env python3
"""Task 570, once: bring an `assets-local/` extracted before 17 November 2026 to the new layout of the
wizards' gems, touching only the pictures that move (no full unpack and pack):

- the original's finished gems (`lilwiz`, 14 wizards x 5 colours, and the five newer colours that
  `make_banner_colours.py` made) leave the wizards' sheets for `assets-unused/art/lilwiz/NNN_FF.png`;
- the five base diamonds (`magic` 1 to 5) go to `assets-unused/art/magic/`: the client keeps its own
  copy (`crates/f4x-client/assets/gem/`, `make_gem_frames.py`);
- the wizards' portraits (`magic` 18 to 31) go to the wizards' sheets as `face_00.png` (and the colour
  cycle mask `face_00_cycle.png`), so `art/magic.sheet.png` is smaller;
- the wizards fading into the mirror (`diplomac` 38 to 51, five frames each) go from `screens/diplomacy/`
  to the wizards' sheets as `appear_00.png` to `appear_04.png`;
- the rows of `index.tsv` follow.

It unpacks the 14 wizard folders and `art/magic` into loose files and drops their rows of `packs.tsv`;
then `f4x-extract pack` (only folders with loose files are packed) builds the sheets again. The
tables are `crates/f4x-client/content/wizard_art.ron` (read as text). A backup of what it changes goes
to the folder given with `--backup`.

    .venv/bin/python scripts/migrate_gem_art.py --backup DIR [--dry-run]
    target/release/f4x-extract pack assets-local
Needs Pillow (.venv). Running it on art that is already migrated changes nothing.
"""
import argparse
import re
import shutil
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
LOCAL = ROOT / "assets-local"
UNUSED = ROOT / "assets-unused"
KEYWORD = "f4x-sheet"
WIZARD_RON = ROOT / "crates/f4x-client/content/wizard_art.ron"
OLD_GEMS_PER_WIZARD = 5
NEW_COLOURS_AT = 100


def wizards():
    """(id, face entry, appear entry) for each wizard in the order of the table."""
    found = []
    for line in WIZARD_RON.read_text().splitlines():
        m = re.match(r'\s*\(id: "(\w+)", pictures: \((.*)\)\)', line)
        if not m:
            continue
        number = lambda key: int(re.search(rf"\b{key}: (\d+)", m.group(2)).group(1))
        found.append((m.group(1), number("face"), number("appear")))
    return found


def read_table():
    table = {}
    for line in (LOCAL / "packs.tsv").read_text().splitlines():
        folder, _, sheet = line.partition("\t")
        table.setdefault(folder, []).append(sheet.strip())
    return table


def write_table(table):
    lines = [f"{folder}\t{sheet}" for folder, sheets in table.items() for sheet in sheets]
    (LOCAL / "packs.tsv").write_text("\n".join(lines) + "\n")


def read_sheets(files):
    """name -> RGBA image of every picture on the sheets (the grid of `f4x_extract::pack`)."""
    pictures = {}
    for file in files:
        sheet = Image.open(LOCAL / file)
        rgba = sheet.convert("RGBA")
        lines = sheet.text[KEYWORD].split("\n")
        cell_w, cell_h = map(int, lines[0].removeprefix("cell ").split("x"))
        columns = rgba.width // cell_w
        for i, line in enumerate(lines[1:]):
            name, w, h = line.split("\t")
            x, y = i % columns * cell_w, i // columns * cell_h
            pictures[name] = rgba.crop((x, y, x + int(w), y + int(h)))
    return pictures


def gem_entry(gem_name, slot):
    """The `lilwiz` entry of a picture `gem_<colour>_<frame>[_cycle].png` of the wizard in `slot`."""
    colour = int(gem_name.split("_")[1])
    first = slot * OLD_GEMS_PER_WIZARD
    return first + colour if colour < OLD_GEMS_PER_WIZARD else NEW_COLOURS_AT + first + colour - OLD_GEMS_PER_WIZARD


def save(image, path, dry):
    if not dry:
        path.parent.mkdir(parents=True, exist_ok=True)
        image.save(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--backup", required=True, help="a folder for copies of what changes")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    backup, dry = Path(args.backup), args.dry_run
    table = read_table()
    roster = wizards()
    folders = [f"art/wizards/{wid}" for wid, *_ in roster]
    if not any(f in table for f in folders):
        print("the wizards' sheets are gone or already migrated; nothing to do")
        return 0
    if not dry:
        backup.mkdir(parents=True, exist_ok=True)
        for rel in [sheet for f in folders + ["art/magic"] for sheet in table.get(f, [])] + ["packs.tsv", "index.tsv"]:
            target = backup / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(LOCAL / rel, target)
        for path in sorted((LOCAL / "screens/diplomacy").glob("diplomac_0[345]*_0?.png")):
            shutil.copy2(path, backup / path.name)
    index = {}
    for line in (LOCAL / "index.tsv").read_text().splitlines():
        lbx, entry, place = line.split("\t")
        index[(lbx, int(entry))] = place
    face_owner = {face: wid for wid, face, _ in roster}
    # The wizards' folders: everything but the finished gems stays, as loose files for the pack.
    moved_gems = 0
    for slot, (wid, face, appear) in enumerate(roster):
        folder = f"art/wizards/{wid}"
        for name, image in read_sheets(table.pop(folder, [])).items():
            if name.startswith("gem_"):
                entry = gem_entry(name, slot)
                suffix = name.removeprefix(f"gem_{name.split('_')[1]}_")
                save(image, UNUSED / f"art/lilwiz/{entry:03d}_{suffix}", dry)
                moved_gems += 1
            else:
                save(image, LOCAL / folder / name, dry)
        for old in range(OLD_GEMS_PER_WIZARD):
            index.pop(("lilwiz", slot * OLD_GEMS_PER_WIZARD + old), None)
        for colour in range(OLD_GEMS_PER_WIZARD * 2):
            entry = gem_entry(f"gem_{colour}_00", slot)
            index[("lilwiz", entry)] = f"../assets-unused/art/lilwiz/{entry:03d}"
        index[("diplomac", appear)] = f"art/wizards/{wid}/appear"
        for frame in range(5):
            source = LOCAL / f"screens/diplomacy/diplomac_{appear:03d}_{frame:02d}.png"
            if source.exists() and not dry:
                shutil.move(source, LOCAL / folder / f"appear_{frame:02d}.png")
    # `magic`: the diamonds and the portraits leave, the rest stays as loose files for the pack.
    for name, image in read_sheets(table.pop("art/magic", [])).items():
        entry = int(name[:3])
        if 1 <= entry <= 5:
            save(image, UNUSED / "art/magic" / name, dry)
            index[("magic", entry)] = f"../assets-unused/art/magic/{entry:03d}"
        elif entry in face_owner:
            save(image, LOCAL / f"art/wizards/{face_owner[entry]}" / name.replace(f"{entry:03d}", "face"), dry)
            index[("magic", entry)] = f"art/wizards/{face_owner[entry]}/face"
        else:
            save(image, LOCAL / "art/magic" / name, dry)
    if not dry:
        for sheet in [s for f in folders + ["art/magic"] for s in read_table().get(f, [])]:
            (LOCAL / sheet).unlink(missing_ok=True)
        write_table(table)
        (LOCAL / "index.tsv").write_text("".join(f"{lbx}\t{entry}\t{place}\n" for (lbx, entry), place in index.items()))
    print(f"{moved_gems} finished gems to assets-unused/art/lilwiz; now run `f4x-extract pack assets-local`")
    return 0


if __name__ == "__main__":
    sys.exit(main())
