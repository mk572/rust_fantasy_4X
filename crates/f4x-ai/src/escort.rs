//! A settler does not walk to its site alone: a soldier goes with it (`Role::Escort`) and the two
//! walk as one stack, the slower setting the pace. The escort ends when its charge is gone (it
//! founded its city, or died); the soldier is then a free unit in the new city or where it stands.
//!
//! Who is escorted: a healer or a weak hero standing alone outside a city (`stragglers`), an engineer in the field, a spirit bound
//! for a node, and the settlers that have a site (`Role::Settle` with a target) and leave a city.
//! Where a settler is sent (`walk`), the escort is taken from the city's soldiers above what the city wants (`garrison_wanted`); a
//! city with none to spare sends its settler alone. The registry ties the escort to its charge
//! (`with`); see `docs/ai-unit-registry.md`.

use crate::roles::Role;
use crate::view::{is_fighter, unit_strength};
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
    // A charge that can cross what its escort cannot (a swimming settler, a walking soldier) goes
    // alone: a stack moves like its slowest member, and one that has no way is no escort.
    if units.len() > 1 && !game.reaches_any(&units, &[to]) && game.reaches_any(&units[..1], &[to]) {
        if let Some(escort) = ai.roles.escort_of(charge) {
            ai.roles.release_many(&[escort], Role::Escort);
        }
        units.truncate(1);
    }
    // With no escort at its side and an enemy near, the charge waits where it stands (the order
    // is asked again next turn, when a soldier may be spare).
    if units.len() == 1 && at.is_some_and(|at| enemy_near(&PlayerView::new(game, me), at)) {
        return true;
    }
    ai.send(game, Command::GoTo { units, to }, events).is_ok()
}

/// Squares within which an enemy unit is a danger to a charge walking alone.
const ENEMY_NEAR: i32 = 4;

/// Whether an enemy unit the wizard sees stands within [`ENEMY_NEAR`] squares of `at`.
fn enemy_near(view: &PlayerView, at: Position) -> bool {
    (-ENEMY_NEAR..=ENEMY_NEAR).any(|dy| {
        (-ENEMY_NEAR..=ENEMY_NEAR).any(|dx| {
            view.shift(at, dx, dy)
                .and_then(|p| view.hostile_at(p))
                .is_some_and(|units| !units.is_empty())
        })
    })
}

/// Whether the unit holds a role whose bearer is escorted: a settler on its way, an engineer on
/// the road, a spirit bound for a node.
fn guarded(ai: &Ai, view: &PlayerView, charge: Serial) -> bool {
    if [Role::Settle, Role::Road, Role::Meld]
        .into_iter()
        .any(|r| ai.roles.is(charge, r))
    {
        return true;
    }
    // A healer or a weak hero alone in the field (`stragglers`) keeps its guard on the way home.
    matches!(
        ai.roles.role_of(charge),
        Some(Role::MoveTo | Role::AwaitingOrders)
    ) && view
        .my_units()
        .into_iter()
        .find(|&u| view.serial_of(u) == charge)
        .is_some_and(|u| {
            needs_escort(view, u)
                && view
                    .unit_position(u)
                    .is_some_and(|p| view.city_at(p).is_none())
        })
}

/// Ability of the Healer.
const HEALER: f4x_def::rules::Ability = f4x_def::rules::Ability::new(27);

/// A hero this much weaker than the average fighter needs a guard in the field.
const WEAK_HERO: f32 = 0.5;

/// Whether the unit cannot defend itself and is worth a guard when it stands alone in the field:
/// a healer, or a hero much weaker than the army.
fn needs_escort(view: &PlayerView, unit: hecs::Entity) -> bool {
    if view.unit_has_ability(unit, HEALER) {
        return true;
    }
    if !view
        .unit_def(unit)
        .is_some_and(|d| d.kind == f4x_def::rules::UnitKind::Hero)
    {
        return false;
    }
    let fighters: Vec<f32> = view
        .my_units()
        .into_iter()
        .filter(|&u| {
            is_fighter(view, u)
                && view
                    .unit_def(u)
                    .is_some_and(|d| d.kind != f4x_def::rules::UnitKind::Hero)
        })
        .map(|u| unit_strength(view, u))
        .collect();
    if fighters.is_empty() {
        return false;
    }
    let average = fighters.iter().sum::<f32>() / fighters.len() as f32;
    unit_strength(view, unit) < average * WEAK_HERO
}

