use super::*;
use crate::game::testutil::content_with_magic;
use crate::map::{Grid, Map, Plane, Position};

use crate::feature::node::Node;
use crate::game::Command;
use crate::rules::Registry;
use crate::wizard::magic::{Casting, Retort};
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

/// Two wizards (lots of mana and skill) and grassland, with the game's real spells. Disenchant
/// Area and Disjunction are made overwhelming (a strength and a cost of 5000, which no enchantment
/// resists) so that what they clear does not depend on luck.
fn world() -> Game {
    use crate::rules::effects::Effect;
    let mut c = content_with_magic();
    let mut defs = c.rules.spells.defs().to_vec();
    for d in defs.iter_mut().filter(|d| d.id == "disenchant_area") {
        d.effects[0].effect = Effect::Dispel { strength: 5000 };
    }
    for d in defs.iter_mut().filter(|d| d.id == "disjunction") {
        d.cost = 5000;
    }
    c.rules.spells = Registry::new(defs);
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(11);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    for (id, name) in [(P0, "Merlin"), (P1, "Rjak")] {
        g.add_player(id, name);
        g.treasury_mut(id).unwrap().mana = 20_000;
        g.init_wizard(id, &[("life", 2)], &[], None).unwrap();
        g.wizard_mut(id)
            .unwrap()
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 99_999;
    }
    g
}

fn sp(g: &Game, id: &str) -> SpellId {
    g.content.rules.spells.id_of(id).unwrap()
}

fn cast(g: &mut Game, spell: &str, target: Target) -> Vec<Event> {
    let s = sp(g, spell);
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(s);
    g.apply(Command::CastSpell {
        player: P0,
        spell: s,
        target: Some(target),
    })
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
fn masteries_and_archmage_multiply_a_spells_difficulty() {
    let mut g = world();
    let aura = sp(&g, "aura_of_majesty"); // sorcery, 400
    assert_eq!(g.dispel_difficulty(aura, Some(P0)), 400);
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::SorceryMastery);
    assert_eq!(
        g.dispel_difficulty(aura, Some(P0)),
        800,
        "its realm's mastery"
    );
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::NatureMastery);
    assert_eq!(
        g.dispel_difficulty(aura, Some(P0)),
        800,
        "another realm's does not count"
    );
    g.wizard_mut(P0)
        .unwrap()
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::Archmage);
    assert_eq!(g.dispel_difficulty(aura, Some(P0)), 1200);
    assert_eq!(g.dispel_difficulty(aura, Some(P1)), 400);
    assert_eq!(g.dispel_difficulty(aura, None), 400);
}

#[test]
fn only_other_wizards_enchantments_are_dispelled() {
    let mut g = world();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let strength = sp(&g, "giant_strength");
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
}

#[test]
fn a_dispel_names_each_spell_it_removes() {
    let mut g = world();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let strength = sp(&g, "giant_strength");
    g.attach_enchantment(Target::Unit(unit), strength, P0);
    let mut gone = Vec::new();
    g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut rng(), &mut gone);
    assert_eq!(
        gone,
        vec![Event::EnchantmentDispelled {
            victim: P0,
            spell: strength,
            on_unit: true
        }]
    );
}

#[test]
fn spell_lock_must_be_dispelled_before_anything_else_on_the_unit() {
    let mut g = world();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let (lock, strength) = (sp(&g, "spell_lock"), sp(&g, "giant_strength"));
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
}

