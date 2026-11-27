//! Quick strikes: the first job of the turn, before any unit has used a move or received an order.
//! A stack that stands next to an enemy it can clearly beat fights it now.
//!
//! The targets are the visible units of wizards we are at war with, of the neutral cities we are
//! at war with, and the roaming bands of monsters (not the guards of a lair: the raids deal with
//! those). A town that is a war target (the plan's goal or next target, the campaign, the march)
//! is left to the army.
//!
//! The strike is outside the long-standing orders and changes almost nothing in the registry: the
//! roles and targets of the units stay as they are. The one change: a unit that walks into an
//! empty city leaves the ship that was to carry it (the pickup plan counts only the units that
//! stand on its boarding tile or sit aboard), so it is taken off the passenger list. A unit that fights has moved from where it was (the
//! winner takes the enemy's square), so the repair of the dispatcher (`dispatch::revalidate`)
//! sends it on to its target again; a city's soldier is sent back to its city at once.

use f4x_core::diplomacy::Treaty;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::MONSTERS;
use f4x_core::world::{Command, Event, World};

use crate::military::{
    campaign_ratio, committed_ratio, engage, neighbours, sortie_ratio, stacks, THREAT_RADIUS,
};
use crate::roles::{MusterKind, Role};
use crate::view::{fighting_strength, party, stack_strength};
use crate::Ai;
use f4x_master::Game;

/// What a stack decides about one square next to it.
enum Sortie {
    /// An enemy city with no one in it, on this square: walk in.
    WalkIn(Position),
    /// Defended: fight if this force is enough.
    Fight(Vec<hecs::Entity>, f32),
}

/// Whether the owner of what stands on `square` is somebody to strike at once.
fn fair_target(
    world: &World,
    view: &PlayerView,
    square: Position,
    defenders: &[hecs::Entity],
    avoid: &[Position],
) -> bool {
    if avoid.contains(&square) {
        return false;
    }
    let owner = defenders
        .first()
        .and_then(|&u| world.units().unit_owner(u))
        .or_else(|| {
            view.city_at(square)
                .and_then(|c| world.economy().city_owner(c))
        });
    match owner {
        Some(MONSTERS) => !view.has_lair(square) && view.city_at(square).is_none(),
        Some(owner) => view.treaty_with(owner) == Treaty::War,
        None => false,
    }
}

/// Whether the garrison of the city at `pos` may go out against the stack on `target`: no other
/// enemy is near the city that the soldiers who stay could not meet.
fn city_safe_without(view: &PlayerView, pos: Position, target: Position) -> bool {
    view.enemies_near(pos, THREAT_RADIUS)
        .into_iter()
        .all(|(square, _)| square == target)
}

