use super::*;
use crate::game::testutil::content_with_magic;
use crate::map::{Grid, Map, Plane, Position};

use crate::game::Command;
use crate::player::Player;
use crate::rules::effects::{Condition, DamageKind, Effect, EffectRule, TargetSel, Trigger};
use crate::rules::exec::Target;
use crate::rules::{CastContext, Duration, Rarity, Registry, SpellDef, SpellTarget, UnitKind};
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

fn spell(id: &str, realm: &str, target: SpellTarget, hook: &str) -> SpellDef {
    SpellDef {
        id: id.into(),
        name: id.into(),
        realm: realm.into(),
        rarity: Rarity::Common,
        cost: 10,
        research_cost: 10,
        contexts: vec![CastContext::Overland],
        overland_x5: false,
        target,
        duration: Duration::Instant,
        upkeep: 0,
        dispel_difficulty: 0,
        save: None,
        effects: vec![EffectRule {
            when: Trigger::OnCast,
            target: TargetSel::Chosen,
            condition: Condition::Always,
            effect: Effect::Custom(hook.into()),
        }],
    }
}

/// Three wizards on grassland; "walker" is a normal unit, "slow" a fantastic creature of chaos,
/// "scout" flies and "spirit" has no body.
fn world() -> Game {
    let mut c = content_with_magic();
    let mut units = c.rules.units.defs().to_vec();
    for u in units.iter_mut() {
        match u.id.as_str() {
            "walker" => u.kind = UnitKind::Normal,
            "scout" | "spirit" => u.kind = UnitKind::Normal,
            "slow" => {
                u.kind = UnitKind::Summoned;
                u.realm = Some("chaos".into());
            }
            _ => {}
        }
    }
    let mut wolves = units.iter().find(|u| u.id == "slow").unwrap().clone();
    wolves.id = "werewolves".into();
    wolves.name = "Werewolves".into();
    wolves.realm = Some("death".into());
    units.push(wolves);
    // A squad of five with two hit points each, to count figures.
    let mut squad = units.iter().find(|u| u.id == "walker").unwrap().clone();
    squad.id = "squad".into();
    squad.stats.insert(crate::rules::Stat::Figures, 5);
    squad.stats.insert(crate::rules::Stat::MaxHp, 2);
    squad.stats.insert(crate::rules::Stat::Resistance, 0);
    units.push(squad);
    c.rules.units = Registry::new(units);
    let mut spells = c.rules.spells.defs().to_vec();
    for (id, realm, target, hook) in [
        (
            "drain_power",
            "death",
            SpellTarget::EnemyWizard,
            DRAIN_POWER,
        ),
        (
            "cruel_unminding",
            "death",
            SpellTarget::EnemyWizard,
            CRUEL_UNMINDING,
        ),
        ("subversion", "death", SpellTarget::EnemyWizard, SUBVERSION),
        (
            "great_unsummoning",
            "sorcery",
            SpellTarget::None,
            GREAT_UNSUMMONING,
        ),
        ("death_wish", "death", SpellTarget::None, DEATH_WISH),
        ("earthquake", "nature", SpellTarget::EnemyCity, EARTHQUAKE),
        ("black_wind", "death", SpellTarget::Tile, BLACK_WIND),
        ("lycanthropy", "death", SpellTarget::OwnUnit, LYCANTHROPY),
        (
            "call_the_void",
            "chaos",
            SpellTarget::EnemyCity,
            crate::spell::world::CALL_THE_VOID,
        ),
        (
            "natures_wrath",
            "nature",
            SpellTarget::None,
            "global_effect",
        ),
        (
            "chaos_channels",
            "chaos",
            SpellTarget::OwnUnit,
            crate::unit::mutation::CHAOS_CHANNELS,
        ),
        ("doom_mastery", "chaos", SpellTarget::None, "global_effect"),
        ("meteor_storm", "chaos", SpellTarget::None, "global_effect"),
        ("great_wasting", "chaos", SpellTarget::None, "global_effect"),
        ("armageddon", "chaos", SpellTarget::None, "global_effect"),
    ] {
        spells.push(spell(id, realm, target, hook));
    }
    // Black Wind reaches every unit on the square, one by one.
    for s in spells.iter_mut().filter(|s| s.id == "black_wind") {
        s.effects[0].target = TargetSel::UnitsOnChosenSquare;
    }
    // The storms, as the content has them: a strength attack on every figure on the square.
    for (id, realm, strength, kind, immune) in [
        ("ice_storm", "nature", 6, DamageKind::Cold, "cold_immunity"),
        ("fire_storm", "chaos", 8, DamageKind::Fire, "fire_immunity"),
    ] {
        let mut storm = spell(id, realm, SpellTarget::Tile, "");
        storm.effects = vec![EffectRule {
            when: Trigger::OnCast,
            target: TargetSel::UnitsOnChosenSquare,
            condition: Condition::Always,
            effect: Effect::Damage {
                amount: strength,
                kind,
                resistible: false,
                immune: vec![immune.into(), "magic_immunity".into()],
                realm: Some(realm.into()),
                resist_modifier: 0,
                traits: crate::rules::effects::DamageTraits {
                    area: true,
                    ..Default::default()
                },
            },
        }];
        spells.push(storm);
    }
    let mut stasis = spell(
        "stasis",
        "sorcery",
        SpellTarget::Tile,
        crate::unit::stasis::STASIS,
    );
    stasis.effects[0].target = TargetSel::UnitsOnChosenSquare;
    spells.push(stasis);
    let mut time_stop = spell("time_stop", "sorcery", SpellTarget::None, "global_effect");
    time_stop.duration = Duration::Permanent;
    time_stop.upkeep = 200;
    spells.push(time_stop);
    c.rules.spells = Registry::new(spells);
    let mut realms = c.rules.realms.defs().to_vec();
    for r in ["sorcery", "death", "arcane"] {
        realms.push(crate::rules::RealmDef {
            id: r.into(),
            name: r.into(),
            ..realms[0].clone()
        });
    }
    c.rules.realms = Registry::new(realms);
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(9);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.fog = crate::map::fog::Fog::new(24, 14);
    for (id, name) in [(P0, "Merlin"), (P1, "Rjak"), (P2, "Sss-ra")] {
        let mut p = Player::new();
        p.mana = 5000;
        g.add_player(id, name, p);
        g.init_wizard(id, &[("death", 3)], &[], None).unwrap();
        g.magic_mut(id).unwrap().skill_left = 99_999;
    }
    g
}

