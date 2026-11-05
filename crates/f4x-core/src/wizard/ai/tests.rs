use super::*;
use crate::game::testutil::{realm, shipped};
use crate::rules::Rules;
use crate::rules::{Registry, WizardDef};
use std::collections::BTreeSet;

const SEEDS: std::ops::Range<u64> = 1..60;
const PICKS: u8 = 11;

fn build(books: &[(&str, u8)], retorts: &[Retort]) -> WizardBuild {
    WizardBuild {
        name: "W".into(),
        gender: Default::default(),
        books: books.iter().map(|&(r, n)| (realm(r), n)).collect(),
        retorts: retorts.iter().copied().collect(),
        spells: Vec::new(),
        banner: None,
        portrait: None,
        preset: false,
    }
}

fn rules() -> Rules {
    shipped().rules
}

/// A game that knows the retorts, which the computer wizards are drawn from.
fn new_game(seed: u64) -> Game {
    let mut g = Game::new(seed);
    g.content.rules.retorts = rules().retorts;
    g
}

fn myrran(b: &WizardBuild) -> bool {
    b.retorts.contains(&Retort::new(9) /* myrran */)
}

/// The rules every finished build keeps, whatever the rolls were.
fn assert_sound(b: &WizardBuild, picks: u8) {
    assert!(
        b.picks_spent(&rules()) <= picks,
        "{b:?} spends over {picks} picks"
    );
    assert!(b.retorts.len() <= MAX_RETORTS, "{b:?} has too many retorts");
    assert!(
        b.books.values().all(|&n| n <= MAX_BOOKS),
        "{b:?} has too many books in a realm"
    );
    assert!(book_total(b) <= 12, "{b:?} has too many books");
    for &r in &b.retorts {
        assert!(
            retort_available(&rules(), r, &b.books),
            "{b:?} holds {r:?} unlawfully"
        );
    }
}

fn wizard_def(num: u16, id: &str, books: &[(&str, u8)], retorts: &[Retort]) -> WizardDef {
    WizardDef {
        num,
        id: id.into(),
        help: 0,
        gender: Default::default(),
        books: books
            .iter()
            .map(|&(r, n)| {
                let mut r: crate::rules::Ref<crate::rules::RealmDef> = r.into();
                r.set(Some(realm(r.name())));
                (r, n)
            })
            .collect(),
        retorts: retorts
            .iter()
            .map(|&r| {
                let rules = rules();
                let mut out = crate::rules::Ref::new(rules.retorts.get(r).id.clone());
                out.set(Some(r));
                out
            })
            .collect(),
    }
}

/// A game with the given ready-made wizards and the settings the AI wizard rules read.
fn game_with(seed: u64, defs: Vec<WizardDef>) -> Game {
    let mut g = new_game(seed);
    g.content.rules.wizards = Registry::new(defs);
    g
}

fn settings(opponents: u8, picks: u8) -> GameSettings {
    let mut s = GameSettings::flat();
    s.basic.opponents = opponents;
    s.wizards.picks.ai = picks;
    s
}

fn plain(_: usize) -> WizardBuild {
    build(&[("life", 3), ("nature", 2)], &[])
}

#[test]
fn the_myrran_share_follows_the_planes_limits() {
    assert_eq!(myrran_range(1), (0, 1));
    assert_eq!(myrran_range(2), (0, 1));
    assert_eq!(myrran_range(4), (1, 2));
    assert_eq!(myrran_range(7), (3, 4));
    assert_eq!(myrran_range(8), (3, 4));
}

#[test]
fn too_few_myrran_wizards_are_made_by_giving_up_three_books() {
    for seed in SEEDS {
        let mut g = new_game(seed);
        let before = build(&[("life", 2), ("nature", 2), ("sorcery", 2)], &[]);
        let mut builds = vec![before.clone(); 6];
        balance_planes(&mut g, &mut builds, false, 6, PICKS);
        let made: Vec<&WizardBuild> = builds.iter().filter(|b| myrran(b)).collect();
        let (least, most) = myrran_range(6);
        assert!(
            (least..=most).contains(&made.len()),
            "seed {seed}: {} Myrran of 6",
            made.len()
        );
        for b in made {
            assert_eq!(book_total(b), book_total(&before) - 3, "seed {seed}");
        }
    }
}

