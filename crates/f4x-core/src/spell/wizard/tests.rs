use super::*;
use crate::game::testutil::{num, shipped};
use crate::map::{Grid, Map, Plane, Position};
use crate::spell::target_error::TargetError;
use crate::testdir::TestResult;

use crate::game::Command;
use crate::rules::exec::Target;
use crate::rules::{Registry, UnitKind};
use rand::SeedableRng;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// Three wizards on grassland; "high_men_cavalry" is a normal unit, "high_men_spearmen" a fantastic creature of chaos,
/// "high_elf_pegasai" flies and "magic_spirit" has no body.
fn world() -> Game {
    let mut c = shipped();
    let mut units = c.rules.units.defs().to_vec();
    for u in units.iter_mut() {
        match u.id.as_str() {
            "high_men_cavalry" => u.kind = UnitKind::Normal,
            "high_elf_pegasai" | "magic_spirit" => u.kind = UnitKind::Normal,
            "high_men_spearmen" => {
                u.kind = UnitKind::Summoned;
                u.realm = Some("chaos".into());
            }
            _ => {}
        }
    }
    c.rules.units = Registry::new_sorted(units);
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("id_of succeeds");
    let mut g = Game::new(9);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.fog = crate::map::fog::Fog::new(24, 14);
    for (id, name) in [(P0, "Merlin"), (P1, "Rjak"), (P2, "Sss-ra")] {
        g.add_player(id, name);
        g.treasury_mut(id).expect("treasury_mut succeeds").mana = 5000;
        g.init_wizard(id, &[("death", 3)], &[], None)
            .expect("init_wizard succeeds");
        g.wizard_mut(id)
            .expect("value succeeds")
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 99_999;
    }
    g
}

fn sp(g: &Game, id: &str) -> crate::rules::SpellId {
    g.content.rules.spells.id_of(id).expect("id_of succeeds")
}

fn unit(g: &mut Game, owner: PlayerId, id: &str, pos: Position) -> hecs::Entity {
    let def = g.content.rules.units.id_of(id).expect("id_of succeeds");
    g.spawn_unit(owner, pos, def)
}

fn cast(g: &mut Game, spell: &str, target: Option<Target>) -> Vec<Event> {
    let s = sp(g, spell);
    g.wizard_mut(P0)
        .expect("value succeeds")
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(s);
    g.apply(Command::CastSpell {
        player: P0,
        spell: s,
        target,
    })
}

#[test]
fn drain_power_takes_what_the_victim_has_up_to_the_roll() -> TestResult {
    let mut g = world();
    g.make_contact(P0, P1);
    g.treasury_mut(P1).ok_or("treasury_mut is missing")?.mana = 1000;
    let events = cast(&mut g, "drain_power", Some(Target::Player(P1)));
    let taken = events
        .iter()
        .find_map(|e| match e {
            Event::PowerDrained { amount, .. } => Some(*amount),
            _ => None,
        })
        .ok_or_else(|| format!("{events:?}"))?;
    assert!((10..=200).contains(&taken), "{taken}");
    // The client plays the spell's picture on the victim's portrait.
    assert!(events.iter().any(|e| matches!(
        e,
        Event::SpellCastOnWizard { player, victim, .. } if *player == P0 && *victim == P1
    )));
    assert_eq!(
        g.player(P1).ok_or("player is missing")?.treasury().mana,
        1000 - taken
    );
    // A poor victim loses only what is there.
    g.treasury_mut(P1).ok_or("treasury_mut is missing")?.mana = 4;
    let events = cast(&mut g, "drain_power", Some(Target::Player(P1)));
    assert!(events.contains(&Event::PowerDrained {
        player: P0,
        victim: P1,
        amount: 4
    }));
    assert_eq!(g.player(P1).ok_or("player is missing")?.treasury().mana, 0);
    Ok(())
}