/// The healers and weak heroes that stand alone outside a city.
fn stragglers(ai: &Ai, game: &Game) -> Vec<hecs::Entity> {
    let view = PlayerView::new(game, ai.player);
    let mut out: Vec<hecs::Entity> = view
        .my_units()
        .into_iter()
        .filter(|&u| needs_escort(&view, u))
        .filter(|&u| {
            let s = view.serial_of(u);
            // A unit with a task of its own travels with the party of that task.
            matches!(
                ai.roles.role_of(s),
                Some(Role::MoveTo | Role::AwaitingOrders)
            )
        })
        .filter(|&u| {
            view.unit_position(u).is_some_and(|p| {
                view.city_at(p).is_none()
                    && !view
                        .garrison(p)
                        .into_iter()
                        .any(|o| o != u && is_fighter(&view, o) && !needs_escort(&view, o))
            })
        })
        .collect();
    view.sort_by_serial(&mut out);
    out
}

/// Make sure a unit that works away from any city (an engineer on a road toward the enemy) has an
/// escort standing with it: one is taken from the nearest own city that can spare a soldier and
/// sent to it. Whether the escort is with its charge now.
pub fn guard(ai: &mut Ai, game: &mut Game, charge: hecs::Entity, events: &mut Vec<Event>) -> bool {
    let me = ai.player;
    let turn = game.turn;
    let (serial, at) = {
        let view = PlayerView::new(game, me);
        let Some(at) = view.unit_position(charge) else {
            return false;
        };
        (view.serial_of(charge), at)
    };
    if ai.roles.escort_of(serial).is_none() {
        let spare = {
            let view = PlayerView::new(game, me);
            let mut cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
            cities.sort_by_key(|&p| (view.distance(p, at).unwrap_or(u32::MAX), p.y, p.x));
            cities.into_iter().find_map(|city| {
                let soldier = spare_soldier(ai, game, city)?;
                view.can_reach(&[soldier], at).then_some(soldier)
            })
        };
        if let Some(soldier) = spare {
            let escort = PlayerView::new(game, me).serial_of(soldier);
            if ai.roles.claim(escort, Role::Escort, turn, None).is_ok() {
                let party = ai.roles.new_party();
                ai.roles.group(&[escort], Role::Escort, party, Some(serial));
            }
        }
    }
    let Some(escort) = ai.roles.escort_of(serial) else {
        return false;
    };
    let (unit, there) = {
        let view = PlayerView::new(game, me);
        let unit = view
            .my_units()
            .into_iter()
            .find(|&u| view.serial_of(u) == escort);
        (unit, unit.and_then(|u| view.unit_position(u)))
    };
    match (unit, there) {
        (Some(_), Some(p)) if p == at => true,
        (Some(u), _) => {
            // A square with no room for it is not asked: the escort waits till one leaves.
            if !PlayerView::new(game, me).unit_has_order(u) && Ai::room_at(game, at, 1) {
                ai.order(
                    game,
                    Command::GoTo {
                        units: vec![u],
                        to: at,
                    },
                    events,
                );
            }
            false
        }
        _ => false,
    }
}

/// The soldier a city can spare for an escort: the strongest free one above what the city wants.
fn spare_soldier(ai: &Ai, game: &Game, at: Position) -> Option<hecs::Entity> {
    let view = PlayerView::new(game, ai.player);
    view.city_at(at)?;
    let mut free: Vec<hecs::Entity> = view
        .garrison(at)
        .into_iter()
        .filter(|&u| is_fighter(&view, u) && ai.roles.is_free(view.serial_of(u)))
        .collect();
    free.sort_by(|&a, &b| {
        unit_strength(&view, b)
            .total_cmp(&unit_strength(&view, a))
            .then(view.serial_of(a).cmp(&view.serial_of(b)))
    });
    free.first().copied()
}