#[test]
fn a_wizard_with_one_book_left_gives_up_no_more() {
    for seed in SEEDS {
        let mut g = new_game(seed);
        let mut builds = vec![build(&[("life", 2)], &[]); 2];
        balance_planes(&mut g, &mut builds, false, 4, PICKS);
        assert!(
            builds.iter().filter(|b| myrran(b)).count() >= 1,
            "seed {seed}"
        );
        assert!(builds.iter().all(|b| book_total(b) >= 1), "seed {seed}");
    }
}

#[test]
fn the_human_myrran_wizard_counts_toward_the_share() {
    for seed in SEEDS {
        let mut g = new_game(seed);
        let mut builds = vec![build(&[("life", 5)], &[]); 3];
        balance_planes(&mut g, &mut builds, true, 4, PICKS);
        // One of four is already the human: nothing is lacking.
        assert!(builds.iter().all(|b| !myrran(b)), "seed {seed}");
        assert!(builds.iter().all(|b| book_total(b) == 5), "seed {seed}");
    }
}

#[test]
fn too_many_myrran_wizards_are_made_ordinary_and_refilled() {
    for seed in SEEDS {
        let mut g = new_game(seed);
        let mut builds = vec![build(&[("nature", 1)], &[Retort::new(9) /* myrran */]); 4];
        balance_planes(&mut g, &mut builds, false, 4, PICKS);
        let (_, most) = myrran_range(4);
        assert_eq!(
            builds.iter().filter(|b| myrran(b)).count(),
            most,
            "seed {seed}"
        );
        for b in builds.iter().filter(|b| !myrran(b)) {
            assert_sound(b, PICKS);
            assert!(
                b.picks_spent(&rules()) > 1,
                "seed {seed}: not refilled: {b:?}"
            );
        }
    }
}

#[test]
fn the_refill_spends_the_picks_without_breaking_a_rule() {
    let mut spent = BTreeSet::new();
    for seed in SEEDS {
        let mut g = new_game(seed);
        let mut b = build(&[], &[]);
        refill(&mut g, &mut b, PICKS);
        assert_sound(&b, PICKS);
        assert!(!myrran(&b), "seed {seed}: the refill picked Myrran");
        assert!(
            books(&b, realm("life")) == 0 || books(&b, realm("death")) == 0,
            "seed {seed}: opposed realms in {b:?}"
        );
        spent.insert(b.picks_spent(&rules()));
    }
    // Most fill the budget; the odd wizard may be left a pick short when no roll fitted.
    assert!(
        spent.contains(&PICKS),
        "no seed filled the budget: {spent:?}"
    );
}

#[test]
fn the_refill_leaves_a_full_build_alone() {
    let mut g = new_game(3);
    let mut b = build(&[("life", 5), ("nature", 6)], &[]);
    let before = b.clone();
    refill(&mut g, &mut b, PICKS);
    assert_eq!(b, before);
}

#[test]
fn the_same_seed_makes_the_same_wizard() {
    let make_one = |seed| {
        let mut g = new_game(seed);
        let mut b = build(&[], &[]);
        refill(&mut g, &mut b, PICKS);
        b
    };
    assert_eq!(make_one(7), make_one(7));
    let all: BTreeSet<String> = SEEDS.map(|s| format!("{:?}", make_one(s))).collect();
    assert!(
        all.len() > 10,
        "the seeds make hardly any different wizards"
    );
}

