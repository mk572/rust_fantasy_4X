//! Where the wizard stands in the game, which the other choices follow (task 535): the power
//! split, how many soldiers go raiding, how bold the armies are. It is read from what the wizard
//! can see, never from a counter, and it can go back (a lost war that costs cities).

use f4x_core::player::view::PlayerView;

use crate::wizardry::{kind, Kind};

/// The turn from which a game is in its late phase whatever else is true.
const LATE_TURN: u32 = 300;
/// The turn before which a wizard that has not settled its cities is still building up.
const EARLY_TURN: u32 = 200;

/// The three phases of a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Found and grow cities; research and cast little.
    Build,
    /// Scout, take sites and neutral cities, hold the borders, start casting.
    Secure,
    /// Win by conquest or by the Spell of Mastery.
    Win,
}

/// Is the Spell of Mastery known, being researched or being cast?
fn mastery_under_way(view: &PlayerView) -> bool {
    let Some(w) = view.wizard() else {
        return false;
    };
    let is_mastery = |s| kind(view.spell(s)) == Kind::Mastery;
    w.researching().is_some_and(is_mastery)
        || w.casting().is_some_and(|c| is_mastery(c.spell))
        || w.known().into_iter().any(is_mastery)
}

/// The phase the wizard is in.
pub fn phase(view: &PlayerView) -> Phase {
    let wanted = crate::objective::traits(view).cities_wanted;
    let cities = view.my_cities().len();
    let turn = view.turn();
    if mastery_under_way(view) || turn >= LATE_TURN || cities >= 2 * wanted {
        Phase::Win
    } else if cities < wanted && turn < EARLY_TURN {
        Phase::Build
    } else {
        Phase::Secure
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{fresh_game, P1};

    #[test]
    fn a_new_wizard_with_one_city_is_building_up() -> Result<(), Box<dyn std::error::Error>> {
        let g = fresh_game()?;
        assert_eq!(phase(&PlayerView::new(&g, P1)), Phase::Build);
        Ok(())
    }

    #[test]
    fn a_late_turn_is_the_winning_phase() -> Result<(), Box<dyn std::error::Error>> {
        let mut g = fresh_game()?;
        g.turn = LATE_TURN;
        assert_eq!(phase(&PlayerView::new(&g, P1)), Phase::Win);
        g.turn = EARLY_TURN;
        // Not enough cities, but the early turns are over: secure.
        assert_eq!(phase(&PlayerView::new(&g, P1)), Phase::Secure);
        Ok(())
    }

    #[test]
    fn the_spell_of_mastery_known_is_the_winning_phase() -> Result<(), Box<dyn std::error::Error>> {
        let mut g = fresh_game()?;
        let mastery = g
            .content()
            .rules
            .spells
            .iter()
            .find(|(_, d)| kind(d) == Kind::Mastery)
            .map(|(id, _)| id)
            .ok_or("a spell of mastery")?;
        g.cheat_learn_spell(P1, mastery);
        assert_eq!(phase(&PlayerView::new(&g, P1)), Phase::Win);
        Ok(())
    }
}
