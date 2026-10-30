//! The shipped content against the original's own numbers (task 404). `tests/original/*.tsv` are
//! the original's tables as `scripts/export_original_*.py` read them from the ReMoM clone, in the
//! original's row order; the content rows are in the same order. A number of the content may
//! differ from the original only where `DEPARTURES` says so, with the reason.

use f4x_core::rules::Stat;
use std::collections::BTreeMap;

fn content() -> f4x_core::content::Content {
    f4x_data::load_content(std::path::Path::new("../../content")).expect("the rules data loads")
}

/// The rows of a TSV table as column name to value, one map per row.
fn table(text: &str) -> Vec<BTreeMap<String, i32>> {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().expect("a header").split('\t').collect();
    lines
        .map(|line| {
            head.iter()
                .zip(line.split('\t'))
                .map(|(name, v)| (name.to_string(), v.parse().expect("a number")))
                .collect()
        })
        .collect()
}

/// A number of a unit that is deliberately not the original's: `(unit id, column, our value,
/// reason)`.
const DEPARTURES: &[(&str, &str, i32, &str)] = &[(
    "chosen",
    "upkeep",
    12,
    "Torin the Chosen costs 12 mana a turn instead of no upkeep (docs/heroes-items-design.md, H10)",
)];

fn departure(id: &str, column: &str, ours: i32) -> bool {
    DEPARTURES
        .iter()
        .any(|(i, c, v, _)| *i == id && *c == column && *v == ours)
}