#[test]
fn books_stop_at_the_limit_and_at_an_opposed_realm() {
    let mut b = build(&[("life", 9)], &[]);
    add_books(&mut b, realm("life"), 4, Some(realm("death")));
    assert_eq!(books(&b, realm("life")), MAX_BOOKS);
    add_books(&mut b, realm("life"), 1, Some(realm("death")));
    assert_eq!(books(&b, realm("life")), MAX_BOOKS);

    let mut b = build(&[("life", 2)], &[]);
    add_books(&mut b, realm("death"), 3, Some(realm("life")));
    assert_eq!(books(&b, realm("death")), 0);
    add_books(&mut b, realm("nature"), 3, None);
    assert_eq!(books(&b, realm("nature")), 3);
}

#[test]
fn a_retort_needs_its_books_and_its_picks() {
    for seed in SEEDS {
        let mut g = new_game(seed);
        // No books: no mastery can come, and only picks for a one-pick retort remain.
        let mut b = build(&[("life", 1)], &[]);
        for _ in 0..200 {
            add_retort(&mut g, &mut b, 3);
        }
        assert_sound(&b, 3);
        assert!(!myrran(&b), "seed {seed}");
        assert!(
            b.retorts
                .iter()
                .all(|&r| r != Retort::new(6) /* divine_power */ && r != Retort::new(3) /* nature_mastery */),
            "seed {seed}: {b:?}"
        );
    }
}

#[test]
fn a_mastery_comes_from_a_realm_of_four_books() {
    let mut got: BTreeSet<Retort> = BTreeSet::new();
    for seed in SEEDS {
        let mut g = new_game(seed);
        let mut b = build(&[("nature", 4), ("chaos", 1)], &[]);
        for _ in 0..300 {
            add_retort(&mut g, &mut b, PICKS);
        }
        assert_sound(&b, PICKS);
        got.extend(
            b.retorts
                .iter()
                .copied()
                .filter(|&r| r == Retort::new(3) /* nature_mastery */ || r == Retort::new(2) /* chaos_mastery */),
        );
    }
    assert!(got.contains(&Retort::new(3) /* nature_mastery */));
    assert!(
        !got.contains(&Retort::new(2) /* chaos_mastery */),
        "chaos has one book"
    );
}

#[test]
fn the_ready_made_wizards_are_drawn_once_each() {
    let defs = vec![
        wizard_def(1, "a", &[("life", 5)], &[]),
        wizard_def(2, "b", &[("nature", 5)], &[]),
        wizard_def(3, "c", &[("chaos", 5)], &[]),
    ];
    for seed in SEEDS {
        let mut g = game_with(seed, defs.clone());
        let mut taken = vec!["a".to_string()];
        let builds = make(&mut g, &settings(2, 5), &mut taken, 2, false, &plain);
        assert_eq!(builds.len(), 2);
        assert!(builds.iter().all(|b| b.preset));
        let set: BTreeSet<&String> = taken.iter().collect();
        assert_eq!(taken.len(), 3, "seed {seed}: {taken:?}");
        assert_eq!(set.len(), 3, "seed {seed}: a wizard was drawn twice");
        let places: BTreeSet<Option<u8>> = builds.iter().map(|b| b.portrait).collect();
        assert_eq!(places, BTreeSet::from([Some(1), Some(2)]), "seed {seed}");
    }
}

#[test]
fn without_ready_made_wizards_the_fallback_builds_them() {
    let mut g = game_with(4, Vec::new());
    let mut taken = Vec::new();
    // The human wizard is the Myrran one the planes lack, so nothing is changed.
    let builds = make(&mut g, &settings(2, 5), &mut taken, 2, true, &plain);
    assert_eq!(builds.len(), 2);
    assert!(taken.is_empty());
    for b in &builds {
        assert_eq!(books(b, realm("life")), 3);
        assert_eq!(books(b, realm("nature")), 2);
        assert!(b.preset, "they are marked as made by the same path");
    }
}

