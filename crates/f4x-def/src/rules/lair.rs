//! The rules of a guarded site's guards and hoard as data (`crates/f4x-def/content/lairs.ron`): which realm
//! guards which kind of site, the budget of each, how the budget buys guards and how the rest
//! fills the hoard. The unit list is not here: the guards come from `units.ron`.
//!
//! The defaults are the original's values (ReMoM `Create_Lair`, `Lair_Make_Guardians`,
//! `Generate_Lairs`; differences in `docs/lairs.md`), and `crates/f4x-def/content/lairs.ron` repeats them; a
//! test keeps the two equal.
//!
//! Tests: in feature/lair/tests.rs.

use serde::{Deserialize, Serialize};

/// `base + step * Random(rolls)`: a budget rolled once. `Random(n)` is `1..=n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
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

/// Which of patch 1.50's changes a new world's lairs follow (the settings `stronger_lairs` and
/// `treasure_patch_150`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LairStyle {
    /// Stronger guards: the budgets, the divisors, the creature costs and the difficulty scale of
    /// the patch.
    pub stronger: bool,
    /// The patch's treasure: its budget, its table and its prices.
    pub treasure: bool,
}

/// What patch 1.50 changes in the lairs (`docs/lairs.md`, "Patch 1.50"). The ranges of the wiki's
/// Encounter Zone article fix these numbers; the budget steps are the ones that fit the ranges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Patch150Rules {
    /// Stronger lairs: the budget of a tower, and of a strong and a weak site per plane.
    pub tower_budget: Roll,
    pub strong_budget: [Roll; 2],
    pub weak_budget: [Roll; 2],
    /// At most this many of the first kind of guard, and the divisors that pick the kinds.
    pub max_first_guards: i32,
    pub first_spread: i32,
    pub second_spread: i32,
    /// The share of the budget the guards get, by the game's difficulty level 0 to 5 (percent).
    pub guard_percent: Vec<i32>,
    /// The creature costs the patch changed: unit number and cost.
    pub costs: Vec<(u16, i32)>,
    /// Treasure: the guards' value is the budget (the second kind at its full cost, no reverse
    /// of the difficulty), times this percent for each plane, plus `bonus`.
    pub second_share: i32,
    pub plane_loot_percent: [i32; 2],
    pub bonus: i32,
    /// The table of a hoard and what each kind costs.
    pub hoard: Vec<(HoardKind, u32)>,
    /// Gold and mana: `Random(coin_rolls) * step`, the exact amount is what it costs.
    pub coin_rolls: i32,
    pub gold_step: i32,
    pub mana_step: i32,
    /// An item: worth `item_base + Random(item_rolls) * item_step` at most what is left (and kept
    /// as `item_max_percent` of that).
    pub item_base: i32,
    pub item_rolls: i32,
    pub item_step: i32,
    pub item_max_percent: i32,
    pub prisoner_cost: i32,
    /// A spell of rarity r costs `r * r * spell_cost`, one a hoard; a tower must qualify for it.
    pub spell_cost: i32,
    /// A book or retort: `special_cost`, rerolled `special_discard_percent` times out of 100; two
    /// picks when `special_double_sure` is left. It no longer clears the rest of the hoard.
    pub special_cost: i32,
    pub special_discard_percent: i32,
    pub special_double_sure: i32,
}

/// The guard and treasure rules of the sites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
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
    /// What patch 1.50 changes, used by the settings `stronger_lairs` and `treasure_patch_150`.
    pub patch_150: Patch150Rules,
    /// How strong guards that nobody has seen look, in percent of their true strength: one of
    /// these, each as likely, drawn for each new set of guards.
    pub apparent_percent: Vec<u32>,
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
