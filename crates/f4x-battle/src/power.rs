//! The one measure of what a unit adds to a battle: how much more likely a side is to win with it
//! than without it. Everyone that weighs units uses it: the strategic AI (who goes into the army,
//! how strong a garrison is, what to build), the battle AI, the pick screen and the engine's
//! fighter preset.
//!
//! A battle is a series of blows from both sides, so the measure has no side to it: what a unit
//! does when it attacks and what it takes when it is attacked are one number. It reads the unit's
//! *full* health (`MaxHp` over all figures, and every figure also strikes), not what it has left: moving the hurt back to safety
//! is the army builder's business (`f4x_ai::recover`), not part of how strong a unit is. The
//! numbers are a first draft, eyeballed; refine them here and every user follows.
//!
//! How fast a unit gets to a square is another measure (mobility), which only some jobs ask for:
//! a garrison does not care, a strike party is as slow as its slowest unit, a scout is all about it.
//!
//! Tests: below.

use f4x_def::rules::{Ability, Stat, UnitDef};

/// What a special ability adds to a unit's worth in a battle, as a share, from the offence it gives
/// and from the defence it gives (the ability numbers are those of `abilities.ron`). A battle has
/// no side, so the two shares add.
pub(crate) const ABILITY_BONUS: &[(u16, f32, f32)] = &[
    (1, 0.25, 0.0),   // armor piercing
    (8, 0.10, 0.0),   // cause fear
    (9, 0.05, 0.0),   // cavalry
    (15, 0.15, 0.0),  // death gaze
    (20, 0.25, 0.0),  // fire breath
    (22, 0.20, 0.0),  // fireball attack
    (23, 0.15, 0.0),  // first strike
    (32, 0.10, 0.0),  // immolation
    (35, 0.15, 0.0),  // life steal
    (36, 0.20, 0.0),  // lightning bolt
    (37, 0.25, 0.0),  // lightning breath
    (41, 0.10, 0.0),  // magical weapon
    (53, 0.20, 0.0),  // poison touch
    (64, 0.15, 0.0),  // stoning gaze
    (66, 0.20, 0.0),  // stoning touch
    (69, 0.10, 0.0),  // thrown
    (70, 0.10, 0.0),  // wall crusher
    (24, 0.10, 0.10), // flying
    (28, 0.10, 0.10), // holy bonus
    (33, 0.0, 0.10),  // invisibility
    (34, 0.0, 0.15),  // large shield
    (39, 0.05, 0.10), // lucky
    (40, 0.0, 0.20),  // magic immunity
    (44, 0.0, 0.30),  // missile immunity
    (48, 0.0, 0.10),  // negate first strike
    (49, 0.0, 0.20),  // non-corporeal
    (55, 0.0, 0.30),  // regeneration
    (56, 0.0, 0.15),  // resist all
    (72, 0.0, 0.40),  // weapon immunity
];

/// The offence and the defence multipliers of the abilities `has` says a unit has (1 for none).
pub(crate) fn ability_bonus(has: impl Fn(Ability) -> bool) -> (f32, f32) {
    ABILITY_BONUS
        .iter()
        .filter(|&&(n, _, _)| has(Ability::new(n)))
        .fold((1.0, 1.0), |(o, d), &(_, bo, bd)| (o + bo, d + bd))
}

/// The kernel: what an attack value, a defence value and a number of hit points are worth together.
/// The blow a unit deals counts one and a half times what the defence it puts up does.
pub(crate) fn kernel(attack: f32, defense: f32, hits: f32) -> f32 {
    (attack.max(0.0) * 1.5 + defense.max(0.0) + 1.0) * hits.max(0.0)
}

