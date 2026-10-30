//! The independent cities. Each is its own player (the `Neutral` marker) and this plays them:
//! farmers, taxes and what the city builds, after the original's `Player_Colony_Autobuild_NP`
//! (`AIBUILD.c`): a Barracks first, then soldiers while the garrison is short and buildings
//! once it is not, never settlers. A city with a guarded site (tower, node or encounter site)
//! it has explored within [`RAID_RANGE`] squares also builds a raiding party of [`RAID_PARTY`]
//! beyond its garrison and sends it to assault the site, then home again (task 354; what a
//! winner may keep is `claim_lair`'s rule, task 353). They never attack anyone else or cast,
//! until one raises a raiding party (task 376): that city's player is then at war (see
//! [`Game::is_aggressive_player`]) and every soldier it can spare marches on the lairs and
//! cities near it, other independent cities and wizards' included. The monsters that roam the
//! map are played here too (`monster.rs`). The game does none of this itself, so a host that
//! ends turns must call [`play_neutrals`] (or [`play_neutrals_for`]) first, or the cities starve.

use f4x_core::city::autobuild::{BuildPlan, Focus, Mix};
use f4x_core::city::production::ProductionItem;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::rules::Stat;

use crate::military::expected_defence;
use crate::view::{distance, garrison_wanted, is_fighter, stack_strength};
use crate::{Ai, Attack};

/// Soldiers wanted per four citizens, as many melee as ranged (the garrison a city starts with).
const GARRISON_PER_FOUR: usize = 2;

/// Squares from a city within which it looks for a site to raid.
pub const RAID_RANGE: i32 = 8;

/// Soldiers beyond the garrison that make a raiding party.
pub const RAID_PARTY: usize = 3;

/// Squares from a city within which a city at war looks for lairs and cities to conquer.
pub const WAR_RANGE: i32 = 14;

/// A city at war attacks when its force is this much stronger than what it expects to meet.
const WAR_EDGE: f32 = 1.2;

/// Play every independent city's turn, and the roaming monsters'. Returns the events their
/// commands caused. Battles against anyone are fought out by the battle AI.
pub fn play_neutrals(game: &mut Game) -> Vec<Event> {
    play_neutrals_for(game, &[]).0
}

/// Like [`play_neutrals`], but the players in `defended_by` fight the battles against them
/// themselves: an attack on them is not resolved but returned, for the host to play out.
pub fn play_neutrals_for(game: &mut Game, defended_by: &[PlayerId]) -> (Vec<Event>, Vec<Attack>) {
    let cities: Vec<_> = game
        .player_ids()
        .into_iter()
        .filter(|id| game.is_neutral_player(*id))
        .collect();
    let mut events = Vec::new();
    let mut attacks = Vec::new();
    for id in cities {
        let mut ai = Ai::new(id);
        ai.leave_defence_to(defended_by);
        game.balance_farmers(id);
        // Taxes are looked at only when the city is not paying its way: finding the lowest
        // rate that does costs a forecast per rate.
        if PlayerView::new(game, id).budget().net_gold() < 1 {
            crate::economy::taxes(&mut ai, game, &mut events);
        }
        if game.is_aggressive_player(id) {
            war(&mut ai, game, &mut events);
        } else {
            raid(&ai, game, &mut events);
        }
        production(&ai, game, &mut events);
        attacks.extend(ai.take_attacks());
    }
    let (more, more_attacks) = crate::monster::roam(game, defended_by);
    events.extend(more);
    attacks.extend(more_attacks);
    (events, attacks)
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
    sites_within(view, from, party, RAID_RANGE)
}

