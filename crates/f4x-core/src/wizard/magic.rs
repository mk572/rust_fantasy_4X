//! Wizards: spell books, retorts, the power split, research, casting and mana upkeep.
//!
//! Formulas follow the original game's code (see `docs/magic.md` for the list and for where
//! this differs). The wizard is an entity (`wizard/entity.rs`); its sheet is components
//! (`wizard/sheet.rs`). This file keeps the retorts, and the rules that need nothing but the
//! content and the random numbers. What needs the world (power from the cities,
//! the upkeep items, resolving a spell, the turn change) is in `wizard/magic_game.rs`.
//!
//! Tests: below.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::spell::cast::UpkeepItem;

use crate::content::Content;
use crate::game::rng::GameRng;
use crate::rules::effects::Effect;
use crate::rules::{Rarity, SpellDef, SpellId};

/// Special abilities a wizard can pick ("retorts" in the original's manual).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Retort {
    Alchemy,
    Warlord,
    ChaosMastery,
    NatureMastery,
    SorceryMastery,
    Channeler,
    Archmage,
    Artificer,
    Conjurer,
    SageMaster,
    Myrran,
    DivinePower,
    InfernalPower,
    Famous,
    Runemaster,
    Charismatic,
    ManaFocusing,
    NodeMastery,
}

impl Retort {
    /// The retorts in the order the original lists them (its wizard record's field order).
    pub const LISTED: [Retort; 18] = [
        Retort::Alchemy,
        Retort::Warlord,
        Retort::ChaosMastery,
        Retort::NatureMastery,
        Retort::SorceryMastery,
        Retort::InfernalPower,
        Retort::DivinePower,
        Retort::SageMaster,
        Retort::Channeler,
        Retort::Myrran,
        Retort::Archmage,
        Retort::ManaFocusing,
        Retort::NodeMastery,
        Retort::Famous,
        Retort::Runemaster,
        Retort::Conjurer,
        Retort::Charismatic,
        Retort::Artificer,
    ];
}

/// How many spells are offered for research at once.
pub const RESEARCH_SLOTS: usize = 8;

/// A spell the wizard is casting overland: the mana still to be paid (read from the cast in
/// progress, a spell instance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Casting {
    pub spell: SpellId,
    pub remaining: i32,
}

/// Casting skill for an invested total: the number of triangular steps (2, 6, 12, 20...)
/// the total has passed, plus 10 for an archmage.
pub fn base_casting_skill(points: i32, archmage: bool) -> i32 {
    let (mut skill, mut step, mut threshold) = (0, 0, 0);
    while points > threshold {
        skill += 1;
        step += 2;
        threshold += step;
    }
    skill + if archmage { 10 } else { 0 }
}

/// Smallest invested total whose casting skill reaches `target`.
pub fn skill_points_for(target: i32, archmage: bool) -> i32 {
    let mut points = 0;
    while base_casting_skill(points, archmage) < target {
        points += 1;
    }
    points
}

/// The power base split three ways, as it will be applied at the turn change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PowerShares {
    pub mana: i32,
    pub skill: i32,
    /// Includes research points from the wizard's cities.
    pub research: i32,
}

/// Why a magic command was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicError {
    UnknownPlayer,
    /// The spell is not known (cast) or not researchable (research).
    NotAvailable,
    AlreadyResearching,
    AlreadyCasting,
    /// Percentages must add up to 100.
    BadSplit,
    /// This spell cannot be cast on the overland map.
    NotOverland,
    /// The target is not one the spell can be cast on, and why.
    Target(crate::spell::target_error::TargetError),
    /// The unit or city has this lasting spell on it already.
    AlreadyEnchanted,
    /// The spell is not cumulative with one the unit has (or the wizard keeps): Stone Skin with
    /// Iron Skin, Holy Weapon with Holy Arms.
    NotCumulative,
    /// The unit has the ability the spell would give it, by its nature.
    AlreadyAble,
    /// The spell needs a target and none was given.
    NeedsTarget,
    NothingToResolve,
    NotEnoughGold,
    NotEnoughMana,
    /// A banished wizard casts nothing but the Spell of Return.
    Banished,
}

impl MagicError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::UnknownPlayer => "refuse_wizard.magic_error_unknown_player",
            Self::NotAvailable => "refuse_wizard.magic_error_not_available",
            Self::AlreadyResearching => "refuse_wizard.magic_error_already_researching",
            Self::AlreadyCasting => "refuse_wizard.magic_error_already_casting",
            Self::BadSplit => "refuse_wizard.magic_error_bad_split",
            Self::NotOverland => "refuse_wizard.magic_error_not_overland",
            Self::Target(e) => e.key(),
            Self::AlreadyEnchanted => "refuse_wizard.magic_error_already_enchanted",
            Self::NotCumulative => "refuse_wizard.magic_error_not_cumulative",
            Self::AlreadyAble => "refuse_wizard.magic_error_already_able",
            Self::NeedsTarget => "refuse_wizard.magic_error_needs_target",
            Self::NothingToResolve => "refuse_wizard.magic_error_nothing_to_resolve",
            Self::NotEnoughGold => "refuse_wizard.magic_error_not_enough_gold",
            Self::NotEnoughMana => "refuse_wizard.magic_error_not_enough_mana",
            Self::Banished => "refuse_wizard.magic_error_banished",
        }
    }
}

