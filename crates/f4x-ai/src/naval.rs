//! Ships: how the AI reaches an enemy city on another land mass (`docs/ai-design.md` 11.3). A
//! plan names the target, a landing (a land square and a sea square beside it) and the port to
//! sail from.

use f4x_core::city::production::{ProductionError, ProductionItem};
use f4x_core::diplomacy::{DiplomacyCommand, Treaty};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::serial::Serial;
use f4x_core::world::{Command, Event};
use f4x_def::rules::{BuildingId, Rules};

use crate::view::{is_coastal, is_fighter, is_ship};
use crate::Ai;
use f4x_master::Game;

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

/// The ship, or the building towards one, for the port to build: a galley (the bigger ship of the
/// rules' `targets`) when it can, a trireme or the building it needs first when the ship is not
/// for an army (`big` false), the buildings that lead to a galley when the race may have them, a
/// trireme when it may not.
pub fn port_item(view: &PlayerView, city: hecs::Entity, big: bool) -> Option<ProductionItem> {
    let rules = &view.content().rules;
    let can = |i: &ProductionItem| view.can_build(city, *i).is_ok();
    let unit = |num: u16| {
        rules
            .units
            .id_numbered(num)
            .map(ProductionItem::Unit)
            .filter(can)
    };
    // What a ship asks for in a city, and what those buildings ask for in turn.
    let needs = |num: u16| {
        let first = rules.units.id_numbered(num);
        first.map_or_else(Vec::new, |u| chain(rules, &rules.units.get(u).requires))
    };
    let (galley, trireme) = (
        rules.config.targets.galley_unit,
        rules.config.targets.trireme_unit,
    );
    let first_of = |list: Vec<BuildingId>| {
        list.into_iter()
            .map(ProductionItem::Building)
            .find(|i| can(i))
    };
    if let Some(ship) = unit(galley) {
        return Some(ship);
    }
    // A ship that only scouts or carries a settler or two is not worth the galley's chain.
    if !big {
        if let Some(item) = unit(trireme).or_else(|| first_of(needs(trireme))) {
            return Some(item);
        }
    }
    let yard = rules
        .units
        .id_numbered(galley)
        .and_then(|u| rules.units.get(u).requires.first().copied());
    let yard_possible = yard.is_some_and(|y| {
        !matches!(
            view.can_build(city, ProductionItem::Building(y)),
            Err(ProductionError::WrongRace | ProductionError::NotCoastal)
        )
    });
    if yard_possible {
        if let Some(item) = first_of(needs(galley)) {
            return Some(item);
        }
    }
    unit(trireme).or_else(|| first_of(needs(trireme)))
}

/// The buildings `wanted` and all they need, the wanted ones first; of what a building needs, the
/// one it replaces comes before the others.
fn chain(rules: &Rules, wanted: &[BuildingId]) -> Vec<BuildingId> {
    let mut out = wanted.to_vec();
    let mut next = 0;
    while let Some(&b) = out.get(next) {
        let def = rules.buildings.get(b);
        for &need in def.replaces.iter().chain(&def.requires) {
            if !out.contains(&need) {
                out.push(need);
            }
        }
        next += 1;
    }
    out
}

