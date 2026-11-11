//! A neutral player's alchemy (task 522): it has no wizard, no spells and no use for mana, so the
//! AI turns all its mana into gold every turn, at the rate of any player (`Stat::AlchemyPercent`,
//! 50% without a retort), with the ordinary `Alchemy` command. The game has no rule for this.
//!
//! Tests: in this file.

use crate::Ai;
use f4x_core::game::{Command, Event, Game};
use f4x_core::player::view::PlayerView;

/// Turn the whole mana of a player without a wizard into gold.
pub(crate) fn run(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let mana = {
        let view = PlayerView::new(game, ai.player);
        if view.wizard().is_some() {
            return;
        }
        view.treasury().mana.floor()
    };
    if mana > 0 {
        ai.send(
            game,
            Command::Alchemy {
                player: ai.player,
                amount: mana,
                gold_to_mana: false,
            },
            events,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::game::setup::{new_game, NewGame};
    use f4x_def::half_int::HalfInt;
    use f4x_def::testdir::TestResult;

    #[test]
    fn a_player_without_a_wizard_turns_its_mana_into_gold_and_a_wizard_keeps_it() -> TestResult {
        let mut g = new_game(
            f4x_def::testdir::shipped(),
            &NewGame {
                seed: 7,
                all_ai: true,
                ..Default::default()
            },
        )?;
        let race = g
            .economy()
            .cities()
            .into_iter()
            .find_map(|(e, c, _)| g.race_of_player(c.owner).and(g.city_race(e)))
            .ok_or("no neutral city")?;
        let id = f4x_core::player::race_player::race_player_id(race);
        let wizard = f4x_core::player::PlayerId(0);
        for p in [id, wizard] {
            g.treasury_mut(p).ok_or("no treasury")?.mana = HalfInt::from_int(41);
        }
        let gold = |g: &Game, p| g.player(p).map(|p| p.treasury().gold);
        let (race_gold, wizard_gold) = (gold(&g, id), gold(&g, wizard));
        for p in [id, wizard] {
            run(&Ai::new(p), &mut g, &mut Vec::new());
        }
        let mana = |g: &Game, p| g.player(p).map(|p| p.treasury().mana);
        assert_eq!(mana(&g, id), Some(HalfInt::ZERO));
        assert_eq!(
            gold(&g, id),
            race_gold.map(|g| g + HalfInt::from_int(20)),
            "41 mana at 50% is 20 gold"
        );
        assert_eq!(mana(&g, wizard), Some(HalfInt::from_int(41)));
        assert_eq!(gold(&g, wizard), wizard_gold);
        Ok(())
    }
}
