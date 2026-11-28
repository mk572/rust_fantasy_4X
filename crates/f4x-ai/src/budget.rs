//! The mana a battle may take from the reserve (task 601).
//!
//! The battle AI spends all it may in a fight (`battle/cast.rs`: the casting skill of the battle
//! and the reserve minus the upkeep). The strategic AI lowers that for a battle it starts, by
//! three things, the way a player holds back:
//!
//! - **the reserve is thin**: a wizard whose income does not cover its upkeep keeps
//!   `hold_turns` turns of the shortfall;
//! - **the fight is likely won anyway**: the more the attackers outweigh the defence, the less
//!   mana they are worth;
//! - **little is at stake**: a fight in the open away from home, with no hero in it, is worth
//!   less than one for a city, the fortress, a lair or our own ground.
//!
//! The share of what is left that may be spent is the stake times the need; a share this big
//! or bigger is no limit at all. The human has the same choice in the attack prompt
//! (`Command::AutoResolve::mana_budget`); the defender of a battle spends what it may.

use f4x_def::rules::ai_strategy::BattleSpendLevers;

use crate::lever::Levered;
use crate::view::fighting_strength;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;

/// What a fight is worth, 0 to 1.
pub(crate) fn stake(
    view: &PlayerView,
    target: Position,
    attackers: &[hecs::Entity],
    defenders: &[hecs::Entity],
) -> f32 {
    let levers = view.levers().battle_spend;
    let at_home = view.my_cities().iter().any(|&(_, p)| {
        view.distance(p, target)
            .is_some_and(|d| d <= levers.home_range)
    });
    let place: f32 = if view.city_at(target).is_some() {
        levers.stake_city
    } else if view.has_lair(target) {
        levers.stake_lair
    } else if at_home {
        levers.stake_home
    } else {
        levers.stake_open
    };
    // Heroes put at risk, or an enemy hero or wizard's unit to kill, make any fight matter more.
    let heroes = view.my_heroes();
    let hero_in = attackers.iter().any(|a| heroes.contains(a));
    let enemy_hero = defenders
        .iter()
        .any(|&d| view.unit_def(d).is_some_and(|def| def.hero.is_some()));
    if hero_in || enemy_hero {
        place.max(levers.stake_hero)
    } else {
        place
    }
}

/// How much the battle needs its spells, 1 for an even fight to `least_need` for a sure win.
pub(crate) fn need(levers: &BattleSpendLevers, ratio: f32) -> f32 {
    let least = levers.least_need;
    if !ratio.is_finite() || ratio >= levers.sure_ratio {
        return least;
    }
    let span = (levers.sure_ratio - levers.even_ratio).max(f32::EPSILON);
    (1.0 - (ratio - levers.even_ratio).max(0.0) / span * (1.0 - least)).clamp(least.min(1.0), 1.0)
}

/// The mana the battle may take, or `None` for no limit beyond the battle's own: `reserve` is
/// the mana held, `income` the mana a turn leaves after the upkeep (below 0 when the upkeep is
/// the bigger), `stake` and `ratio` as above.
pub(crate) fn allowance(
    levers: &BattleSpendLevers,
    reserve: i32,
    net_income: i32,
    stake: f32,
    ratio: f32,
) -> Option<u32> {
    let kept = (-net_income).max(0) * levers.hold_turns;
    let spare = (reserve - kept).max(0);
    let share = stake.clamp(0.0, 1.0) * need(levers, ratio);
    if share >= levers.no_limit && kept == 0 {
        return None;
    }
    Some((spare as f32 * share.min(1.0)).floor() as u32)
}

