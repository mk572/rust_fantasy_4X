"""Import the original game's prose into local content files for rust_fantasy_4X.

The text import, split off `import_mom.py`. Reads the author's own copy of the original game and
a local clone of the ReMoM reverse-engineering project, and writes only what is not game data:
`content-local/` (city names, event messages, diplomacy lines and building descriptions:
copyrighted prose, gitignored, never to be committed or distributed). Nothing of it is read by the
game or by `f4x-tools`: the game's own texts are `crates/f4x-client/content/text/`. The files are
for comparing the game's text with the original's, and the building descriptions cross-check the
effects in `crates/f4x-def/content/buildings.ron`.

Usage:
    python3 -I scripts/import_mom_text.py [--game DIR] [--remom DIR] [--out DIR] [--rules-out DIR]

Everything the importer cannot map is listed in `<out>/IMPORT_REPORT.txt`
rather than dropped silently.
"""

from __future__ import annotations

import argparse
import logging
import re
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from import_mom import cstr, read_lbx, read_records, ron_str, slug  # noqa: E402

LOG = logging.getLogger("import_mom_text")

REPO = Path(__file__).resolve().parent.parent
DEFAULT_GAME = REPO / "reference/mom-classic/Master of Magic Official Release"
DEFAULT_REMOM = Path.home() / "projects/ReMoM"


# --------------------------------------------------------------------------- #
# Constants taken from the clone's headers and docs
# --------------------------------------------------------------------------- #


RACE_COUNT = 14









# Placeholder "buildings" that are production choices, not structures.
SKIP_BUILDINGS = {"None", "Trade Goods", "Housing"}


# --------------------------------------------------------------------------- #
# Report
# --------------------------------------------------------------------------- #


@dataclass
class Report:
    lines: list[str] = field(default_factory=list)

    def note(self, section: str, text: str) -> None:
        self.lines.append(f"[{section}] {text}")
        LOG.debug("%s: %s", section, text)


# --------------------------------------------------------------------------- #
# Buildings
# --------------------------------------------------------------------------- #


@dataclass
class Building:
    index: int
    name: str
    id: str
    requires: list[int]
    cost: int
    upkeep: int
    replaces: int | None = None  # building index made redundant by this one
    starting_xp: int = 0  # experience units built here start with
    shape: int = 0  # footprint in the city picture: 2x2, 2x3, 3x2, 3x3


def import_buildings(game: Path, report: Report) -> list[Building]:
    records = read_records(read_lbx(game / "BUILDDAT.LBX")[0])
    out = []
    for i, r in enumerate(records):
        name = cstr(r[:20])
        req1, req2, replace = struct.unpack_from("<hhh", r, 20)
        grants_20, grants_60 = struct.unpack_from("<hh", r, 26)
        upkeep = -struct.unpack_from("<h", r, 32)[0]
        cost = struct.unpack_from("<h", r, 44)[0]
        reqs = []
        for req in (req1, req2):
            if req >= 100:  # terrain requirement, not a building
                report.note("buildings", f"{name}: requires terrain type {req - 100}; not modelled")
            elif req > 0:
                reqs.append(req)
        xp = 60 if grants_60 else 20 if grants_20 else 0
        shape = struct.unpack_from("<h", r, 0x30)[0]
        out.append(Building(i, name, slug(name), reqs, cost, max(upkeep, 0),
                            replace if replace > 0 else None, xp, shape if 0 <= shape <= 3 else 0))
    return out


# --------------------------------------------------------------------------- #
# Races and units (tables from the clone)
# --------------------------------------------------------------------------- #


@dataclass
class Race:
    index: int
    id: str
    name: str
    cant_build: list[int]


def import_races(remom: Path) -> list[Race]:
    src = (remom / "MoM/src/RACETYPE.c").read_text(encoding="utf-8", errors="replace")
    names = dict(re.findall(r'race_type_name_(\d+)\[\] = "([^"]*)"', src))
    # Each table row is preceded by its raw bytes in a comment: 12 little-endian words.
    comments = re.findall(r"/\*\s+((?:[0-9A-F]{2}\s+){23}[0-9A-F]{2})\s+\*/", src)
    if len(comments) != RACE_COUNT:
        raise ValueError(f"expected {RACE_COUNT} race rows in RACETYPE.c, found {len(comments)}")
    races = []
    for i, text in enumerate(comments):
        words = struct.unpack("<12h", bytes.fromhex("".join(text.split())))
        count, cant = words[1], words[2 : 2 + 7]
        name = names[f"{i:03d}"]
        races.append(Race(i, slug(name), name, [c for c in cant[:count] if c]))
    return races




def import_city_names(game: Path) -> list[list[str]]:
    """City names per race, in race-table order (20 names of 14 bytes each)."""
    records = read_records(read_lbx(game / "CITYNAME.LBX")[0])
    return [[cstr(rec[i : i + 14]) for i in range(0, len(rec), 14)] for rec in records]


