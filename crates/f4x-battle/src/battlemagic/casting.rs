//! A wizard's spells in a battle: what the wizard has to cast with, and what a spell does to
//! the units. The rules for choosing, costing and resolving spells are in `spells.rs`.
//!
//! The wizard casts once each turn, from the battle's start with the whole of their casting
//! skill to spend over the battle. The skill is the most mana (before the range multiplier)
//! the spells of the whole battle can cost; the mana comes out of the reserve, costing more the
//! further the battle is from the wizard's fortress (`range_percent`).
//!
//! Tests: in battle/tests.rs.

use f4x_def::half_int::HalfInt;
use f4x_def::id::PlayerId;
use f4x_def::rules::attack_nature::AttackNature;
use f4x_def::rules::damage_type::{DamageType, DamageTypes};
use f4x_def::rules::effects::DamageTraits;
use f4x_def::rules::immunity::effective_resistance;
use f4x_def::rules::realm::Realm;
use f4x_def::rules::stats::Modifier;
use f4x_def::rules::unit_effect::{UnitEffect, UnitEffects};
use f4x_def::rules::{Ability, Rules, SpellId};
use rand::RngCore;

use super::caster::BattleCaster;
use super::error::SpellError;
use crate::attack::{
    kill_figure, spell_attack, unblockable, Combatant, Damage, DefenderState, Mode, Volley,
};
use crate::battle::{Battle, BattleError, BattleEvent};
use crate::map::Pos;
use crate::side::Side;
use crate::unit::{BattleUnit, FieldBonus, Lasting, Sheet, Status};

/// A damage spell of `strength` on one unit (see [`Battle::spell_damage`]); the unit is changed
/// in place. Overland units are hit through [`damage_state_with_spell`] instead.
pub fn damage_unit_with_spell(
    rules: &Rules,
    unit: &mut BattleUnit,
    bonus: FieldBonus,
    strength: i32,
    traits: &DamageTraits,
    nature: AttackNature,
    rng: &mut dyn RngCore,
) -> Option<Damage> {
    let mut state = unit.defender_state(rules, bonus);
    let dmg = damage_state_with_spell(rules, &mut state, strength, traits, nature, rng)?;
    unit.set_from(&state);
    unit.record(dmg);
    Some(dmg)
}

/// A damage spell of `strength` on a defender: the figures and wounds in `state` are changed.
/// Nothing is done to a defender that is immune. Overland units are hit with this too: the game
/// builds the state from the unit it has, no battle unit needed.
pub fn damage_state_with_spell(
    rules: &Rules,
    state: &mut DefenderState,
    strength: i32,
    traits: &DamageTraits,
    nature: AttackNature,
    rng: &mut dyn RngCore,
) -> Option<Damage> {
    // A spell is a ranged attack: Righteousness, a type immunity or magic immunity turn it away.
    if state.immunities.has(DamageType::Magic)
        || state
            .immunities
            .stops(nature.types.with(DamageType::Ranged))
    {
        return None;
    }
    let volley = if traits.area {
        Volley::Area
    } else if traits.decaying {
        Volley::Chain
    } else {
        Volley::Bolt
    };
    let spell = Combatant {
        strength,
        to_hit: 0,
        mode: Mode::Ranged,
        nature: AttackNature {
            armor_piercing: traits.armor_piercing,
            ..nature
        },
        special: Default::default(),
        ..Combatant::default()
    };
    Some(spell_attack(&rules.battle, &spell, volley, state, rng))
}

impl Battle {
    pub fn set_caster(&mut self, side: Side, caster: BattleCaster) {
        self.casters[side.index()] = Some(caster);
    }

    /// Limit the mana `side`'s wizard may spend from the reserve in this battle.
    pub fn set_mana_budget(&mut self, side: Side, budget: Option<u32>) {
        if let Some(c) = self.casters[side.index()].as_mut() {
            c.budget = budget.map(|b| HalfInt::from_int(b.min(i32::MAX as u32 / 4) as i32));
        }
    }

    pub fn caster(&self, side: Side) -> Option<&BattleCaster> {
        self.casters[side.index()].as_ref()
    }

    /// Whether the active side's wizard may cast now.
    pub fn check_caster(&self) -> Result<BattleCaster, BattleError> {
        if self.winner.is_some() {
            return Err(BattleError::Over);
        }
        let caster = self
            .caster(self.active)
            .ok_or(BattleError::Spell(SpellError::NoWizard))?;
        if caster.cast_this_turn {
            return Err(BattleError::Spell(SpellError::AlreadyCast));
        }
        Ok(*caster)
    }

    /// The active wizard casts: `skill` points of casting skill, paid for from the reserve.
    /// Returns the mana taken from the reserve.
    pub fn pay_for_spell(&mut self, skill: i32) -> HalfInt {
        let Some(caster) = self.casters[self.active.index()].as_mut() else {
            return HalfInt::default();
        };
        let mana = caster.mana_for(skill).min(caster.mana);
        caster.skill_left -= skill;
        caster.mana -= mana;
        if let Some(b) = caster.budget.as_mut() {
            *b = (*b - mana).max(HalfInt::default());
        }
        caster.cast_this_turn = true;
        mana
    }

