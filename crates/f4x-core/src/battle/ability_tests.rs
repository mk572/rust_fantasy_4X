//! The attack abilities that are not flag bits, run through a real battle: real units from the
//! content, started with `Game::start_battle`, a command from one side, and the figures and the
//! hit points the other side lost, over many seeds. Task 412.
//!
//! Each test compares a unit with the ability against the same unit without it (or against a
//! defender that is immune), so a rule that stops working, or an ability nothing reads, fails.

use super::*;
use crate::game::rng::GameRng;
use crate::game::testutil::{num, shipped};
use crate::game::Game;
use crate::map::{Grid, Map, Plane, Position};
use crate::player::PlayerId;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const SEEDS: u64 = 300;

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
        .expect("grassland is in the content");
    let mut g = Game::new(7);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.add_player(P0, "Attacker");
    g.add_player(P1, "Defender");
    g
}

/// A battle between one real unit of each side, the attacker at (5, 10) and the defender
/// `gap` squares away.
fn duel(attacker: &str, defender: &str, gap: i32) -> Battle {
    let mut g = world();
    let spawn = |g: &mut Game, owner, id: &str, x| {
        let def = g
            .content
            .rules
            .units
            .id_of(id)
            .unwrap_or_else(|| panic!("{id} is not in the content"));
        g.spawn_unit(owner, at(x, 8), def)
    };
    let a = spawn(&mut g, P0, attacker, 8);
    let d = spawn(&mut g, P1, defender, 9);
    g.start_battle(&[a], &[d]).expect("the battle starts");
    let mut b = g.battle().expect("a battle is running").clone();
    for u in &mut b.units {
        u.cell = match u.side {
            Side::Attacker => (5, 10),
            Side::Defender => (5 + gap, 10),
        };
        u.moves_left = u.moves;
    }
    b
}

/// The hit points a unit has left across its figures.
fn hp(u: &BattleUnit) -> i32 {
    if u.figures == 0 {
        0
    } else {
        u.figures as i32 * u.hits - u.front_damage
    }
}

fn index(b: &Battle, side: Side) -> usize {
    b.units
        .iter()
        .position(|u| u.side == side)
        .expect("each side has a unit")
}

/// What the defender lost over `SEEDS` identical attacks: (figures, hit points), where `shoot`
/// chooses a shot over a melee attack and `tweak` changes the battle before each one.
fn losses(b: &Battle, shoot: bool, tweak: impl Fn(&mut Battle)) -> (u32, i32) {
    let (a, d) = (index(b, Side::Attacker), index(b, Side::Defender));
    let (mut figures, mut hits) = (0u32, 0i32);
    for seed in 0..SEEDS {
        let mut c = b.clone();
        c.rng = GameRng::new(seed);
        tweak(&mut c);
        let before = (c.units[d].figures, hp(&c.units[d]));
        let command = if shoot {
            BattleCommand::Shoot { unit: a, target: d }
        } else {
            BattleCommand::Attack { unit: a, target: d }
        };
        c.apply(command).expect("the attack is legal");
        figures += before.0 - c.units[d].figures;
        hits += before.1 - hp(&c.units[d]);
    }
    (figures, hits)
}

fn without(ability: u16) -> impl Fn(&mut Battle) {
    move |b: &mut Battle| {
        for u in &mut b.units {
            if u.side == Side::Attacker {
                u.abilities
                    .retain(|a| *a != crate::game::testutil::ability(ability));
            }
        }
    }
}

fn immune(ability: u16) -> impl Fn(&mut Battle) {
    move |b: &mut Battle| {
        for u in &mut b.units {
            if u.side == Side::Defender {
                u.abilities.push(crate::game::testutil::ability(ability));
            }
        }
    }
}

fn nothing(_: &mut Battle) {}

#[test]
fn a_death_gaze_kills_figures_that_fail_to_resist_unless_the_target_is_immune() {
    let b = duel("night_stalker", "high_men_spearmen", 1);
    let (with, _) = losses(&b, false, nothing);
    let (plain, _) = losses(&b, false, without(15 /* death_gaze */));
    assert!(
        with > plain,
        "the gaze kills figures: {with} against {plain}"
    );
    let (resists, _) = losses(&b, false, immune(16 /* death_immunity */));
    assert!(
        resists < with,
        "death immunity stops the gaze: {resists} against {with}"
    );
}

#[test]
fn a_multiple_gaze_stones_and_kills_unless_the_target_is_immune_to_both() {
    let b = duel("chaos_spawn", "high_men_spearmen", 1);
    let (with, _) = losses(&b, false, nothing);
    let (plain, _) = losses(&b, false, without(46 /* multi_gaze */));
    assert!(
        with > plain,
        "the gaze kills figures: {with} against {plain}"
    );
    // Either gaze alone still kills; the first to roll may leave the other nothing to do, so each
    // is compared with the unit without the ability, not with the two together.
    let (stoning_only, _) = losses(&b, false, immune(16 /* death_immunity */));
    let (death_only, _) = losses(&b, false, immune(65 /* stoning_immunity */));
    let (both, _) = losses(&b, false, |b: &mut Battle| {
        immune(16 /* death_immunity */)(b);
        immune(65 /* stoning_immunity */)(b);
    });
    assert!(
        stoning_only > plain,
        "the stoning gaze works alone: {stoning_only} against {plain}"
    );
    assert!(
        death_only > plain,
        "the death gaze works alone: {death_only} against {plain}"
    );
    assert!(
        both < stoning_only && both < death_only,
        "immune to both, neither gaze is left: {both} against {stoning_only} and {death_only}"
    );
}

