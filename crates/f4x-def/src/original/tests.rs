use super::*;
use crate::rules::Stat;

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
    let c = crate::testdir::shipped();
    let rows = table(UNITS);
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
        let stat = |s: Stat| def.stats.get(&s).map_or(0, |v| v.floor());
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
            ("upkeep", def.upkeep.gold.floor() + def.upkeep.mana as i32),
            ("hits", stat(Stat::MaxHp)),
            // The original's scouting range in squares around the unit; ours is that and a half
            // (`range + 0.5`).
            ("sight", stat(Stat::Sight)),
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

/// The records of the original's building table before the first real building (none, trade
/// goods, housing); the content starts at Barracks.
const FIRST_BUILDING: usize = 3;
/// A requirement of 100 and up in the original is a terrain, not a building.
const TERRAIN: i32 = 100;

#[test]
fn every_building_has_the_originals_cost_upkeep_requirements_and_replacement() {
    let c = crate::testdir::shipped();
    let text = BUILDINGS;
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
        if i32::from(def.num) != row[0].parse::<i32>().expect("a record number") {
            wrong.push(format!(
                "{}: numbered {}, original {}",
                def.id, def.num, row[0]
            ));
            continue;
        }
        let num = |n: usize| -> i32 { row[n].parse().expect("a number") };
        if plain(&def.id) != plain(row[1]) {
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
        let mut ours: Vec<&str> = def
            .requires
            .iter()
            .map(|b| c.rules.buildings.get(*b).id.as_str())
            .collect();
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
        if def.replaces.map(|b| c.rules.buildings.get(b).id.as_str()) != replaces {
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

#[test]
fn every_item_power_has_the_originals_types_cost_group_realm_and_books() {
    let c = crate::testdir::shipped();
    let rows = named_table(ITEM_POWERS);
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
        // The one power we changed: Merging is craftable (6 books, 1500; `docs/divergences.md` 44).
        if def.id == "merging" {
            assert_eq!(
                (def.cost, def.books),
                (1500, 6),
                "the divergence of Merging"
            );
            assert_eq!(number(row, "books"), 22, "the original's books for Merging");
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
        if let Some(realm) = def.realm {
            let realm = Some(i64::from(realm.num()));
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
    use crate::rules::effects::Effect;
    use crate::rules::ModOp;
    let c = crate::testdir::shipped();
    let rows = named_table(ITEMS);
    let powers = named_table(ITEM_POWERS);
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
        let (ours, theirs) = (plain(&def.id), plain(&row["name"]));
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
                    } if stat == s => i64::from(value.floor()),
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
            ("moves", bonus(Stat::Moves), number(row, "moves")),
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
        let mask = def
            .powers
            .iter()
            .fold(0i64, |m, id| m | number(&powers[id.num() as usize], "mask"));
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
    use crate::rules::{effects::Effect, ModOp, UnitKind};
    let c = crate::testdir::shipped();
    let rows = named_table(RACES);
    let defs = c.rules.races.defs();
    assert_eq!(defs.len(), rows.len(), "races");
    let buildings = c.rules.buildings.defs();
    let units = c.rules.units.defs();
    let unit_rows = table(UNITS);
    let mut wrong = Vec::new();
    for (index, (def, row)) in defs.iter().zip(&rows).enumerate() {
        if i64::from(def.num) != number(row, "index") {
            wrong.push(format!(
                "{}: numbered {}, original {}",
                def.id, def.num, row["index"]
            ));
            continue;
        }
        if plain(&def.id) != plain(&row["name"]) {
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
                } => Some(value.floor()),
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
            .filter(|b| !b.buildable_by(crate::rules::RaceId::new(def.num)))
            .map(|b| b.id.as_str())
            .collect();
        cannot.sort_unstable();
        let mut original: Vec<&str> = row["cant_build"]
            .split_whitespace()
            .map(|r| {
                buildings[r.parse::<usize>().expect("a building number") - FIRST_BUILDING]
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
        let mut ours: Vec<&str> = def
            .units
            .iter()
            .map(|u| c.rules.units.get(*u).id.as_str())
            .collect();
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
    let c = crate::testdir::shipped();
    let rows = table(UNITS);
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
            .map(|a| c.rules.abilities.get(*a).id.as_str())
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
