//! The reward of a battle (`docs/learning-ai.md` 7.4), from a finished [`Outcome`].

use f4x_core::battle::{BattleUnit, Side};
use f4x_core::content::Content;

use crate::env::Outcome;

/// What each point of a unit's cost adds to its value: a unit is worth `1 + 0.01 * cost`, the
/// cost being what the rules give it (production or gold, or the mana of a summoning).
pub const COST_VALUE: f32 = 0.01;
/// An enemy still standing below half its hit points is worth this share of its kill value to a
/// side that is losing.
pub const HURT_ENEMY_CREDIT: f32 = 0.1;
/// An own unit that survives below half its hit points is worth this much less than a whole one.
pub const HURT_OWN_DISCOUNT: f32 = 0.1;
/// The share of fleeing units that gets away (task 250: each is lost with one chance in two).
pub const FLEE_SURVIVAL: f32 = 0.5;

/// The value of a battle unit: `1 + COST_VALUE * cost`.
pub fn unit_value(content: &Content, unit: &BattleUnit) -> f32 {
    let cost = content
        .rules
        .units
        .id_of(&unit.unit_id)
        .map_or(0, |id| content.rules.units.get(id).cost);
    1.0 + COST_VALUE * cost as f32
}

/// Whether a standing unit has less than half its hit points left.
pub fn is_hurt(u: &BattleUnit) -> bool {
    let most = u.figures_start as i32 * u.hits;
    if most <= 0 || u.figures == 0 {
        return false;
    }
    let left = (u.figures as i32 - 1) * u.hits + (u.hits - u.front_damage).max(0);
    left * 2 < most
}

/// A win scores `1 - stakes * (own value lost / own value sent)`, where an own unit that stands
/// below half health counts as a tenth lost. A loss scores `-1` plus the value of the enemy units
/// destroyed (and a tenth of the value of those left below half health) and of own units saved by
/// fleeing (nine tenths for those below half health), over the value sent by both sides, so it
/// never reaches the lowest win. Units are worth `1 + 0.01 * cost` ([`unit_value`]).
pub fn reward(o: &Outcome, side: Side, stakes: f32) -> f32 {
    reward_with_budget(o, side, stakes, None)
}

/// The reward with the mana term: a small cost for the share of the mana budget spent (at most [`MANA_COST`]).
pub fn reward_with_budget(o: &Outcome, side: Side, stakes: f32, budget: Option<u32>) -> f32 {
    let mana = budget.filter(|&b| b > 0).map_or(0.0, |b| {
        MANA_COST * (o.mana_spent[side.index()] as f32 / b as f32).clamp(0.0, 1.0)
    });
    base_reward(o, side, stakes) - mana
}

/// The most the mana term takes off.
pub const MANA_COST: f32 = 0.025;

/// What each different spell a wizard casts in a battle adds, for training only (evaluations
/// leave it out, so their scores stay comparable): a push to try spells, not part of what a
/// battle is worth.
pub const SPELL_BONUS: f32 = 0.01;

