use super::*;
use crate::game::testutil::{num, shipped};
use crate::map::{Grid, Map, Plane, Position};
use crate::testdir::TestResult;

use crate::game::Command;
use crate::rules::exec::Target;
use crate::rules::Registry;
use crate::spell::target_error::TargetError;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// Two wizards on grassland, with the spells and buildings these tests need added to the
/// test content.
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
        g.treasury_mut(id).expect("treasury_mut succeeds").mana = 1000;
        g.player_component_mut::<crate::player::Tax>(id)
            .expect("value succeeds")
            .0 = 4;
    }
    g.init_wizard(P0, &[("life", 2)], &[], None)
        .expect("init_wizard succeeds");
    g.init_wizard(P1, &[("chaos", 2)], &[], None)
        .expect("init_wizard succeeds");
    g
}

fn found(g: &mut Game, owner: PlayerId, pos: Position, pop: u32) -> hecs::Entity {
    let race = g.content.rules.races.id_of("high_elf");
    g.apply(Command::FoundCity {
        owner,
        pos,
        name: "Town".into(),
        race,
        population: pop,
    });
    g.economy().city_at(pos).expect("city_at succeeds")
}

fn add_building(g: &mut Game, city: hecs::Entity, id: &str) {
    let b = g.content.rules.buildings.id_of(id).expect("id_of succeeds");
    g.world
        .get::<&mut Buildings>(city)
        .expect("value succeeds")
        .0
        .push(b);
}

fn enchant(g: &mut Game, city: hecs::Entity, spell: &str, caster: PlayerId) {
    let s = g.content.rules.spells.id_of(spell).expect("id_of succeeds");
    g.add_city_enchantment(city, s, caster);
}

#[test]
fn wall_of_stone_builds_walls_once() -> TestResult {
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    let spell = g
        .content
        .rules
        .spells
        .id_of("wall_of_stone")
        .ok_or("wall_of_stone is not in the content")?;
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(spell);
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 500;
    assert!(!g.economy().city_has_building(city, "city_walls"));
    let cast = |g: &mut Game| {
        g.apply(Command::CastSpell {
            player: P0,
            spell,
            target: Some(Target::City(city)),
        })
    };
    let events = cast(&mut g);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::ProductionCompleted { .. })));
    assert!(g.economy().city_has_building(city, "city_walls"));
    assert_eq!(
        g.check_target(P0, spell, Some(Target::City(city))),
        Err(crate::wizard::magic::MagicError::Target(
            crate::spell::target_error::TargetError::Walled
        )),
        "a walled city is refused"
    );
    // Nor does it work on somebody else's city.
    let other = found(&mut g, P1, at(18, 6), 4);
    assert_eq!(
        g.check_target(P0, spell, Some(Target::City(other))),
        Err(crate::wizard::magic::MagicError::Target(
            TargetError::NotYourCity
        ))
    );
    Ok(())
}

#[test]
fn evil_presence_destroys_the_power_of_shrines_unless_the_owner_has_death_magic() -> TestResult {
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 10);
    let mana = |g: &Game| {
        g.economy()
            .city_economy(city)
            .expect("city_economy succeeds")
            .mana
    };
    let rebels = |g: &Game| {
        g.economy()
            .city_economy(city)
            .expect("city_economy succeeds")
            .rebels
    };
    let bare = mana(&g);
    add_building(&mut g, city, "shrine");
    add_building(&mut g, city, "temple");
    let blessed = mana(&g);
    assert!(blessed > bare, "the buildings make power");
    let calm = rebels(&g);
    enchant(&mut g, city, "evil_presence", P1);
    assert_eq!(mana(&g), bare, "Evil Presence destroys it");
    assert_eq!(rebels(&g), calm, "but they still pacify");
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Books>()
        .0
        .insert("death".into(), 1);
    assert_eq!(mana(&g), blessed, "a wizard with death magic is unharmed");
    Ok(())
}

#[test]
fn divine_and_infernal_power_add_half_again_to_the_religious_buildings() -> TestResult {
    use crate::rules::stats::compute;
    use crate::rules::Stat;
    use crate::wizard::magic::Retort;
    for retort in [Retort::DivinePower, Retort::InfernalPower] {
        let mut g = world();
        let city = found(&mut g, P0, at(10, 6), 10);
        for b in ["shrine", "temple", "parthenon"] {
            add_building(&mut g, city, b);
        }
        let totals = |g: &Game| {
            let mods = g.economy().city_modifiers(city);
            (
                compute(Stat::CityMana, 0, &mods),
                -compute(Stat::CityUnrest, 0, &mods),
            )
        };
        let (power, pacify) = totals(&g);
        assert!(power >= 6 && pacify >= 3, "{power} {pacify}");
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .insert(retort);
        assert_eq!(totals(&g), (power * 3 / 2, pacify + pacify / 2));
    }
    Ok(())
}

