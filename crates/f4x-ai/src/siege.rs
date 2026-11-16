//! Mana as strength at a hard site (task 502). A tower's guards are worth 900 to 3,900; nine
//! soldiers alone do not beat them, but the wizard's battle spells (the battle AI casts from the
//! pool, `battle/cast.rs`) do the work of more soldiers. So the party that goes to a site is judged
//! by its soldiers plus the spells the pool can pay for, and a wizard that has a hard tower in
//! reach and not the mana for it turns surplus gold into mana (`wizardry::alchemy`) until it has.
//!
//! The AI plays by the human's rules: it counts only the spells it knows and the guards it has
//! seen or guessed (`site::expected`).

use f4x_core::player::view::PlayerView;
use f4x_def::rules::CastContext;

use crate::military::average_strength;
use crate::view::{fighting_strength, is_fighter};

/// Mana that does the work of one average soldier in a fight (a bolt or a blessing: 15 to 25).
pub(crate) const MANA_PER_SOLDIER: f32 = 20.0;
/// Most soldiers' worth the spells are counted for: a battle lasts a few rounds.
pub(crate) const MOST_SOLDIERS: f32 = 6.0;
/// The margin the party and its spells need over the guards the wizard expects.
const SIEGE_EDGE: f32 = 1.2;

/// Does the wizard know a spell it can cast in a battle?
fn has_battle_spell(view: &PlayerView) -> bool {
    view.wizard().is_some_and(|w| {
        w.known()
            .iter()
            .any(|&s| view.spell(s).contexts.contains(&CastContext::Battle))
    })
}

/// The mana a fight can use: the pool, up to the casting skill (a wizard casts only so much in
/// one battle).
fn usable_mana(view: &PlayerView) -> f32 {
    let Some(w) = view.wizard() else {
        return 0.0;
    };
    let skill = w.casting_skill(&view.content().rules).max(0) as f32;
    (view.treasury().mana.floor().max(0) as f32).min(skill * 3.0)
}

/// The strength the wizard's spells add to a party in a site fight: nothing without a battle
/// spell, else the soldiers the usable mana is worth, at most [`MOST_SOLDIERS`].
pub(crate) fn spell_power(view: &PlayerView) -> f32 {
    if !has_battle_spell(view) {
        return 0.0;
    }
    (usable_mana(view) / MANA_PER_SOLDIER).min(MOST_SOLDIERS) * average_strength(view)
}

/// The mana the wizard wants in the pool for the hardest intact tower in reach that its nine best
/// soldiers cannot take alone, but soldiers and spells together could (0 when there is none, or
/// when the wizard has no battle spell).
pub(crate) fn mana_goal(view: &PlayerView) -> i32 {
    if !has_battle_spell(view) || crate::phase::phase(view) == crate::phase::Phase::Build {
        return 0;
    }
    let fighters: Vec<_> = view
        .my_units()
        .into_iter()
        .filter(|&u| is_fighter(view, u) && !crate::recover::needs_rest(view, u))
        .collect();
    let best_nine = fighting_strength(view, &fighters);
    let each = average_strength(view).max(1.0);
    let reachable = best_nine + MOST_SOLDIERS * each;
    let Some(start) = view.my_cities().first().map(|&(_, p)| p) else {
        return 0;
    };
    view.towers()
        .into_iter()
        .filter(|&(p, intact)| {
            intact && p.plane == start.plane && crate::view::distance(view, start, p).is_some()
        })
        .map(|(p, _)| crate::site::expected(view, p) * SIEGE_EDGE)
        .filter(|&need| need > best_nine && need <= reachable)
        .map(|need| ((need - best_nine) / each * MANA_PER_SOLDIER).ceil() as i32)
        .min()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};
    use f4x_core::game::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Teach the wizard a spell it can cast in a battle (`fire_bolt`, 15 mana).
    fn learn_a_battle_spell(g: &mut Game) -> TestResult {
        let id = g
            .content()
            .rules
            .spells
            .id_numbered(5)
            .ok_or("a battle spell is in the content")?;
        let spell = g.content().rules.spells.get(id);
        assert!(spell.contexts.contains(&CastContext::Battle));
        g.cheat_learn_spell(P0, id);
        g.wizard_mut(P0)
            .ok_or("player 0 is a wizard")?
            .put(Some(f4x_core::wizard::sheet::Skill {
                points: 400,
                left: 10,
            }));
        Ok(())
    }

    fn set_mana(g: &mut Game, mana: i32) -> TestResult {
        g.treasury_mut(P0).ok_or("a treasury")?.mana = f4x_def::half_int::HalfInt::from_int(mana);
        Ok(())
    }

    #[test]
    fn spells_add_strength_only_with_a_battle_spell_and_mana_to_cast_it() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut g, P0, at(5, 5), 113 /* high_men_paladins */, 3);
        set_mana(&mut g, 200)?;
        assert_eq!(spell_power(&PlayerView::new(&g, P0)), 0.0, "no spell known");
        learn_a_battle_spell(&mut g)?;
        let rich = spell_power(&PlayerView::new(&g, P0));
        assert!(rich > 0.0);
        set_mana(&mut g, 0)?;
        assert_eq!(spell_power(&PlayerView::new(&g, P0)), 0.0, "no mana");
        Ok(())
    }

    #[test]
    fn a_tower_the_soldiers_cannot_take_alone_is_a_goal_for_mana() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.turn = 400; // the end of the game: not a build-up
        spawn(&mut g, P0, at(10, 6), 113 /* high_men_paladins */, 1);
        crate::testworld::spawn_tower(&mut g, at(33, 9), true)?;
        g.reveal_map(P0);
        assert_eq!(mana_goal(&PlayerView::new(&g, P0)), 0, "no battle spell");
        learn_a_battle_spell(&mut g)?;
        let goal = mana_goal(&PlayerView::new(&g, P0));
        assert!(
            goal > 0,
            "one soldier is short of a tower's guess of guards"
        );
        // The goal is spells for the shortfall only: a tenth soldier more and it shrinks.
        spawn(&mut g, P0, at(10, 6), 113 /* high_men_paladins */, 4);
        assert!(mana_goal(&PlayerView::new(&g, P0)) < goal);
        Ok(())
    }

    #[test]
    fn a_party_with_mana_for_spells_is_enough_for_a_tower_it_could_not_take_without() -> TestResult
    {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let paladins = spawn(&mut g, P0, at(10, 6), 113 /* high_men_paladins */, 1);
        crate::testworld::spawn_tower(&mut g, at(33, 9), true)?;
        g.reveal_map(P0);
        learn_a_battle_spell(&mut g)?;
        set_mana(&mut g, 200)?;
        let view = PlayerView::new(&g, P0);
        let soldiers = crate::view::stack_strength(&view, &paladins);
        let with = soldiers + spell_power(&view);
        let near = |s| crate::site::sites_near(&view, at(10, 6), s);
        assert!(near(soldiers).is_empty());
        assert_eq!(near(with), vec![at(33, 9)]);
        Ok(())
    }
}
