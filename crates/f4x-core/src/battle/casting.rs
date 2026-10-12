//! A wizard's spells in a battle: what the wizard has to cast with, and what a spell does to
//! the units. The rules for choosing, costing and resolving spells are in `spells.rs`.
//!
//! The wizard casts once each turn, from the battle's start with the whole of their casting
//! skill to spend over the battle. The skill is the most mana (before the range multiplier)
//! the spells of the whole battle can cost; the mana comes out of the reserve, costing more the
//! further the battle is from the wizard's fortress (`range_percent`).

use rand::RngCore;

use super::attack::{kill_figure, spell_attack, unblockable, Combatant, Damage, Mode, Volley};
use super::immunity::{effective_resistance, AttackNature, Realm};
use super::map::{is_wall_corner, Pos};
use super::{Battle, BattleEvent, BattleUnit, Side, Status};
use crate::city::PlayerId;
use crate::rules::effects::DamageTraits;
use crate::rules::stats::{compute, Modifier, Stat};
use crate::rules::SpellId;

/// The most creatures a side can have in a battle: summoning stops here.
pub const MAX_SIDE_UNITS: usize = 9;

/// A damage spell of `strength` on one unit (see [`Battle::spell_damage`]); the unit is changed
/// in place. Overland spells that hurt units (`Game::process_chaos_rifts`) use it too.
pub(crate) fn damage_unit_with_spell(
    unit: &mut BattleUnit,
    strength: i32,
    traits: &DamageTraits,
    nature: AttackNature,
    rng: &mut dyn RngCore,
) -> Option<Damage> {
    let mut state = unit.defender_state();
    let ids = state.enchantments.iter().map(String::as_str);
    let protected = ids.clone().any(|i| i == "righteousness")
        && matches!(nature.realm, Some(Realm::Chaos | Realm::Death));
    if state.immunities.magic || protected {
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
        ignores_defense: traits.ignores_defense,
        mode: Mode::Ranged,
        nature: AttackNature {
            armor_piercing: traits.armor_piercing,
            ..nature
        },
        special: Default::default(),
        sees_through: false,
    };
    let dmg = spell_attack(&spell, volley, &mut state, rng);
    unit.set_from(&state);
    unit.record(dmg);
    Some(dmg)
}

/// A wizard casting in a battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleCaster {
    /// Casting skill left to spend this battle.
    pub skill_left: i32,
    /// Mana in the reserve.
    pub mana: i32,
    /// What each point of skill costs from the reserve, in tenths: 10 is one for one.
    pub range: i32,
    pub cast_this_turn: bool,
    /// The most reserve mana the wizard may still spend in this battle, if limited (the brief
    /// of the strategic AI, `docs/learning-ai.md` section 2). `None` is no limit.
    pub budget: Option<i32>,
}

impl BattleCaster {
    pub fn new(skill: i32, mana: i32, range: i32) -> Self {
        Self {
            skill_left: skill.max(0),
            mana: mana.max(0),
            range: range.max(1),
            cast_this_turn: false,
            budget: None,
        }
    }

    /// Mana from the reserve for `skill` points of casting.
    pub fn mana_for(&self, skill: i32) -> i32 {
        skill * self.range / 10
    }

    /// The most skill a spell can cost now: what is left, and what the reserve pays for.
    pub fn affordable(&self) -> i32 {
        let by_budget = self.budget.map_or(i32::MAX, |b| b.max(0) * 10 / self.range);
        self.skill_left
            .min(self.mana * 10 / self.range)
            .min(by_budget)
    }
}

/// What the cost of casting is multiplied by, in tenths, for a battle `distance` squares from
/// the fortress (`Combat_Casting_Cost_Multiplier`). `None` is a battle on the other plane, or
/// from a wizard with no fortress: as far as it gets. At the fortress itself casting costs
/// half; beyond five squares each five more add half the cost again, up to three times.
/// A Channeler counts as two squares away anywhere but at the fortress.
pub fn range_percent(distance: Option<u32>, channeler: bool) -> i32 {
    let distance = distance.unwrap_or(30);
    if distance == 0 {
        return 5;
    }
    let distance = if channeler { 2 } else { distance };
    10 + 5 * [5, 10, 15, 20].iter().filter(|&&t| distance > t).count() as i32
}

