use super::*;
use crate::game::testutil::{num, shipped};
use crate::game::Command;
use crate::map::{Grid, Map, Plane};
use crate::rules::exec::Target;
use crate::rules::RealmId;
use f4x_def::testdir::TestResult;

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
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("id_of succeeds");
    let mut g = Game::new(3);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.fog = crate::map::fog::Fog::new(24, 14);
    for id in [P0, P1] {
        g.add_player(id, "W");
        g.treasury_mut(id).expect("treasury_mut succeeds").mana =
            f4x_def::half_int::HalfInt::from_int(1000);
    }
    g.init_wizard(P0, &[(RealmId::new(3 /* life */), 2)], &[], None)
        .expect("init_wizard succeeds");
    g
}

fn cast(g: &mut Game, id: u16, pos: Position) -> Vec<Event> {
    let s = g
        .content
        .rules
        .spells
        .id_numbered(id)
        .expect("id_of succeeds");
    g.wizard_mut(P0)
        .expect("value succeeds")
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(s);
    g.wizard_mut(P0)
        .expect("value succeeds")
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 500;
    g.apply(Command::CastSpell {
        extra: 0,
        player: P0,
        spell: s,
        target: Some(Target::Tile(pos)),
    })
}

#[test]
fn the_corruption_spell_spoils_land_but_not_water() -> TestResult {
    let mut g = world();
    assert_eq!(g.check_corrupt(at(5, 5)), Ok(()));
    let events = cast(&mut g, 85 /* corruption */, at(5, 5));
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
        .ok_or("first_of_class is missing")?;
    g.map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set(8, 8, ocean);
    assert_eq!(g.check_corrupt(at(8, 8)), Err(TargetError::CorruptWater));
    let events = cast(&mut g, 85 /* corruption */, at(8, 8));
    assert!(events.iter().any(|e| matches!(e, Event::Rejected(_))));
    Ok(())
}

#[test]
fn magic_nodes_cannot_be_corrupted() -> TestResult {
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
            .ok_or("first_of_class is missing")?;
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set(x, 5, node);
        assert_eq!(g.check_corrupt(at(x, 5)), Err(TargetError::CorruptNode));
        let events = cast(&mut g, 85 /* corruption */, at(x, 5));
        assert!(events.iter().any(|e| matches!(e, Event::Rejected(_))));
        assert!(!g.is_corrupted(at(x, 5)));
    }
    Ok(())
}

#[test]
fn corrupted_squares_give_a_city_nothing() -> TestResult {
    let mut g = world();
    g.put_feature(at(10, 8), num::feature::GOLD_ORE);
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(10, 6),
        name: "T".into(),
        race: g.content.rules.races.id_numbered(7),
        population: 4,
    });
    let city = g.economy().city_at(at(10, 6)).ok_or("city_at is missing")?;
    let before = g
        .economy()
        .city_economy(city)
        .ok_or("city_economy is missing")?;
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
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set_corrupted(p.x, p.y, true);
    }
    let after = g
        .economy()
        .city_economy(city)
        .ok_or("city_economy is missing")?;
    assert!(after.gold < before.gold, "no ore bonus from spoiled land");
    assert!(after.max_size < before.max_size, "less food from the land");
    Ok(())
}

#[test]
fn purifiers_clean_a_square_in_three_turns_and_moving_cancels() -> TestResult {
    let mut g = world();
    g.map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set_corrupted(5, 5, true);
    let walker = g
        .content
        .rules
        .units
        .id_numbered(108)
        .ok_or("high_men_priests is not in the content")?;
    let slow = g
        .content
        .rules
        .units
        .id_numbered(104)
        .ok_or("high_men_spearmen is not in the content")?;
    let cleric = g.spawn_unit(P0, at(5, 5), walker);
    let other = g.spawn_unit(P0, at(5, 5), slow);
    assert_eq!(g.purify(&[]), Err(PurifyError::NoUnits));
    assert_eq!(g.purify(&[other]), Err(PurifyError::CannotPurify));
    let clean_spot = g.spawn_unit(P0, at(7, 7), walker);
    assert_eq!(g.purify(&[clean_spot]), Err(PurifyError::NotCorrupted));
    g.purify(&[cleric, other]).map_err(|e| format!("{e:?}"))?;
    assert!(g.process_purify_all().is_empty());
    assert!(g.is_corrupted(at(5, 5)), "two turns is not enough");
    assert!(g.process_purify_all().is_empty());
    let events = g.process_purify_all();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::LandPurified { .. })));
    assert!(!g.is_corrupted(at(5, 5)));
    // Moving away cancels the work.
    g.map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set_corrupted(5, 5, true);
    g.purify(&[cleric]).map_err(|e| format!("{e:?}"))?;
    let _ = g.world.insert_one(cleric, at(6, 5));
    g.process_purify_all();
    assert!(g.world.get::<&PurifyWork>(cleric).is_err());
    Ok(())
}

