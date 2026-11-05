#!/usr/bin/env python3
"""Write the font colour ramps the original's screens make with Set_Font_Colors_15.

A ramp is a colour set of its own (the original fills its custom set 15 per call): the palette
indexes in RAMPS below, resolved to RGB through the base palette plus the colours of the screen's
own picture (`docs/palette_screens.tsv`). The atlases of `f4x-extract art` append the ramps
after the original's 16 sets, so a ramp is set 16 + its position, which `font_ramp.rs` names.
Usage: scripts/font_ramps.py   (writes content/font_colour_ramps.ron and crates/f4x-client/src/font_ramp.rs)
NOTE: the finished fonts in crates/f4x-client/assets/fonts are hand-edited (accents and more) and are the
source of truth. Do not copy f4x-extract art output over them (docs/extract-art.md section 5).
A set entry the screen leaves out repeats its last colour (the original leaves the earlier colours there).
"""
import csv
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RAMPS_RON = ROOT / "content/font_colour_ramps.ron"
RAMPS_RS = ROOT / "crates/f4x-client/src/font_ramp.rs"

# (constant name, screen picture whose palette gives indexes 224+ as (lbx stem, entry) or None,
#  palette indexes of set entries 0.., where in ReMoM it is made)
RAMPS = [
    ("LIST_TITLE_FAR", ("armylist", 0), [237] * 5, "the title of the army and city lists, the farthest copy (ArmyList.c, CityList.c)"),
    ("LIST_TITLE_MID", ("armylist", 0), [238] * 5, "the title of the army and city lists, the middle copies"),
    ("LIST_TITLE_NEAR", ("armylist", 0), [239] * 5, "the title of the army and city lists, the nearest copy"),
    ("LIST_TITLE", ("armylist", 0), [236, 177, 178, 177, 176], "the title of the army and city lists, the gold on top"),
    ("LIST_TEXT", ("armylist", 0), [236, 129, 129, 177, 176], "the text of the army and city lists"),
    ("CITY_LIST_TEXT", ("armylist", 0), [236, 129, 129, 239, 239], "the text of the city list (CityList.c: the entries 3 and 4 are left from the title)"),
    ("GOLD_TEXT", None, [0, 178, 179, 180, 181, 182], "the gold of titles and names on the Advisor and wizard screens (AdvsrScr.c, WIZVIEW.c: colors[i] = 177 + i)"),
    ("UV_TITLE", ("unitview", 0), [197, 14, 255, 80, 255], "unit view: names and labels in style 4 (Thing_View_Draw__WIP)"),
    ("UV_STATS", ("unitview", 0), [197, 255, 255, 80, 255], "unit view: figures and labels in styles 1 and 2"),
    ("UV_LIST", ("unitview", 0), [197, 255, 80, 255], "unit view: the abilities list (USW_DrawListPage)"),
    ("PROD_NORMAL", ("unitview", 0), [251, 0], "production list: an item you can pick (ProdScr.c)"),
    ("PROD_PICKED", ("unitview", 0), [244, 255], "production list: the picked item"),
    ("PROD_BLOCKED", ("unitview", 0), [4, 5], "production list: a unit the full garrison cannot take"),
    ("CITY_FIG", None, [190, 179], "city screen: the race, population and turns (City_Screen_Draw2__WIP)"),
    ("NAME_INPUT", None, [97, 97, 98, 98, 99, 99, 100, 100, 101, 101], "the city name being typed (Change_City_Name_Popup)"),
    ("SURVEY", None, [8, 161, 161], "the surveyor's and road planner's text (Surveyor_IDK_Set_Font_Stuff__1)"),
    ("ITEM_NAME", ("armylist", 5), [225, 179], "item screen: the hero's name (ItemScrn.c)"),
    ("ITEM_SAME", ("armylist", 5), [225, 157], "item screen: Same Location"),
    ("ITEM_TITLE", None, [19, 177, 178, 179, 179], "item making: the spell page title (ItemMake.c colors1)"),
    ("CRAFT_POWER_ON", None, [0, 177, 178, 179], "item making: a power that is picked (m_itemmake_colors1)"),
    ("CRAFT_POWER_OFF", None, [0, 31, 29, 28], "item making: a power that is not picked (m_itemmake_colors2)"),
    ("CRAFT_COST", None, [17, 9, 8, 8], "item making: the cost (COL_DeselectItemName)"),
    ("UNIT_MARK", None, [10, 15], "the B and G of a unit in the main screen's unit window (MainScr.c)"),
    ("NOTIFY_TEXT", None, [25, 178, 179, 180, 180], "the text of the notification boxes (Notify1_Draw: notify_text_colors)"),
    ("INTRO_COPYRIGHT", None, [79, 163], "the copyright line of the intro (INTRO.c)"),
    ("DIALOG_TEXT", None, [6, 9, 11, 13, 15], "the text of the confirmation box (COL_Dialog_Text)"),
    ("HOF_TITLE", ("halofam", 0), [246, 49, 176, 162, 178, 179, 51, 180, 184, 185, 213, 186, 246, 246, 246, 246], "hall of fame: title and entries (SCORE.c color_array)"),
    ("LEVEL_TITLE", None, [240, 178, 179, 180, 180], "level up: the title (LVLMAKE.c)"),
    ("LEVEL_TEXT", None, [240, 178, 178, 178, 178], "level up: the lines (LVLMAKE.c)"),
    ("BOOK_NAME", None, [184, 187], "spellbook: a spell's name and cost (Spellbook.c colors 184, 187)"),
    ("BOOK_TITLE", None, [53, 46], "spellbook: the page title (Spellbook.c colors 53, 46)"),
    ("BATTLE_NAME_0", None, [97, 98, 99, 100], "battle: the player name of banner 0 (combat_name_font_colors_by_banner)"),
    ("BATTLE_NAME_1", None, [66, 67, 68, 69], "battle: the player name of banner 1 (combat_name_font_colors_by_banner)"),
    ("BATTLE_NAME_2", None, [33, 34, 35, 36], "battle: the player name of banner 2 (combat_name_font_colors_by_banner)"),
    ("BATTLE_NAME_3", None, [201, 202, 203, 166], "battle: the player name of banner 3 (combat_name_font_colors_by_banner)"),
    ("BATTLE_NAME_4", None, [160, 161, 162, 163], "battle: the player name of banner 4 (combat_name_font_colors_by_banner)"),
    ("BATTLE_NAME_5", None, [28, 27, 26, 25], "battle: the player name of banner 5 (combat_name_font_colors_by_banner)"),
    ("ALCHEMY_AMOUNT", None, [192, 179, 172, 0, 0, 0], "Alchemy_Popup_Draw: colors[]"),
]


