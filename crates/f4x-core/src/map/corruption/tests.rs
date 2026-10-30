use super::*;
use crate::game::testutil::content_with_magic;
use crate::game::Command;
use crate::map::{Grid, Map, Plane};
use crate::rules::exec::Target;
use crate::rules::Registry;
use crate::unit::MovesLeft;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

fn world() -> Game {
    let mut c = content_with_magic();
    let mut units = c.rules.units.defs().to_vec();
    for u in units.iter_mut() {
        match u.id.as_str() {
            "high_men_cavalry" => u.abilities.push("purify".into()),
            "high_men_spearmen" => u.abilities.push("land_corruption".into()),
            _ => {}
        }
    }
    c.rules.units = Registry::new_sorted(units);
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(3);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.fog = crate::map::fog::Fog::new(24, 14);
    for id in [P0, P1] {
        g.add_player(id, "W");
        g.treasury_mut(id).unwrap().mana = 1000;
    }
    g.init_wizard(P0, &[("life", 2)], &[], None).unwrap();
    g
}

fn cast(g: &mut Game, id: &str, pos: Position) -> Vec<Event> {
    let s = g.content.rules.spells.id_of(id).unwrap();
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(s);
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 500;
    g.apply(Command::CastSpell {
        player: P0,
        spell: s,
        target: Some(Target::Tile(pos)),
    })
}

#[test]
fn the_corruption_spell_spoils_land_but_not_water() {
    let mut g = world();
    assert_eq!(g.check_corrupt(at(5, 5)), Ok(()));
    let events = cast(&mut g, "corruption", at(5, 5));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::LandCorrupted { .. })));
    assert!(g.is_corrupted(at(5, 5)));
    assert_eq!(
        g.check_corrupt(at(5, 5)),
        Err(TargetError::AlreadyCorrupted)
    );
    let ocean = g
        .content
        .terrains
        .first_of_class(TerrainClass::Ocean, Plane::Arcanus)
        .unwrap();
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set(8, 8, ocean);
    assert_eq!(g.check_corrupt(at(8, 8)), Err(TargetError::CorruptWater));
    let events = cast(&mut g, "corruption", at(8, 8));
    assert!(events.iter().any(|e| matches!(e, Event::Rejected(_))));
}

#[test]
fn magic_nodes_cannot_be_corrupted() {
    let mut g = world();
    for (x, class) in [
        (4, TerrainClass::SorceryNode),
        (6, TerrainClass::NatureNode),
        (8, TerrainClass::ChaosNode),
    ] {
        let node = g
            .content
            .terrains
            .first_of_class(class, Plane::Arcanus)
            .unwrap();
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set(x, 5, node);
        assert_eq!(g.check_corrupt(at(x, 5)), Err(TargetError::CorruptNode));
        let events = cast(&mut g, "corruption", at(x, 5));
        assert!(events.iter().any(|e| matches!(e, Event::Rejected(_))));
        assert!(!g.is_corrupted(at(x, 5)));
    }
}

#[test]
fn corrupted_squares_give_a_city_nothing() {
    let mut g = world();
    g.put_feature(at(10, 8), "gold");
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(10, 6),
        name: "T".into(),
        race: g.content.rules.races.id_of("high_elf"),
        population: 4,
    });
    let city = g.economy().city_at(at(10, 6)).unwrap();
    let before = g.economy().city_economy(city).unwrap();
    assert!(before.gold >= 3, "the gold ore pays");
    let area: Vec<Position> = g
        .economy()
        .city_area(at(10, 6))
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    for p in area {
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_corrupted(p.x, p.y, true);
    }
    let after = g.economy().city_economy(city).unwrap();
    assert!(after.gold < before.gold, "no ore bonus from spoiled land");
    assert!(after.max_size < before.max_size, "less food from the land");
}

#[test]
fn purifiers_clean_a_square_in_three_turns_and_moving_cancels() {
    let mut g = world();
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set_corrupted(5, 5, true);
    let walker = g.content.rules.units.id_of("high_men_cavalry").unwrap();
    let slow = g.content.rules.units.id_of("high_men_spearmen").unwrap();
    let cleric = g.spawn_unit(P0, at(5, 5), walker);
    let other = g.spawn_unit(P0, at(5, 5), slow);
    assert_eq!(g.purify(&[]), Err(PurifyError::NoUnits));
    assert_eq!(g.purify(&[other]), Err(PurifyError::CannotPurify));
    let clean_spot = g.spawn_unit(P0, at(7, 7), walker);
    assert_eq!(g.purify(&[clean_spot]), Err(PurifyError::NotCorrupted));
    g.purify(&[cleric, other]).unwrap();
    assert!(g.process_purify().is_empty());
    assert!(g.is_corrupted(at(5, 5)), "two turns is not enough");
    assert!(g.process_purify().is_empty());
    let events = g.process_purify();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::LandPurified { .. })));
    assert!(!g.is_corrupted(at(5, 5)));
    // Moving away cancels the work.
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set_corrupted(5, 5, true);
    g.purify(&[cleric]).unwrap();
    let _ = g.world.insert_one(cleric, at(6, 5));
    g.process_purify();
    assert!(g.world.get::<&PurifyWork>(cleric).is_err());
}

