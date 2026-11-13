//! Temporary effects: modifiers and abilities that instant spells and rules put on a unit or
//! a city without an enchantment to carry them.
//!
//! Each carries the turns it has left (`None` lasts until the bearer is gone). Units read them
//! through `unit_modifiers` and `unit_abilities`, cities through `city_modifiers`. Cities have
//! no abilities, so granting or removing one there does nothing.

use crate::battle::immunity::{effective_resistance, Immunities, Realm};
use crate::game::Game;
use crate::rules::effects::{Condition, Effect, Trigger};
use crate::rules::exec::Target;
use crate::rules::stats::passive_modifiers;
use crate::rules::SpellId;
use crate::rules::{Ability, ModOp, Modifier, Source, Stat};
use crate::unit::UnitView;
use hecs::World;

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
    /// Turns left, counted down once per turn change; `None` never runs out.
    pub turns: Option<u16>,
}

/// The temporary effects on one unit or city.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TempEffects(pub Vec<TempEffect>);

/// Put an effect on a unit or city (cities take only modifiers).
fn push_temp(world: &mut World, who: Target, kind: TempKind, turns: Option<u16>) {
    let e = match who {
        Target::Unit(e) | Target::City(e) if world.contains(e) => e,
        _ => return,
    };
    if matches!(who, Target::City(_)) && !matches!(kind, TempKind::Modifier { .. }) {
        return;
    }
    let effect = TempEffect { kind, turns };
    if let Ok(mut list) = world.get::<&mut TempEffects>(e) {
        list.0.push(effect);
        return;
    }
    crate::game::put_in(world, e, TempEffects(vec![effect]));
}

/// Put a stat modifier on a unit or city for `turns` turns (`None`: for good).
pub(crate) fn add_modifier(world: &mut World, who: Target, m: Modifier, turns: Option<u16>) {
    let kind = TempKind::Modifier {
        stat: m.stat,
        op: m.op,
        value: m.value,
    };
    push_temp(world, who, kind, turns);
}

/// Give a unit an ability it does not have for `turns` turns (`None`: for good).
pub(crate) fn grant_ability(world: &mut World, who: Target, ability: Ability, turns: Option<u16>) {
    push_temp(world, who, TempKind::Grant(ability), turns);
}

/// Take an ability away from a unit for `turns` turns (`None`: for good).
pub(crate) fn remove_ability(world: &mut World, who: Target, ability: Ability, turns: Option<u16>) {
    push_temp(world, who, TempKind::Remove(ability), turns);
}

/// Count every timed effect down; drop the ones that ran out.
pub(crate) fn tick(world: &mut World) {
    let holders: Vec<hecs::Entity> = world
        .query::<&TempEffects>()
        .iter()
        .map(|(e, _)| e)
        .collect();
    for e in holders {
        let empty = {
            let Ok(mut list) = world.get::<&mut TempEffects>(e) else {
                continue;
            };
            for t in &mut list.0 {
                if let Some(n) = t.turns.as_mut() {
                    *n = n.saturating_sub(1);
                }
            }
            list.0.retain(|t| t.turns != Some(0));
            list.0.is_empty()
        };
        if empty {
            crate::game::strip_in::<TempEffects>(world, e);
        }
    }
}

impl<'a> UnitView<'a> {
    /// The temporary effects on a unit or city.
    pub fn temp_effects(&self, e: hecs::Entity) -> Vec<TempEffect> {
        self.world
            .get::<&TempEffects>(e)
            .map(|t| t.0.clone())
            .unwrap_or_default()
    }

