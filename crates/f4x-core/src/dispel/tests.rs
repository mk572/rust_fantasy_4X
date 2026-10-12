use super::*;
use crate::city::Position;
use crate::magic::{Casting, Retort};
use crate::map::{Grid, Map, Node, Plane};
use crate::player::Player;
use crate::rules::effects::{EffectRule, Trigger};
use crate::rules::{CastContext, Duration, Rarity, Registry, SpellDef, SpellTarget};
use crate::testutil::content_with_magic;
use crate::Command;
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

/// A spell: lasting ones have `dispel_difficulty` equal to their cost.
fn spell(
    id: &str,
    realm: &str,
    cost: u32,
    target: SpellTarget,
    lasting: bool,
    effect: Option<crate::rules::effects::Effect>,
) -> SpellDef {
    SpellDef {
        id: id.into(),
        name: id.into(),
        realm: realm.into(),
        rarity: Rarity::Common,
        cost,
        research_cost: 10,
        contexts: vec![CastContext::Overland],
        overland_x5: false,
        target,
        duration: if lasting {
            Duration::Permanent
        } else {
            Duration::Instant
        },
        upkeep: 0,
        dispel_difficulty: if lasting { cost } else { 0 },
        save: None,
        effects: effect
            .map(|e| {
                vec![EffectRule {
                    when: Trigger::OnCast,
                    target: crate::rules::effects::TargetSel::Chosen,
                    condition: crate::rules::effects::Condition::Always,
                    effect: e,
                }]
            })
            .unwrap_or_default(),
    }
}

/// Two wizards (lots of mana and skill), grassland, and the spells the tests use.
fn world() -> Game {
    use crate::rules::effects::Effect;
    let mut c = content_with_magic();
    let mut defs = c.rules.spells.defs().to_vec();
    defs.extend([
        spell(
            "spell_lock",
            "sorcery",
            20,
            SpellTarget::OwnUnit,
            true,
            None,
        ),
        spell("aura", "sorcery", 100, SpellTarget::None, true, None),
        spell(
            "suppress_magic",
            "sorcery",
            1250,
            SpellTarget::None,
            true,
            None,
        ),
        spell(
            "disjunction",
            "arcane",
            5000,
            SpellTarget::GlobalEnchantment,
            false,
            Some(Effect::Custom(DISJUNCTION.into())),
        ),
        spell(
            "spell_binding",
            "sorcery",
            5000,
            SpellTarget::GlobalEnchantment,
            false,
            Some(Effect::Custom(SPELL_BINDING.into())),
        ),
        spell(
            "spell_blast",
            "sorcery",
            50,
            SpellTarget::EnemyWizard,
            false,
            Some(Effect::Custom(SPELL_BLAST.into())),
        ),
        spell(
            "disenchant_area",
            "arcane",
            5000,
            SpellTarget::Tile,
            false,
            Some(Effect::Dispel { strength: 5000 }),
        ),
    ]);
    c.rules.spells = Registry::new(defs);
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(11);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    for (id, name) in [(P0, "Merlin"), (P1, "Rjak")] {
        let mut p = Player::new(id, name);
        p.mana = 20_000;
        g.players.push(p);
        g.init_wizard(id, &[("life", 2)], &[], None).unwrap();
        g.magic_mut(id).unwrap().skill_left = 99_999;
    }
    g
}

fn sp(g: &Game, id: &str) -> SpellId {
    g.content.rules.spells.id_of(id).unwrap()
}