/// What a spell is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellAim {
    None,
    Unit(usize),
    Tile(Pos),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellError {
    /// `Battle::apply` cannot cast: go through the game.
    NeedsGame,
    /// This side has no wizard.
    NoWizard,
    /// The wizard has cast this turn already.
    AlreadyCast,
    NotKnown,
    /// A spell for the map, not the battlefield.
    NotForBattle,
    /// Its effects are not in the game yet.
    Unsupported,
    /// More than the casting skill left, or the reserve, will pay.
    TooExpensive,
    /// Less than the spell costs was put in.
    TooLittle,
    /// Not a unit or square the spell can be cast on.
    BadTarget,
    /// No square for a creature, or the side has as many as it can control.
    NoRoom,
    /// The side has this battlefield spell up already.
    AlreadyUp,
    /// A Spell Ward on the city guards it from the spell's realm, and the caster is attacking it.
    Warded,
}

impl Battle {
    pub fn set_caster(&mut self, side: Side, caster: BattleCaster) {
        self.casters[side.index()] = Some(caster);
    }

    /// Limit the mana `side`'s wizard may spend from the reserve in this battle.
    pub fn set_mana_budget(&mut self, side: Side, budget: Option<u32>) {
        if let Some(c) = self.casters[side.index()].as_mut() {
            c.budget = budget.map(|b| b.min(i32::MAX as u32) as i32);
        }
    }

    pub fn caster(&self, side: Side) -> Option<&BattleCaster> {
        self.casters[side.index()].as_ref()
    }

    /// Whether the active side's wizard may cast now.
    pub fn check_caster(&self) -> Result<BattleCaster, super::BattleError> {
        if self.winner.is_some() {
            return Err(super::BattleError::Over);
        }
        let caster = self
            .caster(self.active)
            .ok_or(super::BattleError::Spell(SpellError::NoWizard))?;
        if caster.cast_this_turn {
            return Err(super::BattleError::Spell(SpellError::AlreadyCast));
        }
        Ok(*caster)
    }

    /// The active wizard casts: `skill` points of casting skill, paid for from the reserve.
    /// Returns the mana taken from the reserve.
    pub fn pay_for_spell(&mut self, skill: i32) -> i32 {
        let Some(caster) = self.casters[self.active.index()].as_mut() else {
            return 0;
        };
        let mana = caster.mana_for(skill).min(caster.mana);
        caster.skill_left -= skill;
        caster.mana -= mana;
        if let Some(b) = caster.budget.as_mut() {
            *b = (*b - mana).max(0);
        }
        caster.cast_this_turn = true;
        mana
    }

    /// Whether a creature can be summoned onto `cell` for `side`: a free, open square on its
    /// half of the field.
    pub fn is_summon_square(&self, side: Side, cell: Pos) -> bool {
        let own_half = match side {
            Side::Attacker => cell.0 <= super::map::GRID_W / 2,
            Side::Defender => cell.0 >= super::map::GRID_W / 2,
        };
        own_half
            && self.map.enter_cost(cell, false).is_some()
            && !(is_wall_corner(cell) && self.map.has_wall(cell))
            && self.unit_at(cell).is_none()
    }

    /// A free summoning square for `side`, nearest its edge first (for an AI, or a default).
    pub fn first_summon_square(&self, side: Side) -> Option<Pos> {
        (0..super::map::GRID_H)
            .flat_map(|y| (0..super::map::GRID_W).map(move |x| (x, y)))
            .filter(|&c| self.is_summon_square(side, c))
            .min_by_key(|&(x, y)| {
                let edge = match side {
                    Side::Attacker => x,
                    Side::Defender => super::map::GRID_W - 1 - x,
                };
                (edge, (y - super::map::GRID_H / 2).abs(), y)
            })
    }

