//! How the computer wizards are made (`docs/game-settings.md` 4.8), after the original's
//! `Init_Computer_Players_Wizard_Profile`: a random ready-made wizard each, opposed realms merged
//! now and then, "erasure dice" that take books away, one or two made Myrran, and the picks
//! filled up to the game's budget with random books and retorts. Every roll comes from the
//! game's seeded generator.
//!
//! Where this differs from the original: a retort the books do not allow is not given (the
//! original's refill did not check), the refill never picks Myrran (the Myrran rule makes them),
//! and the erasure dice that clear retorts clear them all (the original's loop has an index bug
//! that clears one slot).
//!
//! Tests: in wizard/ai/tests.rs.

use crate::game::setting::GameSettings;
use crate::game::Game;
use crate::rules::{RealmId, Rules};
use crate::wizard::magic::Retort;
use crate::wizard::{
    all_retorts, retort_available, retort_cost, WizardBuild, MAX_BOOKS, MAX_RETORTS,
};

const NATURE: RealmId = RealmId::new(0) /* nature */;
const SORCERY: RealmId = RealmId::new(1) /* sorcery */;
const CHAOS: RealmId = RealmId::new(2) /* chaos */;
const LIFE: RealmId = RealmId::new(3) /* life */;
const DEATH: RealmId = RealmId::new(4) /* death */;

/// The realms in the order the original's loops go.
const REALMS: [RealmId; 5] = [LIFE, DEATH, CHAOS, NATURE, SORCERY];

/// The realm pairs the original merges, in its order: the first named is the one that may take
/// the other's books.
const OPPOSED: [(RealmId, RealmId); 8] = [
    (DEATH, CHAOS),
    (DEATH, NATURE),
    (DEATH, SORCERY),
    (LIFE, CHAOS),
    (LIFE, NATURE),
    (LIFE, SORCERY),
    (SORCERY, CHAOS),
    (SORCERY, NATURE),
];

fn books(b: &WizardBuild, realm: RealmId) -> u8 {
    b.books.get(&realm).copied().unwrap_or(0)
}

fn book_total(b: &WizardBuild) -> u8 {
    b.books.values().sum()
}

fn picks_used(rules: &Rules, b: &WizardBuild) -> u8 {
    b.picks_spent(rules)
}

/// Take away retorts the books no longer allow.
fn drop_unavailable(rules: &Rules, b: &mut WizardBuild) {
    let books = b.books.clone();
    b.retorts.retain(|&r| retort_available(rules, r, &books));
}

/// How many of `total` wizards may start on Myrror, from least to most: the planes' wizards
/// differ by at most two on Arcanus's side and one on Myrror's, so for `n` on Arcanus there are
/// `n - 2` to `n + 1` on Myrror (7 wizards: 4 and 3, or 3 and 4; 8: 5 and 3, or 4 and 4).
pub(crate) fn myrran_range(total: usize) -> (usize, usize) {
    (total.saturating_sub(2).div_ceil(2), total.div_ceil(2))
}

/// Make the share of Myrran wizards fit [`myrran_range`]: random computer wizards without
/// the retort are made Myrran (three books given up, one kept at least) while there are too few,
/// and random Myrran ones are made ordinary (their picks refilled) while there are too many.
/// `builds` are the computer wizards, `human_myrran` says whether the human wizard is one,
/// `total` is all the wizards.
pub(crate) fn balance_planes(
    game: &mut Game,
    builds: &mut [WizardBuild],
    human_myrran: bool,
    total: usize,
    picks: u8,
) {
    let myrran = |b: &WizardBuild| b.retorts.contains(&Retort::new(9) /* myrran */);
    let (least, most) = myrran_range(total);
    let count = |builds: &[WizardBuild]| {
        builds.iter().filter(|b| myrran(b)).count() + usize::from(human_myrran)
    };
    while count(builds) < least {
        let ordinary: Vec<usize> = (0..builds.len()).filter(|&k| !myrran(&builds[k])).collect();
        if ordinary.is_empty() {
            break;
        }
        let k = ordinary[game.roll(ordinary.len() as u32) as usize - 1];
        for _ in 0..3 {
            // A book from a realm picked by its share of the books, keeping at least one.
            let total = u32::from(book_total(&builds[k]));
            if total <= 1 {
                break;
            }
            let mut r = game.roll(total);
            for realm in REALMS {
                let n = u32::from(books(&builds[k], realm));
                if r <= n {
                    if let Some(b) = builds[k].books.get_mut(&realm) {
                        *b -= 1;
                    }
                    break;
                }
                r -= n;
            }
        }
        builds[k].books.retain(|_, n| *n > 0);
        builds[k].retorts.insert(Retort::new(9) /* myrran */);
        drop_unavailable(&game.content.rules, &mut builds[k]);
    }
    while count(builds) > most {
        let myrrans: Vec<usize> = (0..builds.len()).filter(|&k| myrran(&builds[k])).collect();
        if myrrans.is_empty() {
            break;
        }
        let k = myrrans[game.roll(myrrans.len() as u32) as usize - 1];
        builds[k].retorts.remove(&Retort::new(9) /* myrran */);
        refill(game, &mut builds[k], picks);
    }
}

