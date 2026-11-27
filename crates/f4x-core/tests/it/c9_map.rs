//! Class 9, the map: roads and the engineers who build them, corrupted land, what a player has
//! explored and sees, and the survey of a city site. The terrain, spells, units and buildings are
//! the content's.

use crate::support::{around, content, state, text, worlds};
use f4x_core::map::corruption::{PurifyError, PURIFY_TURNS};
use f4x_core::map::road::{RoadError, RIVER_BRIDGE_TURNS};
use f4x_core::map::start_road::path_wrap;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::rules::exec::Target;
use f4x_core::rules::{BuildingId, SpellId, UnitId};
use f4x_core::spell::target_error::TargetError;
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;
use f4x_def::testdir::{deep, sample};

const SPEARMEN: u16 = 104; // high_men_spearmen
const ENGINEERS: u16 = 110; // high_men_engineers
const SHAMAN: u16 = 43; // barbarian_shaman
const NIGHTBLADES: u16 = 63; // dark_elf_nightblades
const SKELETONS: u16 = 165; // skeletons
const NATURES_EYE: u16 = 12; // natures_eye
const GAIAS_BLESSING: u16 = 29; // gaias_blessing
const NATURE_AWARENESS: u16 = 34; // nature_awareness
const CORRUPTION: u16 = 85; // corruption
const CONSECRATION: u16 = 155; // consecration
const WARP_NODE: u16 = 186; // warp_node
const MAGIC_SPIRIT: u16 = 154; // magic_spirit
const ORACLE: u16 = 18; // oracle
const CITY_WALLS: u16 = 35; // city_walls

fn at(plane: Plane, x: u32, y: u32) -> Position {
    Position { x, y, plane }
}

fn squares(w: &World, plane: Plane) -> impl Iterator<Item = Position> {
    let map = w.map().expect("a map");
    let (width, height) = (map.width(), map.height());
    (0..height).flat_map(move |y| (0..width).map(move |x| at(plane, x, y)))
}

fn has_road(w: &World, p: Position) -> bool {
    let map = w.map().expect("a map");
    map.plane(p.plane).has_road(p.x as i32, p.y as i32)
}

fn dry(w: &World, p: Position) -> bool {
    w.economy().terrain_class_at(p) != Some(TerrainClass::Ocean)
}

/// A square with nothing on it: dry, no unit, city, feature or road, not a node.
fn bare(w: &World, p: Position) -> bool {
    w.economy()
        .terrain_class_at(p)
        .is_some_and(|c| c != TerrainClass::Ocean && !c.is_node())
        && w.units().units_at(p).is_empty()
        && w.economy().city_at(p).is_none()
        && w.economy().feature_entity_at(p).is_none()
        && !has_road(w, p)
}

/// A way of `length` bare squares, each beside the next, none of them beside a city.
fn bare_way(w: &World, plane: Plane, length: usize) -> Option<Vec<Position>> {
    let clear = |p: Position| {
        bare(w, p)
            && around(w, p)
                .into_iter()
                .all(|q| w.economy().city_at(q).is_none())
    };
    squares(w, plane).find_map(|start| {
        let mut way = Vec::new();
        let mut p = start;
        // Eastwards, one square at a time.
        while way.len() < length && clear(p) {
            way.push(p);
            let width = w.map()?.width();
            p = at(p.plane, (p.x + 1) % width, p.y);
        }
        (way.len() == length).then_some(way)
    })
}

fn create(w: &mut Table, owner: PlayerId, pos: Position, unit: u16) -> hecs::Entity {
    let before = w.units().units_at(pos);
    let events = w.apply(Command::CreateUnit {
        owner,
        pos,
        unit: UnitId::new(unit),
    });
    let made = w
        .units()
        .units_at(pos)
        .into_iter()
        .find(|u| !before.contains(u));
    made.unwrap_or_else(|| panic!("no unit was made: {events:?}"))
}

fn refusal(events: &[Event]) -> Option<&Rejection> {
    events.iter().find_map(|e| match e {
        Event::Rejected(why) => Some(why),
        _ => None,
    })
}

/// The player's next turn: moves back and the work carried on.
fn next_turn(w: &mut Table, p: PlayerId) -> Vec<Event> {
    w.start_player_turn(p, 2)
}

/// `player` casts spell `number` on `target`, paid at once.
fn cast(w: &mut Table, player: PlayerId, number: u16, target: Target) -> Vec<Event> {
    let spell = SpellId::new(number);
    w.cheat_learn_spell(player, spell);
    w.treasury_mut(player).expect("a treasury").mana = HalfInt::from_int(100_000);
    w.scenario_casting_with(player, spell, 0, 0);
    w.apply(Command::ResolveCast { player, target })
}

