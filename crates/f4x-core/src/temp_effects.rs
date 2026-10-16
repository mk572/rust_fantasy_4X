//! Temporary effects: modifiers and abilities that instant spells and rules put on a unit or
//! a city without an enchantment to carry them.
//!
//! Each carries the turns it has left (`None` lasts until the bearer is gone). Units read them
//! through `unit_modifiers` and `unit_abilities`, cities through `city_modifiers`. Cities have
//! no abilities, so granting or removing one there does nothing.

use crate::battle::immunity::{effective_resistance, Immunities, Realm};
use crate::rules::effects::{Condition, Effect, Trigger};
use crate::rules::exec::Target;
use crate::rules::stats::passive_modifiers;
use crate::rules::{ModOp, Modifier, Source, Stat};
use crate::units::Units;
use crate::Game;
use hecs::World;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempKind {
    Modifier { stat: Stat, op: ModOp, value: i32 },
    Grant(String),
    Remove(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    let _ = world.insert_one(e, TempEffects(vec![effect]));
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
pub(crate) fn grant_ability(world: &mut World, who: Target, ability: &str, turns: Option<u16>) {
    push_temp(world, who, TempKind::Grant(ability.into()), turns);
}

/// Take an ability away from a unit for `turns` turns (`None`: for good).
pub(crate) fn remove_ability(world: &mut World, who: Target, ability: &str, turns: Option<u16>) {
    push_temp(world, who, TempKind::Remove(ability.into()), turns);
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
            let _ = world.remove_one::<TempEffects>(e);
        }
    }
}