    /// The temporary modifiers on a unit or city.
    pub(crate) fn temp_modifiers(&self, e: hecs::Entity) -> Vec<Modifier> {
        let Ok(list) = self.world.get::<&TempEffects>(e) else {
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
        let hero = self.world.get::<&crate::hero::Hero>(unit).ok();
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
        let spells = crate::spell::instance::Spells::new(self.world)
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
        if let Some(kept) = crate::unit::kept::abilities(self.world, unit) {
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
        crate::unit::kept::keep_abilities(self.world, unit, &list);
        list
    }

    fn compute_abilities(&self, unit: hecs::Entity) -> Vec<Ability> {
        let mut out = self
            .unit_def(unit)
            .map(|d| d.abilities.clone())
            .unwrap_or_default();
        if let Ok(hero) = self.world.get::<&crate::hero::Hero>(unit) {
            for a in hero.abilities.ids() {
                if !out.contains(&a) {
                    out.push(a);
                }
            }
        }
        for (spell, _) in crate::spell::instance::Spells::new(self.world).unit_enchantments(unit) {
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
            for &a in crate::unit::undead::UNDEAD_IMMUNITIES {
                if !out.contains(&a) {
                    out.push(a);
                }
            }
        }
        if let Ok(list) = self.world.get::<&TempEffects>(unit) {
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
        // Ships and catapults are `Fantastic` in the data, as in the original, but they are not
        // creatures: their blows and boulders are stopped by weapon immunity (the Unofficial Patch
        // fixes the original, where they were not).
        self.unit_def(unit).is_some_and(|d| {
            d.kind != crate::rules::UnitKind::Normal
                && !(d.kind == crate::rules::UnitKind::Fantastic
                    && (d.movement == crate::rules::MovementType::Sailing
                        || d.abilities.contains(&Ability::new(70))))
        }) || self.unit_weapon_quality(unit) > 0
            || (self.unit_owner(unit).is_some_and(|o| {
                self.has_global_effect(o, crate::spell::global_enchantment::GlobalEffect::HolyArms)
            }) && !self.unit_is_undead(unit))
            || self
                .unit_abilities(unit)
                .iter()
                .any(|a| *a == Ability::new(41))
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
            &Immunities::of(&self.unit_abilities(unit)),
            realm,
            self.unit_effects(unit),
        )
    }

    /// Whether the unit cannot be hurt by spells of `realm` at all: Righteousness against
    /// chaos and death.
    pub fn unit_protected_from(&self, unit: hecs::Entity, realm: Option<Realm>) -> bool {
        matches!(realm, Some(Realm::Chaos | Realm::Death))
            && self
                .unit_effects(unit)
                .contains(crate::spell::unit_effect::UnitEffect::Righteousness)
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

impl Game {
    /// Put a stat modifier on a unit or city for `turns` turns (`None`: for good).
    pub fn add_temp_modifier(&mut self, who: Target, m: Modifier, turns: Option<u16>) {
        add_modifier(&mut self.world, who, m, turns);
    }

    /// Give a unit an ability it does not have for `turns` turns (`None`: for good).
    pub fn grant_temp_ability(&mut self, who: Target, ability: Ability, turns: Option<u16>) {
        grant_ability(&mut self.world, who, ability, turns);
    }

    /// Take an ability away from a unit for `turns` turns (`None`: for good).
    pub fn remove_temp_ability(&mut self, who: Target, ability: Ability, turns: Option<u16>) {
        remove_ability(&mut self.world, who, ability, turns);
    }

    /// Count every timed effect down; drop the ones that ran out.
    pub(crate) fn tick_temp_effects(&mut self) {
        tick(&mut self.world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::immunity::Realm;
    use crate::game::testutil::{num, shipped};
    use crate::map::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::player::PlayerId;
    use f4x_def::testdir::TestResult;

    const P0: PlayerId = PlayerId(0);
    const AT: Position = Position {
        plane: Plane::Arcanus,
        x: 5,
        y: 5,
    };

    fn world() -> Game {
        let c = shipped();
        let grass = c
            .terrains
            .id_numbered(num::terrain::GRASSLAND)
            .expect("id_of succeeds");
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.add_player(P0, "W");
        g
    }

    fn unit(g: &mut Game, id: u16) -> hecs::Entity {
        let id = g
            .content
            .rules
            .units
            .id_numbered(id)
            .expect("id_of succeeds");
        g.spawn_unit(P0, AT, id)
    }

    fn plus(stat: Stat, value: i32) -> Modifier {
        Modifier::new(stat, ModOp::Add, value, Source::Other)
    }

    #[test]
    fn a_modifier_changes_the_stat_until_it_runs_out() {
        let mut g = world();
        let u = unit(&mut g, 95 /* high_elf_spearmen */);
        let base = g.units().unit_stat(u, Stat::Melee);
        g.add_temp_modifier(Target::Unit(u), plus(Stat::Melee, 3), Some(2));
        assert_eq!(g.units().unit_stat(u, Stat::Melee), base + 3);
        g.tick_temp_effects();
        assert_eq!(g.units().unit_stat(u, Stat::Melee), base + 3);
        g.tick_temp_effects();
        assert_eq!(g.units().unit_stat(u, Stat::Melee), base);
        assert!(g.units().temp_effects(u).is_empty());
    }

    #[test]
    fn an_untimed_effect_stays() {
        let mut g = world();
        let u = unit(&mut g, 95 /* high_elf_spearmen */);
        let base = g.units().unit_stat(u, Stat::Melee);
        g.add_temp_modifier(Target::Unit(u), plus(Stat::Melee, 1), None);
        for _ in 0..50 {
            g.tick_temp_effects();
        }
        assert_eq!(g.units().unit_stat(u, Stat::Melee), base + 1);
    }

    #[test]
    fn a_granted_ability_counts_until_it_runs_out() {
        let mut g = world();
        let u = unit(&mut g, 95 /* high_elf_spearmen */);
        assert!(!g.units().unit_has_ability(u, Ability::new(24)));
        g.grant_temp_ability(Target::Unit(u), Ability::new(24), Some(1));
        assert!(g.units().unit_has_ability(u, Ability::new(24)));
        g.tick_temp_effects();
        assert!(!g.units().unit_has_ability(u, Ability::new(24)));
    }

    #[test]
    fn a_removed_ability_is_gone_until_it_is_granted_again() {
        let mut g = world();
        // The Pegasai fly by nature.
        let u = unit(&mut g, 103 /* high_elf_pegasai */);
        assert!(g.units().unit_has_ability(u, Ability::new(24)));
        g.remove_temp_ability(Target::Unit(u), Ability::new(24), None);
        assert!(!g.units().unit_has_ability(u, Ability::new(24)));
        g.grant_temp_ability(Target::Unit(u), Ability::new(24), None);
        assert!(
            g.units().unit_has_ability(u, Ability::new(24)),
            "a later grant wins"
        );
    }

    #[test]
    fn a_city_takes_modifiers_but_no_abilities() {
        let mut g = world();
        let city = g.world.spawn((crate::game::serial::Serial(900),));
        g.add_temp_modifier(Target::City(city), plus(Stat::CityGold, 4), Some(3));
        g.grant_temp_ability(Target::City(city), Ability::new(24), None);
        assert_eq!(g.units().temp_effects(city).len(), 1);
        assert!(g.economy().city_modifiers(city).iter().any(
            |m| m.stat == Stat::CityGold && m.value == f4x_def::half_int::HalfInt::from_int(4)
        ));
    }

    #[test]
    fn effects_survive_a_save() -> TestResult {
        let mut g = world();
        let u = unit(&mut g, 95 /* high_elf_spearmen */);
        g.add_temp_modifier(Target::Unit(u), plus(Stat::Melee, 3), Some(5));
        g.grant_temp_ability(Target::Unit(u), Ability::new(24), None);
        let snap = g.snapshot();
        let back = snap.clone().restore(g.content.clone())?;
        assert_eq!(back.snapshot(), snap);
        let u = back.units().units_at(AT)[0];
        assert_eq!(back.units().temp_effects(u).len(), 2);
        assert!(back.units().unit_has_ability(u, Ability::new(24)));
        Ok(())
    }

    #[test]
    fn a_rules_effect_reaches_the_store_through_the_host() {
        use crate::rules::exec::EffectHost;
        let mut g = world();
        let u = unit(&mut g, 95 /* high_elf_spearmen */);
        let melee = g.units().unit_stat(u, Stat::Melee);
        let mut host = crate::spell::cast::tests_support::host(&mut g);
        host.add_modifier(Target::Unit(u), plus(Stat::Melee, 2));
        host.grant_ability(Target::Unit(u), Ability::new(24) /* flying */);
        drop(host);
        assert_eq!(g.units().unit_stat(u, Stat::Melee), melee + 2);
        assert!(g.units().unit_has_ability(u, Ability::new(24)));
        g.remove_temp_ability(Target::Unit(u), Ability::new(24) /* flying */, None);
        assert!(!g.units().unit_has_ability(u, Ability::new(24)));
    }

    fn enchant(g: &mut Game, u: hecs::Entity, spell: u16) {
        let id = g
            .content
            .rules
            .spells
            .id_numbered(spell)
            .expect("id_of succeeds");
        g.attach_enchantment(Target::Unit(u), id, P0);
    }

    #[test]
    fn a_lasting_spell_can_grant_an_ability() {
        let mut g = world();
        let u = unit(&mut g, 95 /* high_elf_spearmen */);
        assert!(!g
            .units()
            .unit_abilities(u)
            .iter()
            .any(|a| *a == Ability::new(40)));
        enchant(&mut g, u, 69 /* magic_immunity */);
        assert!(g
            .units()
            .unit_abilities(u)
            .iter()
            .any(|a| *a == Ability::new(40)));
        // A temporary removal still takes it away again.
        g.remove_temp_ability(Target::Unit(u), Ability::new(40), None);
        assert!(!g
            .units()
            .unit_abilities(u)
            .iter()
            .any(|a| *a == Ability::new(40)));
    }

    #[test]
    fn resistance_against_a_realm_counts_immunity_and_enchantments() {
        let mut g = world();
        let u = unit(&mut g, 95 /* high_elf_spearmen */);
        let base = g.units().unit_stat(u, Stat::Resistance);
        assert_eq!(
            g.units().unit_resistance_against(u, Some(Realm::Chaos)),
            base
        );
        enchant(&mut g, u, 2 /* resist_elements */);
        assert_eq!(
            g.units().unit_resistance_against(u, Some(Realm::Chaos)),
            base + 3
        );
        assert_eq!(
            g.units().unit_resistance_against(u, Some(Realm::Nature)),
            base + 3
        );
        assert_eq!(
            g.units().unit_resistance_against(u, Some(Realm::Life)),
            base
        );
        enchant(&mut g, u, 69 /* magic_immunity */);
        assert_eq!(
            g.units().unit_resistance_against(u, Some(Realm::Life)),
            base + 30
        );
        assert_eq!(
            g.units().unit_resistance_against(u, Some(Realm::Chaos)),
            base + 33
        );
        assert_eq!(g.units().unit_resistance_against(u, None), base);
    }

    #[test]
    fn righteousness_protects_against_chaos_and_death_only() {
        let mut g = world();
        let u = unit(&mut g, 95 /* high_elf_spearmen */);
        for realm in [Realm::Chaos, Realm::Death, Realm::Nature] {
            assert!(!g.units().unit_protected_from(u, Some(realm)));
        }
        enchant(&mut g, u, 144 /* righteousness */);
        assert!(g.units().unit_protected_from(u, Some(Realm::Chaos)));
        assert!(g.units().unit_protected_from(u, Some(Realm::Death)));
        assert!(!g.units().unit_protected_from(u, Some(Realm::Nature)));
        assert!(!g.units().unit_protected_from(u, None));
    }

    #[test]
    fn ships_and_catapults_are_fantastic_but_their_weapons_are_not_magical() {
        let mut g = world();
        // The game's ship and its catapult.
        for id in [35 /* trireme */, 37 /* catapult */] {
            let m = unit(&mut g, id);
            assert!(!g.units().unit_has_magical_weapon(m), "{id}");
        }
    }

    #[test]
    fn heroes_fantastic_creatures_and_flaming_blades_have_magical_weapons() {
        let mut g = world();
        let u = unit(&mut g, 95 /* high_elf_spearmen */);
        assert!(!g.units().unit_has_magical_weapon(u));
        enchant(&mut g, u, 94 /* flame_blade */);
        assert!(g.units().unit_has_magical_weapon(u));
        let trained = unit(&mut g, 95 /* high_elf_spearmen */);
        assert!(!g.units().unit_has_magical_weapon(trained));
        let _ = g.world.insert_one(
            trained,
            crate::unit::WeaponQuality(crate::unit::WEAPON_MAGIC),
        );
        assert!(
            g.units().unit_has_magical_weapon(trained),
            "magic weapons are magical"
        );
        // A hero and a summoned (fantastic) creature, as the game has them. The data's `Fantastic`
        // kind is only the ships and the catapult, whose weapons are not magical (tested above).
        for id in [0 /* dwarf */, 169 /* demon */] {
            let w = unit(&mut g, id);
            assert!(g.units().unit_has_magical_weapon(w), "{id}");
        }
    }
}
