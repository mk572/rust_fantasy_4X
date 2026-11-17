//! Ships: how the AI reaches an enemy city on another land mass (`docs/ai-design.md` 11.3). A
//! plan names the target, a landing (a land square and a sea square beside it) and the port to
//! sail from.

use f4x_core::city::production::{ProductionError, ProductionItem};
use f4x_core::diplomacy::{DiplomacyCommand, Treaty};
use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::view::{is_coastal, is_fighter, is_ship};
use crate::Ai;

/// Fighters that must stand in the port before a ship sails.
pub(crate) const MIN_LOAD: usize = 3;

/// Where a landing goes, and where it starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// The ship, or the building towards one, for the port to build: a galley (five units) when it
/// can, a trireme or its guild first when the ship is not for an army (`big` false), the buildings that lead to one when the race may have them, a trireme (two) when it may
/// not.
pub fn port_item(view: &PlayerView, city: hecs::Entity, big: bool) -> Option<ProductionItem> {
    let rules = &view.content().rules;
    let unit = |id: u16| {
        rules
            .units
            .id_numbered(id)
            .map(ProductionItem::Unit)
            .filter(|&i| view.can_build(city, i).is_ok())
    };
    let building = |num: u16| {
        Some(ProductionItem::Building(f4x_def::rules::BuildingId::new(
            num,
        )))
    };
    if let Some(galley) = unit(36 /* galley */) {
        return Some(galley);
    }
    // A ship that only scouts or carries a settler or two is not worth the yard's chain.
    if !big {
        if let Some(trireme) = unit(35 /* trireme */) {
            return Some(trireme);
        }
        if let Some(guild) =
            building(12 /* ship_wrights_guild */).filter(|&i| view.can_build(city, i).is_ok())
        {
            return Some(guild);
        }
    }
    let yard = building(13 /* ship_yard */);
    let yard_possible = yard.is_some_and(|y| {
        !matches!(view.can_build(city, y), Err(ProductionError::WrongRace))
            && !matches!(view.can_build(city, y), Err(ProductionError::NotCoastal))
    });
    if yard_possible {
        for id in [
            13, /* ship_yard */
            12, /* ship_wrights_guild */
            15, /* sawmill */
        ] {
            if let Some(item) = building(id).filter(|&i| view.can_build(city, i).is_ok()) {
                return Some(item);
            }
        }
    }
    if let Some(trireme) = unit(35 /* trireme */) {
        return Some(trireme);
    }
    building(12 /* ship_wrights_guild */).filter(|&i| view.can_build(city, i).is_ok())
}

/// Whether the item is the trireme.
pub(crate) fn is_trireme(view: &PlayerView, item: ProductionItem) -> bool {
    matches!(item, ProductionItem::Unit(u)
        if view.content().rules.units.id_numbered(35 /* trireme */) == Some(u))
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
pub(crate) fn landing(view: &PlayerView, city: Position) -> Option<(Position, Position)> {
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
                    crate::military::neighbours(view, target)
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
            crate::military::expected_defence(view, &ai.city_memory, view.turn(), *t)
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

/// Keep the landing plan (the war job gathers the army at its port and tags it for the ships,
/// `pickup.rs` carries it) and the sea scout.
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
            }
        }
        if ai.naval.is_none() && ai.campaign.is_none() {
            ai.naval = choose_plan(ai, &view);
        }
    }
    sea_scout(ai, game);
    if ai.naval.is_none() {
        return;
    }
    // The sea scout has found its enemy: the ship now carries the landing.
    let scouting: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        my_ships(&view)
            .into_iter()
            .filter(|&s| ai.roles.is_scouting(view.serial_of(s)))
            .collect()
    };
    if !scouting.is_empty() {
        let serials: Vec<Serial> = {
            let view = PlayerView::new(game, me);
            scouting.iter().map(|&s| view.serial_of(s)).collect()
        };
        ai.roles.release_many(&serials, crate::roles::Role::Scout);
        ai.roles.release_role(crate::roles::Role::SeaParty);
        ai.order(game, Command::ClearOrders { units: scouting }, events);
    }
}