#[test]
fn the_caster_pays_upkeep_for_enchantments_on_enemy_cities() -> TestResult {
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    let mut defs = g.content.rules.spells.defs().to_vec();
    defs.iter_mut()
        .find(|d| d.id == "chaos_rift")
        .ok_or("find is missing")?
        .upkeep = 10;
    g.content.rules.spells = Registry::new(defs);
    enchant(&mut g, city, "chaos_rift", P1);
    let cost = |g: &Game, p| g.upkeep_items(p).iter().map(|i| i.mana).sum::<i32>();
    assert_eq!(cost(&g, P1), 10, "the wizard who cast it pays");
    assert_eq!(cost(&g, P0), 0, "the city's owner does not");
    Ok(())
}

#[test]
fn a_chaos_rift_bolts_the_units_inside_and_may_bring_a_building_down() -> TestResult {
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    add_building(&mut g, city, "shrine");
    enchant(&mut g, city, "chaos_rift", P1);
    let spear = g
        .content
        .rules
        .units
        .id_of("high_elf_spearmen")
        .ok_or("high_elf_spearmen is not in the content")?;
    let victim = g.spawn_unit(P0, at(10, 6), spear);
    let elsewhere = g.spawn_unit(P0, at(14, 3), spear);
    let hurt = |g: &Game, u| g.world.get::<&crate::unit::Hurt>(u).map_or(0, |h| h.0);
    let mut fell = false;
    for _ in 0..200 {
        let events = g.process_chaos_rifts();
        assert!(events
            .iter()
            .all(|e| matches!(e, Event::ChaosRiftStrikes { .. })));
        fell |= events.iter().any(|e| {
            matches!(
                e,
                Event::ChaosRiftStrikes {
                    building: Some(_),
                    ..
                }
            )
        });
        if g.world.get::<&crate::unit::Unit>(victim).is_err() {
            break;
        }
    }
    assert!(
        g.world.get::<&crate::unit::Unit>(victim).is_err() || hurt(&g, victim) > 0,
        "five strength 8 bolts a turn do not leave a spearman unharmed"
    );
    assert_eq!(hurt(&g, elsewhere), 0, "units outside the city are safe");
    // The building fell sooner or later (5 percent a turn); run on until it does.
    let mut turns = 0;
    while !fell && turns < 2000 {
        turns += 1;
        fell |= g.process_chaos_rifts().iter().any(|e| {
            matches!(
                e,
                Event::ChaosRiftStrikes {
                    building: Some(_),
                    ..
                }
            )
        });
    }
    assert!(fell, "a building fell");
    assert!(!g.economy().city_has_building(city, "shrine"));
    Ok(())
}

#[test]
fn a_battle_at_an_enchanted_city_starts_with_its_walls_and_shadow() -> TestResult {
    use crate::battle::field::Field;
    use crate::battle::Side;
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let defender = g.spawn_unit(P0, at(10, 6), walker);
    let attacker = g.spawn_unit(P1, at(11, 6), walker);
    g.start_battle(&[attacker], &[defender])
        .map_err(|e| format!("{e:?}"))?;
    let plain = g.battle().ok_or("battle is missing")?;
    assert!(!plain.wall_of_fire && !plain.wall_of_darkness);
    assert!(!plain.map.is_flying_fortress());
    assert!(plain.fields[Side::Defender.index()].is_empty());
    g.battle = None;
    for s in [
        "wall_of_fire",
        "wall_of_darkness",
        "flying_fortress",
        "cloud_of_shadow",
    ] {
        enchant(&mut g, city, s, P0);
    }
    g.start_battle(&[attacker], &[defender])
        .map_err(|e| format!("{e:?}"))?;
    let b = g.battle().ok_or("battle is missing")?;
    assert!(b.wall_of_fire && b.wall_of_darkness);
    assert!(b.map.is_flying_fortress());
    assert!(b.fields[Side::Defender.index()]
        .iter()
        .any(|&(f, _)| f == Field::Darkness));
    assert!(b.fields[Side::Attacker.index()].is_empty());
    Ok(())
}