// ---- roads ----

struct Crew {
    name: String,
    w: Table,
    p: PlayerId,
    way: Vec<Position>,
}

/// Each world with a bare way of four squares.
fn crews() -> Vec<Crew> {
    let all: Vec<Crew> = worlds()
        .into_iter()
        .filter_map(|(name, mut w)| {
            let p = w.wizards()[0];
            let way = bare_way(&w, Plane::Arcanus, 4)?;
            w.cheat_reveal(p);
            Some(Crew { name, w, p, way })
        })
        .collect();
    assert!(!all.is_empty(), "no world had a bare way");
    all
}

#[test]
fn a_road_takes_work_on_land_none_where_one_is_and_cannot_cross_water() {
    for (name, w) in worlds() {
        for plane in Plane::all() {
            for p in squares(&w, plane) {
                let turns = w.turns_to_build_road(p);
                if has_road(&w, p) {
                    assert_eq!(turns, Some(0), "{name}: {p:?}");
                } else if dry(&w, p) {
                    assert!(turns.is_some_and(|t| t > 0), "{name}: {p:?} {turns:?}");
                } else {
                    assert_eq!(turns, None, "{name}: {p:?} is water");
                }
            }
        }
    }
}

#[test]
fn a_river_adds_its_bridge_to_the_work() {
    for mut c in crews() {
        let p = c.way[1];
        c.w.cheat_set_river(p, false);
        let plain = c.w.turns_to_build_road(p).expect("land");
        assert!(c.w.cheat_set_river(p, true), "{}", c.name);
        assert_eq!(
            c.w.turns_to_build_road(p),
            Some(plain + RIVER_BRIDGE_TURNS),
            "{}",
            c.name
        );
    }
}

#[test]
fn more_engineers_take_less_time_and_never_none() {
    for mut c in crews() {
        let here = c.way[0];
        let crew: Vec<hecs::Entity> = (0..8)
            .map(|_| create(&mut c.w, c.p, here, ENGINEERS))
            .collect();
        let one = c.w.road_turns(&crew[..1], here).expect("land");
        let mut last = one;
        for n in 1..=crew.len() {
            let points = c.w.construction_points(&crew[..n]);
            assert_eq!(
                points,
                n as u32 * c.w.construction_points(&crew[..1]),
                "{}",
                c.name
            );
            let turns = c.w.road_turns(&crew[..n], here).expect("land");
            assert!(turns >= 1 && turns <= last, "{}: {n} engineers", c.name);
            last = turns;
        }
        assert!(last < one, "{}: eight are no quicker than one", c.name);
    }
}

#[test]
fn a_road_order_that_cannot_be_carried_out_changes_nothing() {
    for mut c in crews() {
        let (a, b, d) = (c.way[0], c.way[1], c.way[3]);
        let engineer = create(&mut c.w, c.p, a, ENGINEERS);
        let stray = create(&mut c.w, c.p, b, ENGINEERS);
        let spearmen = create(&mut c.w, c.p, a, SPEARMEN);
        // Water beside the way's start.
        let water = around(&c.w, a)
            .into_iter()
            .find(|&q| bare(&c.w, q) && !c.way.contains(&q))
            .expect("a square beside the way");
        let sea = squares(&c.w, water.plane)
            .find(|&q| !dry(&c.w, q))
            .and_then(|q| c.w.map()?.plane(q.plane).get(q.x as i32, q.y as i32))
            .expect("water on the map");
        c.w.scenario_set_terrain(water, sea);
        for (units, path, why) in [
            (vec![], vec![a, b], RoadError::BadSelection),
            (vec![engineer, stray], vec![a, b], RoadError::BadSelection),
            (vec![spearmen], vec![a, b], RoadError::NoBuilders),
            (vec![engineer], vec![a, d], RoadError::BadPath),
            (vec![engineer], vec![a, water], RoadError::OnWater(water)),
        ] {
            let before = state(&c.w);
            let events = c.w.apply(Command::BuildRoad {
                units,
                path: path.clone(),
            });
            assert_eq!(
                refusal(&events),
                Some(&Rejection::Road(why)),
                "{}: {why:?}",
                c.name
            );
            assert_eq!(state(&c.w), before, "{}: {why:?}", c.name);
        }
    }
}

