//! Towers, magic nodes and encounter sites: a wizard's spare soldiers clear the guarded sites
//! near their cities. The AI plays by the human's rules (`design.md` section 8): it knows the
//! guards only from a fight it fought or a spell that shows them (`PlayerView::lair_guards`),
//! and guesses otherwise. A party goes in through `Command::EnterSite`, the battle AI fighting.

use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::assault::{around, assault, guard_strength, march, strike};
use crate::military::average_strength;
use crate::view::{
    distance, fighting_strength, garrison_wanted, is_fighter, stack_strength, unit_strength,
};
use crate::Ai;

/// Squares from a city (or from a party in the field) within which a site is looked for.
const SITE_RANGE: i32 = 10;
/// Squares within which a party looks for a tower: it is the way to the other plane.
const TOWER_RANGE: u32 = 30;
/// Most soldiers out on raids at once.
const MAX_RAIDERS: usize = 2 * f4x_core::unit::MAX_STACK;
/// The fewest spare soldiers that make a party.
const SITE_PARTY: usize = 3;
/// What an unknown site's guards are guessed to be worth, in average soldiers of this wizard.
const GUESS_GUARDS: f32 = 2.5;

/// A party attacks when it is this much stronger than the guards, known or guessed.
fn edge() -> f32 {
    crate::knob("SITE", 1.4)
}

/// What the AI expects the site's guards to be worth.
pub(crate) fn expected(view: &PlayerView, site: Position) -> f32 {
    guard_strength(view, site).unwrap_or_else(|| GUESS_GUARDS * average_strength(view))
}

/// Is there a city of the neutral cities (no wizard's) on the square?
fn neutral_city(view: &PlayerView, p: Position) -> bool {
    view.known_cities()
        .iter()
        .any(|&(_, owner, q)| q == p && !view.is_rival_wizard(owner))
}

/// What a neutral city's defenders are worth (a city is stronger than the same men outside).
fn city_defence(view: &PlayerView, p: Position) -> f32 {
    match view.hostile_at(p) {
        Some(defenders) => fighting_strength(view, &defenders) * 1.3,
        None => 2.0 * GUESS_GUARDS * average_strength(view),
    }
}

/// The explored, intact sites within [`SITE_RANGE`] of `from` (towers within [`TOWER_RANGE`]) that a party of `strength` should
/// take, nearest first.
fn sites_near(view: &PlayerView, from: Position, strength: f32) -> Vec<Position> {
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
            && distance(view, from, p).is_some_and(|d| d <= TOWER_RANGE)
            && strength >= expected(view, p) * edge()
        {
            found.push(p);
        }
    }
    found.sort_by_key(|&p| (distance(view, from, p), p.y, p.x));
    found
}

pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // The parties out already: strike the site next to them, walk to the next, or go home.
    let out = {
        let view = PlayerView::new(game, me);
        let alive: Vec<Serial> = view.my_units().iter().map(|&u| view.serial_of(u)).collect();
        ai.raiders.retain(|s| alive.contains(s));
        let raiders = ai.raiders.clone();
        ai.raid_targets.retain(|s, _| raiders.contains(s));
        let mut out: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
        let mut home: Vec<Serial> = Vec::new();
        for u in view.my_units() {
            let serial = view.serial_of(u);
            if !ai.raiders.contains(&serial) {
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
        ai.raiders.retain(|s| !home.contains(s));
        out.sort_by_key(|(p, _)| (p.plane as u8, p.y, p.x));
        out
    };
    for (pos, units) in out {
        if game.game_over.is_some() {
            return;
        }
        let (next_to, city_next, candidates, home) = {
            let view = PlayerView::new(game, me);
            let strength = stack_strength(&view, &units);
            let next_to = around(&view, pos)
                .into_iter()
                .find(|&p| view.has_lair(p) && strength >= expected(&view, p) * edge());
            let city_next = around(&view, pos)
                .into_iter()
                .find(|&p| neutral_city(&view, p) && strength >= city_defence(&view, p) * edge());
            // The site the party is set on stays its target while it is still worth taking:
            // "the nearest" changes as the party walks, and a party between two sites went to and
            // fro for ever.
            let sites = sites_near(&view, pos, strength);
            let kept = units
                .iter()
                .find_map(|&u| ai.raid_targets.get(&view.serial_of(u)).copied())
                .filter(|t| sites.contains(t));
            // The site it is set on first, then the nearest few: one it cannot walk to is
            // passed over, and a party with none it can reach goes home.
            let mut candidates: Vec<Position> = kept.into_iter().collect();
            candidates.extend(sites.iter().copied().filter(|&t| Some(t) != kept).take(4));
            let home = view
                .my_cities()
                .into_iter()
                .map(|(_, p)| p)
                .min_by_key(|&p| (distance(&view, pos, p), p.y, p.x));
            (next_to, city_next, candidates, home)
        };
        let party: Vec<Serial> = {
            let view = PlayerView::new(game, me);
            units.iter().map(|&u| view.serial_of(u)).collect()
        };
        let before = events.len();
        if let Some(site) = next_to {
            assault(ai, game, &units, site, events);
            // A cleared site is the raid's work done: the party has not stalled, so its time
            // limit starts again (`plan_clock.rs`).
            if events[before..]
                .iter()
                .any(|e| matches!(e, Event::SiteCleared { player, .. } if *player == me))
            {
                ai.clock.restart_raiders(&party);
            }
        } else if let Some(city) = city_next {
            let defenders = PlayerView::new(game, me)
                .hostile_at(city)
                .unwrap_or_default();
            if defenders.is_empty() {
                ai.order(game, Command::MoveStack { units, to: city }, events);
            } else {
                strike(ai, game, units, defenders, events);
            }
            // A city taken ends the raid: the party moves in and is its garrison.
            if events[before..]
                .iter()
                .any(|e| matches!(e, Event::CityCaptured { to, .. } if *to == me))
            {
                ai.raiders.retain(|s| !party.contains(s));
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
                            ai.note_failure(game, format!("march to {t:?}"), format!("{why:?}"));
                        }
                        false
                    }
                })
        {
            for &s in &party {
                ai.raid_targets.insert(s, site);
            }
        } else if let Some(home) = home {
            party.iter().for_each(|s| {
                ai.raid_targets.remove(s);
            });
            let serials: Vec<Serial> = {
                let view = PlayerView::new(game, me);
                units.iter().map(|&u| view.serial_of(u)).collect()
            };
            // A party next to a city of its own is home: the raid is over. (A full city lets
            // nobody in, and a party sent into it waits at its gate for ever: task 515.)
            if distance(&PlayerView::new(game, me), pos, home).is_some_and(|d| d <= 1) {
                ai.raiders.retain(|s| !serials.contains(s));
                continue;
            }
            // A party that cannot find its way home (cut off by guarded ground or the sea) gives
            // up the raid and is left to the army's rules.
            let reachable = PlayerView::new(game, me).can_reach(&units, home);
            if !reachable
                || !ai
                    .send(game, Command::GoTo { units, to: home }, events)
                    .is_ok()
            {
                ai.raiders.retain(|s| !serials.contains(s));
            }
        }
    }
    // A new party leaves a city with soldiers to spare, one raid at a time and never while the
    // army marches (its campaign comes first).
    if ai.raiders.len() >= MAX_RAIDERS || !ai.marching.is_empty() {
        return;
    }
    let cities = {
        let view = PlayerView::new(game, me);
        let mut cities = view.my_cities();
        cities.sort_by_key(|(e, _)| view.serial_of(*e));
        cities
    };
    for (_, pos) in cities {
        let (party, sites) = {
            let view = PlayerView::new(game, me);
            // Heroes go last in the order, so the garrison is the soldiers and the heroes are the
            // spare ones: they lead the raids, level up and bring the loot's items home.
            let heroes = view.my_heroes();
            let mut fighters: Vec<hecs::Entity> = view
                .garrison(pos)
                .into_iter()
                .filter(|&u| is_fighter(&view, u) && !ai.scouts.contains(&view.serial_of(u)))
                .collect();
            crate::view::weakest_first(&view, &mut fighters);
            fighters.sort_by_key(|u| heroes.contains(u));
            let spare: Vec<hecs::Entity> = fighters
                .into_iter()
                .skip(garrison_wanted(&view, pos))
                .take(f4x_core::unit::MAX_STACK)
                .collect();
            if spare.len() < SITE_PARTY {
                continue;
            }
            let sites = sites_near(&view, pos, stack_strength(&view, &spare));
            (spare, sites)
        };
        for site in sites {
            if march(ai, game, &party, site, events).is_ok() {
                let view = PlayerView::new(game, me);
                ai.raiders.extend(party.iter().map(|&u| view.serial_of(u)));
                return;
            }
        }
    }
}