#[test]
fn a_wizard_cannot_drain_themselves() {
    let mut g = world();
    let events = cast(&mut g, "drain_power", Some(Target::Player(P0)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
}

#[test]
fn cruel_unminding_takes_a_tenth_at_most_and_at_least_one_point() -> TestResult {
    let mut g = world();
    g.make_contact(P0, P1);
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Skill>()
        .points = skill_points_for(60, false);
    let before = g.wizard(P1).ok_or("wizard is missing")?.casting_skill();
    assert_eq!(before, 60);
    cast(&mut g, "cruel_unminding", Some(Target::Player(P1)));
    let after = g.wizard(P1).ok_or("wizard is missing")?.casting_skill();
    assert!((54..=59).contains(&after), "{after}");
    // A tiny skill still loses a point.
    g.wizard_mut(P2)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Skill>()
        .points = skill_points_for(5, false);
    let before = g.wizard(P2).ok_or("wizard is missing")?.casting_skill();
    let mut rng = crate::game::rng::GameRng::new(1);
    let lost = g.cruel_unminding(P2, &mut rng);
    assert_eq!(lost, 1);
    assert_eq!(
        g.wizard(P2).ok_or("wizard is missing")?.casting_skill(),
        before - 1
    );
    Ok(())
}

#[test]
fn an_archmages_ten_points_are_safe() -> TestResult {
    let mut g = world();
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::Archmage);
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Skill>()
        .points = skill_points_for(40, true);
    assert_eq!(g.wizard(P1).ok_or("wizard is missing")?.casting_skill(), 40);
    let mut rng = crate::game::rng::GameRng::new(2);
    let lost = g.cruel_unminding(P1, &mut rng);
    assert!((1..=4).contains(&lost), "{lost}");
    assert_eq!(
        g.wizard(P1).ok_or("wizard is missing")?.casting_skill(),
        40 - lost
    );
    Ok(())
}

#[test]
fn subversion_sours_the_others_on_the_target_and_ends_their_peace() {
    let mut g = world();
    g.make_contact(P2, P1);
    g.make_contact(P0, P1);
    let before = g.relation(P2, P1);
    let events = cast(&mut g, "subversion", Some(Target::Player(P1)));
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
    assert_eq!(g.relation(P2, P1), (before - 25).max(-100));
    // Someone who never met the target feels nothing.
    let mut g = world();
    let before = g.relation(P2, P1);
    cast(&mut g, "subversion", Some(Target::Player(P1)));
    assert_eq!(g.relation(P2, P1), before);
}

#[test]
fn great_unsummoning_banishes_fantastic_creatures_of_every_wizard() {
    let mut g = world();
    let mine: Vec<_> = (0..40)
        .map(|i| unit(&mut g, P0, "high_men_spearmen", at(i % 20, 1)))
        .collect();
    let theirs: Vec<_> = (0..40)
        .map(|i| unit(&mut g, P1, "high_men_spearmen", at(i % 20, 2)))
        .collect();
    let men = unit(&mut g, P1, "high_men_cavalry", at(3, 3));
    let events = cast(&mut g, "great_unsummoning", None);
    let slain = events
        .iter()
        .find_map(|e| match e {
            Event::UnitsSlain { count, .. } => Some(*count),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{events:?}"));
    let left = mine
        .iter()
        .chain(&theirs)
        .filter(|&&u| g.world.contains(u))
        .count() as u32;
    assert_eq!(slain + left, 80);
    assert!(slain > 0, "most fail the roll");
    assert!(g.world.contains(men), "normal units are not summoned");
}

#[test]
fn death_wish_spares_the_casters_units_and_the_fantastic() {
    let mut g = world();
    let own: Vec<_> = (0..30)
        .map(|i| unit(&mut g, P0, "high_men_cavalry", at(i % 20, 1)))
        .collect();
    let enemy: Vec<_> = (0..30)
        .map(|i| unit(&mut g, P1, "high_men_cavalry", at(i % 20, 2)))
        .collect();
    let summoned = unit(&mut g, P1, "high_men_spearmen", at(1, 5));
    cast(&mut g, "death_wish", None);
    assert!(own.iter().all(|&u| g.world.contains(u)));
    assert!(g.world.contains(summoned));
    let left = enemy.iter().filter(|&&u| g.world.contains(u)).count();
    assert!(left < 30, "some failed to resist");
}

#[test]
fn an_earthquake_spares_flyers_and_the_bodiless() -> TestResult {
    let mut g = world();
    let city = {
        let race = g.content.rules.races.id_of("high_elf");
        g.apply(Command::FoundCity {
            owner: P1,
            pos: at(5, 5),
            name: "Town".into(),
            race,
            population: 4,
        });
        g.economy().city_at(at(5, 5)).ok_or("city_at is missing")?
    };
    let walkers: Vec<_> = (0..1)
        .map(|_| unit(&mut g, P1, "high_men_cavalry", at(5, 5)))
        .collect();
    let flyer = unit(&mut g, P1, "high_elf_pegasai", at(5, 5));
    let ghost = unit(&mut g, P1, "magic_spirit", at(5, 5));
    let mut rng = crate::game::rng::GameRng::new(4);
    for _ in 0..50 {
        g.earthquake(city, &mut rng);
    }
    assert!(g.world.contains(flyer));
    assert!(g.world.contains(ghost));
    assert!(
        walkers.iter().all(|&u| !g.world.contains(u)),
        "fifty tremors"
    );
    Ok(())
}

#[test]
fn an_earthquake_is_cast_at_an_enemy_city_and_reports_the_toll() -> TestResult {
    let mut g = world();
    let race = g.content.rules.races.id_of("high_elf");
    g.apply(Command::FoundCity {
        owner: P1,
        pos: at(5, 5),
        name: "Town".into(),
        race,
        population: 4,
    });
    let city = g.economy().city_at(at(5, 5)).ok_or("city_at is missing")?;
    let events = cast(&mut g, "earthquake", Some(Target::City(city)));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::EarthquakeStruck { .. })),
        "{events:?}"
    );
    // Not at one's own city.
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(9, 9),
        name: "Home".into(),
        race,
        population: 4,
    });
    let own = g.economy().city_at(at(9, 9)).ok_or("city_at is missing")?;
    let events = cast(&mut g, "earthquake", Some(Target::City(own)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    Ok(())
}

/// Black Wind on every unit of a square, as the spell does; the units destroyed.
fn black_wind_square(g: &mut Game, at: Position, rng: &mut crate::game::rng::GameRng) -> u32 {
    g.units()
        .units_at(at)
        .into_iter()
        .filter(|&u| g.black_wind_unit(u, rng))
        .count() as u32
}

#[test]
fn black_wind_strikes_every_unit_on_the_square_figure_by_figure() {
    let mut g = world();
    // A resistance of 5 against -1: each of the eight figures fails on a roll of 5 to 10, so
    // some of them rot away.
    let mut dead = 0;
    let mut hurt = 0;
    let units: Vec<_> = (0..9)
        .map(|i| unit(&mut g, P1, "barbarian_spearmen", at(i, 1)))
        .collect();
    let mut rng = crate::game::rng::GameRng::new(3);
    for i in 0..9 {
        dead += black_wind_square(&mut g, at(i, 1), &mut rng);
        let u = units[i as usize];
        if g.world.contains(u) && g.units().unit_hits(u).0 < g.units().unit_hits(u).1 {
            hurt += 1;
        }
    }
    assert!(dead + hurt > 0, "some of eight figures rot");
}

#[test]
fn black_wind_wounds_a_unit_that_resists_some_figures() -> TestResult {
    let mut g = world();
    let u = unit(&mut g, P1, "barbarian_spearmen", at(4, 4));
    g.world.insert_one(u, crate::unit::Hurt(0))?;
    // The spearmen's resistance of 5 against -1: each figure fails on a roll of 5 to 10 (60 percent).
    let mut rng = crate::game::rng::GameRng::new(11);
    let mut wounded = 0;
    let mut killed = 0;
    for _ in 0..200 {
        let u = unit(&mut g, P1, "barbarian_spearmen", at(6, 6));
        killed += black_wind_square(&mut g, at(6, 6), &mut rng);
        if g.world.contains(u) && g.units().unit_hits(u).0 < g.units().unit_hits(u).1 {
            wounded += 1;
        }
        if g.world.contains(u) {
            let _ = g.world.despawn(u);
        }
    }
    assert!(
        wounded > 0 && killed > 0,
        "{wounded} wounded, {killed} killed"
    );
    Ok(())
}

#[test]
fn black_wind_needs_a_square_with_an_enemy_on_it() {
    let mut g = world();
    unit(&mut g, P0, "high_men_cavalry", at(2, 2));
    let events = cast(&mut g, "black_wind", Some(Target::Tile(at(2, 2))));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    unit(&mut g, P1, "high_men_cavalry", at(2, 2));
    let events = cast(&mut g, "black_wind", Some(Target::Tile(at(2, 2))));
    assert!(
        events.iter().any(|e| matches!(e, Event::UnitsSlain { .. })),
        "{events:?}"
    );
}

#[test]
fn lycanthropy_makes_a_normal_unit_a_werewolf_and_forgets_its_experience() -> TestResult {
    let mut g = world();
    let u = unit(&mut g, P0, "high_men_cavalry", at(3, 3));
    g.world.insert_one(u, crate::unit::Experience(120))?;
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(u)));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::UnitTransformed { .. })),
        "{events:?}"
    );
    assert_eq!(
        g.units().unit_def(u).ok_or("unit_def is missing")?.id,
        "werewolves"
    );
    assert!(g.units().unit_stat(u, crate::rules::Stat::Figures) > 0);
    assert!(g.world.get::<&crate::unit::Experience>(u).is_err());
    // Not twice: Werewolves are fantastic.
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(u)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    Ok(())
}