#[test]
fn engineers_work_along_a_path_square_by_square_and_stop_at_its_end() {
    for mut c in crews() {
        let here = c.way[0];
        let engineer = create(&mut c.w, c.p, here, ENGINEERS);
        let events = c.w.apply(Command::BuildRoad {
            units: vec![engineer],
            path: c.way.clone(),
        });
        assert_eq!(refusal(&events), None, "{}: {events:?}", c.name);
        assert!(c.w.unit_has_order(engineer), "{}", c.name);
        assert!(!has_road(&c.w, here), "{}: done before any turn", c.name);
        // One turn for each turn of work, and at most one more to step to each next square.
        let work: u32 = c
            .way
            .iter()
            .map(|&p| u32::from(c.w.road_turns(&[engineer], p).expect("land")))
            .sum();
        let mut done = Vec::new();
        let mut turns = 0;
        while c.w.unit_has_order(engineer) {
            turns += 1;
            assert!(turns <= 3 * work, "{}: the road never ends", c.name);
            for e in next_turn(&mut c.w, c.p) {
                if let Event::RoadBuilt { pos } = e {
                    done.push(pos);
                }
            }
        }
        assert!(
            turns >= work,
            "{}: {turns} turns for {work} of work",
            c.name
        );
        assert_eq!(done, c.way, "{}: square by square", c.name);
        assert!(c.way.iter().all(|&p| has_road(&c.w, p)), "{}", c.name);
        assert_eq!(
            c.w.units().unit_position(engineer),
            c.way.last().copied(),
            "{}",
            c.name
        );
    }
}

#[test]
fn road_work_goes_on_the_same_after_a_save() -> Result<(), Box<dyn std::error::Error>> {
    for mut c in crews() {
        let engineer = create(&mut c.w, c.p, c.way[0], ENGINEERS);
        c.w.apply(Command::BuildRoad {
            units: vec![engineer],
            path: c.way.clone(),
        });
        next_turn(&mut c.w, c.p);
        let saved = c.w.snapshot().to_text()?;
        let mut back = Snapshot::from_text(&saved)?.restore(content())?;
        assert_eq!(text(&back), saved, "{}", c.name);
        let loaded = back
            .units()
            .units_at(c.w.units().unit_position(engineer).expect("a square"));
        assert!(
            loaded.iter().any(|&u| back.unit_has_order(u)),
            "{}: the order is lost",
            c.name
        );
        let mut rng = c.w.rng.clone();
        for round in 0..30 {
            c.w.start_player_turn(c.p, 2);
            back.start_player_turn(c.p, 2, &mut rng);
            assert_eq!(text(&back), text(&c.w), "{}: turn {round}", c.name);
        }
    }
    Ok(())
}

#[test]
fn engineers_moved_away_give_the_road_up() {
    for mut c in crews() {
        let here = c.way[0];
        let engineer = create(&mut c.w, c.p, here, ENGINEERS);
        c.w.apply(Command::BuildRoad {
            units: vec![engineer],
            path: c.way.clone(),
        });
        c.w.cheat_move_unit(engineer, c.way[2]);
        for _ in 0..12 {
            next_turn(&mut c.w, c.p);
        }
        assert!(!c.w.unit_has_order(engineer), "{}", c.name);
        assert!(c.way.iter().all(|&p| !has_road(&c.w, p)), "{}", c.name);
    }
}

#[test]
fn a_road_finished_beside_a_town_gives_the_town_a_road() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (_, town) = w.economy().cities_of(p)[0];
        let here = around(&w, town)
            .into_iter()
            .find(|&q| bare(&w, q))
            .unwrap_or_else(|| panic!("{name}: no bare square beside the town"));
        w.scenario_set_road(town, false);
        let engineer = create(&mut w, p, here, ENGINEERS);
        w.apply(Command::BuildRoad {
            units: vec![engineer],
            path: vec![here],
        });
        for _ in 0..12 {
            next_turn(&mut w, p);
        }
        assert!(has_road(&w, here), "{name}");
        assert!(has_road(&w, town), "{name}: the town has no road");
    }
}

#[test]
fn the_roads_of_a_new_world_stand_under_every_city_and_never_on_water() {
    // Roads that lead from city to city, over all the worlds.
    let mut between = 0;
    for (name, mut w) in worlds() {
        for plane in Plane::all() {
            let roads = |w: &World| -> Vec<Position> {
                squares(w, plane).filter(|&p| has_road(w, p)).collect()
            };
            // The worlds were made with the roads: laid afresh they are the same again.
            let made = roads(&w);
            for &p in &made {
                w.scenario_set_road(p, false);
            }
            w.lay_start_roads(plane);
            assert_eq!(roads(&w), made, "{name}: {plane:?}");
            let cities: Vec<Position> = w
                .economy()
                .cities()
                .into_iter()
                .map(|(.., p)| p)
                .filter(|p| p.plane == plane)
                .collect();
            for &p in &cities {
                assert!(has_road(&w, p), "{name}: no road under {p:?}");
            }
            for p in made {
                if cities.contains(&p) {
                    continue;
                }
                between += 1;
                assert!(dry(&w, p), "{name}: a road on water at {p:?}");
                let grid = w.map().expect("a map").plane(plane);
                assert_eq!(
                    grid.has_enchanted_road(p.x as i32, p.y as i32),
                    plane == Plane::Myrror,
                    "{name}: {p:?}"
                );
            }
        }
    }
    assert!(between > 0, "no world has a road between two cities");
}

