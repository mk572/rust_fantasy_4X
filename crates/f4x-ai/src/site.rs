//! Towers, magic nodes and encounter sites: a wizard's spare soldiers clear the guarded sites
//! near their cities. The AI plays by the human's rules (`design.md` section 8): it knows the
//! guards only from a fight it fought or a spell that shows them (`PlayerView::lair_guards`),
//! and guesses otherwise. A party goes in through `Command::EnterSite`, the battle AI fighting.

use f4x_core::map::{Plane, Position};
use f4x_core::player::view::PlayerView;

use crate::assault::guard_strength;
use crate::military::average_strength;
use crate::view::{distance, fighting_strength};

/// Squares from a city (or from a party in the field) within which a site is looked for.
pub(crate) const SITE_RANGE: i32 = 10;
/// Squares within which a party looks for a tower: it is the way to the other plane.
pub(crate) const TOWER_RANGE: u32 = 30;
/// What an unknown site's guards are guessed to be worth, in average soldiers of this wizard.
const GUESS_GUARDS: f32 = 2.5;

/// A party attacks when it is this much stronger than the guards, known or guessed.
pub(crate) fn edge() -> f32 {
    crate::knob("SITE", 1.4)
}

/// What the AI expects the site's guards to be worth.
pub(crate) fn expected(view: &PlayerView, site: Position) -> f32 {
    guard_strength(view, site).unwrap_or_else(|| GUESS_GUARDS * average_strength(view))
}

/// Is there a city of the neutral cities (no wizard's) on the square?
pub(crate) fn neutral_city(view: &PlayerView, p: Position) -> bool {
    view.known_cities()
        .iter()
        .any(|&(_, owner, q)| q == p && !view.is_rival_wizard(owner))
}

/// What a neutral city's defenders are worth (a city is stronger than the same men outside).
/// What guards the site: a city's defence, or the guards a lair or tower is expected to have.
pub(crate) fn defence_of(view: &PlayerView, p: Position) -> f32 {
    if view.city_at(p).is_some() {
        city_defence(view, p)
    } else {
        expected(view, p)
    }
}

pub(crate) fn city_defence(view: &PlayerView, p: Position) -> f32 {
    match view.hostile_at(p) {
        Some(defenders) => fighting_strength(view, &defenders) * 1.3,
        None => 2.0 * GUESS_GUARDS * average_strength(view),
    }
}

/// The explored, intact sites within [`SITE_RANGE`] of `from` (towers within [`TOWER_RANGE`]) that a party of `strength` should
/// take, nearest first.
pub(crate) fn sites_near(
    view: &PlayerView,
    from: Position,
    strength: f32,
    blocked: &[f4x_core::map::Plane],
) -> Vec<Position> {
    let mut found: Vec<Position> = Vec::new();
    for dy in -SITE_RANGE..=SITE_RANGE {
        for dx in -SITE_RANGE..=SITE_RANGE {
            if let Some(p) = view.shift(from, dx, dy) {
                if view.has_lair(p) && strength >= expected(view, p) * edge() && !found.contains(&p)
                {
                    found.push(p);
                }
            }
        }
    }
    for (_, owner, p) in view.known_cities() {
        if !view.is_rival_wizard(owner)
            && distance(view, from, p).is_some_and(|d| d <= SITE_RANGE as u32)
            && strength >= city_defence(view, p) * edge()
            && !found.contains(&p)
        {
            found.push(p);
        }
    }
    for (p, intact) in view.towers() {
        if intact
            && !found.contains(&p)
            && !blocked.contains(&p.plane)
            && distance(view, from, p).is_some_and(|d| d <= TOWER_RANGE)
            && strength >= expected(view, p) * edge()
        {
            found.push(p);
        }
    }
    found.sort_by_key(|&p| (distance(view, from, p), p.y, p.x));
    found
}

/// The planes where a rival's city stands in the way of the towers: a known city of a rival
/// wizard that the army can walk to (not one in `unreachable`, which a ship has to reach) or that a
/// landing is set on (`naval_target`). The rivals come before the towers; a rival across the sea
/// that no landing is bound for does not hold the towers back.
pub(crate) fn blocked_planes(
    view: &PlayerView,
    unreachable: &[Position],
    naval_target: Option<Position>,
) -> Vec<Plane> {
    let mut planes: Vec<Plane> = Vec::new();
    for (_, owner, p) in view.known_cities() {
        if view.is_rival_wizard(owner)
            && (!unreachable.contains(&p) || naval_target == Some(p))
            && !planes.contains(&p.plane)
        {
            planes.push(p.plane);
        }
    }
    planes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn_tower, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_tower_is_left_alone_while_a_rival_city_that_can_be_walked_to_is_on_its_plane() -> TestResult
    {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut g, at(33, 9), true)?;
        g.reveal_map(P0);
        let view = PlayerView::new(&g, P0);
        let rival = at(28, 6);
        // A rival city the army can walk to holds the towers back.
        let blocked = blocked_planes(&view, &[], None);
        assert_eq!(blocked, vec![rival.plane]);
        assert!(sites_near(&view, at(10, 6), 10_000.0, &blocked).is_empty());
        // One that only a ship can reach does not, unless a landing is set on it.
        assert!(blocked_planes(&view, &[rival], None).is_empty());
        assert_eq!(
            blocked_planes(&view, &[rival], Some(rival)),
            vec![rival.plane]
        );
        assert_eq!(sites_near(&view, at(10, 6), 10_000.0, &[]), vec![at(33, 9)]);
        Ok(())
    }
}