/// The same for any march against a city that is not on the AI's own ground: the landing, and the
/// crossing by a tower (the war comes first, so that what stands in the way can be attacked).
pub(crate) fn declare_war_on_city(
    ai: &mut Ai,
    game: &mut Game,
    target: Position,
    events: &mut Vec<Event>,
) -> bool {
    let me = ai.player;
    let owner = {
        let view = PlayerView::new(game, me);
        view.city_at(target)
            .and_then(|c| view.city_owner(c))
            .filter(|&o| view.is_rival_wizard(o))
    };
    let Some(owner) = owner else {
        return true;
    };
    match game.treaty(me, owner) {
        Treaty::War => true,
        // A wizard it has not met cannot be declared war on: the landing goes on.
        Treaty::None if !game.have_met(me, owner) => true,
        Treaty::None => {
            if ai
                .send(
                    game,
                    Command::Diplomacy(DiplomacyCommand::DeclareWar {
                        player: me,
                        on: owner,
                    }),
                    events,
                )
                .is_ok()
            {
                ai.quarrel = Some(owner);
            }
            true
        }
        Treaty::Pact | Treaty::Alliance => false,
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
        let _ = ai
            .roles
            .claim(serial, crate::roles::Role::Scout, game.turn, None);
        ai.frontier_closed = false;
        ai.sea_scouting = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{
        at, build, game, spawn, spawn_tower, COAST_CITY, ENEMY_CITY, INLAND_CITY, P0,
    };
    use f4x_core::map::terrain::TerrainClass;
    use f4x_def::half_int::HalfInt;

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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        assert_eq!(port_builds(&g).as_deref(), Some("ship_wrights_guild"));
        build(&mut g, port(), &[12 /* ship_wrights_guild */]);
        assert_eq!(port_builds(&g).as_deref(), Some("sawmill"));
        build(&mut g, port(), &[15 /* sawmill */]);
        assert_eq!(port_builds(&g).as_deref(), Some("ship_yard"));
        build(&mut g, port(), &[13 /* ship_yard */]);
        assert_eq!(port_builds(&g).as_deref(), Some("galley"));
    }

    #[test]
    fn a_race_without_a_ship_yard_stops_at_the_trireme() {
        // Beastmen may build a Ship Wrights Guild but no Ship Yard.
        let mut g = game(COAST_CITY, 1 /* beastmen */);
        assert_eq!(port_builds(&g).as_deref(), Some("ship_wrights_guild"));
        build(&mut g, port(), &[12 /* ship_wrights_guild */]);
        assert_eq!(port_builds(&g).as_deref(), Some("trireme"));
    }

    #[test]
    fn a_city_away_from_the_sea_is_never_offered_a_ship() -> TestResult {
        // With every ship building in place, an inland city still cannot build a ship.
        let mut g = game(INLAND_CITY, 8 /* high_men */);
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        build(
            &mut g,
            inland,
            &[
                12, /* ship_wrights_guild */
                15, /* sawmill */
                13, /* ship_yard */
            ],
        );
        let view = PlayerView::new(&g, P0);
        let city = view.city_at(inland).ok_or("the city is there")?;
        assert_eq!(port_item(&view, city, true), None);
        Ok(())
    }

    #[test]
    fn a_landing_is_a_shore_square_beside_the_city_and_the_sea_beside_that() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
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
        let g = game(COAST_CITY, 8 /* high_men */);
        let view = PlayerView::new(&g, P0);
        assert_eq!(landing(&view, enemy()), None);
    }

    #[test]
    fn the_plan_sails_from_the_coastal_city_to_a_landing_by_the_enemy() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
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
        let mut inland = game(INLAND_CITY, 8 /* high_men */);
        inland.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.unreachable.push(enemy());
        assert_eq!(choose_plan(&ai, &PlayerView::new(&inland, P0)), None);
    }

    #[test]
    fn a_guarded_tower_across_the_sea_is_a_target_while_no_rival_is_known() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let tower = at(33, 9);
        spawn_tower(&mut g, tower, true)?;
        g.explore(P0, tower, HalfInt::from_raw(5));
        let ai = Ai::new(P0);
        let plan = choose_plan(&ai, &PlayerView::new(&g, P0)).ok_or("a plan for the tower")?;
        assert_eq!(plan.target, tower);
        assert_eq!(plan.port, port());
        // Once its ground across is explored, it is not worth the trip.
        for plane in f4x_core::map::Plane::all() {
            g.explore(P0, Position { plane, ..tower }, HalfInt::from_raw(5));
        }
        g.reveal_map(P0);
        assert_eq!(choose_plan(&ai, &PlayerView::new(&g, P0)), None);
        Ok(())
    }

    #[test]
    fn an_idle_ship_scouts_once_no_rival_is_known_and_the_land_is_explored() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let ship = spawn(&mut g, P0, port(), 36 /* galley */, 1)[0];
        let serial = PlayerView::new(&g, P0).serial_of(ship);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        sea_scout(&mut ai, &mut g);
        assert!(ai.sea_scouting);
        assert!(ai.roles.is_scouting(serial));
        assert!(!ai.frontier_closed, "scouting by sea reopens the search");
    }

    #[test]
    fn no_ship_scouts_while_a_rival_is_known_or_land_is_still_open() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut g, P0, port(), 36 /* galley */, 1);
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        sea_scout(&mut ai, &mut g);
        assert!(!ai.sea_scouting, "the enemy city is known");
        let mut fresh = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut fresh, P0, port(), 36 /* galley */, 1);
        let mut ai = Ai::new(P0);
        sea_scout(&mut ai, &mut fresh);
        assert!(!ai.sea_scouting, "land scouting is not over");
    }

    #[test]
    fn a_sea_scout_is_called_off_once_a_landing_is_planned() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let ship = spawn(&mut g, P0, port(), 36 /* galley */, 1)[0];
        let serial = PlayerView::new(&g, P0).serial_of(ship);
        let mut ai = Ai::new(P0);
        let _ = ai.roles.claim(serial, crate::roles::Role::Scout, 1, None);
        ai.unreachable.push(enemy());
        run(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.naval.is_some());
        assert!(
            !ai.roles.is_scouting(serial),
            "the ship carries the landing now"
        );
    }
}
