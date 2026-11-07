use super::*;
use crate::game::testutil::{num, shipped};
use crate::map::{Grid, Map, Plane, Position};
use crate::rules::Ability;
use crate::rules::RealmId;
use f4x_def::testdir::TestResult;

use crate::feature::node::Node;
use crate::game::Command;
use crate::wizard::magic::Retort;
use rand::SeedableRng;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

fn rng() -> rand_pcg::Pcg64Mcg {
    rand_pcg::Pcg64Mcg::seed_from_u64(5)
}

/// Two wizards (lots of mana and skill) and grassland, with the game's real spells.
fn world() -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("id_of succeeds");
    let mut g = Game::new(11);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    for (id, name) in [(P0, "Merlin"), (P1, "Rjak")] {
        g.add_player(id, name);
        g.treasury_mut(id).expect("treasury_mut succeeds").mana =
            f4x_def::half_int::HalfInt::from_int(20_000);
        g.init_wizard(id, &[(RealmId::new(3 /* life */), 2)], &[], None)
            .expect("init_wizard succeeds");
        g.wizard_mut(id)
            .expect("value succeeds")
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 99_999;
    }
    g
}

fn sp(g: &Game, id: u16) -> SpellId {
    g.content
        .rules
        .spells
        .id_numbered(id)
        .expect("id_of succeeds")
}

/// The target that names the global enchantment `player` keeps going.
fn global(g: &Game, player: PlayerId, spell: SpellId) -> Target {
    Target::Spell(
        g.global_instance(player, spell)
            .expect("global_instance succeeds"),
    )
}

fn cast(g: &mut Game, spell: u16, target: Target) -> Vec<Event> {
    cast_with(g, spell, target, 0)
}

fn cast_with(g: &mut Game, spell: u16, target: Target, extra: i32) -> Vec<Event> {
    cast_as(g, P0, spell, target, extra)
}

fn cast_as(g: &mut Game, who: PlayerId, spell: u16, target: Target, extra: i32) -> Vec<Event> {
    let s = sp(g, spell);
    g.wizard_mut(who)
        .expect("value succeeds")
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(s);
    g.apply(Command::CastSpell {
        extra,
        player: who,
        spell: s,
        target: Some(target),
    })
}

/// A dispel is a roll, so a test that needs it to succeed casts again (the wizards have mana to
/// spare) until `done`, and gets every event of the casts. A cast that finds nothing to dispel is
/// refused, so a test that is done never casts again.
fn cast_until(
    g: &mut Game,
    who: PlayerId,
    spell: u16,
    target: impl Fn(&Game) -> Target,
    done: impl Fn(&Game) -> bool,
) -> Vec<Event> {
    let mut all = Vec::new();
    for _ in 0..200 {
        if done(g) {
            return all;
        }
        let aim = target(g);
        all.extend(cast_as(g, who, spell, aim, 0));
    }
    all
}

#[test]
fn the_chance_is_strength_over_strength_plus_difficulty() {
    let mut r = rng();
    let mut hits = |strength, difficulty| {
        (0..4000)
            .filter(|_| dispel_roll(strength, difficulty, &mut r))
            .count()
    };
    assert_eq!(hits(0, 10), 0);
    assert!((900..1100).contains(&hits(10, 30)), "a quarter");
    assert!((1900..2100).contains(&hits(30, 30)), "a half");
    // The original's roll of 250 never beats 249: even overwhelming strength fails 1 in 250.
    assert!(hits(100_000, 1) > 3900);
}

#[test]
fn masteries_and_archmage_multiply_a_spells_difficulty() -> TestResult {
    let mut g = world();
    let aura = sp(&g, 59 /* aura_of_majesty */); // sorcery, 400
    assert_eq!(g.dispel_difficulty(aura, Some(P0)), 400);
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::new(4) /* sorcery_mastery */);
    assert_eq!(
        g.dispel_difficulty(aura, Some(P0)),
        800,
        "its realm's mastery"
    );
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::new(3) /* nature_mastery */);
    assert_eq!(
        g.dispel_difficulty(aura, Some(P0)),
        800,
        "another realm's does not count"
    );
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::new(10) /* archmage */);
    assert_eq!(g.dispel_difficulty(aura, Some(P0)), 1200);
    assert_eq!(g.dispel_difficulty(aura, Some(P1)), 400);
    assert_eq!(g.dispel_difficulty(aura, None), 400);
    Ok(())
}

