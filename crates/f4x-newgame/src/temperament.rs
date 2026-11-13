//! A wizard's personality and objective, as the original's `Init_Magic_Personalities_Objectives`
//! draws them at game start (`INITGAME.c`). The personality steers diplomacy; the
//! objective is recorded and shown on a rival's wizard sheet. The kinds are
//! `f4x_core::wizard::objective`.
//!
//! Tests: below.

use f4x_core::game::Game;
use f4x_core::rules::RealmId;
use f4x_core::wizard::magic::Retort;
use f4x_core::wizard::objective::{Objective, Personality};

/// The original's realm order, which its weight table is indexed by (`sbr_Nature` to `sbr_Death`).
const REALMS: &[RealmId] = f4x_core::rules::BOOK_REALMS;

/// `TBL_AI_Realm_OBJ`: the base weight of each objective by primary realm
/// (Pragmatist, Militarist, Theurgist, Perfectionist, Expansionist).
const REALM_WEIGHTS: &[[i32; 5]] = &[
    [0, 2, 2, 4, 2],
    [0, 1, 4, 2, 3],
    [0, 3, 2, 2, 3],
    [0, 2, 3, 4, 1],
    [0, 4, 1, 1, 4],
];

/// The index (in `REALMS`) of the realm with most books; the earlier one on a tie.
fn primary_realm(books: &impl Fn(RealmId) -> i32) -> usize {
    let mut primary = 0;
    for (i, realm) in REALMS.iter().enumerate().skip(1) {
        if books(*realm) > books(REALMS[primary]) {
            primary = i;
        }
    }
    primary
}

/// `TBL_AI_Realm_PRS`: the base weight of each personality by primary realm (Maniacal, Ruthless,
/// Aggressive, Chaotic, Lawful, Peaceful).
const PERSONALITY_WEIGHTS: &[[i32; 6]] = &[
    [0, 1, 1, 1, 5, 2],
    [1, 2, 3, 1, 2, 1],
    [1, 2, 2, 5, 0, 0],
    [0, 1, 2, 0, 2, 5],
    [3, 3, 2, 1, 1, 0],
];

const PERSONALITIES: &[Personality] = &[
    Personality::Maniacal,
    Personality::Ruthless,
    Personality::Aggressive,
    Personality::Chaotic,
    Personality::Lawful,
    Personality::Peaceful,
];