#[test]
fn a_unit_that_stays_put_corrupts_its_square_when_it_has_land_corruption() {
    let mut g = world();
    let slow = g.content.rules.units.id_of("high_men_spearmen").unwrap();
    let still = g.spawn_unit(P0, at(5, 5), slow);
    let moved = g.spawn_unit(P0, at(9, 9), slow);
    let _ = g.world.insert_one(moved, MovesLeft(0));
    g.apply(Command::EndTurn);
    assert!(g.is_corrupted(at(5, 5)));
    assert!(!g.is_corrupted(at(9, 9)), "it moved this turn");
    let _ = still;
}

#[test]
fn gaias_blessing_cleans_land_and_greens_deserts() {
    let mut g = world();
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(10, 6),
        name: "T".into(),
        race: g.content.rules.races.id_of("high_elf"),
        population: 3,
    });
    let city = g.economy().city_at(at(10, 6)).unwrap();
    let gaia = g.content.rules.spells.id_of("gaias_blessing").unwrap();
    g.add_city_enchantment(city, gaia, P0);
    let desert = g
        .content
        .terrains
        .first_of_class(TerrainClass::Desert, Plane::Arcanus)
        .unwrap();
    {
        let grid = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
        grid.set(11, 6, desert);
        grid.set_corrupted(9, 6, true);
    }
    for _ in 0..200 {
        g.process_gaias_blessing();
    }
    assert!(!g.is_corrupted(at(9, 6)));
    assert_eq!(
        g.economy().terrain_class_at(at(11, 6)),
        Some(TerrainClass::Grassland)
    );
    // Without the spell nothing changes.
    g.clear_city_enchantments(city);
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set_corrupted(9, 6, true);
    for _ in 0..200 {
        g.process_gaias_blessing();
    }
    assert!(g.is_corrupted(at(9, 6)));
}

#[test]
fn standing_in_a_tower_claims_it_and_shows_both_planes() {
    let mut g = world();
    let tower = g
        .content
        .features
        .first_of_kind(crate::feature::def::FeatureKind::Tower)
        .unwrap();
    g.spawn_feature(crate::feature::entity::Placement {
        at: at(8, 8),
        feature: tower,
        node: None,
        lair: None,
    });
    let walker = g.content.rules.units.id_of("high_men_cavalry").unwrap();
    let u = g.spawn_unit(P0, at(7, 8), walker);
    assert_eq!(g.economy().tower_owner(at(8, 8)), None);
    g.move_stack(&[u], at(8, 8)).unwrap();
    assert_eq!(g.economy().tower_owner(at(8, 8)), Some(PlayerId(0)));
    let other_plane = Position {
        plane: Plane::Myrror,
        ..at(8, 8)
    };
    assert!(g.fog.is_explored(P0, other_plane.plane, 8, 8));
    // A rival takes it over by standing there; the seal stops claims.
    let rival = g.spawn_unit(P1, at(9, 8), walker);
    let _ = g.world.insert_one(u, at(7, 9));
    g.move_stack(&[rival], at(8, 8)).unwrap();
    assert_eq!(g.economy().tower_owner(at(8, 8)), Some(PlayerId(1)));
    let seal = g.content.rules.spells.id_of("planar_seal");
    if let Some(s) = seal {
        g.attach_enchantment(Target::Player(P0), s, P0);
        let third = g.spawn_unit(P0, at(7, 8), walker);
        g.move_stack(&[third], at(8, 8))
            .unwrap_or(crate::unit::movement::Entry::Free);
        assert_eq!(g.economy().tower_owner(at(8, 8)), Some(PlayerId(1)));
    }
}

#[test]
fn warp_node_turns_a_rivals_node_against_them() {
    use crate::feature::node::Node;
    let mut g = world();
    g.put_node_data(
        at(6, 6),
        "node",
        Node {
            realm: "chaos".into(),
            power: 8,
            aura: vec![(6, 6)],
            guardian: false,
            warped: false,
        },
        Some(PlayerId(1)),
    );
    assert_eq!(g.check_warp_node(at(6, 6), P0), Ok(()));
    assert_eq!(g.check_warp_node(at(6, 6), P1), Err(TargetError::NodeOwn));
    assert_eq!(g.check_warp_node(at(7, 7), P0), Err(TargetError::NoNode));
    let events = cast(&mut g, "warp_node", at(6, 6));
    assert!(
        events.iter().any(|e| matches!(e, Event::NodeWarped { .. })),
        "{events:?}"
    );
    assert_eq!(g.node_income(P1), -5);
    assert_eq!(
        g.check_warp_node(at(6, 6), P0),
        Err(TargetError::NodeWarped)
    );
}

#[test]
fn consecration_cleans_every_corrupted_square_of_its_city_at_once() {
    let mut g = world();
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(10, 6),
        name: "T".into(),
        race: g.content.rules.races.id_of("high_elf"),
        population: 3,
    });
    let city = g.economy().city_at(at(10, 6)).unwrap();
    {
        let grid = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
        grid.set_corrupted(9, 6, true);
        grid.set_corrupted(11, 7, true);
    }
    g.process_gaias_blessing();
    assert!(g.is_corrupted(at(9, 6)), "no spell, no cleaning");
    let consecration = g.content.rules.spells.id_of("consecration").unwrap();
    g.add_city_enchantment(city, consecration, P0);
    g.process_gaias_blessing();
    assert!(!g.is_corrupted(at(9, 6)));
    assert!(!g.is_corrupted(at(11, 7)));
}