# Palette indexes the original makes a one-colour set of (`colors[0..5] = banner_colors[...]`): the
# banner colours of the wizards' names and enchantments. `flat(index)` in `font_ramp.rs` gives the set.
FLATS = [0, 6, 39, 43, 98, 166, 50, 79, 123, 124, 125, 127, 172, 179, 201, 211, 216, 218, 221, 222, 5, 45, 47, 55, 177, 13, 114, 16, 31, 187, 178, 2, 20, 22, 176, 180]


def palette_lookup():
    base = {}
    with open(ROOT / "docs/palette.tsv") as f:
        for r in csv.DictReader(f, delimiter="\t"):
            base[int(r["index"])] = (int(r["r"]), int(r["g"]), int(r["b"]))
    screens = {}
    with open(ROOT / "docs/palette_screens.tsv") as f:
        for r in csv.DictReader(f, delimiter="\t"):
            for entry in r["entries"].split(","):
                screens[(r["source"], entry, int(r["index"]))] = (int(r["r"]), int(r["g"]), int(r["b"]))

    def look(screen, index):
        if index < 224 or screen is None:
            return base[index]
        key = (screen[0], str(screen[1]), index)
        if key not in screens:
            raise SystemExit(f"no colour {index} in {screen}")
        return screens[key]

    return look


def main() -> int:
    look = palette_lookup()
    ron = [
        "// Colour sets of the original's screens (Set_Font_Colors_15), appended after the 16 of the fonts.",
        "// Written by scripts/font_ramps.py. colours: glyph colour indexes 1 to 15 as (r, g, b).",
        "(",
        "    ramps: [",
    ]
    rust = [
        "//! The colour sets the original's screens make with `Set_Font_Colors_15`, by number.",
        "//! Generated by `scripts/font_ramps.py` from `content/font_colour_ramps.ron`: do not edit.",
        "",
        "#![allow(dead_code)]",
        "",
    ]
    RAMPS_ALL = RAMPS + [(f"FLAT_{i}", None, [i] * 6, f"palette colour {i} on its own") for i in FLATS]
    for k, (name, screen, indexes, origin) in enumerate(RAMPS_ALL):
        colours = [look(screen, indexes[min(c, len(indexes) - 1)]) for c in range(1, 16)]
        body = ", ".join(f"({r}, {g}, {b})" for r, g, b in colours)
        ron.append(f'        (name: "{name.lower()}", colours: [{body}]),')
        rust.append(f"/// {origin}.")
        rust.append(f"pub(crate) const {name}: usize = {16 + k};")
    rust.append("/// How many ramps there are; the atlases of styles 0 to 5 stack 16 + this many colour sets.")
    rust.append(f"pub(crate) const COUNT: usize = {len(RAMPS_ALL)};")
    rust.append("")
    rust.append("/// The colour set of one palette colour on its own (`FLATS` in the script); a colour not")
    rust.append("/// listed there panics: add it to `FLATS` and run the script again.")
    rust.append("pub(crate) fn flat(index: u8) -> usize {")
    rust.append("    match index {")
    for i in FLATS:
        rust.append(f"        {i} => FLAT_{i},")
    rust.append('        other => panic!("{other}"),')
    rust.append("    }")
    rust.append("}")
    ron += ["    ],", ")", ""]
    RAMPS_RON.write_text("\n".join(ron))
    RAMPS_RS.write_text("\n".join(rust) + "\n")
    print(f"wrote {len(RAMPS_ALL)} ramps")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
