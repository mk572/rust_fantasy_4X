//! Ships: how the AI reaches an enemy city on another land mass (`docs/ai-design.md` 11.3). A
//! plan names the target, a landing (a land square and a sea square beside it) and the port to
//! sail from.

use f4x_core::city::production::{ProductionError, ProductionItem};
use f4x_core::diplomacy::{DiplomacyCommand, Treaty};
use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::Stat;

use crate::view::{garrison_wanted, is_coastal, is_fighter, is_ship};
use crate::Ai;

/// Fighters that must stand in the port before a ship sails.
pub(crate) const MIN_LOAD: usize = 3;
/// The edge a landing party wants over the garrison it expects, and how many turns it waits
/// for it before sailing anyway.
const LANDING_EDGE: f32 = 1.2;
const WAIT_TURNS: u32 = 25;

/// Where a landing goes, and where it starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// The enemy city.
    pub target: Position,
    /// The land square beside it where the troops step ashore.
    pub land: Position,
    /// The sea square beside that, where the ship waits.
    pub sea: Position,
    /// The own city the ships sail from.
    pub port: Position,
}

/// A ship on a voyage and the units it carries.
#[derive(Debug, Clone)]
pub struct Ferry {
    pub ship: Serial,
    /// The units aboard; empty once they have landed.
    pub group: Vec<Serial>,
}

/// The ship, or the building towards one, for the port to build: a galley (five units) when it
/// can, a trireme or its guild first when the ship is not for an army (`big` false), the buildings that lead to one when the race may have them, a trireme (two) when it may
/// not.
pub fn port_item(view: &PlayerView, city: hecs::Entity, big: bool) -> Option<ProductionItem> {
    let rules = &view.content().rules;
    let unit = |id: &str| {
        rules
            .units
            .id_of(id)
            .map(ProductionItem::Unit)
            .filter(|&i| view.can_build(city, i).is_ok())
    };
    let building = |id: &str| rules.buildings.id_of(id).map(ProductionItem::Building);
    if let Some(galley) = unit("galley") {
        return Some(galley);
    }
    // A ship that only scouts or carries a settler or two is not worth the yard's chain.
    if !big {
        if let Some(trireme) = unit("trireme") {
            return Some(trireme);
        }
        if let Some(guild) =
            building("ship_wrights_guild").filter(|&i| view.can_build(city, i).is_ok())
        {
            return Some(guild);
        }
    }
    let yard = building("ship_yard");
    let yard_possible = yard.is_some_and(|y| {
        !matches!(view.can_build(city, y), Err(ProductionError::WrongRace))
            && !matches!(view.can_build(city, y), Err(ProductionError::NotCoastal))
    });
    if yard_possible {
        for id in ["ship_yard", "ship_wrights_guild", "sawmill"] {
            if let Some(item) = building(id).filter(|&i| view.can_build(city, i).is_ok()) {
                return Some(item);
            }
        }
    }
    if let Some(trireme) = unit("trireme") {
        return Some(trireme);
    }
    building("ship_wrights_guild").filter(|&i| view.can_build(city, i).is_ok())
}

/// The turn the game is at (for the age of what the AI remembers).
fn view_turn(view: &PlayerView) -> u32 {
    view.turn()
}

/// The AI's ships, oldest first.
fn my_ships(view: &PlayerView) -> Vec<hecs::Entity> {
    let mut ships: Vec<hecs::Entity> = view
        .my_units()
        .into_iter()
        .filter(|&u| is_ship(view, u))
        .collect();
    view.sort_by_serial(&mut ships);
    ships
}