#[test]
fn only_other_wizards_enchantments_are_dispelled() -> TestResult {
    let mut g = world();
    let spear = g
        .content
        .rules
        .units
        .id_numbered(95)
        .ok_or("high_elf_spearmen is not in the content")?;
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let strength = sp(&g, 4 /* giant_strength */);
    g.attach_enchantment(Target::Unit(unit), strength, P0);
    let mut r = rng();
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P0, &mut r, &mut Vec::new()),
        0
    );
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r, &mut Vec::new()),
        1
    );
    Ok(())
}

#[test]
fn a_dispel_names_each_spell_it_removes() -> TestResult {
    let mut g = world();
    let spear = g
        .content
        .rules
        .units
        .id_numbered(95)
        .ok_or("high_elf_spearmen is not in the content")?;
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let strength = sp(&g, 4 /* giant_strength */);
    g.attach_enchantment(Target::Unit(unit), strength, P0);
    let mut gone = Vec::new();
    g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut rng(), &mut gone);
    assert_eq!(
        gone,
        vec![Event::EnchantmentDispelled {
            victim: P0,
            spell: strength,
            site: DispelSite::Unit
        }]
    );
    Ok(())
}

#[test]
fn spell_lock_must_be_dispelled_before_anything_else_on_the_unit() -> TestResult {
    let mut g = world();
    let spear = g
        .content
        .rules
        .units
        .id_numbered(95)
        .ok_or("high_elf_spearmen is not in the content")?;
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let (lock, strength) = (sp(&g, 54 /* spell_lock */), sp(&g, 4 /* giant_strength */));
    g.attach_enchantment(Target::Unit(unit), strength, P0);
    g.attach_enchantment(Target::Unit(unit), lock, P0);
    let mut r = rng();
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r, &mut Vec::new()),
        1,
        "the first dispel only takes the lock"
    );
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r, &mut Vec::new()),
        1
    );
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r, &mut Vec::new()),
        0
    );
    // A weak dispel rarely beats the lock's 150, and a lock never shields from its owner.
    g.attach_enchantment(Target::Unit(unit), lock, P0);
    g.attach_enchantment(Target::Unit(unit), strength, P1);
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P0, &mut r, &mut Vec::new()),
        1
    );
    Ok(())
}