#[test]
fn disenchant_area_clears_a_city_its_units_and_a_warped_node() {
    let mut g = world();
    let race = g.content.rules.races.id_of("elf");
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(6, 6),
        name: "Town".into(),
        race,
        population: 3,
    });
    let city = g.economy().city_at(at(6, 6)).unwrap();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(6, 6), spear);
    let elsewhere = g.spawn_unit(P0, at(9, 9), spear);
    let (strength, aura) = (sp(&g, "giant_strength"), sp(&g, "aura_of_majesty"));
    g.attach_enchantment(Target::City(city), aura, P0);
    g.attach_enchantment(Target::Unit(unit), strength, P0);
    g.attach_enchantment(Target::Unit(elsewhere), strength, P0);
    g.map.as_mut().unwrap().set_nodes(vec![Node {
        plane: Plane::Arcanus,
        x: 6,
        y: 6,
        realm: "chaos".into(),
        power: 8,
        aura: vec![(6, 6)],
        owner: Some(PlayerId(0)),
        guardian: false,
        warped: true,
    }]);
    let events = cast(&mut g, "disenchant_area", Target::Tile(at(6, 6)));
    assert!(
        events.contains(&Event::Dispelled {
            player: P0,
            removed: 1
        }),
        "only the warp on the caster's own node goes, their own enchantments stay: {events:?}"
    );
    g.map.as_mut().unwrap().node_mut(0).unwrap().warped = true;
    // The same cast by the other wizard takes it all.
    let da = sp(&g, "disenchant_area");
    g.wizard_mut(P1)
        .unwrap()
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(da);
    let events = g.apply(Command::CastSpell {
        player: P1,
        spell: sp(&g, "disenchant_area"),
        target: Some(Target::Tile(at(6, 6))),
    });
    assert!(
        events.contains(&Event::Dispelled {
            player: P1,
            removed: 3
        }),
        "{events:?}"
    );
    assert!(!g.node_at(at(6, 6)).unwrap().warped);
    let left = g.world.get::<&UnitEnchantments>(elsewhere).unwrap().0.len();
    assert_eq!(left, 1, "another square is left alone");
}

#[test]
fn disjunction_ends_and_spell_binding_steals_an_enemy_global() {
    let mut g = world();
    let aura = sp(&g, "aura_of_majesty");
    g.attach_enchantment(Target::Player(P1), aura, P1);
    let events = cast(&mut g, "spell_binding", Target::Global(P1, aura));
    assert!(
        events.contains(&Event::GlobalSpellTaken {
            player: P0,
            victim: P1,
            spell: aura
        }),
        "{events:?}"
    );
    assert!(g.wizard(P0).unwrap().has_global(aura));
    assert!(!g.wizard(P1).unwrap().has_global(aura));
    // Now P1 ends it with Disjunction.
    let dj = sp(&g, "disjunction");
    g.wizard_mut(P1)
        .unwrap()
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .insert(dj);
    let events = g.apply(Command::CastSpell {
        player: P1,
        spell: sp(&g, "disjunction"),
        target: Some(Target::Global(P0, aura)),
    });
    assert!(
        events.contains(&Event::GlobalSpellRemoved {
            player: P1,
            victim: P0,
            spell: aura
        }),
        "{events:?}"
    );
    assert!(g.wizard(P0).unwrap().global().is_empty());
    // Nothing to take: refused.
    let events = cast(&mut g, "disjunction", Target::Global(P1, aura));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
    // You cannot disjoin your own.
    g.attach_enchantment(Target::Player(P0), aura, P0);
    let events = cast(&mut g, "disjunction", Target::Global(P0, aura));
    assert!(matches!(events.as_slice(), [Event::Rejected(_)]));
}

#[test]
fn a_weak_disjunction_often_fails() {
    let mut g = world();
    let aura = sp(&g, "aura_of_majesty"); // difficulty 400
    g.attach_enchantment(Target::Player(P1), aura, P1);
    let mut r = rng();
    let wins = (0..2000)
        .filter(|_| {
            let won = g.attack_global(P0, P1, aura, 400, false, &mut r) == Some(true);
            g.attach_enchantment(Target::Player(P1), aura, P1);
            won
        })
        .count();
    assert!((900..1100).contains(&wins), "half of 2000: {wins}");
}

#[test]
fn spell_blast_costs_what_the_victim_has_spent_and_ends_their_spell() {
    let mut g = world();
    let aura = sp(&g, "aura_of_majesty");
    let cost = g.casting_cost(P1, aura, false);
    g.wizard_mut(P1)
        .unwrap()
        .get_mut::<crate::wizard::magic::Magic>()
        .casting = Some(Casting {
        spell: aura,
        remaining: cost - 60,
    });
    assert_eq!(g.mana_spent_casting(P1), Some(60));
    let mana = g.treasury(P0).unwrap().mana;
    let blast = sp(&g, "spell_blast");
    let paid = g.casting_cost(P0, blast, false);
    let events = cast(&mut g, "spell_blast", Target::Player(P1));
    assert!(
        events.contains(&Event::SpellBlasted {
            player: P0,
            victim: P1,
            spell: aura,
            mana: 60
        }),
        "{events:?}"
    );
    assert_eq!(g.treasury(P0).unwrap().mana, mana - paid - 60);
    assert_eq!(g.wizard(P1).unwrap().casting(), None);
    // Nothing being cast: not a target.
    let events = cast(&mut g, "spell_blast", Target::Player(P1));
    assert!(
        matches!(events.as_slice(), [Event::Rejected(_)]),
        "{events:?}"
    );
}