/// [`sites_near`] with a range of the caller's choosing.
fn sites_within(
    view: &PlayerView,
    from: Position,
    party: Option<f32>,
    range: i32,
) -> Vec<Position> {
    let mut found = Vec::new();
    for dy in -range..=range {
        for dx in -range..=range {
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
            .filter(|&p| p != from && view.can_reach(units, p))
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

/// Fight the battle the AI means to fight: the human defends in person when `ai` leaves the
/// defence to them (the host plays it out), else the battle AI plays both sides.
pub(crate) fn strike(
    ai: &mut Ai,
    game: &mut Game,
    attackers: Vec<hecs::Entity>,
    defenders: Vec<hecs::Entity>,
    events: &mut Vec<Event>,
) {
    let in_person = defenders
        .first()
        .and_then(|&d| game.units().unit_owner(d))
        .is_some_and(|owner| ai.defended_by.contains(&owner));
    if in_person {
        ai.attacks.push(Attack {
            attackers,
            defenders,
        });
    } else {
        ai.send(
            game,
            Command::AutoResolve {
                mana_budget: None,
                attackers,
                defenders,
            },
            events,
        );
    }
}

/// What a stack of a city at war does about one square next to it.
enum Target {
    Nothing,
    /// A site to assault.
    Site,
    /// A city nobody defends: walk in.
    Empty,
    /// A city or stack that is defended, and how much it is worth.
    Defended(Vec<hecs::Entity>, f32),
}

/// A city at war: one unit scouts, the soldiers beyond each city's garrison hit what is next to
/// them when they are the stronger, and otherwise march on the nearest lair or city (within
/// [`WAR_RANGE`] of one of the player's cities) they are strong enough for, or go home.
fn war(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    crate::scout::scout(ai, game, events);
    let me = ai.player;
    let (homes, stacks) = {
        let view = PlayerView::new(game, me);
        let mut homes: Vec<(hecs::Entity, Position)> = view.my_cities();
        homes.sort_by_key(|(e, _)| view.serial_of(*e));
        let home_squares: Vec<Position> = homes.iter().map(|&(_, p)| p).collect();
        let mut by_square: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
        for u in view.my_units() {
            if !is_fighter(&view, u) || ai.scouts.contains(&view.serial_of(u)) {
                continue;
            }
            let Some(pos) = view.unit_position(u) else {
                continue;
            };
            match by_square.iter_mut().find(|(p, _)| *p == pos) {
                Some((_, list)) => list.push(u),
                None => by_square.push((pos, vec![u])),
            }
        }
        by_square.sort_by_key(|(p, _)| (p.plane as u8, p.y, p.x));
        let mut stacks = Vec::new();
        for (pos, mut units) in by_square {
            if home_squares.contains(&pos) {
                // The weakest stay at home (the stack is in the order the game lists it).
                let keep = garrison_wanted(&view, pos).min(units.len());
                units.drain(..keep);
            }
            if !units.is_empty() {
                stacks.push((pos, units));
            }
        }
        (home_squares, stacks)
    };
    for (pos, units) in stacks {
        let moving: Vec<hecs::Entity> = {
            let view = PlayerView::new(game, me);
            units
                .into_iter()
                .filter(|&u| view.unit_position(u) == Some(pos))
                .collect()
        };
        if moving.is_empty() {
            continue;
        }
        // 1. Hit what is next to us, when we are the stronger.
        let (mine, targets) = {
            let view = PlayerView::new(game, me);
            let turn = view.turn();
            let mine = stack_strength(&view, &moving);
            let targets: Vec<(Position, Target)> = around(&view, pos)
                .into_iter()
                .map(|n| {
                    let target = if view.has_lair(n) {
                        match guard_strength(&view, n) {
                            Some(theirs) if mine < theirs * WAR_EDGE => Target::Nothing,
                            _ => Target::Site,
                        }
                    } else {
                        match (view.city_at(n), view.hostile_at(n)) {
                            (Some(city), Some(defenders)) if view.city_owner(city) != Some(me) => {
                                if defenders.is_empty() {
                                    Target::Empty
                                } else {
                                    let theirs =
                                        expected_defence(&view, &Default::default(), turn, n);
                                    Target::Defended(defenders, theirs)
                                }
                            }
                            _ => Target::Nothing,
                        }
                    };
                    (n, target)
                })
                .collect();
            (mine, targets)
        };
        let mut acted = false;
        for (n, target) in targets {
            match target {
                Target::Nothing => {}
                Target::Site => {
                    assault(ai, game, &moving, n, events);
                    acted = true;
                }
                Target::Empty => {
                    acted = ai.send(
                        game,
                        Command::MoveStack {
                            units: moving.clone(),
                            to: n,
                        },
                        events,
                    );
                }
                Target::Defended(defenders, theirs) => {
                    if mine >= theirs * WAR_EDGE {
                        strike(ai, game, moving.clone(), defenders, events);
                        acted = true;
                    }
                }
            }
            if acted {
                break;
            }
        }
        if acted {
            continue;
        }
        // 2. March on the nearest target we are strong enough for, or go home.
        let (target, nearest_home) = {
            let view = PlayerView::new(game, me);
            let turn = view.turn();
            let mine = stack_strength(&view, &moving);
            let mut best: Option<(u32, Position)> = None;
            let mut consider = |p: Position| {
                let d = distance(&view, pos, p).unwrap_or(u32::MAX);
                if best.is_none_or(|(bd, bp)| (d, p.y, p.x) < (bd, bp.y, bp.x)) {
                    best = Some((d, p));
                }
            };
            for &h in &homes {
                for site in sites_within(&view, h, Some(mine), WAR_RANGE) {
                    consider(site);
                }
            }
            for (_, owner, city) in view.known_cities() {
                let near = owner != me
                    && homes
                        .iter()
                        .any(|&h| distance(&view, h, city).is_some_and(|d| d <= WAR_RANGE as u32));
                if near
                    && mine >= expected_defence(&view, &Default::default(), turn, city) * WAR_EDGE
                {
                    consider(city);
                }
            }
            let nearest_home = homes
                .iter()
                .copied()
                .min_by_key(|&h| (distance(&view, pos, h), h.y, h.x));
            (best.map(|(_, p)| p), nearest_home)
        };
        match (target, nearest_home) {
            (Some(target), _) => {
                march(ai, game, &moving, target, events);
            }
            (None, Some(home)) if home != pos => {
                ai.send(
                    game,
                    Command::GoTo {
                        units: moving,
                        to: home,
                    },
                    events,
                );
            }
            _ => {}
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
            let party = if game.is_aggressive_player(me) {
                RAID_PARTY * 2
            } else if sites_near(&view, pos, None).is_empty() {
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

    /// A world at war: the city's nine paladins are the free troops of a raid, which nobody
    /// feeds or pays.
    fn war_world() -> Game {
        let mut g = raiding_world(None, false);
        for unit in g.units().player_units(TOWN) {
            g.scenario_set_unpaid(unit, true);
        }
        g.scenario_set_aggressive(TOWN, true);
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

    #[test]
    fn a_city_at_war_sends_all_but_its_garrison_against_a_site_in_range() {
        // Nine paladins and a site in range: the peaceful city sends only what it has above
        // garrison and party; at war it sends everyone but the garrison.
        let mut g = raiding_world(Some(vec![("zombies".into(), 1)]), false);
        g.scenario_set_aggressive(TOWN, true);
        let mut cleared = false;
        for _ in 0..12 {
            for e in play_neutrals(&mut g) {
                if let Event::SiteCleared { player, .. } = e {
                    assert_eq!(player, TOWN);
                    cleared = true;
                }
            }
            g.apply(Command::EndTurn);
        }
        assert!(cleared, "the site is cleared");
    }

    #[test]
    fn a_city_at_war_takes_an_undefended_city_of_a_wizard() {
        let mut g = war_world();
        let target = g.economy().city_at(at(COAST_CITY.0, COAST_CITY.1)).unwrap();
        // The wizard's city has no defenders; the free town knows the land round it.
        g.explore(TOWN, at(COAST_CITY.0, COAST_CITY.1), 8);
        for _ in 0..15 {
            play_neutrals(&mut g);
            g.apply(Command::EndTurn);
            if g.economy().city_owner(target) == Some(TOWN) {
                break;
            }
        }
        assert_eq!(g.economy().city_owner(target), Some(TOWN));
    }

    #[test]
    fn a_peaceful_city_leaves_a_wizards_city_alone() {
        let mut g = raiding_world(None, false);
        let target = g.economy().city_at(at(COAST_CITY.0, COAST_CITY.1)).unwrap();
        g.explore(TOWN, at(COAST_CITY.0, COAST_CITY.1), 8);
        turns(&mut g, 10);
        assert_eq!(g.economy().city_owner(target), Some(crate::testworld::P0));
    }

    #[test]
    fn an_attack_on_a_player_who_defends_in_person_is_kept_for_the_host() {
        let mut g = war_world();
        spawn(
            &mut g,
            crate::testworld::P0,
            at(COAST_CITY.0, COAST_CITY.1),
            "high_men_spearmen",
            1,
        );
        g.explore(TOWN, at(COAST_CITY.0, COAST_CITY.1), 8);
        let mut kept = Vec::new();
        for _ in 0..15 {
            let (_, attacks) = play_neutrals_for(&mut g, &[crate::testworld::P0]);
            kept.extend(attacks);
            g.balance_farmers(crate::testworld::P0);
            g.apply(Command::EndTurn);
            if !kept.is_empty() {
                break;
            }
        }
        assert!(!kept.is_empty(), "the attack waits for the human");
        assert!(!g.in_battle());
    }
}