#[test]
fn a_line_between_two_squares_goes_the_short_way_round_one_step_at_a_time() {
    let (_, w) = worlds().remove(0);
    let map = w.map().expect("a map");
    let (width, height) = (map.width() as i32, map.height() as i32);
    let step = if deep() { 1 } else { 7 };
    for x1 in (0..width).step_by(step) {
        for x2 in (0..width).step_by(step) {
            for (y1, y2) in [(0, 0), (0, height - 1), (height / 2, 3), (5, height / 2)] {
                let line = path_wrap(x1, y1, x2, y2, width);
                let around_x = (x1 - x2).abs().min(width - (x1 - x2).abs());
                let length = around_x.max((y1 - y2).abs());
                let what = format!("({x1}, {y1}) to ({x2}, {y2})");
                assert_eq!(line.len() as i32, length, "{what}");
                let mut last = (x1, y1);
                for &(x, y) in &line {
                    assert!((0..width).contains(&x), "{what}");
                    let dx = (x - last.0).abs().min(width - (x - last.0).abs());
                    assert_eq!(dx.max((y - last.1).abs()), 1, "{what}: {line:?}");
                    last = (x, y);
                }
                assert_eq!(last, (x2, y2), "{what}: {line:?}");
            }
        }
    }
}

// ---- corrupted land ----

/// A bare square in the area of the player's first city.
fn field(w: &World, p: PlayerId) -> (hecs::Entity, Position) {
    let (city, home) = w.economy().cities_of(p)[0];
    let field = w
        .economy()
        .city_area(home)
        .into_iter()
        .map(|(q, _)| q)
        .find(|&q| bare(w, q) && !w.is_corrupted(q));
    (city, field.expect("a field"))
}

#[test]
fn the_corruption_spell_spoils_land_and_the_city_gets_nothing_from_it() {
    for (name, mut w) in worlds() {
        let (p, other) = (w.wizards()[0], w.wizards()[1]);
        w.cheat_reveal(p);
        let (city, home) = w.economy().cities_of(other)[0];
        let land: Vec<Position> = w
            .economy()
            .city_area(home)
            .into_iter()
            .map(|(q, _)| q)
            .filter(|&q| {
                w.economy()
                    .terrain_class_at(q)
                    .is_some_and(|c| c != TerrainClass::Ocean && !c.is_node())
            })
            .collect();
        let before = w.economy().city_economy(city).expect("a city");
        for &q in &land {
            if w.is_corrupted(q) {
                continue;
            }
            assert_eq!(w.check_corrupt(q), Ok(()), "{name}: {q:?}");
            let events = cast(&mut w, p, CORRUPTION, Target::Tile(q));
            assert_eq!(refusal(&events), None, "{name} {q:?}: {events:?}");
            assert!(w.is_corrupted(q), "{name}: {q:?}");
            assert_eq!(
                w.check_corrupt(q),
                Err(TargetError::AlreadyCorrupted),
                "{name}"
            );
            let grid = w.map().expect("a map").plane(q.plane);
            let gives = w.economy().square_yield(
                grid,
                q.x as i32,
                q.y as i32,
                false,
                f4x_core::city::economy::SquareCtx::SITE,
            );
            assert_eq!(
                (gives.production, gives.gold, gives.mana, gives.wild_food),
                (0, HalfInt::ZERO, HalfInt::ZERO, HalfInt::ZERO),
                "{name}: {q:?}"
            );
        }
        let after = w.economy().city_economy(city).expect("a city");
        assert!(
            after.max_size < before.max_size,
            "{name}: {before:?} {after:?}"
        );
        assert!(after.food <= before.food, "{name}: {before:?} {after:?}");
    }
}

#[test]
fn water_and_magic_nodes_cannot_be_corrupted() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        w.cheat_reveal(p);
        let water = squares(&w, Plane::Arcanus)
            .find(|&q| !dry(&w, q))
            .expect("water");
        let node = w.economy().nodes()[0].pos;
        for (q, why) in [
            (water, TargetError::CorruptWater),
            (node, TargetError::CorruptNode),
        ] {
            assert_eq!(w.check_corrupt(q), Err(why), "{name}");
            let events = cast(&mut w, p, CORRUPTION, Target::Tile(q));
            assert!(refusal(&events).is_some(), "{name}: {events:?}");
            assert!(!w.is_corrupted(q), "{name}");
        }
    }
}

