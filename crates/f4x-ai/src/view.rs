//! How the AI weighs what it sees: strengths and who can fight. It reads the game only through
//! a [`PlayerView`], which holds what a player may know.

use f4x_core::battle::ai::strength;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::{MovementType, Stat};

/// How hard a unit hits and how much it can take, as one number.
pub fn unit_strength(view: &PlayerView, unit: hecs::Entity) -> f32 {
    let attack = view
        .unit_stat(unit, Stat::Melee)
        .max(view.unit_stat(unit, Stat::Ranged));
    let (hits, _) = view.unit_hits(unit);
    strength(attack, view.unit_stat(unit, Stat::Defense), hits as i32)
}

pub fn stack_strength(view: &PlayerView, units: &[hecs::Entity]) -> f32 {
    units.iter().map(|&u| unit_strength(view, u)).sum()
}

/// What a defended square puts into a battle: a side fights with at most nine units, the
/// strongest of a larger stack, and the rest are a reserve.
pub fn fighting_strength(view: &PlayerView, units: &[hecs::Entity]) -> f32 {
    let mut each: Vec<f32> = units.iter().map(|&u| unit_strength(view, u)).collect();
    each.sort_by(|a, b| b.total_cmp(a));
    each.iter().take(f4x_core::unit::MAX_STACK).sum()
}

/// A ship: it sails and carries land units.
pub fn is_ship(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_stat(unit, Stat::Transport) > 0
        && view
            .unit_def(unit)
            .is_some_and(|d| d.movement == MovementType::Sailing)
}

/// Does the square touch the sea?
pub fn is_coastal(view: &PlayerView, at: Position) -> bool {
    (-1..=1).any(|dy| {
        (-1..=1).any(|dx| {
            view.shift(at, dx, dy)
                .is_some_and(|p| view.terrain_class(p) == Some(TerrainClass::Ocean))
        })
    })
}

/// A unit that can fight on land (settlers, ships and the like are not counted).
pub fn is_fighter(view: &PlayerView, unit: hecs::Entity) -> bool {
    !view.unit_has_ability(unit, "create_outpost")
        && !is_ship(view, unit)
        && view
            .unit_stat(unit, Stat::Melee)
            .max(view.unit_stat(unit, Stat::Ranged))
            > 0
}

/// King-move distance with the horizontal wrap; `None` across planes.
pub fn distance(view: &PlayerView, a: Position, b: Position) -> Option<u32> {
    view.distance(a, b)
}

/// The nearest square this wizard has not explored that the unit can walk to, looking outwards
/// from where it stands (the rings are walked in steps of two, as the army's scouts do). Of
/// the squares of the nearest ring that can be reached the last one in scan order is taken.
pub fn nearest_unexplored(view: &PlayerView, unit: hecs::Entity) -> Option<Position> {
    let from = view.unit_position(unit)?;
    let reachable = view.reachable(&[unit]);
    for radius in 3i32..=14 {
        let mut found = None;
        for dy in (-radius..=radius).step_by(2) {
            for dx in (-radius..=radius).step_by(2) {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                if let Some(p) = view.shift(from, dx, dy) {
                    if !view.explored(p) && reachable.contains(&p) {
                        found = Some(p);
                    }
                }
            }
        }
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Fighters a city keeps at home: the fortress city holds two, every other city one.
pub fn garrison_wanted(view: &PlayerView, at: Position) -> usize {
    if view.fortress() == Some(at) {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, INLAND_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn city() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    #[test]
    fn a_stronger_unit_has_a_bigger_strength_and_a_stack_adds_up() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let spear = spawn(&mut g, P0, city(), "high_men_spearmen", 1)[0];
        let paladin = spawn(&mut g, P0, city(), "high_men_paladins", 1)[0];
        let view = PlayerView::new(&g, P0);
        let (spear_s, paladin_s) = (unit_strength(&view, spear), unit_strength(&view, paladin));
        assert!(
            spear_s > 0.0 && paladin_s > spear_s,
            "{spear_s} {paladin_s}"
        );
        assert_eq!(
            stack_strength(&view, &[spear, paladin]),
            spear_s + paladin_s
        );
        Ok(())
    }

    #[test]
    fn a_stack_fights_with_its_strongest_nine() {
        let mut g = game(COAST_CITY, "high_men");
        let stack = spawn(&mut g, P0, city(), "high_men_spearmen", 12);
        let view = PlayerView::new(&g, P0);
        let one = unit_strength(&view, stack[0]);
        assert_eq!(fighting_strength(&view, &stack[..3]), 3.0 * one);
        assert_eq!(
            fighting_strength(&view, &stack),
            f4x_core::unit::MAX_STACK as f32 * one,
            "the rest are a reserve"
        );
    }

    #[test]
    fn only_a_unit_that_fights_on_land_is_a_fighter_and_only_a_carrier_that_sails_is_a_ship() {
        let mut g = game(COAST_CITY, "high_men");
        let spear = spawn(&mut g, P0, city(), "high_men_spearmen", 1)[0];
        let galley = spawn(&mut g, P0, city(), "galley", 1)[0];
        let settlers = spawn(&mut g, P0, city(), "high_men_settlers", 1)[0];
        let view = PlayerView::new(&g, P0);
        assert!(is_fighter(&view, spear) && !is_ship(&view, spear));
        assert!(is_ship(&view, galley) && !is_fighter(&view, galley));
        assert!(!is_fighter(&view, settlers) && !is_ship(&view, settlers));
    }

    #[test]
    fn a_square_is_coastal_when_the_sea_touches_it() {
        let g = game(COAST_CITY, "high_men");
        let view = PlayerView::new(&g, P0);
        assert!(is_coastal(&view, city()));
        assert!(!is_coastal(&view, at(INLAND_CITY.0, INLAND_CITY.1)));
    }

    #[test]
    fn a_city_keeps_one_fighter_at_home_and_the_fortress_city_two() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        assert_eq!(garrison_wanted(&PlayerView::new(&g, P0), city()), 1);
        g.wizard_mut(P0)
            .ok_or("player 0 is a wizard")?
            .put(Some(f4x_core::wizard::fortress::Fortress(city())));
        let view = PlayerView::new(&g, P0);
        assert_eq!(garrison_wanted(&view, city()), 2);
        assert_eq!(garrison_wanted(&view, inland), 1);
        Ok(())
    }

    #[test]
    fn the_nearest_unexplored_square_is_one_the_unit_can_walk_to_and_none_is_left_once_all_is_seen(
    ) -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let walker = spawn(&mut g, P0, city(), "high_men_spearmen", 1)[0];
        let view = PlayerView::new(&g, P0);
        let to = nearest_unexplored(&view, walker).ok_or("something is unexplored")?;
        assert!(!view.explored(to));
        assert!(view.reachable(&[walker]).contains(&to));
        g.reveal_map(P0);
        assert_eq!(nearest_unexplored(&PlayerView::new(&g, P0), walker), None);
        Ok(())
    }
}