#[test]
fn spell_blast_fails_when_the_caster_cannot_match_the_mana() {
    let mut g = world();
    let aura = sp(&g, "aura_of_majesty");
    let cost = g.casting_cost(P1, aura, false);
    g.wizard_mut(P1)
        .unwrap()
        .get_mut::<crate::wizard::magic::Magic>()
        .casting = Some(Casting {
        spell: aura,
        remaining: cost - 90,
    });
    g.treasury_mut(P0).unwrap().mana = g.casting_cost(P0, sp(&g, "spell_blast"), false) + 50;
    let events = cast(&mut g, "spell_blast", Target::Player(P1));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::SpellBlasted { .. })),
        "{events:?}"
    );
    assert!(g.wizard(P1).unwrap().casting().is_some());
}

#[test]
fn suppress_magic_stops_enemy_spells_with_its_strength_over_the_spells_cost() {
    let mut g = world();
    let suppress = sp(&g, "suppress_magic");
    g.attach_enchantment(Target::Player(P1), suppress, P1);
    // Strength 500 against difficulty 500: half.
    let big = sp(&g, "spell_of_mastery"); // cost 5000 ...
    let mid = sp(&g, "storm_giant"); // cost 500
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
        sp(&g, "life_drain"),
        sp(&g, "fire_bolt"),
        sp(&g, "life_force"),
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
    let tranq = sp(&g, "tranquility");
    g.attach_enchantment(Target::Player(P1), tranq, P1);
    assert!(n(&mut g, chaos) > 200, "Tranquility guards against chaos");
}

#[test]
fn a_stopped_spell_is_lost_and_reported() {
    let mut g = world();
    let suppress = sp(&g, "suppress_magic");
    g.attach_enchantment(Target::Player(P1), suppress, P1);
    let mut stopped = 0;
    for _ in 0..30 {
        let events = cast(&mut g, "aura_of_majesty", Target::Player(P0));
        if events
            .iter()
            .any(|e| matches!(e, Event::SpellSuppressed { by, .. } if *by == P1))
        {
            stopped += 1;
            assert!(!g
                .wizard(P0)
                .unwrap()
                .global()
                .contains(&sp(&g, "aura_of_majesty")));
            break;
        }
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::magic::Magic>()
            .global
            .clear();
    }
    assert_eq!(
        stopped, 1,
        "aura costs 400 against 500: more than half stopped"
    );
}

#[test]
fn a_dispel_can_undo_an_enchanted_road_on_its_square_only() {
    let mut g = world();
    {
        let grid = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
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
    let grid = g.map.as_ref().unwrap().plane(Plane::Arcanus);
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
}

#[test]
fn a_wizard_turns_off_only_their_own_spell_on_a_unit() {
    let mut g = world();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let strength = sp(&g, "giant_strength");
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
}

#[test]
fn turning_off_water_walking_at_sea_drowns_the_unit() {
    let mut g = world();
    let ocean = g.content.terrains.id_of("ocean").unwrap();
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set(3, 3, ocean);
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let walking = sp(&g, "water_walking");
    g.attach_enchantment(Target::Unit(unit), walking, P0);
    assert!(
        g.drown_stranded().is_empty(),
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
}

#[test]
fn flight_keeps_a_unit_afloat_and_turning_it_off_drowns_it() {
    let mut g = world();
    let ocean = g.content.terrains.id_of("ocean").unwrap();
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set(3, 3, ocean);
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(3, 3), spear);
    let flight = sp(&g, "flight");
    g.attach_enchantment(Target::Unit(unit), flight, P0);
    assert!(g.units().unit_has_ability(unit, "flying"));
    assert!(g.drown_stranded().is_empty());
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
}