#[test]
fn lycanthropy_refuses_fantastic_creatures_undead_and_other_wizards_units() {
    let mut g = world();
    let summoned = unit(&mut g, P0, "high_men_spearmen", at(1, 1));
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(summoned)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    assert_eq!(
        g.check_become_werewolf(summoned),
        Err(TargetError::Summoned)
    );
    let risen = unit(&mut g, P0, "high_men_cavalry", at(1, 2));
    g.make_undead(risen);
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(risen)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    assert_eq!(g.check_become_werewolf(risen), Err(TargetError::Undead));
    let theirs = unit(&mut g, P1, "high_men_cavalry", at(1, 3));
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(theirs)));
    assert!(
        events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
    assert_eq!(
        g.check_target(P0, lycanthropy_id(&g), Some(Target::Unit(theirs))),
        Err(crate::wizard::magic::MagicError::Target(
            TargetError::NotYourUnit
        ))
    );
}

fn lycanthropy_id(g: &Game) -> crate::rules::SpellId {
    g.content
        .rules
        .spells
        .id_of("lycanthropy")
        .expect("id_of succeeds")
}

/// Total hit points the units on a square have left.
fn hits_left(g: &Game, at: Position) -> u32 {
    g.units()
        .units_at(at)
        .into_iter()
        .map(|u| g.units().unit_hits(u).0)
        .sum()
}

#[test]
fn a_storm_hits_every_unit_on_the_square_and_only_that_square() {
    for storm in ["ice_storm", "fire_storm"] {
        let mut g = world();
        let (mine, theirs, far) = (at(5, 5), at(5, 5), at(9, 9));
        unit(&mut g, P0, "barbarian_spearmen", mine);
        unit(&mut g, P1, "barbarian_spearmen", theirs);
        unit(&mut g, P1, "barbarian_spearmen", far);
        let (before_here, before_far) = (hits_left(&g, mine), hits_left(&g, far));
        let events = cast(&mut g, storm, Some(Target::Tile(at(5, 5))));
        assert!(
            events.iter().any(|e| matches!(e, Event::SpellCast { .. })),
            "{storm}: {events:?}"
        );
        assert!(hits_left(&g, mine) < before_here, "{storm} did nothing");
        assert_eq!(hits_left(&g, far), before_far, "{storm} spilled over");
        // Both owners' units were in it: the caster's own is hurt too.
        let own = g
            .units()
            .units_at(mine)
            .into_iter()
            .find(|&u| g.units().unit_owner(u) == Some(P0));
        assert!(own.is_none_or(|u| g.units().unit_hits(u).0 < g.units().unit_hits(u).1));
    }
}

