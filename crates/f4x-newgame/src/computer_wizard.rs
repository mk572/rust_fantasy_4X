//! How the computer wizards are made, after the original's `Init_Computer_Players_Wizard_Profile`:
//! the phases, the rolls and where this differs from the original are in `docs/game-settings.md`
//! 4.8. Every roll comes from the game's seeded generator.
//!
//! Tests: in computer_wizard/tests.rs.

use f4x_core::rules::{RealmId, Rules};
use f4x_core::wizard::magic::Retort;
use f4x_core::wizard::{all_retorts, preset_build_of, retort_available, retort_cost, WizardBuild};
use f4x_core::world::setting::GameSettings;
use f4x_core::world::World;

const NATURE: RealmId = RealmId::new(0) /* nature */;
const SORCERY: RealmId = RealmId::new(1) /* sorcery */;
const CHAOS: RealmId = RealmId::new(2) /* chaos */;
const LIFE: RealmId = RealmId::new(3) /* life */;
const DEATH: RealmId = RealmId::new(4) /* death */;

const REALMS: &[RealmId] = &[LIFE, DEATH, CHAOS, NATURE, SORCERY];

/// The most books a wizard may have in one realm in this game (`max_books_per_realm`).
fn book_cap(world: &World) -> u8 {
    world.settings().wizards.max_books_per_realm
}

