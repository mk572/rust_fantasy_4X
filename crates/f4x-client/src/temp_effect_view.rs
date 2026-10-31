//! What the unit sheet and the city screen say about temporary effects
//! (`temp_effect.rs` in the core holds them): each line names the change and how long it lasts.

use f4x_core::rules::{ModOp, Stat};
use f4x_core::spell::temp_effect::{TempEffect, TempKind};
use f4x_core::text::Args;

use crate::view::App;

/// The name of each statistic.
const STAT_NAMES: [(Stat, &str); 34] = [
    (Stat::Melee, "temp.stat_melee"),
    (Stat::Ranged, "temp.stat_ranged"),
    (Stat::Defense, "temp.stat_defense"),
    (Stat::Resistance, "temp.stat_resistance"),
    (Stat::MaxHp, "temp.stat_max_hp"),
    (Stat::Figures, "temp.stat_figures"),
    (Stat::Moves, "temp.stat_moves"),
    (Stat::HalfMoves, "temp.stat_half_moves"),
    (Stat::Sight, "temp.stat_sight"),
    (Stat::ToHit, "temp.stat_to_hit"),
    (Stat::MeleeToHit, "temp.stat_melee_to_hit"),
    (Stat::RangedToHit, "temp.stat_ranged_to_hit"),
    (Stat::Ammo, "temp.stat_ammo"),
    (Stat::SpecialLevel, "temp.stat_special_level"),
    (Stat::Construction, "temp.stat_construction"),
    (Stat::Transport, "temp.stat_transport"),
    (Stat::ManaIncome, "temp.stat_mana_income"),
    (Stat::ResearchIncome, "temp.stat_research_income"),
    (Stat::CastingSkill, "temp.stat_casting_skill"),
    (Stat::SpellCostPercent, "temp.stat_spell_cost_percent"),
    (Stat::CityFood, "temp.stat_city_food"),
    (Stat::CityFoodFlat, "temp.stat_city_food_flat"),
    (Stat::FarmerFood, "temp.stat_farmer_food"),
    (Stat::CityUnrestPercent, "temp.stat_city_unrest_percent"),
    (Stat::StartingXp, "temp.stat_starting_xp"),
    (Stat::CityProduction, "temp.stat_city_production"),
    (Stat::CityGold, "temp.stat_city_gold"),
    (Stat::CityMana, "temp.stat_city_mana"),
    (Stat::CityResearch, "temp.stat_city_research"),
    (Stat::CityGrowthPercent, "temp.stat_city_growth_percent"),
    (Stat::CityGrowth, "temp.stat_city_growth"),
    (Stat::CityUnrest, "temp.stat_city_unrest"),
    (Stat::UnitCostPercent, "temp.stat_unit_cost_percent"),
    (Stat::CitySight, "temp.stat_city_sight"),
];

impl App {
    /// One line for each temporary effect on a unit or city: "Melee +2 (3 turns)".
    pub(crate) fn temp_effect_lines(&self, e: hecs::Entity) -> Vec<String> {
        self.game
            .units()
            .temp_effects(e)
            .iter()
            .map(|t| self.temp_effect_line(t))
            .collect()
    }

    fn temp_effect_line(&self, t: &TempEffect) -> String {
        let abilities = &self.game.content().rules.abilities;
        let ability = |id: &str| {
            abilities
                .by_name(id)
                .map_or_else(|| id.to_string(), |a| a.name.clone())
        };
        let effect = match &t.kind {
            TempKind::Modifier { stat, op, value } => {
                let name = STAT_NAMES
                    .iter()
                    .find(|(s, _)| s == stat)
                    .map_or_else(String::new, |(_, key)| self.text(key));
                let key = match op {
                    ModOp::Add => "temp.modifier_add",
                    ModOp::Percent => "temp.modifier_percent",
                    ModOp::Min => "temp.modifier_min",
                    ModOp::Max => "temp.modifier_max",
                    ModOp::Set => "temp.modifier_set",
                };
                let shown = match op {
                    ModOp::Add | ModOp::Percent => format!("{value:+}"),
                    _ => value.to_string(),
                };
                self.text_with(key, &Args::new().text("stat", name).text("value", shown))
            }
            TempKind::Grant(id) => {
                self.text_with("temp.grant", &Args::new().text("ability", ability(id)))
            }
            TempKind::Remove(id) => {
                self.text_with("temp.remove", &Args::new().text("ability", ability(id)))
            }
        };
        // An effect that never runs out says nothing about its length.
        match t.turns {
            Some(n) => {
                let when = self.text_with("temp.turns", &Args::new().count("n", u32::from(n)));
                self.text_with(
                    "temp.line",
                    &Args::new().text("effect", effect).text("when", when),
                )
            }
            None => effect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::HUMAN;
    use f4x_core::rules::exec::Target;
    use f4x_core::rules::{Modifier, Source};

    #[test]
    fn a_unit_lists_its_effects_with_their_length_and_a_lasting_one_says_none() {
        let mut app = App::for_test();
        let unit = app.game.units().player_units(HUMAN)[0];
        let who = Target::Unit(unit);
        let plus = Modifier::new(Stat::Melee, ModOp::Add, 2, Source::Other);
        app.game.add_temp_modifier(who, plus, Some(3));
        let set = Modifier::new(Stat::Moves, ModOp::Set, 4, Source::Other);
        app.game.add_temp_modifier(who, set, None);
        app.game.grant_temp_ability(who, "first_strike", Some(1));
        let ability = app
            .game
            .content()
            .rules
            .abilities
            .by_name("first_strike")
            .map(|a| a.name.clone())
            .expect("first_strike is an ability");
        assert_eq!(
            app.temp_effect_lines(unit),
            [
                "Melee +2 (3 turns)".to_string(),
                app.text_with(
                    "temp.modifier_set",
                    &Args::new()
                        .text("stat", app.text("temp.stat_moves"))
                        .text("value", "4".to_string())
                ),
                format!("Gains {ability} (1 turn)"),
            ]
        );
    }

    #[test]
    fn every_stat_a_modifier_can_name_has_its_text() {
        let texts = crate::test_app::shipped::texts();
        for (stat, key) in STAT_NAMES {
            assert!(texts.has(key), "{stat:?} has no text {key}");
        }
    }
}
