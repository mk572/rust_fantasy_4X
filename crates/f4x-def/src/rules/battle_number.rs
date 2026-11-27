//! The groups of numbers in `battle.ron` that the dice, the abilities, the field spells and the
//! battle spells read. The comments of the file say what each number does.
//!
//! Tests: at the end of this file; the refusals in `tests/it/c1_content.rs`; what the numbers do
//! in the tests of `f4x-battle` and `f4x-core`'s battle tests.

use serde::{Deserialize, Serialize};

use crate::half_int::HalfInt;
use crate::issue::Issues;

const FILE: &str = "battle.ron";

/// The pluses and minuses to hit and to block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ToHitRules {
    pub range_penalty_squares: u32,
    pub long_range_most_penalty: i32,
    pub invisibility_penalty_per_square: i32,
    pub blur_penalty: i32,
    pub magic_weapon_bonus: i32,
    pub eldritch_block_penalty: i32,
}

impl ToHitRules {
    /// The penalty to hit with arrows and boulders at `distance` squares.
    pub fn range_penalty(&self, distance: u32, long_range: bool) -> i32 {
        let p = distance
            .checked_div(self.range_penalty_squares)
            .unwrap_or(0) as i32;
        if long_range {
            p.min(self.long_range_most_penalty)
        } else {
            p
        }
    }
}

/// What the Lucky ability gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct LuckyRules {
    pub to_hit: i32,
    pub to_block: i32,
    pub resistance: i32,
    pub attacker_to_hit: i32,
}

/// What a field spell adds to a unit's numbers; a number left out adds nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct StatBonus {
    pub melee: i32,
    pub ranged: i32,
    pub defense: i32,
    pub resistance: i32,
    pub to_hit: i32,
    pub to_block: i32,
}

/// Call Lightning's bolts (their strength is `call_lightning_strength`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct LightningRules {
    pub least_bolts: u32,
    pub most_bolts: u32,
    pub one_in: u32,
    pub tries: u32,
}

/// The spells on the whole field, and the bonuses of the side that are worked out with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FieldRules {
    pub light_and_darkness: i32,
    pub warp_reality: StatBonus,
    pub black_prayer: StatBonus,
    pub prayer: StatBonus,
    pub high_prayer: StatBonus,
    pub metal_fires: StatBonus,
    pub leadership_ranged_percent: i32,
    pub call_lightning: LightningRules,
    pub wrack_modifier: i32,
    pub wrack_damage: i32,
    pub terror_modifier: i32,
    pub entangle_moves: HalfInt,
}

/// The touches of special attacks, and what an item or a spell makes of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TouchRules {
    pub dispel_evil_modifier: i32,
    pub dispel_evil_undead_modifier: i32,
    pub given_stoning_touch_penalty: i32,
    pub given_death_touch_penalty: i32,
    pub given_doom_percent: i32,
}

/// Warp Creature's three harms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WarpCreatureRules {
    pub attack_percent: i32,
    pub defense_percent: i32,
    pub resistance: i32,
}

/// The numbers of Call Chaos's outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CallChaosRules {
    pub healing: i32,
    pub fire_bolt_strength: i32,
    pub disintegrate_below: i32,
}

/// The battle spells that act in a way of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BattleSpellRules {
    pub life_drain_mana_per_penalty: i32,
    pub life_drain_mana_per_point: i32,
    pub mud_radius: i32,
    pub raise_dead_figures_percent: u32,
    pub warp_creature: WarpCreatureRules,
    pub call_chaos: CallChaosRules,
}

/// The chance in a hundred that a unit which flees a lost battle is lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FleeRules {
    pub loss_percent: u32,
    pub hero_loss_percent: u32,
}

impl FleeRules {
    /// The chance for a unit that is a hero or not.
    pub fn percent(&self, hero: bool) -> u32 {
        if hero {
            self.hero_loss_percent
        } else {
            self.loss_percent
        }
    }
}

/// The moves it takes to cross the squares of the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct MovementRules {
    pub open: HalfInt,
    pub rough: HalfInt,
    pub road: HalfInt,
    pub scenery_extra: HalfInt,
    pub sail: HalfInt,
    pub flying: HalfInt,
    pub teleport: HalfInt,
    pub mud: HalfInt,
    pub jump: HalfInt,
    pub diagonal_extra: HalfInt,
}

impl MovementRules {
    /// Report a step that costs nothing and an extra that gives moves back.
    pub(super) fn problems(&self, p: &mut Issues) {
        let steps = [
            ("open", self.open),
            ("rough", self.rough),
            ("road", self.road),
            ("sail", self.sail),
            ("flying", self.flying),
            ("teleport", self.teleport),
            ("mud", self.mud),
            ("jump", self.jump),
        ];
        for (field, cost) in steps {
            if cost <= HalfInt::ZERO {
                p.out_of_range(FILE, "movement", field, cost, "above 0");
            }
        }
        let extras = [
            ("scenery_extra", self.scenery_extra),
            ("diagonal_extra", self.diagonal_extra),
        ];
        for (field, cost) in extras {
            if cost < HalfInt::ZERO {
                p.out_of_range(FILE, "movement", field, cost, "0 or more");
            }
        }
    }
}

/// Report a number that must be at least 1.
pub(super) fn at_least_one(p: &mut Issues, record: &str, field: &str, value: i64) {
    if value < 1 {
        p.out_of_range(FILE, record, field, value, "at least 1");
    }
}

/// Report a percent outside 0 to 100.
pub(super) fn percent(p: &mut Issues, record: &str, field: &str, value: i64) {
    if !(0..=100).contains(&value) {
        p.out_of_range(FILE, record, field, value, "0 to 100");
    }
}

#[cfg(test)]
mod tests {
    fn shipped() -> crate::rules::BattleRules {
        crate::testdir::shipped().rules.battle.clone()
    }

    #[test]
    fn the_range_costs_a_point_to_hit_for_each_step_of_distance_up_to_a_long_range_cap() {
        let to_hit = shipped().to_hit;
        let step = to_hit.range_penalty_squares;
        assert_eq!(to_hit.range_penalty(0, false), 0);
        assert_eq!(to_hit.range_penalty(step - 1, false), 0);
        assert_eq!(to_hit.range_penalty(step - 1, true), 0);
        for steps in 1..=to_hit.long_range_most_penalty + 2 {
            let distance = steps as u32 * step;
            assert_eq!(to_hit.range_penalty(distance, false), steps);
            let capped = steps.min(to_hit.long_range_most_penalty);
            assert_eq!(to_hit.range_penalty(distance, true), capped);
        }
    }

    #[test]
    fn a_weapon_gets_the_bonus_of_its_quality_and_a_plain_one_none() {
        let rules = shipped();
        assert_eq!(rules.weapon_bonus(0), 0);
        for (i, &bonus) in rules.weapon_material_bonus.iter().enumerate() {
            assert_eq!(rules.weapon_bonus(i as u8 + 1), bonus);
        }
        assert_eq!(rules.weapon_bonus(u8::MAX), 0);
    }

    #[test]
    fn a_hero_flees_at_its_own_odds() {
        let flee = shipped().flee;
        assert_eq!(flee.percent(true), flee.hero_loss_percent);
        assert_eq!(flee.percent(false), flee.loss_percent);
    }
}