#[test]
fn a_stoning_touch_turns_figures_to_stone_unless_the_target_is_immune() {
    let b = duel("cockatrices", "high_men_spearmen", 1);
    let (with, _) = losses(&b, false, nothing);
    let (plain, _) = losses(&b, false, without(66 /* stoning_touch */));
    assert!(
        with > plain,
        "the touch kills figures: {with} against {plain}"
    );
    let (resists, _) = losses(&b, false, immune(65 /* stoning_immunity */));
    assert!(
        resists <= plain,
        "stoning immunity stops it: {resists} against {plain}"
    );
}

#[test]
fn a_poison_touch_costs_the_target_hit_points_unless_it_is_immune() {
    let b = duel("orc_wyvern_riders", "high_men_spearmen", 1);
    let (_, with) = losses(&b, false, nothing);
    let (_, plain) = losses(&b, false, without(53 /* poison_touch */));
    assert!(with > plain, "the poison hurts: {with} against {plain}");
    let (_, resists) = losses(&b, false, immune(52 /* poison_immunity */));
    assert!(
        resists <= plain,
        "poison immunity stops it: {resists} against {plain}"
    );
}

/// A ranged magic attack: the ability makes the shot a bolt, which hurts a target at range and
/// is stopped by magic immunity.
fn assert_a_bolt(shooter: &str, ability: u16) {
    let b = duel(shooter, "high_men_spearmen", 6);
    let a = index(&b, Side::Attacker);
    assert!(
        b.units[a]
            .abilities
            .contains(&crate::game::testutil::ability(ability)),
        "{shooter} has {ability}"
    );
    assert_eq!(
        b.units[a].ranged_kind,
        RangedKind::Magic,
        "{ability} is a magic attack"
    );
    assert!(
        b.units[a].ammo > 0 && b.units[a].ranged > 0,
        "{shooter} can shoot"
    );
    let (_, open) = losses(&b, true, nothing);
    assert!(open > 0, "{ability} hurts a target in range");
    let (_, stopped) = losses(&b, true, immune(40 /* magic_immunity */));
    assert!(
        stopped < open,
        "magic immunity softens {ability}: {stopped} against {open}"
    );
}

#[test]
fn a_drow_bolt_is_a_ranged_magic_attack() {
    assert_a_bolt("dark_elf_spearmen", 18 /* drow_bolt */);
}

#[test]
fn a_nature_bolt_is_a_ranged_magic_attack() {
    assert_a_bolt("druid", 47 /* nature_bolt */);
}

#[test]
fn a_shaman_bolt_is_a_ranged_magic_attack() {
    assert_a_bolt("barbarian_shaman", 58 /* shaman_bolt */);
}

#[test]
fn a_sprite_bolt_is_a_ranged_magic_attack() {
    assert_a_bolt("sprites", 63 /* sprite_bolt */);
}

#[test]
fn a_magical_weapon_hurts_what_normal_weapons_barely_scratch() {
    let b = duel("high_men_swordsmen", "high_men_spearmen", 1);
    let weapon_proof = immune(72 /* weapon_immunity */);
    let (_, normal) = losses(&b, false, &weapon_proof);
    let (_, magical) = losses(&b, false, |b: &mut Battle| {
        weapon_proof(b);
        for u in &mut b.units {
            if u.side == Side::Attacker {
                u.abilities.push(Ability::new(41));
                u.magical_weapon = true;
            }
        }
    });
    assert!(
        magical > normal,
        "magical blows pass the immunity: {magical} against {normal}"
    );
}

#[test]
fn an_eldritch_weapon_takes_a_plus_off_the_defenders_blocks() {
    let b = duel("high_men_swordsmen", "high_men_spearmen", 1);
    let (_, plain) = losses(&b, false, nothing);
    let (_, eldritch) = losses(&b, false, |b: &mut Battle| {
        for u in &mut b.units {
            if u.side == Side::Attacker {
                u.abilities.push(Ability::new(19));
            }
        }
    });
    assert!(
        eldritch > plain,
        "the defender blocks less: {eldritch} against {plain}"
    );
}

#[test]
fn a_major_spellcaster_has_forty_mana_and_casts_the_spells_of_its_realm() {
    let major = duel("arch_angel", "high_men_spearmen", 6);
    let a = index(&major, Side::Attacker);
    assert_eq!(major.units[a].mana, 40);
    assert!(major.units[a].has(Ability::new(62)));
    let mut g = world();
    let def = g
        .content
        .rules
        .units
        .id_numbered(178)
        .expect("arch_angel exists");
    let angel = g.spawn_unit(P0, at(8, 8), def);
    let foe_def = g
        .content
        .rules
        .units
        .id_numbered(104)
        .expect("spearmen exist");
    let foe = g.spawn_unit(P1, at(9, 8), foe_def);
    g.start_battle(&[angel], &[foe]).expect("the battle starts");
    let spells = g.battle_unit_spells(index(g.battle().expect("running"), Side::Attacker));
    assert!(
        !spells.is_empty(),
        "it casts the spells of its realm (life)"
    );
}