#[test]
fn a_purifier_cleans_its_square_in_its_turns_and_gives_up_when_moved() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        w.cheat_reveal(p);
        let (_, q) = field(&w, p);
        let spearmen = create(&mut w, p, q, SPEARMEN);
        let shaman = create(&mut w, p, q, SHAMAN);
        let purify = |w: &mut Table, units: Vec<hecs::Entity>| w.apply(Command::Purify { units });
        for (units, why) in [
            (vec![], PurifyError::NoUnits),
            (vec![spearmen], PurifyError::CannotPurify),
            (vec![shaman], PurifyError::NotCorrupted),
        ] {
            let before = state(&w);
            let events = purify(&mut w, units);
            assert_eq!(
                refusal(&events),
                Some(&Rejection::Purify(why)),
                "{name}: {why:?}"
            );
            assert_eq!(state(&w), before, "{name}: {why:?}");
        }
        cast(&mut w, p, CORRUPTION, Target::Tile(q));
        assert!(w.is_corrupted(q), "{name}");
        let events = purify(&mut w, vec![spearmen, shaman]);
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        for turn in 1..PURIFY_TURNS {
            next_turn(&mut w, p);
            assert!(w.is_corrupted(q), "{name}: clean after {turn} turns");
        }
        let events = next_turn(&mut w, p);
        assert!(!w.is_corrupted(q), "{name}: {events:?}");
        assert!(
            events.contains(&Event::LandPurified { pos: q }),
            "{name}: {events:?}"
        );

        // Moved away in the middle, the work is lost.
        cast(&mut w, p, CORRUPTION, Target::Tile(q));
        purify(&mut w, vec![shaman]);
        let aside = around(&w, q)
            .into_iter()
            .find(|&r| bare(&w, r))
            .unwrap_or_else(|| panic!("{name}: no bare square beside the field"));
        w.cheat_move_unit(shaman, aside);
        next_turn(&mut w, p);
        w.cheat_move_unit(shaman, q);
        for _ in 0..PURIFY_TURNS {
            next_turn(&mut w, p);
        }
        assert!(
            w.is_corrupted(q),
            "{name}: cleaned without a purifier at work"
        );
    }
}

#[test]
fn corrupted_land_heals_by_itself_in_time() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        w.cheat_reveal(p);
        let (_, q) = field(&w, p);
        cast(&mut w, p, CORRUPTION, Target::Tile(q));
        assert!(w.is_corrupted(q), "{name}");
        let chance = w
            .content()
            .rules
            .config
            .spell_effects
            .corruption_fade_percent;
        assert!(chance > 0, "the content lets corruption fade");
        // Far more rounds than the chance asks for.
        let rounds = 2000 / chance;
        let healed = (0..rounds).any(|_| {
            w.end_round();
            !w.is_corrupted(q)
        });
        assert!(healed, "{name}: corrupted after {rounds} rounds");
    }
}

#[test]
fn consecration_cleans_a_citys_land_at_once_and_gaias_blessing_in_time() {
    for (spell, at_once) in [(CONSECRATION, true), (GAIAS_BLESSING, false)] {
        for (name, mut w) in worlds() {
            let p = w.wizards()[0];
            w.cheat_reveal(p);
            let (city, q) = field(&w, p);
            cast(&mut w, p, CORRUPTION, Target::Tile(q));
            assert!(w.is_corrupted(q), "{name}");
            w.scenario_enchant_city(city, SpellId::new(spell), p);
            let mut turns = 0;
            while w.is_corrupted(q) {
                turns += 1;
                assert!(turns <= 200, "{name}: spell {spell} never cleans");
                next_turn(&mut w, p);
            }
            if at_once {
                assert_eq!(turns, 1, "{name}");
            }
        }
    }
}

#[test]
fn gaias_blessing_greens_the_deserts_of_its_city() {
    let desert = |w: &World, q| w.economy().terrain_class_at(q) == Some(TerrainClass::Desert);
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (city, q) = field(&w, p);
        let sand = squares(&w, q.plane)
            .find(|&r| desert(&w, r))
            .and_then(|r| w.map()?.plane(r.plane).get(r.x as i32, r.y as i32))
            .unwrap_or_else(|| panic!("{name}: a world without desert"));
        w.scenario_set_terrain(q, sand);
        w.scenario_enchant_city(city, SpellId::new(GAIAS_BLESSING), p);
        let mut turns = 0;
        while desert(&w, q) {
            turns += 1;
            assert!(turns <= 300, "{name}: the desert stays");
            next_turn(&mut w, p);
        }
        assert_eq!(
            w.economy().terrain_class_at(q),
            Some(TerrainClass::Grassland),
            "{name}"
        );
    }
}

