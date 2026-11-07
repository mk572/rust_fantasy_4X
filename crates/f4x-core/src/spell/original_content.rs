//! The shipped spells against the original's spell table.
//!
//! Tests: in this file; the tables are `f4x_def::original`.

use f4x_def::original::{named_table, number, plain, REALMS};

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
    use f4x_def::rules::{effects::Effect, Rarity};
    let c = f4x_def::testdir::shipped();
    let rows = named_table(f4x_def::original::SPELLS);
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
        // The original misspells Invisibility ("Invisiblity"); the game spells it right.
        let original = if row["name"] == "Invisiblity" {
            "Invisibility"
        } else {
            row["name"].as_str()
        };
        if plain(&def.id) != plain(original) {
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
        if c.rules.realms.get(def.realm).id != REALMS[realm] {
            wrong.push(format!(
                "{} realm {}, original {}",
                def.id,
                c.rules.realms.get(def.realm).id,
                REALMS[realm]
            ));
        }
        // The arcane realm is not in tens: the original's research list offers it in three steps
        // (the first three, the next five, the next four), which the wiki names common, uncommon
        // and rare. The Spell of Mastery and the Spell of Return follow, uncommon.
        let tier = if REALMS[realm] == "arcane" {
            match seen[realm] {
                0..=2 => Rarity::Common,
                3..=7 => Rarity::Uncommon,
                8..=11 => Rarity::Rare,
                _ => Rarity::Uncommon,
            }
        } else {
            tiers[(seen[realm] / 10).min(3)]
        };
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
                    Effect::Summon { unit, .. } => Some(&c.rules.units.get(*unit).id),
                    // The Floating Island is a unit in the original; ours is a custom effect of the same name.
                    Effect::Custom { .. }
                        if f4x_core::spell::terraform::TerrainEffect::FloatingIsland.is_in(def) =>
                    {
                        Some(&def.id)
                    }
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
