"""Import Master of Magic data into local content files for rust_fantasy_4X.

Reads the author's own copy of the original game (buildings, spells, items and
spell help text from the `.LBX` files) and the unit and race tables from a local
clone of the ReMoM reverse-engineering project (those tables live in the game
executable, which is not parsed directly). Writes RON files into
`content-local/`, which is gitignored: the output is derived from copyrighted
data and must never be committed or distributed.

Usage:
    python3 -I scripts/import_mom.py [--game DIR] [--remom DIR] [--out DIR]

Everything the importer cannot map is listed in `<out>/IMPORT_REPORT.txt`
rather than dropped silently.
"""

from __future__ import annotations

import argparse
import logging
import re
import shutil
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

LOG = logging.getLogger("import_mom")

REPO = Path(__file__).resolve().parent.parent
DEFAULT_GAME = REPO / "reference/mom-classic/Master of Magic Official Release"
DEFAULT_REMOM = REPO / "reference/ReMoM"


# --------------------------------------------------------------------------- #
# Reading the original files
# --------------------------------------------------------------------------- #


def read_lbx(path: Path) -> list[bytes]:
    """Return the entries of an LBX archive.

    Raises:
        ValueError: if the file does not have the LBX magic number.
    """
    data = path.read_bytes()
    count, magic = struct.unpack_from("<HH", data, 0)
    if magic != 0xFEAD:
        raise ValueError(f"not an LBX file (magic {magic:#06x}): {path}")
    offsets = struct.unpack_from(f"<{count + 1}I", data, 8)
    return [data[offsets[i] : offsets[i + 1]] for i in range(count)]


def read_records(entry: bytes) -> list[bytes]:
    """Split a data-table entry (`u16 count, u16 size, records...`) into records."""
    count, size = struct.unpack_from("<HH", entry, 0)
    return [entry[4 + i * size : 4 + (i + 1) * size] for i in range(count)]


def cstr(raw: bytes) -> str:
    return raw.split(b"\0")[0].decode("latin1")


def slug(name: str) -> str:
    """Lowercase `[a-z0-9_]` id as the content validator requires."""
    s = re.sub(r"[^a-z0-9]+", "_", name.lower().replace("'", ""))
    return s.strip("_")


def ron_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


# --------------------------------------------------------------------------- #
# Constants taken from the clone's headers and docs
# --------------------------------------------------------------------------- #

REALMS = [  # index = realm byte in the spell table
    ("nature", "Nature", (60, 160, 60)),
    ("sorcery", "Sorcery", (60, 110, 220)),
    ("chaos", "Chaos", (210, 70, 50)),
    ("life", "Life", (235, 235, 225)),
    ("death", "Death", (130, 60, 170)),
    ("arcane", "Arcane", (190, 190, 190)),
]

# Spells a wizard starts with when none are chosen: per realm the ten common spells in
# the original's default order (a realm with n books takes the first n - 1).
DEFAULT_START_SPELLS = {
    "nature": ["War Bears", "Stone Skin", "Sprites", "Water Walking", "Giant Strength", "Web",
               "Earth to Mud", "Wall of Stone", "Resist Elements", "Earth Lore"],
    "sorcery": ["Nagas", "Psionic Blast", "Phantom Warriors", "Floating Island", "Confusion",
                "Counter Magic", "Word of Recall", "Dispel Magic True", "Resist Magic",
                "Guardian Wind"],
    "chaos": ["Fire Bolt", "Fire Elemental", "Eldritch Weapon", "Hell Hounds", "Corruption",
              "Warp Creature", "Shatter", "Wall of Fire", "Disrupt", "Warp Wood"],
    "life": ["Heroism", "Guardian Spirit", "Holy Armor", "Just Cause", "Healing", "Holy Weapon",
             "Star Fires", "Endurance", "True Light", "Bless"],
    "death": ["Life Drain", "Ghouls", "Weakness", "Dark Rituals", "Black Sleep", "Darkness",
              "Terror", "Skeletons", "Mana Leak", "Cloak of Fear"],
}

# The ready-made wizards (NewGame.c `_wizard_presets_table`): books in the order life,
# sorcery, nature, death, chaos, and the retort; help topic 659 + position describes them.
WIZARD_PRESETS = [
    ("Merlin", (5, 0, 5, 0, 0), "SageMaster"),
    ("Raven", (0, 6, 5, 0, 0), None),
    ("Sharee", (0, 0, 0, 5, 5), "Conjurer"),
    ("Lo Pan", (0, 5, 0, 0, 5), "Channeler"),
    ("Jafar", (0, 10, 0, 0, 0), "Alchemy"),
    ("Oberic", (0, 0, 5, 0, 5), "ManaFocusing"),
    ("Rjak", (0, 0, 0, 9, 0), "InfernalPower"),
    ("Sss'ra", (4, 0, 0, 0, 4), "Myrran"),
    ("Tauron", (0, 0, 0, 0, 10), "ChaosMastery"),
    ("Freya", (0, 0, 10, 0, 0), "NatureMastery"),
    ("Horus", (5, 5, 0, 0, 0), "Archmage"),
    ("Ariel", (10, 0, 0, 0, 0), "Charismatic"),
    ("Tlaloc", (0, 0, 4, 5, 0), "Warlord"),
    ("Kali", (0, 5, 0, 5, 0), "Artificer"),
]
WIZARD_REALM_ORDER = ["life", "sorcery", "nature", "death", "chaos"]
WIZARD_HELP_FIRST = 659


def wizard_lines(help_topics) -> list[str]:
    by_index = {t.index: t for t in help_topics}
    lines = []
    for i, (name, books, retort) in enumerate(WIZARD_PRESETS):
        book_list = ", ".join(
            f"({ron_str(r)}, {n})" for r, n in zip(WIZARD_REALM_ORDER, books) if n
        )
        topic = by_index.get(WIZARD_HELP_FIRST + i)
        text = topic.text if topic and topic.title.lower() == name.lower() else ""
        retorts = f"[{retort}]" if retort else "[]"
        lines.append(
            f"    (id: {ron_str(slug(name))}, name: {ron_str(name)}, description: {ron_str(text)}, "
            f"books: [{book_list}], retorts: {retorts}),"
        )
    return lines


# Unit table: first and last index of each block.
HERO_RANGE = range(0, 35)
BOAT_RANGE = range(35, 39)  # boats and the catapult: race-neutral
NORMAL_RANGE = range(39, 154)
SUMMONED_RANGE = range(154, 198)

RACE_COUNT = 14
# Units 154..197 take their "race" byte from this block of the race enum.
REALM_BY_RACE_BYTE = {15: "arcane", 16: "nature", 17: "sorcery", 18: "chaos", 19: "life", 20: "death"}

