//! Random and prefab items: the original's `Make_Item` and `Create_Random_Item`
//! (`ItemMake.c`). The merchant, a lair's treasure and the events ask for an item of a grade, a
//! value, and get one of the 250 prefab items or, when none suits, one rolled from the plain
//! bonuses. The wizard's books play no part: they only limit what a wizard may craft.
//!
//! Where the original is wrong this follows its evident intent (`docs/divergences.md`, section
//! 10): the value test accepts a prefab item whose cost is in range (the original's only ever
//! rejects), the books are not looked at (the original compares scrambled bytes with the wizard's
//! books) and a random item's power count follows the kind that was drawn (the
//! original reads a variable not yet set, so always allows four).

use super::crafting::{self, ItemDesign};
use super::Item;
use crate::game::rng::GameRng;
use crate::rules::{ItemKind, Rules};

/// How many tries the prefab search and the random item's power search get.
const TRIES: u32 = 50;

/// The grade of item asked for (the original's `Power` argument).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    /// Any item (merchants, lairs, events).
    Any,
    /// What Enchant Item can make: cheap prefab items (flag clear), at most two plain powers
    /// of 200 or less when rolled.
    Enchant,
    /// What Create Artifact can make: the flagged prefab items, up to four powers when rolled.
    Artifact,
}

/// The item kinds in the original's order, for the random item's draw.
const KINDS: [ItemKind; 10] = [
    ItemKind::Sword,
    ItemKind::Mace,
    ItemKind::Axe,
    ItemKind::Bow,
    ItemKind::Staff,
    ItemKind::Wand,
    ItemKind::Misc,
    ItemKind::Shield,
    ItemKind::Chainmail,
    ItemKind::Platemail,
];

/// Make an item (`Make_Item`). `value` is the worth asked for (0 for no preference). A prefab item is found by up to fifty draws among the
/// 250, each prefab item once a game (`taken` lists the ones given out); its cost has to lie
/// between half of `value` and `value` when one is given. When none is found, a random item is
/// rolled.
pub fn make_item(
    rng: &mut GameRng,
    rules: &Rules,
    grade: Grade,
    value: u32,
    taken: &mut Vec<String>,
) -> Item {
    let defs = rules.items.defs();
    for _ in 0..TRIES {
        if defs.is_empty() {
            break;
        }
        let def = &defs[rng.roll(defs.len() as u32) as usize - 1];
        let wrong_grade = match grade {
            Grade::Any => false,
            Grade::Enchant => def.flag,
            Grade::Artifact => !def.flag,
        };
        let out_of_range = value != 0 && (def.cost > value || value / 2 > def.cost);
        if taken.contains(&def.id) || wrong_grade || out_of_range {
            continue;
        }
        taken.push(def.id.clone());
        return Item::from_def(def);
    }
    random_item(rng, rules, grade, value)
}