#[test]
fn gaias_blessing_cleans_land_and_greens_deserts() -> TestResult {
    let mut g = world();
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(10, 6),
        name: "T".into(),
        race: g.content.rules.races.id_numbered(7),
        population: 3,
    });
    let city = g.economy().city_at(at(10, 6)).ok_or("city_at is missing")?;
    let gaia = g
        .content
        .rules
        .spells
        .id_numbered(29)
        .ok_or("gaias_blessing is not in the content")?;
    g.add_city_enchantment(city, gaia, P0);
    let desert = g
        .content
        .terrains
        .first_of_class(TerrainClass::Desert, Plane::Arcanus)
        .ok_or("first_of_class is missing")?;
    {
        let grid = g
            .map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus);
        grid.set(11, 6, desert);
        grid.set_corrupted(9, 6, true);
    }
    for _ in 0..200 {
        g.process_gaias_blessing_all();
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
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set_corrupted(9, 6, true);
    for _ in 0..200 {
        g.process_gaias_blessing_all();
    }
    assert!(g.is_corrupted(at(9, 6)));
    Ok(())
}

#[test]
fn standing_in_a_tower_claims_it_and_shows_both_planes() -> TestResult {
    let mut g = world();
    let tower = g
        .content
        .features
        .first_of_kind(crate::feature::def::FeatureKind::Tower)
        .ok_or("first_of_kind is missing")?;
    g.spawn_feature(crate::feature::entity::Placement {
        at: at(8, 8),
        feature: tower,
        node: None,
        lair: None,
    });
    let walker = g
        .content
        .rules
        .units
        .id_numbered(108)
        .ok_or("high_men_priests is not in the content")?;
    let u = g.spawn_unit(P0, at(7, 8), walker);
    assert_eq!(g.economy().tower_owner(at(8, 8)), None);
    g.move_stack(&[u], at(8, 8)).map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.economy().tower_owner(at(8, 8)), Some(PlayerId(0)));
    let other_plane = Position {
        plane: Plane::Myrror,
        ..at(8, 8)
    };
    assert!(g.fog.is_explored(P0, other_plane.plane, 8, 8));
    // A rival takes it over by standing there.
    let rival = g.spawn_unit(P1, at(9, 8), walker);
    let _ = g.world.insert_one(u, at(7, 9));
    g.move_stack(&[rival], at(8, 8))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.economy().tower_owner(at(8, 8)), Some(PlayerId(1)));
    let seal = g
        .content
        .rules
        .spells
        .id_numbered(135)
        .ok_or("the seal spell is not in the content")?;
    g.attach_enchantment(Target::Player(P0), seal, P0);
    // Under the seal the tower is still claimed (the rival has to leave it first).
    let _ = g.world.insert_one(rival, at(9, 9));
    let third = g.spawn_unit(P0, at(7, 8), walker);
    g.move_stack(&[third], at(8, 8))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.economy().tower_owner(at(8, 8)), Some(PlayerId(0)));
    Ok(())
}

#[test]
fn a_tower_claimed_under_the_planar_seal_shows_no_other_plane() -> TestResult {
    let mut g = world();
    let tower = g
        .content
        .features
        .first_of_kind(crate::feature::def::FeatureKind::Tower)
        .ok_or("first_of_kind is missing")?;
    g.spawn_feature(crate::feature::entity::Placement {
        at: at(8, 8),
        feature: tower,
        node: None,
        lair: None,
    });
    let seal = g
        .content
        .rules
        .spells
        .id_numbered(135)
        .ok_or("the seal spell is not in the content")?;
    g.attach_enchantment(Target::Player(P1), seal, P1);
    let walker = g
        .content
        .rules
        .units
        .id_numbered(108)
        .ok_or("high_men_priests is not in the content")?;
    let u = g.spawn_unit(P0, at(7, 8), walker);
    g.move_stack(&[u], at(8, 8)).map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.economy().tower_owner(at(8, 8)), Some(PlayerId(0)));
    assert!(g.fog.is_explored(P0, Plane::Arcanus, 8, 8));
    assert!(!g.fog.is_explored(P0, Plane::Myrror, 8, 8));
    Ok(())
}

#[test]
fn warp_node_turns_a_rivals_node_against_them() {
    use crate::feature::node::Node;
    let mut g = world();
    g.put_node_data(
        at(6, 6),
        num::feature::SORCERY_NODE,
        Node {
            realm: crate::game::testutil::realm(2 /* chaos */),
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
    let events = cast(&mut g, 186 /* warp_node */, at(6, 6));
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
fn consecration_cleans_every_corrupted_square_of_its_city_at_once() -> TestResult {
    let mut g = world();
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(10, 6),
        name: "T".into(),
        race: g.content.rules.races.id_numbered(7),
        population: 3,
    });
    let city = g.economy().city_at(at(10, 6)).ok_or("city_at is missing")?;
    {
        let grid = g
            .map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus);
        grid.set_corrupted(9, 6, true);
        grid.set_corrupted(11, 7, true);
    }
    g.process_gaias_blessing_all();
    assert!(g.is_corrupted(at(9, 6)), "no spell, no cleaning");
    let consecration = g
        .content
        .rules
        .spells
        .id_numbered(155)
        .ok_or("consecration is not in the content")?;
    g.add_city_enchantment(city, consecration, P0);
    g.process_gaias_blessing_all();
    assert!(!g.is_corrupted(at(9, 6)));
    assert!(!g.is_corrupted(at(11, 7)));
    Ok(())
}
