//! The settings a game is played with: chosen once when the game is made, saved and loaded with
//! it, and read by the rules through [`World::settings`](crate::world::World::settings)
//! (`docs/game-settings.md`). The records and the figures are data in `f4x-def`
//! (`f4x_def::settings`); here is the code that applies them to a game.

pub use f4x_def::settings::*;

use crate::player::PlayerId;
use f4x_def::half_int::HalfInt;

impl crate::world::World {
    /// Copy the settings that the rules read from the content (half upkeep, the arcane spell
    /// cap) into this game's own copy of the content, so a game keeps the choices it was made or
    /// saved with. Called when a game is made and when one is loaded.
    pub(crate) fn apply_content_settings(&mut self) {
        let rules = self.settings.rules;
        let content = std::sync::Arc::make_mut(&mut self.content);
        let targets = content.rules.config.targets.clone();
        content.settings.half_upkeep.enabled = rules.half_upkeep;
        content.settings.spell_library.arcane_candidate_cap = rules.arcane_candidate_cap;
        // Adamant Alchemy: the retort costs two picks, for the creation screen, the computer
        // wizards and the treasure alike (they all read the cost from the rules).
        if rules.adamant_alchemy {
            if let Some(alchemy) = content.rules.retorts.by_num_mut(targets.alchemy_retort) {
                alchemy.cost = 2;
            }
        }
        // 2-Pick Myrran: two picks, and the fortress on Myrror makes no more mana.
        if rules.two_pick_myrran {
            if let Some(myrran) = content.rules.retorts.by_num_mut(targets.myrran_retort) {
                myrran.cost = 2;
            }
            content.settings.myrror_fortress_power = 0;
        }
        for (num, capacity) in [
            (targets.trireme_unit, rules.trireme_capacity),
            (targets.galley_unit, rules.galley_capacity),
        ] {
            if let Some(unit) = content.rules.units.by_num_mut(num) {
                unit.stats.insert(
                    crate::rules::Stat::Transport,
                    HalfInt::from_int(i32::from(capacity)),
                );
            }
        }
        // Cavalry strike first and halberdiers negate it, whatever their rows say.
        if rules.cavalry_halberdier_abilities {
            for (list, ability) in [
                (&targets.cavalry_units, targets.first_strike),
                (&targets.halberdier_units, targets.negate_first_strike),
            ] {
                let ability = crate::rules::Ability::new(ability);
                for &num in list {
                    if let Some(unit) = content.rules.units.by_num_mut(num) {
                        if !unit.abilities.contains(&ability) {
                            unit.abilities.push(ability);
                        }
                    }
                }
            }
        }
    }

    /// Give each computer wizard its difficulty bonuses from the settings (a human wizard and
    /// the neutral players get none). Called when a game is made and when one is loaded, after
    /// the human is known.
    pub fn apply_wizard_settings(&mut self) {
        let ai = self.settings.ai;
        let human = self.human();
        for (_, (&id, bonus, neutral)) in self.ecs.query_mut::<(
            &PlayerId,
            &mut crate::player::Bonus,
            Option<&crate::player::Neutral>,
        )>() {
            let computer = neutral.is_none() && Some(id) != human;
            *bonus = if computer {
                crate::player::Bonus {
                    growth: i32::from(ai.growth_percent),
                    outpost_growth: i32::from(ai.outpost_growth_percent),
                    production: i32::from(ai.production_percent),
                    gold: i32::from(ai.gold_percent),
                    mana: i32::from(ai.mana_percent),
                    food: i32::from(ai.food_percent),
                    research: i32::from(ai.research_percent),
                    maintenance: i32::from(ai.maintenance_percent),
                    unit_upkeep: i32::from(ai.unit_upkeep_percent),
                }
            } else {
                crate::player::Bonus::default()
            };
        }
    }
}
