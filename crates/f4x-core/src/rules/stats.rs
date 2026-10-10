//! Stats and modifiers. A stat is never stored; it is computed from a base value
//! and the modifiers currently applying, so removing a source removes exactly its
//! effect. Integer math only, applied in a fixed order, so results are
//! deterministic and independent of the order modifiers were added.

use serde::{Deserialize, Serialize};

use super::defs::{AbilityDef, BuildingDef, ItemDef, RaceDef, SpellDef};
use super::effects::{Condition, Effect, EffectRule, Trigger};
use super::ids::Id;

/// Every numeric attribute the rules know about. Fixed in code: content files
/// may use these but not invent new ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Stat {
    // Units and heroes
    Melee,
    Ranged,
    Defense,
    Resistance,
    MaxHp,
    Figures,
    Moves,
    Sight,
    /// Percentage points added to the base to-hit chance (may be negative).
    ToHit,
    Ammo,
    /// Road-building strength: engineers have 1 (some 2); the stack's total divides the
    /// turns a road square takes.
    Construction,
    /// Land units a ship carries.
    Transport,
    // Wizards
    ManaIncome,
    ResearchIncome,
    CastingSkill,
    /// Percent change to spell mana costs.
    SpellCostPercent,
    // Cities
    CityFood,
    /// Food added after the farmland cap is applied (granaries, markets).
    CityFoodFlat,
    /// Extra food each farmer grows.
    FarmerFood,
    /// Percent added to unrest from all sources (famine).
    CityUnrestPercent,
    /// Experience points new units are built with.
    StartingXp,
    CityProduction,
    CityGold,
    CityMana,
    CityResearch,
    CityGrowthPercent,
    /// People gained per turn (a city "figure" is 1000 people); may be negative.
    CityGrowth,
    /// Unrest points; negative values make the city happier.
    CityUnrest,
    /// Percent change to the cost of units built in the city.
    UnitCostPercent,
    /// How far a city sees (base 2); Oracle, City Walls and Nature's Eye raise the minimum.
    CitySight,
}

