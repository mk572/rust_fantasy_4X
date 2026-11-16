//! A spirit goes to the nearest open magic node it can reach and melds with it (`Role::Meld`).
//! A node is open when it is not warped, not already the wizard's, and has no guards standing on
//! it (a guarded one is a site to clear first, `site.rs`). The spirit travels with an escort when
//! the city it leaves has a soldier to spare (`escort::walk`).
//!
//! The spirit that melds is used up. One that cannot get to its node (the way is blocked for a
//! few turns) gives that node up for a while and looks for another.

use crate::roles::Role;
use crate::Ai;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_def::rules::Ability;

/// The ability to meld with a node.
const MELD: u16 = 42;
/// Turns a spirit may stand still on its way before the node is given up.
const PATIENCE: u32 = 4;

/// Whether the unit is a spirit that can meld.
pub fn is_melder(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_has_ability(unit, Ability::new(MELD))
}

/// The squares of nodes a spirit can meld with.
pub(crate) fn open_nodes(ai: &Ai, view: &PlayerView) -> Vec<Position> {
    let me = ai.player;
    let mut nodes: Vec<Position> = view
        .nodes()
        .into_iter()
        .filter(|n| !n.warped && n.owner != Some(me) && !view.has_lair(n.pos))
        .map(|n| n.pos)
        .filter(|p| !ai.bad_sites.contains(p))
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
        // A node somebody else took, or one the spirit cannot get to, is given up.
        let stuck = ai.roles.track(serial, here, ordered);
        if let (Some(t), true) = (target, stuck >= PATIENCE) {
            ai.bad_sites.push(t);
            ai.roles.clear_track(serial);
            ai.roles.release(serial, Role::Meld);
            continue;
        }
        if ordered && target.is_some_and(|t| nodes.contains(&t)) {
            continue;
        }
        let taken = ai.roles.targets(Role::Meld);
        let best = {
            let view = PlayerView::new(game, me);
            nodes
                .iter()
                .copied()
                .filter(|p| p.plane == here.plane)
                .filter(|p| target == Some(*p) || !taken.contains(p))
                .filter(|&p| view.can_reach(&[spirit], p))
                .min_by_key(|&p| (view.distance(here, p).unwrap_or(u32::MAX), p.y, p.x))
        };
        match best {
            Some(node) => {
                if crate::escort::walk(ai, game, spirit, node, events) {
                    let _ = ai.roles.claim(serial, Role::Meld, turn, Some(node));
                } else {
                    ai.bad_sites.push(node);
                }
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

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A node of power 5 on `pos`, with no guards.
    fn node(g: &mut Game, pos: Position) -> Result<(), Box<dyn std::error::Error>> {
        let feature = g
            .content()
            .features
            .id_numbered(20 /* sorcery_node */)
            .ok_or("a node feature")?;
        g.spawn_feature(Placement {
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, at(7, 6), f4x_def::half_int::HalfInt::from_int(5));
        node(&mut g, at(6, 6))?;
        let spirit = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            154, /* magic_spirit */
            1,
        )[0];
        g.balance_farmers(P0);
        Ok((g, spirit))
    }

    #[test]
    fn a_spirit_walks_to_the_open_node_and_melds_with_it() -> TestResult {
        let (mut g, spirit) = world()?;
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut g, &mut Vec::new());
        let serial = PlayerView::new(&g, P0).serial_of(spirit);
        assert_eq!(ai.roles.role_of(serial), Some(Role::Meld));
        assert_eq!(ai.roles.target_of(serial, Role::Meld), Some(at(6, 6)));
        assert!(PlayerView::new(&g, P0).unit_has_order(spirit));
        // Once on the node it melds and is used up (the spirit is put there: the test wizard has
        // no mana to keep it through a turn).
        g.cheat_move_unit(spirit, at(6, 6));
        run(&mut ai, &mut g, &mut Vec::new());
        let node = g.economy().node_at(at(6, 6)).ok_or("the node")?;
        assert_eq!(node.owner, Some(P0), "the node is the wizard's");
        assert!(!ai.roles.any(Role::Meld));
        Ok(())
    }

    #[test]
    fn a_wizard_with_no_open_node_leaves_the_spirit_alone() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let spirit = spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), 154, 1)[0];
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut g, &mut Vec::new());
        let serial = PlayerView::new(&g, P0).serial_of(spirit);
        assert!(!ai.roles.has_role(serial));
        Ok(())
    }
}
