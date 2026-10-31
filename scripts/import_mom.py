"""Import Master of Magic data into local content files for rust_fantasy_4X.

Reads the author's own copy of the original game and a local clone of the ReMoM
reverse-engineering project, and writes only what is not game data: `content-local/` (help
text, screen-to-topic map, city names and preset wizards: copyrighted prose, gitignored,
never to be committed or distributed). The game's own data in `content/` (races, units,
buildings, spells, abilities, items, realms, level paths, rules) is kept by hand; the importer only reads
buildings, races and spells to check them against the help text.

Usage:
    python3 -I scripts/import_mom.py [--game DIR] [--remom DIR] [--out DIR] [--rules-out DIR]

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

LOG = logging.getLogger("import_mom")

REPO = Path(__file__).resolve().parent.parent
DEFAULT_GAME = REPO / "reference/mom-classic/Master of Magic Official Release"
DEFAULT_REMOM = Path.home() / "projects/ReMoM"


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



# The ready-made wizards' names and descriptions (help topic 659 + position). Their books and
# retorts are game rules and live in `content/wizards.ron`, in this order (the original's
# `_wizard_presets_table`).
WIZARD_NAMES = [
    ("merlin", "Merlin"),
    ("raven", "Raven"),
    ("sharee", "Sharee"),
    ("lo_pan", "Lo Pan"),
    ("jafar", "Jafar"),
    ("oberic", "Oberic"),
    ("rjak", "Rjak"),
    ("sssra", "Sss'ra"),
    ("tauron", "Tauron"),
    ("freya", "Freya"),
    ("horus", "Horus"),
    ("ariel", "Ariel"),
    ("tlaloc", "Tlaloc"),
    ("kali", "Kali"),
]
WIZARD_HELP_FIRST = 659


def wizard_text_lines(help_topics) -> list[str]:
    by_index = {t.index: t for t in help_topics}
    lines = []
    for i, (wid, name) in enumerate(WIZARD_NAMES):
        topic = by_index.get(WIZARD_HELP_FIRST + i)
        text = topic.text if topic and topic.title.lower() == name.lower() else ""
        lines.append(
            f"    (id: {ron_str(wid)}, name: {ron_str(name)}, description: {ron_str(text)}),"
        )
    return lines


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




HERO_COUNT = 35
HERO_NAME_SETS = 5


def import_hero_names(game: Path, units_file: Path) -> list[tuple[str, list[str]]]:
    """The original's individual hero names: `NAMES.LBX` holds five sets of 35 names, and
    hero `h` of the unit table takes the names at `h`, `h + 35`, ... Returned with the ids
    of the hero units in `units_file`, in table order."""
    names = [cstr(r) for r in read_records(read_lbx(game / "NAMES.LBX")[0])]
    ids = re.findall(r'\(id: "([^"]+)",[^\n]*?kind: Hero', units_file.read_text(encoding="utf-8"))
    if len(ids) != HERO_COUNT:
        raise ValueError(f"{units_file} has {len(ids)} heroes, expected {HERO_COUNT}")
    return [
        (uid, [names[h + HERO_COUNT * s] for s in range(HERO_NAME_SETS)])
        for h, uid in enumerate(ids)
    ]


def import_city_names(game: Path) -> list[list[str]]:
    """City names per race, in race-table order (20 names of 14 bytes each)."""
    records = read_records(read_lbx(game / "CITYNAME.LBX")[0])
    return [[cstr(rec[i : i + 14]) for i in range(0, len(rec), 14)] for rec in records]




















# --------------------------------------------------------------------------- #
# Spells
# --------------------------------------------------------------------------- #

def norm_name(name: str) -> str:
    return re.sub(r"[^a-z]", "", name.lower())


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
    #: The attack attributes of a damage spell (`ATTACK_FLAGS`: armour piercing, area, ...).
    attributes: int = 0




def import_spell_rows(game: Path) -> list[SpellRow]:
    rows = []
    for i, r in enumerate(read_records(read_lbx(game / "SPELLDAT.LBX")[0])):
        kind, category, realm, elig = struct.unpack_from("<bbbb", r, 21)
        # The research cost is unsigned: the Spell of Mastery's 60,000 does not fit a signed short.
        cost, research = struct.unpack_from("<hH", r, 26)
        p0, p1 = struct.unpack_from("<BB", r, 32)
        attributes = struct.unpack_from("<H", r, 34)[0]
        rows.append(SpellRow(i, cstr(r[:19]), kind, category, realm, elig, cost, research, p0, p1,
                             attributes))
    return rows


# --------------------------------------------------------------------------- #
# Items
# --------------------------------------------------------------------------- #







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


# Help topics the original wrote twice, kept once: area topic -> the topic that stays. Our
# `content/help.ron` has no repeated topic (checked on load), so the areas open the one.
# -1 drops a topic: the area stays and opens no help (the OK buttons need none).
HELP_ALIASES = {
    268: 247,  # Alchemy, from the magic screen and from the item screen
    **{ok: -1 for ok in (241, 248, 252, 269, 307, 500, 640)},  # OK buttons
    641: -1,  # Cancel on the main menu
}


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
            help_idx = HELP_ALIASES.get(help_idx, help_idx)
            fields.append(f"(help: {help_idx}, rect: ({x1}, {y1}, {x2}, {y2}))")
        name = names.get(i, f"screen {i}")
        lines.append(f"    (id: {i}, name: {ron_str(name)}, fields: [{', '.join(fields)}]),")
    return lines


# Help text corrected to match the game's code and data. Where the original help and the
# game disagree, the game wins (decision recorded in docs/help-vs-implementation-conflicts.md).
# Each entry: (topic index, text to replace, replacement, why). The replaced text must occur
# exactly once, so a changed source file fails loudly instead of being skipped.
HELP_CORRECTIONS = [
    (515, "increases the attack strength of all units in combat by one per 2 levels of the hero.",
     "increases the attack strength of all normal units in combat by one per 3 levels of the hero (counting the first level), and the ranged attacks that are not magic by half as much; only the best hero's counts. Super leadership (*) gives one per 2 levels.",
     "the code gives x / 3 and x / 2 for super with x = level + 1, and the manual text in ReMoM says three levels; fantastic units and the undead get nothing (B52)"),
    (516, "increases the frequency and strength of mercenaries approaching the wizard for hire, as well as decreases the initial hiring cost. The hero adds",
     "raises the wizard's fame, which makes mercenaries and heroes come more often and be stronger. The hero adds",
     "nothing in the code lowers a hiring price; the hero's effect is its fame (B53)"),
    (519, "adding the +1/level to the hero's defense.",
     "adding +1 hit point per level to the hero.",
     "the code adds hit points, not defence (B54)"),
    (524, "Luck adds +1 to all of the hero's rolls, including attack, defense, and resistance.",
     "Luck adds 10% to the hero's chance to hit and to block and +1 to its resistance, and takes 10% from the chance to hit of whoever attacks it.",
     "the code's Lucky acts on the rolls, not on the attack strength (B55)"),
    (55, "Note that all roads on Myrror are automatically enchanted.",
     "Note that all roads on Myrror are automatically enchanted. The enchantment is permanent, but a dispel cast on a square (Disenchant Area, Disenchant True) can undo it there.",
     "our own rule: the original's enchanted roads cannot be dispelled; ours can, square by square (divergence 27)"),
    (198, "All nature and life spells cost 50% more than normal to cast.",
     "All nature and life spells cost 50% more than normal to cast, for every wizard including the one who cast Evil Omens. Spells cast from items and by units are not affected.",
     "the casting-cost code applies it to any wizard's cost whoever holds the spell; item and unit casts pay the spell's plain cost (decision for now, B11)"),
    (657, "You may not cast any other spells while banished and you receive no power income.",
     "You may not cast any other spells while banished and you receive no power income. A wizard with no other city is defeated instead. A computer wizard whose casting skill is under 40, or who has no mana, resigns at once; you are asked whether to resign.",
     "the conquest code defeats a banished wizard that cannot pay for the spell, and asks the human whether to resign"),
    (765, "winning a major battle (4 or more opponents), building a new city, or by capturing a village, city or capitol. Likewise, any wizard losing a major battle will lose one point of fame.",
     "winning a major battle (4 or more opponents) or one that defeats a unit costing 600 or more, or by capturing a city (its size class less two points, so a hamlet or village gives none). Building a new city gives no fame. A wizard loses fame when a hero dies in battle (about half the hero's level), when a city is lost (as many points as its size) and, with more than 20 fame, one point for a major defeat.",
     "the original's code gives no fame for founding a city, gives the winner a point for rare foes, and takes the fame of lost heroes and cities; only a human wizard with more than 20 fame loses a point for a major defeat"),
    (593, "fleeing heroes have a 25% chance.",
     "fleeing heroes have a 25% chance. On the Intro and Easy levels the player whose turn it is loses no unit by fleeing, and units that are asleep, confused or held in a web are always lost.",
     "the combat code skips the roll on Intro and Easy for the player on turn, and always destroys sleeping, confused and webbed fleeing units"),
    (607, "increased by 2 (20% improvement in each area), while resistance (crosses) is increased by 3 and the ability to block an attack and to hit is increased by 10%.",
     "increased by 2 (20% improvement in each area), while resistance (crosses) is increased by 3 and the ability to block an attack is increased by 10%.",
     "the code gives +2 to melee and defence, +3 to resistance and +1 (ten percent) to hit and to block"),
    (196, "if the current population exceeds a 1-10 roll.",
     "if the current population exceeds a 1-10 roll. Unrest in the city rises by 2 for as long as the spell lasts.",
     "the city code takes 2 from the city's pacification while Pestilence is on it (the help does not say so)"),
    (773, "increasing the power from bad temples by 50% while halving the power obtained from good temples.",
     "doubling the power from bad temples while halving the power obtained from good temples.",
     "the city magic code adds the full power of the religious buildings for the matching realm (a doubling), where the help says 50% (B50)"),
    (774, "increasing the power from good temples by 50% while halving the power obtained from bad temples.",
     "doubling the power from good temples while halving the power obtained from bad temples.",
     "the city magic code adds the full power of the religious buildings for the matching realm (a doubling), where the help says 50% (B50)"),
    (119, "and each unit inside the city takes 10 points of damage regardless of defense.",
     "each unit inside the city takes 10 points of damage regardless of defense, and each citizen but the first has a 50% chance to die, so the city is never emptied.",
     "the spell's code kills each citizen after the first with an even chance (`Apply_Call_The_Void`); the help does not say so (B51)"),
    (486, "Units get no chance to resist.", "Units get a chance to resist at a penalty of 2.",
     "the code gives Weakness a resistance check at -2 (the spell's own help text says so)"),
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
    (26, "All buildings have a 10% chance of being destroyed. All non- flying, corporeal creatures have a 15% chance of dying.",
     "All buildings have a 15% chance of being destroyed. All non- flying, corporeal creatures have a 25% chance of dying.",
     "the code rolls 15 for each building and one in four for each unit"),
    (57, "Doubles the speed of all ships and sky masters controlled by a wizard, and halves the speed of all others.",
     "Increases the speed of all ships and flying units controlled by a wizard by half. The ships and flying units of every wizard without Wind Mastery move at half speed.",
     "our rule: both bonus and penalty apply to ships and fliers, overland and in battle; nothing stacks"),
    (93, "demon-skin armor (+2 defense), demon wings", "demon-skin armor (+3 defense), demon wings",
     "the code adds 3 to the defence (CC_ARMOR)"),
    (440, "Adds demon-skin armor (+2 defense).", "Adds demon-skin armor (+3 defense).",
     "the code adds 3 to the defence (CC_ARMOR)"),
    (554, "each unit in stasis must resist at -3 to be freed.",
     "each unit in stasis must resist at -5 to be freed.",
     "the code rolls the freeing check at -5, as help topic 68 says"),
    (171, "Drains 50-150 mana points from a target wizard's mana reserve.",
     "Drains 10-200 mana points (105 on average) from a target wizard's mana reserve.",
     "the code adds ten rolls of a twenty-sided die"),
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
    (29, "and a 20% chance that its corrupted lands return to their pristine state.",
     "and a 20% chance that its corrupted lands return to their pristine state. Two rebels are pacified.",
     "the unrest code pacifies two for Gaia's Blessing (the wiki lists it too)"),
    (29, "Two rebels are pacified.",
     "Two rebels are pacified. Forests and nature nodes the city works add twice their usual production bonus.",
     "the production code doubles the bonus of forests and nature nodes under Gaia's Blessing"),
    (415, "and increases the unit's defense to 10 against magical attacks.",
     "and raises the unit's defense to at least 10 against them.",
     "the code gives defence 10 against normal weapons, not against magical attacks"),
    (433, "does half its attack strength in damage automatically",
     "does its full attack strength in damage automatically (half of it when the ability comes from an item or spell)",
     "the code halves doom damage only for a non-innate source; a unit's own is the full strength"),
    (478, "defense is increased by +3.", "defense is increased by +2.",
     "the code adds 2 defence (the spell text already says 2)"),
    (453, "+3 melee attack strength", "+2 melee attack strength",
     "the code adds 2 melee (the spell text already says 2)"),
    (464, "enemy units by -2.", "enemy units by -1.",
     "the code applies -1 to hit against invisible units"),
    (564, "enemy units by -2.", "enemy units by -1.",
     "the code applies -1 to hit against invisible units"),
    (51, "There is a 10% chance that each point of damage done to the target unit will miss.",
     "Attackers have a penalty of 1 (10%) to hit the target units, whatever the distance, unless they are immune to illusions.",
     "this game takes a flat 1 off the chance to hit; the original rolls a 10% miss for each point of damage"),
    (797, "There is a 10% chance that each point of damage done will miss.",
     "Attackers have a penalty of 1 (10%) to hit them, whatever the distance, unless they are immune to illusions.",
     "this game takes a flat 1 off the chance to hit; the original rolls a 10% miss for each point of damage"),
    (401, "Invisibility makes the unit susceptible only to ranged attacks when adjacent to an enemy, and otherwise reduces the attacker's chance to hit by 1.",
     "Invisibility reduces the attacker's chance to hit the unit by 1 for each square between them (1 when adjacent); attackers immune to illusions have no penalty. In battle the enemy cannot see the unit, and so cannot attack, shoot at or cast a spell on it, until it stands next to an enemy unit, attacks or casts a spell (it then stays in view, drawn as a ghost, until its side's next turn); spells that strike an area still reach it.",
     "the original takes 1 off the chance to hit whatever the distance; here the penalty grows with distance, and the unit also shows itself by attacking or casting"),
    (37, "between any two of the casting wizard's cities equipped with earth gates",
     "between any two of the casting wizard's cities on the same plane equipped with earth gates",
     "the original's code and this game only link cities on the same plane"),
    (750, "between any two cities held by the wizard.",
     "between any two cities held by the wizard on the same plane.",
     "the original's code and this game only link cities on the same plane"),
    (731, "reducing unrest by 2.", "reducing unrest by 2, and allows use of nightshade.",
     "the code lets an Oracle use nightshade"),
    (732, "magical weapons with +1 to hit.", "magical weapons with +1 to hit. It also allows use of nightshade.",
     "the code lets an Alchemists Guild use nightshade"),
    (734, "and +3 mana.", "and +3 mana. It also allows use of nightshade.",
     "the code lets a Wizard's Guild use nightshade"),
    (737, "reduces the unrest by 1.", "reduces the unrest by 1, and allows use of nightshade.",
     "the code lets a Parthenon use nightshade"),
    (738, "reduces the unrest level by 1.", "reduces the unrest level by 1, and allows use of nightshade.",
     "the code lets a Cathedral use nightshade"),
    (766, "against any bad city enchantment.",
     "against any bad city enchantment cast on the city, so a spell aimed at the city can fizzle. Only a city with a building that allows use of nightshade counts the plants in its area.",
     "the code checks each spell cast at the city, and only for cities with such a building"),
    (234, "If negative, units will starve to death.",
     "If negative, you are asked before the turn ends which units to let go (the cheapest and newest are marked); if you do not choose, the newest go. Heroes can be let go too and hired again, and their items return to the armory.",
     "task 292: the player (and the AI) chooses who goes when the food or gold runs out, instead of the newest units being dismissed unasked; a ship let go with passengers aboard sails for the nearest shore"),
    (231, "This is the total amount of gold in your treasury.",
     "This is the total amount of gold in your treasury. If the treasury cannot pay the troops' upkeep, you are asked before the turn ends which units to let go, and you may sell buildings or change taxes first; a ship let go with passengers aboard turns neutral and sails for the nearest shore, where it unloads them.",
     "task 292: the player (and the AI) chooses who is let go when the gold runs out"),
    (266, "the reserve will drop to zero, enchantments will fade away and magical creatures will disappear.",
     "the reserve will drop to zero, enchantments will fade away and magical creatures will disappear. Before the turn ends you are asked which of them to give up (the newest are marked).",
     "task 62: the player (and the AI) chooses what is given up when the mana runs out"),
]


def apply_help_corrections(topics: list["HelpTopic"], report: "Report") -> int:
    """Rewrite topics to match the game's code. The reason stays in the report, not in the text
    the player reads.

    Raises:
        ValueError: if a topic is missing or the text to replace is not found exactly once.
    """
    by_index = {t.index: t for t in topics}
    for index, old, new, why in HELP_CORRECTIONS:
        topic = by_index.get(index)
        if topic is None or topic.text.count(old) != 1:
            raise ValueError(f"help correction for topic {index} does not apply: {old!r}")
        topic.text = topic.text.replace(old, new)
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

    spell_rows = import_spell_rows(game)
    help_topics = import_help(game, report)
    apply_help_corrections(help_topics, report)

    local_header = "Generated by scripts/import_mom.py from the original game data. Do not commit."
    # The original's city names are IP: they stay local and replace the invented ones in
    # `content/city_names.ron` (races without a local entry keep those).
    write_list(out / "city_names.ron", local_header, [
        f"    (race: {ron_str(race.id)}, names: [{', '.join(ron_str(n) for n in city_names[race.index] if n)}]),"
        for race in races
    ])
    write_list(out / "hero_names.ron", local_header, [
        f"    (unit: {ron_str(uid)}, names: [{', '.join(ron_str(n) for n in names)}]),"
        for uid, names in import_hero_names(game, rules_out / "units.ron")
    ])
    write_list(out / "wizard_texts.ron", local_header, wizard_text_lines(help_topics))
    write_list(out / "event_texts.ron", local_header, import_event_texts(game))
    write_list(out / "diplomsg.ron", local_header, import_diplomacy_lines(game, report))
    building_texts = import_building_texts(game, buildings)
    write_list(out / "building_texts.ron", local_header, [
        f"    (building: {ron_str(b.id)}, text: {ron_str(t)})," for b, t in building_texts
    ])
    LOG.info("building description cross-check: %d mismatches",
             cross_check_building_texts(building_texts, rules_out, report))
    mismatches = cross_check_help(help_topics, buildings, races, spell_rows, report)
    LOG.info("help cross-check: %d mismatches", mismatches)
    # Which screen area opens which help topic is data (rectangles and topic numbers, no prose),
    # so it is committed; the topics' text is ours (`content/help.ron`), and the original's is only
    # cross-checked above, never written out or read by the game.
    write_list(
        rules_out / "help_screens.ron",
        "Generated by scripts/import_mom.py from HLPENTRY.LBX: the help areas of each screen "
        "in 320x200 coordinates and the topic each opens (-1: none).",
        import_help_screens(game, remom),
    )
    (out / "help_screens.ron").unlink(missing_ok=True)
    # Older runs wrote the rules files (and copies of terrain and features) here; the
    # committed ones in `rules_out` now apply, and a stale copy would shadow them.
    for stale in (*RULES_FILES, "terrain", "features", "wizards", "messages"):
        old = out / f"{stale}.ron"
        if old.exists() and old.resolve() != (rules_out / f"{stale}.ron").resolve():
            old.unlink()
            LOG.info("removed stale %s", old)

    summary = (
        f"races {len(races)} and buildings {len(buildings)} (checked only), "
        f"help topics {len(help_topics)}"
    )
    (out / "IMPORT_REPORT.txt").write_text(summary + "\n\n" + "\n".join(report.lines) + "\n", encoding="utf-8")
    LOG.info("imported: %s (%d notes in IMPORT_REPORT.txt)", summary, len(report.lines))
    return report


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--game", type=Path, default=DEFAULT_GAME)
    parser.add_argument("--remom", type=Path, default=DEFAULT_REMOM)
    parser.add_argument("--out", type=Path, default=REPO / "content-local",
                        help="local-only output (help text, wizards, report)")
    parser.add_argument("--rules-out", type=Path, default=REPO / "content",
                        help="committed rules data (races, units, buildings, spells, ...)")
    args = parser.parse_args(argv)
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    run(args.game, args.remom, args.out, args.rules_out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
