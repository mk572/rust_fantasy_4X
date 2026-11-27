//! The raid job (task 646, job 5 of the dispatcher): the parties that clear the sites. A party out
//! already strikes the site next to it, walks to the next or goes home; new parties form from the
//! soldiers the war left over, and soldiers that cannot take a tower alone muster for it with the
//! wizard's spells (`spellhelper::spell_power`). What a site is worth and where the sites are is
//! `site.rs`.

use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::serial::Serial;
use f4x_core::world::{Command, Event, World};

use crate::assault::{assault, march};
use crate::military::engage;
use crate::site::{
    city_defence, edge, expected_vs, lost_before, neutral_city, sites_near, unarmed_share,
};
use crate::view::{distance, is_fighter, stack_strength, unit_strength};
use crate::Ai;
use f4x_master::Game;

/// The fewest spare soldiers that make a party.
const SITE_PARTY: usize = 3;
/// Turns a muster waits for its soldiers before it sets off with the ones that came.
const MUSTER_TURNS: u32 = 20;

pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // The parties out already: strike the site next to them, walk to the next, or go home.
    let out = {
        let view = PlayerView::new(game, me);
        let alive: Vec<Serial> = view.my_units().iter().map(|&u| view.serial_of(u)).collect();
        ai.roles
            .retain_role(crate::roles::Role::Raid, |s| alive.contains(&s));
        let mut out: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
        let mut home: Vec<Serial> = Vec::new();
        for u in view.my_units() {
            let serial = view.serial_of(u);
            if !ai.roles.is(serial, crate::roles::Role::Raid) {
                continue;
            }
            match view.unit_position(u) {
                Some(p) if view.city_at(p).is_none() => match out.iter_mut().find(|(q, _)| *q == p)
                {
                    Some((_, list)) => list.push(u),
                    None => out.push((p, vec![u])),
                },
                // Back in a city: the raid is over.
                _ => home.push(serial),
            }
        }
        ai.roles.retain_role(crate::roles::Role::Raid, |s| {
            let s = &s;
            !home.contains(s)
        });
        out.sort_by_key(|(p, _)| (p.plane as u8, p.y, p.x));
        out
    };
    for (pos, units) in out {
        if game.game_over().is_some() {
            return;
        }
        let (next_to, city_next, candidates, strength) = {
            let view = PlayerView::new(game, me);
            let strength = stack_strength(&view, &units) + crate::spellhelper::spell_power(&view);
            let unarmed = unarmed_share(&view, &units);
            let next_to = crate::military::neighbours(&view, pos)
                .into_iter()
                .find(|&p| {
                    view.has_lair(p)
                        && strength >= expected_vs(&view, p, unarmed) * edge()
                        && !lost_before(&ai.site_losses, p, strength)
                });
            let city_next = crate::military::neighbours(&view, pos)
                .into_iter()
                .find(|&p| neutral_city(&view, p) && strength >= city_defence(&view, p) * edge());
            // The site the party is set on stays its target while it is still worth taking:
            // "the nearest" changes as the party walks, and a party between two sites went to and
            // fro for ever.
            let mut sites = sites_near(&view, pos, strength, unarmed, &ai.tower_blocked);
            sites.retain(|&t| !lost_before(&ai.site_losses, t, strength));
            let kept = units
                .iter()
                .find_map(|&u| {
                    ai.roles
                        .target_of(view.serial_of(u), crate::roles::Role::Raid)
                })
                .filter(|t| sites.contains(t));
            // The site it is set on first, then the nearest few: one it cannot walk to is
            // passed over, and a party with none it can reach goes home.
            let mut candidates: Vec<Position> = kept.into_iter().collect();
            candidates.extend(sites.iter().copied().filter(|&t| Some(t) != kept).take(4));
            (next_to, city_next, candidates, strength)
        };
        let party: Vec<Serial> = {
            let view = PlayerView::new(game, me);
            units.iter().map(|&u| view.serial_of(u)).collect()
        };
        let before = events.len();
        if let Some(site) = next_to {
            assault(ai, game, &units, site, events);
            // A party that lost the assault (no site cleared, the battle won by the guards) does
            // not go again until it is much stronger.
            let lost = events[before..]
                .iter()
                .any(|e| matches!(e, Event::BattleEnded { loser, .. } if *loser == me));
            let cleared = events[before..]
                .iter()
                .any(|e| matches!(e, Event::SiteCleared { player, .. } if *player == me));
            if lost && !cleared {
                ai.site_losses.retain(|&(p, _)| p != site);
                ai.site_losses.push((site, strength));
            }
            // A cleared site is the raid's work done: the party has not stalled, so its time
            // limit starts again (`plan_clock.rs`).
            if events[before..]
                .iter()
                .any(|e| matches!(e, Event::SiteCleared { player, .. } if *player == me))
            {
                for &serial in &party {
                    ai.roles.restart_plan(serial);
                }
            }
        } else if let Some(city) = city_next {
            let defenders = PlayerView::new(game, me)
                .hostile_at(city)
                .unwrap_or_default();
            if defenders.is_empty() {
                // A party with a unit that has no moves left (it fought or walked this turn)
                // waits: the order would be refused, and the next turn's moves are whole.
                let tired = units
                    .iter()
                    .any(|&u| game.units().unit_moves_left(u) == f4x_def::half_int::HalfInt::ZERO);
                if !tired {
                    ai.order(game, Command::MoveStack { units, to: city }, events);
                }
            } else {
                engage(ai, game, units, defenders, events);
            }
            // A city taken ends the raid: the party moves in and is its garrison.
            if events[before..]
                .iter()
                .any(|e| matches!(e, Event::CityCaptured { to, .. } if *to == me))
            {
                ai.roles.retain_role(crate::roles::Role::Raid, |s| {
                    let s = &s;
                    !party.contains(s)
                });
                let stay: Vec<hecs::Entity> = {
                    let view = PlayerView::new(game, me);
                    view.my_units()
                        .into_iter()
                        .filter(|&u| party.contains(&view.serial_of(u)))
                        .filter(|&u| view.unit_position(u).is_some_and(|p| p != city))
                        .collect()
                };
                if !stay.is_empty() {
                    ai.order(
                        game,
                        Command::GoTo {
                            units: stay,
                            to: city,
                        },
                        events,
                    );
                }
            }
        } else if let Some(site) =
            candidates
                .iter()
                .copied()
                .find(|&t| match march(ai, game, &units, t, events) {
                    Ok(()) => true,
                    Err(why) => {
                        // A site the party cannot walk to is passed over by design (the next
                        // candidate is tried), so it is not a refused order worth logging.
                        if !matches!(why, crate::assault::MarchError::Unreachable) {
                            let detail = matches!(why, crate::assault::MarchError::Refused(_))
                                .then(|| crate::assault::march_detail(ai, game, &units, t));
                            ai.note_failure(
                                game,
                                format!("march to {t:?}"),
                                crate::with_detail(format!("{why:?}"), detail),
                            );
                        }
                        false
                    }
                })
        {
            for &s in &party {
                ai.roles.set_target(s, crate::roles::Role::Raid, Some(site));
            }
        } else {
            // No candidate could be walked to, or there was none: the raid is over where the
            // party stands. Its soldiers are free again and the dispatcher, the rest rules
            // (`recover.rs`) and the garrison job take them from there: nothing needs them in a
            // city, and a walk home was often one they could not make (water, the other plane,
            // a full city).
            if !candidates.is_empty() {
                // A site chosen that nobody could walk to is worth a line (one passed over is not).
                ai.note_failure(
                    game,
                    format!("raid party at {pos:?}: no site to march to among {candidates:?}"),
                    format!(
                        "Unreachable{}{}",
                        crate::DETAIL_SEPARATOR,
                        candidates
                            .iter()
                            .take(3)
                            .map(|&c| format!("{c:?}: {}", game.why_no_path(&units, c)))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                );
            }
            for &s in &party {
                ai.roles.set_target(s, crate::roles::Role::Raid, None);
            }
            ai.roles.retain_role(crate::roles::Role::Raid, |s| {
                let s = &s;
                !party.contains(s)
            });
        }
    }
    form(ai, game, events);
}

