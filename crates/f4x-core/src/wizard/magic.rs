//! Wizards: spell books, retorts, the power split, research, casting and mana upkeep.
//!
//! Formulas follow the original game's code (see `docs/magic.md` for the list and for where
//! this differs). The wizard is an entity (`wizard/entity.rs`); its sheet is components
//! (`wizard/sheet.rs`). This file keeps the retorts, and the rules that need nothing but the
//! content and the random numbers. What needs the world (power from the cities,
//! the upkeep items, resolving a spell, the turn change) is in `wizard/magic_world.rs`.
//!
//! Tests: below.

use f4x_def::half_int::HalfInt;
use std::collections::BTreeSet;

use crate::spell::cast::UpkeepItem;

use crate::rules::effects::Effect;
use crate::rules::{Id, Rarity, RealmId, RetortDef, SpellDef, SpellId};
use crate::world::rng::GameRng;
use f4x_def::content::Content;

/// A retort: its number in `retorts.ron`. The saved game writes the number; the name is the text
/// `retort.<id>`, and what a retort costs and needs is its row in the rules.
pub type Retort = Id<RetortDef>;

/// A spell the wizard is casting overland: the mana still to be paid (read from the cast in
/// progress, a spell instance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Casting {
    pub spell: SpellId,
    pub remaining: i32,
}

/// Casting skill for an invested total: the number of triangular steps (2, 6, 12, 20...)
/// the total has passed, plus the `bonus` the retorts give (10 for an archmage).
pub fn base_casting_skill(points: i32, bonus: i32) -> i32 {
    let (mut skill, mut step, mut threshold) = (0, 0, 0);
    while points > threshold {
        skill += 1;
        step += 2;
        threshold += step;
    }
    skill + bonus
}

/// Smallest invested total whose casting skill reaches `target`.
pub fn skill_points_for(target: i32, bonus: i32) -> i32 {
    let mut points = 0;
    while base_casting_skill(points, bonus) < target {
        points += 1;
    }
    points
}

/// The power base split three ways, as it will be applied at the turn change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PowerShares {
    pub mana: HalfInt,
    pub skill: i32,
    /// Includes research points from the wizard's cities.
    pub research: i32,
}

/// Why a magic command was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicError {
    UnknownPlayer,
    /// A realm name that is none of the rules' realms.
    UnknownRealm,
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
    /// Refused only because the spell would do nothing: the caster may confirm and cast anyway.
    pub fn is_pointless(self) -> bool {
        matches!(self, Self::AlreadyEnchanted | Self::AlreadyAble)
    }

    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::UnknownPlayer => "refuse_wizard.magic_error_unknown_player",
            Self::UnknownRealm => "refuse_wizard.magic_error_unknown_realm",
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
pub fn spells_of(content: &Content, realm: RealmId, rarity: Rarity) -> Vec<SpellId> {
    content
        .rules
        .spells
        .iter()
        .filter(|(_, d)| d.realm == (realm) && d.rarity == rarity)
        .map(|(id, _)| id)
        .collect()
}

/// [`spells_of`] that are in this game (`SpellLibraryDef::is_open`).
pub(crate) fn open_spells_of(content: &Content, realm: RealmId, rarity: Rarity) -> Vec<SpellId> {
    let library = &content.settings.spell_library;
    spells_of(content, realm, rarity)
        .into_iter()
        .filter(|&s| library.is_open(content.rules.spells.get(s)))
        .collect()
}

/// The realm's default starting spells of one rarity, in the order of the realm's default list
/// (`realms.ron`).
pub fn default_spells(content: &Content, realm: RealmId, rarity: Rarity) -> Vec<SpellId> {
    let Some(def) = content.rules.realms.id_numbered(realm.num()) else {
        return Vec::new();
    };
    let def = content.rules.realms.get(def);
    let pool = spells_of(content, realm, rarity);
    def.default_spells
        .iter()
        .copied()
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

/// The order in which the mana upkeep is given up, as indexes into `entries` (each the mana it
/// costs and what it takes to replace it, oldest first): the highest upkeep first, since those
/// are the likely cause of the shortage, then the cheapest to replace, then the newest.
pub fn upkeep_release_order(entries: &[(i32, i32)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&i| {
        let (mana, cost) = entries[i];
        (std::cmp::Reverse(mana), cost, std::cmp::Reverse(i))
    });
    order
}

/// What to do about the mana upkeep: the items to give up (`upkeep_release_order`) so that the
/// reserve can pay the rest, and what is then owed. A Channeler's items count half (`percent` 50).
pub(crate) fn upkeep_plan(
    items: &[UpkeepItem],
    percent: i32,
    reserve: HalfInt,
) -> (Vec<UpkeepItem>, HalfInt) {
    // Whole mana times a percent, kept in halves (a Channeler's half of 3 is 1.5).
    let part = |mana: i32| HalfInt::from_int(mana).percent(percent);
    let mut owed: HalfInt = part(items.iter().map(|i| i.mana).sum::<i32>());
    let entries: Vec<(i32, i32)> = items.iter().map(|i| (i.mana, i.cost)).collect();
    let mut dropped = Vec::new();
    for i in upkeep_release_order(&entries) {
        if owed <= reserve {
            break;
        }
        owed -= part(items[i].mana);
        dropped.push(items[i]);
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
