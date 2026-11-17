//! The second measure of a unit (the first is what it adds to a battle, `battle::power`): how
//! quickly and easily it gets to a target square. It is the unit's movement rate times a
//! multiplier for what its kind of movement and its abilities do for it. A flat multiplier, not a
//! walk over the terrain: a flier with two moves often gets where a walker with four is held up.
//!
//! Who asks: a garrison never does; a march or a raid is as slow as the slowest unit of the party,
//! unless magic carries the whole stack (pathfinding, wind walking), so it matters only when a fast
//! strike party is wanted; a scout is chosen by it first. The numbers are a first draft.
//!
//! Tests: below.

use crate::rules::{Ability, MovementType, Stat, UnitDef};

/// Non-corporeal: doubles the rate and moves through any terrain.
const NON_CORPOREAL: f32 = 3.0;
/// Flying: over any terrain and across the water.
const FLYING: f32 = 2.0;
/// Swimming: the water as well as the land.
const SWIMMING: f32 = 1.5;
/// Mountaineer and forester, each: the hills, mountains and forests at no extra cost.
const TERRAIN_WISE: f32 = 1.2;
/// Water walking: the water as the land, without a ship.
const WATER_WALKING: f32 = 1.2;
/// Cavalry: fast on open ground.
const CAVALRY: f32 = 1.1;

/// What carries a whole stack when any unit of it has it: every square of land costs half a move
/// (pathfinding) or the stack flies (wind walking).
const STACK_PATHFINDING: u16 = 50;
const STACK_WIND_WALKING: u16 = 74;

const ABILITY_NON_CORPOREAL: u16 = 49;
const ABILITY_FLYING: u16 = 24;
const ABILITY_MOUNTAINEER: u16 = 45;
const ABILITY_FORESTER: u16 = 25;
const ABILITY_WATER_WALKING: u16 = 71;
const ABILITY_CAVALRY: u16 = 9;

/// How quickly a unit with `moves` movement points a turn, this kind of `movement` and the
/// abilities `has` says it has, gets to a square: the rate times what its movement gives it.
pub fn unit_mobility(moves: i32, movement: MovementType, has: impl Fn(Ability) -> bool) -> f32 {
    let has = |n: u16| has(Ability::new(n));
    let rate = moves.max(0) as f32;
    // How the unit crosses ground: one of these, the best that applies.
    let kind = if has(ABILITY_NON_CORPOREAL) {
        NON_CORPOREAL
    } else if movement == MovementType::Flying || has(ABILITY_FLYING) {
        FLYING
    } else if movement == MovementType::Swimming {
        SWIMMING
    } else {
        // A walker: what it is wise about adds up, the hills and the forests and the water.
        let mut m = 1.0;
        for (ability, share) in [
            (ABILITY_MOUNTAINEER, TERRAIN_WISE),
            (ABILITY_FORESTER, TERRAIN_WISE),
            (ABILITY_WATER_WALKING, WATER_WALKING),
            (ABILITY_CAVALRY, CAVALRY),
        ] {
            if has(ability) {
                m *= share;
            }
        }
        m
    };
    rate * kind
}

/// [`unit_mobility`] of a unit definition (a unit a city could build).
pub fn def_mobility(d: &UnitDef) -> f32 {
    unit_mobility(
        d.stats.get(&Stat::Moves).map_or(0, |v| v.floor()),
        d.movement,
        |a| d.abilities.contains(&a),
    )
}

/// How quickly a party gets to a square: as quickly as its slowest unit, unless a unit of it has
/// something that carries the whole stack (pathfinding: every land square for half a move; wind
/// walking: the stack flies), which makes every unit of it twice as quick as its rate.
/// `members` are the mobility and the movement rate of each unit; `carried` says whether any unit
/// has a stack-carrying ability.
pub fn party_mobility(members: &[(f32, i32)], carried: bool) -> f32 {
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
    // Carried: each unit goes at twice its own rate, so the slowest of them sets the pace.
    let lifted = members
        .iter()
        .map(|&(m, rate)| m.max(FLYING * rate.max(0) as f32))
        .fold(f32::INFINITY, f32::min);
    lifted.max(slowest)
}

