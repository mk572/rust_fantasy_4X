//! Battles for tests, with no game behind them: units of the shipped content on an open field.
//!
//! Tests: none, because this is test support (`tests/it/` uses it, and `f4x-core`'s tests compare
//! what it builds with what a game builds: `a_unit_the_game_builds_equals_the_one_built_from_its_
//! definition_for_every_shipped_unit` and `a_battle_the_game_starts_has_the_rules_of_the_one_built_
//! for_the_tests` in battle/tests.rs).

use std::sync::{Arc, OnceLock};

use f4x_def::content::Content;
use f4x_def::half_int::HalfInt;
use f4x_def::id::PlayerId;

use crate::attack::{Combatant, DefenderState};
use crate::battle::{Battle, BattleCommand, BattleEvent};
use crate::battlemagic::caster::{range_percent, BattleCaster};
use crate::map::{BattleMap, Pos};
use crate::map::{GRID_H, GRID_W};
use crate::rng::GameRng;
use crate::side::Side;
use crate::unit::BattleUnit;
use f4x_def::rules::attack_nature::Mode;
use f4x_def::rules::Ability;

/// The attacker of a test battle.
pub const A: PlayerId = PlayerId(0);
/// The defender of a test battle.
pub const D: PlayerId = PlayerId(1);

/// The shipped content, loaded once per test process and shared by every battle.
pub fn content() -> Arc<Content> {
    static CONTENT: OnceLock<Arc<Content>> = OnceLock::new();
    Arc::clone(CONTENT.get_or_init(|| Arc::new(f4x_def::testdir::shipped().clone())))
}

/// The unit `number` of the content, as a battle makes it from its definition alone, for a side to
/// add. It is on no square (`(0, 0)`).
pub fn unit_as(number: u16, side: Side) -> BattleUnit {
    let content = content();
    let id = content
        .rules
        .units
        .id_numbered(number)
        .expect("a unit of the content");
    BattleUnit::of_def(&content, side, content.rules.units.get(id))
}

/// The battle rules of the shipped content.
pub fn rules() -> f4x_def::rules::BattleRules {
    content().rules.battle.clone()
}

/// The same battle with its dice started from `seed`.
pub fn reseeded(b: &Battle, seed: u64) -> Battle {
    let mut c = b.clone();
    c.rng = GameRng::new(seed);
    c
}

/// The unit `number` of the content, as a battle makes it, for the attackers to add.
pub fn unit_numbered(number: u16) -> BattleUnit {
    unit_as(number, Side::Attacker)
}

/// A battle of shipped units on an open field: `ours` of player [`A`] (the attacker) and `theirs`
/// of player [`D`], each a unit number of the content and the square it stands on, numbered in
/// the order of the lists (ours first). The dice start from `seed`.
pub fn real_battle(ours: &[(u16, Pos)], theirs: &[(u16, Pos)], seed: u64) -> Battle {
    let mut units = Vec::new();
    for (side, list) in [(Side::Attacker, ours), (Side::Defender, theirs)] {
        for &(number, cell) in list {
            units.push(unit_as(number, side).at(cell));
        }
    }
    let mut battle = Battle::new(content(), A, D, BattleMap::open(), units, seed);
    battle.rng = GameRng::new(seed);
    // Each side's player is a wizard with no casting skill and no mana (what a game makes of a
    // player that was just added), as far from its fortress as the rules say.
    let range = range_percent(&content().rules.config.casting_range, None, |d| d);
    for side in [Side::Attacker, Side::Defender] {
        battle.set_caster(side, BattleCaster::with_mana(0, HalfInt::ZERO, range));
    }
    battle
}
/// A battle of one Merging unit and one spearman each side, and a second spearman for each
/// side so that one unit lost does not end it.
pub fn merging_battle() -> Battle {
    real_battle(
        &[
            (189 /* great_wyrm */, (3, 10)),
            (104 /* high_men_spearmen */, (3, 12)),
        ],
        &[
            (104 /* high_men_spearmen */, (17, 10)),
            (104 /* high_men_spearmen */, (17, 12)),
        ],
        1,
    )
}

/// The battle with the ability `number` taken off `unit`: the one difference between the unit
/// as shipped and the same unit without it.
pub fn without(b: &Battle, unit: usize, number: u16) -> Battle {
    let mut c = b.clone();
    let gone = Ability::new(number);
    c.units[unit].strip(gone);
    c
}

pub fn target(figures: u32, hits: i32, defense: i32) -> DefenderState {
    DefenderState {
        hits,
        defense,
        figures,
        figures_max: figures,
        ..Default::default()
    }
}

pub fn bolt(strength: i32) -> Combatant {
    Combatant {
        strength,
        to_hit: 0,
        mode: Mode::Ranged,
        ..Default::default()
    }
}

pub fn end_turns(b: &mut Battle, n: usize) -> Vec<BattleEvent> {
    let mut all = Vec::new();
    for _ in 0..n {
        all.extend(b.apply(BattleCommand::EndTurn).expect("apply succeeds"));
    }
    all
}

/// Every command that could be written, for the units and squares of a battle.
pub fn every_command(b: &Battle) -> Vec<BattleCommand> {
    let mut all = vec![BattleCommand::EndTurn, BattleCommand::Flee];
    for unit in 0..b.units.len() + 1 {
        all.push(BattleCommand::Done { unit });
        for target in 0..b.units.len() + 1 {
            all.push(BattleCommand::Attack { unit, target });
            all.push(BattleCommand::Shoot { unit, target });
        }
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                all.push(BattleCommand::Move { unit, to: (x, y) });
                all.push(BattleCommand::AttackWall { unit, cell: (x, y) });
                all.push(BattleCommand::ShootWall { unit, cell: (x, y) });
            }
        }
    }
    all
}

/// The list of legal commands is exactly what `apply` accepts.
pub fn check_legal_list(b: &Battle) {
    let legal = b.legal_commands();
    for c in every_command(b) {
        let accepted = b.clone().apply(c.clone()).is_ok();
        assert_eq!(
            accepted,
            legal.contains(&c),
            "{c:?}: apply says {accepted}, the list says otherwise"
        );
    }
}

/// The unit `number` of the content in `b`.
pub fn unit_of(b: &Battle, number: u16) -> BattleUnit {
    let kind = content().rules.units.id_numbered(number);
    b.units
        .iter()
        .find(|u| u.unit_id == kind)
        .expect("the battle has the unit")
        .clone()
}