RANGED_TYPES = {
    10: ("rock_throwing", "Rock Throwing"),
    11: ("cannon", "Cannon"),
    20: ("bow", "Bow"),
    21: ("sling", "Sling"),
    30: ("lightning_bolt", "Lightning Bolt"),
    31: ("fireball_attack", "Fireball"),
    32: ("sorcery_bolt", "Sorcery Bolt"),
    33: ("death_bolt", "Death Bolt"),
    34: ("ice_bolt", "Ice Bolt"),
    35: ("shaman_bolt", "Shaman Bolt"),
    36: ("drow_bolt", "Drow Bolt"),
    37: ("sprite_bolt", "Sprite Bolt"),
    38: ("nature_bolt", "Nature Bolt"),
    100: ("thrown", "Thrown"),
    101: ("fire_breath", "Fire Breath"),
    102: ("lightning_breath", "Lightning Breath"),
    103: ("stoning_gaze", "Stoning Gaze"),
    104: ("multi_gaze", "Multiple Gaze"),
    105: ("death_gaze", "Death Gaze"),
}

# (mask, id, name, description) per flag word of the unit table.
ATTRIB_1_FLAGS = [
    (0x0001, "fire_immunity", "Fire Immunity", "Takes no damage from fire attacks."),
    (0x0002, "stoning_immunity", "Stoning Immunity", "Cannot be turned to stone."),
    (0x0004, "missile_immunity", "Missile Immunity", "Takes no damage from normal missiles."),
    (0x0008, "illusion_immunity", "Illusion Immunity", "Sees through illusions."),
    (0x0010, "cold_immunity", "Cold Immunity", "Takes no damage from cold attacks."),
    (0x0020, "magic_immunity", "Magic Immunity", "Cannot be affected by spells."),
    (0x0040, "death_immunity", "Death Immunity", "Immune to death magic."),
    (0x0080, "poison_immunity", "Poison Immunity", "Immune to poison."),
    (0x0100, "weapon_immunity", "Weapon Immunity", "Only magical attacks hurt it."),
    (0x0200, "flying", "Flying", "Moves over any terrain."),
    (0x0400, "lucky", "Lucky", "Fortune favours this unit."),
    (0x0800, "summon_demon", "Creature Summoning", "Can summon demons to fight beside it."),
    (0x2000, "spellcaster", "Spell Caster", "Casts spells in battle."),
    (0x4000, "spellcaster_major", "Major Spell Caster", "Casts spells in battle, with more mana."),
]
ATTRIB_2_FLAGS = [
    (0x0001, "casts_healing", "Healing Spell", "Casts Healing in battle."),
    (0x0002, "casts_fireball", "Fireball Spell", "Casts Fireball in battle."),
    (0x0004, "casts_doom_bolt", "Doom Bolt Spell", "Casts Doom Bolt in battle."),
    (0x0008, "immolation", "Immolation", "Burns attackers in melee."),
    (0x0010, "casts_web", "Web Spell", "Casts Web in battle."),
    (0x0020, "cause_fear", "Cause Fear", "Enemy figures may flee in fear."),
    (0x0040, "resist_all", "Resist All", "Resists every kind of elemental magic."),
    (0x0080, "holy_bonus", "Holy Bonus", "Strengthened by holy magic."),
]
ABILITY_FLAGS = [
    (0x0002, "large_shield", "Large Shield", "Extra defence against missiles."),
    (0x0004, "planar_travel", "Planar Travel", "Can shift between Arcanus and Myrror."),
    (0x0008, "wall_crusher", "Wall Crusher", "Can breach city walls."),
    (0x0010, "healer", "Healer", "Heals friendly units each turn."),
    (0x0020, "create_outpost", "Create Outpost", "Can found cities."),
    (0x0040, "invisibility", "Invisibility", "Cannot be seen by ordinary units."),
    (0x0080, "create_undead", "Create Undead", "Raises the dead as undead."),
    (0x0100, "long_range", "Long Range", "Ranged attacks reach farther."),
    (0x0200, "land_corruption", "Land Corruption", "Corrupts the land around it."),
    (0x0400, "meld", "Meld", "Can merge with a node."),
    (0x0800, "non_corporeal", "Non-Corporeal", "Passes through walls; resists physical harm."),
    (0x1000, "wind_walking", "Wind Walking", "Carries a stack through the air."),
    (0x2000, "regeneration", "Regeneration", "Heals fully each turn."),
    (0x4000, "purify", "Purify", "Cleanses corrupted land."),
    (0x8000, "negate_first_strike", "Negate First Strike", "Cancels enemy first strike."),
]
ATTACK_FLAGS = [
    (0x0001, "armor_piercing", "Armour Piercing", "Halves the target's defence."),
    (0x0002, "first_strike", "First Strike", "Strikes before the defender can answer."),
    (0x0004, "poison_touch", "Poison Touch", "Poisons what it hits."),
    (0x0008, "life_steal", "Life Steal", "Drains life from its victims."),
    (0x0010, "doom_damage", "Doom Damage", "Ignores defence entirely."),
    (0x0020, "destruction", "Destruction", "Destroys what it hits outright."),
    (0x0040, "illusion_attack", "Illusion", "Cannot be blocked by those who cannot see it."),
    (0x0080, "stoning_touch", "Stoning Touch", "Turns figures to stone."),
    (0x0200, "death_touch", "Death Touch", "Kills figures outright."),
    (0x0400, "power_drain", "Power Drain", "Drains the target's mana."),
    (0x0800, "dispel_evil", "Dispel Evil", "Banishes evil creatures."),
    (0x1000, "area_attack", "Area Attack", "Hits every figure of the target."),
    (0x2000, "damage_limit", "Damage Limit", "Damage is capped by the attacker's hits."),
    (0x4000, "eldritch_weapon", "Eldritch Weapon", "Reduces the defender's chance to block."),
    (0x8000, "warp_lightning", "Warp Lightning", "A chain of lightning bolts."),
]
MOVE_FLAG_ABILITIES = [
    (0x0001, "cavalry", "Cavalry", "Mounted: fast on open ground."),
    (0x0020, "forester", "Forester", "Crosses forests at no extra cost."),
    (0x0040, "mountaineer", "Mountaineer", "Crosses mountains and hills at no extra cost."),
    (0x0010, "teleport", "Teleport", "Moves instantly within a battle."),
    (0x0080, "merging", "Merging", "Merges with other units of its kind."),
]

# Race traits from the original code (CITYCALC.c) and help text, by race id.
RACE_TRAITS = {
    "halfling": "farmer_food: 3",
    "dwarven": "worker_production_x2: 6, double_mining: true",
    "klackon": "worker_production_x2: 6",
    "dark_elf": "mana_per_citizen_x2: 2",
    "high_elf": "mana_per_citizen_x2: 1",
    "beastmen": "mana_per_citizen_x2: 1",
    "draconian": "mana_per_citizen_x2: 1",
    "nomad": "trade_percent: 50",
}

# Unit-table columns, in file order.
UNIT_COLUMNS = (
    "name melee ranged ranged_type ammo to_hit defense resist moves cost upkeep race "
    "bld1 bld2 pict hits sight transport figures construction special "
    "move_flags attr1 attr2 abilities attack_flags sound"
).split()

