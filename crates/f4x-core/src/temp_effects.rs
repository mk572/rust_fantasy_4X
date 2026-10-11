//! Temporary effects: modifiers and abilities that instant spells and rules put on a unit or
//! a city without an enchantment to carry them.
//!
//! Each carries the turns it has left (`None` lasts until the bearer is gone). Units read them
//! through `unit_modifiers` and `unit_abilities`, cities through `city_modifiers`. Cities have
//! no abilities, so granting or removing one there does nothing.

use crate::rules::exec::Target;
use crate::rules::stats::passive_modifiers;
use crate::rules::{ModOp, Modifier, Source, Stat};
use crate::Game;

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

impl Game {
    fn push_temp(&mut self, who: Target, kind: TempKind, turns: Option<u16>) {
        let e = match who {
            Target::Unit(e) | Target::City(e) if self.world.contains(e) => e,
            _ => return,
        };
        if matches!(who, Target::City(_)) && !matches!(kind, TempKind::Modifier { .. }) {
            return;
        }
        let effect = TempEffect { kind, turns };
        if let Ok(mut list) = self.world.get::<&mut TempEffects>(e) {
            list.0.push(effect);
            return;
        }
        let _ = self.world.insert_one(e, TempEffects(vec![effect]));
    }

    /// Put a stat modifier on a unit or city for `turns` turns (`None`: for good).
    pub fn add_temp_modifier(&mut self, who: Target, m: Modifier, turns: Option<u16>) {
        let kind = TempKind::Modifier {
            stat: m.stat,
            op: m.op,
            value: m.value,
        };
        self.push_temp(who, kind, turns);
    }

    /// Give a unit an ability it does not have for `turns` turns (`None`: for good).
    pub fn grant_temp_ability(&mut self, who: Target, ability: &str, turns: Option<u16>) {
        self.push_temp(who, TempKind::Grant(ability.into()), turns);
    }

    /// Take an ability away from a unit for `turns` turns (`None`: for good).
    pub fn remove_temp_ability(&mut self, who: Target, ability: &str, turns: Option<u16>) {
        self.push_temp(who, TempKind::Remove(ability.into()), turns);
    }

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

    /// The unit's abilities now: its own, plus granted ones, minus removed ones. Effects apply in
    /// the order they were added, so a later grant restores what an earlier removal took.
    pub fn unit_abilities(&self, unit: hecs::Entity) -> Vec<String> {
        let mut out = self
            .unit_def(unit)
            .map(|d| d.abilities.clone())
            .unwrap_or_default();
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

    /// Count every timed effect down; drop the ones that ran out.
    pub(crate) fn tick_temp_effects(&mut self) {
        let holders: Vec<hecs::Entity> = self
            .world
            .query::<&TempEffects>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        for e in holders {
            let empty = {
                let Ok(mut list) = self.world.get::<&mut TempEffects>(e) else {
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
                let _ = self.world.remove_one::<TempEffects>(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::{PlayerId, Position};
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
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
        let base = g.unit_stat(u, Stat::Melee);
        g.add_temp_modifier(Target::Unit(u), plus(Stat::Melee, 3), Some(2));
        assert_eq!(g.unit_stat(u, Stat::Melee), base + 3);
        g.tick_temp_effects();
        assert_eq!(g.unit_stat(u, Stat::Melee), base + 3);
        g.tick_temp_effects();
        assert_eq!(g.unit_stat(u, Stat::Melee), base);
        assert!(g.temp_effects(u).is_empty());
    }

    #[test]
    fn an_untimed_effect_stays() {
        let mut g = world();
        let u = unit(&mut g, "spear");
        let base = g.unit_stat(u, Stat::Melee);
        g.add_temp_modifier(Target::Unit(u), plus(Stat::Melee, 1), None);
        for _ in 0..50 {
            g.tick_temp_effects();
        }
        assert_eq!(g.unit_stat(u, Stat::Melee), base + 1);
    }

    #[test]
    fn a_granted_ability_counts_and_brings_its_modifiers() {
        let mut g = world();
        let u = unit(&mut g, "spear");
        let base = g.unit_stat(u, Stat::Defense);
        assert!(!g.unit_has_ability(u, "tough"));
        g.grant_temp_ability(Target::Unit(u), "tough", Some(1));
        assert!(g.unit_has_ability(u, "tough"));
        assert_eq!(g.unit_stat(u, Stat::Defense), base + 2);
        g.tick_temp_effects();
        assert!(!g.unit_has_ability(u, "tough"));
        assert_eq!(g.unit_stat(u, Stat::Defense), base);
    }

    #[test]
    fn a_removed_ability_is_gone_with_its_modifiers() {
        let mut g = world();
        let u = unit(&mut g, "hardy");
        let base = g.unit_stat(u, Stat::Defense);
        g.remove_temp_ability(Target::Unit(u), "tough", None);
        assert!(!g.unit_has_ability(u, "tough"));
        assert_eq!(g.unit_stat(u, Stat::Defense), base - 2);
        g.grant_temp_ability(Target::Unit(u), "tough", None);
        assert!(g.unit_has_ability(u, "tough"), "a later grant wins");
        assert_eq!(g.unit_stat(u, Stat::Defense), base);
    }

    #[test]
    fn a_city_takes_modifiers_but_no_abilities() {
        let mut g = world();
        let city = g.world.spawn((crate::serial::Serial(900),));
        g.add_temp_modifier(Target::City(city), plus(Stat::CityGold, 4), Some(3));
        g.grant_temp_ability(Target::City(city), "flying", None);
        assert_eq!(g.temp_effects(city).len(), 1);
        assert!(g
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
        let u = back.units_at(AT)[0];
        assert_eq!(back.temp_effects(u).len(), 2);
        assert!(back.unit_has_ability(u, "flying"));
    }
}