#[test]
fn the_same_seed_draws_the_same_wizards() {
    let defs: Vec<WizardDef> = (1..=6)
        .map(|n| {
            wizard_def(
                n,
                &format!("w{n}"),
                &[("life", 2)],
                &[Retort::new(0) /* alchemy */],
            )
        })
        .collect();
    let run = |seed| {
        let mut g = game_with(seed, defs.clone());
        let mut s = settings(4, PICKS);
        s.ai.wizards.consolidate_percent = 50;
        s.ai.wizards.erasure_dice = (2, 3);
        make(&mut g, &s, &mut Vec::new(), 4, false, &plain)
    };
    assert_eq!(run(11), run(11));
    assert_ne!(run(11), run(12));
}

#[test]
fn opposed_realms_are_merged_into_one() {
    for seed in SEEDS {
        let mut g = game_with(
            seed,
            vec![wizard_def(1, "a", &[("life", 3), ("chaos", 2)], &[])],
        );
        let mut s = settings(1, 5);
        s.ai.wizards.consolidate_percent = 100;
        let builds = make(&mut g, &s, &mut Vec::new(), 1, false, &plain);
        let b = &builds[0];
        // Five picks are spent, so the refill adds nothing.
        assert_eq!(b.books.len(), 1, "seed {seed}: {b:?}");
        assert_eq!(book_total(b), 5, "seed {seed}: {b:?}");
    }
}

#[test]
fn no_merge_happens_at_zero_percent() {
    for seed in SEEDS {
        let mut g = game_with(
            seed,
            vec![wizard_def(1, "a", &[("life", 3), ("chaos", 2)], &[])],
        );
        let builds = make(&mut g, &settings(1, 5), &mut Vec::new(), 1, false, &plain);
        assert_eq!(builds[0].books.len(), 2, "seed {seed}");
    }
}

#[test]
fn the_erasure_dice_take_books_but_never_the_last_of_a_realm() {
    let def = wizard_def(
        1,
        "a",
        &[("life", 4), ("nature", 4)],
        &[
            Retort::new(0), /* alchemy */
            Retort::new(1), /* warlord */
        ],
    );
    let mut lost_books = false;
    let mut lost_retorts = false;
    for seed in SEEDS {
        let mut g = game_with(seed, vec![def.clone()]);
        // No picks to fill up, so the refill adds nothing back.
        let mut s = settings(1, 0);
        s.ai.wizards.erasure_dice = (3, 6);
        let builds = make(&mut g, &s, &mut Vec::new(), 1, false, &plain);
        let b = &builds[0];
        assert!(
            books(b, realm("life")) >= 1 && books(b, realm("life")) <= 4,
            "seed {seed}: {b:?}"
        );
        assert!(
            books(b, realm("nature")) >= 1 && books(b, realm("nature")) <= 4,
            "seed {seed}: {b:?}"
        );
        assert!(
            b.retorts.is_empty() || b.retorts.len() == 2,
            "seed {seed}: {b:?}"
        );
        lost_books |= book_total(b) < 8;
        lost_retorts |= b.retorts.is_empty();
    }
    assert!(lost_books, "no seed lost a book");
    assert!(lost_retorts, "no seed lost its retorts");
}

#[test]
fn the_myrran_rule_keeps_the_share_within_the_planes() {
    let mut defs = Vec::new();
    for n in 1..=3 {
        defs.push(wizard_def(
            n,
            &format!("m{n}"),
            &[("life", 3)],
            &[Retort::new(9) /* myrran */],
        ));
    }
    for n in 4..=6 {
        defs.push(wizard_def(n, &format!("o{n}"), &[("life", 3)], &[]));
    }
    for seed in SEEDS {
        let mut g = game_with(seed, defs.clone());
        let mut s = settings(3, PICKS);
        s.ai.wizards.myrran_rule = true;
        let builds = make(&mut g, &s, &mut Vec::new(), 3, false, &plain);
        let (least, most) = myrran_range(4);
        let n = builds.iter().filter(|b| myrran(b)).count();
        // The human is not Myrran, so only the three computer wizards count.
        assert!((least..=most).contains(&n), "seed {seed}: {n} Myrran");
        for b in &builds {
            assert_sound(b, PICKS);
        }
    }
}