/// One turn of the escorts after the settlers have moved: end the ones whose charge is gone, give
/// a charge an escort, stop a charge with no escort when an enemy is near. (An escort that fell
/// behind is sent after its charge by `dispatch::repair_escorts`.)
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
                    .is_some_and(|c| live.contains(&c) && guarded(ai, &view, c))
            })
            .collect();
        ai.roles.release_many(&gone, Role::Escort);
    }
    // A settler or a spirit walking with no escort stops when an enemy comes near.
    {
        let view = PlayerView::new(game, me);
        let stop: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| {
                let s = view.serial_of(u);
                (ai.roles.is(s, Role::Settle) || ai.roles.is(s, Role::Meld))
                    && ai.roles.escort_of(s).is_none()
                    && view.unit_has_order(u)
                    && view.unit_position(u).is_some_and(|p| enemy_near(&view, p))
            })
            .collect();
        if !stop.is_empty() {
            ai.order(game, Command::ClearOrders { units: stop }, events);
        }
    }
    // A healer or a weak hero alone in the field is given a soldier.
    for unit in stragglers(ai, game) {
        guard(ai, game, unit, events);
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
                >= crate::view::garrison_wanted(&view, city)
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

    #[test]
    fn a_swimming_settler_does_not_take_a_walking_escort_over_the_sea() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        spawn(&mut g, P0, city, 113 /* high_men_paladins */, 2);
        let settler = spawn(&mut g, P0, city, 125 /* lizardman_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        let charge = PlayerView::new(&g, P0).serial_of(settler);
        // A square of the east island that the swimmer can walk to and the soldiers cannot.
        let walker = g.units().units_at(city)[0];
        let swims = g.reachable_squares(&[settler]);
        let walks = g.reachable_squares(&[walker]);
        let far = swims
            .iter()
            .copied()
            .filter(|p| p.x >= 28 && !walks.contains(p))
            .min_by_key(|p| (p.y, p.x))
            .ok_or("the swimmer reaches the east island")?;
        assert!(walk(&mut ai, &mut g, settler, far, &mut Vec::new()));
        assert_eq!(ai.roles.escort_of(charge), None, "no walking escort");
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
        crate::dispatch::repair_escorts(&mut ai, &mut g, &mut Vec::new());
        assert!(PlayerView::new(&g, P0).unit_has_order(soldiers[0]));
        // The charge founds its city (is gone): the escort is a free soldier again.
        ai.roles.release(charge, Role::Settle);
        run(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(ai.roles.role_of(escort), Some(Role::AwaitingOrders));
        Ok(())
    }

    /// An engineer at work away from the cities is given the soldier a city can spare, which walks
    /// to it.
    #[test]
    fn an_engineer_in_the_field_is_given_an_escort_from_the_nearest_city() -> TestResult {
        let (mut g, _, soldiers) = world();
        let engineer = spawn(&mut g, P0, at(7, 6), 110 /* high_men_engineers */, 1)[0];
        let mut ai = Ai::new(P0);
        let with_it = guard(&mut ai, &mut g, engineer, &mut Vec::new());
        assert!(!with_it, "the escort is still on its way");
        let view = PlayerView::new(&g, P0);
        let charge = view.serial_of(engineer);
        let escort = ai.roles.escort_of(charge).ok_or("an escort was chosen")?;
        assert!(soldiers.iter().any(|&s| view.serial_of(s) == escort));
        // Once it stands with the engineer the engineer may work.
        let escort_unit = soldiers
            .iter()
            .copied()
            .find(|&s| view.serial_of(s) == escort)
            .ok_or("the escort lives")?;
        g.cheat_move_unit(escort_unit, at(7, 6));
        assert!(guard(&mut ai, &mut g, engineer, &mut Vec::new()));
        Ok(())
    }

    /// A healer on its own in the field is given a soldier from the nearest city; one in a city
    /// or in a stack of fighters is left alone.
    #[test]
    fn a_healer_alone_in_the_field_is_given_an_escort() -> TestResult {
        let (mut g, _, soldiers) = world();
        let healer = spawn(&mut g, P0, at(7, 6), 7 /* healer */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &g);
        run(&mut ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        let charge = view.serial_of(healer);
        let escort = ai.roles.escort_of(charge).ok_or("an escort was chosen")?;
        assert!(soldiers.iter().any(|&s| view.serial_of(s) == escort));
        // In a city it needs none.
        let mut g = world().0;
        let in_city = spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), 7, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &g);
        run(&mut ai, &mut g, &mut Vec::new());
        let charge = PlayerView::new(&g, P0).serial_of(in_city);
        assert_eq!(ai.roles.escort_of(charge), None);
        Ok(())
    }

    /// With no soldier to spare and an enemy near, the settler waits instead of walking alone;
    /// without the enemy it goes.
    #[test]
    fn a_charge_with_no_escort_waits_while_an_enemy_is_near() -> TestResult {
        use crate::testworld::P1;
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, at(7, 6), f4x_def::half_int::HalfInt::from_int(6));
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let settler = spawn(&mut g, P0, city, 111 /* high_men_settlers */, 1)[0];
        g.make_contact(P0, P1);
        g.declare_war(P0, P1);
        spawn(&mut g, P1, at(COAST_CITY.0 - 2, COAST_CITY.1), 104, 1);
        g.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        crate::expand::settlers(&mut ai, &mut g, &mut Vec::new());
        let serial = PlayerView::new(&g, P0).serial_of(settler);
        assert!(ai.roles.is(serial, Role::Settle));
        assert!(
            !PlayerView::new(&g, P0).unit_has_order(settler),
            "the settler waits"
        );
        assert!(
            ai.roles.target_of(serial, Role::Settle).is_some(),
            "keeps its site"
        );
        Ok(())
    }
}