#[test]
fn a_warped_node_turns_against_its_owner() {
    for (name, mut w) in worlds() {
        let (p, other) = (w.wizards()[0], w.wizards()[1]);
        w.cheat_reveal(p);
        let node = w
            .economy()
            .nodes()
            .into_iter()
            .find(|n| n.owner.is_none() && w.units().units_at(n.pos).is_empty())
            .expect("a free node");
        let q = node.pos;
        assert_eq!(
            w.check_warp_node(q, p),
            Err(TargetError::NodeUnowned),
            "{name}"
        );
        let spirit = create(&mut w, other, q, MAGIC_SPIRIT);
        w.apply(Command::Meld {
            units: vec![spirit],
        });
        assert_eq!(
            w.check_warp_node(q, other),
            Err(TargetError::NodeOwn),
            "{name}"
        );
        let paid = w.node_income(other);
        assert!(paid > 0, "{name}");
        assert_eq!(w.check_warp_node(q, p), Ok(()), "{name}");
        let events = cast(&mut w, p, WARP_NODE, Target::Tile(q));
        assert_eq!(refusal(&events), None, "{name}: {events:?}");
        let warped = w.economy().node_at(q).expect("the node");
        assert!(warped.warped, "{name}");
        assert_eq!(warped.owner, Some(other), "{name}");
        assert!(w.node_income(other) < 0, "{name}: {}", w.node_income(other));
        assert_eq!(
            w.check_warp_node(q, p),
            Err(TargetError::NodeWarped),
            "{name}"
        );
        let bare_square = squares(&w, q.plane).find(|&r| bare(&w, r)).expect("land");
        assert_eq!(
            w.check_warp_node(bare_square, p),
            Err(TargetError::NoNode),
            "{name}"
        );
    }
}

// ---- exploring and seeing ----

/// Whatever a player sees it has explored, and it sees the squares it stands on.
fn sight_holds(w: &World, name: &str) {
    for p in w.wizards() {
        let economy = w.economy();
        let stands: Vec<Position> = w
            .units()
            .player_units(p)
            .into_iter()
            .filter_map(|u| w.units().unit_position(u))
            .chain(economy.cities_of(p).into_iter().map(|(_, q)| q))
            .collect();
        for plane in Plane::all() {
            let seen = economy.visibility(p, plane);
            for (q, seen) in squares(w, plane).zip(seen) {
                if seen {
                    assert!(w.is_explored(p, q), "{name}: {p:?} sees unexplored {q:?}");
                } else {
                    assert!(!stands.contains(&q), "{name}: {p:?} is blind at {q:?}");
                }
            }
        }
    }
}

#[test]
fn what_a_player_sees_is_explored_and_stays_explored() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        sight_holds(&w, &name);
        let (_, home) = w.economy().cities_of(p)[0];
        let scout = create(&mut w, p, home, SPEARMEN);
        let explored = |w: &Table| -> Vec<Position> {
            Plane::all()
                .flat_map(|plane| squares(w, plane))
                .filter(|&q| w.is_explored(p, q))
                .collect()
        };
        for step in 0..sample(60, 20) {
            let before = explored(&w);
            let here = w.units().unit_position(scout).expect("a square");
            let next: Vec<Position> = around(&w, here)
                .into_iter()
                .filter(|&q| bare(&w, q))
                .collect();
            let Some(&to) = next.get(w.next_u64() as usize % next.len().max(1)) else {
                break;
            };
            w.scenario_set_moves_left(scout, HalfInt::from_int(20));
            w.apply(Command::MoveStack {
                units: vec![scout],
                to,
            });
            let now = explored(&w);
            assert!(
                before.iter().all(|q| now.contains(q)),
                "{name}: step {step} forgot a square"
            );
            sight_holds(&w, &name);
        }
    }
}

