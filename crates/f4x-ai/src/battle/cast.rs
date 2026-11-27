//! The scripted wizard in a battle (`docs/ai-design.md` section 6.5): once a turn it casts the
//! best cast it can pay for, then the units act (`ai::next_command`), or the side runs when the
//! fight is lost (`ai::retreat`).
//!
//! A cast is scored from the spell's data, not by name: damage and slaying by how much of an
//! enemy unit they take (and the odds that it fails its save), healing by how much of a hurt
//! unit they restore, a summon by the strength of what appears, a blessing or a curse by a share
//! of the worth of the unit it falls on, Call Chaos by an average hit on every enemy unit, a
//! Magic Vortex by the enemy and friendly units near the square it is raised on, the battlefield
//! spells, walls, dispels, counter magic, recalls and the ground spells by the units they act on
//! (`docs/battle-spell-ai.md` lists every spell). Every spell has a value; one that has none in
//! a fight waits for what it needs (a hurt unit, a lasting spell to dispel, a shooter, a creature
//! of a realm, an enemy walker on the square).
//!
//! The wizard has no worth floor: it spends what it may, the best cast first. What it may spend
//! is bounded by the casting skill of the battle, the reserve less the upkeep, and the budget of
//! the strategic AI (`budget.rs`); the units' own casts (a hero's spell, a charge) keep a floor
//! (`worth_floor`), because a unit that casts gives up its attack and a scratch is not worth that.
//! The numbers of the values are the levers of `ai_battle.ron`.
//!
//! A vortex of the side that has not moved yet is steered one square toward the enemy
//! ([`steer_choice`]), as a human may (the original's computer never does).
//!
//! Tests: at the end of this file.

use super::lever::levers;
use super::order::{self as ai, worth};
use f4x_battle::battle::{Battle, BattleCommand, RangedKind};
use f4x_battle::battlemagic::caster::SpellAim;
use f4x_battle::battlemagic::field::Field;
use f4x_battle::map::{self, Pos};
use f4x_battle::{BattleUnit, Side};
use f4x_core::battle::spell::{slay_allows, BattleEffect};
use f4x_core::rules::effects::{Effect, EffectRule, TargetSel};
use f4x_core::rules::unit_flag::UnitFlag;
use f4x_core::rules::{ModOp, SpellDef, SpellId, Stat};
use f4x_core::spell::unit_effect::UnitEffect;
use f4x_core::world::World;
use f4x_def::rules::ai_battle::BattleAiRules;
use f4x_def::rules::immunity::Immunities;
use f4x_def::rules::realm::Realm;

/// The next command for the side to play: a cast if one is worth it, else a unit order.
pub fn next_command(world: &World) -> Option<BattleCommand> {
    let battle = world.battle()?;
    Some(
        // Coming up from the ground costs nothing: first.
        ai::surface_order(battle)
            .or_else(|| cast_choice(world, battle))
            .or_else(|| unit_cast_choice(world, battle))
            .or_else(|| ai::merge_order(battle))
            .or_else(|| ai::retreat(battle))
            .or_else(|| steer_choice(battle))
            .unwrap_or_else(|| ai::next_command(battle)),
    )
}