    /// Put a new creature on the field for `side`. Returns its id.
    pub fn add_summoned(&mut self, side: Side, mut unit: BattleUnit, cell: Pos) -> usize {
        let id = self.units.len();
        unit.id = id;
        unit.side = side;
        unit.cell = cell;
        unit.source = None;
        // It arrives ready to act, as the original's summons do.
        unit.moves_left = unit.moves;
        unit.attacked = false;
        unit.status = Status::Active;
        self.units.push(unit);
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
        let rng: &mut dyn RngCore = &mut self.rng;
        damage_unit_with_spell(&mut self.units[target], strength, traits, nature, rng)
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
        immune: &[String],
        below: Option<i32>,
    ) -> Option<Damage> {
        let unit = &self.units[target];
        if immune.iter().any(|i| unit.has(i)) {
            return None;
        }
        let mut state = unit.defender_state();
        let dmg = if let Some(limit) = below {
            let ids: Vec<&str> = state.enchantments.iter().map(String::as_str).collect();
            let res = effective_resistance(state.resistance, &state.immunities, Some(realm), &ids);
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
                if state.resistance_failure(Some(realm), modifier, rng) > 0 {
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

    /// Heal a unit by `points`, bringing dead figures back; returns the hit points it gained.
    pub fn spell_heal(&mut self, target: usize, points: i32) -> i32 {
        let before = self.units[target].hit_points();
        let mut state = self.units[target].defender_state();
        state.heal(points);
        self.units[target].set_from(&state);
        self.units[target].hit_points() - before
    }

    /// Put a lasting spell on a unit: its modifiers and abilities change the unit's numbers
    /// until the battle ends or a dispel takes the spell off.
    pub fn enchant_unit(
        &mut self,
        target: usize,
        spell: SpellId,
        id: &str,
        caster: PlayerId,
        mods: &[Modifier],
        grants: &[String],
    ) {
        let unit = &mut self.units[target];
        // The base is the unit without the battlefield spells' changes too.
        unit.take_off_field();
        if unit.base.is_none() {
            let mut base = unit.clone();
            base.base = None;
            unit.base = Some(Box::new(base));
        }
        unit.combat_spells.push((spell, caster));
        unit.enchantments.push(id.to_string());
        unit.apply_lasting(mods, grants);
    }

    /// Take a unit back to `base` (its numbers from before any combat spell, brought up to date
    /// by the game) and put on the spells it still has, each with its modifiers and abilities
    /// from `lasting`, in step with `combat_spells`.
    pub fn rebuild_unit(
        &mut self,
        target: usize,
        base: BattleUnit,
        kept: Vec<(SpellId, PlayerId)>,
        lasting: &[(String, Vec<Modifier>, Vec<String>)],
    ) {
        let unit = &mut self.units[target];
        unit.restore_stats_from(&base);
        unit.field = Default::default();
        unit.base = Some(Box::new(base));
        unit.combat_spells = kept;
        for (id, mods, grants) in lasting {
            unit.enchantments.push(id.clone());
            unit.apply_lasting(mods, grants);
        }
        if unit.combat_spells.is_empty() {
            unit.base = None;
        }
    }

    pub(super) fn spell_events_for_damage(
        &self,
        target: usize,
        dmg: Option<Damage>,
    ) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        match dmg {
            None => events.push(BattleEvent::SpellImmune { unit: target }),
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

impl BattleUnit {
    /// Change the unit's numbers by a lasting spell's modifiers, and give it its abilities.
    pub fn apply_lasting(&mut self, mods: &[Modifier], grants: &[String]) {
        let comp = |stat: Stat, base: i32| compute(stat, base, mods);
        self.melee = comp(Stat::Melee, self.melee);
        self.ranged = comp(Stat::Ranged, self.ranged);
        self.defense = comp(Stat::Defense, self.defense);
        self.resistance = comp(Stat::Resistance, self.resistance);
        // To hit is kept in pluses (ten points each).
        self.to_hit = comp(Stat::ToHit, self.to_hit * 10).div_euclid(10);
        self.hits = comp(Stat::MaxHp, self.hits).max(1);
        let old = self.moves;
        // Moves are whole moves in the spell data and half-moves here.
        self.moves = comp(Stat::Moves, (self.moves / 2) as i32).max(0) as u32 * 2;
        if self.moves >= old {
            self.moves_left += self.moves - old;
        } else {
            self.moves_left = self.moves_left.min(self.moves);
        }
        for g in grants {
            if !self.has(g) {
                self.abilities.push(g.clone());
            }
        }
        self.sync_abilities();
    }

    /// The flags the battle reads, brought in line with the abilities.
    fn sync_abilities(&mut self) {
        self.first_strike |= self.has("first_strike");
        self.negate_first_strike |= self.has("negate_first_strike");
        self.long_range |= self.has("long_range");
        self.magical_weapon |= self.has("magical_weapon");
        self.flies |= self.has("flying");
    }

    /// Take the numbers (not the damage or the position) from `base`.
    fn restore_stats_from(&mut self, base: &BattleUnit) {
        self.melee = base.melee;
        self.ranged = base.ranged;
        self.defense = base.defense;
        self.resistance = base.resistance;
        self.to_hit = base.to_hit;
        self.to_block = base.to_block;
        self.hits = base.hits;
        self.moves = base.moves;
        self.moves_left = self.moves_left.min(base.moves);
        self.flies = base.flies;
        self.first_strike = base.first_strike;
        self.negate_first_strike = base.negate_first_strike;
        self.long_range = base.long_range;
        self.magical_weapon = base.magical_weapon;
        self.special_level = base.special_level;
        self.enchantments = base.enchantments.clone();
        self.abilities = base.abilities.clone();
        self.ranged_kind = base.ranged_kind;
    }
}
