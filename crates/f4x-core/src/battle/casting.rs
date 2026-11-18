//! A wizard's spells in a battle: what the wizard has to cast with, and what a spell does to
//! the units. The rules for choosing, costing and resolving spells are in `spells.rs`.
//!
//! The wizard casts once each turn, from the battle's start with the whole of their casting
//! skill to spend over the battle. The skill is the most mana (before the range multiplier)
//! the spells of the whole battle can cost; the mana comes out of the reserve, costing more the
//! further the battle is from the wizard's fortress (`range_percent`).
//!
//! Tests: in battle/tests.rs.

use crate::rules::Ability;
use crate::spell::unit_effect::{UnitEffect, UnitEffects};
use f4x_def::half_int::HalfInt;
use rand::RngCore;

use super::attack::{kill_figure, spell_attack, unblockable, Combatant, Damage, Mode, Volley};
use super::immunity::{effective_resistance, AttackNature, Realm};
use super::map::{is_wall_corner, Pos};
use super::{Battle, BattleEvent, BattleUnit, RangedKind, Side, Status};
use crate::player::PlayerId;
use crate::rules::effects::DamageTraits;
use crate::rules::stats::{compute, Modifier, Stat};
use crate::rules::SpellId;

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
    let protected = state.effects.contains(UnitEffect::Righteousness)
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
        ..Combatant::default()
    };
    let dmg = spell_attack(&spell, volley, &mut state, rng);
    unit.set_from(&state);
    unit.record(dmg);
    Some(dmg)
}

/// A wizard casting in a battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BattleCaster {
    /// Casting skill left to spend this battle.
    pub skill_left: i32,
    /// Mana in the reserve (halves count: a spell of 5 at x1.5 costs 7½).
    pub mana: HalfInt,
    /// What each point of skill costs from the reserve, in tenths: 10 is one for one.
    pub range: i32,
    pub cast_this_turn: bool,
    /// The most reserve mana the wizard may still spend in this battle, if limited (the brief
    /// of the strategic AI, `docs/learning-ai.md` section 2). `None` is no limit.
    pub budget: Option<HalfInt>,
}

/// What a unit starts a battle with, before any lasting spell cast in the battle: the numbers
/// its lasting spells are added to. They are kept for as long as the unit has such a spell, and
/// no spell changes them (`BattleUnit::base`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BaseStats {
    pub melee: i32,
    pub ranged: i32,
    pub defense: i32,
    pub resistance: i32,
    pub to_hit: i32,
    pub to_block: i32,
    /// Hit points of one figure, without the extra ones life steal gave.
    pub hits: i32,
    pub moves: HalfInt,
    pub flies: bool,
    pub first_strike: bool,
    pub negate_first_strike: bool,
    pub long_range: bool,
    pub magical_weapon: bool,
    pub special_level: i32,
    pub ranged_kind: RangedKind,
    pub abilities: Vec<Ability>,
    /// The lasting spells it came into the battle with (from the overland unit), and what they do.
    pub enchantments: Vec<SpellId>,
    pub effects: UnitEffects,
}

impl BaseStats {
    /// The numbers of `unit` as they are now.
    pub fn of(unit: &BattleUnit) -> Self {
        Self {
            melee: unit.melee,
            ranged: unit.ranged,
            defense: unit.defense,
            resistance: unit.resistance,
            to_hit: unit.to_hit,
            to_block: unit.to_block,
            hits: unit.hits - unit.extra_hits,
            moves: unit.moves,
            flies: unit.flies,
            first_strike: unit.first_strike,
            negate_first_strike: unit.negate_first_strike,
            long_range: unit.long_range,
            magical_weapon: unit.magical_weapon,
            special_level: unit.special_level,
            ranged_kind: unit.ranged_kind,
            abilities: unit.abilities.clone(),
            enchantments: unit.enchantments.clone(),
            effects: unit.effects,
        }
    }
}

/// A lasting spell cast on a unit in the battle, with what it does to the unit's numbers: the
/// unit's list of these, in the order cast, is all that is added to its base
/// (`BattleUnit::lasting`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Lasting {
    pub spell: SpellId,
    pub caster: PlayerId,
    pub effects: UnitEffects,
    pub mods: Vec<Modifier>,
    pub grants: Vec<Ability>,
}

impl BattleCaster {
    /// A caster with a whole number of mana in the reserve.
    pub fn new(skill: i32, mana: i32, range: i32) -> Self {
        Self::with_mana(skill, HalfInt::from_int(mana.max(0)), range)
    }

    /// A caster whose reserve may hold half a point.
    pub fn with_mana(skill: i32, mana: HalfInt, range: i32) -> Self {
        Self {
            skill_left: skill.max(0),
            mana: if mana < 0 { HalfInt::from_int(0) } else { mana },
            range: range.max(1),
            cast_this_turn: false,
            budget: None,
        }
    }