/// Whether the item is the trireme.
pub(crate) fn is_trireme(view: &PlayerView, item: ProductionItem) -> bool {
    let rules = &view.content().rules;
    matches!(item, ProductionItem::Unit(u)
        if rules.units.id_numbered(rules.config.targets.trireme_unit) == Some(u))
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
            // The ship's passengers are walkers: test one of those when the garrison has one (a
            // flier or a swimmer reaches the tower by itself and says nothing about the ships).
            let fighters: Vec<_> = view
                .garrison(city)
                .into_iter()
                .filter(|&u| is_fighter(view, u))
                .collect();
            fighters
                .iter()
                .copied()
                .find(|&u| view.is_walker(u))
                .or(fighters.first().copied())
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
fn choose_plan(ai: &Ai, view: &PlayerView, round: u32) -> Option<Plan> {
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
        let held =
            |t: &Position| crate::military::expected_defence(view, &ai.city_memory, round, *t);
        held(a)
            .total_cmp(&held(b))
            .then((a.plane as u8, a.y, a.x).cmp(&(b.plane as u8, b.y, b.x)))
    });
    for target in targets {
        let owner = view
            .city_at(target)
            .and_then(|c| view.city_owner(c))
            .filter(|&o| o != view.me() && crate::attack::may_target(view, o));
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
                    .is_some_and(|o| o != me && crate::attack::may_target(&view, o))
            };
            let port = view.city_at(plan.port).and_then(|c| view.city_owner(c)) == Some(me);
            if !enemy || !port {
                ai.naval = None;
            }
        }
        if ai.naval.is_none() && ai.campaign.is_none() {
            ai.naval = choose_plan(ai, &view, game.round());
        }
    }
    sea_scout(ai, game);
    if ai.naval.is_none() {
        return;
    }
    // The sea scout has found its enemy: the ship now carries the landing.
    let scouting: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        my_ships(&view)
            .into_iter()
            .filter(|&s| tasks.roles.is_scouting(view.serial_of(s)))
            .collect()
    };
    if !scouting.is_empty() {
        let serials: Vec<Serial> = {
            let view = PlayerView::new(game, me);
            scouting.iter().map(|&s| view.serial_of(s)).collect()
        };
        ai.write(game)
            .release_many(&serials, crate::roles::Role::Scout);
        ai.write(game).release_role(crate::roles::Role::SeaParty);
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
            .filter(|&o| view.is_diplomat(o))
    };
    let Some(owner) = owner else {
        return true;
    };
    match game.treaty(me, owner) {
        Treaty::War => true,
        // A player it has not met cannot be declared war on: the landing goes on.
        Treaty::None if !game.have_met(me, owner) => true,
        // No war in a peace period.
        Treaty::None if !crate::attack::may_declare(&PlayerView::new(game, me), owner) => false,
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
                ai.memory.put().enemy = Some((owner, game.round()));
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
            .write(game)
            .claim(serial, crate::roles::Role::Scout, None);
        ai.frontier_closed = false;
        ai.sea_scouting = true;
    }
}