/// What a unit with these `stat` values and abilities adds to a battle, at full health. A unit with
/// both a melee and a ranged attack counts its better one and a quarter of the other, and
/// resistance counts a tenth of a point of defence.
pub fn unit_power(stat: impl Fn(Stat) -> i32, has: impl Fn(Ability) -> bool) -> f32 {
    let (melee, ranged) = (stat(Stat::Melee).max(0), stat(Stat::Ranged).max(0));
    let attack = melee.max(ranged) as f32 + 0.25 * melee.min(ranged) as f32;
    let defense = stat(Stat::Defense) as f32 + 0.1 * stat(Stat::Resistance).max(0) as f32;
    let figures = stat(Stat::Figures).max(1);
    let hits = (stat(Stat::MaxHp).max(0) * figures) as f32;
    let (off, def) = ability_bonus(has);
    // Every figure attacks and every figure is hit points, so figures count twice: for the
    // blows dealt and for the blows it takes to wear the unit down. (The firepower falls as
    // figures die, which halves what a unit deals over a fight but not how it grows with them.)
    kernel(attack, defense, hits) * figures as f32 * (off + def - 1.0)
}

/// The formula alone for a unit definition: the strength of a unit with no row in the rating file
/// (`def_power` falls back to it) and the measure the rating's scale is taken from.
pub fn def_formula(d: &UnitDef) -> f32 {
    unit_power(
        |s| d.stats.get(&s).map_or(0, |v| v.floor()),
        |a| d.abilities.contains(&a),
    )
}

/// What a unit definition adds to a battle (a unit that does not exist yet: what a city could
/// build, what a spell summons): its measured combat rating at the first level
/// (`Rules::combat_power`), the formula for a unit that has no row.
pub fn def_power(d: &UnitDef, rules: &f4x_def::rules::Rules) -> f32 {
    rules.combat_power(d, 0).unwrap_or_else(|| def_formula(d))
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::shipped;

    /// A stronger fighter is a higher number: the measure follows the stats it reads.
    #[test]
    fn a_unit_with_more_of_everything_is_worth_more() {
        let base = |attack: i32, defense: i32, hp: i32| {
            unit_power(
                |s| match s {
                    Stat::Melee => attack,
                    Stat::Defense => defense,
                    Stat::MaxHp => hp,
                    Stat::Figures => 1,
                    _ => 0,
                },
                |_| false,
            )
        };
        let one = base(3, 3, 4);
        assert!(base(4, 3, 4) > one);
        assert!(base(3, 4, 4) > one);
        assert!(base(3, 3, 5) > one);
        assert!(base(0, 0, 0) == 0.0);
    }

    /// Every figure strikes and every figure is hit points, and a figure that dies stops
    /// striking: doubling the figures more than doubles what the unit adds to a battle.
    #[test]
    fn figures_count_for_the_blows_and_for_the_hits() {
        let with = |figures: i32| {
            unit_power(
                |s| match s {
                    Stat::Melee => 2,
                    Stat::Defense => 2,
                    Stat::MaxHp => 2,
                    Stat::Figures => figures,
                    _ => 0,
                },
                |_| false,
            )
        };
        assert!(with(2) > 2.0 * with(1));
        assert!(with(6) > 6.0 * with(1));
    }

    /// Every ability of the table that a shipped unit has makes it worth more than the same unit
    /// without it: read from the content, not copied into the test.
    #[test]
    fn an_ability_of_the_table_adds_to_a_unit_that_has_it() {
        let c = shipped();
        let mut checked = 0;
        for (_, d) in c.rules.units.iter() {
            let listed: Vec<Ability> = d
                .abilities
                .iter()
                .copied()
                .filter(|a| ABILITY_BONUS.iter().any(|&(n, ..)| Ability::new(n) == *a))
                .collect();
            if listed.is_empty() {
                continue;
            }
            let stat = |s: Stat| d.stats.get(&s).map_or(0, |v| v.floor());
            let without = unit_power(stat, |a| !listed.contains(&a) && d.abilities.contains(&a));
            assert!(def_formula(d) >= without, "{}", d.id);
            if def_formula(d) > without {
                checked += 1;
            }
        }
        assert!(checked > 10, "shipped units with abilities: {checked}");
    }
}
