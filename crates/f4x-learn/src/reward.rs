//! The reward of a battle (`docs/learning-ai.md` 7.4), from a finished [`Outcome`].

use f4x_core::battle::Side;

use crate::env::Outcome;

/// A win scores `1 - stakes * (own units lost / own units sent)`. A loss scores `-1` plus the
/// enemy units destroyed and own units saved by fleeing, over all the units sent by both sides,
/// so it never reaches the lowest win. Units are counted: the game has no unit value yet.
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
    let sent = o.sent[me].max(1) as f32;
    if o.winner == Some(side) {
        let lost = (o.lost[me] as f32 / sent).clamp(0.0, 1.0);
        1.0 - stakes * lost
    } else {
        let credit = (o.lost[them] + o.saved_by_fleeing[me]) as f32;
        let all = (o.sent[me] + o.sent[them]).max(1) as f32;
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
}