/// One turn of strikes.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let avoid: Vec<Position> = [
        ai.war_plan.goal,
        ai.war_plan.next,
        ai.campaign,
        ai.march_target,
    ]
    .into_iter()
    .flatten()
    .collect();
    let own_cities: Vec<Position> = PlayerView::new(game, me)
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .collect();
    // Free soldiers, those gathering or marching for the campaign, the perimeter's and the
    // garrisons: whatever role a unit holds, it may join a strike and keeps the role.
    let fighters = crate::dispatch::soldiers(ai, game, |roles, s| {
        roles.is_free(s)
            || roles.in_muster(s, MusterKind::Campaign)
            || roles.is(s, Role::March)
            || roles.is(s, Role::Perimeter)
            || roles.is(s, Role::Reinforce)
            || roles.is(s, Role::Garrison)
    });
    let edge = campaign_ratio(game, me, game.round());
    let stack_list = stacks(&PlayerView::new(game, me), fighters);
    for (pos, units) in stack_list {
        let at_city = own_cities.contains(&pos);
        // One view for the whole stack: nothing changes until a command is sent, and a command
        // that works ends the stack's turn.
        let (ratio, sorties) = {
            let view = PlayerView::new(game, me);
            if game.game_over().is_some() {
                return;
            }
            let committed = units
                .iter()
                .any(|&u| ai.roles.is(view.serial_of(u), Role::March));
            let ratio = if at_city {
                sortie_ratio()
            } else if committed {
                committed_ratio()
            } else {
                edge
            };
            let sorties: Vec<(Position, Sortie)> = neighbours(&view, pos)
                .into_iter()
                .filter_map(|target| {
                    let defenders = view.hostile_at(target)?;
                    if !fair_target(game, &view, target, &defenders, &avoid) {
                        return None;
                    }
                    if defenders.is_empty() {
                        return Some((target, Sortie::WalkIn(target)));
                    }
                    // A square we see is a square we know the defence of.
                    let theirs = fighting_strength(&view, &defenders)
                        * if view.city_at(target).is_some() {
                            1.3
                        } else {
                            1.0
                        };
                    Some((target, Sortie::Fight(defenders, theirs)))
                })
                .collect();
            (ratio, sorties)
        };
        for (target, sortie) in sorties {
            match sortie {
                Sortie::WalkIn(city) => {
                    // An empty enemy city: walk in, with the units that can still move.
                    let walkers: Vec<hecs::Entity> = {
                        let view = PlayerView::new(game, me);
                        units
                            .iter()
                            .copied()
                            .filter(|&u| {
                                view.unit_moves_left(u) > 0
                                    && !ai.roles.is(view.serial_of(u), Role::Garrison)
                            })
                            .collect()
                    };
                    if walkers.is_empty() {
                        continue;
                    }
                    let walkers_sent = walkers.clone();
                    let to = Command::MoveStack {
                        units: walkers,
                        to: city,
                    };
                    if ai.send(game, to, events).is_ok() {
                        // The units are off the ship, or off its boarding tile: its pickup plan
                        // must not count them any more.
                        let serials: Vec<_> = {
                            let view = PlayerView::new(game, me);
                            walkers_sent.iter().map(|&u| view.serial_of(u)).collect()
                        };
                        for s in serials {
                            ai.roles.unboard(s);
                        }
                        break;
                    }
                }
                Sortie::Fight(defenders, theirs) => {
                    // A city's garrison goes out only when the city is safe without it.
                    let (attackers, lent) = {
                        let view = PlayerView::new(game, me);
                        let lend = at_city && city_safe_without(&view, pos, target);
                        let fit: Vec<hecs::Entity> = units
                            .iter()
                            .copied()
                            .filter(|&u| lend || !ai.roles.is(view.serial_of(u), Role::Garrison))
                            .collect();
                        let band = party(&view, &fit);
                        let lent: Vec<hecs::Entity> = band
                            .iter()
                            .copied()
                            .filter(|&u| ai.roles.is(view.serial_of(u), Role::Garrison))
                            .collect();
                        (band, lent)
                    };
                    let mine = stack_strength(&PlayerView::new(game, me), &attackers);
                    if attackers.is_empty() || mine < theirs * ratio {
                        continue;
                    }
                    engage(ai, game, attackers, defenders, events);
                    // The winners stand on the enemy's square: a city's soldier goes home.
                    let out: Vec<hecs::Entity> = {
                        let view = PlayerView::new(game, me);
                        lent.into_iter()
                            .filter(|&u| view.unit_position(u).is_some_and(|p| p != pos))
                            .collect()
                    };
                    if !out.is_empty() {
                        ai.order(
                            game,
                            Command::GoTo {
                                units: out,
                                to: pos,
                            },
                            events,
                        );
                    }
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, spawn_site, COAST_CITY, P0, P1};
    use f4x_master::Game;

    /// West's six swordsmen stand next to one of East's spearmen, away from every city, and the
    /// two wizards are at war.
    fn field(war: bool) -> (Game, Vec<hecs::Entity>) {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(6, 3), 105 /* high_men_swordsmen */, 6);
        let defenders = spawn(&mut w, P1, at(7, 3), 104 /* high_men_spearmen */, 1);
        w.make_contact(P0, P1);
        if war {
            w.declare_war(P0, P1);
        }
        (w, defenders)
    }

    /// A stack of a square's size attacks with the nine a battle holds and the rest stay on the
    /// square, available.
    #[test]
    fn a_stack_attacks_with_a_battle_party_and_leaves_the_rest_on_its_square() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let (side, tile) = (w.max_battle_side(), w.max_tile());
        let stack = spawn(
            &mut w,
            P0,
            at(6, 3),
            105, /* high_men_swordsmen */
            tile,
        );
        spawn(&mut w, P1, at(7, 3), 104 /* high_men_spearmen */, 1);
        w.make_contact(P0, P1);
        w.declare_war(P0, P1);
        let mut ai = Ai::new(P0);
        ai.leave_defence_to(&[P1]);
        run(&mut ai, &mut w, &mut Vec::new());
        let attacks = ai.take_attacks();
        assert_eq!(attacks.len(), 1);
        assert_eq!(attacks[0].attackers.len(), side);
        assert!(side < stack.len());
        let view = PlayerView::new(&w, P0);
        let left: Vec<_> = stack
            .iter()
            .filter(|u| !attacks[0].attackers.contains(u))
            .collect();
        assert_eq!(left.len(), tile - side);
        assert!(left
            .iter()
            .all(|&&u| view.unit_position(u) == Some(at(6, 3))));
    }

    #[test]
    fn an_attack_on_a_wizard_who_defends_in_person_is_kept_not_fought() {
        let (mut w, defenders) = field(true);
        let mut ai = Ai::new(P0);
        ai.leave_defence_to(&[P1]);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let attacks = ai.take_attacks();
        assert_eq!(attacks.len(), 1, "{events:?}");
        assert_eq!(attacks[0].defenders, defenders);
        assert_eq!(attacks[0].attackers.len(), 6);
        assert!(
            w.units().unit_owner(defenders[0]).is_some(),
            "nothing was fought"
        );
        assert!(ai.take_attacks().is_empty(), "taken once");
    }

    #[test]
    fn without_one_the_same_attack_is_fought_at_once() {
        let (mut w, defenders) = field(true);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        assert!(ai.take_attacks().is_empty());
        assert!(
            w.units().unit_owner(defenders[0]).is_none(),
            "the spearman fell: {events:?}"
        );
    }

    /// Only wizards at war are struck: the same stack beside a wizard it is at peace with leaves
    /// it alone.
    #[test]
    fn a_wizard_not_at_war_is_left_alone() {
        let (mut w, defenders) = field(false);
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(ai.take_attacks().is_empty());
        assert!(w.units().unit_owner(defenders[0]).is_some());
    }

    /// A town that is the campaign's target is the army's, not a quick strike.
    #[test]
    fn a_war_target_is_not_struck_in_passing() {
        let (mut w, defenders) = field(true);
        let mut ai = Ai::new(P0);
        ai.campaign = Some(at(7, 3));
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(w.units().unit_owner(defenders[0]).is_some());
    }

    /// A roaming band of monsters is struck; the guards of a lair are not.
    #[test]
    fn a_roaming_band_is_struck_and_the_guards_of_a_lair_are_not(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(6, 3), 105 /* high_men_swordsmen */, 6);
        let band = spawn(
            &mut w,
            MONSTERS,
            at(7, 3),
            104, /* high_men_spearmen */
            1,
        );
        spawn(&mut w, P0, at(6, 8), 105, 6);
        spawn_site(
            &mut w,
            at(7, 8),
            vec![(f4x_def::rules::UnitId::new(174), 1)],
            Vec::new(),
        )?;
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        assert!(
            w.units().unit_owner(band[0]).is_none(),
            "the band fell: {events:?}"
        );
        assert!(w.economy().lair_at(at(7, 8)).is_some(), "the lair stands");
        Ok(())
    }

    /// A unit booked on a ship that walks into an empty enemy city is off the ship's passenger
    /// list: the pickup plan does not wait for it or walk it back.
    #[test]
    fn a_unit_that_walks_into_an_empty_city_leaves_the_ships_passenger_list() {
        use crate::testworld::ENEMY_CITY;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let beside = at(ENEMY_CITY.0 - 1, ENEMY_CITY.1);
        let soldier = spawn(&mut w, P0, beside, 105 /* high_men_swordsmen */, 1);
        w.make_contact(P0, P1);
        w.declare_war(P0, P1);
        let mut ai = Ai::new(P0);
        let serial = PlayerView::new(&w, P0).serial_of(soldier[0]);
        let _ = ai
            .roles
            .claim(serial, Role::Reinforce, w.round(), Some(beside));
        ai.roles.board_at(serial, serial, beside);
        assert!(ai.roles.boarding_of(serial).is_some());
        run(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(
            w.units().unit_position(soldier[0]),
            Some(at(ENEMY_CITY.0, ENEMY_CITY.1)),
            "it walked into the city"
        );
        assert_eq!(ai.roles.boarding_of(serial), None, "off the passenger list");
    }
}