#[test]
fn disenchant_area_clears_a_city_its_units_and_a_warped_node() -> TestResult {
    let mut g = world();
    let race = g.content.rules.races.id_numbered(7);
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(6, 6),
        name: "Town".into(),
        race,
        population: 3,
    });
    let city = g.economy().city_at(at(6, 6)).ok_or("city_at is missing")?;
    let spear = g
        .content
        .rules
        .units
        .id_numbered(95)
        .ok_or("high_elf_spearmen is not in the content")?;
    let unit = g.spawn_unit(P0, at(6, 6), spear);
    let elsewhere = g.spawn_unit(P0, at(9, 9), spear);
    let (strength, aura) = (
        sp(&g, 4 /* giant_strength */),
        sp(&g, 59 /* aura_of_majesty */),
    );
    g.attach_enchantment(Target::City(city), aura, P0);
    g.attach_enchantment(Target::Unit(unit), strength, P0);
    g.attach_enchantment(Target::Unit(elsewhere), strength, P0);
    g.put_node_data(
        at(6, 6),
        num::feature::SORCERY_NODE,
        Node {
            realm: crate::game::testutil::realm(2 /* chaos */),
            power: 8,
            aura: vec![(6, 6)],
            guardian: false,
            warped: true,
        },
        Some(PlayerId(0)),
    );
    let node = g.economy().nodes()[0].entity;
    let events = cast_until(
        &mut g,
        P0,
        204, /* disenchant_area */
        |_| Target::Tile(at(6, 6)),
        |g| !g.world.get::<&Node>(node).is_ok_and(|n| n.warped),
    );
    let removed: u32 = events
        .iter()
        .map(|e| match e {
            Event::Dispelled { removed, .. } => *removed,
            _ => 0,
        })
        .sum();
    assert_eq!(
        removed, 1,
        "only the warp on the caster's own node goes, their own enchantments stay: {events:?}"
    );
    g.world.get::<&mut Node>(node)?.warped = true;
    // The same cast by the other wizard takes it all.
    let events = cast_until(
        &mut g,
        P1,
        204, /* disenchant_area */
        |_| Target::Tile(at(6, 6)),
        |g| {
            !g.world.get::<&Node>(node).is_ok_and(|n| n.warped)
                && g.units().unit_enchantments(unit).is_empty()
                && g.city_enchantments(city).is_empty()
        },
    );
    let removed: u32 = events
        .iter()
        .map(|e| match e {
            Event::Dispelled { removed, .. } => *removed,
            _ => 0,
        })
        .sum();
    assert_eq!(removed, 3, "{events:?}");
    assert!(!g.node_at(at(6, 6)).ok_or("node_at is missing")?.warped);
    let left = g.units().unit_enchantments(elsewhere).len();
    assert_eq!(left, 1, "another square is left alone");
    Ok(())
}

#[test]
fn disjunction_ends_and_spell_binding_steals_an_enemy_global() -> TestResult {
    let mut g = world();
    let aura = sp(&g, 59 /* aura_of_majesty */);
    g.attach_enchantment(Target::Player(P1), aura, P1);
    let held = global(&g, P1, aura);
    let events = cast_until(
        &mut g,
        P0,
        76, /* spell_binding */
        |g| global(g, P1, aura),
        |g| g.wizard(P0).is_some_and(|w| w.has_global(aura)),
    );
    assert!(
        events.contains(&Event::GlobalSpellTaken {
            player: P0,
            victim: P1,
            spell: aura
        }),
        "{events:?}"
    );
    assert!(g.wizard(P0).ok_or("wizard is missing")?.has_global(aura));
    assert!(!g.wizard(P1).ok_or("wizard is missing")?.has_global(aura));
    // Now P1 ends it with Disjunction.
    let events = cast_until(
        &mut g,
        P1,
        210, /* disjunction */
        |g| global(g, P0, aura),
        |g| g.wizard(P0).is_some_and(|w| w.global().is_empty()),
    );
    assert!(
        events.contains(&Event::GlobalSpellRemoved {
            player: P1,
            victim: P0,
            spell: aura
        }),
        "{events:?}"
    );
    assert!(g.wizard(P0).ok_or("wizard is missing")?.global().is_empty());
    // Nothing to take: refused.
    let events = cast(&mut g, 210 /* disjunction */, held);
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    // You cannot disjoin your own.
    g.attach_enchantment(Target::Player(P0), aura, P0);
    let own = global(&g, P0, aura);
    let events = cast(&mut g, 210 /* disjunction */, own);
    assert!(matches!(events.as_slice(), [Event::Rejected(_)]));
    Ok(())
}

#[test]
fn the_rivals_globals_are_listed_wizard_by_wizard() -> TestResult {
    let mut g = world();
    let aura = sp(&g, 59 /* aura_of_majesty */);
    let omens = sp(&g, 198 /* evil_omens */);
    g.attach_enchantment(Target::Player(P1), aura, P1);
    g.attach_enchantment(Target::Player(P1), omens, P1);
    g.attach_enchantment(Target::Player(P0), aura, P0);
    let seen: Vec<_> = g
        .rival_globals(P0)
        .into_iter()
        .map(|i| g.spells().def(i).expect("def succeeds"))
        .collect();
    assert_eq!(seen, [aura, omens], "P0's own is no target");
    assert_eq!(g.rival_globals(P1).len(), 1);
    Ok(())
}

