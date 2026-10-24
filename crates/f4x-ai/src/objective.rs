//! What a wizard's objective and personality change in the scripted AI (`docs/ai-objectives.md`).
//!
//! An objective changes emphasis, not capability: one row of numbers each, read where the AI
//! decides how many cities to settle, how big an army to keep, what its cities build, how it
//! splits its power and which spells it researches first. The Pragmatist row is the AI as it was
//! before objectives steered anything, and it is also the row of a wizard without an objective
//! (the human's, a test's). The personality moves the army a little and decides who starts wars
//! (`diplomacy::quarrel`).

use f4x_core::city::autobuild::Focus;
use f4x_core::player::view::PlayerView;
use f4x_core::wizard::objective::{Objective, Personality};

/// A kind of spell an objective likes to research first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Like {
    Summon,
    UnitBuff,
    CityBuff,
    GlobalBuff,
    Explore,
    /// Spells of the realms of life and arcane.
    Holy,
}

/// The plan of a city: the focus pair it follows.
pub type Focuses = (Focus, Option<Focus>);

/// One objective's numbers.
#[derive(Debug, Clone, Copy)]
pub struct Traits {
    /// The most cities settled before the AI only builds up.
    pub cities_wanted: usize,
    /// Fighters beyond the home guard, before a half per city more.
    pub strike_base: usize,
    /// The focus of a city while the wizard still expands, and once it does not.
    pub expanding: Focuses,
    pub settled: Focuses,
    /// Rows to skip in the power split's candidates: a positive number gives mana more, research
    /// less.
    pub split_shift: usize,
    /// The wizard spends its mana down to the upkeep even without a reserve: more research.
    pub lean_mana: bool,
    /// The spell kinds researched first (their cost counts as the share `like_share` less).
    pub likes: &'static [Like],
    /// What a liked spell's research cost is multiplied by.
    pub like_share: f32,
}

const PRAGMATIST: Traits = Traits {
    cities_wanted: 7,
    strike_base: 4,
    expanding: (Focus::Production, Some(Focus::Population)),
    settled: (Focus::Balanced, None),
    split_shift: 0,
    lean_mana: false,
    likes: &[],
    like_share: 1.0,
};

const MILITARIST: Traits = Traits {
    cities_wanted: 6,
    strike_base: 6,
    expanding: (Focus::Production, Some(Focus::Offense)),
    settled: (Focus::Offense, Some(Focus::Production)),
    split_shift: 1,
    likes: &[Like::Summon, Like::UnitBuff],
    like_share: 0.67,
    ..PRAGMATIST
};

const THEURGIST: Traits = Traits {
    cities_wanted: 6,
    strike_base: 3,
    expanding: (Focus::Production, Some(Focus::Magic)),
    settled: (Focus::Magic, Some(Focus::Population)),
    lean_mana: true,
    likes: &[Like::Holy],
    like_share: 0.67,
    ..PRAGMATIST
};

const PERFECTIONIST: Traits = Traits {
    cities_wanted: 5,
    strike_base: 3,
    expanding: (Focus::Population, Some(Focus::Production)),
    settled: (Focus::Magic, Some(Focus::Gold)),
    lean_mana: true,
    likes: &[Like::CityBuff, Like::GlobalBuff],
    like_share: 0.67,
    ..PRAGMATIST
};

const EXPANSIONIST: Traits = Traits {
    cities_wanted: 10,
    expanding: (Focus::Population, Some(Focus::Production)),
    settled: (Focus::Production, Some(Focus::Population)),
    likes: &[Like::Explore],
    like_share: 0.67,
    ..PRAGMATIST
};

/// The numbers of an objective.
pub fn traits_of(objective: Option<Objective>) -> Traits {
    match objective {
        None | Some(Objective::Pragmatist) => PRAGMATIST,
        Some(Objective::Militarist) => MILITARIST,
        Some(Objective::Theurgist) => THEURGIST,
        Some(Objective::Perfectionist) => PERFECTIONIST,
        Some(Objective::Expansionist) => EXPANSIONIST,
    }
}

/// The numbers of the viewing player's wizard.
pub fn traits(view: &PlayerView) -> Traits {
    traits_of(view.wizard().and_then(|w| w.objective()))
}

/// The temper in order of aggression, the gentlest first.
const TEMPERS: [Personality; 6] = [
    Personality::Peaceful,
    Personality::Lawful,
    Personality::Chaotic,
    Personality::Aggressive,
    Personality::Ruthless,
    Personality::Maniacal,
];

/// How many steps the difficulty slides every AI's temper towards Maniacal (task 148): none up to
/// Normal, one on Hard, two on Impossible. The AI always plays its best; this only makes it
/// fiercer. Ours, the original has no such thing.
pub fn aggression_steps(difficulty: u8) -> i32 {
    i32::from(difficulty.saturating_sub(2))
}

