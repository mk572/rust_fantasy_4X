//! Temporary effects: modifiers and abilities that instant spells and rules put on a unit or
//! a city without an enchantment to carry them.
//!
//! They last until the bearer is gone (the original has no timed effects). Units read them
//! through `modifiers_shared` and `unit_abilities`, cities through `city_modifiers`. Cities have
//! no abilities, so granting or removing one there does nothing.

use crate::rules::effects::{Condition, Effect, Trigger};
use crate::rules::exec::Target;
use crate::rules::stats::passive_modifiers;
use crate::rules::SpellId;
use crate::rules::{Ability, ModOp, Modifier, Source, Stat};
use crate::unit::UnitView;
use crate::world::World;
use f4x_def::rules::immunity::effective_resistance;
use f4x_def::rules::immunity::Immunities;
use f4x_def::rules::realm::Realm;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TempKind {
    Modifier {
        stat: Stat,
        op: ModOp,
        value: f4x_def::half_int::HalfInt,
    },
    Grant(Ability),
    Remove(Ability),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TempEffect {
    pub kind: TempKind,
}

/// The temporary effects on one unit or city.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TempEffects(pub Vec<TempEffect>);

/// Put an effect on a unit or city (cities take only modifiers).
fn push_temp(ecs: &mut hecs::World, who: Target, kind: TempKind) {
    let e = match who {
        Target::Unit(e) | Target::City(e) if ecs.contains(e) => e,
        _ => return,
    };
    if matches!(who, Target::City(_)) && !matches!(kind, TempKind::Modifier { .. }) {
        return;
    }
    let effect = TempEffect { kind };
    if let Ok(mut list) = ecs.get::<&mut TempEffects>(e) {
        list.0.push(effect);
        return;
    }
    crate::world::put_in(ecs, e, TempEffects(vec![effect]));
}

/// Put a stat modifier on a unit or city for good.
pub(crate) fn add_modifier(ecs: &mut hecs::World, who: Target, m: Modifier) {
    let kind = TempKind::Modifier {
        stat: m.stat,
        op: m.op,
        value: m.value,
    };
    push_temp(ecs, who, kind);
}

impl<'a> UnitView<'a> {
    /// The temporary effects on a unit or city.
    pub fn temp_effects(&self, e: hecs::Entity) -> Vec<TempEffect> {
        self.ecs
            .get::<&TempEffects>(e)
            .map(|t| t.0.clone())
            .unwrap_or_default()
    }

    /// The temporary modifiers on a unit or city.
    pub(crate) fn temp_modifiers(&self, e: hecs::Entity) -> Vec<Modifier> {
        let Ok(list) = self.ecs.get::<&TempEffects>(e) else {
            return Vec::new();
        };
        list.0
            .iter()
            .filter_map(|t| match t.kind {
                TempKind::Modifier { stat, op, value } => {
                    Some(Modifier::amount(stat, op, value, Source::Other))
                }
                _ => None,
            })
            .collect()
    }

    /// The abilities that come to the unit from its items and from lasting spells, and are not
    /// its own: the original counts their special attacks as not innate (`Combat.c` 12573).
    pub fn unit_granted_abilities(&self, unit: hecs::Entity) -> Vec<Ability> {
        let own = self
            .unit_def(unit)
            .map(|d| d.abilities.clone())
            .unwrap_or_default();
        let hero = self.ecs.get::<&crate::hero::Hero>(unit).ok();
        let mut out: Vec<Ability> = Vec::new();
        let mut take = |a: Ability| {
            let innate = own.contains(&a)
                || hero
                    .as_ref()
                    .is_some_and(|h| h.abilities.ids().contains(&a));
            if !innate && !out.contains(&a) {
                out.push(a);
            }
        };
        let spells = crate::spell::instance::Spells::new(self.ecs)
            .unit_enchantments(unit)
            .into_iter()
            .map(|(s, _)| s)
            .chain(self.items().spells(unit));
        for spell in spells {
            for rule in &self.content.rules.spells.get(spell).effects {
                if let (Trigger::Passive, Condition::Always, Effect::GrantAbility(a)) =
                    (rule.when, &rule.condition, &rule.effect)
                {
                    take(*a);
                }
            }
        }
        for a in self.items().abilities(unit) {
            take(a);
        }
        out
    }

    /// The unit's abilities now: its own, those its lasting spells grant (Magic Immunity,
    /// Guardian Wind), plus temporary grants, minus temporary removals. Temporary effects apply
    /// in the order they were added, so a later grant restores what an earlier removal took.
    pub fn unit_abilities(&self, unit: hecs::Entity) -> Vec<Ability> {
        self.abilities_shared(unit).as_ref().clone()
    }

    /// [`Self::unit_abilities`], shared: kept for a computer wizard's turn (see `unit::kept`).
    pub(crate) fn abilities_shared(&self, unit: hecs::Entity) -> std::rc::Rc<Vec<Ability>> {
        if let Some(kept) = crate::unit::kept::abilities(self.ecs, unit) {
            if crate::unit::kept::checking() {
                assert_eq!(
                    *kept,
                    self.compute_abilities(unit),
                    "kept abilities of {unit:?}"
                );
            }
            return kept;
        }
        let list = std::rc::Rc::new(self.compute_abilities(unit));
        crate::unit::kept::keep_abilities(self.ecs, unit, &list);
        list
    }

    fn compute_abilities(&self, unit: hecs::Entity) -> Vec<Ability> {
        let mut out = self
            .unit_def(unit)
            .map(|d| d.abilities.clone())
            .unwrap_or_default();
        if let Ok(hero) = self.ecs.get::<&crate::hero::Hero>(unit) {
            for a in hero.abilities.ids() {
                if !out.contains(&a) {
                    out.push(a);
                }
            }
        }
        for (spell, _) in crate::spell::instance::Spells::new(self.ecs).unit_enchantments(unit) {
            for rule in &self.content.rules.spells.get(spell).effects {
                if let (Trigger::Passive, Condition::Always, Effect::GrantAbility(a)) =
                    (rule.when, &rule.condition, &rule.effect)
                {
                    if !out.contains(a) {
                        out.push(*a);
                    }
                }
            }
        }
        for a in self.items().abilities(unit) {
            if !out.contains(&a) {
                out.push(a);
            }
        }
        for spell in self.items().spells(unit) {
            for rule in &self.content.rules.spells.get(spell).effects {
                if let (Trigger::Passive, Condition::Always, Effect::GrantAbility(a)) =
                    (rule.when, &rule.condition, &rule.effect)
                {
                    if !out.contains(a) {
                        out.push(*a);
                    }
                }
            }
        }
        if self.unit_counts_as_undead(unit) {
            for &n in &self.content.rules.config.abilities.undead_immunities {
                let a = Ability::new(n);
                if !out.contains(&a) {
                    out.push(a);
                }
            }
        }
        if let Ok(list) = self.ecs.get::<&TempEffects>(unit) {
            for t in &list.0 {
                match &t.kind {
                    TempKind::Grant(a) if !out.contains(a) => out.push(*a),
                    TempKind::Remove(a) => out.retain(|x| x != a),
                    _ => {}
                }
            }
        }
        out
    }

    /// Whether the unit's normal attacks count as magical, so weapon immunity does not stop
    /// them: heroes, fantastic and summoned creatures, units trained with magic, mithril or
    /// adamantium weapons, anything with the `magical_weapon` ability (a flaming or blessed
    /// blade), and the living normal units of a wizard who keeps Holy Arms.
    pub fn unit_has_magical_weapon(&self, unit: hecs::Entity) -> bool {
        self.unit_has_innate_magical_weapon(unit)
            || (self.unit_owner(unit).is_some_and(|o| {
                self.has_global_effect(o, crate::spell::global_enchantment::GlobalEffect::HolyArms)
            }) && !self.unit_is_undead(unit))
    }

    /// [`Self::unit_has_magical_weapon`] without what its owner's global enchantments give (Holy
    /// Arms): a battle works that out itself.
    pub fn unit_has_innate_magical_weapon(&self, unit: hecs::Entity) -> bool {
        self.unit_def(unit)
            .is_some_and(|d| d.innate_magical_weapon())
            || self.unit_weapon_quality(unit) > 0
            || self
                .flags_of(&self.unit_abilities(unit))
                .contains(crate::rules::unit_flag::UnitFlag::MagicalWeapon)
    }

    /// The ids of the lasting spells on a unit.
    pub(crate) fn unit_enchantment_ids(&self, unit: hecs::Entity) -> Vec<SpellId> {
        let mut ids: Vec<SpellId> = self
            .unit_enchantments(unit)
            .iter()
            .filter(|&&(spell, _)| !self.unit_spell_superseded(unit, spell))
            .map(|&(spell, _)| spell)
            .collect();
        // What its items put on the hero counts as well.
        for spell in self.items().spells(unit) {
            if self.unit_spell_superseded(unit, spell) {
                continue;
            }
            if !ids.contains(&spell) {
                ids.push(spell);
            }
        }
        ids
    }

    /// The unit's resistance against a spell or attack of `realm`: its own, plus what magic
    /// immunity and its lasting spells add against that realm (`battle::immunity`).
    pub fn unit_resistance_against(&self, unit: hecs::Entity, realm: Option<Realm>) -> i32 {
        effective_resistance(
            self.unit_stat(unit, Stat::Resistance),
            &Immunities::of(
                &self.unit_abilities(unit),
                &self.unit_enchantment_ids(unit),
                &self.content.rules,
            ),
            realm,
        )
    }

    /// Whether the unit cannot be hurt by spells of `realm` at all: Righteousness against
    /// chaos and death.
    pub fn unit_protected_from(&self, unit: hecs::Entity, realm: Option<Realm>) -> bool {
        realm.is_some_and(|realm| {
            Immunities::of(
                &self.unit_abilities(unit),
                &self.unit_enchantment_ids(unit),
                &self.content.rules,
            )
            .stops(crate::rules::damage_type::DamageTypes::of(&[
                realm.damage_type()
            ]))
        })
    }

    /// What the lasting spells on the unit do (the `unit_effect` hook).
    pub fn unit_effects(&self, unit: hecs::Entity) -> crate::spell::unit_effect::UnitEffects {
        let mut set = crate::spell::unit_effect::UnitEffects::NONE;
        for spell in self.unit_enchantment_ids(unit) {
            set.union(crate::spell::unit_effect::UnitEffects::of(
                self.content.rules.spells.get(spell),
            ));
        }
        set
    }

    /// The passive modifiers of every ability the unit has now.
    pub(crate) fn ability_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let rules = &self.content.rules;
        let mut mods = Vec::new();
        for id in self.unit_abilities(unit) {
            mods.extend(passive_modifiers(
                &rules.abilities.get(id).effects,
                Source::Ability(id),
            ));
        }
        mods
    }
}

impl World {
    /// Put a stat modifier on a unit or city for good.
    pub fn add_temp_modifier(&mut self, who: Target, m: Modifier) {
        add_modifier(&mut self.ecs, who, m);
    }

    /// Give a unit an ability it does not have, for good.
    pub fn grant_temp_ability(&mut self, who: Target, ability: Ability) {
        push_temp(&mut self.ecs, who, TempKind::Grant(ability));
    }

    /// Take an ability away from a unit for good.
    pub fn remove_temp_ability(&mut self, who: Target, ability: Ability) {
        push_temp(&mut self.ecs, who, TempKind::Remove(ability));
    }
}
