//! A wizard's personality and objective, as the original's `Init_Magic_Personalities_Objectives`
//! draws them at game start (`INITGAME.c`). The personality steers diplomacy; the
//! objective is recorded and shown on a rival's wizard sheet.

use serde::{Deserialize, Serialize};

use crate::game::Game;
use crate::player::PlayerId;
use crate::wizard::entity::Wizard;
use crate::wizard::magic::Retort;

/// On a wizard: its temper. The human has none and counts as `Chaotic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Personality {
    Maniacal,
    Ruthless,
    Aggressive,
    #[default]
    Chaotic,
    Lawful,
    Peaceful,
}

impl Personality {
    /// The key of the personality's name in `content/text/`.
    pub fn key(self) -> &'static str {
        match self {
            Personality::Maniacal => "personality.maniacal",
            Personality::Ruthless => "personality.ruthless",
            Personality::Aggressive => "personality.aggressive",
            Personality::Chaotic => "personality.chaotic",
            Personality::Lawful => "personality.lawful",
            Personality::Peaceful => "personality.peaceful",
        }
    }

    /// The original's `TBL_AI_PRS_IDK_Mod`: how well the wizard takes proposals.
    pub fn modifier(self) -> i32 {
        self as i32 * 10
    }
}

/// What an AI wizard is after (the original's `AI_OBJ_*`, in its order). On a wizard; the human
/// has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Objective {
    Pragmatist,
    Militarist,
    Theurgist,
    Perfectionist,
    Expansionist,
}

impl Objective {
    pub const ALL: [Objective; 5] = [
        Objective::Pragmatist,
        Objective::Militarist,
        Objective::Theurgist,
        Objective::Perfectionist,
        Objective::Expansionist,
    ];

    /// The key of the objective's name in `content/text/`.
    pub fn key(self) -> &'static str {
        match self {
            Objective::Pragmatist => "personality.pragmatist",
            Objective::Militarist => "personality.militarist",
            Objective::Theurgist => "personality.theurgist",
            Objective::Perfectionist => "personality.perfectionist",
            Objective::Expansionist => "personality.expansionist",
        }
    }
}

/// The original's realm order, which its weight table is indexed by (`sbr_Nature` to `sbr_Death`).
const REALMS: [&str; 5] = ["nature", "sorcery", "chaos", "life", "death"];

/// `TBL_AI_Realm_OBJ`: the base weight of each objective by primary realm
/// (Pragmatist, Militarist, Theurgist, Perfectionist, Expansionist).
const REALM_WEIGHTS: [[i32; 5]; 5] = [
    [0, 2, 2, 4, 2],
    [0, 1, 4, 2, 3],
    [0, 3, 2, 2, 3],
    [0, 2, 3, 4, 1],
    [0, 4, 1, 1, 4],
];

/// The index (in `REALMS`) of the realm with most books; the earlier one on a tie.
fn primary_realm(books: &impl Fn(&str) -> i32) -> usize {
    let mut primary = 0;
    for (i, realm) in REALMS.iter().enumerate().skip(1) {
        if books(realm) > books(REALMS[primary]) {
            primary = i;
        }
    }
    primary
}

/// `TBL_AI_Realm_PRS`: the base weight of each personality by primary realm (Maniacal, Ruthless,
/// Aggressive, Chaotic, Lawful, Peaceful).
const PERSONALITY_WEIGHTS: [[i32; 6]; 5] = [
    [0, 1, 1, 1, 5, 2],
    [1, 2, 3, 1, 2, 1],
    [1, 2, 2, 5, 0, 0],
    [0, 1, 2, 0, 2, 5],
    [3, 3, 2, 1, 1, 0],
];

const PERSONALITIES: [Personality; 6] = [
    Personality::Maniacal,
    Personality::Ruthless,
    Personality::Aggressive,
    Personality::Chaotic,
    Personality::Lawful,
    Personality::Peaceful,
];

