//! Towers, magic nodes and encounter sites: a wizard's spare soldiers clear the guarded sites
//! near their cities. The AI plays by the human's rules (`design.md` section 8): it knows the
//! guards only from a fight it fought or a spell that shows them (`PlayerView::lair_guards`),
//! and guesses otherwise. A party goes in through `Command::EnterSite`, the battle AI fighting.

use crate::lever::Levered;
use f4x_core::map::{Plane, Position};
use f4x_core::player::view::PlayerView;

use crate::assault::guard_strength;
use crate::view::unit_strength;

/// What a site whose guards the wizard has not seen looks to be worth (0 where there is none).
fn apparent(view: &PlayerView, site: Position) -> f32 {
    view.lair_apparent(site).map_or(0.0, |s| s as f32)
}

/// What the AI expects the site's guards to be worth.
pub(crate) fn expected(view: &PlayerView, site: Position) -> f32 {
    guard_strength(view, site).unwrap_or_else(|| apparent(view, site))
}

/// Did a party as strong as this one (not `retry_edge` times as strong) already lose its assault
/// on the site? Then it is left alone:
/// what the party is worth against the guards was misjudged, and the same fight would come out the
/// same.
pub(crate) fn lost_before(
    losses: &[(Position, f32)],
    site: Position,
    strength: f32,
    retry_edge: f32,
) -> bool {
    losses
        .iter()
        .any(|&(p, lost)| p == site && strength < lost * retry_edge)
}

/// What the AI expects the guards to be worth to a party of which `unarmed` (0 to 1) is the share
/// of its strength with normal weapons and no spell to arm it: the guards that weapons barely hurt
/// count for more, up to `1 / immune_discount` times, as that share of the party is worth less.
pub(crate) fn expected_vs(view: &PlayerView, site: Position, unarmed: f32) -> f32 {
    match crate::assault::guard_parts(view, site) {
        Some((all, immune)) => {
            all + immune * unarmed * (1.0 / view.levers().sites.immune_discount - 1.0)
        }
        None => apparent(view, site),
    }
}

/// The share (0 to 1) of the strength of `units` that has normal weapons and cannot be armed with
/// a spell of the wizard before the fight (one cast arms one unit, as far as the mana of a battle
/// goes): against weapon-immune guards that share is worth little.
pub(crate) fn unarmed_share(view: &PlayerView, units: &[hecs::Entity]) -> f32 {
    let total: f32 = units.iter().map(|&u| unit_strength(view, u)).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let normal: f32 = units
        .iter()
        .filter(|&&u| !view.unit_has_magical_weapon(u))
        .map(|&u| unit_strength(view, u))
        .sum();
    normal / total * (1.0 - crate::spellhelper::arming_share(view, units.len()))
}

/// Is a lair or an intact tower, as far as the player knows, on the square?
pub(crate) fn live(view: &PlayerView, p: Position) -> bool {
    view.has_lair(p) && view.towers().iter().all(|&(t, intact)| t != p || intact)
}

/// The sites the player has explored that may be worth taking: lairs and nodes, and on each plane
/// the intact towers, but for the planes `blocked`.
pub(crate) fn known_sites(view: &PlayerView, blocked: &[f4x_core::map::Plane]) -> Vec<Position> {
    let towers = view.towers();
    let mut found: Vec<Position> = view
        .lair_sites()
        .into_iter()
        .filter(|&p| live(view, p) && towers.iter().all(|&(t, _)| (t.x, t.y) != (p.x, p.y)))
        .collect();
    found.extend(
        towers
            .iter()
            .filter(|&&(p, intact)| intact && !blocked.contains(&p.plane))
            .map(|&(p, _)| p),
    );
    found.sort_by_key(|&p| (p.plane as u8, p.y, p.x));
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

    /// Guards the wizard has not seen are judged by how strong they look; guards it has fought
    /// by what they are.
    #[test]
    fn an_unseen_site_is_judged_by_how_it_looks_and_a_fought_one_by_its_guards() -> TestResult {
        use crate::testworld::spawn_site_looking;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let guards = vec![(f4x_def::rules::UnitId::new(164) /* great_drake */, 2)];
        let (unseen, fought) = (at(12, 6), at(14, 6));
        spawn_site_looking(&mut w, unseen, guards.clone(), Vec::new(), Some(7))?;
        spawn_site_looking(&mut w, fought, guards, vec![P0], Some(7))?;
        w.reveal_map(P0);
        let view = PlayerView::new(&w, P0);
        assert_eq!(expected(&view, unseen), 7.0);
        assert_eq!(expected_vs(&view, unseen, 1.0), 7.0);
        assert!(expected(&view, fought) > 7.0);
        Ok(())
    }

    #[test]
    fn a_tower_is_left_alone_while_a_rival_city_that_can_be_walked_to_is_on_its_plane() -> TestResult
    {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut w, at(33, 9), true)?;
        w.reveal_map(P0);
        let view = PlayerView::new(&w, P0);
        let rival = at(28, 6);
        // A rival city the army can walk to holds the towers back.
        let blocked = blocked_planes(&view, &[], None);
        assert_eq!(blocked, vec![rival.plane]);
        assert!(!known_sites(&view, &blocked).contains(&at(33, 9)));
        // One that only a ship can reach does not, unless a landing is set on it.
        assert!(blocked_planes(&view, &[rival], None).is_empty());
        assert_eq!(
            blocked_planes(&view, &[rival], Some(rival)),
            vec![rival.plane]
        );
        assert!(known_sites(&view, &[]).contains(&at(33, 9)));
        Ok(())
    }
}