    /// Mana from the reserve for `skill` points of casting, exact: `range` is a multiple of 5
    /// tenths, so the price is a whole or a half number.
    pub fn mana_for(&self, skill: i32) -> HalfInt {
        HalfInt::from_raw(skill * self.range / 5)
    }

    /// The most skill a spell can cost now: what is left, and what the reserve pays for.
    pub fn affordable(&self) -> i32 {
        // The most skill whose price in mana the reserve (or the budget) covers.
        let skill_for = |mana: HalfInt| mana.raw().max(0) * 5 / self.range;
        let by_budget = self.budget.map_or(i32::MAX, skill_for);
        self.skill_left.min(skill_for(self.mana)).min(by_budget)
    }
}

/// What the cost of casting is multiplied by, in tenths, for a battle `distance` squares from
/// the fortress (`Combat_Casting_Cost_Multiplier`). `None` is a battle on the other plane, or
/// from a wizard with no fortress: as far as it gets. At the fortress itself casting costs
/// half; beyond five squares each five more add half the cost again, up to three times.
/// `counted` turns the distance into the one the wizard's retorts make of it (a Channeler counts
/// as two squares away anywhere but at the fortress: `Stat::CastingDistance`).
pub fn range_percent(
    range: &f4x_def::rules::CastingRange,
    distance: Option<u32>,
    counted: impl FnOnce(i32) -> i32,
) -> i32 {
    let distance = distance.unwrap_or(range.far);
    if distance == 0 {
        return range.at_fortress;
    }
    let distance = u32::try_from(counted(i32::try_from(distance).unwrap_or(i32::MAX))).unwrap_or(0);
    range.base + range.step * range.beyond.iter().filter(|&&t| distance > t).count() as i32
}

/// What a spell is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Not a unit or square the spell can be cast on, and why.
    Target(crate::spell::target_error::TargetError),
    /// No square for a creature, or the side has as many as it can control.
    NoRoom,
    /// The side has this battlefield spell up already.
    AlreadyUp,
    /// The unit has this lasting spell on it already.
    AlreadyEnchanted,
    /// The spell is not cumulative with one the unit has (or the wizard keeps).
    NotCumulative,
    /// The unit has the ability the spell would give it.
    AlreadyAble,
    /// A Spell Ward on the city guards it from the spell's realm, and the caster is attacking it.
    Warded,
}

impl From<crate::spell::stacking::Stacking> for SpellError {
    fn from(why: crate::spell::stacking::Stacking) -> Self {
        use crate::spell::stacking::Stacking;
        match why {
            Stacking::AlreadyEnchanted => Self::AlreadyEnchanted,
            Stacking::NotCumulative => Self::NotCumulative,
            Stacking::AlreadyAble => Self::AlreadyAble,
        }
    }
}

impl SpellError {
    /// Refused only because the spell would do nothing: the caster may confirm and cast anyway.
    pub fn is_pointless(self) -> bool {
        matches!(self, Self::AlreadyEnchanted | Self::AlreadyAble)
    }
}

