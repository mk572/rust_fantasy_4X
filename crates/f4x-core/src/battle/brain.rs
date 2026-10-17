//! The scripted wizard in a battle (`docs/ai-design.md` section 6.5): once a turn it casts the
//! spell that is worth the most for its mana, if any is worth casting at all; otherwise the
//! units act (`ai::next_command`).
//!
//! A cast is scored from the spell's data, not by name: damage and slaying by how much of an
//! enemy unit they take, healing by how much of a hurt unit they restore, a summon by the
//! strength of what appears, a blessing or a curse by a share of the strength of the unit it
//! falls on. Spells the AI cannot judge (fields, dispels, counter magic, Call Chaos) are left
//! alone. The mana kept back pays the overland upkeep, so a battle does not starve the spells and
//! creatures the wizard keeps going.

use super::ai::{self, worth};
use super::casting::SpellAim;
use super::{Battle, BattleCommand, BattleUnit, Side};
use crate::rules::effects::{Effect, EffectRule, TargetSel};
use crate::rules::{SpellDef, Stat};
use crate::Game;

/// A cast must be worth at least this share of the dearest enemy unit to be made.
const WORTH_FLOOR: f32 = 0.12;
/// Mana added to a cost when weighing value against price, so cheap spells do not always win.
const MANA_SOFTENING: f32 = 12.0;

/// The next command for the side to play: a cast if one is worth it, else a unit order.
pub fn next_command(game: &Game) -> Option<BattleCommand> {
    let battle = game.battle()?;
    Some(
        cast_choice(game, battle)
            .or_else(|| unit_cast_choice(game, battle))
            .unwrap_or_else(|| ai::next_command(battle)),
    )
}

