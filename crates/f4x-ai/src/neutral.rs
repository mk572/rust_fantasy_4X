//! The independent cities. Each is its own player (the `Neutral` marker) and this plays them:
//! farmers, taxes and what the city builds, after the original's `Player_Colony_Autobuild_NP`
//! (`AIBUILD.c`): a Barracks first, then soldiers while the garrison is short and buildings
//! once it is not, never settlers. A city with a guarded site (tower, node or encounter site)
//! it has explored within [`RAID_RANGE`] squares also builds a raiding party of [`RAID_PARTY`]
//! beyond its garrison and sends it to assault the site, then home again (task 354; what a
//! winner may keep is `claim_lair`'s rule, task 353). They never attack anyone else or cast.
//! The game does none of this itself, so a host that ends turns must call [`play_neutrals`]
//! first, or the cities starve.

use f4x_core::city::autobuild::{BuildPlan, Focus, Mix};
use f4x_core::city::production::ProductionItem;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::Stat;

use crate::view::{distance, is_fighter, stack_strength};
use crate::Ai;

/// Soldiers wanted per four citizens, as many melee as ranged (the garrison a city starts with).
const GARRISON_PER_FOUR: usize = 2;

/// Squares from a city within which it looks for a site to raid.
pub const RAID_RANGE: i32 = 8;

/// Soldiers beyond the garrison that make a raiding party.
pub const RAID_PARTY: usize = 3;

/// Play every independent city's turn. Returns the events its commands caused.
pub fn play_neutrals(game: &mut Game) -> Vec<Event> {
    let cities: Vec<_> = game
        .player_ids()
        .into_iter()
        .filter(|id| game.is_neutral_player(*id))
        .collect();
    let mut events = Vec::new();
    for id in cities {
        let mut ai = Ai::new(id);
        game.balance_farmers(id);
        // Taxes are looked at only when the city is not paying its way: finding the lowest
        // rate that does costs a forecast per rate.
        if PlayerView::new(game, id).budget().net_gold() < 1 {
            crate::economy::taxes(&mut ai, game, &mut events);
        }
        raid(&ai, game, &mut events);
        production(&ai, game, &mut events);
    }
    events
}

/// A party must be this much stronger than guards it knows.
const RAID_EDGE: f32 = 1.5;

/// How strong the guards of a site are, as `unit_strength` measures units, when the player has
/// fought for the site once; `None` when it does not know them.
pub(crate) fn guard_strength(view: &PlayerView, site: Position) -> Option<f32> {
    let rules = &view.content().rules;
    let guards = view.lair_guards(site)?;
    Some(
        guards
            .iter()
            .filter_map(|(id, n)| {
                let def = rules.units.get(rules.units.id_of(id)?);
                let stat = |s: Stat| def.stats.get(&s).copied().unwrap_or(0).max(0) as f32;
                let attack = stat(Stat::Melee).max(stat(Stat::Ranged));
                let hits = stat(Stat::MaxHp) * stat(Stat::Figures).max(1.0);
                Some((attack * 1.5 + stat(Stat::Defense) + 1.0) * hits * *n as f32)
            })
            .sum(),
    )
}

/// The squares within [`RAID_RANGE`] of `from` that hold an explored, intact guarded site,
/// nearest first. With `party` (the strength of the raiders), a site whose guards the city
/// knows from an earlier fight and which are too strong for them is left out.
fn sites_near(view: &PlayerView, from: Position, party: Option<f32>) -> Vec<Position> {
    let mut found = Vec::new();
    for dy in -RAID_RANGE..=RAID_RANGE {
        for dx in -RAID_RANGE..=RAID_RANGE {
            if let Some(p) = view.shift(from, dx, dy) {
                let too_strong = party
                    .zip(guard_strength(view, p))
                    .is_some_and(|(mine, theirs)| mine < theirs * RAID_EDGE);
                if view.has_lair(p) && !too_strong && !found.contains(&p) {
                    found.push(p);
                }
            }
        }
    }
    found.sort_by_key(|&p| (distance(view, from, p), p.y, p.x));
    found
}

/// The eight squares round `at`.
pub(crate) fn around(view: &PlayerView, at: Position) -> Vec<Position> {
    let mut out = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            if (dx, dy) != (0, 0) {
                out.extend(view.shift(at, dx, dy));
            }
        }
    }
    out
}