fn book_total(b: &WizardBuild) -> u8 {
    b.books.values().sum()
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
/// `total` is all the wizards, `ban` says whether Life and Death books are kept apart.
pub(crate) fn balance_planes(
    world: &mut World,
    builds: &mut [WizardBuild],
    human_myrran: bool,
    total: usize,
    picks: u8,
    ban: bool,
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
        let k = ordinary[world.roll(ordinary.len() as u32) as usize - 1];
        for _ in 0..3 {
            // A book from a realm picked by its share of the books, keeping at least one.
            let total = u32::from(book_total(&builds[k]));
            if total <= 1 {
                break;
            }
            let mut r = world.roll(total);
            for &realm in REALMS {
                let n = u32::from(builds[k].books_in(realm));
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
        drop_unavailable(&world.content().rules, &mut builds[k]);
    }
    while count(builds) > most {
        let myrrans: Vec<usize> = (0..builds.len()).filter(|&k| myrran(&builds[k])).collect();
        if myrrans.is_empty() {
            break;
        }
        let k = myrrans[world.roll(myrrans.len() as u32) as usize - 1];
        builds[k].retorts.remove(&Retort::new(9) /* myrran */);
        refill(world, &mut builds[k], picks, ban);
    }
}

/// Make `count` computer wizards from the ready-made ones not in `taken` (which gains the ones
/// used). When the ready-made wizards run out (or are not installed: they are local-only
/// content), `fallback(i)` gives the i-th computer wizard's plain build, which goes through the
/// same changes.
pub(crate) fn make(
    world: &mut World,
    presets: &[f4x_core::rules::WizardDef],
    settings: &GameSettings,
    taken: &mut Vec<u16>,
    count: usize,
    human_myrran: bool,
    fallback: &dyn Fn(usize) -> WizardBuild,
) -> Vec<WizardBuild> {
    let rules = settings.ai.wizards;
    let picks = settings.wizards.picks.ai;
    let ban = settings.rules.ban_life_and_death;
    let ids: Vec<u16> = presets.iter().map(|w| w.num).collect();
    let myran = |id: u16| {
        preset_build_of(presets, id)
            .is_some_and(|b| b.retorts.contains(&Retort::new(9) /* myrran */))
    };

    // Who: a random ready-made wizard each, as many of them Myrran as the planes can take.
    let mut builds: Vec<WizardBuild> = Vec::new();
    let mut myrrans = usize::from(human_myrran);
    let wizards = usize::from(settings.basic.opponents) + 1;
    let (_, most) = myrran_range(wizards);
    for i in 0..count {
        let free: Vec<&u16> = ids
            .iter()
            .filter(|id| !taken.contains(id))
            .filter(|id| !(rules.myrran_rule && myrrans >= most && myran(**id)))
            .collect();
        if free.is_empty() {
            builds.push(fallback(i));
            continue;
        }
        let id = *free[world.roll(free.len() as u32) as usize - 1];
        let Some(build) = preset_build_of(presets, id) else {
            builds.push(fallback(i));
            continue;
        };
        taken.push(id);
        let mut build = build;
        fit_books(world, &mut build);
        if build.retorts.contains(&Retort::new(9) /* myrran */) {
            myrrans += 1;
        }
        builds.push(build);
    }

    // Opposed realms merged, one time in a hundred by the setting.
    if rules.consolidate_percent > 0 {
        // The pairs the content allows to be merged (`rules.ron`, `mergeable_realms`).
        let pairs: Vec<(RealmId, RealmId)> = {
            let realms = &world.content().rules.realms;
            world
                .content()
                .rules
                .config
                .mergeable_realms
                .iter()
                .filter_map(|&(a, b)| Some((realms.id_numbered(a)?, realms.id_numbered(b)?)))
                .collect()
        };
        for b in &mut builds {
            if world.roll(100) > u32::from(rules.consolidate_percent) {
                continue;
            }
            for &(first, second) in &pairs {
                let (x, y) = (b.books_in(first), b.books_in(second));
                if x == 0 || y == 0 {
                    continue;
                }
                let (to, from) = if world.roll(2) == 1 {
                    (first, second)
                } else {
                    (second, first)
                };
                let sum = (x + y).min(book_cap(world));
                b.books.insert(to, sum);
                b.books.remove(&from);
            }
            drop_unavailable(&world.content().rules, b);
        }
    }

    // Erasure dice: each pip takes a book from a realm (never its last) or the retorts.
    let (dice, sides) = rules.erasure_dice;
    if dice > 0 && sides > 0 {
        for b in &mut builds {
            let pips: u32 = (0..dice).map(|_| world.roll(u32::from(sides))).sum();
            for _ in 0..pips {
                let t = world.roll(6) as usize;
                if t <= 5 {
                    let realm = REALMS[t - 1];
                    if b.books_in(realm) > 1 {
                        if let Some(n) = b.books.get_mut(&realm) {
                            *n -= 1;
                        }
                    }
                } else {
                    b.retorts.clear();
                }
            }
            drop_unavailable(&world.content().rules, b);
        }
    }

    // Myrran: the wizards are shared between the planes (`balance_planes`).
    balance_planes(world, &mut builds, human_myrran, wizards, picks, ban);

    // Fill the picks up with random books and retorts.
    for b in &mut builds {
        refill(world, b, picks, ban);
        b.preset = true;
    }
    builds
}

/// A ready-made wizard with more books in a realm than the game allows loses the surplus, and
/// the retorts the books no longer allow; the picks it frees are spent again by [`refill`].
pub(crate) fn fit_books(world: &World, b: &mut WizardBuild) {
    if b.trim_books(book_cap(world)) {
        drop_unavailable(&world.content().rules, b);
        // Still a ready-made wizard to the pick rules: the refill spends what is left.
        b.preset = true;
    }
}

/// [`fit_books`] and then spend the freed picks, for a computer wizard taken as it is (the
/// setting that draws no random ready-made wizards).
pub(crate) fn fit_books_and_refill(world: &mut World, b: &mut WizardBuild) {
    let before = b.books.clone();
    fit_books(world, b);
    if b.books != before {
        let (picks, ban) = (
            world.settings().wizards.picks.ai,
            world.settings().rules.ban_life_and_death,
        );
        refill(world, b, picks, ban);
    }
}

fn refill(world: &mut World, b: &mut WizardBuild, picks: u8, ban: bool) {
    let cap = book_cap(world);
    for _ in 0..500 {
        let used = b.picks_spent(&world.content().rules);
        if used >= picks {
            return;
        }
        let book_count = book_total(b);
        let mut kind = world.roll(8);
        let mut value = 1 + world.roll(4) as u8;
        if value + used > picks {
            value = picks - used;
        }
        let all_books = world.settings().wizards.max_books_total;
        if value + book_count > all_books {
            value = all_books.saturating_sub(book_count);
        }
        if value == 0 {
            kind = 6;
        }
        if kind == 6 {
            value = 1;
        }
        match kind {
            1 => add_books(b, LIFE, value, ban.then_some(DEATH), cap),
            2 => add_books(b, DEATH, value, ban.then_some(LIFE), cap),
            3 => add_books(b, NATURE, value, None, cap),
            4 => add_books(b, SORCERY, value, None, cap),
            5 => add_books(b, CHAOS, value, None, cap),
            _ => add_retort(world, b, picks),
        }
    }
}

/// Add books to `realm` (up to the limit), unless the wizard has books in the realm opposed to it.
fn add_books(b: &mut WizardBuild, realm: RealmId, value: u8, opposed: Option<RealmId>, cap: u8) {
    if b.books_in(realm) >= cap || opposed.is_some_and(|o| b.books_in(o) > 0) {
        return;
    }
    let n = (b.books_in(realm) + value).min(cap);
    b.books.insert(realm, n);
}

fn add_retort(world: &mut World, b: &mut WizardBuild, picks: u8) {
    let used = b.picks_spent(&world.content().rules);
    let all: Vec<Retort> = all_retorts(&world.content().rules).collect();
    let r = all[world.roll(all.len() as u32) as usize - 1];
    if b.retorts.len() >= world.content().rules.config.wizard.max_retorts || r == Retort::new(9)
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
    let two_pick = retort_cost(&world.content().rules, r) == 2;
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
                .map(|&(realm, _, held)| {
                    if held {
                        0
                    } else {
                        u32::from(b.books_in(realm))
                    }
                })
                .collect();
            let total: u32 = weights.iter().sum();
            if total == 0 {
                return;
            }
            let mut roll = world.roll(total);
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
        _ if two_pick => {
            if u16::from(picks) <= u16::from(used) + 1 {
                return;
            }
            r
        }
        other => other,
    };
    if retort_available(&world.content().rules, retort, &b.books)
        && u16::from(used) + u16::from(retort_cost(&world.content().rules, retort))
            <= u16::from(picks)
    {
        b.retorts.insert(retort);
    }
}

#[cfg(test)]
mod tests;