/// Make `count` computer wizards from the ready-made ones not in `taken` (which gains the ones
/// used). When the ready-made wizards run out (or are not installed: they are local-only
/// content), `fallback(i)` gives the i-th computer wizard's plain build, which goes through the
/// same changes.
pub(crate) fn make(
    game: &mut Game,
    settings: &GameSettings,
    taken: &mut Vec<String>,
    count: usize,
    human_myrran: bool,
    fallback: &dyn Fn(usize) -> WizardBuild,
) -> Vec<WizardBuild> {
    let rules = settings.ai.wizards;
    let picks = settings.wizards.picks.ai;
    let ids: Vec<String> = game
        .content
        .rules
        .wizards
        .defs()
        .iter()
        .map(|w| w.id.clone())
        .collect();
    let myran = |game: &Game, id: &str| {
        game.preset_build(id)
            .is_some_and(|b| b.retorts.contains(&Retort::new(9) /* myrran */))
    };

    // Who: a random ready-made wizard each, as many of them Myrran as the planes can take.
    let mut builds: Vec<WizardBuild> = Vec::new();
    let mut myrrans = usize::from(human_myrran);
    let wizards = usize::from(settings.basic.opponents) + 1;
    let (_, most) = myrran_range(wizards);
    for i in 0..count {
        let free: Vec<&String> = ids
            .iter()
            .filter(|id| !taken.contains(id))
            .filter(|id| !(rules.myrran_rule && myrrans >= most && myran(game, id)))
            .collect();
        if free.is_empty() {
            builds.push(fallback(i));
            continue;
        }
        let id = free[game.roll(free.len() as u32) as usize - 1].clone();
        let Some(build) = game.preset_build(&id) else {
            builds.push(fallback(i));
            continue;
        };
        if build.retorts.contains(&Retort::new(9) /* myrran */) {
            myrrans += 1;
        }
        taken.push(id);
        builds.push(build);
    }

    // Opposed realms merged, one time in a hundred by the setting.
    if rules.consolidate_percent > 0 {
        for b in &mut builds {
            if game.roll(100) > u32::from(rules.consolidate_percent) {
                continue;
            }
            for (first, second) in OPPOSED {
                let (x, y) = (books(b, first), books(b, second));
                if x == 0 || y == 0 {
                    continue;
                }
                let (to, from) = if game.roll(2) == 1 {
                    (first, second)
                } else {
                    (second, first)
                };
                let sum = (x + y).min(MAX_BOOKS);
                b.books.insert(to, sum);
                b.books.remove(&from);
            }
            drop_unavailable(&game.content.rules, b);
        }
    }

    // Erasure dice: each pip takes a book from a realm (never its last) or the retorts.
    let (dice, sides) = rules.erasure_dice;
    if dice > 0 && sides > 0 {
        for b in &mut builds {
            let pips: u32 = (0..dice).map(|_| game.roll(u32::from(sides))).sum();
            for _ in 0..pips {
                let t = game.roll(6) as usize;
                if t <= 5 {
                    let realm = REALMS[t - 1];
                    if books(b, realm) > 1 {
                        if let Some(n) = b.books.get_mut(&realm) {
                            *n -= 1;
                        }
                    }
                } else {
                    b.retorts.clear();
                }
            }
            drop_unavailable(&game.content.rules, b);
        }
    }

    // Myrran: the wizards are shared between the planes (`balance_planes`).
    balance_planes(game, &mut builds, human_myrran, wizards, picks);

    // Fill the picks up with random books and retorts.
    for b in &mut builds {
        refill(game, b, picks);
        b.preset = true;
    }
    builds
}