/// The cast a unit of the side to play should make now, if one is worth it (a hero's spell, an
/// item's charge, a creature caster, an ability): scored as the wizard's casts are, against
/// the unit's own mana. A free cast (a charge, an ability) costs nothing.
pub fn unit_cast_choice(world: &World, battle: &Battle) -> Option<BattleCommand> {
    let side = battle.active;
    let enemies: Vec<&BattleUnit> = battle
        .active_units(side.other())
        .filter(|e| battle.seen_by(side, e.id))
        .collect();
    let friends: Vec<&BattleUnit> = battle.active_units(side).collect();
    let choice = levers(&battle.content.rules).choice;
    let floor = enemies.iter().map(|e| worth(battle, e)).fold(0.0, f32::max) * choice.worth_floor;
    // A demon is worth calling whenever the unit can: it is free and strong.
    if let Some(unit) = friends
        .iter()
        .map(|u| u.id)
        .find(|&u| world.can_summon_demon(u))
    {
        return Some(BattleCommand::SummonDemon { unit });
    }
    let scene = Scene {
        world,
        battle,
        side,
        friends: &friends,
        enemies: &enemies,
    };
    let dearest = enemies.iter().map(|e| worth(battle, e)).fold(0.0, f32::max);
    let mut best: Option<(Tier, f32, BattleCommand)> = None;
    for (unit, cast) in world.legal_unit_casts() {
        let free = world
            .battle_unit_spells(unit)
            .iter()
            .any(|s| s.spell == cast.spell && s.cost == 0);
        let mana = if free { 0 } else { cast.least_mana };
        let def = world.content().rules.spells.get(cast.spell);
        for &aim in &cast.aims {
            let judged = scene.judge(cast.spell, def, aim, cast.least_mana, cast.least_mana);
            if judged.value < scene.floor_for(&judged, floor) {
                continue;
            }
            let tier = judged.tier(choice.wound_enough * dearest);
            let score = judged.value / (mana as f32 + choice.mana_softening);
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
pub fn cast_choice(world: &World, battle: &Battle) -> Option<BattleCommand> {
    let side = battle.active;
    let player = battle.player_of(side);
    let reserve = world
        .player(player)
        .map_or(0, |p| p.treasury().mana.floor());
    let upkeep = world.mana_upkeep(player).ceil();
    let enemies: Vec<&BattleUnit> = battle
        .active_units(side.other())
        .filter(|e| battle.seen_by(side, e.id))
        .collect();
    let friends: Vec<&BattleUnit> = battle.active_units(side).collect();
    let scene = Scene {
        world,
        battle,
        side,
        friends: &friends,
        enemies: &enemies,
    };
    let dearest = enemies.iter().map(|e| worth(battle, e)).fold(0.0, f32::max);
    let choice = levers(&battle.content.rules).choice;
    let mut best: Option<(Tier, f32, BattleCommand)> = None;
    for cast in world.legal_battle_casts() {
        let def = world.content().rules.spells.get(cast.spell);
        for &aim in &cast.aims {
            // Mana beyond the cost only from what the upkeep leaves in the reserve.
            let spare = u32::try_from(reserve - upkeep).unwrap_or(0);
            let most = cast.most_mana.min(spare).max(cast.least_mana);
            let judged = scene.judge(cast.spell, def, aim, cast.least_mana, most);
            let mana = judged.mana;
            // The wizard spends what it may, the best cast first, whatever it is worth: a battle
            // is where the mana is for. Only a cast of no worth is left alone.
            if (reserve - mana as i32) < upkeep || judged.value <= 0.0 {
                continue;
            }
            let tier = judged.tier(choice.wound_enough * dearest);
            let score = judged.value / (mana as f32 + choice.mana_softening);
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
    world: &'a World,
    battle: &'a Battle,
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
    /// Magical weapons for units that cannot hurt the weapon-immune enemies in the fight: before
    /// anything else, as the fight cannot be won without them (`Scene::blocked`).
    Arm,
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
    /// The cast gives magical weapons to units that need them against weapon-immune enemies.
    arms: bool,
}

impl Judged {
    /// `wound_from`: the harm from which a cast counts as a wound.
    fn tier(&self, wound_from: f32) -> Tier {
        if self.arms {
            Tier::Arm
        } else if self.kills {
            Tier::Kill
        } else if self.harm >= wound_from {
            Tier::Wound
        } else if self.value > self.harm {
            Tier::Support
        } else {
            Tier::Scratch
        }
    }
}

impl Scene<'_> {
    fn levers(&self) -> BattleAiRules {
        levers(&self.battle.content.rules)
    }

    /// The least a cast must be worth: for a blow, a share of the dearest enemy; for a gift to
    /// our own units (the fallback when no blow is worth it), a share of our dearest unit.
    fn floor_for(&self, judged: &Judged, blow_floor: f32) -> f32 {
        let choice = self.levers().choice;
        if judged.harm >= choice.blow_share * judged.value {
            blow_floor
        } else {
            choice.worth_floor
                * self
                    .friends
                    .iter()
                    .map(|u| worth(self.battle, u))
                    .fold(0.0, f32::max)
        }
    }

    /// Our units that cannot hurt the enemy: normal weapons against a side with weapon immunity
    /// (a defence floor against the weapon type) in it, where a unit's weapons are magical for fantastic creatures,
    /// heroes and what a spell armed (`magical_weapon`, 41). None when no enemy is immune.
    fn blocked(&self) -> Vec<&BattleUnit> {
        let rules = &self.world.content().rules;
        let immune = self.enemies.iter().any(|u| {
            Immunities::of(&u.abilities, &u.enchantments, rules)
                .has(f4x_core::rules::damage_type::DamageType::MundaneWeapon)
        });
        if !immune {
            return Vec::new();
        }
        self.friends
            .iter()
            .copied()
            .filter(|u| !u.flag(UnitFlag::MagicalWeapon) && self.battle.now_of(u).melee() > 0)
            .collect()
    }

    /// The worth of the units of one side that `keep` picks.
    fn sum(&self, units: &[&BattleUnit], keep: impl Fn(&BattleUnit) -> bool) -> f32 {
        units
            .iter()
            .filter(|u| keep(u))
            .map(|u| worth(self.battle, u))
            .sum()
    }

    /// What a battlefield enchantment is worth to the side that casts it, and the part of that
    /// which is harm to the enemy: a share of the worth of the units it falls on.
    fn field_value(&self, field: Field) -> (f32, f32) {
        let (own, foe) = (self.friends, self.enemies);
        let all = |_: &BattleUnit| true;
        // The units the enchantment cannot harm: those immune to what it harms with.
        let spared = |u: &BattleUnit| {
            self.world
                .battle()
                .is_some_and(|b| b.field_spares(u, field))
        };
        let realm = |r: Realm| move |u: &BattleUnit| u.realm == Some(r);
        let levers = self.levers();
        let f = levers.field_spells;
        let (gain, harm) = match field {
            Field::Prayer => (f.prayer_share * self.sum(own, all), 0.0),
            Field::HighPrayer => (f.high_prayer_share * self.sum(own, all), 0.0),
            Field::BlackPrayer => (0.0, f.black_prayer_share * self.sum(foe, all)),
            Field::MetalFires => (
                f.metal_fires_share * self.sum(own, |u| !u.is_fantastic())
                    + levers.unit_spells.arm_share * self.sum(&self.blocked(), |_| true),
                0.0,
            ),
            Field::MassInvisibility => (f.mass_invisibility_share * self.sum(own, all), 0.0),
            // Blur only matters against shooters: scaled by their share of the enemy's worth.
            Field::Blur => {
                let shooters = self.sum(foe, |u| u.ranged_kind != RangedKind::None);
                let total = self.sum(foe, all).max(1.0);
                (f.blur_share * self.sum(own, all) * shooters / total, 0.0)
            }
            Field::Terror => (0.0, f.terror_share * self.sum(foe, |u| !spared(u))),
            Field::Entangle => (0.0, f.entangle_share * self.sum(foe, all)),
            Field::Wrack => (0.0, f.wrack_share * self.sum(foe, |u| !spared(u))),
            Field::CallLightning => (0.0, f.lightning_share * self.sum(foe, all)),
            Field::ManaLeak => {
                let wizard = self.world.battle().map(|b| b.player_of(self.side.other()));
                let has_mana = wizard
                    .and_then(|p| self.world.player(p))
                    .is_some_and(|p| p.treasury().mana.floor() > 0);
                let dearest = foe
                    .iter()
                    .map(|u| worth(self.battle, u))
                    .fold(0.0, f32::max);
                (
                    0.0,
                    if has_mana {
                        f.mana_leak_share * dearest
                    } else {
                        0.0
                    },
                )
            }
            // Light helps the creatures of life and hurts those of death, Darkness the reverse.
            Field::TrueLight | Field::Darkness => {
                let (good, bad) = if field == Field::TrueLight {
                    (Realm::Life, Realm::Death)
                } else {
                    (Realm::Death, Realm::Life)
                };
                let up = self.sum(own, realm(good)) + self.sum(foe, realm(bad));
                let down = self.sum(own, realm(bad)) + self.sum(foe, realm(good));
                let hurt = f.light_share * self.sum(foe, realm(bad));
                (f.light_share * (up - down), hurt)
            }
            // Warp Reality weakens every unit but the chaos creatures.
            Field::WarpReality => {
                let hit = |u: &BattleUnit| u.realm != Some(Realm::Chaos);
                let foes = f.warp_reality_share * self.sum(foe, hit);
                (foes - f.warp_reality_share * self.sum(own, hit), foes)
            }
        };
        let value = gain + harm;
        (value.max(0.0), harm.min(value.max(0.0)))
    }

    /// What the spells that act on a square of the ground are worth there: mud holds back the
    /// enemy walkers near it, a fissure may take the walker over it and opens a wall, Disrupt
    /// opens a wall section for the side that attacks the city.
    fn ground_value(&self, effect: BattleEffect, cell: Pos) -> f32 {
        let Some(battle) = self.world.battle() else {
            return 0.0;
        };
        let walks = |u: &BattleUnit| !u.flag(UnitFlag::Flies) && !u.flag(UnitFlag::NonCorporeal);
        let near = |u: &&BattleUnit, reach: u32| map::distance(u.cell, cell) <= reach;
        let ours = |reach: u32| self.sum(self.friends, |u| walks(u) && near(&u, reach));
        let theirs = |reach: u32| self.sum(self.enemies, |u| walks(u) && near(&u, reach));
        let own_worth: f32 = self.friends.iter().map(|u| worth(self.battle, u)).sum();
        let g = self.levers().ground_spells;
        // A section of wall comes down: the attacker's gain, more with the enemy at the gap.
        let breach = if self.side == Side::Attacker && battle.map.has_wall(cell) {
            let gap = if self.enemies.iter().any(|u| near(u, g.breach_reach)) {
                1.0
            } else {
                g.breach_far_part
            };
            g.wall_break_share * own_worth * gap
        } else {
            0.0
        };
        match effect {
            BattleEffect::EarthToMud => {
                (g.mud_share * (theirs(g.mud_reach) - ours(g.mud_reach))).max(0.0)
            }
            BattleEffect::CracksCall => {
                let over = |units: &[&BattleUnit]| {
                    self.sum(units, |u| {
                        u.cell == cell
                            && walks(u)
                            && !u.has(f4x_core::rules::Ability::new(43 /* merging */))
                    })
                };
                let odds = battle.content.rules.battle.fissure_odds();
                (odds * (over(self.enemies) - over(self.friends)) + breach).max(0.0)
            }
            _ => breach,
        }
    }

    /// Counter Magic: worth something while the enemy wizard has mana to cast with.
    fn counter_value(&self) -> f32 {
        let wizard = self.world.battle().map(|b| b.player_of(self.side.other()));
        let mana = wizard
            .and_then(|p| self.world.player(p))
            .map_or(0, |p| p.treasury().mana.floor());
        if mana > 0 {
            self.levers().unit_spells.counter_share
                * self
                    .friends
                    .iter()
                    .map(|u| worth(self.battle, u))
                    .fold(0.0, f32::max)
        } else {
            0.0
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
        let odds = self.world.preview_spell_damage(spell, target.id, 0)?;
        let left = target.hit_points().max(1) as f32;
        let hurt = |mean: f32| (mean.min(left) / left) * worth(self.battle, target);
        let choice = self.levers().choice;
        let killed = || choice.kill_bonus * worth(self.battle, target);
        if odds.kill_chance >= choice.kill_odds {
            return Some((killed(), true, least));
        }
        // Not enough: more mana, if the spell takes it, until it is.
        let mut extra = 1;
        while least + extra <= most {
            if let Some(more) = self.world.preview_spell_damage(spell, target.id, extra) {
                if more.kill_chance >= choice.kill_odds {
                    return Some((killed(), true, least + extra));
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
            SpellAim::Unit(id) | SpellAim::UnitTo(id, _) => self.unit(id),
            _ => None,
        };
        let own = BattleEffect::of(def);
        let spells = self.levers().unit_spells;
        // A curse the enemy may resist is worth what it is likely to take: a unit with a high
        // resistance is left alone (a spell that also damages is judged by its own odds).
        let landing = match aimed {
            Some(u)
                if def.save.is_some()
                    && u.side != self.side
                    && !def
                        .effects
                        .iter()
                        .any(|r| matches!(r.effect, Effect::Damage { .. })) =>
            {
                self.world
                    .battle()
                    .map_or(1.0, |b| b.save_failure_chance(def, u.id))
            }
            _ => 1.0,
        };
        let mut out = Judged {
            value: 0.0,
            harm: 0.0,
            kills: false,
            mana: least,
            arms: false,
        };
        // A lasting spell that only gives the unit defence rules (a blessing, Elemental Armor,
        // Righteousness): a share of the unit's worth, as any other lasting spell on a unit.
        if def.effects.is_empty() && !def.defence.is_empty() {
            out.value +=
                aimed.map_or(0.0, |u| spells.lasting_share * worth(self.battle, u)) * landing;
        }
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
                            (spells.chaos_damage.min(u.hit_points().max(1) as f32)
                                / u.hit_points().max(1) as f32)
                                * worth(self.battle, u)
                        })
                        .sum();
                    (v, v)
                }
                Effect::Custom { .. } if own == Some(BattleEffect::Vortex) => match aim {
                    SpellAim::Tile(cell) => {
                        let v = vortex_reach_value(self.battle, cell, self.friends, self.enemies);
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
                        .map_or(0.0, |u| spells.warp_wood_share * worth(self.battle, u));
                    (v, v)
                }
                Effect::Custom { name, .. } if name == "battlefield" => {
                    if Field::of(def) == Some(Field::MetalFires) && !self.blocked().is_empty() {
                        out.arms = true;
                    }
                    Field::of(def).map_or((0.0, 0.0), |f| self.field_value(f))
                }
                // Magical weapons for a unit that cannot hurt the weapon-immune enemies.
                Effect::GrantAbility(a)
                    if *a == f4x_core::rules::Ability::new(41 /* magical_weapon */)
                        && aimed.is_some_and(|u| {
                            u.side == self.side && self.blocked().iter().any(|b| b.id == u.id)
                        }) =>
                {
                    out.arms = true;
                    (
                        spells.arm_share * aimed.map_or(0.0, |u| worth(self.battle, u)),
                        0.0,
                    )
                }
                Effect::Custom { name, .. } if name == "wall" => {
                    let v = spells.wall_share
                        * self
                            .friends
                            .iter()
                            .map(|u| worth(self.battle, u))
                            .sum::<f32>();
                    (v, 0.0)
                }
                Effect::Custom { name, args } if name == "counter_effect" => {
                    match args.first().map(String::as_str) {
                        Some("dispel") => {
                            let v = aimed.filter(|u| u.side != self.side).map_or(0.0, |u| {
                                let most = spells.dispel_most_spells as usize;
                                spells.dispel_share
                                    * worth(self.battle, u)
                                    * u.enchantments.len().min(most) as f32
                            });
                            (v, v)
                        }
                        Some("counter_magic") => (self.counter_value(), 0.0),
                        _ => (0.0, 0.0),
                    }
                }
                // Word of Recall and Recall Hero: a unit that is nearly dead is taken out of the
                // fight whole.
                Effect::Custom { name, .. } if name == "recall" => {
                    let v = aimed.filter(|u| u.side == self.side).map_or(0.0, |u| {
                        let left = u.health();
                        if left < spells.recall_health {
                            spells.recall_share * (1.0 - left) * worth(self.battle, u)
                        } else {
                            0.0
                        }
                    });
                    (v, 0.0)
                }
                Effect::Custom { .. } if own == Some(BattleEffect::LifeDrain) => {
                    let v = aimed
                        .filter(|u| u.side != self.side)
                        .map_or(0.0, |u| spells.drain_share * worth(self.battle, u));
                    (v, v)
                }
                Effect::Custom { .. }
                    if matches!(
                        own,
                        Some(
                            BattleEffect::EarthToMud
                                | BattleEffect::CracksCall
                                | BattleEffect::Disrupt
                        )
                    ) =>
                {
                    let v = match (aim, own) {
                        (SpellAim::Tile(cell), Some(effect)) => self.ground_value(effect, cell),
                        _ => 0.0,
                    };
                    (v, v)
                }
                // The dead come back on our side: the unit that fell, at a share of its worth.
                Effect::Custom { .. }
                    if matches!(own, Some(BattleEffect::Raise | BattleEffect::Animate)) =>
                {
                    let share = if own == Some(BattleEffect::Raise) {
                        spells.raise_share
                    } else {
                        spells.animate_share
                    };
                    let v = match aim {
                        SpellAim::Unit(id) => self
                            .world
                            .battle()
                            .and_then(|b| b.unit(id))
                            .map_or(0.0, |u| share * worth(self.battle, u)),
                        _ => 0.0,
                    };
                    (v, 0.0)
                }
                // A lasting spell on a unit (a blessing, a web, a weakness): a share of the unit's
                // worth, a gain on one of ours and harm on an enemy. The game refuses the spells
                // the unit already carries or cannot take. One that holds a unit or takes it
                // over is worth more.
                Effect::Custom { name, args } if name == "unit_effect" || name == "heroism" => {
                    let share = if args
                        .iter()
                        .any(|a| a == "possession" || a == "creature_binding")
                    {
                        spells.steal_share
                    } else if args.iter().any(|a| a == "holds") {
                        spells.hold_share
                    } else {
                        spells.lasting_share
                    };
                    let v = aimed.map_or(0.0, |u| share * worth(self.battle, u));
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
                        self.battle,
                        self.world,
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
            out.value += value * landing;
            out.harm += harm * landing;
        }
        out
    }
}

/// What a vortex raised on `cell` is worth: a share of the worth of each enemy unit within reach,
/// less the same for ours (it strikes whoever it comes to).
fn vortex_reach_value(
    battle: &Battle,
    cell: Pos,
    friends: &[&BattleUnit],
    enemies: &[&BattleUnit],
) -> f32 {
    let ground = levers(&battle.content.rules).ground_spells;
    let sum = |units: &[&BattleUnit]| -> f32 {
        units
            .iter()
            .filter(|u| map::distance(u.cell, cell) <= ground.vortex_reach)
            .map(|u| worth(battle, u))
            .sum()
    };
    ground.vortex_share * (sum(enemies) - sum(friends))
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
    let landing_share = levers(&battle.content.rules)
        .ground_spells
        .vortex_landing_share;
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
                    sign * worth(battle, u)
                });
                let value =
                    vortex_reach_value(battle, to, &friends, &enemies) + landing_share * on_square;
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
    battle: &Battle,
    world: &World,
    rule: &EffectRule,
    aimed: Option<&BattleUnit>,
    side: Side,
    friends: &[&BattleUnit],
    enemies: &[&BattleUnit],
) -> f32 {
    let on = victims(rule, aimed, friends, enemies);
    let ours = |u: &BattleUnit| u.side == side;
    let spells = levers(&battle.content.rules).unit_spells;
    match &rule.effect {
        // A slaying spell: a share of the worth of each enemy it can touch, by the odds that
        // the enemy fails its resistance check.
        Effect::Slay {
            types,
            realm,
            modifier,
            only,
            ..
        } => on
            .iter()
            .filter(|u| {
                !ours(u)
                    && !u.is_immune_to(types, &world.content().rules)
                    && slay_allows(*only, u)
                    // Spell Lock keeps a unit from the spells that sever or burn it.
                    && !u.effects(&world.content().rules).contains(UnitEffect::SpellLock)
            })
            .map(|u| {
                let chance = world.battle().map_or(1.0, |b| {
                    b.failure_chance(u.id, Realm::of(*realm), *modifier)
                });
                spells.slay_share * worth(battle, u) * chance
            })
            .sum(),
        // Healing is kept for a unit that has lost half its hit points or more: a cast for a
        // scratch wastes the mana.
        Effect::Heal(points) => on
            .iter()
            .filter(|u| ours(u) && u.health() <= spells.heal_health)
            .map(|u| {
                let missing = u.lost().max(0) as f32;
                (missing.min(*points as f32) / u.max_hit_points().max(1) as f32) * worth(battle, u)
            })
            .sum(),
        Effect::Summon {
            unit: id, count, ..
        } => {
            let d = world.content().rules.units.get(*id);
            let stat = |s: Stat| d.stats.get(&s).map_or(0, |v| v.floor());
            let hp = stat(Stat::MaxHp).max(1) * stat(Stat::Figures).max(1);
            // Fresh units fight at once but are an extra target: a share of their worth, on the
            // footing of `worth` above (the strength of a unit as it stands in a fight).
            spells.summon_share
                * f4x_battle::battle::strength(
                    stat(Stat::Melee).max(stat(Stat::Ranged)),
                    stat(Stat::Defense),
                    hp,
                )
                * f32::from(*count)
        }
        Effect::Modify { op, value, .. } => {
            // A blessing on the strongest of ours is worth a share of its strength; a curse on
            // an enemy likewise. The sign of the change says which it is; a stat set to nothing
            // (a web holds a unit still) is a curse.
            let zero = f4x_def::half_int::HalfInt::ZERO;
            // A cap (Shatter lets a unit hit for one at most) is a curse whatever its figure.
            let good =
                *value >= zero && !(*op == ModOp::Set && *value == zero) && *op != ModOp::Max;
            on.iter()
                .filter(|u| ours(u) == good)
                .map(|u| spells.lasting_share * worth(battle, u))
                .sum()
        }
        // A gift of an ability the unit already has is worth nothing.
        Effect::GrantAbility(a) => on
            .iter()
            .filter(|u| ours(u) && !u.has(*a))
            .map(|u| spells.lasting_share * worth(battle, u))
            .sum(),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, shipped, COAST_CITY, P0, P1};
    use f4x_battle::battle::BattleCommand;
    use f4x_battle::Side;
    use f4x_core::player::view::PlayerView;
    use f4x_core::world::Command;
    use f4x_def::testdir::TestResult;
    use f4x_master::Game;

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
        let mut w = game(COAST_CITY, 7);
        let id = shipped()
            .rules
            .spells
            .id_numbered(spell)
            .ok_or("the spell is not in the content")?;
        w.cheat_learn_spell(P0, id);
        w.wizard_mut(P0)
            .ok_or("no wizard")?
            .get_mut::<f4x_core::wizard::sheet::Skill>()
            .left = 500;
        w.treasury_mut(P0).ok_or("no treasury")?.mana =
            f4x_def::half_int::HalfInt::from_int(reserve);
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let ours = w.spawn_unit(P0, at(4, 4), walker);
        w.scenario_set_hurt(ours, hurt);
        let theirs: Vec<_> = (0..foes)
            .map(|_| w.spawn_unit(P1, at(5, 4), walker))
            .collect();
        w.command(Command::BeginBattle {
            attackers: vec![ours],
            defenders: theirs,
        });
        w.scenario_battle_caster(Side::Attacker, mana);
        Ok(w)
    }

    #[test]
    fn the_scripted_wizard_casts_a_damage_spell_at_an_enemy_when_it_can_pay() -> TestResult {
        let w = spell_duel(83 /* fire_bolt */, 2, 50, 50)?;
        let bolt = shipped()
            .rules
            .spells
            .id_numbered(83)
            .ok_or("no fire bolt")?;
        let Some(BattleCommand::Cast { spell, aim, .. }) = next_command(&w) else {
            return Err("the wizard did not cast".into());
        };
        assert_eq!(spell, bolt);
        let SpellAim::Unit(target) = aim else {
            return Err(format!("a bolt is aimed at a unit, not {aim:?}").into());
        };
        let battle = w.battle().ok_or("battle is missing")?;
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
            let w = spell_duel(name, 3, 200, 200)?;
            let id = shipped()
                .rules
                .spells
                .id_numbered(name)
                .ok_or("no such spell")?;
            let Some(BattleCommand::Cast { spell, aim, .. }) = next_command(&w) else {
                return Err(format!("the wizard did not cast {name}").into());
            };
            assert_eq!(spell, id, "{name}");
            if name == 105 {
                let SpellAim::Tile(cell) = aim else {
                    return Err(format!("a vortex is raised on a square, not {aim:?}").into());
                };
                let battle = w.battle().ok_or("battle is missing")?;
                assert!(
                    battle.units.iter().any(|u| u.side == Side::Defender
                        && map::distance(u.cell, cell)
                            <= shipped().rules.ai_battle.ground_spells.vortex_reach),
                    "the vortex is raised among the enemy, not at {cell:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn the_scripted_side_steers_its_vortex_onto_an_enemy_once_and_leaves_the_enemys_alone() {
        let mut b = crate::testworld::real_battle(
            &[(105 /* high_men_swordsmen */, (3, 10))],
            &[(105, (10, 10))],
            5,
        );
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
        let w = spell_duel(83 /* fire_bolt */, 2, 0, 50)?;
        assert!(!matches!(
            next_command(&w),
            Some(BattleCommand::Cast { .. })
        ));
        // Mana in the battle, but an empty reserve overland: the upkeep comes first.
        let mut w = spell_duel(83 /* fire_bolt */, 2, 50, 0)?;
        w.treasury_mut(P0).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(-5);
        assert!(!matches!(
            next_command(&w),
            Some(BattleCommand::Cast { .. })
        ));
        Ok(())
    }

    #[test]
    fn the_scripted_wizard_heals_a_unit_down_to_half_but_not_a_whole_or_scratched_one() -> TestResult
    {
        let w = spell_duel(125 /* healing */, 1, 50, 50)?;
        assert!(
            !matches!(next_command(&w), Some(BattleCommand::Cast { .. })),
            "nothing is hurt"
        );
        let hits = cavalry_hit_points()?;
        let w = spell_duel_hurt(125 /* healing */, 1, 50, 50, hits.div_ceil(2) - 1)?;
        assert!(
            !matches!(next_command(&w), Some(BattleCommand::Cast { .. })),
            "a unit above half health is left alone"
        );
        let w = spell_duel_hurt(125 /* healing */, 1, 50, 50, hits.div_ceil(2))?;
        let Some(BattleCommand::Cast { aim, .. }) = next_command(&w) else {
            return Err("the wizard did not heal".into());
        };
        let ours = w
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
    /// stack of walkers: the game, the attackers and the defenders, before the battle.
    #[allow(clippy::type_complexity)]
    fn strategic_setup(
        spells: &[u16],
        mana: i32,
    ) -> Result<(Game, Vec<hecs::Entity>, Vec<hecs::Entity>), Box<dyn std::error::Error>> {
        crate::battle::install();
        let mut w = game(COAST_CITY, 7);
        w.init_wizard(
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
            w.cheat_learn_spell(P0, id);
        }
        w.treasury_mut(P0).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(mana);
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let ours: Vec<_> = (0..3).map(|_| w.spawn_unit(P0, at(4, 4), walker)).collect();
        let theirs: Vec<_> = (0..6).map(|_| w.spawn_unit(P1, at(5, 4), walker)).collect();
        Ok((w, ours, theirs))
    }

    /// The mana player 0 has spent once the fight is over.
    fn spent(w: &mut Game, mana: i32) -> Result<i32, Box<dyn std::error::Error>> {
        assert!(!w.in_battle());
        Ok(mana - w.treasury_mut(P0).ok_or("no treasury")?.mana.floor())
    }

    /// A fight of player 0's wizard left to the battle AI, with the command's own budget (or
    /// none); returns the mana spent.
    fn strategic_fight(
        spells: &[u16],
        mana: i32,
        budget: Option<u32>,
    ) -> Result<i32, Box<dyn std::error::Error>> {
        let (mut w, ours, theirs) = strategic_setup(spells, mana)?;
        w.command(Command::AutoResolve {
            attackers: ours,
            defenders: theirs,
            mana_budget: budget.map(|b| (P0, b)),
        });
        spent(&mut w, mana)
    }

    /// The same fight as the strategic AI starts it (`military::engage`, which sets the budget
    /// by the stake and the odds, `budget.rs`); returns the budget it set and the mana spent.
    fn strategic_ai_fight(
        spells: &[u16],
        mana: i32,
    ) -> Result<(Option<u32>, i32), Box<dyn std::error::Error>> {
        let (mut w, ours, theirs) = strategic_setup(spells, mana)?;
        let budget = crate::budget::for_attack(&PlayerView::new(&w, P0), &ours, &theirs)
            .map(|(_, mana)| mana);
        let mut ai = crate::Ai::new(P0);
        crate::military::engage(&mut ai, &mut w, ours, theirs, &mut Vec::new());
        Ok((budget, spent(&mut w, mana)?))
    }

    #[test]
    fn the_strategic_ai_starts_its_fights_with_a_budget_the_battle_ai_keeps_to() -> TestResult {
        let spells = [
            4,   /* giant_strength */
            7,   /* stone_skin */
            121, /* bless */
        ];
        // Three against six in the open, far from home: an even fight of little stake, so a
        // part of what is spare, and the battle spends no more than it.
        let mana = 40;
        let free = strategic_fight(&spells, mana, None)?;
        let (budget, used) = strategic_ai_fight(&spells, mana)?;
        let budget = budget.ok_or("the fight was given no budget")?;
        assert!(
            i64::from(budget) < i64::from(mana),
            "a part of the reserve: {budget}"
        );
        assert!(
            free > i32::try_from(budget)?,
            "unlimited it spends {free}, over {budget}"
        );
        assert!(used <= i32::try_from(budget)?, "spent {used} of {budget}");
        Ok(())
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
        let mut w = game(COAST_CITY, 7);
        let walker = rules.units.id_numbered(107).ok_or("no cavalry")?;
        let ours = w.spawn_unit(P0, at(4, 4), f4x_core::rules::UnitId::new(caster.num));
        let theirs = w.spawn_unit(P1, at(5, 4), walker);
        w.command(Command::BeginBattle {
            attackers: vec![ours],
            defenders: vec![theirs],
        });
        let chosen = unit_cast_choice(&w, w.battle().ok_or("no battle")?);
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
        duel_prepared(spells, foe, |_, _, _| {})
    }

    /// [`duel_with`], with `prepare(game, ours, theirs)` run on the two units before the battle
    /// begins (a wound, a lasting spell: whatever a spell is conditional on).
    fn duel_prepared(
        spells: &[u16],
        foe: f4x_core::rules::UnitId,
        prepare: impl FnOnce(&mut Game, hecs::Entity, hecs::Entity),
    ) -> Result<Game, Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 7);
        for &n in spells {
            let id = shipped()
                .rules
                .spells
                .id_numbered(n)
                .ok_or("no such spell")?;
            w.cheat_learn_spell(P0, id);
        }
        w.wizard_mut(P0)
            .ok_or("no wizard")?
            .get_mut::<f4x_core::wizard::sheet::Skill>()
            .left = 500;
        w.treasury_mut(P0).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(200);
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let ours = w.spawn_unit(P0, at(4, 4), walker);
        let theirs = w.spawn_unit(P1, at(5, 4), foe);
        prepare(&mut w, ours, theirs);
        w.command(Command::BeginBattle {
            attackers: vec![ours],
            defenders: vec![theirs],
        });
        w.scenario_battle_caster(Side::Attacker, 100);
        Ok(w)
    }

    /// The most player 0's wizard finds `spell` worth in the duel `g`, over every aim there is
    /// (no aim, the square of each unit, each unit).
    fn best_worth(w: &Game, spell: u16) -> Result<f32, Box<dyn std::error::Error>> {
        let battle = w.battle().ok_or("no battle")?;
        let enemy: Vec<&BattleUnit> = battle.active_units(Side::Defender).collect();
        let friends: Vec<&BattleUnit> = battle.active_units(Side::Attacker).collect();
        let scene = Scene {
            world: w,
            battle: w.battle().ok_or("no battle")?,
            side: Side::Attacker,
            friends: &friends,
            enemies: &enemy,
        };
        let id = shipped()
            .rules
            .spells
            .id_numbered(spell)
            .ok_or("no such spell")?;
        let def = w.content().rules.spells.get(id);
        let mut aims = vec![SpellAim::None];
        for u in &battle.units {
            aims.push(SpellAim::Unit(u.id));
            aims.push(SpellAim::Tile(u.cell));
        }
        Ok(aims
            .into_iter()
            .map(|aim| scene.judge(id, def, aim, def.cost, def.cost).value)
            .fold(0.0, f32::max))
    }

    /// `spell` is worth nothing in the plain duel against `foe` and something once `prepare` has
    /// given the fight what the spell needs.
    fn needs(
        spell: u16,
        foe: f4x_core::rules::UnitId,
        prepare: impl FnOnce(&mut Game, hecs::Entity, hecs::Entity),
    ) -> TestResult {
        let plain = best_worth(&duel_with(&[], foe)?, spell)?;
        assert_eq!(
            plain, 0.0,
            "spell {spell} is worth something in a plain duel"
        );
        let met = best_worth(&duel_prepared(&[], foe, prepare)?, spell)?;
        assert!(
            met > 0.0,
            "spell {spell} is worth nothing even with its need met"
        );
        Ok(())
    }

    /// The hit points of the cavalry over all its figures.
    fn cavalry_hit_points() -> Result<u32, Box<dyn std::error::Error>> {
        let def = shipped().rules.units.get(cavalry()?);
        let stat = |s: Stat| def.stats.get(&s).map_or(0, |v| v.floor()).max(1) as u32;
        Ok(stat(Stat::MaxHp) * stat(Stat::Figures))
    }

    fn cavalry() -> Result<f4x_core::rules::UnitId, Box<dyn std::error::Error>> {
        Ok(shipped().rules.units.id_numbered(107).ok_or("no cavalry")?)
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
    fn magical_weapons_come_first_against_an_enemy_that_normal_weapons_barely_hurt() -> TestResult {
        // The enemy has weapon immunity and our cavalry has normal weapons: Flame Blade on the
        // cavalry goes before everything, a bolt that would be worth more at any other time
        // included. Against an enemy that has no such immunity it is no more than a blessing.
        let flame_blade = shipped()
            .rules
            .spells
            .id_numbered(94)
            .ok_or("no flame blade")?;
        let immune = unit_with_ability(72 /* weapon_immunity */)?;
        let w = duel_with(&[83 /* fire_bolt */, 94 /* flame_blade */], immune)?;
        let Some(BattleCommand::Cast { spell, aim, .. }) = next_command(&w) else {
            return Err("the wizard cast nothing".into());
        };
        assert_eq!(spell, flame_blade, "Flame Blade before any bolt");
        let SpellAim::Unit(id) = aim else {
            return Err("Flame Blade is aimed at a unit".into());
        };
        assert_eq!(
            w.battle()
                .ok_or("no battle")?
                .unit(id)
                .ok_or("no unit")?
                .side,
            Side::Attacker
        );
        let w = duel_with(&[83 /* fire_bolt */, 94 /* flame_blade */], cavalry()?)?;
        assert!(
            !matches!(next_command(&w), Some(BattleCommand::Cast { spell, .. }) if spell == flame_blade),
            "no rush to arm the cavalry against an enemy that weapons hurt"
        );
        Ok(())
    }

    #[test]
    fn a_fire_bolt_is_not_cast_at_a_unit_that_is_immune_to_fire_but_a_blessing_is() -> TestResult {
        let foe = unit_with_ability(21 /* fire_immunity */)?;
        let w = duel_with(&[83 /* fire_bolt */], foe)?;
        assert!(
            !matches!(next_command(&w), Some(BattleCommand::Cast { .. })),
            "nothing worth casting: {:?}",
            next_command(&w)
        );
        // With a blessing known, the wizard falls back on strengthening its own unit.
        let w = duel_with(&[83 /* fire_bolt */, 121 /* bless */], foe)?;
        let bless = shipped().rules.spells.id_numbered(121).ok_or("no bless")?;
        let Some(BattleCommand::Cast { spell, aim, .. }) = next_command(&w) else {
            return Err("the wizard did not fall back on a blessing".into());
        };
        assert_eq!(spell, bless);
        let SpellAim::Unit(id) = aim else {
            return Err("a blessing is aimed at a unit".into());
        };
        assert_eq!(
            w.battle()
                .ok_or("no battle")?
                .unit(id)
                .ok_or("no unit")?
                .side,
            Side::Attacker
        );
        Ok(())
    }

    /// What a cast of `spell` at the one enemy unit of a duel is worth.
    fn curse_value(
        spell: u16,
        foe: f4x_core::rules::UnitId,
    ) -> Result<f32, Box<dyn std::error::Error>> {
        let w = duel_with(&[spell], foe)?;
        let battle = w.battle().ok_or("no battle")?;
        let enemy: Vec<&BattleUnit> = battle.active_units(Side::Defender).collect();
        let friends: Vec<&BattleUnit> = battle.active_units(Side::Attacker).collect();
        let scene = Scene {
            world: &w,
            battle: w.battle().ok_or("no battle")?,
            side: Side::Attacker,
            friends: &friends,
            enemies: &enemy,
        };
        let id = shipped()
            .rules
            .spells
            .id_numbered(spell)
            .ok_or("no spell")?;
        let def = w.content().rules.spells.get(id);
        Ok(scene
            .judge(id, def, SpellAim::Unit(enemy[0].id), 0, 0)
            .value)
    }

    #[test]
    fn a_curse_is_worth_less_against_a_unit_that_resists_it_better() -> TestResult {
        let rules = &shipped().rules;
        let resistance =
            |d: &f4x_core::rules::UnitDef| d.stats.get(&Stat::Resistance).map_or(0, |v| v.floor());
        let defs = rules.units.defs();
        let weak = defs
            .iter()
            .filter(|d| d.num != 107)
            .min_by_key(|d| resistance(d))
            .ok_or("no unit")?;
        let strong = defs.iter().max_by_key(|d| resistance(d)).ok_or("no unit")?;
        assert!(resistance(strong) > resistance(weak));
        let (weak, strong) = (
            f4x_core::rules::UnitId::new(weak.num),
            f4x_core::rules::UnitId::new(strong.num),
        );
        let against = |foe| curse_value(88 /* shatter */, foe);
        let (easy, hard) = (against(weak)?, against(strong)?);
        assert!(
            easy > 0.0,
            "a curse on a weak-willed unit is worth something"
        );
        assert!(hard < easy, "{hard} against {easy}");
        Ok(())
    }

    /// The battle spells the plain duel gives no worth: the ones that need something in the fight.
    fn unvalued_in_a_plain_duel() -> Result<Vec<String>, Box<dyn std::error::Error>> {
        use f4x_core::rules::defs::CastContext;
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let w = duel_with(&[], walker)?;
        let battle = w.battle().ok_or("no battle")?;
        let enemy: Vec<&BattleUnit> = battle.active_units(Side::Defender).collect();
        let friends: Vec<&BattleUnit> = battle.active_units(Side::Attacker).collect();
        let scene = Scene {
            world: &w,
            battle: w.battle().ok_or("no battle")?,
            side: Side::Attacker,
            friends: &friends,
            enemies: &enemy,
        };
        let mut aims = vec![SpellAim::None, SpellAim::Tile((4, 4))];
        aims.extend(battle.units.iter().map(|u| SpellAim::Unit(u.id)));
        let mut none = Vec::new();
        for def in shipped().rules.spells.defs() {
            if !def.contexts.contains(&CastContext::Battle) {
                continue;
            }
            let id = SpellId::new(def.num);
            let best = aims
                .iter()
                .map(|&aim| scene.judge(id, def, aim, def.cost, def.cost).value)
                .fold(0.0, f32::max);
            if best <= 0.0 {
                none.push(def.id.clone());
            }
        }
        Ok(none)
    }

    #[test]
    fn every_battle_spell_has_a_value_or_waits_for_what_it_needs() -> TestResult {
        // In a plain duel of two walkers, with no mana on the enemy side, these have no worth:
        // each needs something the duel lacks (a hurt unit, a lasting spell to dispel, a unit
        // that is nearly dead, a missile unit, a creature of the realm, an enemy walker on the
        // square, a wizard with mana, a wall to break). The tests above each prove the spell
        // gains its worth when the need is met; Disrupt needs a walled city under attack and
        // Magic Vortex a square near the enemy, which the plain duel has none of.
        let mut waits = unvalued_in_a_plain_duel()?;
        waits.sort();
        let mut expected = vec![
            "banish",
            "blur",
            "counter_magic",
            "cracks_call",
            "dispel_evil",
            "dispel_magic",
            "dispel_magic_true",
            "disrupt",
            "earth_to_mud",
            "healing",
            "holy_word",
            "magic_vortex",
            "mana_leak",
            "mass_healing",
            "recall_hero",
            "true_light",
            "darkness",
            "warp_wood",
            "word_of_recall",
        ];
        expected.sort();
        assert_eq!(waits, expected, "a spell gained or lost its worth");
        Ok(())
    }

    #[test]
    fn healing_is_worth_something_only_for_a_unit_of_ours_down_to_half_its_hit_points() -> TestResult
    {
        let hits = cavalry_hit_points()?;
        let half = hits.div_ceil(2);
        for spell in [125 /* healing */, 149 /* mass_healing */] {
            needs(spell, cavalry()?, |w, ours, _| {
                w.scenario_set_hurt(ours, half)
            })?;
            // A scratch, or a unit with more than half its hit points, is not worth the mana.
            let scratched = best_worth(
                &duel_prepared(&[], cavalry()?, |w, ours, _| {
                    w.scenario_set_hurt(ours, half - 1)
                })?,
                spell,
            )?;
            assert_eq!(scratched, 0.0, "spell {spell} for a unit above half health");
        }
        Ok(())
    }

    /// A hero that knows Healing, with a cavalry friend hurt by `hurt`, against one enemy cavalry.
    fn healer_hero_battle(hurt: u32) -> Result<Game, Box<dyn std::error::Error>> {
        use f4x_core::hero::{Hero, HeroAbilities};
        let rules = &shipped().rules;
        let healing = rules.spells.id_numbered(125).ok_or("no healing")?;
        let kind = rules
            .units
            .defs()
            .iter()
            .find_map(|d| d.hero.as_ref().map(|_| d.num))
            .ok_or("no hero")? as usize;
        let mut w = game(COAST_CITY, 7);
        w.scenario_make_hero(Hero {
            kind,
            name: "Ann".into(),
            abilities: HeroAbilities::default(),
            casting_skill: 200,
            spells: vec![healing],
        });
        let hero = w
            .spawn_hero(P0, at(4, 4), kind)
            .map_err(|e| format!("{e:?}"))?;
        let walker = rules.units.id_numbered(107).ok_or("no cavalry")?;
        let friend = w.spawn_unit(P0, at(4, 4), walker);
        w.scenario_set_hurt(friend, hurt);
        let foe = w.spawn_unit(P1, at(5, 4), walker);
        w.command(Command::BeginBattle {
            attackers: vec![hero, friend],
            defenders: vec![foe],
        });
        Ok(w)
    }

    #[test]
    fn a_unit_that_knows_healing_heals_a_friend_down_to_half_but_not_a_scratched_one() -> TestResult
    {
        let hits = cavalry_hit_points()?;
        let w = healer_hero_battle(hits.div_ceil(2) - 1)?;
        let b = w.battle().ok_or("no battle")?;
        assert!(
            !matches!(
                unit_cast_choice(&w, b),
                Some(BattleCommand::UnitCast { spell, .. }) if spell == shipped().rules.spells.id_numbered(125).ok_or("no healing")?
            ),
            "a friend above half health is left alone"
        );
        let w = healer_hero_battle(hits.div_ceil(2))?;
        let b = w.battle().ok_or("no battle")?;
        let chosen = unit_cast_choice(&w, b);
        assert!(
            matches!(
                chosen,
                Some(BattleCommand::UnitCast {
                    aim: SpellAim::Unit(1),
                    ..
                })
            ),
            "the hero should heal the hurt friend: {chosen:?}"
        );
        Ok(())
    }

    #[test]
    fn a_dispel_is_worth_something_only_against_a_lasting_spell_on_an_enemy() -> TestResult {
        let bless = shipped().rules.spells.id_numbered(121).ok_or("no bless")?;
        for spell in [202 /* dispel_magic */, 42 /* dispel_magic_true */] {
            needs(spell, cavalry()?, |w, _, theirs| {
                w.scenario_enchant_unit(theirs, bless, P1);
            })?;
        }
        Ok(())
    }

    #[test]
    fn a_recall_is_worth_something_only_for_a_unit_that_is_nearly_dead() -> TestResult {
        let hits = {
            let def = shipped().rules.units.get(cavalry()?);
            let stat = |s: Stat| def.stats.get(&s).map_or(0, |v| v.floor()).max(1) as u32;
            stat(Stat::MaxHp) * stat(Stat::Figures)
        };
        for spell in [47 /* word_of_recall */, 205 /* recall_hero */] {
            needs(spell, cavalry()?, |w, ours, _| {
                w.scenario_set_hurt(ours, hits - 1);
            })?;
        }
        Ok(())
    }

    #[test]
    fn blur_is_worth_something_only_against_shooters() -> TestResult {
        let shooter = unit_with_ability(3 /* bow */)?;
        let plain = best_worth(&duel_with(&[], cavalry()?)?, 51 /* blur */)?;
        let shot = best_worth(&duel_with(&[], shooter)?, 51 /* blur */)?;
        assert_eq!(plain, 0.0, "no shooter, no use for blur");
        assert!(shot > 0.0, "against archers blur is worth {shot}");
        Ok(())
    }

    #[test]
    fn light_and_the_slays_of_a_realm_are_worth_something_only_against_its_creatures() -> TestResult
    {
        let rules = &shipped().rules;
        let skeletons = rules.units.id_numbered(165).ok_or("no skeletons")?;
        let angel = rules.units.id_numbered(177).ok_or("no angel")?;
        // True Light hurts the dead, Darkness the creatures of life.
        needs_foe(128 /* true_light */, skeletons)?;
        needs_foe(169 /* darkness */, angel)?;
        // Banish, Holy Word and Dispel Evil slay only what they are meant for.
        for spell in [
            64,  /* banish */
            150, /* holy_word */
            134, /* dispel_evil */
        ] {
            needs_foe(spell, skeletons)?;
        }
        Ok(())
    }

    /// [`needs`] where the need is a foe of a realm the spell is meant for (the plain duel is
    /// against cavalry).
    fn needs_foe(spell: u16, foe: f4x_core::rules::UnitId) -> TestResult {
        let plain = best_worth(&duel_with(&[], cavalry()?)?, spell)?;
        assert_eq!(
            plain, 0.0,
            "spell {spell} is worth something against cavalry"
        );
        let met = best_worth(&duel_with(&[], foe)?, spell)?;
        assert!(
            met > 0.0,
            "spell {spell} is worth nothing against its realm"
        );
        Ok(())
    }

    #[test]
    fn the_ground_spells_are_worth_something_only_against_an_enemy_walker_on_the_square(
    ) -> TestResult {
        // A dearer walker of theirs than ours, so that mud and a vortex near both do net good.
        let mammoth = shipped().rules.units.id_numbered(153).ok_or("no mammoth")?;
        for spell in [
            1,   /* earth_to_mud */
            11,  /* cracks_call */
            105, /* magic_vortex */
        ] {
            let plain = best_worth(&duel_with(&[], cavalry()?)?, spell)?;
            let met = best_worth(&duel_with(&[], mammoth)?, spell)?;
            assert!(met > plain, "spell {spell}: {met} against {plain}");
            assert!(
                met > 0.0,
                "spell {spell} is worth nothing against a mammoth"
            );
        }
        Ok(())
    }

    #[test]
    fn counter_magic_and_mana_leak_are_worth_something_once_the_enemy_wizard_has_mana() -> TestResult
    {
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let mut w = duel_with(&[], walker)?;
        w.treasury_mut(P1).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(100);
        let battle = w.battle().ok_or("no battle")?;
        let enemy: Vec<&BattleUnit> = battle.active_units(Side::Defender).collect();
        let friends: Vec<&BattleUnit> = battle.active_units(Side::Attacker).collect();
        let scene = Scene {
            world: &w,
            battle: w.battle().ok_or("no battle")?,
            side: Side::Attacker,
            friends: &friends,
            enemies: &enemy,
        };
        for name in ["counter_magic", "mana_leak"] {
            let def = w
                .content()
                .rules
                .spells
                .defs()
                .iter()
                .find(|d| d.id == name)
                .ok_or("no such spell")?;
            let judged = scene.judge(
                SpellId::new(def.num),
                def,
                SpellAim::None,
                def.cost,
                def.cost,
            );
            assert!(judged.value > 0.0, "{name} is worth nothing");
        }
        Ok(())
    }

    #[test]
    fn a_damage_spell_that_cannot_kill_is_fed_mana_until_it_does() -> TestResult {
        let walker = shipped().rules.units.id_numbered(107).ok_or("no cavalry")?;
        let w = duel_with(&[83 /* fire_bolt */], walker)?;
        let battle = w.battle().ok_or("no battle")?;
        let enemy: Vec<&BattleUnit> = battle.active_units(Side::Defender).collect();
        let friends: Vec<&BattleUnit> = battle.active_units(Side::Attacker).collect();
        let scene = Scene {
            world: &w,
            battle: w.battle().ok_or("no battle")?,
            side: Side::Attacker,
            friends: &friends,
            enemies: &enemy,
        };
        let bolt = shipped().rules.spells.id_numbered(83).ok_or("no bolt")?;
        let def = w.content().rules.spells.get(bolt);
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
        let w = duel_with(&[81 /* warp_wood */], walker)?;
        let wood = shipped()
            .rules
            .spells
            .id_numbered(81)
            .ok_or("no warp wood")?;
        let def = w.content().rules.spells.get(wood);
        // Longbowmen of the other side, numbered 7 for the aim.
        let mut archer = crate::testworld::real_battle(
            &[(107 /* high_men_cavalry */, (3, 10))],
            &[(101 /* high_elf_longbowmen */, (10, 10))],
            1,
        )
        .units[1]
            .clone();
        assert_eq!(archer.ranged_kind, RangedKind::Missile);
        archer.id = 7;
        let mut spent = archer.clone();
        spent.set_ammo(0);
        let mut plain = archer.clone();
        plain.put_ranged_kind(RangedKind::None);
        let value = |u: &BattleUnit| {
            let enemies = [u];
            let scene = Scene {
                world: &w,
                battle: w.battle().expect("a battle is on"),
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
        let wound_from = 30.0;
        let j = |value, harm, kills| Judged {
            value,
            harm,
            kills,
            mana: 5,
            arms: false,
        };
        assert!(j(10.0, 10.0, true).tier(wound_from) > j(50.0, 50.0, false).tier(wound_from));
        assert!(j(50.0, 50.0, false).tier(wound_from) > j(20.0, 0.0, false).tier(wound_from));
        assert!(j(20.0, 0.0, false).tier(wound_from) > j(5.0, 5.0, false).tier(wound_from));
    }

    /// Rows 138, 148, 467, 683 and 684 of the 1.50 notes (zero-health units, repeated death,
    /// lists that run out of bounds after a removal): in many fights of mixed stacks whose
    /// wizards know every battle spell, after every order no unit that is alive has no figures
    /// or no hit points, nothing dead comes back, and a unit dies in the events at most once.
    #[test]
    fn death_is_one_way_and_no_live_unit_has_no_hit_points_in_fights_with_every_spell() -> TestResult
    {
        use f4x_battle::battle::BattleEvent;
        use f4x_battle::Status;
        use f4x_core::world::Event;
        use f4x_def::rules::{CastContext, UnitKind};
        crate::battle::install();
        let rules = &shipped().rules;
        let kinds: Vec<_> = rules
            .units
            .defs()
            .iter()
            .filter(|d| matches!(d.kind, UnitKind::Normal | UnitKind::Fantastic))
            .map(|d| rules.units.id_numbered(d.num).ok_or("no unit"))
            .collect::<Result<_, _>>()?;
        let spells: Vec<_> = rules
            .spells
            .iter()
            .filter(|(_, d)| d.contexts.contains(&CastContext::Battle))
            .map(|(id, _)| id)
            .collect();
        let mut orders = 0;
        for seed in 0..40u64 {
            // A small generator of the test's own, so the stacks differ from seed to seed.
            let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut below = |n: usize| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state % n as u64) as usize
            };
            let mut w = game(COAST_CITY, 7);
            for _ in 0..seed {
                w.next_u64();
            }
            for p in [P0, P1] {
                let books: Vec<_> = (0..5)
                    .map(|r| (f4x_core::rules::RealmId::new(r), 6))
                    .collect();
                w.init_wizard(p, &books, &[], None)
                    .map_err(|e| format!("{e:?}"))?;
                for &s in &spells {
                    w.cheat_learn_spell(p, s);
                }
                w.treasury_mut(p).ok_or("no treasury")?.mana =
                    f4x_def::half_int::HalfInt::from_int(800);
            }
            let mut stack = |w: &mut f4x_core::world::World, p, cell| {
                (0..=below(5))
                    .map(|_| w.spawn_unit(p, cell, kinds[below(kinds.len())]))
                    .collect::<Vec<_>>()
            };
            let ours = stack(&mut w, P0, at(4, 4));
            let theirs = stack(&mut w, P1, at(5, 4));
            w.command(Command::BeginBattle {
                attackers: ours,
                defenders: theirs,
            });
            let mut dead: std::collections::BTreeSet<usize> = Default::default();
            let mut died: std::collections::BTreeSet<usize> = Default::default();
            for _ in 0..3000 {
                let Some(command) = w.next_battle_command() else {
                    break;
                };
                let Ok(events) = w.battle_command(command.clone()) else {
                    break;
                };
                orders += 1;
                for e in &events {
                    if let Event::Battle(BattleEvent::UnitDied { unit }) = e {
                        assert!(died.insert(*unit), "seed {seed}: unit {unit} died twice");
                    }
                }
                let Some(b) = w.battle() else {
                    break;
                };
                for u in &b.units {
                    if u.is_active() {
                        assert!(
                            u.figures > 0 && u.hit_points() > 0,
                            "seed {seed} after {command:?}: {u:?}"
                        );
                        assert!(
                            !dead.contains(&u.id),
                            "seed {seed}: unit {} came back",
                            u.id
                        );
                    }
                    if u.status == Status::Dead {
                        dead.insert(u.id);
                    }
                }
            }
        }
        assert!(orders > 500, "the fights were too short: {orders} orders");
        Ok(())
    }
}