# Normal units stop at Elite by experience; Ultra-Elite and Champion need the Warlord
# ability / Crusade spell (help topics 530-532), which a level path cannot express.
LEVELS_NORMAL = [("Recruit", 0), ("Regular", 20), ("Veteran", 60), ("Elite", 120)]
LEVELS_HERO = [
    ("Hero", 0), ("Myrmidon", 20), ("Captain", 60), ("Commander", 120), ("Champion", 200),
    ("Lord", 300), ("Grand Lord", 450), ("Super Hero", 600), ("Demi-God", 1000),
]

# Building effects, by building name. Values follow the clone's building notes.
def _mod(stat: str, op: str, value: int) -> str:
    return f"(effect: Modify(stat: {stat}, op: {op}, value: {value}))"


# Building effects, by building name, from the original help text (topics 716-748).
BUILDING_EFFECTS = {
    "Marketplace": [_mod("CityGold", "Percent", 50)],
    "Bank": [_mod("CityGold", "Percent", 50)],
    "Merchants Guild": [_mod("CityGold", "Percent", 100)],
    "Library": [_mod("CityResearch", "Add", 2)],
    "Sages Guild": [_mod("CityResearch", "Add", 3)],
    "University": [_mod("CityResearch", "Add", 5)],
    "Wizards Guild": [_mod("CityResearch", "Add", 8), _mod("CityMana", "Add", 3)],
    "Oracle": [_mod("CityUnrest", "Add", -2), _mod("CitySight", "Min", 4)],
    "City Walls": [_mod("CitySight", "Min", 3)],
    "Shrine": [_mod("CityMana", "Add", 1), _mod("CityUnrest", "Add", -1)],
    "Temple": [_mod("CityMana", "Add", 2), _mod("CityUnrest", "Add", -1)],
    "Parthenon": [_mod("CityMana", "Add", 3), _mod("CityUnrest", "Add", -1)],
    "Cathedral": [_mod("CityMana", "Add", 4), _mod("CityUnrest", "Add", -1)],
    # Granary and market food arrives after the farmland limit is applied.
    "Granary": [_mod("CityFoodFlat", "Add", 2), _mod("CityGrowth", "Add", 20)],
    "Farmers Market": [_mod("CityFoodFlat", "Add", 3), _mod("CityGrowth", "Add", 30)],
    # +1 food for every farmer; +3 mana is what the original game's code gives this guild.
    "Animists Guild": [_mod("FarmerFood", "Add", 1), _mod("CityUnrest", "Add", -1), _mod("CityMana", "Add", 3)],
    "Sawmill": [_mod("CityProduction", "Percent", 25)],
    "Foresters Guild": [_mod("CityFood", "Add", 2), _mod("CityProduction", "Percent", 25)],
    "Miners Guild": [_mod("CityProduction", "Percent", 50)],
    "Mechanicians Guild": [_mod("CityProduction", "Percent", 50)],
}
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
        out.append(Building(i, name, slug(name), reqs, cost, max(upkeep, 0),
                            replace if replace > 0 else None, xp))
    return out


def building_ron(b: Building, by_index: dict[int, Building], races_allowed: list[str]) -> str:
    requires = [by_index[r].id for r in b.requires if r in by_index]
    parts = [f"id: {ron_str(b.id)}", f"name: {ron_str(b.name)}", f"cost: {b.cost}"]
    if b.upkeep:
        parts.append(f"upkeep_gold: {b.upkeep}")
    if requires:
        parts.append("requires: [" + ", ".join(ron_str(r) for r in requires) + "]")
    if races_allowed:
        parts.append("races: [" + ", ".join(ron_str(r) for r in races_allowed) + "]")
    if b.replaces is not None and b.replaces in by_index:
        parts.append(f"replaces: Some({ron_str(by_index[b.replaces].id)})")
    effects = list(BUILDING_EFFECTS.get(b.name, []))
    if b.starting_xp:
        effects.append(_mod("StartingXp", "Min", b.starting_xp))
    if effects:
        parts.append(f"effects: [{', '.join(effects)}]")
    return "    (" + ", ".join(parts) + "),"


# --------------------------------------------------------------------------- #
# Races and units (tables from the clone)
# --------------------------------------------------------------------------- #


@dataclass
class Race:
    index: int
    id: str
    name: str
    cant_build: list[int]
    growth_people: int  # people per turn (the table counts in tens)
    outpost_growth: int = 10  # base chance (percent) an outpost grows each turn


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
        growth_mod = words[10]
        name = names[f"{i:03d}"]
        races.append(Race(i, slug(name), name, [c for c in cant[:count] if c], growth_mod * 10, words[9]))
    return races


@dataclass
class UnitRow:
    index: int
    v: dict[str, int]
    name: str


def import_city_names(game: Path) -> list[list[str]]:
    """City names per race, in race-table order (20 names of 14 bytes each)."""
    records = read_records(read_lbx(game / "CITYNAME.LBX")[0])
    return [[cstr(rec[i : i + 14]) for i in range(0, len(rec), 14)] for rec in records]


def import_unit_rows(remom: Path) -> list[UnitRow]:
    src = (remom / "MoM/src/UNITTYPE.c").read_text(encoding="utf-8", errors="replace")
    names = {int(i): n for i, n in re.findall(r'char unit_type_name_(\d+)\[\] = "([^"]*)"', src)}
    body = src.split("_unit_type_table[NUM_UNIT_TYPES] =", 1)[1]
    rows = re.findall(r"\{&_unit_type_names\[(\d+)\],([^}]*)\}", body)
    out = []
    for idx, (name_idx, rest) in enumerate(rows):
        nums = [int(x, 0) for x in rest.replace("\n", " ").split(",") if x.strip()]
        if len(nums) != len(UNIT_COLUMNS) - 1:
            raise ValueError(f"unit row {idx}: expected {len(UNIT_COLUMNS) - 1} numbers, got {len(nums)}")
        values = dict(zip(UNIT_COLUMNS[1:], nums))
        out.append(UnitRow(idx, values, names[int(name_idx)]))
    return out


def flag_ids(word: int, table: list[tuple[int, str, str, str]], used: dict[str, tuple[str, str]]) -> list[str]:
    ids = []
    for mask, ident, name, desc in table:
        if word & mask:
            used[ident] = (name, desc)
            ids.append(ident)
    return ids


def unit_abilities(row: UnitRow, used: dict[str, tuple[str, str]]) -> list[str]:
    v = row.v
    ids = []
    ids += flag_ids(v["attr1"], ATTRIB_1_FLAGS, used)
    ids += flag_ids(v["attr2"], ATTRIB_2_FLAGS, used)
    ids += flag_ids(v["abilities"], ABILITY_FLAGS, used)
    ids += flag_ids(v["attack_flags"], ATTACK_FLAGS, used)
    ids += flag_ids(v["move_flags"], MOVE_FLAG_ABILITIES, used)
    rt = v["ranged_type"]
    if rt in RANGED_TYPES:
        ident, name = RANGED_TYPES[rt]
        used[ident] = (name, f"Ranged attack type: {name}.")
        ids.append(ident)
    # the same id can come from two flag words (flying): keep first
    return list(dict.fromkeys(ids))


