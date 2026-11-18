//! The second measure of a unit (the first is what it adds to a battle, `battle::power`): how
//! quickly and easily it gets to a target square. It is the unit's movement rate times a
//! multiplier for what its kind of movement and its abilities do for it: the best one that applies,
//! as they mostly overlap, never several multiplied together. A flat multiplier, not a
//! walk over the terrain: a flier with two moves often gets where a walker with four is held up.
//!
//! Who asks: a garrison never does; a march or a raid is as slow as the slowest unit of the party,
//! unless magic carries the whole stack (pathfinding, wind walking), so it matters only when a fast
//! strike party is wanted; a scout is chosen by it first. The numbers are a first draft.
//!
//! Tests: below.

use crate::rules::{Ability, AbilityRules, MovementType, Stat, UnitDef};

/// How quickly a unit with `moves` movement points a turn, this kind of `movement` and the
/// abilities `has` says it has, gets to a square: the rate times what its movement gives it
/// (`AbilityRules::mobility` and the movement kinds, `rules.ron`).
pub fn unit_mobility(
    moves: i32,
    movement: MovementType,
    has: impl Fn(Ability) -> bool,
    rules: &AbilityRules,
) -> f32 {
    let rate = moves.max(0) as f32;
    let percent = |p: u32| p as f32 / 100.0;
    // How the unit crosses ground: most of these overlap, so the best one that applies counts and
    // they are never multiplied together.
    let by_ability = rules
        .mobility
        .iter()
        .filter(|&&(n, _)| has(Ability::new(n)))
        .map(|&(_, p)| percent(p));
    let by_movement = match movement {
        MovementType::Flying => Some(percent(rules.flying_movement_pct)),
        MovementType::Swimming => Some(percent(rules.swimming_movement_pct)),
        _ => None,
    };
    rate * by_ability.chain(by_movement).fold(1.0, f32::max)
}

/// [`unit_mobility`] of a unit definition (a unit a city could build).
pub fn def_mobility(d: &UnitDef, rules: &AbilityRules) -> f32 {
    unit_mobility(
        d.stats.get(&Stat::Moves).map_or(0, |v| v.floor()),
        d.movement,
        |a| d.abilities.contains(&a),
        rules,
    )
}

/// How quickly a party gets to a square: as quickly as its slowest unit, unless a unit of it has
/// something that carries the whole stack (pathfinding: every land square for half a move; wind
/// walking: the stack flies), which makes every unit of it twice as quick as its rate.
/// `members` are the mobility and the movement rate of each unit; `carried` says whether any unit
/// has a stack-carrying ability. `flying` is the flying multiplier in percent.
pub fn party_mobility(members: &[(f32, i32)], carried: bool, flying: f32) -> f32 {
    let slowest = members
        .iter()
        .map(|&(m, _)| m)
        .fold(f32::INFINITY, f32::min);
    if members.is_empty() {
        return 0.0;
    }
    if !carried {
        return slowest;
    }
    // Carried: each unit goes at the flying rate, so the slowest of them sets the pace.
    let lifted = members
        .iter()
        .map(|&(m, rate)| m.max(flying * rate.max(0) as f32))
        .fold(f32::INFINITY, f32::min);
    lifted.max(slowest)
}