/// The original's refill loop: five rolls in eight add two to five books to a realm, three in
/// eight add a retort.
fn refill(game: &mut Game, b: &mut WizardBuild, picks: u8) {
    for _ in 0..500 {
        let used = picks_used(&game.content.rules, b);
        if used >= picks {
            return;
        }
        let book_count = book_total(b);
        let mut kind = game.roll(8);
        let mut value = 1 + game.roll(4) as u8;
        if value + used > picks {
            value = picks - used;
        }
        if value + book_count > 12 {
            value = 12u8.saturating_sub(book_count);
        }
        if value == 0 {
            kind = 6;
        }
        if kind == 6 {
            value = 1;
        }
        match kind {
            1 => add_books(b, LIFE, value, Some(DEATH)),
            2 => add_books(b, DEATH, value, Some(LIFE)),
            3 => add_books(b, NATURE, value, None),
            4 => add_books(b, SORCERY, value, None),
            5 => add_books(b, CHAOS, value, None),
            _ => add_retort(game, b, picks),
        }
    }
}

/// Add books to `realm` (up to the limit), unless the wizard has books in the realm opposed to it.
fn add_books(b: &mut WizardBuild, realm: RealmId, value: u8, opposed: Option<RealmId>) {
    if books(b, realm) >= MAX_BOOKS || opposed.is_some_and(|o| books(b, o) > 0) {
        return;
    }
    let n = (books(b, realm) + value).min(MAX_BOOKS);
    b.books.insert(realm, n);
}

fn add_retort(game: &mut Game, b: &mut WizardBuild, picks: u8) {
    let used = picks_used(&game.content.rules, b);
    let all: Vec<Retort> = all_retorts(&game.content.rules).collect();
    let r = all[game.roll(all.len() as u32) as usize - 1];
    if b.retorts.len() >= MAX_RETORTS || r == Retort::new(9)
    /* myrran */
    {
        return;
    }
    let realm_retorts = [
        Retort::new(2), /* chaos_mastery */
        Retort::new(3), /* nature_mastery */
        Retort::new(4), /* sorcery_mastery */
        Retort::new(6), /* divine_power */
        Retort::new(5), /* infernal_power */
    ];
    let two_pick = retort_cost(&game.content.rules, r) == 2;
    let retort = match r {
        // A realm mastery (or Divine or Infernal Power): of a realm picked by its books, among
        // those the wizard does not yet hold the mastery of, the two powers only with picks to
        // spare.
        _ if realm_retorts.contains(&r) => {
            let has = |x| b.retorts.contains(&x);
            let tight = u16::from(picks) < u16::from(used) + 2;
            let shelf = [
                (
                    NATURE,
                    Retort::new(3), /* nature_mastery */
                    has(Retort::new(3) /* nature_mastery */),
                ),
                (
                    SORCERY,
                    Retort::new(4), /* sorcery_mastery */
                    has(Retort::new(4) /* sorcery_mastery */),
                ),
                (
                    CHAOS,
                    Retort::new(2), /* chaos_mastery */
                    has(Retort::new(2) /* chaos_mastery */),
                ),
                (
                    LIFE,
                    Retort::new(6), /* divine_power */
                    has(Retort::new(6) /* divine_power */) || tight,
                ),
                (
                    DEATH,
                    Retort::new(5), /* infernal_power */
                    has(Retort::new(5) /* infernal_power */) || tight,
                ),
            ];
            let weights: Vec<u32> = shelf
                .iter()
                .map(|&(realm, _, held)| if held { 0 } else { u32::from(books(b, realm)) })
                .collect();
            let total: u32 = weights.iter().sum();
            if total == 0 {
                return;
            }
            let mut roll = game.roll(total);
            let mut chosen = shelf[0].1;
            for (w, s) in weights.iter().zip(shelf) {
                if roll <= *w {
                    chosen = s.1;
                    break;
                }
                roll -= w;
            }
            chosen
        }
        // The two-pick retorts need two picks to spare, as in the original.
        _ if two_pick => {
            if u16::from(picks) <= u16::from(used) + 1 {
                return;
            }
            r
        }
        other => other,
    };
    if retort_available(&game.content.rules, retort, &b.books)
        && u16::from(used) + u16::from(retort_cost(&game.content.rules, retort)) <= u16::from(picks)
    {
        b.retorts.insert(retort);
    }
}

#[cfg(test)]
mod tests;
