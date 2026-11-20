//! What lasting spells do to a unit's numbers in a battle: the base it started with, the list of
//! spells on it and the recomputation of its numbers.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::half_int::HalfInt;
use f4x_def::id::PlayerId;
use f4x_def::rules::damage_type::RangedKind;
use f4x_def::rules::stats::{compute, compute_half, Modifier, Stat};
use f4x_def::rules::unit_effect::UnitEffect;
use f4x_def::rules::unit_flag::{UnitFlag, UnitFlags};
use f4x_def::rules::{Ability, Rules, SpellId};

use super::{BattleUnit, Status};
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
    pub flags: UnitFlags,
    pub special_level: i32,
    pub ranged_kind: RangedKind,
    pub abilities: Vec<Ability>,
    /// The lasting spells it came into the battle with (from the overland unit), and what they do.
    pub enchantments: Vec<SpellId>,
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
            flags: unit.flags,
            special_level: unit.special_level,
            ranged_kind: unit.ranged_kind,
            abilities: unit.abilities.clone(),
            enchantments: unit.enchantments.clone(),
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
    pub mods: Vec<Modifier>,
    pub grants: Vec<Ability>,
}

impl BattleUnit {
    /// A web closes round the unit: whole again, and the unit loses what it had left of its turn.
    pub fn web_closes(&mut self, web_hits: i32) {
        self.web_hits = web_hits;
        self.moves_left = HalfInt::ZERO;
        self.attacked = true;
    }

    /// The lasting spells of the battle on the unit, and who cast each, in the order cast.
    pub fn combat_spells(&self) -> Vec<(SpellId, PlayerId)> {
        self.lasting.iter().map(|l| (l.spell, l.caster)).collect()
    }

    /// Keep what the unit started the battle with, once, before anything changes its numbers:
    /// no lasting spell, no battlefield spell (`Battle::refresh_fields` keeps it first) and no
    /// extra hit points of life steal, which the unit keeps whatever happens to its spells.
    pub fn keep_base(&mut self) {
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
    pub fn recompute(&mut self, rules: &Rules) {
        let Some(base) = self.base.clone() else {
            return;
        };
        let (ceiling, hit_points) = (self.hit_point_ceiling(), self.hit_points());
        let moves_before = self.moves;
        self.melee = base.melee;
        self.ranged = base.ranged;
        self.defense = base.defense;
        self.resistance = base.resistance;
        self.to_hit = base.to_hit;
        self.to_block = base.to_block;
        self.hits = base.hits;
        self.moves = base.moves;
        self.flags = base.flags;
        self.special_level = base.special_level;
        self.enchantments = base.enchantments.clone();
        self.abilities = base.abilities.clone();
        self.ranged_kind = base.ranged_kind;
        for l in std::mem::take(&mut self.lasting) {
            self.enchantments.push(l.spell);
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
        self.sync_abilities(rules);
        // A web that is gone is gone with its hit points.
        if !self.effects(rules).contains(UnitEffect::Web) {
            self.web_hits = 0;
        }
        self.settle_hit_points(ceiling, hit_points);
    }

    /// One lasting spell's modifiers and abilities on the numbers worked out so far.
    pub fn apply(&mut self, mods: &[Modifier], grants: &[Ability]) {
        let comp = |stat: Stat, base: i32| compute(stat, base, mods);
        self.melee = comp(Stat::Melee, self.melee);
        self.ranged = comp(Stat::Ranged, self.ranged);
        self.defense = comp(Stat::Defense, self.defense);
        self.resistance = comp(Stat::Resistance, self.resistance);
        self.to_hit = comp(Stat::ToHit, self.to_hit);
        self.hits = comp(Stat::MaxHp, self.hits).max(1);
        self.moves = compute_half(Stat::Moves, self.moves, mods).max(HalfInt::ZERO);
        for g in grants {
            if !self.has(*g) {
                self.abilities.push(*g);
            }
        }
    }

    /// The most hit points the unit can have over all its figures.
    pub fn hit_point_ceiling(&self) -> i32 {
        self.figures_start as i32 * self.hits
    }

    /// The unit's hit points were `old_hp` of a ceiling of `old_ceiling` and the ceiling is
    /// lower now (a spell that gave hit points is gone). The damage it has taken stays, so the
    /// hit points fall by as much as the ceiling did; the figures only show what is left. At
    /// zero or less the whole unit is dead (row 138 of the 1.50 notes: the original left such a
    /// unit alive).
    pub fn settle_hit_points(&mut self, old_ceiling: i32, old_hp: i32) {
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
    pub fn sync_abilities(&mut self, rules: &Rules) {
        for &a in &self.abilities {
            for &f in &rules.abilities.get(a).flags {
                self.flags.insert(f);
            }
        }
        // A web or sleep grounds a flyer.
        if self.flags.contains(UnitFlag::Grounds) {
            self.flags = self.flags.without(UnitFlag::Flies);
        }
    }
}