/// Roll an item from the plain bonuses (`Create_Random_Item`): a kind, then powers drawn from
/// the plain ones that kind may carry, one of each group, until the power count is reached, the
/// value is passed (the power that passes it stays) or fifty draws are spent.
pub fn random_item(rng: &mut GameRng, rules: &Rules, grade: Grade, value: u32) -> Item {
    let value = if value == 0 {
        700 + (rng.roll(10) - 1) * 100
    } else {
        value
    };
    let kind = KINDS[rng.roll(10) as usize - 1];
    let (max_cost, max_powers) = match grade {
        Grade::Enchant => (crafting::Craft::EnchantItem.max_power_cost(), 2),
        _ => (
            20_000,
            match kind {
                ItemKind::Sword
                | ItemKind::Mace
                | ItemKind::Axe
                | ItemKind::Staff
                | ItemKind::Wand => 4,
                _ => 3,
            },
        ),
    };
    let plain: Vec<_> = rules
        .item_powers
        .defs()
        .iter()
        .filter(|p| p.bonus.is_some())
        .collect();
    let mut design = ItemDesign {
        name: String::new(),
        kind,
        icon: {
            let (first, last) = kind.icon_range();
            first + rng.roll(u32::from(last - first) + 1) as u16 - 1
        },
        powers: Vec::new(),
        charges: None,
    };
    let mut groups: Vec<u8> = Vec::new();
    let mut tries = 0;
    while !plain.is_empty() {
        tries += 1;
        let power = plain[rng.roll(plain.len() as u32) as usize - 1];
        if power.kinds.contains(&kind) && power.cost <= max_cost && !groups.contains(&power.group) {
            groups.push(power.group);
            design.powers.push(power.id.clone());
        }
        if !design.powers.is_empty()
            && (design.powers.len() == max_powers
                || crafting::cost(&design, rules) > value
                || tries > TRIES)
        {
            break;
        }
        // No plain power fits the kind: give up rather than draw for ever.
        if tries > 20 * TRIES {
            break;
        }
    }
    crafting::build(&design, rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::shipped;
    use crate::testdir::TestResult;

    fn cost_of(rules: &Rules, id: &str) -> Result<u32, String> {
        rules
            .items
            .by_name(id)
            .map(|d| d.cost)
            .ok_or(format!("{id} is not in the content"))
    }

    fn flagged(rules: &Rules, id: &str) -> Result<bool, String> {
        rules
            .items
            .by_name(id)
            .map(|d| d.flag)
            .ok_or(format!("{id} is not in the content"))
    }

    #[test]
    fn a_prefab_item_is_found_by_value_and_grade_and_given_once() -> TestResult {
        let rules = shipped().rules;
        let mut rng = GameRng::new(1);
        let mut taken = Vec::new();
        // Create Artifact: a flagged prefab item whose cost lies between half the value and it.
        let item = make_item(&mut rng, &rules, Grade::Artifact, 700, &mut taken);
        let id = item.prefab.clone().ok_or("a prefab item was expected")?;
        assert!(flagged(&rules, &id)?, "{id} is not a flagged item");
        assert!((350..=700).contains(&cost_of(&rules, &id)?), "{id}");
        assert_eq!(taken, vec![id.clone()]);
        // It is not given again: the next call gives another item.
        let again = make_item(&mut rng, &rules, Grade::Artifact, 700, &mut taken);
        assert_ne!(again.prefab, Some(id));
        // Enchant Item makes only the unflagged prefab items.
        let mut taken = Vec::new();
        let item = make_item(&mut rng, &rules, Grade::Enchant, 500, &mut taken);
        let id = item.prefab.ok_or("a prefab item was expected")?;
        assert!(!flagged(&rules, &id)?, "{id} is a flagged item");
        assert!((250..=500).contains(&cost_of(&rules, &id)?), "{id}");
        Ok(())
    }

    #[test]
    fn without_a_value_any_prefab_item_may_come() {
        let rules = shipped().rules;
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..60 {
            let mut taken = Vec::new();
            let item = make_item(&mut GameRng::new(seed), &rules, Grade::Any, 0, &mut taken);
            if let Some(id) = item.prefab {
                seen.insert(id);
            }
        }
        assert!(seen.len() > 30, "{} different items", seen.len());
    }

    #[test]
    fn a_random_item_has_plain_powers_only_up_to_the_count_and_value() -> TestResult {
        let rules = shipped().rules;
        for seed in 0..200 {
            let mut rng = GameRng::new(seed);
            let item = random_item(&mut rng, &rules, Grade::Enchant, 0);
            // Enchant Item grade: at most two powers, none costing over 200, all plain.
            assert!(item.powers.len() <= 2, "{item:?}");
            for power in &item.powers {
                let def = rules
                    .item_powers
                    .by_name(power)
                    .ok_or("a power is not in the content")?;
                assert!(def.bonus.is_some(), "no special powers: {item:?}");
                assert!(def.cost <= 200, "{item:?}");
            }
            assert_eq!(item.prefab, None);
            assert!(item.cost >= 100, "{item:?}");
            let (first, last) = item.kind.icon_range();
            assert!((first..=last).contains(&item.icon), "{item:?}");
        }
        Ok(())
    }

    #[test]
    fn a_random_enchant_item_takes_two_cheap_powers_at_most() {
        let rules = shipped().rules;
        for seed in 0..200 {
            let mut rng = GameRng::new(seed);
            let item = random_item(&mut rng, &rules, Grade::Enchant, 100_000);
            let plain = [item.bonus.attack, item.bonus.to_hit, item.bonus.defense]
                .iter()
                .filter(|v| **v != 0)
                .count();
            // Chain and plate mail add a defence of their own.
            let armour = matches!(item.kind, ItemKind::Chainmail | ItemKind::Platemail);
            assert!(plain <= 2 + usize::from(armour), "{item:?}");
            // Attack 4 (350) and to hit (400) cost over 200.
            assert!(item.bonus.attack <= 3 && item.bonus.to_hit == 0, "{item:?}");
        }
    }

    #[test]
    fn a_random_item_with_a_value_stops_once_it_is_passed() {
        let rules = shipped().rules;
        for seed in 0..100 {
            let mut rng = GameRng::new(seed);
            let item = random_item(&mut rng, &rules, Grade::Artifact, 300);
            // The power that passes the value stays, so without its last power the item costs
            // no more than the value.
            let Some((_, before)) = item.powers.split_last() else {
                continue;
            };
            let design = ItemDesign {
                name: String::new(),
                kind: item.kind,
                icon: item.icon,
                powers: before.to_vec(),
                charges: None,
            };
            assert!(crafting::cost(&design, &rules) <= 300, "{item:?}");
        }
    }
}