#[test]
fn magic_immunity_stops_a_storm_and_each_storm_has_its_own_element() {
    let mut g = world();
    let mut defs = g.content.rules.units.defs().to_vec();
    for (id, ability) in [
        ("high_men_cavalry", "magic_immunity"),
        ("high_elf_pegasai", "cold_immunity"),
        ("magic_spirit", "fire_immunity"),
    ] {
        defs.iter_mut()
            .filter(|d| d.id == id)
            .for_each(|d| d.abilities.push(ability.to_string()));
    }
    g.content.rules.units = Registry::new_sorted(defs);
    for (storm, safe, hurt) in [
        (
            "ice_storm",
            ["high_men_cavalry", "high_elf_pegasai"],
            "magic_spirit",
        ),
        (
            "fire_storm",
            ["high_men_cavalry", "magic_spirit"],
            "high_elf_pegasai",
        ),
    ] {
        let square = at(6, 6);
        let safe_units: Vec<_> = safe.iter().map(|s| unit(&mut g, P1, s, square)).collect();
        let weak = unit(&mut g, P1, hurt, square);
        for _ in 0..5 {
            cast(&mut g, storm, Some(Target::Tile(square)));
        }
        for u in safe_units {
            assert_eq!(
                g.units().unit_hits(u).0,
                g.units().unit_hits(u).1,
                "{storm}"
            );
            let _ = g.world.despawn(u);
        }
        let _ = g.world.despawn(weak);
    }
}

#[test]
fn a_storm_reaches_the_units_in_a_city() -> TestResult {
    let mut g = world();
    let owner = g.player_entity(P1).ok_or("player_entity is missing")?;
    let city = g.world.spawn((
        crate::city::City {
            name: "Test".into(),
        },
        crate::player::Owner(owner),
        at(7, 7),
    ));
    let _ = city;
    let u = unit(&mut g, P1, "barbarian_spearmen", at(7, 7));
    let before = g.units().unit_hits(u).0;
    let mut hurt = false;
    for _ in 0..5 {
        cast(&mut g, "fire_storm", Some(Target::Tile(at(7, 7))));
        hurt |= !g.world.contains(u) || g.units().unit_hits(u).0 < before;
    }
    assert!(hurt, "the garrison was untouched");
    Ok(())
}

#[test]
fn a_storm_needs_a_square_with_an_enemy_on_it() {
    let mut g = world();
    unit(&mut g, P0, "high_men_cavalry", at(2, 2));
    let events = cast(&mut g, "ice_storm", Some(Target::Tile(at(2, 2))));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
}

#[test]
fn a_storm_that_destroys_a_unit_says_so() {
    let mut g = world();
    let mut told = false;
    for i in 0..20 {
        unit(&mut g, P1, "barbarian_spearmen", at(3, 3));
        let events = cast(&mut g, "fire_storm", Some(Target::Tile(at(3, 3))));
        told |= events
            .iter()
            .any(|e| matches!(e, Event::UnitsSlain { count, .. } if *count > 0));
        let _ = i;
    }
    assert!(told, "twenty fire storms destroyed nothing");
}

fn city_for(g: &mut Game, owner: PlayerId, pos: Position) -> hecs::Entity {
    let race = g.content.rules.races.id_of("high_elf");
    g.apply(Command::FoundCity {
        owner,
        pos,
        name: "Town".into(),
        race,
        population: 4,
    });
    g.economy().city_at(pos).expect("city_at succeeds")
}

fn keep(g: &mut Game, player: PlayerId, spell: &str) {
    let s = sp(g, spell);
    g.add_global(player, s);
}

#[test]
fn call_the_void_kills_citizens_by_chance_but_never_the_last() {
    let mut g = world();
    let city = city_for(&mut g, P1, at(10, 6));
    g.scenario_set_citizens(city, 20);
    let events = cast(&mut g, "call_the_void", Some(Target::City(city)));
    let lost = events
        .iter()
        .find_map(|e| match e {
            Event::WorldSpellStruck {
                population_lost, ..
            } => Some(*population_lost),
            _ => None,
        })
        .expect("the spell struck");
    assert!((3..=17).contains(&lost), "about half of 19: {lost}");
    assert_eq!(g.city_population(city).figures, 20 - lost);
    let small = city_for(&mut g, P1, at(16, 8));
    g.scenario_set_citizens(small, 1);
    for _ in 0..5 {
        cast(&mut g, "call_the_void", Some(Target::City(small)));
    }
    assert_eq!(
        g.city_population(small).figures,
        1,
        "a city is never emptied"
    );
}

