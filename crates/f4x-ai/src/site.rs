//! Towers, magic nodes and encounter sites: a wizard's spare soldiers clear the guarded sites
//! near their cities for the treasure and the nodes (task 356). The AI plays by the human's
//! rules (`design.md` section 8): it knows a site once it has explored it, knows what guards it
//! only from a fight it fought or a spell that shows it (`PlayerView::lair_guards`), guesses
//! otherwise, and sends a party through `Command::EnterSite` with the battle AI fighting. What it
//! wins is spent by the AI it already has: items are equipped (`offer::equip`), the freed
//! prisoner is hired from the offers, gold and mana go to the plans.

use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::military::average_strength;
use crate::neutral::{around, assault, guard_strength, march};
use crate::view::{distance, garrison_wanted, is_fighter, stack_strength};
use crate::Ai;

/// Squares from a city (or from a party in the field) within which a site is looked for.
const SITE_RANGE: i32 = 10;
/// The fewest spare soldiers that make a party.
const SITE_PARTY: usize = 3;
/// What an unknown site's guards are guessed to be worth, in average soldiers of this wizard.
const GUESS_GUARDS: f32 = 2.5;

/// A party attacks when it is this much stronger than the guards, known or guessed.
fn edge() -> f32 {
    crate::knob("SITE", 1.4)
}

/// What the AI expects the site's guards to be worth.
fn expected(view: &PlayerView, site: Position) -> f32 {
    guard_strength(view, site).unwrap_or_else(|| GUESS_GUARDS * average_strength(view))
}

/// The explored, intact sites within [`SITE_RANGE`] of `from` that a party of `strength` should
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
        let (next_to, target, home) = {
            let view = PlayerView::new(game, me);
            let strength = stack_strength(&view, &units);
            let next_to = around(&view, pos)
                .into_iter()
                .find(|&p| view.has_lair(p) && strength >= expected(&view, p) * edge());
            let target = sites_near(&view, pos, strength).into_iter().next();
            let home = view
                .my_cities()
                .into_iter()
                .map(|(_, p)| p)
                .min_by_key(|&p| (distance(&view, pos, p), p.y, p.x));
            (next_to, target, home)
        };
        if let Some(site) = next_to {
            assault(ai, game, &units, site, events);
        } else if let Some(site) = target {
            march(ai, game, &units, site, events);
        } else if let Some(home) = home {
            ai.send(game, Command::GoTo { units, to: home }, events);
        }
    }
    // A new party leaves a city with soldiers to spare, one raid at a time and never while the
    // army marches (its campaign comes first).
    if !ai.raiders.is_empty() || !ai.marching.is_empty() {
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
            let spare: Vec<hecs::Entity> = view
                .garrison(pos)
                .into_iter()
                .filter(|&u| is_fighter(&view, u) && !ai.scouts.contains(&view.serial_of(u)))
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
            if march(ai, game, &party, site, events) {
                let view = PlayerView::new(game, me);
                ai.raiders.extend(party.iter().map(|&u| view.serial_of(u)));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY};
    use f4x_core::feature::entity::Placement;
    use f4x_core::feature::lair::Lair;
    use f4x_core::player::PlayerId;

    const P0: PlayerId = PlayerId(0);

    /// The west wizard with `paladins` in its city at (10, 6) and an encounter site of the
    /// given guards two squares west. `known`: the wizard has fought for the site before.
    fn world(paladins: usize, guards: Vec<(String, u32)>, known: bool) -> Game {
        let mut g = game(COAST_CITY, "high_men");
        g.treasury_mut(P0).unwrap().gold = 1_000_000;
        spawn(&mut g, P0, at(10, 6), "high_men_paladins", paladins);
        let feature = g.content().features.id_of("monster_lair").unwrap();
        g.spawn_feature(Placement {
            at: at(8, 6),
            feature,
            node: None,
            lair: Some(Lair {
                intact: true,
                guards,
                gold: 300,
                mana: 0,
                items: Vec::new(),
                prisoner: false,
                special: 0,
                fought_by: if known { vec![P0] } else { Vec::new() },
            }),
        });
        g
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
    fn a_strong_party_next_to_an_encounter_site_clears_it() {
        let mut g = world(5, vec![("zombies".into(), 1)], false);
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
    }

    #[test]
    fn a_weak_party_leaves_a_site_whose_guards_it_knows_to_be_too_strong() {
        let mut g = world(4, vec![("great_drake".into(), 2)], true);
        let mut ai = Ai::new(P0);
        turns(&mut g, &mut ai, 4);
        assert!(ai.raiders.is_empty());
        assert!(g.economy().lair_at(at(8, 6)).is_some());
        assert!(g
            .units()
            .player_units(P0)
            .iter()
            .all(|&u| g.units().unit_position(u) == Some(at(10, 6))));
    }

    #[test]
    fn a_city_keeps_its_garrison_and_waits_for_a_party() {
        // One spare fighter beyond the single guard is not a party.
        let mut g = world(2, vec![("zombies".into(), 1)], false);
        let mut ai = Ai::new(P0);
        turns(&mut g, &mut ai, 4);
        assert!(ai.raiders.is_empty());
        assert!(g.economy().lair_at(at(8, 6)).is_some());
    }

    #[test]
    fn the_survivors_come_home_after_the_raid() {
        let mut g = world(6, vec![("zombies".into(), 1)], false);
        let mut ai = Ai::new(P0);
        turns(&mut g, &mut ai, 14);
        assert!(
            g.economy().lair_at(at(8, 6)).is_none(),
            "the site is cleared"
        );
        assert!(ai.raiders.is_empty(), "{:?}", ai.raiders);
        assert!(g
            .units()
            .player_units(P0)
            .iter()
            .all(|&u| g.units().unit_position(u) == Some(at(10, 6))));
    }
}
