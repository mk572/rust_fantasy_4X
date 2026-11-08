//! Where the wizard stands in the game, which the other choices follow (task 502): the power
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

/// Races whose strength is in the field (wide), and those whose strength is in building and
/// magic (tall). A starting point to tune; the rest are neutral.
const WIDE_RACES: &[&str] = &[
    "orc",
    "barbarian",
    "troll",
    "gnoll",
    "beastmen",
    "draconian",
];
const TALL_RACES: &[&str] = &["high_elf", "dark_elf", "halfling", "dwarven", "nomad"];
/// Retorts that favour spreading out (heroes, armies, summons) and building up (research, mana,
/// gold, items).
const WIDE_RETORTS: &[&str] = &["warlord", "famous", "charismatic", "conjurer"];
const TALL_RETORTS: &[&str] = &[
    "sage_master",
    "alchemy",
    "channeler",
    "mana_focusing",
    "archmage",
    "node_mastery",
    "runemaster",
    "artificer",
];

/// How the wizard should grow: 0 is all tall (buildings, research, housing, magic), 1 is all wide
/// (settlers, armies, ships, conquest). It follows the wizard's race, retorts and books, the
/// phase of the game and the situation: room to settle, and neighbours stronger or weaker than
/// the wizard.
pub fn width(view: &PlayerView) -> f32 {
    let mut w: f32 = 0.5;
    if let Some(m) = view.wizard() {
        let rules = &view.content().rules;
        if let Some(race) = m.capital_race() {
            let id = rules.races.get(race).id.as_str();
            if WIDE_RACES.contains(&id) {
                w += 0.1;
            } else if TALL_RACES.contains(&id) {
                w -= 0.1;
            }
        }
        for retort in m.retorts() {
            let id = rules.retorts.get(retort).id.as_str();
            if WIDE_RETORTS.contains(&id) {
                w += 0.08;
            } else if TALL_RETORTS.contains(&id) {
                w -= 0.08;
            }
        }
        // The wizard's temper and aim: the aggressive and the militarist and expansionist go
        // wide, the peaceful, theurgist and perfectionist build up.
        use f4x_core::wizard::objective::{Objective, Personality};
        w += match m.personality() {
            Personality::Maniacal | Personality::Ruthless => 0.12,
            Personality::Aggressive => 0.08,
            Personality::Chaotic => 0.0,
            Personality::Lawful => -0.04,
            Personality::Peaceful => -0.1,
        };
        w += match m.objective() {
            Some(Objective::Militarist) | Some(Objective::Expansionist) => 0.1,
            Some(Objective::Theurgist) | Some(Objective::Perfectionist) => -0.1,
            Some(Objective::Pragmatist) | None => 0.0,
        };
        // Chaos and death books favour the fighter, the others the builder.
        let books = |n: u16| m.books_in(f4x_def::rules::RealmId::new(n));
        w += 0.03 * (books(2) + books(4)) as f32;
        w -= 0.02 * (books(0) + books(1) + books(3)) as f32;
    }
    w += match phase(view) {
        Phase::Build => 0.2,
        Phase::Secure => 0.0,
        Phase::Win if mastery_under_way(view) => -0.2,
        Phase::Win => 0.1,
    };
    w += if crate::expand::room_for_city(view) {
        0.2
    } else {
        -0.2
    };
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
        if strongest > mine * 1.5 {
            w -= 0.1;
        } else if strongest * 1.5 < mine {
            w += 0.1;
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
        let g = fresh_game()?;
        assert_eq!(phase(&PlayerView::new(&g, P1)), Phase::Build);
        Ok(())
    }

    #[test]
    fn a_new_wizard_with_room_to_settle_leans_wide() -> Result<(), Box<dyn std::error::Error>> {
        let g = fresh_game()?;
        let w = width(&PlayerView::new(&g, P1));
        assert!(w > 0.5, "build phase and room: {w}");
        assert!((0.0..=1.0).contains(&w));
        Ok(())
    }

    #[test]
    fn a_wizard_about_to_win_with_the_spell_of_mastery_leans_tall(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut g = fresh_game()?;
        let before = width(&PlayerView::new(&g, P1));
        let mastery = g
            .content()
            .rules
            .spells
            .iter()
            .find(|(_, d)| kind(d) == Kind::Mastery)
            .map(|(id, _)| id)
            .ok_or("a spell of mastery")?;
        g.cheat_learn_spell(P1, mastery);
        assert!(width(&PlayerView::new(&g, P1)) < before);
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