    /// Whether a creature can be summoned onto `cell` for `side`: a free, open square on its
    /// half of the field.
    pub fn is_summon_square(&self, side: Side, cell: Pos) -> bool {
        let own_half = match side {
            Side::Attacker => cell.0 <= crate::map::GRID_W / 2,
            Side::Defender => cell.0 >= crate::map::GRID_W / 2,
        };
        own_half
            && self.map.enter_cost(cell, false).is_some()
            && !self.map.is_shut_tower(cell)
            && self.unit_at(cell).is_none()
    }

    /// A free summoning square for `side`, nearest its edge first (for an AI, or a default).
    pub fn first_summon_square(&self, side: Side) -> Option<Pos> {
        (0..crate::map::GRID_H)
            .flat_map(|y| (0..crate::map::GRID_W).map(move |x| (x, y)))
            .filter(|&c| self.is_summon_square(side, c))
            .min_by_key(|&(x, y)| {
                let edge = match side {
                    Side::Attacker => x,
                    Side::Defender => crate::map::GRID_W - 1 - x,
                };
                (edge, (y - crate::map::GRID_H / 2).abs(), y)
            })
    }

    /// Put a new creature on the field for `side`. Returns its id.
    pub fn add_summoned(&mut self, side: Side, mut unit: BattleUnit, cell: Pos) -> usize {
        let id = self.units.len();
        unit.id = id;
        unit.side = side;
        unit.cell = cell;
        unit.source = None;
        // It arrives ready to act.
        unit.refill_moves();
        unit.attacked = false;
        unit.status = Status::Active;
        unit.bind(&self.content.arc());
        unit.set_wizards(self.wizards.clone());
        self.units.push(unit);
        self.note_invisibility();
        id
    }

    /// A damage spell of `strength` on a unit. It is stopped outright by magic immunity, and by
    /// Righteousness for a chaos or death spell; otherwise the unit's defence, as it is against
    /// the realm and immunities of the spell, blocks as it does a blow. Returns `None` if the
    /// unit took no harm.
    pub fn spell_damage(
        &mut self,
        target: usize,
        strength: i32,
        traits: &DamageTraits,
        nature: AttackNature,
    ) -> Option<Damage> {
        let bonus = self.field_bonus_for(&self.units[target]);
        let rng: &mut dyn RngCore = &mut self.rng;
        damage_unit_with_spell(
            &self.content.rules,
            &mut self.units[target],
            bonus,
            strength,
            traits,
            nature,
            rng,
        )
    }

    /// A die of `sides` sides, from the battle's own generator: one to `sides`.
    pub fn roll(&mut self, sides: u32) -> u32 {
        use rand::Rng;
        self.rng.gen_range(1..=sides.max(1))
    }

    /// Each figure of the unit must pass a resistance check against `realm` (with `modifier`
    /// added to the resistance) or be destroyed; with `below`, the whole unit goes if its
    /// resistance plus `modifier` is under it. `None` if the unit is immune to the spell
    /// (`immune` abilities, or protected by a spell it carries).
    pub fn spell_slay(
        &mut self,
        target: usize,
        realm: Realm,
        modifier: i32,
        types: &[DamageType],
        below: Option<i32>,
    ) -> Option<Damage> {
        let unit = &self.units[target];
        if unit.is_immune_to(types, &self.content.rules) {
            return None;
        }
        let mut state = unit.defender_state(&self.content.rules, self.field_bonus_for(unit));
        let dmg = if let Some(limit) = below {
            let res = effective_resistance(state.resistance, &state.immunities, Some(realm));
            if res + modifier < limit {
                let all = state.hit_points();
                unblockable(&mut state, all)
            } else {
                Damage::default()
            }
        } else {
            let rng: &mut dyn RngCore = &mut self.rng;
            let mut total = Damage::default();
            for _ in 0..state.figures {
                if state.resistance_failure(&self.content.rules.battle, Some(realm), modifier, rng)
                    > 0
                {
                    total += kill_figure(&mut state);
                }
            }
            total
        };
        self.units[target].set_from(&state);
        // What severs or burns leaves nothing to raise.
        let dmg = dmg.all_gone();
        self.units[target].record(dmg);
        Some(dmg)
    }

    /// Destroy a unit outright, no defence or resistance helping (Cracks Call). Nothing is left
    /// to raise.
    pub fn destroy_unit(&mut self, target: usize) -> Damage {
        let mut state = self.units[target].defender_state(
            &self.content.rules,
            self.field_bonus_for(&self.units[target]),
        );
        let all = state.hit_points();
        let dmg = unblockable(&mut state, all).all_gone();
        self.units[target].set_from(&state);
        self.units[target].record(dmg);
        dmg
    }