#[test]
fn a_weak_disjunction_often_fails() -> TestResult {
    let mut g = world();
    let aura = sp(&g, 59 /* aura_of_majesty */); // difficulty 400
    g.attach_enchantment(Target::Player(P1), aura, P1);
    let mut r = rng();
    let wins = (0..2000)
        .filter(|_| {
            let held = g
                .global_instance(P1, aura)
                .expect("global_instance succeeds");
            let won = g.attack_global(P0, held, 400, false, &mut r) == Some(true);
            g.attach_enchantment(Target::Player(P1), aura, P1);
            won
        })
        .count();
    assert!((900..1100).contains(&wins), "half of 2000: {wins}");
    Ok(())
}

#[test]
fn spell_blast_costs_what_the_victim_has_spent_and_ends_their_spell() -> TestResult {
    let mut g = world();
    g.make_contact(P0, P1);
    let aura = sp(&g, 59 /* aura_of_majesty */);
    let cost = g.casting_cost(P1, aura, false);
    g.scenario_casting(P1, aura, cost - 60);
    assert_eq!(g.mana_spent_casting(P1), Some(60));
    let mana = g.player(P0).ok_or("player is missing")?.treasury().mana;
    let blast = sp(&g, 58 /* spell_blast */);
    let paid = g.casting_cost(P0, blast, false);
    let events = cast(&mut g, 58 /* spell_blast */, Target::Player(P1));
    assert!(
        events.contains(&Event::SpellBlasted {
            player: P0,
            victim: P1,
            spell: aura,
            mana: 60
        }),
        "{events:?}"
    );
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.treasury().mana,
        mana - paid - 60
    );
    assert_eq!(g.wizard(P1).ok_or("wizard is missing")?.casting(), None);
    // Nothing being cast: not a target.
    let events = cast(&mut g, 58 /* spell_blast */, Target::Player(P1));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    Ok(())
}

#[test]
fn spell_blast_fails_when_the_caster_cannot_match_the_mana() -> TestResult {
    let mut g = world();
    let aura = sp(&g, 59 /* aura_of_majesty */);
    let cost = g.casting_cost(P1, aura, false);
    g.scenario_casting(P1, aura, cost - 90);
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(
            g.casting_cost(P0, sp(&g, 58 /* spell_blast */), false) + 50,
        );
    let events = cast(&mut g, 58 /* spell_blast */, Target::Player(P1));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::SpellBlasted { .. })),
        "{events:?}"
    );
    assert!(g.wizard(P1).ok_or("wizard is missing")?.casting().is_some());
    Ok(())
}

#[test]
fn suppress_magic_stops_enemy_spells_with_its_strength_over_the_spells_cost() {
    let mut g = world();
    let suppress = sp(&g, 79 /* suppress_magic */);
    g.attach_enchantment(Target::Player(P1), suppress, P1);
    // Strength 500 against difficulty 500: half.
    let big = sp(&g, 213 /* spell_of_mastery */); // cost 5000 ...
    let mid = sp(&g, 65 /* storm_giant */); // cost 500
    let stopped = (0..2000)
        .filter(|_| g.suppressed_by(P0, mid).is_some())
        .count();
    assert!((900..1100).contains(&stopped), "half of 2000: {stopped}");
    assert!(
        g.suppressed_by(P1, mid).is_none(),
        "never against its own wizard"
    );
    let stopped_big = (0..2000)
        .filter(|_| g.suppressed_by(P0, big).is_some())
        .count();
    assert!(
        stopped_big < 400,
        "a costly spell mostly gets through: {stopped_big}"
    );
}

#[test]
fn life_force_and_tranquility_guard_against_their_realms_only() {
    let mut g = world();
    let (death, chaos, life) = (
        sp(&g, 167 /* life_drain */),
        sp(&g, 83 /* fire_bolt */),
        sp(&g, 156 /* life_force */),
    );
    g.attach_enchantment(Target::Player(P1), life, P1);
    let n = |g: &mut Game, s| {
        (0..400)
            .filter(|_| g.suppressed_by(P0, s).is_some())
            .count()
    };
    assert!(n(&mut g, death) > 200, "death spells are checked");
    assert_eq!(n(&mut g, chaos), 0, "chaos spells are not");
    assert!(g.suppressed_by(P1, death).is_none(), "never its own wizard");
    let tranq = sp(&g, 157 /* tranquility */);
    g.attach_enchantment(Target::Player(P1), tranq, P1);
    assert!(n(&mut g, chaos) > 200, "Tranquility guards against chaos");
}