pub(crate) fn is_summoning(def: &SpellDef) -> bool {
    def.effects
        .iter()
        .any(|r| matches!(r.effect, Effect::Summon { .. }))
}

pub(crate) fn is_crafting(def: &SpellDef) -> bool {
    matches!(def.id.as_str(), "enchant_item" | "create_artifact")
}

/// Spells of one realm and rarity in file order (the order the data lists them in).
pub fn spells_of(content: &Content, realm: &str, rarity: Rarity) -> Vec<SpellId> {
    content
        .rules
        .spells
        .iter()
        .filter(|(_, d)| d.realm == realm && d.rarity == rarity)
        .map(|(id, _)| id)
        .collect()
}

/// The realm's default starting spells of one rarity, in the order of the realm's default list
/// (`realms.ron`).
pub fn default_spells(content: &Content, realm: &str, rarity: Rarity) -> Vec<SpellId> {
    let Some(def) = content.rules.realms.defs().iter().find(|r| r.id == realm) else {
        return Vec::new();
    };
    let pool = spells_of(content, realm, rarity);
    def.default_spells
        .iter()
        .filter_map(|s| content.rules.spells.id_of(s))
        .filter(|s| pool.contains(s))
        .collect()
}

/// Pick `count` distinct spells from `pool` at random, skipping any in `have`.
pub(crate) fn pick_spells(
    rng: &mut GameRng,
    pool: &[SpellId],
    have: &BTreeSet<SpellId>,
    count: usize,
) -> Vec<SpellId> {
    let mut left: Vec<SpellId> = pool.iter().copied().filter(|s| !have.contains(s)).collect();
    let mut out = Vec::new();
    while out.len() < count && !left.is_empty() {
        let i = rng.roll(left.len() as u32) as usize - 1;
        out.push(left.remove(i));
    }
    out
}

/// What to do about the mana upkeep: the items to give up (newest first) so that the reserve
/// can pay the rest, and what is then owed. A Channeler's items count half (`divisor` 2).
pub(crate) fn upkeep_plan(
    items: &[UpkeepItem],
    divisor: i32,
    reserve: i32,
) -> (Vec<UpkeepItem>, i32) {
    let mut owed: i32 = items.iter().map(|i| i.mana).sum::<i32>() / divisor;
    let mut rest = items.to_vec();
    let mut dropped = Vec::new();
    while owed > reserve {
        let Some(item) = rest.pop() else {
            break;
        };
        owed -= item.mana / divisor;
        dropped.push(item);
    }
    (dropped, owed)
}

