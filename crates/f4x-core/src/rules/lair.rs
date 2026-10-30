//! The rules of a guarded site's guards and hoard as data (`content/lairs.ron`): which realm
//! guards which kind of site, the budget of each, how the budget buys guards and how the rest
//! fills the hoard. The unit list is not here: the guards come from `units.ron`.
//!
//! The defaults are the original's values (ReMoM `Create_Lair`, `Lair_Make_Guardians`,
//! `Generate_Lairs`; differences in `docs/lairs.md`), and `content/lairs.ron` repeats them; a
//! test keeps the two equal.
//!
//! Tests: in feature/lair/tests.rs.

use serde::{Deserialize, Serialize};

/// `base + step * Random(rolls)`: a budget rolled once. `Random(n)` is `1..=n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roll {
    pub base: i32,
    pub rolls: i32,
    pub step: i32,
}

impl Roll {
    pub const fn new(base: i32, rolls: i32, step: i32) -> Self {
        Self { base, rolls, step }
    }
}

/// What one slot of the hoard's table gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoardKind {
    Gold,
    Mana,
    Item,
    Prisoner,
    /// A spell of a random rarity.
    Spell,
    /// A spell book or retort.
    Special,
}

/// The realms that guard a kind of site, by the realm's number, with their weights (a roll over
/// the total).
pub type RealmWeights = Vec<(u16, u32)>;

/// The guard and treasure rules of the sites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LairRules {
    /// Guards in all, and of the first kind.
    pub max_guards: u32,
    pub max_first_guards: i32,
    /// The first kind costs at most the budget over `Random(first_spread)`; the second at most
    /// the rest over `Random(second_spread - first count)`.
    pub first_spread: i32,
    pub second_spread: i32,
    /// Realms of the guards of a tower, of a temple or ruin, and of any other encounter site.
    pub tower_realms: RealmWeights,
    pub holy_realms: RealmWeights,
    pub wild_realms: RealmWeights,
    /// A node's guards come from its own realm if it is one of these, else from `node_other`.
    pub node_own_realms: Vec<u16>,
    pub node_other: u16,
    /// The budget of a tower.
    pub tower_budget: Roll,
    /// A node's budget is `power * power * (node_base + Random(node_rolls)) * (magic + 1) / 2`,
    /// at most `node_cap`.
    pub node_base: i32,
    pub node_rolls: i32,
    pub node_cap: i32,
    /// The budget of a strong and of a weak encounter site, for Arcanus and Myrror.
    pub strong_budget: [Roll; 2],
    pub weak_budget: [Roll; 2],
    /// The hoard's value is the first guards' value plus this share of the second's (percent),
    /// times the treasure setting, times `base + Random(rolls)` percent for the plane.
    pub second_share: i32,
    pub plane_loot: [Roll; 2],
    /// The least value a hoard has, and below which the table stops.
    pub min_loot: i32,
    /// Items at most in a hoard.
    pub max_items: usize,
    /// The table: a slot is `Random(total weight)`.
    pub hoard: Vec<(HoardKind, u32)>,
    /// Gold or mana: `Random(coin_rolls) * coin_step`, at most the value left, costing `coin_cost`.
    pub coin_rolls: i32,
    pub coin_step: i32,
    pub coin_cost: i32,
    /// An item: at least `item_min` left; worth `item_base + Random(item_rolls) * item_step`.
    pub item_min: i32,
    pub item_base: i32,
    pub item_rolls: i32,
    pub item_step: i32,
    /// A prisoner: at least `prisoner_min` left; costs `prisoner_cost`.
    pub prisoner_min: i32,
    pub prisoner_cost: i32,
    /// A spell: rarity `Random(spell_rarities)`, costing rarity squared times `spell_cost`;
    /// a hoard holds spells up to rarity `spell_total`. A tower always has one, rolled first
    /// and paying `rarity² * spell_cost - tower_spell_refund`.
    pub spell_rarities: i32,
    pub spell_cost: i32,
    pub spell_total: i32,
    pub tower_spell_refund: i32,
    /// A book or retort costs `special_cost`; it takes two picks when `Random(double_chance)`
    /// is 1 and `special_double_min` is left, or when `special_double_sure` is; else one pick
    /// when `special_min` is left.
    pub special_cost: i32,
    pub double_chance: i32,
    pub special_double_min: i32,
    pub special_double_sure: i32,
    pub special_min: i32,
}

impl Default for LairRules {
    fn default() -> Self {
        // Realm numbers as in `realms.ron`: nature 0, sorcery 1, chaos 2, life 3, death 4.
        let weights = |list: &[(u16, u32)]| list.to_vec();
        Self {
            max_guards: 9,
            max_first_guards: 8,
            first_spread: 4,
            second_spread: 10,
            tower_realms: weights(&[(4, 2), (2, 1), (3, 1), (0, 1), (1, 1)]),
            holy_realms: weights(&[(4, 3), (3, 1)]),
            wild_realms: weights(&[(4, 2), (2, 2), (0, 1)]),
            node_own_realms: vec![1, 0],
            node_other: 2,
            tower_budget: Roll::new(650, 11, 50),
            node_base: 4,
            node_rolls: 11,
            node_cap: 32767,
            strong_budget: [Roll::new(50, 29, 50), Roll::new(100, 24, 100)],
            weak_budget: [Roll::new(0, 10, 10), Roll::new(0, 20, 10)],
            second_share: 50,
            plane_loot: [Roll::new(49, 76, 1), Roll::new(75, 100, 1)],
            min_loot: 50,
            max_items: 3,
            hoard: vec![
                (HoardKind::Gold, 2),
                (HoardKind::Mana, 2),
                (HoardKind::Item, 5),
                (HoardKind::Prisoner, 1),
                (HoardKind::Spell, 3),
                (HoardKind::Special, 2),
            ],
            coin_rolls: 20,
            coin_step: 10,
            coin_cost: 200,
            item_min: 300,
            item_base: 300,
            item_rolls: 27,
            item_step: 100,
            prisoner_min: 400,
            prisoner_cost: 1000,
            spell_rarities: 4,
            spell_cost: 50,
            spell_total: 4,
            tower_spell_refund: 100,
            special_cost: 3000,
            double_chance: 4,
            special_double_min: 3000,
            special_double_sure: 2000,
            special_min: 1000,
        }
    }
}

impl LairRules {
    /// The realm a roll of `weights` picks, given `roll` in `1..=total`.
    pub fn realm_for(weights: &RealmWeights, roll: u32) -> u16 {
        let mut left = roll;
        for (realm, weight) in weights {
            if left <= *weight {
                return *realm;
            }
            left -= weight;
        }
        weights.last().map_or(0, |&(r, _)| r)
    }

    /// The sum of a realm table's weights.
    pub fn total(weights: &RealmWeights) -> u32 {
        weights.iter().map(|(_, w)| w).sum()
    }
}