/// The raiding parties of one city player: those out of the city assault the site next to them,
/// walk to the nearest one in range, or go home; the soldiers above the garrison leave in a
/// party when a site is in range.
fn raid(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let (homes, out, ready) = {
        let view = PlayerView::new(game, me);
        let mut homes: Vec<(hecs::Entity, Position)> = view.my_cities();
        homes.sort_by_key(|(e, _)| view.serial_of(*e));
        let home_squares: Vec<Position> = homes.iter().map(|&(_, p)| p).collect();
        let mut out: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
        for u in view.my_units() {
            let Some(pos) = view.unit_position(u).filter(|p| !home_squares.contains(p)) else {
                continue;
            };
            if !is_fighter(&view, u) {
                continue;
            }
            match out.iter_mut().find(|(p, _)| *p == pos) {
                Some((_, list)) => list.push(u),
                None => out.push((pos, vec![u])),
            }
        }
        out.sort_by_key(|(p, _)| (p.plane as u8, p.y, p.x));
        let mut ready: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
        for &(city, pos) in &homes {
            let Some(figures) = view.city_economy(city).map(|e| e.figures) else {
                continue;
            };
            let wanted = GARRISON_PER_FOUR * (figures as usize / 4).max(1);
            let mut soldiers: Vec<hecs::Entity> = view
                .garrison(pos)
                .into_iter()
                .filter(|&u| is_fighter(&view, u))
                .collect();
            if soldiers.len() >= wanted + RAID_PARTY {
                // The weakest stay (the stack is in the order the game lists it).
                let party = soldiers.split_off(wanted);
                let strength = stack_strength(&view, &party);
                if !sites_near(&view, pos, Some(strength)).is_empty() {
                    ready.push((pos, party));
                }
            }
        }
        (home_squares, out, ready)
    };
    for (pos, units) in out {
        let (next_to, target, nearest_home) = {
            let view = PlayerView::new(game, me);
            let next_to = around(&view, pos).into_iter().find(|&p| view.has_lair(p));
            let strength = stack_strength(&view, &units);
            let target = homes
                .iter()
                .flat_map(|&h| sites_near(&view, h, Some(strength)))
                .min_by_key(|&p| (distance(&view, pos, p), p.y, p.x));
            let nearest_home = homes
                .iter()
                .copied()
                .min_by_key(|&h| (distance(&view, pos, h), h.y, h.x));
            (next_to, target, nearest_home)
        };
        if let Some(site) = next_to {
            assault(ai, game, &units, site, events);
        } else if let Some(site) = target {
            march(ai, game, &units, site, events);
        } else if let Some(home) = nearest_home {
            ai.send(game, Command::GoTo { units, to: home }, events);
        }
    }
    for (pos, party) in ready {
        let site = {
            let view = PlayerView::new(game, me);
            let strength = stack_strength(&view, &party);
            sites_near(&view, pos, Some(strength)).into_iter().next()
        };
        if let Some(site) = site {
            march(ai, game, &party, site, events);
        }
    }
}

/// Send a party to the free square next to `site` that it can reach and is nearest to it.
pub(crate) fn march(
    ai: &Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) -> bool {
    let spots = {
        let view = PlayerView::new(game, ai.player);
        let from = view.unit_position(units[0]).unwrap_or(site);
        let mut spots: Vec<Position> = around(&view, site)
            .into_iter()
            .filter(|&p| view.hostile_at(p).is_none() && !view.has_lair(p))
            .filter(|&p| p == from || view.can_reach(units, p))
            .collect();
        spots.sort_by_key(|&p| (distance(&view, from, p), p.y, p.x));
        spots
    };
    for spot in spots {
        let moved = ai.send(
            game,
            Command::GoTo {
                units: units.to_vec(),
                to: spot,
            },
            events,
        );
        if moved {
            return true;
        }
    }
    false
}

/// Assault the site with the units that still have moves, and fight the battle out.
pub(crate) fn assault(
    ai: &Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) {
    let fighters: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, ai.player);
        units
            .iter()
            .copied()
            .filter(|&u| view.unit_moves_left(u) > 0)
            .collect()
    };
    if fighters.is_empty() {
        return;
    }
    let sent = ai.send(
        game,
        Command::EnterSite {
            units: fighters.clone(),
            to: site,
        },
        events,
    );
    if !sent {
        // A site nobody guards (the guards cannot be seen from outside) is not fought for: the
        // stack walks onto it and takes the treasure.
        ai.send(
            game,
            Command::GoTo {
                units: fighters,
                to: site,
            },
            events,
        );
        return;
    }
    // The battle AI plays both sides; a battle that stalls ends with the active side fleeing.
    for _ in 0..20_000 {
        let Some(command) = game.next_battle_command() else {
            break;
        };
        if !ai.send(game, Command::Battle(command), events) {
            break;
        }
    }
    while game.in_battle() {
        if !ai.send(
            game,
            Command::Battle(f4x_core::battle::BattleCommand::Flee),
            events,
        ) {
            break;
        }
    }
}