/// The cast a unit of the side to play should make now, if one is worth it (a hero's spell, an
/// item's charge, a creature caster, an ability): scored as the wizard's casts are, against
/// the unit's own mana. A free cast (a charge, an ability) costs nothing.
pub fn unit_cast_choice(game: &Game, battle: &Battle) -> Option<BattleCommand> {
    let side = battle.active;
    let enemies: Vec<&BattleUnit> = battle
        .active_units(side.other())
        .filter(|e| battle.seen_by(side, e.id))
        .collect();
    let friends: Vec<&BattleUnit> = battle.active_units(side).collect();
    let floor = enemies.iter().map(|e| worth(e)).fold(0.0, f32::max) * WORTH_FLOOR;
    // A demon is worth calling whenever the unit can: it is free and strong.
    if let Some(unit) = friends
        .iter()
        .map(|u| u.id)
        .find(|&u| game.can_summon_demon(u))
    {
        return Some(BattleCommand::SummonDemon { unit });
    }
    let mut best: Option<(f32, BattleCommand)> = None;
    for (unit, cast) in game.legal_unit_casts() {
        let free = game
            .battle_unit_spells(unit)
            .iter()
            .any(|s| s.spell == cast.spell && s.cost == 0);
        let mana = if free { 0 } else { cast.least_mana };
        let def = game.content().rules.spells.get(cast.spell);
        for &aim in &cast.aims {
            let value = cast_value(game, def, aim, side, &friends, &enemies);
            if value < floor {
                continue;
            }
            let score = value / (mana as f32 + MANA_SOFTENING);
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((
                    score,
                    BattleCommand::UnitCast {
                        unit,
                        spell: cast.spell,
                        aim,
                        mana: cast.least_mana,
                    },
                ));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// The cast the wizard of the side to play should make now, if there is one.
pub fn cast_choice(game: &Game, battle: &Battle) -> Option<BattleCommand> {
    let side = battle.active;
    let player = battle.player_of(side);
    let reserve = game.player(player).map_or(0, |p| p.mana);
    let upkeep = game.income(player).mana_upkeep;
    let enemies: Vec<&BattleUnit> = battle
        .active_units(side.other())
        .filter(|e| battle.seen_by(side, e.id))
        .collect();
    let friends: Vec<&BattleUnit> = battle.active_units(side).collect();
    let floor = enemies.iter().map(|e| worth(e)).fold(0.0, f32::max) * WORTH_FLOOR;
    let mut best: Option<(f32, BattleCommand)> = None;
    for cast in game.legal_battle_casts() {
        let mana = cast.least_mana;
        if (reserve - mana as i32) < upkeep {
            continue;
        }
        let def = game.content().rules.spells.get(cast.spell);
        for &aim in &cast.aims {
            let value = cast_value(game, def, aim, side, &friends, &enemies);
            if value < floor {
                continue;
            }
            let score = value / (mana as f32 + MANA_SOFTENING);
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((
                    score,
                    BattleCommand::Cast {
                        spell: cast.spell,
                        aim,
                        mana,
                    },
                ));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// What one cast is worth, in the units of [`worth`].
fn cast_value(
    game: &Game,
    def: &SpellDef,
    aim: SpellAim,
    side: Side,
    friends: &[&BattleUnit],
    enemies: &[&BattleUnit],
) -> f32 {
    let unit_of = |id: usize| {
        friends
            .iter()
            .chain(enemies.iter())
            .find(|u| u.id == id)
            .copied()
    };
    let aimed = match aim {
        SpellAim::Unit(id) => unit_of(id),
        _ => None,
    };
    let mut total = 0.0;
    for rule in &def.effects {
        total += rule_value(game, rule, aimed, side, friends, enemies);
    }
    total
}

/// The units a rule falls on, given the spell's aim.
fn victims<'a>(
    rule: &EffectRule,
    aimed: Option<&'a BattleUnit>,
    friends: &[&'a BattleUnit],
    enemies: &[&'a BattleUnit],
) -> Vec<&'a BattleUnit> {
    match rule.target {
        TargetSel::AllEnemyUnits => enemies.to_vec(),
        TargetSel::AllOwnUnits => friends.to_vec(),
        TargetSel::Chosen => aimed.into_iter().collect(),
        _ => Vec::new(),
    }
}

fn rule_value(
    game: &Game,
    rule: &EffectRule,
    aimed: Option<&BattleUnit>,
    side: Side,
    friends: &[&BattleUnit],
    enemies: &[&BattleUnit],
) -> f32 {
    let on = victims(rule, aimed, friends, enemies);
    let ours = |u: &BattleUnit| u.side == side;
    match &rule.effect {
        Effect::Damage {
            amount, resistible, ..
        } => {
            let dmg = if *resistible {
                *amount as f32 * 0.7
            } else {
                *amount as f32
            };
            on.iter()
                .filter(|u| !ours(u))
                .map(|u| {
                    let left = u.hit_points().max(1) as f32;
                    (dmg.min(left) / left) * worth(u)
                })
                .sum()
        }
        Effect::Slay { .. } => on
            .iter()
            .filter(|u| !ours(u))
            .map(|u| 0.35 * worth(u))
            .sum(),
        Effect::Heal(points) => on
            .iter()
            .filter(|u| ours(u))
            .map(|u| {
                let missing = (u.max_hit_points() - u.hit_points()).max(0) as f32;
                (missing.min(*points as f32) / u.max_hit_points().max(1) as f32) * worth(u)
            })
            .sum(),
        Effect::Summon { unit, count } => {
            let rules = &game.content().rules;
            let Some(id) = rules.units.id_of(unit) else {
                return 0.0;
            };
            let d = rules.units.get(id);
            let stat = |s: Stat| d.stats.get(&s).copied().unwrap_or(0) as f32;
            let attack = stat(Stat::Melee).max(stat(Stat::Ranged));
            let hp = stat(Stat::MaxHp).max(1.0) * stat(Stat::Figures).max(1.0);
            // Fresh units fight at once but are an extra target: three quarters of their worth.
            0.75 * (attack * 1.5 + stat(Stat::Defense) + 1.0) * hp * f32::from(*count)
        }
        Effect::Modify { value, .. } => {
            // A blessing on the strongest of ours is worth a share of its strength; a curse on
            // an enemy likewise. The sign of the change says which it is.
            let good = *value >= 0;
            on.iter()
                .filter(|u| ours(u) == good)
                .map(|u| 0.25 * worth(u))
                .sum()
        }
        Effect::GrantAbility(_) => on.iter().filter(|u| ours(u)).map(|u| 0.25 * worth(u)).sum(),
        _ => 0.0,
    }
}