fn cast(g: &mut Game, spell: &str, target: Target) -> Vec<Event> {
    let s = sp(g, spell);
    g.magic_mut(P0).unwrap().known.insert(s);
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
    let aura = sp(&g, "aura"); // sorcery, 100
    assert_eq!(g.dispel_difficulty(aura, Some(P0)), 100);
    g.magic_mut(P0)
        .unwrap()
        .retorts
        .insert(Retort::SorceryMastery);
    assert_eq!(
        g.dispel_difficulty(aura, Some(P0)),
        200,
        "its realm's mastery"
    );
    g.magic_mut(P0)
        .unwrap()
        .retorts
        .insert(Retort::NatureMastery);
    assert_eq!(
        g.dispel_difficulty(aura, Some(P0)),
        200,
        "another realm's does not count"
    );
    g.magic_mut(P0).unwrap().retorts.insert(Retort::Archmage);
    assert_eq!(g.dispel_difficulty(aura, Some(P0)), 300);
    assert_eq!(g.dispel_difficulty(aura, Some(P1)), 100);
    assert_eq!(g.dispel_difficulty(aura, None), 100);
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
        g.dispel_target(Target::Unit(unit), 1_000_000, P0, &mut r),
        0
    );
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r),
        1
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
        g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r),
        1,
        "the first dispel only takes the lock"
    );
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r),
        1
    );
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P1, &mut r),
        0
    );
    // A weak dispel rarely beats the lock's 150, and a lock never shields from its owner.
    g.attach_enchantment(Target::Unit(unit), lock, P0);
    g.attach_enchantment(Target::Unit(unit), strength, P1);
    assert_eq!(
        g.dispel_target(Target::Unit(unit), 1_000_000, P0, &mut r),
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
    let city = g.city_at(at(6, 6)).unwrap();
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let unit = g.spawn_unit(P0, at(6, 6), spear);
    let elsewhere = g.spawn_unit(P0, at(9, 9), spear);
    let (strength, aura) = (sp(&g, "giant_strength"), sp(&g, "aura"));
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
        owner: Some(0),
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
    g.magic_mut(P1).unwrap().known.insert(da);
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
    let aura = sp(&g, "aura");
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
    assert!(g.magic(P0).unwrap().global.contains(&aura));
    assert!(!g.magic(P1).unwrap().global.contains(&aura));
    // Now P1 ends it with Disjunction.
    let dj = sp(&g, "disjunction");
    g.magic_mut(P1).unwrap().known.insert(dj);
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
    assert!(g.magic(P0).unwrap().global.is_empty());
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
    let aura = sp(&g, "aura"); // difficulty 100
    g.attach_enchantment(Target::Player(P1), aura, P1);
    let mut r = rng();
    let wins = (0..2000)
        .filter(|_| {
            let won = g.attack_global(P0, P1, aura, 100, false, &mut r) == Some(true);
            g.attach_enchantment(Target::Player(P1), aura, P1);
            won
        })
        .count();
    assert!((900..1100).contains(&wins), "half of 2000: {wins}");
}

#[test]
fn spell_blast_costs_what_the_victim_has_spent_and_ends_their_spell() {
    let mut g = world();
    let aura = sp(&g, "aura");
    let cost = g.casting_cost(P1, aura, false);
    g.magic_mut(P1).unwrap().casting = Some(Casting {
        spell: aura,
        remaining: cost - 60,
    });
    assert_eq!(g.mana_spent_casting(P1), Some(60));
    let mana = g.player(P0).unwrap().mana;
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
    assert_eq!(g.player(P0).unwrap().mana, mana - paid - 60);
    assert_eq!(g.magic(P1).unwrap().casting, None);
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
    let aura = sp(&g, "aura");
    let cost = g.casting_cost(P1, aura, false);
    g.magic_mut(P1).unwrap().casting = Some(Casting {
        spell: aura,
        remaining: cost - 90,
    });
    g.player_mut(P0).unwrap().mana = g.casting_cost(P0, sp(&g, "spell_blast"), false) + 50;
    let events = cast(&mut g, "spell_blast", Target::Player(P1));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::SpellBlasted { .. })),
        "{events:?}"
    );
    assert!(g.magic(P1).unwrap().casting.is_some());
}

#[test]
fn suppress_magic_stops_enemy_spells_with_its_strength_over_the_spells_cost() {
    let mut g = world();
    let suppress = sp(&g, "suppress_magic");
    g.attach_enchantment(Target::Player(P1), suppress, P1);
    // Strength 500 against difficulty 500: half.
    let big = sp(&g, "spell_binding"); // cost 5000 ...
    let mid = {
        let mut defs = g.content.rules.spells.defs().to_vec();
        defs.push(spell("mid", "life", 500, SpellTarget::None, false, None));
        g.content.rules.spells = Registry::new(defs);
        sp(&g, "mid")
    };
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
fn a_stopped_spell_is_lost_and_reported() {
    let mut g = world();
    let suppress = sp(&g, "suppress_magic");
    g.attach_enchantment(Target::Player(P1), suppress, P1);
    let mut stopped = 0;
    for _ in 0..30 {
        let events = cast(&mut g, "aura", Target::Player(P0));
        if events
            .iter()
            .any(|e| matches!(e, Event::SpellSuppressed { by, .. } if *by == P1))
        {
            stopped += 1;
            assert!(!g.magic(P0).unwrap().global.contains(&sp(&g, "aura")));
            break;
        }
        g.magic_mut(P0).unwrap().global.clear();
    }
    assert_eq!(stopped, 1, "aura costs 100 against 500: about 83% stopped");
}
