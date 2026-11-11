//! The scripted wizard in a battle (`docs/ai-design.md` section 6.5): once a turn it casts the
//! spell that is worth the most for its mana, if any is worth casting at all; otherwise the
//! units act (`ai::next_command`), or the side runs when the fight is lost (`ai::retreat`).
//!
//! A cast is scored from the spell's data, not by name: damage and slaying by how much of an
//! enemy unit they take, healing by how much of a hurt unit they restore, a summon by the
//! strength of what appears, a blessing or a curse by a share of the strength of the unit it
//! falls on, Call Chaos by an average hit on every enemy unit, a Magic Vortex by the enemy and
//! friendly units near the square it is raised on. Spells the AI cannot judge (fields, dispels,
//! counter magic) are left alone. The mana kept back pays the overland upkeep, so a battle does not starve the spells and
//! creatures the wizard keeps going.
//!
//! A vortex of the side that has not moved yet is steered one square toward the enemy
//! ([`steer_choice`]), as a human may (the original's computer never does).
//!
//! Tests: in battle/tests.rs (`the_scripted_wizard_...` for the wizard, `the_legal_unit_casts_...` for the units).

use super::order::{self as ai, worth};
use f4x_core::battle::casting::SpellAim;
use f4x_core::battle::map::{self, Pos};
use f4x_core::battle::spell::BattleEffect;
use f4x_core::battle::{Battle, BattleCommand, BattleUnit, RangedKind, Side};
use f4x_core::game::Game;
use f4x_core::rules::effects::{Effect, EffectRule, TargetSel};
use f4x_core::rules::{ModOp, SpellDef, SpellId, Stat};

/// A cast must be worth at least this share of the dearest enemy unit to be made.
const WORTH_FLOOR: f32 = 0.12;
/// Hit points an average outcome of Call Chaos takes from an enemy unit (eight outcomes: nothing,
/// five points of healing, two changes, a Fire Bolt, a Warp Lightning, a Doom Bolt, Disintegrate).
const CHAOS_DAMAGE: f32 = 6.0;
/// Squares from a raised vortex within which a unit is likely to be struck as it wanders.
const VORTEX_REACH: u32 = 2;
/// A struck unit's worth lost to one vortex, as a share of its worth. A vortex hits at random.
const VORTEX_SHARE: f32 = 0.1;
/// The odds at which a damage spell counts as one that kills.
/// A lasting spell on a unit is worth this share of the unit's worth.
const LASTING_SHARE: f32 = 0.25;
/// Warp Wood takes a missile unit's arrows: a share of its worth.
const WARP_WOOD_SHARE: f32 = 0.5;
/// The odds at which a damage spell counts as one that kills.
const KILL_ODDS: f32 = 0.6;
/// A kill is worth more than the damage it does: the unit is gone, with what it would still do.
const KILL_BONUS: f32 = 1.5;
/// A harmful cast must take this share of the worth of the dearest enemy to be better than
/// strengthening our own units.
const WOUND_ENOUGH: f32 = 0.3;
/// Mana added to a cost when weighing value against price, so cheap spells do not always win.
const MANA_SOFTENING: f32 = 12.0;

