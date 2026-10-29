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
    let rows = table(include_str!("original/units.tsv"));
    let defs = c.rules.units.defs();
    assert_eq!(
        defs.len(),
        rows.len(),
        "units in the content and in the original"
    );
    let mut wrong = Vec::new();
    for (def, row) in defs.iter().zip(&rows) {
        let mut row = row.clone();
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
    let text = include_str!("original/buildings.tsv");
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
    let rows = named_table(include_str!("original/item_powers.tsv"));
    let defs = c.rules.item_powers.defs();
    assert_eq!(defs.len(), rows.len(), "item powers");
    let mut wrong = Vec::new();
    for (def, row) in defs.iter().zip(&rows) {
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
    let rows = named_table(include_str!("original/items.tsv"));
    let powers = named_table(include_str!("original/item_powers.tsv"));
    let defs = c.rules.items.defs();
    assert_eq!(defs.len(), rows.len(), "items");
    let mut wrong = Vec::new();
    for (def, row) in defs.iter().zip(&rows) {
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
            let at = c.rules.item_powers.id_of(id).map(|i| i.index());
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
