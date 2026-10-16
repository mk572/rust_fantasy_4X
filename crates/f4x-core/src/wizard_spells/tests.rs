use super::*;
use crate::city::Position;
use crate::map::{Grid, Map, Plane};
use crate::player::Player;
use crate::rules::effects::{Condition, DamageKind, Effect, EffectRule, TargetSel, Trigger};
use crate::rules::exec::Target;
use crate::rules::{CastContext, Duration, Rarity, Registry, SpellDef, SpellTarget, UnitKind};
use crate::testutil::content_with_magic;
use crate::Command;

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
    g.fog = crate::fog::Fog::new(24, 14);
    for (id, name) in [(P0, "Merlin"), (P1, "Rjak"), (P2, "Sss-ra")] {
        let mut p = Player::new(id, name);
        p.mana = 5000;
        g.players.push(p);
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
    let mut rng = crate::rng::GameRng::new(1);
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
    let mut rng = crate::rng::GameRng::new(2);
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
    let mut rng = crate::rng::GameRng::new(4);
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
fn black_wind_square(g: &mut Game, at: Position, rng: &mut crate::rng::GameRng) -> u32 {
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
    let mut rng = crate::rng::GameRng::new(3);
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
    g.world.insert_one(u, crate::units::Hurt(0)).unwrap();
    // A resistance of 5 against -1: each figure fails on a roll of 5 to 10 (60 percent).
    let stats = {
        let mut defs = g.content.rules.units.defs().to_vec();
        for d in defs.iter_mut().filter(|d| d.id == "squad") {
            d.stats.insert(crate::rules::Stat::Resistance, 5);
        }
        defs
    };
    g.content.rules.units = Registry::new(stats);
    let mut rng = crate::rng::GameRng::new(11);
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
    g.world
        .insert_one(u, crate::units::Experience(120))
        .unwrap();
    let events = cast(&mut g, "lycanthropy", Some(Target::Unit(u)));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::UnitTransformed { .. })),
        "{events:?}"
    );
    assert_eq!(g.units().unit_def(u).unwrap().id, "werewolves");
    assert!(g.units().unit_stat(u, crate::rules::Stat::Figures) > 0);
    assert!(g.world.get::<&crate::units::Experience>(u).is_err());
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
