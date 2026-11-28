//! Where the wizard stands in the game, which the other choices follow (task 502): the power
//! split, how many soldiers go raiding, how bold the armies are. It is read from what the wizard
//! can see, never from a counter, and it can go back (a lost war that costs cities).

use crate::lever::Levered;
use f4x_core::player::view::PlayerView;

use crate::wizardry::{kind, Kind};

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
    let is_mastery = |s| kind(&view.content().rules, view.spell(s)) == Kind::Mastery;
    w.researching().is_some_and(is_mastery)
        || w.casting().is_some_and(|c| is_mastery(c.spell))
        || w.known().into_iter().any(is_mastery)
}

/// What the wizard puts its resources into once it has settled: the Spell of Mastery or the
/// conquest of the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    /// The wizard is on its way to the Spell of Mastery (researching, casting or holding it): gold
    /// and mana go to its casting, and the buildings that make mana.
    Mastery,
    /// The wizard wins by taking the rivals' cities: gold and mana go to units, heroes, summons
    /// and battle spells. This is also the goal when the Spell of Mastery is not open to it.
    Conquest,
}

/// The goal of the wizard (task 502): Mastery only while it is open to the wizard and under way.
pub fn goal(view: &PlayerView) -> Goal {
    if view.mastery_open() && mastery_under_way(view) {
        Goal::Mastery
    } else {
        Goal::Conquest
    }
}

/// The phase the wizard is in.
pub fn phase(view: &PlayerView, round: u32) -> Phase {
    let wanted = crate::objective::traits(view).cities_wanted;
    let cities = view.my_cities().len();
    let turn = round;
    if mastery_under_way(view)
        || turn >= view.levers().planning.late_turn
        || cities >= view.levers().planning.win_cities_times * wanted
    {
        Phase::Win
    } else if cities < wanted && turn < view.levers().planning.early_turn {
        Phase::Build
    } else {
        Phase::Secure
    }
}

