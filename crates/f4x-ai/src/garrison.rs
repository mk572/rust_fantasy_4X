//! How many units stand in a city: what its defence wants (`wanted_strength`, `post`), the
//! late-game dismissal of cheap units (`thin_out`), and the slot the wizard's summoning circle
//! keeps free for the creatures a summoning spell brings (`make_room`). Sending soldiers to a
//! city that holds too little is `reinforce.rs`.
//!
//! Tests: below, on the shipped content, with a second city, a full circle and a summon.

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::view::{garrison_wanted, is_fighter, unit_strength, ReachCache};
use crate::Ai;

/// The square the wizard's summoned creatures appear on.
fn circle(view: &PlayerView) -> Option<Position> {
    view.wizard().and_then(|w| w.summon_point())
}

/// How many units the AI lets stand on `at`: what a square holds (the game's `max_tile`), less
/// one at the summoning circle.
pub fn city_cap(view: &PlayerView, at: Position) -> usize {
    let limit = view.max_tile();
    let cap = limit;
    if circle(view) == Some(at) {
        cap.min(limit.saturating_sub(1))
    } else {
        cap
    }
}

/// Moves from which a unit is fast: it serves the scouting and the war better than the defence
/// of a city (cavalry, fliers).
const FAST: i32 = 2;

/// The strength an average soldier of the wizard has: the unit the defence wanted of a city is
/// counted in.
fn typical_strength(view: &PlayerView) -> f32 {
    let each: Vec<f32> = view
        .my_units()
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .map(|u| unit_strength(view, u))
        .collect();
    if each.is_empty() {
        return 1.0;
    }
    each.iter().sum::<f32>() / each.len() as f32
}

/// What share of the peace-time defence a wizard in `phase` keeps (task 645): while it builds, a
/// young city is held by little and the gold goes to settlers and buildings. A tuning number
/// (`AIK_BUILD_GARRISON`), to be measured by the soak.
fn phase_factor(phase: crate::phase::Phase) -> f32 {
    match phase {
        crate::phase::Phase::Build => crate::knob("BUILD_GARRISON", 0.6),
        crate::phase::Phase::Secure | crate::phase::Phase::Win => 1.0,
    }
}

/// The defence strength a city wants: the soldiers `garrison_wanted` asks for, each of the
/// strength of an average soldier of the wizard (so a few strong units or many weak ones meet it
/// alike). A city with no enemy near it (the base count) wants the share its phase gives; a
/// city near the border keeps what the border asks.
pub fn wanted_strength(view: &PlayerView, at: Position) -> f32 {
    wanted_share(view, at) * typical_strength(view)
}

/// How many average soldiers a city wants: `garrison_wanted`, less the share a building wizard
/// does not keep in a city with no enemy near it. The autobuilder counts the same.
pub fn wanted_share(view: &PlayerView, at: Position) -> f32 {
    let wanted = garrison_wanted(view, at);
    let base = if view.fortress() == Some(at) { 2 } else { 1 };
    let factor = if wanted == base {
        phase_factor(crate::phase::phase(view))
    } else {
        1.0
    };
    wanted as f32 * factor
}

/// The soldiers of `at` that are its garrison (`Role::Garrison`): the slow, defensive units
/// first, a hero or a fast unit only when nothing else makes the strength wanted. The others of
/// the city are free for the dispatcher's jobs (task 645).
pub fn post(view: &PlayerView, at: Position, soldiers: &[hecs::Entity]) -> Vec<hecs::Entity> {
    let wanted = wanted_strength(view, at);
    let hero = |u: hecs::Entity| {
        view.unit_def(u)
            .is_some_and(|d| d.kind == f4x_def::rules::UnitKind::Hero)
    };
    let mut order: Vec<hecs::Entity> = soldiers.to_vec();
    order.sort_by(|&a, &b| {
        let key = |u: hecs::Entity| {
            (
                hero(u),
                view.unit_stat(u, f4x_def::rules::Stat::Moves) >= FAST,
            )
        };
        key(a)
            .cmp(&key(b))
            .then(unit_strength(view, b).total_cmp(&unit_strength(view, a)))
            .then(view.serial_of(a).cmp(&view.serial_of(b)))
    });
    let mut kept = Vec::new();
    let mut strength = 0.0;
    for u in order {
        if strength >= wanted {
            break;
        }
        strength += unit_strength(view, u);
        kept.push(u);
    }
    kept
}

