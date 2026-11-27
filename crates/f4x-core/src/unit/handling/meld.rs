//! A spirit goes to the nearest open magic node it can reach and melds with it (`Role::Meld`).
//! A node is open when it is not warped, not already the wizard's, and has no guards standing on
//! it (a guarded one is a site to clear first, `site.rs`). The spirit travels with an escort when
//! the city it leaves has a soldier to spare (`escort::walk`).
//!
//! The spirit that melds is used up. One that stands still on its way for a few turns drops the
//! order, is logged with the cause, and takes another node; nothing is held against the node.

use super::roles::Role;
use super::Handler;
use super::Host;
use crate::map::Position;
use crate::player::view::PlayerView;
use crate::world::{Command, Event};
use f4x_def::rules::Ability;

/// Turns a spirit may stand still on its way before the node is given up.
const PATIENCE: u32 = 4;

/// Whether the unit is a spirit that can meld.
pub fn is_melder(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_has_ability(unit, Ability::new(42 /* meld */))
}

/// The squares of nodes a spirit can meld with.
pub(crate) fn open_nodes(handler: &Handler, view: &PlayerView) -> Vec<Position> {
    let me = handler.player;
    let mut nodes: Vec<Position> = view
        .nodes()
        .into_iter()
        .filter(|n| !n.warped && n.owner != Some(me) && !view.has_lair(n.pos))
        .map(|n| n.pos)
        .collect();
    nodes.sort_by_key(|p| (p.plane as u8, p.y, p.x));
    nodes
}

/// One turn of the spirits.
pub fn run(handler: &mut Handler, game: &mut dyn Host, events: &mut Vec<Event>) {
    let me = handler.player;
    let turn = game.round();
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
                open_nodes(handler, &view),
            )
        };
        let Some(here) = here else { continue };
        // With no node to go to the spirit is left to the others (a scout, say).
        if nodes.is_empty() && !handler.roles.is(serial, Role::Meld) {
            continue;
        }
        if handler.roles.claim(serial, Role::Meld, turn, None).is_err() {
            continue;
        }
        // On an open node: meld.
        if nodes.contains(&here) {
            let _ = handler.send(
                game,
                Command::Meld {
                    units: vec![spirit],
                },
                events,
            );
            handler.roles.release(serial, Role::Meld);
            continue;
        }
        let target = handler.roles.target_of(serial, Role::Meld);
        // A spirit that has stood still on its order for too long is not getting there: the order
        // is dropped, the refused-order log gets the cause, and another node is chosen this turn.
        // Nothing is remembered against the node (a node that is taken or guarded is not open).
        let stuck = handler.roles.track(serial, here, ordered);
        let mut skip = None;
        if let (Some(t), true) = (target, stuck >= PATIENCE) {
            handler.note_failure(
                game,
                format!("spirit at {here:?} stood still on its way to {t:?}"),
                format!(
                    "Stuck{}{}",
                    super::DETAIL_SEPARATOR,
                    game.why_no_path(&[spirit], t)
                ),
            );
            handler.order(
                game,
                Command::ClearOrders {
                    units: vec![spirit],
                },
                events,
            );
            handler.roles.clear_track(serial);
            handler.roles.release(serial, Role::Meld);
            skip = Some(t);
        } else if ordered && target.is_some_and(|t| nodes.contains(&t)) {
            continue;
        }
        let taken = handler.roles.targets(Role::Meld);
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
            .find(|&node| super::escort::walk(handler, game, spirit, node, events));
        match chosen {
            Some(node) => {
                let _ = handler.roles.claim(serial, Role::Meld, turn, Some(node));
            }
            None => {
                handler.roles.release(serial, Role::Meld);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testworld::Game;
    use super::super::testworld::{at, game, spawn, COAST_CITY, P0};
    use super::*;
    use crate::feature::entity::Placement;

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
            node: Some(crate::feature::node::Node {
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
        let mut handler = super::super::testworld::handler(P0);
        run(&mut handler, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(spirit);
        assert_eq!(handler.roles.role_of(serial), Some(Role::Meld));
        assert_eq!(handler.roles.target_of(serial, Role::Meld), Some(at(6, 6)));
        assert!(PlayerView::new(&w, P0).unit_has_order(spirit));
        // Once on the node it melds and is used up (the spirit is put there: the test wizard has
        // no mana to keep it through a turn).
        w.cheat_move_unit(spirit, at(6, 6));
        run(&mut handler, &mut w, &mut Vec::new());
        let node = w.economy().node_at(at(6, 6)).ok_or("the node")?;
        assert_eq!(node.owner, Some(P0), "the node is the wizard's");
        assert!(!handler.roles.any(Role::Meld));
        Ok(())
    }

    #[test]
    fn a_spirit_that_stands_still_takes_another_node_and_holds_nothing_against_the_first(
    ) -> TestResult {
        let (mut w, spirit) = world()?;
        node(&mut w, at(5, 6))?;
        let mut handler = super::super::testworld::handler(P0);
        handler.record_refusals(true);
        run(&mut handler, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(spirit);
        let first = handler
            .roles
            .target_of(serial, Role::Meld)
            .ok_or("it has a node")?;
        // The turns pass and the spirit does not move.
        for _ in 0..=PATIENCE {
            run(&mut handler, &mut w, &mut Vec::new());
        }
        let now = handler
            .roles
            .target_of(serial, Role::Meld)
            .ok_or("it has a node again")?;
        assert_ne!(now, first, "another node");
        let view = PlayerView::new(&w, P0);
        assert!(
            open_nodes(&handler, &view).contains(&first),
            "the first stays open"
        );
        let said: Vec<String> = handler
            .take_refusals()
            .into_iter()
            .map(|r| r.reason)
            .collect();
        assert!(said.iter().any(|r| r.starts_with("Stuck")), "{said:?}");
        Ok(())
    }

    #[test]
    fn a_wizard_with_no_open_node_leaves_the_spirit_alone() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let spirit = spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 154, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        run(&mut handler, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(spirit);
        assert!(!handler.roles.has_role(serial));
        Ok(())
    }
}