/// New raiding parties from the pool: the free soldiers on each square, the strongest nine of
/// them, that are strong enough (with the wizard's spells) for a site in reach, march to it. The
/// war job has taken its soldiers before this runs, and the garrisons are not in the pool, so
/// nothing caps the parties. Soldiers that cannot take any site together muster for a tower with
/// the spells (`gather`). Returns whether any party left.
pub fn form(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) -> bool {
    let me = ai.player;
    release(ai, game, game.round());
    let free = crate::dispatch::soldiers(ai, game, |roles, s| roles.is_free(s));
    let by_square = crate::military::stacks(&PlayerView::new(game, me), free.clone());
    let mut left = false;
    // Parties of one kind on one square find the same sites out of reach.
    let mut reached = crate::view::ReachCache::default();
    for (pos, mut units) in by_square {
        // The best nine first: a battle holds nine a side, and the hardest sites need the best.
        {
            let view = PlayerView::new(game, me);
            units.sort_by(|&a, &b| {
                unit_strength(&view, b)
                    .total_cmp(&unit_strength(&view, a))
                    .then(view.serial_of(a).cmp(&view.serial_of(b)))
            });
        }
        for party in units.chunks(game.max_battle_side()) {
            if party.len() < SITE_PARTY {
                continue;
            }
            let sites = {
                let view = PlayerView::new(game, me);
                let strength =
                    stack_strength(&view, party) + crate::spellhelper::spell_power(&view);
                let unarmed = unarmed_share(&view, party);
                let mut sites = sites_near(&view, pos, strength, unarmed, &ai.tower_blocked);
                sites.retain(|&t| !lost_before(&ai.site_losses, t, strength));
                sites
            };
            for site in sites {
                if crate::assault::march_with(&mut reached, ai, game, party, site, events).is_ok() {
                    let view = PlayerView::new(game, me);
                    let serials: Vec<Serial> = party.iter().map(|&u| view.serial_of(u)).collect();
                    ai.roles
                        .claim_all(serials, crate::roles::Role::Raid, game.round());
                    left = true;
                    break;
                }
            }
        }
    }
    if !left {
        gather(ai, game, &free, events);
    }
    left
}

