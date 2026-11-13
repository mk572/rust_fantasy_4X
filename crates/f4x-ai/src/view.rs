//! How the AI weighs what it sees: strengths and who can fight. It reads the game only through
//! a [`PlayerView`], which holds what a player may know.

use f4x_core::battle::strength;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_def::rules::Ability;
use f4x_def::rules::{MovementType, Stat};

/// The answers of `can_reach` for one stretch of the AI's turn in which nothing moves: stacks that
/// stand on one square and move alike (the same kinds of unit) get one path search between them,
/// where a search for each of hundreds of units would cost most of the turn.
#[derive(Default)]
pub struct ReachCache {
    squares: std::collections::HashMap<(f4x_core::unit::pathing::ReachSignature, Position), bool>,
    /// Whether any square round a site can be reached, by the site.
    around: std::collections::HashMap<(f4x_core::unit::pathing::ReachSignature, Position), bool>,
}

impl ReachCache {
    /// How many searches the cache has made (one for each stack signature and target).
    #[cfg(test)]
    pub fn searches(&self) -> usize {
        self.squares.len() + self.around.len()
    }

    pub fn can_reach(&mut self, view: &PlayerView, units: &[hecs::Entity], to: Position) -> bool {
        match view.reach_signature(units) {
            Some(key) => *self
                .squares
                .entry((key, to))
                .or_insert_with(|| view.can_reach(units, to)),
            None => view.can_reach(units, to),
        }
    }

    /// Whether the stack can walk to any of `squares`, which are the free squares round `site`.
    pub fn can_reach_round(
        &mut self,
        view: &PlayerView,
        units: &[hecs::Entity],
        site: Position,
        squares: &[Position],
    ) -> bool {
        match view.reach_signature(units) {
            Some(key) => *self
                .around
                .entry((key, site))
                .or_insert_with(|| view.reaches_any(units, squares)),
            None => view.reaches_any(units, squares),
        }
    }
}

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
    !view.unit_has_ability(unit, Ability::new(12))
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

/// Squares within which a city of a wizard at war counts as near.
const BORDER_RANGE: u32 = 12;

/// Fighters a city keeps at home. A full garrison is for a city at war with an enemy city near,
/// or one that matters (the fortress); every other city keeps one, the fortress two. The rest
/// of the soldiers are free to scout, relieve, and take sites and neutral cities.
pub fn garrison_wanted(view: &PlayerView, at: Position) -> usize {
    let fortress = view.fortress() == Some(at);
    let base = if fortress { 2 } else { 1 };
    let enemies = view.rivals_at_war();
    if enemies.is_empty() {
        return base;
    }
    let near = view.known_cities().into_iter().any(|(_, owner, p)| {
        enemies.contains(&owner) && view.distance(at, p).is_some_and(|d| d <= BORDER_RANGE)
    });
    match (near, fortress) {
        (true, true) => 6,
        (true, false) => 4,
        (false, true) => 3,
        (false, false) => base,
    }
}

