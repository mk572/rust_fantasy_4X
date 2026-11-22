//! A wizard casting in a battle: the skill and reserve it has, what casting costs at the range of
//! the battle, and what a spell is aimed at.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::half_int::HalfInt;

use crate::map::Pos;

/// A wizard casting in a battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BattleCaster {
    /// Casting skill left to spend this battle.
    pub skill_left: i32,
    /// Mana in the reserve (halves count: a spell of 5 at x1.5 costs 7½).
    pub mana: HalfInt,
    /// What each point of skill costs from the reserve, in tenths: 10 is one for one.
    pub range: i32,
    pub cast_this_turn: bool,
    /// The most reserve mana the wizard may still spend in this battle, if limited (the brief
    /// of the strategic AI, `docs/learning-ai.md` section 2). `None` is no limit.
    pub budget: Option<HalfInt>,
}

impl BattleCaster {
    /// A caster with a whole number of mana in the reserve.
    pub fn new(skill: i32, mana: i32, range: i32) -> Self {
        Self::with_mana(skill, HalfInt::from_int(mana.max(0)), range)
    }

    /// A caster whose reserve may hold half a point.
    pub fn with_mana(skill: i32, mana: HalfInt, range: i32) -> Self {
        Self {
            skill_left: skill.max(0),
            mana: if mana < 0 { HalfInt::from_int(0) } else { mana },
            range: range.max(1),
            cast_this_turn: false,
            budget: None,
        }
    }

    /// Mana from the reserve for `skill` points of casting: `range` is the multiplier in
    /// tenths, so the price is a whole or a half number.
    pub fn mana_for(&self, skill: i32) -> HalfInt {
        HalfInt::from_int(skill.saturating_mul(self.range)).percent(10)
    }

    /// The most skill a spell can cost now: what is left, and what the reserve pays for.
    pub fn affordable(&self) -> i32 {
        // The most skill whose price in mana the reserve (or the budget) covers.
        let skill_for = |mana: HalfInt| (mana.max(HalfInt::ZERO) * 10).floor() / self.range;
        let by_budget = self.budget.map_or(i32::MAX, skill_for);
        self.skill_left.min(skill_for(self.mana)).min(by_budget)
    }
}

/// What the cost of casting is multiplied by, in tenths, for a battle `distance` squares from
/// the fortress (`Combat_Casting_Cost_Multiplier`). `None` is a battle on the other plane, or
/// from a wizard with no fortress: as far as it gets. At the fortress itself casting costs
/// half; beyond five squares each five more add half the cost again, up to three times.
/// `counted` turns the distance into the one the wizard's retorts make of it (a Channeler counts
/// as two squares away anywhere but at the fortress: `Stat::CastingDistance`).
pub fn range_percent(
    range: &f4x_def::rules::CastingRange,
    distance: Option<u32>,
    counted: impl FnOnce(i32) -> i32,
) -> i32 {
    let distance = distance.unwrap_or(range.far);
    if distance == 0 {
        return range.at_fortress;
    }
    let distance = u32::try_from(counted(i32::try_from(distance).unwrap_or(i32::MAX))).unwrap_or(0);
    range.base + range.step * range.beyond.iter().filter(|&&t| distance > t).count() as i32
}

/// What a spell is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SpellAim {
    None,
    Unit(usize),
    /// A unit and the summoning circle it goes to: Word of Recall and Recall Hero, when the wizard has
    /// several circles.
    UnitTo(usize, f4x_def::map::Position),
    Tile(Pos),
}
