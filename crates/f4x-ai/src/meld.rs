//! A spirit goes to the nearest open magic node it can reach and melds with it (`Role::Meld`).
//! A node is open when it is not warped, not already the wizard's, and has no guards standing on
//! it (a guarded one is a site to clear first, `site.rs`). The spirit travels with an escort when
//! the city it leaves has a soldier to spare (`escort::walk`).
//!
//! The spirit that melds is used up. One that stands still on its way for a few turns drops the
//! order, is logged with the cause, and takes another node; nothing is held against the node.

use crate::roles::Role;
use crate::Ai;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::{Command, Event};
use f4x_def::rules::Ability;
use f4x_master::Game;

/// Turns a spirit may stand still on its way before the node is given up.
const PATIENCE: u32 = 4;

/// Whether the unit is a spirit that can meld.
pub fn is_melder(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_has_ability(unit, Ability::new(42 /* meld */))
}

/// The squares of nodes a spirit can meld with.
pub(crate) fn open_nodes(ai: &Ai, view: &PlayerView) -> Vec<Position> {
    let me = ai.player;
    let mut nodes: Vec<Position> = view
        .nodes()
        .into_iter()
        .filter(|n| !n.warped && n.owner != Some(me) && !view.has_lair(n.pos))
        .map(|n| n.pos)
        .collect();
    nodes.sort_by_key(|p| (p.plane as u8, p.y, p.x));
    nodes
}

/// Whether the wizard wants a spirit now and has none: a node is open, or a raid is out to clear
/// one, so a spirit should be ready to meld when it is done.
pub(crate) fn spirit_wanted(ai: &Ai, view: &PlayerView) -> bool {
    if view.my_units().into_iter().any(|u| is_melder(view, u)) {
        return false;
    }
    if !open_nodes(ai, view).is_empty() {
        return true;
    }
    // Early in the game a spirit is the best scout there is: it flies over what a soldier cannot
    // cross, and it is cheap (`scout.rs` recruits it).
    if crate::phase::phase(view) == crate::phase::Phase::Build
        && !ai.frontier_closed
        && ai.roles.scouting().is_empty()
    {
        return true;
    }
    let raided = ai.roles.targets(Role::Raid);
    view.nodes()
        .into_iter()
        .any(|n| n.owner != Some(ai.player) && !n.warped && raided.contains(&n.pos))
}