/// The next command for the side to play: a cast if one is worth it, else a unit order.
pub fn next_command(game: &Game) -> Option<BattleCommand> {
    let battle = game.battle()?;
    Some(
        cast_choice(game, battle)
            .or_else(|| unit_cast_choice(game, battle))
            .or_else(|| ai::retreat(battle))
            .or_else(|| steer_choice(battle))
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
    let scene = Scene {
        game,
        side,
        friends: &friends,
        enemies: &enemies,
    };
    let dearest = enemies.iter().map(|e| worth(e)).fold(0.0, f32::max);
    let mut best: Option<(Tier, f32, BattleCommand)> = None;
    for (unit, cast) in game.legal_unit_casts() {
        let free = game
            .battle_unit_spells(unit)
            .iter()
            .any(|s| s.spell == cast.spell && s.cost == 0);
        let mana = if free { 0 } else { cast.least_mana };
        let def = game.content().rules.spells.get(cast.spell);
        for &aim in &cast.aims {
            let judged = scene.judge(cast.spell, def, aim, cast.least_mana, cast.least_mana);
            if judged.value < scene.floor_for(&judged, floor) {
                continue;
            }
            let tier = judged.tier(dearest);
            let score = judged.value / (mana as f32 + MANA_SOFTENING);
            if best
                .as_ref()
                .is_none_or(|(t, s, _)| (tier, score) > (*t, *s))
            {
                best = Some((
                    tier,
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
    best.map(|(_, _, c)| c)
}

/// The cast the wizard of the side to play should make now, if there is one.
pub fn cast_choice(game: &Game, battle: &Battle) -> Option<BattleCommand> {
    let side = battle.active;
    let player = battle.player_of(side);
    let reserve = game.player(player).map_or(0, |p| p.treasury().mana.floor());
    let upkeep = game.mana_upkeep(player).ceil();
    let enemies: Vec<&BattleUnit> = battle
        .active_units(side.other())
        .filter(|e| battle.seen_by(side, e.id))
        .collect();
    let friends: Vec<&BattleUnit> = battle.active_units(side).collect();
    let floor = enemies.iter().map(|e| worth(e)).fold(0.0, f32::max) * WORTH_FLOOR;
    let scene = Scene {
        game,
        side,
        friends: &friends,
        enemies: &enemies,
    };
    let dearest = enemies.iter().map(|e| worth(e)).fold(0.0, f32::max);
    let mut best: Option<(Tier, f32, BattleCommand)> = None;
    for cast in game.legal_battle_casts() {
        let def = game.content().rules.spells.get(cast.spell);
        for &aim in &cast.aims {
            // Mana beyond the cost only from what the upkeep leaves in the reserve.
            let spare = u32::try_from(reserve - upkeep).unwrap_or(0);
            let most = cast.most_mana.min(spare).max(cast.least_mana);
            let judged = scene.judge(cast.spell, def, aim, cast.least_mana, most);
            let mana = judged.mana;
            if (reserve - mana as i32) < upkeep || judged.value < scene.floor_for(&judged, floor) {
                continue;
            }
            let tier = judged.tier(dearest);
            let score = judged.value / (mana as f32 + MANA_SOFTENING);
            if best
                .as_ref()
                .is_none_or(|(t, s, _)| (tier, score) > (*t, *s))
            {
                best = Some((
                    tier,
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
    best.map(|(_, _, c)| c)
}

/// The fight a cast is judged in.
struct Scene<'a> {
    game: &'a Game,
    side: Side,
    friends: &'a [&'a BattleUnit],
    enemies: &'a [&'a BattleUnit],
}

/// How a cast ranks against the others: the decision tree. A cast that kills comes first, then
/// one that hurts the enemy enough to matter; failing both, the wizard strengthens or mends its
/// own units (the fallback); a blow that barely scratches comes last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Scratch,
    Support,
    Wound,
    Kill,
}

/// What one cast is worth, and of that how much harms the enemy.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Judged {
    /// In the units of [`worth`].
    value: f32,
    /// The part of the value that is harm to the enemy.
    harm: f32,
    /// A damage or slaying spell that is likely to destroy the unit it is aimed at.
    kills: bool,
    /// The mana to put in (more than the cost when that makes a kill).
    mana: u32,
}

impl Judged {
    fn tier(&self, dearest_enemy: f32) -> Tier {
        if self.kills {
            Tier::Kill
        } else if self.harm >= WOUND_ENOUGH * dearest_enemy {
            Tier::Wound
        } else if self.value > self.harm {
            Tier::Support
        } else {
            Tier::Scratch
        }
    }
}

impl Scene<'_> {
    /// The least a cast must be worth: for a blow, a share of the dearest enemy; for a gift to
    /// our own units (the fallback when no blow is worth it), a share of our dearest unit.
    fn floor_for(&self, judged: &Judged, blow_floor: f32) -> f32 {
        if judged.harm * 2.0 >= judged.value {
            blow_floor
        } else {
            WORTH_FLOOR * self.friends.iter().map(|u| worth(u)).fold(0.0, f32::max)
        }
    }

    fn unit(&self, id: usize) -> Option<&BattleUnit> {
        self.friends
            .iter()
            .chain(self.enemies.iter())
            .find(|u| u.id == id)
            .copied()
    }

    /// What a damage spell does to one enemy: the value, and whether it kills. With mana to
    /// spare the spell may be fed until it kills.
    fn damage_value(
        &self,
        spell: SpellId,
        target: &BattleUnit,
        least: u32,
        most: u32,
    ) -> Option<(f32, bool, u32)> {
        let odds = self.game.preview_spell_damage(spell, target.id, 0)?;
        let left = target.hit_points().max(1) as f32;
        let hurt = |mean: f32| (mean.min(left) / left) * worth(target);
        if odds.kill_chance >= KILL_ODDS {
            return Some((KILL_BONUS * worth(target), true, least));
        }
        // Not enough: more mana, if the spell takes it, until it is.
        let mut extra = 1;
        while least + extra <= most {
            if let Some(more) = self.game.preview_spell_damage(spell, target.id, extra) {
                if more.kill_chance >= KILL_ODDS {
                    return Some((KILL_BONUS * worth(target), true, least + extra));
                }
            }
            extra += 1;
        }
        Some((hurt(odds.mean), false, least))
    }

    fn judge(
        &self,
        spell: SpellId,
        def: &SpellDef,
        aim: SpellAim,
        least: u32,
        most: u32,
    ) -> Judged {
        let aimed = match aim {
            SpellAim::Unit(id) => self.unit(id),
            _ => None,
        };
        let own = BattleEffect::of(def);
        let mut out = Judged {
            value: 0.0,
            harm: 0.0,
            kills: false,
            mana: least,
        };
        for rule in &def.effects {
            let (value, harm) = match &rule.effect {
                Effect::Damage { .. } => {
                    let targets: Vec<&BattleUnit> = match rule.target {
                        TargetSel::AllEnemyUnits => self.enemies.to_vec(),
                        _ => aimed.into_iter().filter(|u| u.side != self.side).collect(),
                    };
                    let mut sum = 0.0;
                    for t in targets {
                        if let Some((v, kills, mana)) = self.damage_value(spell, t, least, most) {
                            sum += v;
                            if kills {
                                out.kills = true;
                                out.mana = out.mana.max(mana);
                            }
                        }
                    }
                    (sum, sum)
                }
                Effect::Custom { .. } if own == Some(BattleEffect::CallChaos) => {
                    let v = self
                        .enemies
                        .iter()
                        .map(|u| {
                            (CHAOS_DAMAGE.min(u.hit_points().max(1) as f32)
                                / u.hit_points().max(1) as f32)
                                * worth(u)
                        })
                        .sum();
                    (v, v)
                }
                Effect::Custom { .. } if own == Some(BattleEffect::Vortex) => match aim {
                    SpellAim::Tile(cell) => {
                        let v = vortex_reach_value(cell, self.friends, self.enemies);
                        (v, v.max(0.0))
                    }
                    _ => (0.0, 0.0),
                },
                // Warp Wood: only worth anything against a missile unit that still has arrows;
                // it takes away what the unit shoots with, a share of its worth.
                Effect::Custom { .. } if own == Some(BattleEffect::WarpWood) => {
                    let v = aimed
                        .filter(|u| {
                            u.side != self.side
                                && u.ranged_kind == RangedKind::Missile
                                && u.ammo > 0
                        })
                        .map_or(0.0, |u| WARP_WOOD_SHARE * worth(u));
                    (v, v)
                }
                // A lasting spell on a unit (a blessing, a web, a weakness): a share of the unit's
                // worth, a gain on one of ours and harm on an enemy. The game refuses the spells
                // the unit already carries or cannot take.
                Effect::Custom { name, .. } if name == "unit_effect" => {
                    let v = aimed.map_or(0.0, |u| LASTING_SHARE * worth(u));
                    (
                        v,
                        if aimed.is_some_and(|u| u.side != self.side) {
                            v
                        } else {
                            0.0
                        },
                    )
                }
                _ => {
                    let v = rule_value(
                        self.game,
                        rule,
                        aimed,
                        self.side,
                        self.friends,
                        self.enemies,
                    );
                    let on_enemy = matches!(rule.target, TargetSel::AllEnemyUnits)
                        || aimed.is_some_and(|u| u.side != self.side);
                    (v, if on_enemy { v } else { 0.0 })
                }
            };
            out.value += value;
            out.harm += harm;
        }
        out
    }
}

/// What a vortex raised on `cell` is worth: a share of the worth of each enemy unit within reach,
/// less the same for ours (it strikes whoever it comes to).
fn vortex_reach_value(cell: Pos, friends: &[&BattleUnit], enemies: &[&BattleUnit]) -> f32 {
    let sum = |units: &[&BattleUnit]| -> f32 {
        units
            .iter()
            .filter(|u| map::distance(u.cell, cell) <= VORTEX_REACH)
            .map(|u| worth(u))
            .sum()
    };
    VORTEX_SHARE * (sum(enemies) - sum(friends))
}

/// The move of a vortex of the side to play that costs the enemy most, if any move is worth
/// making: one square, onto a free square, scored as a vortex raised there is.
pub fn steer_choice(battle: &Battle) -> Option<BattleCommand> {
    let side = battle.active;
    let enemies: Vec<&BattleUnit> = battle
        .active_units(side.other())
        .filter(|e| battle.seen_by(side, e.id))
        .collect();
    let friends: Vec<&BattleUnit> = battle.active_units(side).collect();
    let mut best: Option<(f32, BattleCommand)> = None;
    for (index, v) in battle.vortices.iter().enumerate() {
        if v.owner != side || v.steered {
            continue;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                let to = (v.cell.0 + dx, v.cell.1 + dy);
                if map::distance(v.cell, to) != 1
                    || !battle.map.is_playable(to)
                    || battle.vortex_at(to).is_some()
                {
                    continue;
                }
                // Landing on a unit strikes it for certain: weigh that square's unit again.
                let on_square = battle.unit_at(to).map_or(0.0, |u| {
                    let sign = if u.side == side { -1.0 } else { 1.0 };
                    sign * worth(u)
                });
                let value = vortex_reach_value(to, &friends, &enemies) + 0.3 * on_square;
                if value > 0.0 && best.as_ref().is_none_or(|(s, _)| value > *s) {
                    best = Some((value, BattleCommand::MoveVortex { index, to }));
                }
            }
        }
    }
    best.map(|(_, c)| c)
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
        // A rule without a target falls on the unit the spell is aimed at (a web, a blessing).
        TargetSel::Chosen | TargetSel::This => aimed.into_iter().collect(),
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
        Effect::Slay { immune, .. } => on
            .iter()
            .filter(|u| !ours(u) && !immune.iter().any(|&a| u.has(a)))
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
        Effect::Summon {
            unit: id, count, ..
        } => {
            let d = game.content().rules.units.get(*id);
            let stat = |s: Stat| d.stats.get(&s).map_or(0, |v| v.floor()) as f32;
            let attack = stat(Stat::Melee).max(stat(Stat::Ranged));
            let hp = stat(Stat::MaxHp).max(1.0) * stat(Stat::Figures).max(1.0);
            // Fresh units fight at once but are an extra target: three quarters of their worth.
            0.75 * (attack * 1.5 + stat(Stat::Defense) + 1.0) * hp * f32::from(*count)
        }
        Effect::Modify { op, value, .. } => {
            // A blessing on the strongest of ours is worth a share of its strength; a curse on
            // an enemy likewise. The sign of the change says which it is; a stat set to nothing
            // (a web holds a unit still) is a curse.
            let zero = f4x_def::half_int::HalfInt::ZERO;
            let good = *value >= zero && !(*op == ModOp::Set && *value == zero);
            on.iter()
                .filter(|u| ours(u) == good)
                .map(|u| 0.25 * worth(u))
                .sum()
        }
        // A gift of an ability the unit already has is worth nothing.
        Effect::GrantAbility(a) => on
            .iter()
            .filter(|u| ours(u) && !u.has(*a))
            .map(|u| 0.25 * worth(u))
            .sum(),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, shipped, COAST_CITY, P0, P1};
    use f4x_core::battle::{BattleCommand, Side};
    use f4x_core::game::Command;
    use f4x_def::testdir::TestResult;

    /// A battle of player 0's cavalry against `foes` more, in which player 0's wizard knows
    /// `spell`, has `mana` in the battle and `reserve` overland.
    fn spell_duel(
        spell: u16,
        foes: usize,
        mana: i32,
        reserve: i32,
    ) -> Result<Game, Box<dyn std::error::Error>> {
        spell_duel_hurt(spell, foes, mana, reserve, 0)
    }

    /// [`spell_duel`] with our cavalry wounded by `hurt` hit points before the battle.
    fn spell_duel_hurt(
        spell: u16,
        foes: usize,
        mana: i32,
        reserve: i32,
        hurt: u32,
    ) -> Result<Game, Box<dyn std::error::Error>> {
        let mut g = game(COAST_CITY, 7);
        let id = shipped()
            .rules
            .spells
            .id_numbered(spell)
            .ok_or("the spell is not in the content")?;
        g.cheat_learn_spell(P0, id);
        g.wizard_mut(P0)
            .ok_or("no wizard")?
            .get_mut::<f4x_core::wizard::sheet::Skill>()
            .left = 500;
        g.treasury_mut(P0).ok_or("no treasury")?.mana =
            f4x_def::half_int::HalfInt::from_int(reserve);
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let ours = g.spawn_unit(P0, at(4, 4), walker);
        g.scenario_set_hurt(ours, hurt);
        let theirs: Vec<_> = (0..foes)
            .map(|_| g.spawn_unit(P1, at(5, 4), walker))
            .collect();
        g.apply(Command::BeginBattle {
            attackers: vec![ours],
            defenders: theirs,
        });
        g.scenario_battle_caster(Side::Attacker, mana);
        Ok(g)
    }

    #[test]
    fn the_scripted_wizard_casts_a_damage_spell_at_an_enemy_when_it_can_pay() -> TestResult {
        let g = spell_duel(83 /* fire_bolt */, 2, 50, 50)?;
        let bolt = shipped()
            .rules
            .spells
            .id_numbered(83)
            .ok_or("no fire bolt")?;
        let Some(BattleCommand::Cast { spell, aim, .. }) = next_command(&g) else {
            return Err("the wizard did not cast".into());
        };
        assert_eq!(spell, bolt);
        let SpellAim::Unit(target) = aim else {
            return Err(format!("a bolt is aimed at a unit, not {aim:?}").into());
        };
        let battle = g.battle().ok_or("battle is missing")?;
        assert_eq!(
            battle.unit(target).ok_or("no such unit")?.side,
            Side::Defender
        );
        Ok(())
    }

    #[test]
    fn the_scripted_wizard_casts_call_chaos_and_a_magic_vortex_when_enemies_are_there() -> TestResult
    {
        for name in [115 /* call_chaos */, 105 /* magic_vortex */] {
            let g = spell_duel(name, 3, 200, 200)?;
            let id = shipped()
                .rules
                .spells
                .id_numbered(name)
                .ok_or("no such spell")?;
            let Some(BattleCommand::Cast { spell, aim, .. }) = next_command(&g) else {
                return Err(format!("the wizard did not cast {name}").into());
            };
            assert_eq!(spell, id, "{name}");
            if name == 105 {
                let SpellAim::Tile(cell) = aim else {
                    return Err(format!("a vortex is raised on a square, not {aim:?}").into());
                };
                let battle = g.battle().ok_or("battle is missing")?;
                assert!(
                    battle
                        .units
                        .iter()
                        .any(|u| u.side == Side::Defender && map::distance(u.cell, cell) <= 2),
                    "the vortex is raised among the enemy, not at {cell:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn the_scripted_side_steers_its_vortex_onto_an_enemy_once_and_leaves_the_enemys_alone() {
        use f4x_core::battle::map::BattleMap;
        let units = vec![
            BattleUnit::simple(Side::Attacker, 3, 2, 3, 2).at((3, 10)),
            BattleUnit::simple(Side::Defender, 3, 2, 3, 2).at((10, 10)),
        ];
        let mut b = Battle::new(P0, P1, BattleMap::open(), units, 5);
        b.make_vortex(Side::Attacker, (9, 10));
        assert_eq!(
            steer_choice(&b),
            Some(BattleCommand::MoveVortex {
                index: 0,
                to: (10, 10)
            })
        );
        b.vortices[0].steered = true;
        assert_eq!(steer_choice(&b), None, "once a turn");
        b.vortices[0].steered = false;
        b.vortices[0].owner = Side::Defender;
        assert_eq!(
            steer_choice(&b),
            None,
            "the other side's vortex is not ours to move"
        );
    }

    #[test]
    fn the_scripted_wizard_with_no_mana_to_spare_moves_its_units_instead() -> TestResult {
        let g = spell_duel(83 /* fire_bolt */, 2, 0, 50)?;
        assert!(!matches!(
            next_command(&g),
            Some(BattleCommand::Cast { .. })
        ));
        // Mana in the battle, but an empty reserve overland: the upkeep comes first.
        let mut g = spell_duel(83 /* fire_bolt */, 2, 50, 0)?;
        g.treasury_mut(P0).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(-5);
        assert!(!matches!(
            next_command(&g),
            Some(BattleCommand::Cast { .. })
        ));
        Ok(())
    }

    #[test]
    fn the_scripted_wizard_heals_a_hurt_unit_but_not_a_whole_one() -> TestResult {
        let g = spell_duel(125 /* healing */, 1, 50, 50)?;
        assert!(
            !matches!(next_command(&g), Some(BattleCommand::Cast { .. })),
            "nothing is hurt"
        );
        let g = spell_duel_hurt(125 /* healing */, 1, 50, 50, 5)?;
        let Some(BattleCommand::Cast { aim, .. }) = next_command(&g) else {
            return Err("the wizard did not heal".into());
        };
        let ours = g
            .battle()
            .ok_or("no battle")?
            .units
            .iter()
            .find(|u| u.side == Side::Attacker)
            .ok_or("no attacker")?
            .id;
        assert_eq!(aim, SpellAim::Unit(ours));
        Ok(())
    }

    /// A fight of player 0's wizard (who knows `spells`, with `mana` in the reserve) against a
    /// stack of walkers, left to the battle AI; returns the mana spent.
    fn strategic_fight(
        spells: &[u16],
        mana: i32,
        budget: Option<u32>,
    ) -> Result<i32, Box<dyn std::error::Error>> {
        crate::battle::install();
        let mut g = game(COAST_CITY, 7);
        g.init_wizard(
            P0,
            &[
                (f4x_core::rules::RealmId::new(0 /* nature */), 5),
                (f4x_core::rules::RealmId::new(3 /* life */), 5),
                (f4x_core::rules::RealmId::new(4 /* death */), 5),
            ],
            &[],
            None,
        )
        .map_err(|e| format!("{e:?}"))?;
        for name in spells {
            let id = shipped()
                .rules
                .spells
                .id_numbered(*name)
                .ok_or("no such spell")?;
            g.cheat_learn_spell(P0, id);
        }
        g.treasury_mut(P0).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(mana);
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let ours: Vec<_> = (0..3).map(|_| g.spawn_unit(P0, at(4, 4), walker)).collect();
        let theirs: Vec<_> = (0..6).map(|_| g.spawn_unit(P1, at(5, 4), walker)).collect();
        g.apply(Command::AutoResolve {
            attackers: ours,
            defenders: theirs,
            mana_budget: budget.map(|b| (P0, b)),
        });
        assert!(!g.in_battle());
        Ok(mana - g.treasury_mut(P0).ok_or("no treasury")?.mana.floor())
    }

    #[test]
    fn the_battle_ai_spends_mana_in_a_strategic_fight_and_a_limit_stops_it() -> TestResult {
        let spells = [
            4,   /* giant_strength */
            7,   /* stone_skin */
            121, /* bless */
        ];
        let free = strategic_fight(&spells, 500, None)?;
        assert!(free > 3, "the AI casts when it may: spent {free}");
        let limited = strategic_fight(&spells, 500, Some(3))?;
        assert!(limited <= 3, "limited to 3, spent {limited}");
        assert_eq!(strategic_fight(&spells, 500, Some(0))?, 0);
        Ok(())
    }

    #[test]
    fn a_unit_that_casts_by_its_ability_makes_the_cast_at_the_enemy() -> TestResult {
        let rules = &shipped().rules;
        let ability = rules.abilities.id_numbered(5).ok_or("no casts_fireball")?;
        let fireball = rules.spells.id_numbered(96).ok_or("no fireball")?;
        let caster = rules
            .units
            .defs()
            .iter()
            .find(|d| d.abilities.contains(&ability))
            .ok_or("no unit casts fireball")?;
        let mut g = game(COAST_CITY, 7);
        let walker = rules.units.id_numbered(107).ok_or("no cavalry")?;
        let ours = g.spawn_unit(P0, at(4, 4), f4x_core::rules::UnitId::new(caster.num));
        let theirs = g.spawn_unit(P1, at(5, 4), walker);
        g.apply(Command::BeginBattle {
            attackers: vec![ours],
            defenders: vec![theirs],
        });
        let chosen = unit_cast_choice(&g, g.battle().ok_or("no battle")?);
        assert!(
            matches!(chosen, Some(BattleCommand::UnitCast { spell, aim: SpellAim::Unit(1), .. }) if spell == fireball),
            "{chosen:?}"
        );
        Ok(())
    }

    /// Player 0's wizard (knowing `spells`, with 100 mana in the battle) against one `foe` of
    /// player 1's, a single walker of ours at its side.
    fn duel_with(
        spells: &[u16],
        foe: f4x_core::rules::UnitId,
    ) -> Result<Game, Box<dyn std::error::Error>> {
        let mut g = game(COAST_CITY, 7);
        for &n in spells {
            let id = shipped()
                .rules
                .spells
                .id_numbered(n)
                .ok_or("no such spell")?;
            g.cheat_learn_spell(P0, id);
        }
        g.wizard_mut(P0)
            .ok_or("no wizard")?
            .get_mut::<f4x_core::wizard::sheet::Skill>()
            .left = 500;
        g.treasury_mut(P0).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(200);
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let ours = g.spawn_unit(P0, at(4, 4), walker);
        let theirs = g.spawn_unit(P1, at(5, 4), foe);
        g.apply(Command::BeginBattle {
            attackers: vec![ours],
            defenders: vec![theirs],
        });
        g.scenario_battle_caster(Side::Attacker, 100);
        Ok(g)
    }

    fn unit_with_ability(num: u16) -> Result<f4x_core::rules::UnitId, Box<dyn std::error::Error>> {
        let rules = &shipped().rules;
        let ability = rules.abilities.id_numbered(num).ok_or("no such ability")?;
        let def = rules
            .units
            .defs()
            .iter()
            .find(|d| d.abilities.contains(&ability))
            .ok_or("no unit has the ability")?;
        Ok(f4x_core::rules::UnitId::new(def.num))
    }

    #[test]
    fn a_fire_bolt_is_not_cast_at_a_unit_that_is_immune_to_fire_but_a_blessing_is() -> TestResult {
        let foe = unit_with_ability(21 /* fire_immunity */)?;
        let g = duel_with(&[83 /* fire_bolt */], foe)?;
        assert!(
            !matches!(next_command(&g), Some(BattleCommand::Cast { .. })),
            "nothing worth casting: {:?}",
            next_command(&g)
        );
        // With a blessing known, the wizard falls back on strengthening its own unit.
        let g = duel_with(&[83 /* fire_bolt */, 121 /* bless */], foe)?;
        let bless = shipped().rules.spells.id_numbered(121).ok_or("no bless")?;
        let Some(BattleCommand::Cast { spell, aim, .. }) = next_command(&g) else {
            return Err("the wizard did not fall back on a blessing".into());
        };
        assert_eq!(spell, bless);
        let SpellAim::Unit(id) = aim else {
            return Err("a blessing is aimed at a unit".into());
        };
        assert_eq!(
            g.battle()
                .ok_or("no battle")?
                .unit(id)
                .ok_or("no unit")?
                .side,
            Side::Attacker
        );
        Ok(())
    }

    #[test]
    fn a_damage_spell_that_cannot_kill_is_fed_mana_until_it_does() -> TestResult {
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let g = duel_with(&[83 /* fire_bolt */], walker)?;
        let battle = g.battle().ok_or("no battle")?;
        let enemy: Vec<&BattleUnit> = battle.active_units(Side::Defender).collect();
        let friends: Vec<&BattleUnit> = battle.active_units(Side::Attacker).collect();
        let scene = Scene {
            game: &g,
            side: Side::Attacker,
            friends: &friends,
            enemies: &enemy,
        };
        let bolt = shipped().rules.spells.id_numbered(83).ok_or("no bolt")?;
        let def = g.content().rules.spells.get(bolt);
        let aim = SpellAim::Unit(enemy[0].id);
        let cheap = scene.judge(bolt, def, aim, 5, 5);
        let fed = scene.judge(bolt, def, aim, 5, 100);
        assert!(
            !cheap.kills,
            "five points do not kill a horseman: {cheap:?}"
        );
        assert!(fed.kills && fed.mana > 5, "fed with mana it kills: {fed:?}");
        assert!(fed.value > cheap.value);
        Ok(())
    }

    #[test]
    fn warp_wood_is_worth_something_only_against_archers_with_arrows() -> TestResult {
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let g = duel_with(&[81 /* warp_wood */], walker)?;
        let wood = shipped()
            .rules
            .spells
            .id_numbered(81)
            .ok_or("no warp wood")?;
        let def = g.content().rules.spells.get(wood);
        let mut archer = BattleUnit::simple(Side::Defender, 6, 1, 1, 1);
        archer.id = 7;
        archer.ranged_kind = RangedKind::Missile;
        archer.ammo = 4;
        let mut spent = archer.clone();
        spent.ammo = 0;
        let mut plain = archer.clone();
        plain.ranged_kind = RangedKind::None;
        let value = |u: &BattleUnit| {
            let enemies = [u];
            let scene = Scene {
                game: &g,
                side: Side::Attacker,
                friends: &[],
                enemies: &enemies,
            };
            scene.judge(wood, def, SpellAim::Unit(7), 10, 10).value
        };
        assert!(value(&archer) > 0.0);
        assert_eq!(value(&spent), 0.0, "no arrows left to destroy");
        assert_eq!(value(&plain), 0.0, "no bow to warp");
        Ok(())
    }

    #[test]
    fn a_kill_outranks_a_wound_which_outranks_a_blessing_which_outranks_a_scratch() {
        let j = |value, harm, kills| Judged {
            value,
            harm,
            kills,
            mana: 5,
        };
        assert!(j(10.0, 10.0, true).tier(100.0) > j(50.0, 50.0, false).tier(100.0));
        assert!(j(50.0, 50.0, false).tier(100.0) > j(20.0, 0.0, false).tier(100.0));
        assert!(j(20.0, 0.0, false).tier(100.0) > j(5.0, 5.0, false).tier(100.0));
    }
}
