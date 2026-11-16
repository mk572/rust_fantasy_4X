//! A settler does not walk to its site alone: a soldier goes with it (`Role::Escort`) and the two
//! walk as one stack, the slower setting the pace. The escort ends when its charge is gone (it
//! founded its city, or died); the soldier is then a free unit in the new city or where it stands.
//!
//! Who is escorted: the settlers that have a site (`Role::Settle` with a target) and leave a city.
//! Where a settler is sent (`walk`), the escort is taken from the city's soldiers above what the city wants (`garrison_wanted`); a
//! city with none to spare sends its settler alone. The registry ties the escort to its charge
//! (`with`); see `docs/ai-unit-registry.md`.

use crate::roles::Role;
use crate::view::{garrison_wanted, is_fighter, unit_strength};
use crate::Ai;
use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

/// Send a settler to its site, with an escort when it leaves a city that has a soldier to spare:
/// the two walk as one stack. Whether the order was accepted. (`expand.rs` calls this where it
/// would send the settler alone.)
pub fn walk(
    ai: &mut Ai,
    game: &mut Game,
    settler: hecs::Entity,
    to: Position,
    events: &mut Vec<Event>,
) -> bool {
    let me = ai.player;
    let turn = game.turn;
    let (charge, at) = {
        let view = PlayerView::new(game, me);
        (view.serial_of(settler), view.unit_position(settler))
    };
    let mut units = vec![settler];
    if let Some(at) = at {
        if ai.roles.escort_of(charge).is_none() {
            if let Some(escort) = spare_soldier(ai, game, at) {
                let serial = PlayerView::new(game, me).serial_of(escort);
                if ai.roles.claim(serial, Role::Escort, turn, None).is_ok() {
                    let party = ai.roles.new_party();
                    ai.roles.group(&[serial], Role::Escort, party, Some(charge));
                }
            }
        }
        // The escort goes along when it stands with its charge.
        let view = PlayerView::new(game, me);
        if let Some(escort) = ai.roles.escort_of(charge).and_then(|e| {
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == e && view.unit_position(u) == Some(at))
        }) {
            units.push(escort);
        }
    }
    ai.send(game, Command::GoTo { units, to }, events).is_ok()
}

/// The soldier a city can spare for an escort: the strongest free one above what the city wants.
fn spare_soldier(ai: &Ai, game: &Game, at: Position) -> Option<hecs::Entity> {
    let view = PlayerView::new(game, ai.player);
    view.city_at(at)?;
    let garrison = view.garrison(at);
    if garrison.len() <= garrison_wanted(&view, at) {
        return None;
    }
    let mut free: Vec<hecs::Entity> = garrison
        .into_iter()
        .filter(|&u| is_fighter(&view, u) && !ai.roles.has_role(view.serial_of(u)))
        .collect();
    free.sort_by(|&a, &b| {
        unit_strength(&view, b)
            .total_cmp(&unit_strength(&view, a))
            .then(view.serial_of(a).cmp(&view.serial_of(b)))
    });
    free.first().copied()
}