/// The weight of each personality: the primary realm's row plus the retorts' bonuses; a Life
/// wizard is never Maniacal and a Death wizard never Peaceful.
pub fn personality_weights(books: impl Fn(&str) -> i32, has: impl Fn(Retort) -> bool) -> [i32; 6] {
    let primary = primary_realm(&books);
    let mut w = PERSONALITY_WEIGHTS[primary];
    let [maniacal, ruthless, aggressive, chaotic, lawful, peaceful] = &mut w;
    if has(Retort::Warlord) {
        *ruthless += 2;
        *aggressive += 3;
    }
    if has(Retort::ChaosMastery) {
        *chaotic += 3;
    }
    if has(Retort::NatureMastery) {
        *lawful += 3;
    }
    if has(Retort::InfernalPower) {
        *maniacal += 4;
    }
    if has(Retort::DivinePower) {
        *peaceful += 4;
    }
    if has(Retort::Channeler) {
        *aggressive += 2;
    }
    match REALMS[primary] {
        "life" => *maniacal = 0,
        "death" => *peaceful = 0,
        _ => {}
    }
    w
}

/// The original's weighted pick for personalities (same walk as [`pick_objective`]).
pub fn pick_personality(weights: &[i32; 6], roll: i32) -> Personality {
    let mut left = roll - weights[0];
    let mut picked = 0;
    while left > 0 && picked < 5 {
        picked += 1;
        left -= weights[picked];
    }
    PERSONALITIES[picked]
}

/// The weight of each objective for a wizard with these books and retorts: the table row of the
/// primary realm (the realm with most books, the earlier one on a tie) plus the retorts' bonuses.
pub fn objective_weights(books: impl Fn(&str) -> i32, has: impl Fn(Retort) -> bool) -> [i32; 5] {
    let primary = primary_realm(&books);
    let mut w = REALM_WEIGHTS[primary];
    let [_, militarist, theurgist, perfectionist, expansionist] = &mut w;
    if has(Retort::Warlord) {
        *militarist += 2;
        *expansionist += 3;
    }
    if has(Retort::ChaosMastery) {
        *theurgist += 1;
    }
    if has(Retort::NatureMastery) {
        *theurgist += 1;
    }
    if has(Retort::Alchemy) {
        *perfectionist += 2;
    }
    if has(Retort::Archmage) {
        *theurgist += 4;
    }
    if has(Retort::Myrran) {
        *expansionist += 4;
    }
    if has(Retort::Conjurer) {
        *militarist += 3;
    }
    w
}

/// The original's weighted pick: a roll of `1..=total` walks down the weights.
pub fn pick_objective(weights: &[i32; 5], roll: i32) -> Objective {
    let mut left = roll - weights[0];
    let mut picked = 0;
    while left > 0 && picked < 4 {
        picked += 1;
        left -= weights[picked];
    }
    Objective::ALL[picked]
}

impl Wizard<'_> {
    /// The wizard's temper; the human's is `Chaotic`.
    pub fn personality(&self) -> Personality {
        self.component::<Personality>()
            .map_or_else(Default::default, |p| *p)
    }

    /// What the wizard is after (none for the human).
    pub fn objective(&self) -> Option<Objective> {
        self.component::<Objective>().map(|o| *o)
    }
}

impl Game {
    /// The names of a wizard's heroes (a hero's own name is not kept yet: its class stands for it).
    pub fn wizard_heroes(&self, wizard: PlayerId) -> Vec<String> {
        let units = self.units();
        units
            .player_units(wizard)
            .into_iter()
            .filter_map(|u| units.unit_def(u))
            .filter(|d| d.kind == crate::rules::UnitKind::Hero)
            .map(|d| d.name.clone())
            .collect()
    }