/// What each of the player's cities builds: the Barracks if it has none, else a plan of soldiers
/// while the garrison is short and the treasury can pay for one more, else of buildings.
fn production(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let cities = {
        let view = PlayerView::new(game, me);
        let mut mine = view.my_cities();
        mine.sort_by_key(|(e, _)| view.serial_of(*e));
        mine
    };
    for (city, pos) in cities {
        let (barracks, plan) = {
            let view = PlayerView::new(game, me);
            let Some(economy) = view.city_economy(city).filter(|e| e.figures > 0) else {
                continue;
            };
            let barracks = view
                .content()
                .rules
                .buildings
                .id_of("barracks")
                .map(ProductionItem::Building)
                .filter(|&item| view.can_build(city, item).is_ok())
                .filter(|&item| view.city_production(city).item != Some(item));
            let soldiers = view
                .garrison(pos)
                .into_iter()
                .filter(|&u| is_fighter(&view, u))
                .count();
            let party = if sites_near(&view, pos, None).is_empty() {
                0
            } else {
                RAID_PARTY
            };
            let wanted = GARRISON_PER_FOUR * (economy.figures as usize / 4).max(1) + party;
            let treasury = view.budget();
            let affordable = treasury.net_gold() >= 1 && treasury.food_surplus >= 1;
            let mix = if soldiers < wanted && affordable {
                Mix::Units
            } else {
                Mix::Buildings
            };
            let plan = BuildPlan {
                mix,
                primary: Focus::Defense,
                secondary: None,
                settlers: false,
                min_settler_pop: 3,
                event_prompt: false,
                eager_settlers: false,
            };
            (barracks, plan)
        };
        if game.autobuild_of(city).map(|a| a.plan) != Some(plan) {
            ai.send(
                game,
                Command::SetAutoBuild {
                    city,
                    plan: Some(plan),
                },
                events,
            );
        }
        if let Some(item) = barracks {
            ai.send(game, Command::SetProduction { city, item }, events);
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

    const TOWN: PlayerId = PlayerId(100);

    /// A world with an independent city at (4, 4), nine paladins in it and an encounter site
    /// of one guard at (6, 4).
    fn raiding_world(guards: Option<Vec<(String, u32)>>, known: bool) -> Game {
        let mut g = game(COAST_CITY, "high_men");
        g.add_neutral_player(TOWN);
        let race = g.content().rules.races.id_of("high_men");
        let events = g.apply(Command::FoundCity {
            owner: TOWN,
            pos: at(4, 4),
            name: "Free Town".into(),
            race,
            population: 8,
        });
        assert!(!events.iter().any(|e| matches!(e, Event::Rejected(_))));
        g.treasury_mut(TOWN).unwrap().gold = 1_000_000;
        spawn(&mut g, TOWN, at(4, 4), "high_men_paladins", 9);
        let feature = g.content().features.id_of("monster_lair").unwrap();
        let Some(guards) = guards else {
            return g;
        };
        g.spawn_feature(Placement {
            at: at(6, 4),
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
                fought_by: if known { vec![TOWN] } else { Vec::new() },
            }),
        });
        g
    }

    fn turns(g: &mut Game, n: usize) {
        for _ in 0..n {
            play_neutrals(g);
            g.apply(Command::EndTurn);
        }
    }

    #[test]
    fn a_city_with_soldiers_to_spare_clears_a_weak_site_and_keeps_the_gold() {
        let mut g = raiding_world(Some(vec![("zombies".into(), 1)]), false);
        let mut gold = 0;
        let mut cleared = false;
        for _ in 0..12 {
            for e in play_neutrals(&mut g) {
                if let Event::SiteCleared { player, loot, .. } = e {
                    assert_eq!(player, TOWN);
                    gold += loot.gold;
                    cleared = true;
                }
            }
            g.apply(Command::EndTurn);
        }
        assert!(cleared, "the site is cleared");
        assert_eq!(gold, 300, "the hoard pays out to the city");
        assert!(g.economy().lair_at(at(6, 4)).is_none());
        // What is left of the party comes home.
        let away = g
            .units()
            .player_units(TOWN)
            .iter()
            .filter(|&&u| g.units().unit_position(u) != Some(at(4, 4)))
            .count();
        assert_eq!(away, 0, "{away} soldiers are still out");
    }

    #[test]
    fn a_city_with_no_site_in_range_sends_nobody_out() {
        let mut g = raiding_world(None, false);
        turns(&mut g, 4);
        assert!(g
            .units()
            .player_units(TOWN)
            .iter()
            .all(|&u| g.units().unit_position(u) == Some(at(4, 4))));
    }

    #[test]
    fn a_city_that_knows_the_guards_leaves_a_site_too_strong_for_its_party() {
        // Two great drakes against a party of paladins; a fight the city lost earlier taught it
        // what stands there.
        let mut g = raiding_world(Some(vec![("great_drake".into(), 2)]), true);
        turns(&mut g, 4);
        assert!(g
            .units()
            .player_units(TOWN)
            .iter()
            .all(|&u| g.units().unit_position(u) == Some(at(4, 4))));
        assert!(g.economy().lair_at(at(6, 4)).is_some());
    }
}