/// The border of the known land is shaded on its last squares.
#[test]
fn explored_squares_are_counted_and_shaded_at_the_border_of_the_known_land() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let all: Vec<Position> = Plane::all().flat_map(|plane| squares(&w, plane)).collect();
        let known = all.iter().filter(|&&q| w.is_explored(p, q)).count();
        assert_eq!(w.explored_squares(p), known, "{name}");
        let mut shaded = 0;
        for &q in &all {
            let corners = w.explored_corners(p, q);
            assert!(corners <= 15, "{name}: {q:?}");
            assert_eq!(corners == 0, !w.is_explored(p, q), "{name}: {q:?}");
            if corners == 0 {
                continue;
            }
            if around(&w, q).into_iter().all(|r| w.is_explored(p, r)) {
                assert_eq!(corners, 15, "{name}: {q:?} lies inside the known land");
            } else if corners < 15 {
                shaded += 1;
            }
        }
        assert!(shaded > 0, "{name}: no border");
        let (_, home) = w.economy().cities_of(p)[0];
        w.restrict_view(p, home, HalfInt::ZERO);
        assert_eq!(w.explored_squares(p), 1, "{name}");
        assert!(w.is_explored(p, home), "{name}");
        w.cheat_reveal(p);
        assert_eq!(w.explored_squares(p), all.len(), "{name}");
        assert!(
            all.iter().all(|&q| w.explored_corners(p, q) == 15),
            "{name}"
        );
    }
}

/// The squares `p` sees on both planes.
fn in_view(w: &World, p: PlayerId) -> usize {
    Plane::all()
        .map(|plane| {
            w.economy()
                .visibility(p, plane)
                .into_iter()
                .filter(|&s| s)
                .count()
        })
        .sum()
}

#[test]
fn walls_an_oracle_and_natures_eye_make_a_city_see_further() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (city, _) = w.economy().cities_of(p)[0];
        w.scenario_set_building(city, BuildingId::new(CITY_WALLS), false);
        w.scenario_set_building(city, BuildingId::new(ORACLE), false);
        let sight = |w: &Table| w.economy().city_sight(city);
        let (plain, seen) = (sight(&w), in_view(&w, p));
        for what in [CITY_WALLS, ORACLE] {
            w.scenario_set_building(city, BuildingId::new(what), true);
            assert!(sight(&w) > plain, "{name}: building {what}");
            w.scenario_set_building(city, BuildingId::new(what), false);
            assert_eq!(sight(&w), plain, "{name}: building {what} gone");
        }
        w.scenario_enchant_city(city, SpellId::new(NATURES_EYE), p);
        assert!(sight(&w) > plain, "{name}: Nature's Eye");
        assert!(in_view(&w, p) >= seen, "{name}");
    }
}

#[test]
fn nature_awareness_shows_the_whole_world() {
    for (name, mut w) in worlds() {
        let (p, other) = (w.wizards()[0], w.wizards()[1]);
        let theirs = in_view(&w, other);
        w.scenario_add_global(p, SpellId::new(NATURE_AWARENESS));
        let all: usize = Plane::all().map(|plane| squares(&w, plane).count()).sum();
        assert_eq!(in_view(&w, p), all, "{name}");
        assert_eq!(in_view(&w, other), theirs, "{name}");
    }
}

#[test]
fn an_invisible_unit_shows_only_to_who_sees_through_illusions_nearby() {
    for Crew {
        name,
        mut w,
        p,
        way,
    } in crews()
    {
        let other = w.wizards()[1];
        let (there, here) = (way[0], way[2]);
        let hidden = create(&mut w, other, there, NIGHTBLADES);
        let shows = |w: &Table, to| w.economy().unit_visible_to(hidden, to);
        let told = |w: &Table| w.foreign_sightings(p).iter().any(|s| s.pos == there);
        assert!(shows(&w, other), "{name}: not to its own wizard");
        assert!(!shows(&w, p), "{name}");
        // Its square is in sight and still nothing is told of it.
        let watcher = create(&mut w, p, way[1], SPEARMEN);
        assert!(w.economy().visibility(p, there.plane).iter().any(|&s| s));
        assert!(!shows(&w, p), "{name}: spearmen see through nothing");
        assert!(!told(&w), "{name}");
        let seer = create(&mut w, p, here, SKELETONS);
        assert!(!shows(&w, p), "{name}: skeletons two squares off");
        w.cheat_move_unit(seer, way[1]);
        assert!(shows(&w, p), "{name}: skeletons beside it");
        assert!(told(&w), "{name}");
        w.cheat_remove_unit(seer);
        assert!(!shows(&w, p), "{name}: the seer is gone");
        w.cheat_remove_unit(watcher);
        let plain = create(&mut w, other, there, SPEARMEN);
        assert!(w.economy().unit_visible_to(plain, p), "{name}");
    }
}