/// The sea scout is called off: the ship has a settler to carry.
pub(crate) fn release_scouts(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let ships: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        view.my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u) && tasks.roles.is_scouting(view.serial_of(u)))
            .collect()
    };
    if ships.is_empty() {
        return;
    }
    let serials: Vec<Serial> = {
        let view = PlayerView::new(game, me);
        ships.iter().map(|&s| view.serial_of(s)).collect()
    };
    ai.write(game)
        .release_many(&serials, crate::roles::Role::Scout);
    ai.write(game).release_role(crate::roles::Role::SeaParty);
    ai.sea_scouting = false;
    ai.order(game, Command::ClearOrders { units: ships }, events);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{
        at, build, game, spawn, spawn_tower, COAST_CITY, ENEMY_CITY, INLAND_CITY, P0,
    };
    use f4x_core::map::terrain::TerrainClass;
    use f4x_core::world::World;
    use f4x_def::half_int::HalfInt;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn port() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    fn enemy() -> Position {
        at(ENEMY_CITY.0, ENEMY_CITY.1)
    }

    /// The id of what the port would build.
    fn port_builds(world: &World) -> Option<String> {
        let view = PlayerView::new(world, P0);
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
        let mut w = game(COAST_CITY, 8 /* high_men */);
        assert_eq!(port_builds(&w).as_deref(), Some("ship_wrights_guild"));
        build(&mut w, port(), &[12 /* ship_wrights_guild */]);
        assert_eq!(port_builds(&w).as_deref(), Some("sawmill"));
        build(&mut w, port(), &[15 /* sawmill */]);
        assert_eq!(port_builds(&w).as_deref(), Some("ship_yard"));
        build(&mut w, port(), &[13 /* ship_yard */]);
        assert_eq!(port_builds(&w).as_deref(), Some("galley"));
    }

    #[test]
    fn a_race_without_a_ship_yard_stops_at_the_trireme() {
        // Beastmen may build a Ship Wrights Guild but no Ship Yard.
        let mut w = game(COAST_CITY, 1 /* beastmen */);
        assert_eq!(port_builds(&w).as_deref(), Some("ship_wrights_guild"));
        build(&mut w, port(), &[12 /* ship_wrights_guild */]);
        assert_eq!(port_builds(&w).as_deref(), Some("trireme"));
    }

    #[test]
    fn a_city_away_from_the_sea_is_never_offered_a_ship() -> TestResult {
        // With every ship building in place, an inland city still cannot build a ship.
        let mut w = game(INLAND_CITY, 8 /* high_men */);
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        build(
            &mut w,
            inland,
            &[
                12, /* ship_wrights_guild */
                15, /* sawmill */
                13, /* ship_yard */
            ],
        );
        let view = PlayerView::new(&w, P0);
        let city = view.city_at(inland).ok_or("the city is there")?;
        assert_eq!(port_item(&view, city, true), None);
        Ok(())
    }

    #[test]
    fn a_landing_is_a_shore_square_beside_the_city_and_the_sea_beside_that() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let view = PlayerView::new(&w, P0);
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
        let w = game(COAST_CITY, 8 /* high_men */);
        let view = PlayerView::new(&w, P0);
        assert_eq!(landing(&view, enemy()), None);
    }

    #[test]
    fn the_plan_sails_from_the_coastal_city_to_a_landing_by_the_enemy() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.unreachable.push(enemy());
        let view = PlayerView::new(&w, P0);
        let plan = choose_plan(&ai, &view, w.round()).ok_or("a plan")?;
        assert_eq!(plan.target, enemy());
        assert_eq!(plan.port, port());
        assert_eq!(view.terrain_class(plan.sea), Some(TerrainClass::Ocean));
        assert_ne!(view.terrain_class(plan.land), Some(TerrainClass::Ocean));
        Ok(())
    }

    #[test]
    fn there_is_no_plan_without_a_target_a_coast_or_after_a_failure() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        // Nothing is unreachable by land.
        assert_eq!(choose_plan(&ai, &PlayerView::new(&w, P0), w.round()), None);
        // A target already given up on.
        ai.unreachable.push(enemy());
        ai.naval_bad.push(enemy());
        assert_eq!(choose_plan(&ai, &PlayerView::new(&w, P0), w.round()), None);
        // A port already given up on.
        ai.naval_bad = vec![port()];
        assert_eq!(choose_plan(&ai, &PlayerView::new(&w, P0), w.round()), None);
        // No coastal city at all.
        let mut inland = game(INLAND_CITY, 8 /* high_men */);
        inland.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.unreachable.push(enemy());
        assert_eq!(
            choose_plan(&ai, &PlayerView::new(&inland, P0), w.round()),
            None
        );
    }

    #[test]
    fn a_guarded_tower_across_the_sea_is_a_target_while_no_rival_is_known() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let tower = at(33, 9);
        spawn_tower(&mut w, tower, true)?;
        w.explore(P0, tower, HalfInt::from_raw(5));
        let ai = Ai::new(P0);
        let plan =
            choose_plan(&ai, &PlayerView::new(&w, P0), w.round()).ok_or("a plan for the tower")?;
        assert_eq!(plan.target, tower);
        assert_eq!(plan.port, port());
        // Once its ground across is explored, it is not worth the trip.
        for plane in f4x_core::map::Plane::all() {
            w.explore(P0, Position { plane, ..tower }, HalfInt::from_raw(5));
        }
        w.reveal_map(P0);
        assert_eq!(choose_plan(&ai, &PlayerView::new(&w, P0), w.round()), None);
        Ok(())
    }

    #[test]
    fn an_idle_ship_scouts_once_no_rival_is_known_and_the_land_is_explored() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let ship = spawn(&mut w, P0, port(), 36 /* galley */, 1)[0];
        let serial = PlayerView::new(&w, P0).serial_of(ship);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        sea_scout(&mut ai, &mut w);
        assert!(ai.sea_scouting);
        assert!(ai.tasks(&w).roles.is_scouting(serial));
        assert!(!ai.frontier_closed, "scouting by sea reopens the search");
    }

    #[test]
    fn no_ship_scouts_while_a_rival_is_known_or_land_is_still_open() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, port(), 36 /* galley */, 1);
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        sea_scout(&mut ai, &mut w);
        assert!(!ai.sea_scouting, "the enemy city is known");
        let mut fresh = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut fresh, P0, port(), 36 /* galley */, 1);
        let mut ai = Ai::new(P0);
        sea_scout(&mut ai, &mut fresh);
        assert!(!ai.sea_scouting, "land scouting is not over");
    }

    #[test]
    fn a_sea_scout_is_called_off_once_a_landing_is_planned() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let ship = spawn(&mut w, P0, port(), 36 /* galley */, 1)[0];
        let serial = PlayerView::new(&w, P0).serial_of(ship);
        let mut ai = Ai::new(P0);
        let _ = ai
            .write(&mut w)
            .claim(serial, crate::roles::Role::Scout, None);
        ai.unreachable.push(enemy());
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(ai.naval.is_some());
        assert!(
            !ai.tasks(&w).roles.is_scouting(serial),
            "the ship carries the landing now"
        );
    }
}