/// Whether the unit has an ability that carries the stack it stands in (pathfinding, wind walking).
pub fn carries_stack(has: impl Fn(Ability) -> bool, rules: &AbilityRules) -> bool {
    rules.stack_carriers.iter().any(|&n| has(Ability::new(n)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::shipped;

    const NONE: fn(Ability) -> bool = |_| false;

    fn rules() -> AbilityRules {
        shipped().rules.config.abilities.clone()
    }

    /// What the best ability of the table gives a walker of two moves.
    fn with(n: u16) -> f32 {
        unit_mobility(2, MovementType::Walking, |a| a == Ability::new(n), &rules())
    }

    fn pct(rules: &AbilityRules, n: u16) -> f32 {
        let p = rules.mobility.iter().find(|&&(a, _)| a == n);
        p.map_or(0.0, |&(_, p)| p as f32 / 100.0)
    }

    /// Each ability of the table multiplies the rate by its percent, the movement kinds too; the best
    /// one counts, none is multiplied.
    #[test]
    fn the_movement_kinds_multiply_the_rate() {
        let rules = rules();
        assert_eq!(unit_mobility(2, MovementType::Walking, NONE, &rules), 2.0);
        for &(n, p) in &rules.mobility {
            let want = 2.0 * (p as f32 / 100.0).max(1.0);
            assert!((with(n) - want).abs() < 1e-5, "ability {n}");
        }
        let flying = unit_mobility(2, MovementType::Flying, NONE, &rules);
        assert!((flying - 2.0 * rules.flying_movement_pct as f32 / 100.0).abs() < 1e-5);
        let swimming = unit_mobility(2, MovementType::Swimming, NONE, &rules);
        assert!((swimming - 2.0 * rules.swimming_movement_pct as f32 / 100.0).abs() < 1e-5);
        // Two abilities of the table: the better one, not both multiplied.
        let mut best = rules.mobility.clone();
        best.sort_by_key(|&(_, p)| p);
        let ((a, pa), (b, pb)) = (best[best.len() - 1], best[best.len() - 2]);
        let both = unit_mobility(
            2,
            MovementType::Walking,
            |x| x == Ability::new(a) || x == Ability::new(b),
            &rules,
        );
        assert!((both - 2.0 * pa.max(pb) as f32 / 100.0).abs() < 1e-5);
        assert!(pct(&rules, a) > 0.0);
    }

    /// What carries a whole stack rates at least as high as what only helps the unit.
    #[test]
    fn what_carries_the_stack_rates_above_what_helps_one_unit() {
        let rules = rules();
        let flying = unit_mobility(2, MovementType::Flying, NONE, &rules);
        for &n in &rules.stack_carriers {
            assert!(with(n) >= flying.min(4.0), "ability {n}");
        }
        assert!(!rules.stack_carriers.is_empty());
    }

    /// A party is as slow as its slowest unit, and magic that carries the stack lifts the slow
    /// ones.
    #[test]
    fn a_party_goes_at_the_pace_of_its_slowest_unit_unless_the_stack_is_carried() {
        let flying = shipped().rules.config.abilities.flying_movement_pct as f32 / 100.0;
        let party = [(1.0, 1), (6.0, 3), (2.0, 2)];
        assert_eq!(party_mobility(&party, false, flying), 1.0);
        // Carried, each unit goes at the flying rate, the slowest of them sets the pace.
        let lifted = party_mobility(&party, true, flying);
        assert_eq!(lifted, (flying * 1.0).min(flying * 2.0).max(1.0));
        assert_eq!(party_mobility(&[], false, flying), 0.0);
        assert_eq!(
            party_mobility(&[(4.0 * flying, 2)], true, flying),
            4.0 * flying,
            "never lifted below what it has"
        );
    }

    /// On the shipped content: no unit is less mobile than its rate, and a flier is at least as
    /// mobile as the flying movement makes it.
    #[test]
    fn the_shipped_units_are_never_less_mobile_than_their_rate() {
        let c = shipped();
        let rules = &c.rules.config.abilities;
        let mut fliers = 0;
        for (_, d) in c.rules.units.iter() {
            let rate = d.stats.get(&Stat::Moves).map_or(0, |v| v.floor());
            let m = def_mobility(d, rules);
            assert!(m >= rate as f32, "{}", d.id);
            if d.movement == MovementType::Flying {
                let flies = rules.flying_movement_pct as f32 / 100.0;
                assert!(m >= flies * rate as f32, "{}", d.id);
                fliers += 1;
            }
        }
        assert!(fliers > 5, "shipped fliers: {fliers}");
    }

    /// The abilities the table names are in the content.
    #[test]
    fn the_abilities_of_the_table_are_in_the_content() {
        let c = shipped();
        let rules = &c.rules.config.abilities;
        let numbers = rules
            .mobility
            .iter()
            .map(|&(n, _)| n)
            .chain(rules.stack_carriers.iter().copied());
        for n in numbers {
            assert!(c.rules.abilities.iter().any(|(_, d)| d.num == n), "{n}");
        }
    }
}