/// The weight of each personality: the primary realm's row plus the retorts' bonuses; a Life
/// wizard is never Maniacal and a Death wizard never Peaceful.
pub fn personality_weights(
    books: impl Fn(RealmId) -> i32,
    has: impl Fn(Retort) -> bool,
) -> [i32; 6] {
    let primary = primary_realm(&books);
    let mut w = PERSONALITY_WEIGHTS[primary];
    let [maniacal, ruthless, aggressive, chaotic, lawful, peaceful] = &mut w;
    if has(Retort::new(1) /* warlord */) {
        *ruthless += 2;
        *aggressive += 3;
    }
    if has(Retort::new(2) /* chaos_mastery */) {
        *chaotic += 3;
    }
    if has(Retort::new(3) /* nature_mastery */) {
        *lawful += 3;
    }
    if has(Retort::new(5) /* infernal_power */) {
        *maniacal += 4;
    }
    if has(Retort::new(6) /* divine_power */) {
        *peaceful += 4;
    }
    if has(Retort::new(8) /* channeler */) {
        *aggressive += 2;
    }
    match REALMS[primary].num() {
        3 /* life */ => *maniacal = 0,
        4 /* death */ => *peaceful = 0,
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
pub fn objective_weights(books: impl Fn(RealmId) -> i32, has: impl Fn(Retort) -> bool) -> [i32; 5] {
    let primary = primary_realm(&books);
    let mut w = REALM_WEIGHTS[primary];
    let [_, militarist, theurgist, perfectionist, expansionist] = &mut w;
    if has(Retort::new(1) /* warlord */) {
        *militarist += 2;
        *expansionist += 3;
    }
    if has(Retort::new(2) /* chaos_mastery */) {
        *theurgist += 1;
    }
    if has(Retort::new(3) /* nature_mastery */) {
        *theurgist += 1;
    }
    if has(Retort::new(0) /* alchemy */) {
        *perfectionist += 2;
    }
    if has(Retort::new(10) /* archmage */) {
        *theurgist += 4;
    }
    if has(Retort::new(9) /* myrran */) {
        *expansionist += 4;
    }
    if has(Retort::new(15) /* conjurer */) {
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
    Objective::all()
        .nth(picked)
        .unwrap_or(Objective::Pragmatist)
}

/// Draw a personality, then an objective, for every wizard but the human, in turn order, as
/// the original does. Called when a game starts.
pub fn assign_temperaments(game: &mut Game) {
    for id in game.player_ids() {
        if game.human() == Some(id) {
            continue;
        }
        let Some(sheet) = game.wizard(id) else {
            continue;
        };
        let personality = personality_weights(|r| sheet.books_in(r), |r| sheet.has(r));
        let objective = objective_weights(|r| sheet.books_in(r), |r| sheet.has(r));
        let mut draw = |weights: &[i32]| match weights.iter().sum::<i32>() {
            total if total > 0 => game.roll(total as u32) as i32,
            _ => 0,
        };
        let personality = pick_personality(&personality, draw(&personality));
        let objective = pick_objective(&objective, draw(&objective));
        if let Some(mut w) = game.wizard_mut(id) {
            w.put(Some(personality));
            w.put(Some(objective));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weights(books: &[(RealmId, i32)], retorts: &[Retort]) -> [i32; 5] {
        objective_weights(
            |r| books.iter().find(|b| b.0 == r).map_or(0, |b| b.1),
            |r| retorts.contains(&r),
        )
    }

    #[test]
    fn the_primary_realm_picks_the_table_row() {
        assert_eq!(
            weights(&[(RealmId::new(3 /* life */), 4)], &[]),
            [0, 2, 3, 4, 1]
        );
        assert_eq!(
            weights(
                &[
                    (RealmId::new(4 /* death */), 3),
                    (RealmId::new(2 /* chaos */), 2)
                ],
                &[]
            ),
            [0, 4, 1, 1, 4]
        );
    }

    #[test]
    fn a_tie_goes_to_the_earlier_realm() {
        assert_eq!(
            weights(
                &[
                    (RealmId::new(1 /* sorcery */), 2),
                    (RealmId::new(2 /* chaos */), 2)
                ],
                &[]
            ),
            [0, 1, 4, 2, 3]
        );
    }

    #[test]
    fn retorts_add_their_bonuses() {
        let w = weights(
            &[(RealmId::new(0 /* nature */), 3)],
            &[
                Retort::new(1),  /* warlord */
                Retort::new(10), /* archmage */
            ],
        );
        assert_eq!(w, [0, 2 + 2, 2 + 4, 4, 2 + 3]);
        let w = weights(
            &[(RealmId::new(0 /* nature */), 3)],
            &[
                Retort::new(0),  /* alchemy */
                Retort::new(9),  /* myrran */
                Retort::new(15), /* conjurer */
            ],
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
        let w = |b: &[(RealmId, i32)], r: &[Retort]| {
            personality_weights(
                |x| b.iter().find(|p| p.0 == x).map_or(0, |p| p.1),
                |x| r.contains(&x),
            )
        };
        assert_eq!(
            w(
                &[(RealmId::new(3 /* life */), 3)],
                &[Retort::new(5) /* infernal_power */]
            ),
            [0, 1, 2, 0, 2, 5]
        );
        assert_eq!(
            w(
                &[(RealmId::new(4 /* death */), 3)],
                &[Retort::new(6) /* divine_power */]
            ),
            [3, 3, 2, 1, 1, 0]
        );
        assert_eq!(
            w(
                &[(RealmId::new(2 /* chaos */), 3)],
                &[
                    Retort::new(1), /* warlord */
                    Retort::new(8)  /* channeler */
                ]
            ),
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