/// The limit for an attack by `me` on `defenders` with `attackers`, as `AutoResolve` takes it.
pub(crate) fn for_attack(
    view: &PlayerView,
    attackers: &[hecs::Entity],
    defenders: &[hecs::Entity],
) -> Option<(PlayerId, u32)> {
    // Only a wizard has a reserve to spend (a monster or a neutral raid has none).
    view.wizard()?;
    let target = view.unit_position(*defenders.first()?)?;
    let theirs = fighting_strength(view, defenders)
        * if view.city_at(target).is_some() {
            view.levers().shared.city_defence_factor
        } else {
            1.0
        };
    let ratio = if theirs > 0.0 {
        fighting_strength(view, attackers) / theirs
    } else {
        f32::INFINITY
    };
    let income = view.income();
    let net = (income.split.mana - income.mana_upkeep).floor();
    let mana = allowance(
        &view.levers().battle_spend,
        view.treasury().mana.floor(),
        net,
        stake(view, target, attackers, defenders),
        ratio,
    )?;
    Some((view.me(), mana))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spend() -> BattleSpendLevers {
        crate::testworld::levers().battle_spend
    }
    use f4x_master::Game;

    #[test]
    fn an_even_fight_needs_everything_and_a_sure_win_little() {
        assert_eq!(need(&spend(), 0.5), 1.0);
        assert_eq!(need(&spend(), spend().even_ratio), 1.0);
        assert_eq!(need(&spend(), spend().sure_ratio), spend().least_need);
        assert_eq!(need(&spend(), f32::INFINITY), spend().least_need);
        assert!(
            need(&spend(), 2.0) < need(&spend(), 1.5) && need(&spend(), 2.0) > need(&spend(), 2.5)
        );
    }

    #[test]
    fn a_fight_that_matters_and_is_not_won_has_no_limit() {
        assert_eq!(allowance(&spend(), 500, 5, 1.0, 1.0), None);
        // A city taken by a sure winner may use a fifth of what is spare.
        assert_eq!(allowance(&spend(), 500, 5, 1.0, 4.0), Some(100));
    }

    #[test]
    fn little_at_stake_means_a_small_allowance() {
        let k = spend();
        let part = |share: f32| Some((500.0 * share).floor() as u32);
        // The open field away from home in an even fight: that part of the reserve.
        assert_eq!(allowance(&k, 500, 5, k.stake_open, 1.0), part(k.stake_open));
        // And a sure win there: less again.
        assert_eq!(
            allowance(&k, 500, 5, k.stake_open, k.sure_ratio),
            part(k.stake_open * k.least_need)
        );
    }

    #[test]
    fn a_wizard_short_of_mana_keeps_the_shortfall_of_ten_turns() {
        // Upkeep 20 over income: 200 held back, and then a city fight is still limited to the rest.
        assert_eq!(allowance(&spend(), 500, -20, 1.0, 1.0), Some(300));
        assert_eq!(allowance(&spend(), 150, -20, 1.0, 1.0), Some(0));
    }

    use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0, P1};

    /// The limit P0 gets for `n` spearmen against `m` of P1 on `target`, with 500 mana.
    fn limit(w: &mut Game, from: Position, target: Position, n: usize, m: usize) -> Option<u32> {
        w.treasury_mut(P0).expect("a treasury").mana = f4x_def::half_int::HalfInt::from_int(500);
        let ours = spawn(w, P0, from, 104 /* high_men_spearmen */, n);
        let theirs = spawn(w, P1, target, 104 /* high_men_spearmen */, m);
        w.reveal_map(P0);
        for_attack(&PlayerView::new(w, P0), &ours, &theirs).map(|(who, mana)| {
            assert_eq!(who, P0);
            mana
        })
    }

    #[test]
    fn a_fight_in_the_open_away_from_home_gets_a_part_of_the_reserve_and_a_city_all_of_it() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let open = limit(&mut w, at(2, 2), at(3, 2), 1, 1);
        assert!(open.is_some_and(|m| m <= 200), "{open:?}");
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let city = limit(&mut w, at(27, 6), at(ENEMY_CITY.0, ENEMY_CITY.1), 1, 1);
        assert_eq!(city, None, "an even fight for a city: no limit");
    }

    #[test]
    fn a_sure_win_gets_less_than_a_close_fight() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let close = limit(&mut w, at(2, 2), at(3, 2), 1, 1).expect("a limit");
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let sure = limit(&mut w, at(2, 2), at(3, 2), 6, 1).expect("a limit");
        assert!(sure < close, "sure {sure}, close {close}");
    }
}