/// A temper slid by the difficulty: a peaceful wizard turns lawful or chaotic, a fierce one stays
/// Maniacal at most. The steps left over past Maniacal go to the army (`army_bias`).
pub fn slid(personality: Personality, difficulty: u8) -> (Personality, i32) {
    let rank = TEMPERS.iter().position(|&p| p == personality).unwrap_or(2) as i32;
    let want = rank + aggression_steps(difficulty);
    let top = TEMPERS.len() as i32 - 1;
    (TEMPERS[want.min(top) as usize], (want - top).max(0))
}

/// The temper the AI acts on: the drawn one, slid by the difficulty.
pub fn temper(personality: Personality, difficulty: u8) -> Personality {
    slid(personality, difficulty).0
}

/// Fighters a temper adds to (or takes from) the strike army: the cruel keep more, the peaceful
/// fewer, and a temper already at the top keeps one more per step the difficulty slides it past.
/// The original uses personality only in diplomacy; this is ours.
pub fn army_bias(personality: Personality, difficulty: u8) -> i32 {
    let (shown, over) = slid(personality, difficulty);
    let base = match shown {
        Personality::Maniacal => 2,
        Personality::Ruthless | Personality::Aggressive => 1,
        Personality::Chaotic | Personality::Lawful => 0,
        Personality::Peaceful => -1,
    };
    base + over
}

/// Whether this wizard may start a war on a weaker neighbour of its own accord: a militarist or
/// an expansionist whose temper is not lawful or peaceful (`DIPLOMAC.c`).
pub fn picks_fights(objective: Option<Objective>, personality: Personality) -> bool {
    matches!(
        objective,
        Some(Objective::Militarist | Objective::Expansionist)
    ) && !matches!(personality, Personality::Lawful | Personality::Peaceful)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pragmatist_row_is_the_ai_as_it_was() {
        let t = traits_of(None);
        assert_eq!((t.cities_wanted, t.strike_base), (7, 4));
        assert_eq!(t.settled, (Focus::Balanced, None));
        assert_eq!(t.split_shift, 0);
        assert!(t.likes.is_empty());
    }

    #[test]
    fn each_objective_moves_its_numbers_in_the_stated_direction() {
        let base = traits_of(Some(Objective::Pragmatist));
        assert!(traits_of(Some(Objective::Expansionist)).cities_wanted > base.cities_wanted);
        assert!(traits_of(Some(Objective::Militarist)).strike_base > base.strike_base);
        assert!(traits_of(Some(Objective::Militarist)).split_shift > 0);
        assert!(traits_of(Some(Objective::Perfectionist)).cities_wanted < base.cities_wanted);
        assert!(traits_of(Some(Objective::Theurgist)).lean_mana);
        for o in Objective::ALL {
            assert!(traits_of(Some(o)).like_share <= 1.0);
        }
    }

    #[test]
    fn the_difficulty_slides_every_temper_up_and_the_top_keeps_a_bigger_army() {
        // Normal and below change nothing.
        for d in 0..=2 {
            for &p in &TEMPERS {
                assert_eq!(slid(p, d), (p, 0));
            }
        }
        assert_eq!(temper(Personality::Peaceful, 3), Personality::Lawful);
        assert_eq!(temper(Personality::Peaceful, 4), Personality::Chaotic);
        assert_eq!(temper(Personality::Ruthless, 4), Personality::Maniacal);
        assert_eq!(slid(Personality::Maniacal, 4), (Personality::Maniacal, 2));
        assert_eq!(
            army_bias(Personality::Maniacal, 4),
            army_bias(Personality::Maniacal, 2) + 2
        );
        for &p in &TEMPERS {
            assert!(army_bias(p, 4) >= army_bias(p, 2));
        }
    }

    #[test]
    fn a_crueller_temper_keeps_a_bigger_army() {
        let temper = [
            Personality::Peaceful,
            Personality::Lawful,
            Personality::Chaotic,
            Personality::Aggressive,
            Personality::Ruthless,
            Personality::Maniacal,
        ];
        let bias: Vec<i32> = temper.iter().map(|&p| army_bias(p, 2)).collect();
        assert!(bias.windows(2).all(|w| w[0] <= w[1]), "{bias:?}");
        assert!(bias[0] < 0 && bias[5] > 0);
    }

    #[test]
    fn only_a_fierce_militarist_or_expansionist_picks_fights() {
        assert!(picks_fights(
            Some(Objective::Militarist),
            Personality::Aggressive
        ));
        assert!(picks_fights(
            Some(Objective::Expansionist),
            Personality::Chaotic
        ));
        assert!(!picks_fights(
            Some(Objective::Militarist),
            Personality::Peaceful
        ));
        assert!(!picks_fights(
            Some(Objective::Expansionist),
            Personality::Lawful
        ));
        assert!(!picks_fights(
            Some(Objective::Theurgist),
            Personality::Maniacal
        ));
        assert!(!picks_fights(None, Personality::Maniacal));
    }
}