/// How the wizard should grow: 0 is all tall (buildings, research, housing, magic), 1 is all wide
/// (settlers, armies, ships, conquest). It follows the wizard's race, retorts and books, the
/// phase of the game and the situation: room to settle, and neighbours stronger or weaker than
/// the wizard.
pub fn width(view: &PlayerView, room_for_city: bool, round: u32) -> f32 {
    let k = &view.levers().width;
    let mut w: f32 = k.start;
    if let Some(m) = view.wizard() {
        let rules = &view.content().rules;
        if let Some(race) = m.capital_race() {
            let num = rules.races.get(race).num;
            if k.wide_races.contains(&num) {
                w += k.race;
            } else if k.tall_races.contains(&num) {
                w -= k.race;
            }
        }
        for retort in m.retorts() {
            let num = rules.retorts.get(retort).num;
            if k.wide_retorts.contains(&num) {
                w += k.retort;
            } else if k.tall_retorts.contains(&num) {
                w -= k.retort;
            }
        }
        // The wizard's temper and aim: the aggressive and the militarist and expansionist go
        // wide, the peaceful, theurgist and perfectionist build up.
        use f4x_core::wizard::objective::{Objective, Personality};
        w += match m.personality() {
            Personality::Maniacal | Personality::Ruthless => k.temper_fierce,
            Personality::Aggressive => k.temper_aggressive,
            Personality::Chaotic => 0.0,
            Personality::Lawful => -k.temper_lawful,
            Personality::Peaceful => -k.temper_peaceful,
        };
        w += match m.objective() {
            Some(Objective::Militarist) | Some(Objective::Expansionist) => k.objective,
            Some(Objective::Theurgist) | Some(Objective::Perfectionist) => -k.objective,
            Some(Objective::Pragmatist) | None => 0.0,
        };
        // The books of some realms favour the fighter, the others the builder.
        let books = |realms: &[u16]| {
            realms
                .iter()
                .map(|&n| m.books_in(f4x_def::rules::RealmId::new(n)))
                .sum::<i32>()
        };
        w += k.wide_book * books(&k.wide_realms) as f32;
        w -= k.tall_book * books(&k.tall_realms) as f32;
    }
    w += match phase(view, round) {
        Phase::Build => k.phase_build,
        Phase::Secure => 0.0,
        Phase::Win if mastery_under_way(view) => -k.phase_mastery,
        Phase::Win => k.phase_win,
    };
    w += if room_for_city { k.room } else { -k.room };
    // Neighbours: more cities than the wizard hold it back, fewer invite it out.
    let mine = view.my_cities().len() as f32;
    let theirs: Vec<f32> = view
        .rivals_at_war()
        .into_iter()
        .map(|p| {
            view.known_cities()
                .iter()
                .filter(|&&(_, o, _)| o == p)
                .count() as f32
        })
        .filter(|&n| n > 0.0)
        .collect();
    if let Some(strongest) = theirs.iter().copied().reduce(f32::max) {
        if strongest > mine * k.neighbour_ratio {
            w -= k.neighbour;
        } else if strongest * k.neighbour_ratio < mine {
            w += k.neighbour;
        }
    }
    w.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{fresh_game, P1};

    #[test]
    fn a_new_wizard_with_one_city_is_building_up() -> Result<(), Box<dyn std::error::Error>> {
        let w = fresh_game()?;
        assert_eq!(phase(&PlayerView::new(&w, P1), w.round()), Phase::Build);
        Ok(())
    }

    #[test]
    fn a_new_wizard_with_room_to_settle_leans_wide() -> Result<(), Box<dyn std::error::Error>> {
        let w = fresh_game()?;
        let value = width(&PlayerView::new(&w, P1), true, w.round());
        assert!(value > 0.5, "build phase and room: {value}");
        assert!((0.0..=1.0).contains(&value));
        Ok(())
    }

    #[test]
    fn a_wizard_with_no_site_left_leans_less_wide() -> Result<(), Box<dyn std::error::Error>> {
        let w = fresh_game()?;
        let view = PlayerView::new(&w, P1);
        assert!(width(&view, false, w.round()) < width(&view, true, w.round()));
        Ok(())
    }

    #[test]
    fn a_wizard_about_to_win_with_the_spell_of_mastery_leans_tall(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut w = fresh_game()?;
        let before = width(&PlayerView::new(&w, P1), true, w.round());
        let mastery = w
            .content()
            .rules
            .spells
            .iter()
            .find(|(_, d)| kind(&w.content().rules, d) == Kind::Mastery)
            .map(|(id, _)| id)
            .ok_or("a spell of mastery")?;
        w.cheat_learn_spell(P1, mastery);
        assert!(width(&PlayerView::new(&w, P1), true, w.round()) < before);
        Ok(())
    }

    #[test]
    fn a_late_turn_is_the_winning_phase() -> Result<(), Box<dyn std::error::Error>> {
        let mut w = fresh_game()?;
        w.set_round(crate::testworld::levers().planning.late_turn);
        assert_eq!(phase(&PlayerView::new(&w, P1), w.round()), Phase::Win);
        w.set_round(crate::testworld::levers().planning.early_turn);
        // Not enough cities, but the early turns are over: secure.
        assert_eq!(phase(&PlayerView::new(&w, P1), w.round()), Phase::Secure);
        Ok(())
    }

    #[test]
    fn the_spell_of_mastery_known_is_the_winning_phase() -> Result<(), Box<dyn std::error::Error>> {
        let mut w = fresh_game()?;
        let mastery = w
            .content()
            .rules
            .spells
            .iter()
            .find(|(_, d)| kind(&w.content().rules, d) == Kind::Mastery)
            .map(|(id, _)| id)
            .ok_or("a spell of mastery")?;
        w.cheat_learn_spell(P1, mastery);
        assert_eq!(phase(&PlayerView::new(&w, P1), w.round()), Phase::Win);
        Ok(())
    }

    #[test]
    fn a_wizard_is_after_conquest_until_the_spell_of_mastery_is_under_way(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut w = fresh_game()?;
        assert_eq!(goal(&PlayerView::new(&w, P1)), Goal::Conquest);
        let mastery = w
            .content()
            .rules
            .spells
            .iter()
            .find(|(_, d)| kind(&w.content().rules, d) == Kind::Mastery)
            .map(|(id, _)| id)
            .ok_or("a spell of mastery")?;
        w.cheat_learn_spell(P1, mastery);
        assert_eq!(goal(&PlayerView::new(&w, P1)), Goal::Mastery);
        Ok(())
    }
}
