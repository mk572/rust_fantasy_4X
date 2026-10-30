//! Random and prefab items (task 262): the original's `Make_Item` and `Create_Random_Item`
//! (`ItemMake.c`). The merchant, a lair's treasure and the events ask for an item of a grade, a
//! value, and get one of the 250 prefab items or, when none suits, one rolled from the plain
//! bonuses. The wizard's books play no part: they only limit what a wizard may craft.
//!
//! Where the original is wrong this follows its evident intent (`docs/divergences.md`, section
//! 10): the value test accepts a prefab item whose cost is in range (the original's only ever
//! rejects), the books are not looked at (the original compares scrambled bytes with the wizard's
//! books, task 244) and a random item's power count follows the kind that was drawn (the
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
    use crate::rules::{ItemDef, ItemPowerDef, PowerBonus, Registry};

    fn def(id: &str, kind: ItemKind, cost: u32, powers: &[&str], flag: bool) -> ItemDef {
        ItemDef {
            num: 0,
            id: id.into(),
            name: id.into(),
            slot: kind.slot(),
            kind,
            cost,
            icon: 0,
            effects: Vec::new(),
            spell_skill: 0,
            spell_save: 0,
            powers: powers.iter().map(|p| p.to_string()).collect(),
            charges: None,
            flag,
        }
    }

    fn power(
        id: &str,
        group: u8,
        cost: u32,
        bonus: Option<(PowerBonus, i32)>,
        realm: Option<(&str, u32)>,
    ) -> ItemPowerDef {
        ItemPowerDef {
            num: 0,
            id: id.into(),
            name: id.into(),
            group,
            cost,
            kinds: vec![
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
            ],
            realm: realm.map(|(r, _)| r.into()),
            books: realm.map_or(0, |(_, b)| b),
            bonus,
            spell: None,
            attack: None,
        }
    }

    fn rules() -> Rules {
        let mut rules = crate::game::testutil::content_with_magic().rules;
        rules.item_powers = Registry::new(crate::game::testutil::numbered_powers(vec![
            power("attack_1", 0, 50, Some((PowerBonus::Attack, 1)), None),
            power("attack_2", 0, 100, Some((PowerBonus::Attack, 2)), None),
            power("attack_6", 0, 800, Some((PowerBonus::Attack, 6)), None),
            power("defense_1", 2, 50, Some((PowerBonus::Defense, 1)), None),
            power("to_hit_1", 1, 400, Some((PowerBonus::ToHit, 1)), None),
            power("flaming", 9, 300, None, Some(("chaos", 2))),
            power("lightning", 9, 800, None, Some(("chaos", 5))),
            power("true_sight", 9, 500, None, Some(("life", 6))),
        ]));
        rules.items = Registry::new(crate::game::testutil::numbered_items(vec![
            def("cheap", ItemKind::Sword, 250, &[], false),
            def("flame", ItemKind::Sword, 600, &["flaming"], true),
            def("bolt", ItemKind::Mace, 900, &["lightning", "flaming"], true),
            def("sight", ItemKind::Misc, 1000, &["true_sight"], true),
        ]));
        rules
    }

    #[test]
    fn a_prefab_item_is_found_by_value_and_grade_and_given_once() {
        let rules = rules();
        let mut rng = GameRng::new(1);
        let mut taken = Vec::new();
        // Only "flame" costs between 350 and 700 among the flagged items.
        let item = make_item(&mut rng, &rules, Grade::Artifact, 700, &mut taken);
        assert_eq!(item.prefab.as_deref(), Some("flame"));
        assert_eq!(taken, vec!["flame".to_string()]);
        // It is not given again: the next call rolls a random item instead.
        let again = make_item(&mut rng, &rules, Grade::Artifact, 700, &mut taken);
        assert_eq!(again.prefab, None);
        assert_eq!(taken.len(), 1);
        // Enchant Item makes only the unflagged prefab items.
        let mut taken = Vec::new();
        let item = make_item(&mut rng, &rules, Grade::Enchant, 500, &mut taken);
        assert_eq!(item.prefab.as_deref(), Some("cheap"));
    }

    #[test]
    fn without_a_value_any_prefab_item_may_come() {
        let rules = rules();
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..60 {
            let mut taken = Vec::new();
            let item = make_item(&mut GameRng::new(seed), &rules, Grade::Any, 0, &mut taken);
            if let Some(id) = item.prefab {
                seen.insert(id);
            }
        }
        assert_eq!(seen.len(), 4, "{seen:?}");
    }

    #[test]
    fn a_random_item_has_plain_powers_only_up_to_the_count_and_value() {
        let rules = rules();
        for seed in 0..200 {
            let mut rng = GameRng::new(seed);
            let item = random_item(&mut rng, &rules, Grade::Enchant, 0);
            // Enchant Item grade: at most two powers, none costing over 200.
            assert!(item.powers.is_empty(), "no special powers: {item:?}");
            assert_eq!(item.prefab, None);
            assert!(item.cost >= 100, "{item:?}");
            let (first, last) = item.kind.icon_range();
            assert!((first..=last).contains(&item.icon), "{item:?}");
            let item = random_item(&mut rng, &rules, Grade::Artifact, 1000);
            assert!(item.powers.is_empty() && item.cost > 0, "{item:?}");
        }
    }

    #[test]
    fn a_random_enchant_item_takes_two_cheap_powers_at_most() {
        let rules = rules();
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
            // Attack 6 (800) and to hit (400) cost over 200.
            assert!(item.bonus.attack <= 2 && item.bonus.to_hit == 0, "{item:?}");
        }
    }

    #[test]
    fn a_random_item_with_a_value_stops_once_it_is_passed() {
        let rules = rules();
        for seed in 0..100 {
            let mut rng = GameRng::new(seed);
            let item = random_item(&mut rng, &rules, Grade::Artifact, 300);
            // The power that passes the value stays, so the cost may be over it by one power
            // (the dearest is 800, doubled on a miscellaneous item), never by two.
            assert!(item.cost <= 300 + 1600, "{item:?}");
        }
    }
}
