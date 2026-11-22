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