    /// Life Drain: a death resistance check with `modifier` added (the extra mana makes it
    /// harder to resist), and the unit loses that many hit points as drained life, which can
    /// raise it as undead. `None` if the spell spares the unit (it is immune to death, and the exceptions rule is off).
    pub fn drain_life(&mut self, target: usize, modifier: i32, spell: SpellId) -> Option<Damage> {
        let unit = &self.units[target];
        if self.spares(unit, spell) {
            return None;
        }
        let mut state = unit.defender_state(&self.content.rules, self.field_bonus_for(unit));
        let rng: &mut dyn RngCore = &mut self.rng;
        let amount = state.resistance_failure(
            &self.content.rules.battle,
            Some(Realm::Death),
            modifier - self.save_penalty,
            rng,
        );
        let dmg = unblockable(&mut state, amount).all_drained();
        self.units[target].set_from(&state);
        self.units[target].record(dmg);
        Some(dmg)
    }

    /// Turn the squares round `center` to mud (Earth to Mud): those within `mud_radius`.
    pub fn spread_mud(&mut self, center: Pos) {
        let radius = self.rules().spells.mud_radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                self.map.make_mud((center.0 + dx, center.1 + dy));
            }
        }
    }

    /// Heal a unit by `points`, bringing dead figures back; returns the hit points it gained.
    pub fn spell_heal(&mut self, target: usize, points: i32) -> i32 {
        self.units[target].heal_points(points, false)
    }

    /// Put a unit on `to`'s side for the rest of the battle (Creature Binding, Possession). It
    /// has done its turn for now; it goes home when the battle ends or the binding is dispelled.
    pub fn change_sides(&mut self, target: usize, to: Side) {
        let u = &mut self.units[target];
        if u.side == to {
            return;
        }
        u.bound_from.get_or_insert(u.side);
        u.side = to;
        u.end_moves();
        u.attacked = true;
    }

    /// Take a bound unit back to the side it belongs to.
    pub fn return_unit(&mut self, target: usize) {
        let u = &mut self.units[target];
        if let Some(owner) = u.bound_from.take() {
            u.side = owner;
            u.end_moves();
            u.attacked = true;
        }
    }

    /// Put a lasting spell on a unit: it joins the unit's list of lasting spells, and the unit's
    /// numbers are worked out again from what it started the battle with and that list. They
    /// last until the battle ends or a dispel takes the spell off.
    pub fn enchant_unit(
        &mut self,
        target: usize,
        spell: SpellId,
        caster: PlayerId,
        mods: &[Modifier],
        grants: &[Ability],
    ) {
        let web_hits = self.rules().web_hits;
        let unit = &mut self.units[target];
        let effects = UnitEffects::of(self.content.rules.spells.get(spell));
        let webbing = effects.contains(UnitEffect::Web);
        // A web cast again on a unit that has it (its web broken) only closes round it anew.
        if webbing && unit.enchantments.contains(&spell) {
            unit.web_closes(web_hits);
            return;
        }
        // Sleep ends the unit's turn.
        if effects.contains(UnitEffect::BlackSleep) {
            unit.attacked = true;
        }
        unit.lasting_mut().push(Lasting {
            spell,
            caster,
            mods: mods.to_vec(),
            grants: grants.to_vec(),
        });
        if webbing {
            unit.web_closes(web_hits);
        }
    }

    /// Give a unit the sheet the game made of it (brought up to date when the unit it was made from
    /// changed) and the lasting spells it still has, `lasting`: what a dispel leaves. Nothing but
    /// the sheet and the list decides its numbers.
    pub fn rebuild_unit(&mut self, target: usize, sheet: Sheet, lasting: Vec<Lasting>) {
        let unit = &mut self.units[target];
        unit.set_sheet(sheet);
        *unit.lasting_mut() = lasting;
        // A web that is gone has nothing left to break.
        let rules = &self.content.rules;
        let unit = &mut self.units[target];
        if !unit
            .effects(rules)
            .contains(f4x_def::rules::unit_effect::UnitEffect::Web)
        {
            unit.web_hits = 0;
        }
        // Taking off hit points can leave a unit with none: it is dead (`divergences.md` 45).
        if unit.hit_points() <= 0 && unit.is_active() {
            unit.kill();
            unit.status = crate::Status::Dead;
        }
    }

    pub fn spell_events_for_damage(
        &self,
        target: usize,
        dmg: Option<Damage>,
        types: DamageTypes,
    ) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        match dmg {
            None => {
                // A spell is a ranged attack.
                let by = self.units[target]
                    .defender_state(
                        &self.content.rules,
                        self.field_bonus_for(&self.units[target]),
                    )
                    .immunities
                    .stopped_by(types.with(DamageType::Ranged));
                events.push(BattleEvent::SpellImmune { unit: target, by });
            }
            Some(d) => {
                events.push(BattleEvent::SpellDamaged {
                    unit: target,
                    dealt: d.points,
                    kills: d.kills,
                });
                if !self.units[target].is_active() {
                    events.push(BattleEvent::UnitDied { unit: target });
                }
            }
        }
        events
    }
}