#[test]
fn a_stopped_spell_is_lost_and_reported() -> TestResult {
    let mut g = world();
    let suppress = sp(&g, 79 /* suppress_magic */);
    g.attach_enchantment(Target::Player(P1), suppress, P1);
    let mut stopped = 0;
    for _ in 0..30 {
        let events = cast(&mut g, 59 /* aura_of_majesty */, Target::Player(P0));
        if events
            .iter()
            .any(|e| matches!(e, Event::SpellSuppressed { by, .. } if *by == P1))
        {
            stopped += 1;
            assert!(!g
                .wizard(P0)
                .ok_or("wizard is missing")?
                .global()
                .contains(&sp(&g, 59 /* aura_of_majesty */)));
            break;
        }
        g.clear_globals(P0);
    }
    assert_eq!(
        stopped, 1,
        "aura costs 400 against 500: more than half stopped"
    );
    Ok(())
}

#[test]
fn a_stopped_spell_names_the_enchantment_that_stopped_it() {
    for (guard, spell) in [
        (157 /* tranquility */, 83 /* fire_bolt */),
        (156 /* life_force */, 167 /* life_drain */),
    ] {
        let mut g = world();
        let (guard_id, spell_id) = (sp(&g, guard), sp(&g, spell));
        g.attach_enchantment(Target::Player(P1), guard_id, P1);
        let named = (0..60).find_map(|_| g.suppressed_by(P0, spell_id));
        assert_eq!(named, Some((P1, guard_id)), "{guard} stops {spell}");
    }
}

#[test]
fn a_dispel_can_undo_an_enchanted_road_on_its_square_only() -> TestResult {
    let mut g = world();
    {
        let grid = g
            .map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus);
        for x in 5..=6 {
            grid.set_road(x, 5, true);
            grid.set_enchanted_road(x, 5, true);
        }
    }
    let mut r = rng();
    // A dispel far stronger than Enchant Road's difficulty always works.
    let removed = g.dispel_target(
        Target::Tile(at(5, 5)),
        1_000_000,
        P1,
        &mut r,
        &mut Vec::new(),
    );
    assert_eq!(removed, 1);
    let grid = g
        .map
        .as_ref()
        .ok_or("as_ref is missing")?
        .plane(Plane::Arcanus);
    assert!(grid.has_road(5, 5), "the road itself stays");
    assert!(!grid.has_enchanted_road(5, 5));
    assert!(
        grid.has_enchanted_road(6, 5),
        "the next square is left alone"
    );
    // A plain road has nothing to dispel.
    assert_eq!(
        g.dispel_target(
            Target::Tile(at(5, 5)),
            1_000_000,
            P1,
            &mut r,
            &mut Vec::new()
        ),
        0
    );
    Ok(())
}

#[test]
fn a_wizard_turns_off_only_their_own_spell_on_a_unit() -> TestResult {
    let mut g = world();
    let spear = g
        .content
        .rules
        .units
        .id_numbered(95)
        .ok_or("high_elf_spearmen is not in the content")?;
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let strength = sp(&g, 4 /* giant_strength */);
    g.attach_enchantment(Target::Unit(unit), strength, P0);
    g.attach_enchantment(Target::Unit(unit), strength, P1);
    let off = |g: &mut Game, player| {
        g.apply(Command::TurnOffUnitSpell {
            player,
            unit,
            spell: strength,
        })
    };
    assert_eq!(
        off(&mut g, P0),
        vec![Event::EnchantmentLapsed {
            player: P0,
            spell: strength
        }]
    );
    assert_eq!(g.units().unit_enchantments(unit), vec![(strength, P1)]);
    // The other wizard's copy is not theirs to turn off; nor is it a second time.
    assert!(matches!(off(&mut g, P0)[..], [Event::Rejected(_)]));
    assert_eq!(g.units().unit_enchantments(unit), vec![(strength, P1)]);
    Ok(())
}