#[test]
fn call_the_void_hurts_everyone_inside_and_corrupts_the_land_around() {
    let mut g = world();
    let city = city_for(&mut g, P1, at(10, 6));
    let squad = unit(&mut g, P1, "barbarian_spearmen", at(10, 6)); // 10 hit points
    let walker = unit(&mut g, P1, "high_men_cavalry", at(10, 6));
    let events = cast(&mut g, "call_the_void", Some(Target::City(city)));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::WorldSpellStruck { victim: Some(v), .. } if *v == P1)),
        "{events:?}"
    );
    assert!(!g.world.contains(squad), "10 damage kills a 10 point unit");
    assert!(
        !g.world.contains(walker) || g.units().unit_hits(walker).0 < g.units().unit_hits(walker).1
    );
    let bad = (8..=12)
        .flat_map(|x| (4..=8).map(move |y| at(x, y)))
        .filter(|&p| g.is_corrupted(p))
        .count();
    assert!((3..=22).contains(&bad), "about half of 25: {bad}");
    let far = (0..24)
        .filter(|&x| !(8..=12).contains(&x))
        .any(|x| g.is_corrupted(at(x, 6)));
    assert!(!far, "nothing outside the radius");
}

#[test]
fn natures_wrath_shakes_the_cities_of_whoever_casts_death_or_chaos_while_it_lasts() -> TestResult {
    let mut g = world();
    keep(&mut g, P1, "natures_wrath");
    let city = city_for(&mut g, P0, at(5, 5));
    let walkers: Vec<_> = (0..60)
        .map(|_| unit(&mut g, P0, "high_men_cavalry", at(5, 5)))
        .collect();
    let spared = city_for(&mut g, P2, at(15, 5));
    let others: Vec<_> = (0..20)
        .map(|_| unit(&mut g, P2, "high_men_cavalry", at(15, 5)))
        .collect();
    g.make_contact(P0, P2);
    let events = cast(&mut g, "drain_power", Some(Target::Player(P2)));
    let lost = events.iter().find_map(|e| match e {
        Event::WorldSpellStruck {
            victim: Some(v),
            units_lost,
            ..
        } if *v == P0 => Some(*units_lost),
        _ => None,
    });
    let lost = lost.ok_or_else(|| format!("{events:?}"))?;
    let gone = walkers.iter().filter(|&&u| !g.world.contains(u)).count();
    assert!(lost > 0 && gone >= lost as usize, "10% of 60: lost {lost}");
    assert!(
        others.iter().all(|&u| g.world.contains(u)),
        "other cities are safe"
    );
    let _ = (city, spared);
    // The holder's own casting, or a spell of another realm, does nothing.
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 99_999;
    let s = sp(&g, "drain_power");
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(s);
    let events = g.apply(Command::CastSpell {
        player: P1,
        spell: s,
        target: Some(Target::Player(P2)),
    });
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::WorldSpellStruck { .. })));
    let events = cast(&mut g, "great_unsummoning", None);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::WorldSpellStruck { .. })),
        "sorcery"
    );
    Ok(())
}

#[test]
fn meteor_storm_strikes_units_outside_cities_but_not_inside_and_spares_the_casters_buildings() {
    let mut g = world();
    keep(&mut g, P0, "meteor_storm");
    city_for(&mut g, P1, at(5, 5));
    let inside: Vec<_> = (0..10)
        .map(|_| unit(&mut g, P1, "high_men_cavalry", at(5, 5)))
        .collect();
    let outside: Vec<_> = (0..10)
        .map(|_| unit(&mut g, P1, "high_men_cavalry", at(15, 8)))
        .collect();
    let mine: Vec<_> = (0..10)
        .map(|_| unit(&mut g, P0, "high_men_cavalry", at(18, 8)))
        .collect();
    for _ in 0..60 {
        g.process_world_enchantments();
    }
    assert!(
        inside.iter().all(|&u| g.world.contains(u)),
        "cities shelter"
    );
    assert!(
        outside.iter().all(|&u| !g.world.contains(u)),
        "sixty storms"
    );
    assert!(
        mine.iter().all(|&u| !g.world.contains(u)),
        "the caster's units too"
    );
}

#[test]
fn great_wasting_corrupts_three_to_six_land_squares_a_turn_anywhere() {
    let mut g = world();
    keep(&mut g, P0, "great_wasting");
    city_for(&mut g, P0, at(5, 5));
    let mut near = false;
    for _ in 0..40 {
        g.process_world_enchantments();
    }
    for plane in [Plane::Arcanus, Plane::Myrror] {
        for x in 0..24 {
            for y in 0..14 {
                let p = Position { plane, x, y };
                if g.is_corrupted(p)
                    && plane == Plane::Arcanus
                    && (x as i32 - 5).abs().max((y as i32 - 5).abs()) <= 2
                {
                    near = true;
                }
            }
        }
    }
    assert!(near, "the caster's own city is not spared");
    // One turn: three to six.
    let mut g = world();
    keep(&mut g, P0, "great_wasting");
    g.process_world_enchantments();
    let bad = [Plane::Arcanus, Plane::Myrror]
        .into_iter()
        .flat_map(|plane| (0..24).flat_map(move |x| (0..14).map(move |y| Position { plane, x, y })))
        .filter(|&p| g.is_corrupted(p))
        .count();
    assert!((3..=6).contains(&bad), "{bad}");
}