fn sp(g: &Game, id: &str) -> crate::rules::SpellId {
    g.content.rules.spells.id_of(id).unwrap()
}

fn unit(g: &mut Game, owner: PlayerId, id: &str, pos: Position) -> hecs::Entity {
    let def = g.content.rules.units.id_of(id).unwrap();
    g.spawn_unit(owner, pos, def)
}

fn cast(g: &mut Game, spell: &str, target: Option<Target>) -> Vec<Event> {
    let s = sp(g, spell);
    g.magic_mut(P0).unwrap().known.insert(s);
    g.apply(Command::CastSpell {
        player: P0,
        spell: s,
        target,
    })
}

#[test]
fn drain_power_takes_what_the_victim_has_up_to_the_roll() {
    let mut g = world();
    g.player_mut(P1).unwrap().mana = 1000;
    let events = cast(&mut g, "drain_power", Some(Target::Player(P1)));
    let taken = events
        .iter()
        .find_map(|e| match e {
            Event::PowerDrained { amount, .. } => Some(*amount),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{events:?}"));
    assert!((10..=200).contains(&taken), "{taken}");
    assert_eq!(g.player(P1).unwrap().mana, 1000 - taken);
    // A poor victim loses only what is there.
    g.player_mut(P1).unwrap().mana = 4;
    let events = cast(&mut g, "drain_power", Some(Target::Player(P1)));
    assert!(events.contains(&Event::PowerDrained {
        player: P0,
        victim: P1,
        amount: 4
    }));
    assert_eq!(g.player(P1).unwrap().mana, 0);
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
fn cruel_unminding_takes_a_tenth_at_most_and_at_least_one_point() {
    let mut g = world();
    g.magic_mut(P1).unwrap().skill_points = skill_points_for(60, false);
    let before = g.magic(P1).unwrap().casting_skill();
    assert_eq!(before, 60);
    cast(&mut g, "cruel_unminding", Some(Target::Player(P1)));
    let after = g.magic(P1).unwrap().casting_skill();
    assert!((54..=59).contains(&after), "{after}");
    // A tiny skill still loses a point.
    g.magic_mut(P2).unwrap().skill_points = skill_points_for(5, false);
    let before = g.magic(P2).unwrap().casting_skill();
    let mut rng = crate::game::rng::GameRng::new(1);
    let lost = g.cruel_unminding(P2, &mut rng);
    assert_eq!(lost, 1);
    assert_eq!(g.magic(P2).unwrap().casting_skill(), before - 1);
}

#[test]
fn an_archmages_ten_points_are_safe() {
    let mut g = world();
    g.magic_mut(P1).unwrap().retorts.insert(Retort::Archmage);
    g.magic_mut(P1).unwrap().skill_points = skill_points_for(40, true);
    assert_eq!(g.magic(P1).unwrap().casting_skill(), 40);
    let mut rng = crate::game::rng::GameRng::new(2);
    let lost = g.cruel_unminding(P1, &mut rng);
    assert!((1..=4).contains(&lost), "{lost}");
    assert_eq!(g.magic(P1).unwrap().casting_skill(), 40 - lost);
}

#[test]
fn subversion_sours_the_others_on_the_target_and_ends_their_peace() {
    let mut g = world();
    g.make_contact(P2, P1);
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
        .map(|i| unit(&mut g, P0, "slow", at(i % 20, 1)))
        .collect();
    let theirs: Vec<_> = (0..40)
        .map(|i| unit(&mut g, P1, "slow", at(i % 20, 2)))
        .collect();
    let men = unit(&mut g, P1, "walker", at(3, 3));
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
        .map(|i| unit(&mut g, P0, "walker", at(i % 20, 1)))
        .collect();
    let enemy: Vec<_> = (0..30)
        .map(|i| unit(&mut g, P1, "walker", at(i % 20, 2)))
        .collect();
    let summoned = unit(&mut g, P1, "slow", at(1, 5));
    cast(&mut g, "death_wish", None);
    assert!(own.iter().all(|&u| g.world.contains(u)));
    assert!(g.world.contains(summoned));
    let left = enemy.iter().filter(|&&u| g.world.contains(u)).count();
    assert!(left < 30, "some failed to resist");
}

#[test]
fn an_earthquake_spares_flyers_and_the_bodiless() {
    let mut g = world();
    let city = {
        let race = g.content.rules.races.id_of("elf");
        g.apply(Command::FoundCity {
            owner: P1,
            pos: at(5, 5),
            name: "Town".into(),
            race,
            population: 4,
        });
        g.economy().city_at(at(5, 5)).unwrap()
    };
    let walkers: Vec<_> = (0..1)
        .map(|_| unit(&mut g, P1, "walker", at(5, 5)))
        .collect();
    let flyer = unit(&mut g, P1, "scout", at(5, 5));
    let ghost = unit(&mut g, P1, "spirit", at(5, 5));
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
}

#[test]
fn an_earthquake_is_cast_at_an_enemy_city_and_reports_the_toll() {
    let mut g = world();
    let race = g.content.rules.races.id_of("elf");
    g.apply(Command::FoundCity {
        owner: P1,
        pos: at(5, 5),
        name: "Town".into(),
        race,
        population: 4,
    });
    let city = g.economy().city_at(at(5, 5)).unwrap();
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
    let own = g.economy().city_at(at(9, 9)).unwrap();
    let events = cast(&mut g, "earthquake", Some(Target::City(own)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
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
    // Zero resistance: a roll of 1 to 10 against -1 fails every time, so five figures of two
    // hit points each usually all rot away.
    let mut dead = 0;
    let mut hurt = 0;
    for i in 0..9 {
        unit(&mut g, P1, "squad", at(i, 1));
    }
    let mut rng = crate::game::rng::GameRng::new(3);
    for i in 0..9 {
        let before = g.units().units_at(at(i, 1)).len();
        let killed = black_wind_square(&mut g, at(i, 1), &mut rng);
        dead += killed;
        if before == 1 && killed == 0 {
            hurt += 1;
        }
    }
    assert!(dead + hurt == 9);
    assert!(dead > 0, "five figures at zero resistance usually all rot");
}

#[test]
fn black_wind_wounds_a_unit_that_resists_some_figures() {
    let mut g = world();
    let u = unit(&mut g, P1, "squad", at(4, 4));
    g.world.insert_one(u, crate::unit::Hurt(0)).unwrap();
    // A resistance of 5 against -1: each figure fails on a roll of 5 to 10 (60 percent).
    let stats = {
        let mut defs = g.content.rules.units.defs().to_vec();
        for d in defs.iter_mut().filter(|d| d.id == "squad") {
            d.stats.insert(crate::rules::Stat::Resistance, 5);
        }
        defs
    };
    g.content.rules.units = Registry::new(stats);
    let mut rng = crate::game::rng::GameRng::new(11);
    let mut wounded = 0;
    let mut killed = 0;
    for _ in 0..200 {
        let u = unit(&mut g, P1, "squad", at(6, 6));
        killed += black_wind_square(&mut g, at(6, 6), &mut rng);
        if g.world.contains(u) && g.units().unit_hits(u).0 < 10 {
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
}

#[test]
fn black_wind_needs_a_square_with_an_enemy_on_it() {
    let mut g = world();
    unit(&mut g, P0, "walker", at(2, 2));
    let events = cast(&mut g, "black_wind", Some(Target::Tile(at(2, 2))));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    unit(&mut g, P1, "walker", at(2, 2));
    let events = cast(&mut g, "black_wind", Some(Target::Tile(at(2, 2))));
    assert!(
        events.iter().any(|e| matches!(e, Event::UnitsSlain { .. })),
        "{events:?}"
    );
}

#[test]
fn lycanthropy_makes_a_normal_unit_a_werewolf_and_forgets_its_experience() {
    let mut g = world();
    let u = unit(&mut g, P0, "walker", at(3, 3));
    g.world.insert_one(u, crate::unit::Experience(120)).unwrap();
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(u)));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::UnitTransformed { .. })),
        "{events:?}"
    );
    assert_eq!(g.units().unit_def(u).unwrap().id, "werewolves");
    assert!(g.units().unit_stat(u, crate::rules::Stat::Figures) > 0);
    assert!(g.world.get::<&crate::unit::Experience>(u).is_err());
    // Not twice: Werewolves are fantastic.
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(u)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
}

#[test]
fn lycanthropy_refuses_fantastic_creatures_undead_and_other_wizards_units() {
    let mut g = world();
    let summoned = unit(&mut g, P0, "slow", at(1, 1));
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(summoned)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    let risen = unit(&mut g, P0, "walker", at(1, 2));
    g.make_undead(risen);
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(risen)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    let theirs = unit(&mut g, P1, "walker", at(1, 3));
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(theirs)));
    assert!(
        events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
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
        unit(&mut g, P0, "squad", mine);
        unit(&mut g, P1, "squad", theirs);
        unit(&mut g, P1, "squad", far);
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
        ("walker", "magic_immunity"),
        ("scout", "cold_immunity"),
        ("spirit", "fire_immunity"),
    ] {
        defs.iter_mut()
            .filter(|d| d.id == id)
            .for_each(|d| d.abilities.push(ability.to_string()));
    }
    g.content.rules.units = Registry::new(defs);
    for (storm, safe, hurt) in [
        ("ice_storm", ["walker", "scout"], "spirit"),
        ("fire_storm", ["walker", "spirit"], "scout"),
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
fn a_storm_reaches_the_units_in_a_city() {
    let mut g = world();
    let city = g.world.spawn((
        crate::city::City {
            name: "Test".into(),
            owner: P1,
        },
        at(7, 7),
    ));
    let _ = city;
    let u = unit(&mut g, P1, "squad", at(7, 7));
    let before = g.units().unit_hits(u).0;
    let mut hurt = false;
    for _ in 0..5 {
        cast(&mut g, "fire_storm", Some(Target::Tile(at(7, 7))));
        hurt |= !g.world.contains(u) || g.units().unit_hits(u).0 < before;
    }
    assert!(hurt, "the garrison was untouched");
}

#[test]
fn a_storm_needs_a_square_with_an_enemy_on_it() {
    let mut g = world();
    unit(&mut g, P0, "walker", at(2, 2));
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
        unit(&mut g, P1, "squad", at(3, 3));
        let events = cast(&mut g, "fire_storm", Some(Target::Tile(at(3, 3))));
        told |= events
            .iter()
            .any(|e| matches!(e, Event::UnitsSlain { count, .. } if *count > 0));
        let _ = i;
    }
    assert!(told, "twenty fire storms destroyed nothing");
}

fn city_for(g: &mut Game, owner: PlayerId, pos: Position) -> hecs::Entity {
    let race = g.content.rules.races.id_of("elf");
    g.apply(Command::FoundCity {
        owner,
        pos,
        name: "Town".into(),
        race,
        population: 4,
    });
    g.economy().city_at(pos).unwrap()
}

fn keep(g: &mut Game, player: PlayerId, spell: &str) {
    let s = sp(g, spell);
    g.magic_mut(player).unwrap().global.push(s);
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
    let squad = unit(&mut g, P1, "squad", at(10, 6)); // 10 hit points
    let walker = unit(&mut g, P1, "walker", at(10, 6));
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
fn natures_wrath_shakes_the_cities_of_whoever_casts_death_or_chaos_while_it_lasts() {
    let mut g = world();
    keep(&mut g, P1, "natures_wrath");
    let city = city_for(&mut g, P0, at(5, 5));
    let walkers: Vec<_> = (0..60)
        .map(|_| unit(&mut g, P0, "walker", at(5, 5)))
        .collect();
    let spared = city_for(&mut g, P2, at(15, 5));
    let others: Vec<_> = (0..20)
        .map(|_| unit(&mut g, P2, "walker", at(15, 5)))
        .collect();
    let events = cast(&mut g, "drain_power", Some(Target::Player(P2)));
    let lost = events.iter().find_map(|e| match e {
        Event::WorldSpellStruck {
            victim: Some(v),
            units_lost,
            ..
        } if *v == P0 => Some(*units_lost),
        _ => None,
    });
    let lost = lost.unwrap_or_else(|| panic!("{events:?}"));
    let gone = walkers.iter().filter(|&&u| !g.world.contains(u)).count();
    assert!(lost > 0 && gone >= lost as usize, "10% of 60: lost {lost}");
    assert!(
        others.iter().all(|&u| g.world.contains(u)),
        "other cities are safe"
    );
    let _ = (city, spared);
    // The holder's own casting, or a spell of another realm, does nothing.
    g.magic_mut(P1).unwrap().skill_left = 99_999;
    let s = sp(&g, "drain_power");
    g.magic_mut(P1).unwrap().known.insert(s);
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
}

#[test]
fn meteor_storm_strikes_units_outside_cities_but_not_inside_and_spares_the_casters_buildings() {
    let mut g = world();
    keep(&mut g, P0, "meteor_storm");
    city_for(&mut g, P1, at(5, 5));
    let inside: Vec<_> = (0..10)
        .map(|_| unit(&mut g, P1, "walker", at(5, 5)))
        .collect();
    let outside: Vec<_> = (0..10)
        .map(|_| unit(&mut g, P1, "walker", at(15, 8)))
        .collect();
    let mine: Vec<_> = (0..10)
        .map(|_| unit(&mut g, P0, "walker", at(18, 8)))
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
fn a_battle_on_corrupted_ground_costs_no_range_penalty_while_great_wasting_lasts() {
    use crate::battle::Side;
    let range = |g: &mut Game| {
        let a = unit(g, P0, "walker", at(8, 8));
        let d = unit(g, P1, "walker", at(9, 8));
        g.start_battle(&[a], &[d]).unwrap();
        let r = g.battle().unwrap().caster(Side::Attacker).unwrap().range;
        g.battle = None;
        r
    };
    let mut g = world();
    let far = range(&mut g);
    assert!(far > 10, "no fortress: the greatest penalty ({far})");
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set_corrupted(9, 8, true);
    assert_eq!(range(&mut g), far, "corrupted ground alone does nothing");
    keep(&mut g, P0, "great_wasting");
    assert_eq!(range(&mut g), 10, "while the spell lasts");
    g.magic_mut(P0).unwrap().global.clear();
    assert_eq!(range(&mut g), far, "and not after");
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
fn armageddon_pays_for_every_volcano_in_the_world_but_only_its_own_wizard() {
    let mut g = world();
    for (x, o) in [(2, P0), (3, P1), (4, P1)] {
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Myrror)
            .set_volcano(x, 2, Some(o.0));
    }
    assert_eq!((g.volcano_count(P0), g.volcano_count(P1)), (1, 2));
    keep(&mut g, P0, "armageddon");
    assert_eq!((g.volcano_count(P0), g.volcano_count(P1)), (3, 2));
    keep(&mut g, P1, "armageddon");
    assert_eq!((g.volcano_count(P0), g.volcano_count(P1)), (3, 3));
}

#[test]
fn an_armageddon_volcano_can_rise_in_the_sea_and_sinks_the_ships_there() {
    let mut g = world();
    let ocean = g.content.terrains.id_of("ocean").unwrap();
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set(6, 6, ocean);
    let boat = unit(&mut g, P1, "boat", at(6, 6));
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(3);
    assert!(g.can_armageddon_volcano(at(6, 6)));
    assert!(
        !g.can_raise_volcano(at(6, 6)),
        "Raise Volcano still needs land"
    );
    g.armageddon_volcano(P0, at(6, 6), &mut rng);
    assert_eq!(
        g.economy().terrain_class_at(at(6, 6)),
        Some(crate::content::TerrainClass::Mountains)
    );
    assert_eq!(g.volcano_count(P0), 1);
    assert!(!g.world.contains(boat), "the ship went down");
    assert!(!g.can_armageddon_volcano(at(6, 6)), "already a volcano");
}

#[test]
fn an_armageddon_volcano_under_a_city_costs_a_quarter_of_everything_but_spares_the_neighbours() {
    let mut g = world();
    let city = city_for(&mut g, P1, at(10, 6));
    let near = city_for(&mut g, P1, at(16, 6));
    let walkers: Vec<_> = (0..80)
        .map(|_| unit(&mut g, P1, "walker", at(10, 6)))
        .collect();
    let next_door: Vec<_> = (0..20)
        .map(|_| unit(&mut g, P1, "walker", at(16, 6)))
        .collect();
    g.world
        .get::<&mut crate::city::economy::Population>(city)
        .unwrap()
        .figures = 20;
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(11);
    let events = g.armageddon_volcano(P0, at(10, 6), &mut rng);
    let gone = walkers.iter().filter(|&&u| !g.world.contains(u)).count();
    assert!((8..=32).contains(&gone), "a quarter of 80: {gone}");
    let pop = g
        .world
        .get::<&crate::city::economy::Population>(city)
        .unwrap()
        .figures;
    assert!(
        (8..=19).contains(&pop),
        "a quarter of 20 citizens gone: {pop}"
    );
    assert!(next_door.iter().all(|&u| g.world.contains(u)));
    assert_eq!(
        g.world
            .get::<&crate::city::economy::Population>(near)
            .unwrap()
            .figures,
        4
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::WorldSpellStruck { .. })));
}

#[test]
fn volcanoes_sometimes_cool_and_sometimes_bring_a_mineral() {
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
                .unwrap()
                .plane(Plane::Arcanus)
                .feature(x, y)
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
        let u = unit(&mut g, P0, "walker", at(3, 3));
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
    let beast = unit(&mut g, P0, "slow", at(3, 3));
    let events = cast(&mut g, "chaos_channels", Some(Target::Unit(beast)));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    for _ in 0..30 {
        let flyer = unit(&mut g, P0, "scout", at(4, 4));
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
    let mine = unit(&mut g, P0, "walker", at(3, 3));
    let theirs = unit(&mut g, P1, "walker", at(4, 3));
    g.doom_mastery_mutates(mine, P0);
    g.doom_mastery_mutates(theirs, P1);
    assert!(g.units().unit_is_mutated(mine));
    assert!(!g.units().unit_is_mutated(theirs));
}

#[test]
fn stasis_holds_enemy_units_on_the_square_for_a_turn_and_then_lets_them_roll_to_break_free() {
    let mut g = world();
    // No upkeep and no food, so that nobody is lost while the turns go by.
    let mut defs = g.content.rules.units.defs().to_vec();
    for d in defs.iter_mut() {
        d.upkeep = Default::default();
        d.kind = UnitKind::Fantastic;
        d.stats.insert(crate::rules::Stat::Resistance, 12);
    }
    g.content.rules.units = Registry::new(defs);
    let enemies: Vec<_> = (0..6)
        .map(|_| unit(&mut g, P1, "walker", at(8, 8)))
        .collect();
    let mine = unit(&mut g, P0, "walker", at(8, 8));
    let other = unit(&mut g, P1, "walker", at(9, 8));
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
    g.magic_mut(P0).unwrap().skill_left = 99_999;
    let held = unit(&mut g, P1, "walker", at(12, 8));
    cast(&mut g, "stasis", Some(Target::Tile(at(12, 8))));
    assert!(g.unit_in_stasis(held));
    let again = cast(&mut g, "stasis", Some(Target::Tile(at(12, 8))));
    assert!(
        matches!(again.as_slice(), [Event::Rejected(_)]),
        "{again:?}"
    );
}

#[test]
fn time_stop_freezes_everyone_but_its_wizard_who_pays_only_for_the_spell() {
    let mut g = world();
    let mut defs = g.content.rules.units.defs().to_vec();
    for d in defs.iter_mut() {
        d.upkeep = Default::default();
        d.kind = UnitKind::Fantastic;
    }
    g.content.rules.units = Registry::new(defs);
    let mine = unit(&mut g, P0, "walker", at(3, 3));
    let theirs = unit(&mut g, P1, "walker", at(4, 3));
    keep(&mut g, P0, "time_stop");
    g.attach_enchantment(Target::Player(P0), sp(&g, "time_stop"), P0);
    // Another global enchantment of the wizard's costs nothing now.
    g.player_mut(P0).unwrap().mana = 500;
    g.player_mut(P0).unwrap().gold = 100;
    g.magic_mut(P1).unwrap().skill_left = 5;
    g.apply(Command::EndTurn);
    assert!(g.units().unit_moves_left(mine) > 0);
    assert_eq!(
        g.units().unit_moves_left(theirs),
        0,
        "their troops are frozen"
    );
    assert_eq!(g.player(P0).unwrap().mana, 300, "200 for Time Stop");
    assert_eq!(g.player(P0).unwrap().gold, 100, "no gold either way");
    assert!(g.time_stopper() == Some(P0));
    // Out of mana, the spell is lost and time runs again.
    g.apply(Command::EndTurn);
    assert_eq!(g.player(P0).unwrap().mana, 100);
    g.apply(Command::EndTurn);
    assert_eq!(g.time_stopper(), None, "it could not be paid for");
    g.apply(Command::EndTurn);
    assert!(g.units().unit_moves_left(theirs) > 0);
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