# --------------------------------------------------------------------------- #
# Output
# --------------------------------------------------------------------------- #


# What the data bytes of an `EVENTMSG.LBX` line stand for (`Get_Event_Message`): the client
# fills the names in. $81 and $82 name the city of a marriage, which is the event's city too.
EVENT_TOKENS = {
    0x80: "{city}", 0x81: "{city}", 0x82: "{size}", 0x83: "{number}", 0x89: "{ore}",
    0x8A: "{item}", 0x8B: "{size}", 0x8C: "{player}", 0x8F: "{units}", 0x90: "{people}",
    0x91: "{buildings}", 0x93: "{units_s}", 0x94: "{buildings_s}",
}


def import_event_texts(game: Path) -> list[str]:
    """The event messages (`EVENTMSG.LBX`, 7 lines per event, 200 bytes each) as RON lines."""
    (entry,) = read_lbx(game / "EVENTMSG.LBX")
    lines = []
    for index, record in enumerate(read_records(entry)):
        raw = record.split(b"\0")[0]
        if not raw:
            continue
        text = "".join(EVENT_TOKENS[b] if b >= 0x80 else chr(b) for b in raw)
        lines.append(f"    (event: {index // 7 + 1}, voice: {index % 7}, text: {ron_str(text)}),")
    return lines


def import_building_texts(game: Path, buildings: list[Building]) -> list[tuple[Building, str]]:
    """The original's one-paragraph description of each building (BUILDESC.LBX entry 0, 200
    bytes a record, indexed like BUILDDAT). Kept local and used only to check our numbers; the
    game shows its own descriptions (`content/text/en/building.ron`)."""
    records = read_records(read_lbx(game / "BUILDESC.LBX")[0])
    out = []
    for b in buildings:
        text = " ".join(cstr(records[b.index]).split()) if b.index < len(records) else ""
        if text:
            out.append((b, text))
    return out


# What a description's figure means: (pattern, our effect stat, sign of our value, op).
DESCRIPTION_CLAIMS = (
    (r"\+(\d+) spell research", "CityResearch", 1, "Add"),
    (r"\+(\d+) mana(?! points a turn)", "CityMana", 1, "Add"),
    (r"upkeep cost of (\d+) mana", "CityMana", -1, "Add"),
    (r"unrest(?: level)? by (\d+)", "CityUnrest", -1, "Add"),
    (r"revenues? of (?:the|a) city by \+?(\d+)%", "CityGold", 1, "Percent"),
    (r"production(?: points of a city)? by \+(\d+)%", "CityProduction", 1, "Percent"),
    (r"production of forest squares by (\d+)%", "CityProduction", 1, "Percent"),
    (r"and production by \+(\d+)%", "CityProduction", 1, "Percent"),
    (r"\+(\d+) food(?! and)", "CityFoodFlat", 1, "Add"),
    (r"harvest by \+(\d+)", "CityFood", 1, "Add"),
    (r"\+(\d+) people per turn", "CityGrowth", 1, "Add"),
)


def cross_check_building_texts(texts: list[tuple[Building, str]], rules_out: Path,
                               report: Report) -> int:
    """Compare the figures the original's descriptions state with the effects in our
    `buildings.ron`; each difference is a `check` note in the report."""
    lines = {}
    for line in (rules_out / "buildings.ron").read_text(encoding="utf-8").splitlines():
        m = re.search(r'\(id: "([a-z0-9_]+)"', line)
        if m:
            lines[m.group(1)] = line
    found = 0
    for b, text in texts:
        line = lines.get(b.id)
        if line is None:
            continue
        effects = re.findall(r"stat: (\w+), op: (\w+), value: (-?\d+)", line)
        for pattern, stat, sign, op in DESCRIPTION_CLAIMS:
            m = re.search(pattern, text)
            if not m:
                continue
            ours = sum(int(v) for (st, o, v) in effects if st == stat and o == op) * sign
            if ours != int(m.group(1)):
                found += 1
                report.note("check", f"building {b.name}: {stat} {ours} in our data, "
                                     f"description says {m.group(1)} ({text!r})")
    return found


# What the control bytes of a `DIPLOMSG.LBX` line stand for (`Get_Diplomacy_Statement`, `code & 0x7F`);
# the client fills the names in. Code 6 is not used by any line.
DIPLOMSG_TOKENS = {
    0: "{player}", 1: "{wizard}", 2: "{third}", 3: "{city}", 4: "{size}", 5: "{other}",
    7: "{victim}", 8: "{gold}", 9: "{wanted}", 10: "{gold} gold", 11: "{broken_treaty}",
    12: "{spell}", 13: "{unit}", 14: "{article}", 15: "{treaty}", 16: "{year}", 17: "{gift}",
    18: "{target}", 19: "{third_other}", 20: "{number}",
}


