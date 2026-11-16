//! The mana a battle may take from the reserve (task 601).
//!
//! The battle AI spends all it may in a fight (`battle/cast.rs`: the casting skill of the battle
//! and the reserve minus the upkeep). The strategic AI lowers that for a battle it starts, by
//! three things, the way a player holds back:
//!
//! - **the reserve is thin**: a wizard whose income does not cover its upkeep keeps
//!   [`HOLD_TURNS`] turns of the shortfall;
//! - **the fight is likely won anyway**: the more the attackers outweigh the defence, the less
//!   mana they are worth;
//! - **little is at stake**: a fight in the open away from home, with no hero in it, is worth
//!   less than one for a city, the fortress, a lair or our own ground.
//!
//! The share of what is left that may be spent is the stake times the need; a share this big
//! or bigger is no limit at all. The human has the same choice in the attack prompt
//! (`Command::AutoResolve::mana_budget`); the defender of a battle spends what it may.

use crate::view::fighting_strength;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;

/// Turns of an upkeep shortfall the reserve keeps for itself.
const HOLD_TURNS: i32 = 10;
/// The share of the spare mana at which the limit falls away.
const NO_LIMIT: f32 = 0.75;
/// Attackers this many times the defence are certain winners: the least need.
const SURE_RATIO: f32 = 3.0;
/// Attackers this many times the defence or fewer need every spell they have.
const EVEN_RATIO: f32 = 1.2;
/// The least need there is: even a sure win may use a little mana to lose fewer units.
const LEAST_NEED: f32 = 0.2;
/// Squares from one of our cities that a fight is still on our own ground.
const HOME_RANGE: u32 = 3;

/// What a fight is worth, 0 to 1.
pub(crate) fn stake(
    view: &PlayerView,
    target: Position,
    attackers: &[hecs::Entity],
    defenders: &[hecs::Entity],
) -> f32 {
    let at_home = view
        .my_cities()
        .iter()
        .any(|&(_, p)| view.distance(p, target).is_some_and(|d| d <= HOME_RANGE));
    let place: f32 = if view.city_at(target).is_some() {
        1.0
    } else if view.has_lair(target) {
        0.6
    } else if at_home {
        0.7
    } else {
        0.3
    };
    // Heroes put at risk, or an enemy hero or wizard's unit to kill, make any fight matter more.
    let heroes = view.my_heroes();
    let hero_in = attackers.iter().any(|a| heroes.contains(a));
    let enemy_hero = defenders
        .iter()
        .any(|&d| view.unit_def(d).is_some_and(|def| def.hero.is_some()));
    if hero_in || enemy_hero {
        place.max(0.6)
    } else {
        place
    }
}

/// How much the battle needs its spells, 1 for an even fight to `LEAST_NEED` for a sure win.
pub(crate) fn need(ratio: f32) -> f32 {
    if !ratio.is_finite() || ratio >= SURE_RATIO {
        return LEAST_NEED;
    }
    let span = SURE_RATIO - EVEN_RATIO;
    (1.0 - (ratio - EVEN_RATIO).max(0.0) / span * (1.0 - LEAST_NEED)).clamp(LEAST_NEED, 1.0)
}

/// The mana the battle may take, or `None` for no limit beyond the battle's own: `reserve` is
/// the mana held, `income` the mana a turn leaves after the upkeep (below 0 when the upkeep is
/// the bigger), `stake` and `ratio` as above.
pub(crate) fn allowance(reserve: i32, net_income: i32, stake: f32, ratio: f32) -> Option<u32> {
    let kept = (-net_income).max(0) * HOLD_TURNS;
    let spare = (reserve - kept).max(0);
    let share = stake.clamp(0.0, 1.0) * need(ratio);
    if share >= NO_LIMIT && kept == 0 {
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
            1.3
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

    #[test]
    fn an_even_fight_needs_everything_and_a_sure_win_little() {
        assert_eq!(need(0.5), 1.0);
        assert_eq!(need(EVEN_RATIO), 1.0);
        assert_eq!(need(SURE_RATIO), LEAST_NEED);
        assert_eq!(need(f32::INFINITY), LEAST_NEED);
        assert!(need(2.0) < need(1.5) && need(2.0) > need(2.5));
    }

    #[test]
    fn a_fight_that_matters_and_is_not_won_has_no_limit() {
        assert_eq!(allowance(500, 5, 1.0, 1.0), None);
        // A city taken by a sure winner may use a fifth of what is spare.
        assert_eq!(allowance(500, 5, 1.0, 4.0), Some(100));
    }

    #[test]
    fn little_at_stake_means_a_small_allowance() {
        // The open field away from home (0.3) in an even fight: three tenths of the reserve.
        assert_eq!(allowance(500, 5, 0.3, 1.0), Some(150));
        // And a sure win there: a sixtieth part.
        assert_eq!(allowance(500, 5, 0.3, 4.0), Some(30));
    }

    #[test]
    fn a_wizard_short_of_mana_keeps_the_shortfall_of_ten_turns() {
        // Upkeep 20 over income: 200 held back, and then a city fight is still limited to the rest.
        assert_eq!(allowance(500, -20, 1.0, 1.0), Some(300));
        assert_eq!(allowance(150, -20, 1.0, 1.0), Some(0));
    }

    use crate::testworld::unit::HIGH_MEN_SPEARMEN;
    use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
    use f4x_core::game::Game;

    /// The limit P0 gets for `n` spearmen against `m` of P1 on `target`, with 500 mana.
    fn limit(g: &mut Game, from: Position, target: Position, n: usize, m: usize) -> Option<u32> {
        g.treasury_mut(P0).expect("a treasury").mana = f4x_def::half_int::HalfInt::from_int(500);
        let ours = spawn(g, P0, from, HIGH_MEN_SPEARMEN, n);
        let theirs = spawn(g, P1, target, HIGH_MEN_SPEARMEN, m);
        g.reveal_map(P0);
        for_attack(&PlayerView::new(g, P0), &ours, &theirs).map(|(who, mana)| {
            assert_eq!(who, P0);
            mana
        })
    }

    #[test]
    fn a_fight_in_the_open_away_from_home_gets_a_part_of_the_reserve_and_a_city_all_of_it() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let open = limit(&mut g, at(2, 2), at(3, 2), 1, 1);
        assert!(open.is_some_and(|m| m <= 200), "{open:?}");
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let city = limit(&mut g, at(27, 6), at(ENEMY_CITY.0, ENEMY_CITY.1), 1, 1);
        assert_eq!(city, None, "an even fight for a city: no limit");
    }

    #[test]
    fn a_sure_win_gets_less_than_a_close_fight() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let close = limit(&mut g, at(2, 2), at(3, 2), 1, 1).expect("a limit");
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let sure = limit(&mut g, at(2, 2), at(3, 2), 6, 1).expect("a limit");
        assert!(sure < close, "sure {sure}, close {close}");
    }
}