/// A landing for `city`: the land square nearest to it (by steps over land) that touches the sea,
/// and a sea square beside that. The troops walk the rest of the way. `None` if no coast is
/// known on its land mass.
fn landing(view: &PlayerView, city: Position) -> Option<(Position, Position)> {
    use f4x_core::map::terrain::TerrainClass;
    use std::collections::{HashSet, VecDeque};
    let mut seen: HashSet<Position> = HashSet::new();
    let mut queue: VecDeque<Position> = VecDeque::new();
    seen.insert(city);
    queue.push_back(city);
    while let Some(here) = queue.pop_front() {
        if seen.len() > 900 {
            break;
        }
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let Some(next) = view.shift(here, dx, dy) else {
                    continue;
                };
                if !seen.insert(next) {
                    continue;
                }
                if matches!(view.terrain_class(next), Some(TerrainClass::Ocean) | None) {
                    continue;
                }
                queue.push_back(next);
                if view.hostile_at(next).is_some() || view.has_lair(next) || view.in_node(next) {
                    continue;
                }
                for sy in -1i32..=1 {
                    for sx in -1i32..=1 {
                        let Some(sea) = view.shift(next, sx, sy) else {
                            continue;
                        };
                        if view.terrain_class(sea) == Some(TerrainClass::Ocean)
                            && view.hostile_at(sea).is_none()
                        {
                            return Some((next, sea));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Whether the plan's target is a tower, not a city.
fn is_tower(view: &PlayerView, target: Position) -> bool {
    view.city_at(target).is_none() && view.towers().iter().any(|&(p, _)| p == target)
}

/// A plan against the nearest guarded tower that cannot be reached on foot, when the way to the
/// other plane is what the wizard lacks: no rival is known and the ground across the tower is
/// unexplored.
fn choose_tower_plan(ai: &Ai, view: &PlayerView, coastal: &[Position]) -> Option<Plan> {
    let nothing_known = !view
        .known_cities()
        .iter()
        .any(|&(_, o, _)| view.is_rival_wizard(o));
    if !nothing_known {
        return None;
    }
    let mut towers: Vec<Position> = view
        .towers()
        .into_iter()
        .filter(|&(p, intact)| {
            intact && !ai.naval_bad.contains(&p) && crate::scout::worth_crossing(view, p)
        })
        .map(|(p, _)| p)
        .collect();
    towers.sort_by_key(|p| (p.plane as u8, p.y, p.x));
    for target in towers {
        // A tower a soldier can walk to is cleared on foot.
        let walkable = view.my_cities().into_iter().any(|(_, city)| {
            view.garrison(city)
                .into_iter()
                .find(|&u| is_fighter(view, u))
                .is_some_and(|u| {
                    crate::neutral::around(view, target)
                        .into_iter()
                        .any(|p| view.hostile_at(p).is_none() && view.can_reach(&[u], p))
                })
        });
        if walkable {
            continue;
        }
        let Some((land, sea)) = landing(view, target) else {
            continue;
        };
        let port = coastal
            .iter()
            .copied()
            .filter(|p| p.plane == sea.plane)
            .min_by_key(|&p| (view.distance(p, sea).unwrap_or(u32::MAX), p.y, p.x));
        if let Some(port) = port {
            return Some(Plan {
                target,
                land,
                sea,
                port,
            });
        }
    }
    None
}

/// A plan against the nearest enemy city that cannot be reached on foot.
fn choose_plan(ai: &Ai, view: &PlayerView) -> Option<Plan> {
    // Real cities only: an outpost builds nothing but housing.
    let coastal: Vec<Position> = view
        .my_cities()
        .into_iter()
        .filter(|&(c, p)| {
            is_coastal(view, p)
                && !ai.naval_bad.contains(&p)
                && view.city_economy(c).is_some_and(|e| e.figures > 0)
        })
        .map(|(_, p)| p)
        .collect();
    let mut targets: Vec<Position> = ai
        .unreachable
        .iter()
        .copied()
        .filter(|t| !ai.naval_bad.contains(t))
        .collect();
    // The lightest held first: a landing of a few units needs a weak garrison.
    targets.sort_by(|a, b| {
        let held = |t: &Position| {
            crate::military::expected_defence(view, &ai.city_memory, view_turn(view), *t)
        };
        held(a)
            .total_cmp(&held(b))
            .then((a.plane as u8, a.y, a.x).cmp(&(b.plane as u8, b.y, b.x)))
    });
    for target in targets {
        let owner = view
            .city_at(target)
            .and_then(|c| view.city_owner(c))
            .filter(|&o| o != view.me());
        if owner.is_none() {
            continue;
        }
        let Some((land, sea)) = landing(view, target) else {
            continue;
        };
        let port = coastal
            .iter()
            .copied()
            .filter(|p| p.plane == sea.plane)
            .min_by_key(|&p| (view.distance(p, sea).unwrap_or(u32::MAX), p.y, p.x));
        if let Some(port) = port {
            return Some(Plan {
                target,
                land,
                sea,
                port,
            });
        }
    }
    choose_tower_plan(ai, view, &coastal)
}

/// Plan, build, load, sail, land.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // Forget a plan whose target is no longer an enemy's or whose port is gone.
    {
        let view = PlayerView::new(game, me);
        if let Some(plan) = ai.naval {
            let enemy = if is_tower(&view, plan.target) {
                view.towers()
                    .iter()
                    .any(|&(p, intact)| p == plan.target && intact)
            } else {
                view.city_at(plan.target)
                    .and_then(|c| view.city_owner(c))
                    .is_some_and(|o| o != me)
            };
            let port = view.city_at(plan.port).and_then(|c| view.city_owner(c)) == Some(me);
            if !enemy || !port {
                ai.naval = None;
                ai.ferry = None;
            }
        }
        if ai.naval.is_none() && ai.campaign.is_none() {
            ai.naval = choose_plan(ai, &view);
        }
    }
    sea_scout(ai, game);
    let Some(plan) = ai.naval else {
        return;
    };
    // The only ship is carrying settlers (`colony.rs`): a landing waits for it.
    if ai.colony.is_some() {
        return;
    }
    // The sea scout has found its enemy: the ship now carries the landing.
    let scouting: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        my_ships(&view)
            .into_iter()
            .filter(|&s| ai.scouts.contains(&view.serial_of(s)))
            .collect()
    };
    if !scouting.is_empty() {
        let serials: Vec<Serial> = {
            let view = PlayerView::new(game, me);
            scouting.iter().map(|&s| view.serial_of(s)).collect()
        };
        ai.scouts.retain(|s| !serials.contains(s));
        ai.sea_party.clear();
        ai.send(game, Command::ClearOrders { units: scouting }, events);
    }
    let (ships, ship_here, serial_ship) = {
        let view = PlayerView::new(game, me);
        let ships = my_ships(&view);
        let ship_here = ships
            .iter()
            .copied()
            .find(|&s| view.unit_position(s) == Some(plan.port));
        let serial_ship = ai
            .ferry
            .as_ref()
            .and_then(|f| ships.iter().copied().find(|&s| view.serial_of(s) == f.ship));
        (ships, ship_here, serial_ship)
    };
    if ships.is_empty() {
        return;
    }
    match (ai.ferry.clone(), serial_ship) {
        (None, _) => load_and_sail(ai, game, plan, ship_here, events),
        (Some(_), None) => ai.ferry = None,
        (Some(ferry), Some(ship)) => voyage(ai, game, plan, ferry, ship, events),
    }
}

/// With a ship in port and enough fighters there, sail.
fn load_and_sail(
    ai: &mut Ai,
    game: &mut Game,
    plan: Plan,
    ship: Option<hecs::Entity>,
    events: &mut Vec<Event>,
) {
    let me = ai.player;
    let Some(ship) = ship else {
        // A ship that is somewhere else goes back to the port.
        let away = {
            let view = PlayerView::new(game, me);
            my_ships(&view)
                .into_iter()
                .find(|&s| !view.unit_has_order(s))
        };
        if let Some(s) = away {
            if !PlayerView::new(game, me).can_reach(&[s], plan.port) {
                // This ship cannot sail to the port (another sea): give the port up.
                ai.naval_bad.push(plan.port);
                ai.naval = None;
                return;
            }
            ai.send(
                game,
                Command::GoTo {
                    units: vec![s],
                    to: plan.port,
                },
                events,
            );
        }
        return;
    };
    let (group, capacity) = {
        let view = PlayerView::new(game, me);
        let capacity = view.unit_stat(ship, Stat::Transport).max(0) as usize;
        let mut here: Vec<hecs::Entity> = view
            .garrison(plan.port)
            .into_iter()
            .filter(|&u| is_fighter(&view, u) && !ai.scouts.contains(&view.serial_of(u)))
            .collect();
        view.sort_by_serial(&mut here);
        // The city keeps its own garrison.
        let keep = garrison_wanted(&view, plan.port);
        let group: Vec<hecs::Entity> = here.into_iter().skip(keep).take(capacity).collect();
        (group, capacity)
    };
    if group.len() < MIN_LOAD.min(capacity) {
        return;
    }
    // Strong enough for the garrison it will meet, or tired of waiting.
    {
        let view = PlayerView::new(game, me);
        let strength = crate::view::stack_strength(&view, &group);
        let expected = if is_tower(&view, plan.target) {
            crate::site::expected(&view, plan.target)
        } else {
            crate::military::expected_defence(&view, &ai.city_memory, game.turn, plan.target)
        };
        if strength * LANDING_EDGE < expected {
            ai.naval_wait += 1;
            if ai.naval_wait < WAIT_TURNS {
                return;
            }
        }
        ai.naval_wait = 0;
    }
    if !is_tower(&PlayerView::new(game, me), plan.target)
        && !declare_war_on_target(ai, game, plan, events)
    {
        ai.naval_bad.push(plan.target);
        ai.naval = None;
        return;
    }
    let stack: Vec<hecs::Entity> = std::iter::once(ship).chain(group.iter().copied()).collect();
    if !PlayerView::new(game, me).can_reach(&stack, plan.sea) {
        // The ship cannot get there from this port: try another port or target.
        ai.naval_bad.push(plan.port);
        ai.naval = None;
        return;
    }
    let (ship_serial, serials) = {
        let view = PlayerView::new(game, me);
        (
            view.serial_of(ship),
            group.iter().map(|&u| view.serial_of(u)).collect::<Vec<_>>(),
        )
    };
    if ai.send(
        game,
        Command::GoTo {
            units: stack,
            to: plan.sea,
        },
        events,
    ) {
        ai.ferry = Some(Ferry {
            ship: ship_serial,
            group: serials,
        });
    }
}

/// The landing is an attack, so the war comes before the ship sails: a wizard with no treaty is
/// declared war on (and the armies go for its cities from then on). Returns `false` when the owner
/// is bound to the AI by a pact or an alliance, which it does not break for a landing.
fn declare_war_on_target(
    ai: &mut Ai,
    game: &mut Game,
    plan: Plan,
    events: &mut Vec<Event>,
) -> bool {
    let me = ai.player;
    let owner = {
        let view = PlayerView::new(game, me);
        view.city_at(plan.target)
            .and_then(|c| view.city_owner(c))
            .filter(|&o| view.is_rival_wizard(o))
    };
    let Some(owner) = owner else {
        return true;
    };
    match game.treaty(me, owner) {
        Treaty::War => true,
        Treaty::None => {
            if ai.send(
                game,
                Command::Diplomacy(DiplomacyCommand::DeclareWar {
                    player: me,
                    on: owner,
                }),
                events,
            ) {
                ai.quarrel = Some(owner);
            }
            true
        }
        Treaty::Pact | Treaty::Alliance => false,
    }
}

/// A ship at sea: keep it sailing, land the troops when it is there, send it home after.
fn voyage(
    ai: &mut Ai,
    game: &mut Game,
    plan: Plan,
    ferry: Ferry,
    ship: hecs::Entity,
    events: &mut Vec<Event>,
) {
    let me = ai.player;
    let (at, idle, group) = {
        let view = PlayerView::new(game, me);
        let group: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| ferry.group.contains(&view.serial_of(u)))
            .collect();
        (view.unit_position(ship), !view.unit_has_order(ship), group)
    };
    let Some(at) = at else {
        ai.ferry = None;
        return;
    };
    if !ferry.group.is_empty() {
        // Soldiers left behind on the way (a stop in a city) cannot be put ashore: the voyage is
        // given up and the target with it.
        let aboard = {
            let view = PlayerView::new(game, me);
            group
                .iter()
                .filter(|&&u| view.unit_position(u) == Some(at))
                .count()
        };
        if aboard == 0 && !group.is_empty() {
            ai.naval_bad.push(plan.target);
            ai.naval = None;
            ai.ferry = None;
            return;
        }
        if at == plan.sea {
            // Step ashore: onto the planned square, else any land square next to both the ship and
            // the city.
            if !group.is_empty() {
                let squares = {
                    let view = PlayerView::new(game, me);
                    let mut squares = vec![plan.land];
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            if let Some(p) = view.shift(plan.sea, dx, dy) {
                                if p != plan.land
                                    && view.terrain_class(p)
                                        != Some(f4x_core::map::terrain::TerrainClass::Ocean)
                                    && p != plan.target
                                {
                                    squares.push(p);
                                }
                            }
                        }
                    }
                    squares
                };
                let landed = squares.into_iter().any(|to| {
                    ai.send(
                        game,
                        Command::GoTo {
                            units: group.clone(),
                            to,
                        },
                        events,
                    )
                });
                if !landed {
                    return;
                }
            }
            // Soldiers put ashore at a tower go on to clear it.
            if is_tower(&PlayerView::new(game, me), plan.target) {
                ai.raiders.extend(ferry.group.iter().copied());
            }
            if let Some(f) = ai.ferry.as_mut() {
                f.group.clear();
            }
        } else if idle {
            // Held up (a blocked path, no moves left): sail on.
            let stack: Vec<hecs::Entity> =
                std::iter::once(ship).chain(group.iter().copied()).collect();
            ai.send(
                game,
                Command::GoTo {
                    units: stack,
                    to: plan.sea,
                },
                events,
            );
        }
        return;
    }
    // Empty ship: home again, and the voyage is over when it is in port.
    if at == plan.port {
        ai.ferry = None;
    } else if idle {
        make_room(ai, game, plan.port, events);
        ai.send(
            game,
            Command::GoTo {
                units: vec![ship],
                to: plan.port,
            },
            events,
        );
    }
}