/// The squares round (x, y) turn to ocean too: no shore to scramble to.
fn open_sea(g: &mut Game, x: u32, y: u32) {
    let ocean = g
        .content
        .terrains
        .id_numbered(num::terrain::OCEAN)
        .expect("id_of succeeds");
    for dy in 0..3 {
        for dx in 0..3 {
            g.map
                .as_mut()
                .expect("value succeeds")
                .plane_mut(Plane::Arcanus)
                .set(x - 1 + dx, y - 1 + dy, ocean);
        }
    }
}

#[test]
fn turning_off_water_walking_at_sea_drowns_the_unit() -> TestResult {
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
        .set(3, 3, ocean);
    open_sea(&mut g, 3, 3);
    let spear = g
        .content
        .rules
        .units
        .id_numbered(95)
        .ok_or("high_elf_spearmen is not in the content")?;
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let walking = sp(&g, 8 /* water_walking */);
    g.attach_enchantment(Target::Unit(unit), walking, P0);
    assert!(
        g.drown_stranded(None).is_empty(),
        "afloat while the spell holds"
    );
    assert_eq!(g.drowned_if_off(unit, walking), 1);
    assert_eq!(g.units().unit_enchantments(unit), vec![(walking, P0)]);
    let events = g.apply(Command::TurnOffUnitSpell {
        player: P0,
        unit,
        spell: walking,
    });
    assert!(events.contains(&Event::UnitsDrowned {
        player: P0,
        units: 1
    }));
    assert!(g.world.get::<&crate::unit::Unit>(unit).is_err());
    Ok(())
}

#[test]
fn flight_keeps_a_unit_afloat_and_turning_it_off_drowns_it() -> TestResult {
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
        .set(3, 3, ocean);
    open_sea(&mut g, 3, 3);
    let spear = g
        .content
        .rules
        .units
        .id_numbered(95)
        .ok_or("high_elf_spearmen is not in the content")?;
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let flight = sp(&g, 56 /* flight */);
    g.attach_enchantment(Target::Unit(unit), flight, P0);
    assert!(g.units().unit_has_ability(unit, Ability::new(24)));
    assert!(g.drown_stranded(None).is_empty());
    assert_eq!(g.drowned_if_off(unit, flight), 1);
    let events = g.apply(Command::TurnOffUnitSpell {
        player: P0,
        unit,
        spell: flight,
    });
    assert!(events.contains(&Event::UnitsDrowned {
        player: P0,
        units: 1
    }));
    Ok(())
}

#[test]
fn disenchant_area_is_refused_where_there_is_nothing_of_another_wizards() {
    let g = world();
    let da = sp(&g, 204 /* disenchant_area */);
    assert_eq!(
        g.check_target(P0, da, Some(Target::Tile(at(9, 9)))),
        Err(crate::wizard::magic::MagicError::Target(
            TargetError::NothingToDispel
        ))
    );
}

#[test]
fn a_spell_blast_is_refused_without_contact_or_the_mana_to_match() -> TestResult {
    let mut g = world();
    let aura = sp(&g, 59 /* aura_of_majesty */);
    let cost = g.casting_cost(P1, aura, false);
    g.scenario_casting(P1, aura, cost - 60);
    let blast = sp(&g, 58 /* spell_blast */);
    let refused = |g: &Game| g.check_target(P0, blast, Some(Target::Player(P1)));
    assert_eq!(
        refused(&g),
        Err(crate::wizard::magic::MagicError::Target(
            TargetError::NoContact
        ))
    );
    g.make_contact(P0, P1);
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(10);
    assert_eq!(
        refused(&g),
        Err(crate::wizard::magic::MagicError::Target(
            TargetError::CannotMatchMana
        ))
    );
    Ok(())
}