    /// Draw a personality, then an objective, for every wizard but the human, in turn order, as
    /// the original does. Called when a game starts.
    pub(crate) fn assign_temperaments(&mut self) {
        let ids: Vec<PlayerId> = crate::player::ids(&self.world);
        for id in ids {
            if self.human() == Some(id) {
                continue;
            }
            let Some(sheet) = crate::wizard::entity::find(&self.world, id) else {
                continue;
            };
            let personality = personality_weights(|r| sheet.books_in(r), |r| sheet.has(r));
            let objective = objective_weights(|r| sheet.books_in(r), |r| sheet.has(r));
            let mut draw = |weights: &[i32]| match weights.iter().sum::<i32>() {
                total if total > 0 => self.rng.roll(total as u32) as i32,
                _ => 0,
            };
            let personality = pick_personality(&personality, draw(&personality));
            let objective = pick_objective(&objective, draw(&objective));
            if let Some(mut w) = self.wizard_mut(id) {
                w.put(Some(personality));
                w.put(Some(objective));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weights(books: &[(&str, i32)], retorts: &[Retort]) -> [i32; 5] {
        objective_weights(
            |r| books.iter().find(|b| b.0 == r).map_or(0, |b| b.1),
            |r| retorts.contains(&r),
        )
    }

    #[test]
    fn the_primary_realm_picks_the_table_row() {
        assert_eq!(weights(&[("life", 4)], &[]), [0, 2, 3, 4, 1]);
        assert_eq!(weights(&[("death", 3), ("chaos", 2)], &[]), [0, 4, 1, 1, 4]);
    }

    #[test]
    fn a_tie_goes_to_the_earlier_realm() {
        assert_eq!(
            weights(&[("sorcery", 2), ("chaos", 2)], &[]),
            [0, 1, 4, 2, 3]
        );
    }

    #[test]
    fn retorts_add_their_bonuses() {
        let w = weights(&[("nature", 3)], &[Retort::Warlord, Retort::Archmage]);
        assert_eq!(w, [0, 2 + 2, 2 + 4, 4, 2 + 3]);
        let w = weights(
            &[("nature", 3)],
            &[Retort::Alchemy, Retort::Myrran, Retort::Conjurer],
        );
        assert_eq!(w, [0, 2 + 3, 2, 4 + 2, 2 + 4]);
    }

    #[test]
    fn the_roll_walks_down_the_weights() {
        let w = [0, 2, 3, 4, 1];
        let picks: Vec<_> = (1..=10).map(|r| pick_objective(&w, r)).collect();
        use Objective::*;
        assert_eq!(
            picks,
            [
                Militarist,
                Militarist,
                Theurgist,
                Theurgist,
                Theurgist,
                Perfectionist,
                Perfectionist,
                Perfectionist,
                Perfectionist,
                Expansionist
            ]
        );
    }

    #[test]
    fn personalities_follow_the_realm_and_retorts() {
        let w = |b: &[(&str, i32)], r: &[Retort]| {
            personality_weights(
                |x| b.iter().find(|p| p.0 == x).map_or(0, |p| p.1),
                |x| r.contains(&x),
            )
        };
        assert_eq!(
            w(&[("life", 3)], &[Retort::InfernalPower]),
            [0, 1, 2, 0, 2, 5]
        );
        assert_eq!(
            w(&[("death", 3)], &[Retort::DivinePower]),
            [3, 3, 2, 1, 1, 0]
        );
        assert_eq!(
            w(&[("chaos", 3)], &[Retort::Warlord, Retort::Channeler]),
            [1, 4, 7, 5, 0, 0]
        );
    }

    #[test]
    fn the_personality_roll_walks_down_the_weights() {
        let w = [0, 1, 2, 0, 2, 5];
        let p = |r| pick_personality(&w, r);
        assert_eq!(p(1), Personality::Ruthless);
        assert_eq!(p(3), Personality::Aggressive);
        assert_eq!(p(4), Personality::Lawful);
        assert_eq!(p(6), Personality::Peaceful);
        assert_eq!(p(10), Personality::Peaceful);
    }
}