def import_diplomacy_lines(game: Path, report: Report) -> list[str]:
    """The wizards' lines (`DIPLOMSG.LBX`): entry 0 has, for each situation, a record of
    three shorts whose last is the number of variants; entry 1 holds 15 lines of 200 bytes per
    situation. Control bytes become `{placeholders}`; a line break inside a line is a space."""
    entries = read_lbx(game / "DIPLOMSG.LBX")
    situations = read_records(entries[0])
    body = entries[1]
    count, size = struct.unpack_from("<HH", body, 0)
    out = []
    for number, record in enumerate(situations):
        variants = struct.unpack_from("<3h", record, 0)[2]
        lines = []
        for v in range(min(variants, 15)):
            at = 4 + (number * 15 + v) * size
            if at + size > len(body):
                report.note("diplomacy", f"situation {number} variant {v} is outside the file")
                break
            text = ""
            for byte in body[at : at + size].split(b"\0")[0]:
                if byte < 128:
                    text += chr(byte)
                elif (byte & 0x7F) in DIPLOMSG_TOKENS:
                    text += DIPLOMSG_TOKENS[byte & 0x7F]
                else:
                    text += chr(byte)
            lines.append(ron_str(" ".join(text.split())))
        if lines:
            out.append(f"    (situation: {number}, lines: [{', '.join(lines)}]),")
    return out


def write_list(path: Path, header: str, lines: list[str]) -> None:
    path.write_text(f"// {header}\n[\n" + "\n".join(lines) + "\n]\n", encoding="utf-8")














# Rules data goes to the committed content directory; the rest stays local.
RULES_FILES = ("races", "units", "buildings", "abilities", "realms", "spells", "items",
               "level_paths", "rules")


def run(game: Path, remom: Path, out: Path, rules_out: Path) -> Report:
    report = Report()
    out.mkdir(parents=True, exist_ok=True)
    rules_out.mkdir(parents=True, exist_ok=True)

    races = import_races(remom)
    city_names = import_city_names(game)
    if len(city_names) < len(races):
        raise ValueError(f"CITYNAME.LBX has {len(city_names)} races, expected {len(races)}")
    buildings = [b for b in import_buildings(game, report) if b.name not in SKIP_BUILDINGS]


    local_header = "Generated by scripts/import_mom_text.py from the original game data. Do not commit."
    # The original's city names are IP: they stay local, as the client's name table
    # `text/en/name/city.ron` (by race number), which replaces the invented one in
    # `crates/f4x-client/content/` as a whole when present.
    city_file = out / "text" / "en" / "name" / "city.ron"
    city_file.parent.mkdir(parents=True, exist_ok=True)
    city_file.write_text(
        f"// {local_header}\n{{\n"
        + "\n".join(
            f"    {race.index}: [{', '.join(ron_str(n) for n in city_names[race.index] if n)}],"
            for race in races
        )
        + "\n}\n",
        encoding="utf-8",
    )
    write_list(out / "event_texts.ron", local_header, import_event_texts(game))
    write_list(out / "diplomsg.ron", local_header, import_diplomacy_lines(game, report))
    building_texts = import_building_texts(game, buildings)
    write_list(out / "building_texts.ron", local_header, [
        f"    (building: {ron_str(b.id)}, text: {ron_str(t)})," for b, t in building_texts
    ])
    LOG.info("building description cross-check: %d mismatches",
             cross_check_building_texts(building_texts, rules_out, report))
    # The importer reads no help: the help text and the screens' help areas are the client's own
    # committed files (`crates/f4x-client/content/`), edited by hand. A copy an older run left in
    # the local folder is removed, so it cannot shadow them.
    (out / "help_screens.ron").unlink(missing_ok=True)
    # Older runs wrote the rules files (and copies of terrain and features) here; the
    # committed ones in `rules_out` now apply, and a stale copy would shadow them.
    for stale in (*RULES_FILES, "terrain", "features", "wizards", "messages"):
        old = out / f"{stale}.ron"
        if old.exists() and old.resolve() != (rules_out / f"{stale}.ron").resolve():
            old.unlink()
            LOG.info("removed stale %s", old)

    summary = (
        f"races {len(races)} and buildings {len(buildings)} (checked only)"
    )
    (out / "IMPORT_REPORT.txt").write_text(summary + "\n\n" + "\n".join(report.lines) + "\n", encoding="utf-8")
    LOG.info("imported: %s (%d notes in IMPORT_REPORT.txt)", summary, len(report.lines))
    return report


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--game", type=Path, default=DEFAULT_GAME)
    parser.add_argument("--remom", type=Path, default=DEFAULT_REMOM)
    parser.add_argument("--out", type=Path, default=REPO / "content-local",
                        help="local-only output (city names, event texts, report)")
    parser.add_argument("--rules-out", type=Path, default=REPO / "crates/f4x-def/content",
                        help="committed rules data (races, units, buildings, spells, ...)")
    args = parser.parse_args(argv)
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    run(args.game, args.remom, args.out, args.rules_out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