/// An army with no enemy city to march on goes raiding: each stack of the soldiers it has in
/// the field (or spare in a city) that is strong enough clears the nearest site, tower or
/// neutral city. A tower cleared opens the way to the other plane. The raids are not capped:
/// these soldiers have nothing else to do. Returns whether any party left.
pub fn deploy(
    ai: &mut Ai,
    game: &mut Game,
    free: &[hecs::Entity],
    events: &mut Vec<Event>,
) -> bool {
    let me = ai.player;
    let mut by_square: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
    {
        let view = PlayerView::new(game, me);
        for &u in free {
            if ai.raiders.contains(&view.serial_of(u)) || ai.scouts.contains(&view.serial_of(u)) {
                continue;
            }
            let Some(p) = view.unit_position(u) else {
                continue;
            };
            match by_square.iter_mut().find(|(q, _)| *q == p) {
                Some((_, list)) => list.push(u),
                None => by_square.push((p, vec![u])),
            }
        }
    }
    by_square.sort_by_key(|(p, _)| (p.plane as u8, p.y, p.x));
    let mut left = false;
    // Parties of one kind on one square find the same sites out of reach.
    let mut reached = crate::view::ReachCache::default();
    for (pos, mut units) in by_square {
        // The best nine first: a battle holds nine a side, and the hardest sites need the best.
        {
            let view = PlayerView::new(game, me);
            units.sort_by(|&a, &b| unit_strength(&view, b).total_cmp(&unit_strength(&view, a)));
        }
        for party in units.chunks(f4x_core::unit::MAX_STACK) {
            if party.len() < SITE_PARTY {
                continue;
            }
            let sites = {
                let view = PlayerView::new(game, me);
                sites_near(&view, pos, stack_strength(&view, party))
            };
            for site in sites {
                if crate::assault::march_with(&mut reached, ai, game, party, site, events).is_ok() {
                    let view = PlayerView::new(game, me);
                    ai.raiders.extend(party.iter().map(|&u| view.serial_of(u)));
                    left = true;
                    break;
                }
            }
        }
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, spawn_site, spawn_tower, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The west wizard with `paladins` in its city at (10, 6) and an encounter site of the
    /// given guards two squares west. `known`: the wizard has fought for the site before.
    fn world(
        paladins: usize,
        guards: Vec<(f4x_def::rules::UnitId, u32)>,
        known: bool,
    ) -> Result<Game, Box<dyn std::error::Error>> {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.treasury_mut(P0).ok_or("player 0 has a treasury")?.gold =
            f4x_def::half_int::HalfInt::from_int(1_000_000);
        spawn(
            &mut g,
            P0,
            at(10, 6),
            113, /* high_men_paladins */
            paladins,
        );
        spawn_site(
            &mut g,
            at(8, 6),
            guards,
            if known { vec![P0] } else { Vec::new() },
        )?;
        Ok(g)
    }

    /// Whether every soldier of the wizard stands in its city.
    fn all_home(g: &Game) -> bool {
        g.units()
            .player_units(P0)
            .iter()
            .all(|&u| g.units().unit_position(u) == Some(at(10, 6)))
    }

    fn turns(g: &mut Game, ai: &mut Ai, n: usize) -> Vec<Event> {
        let mut events = Vec::new();
        g.balance_farmers(P0);
        for _ in 0..n {
            run(ai, g, &mut events);
            g.apply(Command::EndTurn);
            g.balance_farmers(P0);
        }
        events
    }

    #[test]
    fn a_tower_is_looked_for_further_away_than_other_sites() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut g, at(33, 9), true)?;
        spawn_site(
            &mut g,
            at(33, 7),
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            Vec::new(),
        )?;
        g.reveal_map(P0);
        let view = PlayerView::new(&g, P0);
        assert_eq!(sites_near(&view, at(10, 6), 10_000.0), vec![at(33, 9)]);
        Ok(())
    }

    #[test]
    fn a_strong_party_next_to_an_encounter_site_clears_it() -> TestResult {
        let mut g = world(
            5,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        let events = turns(&mut g, &mut ai, 10);
        let gold: u32 = events
            .iter()
            .filter_map(|e| match e {
                Event::SiteCleared { player, loot, .. } if *player == P0 => Some(loot.gold),
                _ => None,
            })
            .sum();
        assert_eq!(gold, 300, "the hoard is the wizard's");
        assert!(g.economy().lair_at(at(8, 6)).is_none());
        Ok(())
    }

    #[test]
    fn a_raid_that_takes_a_city_is_over_and_the_party_is_its_garrison() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        // A city of a neutral player (always at war with everyone) with nobody in it.
        let neutral = f4x_core::player::PlayerId(2);
        g.add_neutral_player(neutral);
        let east = at(33, 6);
        let founded = g.apply(Command::FoundCity {
            owner: neutral,
            pos: east,
            name: "Greyhold".into(),
            race: g.content().rules.races.id_numbered(8),
            population: 8,
        });
        assert!(
            !founded.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{founded:?}"
        );
        g.explore(P0, east, f4x_def::half_int::HalfInt::from_raw(3));
        let party = spawn(&mut g, P0, at(32, 6), 113 /* high_men_paladins */, 4);
        let mut ai = Ai::new(P0);
        {
            let view = PlayerView::new(&g, P0);
            ai.raiders = party.iter().map(|&u| view.serial_of(u)).collect();
        }
        let mut events = Vec::new();
        run(&mut ai, &mut g, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::CityCaptured { to, .. } if *to == P0)),
            "{events:?}"
        );
        assert!(ai.raiders.is_empty(), "the raid is over");
        assert!(
            party
                .iter()
                .all(|&u| g.units().unit_position(u) == Some(east)),
            "the party stands in the city"
        );
        Ok(())
    }

    #[test]
    fn a_cleared_site_gives_the_party_a_fresh_time() -> TestResult {
        let mut g = world(
            5,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        // A party next to the site (the soldiers of the city stay where they are).
        let party = spawn(&mut g, P0, at(9, 6), 113 /* high_men_paladins */, 5);
        {
            let view = PlayerView::new(&g, P0);
            ai.raiders = party.iter().map(|&u| view.serial_of(u)).collect();
        }
        // The party has been out for 90 turns when it clears the site.
        ai.drop_stale_plans(g.turn);
        g.turn += 90;
        ai.drop_stale_plans(g.turn);
        let mut events = Vec::new();
        run(&mut ai, &mut g, &mut events);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::SiteCleared { player, .. } if *player == P0)));
        // Twenty turns later it would be 110 turns out, but the clear started the time again.
        g.turn += 20;
        ai.drop_stale_plans(g.turn);
        g.turn += 20;
        ai.drop_stale_plans(g.turn);
        assert!(!ai.raiders.is_empty(), "not dropped");
        assert_eq!(ai.plans_dropped().iter().map(|(_, n)| n).sum::<u32>(), 0);
        Ok(())
    }

    #[test]
    fn a_party_keeps_the_site_it_is_set_on_while_another_becomes_nearer() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let guards = || vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)];
        spawn_site(&mut g, at(4, 4), guards(), Vec::new())?;
        spawn_site(&mut g, at(4, 9), guards(), Vec::new())?;
        g.reveal_map(P0);
        let party = spawn(&mut g, P0, at(6, 7), 113 /* high_men_paladins */, 5);
        let mut ai = Ai::new(P0);
        let view = PlayerView::new(&g, P0);
        ai.raiders = party.iter().map(|&u| view.serial_of(u)).collect();
        // The party is set on the far site (4, 4) though (4, 9) is nearer to where it stands.
        for &s in &ai.raiders.clone() {
            ai.raid_targets.insert(s, at(4, 4));
        }
        let mut events = Vec::new();
        run(&mut ai, &mut g, &mut events);
        let here = g
            .units()
            .unit_position(party[0])
            .ok_or("the party is on the map")?;
        assert!(here.y < 7, "it went towards (4, 4), not (4, 9): {here:?}");
        assert!(ai.raid_targets.values().all(|&t| t == at(4, 4)));
        Ok(())
    }

    #[test]
    fn a_party_whose_only_site_cannot_be_reached_goes_home_and_the_raid_ends() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        // A tower on the east island: in range, but over the sea from the party.
        spawn_tower(&mut g, at(30, 9), true)?;
        g.reveal_map(P0);
        let party = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 5);
        let mut ai = Ai::new(P0);
        let view = PlayerView::new(&g, P0);
        ai.raiders = party.iter().map(|&u| view.serial_of(u)).collect();
        let mut events = Vec::new();
        for _ in 0..4 {
            run(&mut ai, &mut g, &mut events);
            g.apply(Command::EndTurn);
        }
        assert!(ai.raid_targets.is_empty(), "no target it can walk to");
        assert!(
            ai.raiders.is_empty(),
            "the raid ended at the city: {:?}",
            ai.raiders
        );
        Ok(())
    }

    #[test]
    fn a_march_that_cannot_be_ordered_says_why() -> TestResult {
        use crate::assault::{march, MarchError};
        let mut g = game(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut g, at(30, 9), true)?;
        g.reveal_map(P0);
        let party = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 3);
        let ai = Ai::new(P0);
        let mut events = Vec::new();
        assert_eq!(
            march(&ai, &mut g, &party, at(30, 9), &mut events),
            Err(MarchError::Unreachable),
            "over the sea"
        );
        assert_eq!(
            march(&ai, &mut g, &[], at(30, 9), &mut events),
            Err(MarchError::NoUnits)
        );
        // A site on the party's own island can be marched on.
        let site = at(3, 3);
        spawn_site(&mut g, site, Vec::new(), Vec::new())?;
        assert_eq!(march(&ai, &mut g, &party, site, &mut events), Ok(()));
        Ok(())
    }

    #[test]
    fn a_weak_party_leaves_a_site_whose_guards_it_knows_to_be_too_strong() -> TestResult {
        let mut g = world(
            4,
            vec![(f4x_def::rules::UnitId::new(164) /* great_drake */, 2)],
            true,
        )?;
        let mut ai = Ai::new(P0);
        turns(&mut g, &mut ai, 4);
        assert!(ai.raiders.is_empty());
        assert!(g.economy().lair_at(at(8, 6)).is_some());
        assert!(all_home(&g));
        Ok(())
    }

    #[test]
    fn a_city_keeps_its_garrison_and_waits_for_a_party() -> TestResult {
        // One spare fighter beyond the single guard is not a party.
        let mut g = world(
            2,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        turns(&mut g, &mut ai, 4);
        assert!(ai.raiders.is_empty());
        assert!(g.economy().lair_at(at(8, 6)).is_some());
        Ok(())
    }

    #[test]
    fn the_survivors_come_home_after_the_raid() -> TestResult {
        let mut g = world(
            6,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        turns(&mut g, &mut ai, 14);
        assert!(
            g.economy().lair_at(at(8, 6)).is_none(),
            "the site is cleared"
        );
        assert!(ai.raiders.is_empty(), "{:?}", ai.raiders);
        assert!(all_home(&g));
        Ok(())
    }

    #[test]
    fn an_army_in_the_field_with_nothing_to_march_on_raids_the_nearest_tower() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut g, P0, at(11, 6), 113 /* high_men_paladins */, 6);
        spawn_tower(&mut g, at(8, 6), true)?;
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        let free: Vec<hecs::Entity> = g.units().player_units(P0);
        let mut events = Vec::new();
        assert!(deploy(&mut ai, &mut g, &free, &mut events), "{events:?}");
        assert_eq!(ai.raiders.len(), 6);
        Ok(())
    }
}