/// How a spell cast on the overland map begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastStart {
    /// The skill and mana on hand pay for all of it: it takes effect now.
    Now,
    /// It is paid over turns.
    Paid,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::shipped;
    use crate::spell::cast::UpkeepKind;

    fn some_spell() -> SpellId {
        shipped()
            .rules
            .spells
            .iter()
            .next()
            .map(|(id, _)| id)
            .expect("a spell")
    }

    fn item(mana: i32) -> UpkeepItem {
        UpkeepItem {
            kind: UpkeepKind::Global(some_spell()),
            mana,
        }
    }

    #[test]
    fn casting_skill_grows_one_step_at_each_triangular_total() {
        // The totals 2, 6, 12, 20 ... are the last points of each step: one more passes it.
        let skill = |p| base_casting_skill(p, false);
        assert_eq!([skill(0), skill(1), skill(2)], [0, 1, 1]);
        assert_eq!([skill(3), skill(6), skill(7)], [2, 2, 3]);
        assert_eq!([skill(12), skill(13)], [3, 4]);
        assert_eq!([skill(20), skill(21)], [4, 5]);
        assert_eq!(base_casting_skill(7, true), 13, "an archmage adds ten");
        assert_eq!(base_casting_skill(0, true), 10);
    }

    #[test]
    fn the_points_for_a_skill_are_the_least_that_reach_it() {
        for archmage in [false, true] {
            for target in 0..30 {
                let points = skill_points_for(target, archmage);
                assert!(base_casting_skill(points, archmage) >= target);
                if points > 0 {
                    assert!(
                        base_casting_skill(points - 1, archmage) < target,
                        "{target} {archmage}"
                    );
                }
            }
        }
        assert_eq!(skill_points_for(0, true), 0, "an archmage has ten already");
        assert_eq!(skill_points_for(11, true), 1);
    }

    #[test]
    fn upkeep_is_paid_from_the_reserve_and_the_newest_items_go_first() {
        let items = [item(5), item(10), item(20)];
        let (dropped, owed) = upkeep_plan(&items, 1, 100);
        assert!(dropped.is_empty());
        assert_eq!(owed, 35);
        // 35 owed against 20: drop the newest (20) and 15 is left, within 20.
        let (dropped, owed) = upkeep_plan(&items, 1, 20);
        assert_eq!(dropped, [item(20)]);
        assert_eq!(owed, 15);
        // With nothing in the reserve everything goes.
        let (dropped, owed) = upkeep_plan(&items, 1, 0);
        assert_eq!(dropped, [item(20), item(10), item(5)]);
        assert_eq!(owed, 0);
        assert_eq!(upkeep_plan(&[], 1, 0), (Vec::new(), 0));
    }

    #[test]
    fn a_channeler_pays_half_the_upkeep() {
        let items = [item(10), item(20)];
        assert_eq!(upkeep_plan(&items, 2, 15), (Vec::new(), 15));
        let (dropped, owed) = upkeep_plan(&items, 2, 10);
        assert_eq!(dropped, [item(20)]);
        assert_eq!(owed, 5);
    }

    #[test]
    fn spells_of_a_realm_and_rarity_come_in_file_order() {
        let c = shipped();
        for realm in ["nature", "chaos"] {
            for rarity in [Rarity::Common, Rarity::VeryRare] {
                let spells = spells_of(&c, realm, rarity);
                assert!(!spells.is_empty(), "{realm} {rarity:?}");
                let order: Vec<_> = c.rules.spells.iter().map(|(id, _)| id).collect();
                let at = |s: &SpellId| order.iter().position(|o| o == s);
                assert!(spells.windows(2).all(|w| at(&w[0]) < at(&w[1])));
                for s in spells {
                    let def = c.rules.spells.get(s);
                    assert_eq!((def.realm.as_str(), def.rarity), (realm, rarity));
                }
            }
        }
        assert!(spells_of(&c, "no such realm", Rarity::Common).is_empty());
    }

    #[test]
    fn the_default_spells_are_the_realms_list_of_that_rarity() {
        let c = shipped();
        let chaos = default_spells(&c, "chaos", Rarity::Common);
        let bolt = c.rules.spells.id_of("fire_bolt").expect("fire_bolt");
        assert!(chaos.contains(&bolt));
        for s in &chaos {
            assert_eq!(c.rules.spells.get(*s).rarity, Rarity::Common);
        }
        assert!(default_spells(&c, "no such realm", Rarity::Common).is_empty());
        let all: usize = [
            Rarity::Common,
            Rarity::Uncommon,
            Rarity::Rare,
            Rarity::VeryRare,
        ]
        .iter()
        .map(|&r| default_spells(&c, "chaos", r).len())
        .sum();
        let listed = c
            .rules
            .realms
            .defs()
            .iter()
            .find(|r| r.id == "chaos")
            .map_or(0, |r| r.default_spells.len());
        assert_eq!(all, listed, "every listed default has a rarity");
    }

    #[test]
    fn spells_picked_at_random_are_distinct_new_and_no_more_than_there_are() {
        let pool: Vec<SpellId> = shipped()
            .rules
            .spells
            .iter()
            .map(|(id, _)| id)
            .take(6)
            .collect();
        let have: BTreeSet<SpellId> = [pool[1], pool[4]].into();
        let mut rng = GameRng::new(5);
        let picked = pick_spells(&mut rng, &pool, &have, 3);
        assert_eq!(picked.len(), 3);
        let set: BTreeSet<_> = picked.iter().copied().collect();
        assert_eq!(set.len(), 3, "no spell twice");
        assert!(set.is_disjoint(&have));
        let all = pick_spells(&mut rng, &pool, &have, 10);
        assert_eq!(all.len(), 4, "only four are new");
        assert!(pick_spells(&mut rng, &[], &have, 3).is_empty());
        let (a, b) = (
            pick_spells(&mut GameRng::new(9), &pool, &have, 3),
            pick_spells(&mut GameRng::new(9), &pool, &have, 3),
        );
        assert_eq!(a, b, "the same seed picks the same");
    }

    #[test]
    fn summoning_and_crafting_spells_are_told_by_their_data() {
        let c = shipped();
        let def = |id: &str| c.rules.spells.get(c.rules.spells.id_of(id).expect(id));
        assert!(is_summoning(def("war_bears")));
        assert!(!is_summoning(def("fire_bolt")));
        assert!(is_crafting(def("enchant_item")) && is_crafting(def("create_artifact")));
        assert!(!is_crafting(def("war_bears")));
    }

    #[test]
    fn every_magic_error_has_its_own_sentence_key_in_the_refusal_texts() {
        let all = [
            MagicError::UnknownPlayer,
            MagicError::NotAvailable,
            MagicError::AlreadyResearching,
            MagicError::AlreadyCasting,
            MagicError::BadSplit,
            MagicError::NotOverland,
            MagicError::AlreadyEnchanted,
            MagicError::NotCumulative,
            MagicError::AlreadyAble,
            MagicError::NeedsTarget,
            MagicError::NothingToResolve,
            MagicError::NotEnoughGold,
            MagicError::NotEnoughMana,
            MagicError::Banished,
        ];
        let mut keys: Vec<_> = all.iter().map(|e| e.key()).collect();
        for k in &keys {
            assert!(k.contains("magic_error_"), "{k}");
        }
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), all.len(), "no two errors share a sentence");
    }
}