/// One turn of the spirits.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let turn = game.turn;
    let spirits: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let mut spirits: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_melder(&view, u))
            .collect();
        view.sort_by_serial(&mut spirits);
        spirits
    };
    for spirit in spirits {
        let (serial, here, ordered, nodes) = {
            let view = PlayerView::new(game, me);
            (
                view.serial_of(spirit),
                view.unit_position(spirit),
                view.unit_has_order(spirit),
                open_nodes(ai, &view),
            )
        };
        let Some(here) = here else { continue };
        // With no node to go to the spirit is left to the others (a scout, say).
        if nodes.is_empty() && !ai.roles.is(serial, Role::Meld) {
            continue;
        }
        if ai.roles.claim(serial, Role::Meld, turn, None).is_err() {
            continue;
        }
        // On an open node: meld.
        if nodes.contains(&here) {
            let _ = ai.send(
                game,
                Command::Meld {
                    units: vec![spirit],
                },
                events,
            );
            ai.roles.release(serial, Role::Meld);
            continue;
        }
        let target = ai.roles.target_of(serial, Role::Meld);
        // A spirit that has stood still on its order for too long is not getting there: the order
        // is dropped, the refused-order log gets the cause, and another node is chosen this turn.
        // Nothing is remembered against the node (a node that is taken or guarded is not open).
        let stuck = ai.roles.track(serial, here, ordered);
        let mut skip = None;
        if let (Some(t), true) = (target, stuck >= PATIENCE) {
            ai.note_failure(
                game,
                format!("spirit at {here:?} stood still on its way to {t:?}"),
                format!(
                    "Stuck{}{}",
                    crate::DETAIL_SEPARATOR,
                    game.why_no_path(&[spirit], t)
                ),
            );
            ai.order(
                game,
                Command::ClearOrders {
                    units: vec![spirit],
                },
                events,
            );
            ai.roles.clear_track(serial);
            ai.roles.release(serial, Role::Meld);
            skip = Some(t);
        } else if ordered && target.is_some_and(|t| nodes.contains(&t)) {
            continue;
        }
        let taken = ai.roles.targets(Role::Meld);
        // The nodes it can walk to, nearest first; the first one the walk planner takes an order for.
        let candidates: Vec<Position> = {
            let view = PlayerView::new(game, me);
            let mut list: Vec<Position> = nodes
                .iter()
                .copied()
                .filter(|p| p.plane == here.plane && skip != Some(*p))
                .filter(|p| target == Some(*p) || !taken.contains(p))
                .filter(|&p| view.can_reach(&[spirit], p))
                .collect();
            list.sort_by_key(|&p| (view.distance(here, p).unwrap_or(u32::MAX), p.y, p.x));
            list
        };
        let chosen = candidates
            .into_iter()
            .find(|&node| crate::escort::walk(ai, game, spirit, node, events));
        match chosen {
            Some(node) => {
                let _ = ai.roles.claim(serial, Role::Meld, turn, Some(node));
            }
            None => {
                ai.roles.release(serial, Role::Meld);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};
    use f4x_core::feature::entity::Placement;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A node of power 5 on `pos`, with no guards.
    fn node(w: &mut Game, pos: Position) -> Result<(), Box<dyn std::error::Error>> {
        let feature = w
            .content()
            .features
            .id_numbered(20 /* sorcery_node */)
            .ok_or("a node feature")?;
        w.spawn_feature(Placement {
            at: pos,
            feature,
            node: Some(f4x_core::feature::node::Node {
                realm: f4x_def::rules::RealmId::new(1),
                power: 5,
                aura: vec![(pos.x, pos.y)],
                guardian: false,
                warped: false,
            }),
            lair: None,
        });
        Ok(())
    }

    fn world() -> Result<(Game, hecs::Entity), Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.explore(P0, at(7, 6), f4x_def::half_int::HalfInt::from_int(5));
        node(&mut w, at(6, 6))?;
        let spirit = spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            154, /* magic_spirit */
            1,
        )[0];
        w.balance_farmers(P0);
        Ok((w, spirit))
    }

    #[test]
    fn a_spirit_walks_to_the_open_node_and_melds_with_it() -> TestResult {
        let (mut w, spirit) = world()?;
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(spirit);
        assert_eq!(ai.roles.role_of(serial), Some(Role::Meld));
        assert_eq!(ai.roles.target_of(serial, Role::Meld), Some(at(6, 6)));
        assert!(PlayerView::new(&w, P0).unit_has_order(spirit));
        // Once on the node it melds and is used up (the spirit is put there: the test wizard has
        // no mana to keep it through a turn).
        w.cheat_move_unit(spirit, at(6, 6));
        run(&mut ai, &mut w, &mut Vec::new());
        let node = w.economy().node_at(at(6, 6)).ok_or("the node")?;
        assert_eq!(node.owner, Some(P0), "the node is the wizard's");
        assert!(!ai.roles.any(Role::Meld));
        Ok(())
    }

    #[test]
    fn a_spirit_that_stands_still_takes_another_node_and_holds_nothing_against_the_first(
    ) -> TestResult {
        let (mut w, spirit) = world()?;
        node(&mut w, at(5, 6))?;
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        run(&mut ai, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(spirit);
        let first = ai
            .roles
            .target_of(serial, Role::Meld)
            .ok_or("it has a node")?;
        // The turns pass and the spirit does not move.
        for _ in 0..=PATIENCE {
            run(&mut ai, &mut w, &mut Vec::new());
        }
        let now = ai
            .roles
            .target_of(serial, Role::Meld)
            .ok_or("it has a node again")?;
        assert_ne!(now, first, "another node");
        let view = PlayerView::new(&w, P0);
        assert!(
            open_nodes(&ai, &view).contains(&first),
            "the first stays open"
        );
        let said: Vec<String> = ai.take_refusals().into_iter().map(|r| r.reason).collect();
        assert!(said.iter().any(|r| r.starts_with("Stuck")), "{said:?}");
        Ok(())
    }

    #[test]
    fn a_wizard_with_no_open_node_leaves_the_spirit_alone() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let spirit = spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 154, 1)[0];
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(spirit);
        assert!(!ai.roles.has_role(serial));
        Ok(())
    }
}