#[test]
fn a_battle_on_corrupted_ground_costs_no_range_penalty_while_great_wasting_lasts() -> TestResult {
    use crate::battle::Side;
    let range = |g: &mut Game| {
        let a = unit(g, P0, "high_men_cavalry", at(8, 8));
        let d = unit(g, P1, "high_men_cavalry", at(9, 8));
        g.start_battle(&[a], &[d]).expect("start_battle succeeds");
        let r = g
            .battle()
            .expect("battle succeeds")
            .caster(Side::Attacker)
            .expect("caster succeeds")
            .range;
        g.battle = None;
        r
    };
    let mut g = world();
    let far = range(&mut g);
    assert!(far > 10, "no fortress: the greatest penalty ({far})");
    g.map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set_corrupted(9, 8, true);
    assert_eq!(range(&mut g), far, "corrupted ground alone does nothing");
    keep(&mut g, P0, "great_wasting");
    assert_eq!(range(&mut g), 10, "while the spell lasts");
    g.clear_globals(P0);
    assert_eq!(range(&mut g), far, "and not after");
    Ok(())
}

#[test]
fn armageddon_raises_volcanoes_that_pay_the_caster_power() {
    let mut g = world();
    keep(&mut g, P0, "armageddon");
    g.process_world_enchantments();
    let n = g.volcano_count(P0);
    assert!((3..=6).contains(&n), "{n}");
    assert_eq!(g.volcano_count(P1), 0);
}

#[test]
fn a_volcano_pays_a_point_of_power_to_its_owner_only() -> TestResult {
    let mut g = world();
    for (x, o) in [(2, P0), (3, P1), (4, P1)] {
        let at = Position {
            plane: Plane::Myrror,
            x,
            y: 2,
        };
        g.spawn_volcano(o, at).ok_or("spawn_volcano is missing")?;
    }
    assert_eq!((g.volcano_count(P0), g.volcano_count(P1)), (1, 2));
    // Keeping Armageddon going changes nothing about whose volcanoes pay.
    keep(&mut g, P0, "armageddon");
    assert_eq!((g.volcano_count(P0), g.volcano_count(P1)), (1, 2));
    Ok(())
}

#[test]
fn an_armageddon_volcano_can_rise_in_the_sea_and_sinks_the_ships_there() -> TestResult {
    let mut g = world();
    let ocean = g
        .content
        .terrains
        .id_numbered(num::terrain::OCEAN)
        .ok_or("ocean is not in the content")?;
    g.map
        .as_mut()
        .ok_or("as_mut is missing")?
        .plane_mut(Plane::Arcanus)
        .set(6, 6, ocean);
    let boat = unit(&mut g, P1, "warship", at(6, 6));
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(3);
    assert!(g.can_armageddon_volcano(at(6, 6)));
    assert_eq!(
        g.check_raise_volcano(at(6, 6)),
        Ok(()),
        "Raise Volcano may rise in the sea too"
    );
    g.armageddon_volcano(P0, at(6, 6), &mut rng);
    assert_eq!(
        g.economy().terrain_class_at(at(6, 6)),
        Some(crate::map::terrain::TerrainClass::Mountains)
    );
    assert_eq!(g.volcano_count(P0), 1);
    assert!(!g.world.contains(boat), "the ship went down");
    assert!(!g.can_armageddon_volcano(at(6, 6)), "already a volcano");
    Ok(())
}

#[test]
fn armageddon_raises_far_fewer_volcanoes_at_sea_than_on_land() -> TestResult {
    // The west half of both planes is sea, the east half land, and the same number of squares.
    let mut g = world();
    let ocean = g
        .content
        .terrains
        .first_of_class(crate::map::terrain::TerrainClass::Ocean, Plane::Arcanus)
        .ok_or("first_of_class is missing")?;
    for plane in Plane::ALL {
        for x in 0..12 {
            for y in 0..14 {
                g.map
                    .as_mut()
                    .ok_or("as_mut is missing")?
                    .plane_mut(plane)
                    .set(x, y, ocean);
            }
        }
    }
    keep(&mut g, P0, "armageddon");
    for _ in 0..40 {
        g.process_world_enchantments();
    }
    let volcanoes = g.economy().volcanoes();
    let sea = volcanoes.iter().filter(|(p, _)| p.x < 12).count();
    let land = volcanoes.len() - sea;
    assert!(sea > 0, "volcano islands do rise");
    assert!(sea * 3 < land, "but seldom: {sea} at sea, {land} on land");
    Ok(())
}

#[test]
fn an_armageddon_volcano_under_a_city_costs_a_quarter_of_everything_but_spares_the_neighbours(
) -> TestResult {
    let mut g = world();
    let city = city_for(&mut g, P1, at(10, 6));
    let near = city_for(&mut g, P1, at(16, 6));
    let walkers: Vec<_> = (0..80)
        .map(|_| unit(&mut g, P1, "high_men_cavalry", at(10, 6)))
        .collect();
    let next_door: Vec<_> = (0..20)
        .map(|_| unit(&mut g, P1, "high_men_cavalry", at(16, 6)))
        .collect();
    g.world
        .get::<&mut crate::city::economy::Population>(city)?
        .figures = 20;
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(11);
    let events = g.armageddon_volcano(P0, at(10, 6), &mut rng);
    let gone = walkers.iter().filter(|&&u| !g.world.contains(u)).count();
    assert!((8..=32).contains(&gone), "a quarter of 80: {gone}");
    let pop = g
        .world
        .get::<&crate::city::economy::Population>(city)?
        .figures;
    assert!(
        (8..=19).contains(&pop),
        "a quarter of 20 citizens gone: {pop}"
    );
    assert!(next_door.iter().all(|&u| g.world.contains(u)));
    assert_eq!(
        g.world
            .get::<&crate::city::economy::Population>(near)?
            .figures,
        4
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::WorldSpellStruck { .. })));
    Ok(())
}