/// Units that may be sent away: fighters with moves left and no task.
fn movable(ai: &Ai, view: &PlayerView, units: &[hecs::Entity]) -> Vec<hecs::Entity> {
    let busy = |serial| {
        ai.roles.is_scouting(serial)
            || ai.roles.is(serial, crate::roles::Role::Raid)
            || ai.roles.is(serial, crate::roles::Role::March)
            || ai.roles.is(serial, crate::roles::Role::SeaParty)
            || ai.roles.is(serial, crate::roles::Role::PlanarScout)
            || ai.roles.has_role(serial)
    };
    units
        .iter()
        .copied()
        .filter(|&u| is_fighter(view, u))
        .filter(|&u| view.unit_moves_left(u) > 0 && !view.unit_has_order(u))
        .filter(|&u| !busy(view.serial_of(u)))
        .collect()
}

/// Fighters a city keeps in the late game beyond [`garrison_wanted`]: a spare, so that a raid or a
/// wandering monster does not find it empty.
const LATE_SPARE: usize = 1;

/// Late in the game the cheap units (spearmen, swordsmen) are let go: they cost gold and food, take
/// room in the city's stack and fight no tower or city. A city keeps at least what it wants and the
/// stronger units; the weakest low-tier ones beyond that are dismissed. Not in a threatened city, and
/// not a unit with a task.
pub fn thin_out(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let dismissed: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        if crate::phase::phase(&view) != crate::phase::Phase::Win {
            return;
        }
        let mut out = Vec::new();
        let mine = view.my_units();
        for (_, pos) in view.my_cities() {
            if ai.threatened.contains(&pos) {
                continue;
            }
            let mut fighters: Vec<hecs::Entity> = view
                .garrison(pos)
                .into_iter()
                .filter(|&u| is_fighter(&view, u) && mine.contains(&u))
                .collect();
            let keep = garrison_wanted(&view, pos) + LATE_SPARE;
            if fighters.len() <= keep {
                continue;
            }
            fighters.sort_by(|&a, &b| {
                unit_strength(&view, a)
                    .total_cmp(&unit_strength(&view, b))
                    .then(view.serial_of(a).cmp(&view.serial_of(b)))
            });
            let spare = fighters.len() - keep;
            out.extend(
                fighters
                    .into_iter()
                    .filter(|&u| {
                        let serial = view.serial_of(u);
                        crate::forge::low_tier(&view, u)
                            && !ai.roles.is_scouting(serial)
                            && !ai.roles.is(serial, crate::roles::Role::Raid)
                            && !ai.roles.is(serial, crate::roles::Role::March)
                            && !ai.roles.is(serial, crate::roles::Role::SeaParty)
                    })
                    .take(spare),
            );
        }
        out
    };
    for unit in dismissed {
        ai.order(game, Command::DismissUnit { unit }, events);
    }
}

/// Could a summoning be given room now: the circle's square is full, but a fighter on it can
/// still walk out? (When it has room already this is of no matter.)
pub fn can_make_room(view: &PlayerView) -> bool {
    let Some(at) = circle(view) else {
        return false;
    };
    let units = view.garrison(at);
    units.len() < view.max_tile()
        || units
            .iter()
            .any(|&u| is_fighter(view, u) && view.unit_moves_left(u) > 0)
}