/// In the endgame the weakest soldiers of a city come first, so that the garrison (the first
/// `garrison_wanted`) is the spearmen, and the strong ones (heroes, high-tier and veteran units,
/// summoned creatures) are the ones free to attack and raid. Earlier the order is left alone.
pub fn weakest_first(view: &PlayerView, units: &mut [hecs::Entity]) {
    if crate::phase::phase(view) == crate::phase::Phase::Win {
        units.sort_by(|&a, &b| unit_strength(view, a).total_cmp(&unit_strength(view, b)));
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let spear = spawn(&mut g, P0, city(), 104 /* high_men_spearmen */, 1)[0];
        let paladin = spawn(&mut g, P0, city(), 113 /* high_men_paladins */, 1)[0];
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let stack = spawn(&mut g, P0, city(), 104 /* high_men_spearmen */, 12);
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let spear = spawn(&mut g, P0, city(), 104 /* high_men_spearmen */, 1)[0];
        let galley = spawn(&mut g, P0, city(), 36 /* galley */, 1)[0];
        let settlers = spawn(&mut g, P0, city(), 111 /* high_men_settlers */, 1)[0];
        let view = PlayerView::new(&g, P0);
        assert!(is_fighter(&view, spear) && !is_ship(&view, spear));
        assert!(is_ship(&view, galley) && !is_fighter(&view, galley));
        assert!(!is_fighter(&view, settlers) && !is_ship(&view, settlers));
    }

    #[test]
    fn the_cache_gives_the_answer_of_a_search_for_each_stack_and_keeps_one_per_kind() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let spears = spawn(&mut g, P0, city(), 104 /* high_men_spearmen */, 3);
        let galley = spawn(&mut g, P0, city(), 36 /* galley */, 1)[0];
        let view = PlayerView::new(&g, P0);
        let mut cache = ReachCache::default();
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        let squares = [inland, city(), at(0, 0), at(5, 5)];
        for stack in [&spears[..1], &spears[1..3], &spears[..], &[galley][..]] {
            for &to in &squares {
                // Twice: the second answer comes from the cache.
                for _ in 0..2 {
                    assert_eq!(
                        cache.can_reach(&view, stack, to),
                        view.can_reach(stack, to),
                        "{to:?}"
                    );
                }
            }
            assert_eq!(
                cache.can_reach_round(&view, stack, inland, &squares),
                view.reaches_any(stack, &squares)
            );
        }
        // Spearmen on one square share a signature; a ship does not share theirs.
        assert_eq!(
            view.reach_signature(&spears[..1]),
            view.reach_signature(&spears[1..2])
        );
        assert_ne!(
            view.reach_signature(&spears[..1]),
            view.reach_signature(&[galley])
        );
    }

    #[test]
    fn the_region_of_a_failed_search_is_what_the_stack_can_walk_to() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let spears = spawn(&mut g, P0, city(), 104 /* high_men_spearmen */, 2);
        let galley = spawn(&mut g, P0, city(), 36 /* galley */, 1)[0];
        let view = PlayerView::new(&g, P0);
        let squares = [
            at(INLAND_CITY.0, INLAND_CITY.1),
            at(0, 0),
            at(5, 5),
            at(8, 8),
        ];
        for stack in [&spears[..1], &spears[..], &[galley][..]] {
            for &to in &squares {
                let region = view.unreached_region(stack, &[to]);
                assert_eq!(region.is_none(), view.reaches_any(stack, &[to]), "{to:?}");
                if let Some(region) = region {
                    assert_eq!(region, view.reachable(stack), "{to:?}");
                    assert!(!region.contains(&to));
                }
            }
        }
    }

    #[test]
    fn five_hundred_units_on_a_few_squares_cost_a_few_searches() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let squares = [
            city(),
            at(INLAND_CITY.0, INLAND_CITY.1),
            at(2, 2),
            at(3, 5),
            at(8, 8),
        ];
        let mut all = Vec::new();
        for &square in &squares {
            all.extend(spawn(
                &mut g, P0, square, 104, /* high_men_spearmen */
                100,
            ));
        }
        assert_eq!(all.len(), 500);
        let view = PlayerView::new(&g, P0);
        let mut cache = ReachCache::default();
        let target = at(6, 3);
        let reached = all
            .iter()
            .filter(|&&u| cache.can_reach(&view, &[u], target))
            .count();
        assert!(reached <= 500);
        assert_eq!(
            cache.searches(),
            squares.len(),
            "one search for each square the units stand on, not one for each unit"
        );
    }

    #[test]
    fn a_square_is_coastal_when_the_sea_touches_it() {
        let g = game(COAST_CITY, 8 /* high_men */);
        let view = PlayerView::new(&g, P0);
        assert!(is_coastal(&view, city()));
        assert!(!is_coastal(&view, at(INLAND_CITY.0, INLAND_CITY.1)));
    }

    #[test]
    fn a_city_keeps_one_fighter_at_home_and_the_fortress_city_two() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let walker = spawn(&mut g, P0, city(), 104 /* high_men_spearmen */, 1)[0];
        let view = PlayerView::new(&g, P0);
        let to = nearest_unexplored(&view, walker).ok_or("something is unexplored")?;
        assert!(!view.explored(to));
        assert!(view.reachable(&[walker]).contains(&to));
        g.reveal_map(P0);
        assert_eq!(nearest_unexplored(&PlayerView::new(&g, P0), walker), None);
        Ok(())
    }

    #[test]
    fn in_the_endgame_the_weakest_stand_first_and_so_garrison_the_city() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let paladin = spawn(&mut g, P0, city(), 113 /* high_men_paladins */, 1)[0];
        let spear = spawn(&mut g, P0, city(), 104 /* high_men_spearmen */, 1)[0];
        let order = |g: &f4x_core::game::Game| {
            let view = PlayerView::new(g, P0);
            let mut units = vec![paladin, spear];
            weakest_first(&view, &mut units);
            units
        };
        assert_eq!(order(&g), vec![paladin, spear], "early: left alone");
        g.turn = 1_000;
        assert_eq!(
            order(&g),
            vec![spear, paladin],
            "late: the spearman holds the city"
        );
        Ok(())
    }
}