impl<'a> Units<'a> {
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
                    Some(Modifier::new(stat, op, value, Source::Other))
                }
                _ => None,
            })
            .collect()
    }

    /// The unit's abilities now: its own, those its lasting spells grant (Magic Immunity,
    /// Guardian Wind), plus temporary grants, minus temporary removals. Temporary effects apply
    /// in the order they were added, so a later grant restores what an earlier removal took.
    pub fn unit_abilities(&self, unit: hecs::Entity) -> Vec<String> {
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
        if let Ok(list) = self.world.get::<&crate::units::UnitEnchantments>(unit) {
            for &(spell, _) in &list.0 {
                for rule in &self.content.rules.spells.get(spell).effects {
                    if let (Trigger::Passive, Condition::Always, Effect::GrantAbility(a)) =
                        (rule.when, &rule.condition, &rule.effect)
                    {
                        if !out.contains(a) {
                            out.push(a.clone());
                        }
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
                        out.push(a.clone());
                    }
                }
            }
        }
        if self.unit_counts_as_undead(unit) {
            for a in crate::undead::UNDEAD_IMMUNITIES {
                if !out.iter().any(|x| x == a) {
                    out.push(a.to_string());
                }
            }
        }
        if let Ok(list) = self.world.get::<&TempEffects>(unit) {
            for t in &list.0 {
                match &t.kind {
                    TempKind::Grant(a) if !out.contains(a) => out.push(a.clone()),
                    TempKind::Remove(a) => out.retain(|x| x != a),
                    _ => {}
                }
            }
        }
        out
    }

    /// Whether the unit's normal attacks count as magical, so weapon immunity does not stop
    /// them: heroes, fantastic and summoned creatures, units trained with magic, mithril or
    /// adamantium weapons, and anything with the `magical_weapon` ability (a flaming or blessed
    /// blade).
    pub fn unit_has_magical_weapon(&self, unit: hecs::Entity) -> bool {
        self.unit_def(unit)
            .is_some_and(|d| d.kind != crate::rules::UnitKind::Normal)
            || self.unit_weapon_quality(unit) > 0
            || self
                .unit_abilities(unit)
                .iter()
                .any(|a| a == "magical_weapon")
    }

    /// The ids of the lasting spells on a unit.
    pub(crate) fn unit_enchantment_ids(&self, unit: hecs::Entity) -> Vec<String> {
        let mut ids: Vec<String> = self
            .world
            .get::<&crate::units::UnitEnchantments>(unit)
            .map(|list| {
                list.0
                    .iter()
                    .map(|&(spell, _)| self.content.rules.spells.get(spell).id.clone())
                    .collect()
            })
            .unwrap_or_default();
        // What its items put on the hero counts as well.
        for spell in self.items().spells(unit) {
            let id = &self.content.rules.spells.get(spell).id;
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids
    }

    /// The unit's resistance against a spell or attack of `realm`: its own, plus what magic
    /// immunity and its lasting spells add against that realm (`battle::immunity`).
    pub fn unit_resistance_against(&self, unit: hecs::Entity, realm: Option<Realm>) -> i32 {
        let ids = self.unit_enchantment_ids(unit);
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        effective_resistance(
            self.unit_stat(unit, Stat::Resistance),
            &Immunities::of(&self.unit_abilities(unit)),
            realm,
            &ids,
        )
    }

    /// Whether the unit cannot be hurt by spells of `realm` at all: Righteousness against
    /// chaos and death.
    pub fn unit_protected_from(&self, unit: hecs::Entity, realm: Option<Realm>) -> bool {
        matches!(realm, Some(Realm::Chaos | Realm::Death))
            && self
                .unit_enchantment_ids(unit)
                .iter()
                .any(|i| i == "righteousness")
    }

    /// The passive modifiers of every ability the unit has now.
    pub(crate) fn ability_modifiers(&self, unit: hecs::Entity) -> Vec<Modifier> {
        let rules = &self.content.rules;
        let mut mods = Vec::new();
        for name in self.unit_abilities(unit) {
            if let Some(id) = rules.abilities.id_of(&name) {
                mods.extend(passive_modifiers(
                    &rules.abilities.get(id).effects,
                    Source::Ability(id),
                ));
            }
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
    pub fn grant_temp_ability(&mut self, who: Target, ability: &str, turns: Option<u16>) {
        grant_ability(&mut self.world, who, ability, turns);
    }

    /// Take an ability away from a unit for `turns` turns (`None`: for good).
    pub fn remove_temp_ability(&mut self, who: Target, ability: &str, turns: Option<u16>) {
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
    use crate::city::{PlayerId, Position};
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::UnitKind;
    use crate::testutil::content_with_magic;

    const P0: PlayerId = PlayerId(0);
    const AT: Position = Position {
        plane: Plane::Arcanus,
        x: 5,
        y: 5,
    };

    fn world() -> Game {
        let c = content_with_magic();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.players.push(Player::new(P0, "W"));
        g
    }

    fn unit(g: &mut Game, id: &str) -> hecs::Entity {
        let id = g.content.rules.units.id_of(id).unwrap();
        g.spawn_unit(P0, AT, id)
    }

    fn plus(stat: Stat, value: i32) -> Modifier {
        Modifier::new(stat, ModOp::Add, value, Source::Other)
    }

    #[test]
    fn a_modifier_changes_the_stat_until_it_runs_out() {
        let mut g = world();
        let u = unit(&mut g, "spear");
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
        let u = unit(&mut g, "spear");
        let base = g.units().unit_stat(u, Stat::Melee);
        g.add_temp_modifier(Target::Unit(u), plus(Stat::Melee, 1), None);
        for _ in 0..50 {
            g.tick_temp_effects();
        }
        assert_eq!(g.units().unit_stat(u, Stat::Melee), base + 1);
    }

    #[test]
    fn a_granted_ability_counts_and_brings_its_modifiers() {
        let mut g = world();
        let u = unit(&mut g, "spear");
        let base = g.units().unit_stat(u, Stat::Defense);
        assert!(!g.units().unit_has_ability(u, "tough"));
        g.grant_temp_ability(Target::Unit(u), "tough", Some(1));
        assert!(g.units().unit_has_ability(u, "tough"));
        assert_eq!(g.units().unit_stat(u, Stat::Defense), base + 2);
        g.tick_temp_effects();
        assert!(!g.units().unit_has_ability(u, "tough"));
        assert_eq!(g.units().unit_stat(u, Stat::Defense), base);
    }

    #[test]
    fn a_removed_ability_is_gone_with_its_modifiers() {
        let mut g = world();
        let u = unit(&mut g, "hardy");
        let base = g.units().unit_stat(u, Stat::Defense);
        g.remove_temp_ability(Target::Unit(u), "tough", None);
        assert!(!g.units().unit_has_ability(u, "tough"));
        assert_eq!(g.units().unit_stat(u, Stat::Defense), base - 2);
        g.grant_temp_ability(Target::Unit(u), "tough", None);
        assert!(g.units().unit_has_ability(u, "tough"), "a later grant wins");
        assert_eq!(g.units().unit_stat(u, Stat::Defense), base);
    }

    #[test]
    fn a_city_takes_modifiers_but_no_abilities() {
        let mut g = world();
        let city = g.world.spawn((crate::serial::Serial(900),));
        g.add_temp_modifier(Target::City(city), plus(Stat::CityGold, 4), Some(3));
        g.grant_temp_ability(Target::City(city), "flying", None);
        assert_eq!(g.units().temp_effects(city).len(), 1);
        assert!(g
            .economy()
            .city_modifiers(city)
            .iter()
            .any(|m| m.stat == Stat::CityGold && m.value == 4));
    }

    #[test]
    fn effects_survive_a_save() {
        let mut g = world();
        let u = unit(&mut g, "spear");
        g.add_temp_modifier(Target::Unit(u), plus(Stat::Melee, 3), Some(5));
        g.grant_temp_ability(Target::Unit(u), "flying", None);
        let snap = g.snapshot();
        let back = snap.clone().restore(g.content.clone()).unwrap();
        assert_eq!(back.snapshot(), snap);
        let u = back.units().units_at(AT)[0];
        assert_eq!(back.units().temp_effects(u).len(), 2);
        assert!(back.units().unit_has_ability(u, "flying"));
    }

    #[test]
    fn a_rules_effect_reaches_the_store_through_the_host() {
        use crate::rules::exec::EffectHost;
        let mut g = world();
        let u = unit(&mut g, "spear");
        let melee = g.units().unit_stat(u, Stat::Melee);
        let mut host = crate::cast::tests_support::host(&mut g);
        host.add_modifier(Target::Unit(u), plus(Stat::Melee, 2));
        host.grant_ability(Target::Unit(u), "flying");
        drop(host);
        assert_eq!(g.units().unit_stat(u, Stat::Melee), melee + 2);
        assert!(g.units().unit_has_ability(u, "flying"));
        let mut host = crate::cast::tests_support::host(&mut g);
        host.remove_ability(Target::Unit(u), "flying");
        drop(host);
        assert!(!g.units().unit_has_ability(u, "flying"));
    }

    fn enchant(g: &mut Game, u: hecs::Entity, spell: &str) {
        let id = g.content.rules.spells.id_of(spell).unwrap();
        g.attach_enchantment(Target::Unit(u), id, P0);
    }

    #[test]
    fn a_lasting_spell_can_grant_an_ability() {
        let mut g = world();
        let u = unit(&mut g, "spear");
        assert!(!g
            .units()
            .unit_abilities(u)
            .iter()
            .any(|a| a == "magic_immunity"));
        enchant(&mut g, u, "magic_immunity");
        assert!(g
            .units()
            .unit_abilities(u)
            .iter()
            .any(|a| a == "magic_immunity"));
        // A temporary removal still takes it away again.
        g.remove_temp_ability(Target::Unit(u), "magic_immunity", None);
        assert!(!g
            .units()
            .unit_abilities(u)
            .iter()
            .any(|a| a == "magic_immunity"));
    }

    #[test]
    fn resistance_against_a_realm_counts_immunity_and_enchantments() {
        let mut g = world();
        let u = unit(&mut g, "spear");
        let base = g.units().unit_stat(u, Stat::Resistance);
        assert_eq!(
            g.units().unit_resistance_against(u, Some(Realm::Chaos)),
            base
        );
        enchant(&mut g, u, "resist_elements");
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
        enchant(&mut g, u, "magic_immunity");
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
        let u = unit(&mut g, "spear");
        for realm in [Realm::Chaos, Realm::Death, Realm::Nature] {
            assert!(!g.units().unit_protected_from(u, Some(realm)));
        }
        enchant(&mut g, u, "righteousness");
        assert!(g.units().unit_protected_from(u, Some(Realm::Chaos)));
        assert!(g.units().unit_protected_from(u, Some(Realm::Death)));
        assert!(!g.units().unit_protected_from(u, Some(Realm::Nature)));
        assert!(!g.units().unit_protected_from(u, None));
    }

    #[test]
    fn heroes_fantastic_creatures_and_flaming_blades_have_magical_weapons() {
        let mut g = world();
        let u = unit(&mut g, "spear");
        assert!(!g.units().unit_has_magical_weapon(u));
        enchant(&mut g, u, "flame_blade");
        assert!(g.units().unit_has_magical_weapon(u));
        let trained = unit(&mut g, "spear");
        assert!(!g.units().unit_has_magical_weapon(trained));
        let _ = g.world.insert_one(
            trained,
            crate::units::WeaponQuality(crate::units::WEAPON_MAGIC),
        );
        assert!(
            g.units().unit_has_magical_weapon(trained),
            "magic weapons are magical"
        );
        let kinds = [UnitKind::Hero, UnitKind::Fantastic, UnitKind::Summoned];
        for (i, kind) in kinds.into_iter().enumerate() {
            let mut defs = g.content.rules.units.defs().to_vec();
            let mut def = defs.iter().find(|d| d.id == "spear").unwrap().clone();
            def.id = format!("special{i}");
            def.kind = kind;
            defs.push(def);
            g.content.rules.units = crate::rules::Registry::new(defs);
            let w = unit(&mut g, &format!("special{i}"));
            assert!(g.units().unit_has_magical_weapon(w), "{kind:?}");
        }
    }
}