#[test]
fn every_unit_has_the_originals_numbers() {
    let c = content();
    let rows = table(include_str!("../original/units.tsv"));
    let defs = c.rules.units.defs();
    assert_eq!(
        defs.len(),
        rows.len(),
        "units in the content and in the original"
    );
    let mut wrong = Vec::new();
    for (def, row) in defs.iter().zip(&rows) {
        let mut row = row.clone();
        if i32::from(def.num) != row["index"] {
            wrong.push(format!(
                "{}: numbered {}, original {}",
                def.id, def.num, row["index"]
            ));
        }
        if row["ranged"] > 0 && row["ammo"] == 0 {
            row.insert("ammo".into(), 1);
        }
        let stat = |s: Stat| def.stats.get(&s).copied().unwrap_or(0);
        let ours = [
            ("melee", stat(Stat::Melee)),
            ("ranged", stat(Stat::Ranged)),
            // A ranged attack with no ammunition in the original (a magical one) has one shot of ours.
            ("ammo", stat(Stat::Ammo)),
            ("to_hit", stat(Stat::ToHit)),
            ("defense", stat(Stat::Defense)),
            ("resist", stat(Stat::Resistance)),
            ("moves_half", stat(Stat::Moves) * 2),
            ("cost", def.cost as i32),
            // Gold, or mana for a summoned creature; the food of a normal unit is a rule of its own.
            ("upkeep", (def.upkeep.gold + def.upkeep.mana) as i32),
            ("hits", stat(Stat::MaxHp)),
            // The original's scouting range in squares around the unit; ours counts the unit's own
            // square and a ring per point (`2 * range + 1`).
            ("sight", (stat(Stat::Sight) - 1) / 2),
            ("transport", stat(Stat::Transport)),
            ("figures", stat(Stat::Figures)),
        ];
        for (column, ours) in ours {
            if row[column] != ours && !departure(&def.id, column, ours) {
                wrong.push(format!(
                    "{} {column}: {ours}, original {}",
                    def.id, row[column]
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} numbers differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// Letters and digits in lower case, to compare names that differ in an apostrophe.
fn plain(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The records of the original's building table before the first real building (none, trade
/// goods, housing); the content starts at Barracks.
const FIRST_BUILDING: usize = 3;
/// A requirement of 100 and up in the original is a terrain, not a building.
const TERRAIN: i32 = 100;

#[test]
fn every_building_has_the_originals_cost_upkeep_requirements_and_replacement() {
    let c = content();
    let text = include_str!("../original/buildings.tsv");
    let rows: Vec<Vec<&str>> = text
        .lines()
        .skip(1)
        .map(|l| l.split('\t').collect())
        .collect();
    let defs = c.rules.buildings.defs();
    assert_eq!(defs.len(), rows.len() - FIRST_BUILDING, "buildings");
    let id_of = |record: i32| -> &str { &defs[record as usize - FIRST_BUILDING].id };
    let mut wrong = Vec::new();
    for (def, row) in defs.iter().zip(&rows[FIRST_BUILDING..]) {
        if i32::from(def.num) != row[0].parse::<i32>().unwrap() {
            wrong.push(format!(
                "{}: numbered {}, original {}",
                def.id, def.num, row[0]
            ));
            continue;
        }
        let num = |n: usize| -> i32 { row[n].parse().unwrap() };
        if plain(&def.name) != plain(row[1]) {
            wrong.push(format!("{}: named {} in the original", def.id, row[1]));
            continue;
        }
        if def.cost as i32 != num(2) {
            wrong.push(format!(
                "{} cost: {}, original {}",
                def.id,
                def.cost,
                num(2)
            ));
        }
        if def.upkeep_gold as i32 != num(3) {
            wrong.push(format!(
                "{} upkeep: {}, original {}",
                def.id,
                def.upkeep_gold,
                num(3)
            ));
        }
        let mut ours: Vec<&str> = def.requires.iter().map(String::as_str).collect();
        ours.sort_unstable();
        let mut theirs: Vec<&str> = [num(4), num(5)]
            .into_iter()
            .filter(|&r| r > 0 && r < TERRAIN)
            .map(id_of)
            .collect();
        theirs.sort_unstable();
        if ours != theirs {
            wrong.push(format!("{} requires {ours:?}, original {theirs:?}", def.id));
        }
        let replaces = (num(6) > 0).then(|| id_of(num(6)));
        if def.replaces.as_deref() != replaces {
            wrong.push(format!(
                "{} replaces {:?}, original {replaces:?}",
                def.id, def.replaces
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// The rows of a TSV table with a name column, as the cells of each row by column name.
fn named_table(text: &str) -> Vec<BTreeMap<String, String>> {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().expect("a header").split('\t').collect();
    lines
        .map(|line| {
            head.iter()
                .zip(line.split('\t'))
                .map(|(name, v)| (name.to_string(), v.to_string()))
                .collect()
        })
        .collect()
}

fn number(row: &BTreeMap<String, String>, column: &str) -> i64 {
    row[column].parse().unwrap()
}

/// The realms in the original's numbering.
const REALMS: [&str; 6] = ["nature", "sorcery", "chaos", "life", "death", "arcane"];

#[test]
fn every_item_power_has_the_originals_types_cost_group_realm_and_books() {
    let c = content();
    let rows = named_table(include_str!("../original/item_powers.tsv"));
    let defs = c.rules.item_powers.defs();
    assert_eq!(defs.len(), rows.len(), "item powers");
    let mut wrong = Vec::new();
    for (def, row) in defs.iter().zip(&rows) {
        if i64::from(def.num) != number(row, "index") {
            wrong.push(format!(
                "{}: numbered {}, original {}",
                def.id, def.num, row["index"]
            ));
            continue;
        }
        if plain(&def.name) != plain(&row["name"]) {
            wrong.push(format!("{}: named {} in the original", def.id, row["name"]));
            continue;
        }
        let kinds = def.kinds.iter().fold(0, |m, k| m | 1 << *k as i64);
        if kinds != number(row, "kinds") {
            wrong.push(format!(
                "{} types: {kinds:#b}, original {:#b}",
                def.id,
                number(row, "kinds")
            ));
        }
        if i64::from(def.cost) != number(row, "cost") {
            wrong.push(format!(
                "{} cost: {}, original {}",
                def.id, def.cost, row["cost"]
            ));
        }
        if i64::from(def.group) != number(row, "group") {
            wrong.push(format!(
                "{} group: {}, original {}",
                def.id, def.group, row["group"]
            ));
        }
        // Only a special power needs books and a realm: the content leaves them off a plain
        // bonus, whose book byte the original scrambled (docs/heroes-items-design.md 2.1).
        if let Some(realm) = &def.realm {
            let realm = REALMS.iter().position(|n| n == realm).map(|r| r as i64);
            if realm != Some(number(row, "realm")) {
                wrong.push(format!(
                    "{} realm {realm:?}, original {}",
                    def.id, row["realm"]
                ));
            }
            if i64::from(def.books) != number(row, "books") {
                wrong.push(format!(
                    "{} books: {}, original {}",
                    def.id, def.books, row["books"]
                ));
            }
        } else if def.bonus.is_none() && number(row, "mask") != 0 {
            wrong.push(format!("{} has a power mask but no realm", def.id));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn every_prefab_item_has_the_originals_type_cost_bonuses_and_powers() {
    use f4x_core::rules::effects::Effect;
    use f4x_core::rules::ModOp;
    let c = content();
    let rows = named_table(include_str!("../original/items.tsv"));
    let powers = named_table(include_str!("../original/item_powers.tsv"));
    let defs = c.rules.items.defs();
    assert_eq!(defs.len(), rows.len(), "items");
    let mut wrong = Vec::new();
    for (def, row) in defs.iter().zip(&rows) {
        if i64::from(def.num) != number(row, "index") {
            wrong.push(format!(
                "{}: numbered {}, original {}",
                def.id, def.num, row["index"]
            ));
            continue;
        }
        // The two names the original uses twice carry I and II in the content.
        let (ours, theirs) = (plain(&def.name), plain(&row["name"]));
        if ours != theirs && ours != theirs.clone() + "i" && ours != theirs + "ii" {
            wrong.push(format!("{}: named {} in the original", def.id, row["name"]));
            continue;
        }
        let bonus = |s: Stat| -> i64 {
            def.effects
                .iter()
                .map(|r| match r.effect {
                    Effect::Modify {
                        stat,
                        op: ModOp::Add,
                        value,
                    } if stat == s => i64::from(value),
                    _ => 0,
                })
                .sum()
        };
        let checks = [
            ("type", def.kind as i64, number(row, "type")),
            ("cost", i64::from(def.cost), number(row, "cost")),
            ("attack", bonus(Stat::Melee), number(row, "attack")),
            ("to_hit", bonus(Stat::ToHit), number(row, "to_hit")),
            ("defense", bonus(Stat::Defense), number(row, "defense")),
            // The file stores whole moves; the content counts half-moves.
            ("moves", bonus(Stat::HalfMoves), 2 * number(row, "moves")),
            ("resist", bonus(Stat::Resistance), number(row, "resist")),
            (
                "spell_skill",
                i64::from(def.spell_skill),
                number(row, "spell_skill"),
            ),
            (
                "spell_save",
                i64::from(def.spell_save),
                number(row, "spell_save"),
            ),
            (
                "charges",
                def.charges.as_ref().map_or(0, |(_, n)| i64::from(*n)),
                number(row, "charges"),
            ),
            ("flag", i64::from(def.flag), number(row, "flag")),
        ];
        for (column, ours, theirs) in checks {
            if ours != theirs {
                wrong.push(format!("{} {column}: {ours}, original {theirs}", def.id));
            }
        }
        // The powers: the bits of the powers it names are the bits the file sets.
        let mask = def.powers.iter().fold(0i64, |m, id| {
            let at = c.rules.item_powers.id_of(id).map(|i| i.num() as usize);
            m | at.map_or(0, |at| number(&powers[at], "mask"))
        });
        if mask != number(row, "powers") {
            wrong.push(format!(
                "{} powers: {:?} make {mask:#x}, original {:#x}",
                def.id,
                def.powers,
                number(row, "powers")
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn every_race_has_the_originals_growth_houses_forbidden_buildings_and_units() {
    use f4x_core::rules::{effects::Effect, ModOp, UnitKind};
    let c = content();
    let rows = named_table(include_str!("../original/races.tsv"));
    let defs = c.rules.races.defs();
    assert_eq!(defs.len(), rows.len(), "races");
    let buildings = c.rules.buildings.defs();
    let units = c.rules.units.defs();
    let unit_rows = table(include_str!("../original/units.tsv"));
    let mut wrong = Vec::new();
    for (index, (def, row)) in defs.iter().zip(&rows).enumerate() {
        if i64::from(def.num) != number(row, "index") {
            wrong.push(format!(
                "{}: numbered {}, original {}",
                def.id, def.num, row["index"]
            ));
            continue;
        }
        if plain(&def.name) != plain(&row["name"]) {
            wrong.push(format!("{}: named {} in the original", def.id, row["name"]));
            continue;
        }
        let growth: i32 = def
            .effects
            .iter()
            .filter_map(|e| match &e.effect {
                Effect::Modify {
                    stat: Stat::CityGrowth,
                    op: ModOp::Add,
                    value,
                } => Some(*value),
                _ => None,
            })
            .sum();
        for (what, ours, theirs) in [
            (
                "outpost growth",
                i64::from(def.outpost_growth),
                number(row, "outpost_growth"),
            ),
            (
                "house type",
                i64::from(def.house_type),
                number(row, "house_type"),
            ),
            // The original's modifier is in tens of percent.
            (
                "city growth",
                i64::from(growth),
                number(row, "growth_mod") * 10,
            ),
        ] {
            if ours != theirs {
                wrong.push(format!("{} {what}: {ours}, original {theirs}", def.id));
            }
        }
        // A building with no race list is open to every race.
        let mut cannot: Vec<&str> = buildings
            .iter()
            .filter(|b| !b.races.is_empty() && !b.races.contains(&def.id))
            .map(|b| b.id.as_str())
            .collect();
        cannot.sort_unstable();
        let mut original: Vec<&str> = row["cant_build"]
            .split_whitespace()
            .map(|r| {
                buildings[r.parse::<usize>().unwrap() - FIRST_BUILDING]
                    .id
                    .as_str()
            })
            .collect();
        original.sort_unstable();
        if cannot != original {
            wrong.push(format!(
                "{} cannot build {cannot:?}, original {original:?}",
                def.id
            ));
        }
        let mut ours: Vec<&str> = def.units.iter().map(String::as_str).collect();
        ours.sort_unstable();
        let mut theirs: Vec<&str> = units
            .iter()
            .zip(&unit_rows)
            // Heroes carry a race in the original but are hired, not trained by a city.
            .filter(|(u, r)| u.kind == UnitKind::Normal && r["race"] == index as i32)
            .map(|(u, _)| u.id.as_str())
            .collect();
        theirs.sort_unstable();
        if ours != theirs {
            wrong.push(format!("{} units {ours:?}, original {theirs:?}", def.id));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// Casting cost of a spell the original leaves at 0 because the caster chooses it, and the cost
/// our content gives it instead: `(spell id, our cost, reason)`.
const SPELL_DEPARTURES: &[(&str, u32, &str)] = &[
    (
        "enchant_item",
        1,
        "the original asks for the mana of the item's powers; ours starts at 1",
    ),
    (
        "create_artifact",
        1,
        "the original asks for the mana of the item's powers; ours starts at 1",
    ),
];

#[test]
fn every_spell_has_the_originals_name_realm_cost_research_rarity_and_summoned_unit() {
    use f4x_core::rules::{effects::Effect, Rarity};
    let c = content();
    let rows = named_table(include_str!("../original/spells.tsv"));
    let defs = c.rules.spells.defs();
    assert_eq!(defs.len(), rows.len(), "spells");
    let units = c.rules.units.defs();
    // Each realm's spells come as ten common, ten uncommon, ten rare and ten very rare, in the
    // original's order.
    let tiers = [
        Rarity::Common,
        Rarity::Uncommon,
        Rarity::Rare,
        Rarity::VeryRare,
    ];
    let mut seen = [0usize; 6];
    let mut wrong = Vec::new();
    for (def, row) in defs.iter().zip(&rows) {
        if plain(&def.name) != plain(&row["name"]) {
            wrong.push(format!("{}: named {} in the original", def.id, row["name"]));
            continue;
        }
        // The spell's number is the original's record number (the table starts at record 1).
        if u32::from(def.num) != number(row, "index") as u32 + 1 {
            wrong.push(format!(
                "{}: numbered {}, original {}",
                def.id,
                def.num,
                number(row, "index") + 1
            ));
        }
        let realm = number(row, "realm") as usize;
        if def.realm != REALMS[realm] {
            wrong.push(format!(
                "{} realm {}, original {}",
                def.id, def.realm, REALMS[realm]
            ));
        }
        let tier = tiers[(seen[realm] / 10).min(3)];
        seen[realm] += 1;
        if def.rarity != tier {
            wrong.push(format!(
                "{} rarity {:?}, original {tier:?}",
                def.id, def.rarity
            ));
        }
        let cost = number(row, "cost");
        let departs = SPELL_DEPARTURES
            .iter()
            .any(|(id, ours, _)| *id == def.id && i64::from(*ours) == i64::from(def.cost));
        if i64::from(def.cost) != cost && !departs {
            wrong.push(format!("{} cost {}, original {cost}", def.id, def.cost));
        }
        if i64::from(def.research_cost) != number(row, "research") {
            wrong.push(format!(
                "{} research {}, original {}",
                def.id,
                def.research_cost,
                number(row, "research")
            ));
        }
        // Category 0 is a summoning; its first parameter is the unit it creates.
        if number(row, "category") == 0 {
            let original = &units[number(row, "param") as usize].id;
            let ours: Vec<&String> = def
                .effects
                .iter()
                .filter_map(|e| match &e.effect {
                    Effect::Summon { unit, .. } => Some(unit),
                    // The Floating Island is a unit in the original; ours is a custom effect of the same name.
                    Effect::Custom(name) if *name == def.id => Some(name),
                    _ => None,
                })
                .collect();
            if ours != [original] {
                wrong.push(format!("{} summons {ours:?}, original {original}", def.id));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// The unit abilities the original keeps as bits, as `(column, bit, our ability id)`: the movement
/// flags, the first attribute word, the second (what the unit casts as if it had the spell) and
/// the ability word (`ReMoM`'s `MOM_DEF.h`). The ability word's `Fantastic` bit is the unit's kind
/// and its `Land Corruption` bit has no ability of ours.
const FLAG_ABILITIES: &[(&str, i32, &str)] = &[
    ("move_flags", 0x01, "cavalry"),
    ("move_flags", 0x10, "teleport"),
    ("move_flags", 0x20, "forester"),
    ("move_flags", 0x40, "mountaineer"),
    ("attribs1", 0x0001, "fire_immunity"),
    ("attribs1", 0x0002, "stoning_immunity"),
    ("attribs1", 0x0004, "missile_immunity"),
    ("attribs1", 0x0008, "illusion_immunity"),
    ("attribs1", 0x0010, "cold_immunity"),
    ("attribs1", 0x0020, "magic_immunity"),
    ("attribs1", 0x0040, "death_immunity"),
    ("attribs1", 0x0080, "poison_immunity"),
    ("attribs1", 0x0100, "weapon_immunity"),
    ("attribs1", 0x0200, "flying"),
    ("attribs1", 0x0400, "lucky"),
    ("attribs2", 0x01, "casts_healing"),
    ("attribs2", 0x02, "casts_fireball"),
    ("attribs2", 0x04, "casts_doom_bolt"),
    ("attribs2", 0x08, "immolation"),
    ("attribs2", 0x10, "casts_web"),
    ("attribs2", 0x20, "cause_fear"),
    ("attribs2", 0x40, "resist_all"),
    ("attribs2", 0x80, "holy_bonus"),
    ("abilities", 0x0002, "large_shield"),
    ("abilities", 0x0004, "planar_travel"),
    ("abilities", 0x0008, "wall_crusher"),
    ("abilities", 0x0010, "healer"),
    ("abilities", 0x0020, "create_outpost"),
    ("abilities", 0x0040, "invisibility"),
    ("abilities", 0x0080, "create_undead"),
    ("abilities", 0x0100, "long_range"),
    ("abilities", 0x0400, "meld"),
    ("abilities", 0x0800, "non_corporeal"),
    ("abilities", 0x1000, "wind_walking"),
    ("abilities", 0x2000, "regeneration"),
    ("abilities", 0x4000, "purify"),
    ("abilities", 0x8000, "negate_first_strike"),
];

#[test]
fn every_unit_has_the_abilities_the_originals_flags_give_it() {
    let c = content();
    let rows = table(include_str!("../original/units.tsv"));
    let defs = c.rules.units.defs();
    let mut wrong = Vec::new();
    let mut compared = 0;
    for (def, row) in defs.iter().zip(&rows) {
        let mut theirs: Vec<&str> = FLAG_ABILITIES
            .iter()
            .filter(|(column, bit, _)| row[*column] & bit != 0)
            .map(|(_, _, id)| *id)
            .collect();
        theirs.sort_unstable();
        // Only the abilities the flags cover: the rest (attack types, gazes, breaths) are not bits.
        let mut ours: Vec<&str> = def
            .abilities
            .iter()
            .map(String::as_str)
            .filter(|a| FLAG_ABILITIES.iter().any(|(_, _, id)| id == a))
            .collect();
        ours.sort_unstable();
        compared += theirs.len();
        if ours != theirs {
            wrong.push(format!(
                "{} has {ours:?}, original flags {theirs:?}",
                def.id
            ));
        }
    }
    assert!(
        compared > 100,
        "the walk compared only {compared} abilities"
    );
    assert!(
        wrong.is_empty(),
        "{} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// The original's retorts ("special abilities") in the order of its wizard record
/// (`wsa_*` of `NewGame.c`), which is `Retort::LISTED`.
const ORIGINAL_RETORTS: [&str; 18] = [
    "Alchemy",
    "Warlord",
    "ChaosMastery",
    "NatureMastery",
    "SorceryMastery",
    "InfernalPower",
    "DivinePower",
    "SageMaster",
    "Channeler",
    "Myrran",
    "Archmage",
    "ManaFocusing",
    "NodeMastery",
    "Famous",
    "Runemaster",
    "Conjurer",
    "Charismatic",
    "Artificer",
];

/// Picks a retort costs in the original's creation screen: one, two for the five of
/// `TBL_2pickRetort_Lbls` (Warlord, Channeler, Divine Power, Famous, Infernal Power), three for Myrran.
fn original_retort_cost(name: &str) -> u8 {
    match name {
        "Warlord" | "Channeler" | "DivinePower" | "Famous" | "InfernalPower" => 2,
        "Myrran" => 3,
        _ => 1,
    }
}

/// Whether the original's creation screen lets the wizard pick the retort with these books
/// (`wsa_picklist_start` and the unlocks after it), not counting the pick budget. `n` is
/// life, death, chaos, nature, sorcery.
fn original_retort_available(name: &str, n: [u8; 5]) -> bool {
    let [life, death, chaos, nature, sorcery] = n;
    let realms = n.iter().filter(|&&b| b > 0).count();
    let strong_realms = n.iter().filter(|&&b| b > 1).count();
    let mastery_book = n.iter().any(|&b| b > 3);
    match name {
        "Alchemy" | "Warlord" | "Channeler" | "Artificer" | "Conjurer" | "Myrran" | "Famous"
        | "Charismatic" => true,
        "SageMaster" => realms > 1,
        "Runemaster" => strong_realms > 2,
        "Archmage" | "ManaFocusing" => mastery_book,
        "ChaosMastery" => chaos > 3,
        "NatureMastery" => nature > 3,
        "SorceryMastery" => sorcery > 3,
        "InfernalPower" => death > 3,
        "DivinePower" => life > 3,
        "NodeMastery" => chaos > 0 && sorcery > 0 && nature > 0,
        other => panic!("{other} is not a retort of the original"),
    }
}

#[test]
fn every_retort_costs_and_unlocks_as_in_the_originals_creation_screen() {
    use f4x_core::wizard::{magic::Retort, retort_available, retort_cost, ALL_RETORTS};
    assert_eq!(Retort::LISTED.len(), ORIGINAL_RETORTS.len());
    for (retort, name) in Retort::LISTED.iter().zip(ORIGINAL_RETORTS) {
        assert_eq!(
            format!("{retort:?}"),
            name,
            "the retort list follows the original's order"
        );
        assert!(
            ALL_RETORTS.contains(retort),
            "{name} is on the creation screen"
        );
        assert_eq!(
            retort_cost(*retort),
            original_retort_cost(name),
            "{name} cost"
        );
    }
    let realms = ["life", "death", "chaos", "nature", "sorcery"];
    let mut checked = 0;
    // Every way to spend up to eleven picks on books.
    for life in 0..=11u8 {
        for death in 0..=11 - life {
            for chaos in 0..=11 - life - death {
                for nature in 0..=11 - life - death - chaos {
                    for sorcery in 0..=11 - life - death - chaos - nature {
                        let n = [life, death, chaos, nature, sorcery];
                        let books = realms
                            .iter()
                            .zip(n)
                            .map(|(r, b)| (r.to_string(), b))
                            .collect();
                        for (retort, name) in Retort::LISTED.iter().zip(ORIGINAL_RETORTS) {
                            assert_eq!(
                                retort_available(*retort, &books),
                                original_retort_available(name, n),
                                "{name} with life {life}, death {death}, chaos {chaos}, nature {nature}, sorcery {sorcery}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 50_000, "only {checked} cases checked");
}

/// The original's ready-made wizards (`_wizard_presets_table`): name, books in the order life,
/// sorcery, nature, death, chaos, and the retort's place in `ORIGINAL_RETORTS` (none for Raven).
const ORIGINAL_WIZARDS: [(&str, [u8; 5], Option<usize>); 14] = [
    ("Merlin", [5, 0, 5, 0, 0], Some(7)),
    ("Raven", [0, 6, 5, 0, 0], None),
    ("Sharee", [0, 0, 0, 5, 5], Some(15)),
    ("Lo Pan", [0, 5, 0, 0, 5], Some(8)),
    ("Jafar", [0, 10, 0, 0, 0], Some(0)),
    ("Oberic", [0, 0, 5, 0, 5], Some(11)),
    ("Rjak", [0, 0, 0, 9, 0], Some(5)),
    ("Sss'ra", [4, 0, 0, 0, 4], Some(9)),
    ("Tauron", [0, 0, 0, 0, 10], Some(2)),
    ("Freya", [0, 0, 10, 0, 0], Some(3)),
    ("Horus", [5, 5, 0, 0, 0], Some(10)),
    ("Ariel", [10, 0, 0, 0, 0], Some(16)),
    ("Tlaloc", [0, 0, 4, 5, 0], Some(1)),
    ("Kali", [0, 5, 0, 5, 0], Some(17)),
];

#[test]
fn every_ready_made_wizard_has_the_originals_books_and_retort() {
    use f4x_core::wizard::magic::Retort;
    let c = content();
    let defs = c.rules.wizards.defs();
    assert_eq!(defs.len(), ORIGINAL_WIZARDS.len(), "ready-made wizards");
    let realms = ["life", "sorcery", "nature", "death", "chaos"];
    for (def, (name, books, retort)) in defs.iter().zip(ORIGINAL_WIZARDS) {
        assert_eq!(
            plain(&def.id),
            plain(name),
            "wizards come in the original's order"
        );
        let mut ours: Vec<(&str, u8)> = def.books.iter().map(|(r, b)| (r.as_str(), *b)).collect();
        ours.sort_unstable();
        let mut theirs: Vec<(&str, u8)> = realms
            .iter()
            .zip(books)
            .filter(|(_, b)| *b > 0)
            .map(|(r, b)| (*r, b))
            .collect();
        // Lo Pan has one sorcery book fewer than the original (docs/divergences.md).
        if def.id == "lo_pan" {
            theirs.iter_mut().for_each(|(r, b)| {
                if *r == "sorcery" {
                    *b -= 1;
                }
            });
        }
        theirs.sort_unstable();
        assert_eq!(ours, theirs, "{} books", def.id);
        let retorts: Vec<Retort> = retort.map(|i| Retort::LISTED[i]).into_iter().collect();
        assert_eq!(def.retorts, retorts, "{} retorts", def.id);
    }
}
