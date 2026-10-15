//! How the computer wizards are made (task 247, `docs/game-settings.md` 4.8), after the original's
//! `Init_Computer_Players_Wizard_Profile`: a random ready-made wizard each, opposed realms merged
//! now and then, "erasure dice" that take books away, one or two made Myrran, and the picks
//! filled up to the game's budget with random books and retorts. Every roll comes from the
//! game's seeded generator.
//!
//! Where this differs from the original: a retort the books do not allow is not given (the
//! original's refill did not check), the refill never picks Myrran (the Myrran rule makes them),
//! and the erasure dice that clear retorts clear them all (the original's loop has an index bug
//! that clears one slot).

use crate::magic::Retort;
use crate::settings::GameSettings;
use crate::wizard::{
    retort_available, retort_cost, WizardBuild, ALL_RETORTS, MAX_BOOKS, MAX_RETORTS,
};
use crate::Game;

const REALMS: [&str; 5] = ["life", "death", "chaos", "nature", "sorcery"];

/// The realm pairs the original merges, in its order: the first named is the one that may take
/// the other's books.
const OPPOSED: [(&str, &str); 8] = [
    ("death", "chaos"),
    ("death", "nature"),
    ("death", "sorcery"),
    ("life", "chaos"),
    ("life", "nature"),
    ("life", "sorcery"),
    ("sorcery", "chaos"),
    ("sorcery", "nature"),
];

fn books(b: &WizardBuild, realm: &str) -> u8 {
    b.books.get(realm).copied().unwrap_or(0)
}

fn book_total(b: &WizardBuild) -> u8 {
    b.books.values().sum()
}

fn picks_used(b: &WizardBuild) -> u8 {
    b.picks_spent()
}

/// Take away retorts the books no longer allow.
fn drop_unavailable(b: &mut WizardBuild) {
    let books = b.books.clone();
    b.retorts.retain(|&r| retort_available(r, &books));
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
            .is_some_and(|b| b.retorts.contains(&Retort::Myrran))
    };

    // Who: a random ready-made wizard each, at most two of them Myrran.
    let mut builds: Vec<WizardBuild> = Vec::new();
    let mut myrrans = 0;
    for i in 0..count {
        let free: Vec<&String> = ids
            .iter()
            .filter(|id| !taken.contains(id))
            .filter(|id| !(rules.myrran_rule && myrrans >= 2 && myran(game, id)))
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
        if build.retorts.contains(&Retort::Myrran) {
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
                b.books.insert(to.to_string(), sum);
                b.books.remove(from);
            }
            drop_unavailable(b);
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
                        *b.books.get_mut(realm).unwrap() -= 1;
                    }
                } else {
                    b.retorts.clear();
                }
            }
            drop_unavailable(b);
        }
    }

    // Myrran: with four or more wizards, if no computer wizard is Myrran, one or two are made so.
    let wizards = usize::from(settings.basic.opponents) + 1;
    if rules.myrran_rule
        && wizards >= 4
        && !builds.is_empty()
        && !builds.iter().any(|b| b.retorts.contains(&Retort::Myrran))
    {
        let n = (game.roll(2) as usize).min(builds.len());
        let mut chosen: Vec<usize> = Vec::new();
        while chosen.len() < n {
            let k = game.roll(builds.len() as u32) as usize - 1;
            if !chosen.contains(&k) {
                chosen.push(k);
            }
        }
        for k in chosen {
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
                        *builds[k].books.get_mut(realm).unwrap() -= 1;
                        break;
                    }
                    r -= n;
                }
            }
            builds[k].books.retain(|_, n| *n > 0);
            builds[k].retorts.insert(Retort::Myrran);
            drop_unavailable(&mut builds[k]);
        }
    }

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
        let used = picks_used(b);
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
            1 => add_books(b, "life", value, "death"),
            2 => add_books(b, "death", value, "life"),
            3 => add_books(b, "nature", value, ""),
            4 => add_books(b, "sorcery", value, ""),
            5 => add_books(b, "chaos", value, ""),
            _ => add_retort(game, b, picks),
        }
    }
}

/// Add books to `realm` (up to the limit), unless the wizard has books in the realm opposed to it.
fn add_books(b: &mut WizardBuild, realm: &str, value: u8, opposed: &str) {
    if books(b, realm) >= MAX_BOOKS || (!opposed.is_empty() && books(b, opposed) > 0) {
        return;
    }
    let n = (books(b, realm) + value).min(MAX_BOOKS);
    b.books.insert(realm.to_string(), n);
}

fn add_retort(game: &mut Game, b: &mut WizardBuild, picks: u8) {
    let used = picks_used(b);
    let r = ALL_RETORTS[game.roll(ALL_RETORTS.len() as u32) as usize - 1];
    if b.retorts.len() >= MAX_RETORTS || r == Retort::Myrran {
        return;
    }
    let retort = match r {
        // A realm mastery (or Divine or Infernal Power): of a realm picked by its books, among
        // those the wizard does not yet hold the mastery of, the two powers only with picks to
        // spare.
        Retort::ChaosMastery
        | Retort::NatureMastery
        | Retort::SorceryMastery
        | Retort::DivinePower
        | Retort::InfernalPower => {
            let has = |x| b.retorts.contains(&x);
            let tight = u16::from(picks) < u16::from(used) + 2;
            let shelf = [
                ("nature", Retort::NatureMastery, has(Retort::NatureMastery)),
                (
                    "sorcery",
                    Retort::SorceryMastery,
                    has(Retort::SorceryMastery),
                ),
                ("chaos", Retort::ChaosMastery, has(Retort::ChaosMastery)),
                (
                    "life",
                    Retort::DivinePower,
                    has(Retort::DivinePower) || tight,
                ),
                (
                    "death",
                    Retort::InfernalPower,
                    has(Retort::InfernalPower) || tight,
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
        Retort::Warlord | Retort::Channeler | Retort::Famous => {
            if u16::from(picks) <= u16::from(used) + 1 {
                return;
            }
            r
        }
        other => other,
    };
    if retort_available(retort, &b.books)
        && u16::from(used) + u16::from(retort_cost(retort)) <= u16::from(picks)
    {
        b.retorts.insert(retort);
    }
}
