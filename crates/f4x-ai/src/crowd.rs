//! Which unit steps off a full square, and where to. The game asks when a unit is about to
//! appear on a square that holds as many as it may (`f4x_core::unit::crowding`: a finished unit,
//! a summoning, hired mercenaries, a hero): it names the units that could step to a free square
//! next to it, and the AI answers with one of them and one of those squares. The weakest unit
//! goes, so the army keeps its strength; a hero goes last. Only the squares next to the full one
//! are offered, so the unit stays within a step of where it was and the dispatcher finds it
//! again, awaiting orders.
//!
//! Tests: below, on the shipped content, with a full square.

use f4x_core::game::Game;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::unit::crowding::Leaver;

use crate::view::unit_strength;

/// The unit to move off the full square `at` and the square it steps to.
pub fn choose(
    game: &Game,
    player: PlayerId,
    _at: Position,
    leavers: &[Leaver],
) -> Option<(hecs::Entity, Position)> {
    let view = PlayerView::new(game, player);
    let hero = |u: hecs::Entity| {
        view.unit_def(u)
            .is_some_and(|d| d.kind == f4x_def::rules::UnitKind::Hero)
    };
    let (unit, squares) = leavers.iter().min_by(|(a, _), (b, _)| {
        hero(*a)
            .cmp(&hero(*b))
            .then(unit_strength(&view, *a).total_cmp(&unit_strength(&view, *b)))
            .then(view.serial_of(*a).cmp(&view.serial_of(*b)))
    })?;
    let to = squares.iter().min_by_key(|p| (p.plane as u8, p.y, p.x))?;
    Some((*unit, *to))
}

#[cfg(test)]
mod tests {
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_city_that_finishes_a_unit_on_a_full_square_steps_its_weakest_unit_out() -> TestResult {
        crate::battle::install();
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let limit = g.max_tile();
        let paladins = spawn(&mut g, P0, city, 113 /* high_men_paladins */, 2);
        let spears = spawn(
            &mut g,
            P0,
            city,
            104, /* high_men_spearmen */
            limit - 2,
        );
        assert_eq!(g.units().units_at(city).len(), limit);
        assert!(g.can_make_room(city, P0, 1));
        assert!(g.make_room(city, P0, 1));
        let left = g.units().units_at(city);
        assert_eq!(left.len(), limit - 1);
        assert!(paladins.iter().all(|p| left.contains(p)), "the strong stay");
        let gone: Vec<_> = spears.iter().filter(|u| !left.contains(u)).collect();
        assert_eq!(gone.len(), 1);
        let step = g
            .units()
            .unit_position(*gone[0])
            .ok_or("the unit stands somewhere")?;
        let (dx, dy) = (step.x.abs_diff(city.x), step.y.abs_diff(city.y));
        assert_eq!(dx.max(dy), 1, "it stepped to a square next to the city");
        Ok(())
    }
}