#[test]
fn eternal_night_darkens_every_battle_but_not_at_heavenly_light() -> TestResult {
    use crate::battle::field::Field;
    use crate::battle::Side;
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let defender = g.spawn_unit(P0, at(10, 6), walker);
    let attacker = g.spawn_unit(P1, at(11, 6), walker);
    let night = g
        .content
        .rules
        .spells
        .id_of("eternal_night")
        .ok_or("eternal_night is not in the content")?;
    g.add_global(P1, night);
    let dark = |g: &Game| {
        g.battle().expect("battle succeeds").fields[Side::Defender.index()]
            .iter()
            .any(|&(f, _)| f == Field::Darkness)
    };
    g.start_battle(&[attacker], &[defender])
        .map_err(|e| format!("{e:?}"))?;
    assert!(dark(&g), "anyone's Eternal Night darkens the fight");
    g.battle = None;
    enchant(&mut g, city, "heavenly_light", P0);
    g.start_battle(&[attacker], &[defender])
        .map_err(|e| format!("{e:?}"))?;
    assert!(!dark(&g), "Heavenly Light keeps the city out of it");
    assert!(
        g.battle().ok_or("battle is missing")?.fields[Side::Defender.index()]
            .iter()
            .any(|&(f, _)| f == Field::TrueLight),
        "and gives its defender True Light"
    );
    Ok(())
}

fn realm(g: &Game, id: &str) -> crate::rules::RealmId {
    g.content.rules.realms.id_of(id).expect("id_of succeeds")
}

fn cast_ward(g: &mut Game, city: hecs::Entity, id: &str) -> Vec<Event> {
    let spell = g
        .content
        .rules
        .spells
        .id_of("spell_ward")
        .expect("id_of succeeds");
    g.wizard_mut(P0)
        .expect("value succeeds")
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(spell);
    g.wizard_mut(P0)
        .expect("value succeeds")
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 500;
    g.treasury_mut(P0).expect("treasury_mut succeeds").mana = 5000;
    let realm = realm(g, id);
    g.apply(Command::CastSpell {
        player: P0,
        spell,
        target: Some(Target::CityRealm(city, realm)),
    })
}

#[test]
fn a_spell_ward_guards_one_realm_each_time_and_costs_upkeep_for_each() -> TestResult {
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    let other = found(&mut g, P1, at(18, 6), 4);
    let ward = g
        .content
        .rules
        .spells
        .id_of("spell_ward")
        .ok_or("spell_ward is not in the content")?;
    // Only your own city, and one of the five realms.
    for (c, r, why) in [
        (other, "death", TargetError::NotYourCity),
        (city, "arcane", TargetError::NotWardRealm),
    ] {
        let realm = realm(&g, r);
        assert_eq!(
            g.check_target(P0, ward, Some(Target::CityRealm(c, realm))),
            Err(crate::wizard::magic::MagicError::Target(why))
        );
    }
    assert_eq!(
        g.check_target(P0, ward, Some(Target::City(city))),
        Err(crate::wizard::magic::MagicError::Target(
            crate::spell::target_error::TargetError::WrongTarget
        )),
        "a city alone is not enough"
    );
    cast_ward(&mut g, city, "death");
    assert_eq!(g.economy().city_wards(city), vec!["death".to_string()]);
    let upkeep = |g: &Game| {
        g.upkeep_items(P0)
            .iter()
            .filter(|i| matches!(i.kind, crate::spell::cast::UpkeepKind::City(c, _) if c == city))
            .map(|i| i.mana)
            .sum::<i32>()
    };
    assert_eq!(upkeep(&g), 5);
    let death = realm(&g, "death");
    assert_eq!(
        g.check_target(P0, ward, Some(Target::CityRealm(city, death))),
        Err(crate::wizard::magic::MagicError::Target(
            crate::spell::target_error::TargetError::Warded
        )),
        "already warded against death"
    );
    cast_ward(&mut g, city, "chaos");
    assert_eq!(
        g.economy().city_wards(city),
        vec!["death".to_string(), "chaos".into()]
    );
    assert_eq!(upkeep(&g), 10, "five for each ward");
    // Lose the spell and the wards go with it; a new one starts afresh.
    g.clear_city_enchantments(city);
    assert!(g.economy().city_wards(city).is_empty());
    cast_ward(&mut g, city, "life");
    assert_eq!(g.economy().city_wards(city), vec!["life".to_string()]);
    Ok(())
}

