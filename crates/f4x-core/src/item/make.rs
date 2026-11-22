//! Random and prefab items: the original's `Make_Item` and `Create_Random_Item` (`ItemMake.c`).
//! The merchant, a lair's treasure and the events ask for an item of a grade, a value, and get one
//! of the 250 prefab items or, when none suits, one rolled from the plain bonuses. The wizard's
//! books play no part: they only limit what a wizard may craft. Where this differs from the
//! original (the value test, the books, the power count): `docs/divergences.md`, section 10.

use super::crafting::{self, ItemDesign};
use super::Item;
use crate::game::rng::GameRng;
use crate::rules::ItemId;
use crate::rules::ItemPowerId;
use crate::rules::{ItemKind, Rules};

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

/// The item kinds, for the random item's draw.
const KINDS: &[ItemKind] = &[
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
    taken: &mut Vec<ItemId>,
) -> Item {
    let defs = rules.items.defs();
    for _ in 0..rules.config.item.search_tries {
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
        let id = ItemId::new(def.num);
        if taken.contains(&id) || wrong_grade || out_of_range {
            continue;
        }
        taken.push(id);
        return Item::from_def(def);
    }
    random_item(rng, rules, grade, value)
}

/// Roll an item from the plain bonuses (`Create_Random_Item`): a kind, then powers drawn from
/// the plain ones that kind may carry, one of each group, until the power count is reached, the
/// value is passed (the power that passes it stays) or fifty draws are spent.
pub fn random_item(rng: &mut GameRng, rules: &Rules, grade: Grade, value: u32) -> Item {
    let item_rules = &rules.config.item;
    let value = if value == 0 {
        item_rules.random_value_base
            + (rng.roll(item_rules.random_value_steps) - 1) * item_rules.random_value_step
    } else {
        value
    };
    let kind = KINDS[rng.roll(10) as usize - 1];
    let (max_cost, max_powers) = match grade {
        Grade::Enchant => (
            crafting::Craft::EnchantItem.max_power_cost(rules),
            item_rules.enchant_random_powers,
        ),
        _ => (
            item_rules.random_max_power_cost,
            if item_rules.many_power_kinds.contains(&kind) {
                item_rules.many_random_powers
            } else {
                item_rules.other_random_powers
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
            design.powers.push(ItemPowerId::new(power.num));
        }
        if !design.powers.is_empty()
            && (design.powers.len() == max_powers
                || crafting::cost(&design, rules) > value
                || tries > item_rules.search_tries)
        {
            break;
        }
        // No plain power fits the kind: give up rather than draw for ever.
        if tries > 20 * item_rules.search_tries {
            break;
        }
    }
    crafting::build(&design, rules)
}