#[test]
fn volcanoes_sometimes_cool_and_sometimes_bring_a_mineral() -> TestResult {
    let mut g = world();
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(5);
    // A hundred volcanoes: about a fifth of them get a mineral.
    for x in 0..24 {
        for y in 0..5 {
            g.armageddon_volcano(P0, at(x, y), &mut rng);
        }
    }
    let minerals = (0..24)
        .flat_map(|x| (0..5).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            g.map
                .as_ref()
                .expect("value succeeds")
                .plane(Plane::Arcanus)
                .mineral(x, y)
                .is_some()
        })
        .count();
    assert!(
        (6..=40).contains(&minerals),
        "one in five of 120: {minerals}"
    );
    let before = g.volcano_count(P0);
    assert_eq!(before, 120);
    for _ in 0..50 {
        g.cool_volcanoes(&mut rng);
    }
    let after = g.volcano_count(P0);
    assert!(after < before && after > 0, "{before} -> {after}");
    Ok(())
}

#[test]
fn chaos_channels_gives_one_mutation_once_and_makes_a_creature_of_chaos() {
    use crate::unit::mutation::Mutation;
    let mut seen = std::collections::HashSet::new();
    for i in 0..40 {
        let mut g = world();
        for _ in 0..i {
            g.roll(7);
        }
        let u = unit(&mut g, P0, "high_men_cavalry", at(3, 3));
        let (def0, mv0) = (
            g.units().unit_stat(u, crate::rules::Stat::Defense),
            g.units().unit_stat(u, crate::rules::Stat::Moves),
        );
        let events = cast(&mut g, "chaos_channels", Some(Target::Unit(u)));
        let m = events
            .iter()
            .find_map(|e| match e {
                Event::UnitMutated { mutation, .. } => Some(*mutation),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{events:?}"));
        seen.insert(format!("{m:?}"));
        let units = g.units();
        assert_eq!(units.unit_realm(u).as_deref(), Some("chaos"));
        // The unit sheet names the mutation by this marker.
        let marker = crate::unit::mutation::MUTATION_MARKERS
            .iter()
            .find(|(k, _)| *k == m)
            .map(|(_, name)| *name)
            .expect("a marker");
        assert!(units.unit_has_ability(u, marker));
        match m {
            Mutation::Armor => {
                assert_eq!(units.unit_stat(u, crate::rules::Stat::Defense), def0 + 3)
            }
            Mutation::Wings => {
                assert!(units.unit_has_ability(u, "flying"));
                assert!(units.unit_stat(u, crate::rules::Stat::Moves) >= mv0.max(2));
            }
            Mutation::Breath => {
                assert!(units.unit_has_ability(u, "fire_breath"));
                assert_eq!(units.unit_stat(u, crate::rules::Stat::Ranged), 2);
            }
        }
        let again = cast(&mut g, "chaos_channels", Some(Target::Unit(u)));
        assert!(
            matches!(again.as_slice(), [Event::Rejected(_)]),
            "{again:?}"
        );
    }
    assert_eq!(seen.len(), 3, "all three turn up: {seen:?}");
}

#[test]
fn chaos_channels_refuses_fantastic_units_and_never_gives_a_flier_wings() {
    let mut g = world();
    let beast = unit(&mut g, P0, "high_men_spearmen", at(3, 3));
    let events = cast(&mut g, "chaos_channels", Some(Target::Unit(beast)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    for _ in 0..30 {
        let flyer = unit(&mut g, P0, "high_elf_pegasai", at(4, 4));
        let before = g.units().unit_stat(flyer, crate::rules::Stat::Moves);
        let mut rng = crate::game::rng::GameRng::new(g.next_u64());
        g.mutate(flyer, &mut rng);
        assert!(g.units().unit_is_mutated(flyer));
        assert!(g.units().unit_stat(flyer, crate::rules::Stat::Moves) <= before);
    }
}

#[test]
fn doom_mastery_mutates_new_units_of_its_wizard_only() {
    let mut g = world();
    keep(&mut g, P0, "doom_mastery");
    let mine = unit(&mut g, P0, "high_men_cavalry", at(3, 3));
    let theirs = unit(&mut g, P1, "high_men_cavalry", at(4, 3));
    g.doom_mastery_mutates(mine, P0);
    g.doom_mastery_mutates(theirs, P1);
    assert!(g.units().unit_is_mutated(mine));
    assert!(!g.units().unit_is_mutated(theirs));
}

#[test]
fn stasis_holds_enemy_units_on_the_square_for_a_turn_and_then_lets_them_roll_to_break_free(
) -> TestResult {
    let mut g = world();
    // No upkeep and no food, so that nobody is lost while the turns go by.
    let mut defs = g.content.rules.units.defs().to_vec();
    for d in defs.iter_mut() {
        d.upkeep = Default::default();
        d.kind = UnitKind::Fantastic;
        d.stats.insert(crate::rules::Stat::Resistance, 12);
    }
    g.content.rules.units = Registry::new_sorted(defs);
    let enemies: Vec<_> = (0..6)
        .map(|_| unit(&mut g, P1, "high_men_cavalry", at(8, 8)))
        .collect();
    let mine = unit(&mut g, P0, "high_men_cavalry", at(8, 8));
    let other = unit(&mut g, P1, "high_men_cavalry", at(9, 8));
    let events = cast(&mut g, "stasis", Some(Target::Tile(at(8, 8))));
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
    assert!(enemies.iter().all(|&u| g.unit_in_stasis(u)));
    assert!(!g.unit_in_stasis(mine) && !g.unit_in_stasis(other));
    // The first turn change: held whatever they roll, and with no moves.
    g.apply(Command::EndTurn);
    assert!(enemies.iter().all(|&u| g.unit_in_stasis(u)));
    assert!(enemies.iter().all(|&u| g.units().unit_moves_left(u) == 0));
    assert!(g.world.contains(mine), "alive");
    assert!(g.units().unit_max_moves(mine) > 0, "max moves");
    assert!(g.units().unit_moves_left(mine) > 0);
    // Later they break free one by one, and move again.
    for _ in 0..60 {
        g.apply(Command::EndTurn);
    }
    assert!(
        enemies.iter().all(|&u| !g.unit_in_stasis(u)),
        "all freed in time"
    );
    assert!(enemies.iter().all(|&u| g.units().unit_moves_left(u) > 0));
    // A square with no enemy not already held is no target.
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 99_999;
    let held = unit(&mut g, P1, "high_men_cavalry", at(12, 8));
    cast(&mut g, "stasis", Some(Target::Tile(at(12, 8))));
    assert!(g.unit_in_stasis(held));
    let again = cast(&mut g, "stasis", Some(Target::Tile(at(12, 8))));
    assert!(
        matches!(again.as_slice(), [Event::Rejected(_)]),
        "{again:?}"
    );
    Ok(())
}

#[test]
fn time_stop_freezes_everyone_but_its_wizard_who_pays_only_for_the_spell() -> TestResult {
    let mut g = world();
    let mut defs = g.content.rules.units.defs().to_vec();
    for d in defs.iter_mut() {
        d.upkeep = Default::default();
        d.kind = UnitKind::Fantastic;
    }
    g.content.rules.units = Registry::new_sorted(defs);
    let mine = unit(&mut g, P0, "high_men_cavalry", at(3, 3));
    let theirs = unit(&mut g, P1, "high_men_cavalry", at(4, 3));
    keep(&mut g, P0, "time_stop");
    g.attach_enchantment(Target::Player(P0), sp(&g, "time_stop"), P0);
    // Another global enchantment of the wizard's costs nothing now.
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.mana = 500;
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold = 100;
    g.wizard_mut(P1)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Skill>()
        .left = 5;
    g.apply(Command::EndTurn);
    assert!(g.units().unit_moves_left(mine) > 0);
    assert_eq!(
        g.units().unit_moves_left(theirs),
        0,
        "their troops are frozen"
    );
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().mana,
        300,
        "200 for Time Stop"
    );
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().gold,
        100,
        "no gold either way"
    );
    assert!(g.time_stopper() == Some(P0));
    // Out of mana, the spell is lost and time runs again.
    g.apply(Command::EndTurn);
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().mana,
        100
    );
    g.apply(Command::EndTurn);
    assert_eq!(g.time_stopper(), None, "it could not be paid for");
    g.apply(Command::EndTurn);
    assert!(g.units().unit_moves_left(theirs) > 0);
    Ok(())
}

#[test]
fn under_time_stop_only_its_wizard_can_found_a_city() {
    let mut g = world();
    let theirs = unit(&mut g, P1, "high_elf_settlers", at(10, 10));
    let mine = unit(&mut g, P0, "high_elf_settlers", at(18, 3));
    keep(&mut g, P0, "time_stop");
    g.attach_enchantment(Target::Player(P0), sp(&g, "time_stop"), P0);
    let refused = g.apply(Command::SettleOutpost {
        unit: theirs,
        name: "Late".into(),
    });
    assert!(
        refused.iter().any(|e| matches!(
            e,
            Event::Rejected(crate::game::Rejection::FoundCity(
                crate::city::FoundCityError::TimeStopped
            ))
        )),
        "{refused:?}"
    );
    let founded = g.apply(Command::SettleOutpost {
        unit: mine,
        name: "Now".into(),
    });
    assert!(
        founded
            .iter()
            .any(|e| matches!(e, Event::CityFounded { .. })),
        "{founded:?}"
    );
}

#[test]
fn corrupted_squares_sometimes_heal_by_themselves() {
    let mut g = world();
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(9);
    for x in 0..24 {
        for y in 0..14 {
            g.corrupt(at(x, y));
        }
    }
    let count = |g: &Game| {
        (0..24)
            .flat_map(|x| (0..14).map(move |y| at(x, y)))
            .filter(|&p| g.is_corrupted(p))
            .count()
    };
    assert_eq!(count(&g), 336);
    g.fade_corruption(&mut rng);
    let once = count(&g);
    assert!((315..336).contains(&once), "about two in a hundred: {once}");
    for _ in 0..100 {
        g.fade_corruption(&mut rng);
    }
    let later = count(&g);
    assert!(later < once && later > 0, "{once} -> {later}");
}
