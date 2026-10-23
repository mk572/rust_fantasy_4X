//! Whether to raze a city just taken (`docs/ai-design.md`, the original's `Raze_Check`). The
//! decision is the AI's: the core only offers the window (`Command::RazeCity`) to the taker until
//! its turn ends. The score follows the original: the wizard's temper, too few troops left to
//! garrison the city, and how the two wizards stand on the land round it, then a roll of 100
//! against the score. A city of the independents is never razed.

use f4x_core::diplomacy::Treaty;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::wizard::objective::Personality;

use crate::Ai;

/// How near a city counts as being on the same land: the AI cannot count the armies of the
/// former owner (it sees what a human sees), so it counts the cities each side holds near it.
const NEAR: u32 = 14;

/// The score of razing `city`, which `from` held until now; the city is the taker's.
fn score(view: &PlayerView, at: Position, from: PlayerId) -> i32 {
    let mut score = match view.wizard().map(|w| w.personality()) {
        Some(Personality::Maniacal) => 25,
        Some(Personality::Ruthless) => 10,
        // The original asks whether the hidden relation is not zero, which is a slip for being
        // at war: a lawful wizard razes only the cities of a wizard it fights.
        Some(Personality::Lawful) => {
            if view.treaty_with(from) == Treaty::War {
                25
            } else {
                -200
            }
        }
        Some(Personality::Peaceful) => -200,
        _ => 0,
    };
    // Too few troops to garrison it: "if I cannot have it, no one can".
    if view.garrison(at).len() < 3 {
        score += 10;
    }
    let near = |p: Position| view.distance(at, p).is_some_and(|d| d <= NEAR);
    let theirs = view
        .known_cities()
        .into_iter()
        .filter(|&(_, o, p)| o == from && near(p))
        .count() as i32;
    let mine = view
        .my_cities()
        .into_iter()
        .filter(|&(_, p)| near(p))
        .count() as i32;
    if theirs == 0 {
        score -= 200;
    } else {
        score += mine * 50 / theirs - 50;
    }
    if matches!(
        view.wizard().map(|w| w.personality()),
        Some(Personality::Chaotic)
    ) {
        score = score.max(10);
    }
    score
}

/// Decide for every city of a wizard taken in the events so far this turn.
pub(crate) fn run(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let taken: Vec<(hecs::Entity, PlayerId)> = events
        .iter()
        .filter_map(|e| match e {
            Event::CityCaptured { city, from, to } if *to == me => Some((*city, *from)),
            _ => None,
        })
        .collect();
    for (city, from) in taken {
        let wanted = {
            let view = PlayerView::new(game, me);
            if !view.is_rival_wizard(from) {
                continue;
            }
            let Some(at) = view
                .my_cities()
                .into_iter()
                .find(|&(e, _)| e == city)
                .map(|(_, p)| p)
            else {
                continue;
            };
            score(&view, at, from)
        };
        if game.roll(100) as i32 <= wanted {
            ai.send(game, Command::RazeCity { player: me, city }, events);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, COAST_CITY, ENEMY_CITY, P0, P1};

    #[test]
    fn a_peaceful_wizard_never_razes_and_a_maniacal_one_with_nothing_left_nearby_does_not_either() {
        let mut g = game(COAST_CITY, "high_men");
        let city = g.economy().city_at(at(ENEMY_CITY.0, ENEMY_CITY.1)).unwrap();
        // Take the enemy's only city: the former owner has nothing else near, so even the
        // fiercest temper does not raze it (-200 for no rival land on the continent).
        g.wizard_mut(P0).unwrap().put(Some(Personality::Maniacal));
        g.conquer_city(city, P0, 0);
        let view = PlayerView::new(&g, P0);
        let pos = at(ENEMY_CITY.0, ENEMY_CITY.1);
        assert!(score(&view, pos, P1) < 0);
    }

    #[test]
    fn the_temper_sets_the_start_of_the_score() {
        let mut g = game(COAST_CITY, "high_men");
        let enemy = at(ENEMY_CITY.0, ENEMY_CITY.1);
        let other = g.economy().city_at(enemy).unwrap();
        // Give the former owner a second city near the first, so there is land to fight over.
        g.apply(Command::FoundCity {
            owner: P1,
            pos: at(ENEMY_CITY.0 + 4, ENEMY_CITY.1),
            name: "Second".into(),
            race: None,
            population: 4,
        });
        g.conquer_city(other, P0, 0);
        let mut scores = Vec::new();
        for p in [
            Personality::Maniacal,
            Personality::Ruthless,
            Personality::Aggressive,
            Personality::Peaceful,
        ] {
            g.wizard_mut(P0).unwrap().put(Some(p));
            let view = PlayerView::new(&g, P0);
            scores.push(score(&view, enemy, P1));
        }
        assert_eq!(scores[0] - scores[2], 25);
        assert_eq!(scores[1] - scores[2], 10);
        assert_eq!(scores[2] - scores[3], 200);
    }
}