/// One turn of the escorts after the settlers have moved: end the ones whose charge is gone, and
/// send an escort that fell behind after its charge.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // A charge that no longer settles (founded its city, died, was taken over by a voyage) frees
    // its escort.
    {
        let view = PlayerView::new(game, me);
        let live: Vec<Serial> = view
            .my_units()
            .into_iter()
            .map(|u| view.serial_of(u))
            .collect();
        let gone: Vec<Serial> = ai
            .roles
            .members(Role::Escort)
            .into_iter()
            .filter(|&e| {
                !ai.roles
                    .with_of(e)
                    .is_some_and(|c| live.contains(&c) && ai.roles.is(c, Role::Settle))
            })
            .collect();
        ai.roles.release_many(&gone, Role::Escort);
    }
    // An escort that fell behind (the stack was split by a fight, or the charge was ordered on
    // by another routine) catches up with its charge.
    let pairs: Vec<(Serial, Serial)> = ai
        .roles
        .members(Role::Escort)
        .into_iter()
        .filter_map(|e| ai.roles.with_of(e).map(|c| (c, e)))
        .collect();
    for (charge, escort) in pairs {
        let behind = {
            let view = PlayerView::new(game, me);
            let find = |s: Serial| {
                view.my_units()
                    .into_iter()
                    .find(|&u| view.serial_of(u) == s)
            };
            let (Some(c), Some(e)) = (find(charge), find(escort)) else {
                continue;
            };
            match (view.unit_position(c), view.unit_position(e)) {
                (Some(there), Some(here)) if there != here && !view.unit_has_order(e) => {
                    Some((e, there))
                }
                _ => None,
            }
        };
        if let Some((unit, to)) = behind {
            ai.order(
                game,
                Command::GoTo {
                    units: vec![unit],
                    to,
                },
                events,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn world() -> (Game, hecs::Entity, Vec<hecs::Entity>) {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, at(7, 6), f4x_def::half_int::HalfInt::from_int(4));
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let settler = spawn(&mut g, P0, city, 111 /* high_men_settlers */, 1)[0];
        let soldiers = spawn(&mut g, P0, city, 104 /* high_men_spearmen */, 3);
        (g, settler, soldiers)
    }

    /// A settler leaving a city with soldiers to spare is given one as its escort, and they walk
    /// out together.
    #[test]
    fn a_settler_leaving_a_city_gets_an_escort_and_they_walk_as_one() -> TestResult {
        let (mut g, settler, _) = world();
        let mut ai = Ai::new(P0);
        g.balance_farmers(P0);
        crate::expand::settlers(&mut ai, &mut g, &mut Vec::new());
        run(&mut ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        let charge = view.serial_of(settler);
        let escort = ai
            .roles
            .escort_of(charge)
            .ok_or("the settler has an escort")?;
        assert_eq!(ai.roles.role_of(escort), Some(Role::Escort));
        assert_eq!(ai.roles.with_of(escort), Some(charge));
        let escort_unit = view
            .my_units()
            .into_iter()
            .find(|&u| view.serial_of(u) == escort)
            .ok_or("the escort lives")?;
        assert!(view.unit_has_order(settler) && view.unit_has_order(escort_unit));
        // The city keeps the soldiers it wants.
        let city = at(COAST_CITY.0, COAST_CITY.1);
        assert!(
            view.garrison(city)
                .into_iter()
                .filter(|&u| is_fighter(&view, u))
                .count()
                >= garrison_wanted(&view, city)
        );
        Ok(())
    }

    /// With no soldier to spare the settler goes alone.
    #[test]
    fn a_city_with_no_soldier_to_spare_sends_its_settler_alone() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, at(7, 6), f4x_def::half_int::HalfInt::from_int(4));
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let settler = spawn(&mut g, P0, city, 111 /* high_men_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        g.balance_farmers(P0);
        crate::expand::settlers(&mut ai, &mut g, &mut Vec::new());
        run(&mut ai, &mut g, &mut Vec::new());
        let charge = PlayerView::new(&g, P0).serial_of(settler);
        assert_eq!(ai.roles.escort_of(charge), None);
        Ok(())
    }

    /// The escort is released when its charge is gone, and an escort apart from its charge
    /// walks to it.
    #[test]
    fn an_escort_is_freed_when_its_charge_is_gone_and_follows_it_when_apart() -> TestResult {
        let (mut g, settler, soldiers) = world();
        let mut ai = Ai::new(P0);
        let (charge, escort) = {
            let view = PlayerView::new(&g, P0);
            (view.serial_of(settler), view.serial_of(soldiers[0]))
        };
        let _ = ai.roles.claim(charge, Role::Settle, 1, Some(at(9, 9)));
        let _ = ai.roles.claim(escort, Role::Escort, 1, None);
        ai.roles.group(&[escort], Role::Escort, 1, Some(charge));
        // The charge steps away: the escort is sent after it.
        g.cheat_move_unit(settler, at(7, 6));
        run(&mut ai, &mut g, &mut Vec::new());
        assert!(PlayerView::new(&g, P0).unit_has_order(soldiers[0]));
        // The charge founds its city (is gone): the escort is a free soldier again.
        ai.roles.release(charge, Role::Settle);
        run(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(ai.roles.role_of(escort), None);
        Ok(())
    }
}