/// Make sure the summoning circle's square has a free slot for the creatures: when it is full,
/// the weakest fighter that can walk leaves for the nearest other city with room. Returns
/// whether there is room now.
pub fn make_room(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) -> bool {
    let me = ai.player;
    let (at, plan) = {
        let view = PlayerView::new(game, me);
        let Some(at) = circle(&view) else {
            return true;
        };
        let units = view.garrison(at);
        if units.len() < view.max_tile() {
            return true;
        }
        let mut leaving = movable(ai, &view, &units);
        leaving.sort_by(|&a, &b| {
            unit_strength(&view, a)
                .total_cmp(&unit_strength(&view, b))
                .then(view.serial_of(a).cmp(&view.serial_of(b)))
        });
        let mut homes: Vec<(u32, Position)> = view
            .my_cities()
            .into_iter()
            .map(|(_, p)| p)
            .filter(|&p| p != at && view.garrison(p).len() < city_cap(&view, p))
            .filter_map(|p| Some((view.distance(at, p)?, p)))
            .collect();
        homes.sort_by_key(|&(d, p)| (d, p.y, p.x));
        let mut reached = ReachCache::default();
        let plan = leaving.into_iter().find_map(|u| {
            homes
                .iter()
                .find(|&&(_, p)| reached.can_reach(&view, &[u], p))
                .map(|&(_, p)| (u, p))
        });
        (at, plan)
    };
    let Some((unit, to)) = plan else {
        return false;
    };
    // A unit that cannot leave has not made room.
    let left = ai
        .send(
            game,
            Command::GoTo {
                units: vec![unit],
                to,
            },
            events,
        )
        .is_ok();
    let view = PlayerView::new(game, me);
    left && view.garrison(at).len() < view.max_tile()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, INLAND_CITY, P0};
    use f4x_core::wizard::fortress::{Fortress, SummonAt};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Player 0 with a city on the coast and a second one inland, and its circle on the coast.
    fn two_cities() -> Result<Game, Box<dyn std::error::Error>> {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let events = g.apply(Command::FoundCity {
            owner: P0,
            pos: at(INLAND_CITY.0, INLAND_CITY.1),
            name: "Inland".into(),
            race: g.content().rules.races.id_numbered(8),
            population: 8,
        });
        assert!(!events.iter().any(|e| matches!(e, Event::Rejected(_))));
        let coast = at(COAST_CITY.0, COAST_CITY.1);
        let mut w = g.wizard_mut(P0).ok_or("player 0 is a wizard")?;
        w.put(Some(Fortress(coast)));
        w.put(Some(SummonAt(coast)));
        Ok(g)
    }

    fn held(g: &Game, pos: Position) -> usize {
        PlayerView::new(g, P0).garrison(pos).len()
    }

    #[test]
    fn late_in_the_game_the_cheap_units_of_a_full_city_are_let_go_and_the_strong_stay() -> TestResult
    {
        let mut g = two_cities()?;
        let coast = at(COAST_CITY.0, COAST_CITY.1);
        spawn(&mut g, P0, coast, 104 /* high_men_spearmen */, 6);
        let paladins = spawn(&mut g, P0, coast, 113 /* high_men_paladins */, 2);
        g.turn = 400;
        let mut ai = Ai::new(P0);
        thin_out(&mut ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        let left = view.garrison(coast);
        // Wanted is one at peace and a spare: two stay, and they are the better of the cheap.
        assert!(left.len() < 8, "{} units left", left.len());
        assert!(paladins.iter().all(|p| left.contains(p)));
        assert!(left.len() >= garrison_wanted(&view, coast) + LATE_SPARE);
        Ok(())
    }

    #[test]
    fn the_cheap_units_stay_early_in_the_game_and_in_a_threatened_city() -> TestResult {
        let mut g = two_cities()?;
        let coast = at(COAST_CITY.0, COAST_CITY.1);
        spawn(&mut g, P0, coast, 104 /* high_men_spearmen */, 6);
        g.turn = 20;
        let mut ai = Ai::new(P0);
        thin_out(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(held(&g, coast), 6);
        g.turn = 400;
        ai.threatened = vec![coast];
        thin_out(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(held(&g, coast), 6);
        Ok(())
    }

    /// A soldier waiting on a shore for a ship has its role: the garrison does not send it off.
    #[test]
    fn a_soldier_waiting_for_a_ship_is_not_movable() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let units = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            104, /* high_men_spearmen */
            2,
        );
        let mut ai = Ai::new(P0);
        let view = PlayerView::new(&g, P0);
        assert_eq!(movable(&ai, &view, &units).len(), 2);
        let _ = ai.roles.claim(
            view.serial_of(units[0]),
            crate::roles::Role::Reinforce,
            1,
            Some(at(COAST_CITY.0, COAST_CITY.1)),
        );
        assert_eq!(movable(&ai, &view, &units), vec![units[1]]);
        Ok(())
    }

    #[test]
    fn a_full_circle_gets_a_slot_by_sending_a_unit_to_another_city() -> TestResult {
        let mut g = two_cities()?;
        let coast = at(COAST_CITY.0, COAST_CITY.1);
        let limit = g.max_tile();
        spawn(&mut g, P0, coast, 104 /* high_men_spearmen */, limit);
        let ai = Ai::new(P0);
        assert_eq!(held(&g, coast), limit);
        assert!(make_room(&ai, &mut g, &mut Vec::new()));
        assert_eq!(held(&g, coast), limit - 1);
        // With room already, nothing moves.
        assert!(make_room(&ai, &mut g, &mut Vec::new()));
        assert_eq!(held(&g, coast), limit - 1);
        Ok(())
    }

    /// A wizard that is still building holds its cities with less than one that has settled; a
    /// city near an enemy asks the same in both.
    #[test]
    fn a_building_wizard_wants_less_in_a_quiet_city_than_a_settled_one() -> TestResult {
        let mut g = two_cities()?;
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(&mut g, P0, inland, 104 /* high_men_spearmen */, 1);
        g.turn = 1;
        let building = {
            let view = PlayerView::new(&g, P0);
            assert_eq!(crate::phase::phase(&view), crate::phase::Phase::Build);
            wanted_strength(&view, inland)
        };
        g.turn = 250;
        let settled = {
            let view = PlayerView::new(&g, P0);
            assert_eq!(crate::phase::phase(&view), crate::phase::Phase::Secure);
            wanted_strength(&view, inland)
        };
        assert!(building < settled, "{building} against {settled}");
        Ok(())
    }
}