impl Stat {
    /// Lowest value the stat can take, or `None` if it may go negative.
    pub fn floor(self) -> Option<i32> {
        match self {
            Stat::ToHit
            | Stat::CityUnrest
            | Stat::CityGrowth
            | Stat::CityUnrestPercent
            | Stat::SpellCostPercent
            | Stat::UnitCostPercent => None,
            _ => Some(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModOp {
    /// Added to the base.
    Add,
    /// Percent of the (base + adds) total, summed: `+50` is x1.5, `-25` is x0.75.
    Percent,
    /// The result is at least this.
    Min,
    /// The result is at most this.
    Max,
    /// The result is exactly this. If several apply, the lowest wins.
    Set,
}

/// What produced a modifier, so the UI can show "+2 from Iron Armour" and so a
/// source can be removed cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Race(Id<RaceDef>),
    Ability(Id<AbilityDef>),
    Building(Id<BuildingDef>),
    Spell(Id<SpellDef>),
    Item(Id<ItemDef>),
    /// Experience level reached, 0-based.
    Level(u8),
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modifier {
    pub stat: Stat,
    pub op: ModOp,
    pub value: i32,
    pub source: Source,
}

impl Modifier {
    pub fn new(stat: Stat, op: ModOp, value: i32, source: Source) -> Self {
        Self {
            stat,
            op,
            value,
            source,
        }
    }
}

/// Compute `stat` from its `base` and `mods` (modifiers for other stats are ignored).
///
/// Order: `base + Σ Add`, then `× (100 + Σ Percent) / 100` (rounded toward negative
/// infinity), then `Min`/`Max` bounds, then `Set` overrides everything, then the
/// stat's own floor.
pub fn compute(stat: Stat, base: i32, mods: &[Modifier]) -> i32 {
    let of = |op: ModOp| {
        mods.iter()
            .filter(move |m| m.stat == stat && m.op == op)
            .map(|m| m.value)
    };
    let adds: i64 = of(ModOp::Add).map(i64::from).sum();
    let percent: i64 = of(ModOp::Percent).map(i64::from).sum();
    let mut v = (base as i64 + adds) * (100 + percent).max(0) / 100;
    // `/` truncates toward zero; the doc promises floor, so fix up negatives.
    if (base as i64 + adds) * (100 + percent).max(0) % 100 < 0 {
        v -= 1;
    }
    let mut v = v.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    if let Some(min) = of(ModOp::Min).max() {
        v = v.max(min);
    }
    if let Some(max) = of(ModOp::Max).min() {
        v = v.min(max);
    }
    if let Some(set) = of(ModOp::Set).min() {
        v = set;
    }
    match stat.floor() {
        Some(floor) => v.max(floor),
        None => v,
    }
}

/// Stat modifiers from the always-on, unconditional rules in `effects`, tagged with
/// `source`. Rules with a condition, a trigger other than `Passive`, or any other
/// effect are the executor's job (they need game context).
pub fn passive_modifiers(effects: &[EffectRule], source: Source) -> Vec<Modifier> {
    effects
        .iter()
        .filter(|r| r.when == Trigger::Passive && r.condition == Condition::Always)
        .filter_map(|r| match r.effect {
            Effect::Modify { stat, op, value } => Some(Modifier::new(stat, op, value, source)),
            _ => None,
        })
        .collect()
}

/// The modifiers contributing to `stat`, in the given order (for tooltips).
pub fn contributions(stat: Stat, mods: &[Modifier]) -> Vec<Modifier> {
    mods.iter().copied().filter(|m| m.stat == stat).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(stat: Stat, op: ModOp, value: i32) -> Modifier {
        Modifier::new(stat, op, value, Source::Other)
    }

    #[test]
    fn adds_then_percent() {
        let mods = [
            m(Stat::Melee, ModOp::Add, 1),
            m(Stat::Melee, ModOp::Percent, 50),
        ];
        assert_eq!(compute(Stat::Melee, 3, &mods), 6); // (3+1) * 1.5
    }

    #[test]
    fn order_of_modifiers_does_not_matter() {
        let a = [
            m(Stat::Melee, ModOp::Percent, 50),
            m(Stat::Melee, ModOp::Add, 1),
            m(Stat::Melee, ModOp::Add, 2),
        ];
        let b = [a[1], a[2], a[0]];
        assert_eq!(compute(Stat::Melee, 3, &a), compute(Stat::Melee, 3, &b));
    }

    #[test]
    fn percent_rounds_down_including_negatives() {
        // 5 * 1.5 = 7.5 -> 7
        assert_eq!(
            compute(Stat::Melee, 5, &[m(Stat::Melee, ModOp::Percent, 50)]),
            7
        );
        // -5 * 1.5 = -7.5 -> -8 (floor); stat without a floor keeps it
        assert_eq!(
            compute(Stat::ToHit, -5, &[m(Stat::ToHit, ModOp::Percent, 50)]),
            -8
        );
    }

    #[test]
    fn bounds_and_set() {
        let up = [m(Stat::Moves, ModOp::Add, 5), m(Stat::Moves, ModOp::Max, 4)];
        assert_eq!(compute(Stat::Moves, 2, &up), 4);
        let down = [
            m(Stat::Moves, ModOp::Add, -5),
            m(Stat::Moves, ModOp::Min, 1),
        ];
        assert_eq!(compute(Stat::Moves, 2, &down), 1);
        let set = [
            m(Stat::Resistance, ModOp::Add, 3),
            m(Stat::Resistance, ModOp::Set, 10),
            m(Stat::Resistance, ModOp::Set, 8),
        ];
        assert_eq!(
            compute(Stat::Resistance, 4, &set),
            8,
            "lowest Set wins, overrides adds"
        );
    }

    #[test]
    fn floors_apply_only_where_defined() {
        assert_eq!(
            compute(Stat::Melee, 1, &[m(Stat::Melee, ModOp::Add, -4)]),
            0
        );
        assert_eq!(
            compute(Stat::CityUnrest, 1, &[m(Stat::CityUnrest, ModOp::Add, -4)]),
            -3
        );
    }

    #[test]
    fn other_stats_are_ignored_and_contributions_filter() {
        let mods = [
            m(Stat::Defense, ModOp::Add, 9),
            m(Stat::Melee, ModOp::Add, 1),
        ];
        assert_eq!(compute(Stat::Melee, 3, &mods), 4);
        assert_eq!(contributions(Stat::Melee, &mods), vec![mods[1]]);
    }

    #[test]
    fn extreme_values_do_not_overflow() {
        let mods = [
            m(Stat::Melee, ModOp::Add, i32::MAX),
            m(Stat::Melee, ModOp::Percent, 10_000),
        ];
        assert_eq!(compute(Stat::Melee, i32::MAX, &mods), i32::MAX);
    }
}