#[test]
fn a_spell_ward_turns_away_spells_of_its_realm_and_creatures_of_it() -> TestResult {
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    cast_ward(&mut g, city, "death");
    // P1's Evil Presence is a death spell; Chaos Rift is chaos and gets through.
    for (name, fizzles) in [("evil_presence", true), ("chaos_rift", false)] {
        let spell = g
            .content
            .rules
            .spells
            .id_of(name)
            .ok_or("id_of is missing")?;
        g.wizard_mut(P1)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(spell);
        g.wizard_mut(P1)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 500;
        let events = g.apply(Command::CastSpell {
            player: P1,
            spell,
            target: Some(Target::City(city)),
        });
        assert_eq!(
            events
                .iter()
                .any(|e| matches!(e, Event::SpellFizzled { .. })),
            fizzles,
            "{name}"
        );
        assert_eq!(
            g.economy().city_has_enchantment(city, name),
            !fizzles,
            "{name}"
        );
    }
    // A death creature cannot step into the city; a chaos one can.
    let walker = g
        .content
        .rules
        .units
        .id_of("skeletons")
        .ok_or("skeletons is not in the content")?;
    let slow = g
        .content
        .rules
        .units
        .id_of("hell_hounds")
        .ok_or("hell_hounds is not in the content")?;
    let ghoul = g.spawn_unit(P1, at(11, 6), walker);
    let imp = g.spawn_unit(P1, at(11, 7), slow);
    assert_eq!(
        g.move_stack(&[ghoul], at(10, 6)),
        Err(crate::unit::movement::MoveError::Warded)
    );
    assert!(g.move_stack(&[imp], at(10, 6)).is_ok());
    // The battle at the city knows the wards: its attackers cannot cast death spells.
    let defender = g.spawn_unit(P0, at(10, 6), walker);
    g.start_battle(&[ghoul], &[defender])
        .map_err(|e| format!("{e:?}"))?;
    let b = g.battle().ok_or("battle is missing")?;
    assert_eq!(b.warded_realms, vec!["death".to_string()]);
    assert!(b.ward_stops(crate::battle::Side::Attacker, "death"));
    assert!(!b.ward_stops(crate::battle::Side::Defender, "death"));
    assert!(!b.ward_stops(crate::battle::Side::Attacker, "chaos"));
    Ok(())
}

#[test]
fn wards_survive_a_save() -> TestResult {
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    cast_ward(&mut g, city, "death");
    cast_ward(&mut g, city, "life");
    let content = g.content.clone();
    let back = g.snapshot().restore(content)?;
    let again = back
        .economy()
        .city_at(at(10, 6))
        .ok_or("city_at is missing")?;
    assert_eq!(
        back.economy().city_wards(again),
        vec!["death".to_string(), "life".to_string()]
    );
    Ok(())
}

#[test]
fn evil_presence_is_refused_where_its_owner_can_use_death_magic() -> TestResult {
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    let spell = g
        .content
        .rules
        .spells
        .id_of("evil_presence")
        .ok_or("evil_presence is not in the content")?;
    assert_eq!(g.check_target(P1, spell, Some(Target::City(city))), Ok(()));
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Books>()
        .0
        .insert("death".into(), 1);
    assert_eq!(
        g.check_target(P1, spell, Some(Target::City(city))),
        Err(crate::wizard::magic::MagicError::Target(
            crate::spell::target_error::TargetError::EvilPresenceWasted
        ))
    );
    Ok(())
}

#[test]
fn another_wizards_armageddon_adds_two_unrest_and_leaves_the_buildings_working() -> TestResult {
    use crate::rules::stats::compute;
    use crate::rules::Stat;
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 10);
    for b in ["shrine", "temple", "parthenon"] {
        add_building(&mut g, city, b);
    }
    let pacify = |g: &Game| -compute(Stat::CityUnrest, 0, &g.economy().city_modifiers(city));
    let calm = pacify(&g);
    assert!(calm >= 3);
    let a = g
        .content
        .rules
        .spells
        .id_of("armageddon")
        .ok_or("armageddon is not in the content")?;
    g.add_global(P1, a);
    assert_eq!(
        pacify(&g),
        calm - 2,
        "an enemy's Armageddon: two more unrest"
    );
    g.clear_globals(P1);
    g.add_global(P0, a);
    assert_eq!(pacify(&g), calm, "the owner's own changes nothing");
    // Great Wasting does one, and Armageddon's two are not added to it.
    g.clear_globals(P0);
    let wasting = g
        .content
        .rules
        .spells
        .id_of("great_wasting")
        .ok_or("great_wasting is not in the content")?;
    g.add_global(P1, wasting);
    assert_eq!(pacify(&g), calm - 1, "an enemy's Great Wasting: one more");
    g.add_global(P1, a);
    assert_eq!(pacify(&g), calm - 2);
    Ok(())
}