fn base_reward(o: &Outcome, side: Side, stakes: f32) -> f32 {
    let me = side.index();
    let them = side.other().index();
    let sent = o.sent_value[me].max(1.0);
    if o.winner == Some(side) {
        let lost = ((o.lost_value[me] + HURT_OWN_DISCOUNT * o.standing_hurt_value[me]) / sent)
            .clamp(0.0, 1.0);
        1.0 - stakes * lost
    } else {
        let credit = o.lost_value[them]
            + HURT_ENEMY_CREDIT * o.standing_hurt_value[them]
            + o.saved_value[me]
            - HURT_OWN_DISCOUNT * o.saved_hurt_value[me];
        let all = (o.sent_value[me] + o.sent_value[them]).max(1.0);
        -1.0 + (credit / all).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(winner: Option<Side>, lost: [u32; 2], saved: [u32; 2]) -> Outcome {
        Outcome {
            winner,
            sent: [4, 6],
            lost,
            saved_by_fleeing: saved,
            // Every unit worth one, so the values are the counts.
            sent_value: [4.0, 6.0],
            lost_value: lost.map(|n| n as f32),
            saved_value: saved.map(|n| n as f32),
            ..Outcome::default()
        }
    }

    #[test]
    fn the_spell_bonus_counts_different_spells_not_casts() {
        let mut o = outcome(Some(Side::Attacker), [0, 6], [0, 0]);
        o.unique_spells = [3, 0];
        assert_eq!(o.unique_spells[Side::Attacker.index()], 3);
        let bonus = SPELL_BONUS * o.unique_spells[Side::Attacker.index()] as f32;
        assert!((bonus - 0.03).abs() < 1e-6);
    }

    #[test]
    fn spending_the_budget_costs_a_little() {
        let mut o = outcome(Some(Side::Attacker), [0, 6], [0, 0]);
        o.mana_spent = [30, 0];
        let free = reward_with_budget(&o, Side::Attacker, 0.5, None);
        let spent_half = reward_with_budget(&o, Side::Attacker, 0.5, Some(60));
        let spent_all = reward_with_budget(&o, Side::Attacker, 0.5, Some(30));
        assert_eq!(free, 1.0);
        assert!((spent_half - (1.0 - MANA_COST / 2.0)).abs() < 1e-6);
        assert!((spent_all - (1.0 - MANA_COST)).abs() < 1e-6);
        assert_eq!(reward_with_budget(&o, Side::Attacker, 0.5, Some(0)), 1.0);
    }

    #[test]
    fn a_win_is_worth_more_the_fewer_units_it_cost() {
        let cheap = reward(
            &outcome(Some(Side::Attacker), [0, 6], [0, 0]),
            Side::Attacker,
            0.5,
        );
        let dear = reward(
            &outcome(Some(Side::Attacker), [3, 6], [0, 0]),
            Side::Attacker,
            0.5,
        );
        assert_eq!(cheap, 1.0);
        assert!(dear < cheap && dear > 0.0);
        // With stakes 0 every win is worth the same.
        let free = reward(
            &outcome(Some(Side::Attacker), [4, 6], [0, 0]),
            Side::Attacker,
            0.0,
        );
        assert_eq!(free, 1.0);
    }

    #[test]
    fn a_loss_that_kills_many_or_flees_with_the_army_is_better_than_a_wipe_out() {
        let wipe = reward(
            &outcome(Some(Side::Defender), [4, 0], [0, 0]),
            Side::Attacker,
            0.5,
        );
        let fought = reward(
            &outcome(Some(Side::Defender), [4, 3], [0, 0]),
            Side::Attacker,
            0.5,
        );
        let fled = reward(
            &outcome(Some(Side::Defender), [0, 0], [4, 0]),
            Side::Attacker,
            0.5,
        );
        assert_eq!(wipe, -1.0);
        assert!(fought > wipe && fled > wipe);
        // Saved units count like destroyed ones: three kills against three saved.
        let three_saved = reward(
            &outcome(Some(Side::Defender), [1, 0], [3, 0]),
            Side::Attacker,
            0.5,
        );
        assert_eq!(fought, three_saved);
        assert!(fought < 0.0 && fled < 0.0, "a loss stays below any win");
        // The same for the other side.
        assert_eq!(
            reward(
                &outcome(Some(Side::Defender), [4, 0], [0, 0]),
                Side::Defender,
                0.0
            ),
            1.0
        );
    }

    #[test]
    fn losing_or_destroying_dear_units_counts_for_more() {
        // Four units sent, one lost: a cheap one (value 1) or the dearest (value 5 of 8).
        let mut cheap = outcome(Some(Side::Attacker), [1, 6], [0, 0]);
        cheap.sent_value = [8.0, 6.0];
        cheap.lost_value = [1.0, 6.0];
        let mut dear = cheap;
        dear.lost_value = [5.0, 6.0];
        assert!(reward(&dear, Side::Attacker, 0.5) < reward(&cheap, Side::Attacker, 0.5));
        // In a loss, killing the enemy's dear unit is worth more than killing a cheap one.
        let mut kill_cheap = outcome(Some(Side::Defender), [4, 1], [0, 0]);
        kill_cheap.sent_value = [4.0, 8.0];
        kill_cheap.lost_value = [4.0, 1.0];
        let mut kill_dear = kill_cheap;
        kill_dear.lost_value = [4.0, 5.0];
        assert!(reward(&kill_dear, Side::Attacker, 0.5) > reward(&kill_cheap, Side::Attacker, 0.5));
    }

    #[test]
    fn units_below_half_health_count_a_tenth_in_the_loser_s_favour_and_a_tenth_against_survivors() {
        // A loss: an enemy left below half health is worth a tenth of its value.
        let plain = outcome(Some(Side::Defender), [4, 2], [0, 0]);
        let mut hurt = plain;
        hurt.standing_hurt_value = [0.0, 3.0];
        let gain = reward(&hurt, Side::Attacker, 0.5) - reward(&plain, Side::Attacker, 0.5);
        assert!((gain - 0.3 / 10.0).abs() < 1e-6, "{gain}");
        // A loss with units saved by fleeing: those below half health count nine tenths.
        let mut fled = outcome(Some(Side::Defender), [0, 0], [4, 0]);
        let whole = reward(&fled, Side::Attacker, 0.5);
        fled.saved_hurt_value = [2.0, 0.0];
        let less = whole - reward(&fled, Side::Attacker, 0.5);
        assert!((less - 0.2 / 10.0).abs() < 1e-6, "{less}");
        // A win: a survivor below half health counts as a tenth lost.
        let won = outcome(Some(Side::Attacker), [0, 6], [0, 0]);
        let mut worn = won;
        worn.standing_hurt_value = [2.0, 0.0];
        let cost = reward(&won, Side::Attacker, 0.5) - reward(&worn, Side::Attacker, 0.5);
        assert!((cost - 0.5 * 0.2 / 4.0).abs() < 1e-6, "{cost}");
    }
}