/// A computer wizard's turn keeps its sight and patches it as units walk: it must say what a fresh
/// count says, after every kind of command.
#[test]
fn a_kept_sight_says_what_a_fresh_one_says() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (city, home) = w.economy().cities_of(p)[0];
        let scouts: Vec<hecs::Entity> = (0..3).map(|_| create(&mut w, p, home, SPEARMEN)).collect();
        w.keep_for_turn(p);
        let agrees = |w: &Table, after: &str| {
            let view = PlayerView::new(w, p);
            for plane in Plane::all() {
                let fresh = w.economy().visibility(p, plane);
                for (q, fresh) in squares(w, plane).zip(fresh) {
                    assert_eq!(view.sees(q), fresh, "{name}: {q:?} after {after}");
                }
            }
        };
        agrees(&w, "the start");
        for step in 0..sample(80, 25) {
            let unit = scouts[step % scouts.len()];
            if !w.units().all_units().contains(&unit) {
                continue;
            }
            let here = w.units().unit_position(unit).expect("a square");
            let next: Vec<Position> = around(&w, here)
                .into_iter()
                .filter(|&q| bare(&w, q))
                .collect();
            let Some(&to) = next.get(w.next_u64() as usize % next.len().max(1)) else {
                continue;
            };
            w.scenario_set_moves_left(unit, HalfInt::from_int(20));
            w.apply(Command::MoveStack {
                units: vec![unit],
                to,
            });
            agrees(&w, "a step");
            // Now and then something that is not a step.
            match step % 11 {
                3 => {
                    create(&mut w, p, home, SPEARMEN);
                    agrees(&w, "a new unit");
                }
                7 => {
                    w.apply(Command::DismissUnit { unit });
                    agrees(&w, "a unit gone");
                }
                9 => {
                    w.apply(Command::RenameCity {
                        city,
                        name: "Seen".to_owned(),
                    });
                    agrees(&w, "a new name");
                }
                _ => {}
            }
        }
        w.drop_kept();
        agrees(&w, "the end");
    }
}

// ---- the survey of a site ----

#[test]
fn a_survey_counts_what_the_player_has_explored_and_says_why_no_city_can_stand() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let dark = squares(&w, Plane::Arcanus)
            .find(|&q| {
                w.economy()
                    .city_area(q)
                    .iter()
                    .all(|&(r, _)| !w.is_explored(p, r))
            })
            .expect("a dark corner");
        let unknown = w.site_survey(p, dark);
        assert!(unknown.unexplored, "{name}");
        assert_eq!(
            (unknown.max_size, unknown.production_percent),
            (0, 0),
            "{name}"
        );
        w.cheat_reveal(p);
        let step = if deep() { 1 } else { 13 };
        for plane in Plane::all() {
            for q in squares(&w, plane).step_by(step) {
                let survey = w.site_survey(p, q);
                assert!(!survey.unexplored, "{name}: {q:?}");
                assert_eq!(survey.blocked, w.can_found_city(q).err(), "{name}: {q:?}");
                assert_eq!(
                    survey.max_size,
                    w.economy().site_max_size(q),
                    "{name}: {q:?}"
                );
            }
        }
        let home = w.economy().cities_of(p)[0].1;
        assert!(w.site_survey(p, home).blocked.is_some(), "{name}");
        assert!(w.site_survey(p, home).max_size > 0, "{name}");
    }
}

#[test]
fn a_river_and_a_shore_add_to_the_gold_a_survey_promises() {
    for mut c in crews() {
        let q = c.way[1];
        let ocean = squares(&c.w, q.plane)
            .find(|&r| !dry(&c.w, r))
            .and_then(|r| c.w.map()?.plane(r.plane).get(r.x as i32, r.y as i32))
            .expect("water");
        // Land all round, so that the shore is only the one made below.
        let land =
            c.w.map()
                .and_then(|m| m.plane(q.plane).get(q.x as i32, q.y as i32))
                .expect("land");
        for r in around(&c.w, q) {
            c.w.scenario_set_terrain(r, land);
        }
        c.w.cheat_set_river(q, false);
        let gold = |c: &Crew| c.w.site_survey(c.p, q).gold_percent;
        let inland = gold(&c);
        assert_eq!(inland, 0, "{}", c.name);
        assert!(c.w.cheat_set_river(q, true), "{}", c.name);
        let river = gold(&c);
        c.w.cheat_set_river(q, false);
        let beside = around(&c.w, q)[0];
        c.w.scenario_set_terrain(beside, ocean);
        let shore = gold(&c);
        c.w.cheat_set_river(q, true);
        let mouth = gold(&c);
        assert!(
            inland < shore && shore < river && river < mouth,
            "{}: {inland} {shore} {river} {mouth}",
            c.name
        );
    }
}

#[test]
fn a_square_off_the_map_is_nothing_to_the_map_rules() {
    for (name, w) in worlds() {
        let map = w.map().expect("a map");
        let off = at(Plane::Arcanus, 0, map.height() + 3);
        assert_eq!(w.turns_to_build_road(off), None, "{name}");
        assert_eq!(w.check_corrupt(off), Err(TargetError::OffMap), "{name}");
        assert!(!w.is_corrupted(off), "{name}");
    }
}