def upkeep_ron(kind: str, amount: int) -> str:
    if kind == "Summoned":
        return f"(mana: {amount})"
    food = ", food: 1" if kind == "Normal" else ""
    return f"(gold: {amount}{food})"


def movement_of(row: UnitRow) -> str:
    flags = row.v["move_flags"]
    if flags & 0x08 or row.v["attr1"] & 0x0200:
        return "Flying"
    if flags & 0x02:
        return "Sailing"
    if flags & 0x04:
        return "Swimming"
    return "Walking"


def unit_stats(row: UnitRow) -> dict[str, int]:
    v = row.v
    stats = {
        "Melee": v["melee"],
        "Defense": v["defense"],
        "Resistance": v["resist"],
        "MaxHp": max(v["hits"], 1),
        "Figures": max(v["figures"], 1),
        "Moves": max((v["moves"] + 1) // 2, 1),  # the original counts half-moves
        "Sight": max(v["sight"], 1),
    }
    if v["ranged"] > 0:
        stats["Ranged"] = v["ranged"]
        stats["Ammo"] = max(v["ammo"], 1)
    if v["transport"] > 0:
        stats["Transport"] = v["transport"]
    if v["construction"] > 0:
        stats["Construction"] = v["construction"]
    if v["to_hit"] > 0:
        stats["ToHit"] = v["to_hit"] * 10  # the table counts in steps of 10%
    return stats


def stats_ron(stats: dict[str, int]) -> str:
    return "{" + ", ".join(f"{k}: {x}" for k, x in stats.items()) + "}"


@dataclass
class Unit:
    index: int
    id: str
    ron: str
    race: int | None = None  # race index for Normal units


def build_units(
    rows: list[UnitRow],
    races: list[Race],
    buildings: dict[int, Building],
    used_abilities: dict[str, tuple[str, str]],
    spell_cost_by_unit: dict[int, int],
    report: Report,
) -> list[Unit]:
    ids: dict[int, str] = {}
    taken: set[str] = set()
    for row in rows:
        v = row.v
        if row.index in NORMAL_RANGE and v["race"] < RACE_COUNT:
            base = f"{races[v['race']].id}_{slug(row.name)}"
        else:
            base = slug(row.name)
        ident, n = base, 2
        while ident in taken:
            ident, n = f"{base}_{n}", n + 1
        taken.add(ident)
        ids[row.index] = ident

    units = []
    for row in rows:
        v = row.v
        parts = [f"id: {ron_str(ids[row.index])}", f"name: {ron_str(row.name)}"]
        race_index = None
        if row.index in HERO_RANGE:
            kind, cost = "Hero", v["cost"]
        elif row.index in BOAT_RANGE:
            kind, cost = "Fantastic", v["cost"]
        elif row.index in NORMAL_RANGE:
            kind, cost, race_index = "Normal", v["cost"], v["race"]
        else:
            kind = "Summoned"
            cost = v["cost"] or spell_cost_by_unit.get(row.index, 0) or 1
        parts.append(f"kind: {kind}")
        if kind == "Normal":
            parts.append(f"race: Some({ron_str(races[race_index].id)})")
        if kind == "Summoned":
            realm = REALM_BY_RACE_BYTE.get(v["race"])
            if realm is None:
                report.note("units", f"{row.name}: unknown realm byte {v['race']}; using arcane")
                realm = "arcane"
            parts.append(f"realm: Some({ron_str(realm)})")
        parts.append(f"cost: {max(cost, 1) if kind != 'Hero' else cost}")
        if v["upkeep"]:
            parts.append(f"upkeep: {upkeep_ron(kind, v['upkeep'])}")
        parts.append(f"stats: {stats_ron(unit_stats(row))}")
        parts.append(f"movement: {movement_of(row)}")
        abilities = unit_abilities(row, used_abilities)
        if abilities:
            parts.append("abilities: [" + ", ".join(ron_str(a) for a in abilities) + "]")
        if kind in ("Normal", "Fantastic") and row.index not in HERO_RANGE:
            reqs = []
            for b in (v["bld1"], v["bld2"]):
                if b not in buildings or b <= 0:
                    continue
                if race_index is not None and b in races[race_index].cant_build:
                    # The original lists a building the unit's own race cannot build, so the
                    # unit can never be produced there. Keep it buildable and say so.
                    report.note("units", f"{row.name} ({races[race_index].name}): requires "
                                f"{buildings[b].name}, which that race cannot build; requirement dropped")
                    continue
                reqs.append(buildings[b].id)
            if reqs:
                parts.append("requires: [" + ", ".join(ron_str(r) for r in reqs) + "]")
        if kind == "Hero":
            parts.append('level_path: Some("hero_xp")')
            parts.append(f"hero: Some((slots: [Weapon, Armor, Misc], hire_cost: {v['cost']}))")
        elif kind == "Normal":
            parts.append('level_path: Some("standard_xp")')
        lbx, entry = ("units1", row.index) if row.index < 120 else ("units2", row.index - 120)
        parts.append(f"art: Some(({ron_str(lbx)}, {entry}))")
        units.append(Unit(row.index, ids[row.index], "    (" + ", ".join(parts) + "),", race_index))
    return units


# --------------------------------------------------------------------------- #
# Spells
# --------------------------------------------------------------------------- #

@dataclass
class SpellHelp:
    effects: str
    target: str
    upkeep: int
    text: str


def norm_name(name: str) -> str:
    return re.sub(r"[^a-z]", "", name.lower())


def spell_help_from_topics(topics: list["HelpTopic"]) -> dict[str, SpellHelp]:
    """Spell details from the (corrected) help topics: effect class, target, upkeep, text."""
    out = {}
    for topic in topics:
        lines = topic.text.split("\n")
        if not lines[0].startswith("Effects:") or len(lines) < 5:
            continue
        upkeep_digits = re.search(r"\d+", lines[3])
        out[norm_name(topic.title)] = SpellHelp(
            lines[0][len("Effects:"):].strip(),
            lines[1][len("Target:"):].strip(),
            int(upkeep_digits.group()) if upkeep_digits else 0,
            "\n".join(lines[4:]),
        )
    return out


@dataclass
class SpellRow:
    index: int
    name: str
    kind: int  # spell data type
    category: int
    realm: int
    eligibility: int
    cost: int
    research: int
    param0: int
    param1: int


def import_spell_rows(game: Path) -> list[SpellRow]:
    rows = []
    for i, r in enumerate(read_records(read_lbx(game / "SPELLDAT.LBX")[0])):
        kind, category, realm, elig = struct.unpack_from("<bbbb", r, 21)
        cost, research = struct.unpack_from("<hh", r, 26)
        p0, p1 = struct.unpack_from("<BB", r, 32)
        rows.append(SpellRow(i, cstr(r[:19]), kind, category, realm, elig, cost, research, p0, p1))
    return rows


# Unit-enchantment style spells that map directly onto stat modifiers.
# name -> list of (stat, op, value). Sourced from the original manual text.
UNIT_ENCHANT_MODS = {
    "Giant Strength": [("Melee", "Add", 1)],
    "Stone Skin": [("Defense", "Add", 1)],
    "Iron Skin": [("Defense", "Add", 5)],
    "Resist Elements": [("Resistance", "Add", 3)],
    "Resist Magic": [("Resistance", "Add", 3)],
    "Lionheart": [("Melee", "Add", 3), ("Ranged", "Add", 3), ("Resistance", "Add", 3), ("MaxHp", "Add", 3)],
    "Holy Armor": [("Defense", "Add", 2)],
    "Holy Weapon": [("ToHit", "Add", 10)],
    "Endurance": [("Moves", "Add", 1)],
    "Flame Blade": [("Melee", "Add", 2), ("Ranged", "Add", 2)],
    "Haste": [("Moves", "Percent", 100)],
    "Flight": [("Moves", "Set", 3)],
    "Weakness": [("Melee", "Add", -2), ("Ranged", "Add", -2)],
    "Vertigo": [("ToHit", "Add", -20), ("Defense", "Add", -1)],
    "Black Prayer": [("Melee", "Add", -1), ("Defense", "Add", -1), ("Resistance", "Add", -2)],
    "Mind Storm": [("Melee", "Add", -5), ("Ranged", "Add", -5), ("Defense", "Add", -5), ("Resistance", "Add", -5)],
    "Black Channels": [("Melee", "Add", 2), ("Ranged", "Add", 1), ("MaxHp", "Add", 1),
                       ("Resistance", "Add", 1), ("Defense", "Add", 1)],
}
NEGATIVE_UNIT_SPELLS = {"Weakness", "Black Prayer", "Mind Storm", "Warp Creature", "Web", "Vertigo",
                        "Confusion", "Possession", "Black Sleep", "Slow", "Creature Binding", "Petrify"}

CITY_ENCHANT_MODS = {
    # Gaia's Blessing is handled by the engine itself (more food from the land, doubled
    # forest production), so it keeps the placeholder effect.
    "Consecration": [("CityUnrest", "Add", -2)],
    "Natures Eye": [("CitySight", "Min", 4)],
    "Nature's Eye": [("CitySight", "Min", 4)],
    "Inspirations": [("CityProduction", "Percent", 100)],
    "Heavenly Light": [("CityUnrest", "Add", -1)],
    "Altar of Battle": [("StartingXp", "Min", 120)],
    "Stream of Life": [("CityGrowthPercent", "Add", 100)],
    "Prosperity": [("CityGold", "Percent", 100)],
    "Famine": [("CityFood", "Percent", -50), ("CityUnrestPercent", "Add", 25)],
    "Pestilence": [("CityUnrest", "Add", 2)],
    "Cursed Lands": [("CityProduction", "Percent", -50), ("CityUnrest", "Add", 1)],
    "Dark Rituals": [("CityGrowthPercent", "Add", -25), ("CityUnrest", "Add", 1)],
}

DAMAGE_KIND_BY_NAME = {
    "Ice Bolt": "Cold", "Ice Storm": "Cold", "Lightning Bolt": "Lightning", "Call Lightning": "Lightning",
    "Warp Lightning": "Lightning", "Fire Bolt": "Fire", "Fireball": "Fire", "Flame Strike": "Fire",
    "Fire Storm": "Fire", "Doom Bolt": "Physical", "Life Drain": "Death", "Death Spell": "Death",
    "Psionic Blast": "Physical", "Holy Word": "Holy", "Disintegrate": "Physical",
}
DAMAGE_KIND_BY_REALM = {"nature": "Cold", "sorcery": "Lightning", "chaos": "Fire", "life": "Holy",
                        "death": "Death", "arcane": "Physical"}


def mods_effects(mods: list[tuple[str, str, int]]) -> list[str]:
    return [f"(effect: Modify(stat: {s}, op: {op}, value: {v}))" for s, op, v in mods]


# Spells that move the wizard's fortress or summoning circle; the engine runs these hooks.
FORTRESS_HOOKS = {
    "Move Fortress": "move_fortress",
    "Summoning Circle": "move_summoning_circle",
    "Spell of Return": "return_to_fortress",
}


# Spells that reshape one map square; the engine runs these hooks on a tile target.
TILE_HOOKS = {
    "Raise Volcano": "raise_volcano",
    "Change Terrain": "change_terrain",
    "Enchant Road": "enchant_road",
}


# Global enchantments the engine reads from the wizard's list (`globals.rs`); casting them
# runs no effect of its own.
GLOBAL_ENGINE = {
    "Herb Mastery", "Planar Seal", "Evil Omens", "Awareness", "Nature's Awareness",
    "Detect Magic", "Charm of Life",
}
# Global enchantments that change unit stats: (condition, stat, op, value) per rule.
GLOBAL_MODS = {
    "Holy Arms": [("KindIs(Normal)", "Melee", "Add", 1), ("KindIs(Normal)", "Ranged", "Add", 1)],
    "Chaos Surge": [('RealmIs("chaos")', "Melee", "Add", 2), ('RealmIs("chaos")', "Ranged", "Add", 2)],
}


def spell_effects(row: SpellRow, unit_ids: dict[int, str], realm_id: str, report: Report) -> tuple[list[str], str, str]:
    """Return (effects, target, duration-kind) for a spell; `duration-kind` is
    "instant" or "lasting". Unmapped spells get the `unimplemented` custom hook."""
    name, kind = row.name, row.kind
    if name in FORTRESS_HOOKS:
        hook = FORTRESS_HOOKS[name]
        return [f'(when: OnCast, target: Chosen, effect: Custom("{hook}"))'], "OwnCity", "instant"
    if name in TILE_HOOKS:
        hook = TILE_HOOKS[name]
        return [f'(when: OnCast, target: Chosen, effect: Custom("{hook}"))'], "Tile", "instant"
    if name in GLOBAL_ENGINE or name in GLOBAL_MODS:
        rules = ['(when: OnCast, target: Chosen, effect: Custom("global_effect"))']
        for cond, stat, op, value in GLOBAL_MODS.get(name, []):
            rules.append(f"(condition: {cond}, effect: Modify(stat: {stat}, op: {op}, value: {value}))")
        return rules, "None", "lasting"
    unimplemented = ['(when: OnCast, target: Chosen, effect: Custom("unimplemented"))']
    if kind == 0:  # summoning
        unit = unit_ids.get(row.param0 | (row.param1 << 8))
        if unit:
            return [f'(when: OnCast, target: Chosen, effect: Summon(unit: {ron_str(unit)}, count: 1))'], "Tile", "instant"
        report.note("spells", f"{name}: summons unit index {row.param0} which is not a summonable unit")
        return unimplemented, "Tile", "instant"
    if kind in (1, 15):
        mods = UNIT_ENCHANT_MODS.get(name)
        target = "EnemyUnit" if name in NEGATIVE_UNIT_SPELLS else "OwnUnit"
        if mods:
            return mods_effects(mods), target, "lasting"
        report.note("spells", f"{name}: unit enchantment without a stat mapping; custom hook")
        return unimplemented, target, "lasting"
    if kind in (2, 3):
        mods = CITY_ENCHANT_MODS.get(name)
        target = "OwnCity" if kind == 2 else "EnemyCity"
        if mods:
            return mods_effects(mods), target, "lasting"
        report.note("spells", f"{name}: city enchantment without a stat mapping; custom hook")
        return unimplemented, target, "lasting"
    if kind in (4, 22):
        dk = DAMAGE_KIND_BY_NAME.get(name, DAMAGE_KIND_BY_REALM[realm_id])
        amount = max(row.param0, 1)
        return [f"(when: OnCast, target: Chosen, effect: Damage(amount: {amount}, kind: {dk}, resistible: true))"], "EnemyUnit", "instant"
    if kind == 18:
        return [f"(when: OnCast, target: Chosen, effect: Dispel(strength: {max(row.cost, 1)}))"], "AnyUnit", "instant"
    if name == "Healing":
        return ["(when: OnCast, target: Chosen, effect: Heal(5))"], "OwnUnit", "instant"
    target = {6: "Wizard", 17: "Wizard", 9: "None", 10: "Battlefield", 11: "None"}.get(kind)
    if target is None:
        target = "AnyUnit" if row.eligibility <= 0 else "None"
    report.note("spells", f"{name}: spell type {kind} not mapped; custom hook")
    lasting = "lasting" if kind in (9, 10) else "instant"
    return unimplemented, target, lasting


def spell_contexts(row: SpellRow) -> str:
    if row.eligibility < 0:
        return "[Battle]"
    if row.eligibility == 0:
        return "[Battle, Overland]"
    return "[Overland]"


def build_spells(rows: list[SpellRow], help_: dict[str, SpellHelp], unit_ids: dict[int, str],
                 report: Report) -> list[str]:
    out = []
    per_realm: dict[int, int] = {}
    for row in rows[1:]:  # row 0 is the "None" placeholder
        realm_id = REALMS[row.realm][0]
        n = per_realm.get(row.realm, 0)
        per_realm[row.realm] = n + 1
        # The table lists each realm's spells common -> very rare in blocks of ten.
        rarity = ["Common", "Uncommon", "Rare", "VeryRare"][min(n // 10, 3)]
        info = help_.get(norm_name(row.name))
        if info is None:
            report.note("spells", f"{row.name}: no help text found; upkeep estimated")
        effects, target, lasting = spell_effects(row, unit_ids, realm_id, report)
        if lasting == "lasting":
            duration = "Permanent"
            upkeep = info.upkeep if info else max(row.cost // 10, 1)
        else:
            duration, upkeep = "Instant", 0
        parts = [
            f"id: {ron_str(slug(row.name))}", f"name: {ron_str(row.name)}",
            f"realm: {ron_str(realm_id)}", f"rarity: {rarity}",
            f"cost: {max(row.cost, 1)}", f"research_cost: {max(row.research, 1)}",
            f"contexts: {spell_contexts(row)}", f"target: {target}", f"duration: {duration}",
        ]
        if upkeep:
            parts.append(f"upkeep: {upkeep}")
        if lasting == "lasting":
            parts.append(f"dispel_difficulty: {max(row.cost, 1)}")
        parts.append("effects: [" + ", ".join(effects) + "]")
        out.append("    (" + ", ".join(parts) + "),")
    return out


# --------------------------------------------------------------------------- #
# Items
# --------------------------------------------------------------------------- #

ITEM_SLOT_BY_TYPE = {0: "Weapon", 1: "Weapon", 2: "Weapon", 3: "Weapon", 4: "Weapon", 5: "Weapon",
                     6: "Misc", 7: "Armor", 8: "Armor", 9: "Armor"}


def import_items(game: Path, report: Report) -> list[str]:
    out, taken = [], set()
    for r in read_records(read_lbx(game / "ITEMDATA.LBX")[0]):
        name = cstr(r[:30])
        item_type = r[0x21]
        cost = struct.unpack_from("<h", r, 0x22)[0]
        attack, to_hit, defense, moves2, resist = struct.unpack_from("<bbbbb", r, 0x24)
        mods = []
        for stat, value in (("Melee", attack), ("ToHit", to_hit * 10), ("Defense", defense),
                            ("Moves", moves2 // 2), ("Resistance", resist)):
            if value:
                mods.append((stat, "Add", value))
        if not mods or cost <= 0:
            report.note("items", f"{name}: no stat bonus (specials not imported); skipped")
            continue
        ident = slug(name)
        if ident in taken:
            ident = f"{ident}_{len(taken)}"
        taken.add(ident)
        slot = ITEM_SLOT_BY_TYPE.get(item_type, "Misc")
        out.append(f"    (id: {ron_str(ident)}, name: {ron_str(name)}, slot: {slot}, cost: {cost}, "
                   f"effects: [{', '.join(mods_effects(mods))}]),")
    return out


# --------------------------------------------------------------------------- #
# Help
# --------------------------------------------------------------------------- #

HELP_RECORD_SIZE = 1048  # title[30], lbx_name[14], lbx_entry i16, include_entry i16, text[1000]
# HELP.LBX entry 2 declares 1614 records but only the first 807 are real; the rest is
# stale buffer contents the original build tool padded the file with.
HELP_REAL_RECORDS = 807


@dataclass
class HelpTopic:
    index: int
    title: str
    text: str
    picture: tuple[str, int] | None
    include: int | None


def import_help(game: Path, report: Report) -> list[HelpTopic]:
    topics = []
    for i, rec in enumerate(read_records(read_lbx(game / "HELP.LBX")[2])[:HELP_REAL_RECORDS]):
        title = cstr(rec[:30])
        picture = None
        if rec[30] != 0:  # an empty name means no picture; later bytes are stale
            lbx_entry, include = struct.unpack_from("<hh", rec, 44)
            picture = (cstr(rec[30:44]).lower(), lbx_entry)
        else:
            include = struct.unpack_from("<h", rec, 46)[0]
        text = cstr(rec[48:]).replace("\x14", "\n")
        if not title:
            report.note("help", f"entry {i} has no title; kept with an empty title")
        topics.append(HelpTopic(i, title, text, picture, include if include > 0 else None))
    return topics


def help_ron(topic: HelpTopic) -> str:
    parts = [f"index: {topic.index}", f"title: {ron_str(topic.title)}", f"text: {ron_str(topic.text)}"]
    if topic.picture:
        parts.append(f"picture: Some(({ron_str(topic.picture[0])}, {topic.picture[1]}))")
    if topic.include is not None:
        parts.append(f"include: Some({topic.include})")
    return "    (" + ", ".join(parts) + "),"


def import_help_screens(game: Path, remom: Path) -> list[str]:
    """Which screen areas open which help topic (HLPENTRY.LBX)."""
    doc = (remom / "doc/___LBX/HLPENTRY_LBX.md").read_text(encoding="utf-8", errors="replace")
    names = {int(n): label for n, label in re.findall(r'HLPENTRY\.LBX, (\d+)\s+""\s+"([^"]*)"', doc)}
    lines = []
    for i, entry in enumerate(read_lbx(game / "HLPENTRY.LBX")):
        if len(entry) < 4:
            continue
        fields = []
        for rec in read_records(entry):
            help_idx, x1, y1, x2, y2 = struct.unpack("<5h", rec)
            fields.append(f"(help: {help_idx}, rect: ({x1}, {y1}, {x2}, {y2}))")
        name = names.get(i, f"screen {i}")
        lines.append(f"    (id: {i}, name: {ron_str(name)}, fields: [{', '.join(fields)}]),")
    return lines


# Help text corrected to match the game's code and data. Where the original help and the
# game disagree, the game wins (decision recorded in docs/help-vs-implementation-conflicts.md).
# Each entry: (topic index, text to replace, replacement, why). The replaced text must occur
# exactly once, so a changed source file fails loudly instead of being skipped.
HELP_CORRECTIONS = [
    (733, "Maintenance: 2 gold/turn", "Maintenance: 3 gold/turn",
     "the building data file charges 3 gold"),
    (44, "Upkeep: 1 mana/turn", "Upkeep: 2 mana/turn",
     "the unit-enchantment upkeep table charges 2 mana"),
    (44, "making the unit almost totally immune to arrow and sling attacks",
     "making the unit immune to missile attacks",
     "the code grants full missile immunity"),
    (574, "making the unit almost totally immune to arrow and sling attacks",
     "making the unit immune to missile attacks",
     "the code grants full missile immunity"),
    (732, "+1 to hit and +3 mana is generated.", "+1 to hit.",
     "the code gives the +3 mana to the Animist's Guild, not the Alchemists Guild"),
    (723, "doubles the healing rate of units inside the city.",
     "doubles the healing rate of units inside the city. It also adds +3 mana.",
     "the code gives the Animist's Guild +3 mana"),
    (734, "at an upkeep cost of 3 mana points a turn.", "and +3 mana.",
     "the code adds 3 mana for a Wizard's Guild instead of charging it"),
    (667, "which doubles the power and pacifying effects received from shrines, temples, and parthenons.",
     "which increases the power and pacifying effects received from shrines, temples, and parthenons by 50%.",
     "the code applies 150% for Infernal and Divine Power alike"),
    (399, "Healers will heal 25% of the damage that all units stacked with the healer have sustained after a combat.",
     "Healers heal an extra 20% of the total hit points of every unit stacked with the healer each turn.",
     "the code heals 20% of total hit points per turn, not 25% after combat"),
    (728, "It increases the production of forest squares by 25%.",
     "It increases the production of the whole city by 25%.",
     "the code adds 25% to the city's total production"),
    (398, "destroy a section of a stone wall 50% of the time.",
     "destroy a section of a stone wall 50% of the time in melee, 25% of the time with a ranged attack.",
     "the code uses 25% for ranged attacks"),
    (281, "Maritime Guilds.", "Maritime Guilds. Dark elf cities receive 1 power per citizen.",
     "the code adds one mana per citizen for dark elf cities"),
    (478, "defense is increased by +3.", "defense is increased by +2.",
     "the code adds 2 defence (the spell text already says 2)"),
    (453, "+3 melee attack strength", "+2 melee attack strength",
     "the code adds 2 melee (the spell text already says 2)"),
    (464, "enemy units by -2.", "enemy units by -1.",
     "the code applies -1 to hit against invisible units"),
    (564, "enemy units by -2.", "enemy units by -1.",
     "the code applies -1 to hit against invisible units"),
]


def apply_help_corrections(topics: list["HelpTopic"], report: "Report") -> int:
    """Rewrite topics to match the game's code and append a note saying so.

    Raises:
        ValueError: if a topic is missing or the text to replace is not found exactly once.
    """
    by_index = {t.index: t for t in topics}
    for index, old, new, why in HELP_CORRECTIONS:
        topic = by_index.get(index)
        if topic is None or topic.text.count(old) != 1:
            raise ValueError(f"help correction for topic {index} does not apply: {old!r}")
        topic.text = topic.text.replace(old, new) + f"\n[Updated from game code: {why}.]"
        report.note("help", f"topic {index} {topic.title}: updated from game code ({why})")
    return len(HELP_CORRECTIONS)


def cross_check_help(topics: list[HelpTopic], buildings: list[Building], races: list[Race],
                     spell_rows: list[SpellRow], report: Report) -> int:
    """Compare imported numbers with what the original help text states.

    Returns the number of mismatches; each is also written to the report."""
    by_title = {norm_name(t.title): t for t in topics}
    found = 0

    def mismatch(text: str) -> None:
        nonlocal found
        found += 1
        report.note("check", text)

    for b in buildings:
        topic = by_title.get(norm_name(b.name))
        match = topic and re.match(r"Maintenance: (\d+) gold/turn", topic.text)
        if match and int(match.group(1)) != b.upkeep:
            mismatch(f"building {b.name}: upkeep {b.upkeep}, help says {match.group(1)}")
    building_names = {norm_name(b.name): b.index for b in buildings}
    for race in races:
        topic = by_title.get(norm_name(race.name.replace("Dwarven", "Dwarf")) + "townsfolk")
        if not topic:
            continue
        banned = set()
        sentence = re.search(r"(?:may not|cannot|can not) build (.*?)\.", topic.text)
        if sentence:
            for part in re.split(r",| or ", sentence.group(1)):
                key = norm_name(part)
                singular = re.sub(r"ies$", "y", key) if key.endswith("ies") else key.rstrip("s")
                # "Maritime or Wizard's Guilds": a bare word shares the last item's noun.
                for candidate in (key, singular, singular + "guild"):
                    if candidate in building_names:
                        banned.add(building_names[candidate])
        if banned != set(race.cant_build):
            mismatch(f"race {race.name}: cannot-build {sorted(race.cant_build)}, help text implies {sorted(banned)}")
    for row in spell_rows[1:]:
        topic = by_title.get(norm_name(row.name))
        match = topic and re.search(r"Casting Cost: (\d+)", topic.text)
        if match and int(match.group(1)) != row.cost:
            mismatch(f"spell {row.name}: cost {row.cost}, help says {match.group(1)}")
    return found


# --------------------------------------------------------------------------- #
# Output
# --------------------------------------------------------------------------- #


def write_list(path: Path, header: str, lines: list[str]) -> None:
    path.write_text(f"// {header}\n[\n" + "\n".join(lines) + "\n]\n", encoding="utf-8")


def level_path_ron(ident: str, levels: list[tuple[str, int]]) -> str:
    rows = []
    for i, (name, xp) in enumerate(levels):
        effects = []
        if i >= 1:
            effects.append("(effect: Modify(stat: Resistance, op: Add, value: 1))")
        if i >= 2:
            effects.append("(effect: Modify(stat: ToHit, op: Add, value: 10))")
        rows.append(f"        (name: {ron_str(name)}, xp: {xp}, effects: [{', '.join(effects)}]),")
    return f"    (id: {ron_str(ident)}, levels: [\n" + "\n".join(rows) + "\n    ]),"


# Percent unrest caused by each tax rate (the rate is also half the gold per citizen).
TAX_UNREST_PERCENT = [0, 10, 20, 30, 45, 60, 75]


def import_racial_unrest(game: Path) -> list[list[int]]:
    """Unrest in tens of percent: `[capital race][city race]` (TERRSTAT.LBX entry 1)."""
    values = struct.unpack("<196b", read_lbx(game / "TERRSTAT.LBX")[1][4:])
    return [list(values[i * RACE_COUNT : (i + 1) * RACE_COUNT]) for i in range(RACE_COUNT)]


def write_rules(path: Path, game: Path) -> None:
    matrix = ", ".join("[" + ", ".join(str(v) for v in row) + "]" for row in import_racial_unrest(game))
    path.write_text(
        "// Generated by scripts/import_mom.py from the original game data. Do not commit.\n(\n"
        "    to_hit_percent: 30,\n    block_percent: 30,\n    min_city_distance: 4,\n    max_city_size: 25,\n"
        f"    tax_unrest_percent: {TAX_UNREST_PERCENT},\n    racial_unrest: [{matrix}],\n)\n",
        encoding="utf-8",
    )


def run(game: Path, remom: Path, out: Path) -> Report:
    report = Report()
    out.mkdir(parents=True, exist_ok=True)

    races = import_races(remom)
    city_names = import_city_names(game)
    if len(city_names) < len(races):
        raise ValueError(f"CITYNAME.LBX has {len(city_names)} races, expected {len(races)}")
    buildings = [b for b in import_buildings(game, report) if b.name not in SKIP_BUILDINGS]
    by_index = {b.index: b for b in buildings}
    rows = import_unit_rows(remom)

    # Spell summons need unit ids; units need summon costs. Build ids first.
    spell_rows = import_spell_rows(game)
    help_topics = import_help(game, report)
    apply_help_corrections(help_topics, report)
    help_texts = spell_help_from_topics(help_topics)
    spell_cost_by_unit = {r.param0 | (r.param1 << 8): r.cost for r in spell_rows if r.kind == 0}

    used_abilities: dict[str, tuple[str, str]] = {}
    units = build_units(rows, races, by_index, used_abilities, spell_cost_by_unit, report)
    summoned_ids = {u.index: u.id for u in units if u.index in SUMMONED_RANGE}
    spells = build_spells(spell_rows, help_texts, summoned_ids, report)

    # races
    race_lines = []
    for race in races:
        mine = [u.id for u in units if u.race == race.index]
        effects = ""
        if race.growth_people:
            effects = f", effects: [{_mod('CityGrowth', 'Add', race.growth_people)}]"
        traits = RACE_TRAITS.get(race.id)
        extra = f", {traits}" if traits else ""
        race_lines.append(
            f"    (id: {ron_str(race.id)}, name: {ron_str(race.name)}, "
            f"units: [{', '.join(ron_str(u) for u in mine)}], buildings: []{effects}, "
            f"city_names: [{', '.join(ron_str(n) for n in city_names[race.index] if n)}], "
            f"outpost_growth: {race.outpost_growth}{extra}),"
        )
    # buildings, with per-race exclusions turned into allowed-race lists
    building_lines = []
    for b in buildings:
        excluded = {r.id for r in races if b.index in r.cant_build}
        allowed = [r.id for r in races if r.id not in excluded] if excluded else []
        building_lines.append(building_ron(b, by_index, allowed))

    abilities_lines = [
        f"    (id: {ron_str(i)}, name: {ron_str(n)}, description: {ron_str(d)}, effects: []),"
        for i, (n, d) in sorted(used_abilities.items())
    ]
    realm_lines = [
        f"    (id: {ron_str(i)}, name: {ron_str(n)}, color: ({c[0]}, {c[1]}, {c[2]}), "
        f"default_spells: [{', '.join(ron_str(slug(x)) for x in DEFAULT_START_SPELLS.get(i, []))}])," for i, n, c in REALMS
    ]
    items = import_items(game, report)

    header = "Generated by scripts/import_mom.py from the original game data. Do not commit."
    write_list(out / "races.ron", header, race_lines)
    write_list(out / "units.ron", header, [u.ron for u in units])
    write_list(out / "buildings.ron", header, building_lines)
    write_list(out / "abilities.ron", header, abilities_lines)
    write_list(out / "realms.ron", header, realm_lines)
    write_list(out / "spells.ron", header, spells)
    write_list(out / "items.ron", header, items)
    write_list(out / "wizards.ron", header, wizard_lines(help_topics))
    write_list(out / "level_paths.ron", header,
               [level_path_ron("standard_xp", LEVELS_NORMAL), level_path_ron("hero_xp", LEVELS_HERO)])
    mismatches = cross_check_help(help_topics, buildings, races, spell_rows, report)
    LOG.info("help cross-check: %d mismatches", mismatches)
    write_list(out / "help.ron", header, [help_ron(t) for t in help_topics])
    write_list(out / "help_screens.ron", header, import_help_screens(game, remom))
    write_rules(out / "rules.ron", game)
    for generic in ("terrain.ron", "features.ron"):  # terrain and features are not game-specific data
        shutil.copy(REPO / "content" / generic, out / generic)

    summary = (
        f"races {len(races)}, buildings {len(buildings)}, units {len(units)}, "
        f"abilities {len(used_abilities)}, spells {len(spells)}, items {len(items)}, "
        f"help topics {len(help_topics)}"
    )
    (out / "IMPORT_REPORT.txt").write_text(summary + "\n\n" + "\n".join(report.lines) + "\n", encoding="utf-8")
    LOG.info("imported: %s (%d notes in IMPORT_REPORT.txt)", summary, len(report.lines))
    return report


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--game", type=Path, default=DEFAULT_GAME)
    parser.add_argument("--remom", type=Path, default=DEFAULT_REMOM)
    parser.add_argument("--out", type=Path, default=REPO / "content-local")
    args = parser.parse_args(argv)
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    run(args.game, args.remom, args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