#[test]
fn corrupted_squares_starve_and_unsettle_a_city_unless_its_wizard_keeps_great_wasting() -> TestResult
{
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 10);
    let eco = |g: &Game| {
        g.economy()
            .city_economy(city)
            .expect("city_economy succeeds")
    };
    let clean = eco(&g);
    let tiles = [(9, 6), (11, 6), (10, 5), (10, 7)];
    for (x, y) in tiles {
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set_corrupted(x, y, true);
    }
    let blighted = eco(&g);
    assert!(blighted.food < clean.food, "no food from them");
    assert!(
        blighted.rebels > clean.rebels,
        "{} -> {}",
        clean.rebels,
        blighted.rebels
    );
    // Its owner's Great Wasting: the same yields as before, and a mana a square.
    let gw = g
        .content
        .rules
        .spells
        .id_of("great_wasting")
        .ok_or("great_wasting is not in the content")?;
    g.add_global(P0, gw);
    let proof = eco(&g);
    assert_eq!(proof.food, clean.food);
    assert_eq!(proof.rebels, clean.rebels);
    assert_eq!(proof.mana, clean.mana, "the city's own mana is not changed");
    assert_eq!(
        g.economy().blight_power(P0),
        4,
        "four squares, a power each"
    );
    // Someone else's Great Wasting does not protect it.
    g.clear_globals(P0);
    g.add_global(P1, gw);
    // (Its unrest is one more, which may cost the city a farmer too.)
    assert!(eco(&g).food <= blighted.food);
    assert!(eco(&g).rebels >= blighted.rebels);
    assert_eq!(g.economy().blight_power(P0), 0);
    // And the protection ends with the spell.
    g.clear_globals(P1);
    assert_eq!(eco(&g).food, blighted.food);
    Ok(())
}

#[test]
fn a_battle_under_a_node_knows_its_aura_and_whether_it_stands_on_the_node() -> TestResult {
    use crate::battle::immunity::Realm;
    let mut g = world();
    let chaos = g
        .content
        .terrains
        .first_of_class(crate::map::terrain::TerrainClass::ChaosNode, Plane::Arcanus)
        .ok_or("first_of_class is missing")?;
    g.map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set(7, 6, chaos);
    g.put_node_data(
        at(7, 6),
        num::feature::SORCERY_NODE,
        crate::feature::node::Node {
            realm: "chaos".into(),
            power: 2,
            aura: vec![(7, 6), (8, 6)],
            guardian: false,
            warped: false,
        },
        None,
    );
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    for (x, aura, site) in [
        (7, Some(Realm::Chaos), Some(Realm::Chaos)),
        (8, Some(Realm::Chaos), None),
        (12, None, None),
    ] {
        let a = g.spawn_unit(P1, at(x, 6), walker);
        let d = g.spawn_unit(P0, at(x, 6), walker);
        g.start_battle(&[a], &[d]).map_err(|e| format!("{e:?}"))?;
        let b = g.battle().ok_or("battle is missing")?;
        assert_eq!((b.node_aura, b.node_site), (aura, site), "at x = {x}");
        if site.is_some() {
            // A chaos node's ground is mountains, as in the original.
            let set = b.map.look().ok_or("look is missing")?.set;
            assert_eq!(set, crate::battle::map::layout::Set::Mountains);
        }
        g.battle = None;
    }
    Ok(())
}

#[test]
fn consecration_turns_away_death_and_chaos_spells_but_not_the_others() -> TestResult {
    let mut g = world();
    let city = found(&mut g, P0, at(10, 6), 4);
    let consecration = g
        .content
        .rules
        .spells
        .id_of("consecration")
        .ok_or("consecration is not in the content")?;
    g.add_city_enchantment(city, consecration, P0);
    assert_eq!(
        g.economy().city_shielded_realms(city),
        vec!["death".to_string(), "chaos".to_string()]
    );
    assert!(
        g.economy().city_wards(city).is_empty(),
        "it is no spell ward"
    );
    // P1's Evil Presence (death) and Chaos Rift (chaos) are turned away.
    for name in ["evil_presence", "chaos_rift"] {
        let spell = g
            .content
            .rules
            .spells
            .id_of(name)
            .ok_or("id_of is missing")?;
        g.wizard_mut(P1)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(spell);
        g.wizard_mut(P1)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 500;
        let events = g.apply(Command::CastSpell {
            player: P1,
            spell,
            target: Some(Target::City(city)),
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::SpellFizzled { .. })),
            "{name}"
        );
    }
    Ok(())
}