impl SpellError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::NeedsGame => "refuse_battle.spell_error_needs_game",
            Self::NoWizard => "refuse_battle.spell_error_no_wizard",
            Self::AlreadyCast => "refuse_battle.spell_error_already_cast",
            Self::NotKnown => "refuse_battle.spell_error_not_known",
            Self::NotForBattle => "refuse_battle.spell_error_not_for_battle",
            Self::Unsupported => "refuse_battle.spell_error_unsupported",
            Self::TooExpensive => "refuse_battle.spell_error_too_expensive",
            Self::TooLittle => "refuse_battle.spell_error_too_little",
            Self::Target(e) => e.key(),
            Self::NoRoom => "refuse_battle.spell_error_no_room",
            Self::AlreadyUp => "refuse_battle.spell_error_already_up",
            Self::AlreadyEnchanted => "refuse_battle.spell_error_already_enchanted",
            Self::NotCumulative => "refuse_battle.spell_error_not_cumulative",
            Self::AlreadyAble => "refuse_battle.spell_error_already_able",
            Self::Warded => "refuse_battle.spell_error_warded",
        }
    }
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
        self.refresh_fields();
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
        immune: &[Ability],
        below: Option<i32>,
    ) -> Option<Damage> {
        let unit = &self.units[target];
        if immune.iter().any(|&i| unit.has(i)) {
            return None;
        }
        let mut state = unit.defender_state();
        let dmg = if let Some(limit) = below {
            let res = effective_resistance(
                state.resistance,
                &state.immunities,
                Some(realm),
                state.effects,
            );
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

    /// Destroy a unit outright, no defence or resistance helping (Cracks Call). Nothing is left
    /// to raise.
    pub fn destroy_unit(&mut self, target: usize) -> Damage {
        let mut state = self.units[target].defender_state();
        let all = state.hit_points();
        let dmg = unblockable(&mut state, all).all_gone();
        self.units[target].set_from(&state);
        self.units[target].record(dmg);
        dmg
    }

    /// Life Drain: a death resistance check with `modifier` added (the extra mana makes it
    /// harder to resist), and the unit loses that many hit points as drained life, which can
    /// raise it as undead. `None` if the unit is immune: a death creature, or immune to death.
    pub fn drain_life(&mut self, target: usize, modifier: i32) -> Option<Damage> {
        let unit = &self.units[target];
        if unit.realm == Some(Realm::Death) || unit.has(Ability::new(16)) {
            return None;
        }
        let mut state = unit.defender_state();
        let rng: &mut dyn RngCore = &mut self.rng;
        let amount =
            state.resistance_failure(Some(Realm::Death), modifier - self.save_penalty, rng);
        let dmg = unblockable(&mut state, amount).all_drained();
        self.units[target].set_from(&state);
        self.units[target].record(dmg);
        Some(dmg)
    }

    /// Turn the 5 by 5 squares round `center` to mud (Earth to Mud).
    pub fn spread_mud(&mut self, center: Pos) {
        for dy in -2..=2 {
            for dx in -2..=2 {
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
        u.moves_left = HalfInt::ZERO;
        u.attacked = true;
    }

    /// Take a bound unit back to the side it belongs to.
    pub fn return_unit(&mut self, target: usize) {
        let u = &mut self.units[target];
        if let Some(owner) = u.bound_from.take() {
            u.side = owner;
            u.moves_left = HalfInt::ZERO;
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
        effects: UnitEffects,
        mods: &[Modifier],
        grants: &[Ability],
    ) {
        let unit = &mut self.units[target];
        let webbing = effects.contains(UnitEffect::Web);
        // A web cast again on a unit that has it (its web broken) only closes round it anew.
        if webbing && unit.enchantments.contains(&spell) {
            unit.web_closes();
            return;
        }
        unit.keep_base();
        // Sleep ends the unit's turn.
        if effects.contains(UnitEffect::BlackSleep) {
            unit.attacked = true;
        }
        unit.lasting.push(Lasting {
            spell,
            caster,
            effects,
            mods: mods.to_vec(),
            grants: grants.to_vec(),
        });
        unit.recompute();
        if webbing {
            unit.web_closes();
        }
    }

    /// Give a unit the numbers it starts the battle with (`base`, brought up to date by the game
    /// when the unit it was made from changed) and the lasting spells it still has, `lasting`:
    /// what a dispel leaves. Nothing but the list and the base decides the numbers.
    pub fn rebuild_unit(&mut self, target: usize, base: BaseStats, lasting: Vec<Lasting>) {
        let unit = &mut self.units[target];
        unit.base = Some(base);
        unit.lasting = lasting;
        unit.recompute();
        // With no spell and no field left, nothing changes its numbers: it is as it was made.
        if unit.lasting.is_empty() && unit.field == Default::default() {
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
    /// A web closes round the unit: whole again, and the unit loses what it had left of its turn.
    pub(super) fn web_closes(&mut self) {
        self.web_hits = super::WEB_HITS;
        self.moves_left = HalfInt::ZERO;
        self.attacked = true;
        self.extra_attack = false;
    }

    /// The lasting spells of the battle on the unit, and who cast each, in the order cast.
    pub fn combat_spells(&self) -> Vec<(SpellId, PlayerId)> {
        self.lasting.iter().map(|l| (l.spell, l.caster)).collect()
    }

    /// Keep what the unit started the battle with, once, before anything changes its numbers:
    /// no lasting spell, no battlefield spell (`Battle::refresh_fields` keeps it first) and no
    /// extra hit points of life steal, which the unit keeps whatever happens to its spells.
    pub(super) fn keep_base(&mut self) {
        if self.base.is_none() {
            self.base = Some(BaseStats::of(self));
        }
    }

    /// Work the unit's numbers out again from what it started the battle with, its lasting
    /// spells and what the battlefield spells ask of it (`field`, set by `refresh_fields`). The
    /// numbers are a function of those three and nothing else, so whatever a dispel or a field
    /// spell going down takes off, the rest is untouched, and nothing is remembered to be taken
    /// off again. The damage taken is kept, and the unit is dead if its hit points do not cover
    /// it any longer (`settle_hit_points`).
    pub(super) fn recompute(&mut self) {
        let Some(base) = self.base.clone() else {
            return;
        };
        let (ceiling, hit_points) = (self.hit_point_ceiling(), self.hit_points());
        let moves_before = self.moves;
        let hasted_before = self.has(Ability::new(26));
        self.melee = base.melee;
        self.ranged = base.ranged;
        self.defense = base.defense;
        self.resistance = base.resistance;
        self.to_hit = base.to_hit;
        self.to_block = base.to_block;
        self.hits = base.hits;
        self.moves = base.moves;
        self.flies = base.flies;
        self.first_strike = base.first_strike;
        self.negate_first_strike = base.negate_first_strike;
        self.long_range = base.long_range;
        self.magical_weapon = base.magical_weapon;
        self.special_level = base.special_level;
        self.enchantments = base.enchantments.clone();
        self.effects = base.effects;
        self.abilities = base.abilities.clone();
        self.ranged_kind = base.ranged_kind;
        for l in std::mem::take(&mut self.lasting) {
            self.enchantments.push(l.spell);
            self.effects.union(l.effects);
            self.apply(&l.mods, &l.grants);
            self.lasting.push(l);
        }
        // Life steal's extra hit points are no spell's: they stay.
        self.hits += self.extra_hits;
        self.apply_field();
        // What is left of this turn's moves follows the change in the moves of the turn.
        if self.moves >= moves_before {
            self.moves_left += self.moves - moves_before;
        } else {
            self.moves_left = self.moves_left.min(self.moves);
        }
        // Haste: a unit that has not attacked yet may attack twice the turn it gets it; without
        // it, never.
        if !self.has(Ability::new(26)) {
            self.extra_attack = false;
        } else if !hasted_before && !self.attacked {
            self.extra_attack = true;
        }
        self.sync_abilities();
        // A web that is gone is gone with its hit points.
        if !self.effects.contains(UnitEffect::Web) {
            self.web_hits = 0;
        }
        self.settle_hit_points(ceiling, hit_points);
    }

    /// One lasting spell's modifiers and abilities on the numbers worked out so far.
    fn apply(&mut self, mods: &[Modifier], grants: &[Ability]) {
        let comp = |stat: Stat, base: i32| compute(stat, base, mods);
        self.melee = comp(Stat::Melee, self.melee);
        self.ranged = comp(Stat::Ranged, self.ranged);
        self.defense = comp(Stat::Defense, self.defense);
        self.resistance = comp(Stat::Resistance, self.resistance);
        self.to_hit = comp(Stat::ToHit, self.to_hit);
        self.hits = comp(Stat::MaxHp, self.hits).max(1);
        let old = self.moves;
        // The spell data has whole moves; a half move the unit has stays.
        let whole = comp(Stat::Moves, old.floor()).max(0);
        self.moves = if whole == 0 {
            HalfInt::ZERO
        } else {
            HalfInt::from_int(whole) + HalfInt::from_raw(old.raw() % 2)
        };
        for g in grants {
            if !self.has(*g) {
                self.abilities.push(*g);
            }
        }
    }

    /// The most hit points the unit can have over all its figures.
    pub(super) fn hit_point_ceiling(&self) -> i32 {
        self.figures_start as i32 * self.hits
    }

    /// The unit's hit points were `old_hp` of a ceiling of `old_ceiling` and the ceiling is
    /// lower now (a spell that gave hit points is gone). The damage it has taken stays, so the
    /// hit points fall by as much as the ceiling did; the figures only show what is left. At
    /// zero or less the whole unit is dead (row 138 of the 1.50 notes: the original left such a
    /// unit alive).
    pub(super) fn settle_hit_points(&mut self, old_ceiling: i32, old_hp: i32) {
        let lost = old_ceiling - self.hit_point_ceiling();
        if lost <= 0 || self.figures == 0 {
            return;
        }
        let left = old_hp - lost;
        if left <= 0 {
            self.figures = 0;
            self.front_damage = 0;
            if self.status == Status::Active {
                self.status = Status::Dead;
            }
        } else {
            self.figures = ((left + self.hits - 1) / self.hits) as u32;
            self.front_damage = self.figures as i32 * self.hits - left;
        }
    }

    /// The flags the battle reads, brought in line with the abilities.
    fn sync_abilities(&mut self) {
        self.first_strike |= self.has(Ability::new(23));
        self.negate_first_strike |= self.has(Ability::new(48));
        self.long_range |= self.has(Ability::new(38));
        self.magical_weapon |= self.has(Ability::new(41));
        self.flies |= self.has(Ability::new(24));
        // A web or sleep grounds a flyer.
        if self.has(Ability::new(73)) || self.has(Ability::new(2)) {
            self.flies = false;
        }
    }
}