/// End a muster whose soldiers have come (or waited long enough): they are free soldiers again,
/// and `raid::form` sends the best nine of the square they stand on.
pub(crate) fn release(ai: &mut Ai, world: &World, round: u32) {
    use crate::roles::MusterKind;
    if !ai.roles.any_muster(MusterKind::Raid) {
        return;
    }
    let view = PlayerView::new(world, ai.player);
    for (party, muster, members) in ai.roles.musters() {
        if muster.kind != MusterKind::Raid {
            continue;
        }
        let alive: Vec<_> = view
            .my_units()
            .into_iter()
            .filter(|&u| members.contains(&view.serial_of(u)))
            .collect();
        let arrived = alive.iter().all(|&u| {
            view.unit_position(u)
                .is_some_and(|p| distance(&view, p, muster.rally).is_some_and(|d| d <= 1))
        });
        if alive.is_empty() || arrived || round.saturating_sub(muster.since) > MUSTER_TURNS {
            ai.roles.end_muster(party);
        }
    }
}

/// The free soldiers of the empire cannot take a tower from any one square, but the best nine of
/// them with the wizard's spells could: send those nine to the own city nearest the tower. They
/// are kept apart from raids while they walk (`Ai::muster`).
pub(crate) fn gather(
    ai: &mut Ai,
    game: &mut Game,
    free: &[hecs::Entity],
    events: &mut Vec<f4x_core::world::Event>,
) {
    use f4x_core::world::Command;
    let me = ai.player;
    if ai.roles.any_muster(crate::roles::MusterKind::Raid) {
        return;
    }
    let (rally, objective, orders) = {
        let view = PlayerView::new(game, me);
        if crate::phase::phase(&view, game.round()) == crate::phase::Phase::Build {
            return;
        }
        let mut best: Vec<_> = free
            .iter()
            .copied()
            .filter(|&u| {
                is_fighter(&view, u)
                    && !crate::recover::needs_rest(&view, u)
                    && !ai.roles.is(view.serial_of(u), crate::roles::Role::Raid)
                    && !ai.roles.is_scouting(view.serial_of(u))
                    && view.unit_position(u).is_some()
            })
            .collect();
        best.sort_by(|&a, &b| {
            crate::view::unit_strength(&view, b)
                .total_cmp(&crate::view::unit_strength(&view, a))
                .then(view.serial_of(a).cmp(&view.serial_of(b)))
        });
        best.truncate(view.max_battle_side());
        if best.len() < 3 {
            return;
        }
        let strength =
            crate::view::stack_strength(&view, &best) + crate::spellhelper::spell_power(&view);
        // The easiest site (a lair, a neutral city, a tower on a plane with no rival's city) the
        // nine could take, reached from the own city nearest to it.
        let cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
        let mut found: Vec<(f32, u32, Position, Position)> = Vec::new();
        for &city in &cities {
            let unarmed = unarmed_share(&view, &best);
            for site in crate::site::sites_near(&view, city, strength, unarmed, &ai.tower_blocked) {
                // A tower on a plane where a rival has a city waits (the rivals come first).
                let blocked_tower = ai.tower_blocked.contains(&site.plane)
                    && view.towers().iter().any(|&(t, _)| t == site);
                if site.plane != city.plane
                    || blocked_tower
                    || lost_before(&ai.site_losses, site, strength)
                {
                    continue;
                }
                if let Some(d) = distance(&view, city, site) {
                    found.push((crate::site::defence_of(&view, site), d, site, city));
                }
            }
        }
        let chosen = found.into_iter().min_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then((a.1, a.2.y, a.2.x, a.3.y, a.3.x).cmp(&(b.1, b.2.y, b.2.x, b.3.y, b.3.x)))
        });
        let Some((_, _, tower, rally)) = chosen else {
            return;
        };
        let mut reached = crate::view::ReachCache::default();
        let orders: Vec<hecs::Entity> = best
            .into_iter()
            .filter(|&u| {
                view.unit_position(u).is_some_and(|p| {
                    distance(&view, p, rally).is_none_or(|d| d > 1) && !view.unit_has_order(u)
                })
            })
            .filter(|&u| reached.can_reach(&view, &[u], rally))
            .collect();
        (rally, tower, orders)
    };
    if orders.is_empty() {
        return;
    }
    let serials: Vec<_> = {
        let view = PlayerView::new(game, me);
        orders.iter().map(|&u| view.serial_of(u)).collect()
    };
    for u in orders {
        if Ai::room_at(game, rally, 1) {
            let _ = ai.send(
                game,
                Command::GoTo {
                    units: vec![u],
                    to: rally,
                },
                events,
            );
        }
    }
    ai.roles.begin_muster(
        crate::roles::MusterKind::Raid,
        rally,
        Some(objective),
        &serials,
        game.round(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{
        at, game, game_rival_across, spawn, spawn_site, spawn_tower, COAST_CITY, P0,
    };
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The west wizard with `paladins` in its city at (10, 6) and an encounter site of the
    /// given guards two squares west. `known`: the wizard has fought for the site before.
    fn world(
        paladins: usize,
        guards: Vec<(f4x_def::rules::UnitId, u32)>,
        known: bool,
    ) -> Result<Game, Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.treasury_mut(P0).ok_or("player 0 has a treasury")?.gold =
            f4x_def::half_int::HalfInt::from_int(1_000_000);
        spawn(
            &mut w,
            P0,
            at(10, 6),
            113, /* high_men_paladins */
            paladins,
        );
        spawn_site(
            &mut w,
            at(8, 6),
            guards,
            if known { vec![P0] } else { Vec::new() },
        )?;
        Ok(w)
    }

    /// Whether every soldier of the wizard stands in its city.
    fn all_home(w: &World) -> bool {
        w.units()
            .player_units(P0)
            .iter()
            .all(|&u| w.units().unit_position(u) == Some(at(10, 6)))
    }

    fn turns(w: &mut Game, ai: &mut Ai, n: usize) -> Vec<Event> {
        let mut events = Vec::new();
        w.balance_farmers(P0);
        for _ in 0..n {
            run(ai, w, &mut events);
            w.play_round();
            w.balance_farmers(P0);
        }
        events
    }

    #[test]
    fn a_tower_is_looked_for_further_away_than_other_sites() -> TestResult {
        let mut w = game_rival_across(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut w, at(33, 9), true)?;
        spawn_site(
            &mut w,
            at(33, 7),
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            Vec::new(),
        )?;
        w.reveal_map(P0);
        let view = PlayerView::new(&w, P0);
        assert_eq!(
            sites_near(&view, at(10, 6), 10_000.0, 0.0, &[]),
            vec![at(33, 9)]
        );
        Ok(())
    }

    #[test]
    fn a_strong_party_next_to_an_encounter_site_clears_it() -> TestResult {
        let mut w = world(
            5,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        let events = turns(&mut w, &mut ai, 10);
        let gold: u32 = events
            .iter()
            .filter_map(|e| match e {
                Event::SiteCleared { player, loot, .. } if *player == P0 => Some(loot.gold),
                _ => None,
            })
            .sum();
        assert_eq!(gold, 300, "the hoard is the wizard's");
        assert!(w.economy().lair_at(at(8, 6)).is_none());
        Ok(())
    }

    /// A party with a unit that has no moves left does not order the walk into the empty city
    /// (the game refuses it): it waits, and goes in the next turn with its moves whole.
    #[test]
    fn a_party_with_a_tired_unit_waits_before_an_empty_city() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let neutral = f4x_core::player::PlayerId(2);
        w.add_neutral_player(neutral);
        let east = at(33, 6);
        w.command(Command::FoundCity {
            owner: neutral,
            pos: east,
            name: "Greyhold".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 8,
        });
        w.explore(P0, east, f4x_def::half_int::HalfInt::from_raw(3));
        let party = spawn(&mut w, P0, at(32, 6), 113 /* high_men_paladins */, 2);
        w.scenario_set_moves_left(party[0], f4x_def::half_int::HalfInt::ZERO);
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        {
            let view = PlayerView::new(&w, P0);
            ai.roles.assign(
                crate::roles::Role::Raid,
                party.iter().map(|&u| view.serial_of(u)),
                1,
            );
        }
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(ai.take_refusals().is_empty(), "no refused order");
        assert!(party
            .iter()
            .all(|&u| w.units().unit_position(u) == Some(at(32, 6))));
        // The next turn the moves are whole and the party goes in.
        let whole = w.units().unit_max_moves(party[0]);
        w.scenario_set_moves_left(party[0], whole);
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(party
            .iter()
            .all(|&u| w.units().unit_position(u) == Some(east)));
        Ok(())
    }

    #[test]
    fn a_raid_that_takes_a_city_is_over_and_the_party_is_its_garrison() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // A city of a neutral player (always at war with everyone) with nobody in it.
        let neutral = f4x_core::player::PlayerId(2);
        w.add_neutral_player(neutral);
        let east = at(33, 6);
        let founded = w.command(Command::FoundCity {
            owner: neutral,
            pos: east,
            name: "Greyhold".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 8,
        });
        assert!(
            !founded.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{founded:?}"
        );
        w.explore(P0, east, f4x_def::half_int::HalfInt::from_raw(3));
        let party = spawn(&mut w, P0, at(32, 6), 113 /* high_men_paladins */, 4);
        let mut ai = Ai::new(P0);
        {
            let view = PlayerView::new(&w, P0);
            ai.roles.assign(
                crate::roles::Role::Raid,
                party.iter().map(|&u| view.serial_of(u)),
                1,
            );
        }
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::CityCaptured { to, .. } if *to == P0)),
            "{events:?}"
        );
        assert!(!ai.roles.any(crate::roles::Role::Raid), "the raid is over");
        assert!(
            party
                .iter()
                .all(|&u| w.units().unit_position(u) == Some(east)),
            "the party stands in the city"
        );
        Ok(())
    }

    #[test]
    fn a_cleared_site_gives_the_party_a_fresh_time() -> TestResult {
        let mut w = world(
            5,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        // A party next to the site (the soldiers of the city stay where they are).
        let party = spawn(&mut w, P0, at(9, 6), 113 /* high_men_paladins */, 5);
        {
            let view = PlayerView::new(&w, P0);
            ai.roles.assign(
                crate::roles::Role::Raid,
                party.iter().map(|&u| view.serial_of(u)),
                1,
            );
        }
        // The party has been out for 90 turns when it clears the site.
        ai.drop_stale_plans(w.round());
        w.set_round(w.round() + 90);
        ai.drop_stale_plans(w.round());
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::SiteCleared { player, .. } if *player == P0)));
        // Twenty turns later it would be 110 turns out, but the clear started the time again.
        w.set_round(w.round() + 20);
        ai.drop_stale_plans(w.round());
        w.set_round(w.round() + 20);
        ai.drop_stale_plans(w.round());
        assert!(ai.roles.any(crate::roles::Role::Raid), "not dropped");
        assert_eq!(ai.plans_dropped().iter().map(|(_, n)| n).sum::<u32>(), 0);
        Ok(())
    }

    #[test]
    fn a_party_keeps_the_site_it_is_set_on_while_another_becomes_nearer() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let guards = || vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)];
        spawn_site(&mut w, at(4, 4), guards(), Vec::new())?;
        spawn_site(&mut w, at(4, 9), guards(), Vec::new())?;
        w.reveal_map(P0);
        let party = spawn(&mut w, P0, at(6, 7), 113 /* high_men_paladins */, 5);
        let mut ai = Ai::new(P0);
        let view = PlayerView::new(&w, P0);
        ai.roles.assign(
            crate::roles::Role::Raid,
            party.iter().map(|&u| view.serial_of(u)),
            1,
        );
        // The party is set on the far site (4, 4) though (4, 9) is nearer to where it stands.
        for s in ai.roles.members(crate::roles::Role::Raid) {
            ai.roles
                .set_target(s, crate::roles::Role::Raid, Some(at(4, 4)));
        }
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let here = w
            .units()
            .unit_position(party[0])
            .ok_or("the party is on the map")?;
        assert!(here.y < 7, "it went towards (4, 4), not (4, 9): {here:?}");
        assert!(ai
            .roles
            .targets(crate::roles::Role::Raid)
            .iter()
            .all(|&t| t == at(4, 4)));
        Ok(())
    }

    #[test]
    fn a_party_whose_only_site_cannot_be_reached_goes_home_and_the_raid_ends() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // A tower on the east island: in range, but over the sea from the party.
        spawn_tower(&mut w, at(30, 9), true)?;
        w.reveal_map(P0);
        let party = spawn(&mut w, P0, at(5, 8), 113 /* high_men_paladins */, 5);
        let mut ai = Ai::new(P0);
        let view = PlayerView::new(&w, P0);
        ai.roles.assign(
            crate::roles::Role::Raid,
            party.iter().map(|&u| view.serial_of(u)),
            1,
        );
        let mut events = Vec::new();
        for _ in 0..4 {
            run(&mut ai, &mut w, &mut events);
            w.play_round();
        }
        assert!(
            ai.roles.targets(crate::roles::Role::Raid).is_empty(),
            "no target it can walk to"
        );
        assert!(
            !ai.roles.any(crate::roles::Role::Raid),
            "the raid ended at the city: {:?}",
            ai.roles
        );
        Ok(())
    }

    #[test]
    fn a_party_with_no_site_left_ends_the_raid_where_it_stands_on_either_plane() -> TestResult {
        use f4x_core::map::Plane;
        for plane in [Plane::Arcanus, Plane::Myrror] {
            let mut w = game(COAST_CITY, 8 /* high_men */);
            w.reveal_map(P0);
            let spot = Position { plane, ..at(5, 8) };
            let party = spawn(&mut w, P0, spot, 113 /* high_men_paladins */, 4);
            let mut ai = Ai::new(P0);
            ai.record_refusals(true);
            let view = PlayerView::new(&w, P0);
            ai.roles.assign(
                crate::roles::Role::Raid,
                party.iter().map(|&u| view.serial_of(u)),
                1,
            );
            let mut events = Vec::new();
            run(&mut ai, &mut w, &mut events);
            assert!(!ai.roles.any(crate::roles::Role::Raid), "{plane:?}");
            assert!(ai.take_refusals().is_empty(), "{plane:?}: no refused order");
            assert!(
                party
                    .iter()
                    .all(|&u| w.units().unit_position(u) == Some(spot)),
                "{plane:?}: nobody is sent anywhere"
            );
        }
        Ok(())
    }

    #[test]
    fn a_march_that_cannot_be_ordered_says_why() -> TestResult {
        use crate::assault::{march, MarchError};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut w, at(30, 9), true)?;
        w.reveal_map(P0);
        let party = spawn(&mut w, P0, at(5, 8), 113 /* high_men_paladins */, 3);
        let ai = Ai::new(P0);
        let mut events = Vec::new();
        assert_eq!(
            march(&ai, &mut w, &party, at(30, 9), &mut events),
            Err(MarchError::Unreachable),
            "over the sea"
        );
        assert_eq!(
            march(&ai, &mut w, &[], at(30, 9), &mut events),
            Err(MarchError::NoUnits)
        );
        // A site on the party's own island can be marched on.
        let site = at(3, 3);
        spawn_site(&mut w, site, Vec::new(), Vec::new())?;
        assert_eq!(march(&ai, &mut w, &party, site, &mut events), Ok(()));
        Ok(())
    }

    #[test]
    fn a_weak_party_leaves_a_site_whose_guards_it_knows_to_be_too_strong() -> TestResult {
        // The guards are as many great drakes as it takes for them to be worth three times the
        // party in the measure of the rules (`battle::power`): the test is about the decision to
        // leave a site that is too strong by that measure, not about which unit is stronger.
        const PARTY: usize = 4;
        let drake = f4x_def::rules::UnitId::new(164 /* great_drake */);
        let content = game(COAST_CITY, 8 /* high_men */);
        let rules = &content.content().rules;
        let one_paladin = f4x_battle::power::def_power(
            rules.units.get(f4x_def::rules::UnitId::new(
                113, /* high_men_paladins */
            )),
            rules,
        );
        let one_drake = f4x_battle::power::def_power(rules.units.get(drake), rules);
        let guards = (3.0 * PARTY as f32 * one_paladin / one_drake)
            .ceil()
            .max(1.0) as u32;
        let mut w = world(PARTY, vec![(drake, guards)], true)?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 4);
        assert!(!ai.roles.any(crate::roles::Role::Raid));
        assert!(w.economy().lair_at(at(8, 6)).is_some());
        assert!(all_home(&w));
        Ok(())
    }

    #[test]
    fn a_site_a_party_lost_on_is_left_alone_until_a_party_is_much_stronger() -> TestResult {
        // The guards are weak by the measure (one spearman against a party of paladins), so a
        // party would go: it stays home only because a party as strong as this one lost its
        // assault on the site before.
        let guards = vec![(f4x_def::rules::UnitId::new(104 /* high_men_spearmen */), 1)];
        let mut w = world(4, guards.clone(), true)?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 1);
        assert!(
            ai.roles.any(crate::roles::Role::Raid),
            "the party goes when nothing is held against the site"
        );
        let mut w = world(4, guards.clone(), true)?;
        let mut ai = Ai::new(P0);
        ai.site_losses.push((at(8, 6), f32::MAX / 2.0));
        turns(&mut w, &mut ai, 4);
        assert!(!ai.roles.any(crate::roles::Role::Raid));
        assert!(w.economy().lair_at(at(8, 6)).is_some());
        assert!(all_home(&w));
        Ok(())
    }

    #[test]
    fn a_party_with_normal_weapons_leaves_guards_that_weapons_barely_hurt_unless_a_spell_arms_it(
    ) -> TestResult {
        use crate::site::{edge, expected_vs, unarmed_share};
        let rules = &game(COAST_CITY, 8).content().rules.clone();
        let ability = rules
            .abilities
            .id_numbered(72 /* weapon_immunity */)
            .ok_or("no weapon immunity")?;
        let immune = rules
            .units
            .defs()
            .iter()
            .find(|d| d.abilities.contains(&ability))
            .map(|d| f4x_def::rules::UnitId::new(d.num))
            .ok_or("no unit has weapon immunity")?;
        // The most guards the party would take if its weapons hurt them, and no more.
        let mut chosen = None;
        for k in 1..60u32 {
            let w = world(4, vec![(immune, k)], true)?;
            let view = PlayerView::new(&w, P0);
            let party = w.units().player_units(P0);
            let strength = stack_strength(&view, &party);
            let armed = expected_vs(&view, at(8, 6), 0.0) * edge() <= strength;
            let unarmed = unarmed_share(&view, &party);
            let blocked = expected_vs(&view, at(8, 6), unarmed) * edge() > strength;
            if armed && blocked {
                chosen = Some(k);
            }
        }
        let k = chosen.ok_or("no number of guards is beatable only with magical weapons")?;
        let mut w = world(4, vec![(immune, k)], true)?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 4);
        assert!(
            !ai.roles.any(crate::roles::Role::Raid),
            "paladins with normal weapons go against guards they cannot hurt"
        );
        // With Flame Blade known and the casting skill and mana for four casts, the same party goes.
        let mut w = world(4, vec![(immune, k)], true)?;
        let blade = w
            .content()
            .rules
            .spells
            .id_numbered(94)
            .ok_or("no flame blade")?;
        w.cheat_learn_spell(P0, blade);
        w.wizard_mut(P0)
            .ok_or("no wizard")?
            .get_mut::<f4x_core::wizard::sheet::Skill>()
            .points = 1000;
        w.treasury_mut(P0).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(400);
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 1);
        assert!(
            ai.roles.any(crate::roles::Role::Raid),
            "the party goes when a spell can arm it"
        );
        Ok(())
    }

    #[test]
    fn a_city_keeps_its_garrison_and_waits_for_a_party() -> TestResult {
        // One spare fighter beyond the single guard is not a party.
        let mut w = world(
            2,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 4);
        assert!(!ai.roles.any(crate::roles::Role::Raid));
        assert!(w.economy().lair_at(at(8, 6)).is_some());
        Ok(())
    }

    /// Soldiers who muster for the campaign hold that role; a raid takes only free soldiers, so the
    /// same soldiers go on a raid when they are free and do not when they muster.
    #[test]
    fn a_raid_does_not_take_the_soldiers_of_a_campaign_muster() -> TestResult {
        let guards = vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)];
        let city = at(10, 6);
        let serials = |w: &World| {
            let view = PlayerView::new(w, P0);
            view.garrison(city)
                .into_iter()
                .filter(|&u| is_fighter(&view, u))
                .map(|u| view.serial_of(u))
                .collect::<Vec<_>>()
        };
        // Control: free, they raid.
        let mut w = world(6, guards.clone(), false)?;
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.round());
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(ai.roles.any(crate::roles::Role::Raid), "free soldiers raid");
        // Mustering for the campaign, they stay out of it.
        let mut w = world(6, guards, false)?;
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.round());
        let members = serials(&w);
        assert!(ai
            .roles
            .begin_muster(
                crate::roles::MusterKind::Campaign,
                city,
                Some(at(4, 4)),
                &members,
                w.round(),
            )
            .is_some());
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(!ai.roles.any(crate::roles::Role::Raid), "{:?}", ai.roles);
        Ok(())
    }

    #[test]
    fn the_survivors_are_free_soldiers_again_where_the_raid_ended() -> TestResult {
        let mut w = world(
            6,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 14);
        assert!(
            w.economy().lair_at(at(8, 6)).is_none(),
            "the site is cleared"
        );
        assert!(!ai.roles.any(crate::roles::Role::Raid), "{:?}", ai.roles);
        // No walk home is ordered: the soldiers stand free for the jobs that need them.
        let view = PlayerView::new(&w, P0);
        assert!(w
            .units()
            .player_units(P0)
            .into_iter()
            .all(|u| ai.roles.is_free(view.serial_of(u))));
        Ok(())
    }

    #[test]
    fn an_army_in_the_field_with_nothing_to_march_on_raids_the_nearest_tower() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(11, 6), 113 /* high_men_paladins */, 6);
        spawn_tower(&mut w, at(8, 6), true)?;
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        assert!(form(&mut ai, &mut w, &mut events), "{events:?}");
        assert_eq!(ai.roles.count(crate::roles::Role::Raid), 6);
        Ok(())
    }
}
