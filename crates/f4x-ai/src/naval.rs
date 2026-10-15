//! Ships: how the AI reaches an enemy that lives on another land mass (`docs/ai-design.md` 11.3,
//! task 230).
//!
//! When every enemy city it knows is out of reach on foot, the AI picks a landing: a land square
//! beside the target city and a sea square beside that. The nearest coastal city of its own on
//! the same sea becomes the port and builds a ship (and the building that lets it). Fighters
//! gather at the port; when enough stand there, the ship and they sail together to the sea
//! square, the fighters step ashore next turn, and the ship goes back for more. The landed units
//! are ordinary units after that: standing next to the enemy city they attack it when they are
//! strong enough, and each trip adds to them.

use f4x_core::city::Position;
use f4x_core::player_view::PlayerView;
use f4x_core::production::{ProductionError, ProductionItem};
use f4x_core::rules::Stat;
use f4x_core::serial::Serial;
use f4x_core::{Command, Event, Game};

use crate::view::{garrison_wanted, is_coastal, is_fighter, is_ship};
use crate::Ai;

/// Fighters that must stand in the port before a ship sails.
const MIN_LOAD: usize = 3;
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
/// can, the buildings that lead to one when the race may have them, a trireme (two) when it may
/// not.
pub fn port_item(view: &PlayerView, city: hecs::Entity) -> Option<ProductionItem> {
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
    use f4x_core::content::TerrainClass;
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
    None
}

/// Plan, build, load, sail, land.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // Forget a plan whose target is no longer an enemy's or whose port is gone.
    {
        let view = PlayerView::new(game, me);
        if let Some(plan) = ai.naval {
            let enemy = view
                .city_at(plan.target)
                .and_then(|c| view.city_owner(c))
                .is_some_and(|o| o != me);
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
        let expected =
            crate::military::expected_defence(&view, &ai.city_memory, game.turn, plan.target);
        if strength * LANDING_EDGE < expected {
            ai.naval_wait += 1;
            if ai.naval_wait < WAIT_TURNS {
                return;
            }
        }
        ai.naval_wait = 0;
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
                                        != Some(f4x_core::content::TerrainClass::Ocean)
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
    use crate::testworld::{at, build, game, spawn, COAST_CITY, ENEMY_CITY, INLAND_CITY, P0};
    use f4x_core::content::TerrainClass;

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
        Some(match port_item(&view, city)? {
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
    fn a_city_away_from_the_sea_is_never_offered_a_ship() {
        // With every ship building in place, an inland city still cannot build a ship.
        let mut g = game(INLAND_CITY, "high_men");
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        build(
            &mut g,
            inland,
            &["ship_wrights_guild", "sawmill", "ship_yard"],
        );
        let view = PlayerView::new(&g, P0);
        let city = view.city_at(inland).unwrap();
        assert_eq!(port_item(&view, city), None);
    }

    #[test]
    fn a_landing_is_a_shore_square_beside_the_city_and_the_sea_beside_that() {
        let mut g = game(COAST_CITY, "high_men");
        g.reveal_map(P0);
        let view = PlayerView::new(&g, P0);
        let (land, sea) = landing(&view, enemy()).expect("a landing");
        assert_ne!(land, enemy());
        assert_ne!(view.terrain_class(land), Some(TerrainClass::Ocean));
        assert_eq!(view.terrain_class(sea), Some(TerrainClass::Ocean));
        assert_eq!(view.distance(land, sea), Some(1));
        assert!(view.distance(land, enemy()).is_some_and(|d| d <= 2));
    }

    #[test]
    fn no_landing_is_known_for_ground_the_wizard_has_not_seen() {
        let g = game(COAST_CITY, "high_men");
        let view = PlayerView::new(&g, P0);
        assert_eq!(landing(&view, enemy()), None);
    }

    #[test]
    fn the_plan_sails_from_the_coastal_city_to_a_landing_by_the_enemy() {
        let mut g = game(COAST_CITY, "high_men");
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.unreachable.push(enemy());
        let view = PlayerView::new(&g, P0);
        let plan = choose_plan(&ai, &view).expect("a plan");
        assert_eq!(plan.target, enemy());
        assert_eq!(plan.port, port());
        assert_eq!(view.terrain_class(plan.sea), Some(TerrainClass::Ocean));
        assert_ne!(view.terrain_class(plan.land), Some(TerrainClass::Ocean));
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
    fn a_galley_ferries_fighters_to_the_enemy_island() {
        let mut g = game(COAST_CITY, "high_men");
        build(
            &mut g,
            port(),
            &["ship_wrights_guild", "sawmill", "ship_yard"],
        );
        g.reveal_map(P0);
        // No upkeep trouble: the test world has no economy to pay for the army.
        g.player_mut(P0).unwrap().gold = 1_000_000;
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
                        .is_some_and(|p| p.x >= 28 && p != enemy())
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
}