/// A ship cannot enter a city holding a full tile: the last fighter of the port steps out to a
/// square beside it, to make room.
fn make_room(ai: &mut Ai, game: &mut Game, port: Position, events: &mut Vec<Event>) {
    let me = ai.player;
    let (unit, to) = {
        let view = PlayerView::new(game, me);
        let mut here = view.garrison(port);
        if here.len() < view.max_tile() {
            return;
        }
        view.sort_by_serial(&mut here);
        let Some(&unit) = here.iter().rev().find(|&&u| is_fighter(&view, u)) else {
            return;
        };
        let to = (-1i32..=1)
            .flat_map(|dy| (-1i32..=1).map(move |dx| (dx, dy)))
            .filter_map(|(dx, dy)| view.shift(port, dx, dy))
            .filter(|&p| {
                p != port
                    && view.terrain_class(p) != Some(f4x_core::map::terrain::TerrainClass::Ocean)
                    && view.hostile_at(p).is_none()
                    && view.can_reach(&[unit], p)
            })
            .min_by_key(|p| (p.y, p.x));
        let Some(to) = to else {
            return;
        };
        (unit, to)
    };
    ai.send(
        game,
        Command::GoTo {
            units: vec![unit],
            to,
        },
        events,
    );
}

/// With every walkable square explored and no enemy city known, a ship looks for the rest of the
/// world: the first coastal city builds one, and it scouts like any scout.
fn sea_scout(ai: &mut Ai, game: &mut Game) {
    let me = ai.player;
    let view = PlayerView::new(game, me);
    let nothing_known = !view
        .known_cities()
        .iter()
        .any(|&(_, o, _)| view.is_rival_wizard(o));
    if ai.naval.is_some()
        || ai.colony.is_some()
        || ai.campaign.is_some()
        || !ai.frontier_closed
        || !nothing_known
        || ai.sea_done
    {
        ai.sea_port = None;
        return;
    }
    // A city that can build the ship (the first, oldest, real coastal city).
    if ai.sea_port.is_none() {
        ai.sea_port = view
            .my_cities()
            .into_iter()
            .find(|&(c, p)| {
                is_coastal(&view, p) && view.city_economy(c).is_some_and(|e| e.figures > 0)
            })
            .map(|(_, p)| p);
    }
    if let Some(ship) = my_ships(&view).into_iter().next() {
        let serial = view.serial_of(ship);
        if !ai.scouts.contains(&serial) {
            ai.scouts.push(serial);
        }
        ai.frontier_closed = false;
        ai.sea_scouting = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{
        at, build, game, spawn, spawn_tower, COAST_CITY, ENEMY_CITY, INLAND_CITY, P0, P1,
    };
    use f4x_core::map::terrain::TerrainClass;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn port() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    fn enemy() -> Position {
        at(ENEMY_CITY.0, ENEMY_CITY.1)
    }

    /// The id of what the port would build.
    fn port_builds(game: &Game) -> Option<String> {
        let view = PlayerView::new(game, P0);
        let city = view.city_at(port())?;
        let rules = &view.content().rules;
        Some(match port_item(&view, city, true)? {
            ProductionItem::Building(b) => rules.buildings.get(b).id.clone(),
            ProductionItem::Unit(u) => rules.units.get(u).id.clone(),
            other => format!("{other:?}"),
        })
    }

    #[test]
    fn a_race_with_a_ship_yard_works_up_to_a_galley() {
        let mut g = game(COAST_CITY, "high_men");
        assert_eq!(port_builds(&g).as_deref(), Some("ship_wrights_guild"));
        build(&mut g, port(), &["ship_wrights_guild"]);
        assert_eq!(port_builds(&g).as_deref(), Some("sawmill"));
        build(&mut g, port(), &["sawmill"]);
        assert_eq!(port_builds(&g).as_deref(), Some("ship_yard"));
        build(&mut g, port(), &["ship_yard"]);
        assert_eq!(port_builds(&g).as_deref(), Some("galley"));
    }

    #[test]
    fn a_race_without_a_ship_yard_stops_at_the_trireme() {
        // Beastmen may build a Ship Wrights Guild but no Ship Yard.
        let mut g = game(COAST_CITY, "beastmen");
        assert_eq!(port_builds(&g).as_deref(), Some("ship_wrights_guild"));
        build(&mut g, port(), &["ship_wrights_guild"]);
        assert_eq!(port_builds(&g).as_deref(), Some("trireme"));
    }

    #[test]
    fn a_city_away_from_the_sea_is_never_offered_a_ship() -> TestResult {
        // With every ship building in place, an inland city still cannot build a ship.
        let mut g = game(INLAND_CITY, "high_men");
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        build(
            &mut g,
            inland,
            &["ship_wrights_guild", "sawmill", "ship_yard"],
        );
        let view = PlayerView::new(&g, P0);
        let city = view.city_at(inland).ok_or("the city is there")?;
        assert_eq!(port_item(&view, city, true), None);
        Ok(())
    }

    #[test]
    fn a_landing_is_a_shore_square_beside_the_city_and_the_sea_beside_that() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.reveal_map(P0);
        let view = PlayerView::new(&g, P0);
        let (land, sea) = landing(&view, enemy()).ok_or("a landing")?;
        assert_ne!(land, enemy());
        assert_ne!(view.terrain_class(land), Some(TerrainClass::Ocean));
        assert_eq!(view.terrain_class(sea), Some(TerrainClass::Ocean));
        assert_eq!(view.distance(land, sea), Some(1));
        assert!(view.distance(land, enemy()).is_some_and(|d| d <= 2));
        Ok(())
    }

    #[test]
    fn no_landing_is_known_for_ground_the_wizard_has_not_seen() {
        let g = game(COAST_CITY, "high_men");
        let view = PlayerView::new(&g, P0);
        assert_eq!(landing(&view, enemy()), None);
    }

    #[test]
    fn the_plan_sails_from_the_coastal_city_to_a_landing_by_the_enemy() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.unreachable.push(enemy());
        let view = PlayerView::new(&g, P0);
        let plan = choose_plan(&ai, &view).ok_or("a plan")?;
        assert_eq!(plan.target, enemy());
        assert_eq!(plan.port, port());
        assert_eq!(view.terrain_class(plan.sea), Some(TerrainClass::Ocean));
        assert_ne!(view.terrain_class(plan.land), Some(TerrainClass::Ocean));
        Ok(())
    }

    #[test]
    fn there_is_no_plan_without_a_target_a_coast_or_after_a_failure() {
        let mut g = game(COAST_CITY, "high_men");
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        // Nothing is unreachable by land.
        assert_eq!(choose_plan(&ai, &PlayerView::new(&g, P0)), None);
        // A target already given up on.
        ai.unreachable.push(enemy());
        ai.naval_bad.push(enemy());
        assert_eq!(choose_plan(&ai, &PlayerView::new(&g, P0)), None);
        // A port already given up on.
        ai.naval_bad = vec![port()];
        assert_eq!(choose_plan(&ai, &PlayerView::new(&g, P0)), None);
        // No coastal city at all.
        let mut inland = game(INLAND_CITY, "high_men");
        inland.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.unreachable.push(enemy());
        assert_eq!(choose_plan(&ai, &PlayerView::new(&inland, P0)), None);
    }

    #[test]
    fn a_ship_that_is_away_when_the_port_is_ready_is_called_home() {
        let mut g = game(COAST_CITY, "high_men");
        g.reveal_map(P0);
        let far = at(15, 6);
        let ship = spawn(&mut g, P0, far, "galley", 1)[0];
        let mut ai = Ai::new(P0);
        ai.unreachable.push(enemy());
        run(&mut ai, &mut g, &mut Vec::new());
        assert!(PlayerView::new(&g, P0).unit_has_order(ship));
    }

    #[test]
    fn a_galley_ferries_fighters_to_the_enemy_island() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        build(
            &mut g,
            port(),
            &["ship_wrights_guild", "sawmill", "ship_yard"],
        );
        g.reveal_map(P0);
        // Upkeep must not interfere.
        g.treasury_mut(P0).ok_or("player 0 has a treasury")?.gold = 1_000_000;
        spawn(&mut g, P0, port(), "galley", 1);
        // One stays as the garrison; four go.
        let army = spawn(&mut g, P0, port(), "high_men_spearmen", 5);
        let mut ai = Ai::new(P0);
        ai.unreachable.push(enemy());
        let mut ashore = 0;
        for _ in 0..60 {
            g.balance_farmers(P0);
            run(&mut ai, &mut g, &mut Vec::new());
            g.apply(Command::EndTurn);
            ashore = army
                .iter()
                .filter(|&&u| {
                    g.units()
                        .unit_position(u)
                        .is_some_and(|p| p.x >= ENEMY_CITY.0 && p != enemy())
                })
                .count();
            if ashore > 0 {
                break;
            }
        }
        assert!(ashore >= 3, "only {ashore} fighters landed");
        let alive = army
            .iter()
            .filter(|&&u| g.units().unit_position(u).is_some())
            .count();
        assert_eq!(alive, 5, "nobody drowned");
        assert!(
            g.units().unit_position(army[0]).is_some(),
            "the garrison fighter is still alive"
        );
        Ok(())
    }

    #[test]
    fn a_landing_on_a_rival_is_preceded_by_a_declaration_of_war_unless_a_pact_binds() -> TestResult
    {
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        g.reveal_map(P0);
        let plan = Plan {
            target: enemy(),
            land: at(ENEMY_CITY.0 - 1, ENEMY_CITY.1),
            sea: at(ENEMY_CITY.0 - 2, ENEMY_CITY.1),
            port: port(),
        };
        let mut ai = Ai::new(P0);
        assert!(declare_war_on_target(
            &mut ai,
            &mut g,
            plan,
            &mut Vec::new()
        ));
        assert_eq!(g.treaty(P0, P1), Treaty::War);
        assert_eq!(ai.quarrel, Some(P1));
        let mut bound = game(COAST_CITY, "high_men");
        bound.make_contact(P0, P1);
        bound.reveal_map(P0);
        bound.start_treaty(P0, P1, Treaty::Pact);
        let mut ai = Ai::new(P0);
        assert!(!declare_war_on_target(
            &mut ai,
            &mut bound,
            plan,
            &mut Vec::new()
        ));
        assert_eq!(bound.treaty(P0, P1), Treaty::Pact);
        Ok(())
    }

    #[test]
    fn a_guarded_tower_across_the_sea_is_a_target_while_no_rival_is_known() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let tower = at(33, 9);
        spawn_tower(&mut g, tower, true)?;
        g.explore(P0, tower, 5);
        let ai = Ai::new(P0);
        let plan = choose_plan(&ai, &PlayerView::new(&g, P0)).ok_or("a plan for the tower")?;
        assert_eq!(plan.target, tower);
        assert_eq!(plan.port, port());
        // Once its ground across is explored, it is not worth the trip.
        for plane in f4x_core::map::Plane::ALL {
            g.explore(P0, Position { plane, ..tower }, 5);
        }
        g.reveal_map(P0);
        assert_eq!(choose_plan(&ai, &PlayerView::new(&g, P0)), None);
        Ok(())
    }

    #[test]
    fn a_voyage_whose_soldiers_were_left_behind_is_given_up() {
        let mut g = game(COAST_CITY, "high_men");
        let sea = at(12, 6);
        let ship = spawn(&mut g, P0, sea, "galley", 1)[0];
        let left = spawn(&mut g, P0, port(), "high_men_spearmen", 1)[0];
        let view = PlayerView::new(&g, P0);
        let ferry = Ferry {
            ship: view.serial_of(ship),
            group: vec![view.serial_of(left)],
        };
        let plan = Plan {
            target: enemy(),
            land: at(ENEMY_CITY.0 - 1, ENEMY_CITY.1),
            sea,
            port: port(),
        };
        let mut ai = Ai::new(P0);
        ai.naval = Some(plan);
        ai.ferry = Some(ferry.clone());
        voyage(&mut ai, &mut g, plan, ferry, ship, &mut Vec::new());
        assert_eq!(ai.naval, None);
        assert!(ai.ferry.is_none());
        assert!(ai.naval_bad.contains(&enemy()));
    }

    #[test]
    fn an_idle_ship_scouts_once_no_rival_is_known_and_the_land_is_explored() {
        let mut g = game(COAST_CITY, "high_men");
        let ship = spawn(&mut g, P0, port(), "galley", 1)[0];
        let serial = PlayerView::new(&g, P0).serial_of(ship);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        sea_scout(&mut ai, &mut g);
        assert!(ai.sea_scouting);
        assert!(ai.scouts.contains(&serial));
        assert!(!ai.frontier_closed, "scouting by sea reopens the search");
    }

    #[test]
    fn no_ship_scouts_while_a_rival_is_known_or_land_is_still_open() {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, port(), "galley", 1);
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        sea_scout(&mut ai, &mut g);
        assert!(!ai.sea_scouting, "the enemy city is known");
        let mut fresh = game(COAST_CITY, "high_men");
        spawn(&mut fresh, P0, port(), "galley", 1);
        let mut ai = Ai::new(P0);
        sea_scout(&mut ai, &mut fresh);
        assert!(!ai.sea_scouting, "land scouting is not over");
    }

    #[test]
    fn a_sea_scout_is_called_off_once_a_landing_is_planned() {
        let mut g = game(COAST_CITY, "high_men");
        g.reveal_map(P0);
        let ship = spawn(&mut g, P0, port(), "galley", 1)[0];
        let serial = PlayerView::new(&g, P0).serial_of(ship);
        let mut ai = Ai::new(P0);
        ai.scouts.push(serial);
        ai.unreachable.push(enemy());
        run(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.naval.is_some());
        assert!(
            !ai.scouts.contains(&serial),
            "the ship carries the landing now"
        );
    }

    #[test]
    fn a_full_port_lets_a_fighter_out_so_the_ship_can_come_in() {
        let mut g = game(COAST_CITY, "high_men");
        g.reveal_map(P0);
        let full = spawn(
            &mut g,
            P0,
            port(),
            "high_men_spearmen",
            f4x_core::unit::MAX_TILE_LIMIT,
        );
        assert_eq!(
            PlayerView::new(&g, P0).garrison(port()).len(),
            f4x_core::unit::MAX_TILE_LIMIT
        );
        let mut ai = Ai::new(P0);
        make_room(&mut ai, &mut g, port(), &mut Vec::new());
        let left = PlayerView::new(&g, P0).garrison(port()).len();
        assert_eq!(left, f4x_core::unit::MAX_TILE_LIMIT - 1);
        assert!(full.iter().all(|&u| g.units().unit_position(u).is_some()));
    }
}