/// Whether the unit has an ability that carries the stack it stands in (pathfinding, wind walking).
pub fn carries_stack(has: impl Fn(Ability) -> bool) -> bool {
    [STACK_PATHFINDING, STACK_WIND_WALKING]
        .iter()
        .any(|&n| has(Ability::new(n)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::shipped;

    const NONE: fn(Ability) -> bool = |_| false;

    /// The multipliers of the design: non-corporeal three times the rate, flying twice, swimming
    /// one and a half, mountaineer and forester a fifth more each.
    #[test]
    fn the_movement_kinds_multiply_the_rate() {
        let with = |n: u16| move |a: Ability| a == Ability::new(n);
        assert_eq!(unit_mobility(2, MovementType::Walking, NONE), 2.0);
        assert_eq!(unit_mobility(2, MovementType::Walking, with(49)), 6.0);
        assert_eq!(unit_mobility(2, MovementType::Flying, NONE), 4.0);
        assert_eq!(unit_mobility(2, MovementType::Walking, with(24)), 4.0);
        assert_eq!(unit_mobility(2, MovementType::Swimming, NONE), 3.0);
        assert!((unit_mobility(2, MovementType::Walking, with(45)) - 2.4).abs() < 1e-5);
        let both = |a: Ability| a == Ability::new(45) || a == Ability::new(25);
        assert!((unit_mobility(2, MovementType::Walking, both) - 2.88).abs() < 1e-5);
    }

    /// A flier with two moves gets there as quickly as a walker with four: the terrain holds the
    /// walker up.
    #[test]
    fn a_flier_of_two_moves_is_as_mobile_as_a_walker_of_four() {
        assert_eq!(
            unit_mobility(2, MovementType::Flying, NONE),
            unit_mobility(4, MovementType::Walking, NONE)
        );
    }

    /// A party is as slow as its slowest unit, and magic that carries the stack lifts the slow
    /// ones.
    #[test]
    fn a_party_goes_at_the_pace_of_its_slowest_unit_unless_the_stack_is_carried() {
        let party = [(1.0, 1), (6.0, 3), (2.0, 2)];
        assert_eq!(party_mobility(&party, false), 1.0);
        assert_eq!(
            party_mobility(&party, true),
            2.0,
            "the slowest at twice its rate"
        );
        assert_eq!(party_mobility(&[], false), 0.0);
        assert_eq!(
            party_mobility(&[(4.0, 2)], true),
            4.0,
            "never lifted below what it has"
        );
    }

    /// On the shipped content: a unit that flies or is non-corporeal is at least as mobile as a
    /// walker of its own rate, and no unit is less mobile than its rate.
    #[test]
    fn the_shipped_units_are_never_less_mobile_than_their_rate() {
        let c = shipped();
        let mut fliers = 0;
        for (_, d) in c.rules.units.iter() {
            let rate = d.stats.get(&Stat::Moves).map_or(0, |v| v.floor());
            let m = def_mobility(d);
            assert!(m >= rate as f32, "{}", d.id);
            if d.movement == MovementType::Flying {
                assert!(m >= 2.0 * rate as f32, "{}", d.id);
                fliers += 1;
            }
        }
        assert!(fliers > 5, "shipped fliers: {fliers}");
    }

    /// The stack-carrying abilities exist in the shipped content with the numbers the code reads.
    #[test]
    fn the_carrying_abilities_are_pathfinding_and_wind_walking() {
        let c = shipped();
        let id = |n: u16| {
            c.rules
                .abilities
                .iter()
                .map(|(_, d)| d)
                .find(|d| d.num == n)
                .map(|d| d.id.clone())
        };
        assert_eq!(id(STACK_PATHFINDING).as_deref(), Some("pathfinding"));
        assert_eq!(id(STACK_WIND_WALKING).as_deref(), Some("wind_walking"));
        assert_eq!(id(ABILITY_NON_CORPOREAL).as_deref(), Some("non_corporeal"));
        assert_eq!(id(ABILITY_FLYING).as_deref(), Some("flying"));
        assert_eq!(id(ABILITY_MOUNTAINEER).as_deref(), Some("mountaineer"));
        assert_eq!(id(ABILITY_FORESTER).as_deref(), Some("forester"));
        assert_eq!(id(ABILITY_WATER_WALKING).as_deref(), Some("water_walking"));
        assert_eq!(id(ABILITY_CAVALRY).as_deref(), Some("cavalry"));
    }
}
