use super::*;
use crate::game::testutil::shipped;

use crate::rules::RealmId;
use f4x_def::testdir::TestResult;

const P0: PlayerId = PlayerId(0);

fn game() -> Game {
    let mut g = Game::new(5);
    g.content = shipped();
    g.add_player(P0, "W");
    g
}

#[test]
fn a_player_and_its_wizard_are_made_together_and_point_at_each_other() -> TestResult {
    let g = game();
    let w = g.wizard(P0).ok_or("wizard is missing")?;
    assert_eq!(w.name(), "W");
    assert_eq!(w.player(), g.player_entity(P0));
    let back = g.world.get::<&entity::PlayerLink>(w.entity())?.0;
    let link = g.world.get::<&crate::player::WizardLink>(back)?.0;
    assert_eq!(link, w.entity());
    Ok(())
}

#[test]
fn an_independent_city_has_no_wizard_and_is_called_by_its_city() -> TestResult {
    let mut g = game();
    let town = PlayerId(9);
    g.add_neutral_player(town);
    assert!(g.wizard(town).is_none());
    assert_eq!(g.player(town).ok_or("player is missing")?.name(), "");
    g.spawn_city(
        town,
        crate::map::Position {
            plane: crate::map::Plane::Arcanus,
            x: 2,
            y: 2,
        },
        "Free Town".into(),
        None,
        4,
    );
    assert_eq!(
        g.player(town).ok_or("player is missing")?.name(),
        "Free Town"
    );
    assert_eq!(
        g.player(town).ok_or("player is missing")?.banner(),
        crate::player::NEUTRAL_BANNER
    );
    Ok(())
}

/// The `n`th real spell of a realm and rarity, in the order of the game's spell list.
fn nth(g: &Game, realm: u16, rarity: Rarity, n: usize) -> crate::rules::SpellId {
    let realm = realm_id(realm);
    g.content
        .rules
        .spells
        .iter()
        .filter(|(_, d)| d.realm == (realm) && d.rarity == rarity)
        .nth(n)
        .map(|(id, _)| id)
        .expect("the game has ten spells of each realm and rarity")
}

fn realm_id(alias: u16) -> crate::rules::RealmId {
    RealmId::new(alias)
}

fn build(books: &[(RealmId, u8)], retorts: &[Retort]) -> WizardBuild {
    WizardBuild {
        name: "Test".into(),
        books: books.iter().copied().collect(),
        retorts: retorts.iter().copied().collect(),
        ..Default::default()
    }
}

#[test]
fn retorts_cost_one_two_or_three_picks() {
    let rules = &shipped().rules;
    assert_eq!(retort_cost(rules, Retort::new(0) /* alchemy */), 1);
    for r in [
        Retort::new(1),  /* warlord */
        Retort::new(8),  /* channeler */
        Retort::new(6),  /* divine_power */
        Retort::new(13), /* famous */
        Retort::new(5),  /* infernal_power */
    ] {
        assert_eq!(retort_cost(rules, r), 2, "{r:?}");
    }
    assert_eq!(retort_cost(rules, Retort::new(9) /* myrran */), 3);
    assert_eq!(all_retorts(rules).count(), 18);
}

#[test]
fn a_build_may_hold_as_many_books_in_a_realm_as_the_game_allows() {
    let life = RealmId::new(3);
    let mut g = game();
    // 12 books are over the original's 11 and under what a game set to 13 allows; the picks
    // rule does not apply to a ready-made build, so only the books are checked.
    let mut b = build(&[(life, 12)], &[]);
    b.preset = true;
    assert_eq!(
        g.check_wizard_build(&b),
        Err(BuildError::TooManyBooks(life))
    );
    g.settings.wizards.max_books_per_realm = 13;
    assert_eq!(g.check_wizard_build(&b), Ok(()));
    g.settings.wizards.max_books_per_realm = 8;
    b.books.insert(life, 9);
    assert_eq!(
        g.check_wizard_build(&b),
        Err(BuildError::TooManyBooks(life))
    );
}

#[test]
fn a_build_may_hold_as_many_books_in_all_as_the_game_allows() {
    let (life, nature) = (RealmId::new(3), RealmId::new(0));
    let mut g = game();
    let mut b = build(&[(life, 7), (nature, 6)], &[]);
    b.preset = true;
    assert_eq!(g.check_wizard_build(&b), Err(BuildError::TooManyBooksInAll));
    g.settings.wizards.max_books_total = 13;
    assert_eq!(g.check_wizard_build(&b), Ok(()));
}

#[test]
fn a_wizard_holds_no_more_books_in_a_realm_than_the_setting_allows() -> TestResult {
    let life = RealmId::new(3);
    for (most, wanted, held) in [(11, 15, 11), (13, 15, 13), (13, 12, 12), (5, 7, 5)] {
        let mut g = game();
        g.settings.wizards.max_books_per_realm = most;
        g.init_wizard(P0, &[(life, wanted)], &[], None)
            .map_err(|e| format!("{e:?}"))?;
        let w = g.wizard(P0).ok_or("wizard is missing")?;
        assert_eq!(w.books_in(life), held, "most {most}, wanted {wanted}");
    }
    Ok(())
}

#[test]
fn a_ready_made_wizard_over_the_most_books_loses_the_surplus_and_the_picks_are_free() {
    let g = game();
    let (life, nature) = (RealmId::new(3), RealmId::new(0));
    let mut ariel = g.preset_build(11).expect("Ariel is a ready-made wizard");
    assert_eq!(ariel.books[&life], 10);
    assert!(ariel.preset);
    assert!(!ariel.trim_books(11), "nothing over 11");
    assert!(ariel.preset, "and so still ready-made");
    assert!(ariel.trim_books(8));
    assert_eq!(ariel.books[&life], 8);
    assert!(!ariel.preset, "the pick rules ask for the two picks");
    let retorts: u8 = ariel
        .retorts
        .iter()
        .map(|&r| retort_cost(&g.content.rules, r))
        .sum();
    assert_eq!(
        ariel.picks_left(&g.content.rules),
        i16::from(11 - 8 - retorts)
    );
    assert!(
        ariel.picks_left(&g.content.rules) >= 2,
        "the two surplus books' picks at least"
    );
    // A wizard with no realm over it is untouched.
    let mut merlin = g.preset_build(0).expect("Merlin is a ready-made wizard");
    assert!(!merlin.trim_books(8));
    assert_eq!((merlin.books[&life], merlin.books[&nature]), (5, 5));
}

#[test]
fn the_myrran_and_alchemy_options_change_what_the_retorts_cost_in_a_game() {
    let mut g = game();
    let (alchemy, myrran) = (Retort::new(0), Retort::new(9));
    assert_eq!(retort_cost(&g.content.rules, myrran), 3);
    assert_eq!(g.content.settings.myrror_fortress_power, 5);
    g.settings.rules.two_pick_myrran = true;
    g.apply_content_settings();
    assert_eq!(retort_cost(&g.content.rules, myrran), 2);
    assert_eq!(
        retort_cost(&g.content.rules, alchemy),
        1,
        "Alchemy is not changed"
    );
    assert_eq!(g.content.settings.myrror_fortress_power, 0);
    g.settings.rules.adamant_alchemy = true;
    g.apply_content_settings();
    assert_eq!(retort_cost(&g.content.rules, alchemy), 2);
}

#[test]
fn some_retorts_need_books() {
    let rules = &shipped().rules;
    let none = BTreeMap::new();
    let mut b: BTreeMap<crate::rules::RealmId, u8> = BTreeMap::new();
    assert!(retort_available(
        rules,
        Retort::new(0), /* alchemy */
        &none
    ));
    assert!(!retort_available(
        rules,
        Retort::new(10), /* archmage */
        &none
    ));
    assert!(!retort_available(
        rules,
        Retort::new(7), /* sage_master */
        &none
    ));
    b.insert(RealmId::new(3 /* life */), 4);
    assert!(retort_available(
        rules,
        Retort::new(6), /* divine_power */
        &b
    ));
    assert!(retort_available(
        rules,
        Retort::new(10), /* archmage */
        &b
    ));
    assert!(retort_available(
        rules,
        Retort::new(11), /* mana_focusing */
        &b
    ));
    assert!(!retort_available(
        rules,
        Retort::new(5), /* infernal_power */
        &b
    ));
    assert!(
        !retort_available(rules, Retort::new(7) /* sage_master */, &b),
        "one realm only"
    );
    b.insert(RealmId::new(4 /* death */), 1);
    assert!(retort_available(
        rules,
        Retort::new(7), /* sage_master */
        &b
    ));
    assert!(!retort_available(
        rules,
        Retort::new(14), /* runemaster */
        &b
    ));
    b.insert(RealmId::new(4 /* death */), 2);
    b.insert(RealmId::new(2 /* chaos */), 2);
    b.insert(RealmId::new(3 /* life */), 2);
    assert!(
        retort_available(rules, Retort::new(14) /* runemaster */, &b),
        "two books in three realms"
    );
    assert!(!retort_available(
        rules,
        Retort::new(12), /* node_mastery */
        &b
    ));
    for r in [
        2, /* chaos */
        1, /* sorcery */
        0, /* nature */
    ] {
        b.insert(realm_id(r), 1);
    }
    assert!(retort_available(
        rules,
        Retort::new(12), /* node_mastery */
        &b
    ));
}

#[test]
fn a_wizard_spends_exactly_eleven_picks() {
    let g = game();
    assert_eq!(
        g.check_wizard_build(&build(
            &[
                (RealmId::new(3 /* life */), 5),
                (RealmId::new(2 /* chaos */), 5)
            ],
            &[Retort::new(0) /* alchemy */]
        )),
        Ok(())
    );
    assert_eq!(
        g.check_wizard_build(&build(
            &[
                (RealmId::new(3 /* life */), 5),
                (RealmId::new(2 /* chaos */), 5)
            ],
            &[]
        )),
        Err(BuildError::UnspentPicks { left: 1 })
    );
    assert_eq!(
        g.check_wizard_build(&build(
            &[
                (RealmId::new(3 /* life */), 6),
                (RealmId::new(2 /* chaos */), 5)
            ],
            &[Retort::new(0) /* alchemy */]
        )),
        Err(BuildError::TooManyPicks { spent: 12 })
    );
    assert_eq!(
        g.check_wizard_build(&build(
            &[(RealmId::new(3 /* life */), 9)],
            &[Retort::new(1) /* warlord */]
        )),
        Ok(()),
        "warlord costs two"
    );
    assert_eq!(
        g.check_wizard_build(&build(
            &[
                (RealmId::new(3 /* life */), 3),
                (RealmId::new(2 /* chaos */), 3)
            ],
            &[Retort::new(10) /* archmage */]
        )),
        Err(BuildError::RetortNotAvailable(
            Retort::new(10) /* archmage */
        ))
    );
    assert_eq!(
        g.check_wizard_build(&WizardBuild {
            books: [(crate::rules::RealmId::new(99), 11)].into_iter().collect(),
            ..Default::default()
        }),
        Err(BuildError::UnknownRealm(crate::rules::RealmId::new(99)))
    );
}

#[test]
fn at_most_six_retorts() {
    let g = game();
    let seven = [
        Retort::new(0),  /* alchemy */
        Retort::new(15), /* conjurer */
        Retort::new(17), /* artificer */
        Retort::new(16), /* charismatic */
        Retort::new(1),  /* warlord */
        Retort::new(13), /* famous */
        Retort::new(8),  /* channeler */
    ];
    assert_eq!(
        g.check_wizard_build(&build(&[], &seven)),
        Err(BuildError::TooManyRetorts)
    );
}

#[test]
fn starting_spells_are_chosen_then_defaulted() -> TestResult {
    let mut g = game();
    let (c3, c7) = (
        nth(&g, 3 /* life */, Rarity::Common, 3),
        nth(&g, 3 /* life */, Rarity::Common, 7),
    );
    let mut b = build(
        &[
            (RealmId::new(3 /* life */), 4),
            (RealmId::new(2 /* chaos */), 6),
        ],
        &[Retort::new(0) /* alchemy */],
    );
    b.spells = vec![c7, c3];
    g.create_wizard(P0, &b).map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    // Four life books: three commons; two were chosen, one is filled in.
    let known = m.known();
    let life: Vec<_> = known
        .iter()
        .filter(
            |&&s| g.content.rules.spells.get(s).realm.num() == 3, /* life */
        )
        .collect();
    assert_eq!(life.len(), 3);
    assert!(m.knows(c7) && m.knows(c3));
    // Six chaos books: five commons, all from the pool.
    let chaos = m
        .known()
        .iter()
        .filter(
            |&&s| g.content.rules.spells.get(s).realm.num() == 2, /* chaos */
        )
        .count();
    assert_eq!(chaos, 5);
    assert_eq!(g.player(P0).ok_or("player is missing")?.name(), "Test");
    Ok(())
}

#[test]
fn bad_spell_choices_are_refused() {
    let mut g = game();
    let uncommon = nth(&g, 3 /* life */, Rarity::Uncommon, 0);
    let common = nth(&g, 3 /* life */, Rarity::Common, 0);
    let other = nth(&g, 2 /* chaos */, Rarity::Common, 0);
    let mut b = build(
        &[
            (RealmId::new(3 /* life */), 5),
            (RealmId::new(2 /* chaos */), 5),
        ],
        &[Retort::new(0) /* alchemy */],
    );
    b.spells = vec![uncommon];
    assert_eq!(g.create_wizard(P0, &b), Err(BuildError::BadSpell(uncommon)));
    b.spells = vec![common, common];
    assert_eq!(g.create_wizard(P0, &b), Err(BuildError::BadSpell(common)));
    let mut one_book = build(
        &[
            (RealmId::new(3 /* life */), 1),
            (RealmId::new(2 /* chaos */), 9),
        ],
        &[Retort::new(0) /* alchemy */],
    );
    one_book.spells = vec![common];
    assert_eq!(
        g.create_wizard(P0, &one_book),
        Err(BuildError::BadSpell(common)),
        "one book leaves no choices"
    );
    b.spells = vec![other, common];
    assert!(g.create_wizard(P0, &b).is_ok());
}

#[test]
fn a_wizard_chooses_a_banner_and_it_is_saved() -> TestResult {
    let mut g = game();
    let mut b = build(
        &[
            (RealmId::new(3 /* life */), 5),
            (RealmId::new(2 /* chaos */), 5),
        ],
        &[Retort::new(0) /* alchemy */],
    );
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.banner(),
        0,
        "player 0 starts blue"
    );
    b.banner = Some(3);
    g.create_wizard(P0, &b).map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.player(P0).ok_or("player is missing")?.banner(), 3);
    b.banner = Some(9);
    g.create_wizard(P0, &b).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.banner(),
        9,
        "orange is the tenth"
    );
    b.banner = Some(40);
    g.create_wizard(P0, &b).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.banner(),
        9,
        "kept within the ten choices"
    );
    let content = g.content.clone();
    let back = g.snapshot().restore(content)?;
    assert_eq!(back.player(P0).ok_or("player is missing")?.banner(), 9);
    g.add_neutral_player(PlayerId(9));
    assert_eq!(
        g.player(PlayerId(9)).ok_or("player is missing")?.banner(),
        crate::player::NEUTRAL_BANNER,
        "no wizard"
    );
    Ok(())
}

#[test]
fn presets_come_from_the_rules() -> TestResult {
    let mut g = game();
    let b = g
        .preset_build(3 /* lo_pan */)
        .ok_or("preset_build is missing")?;
    assert!(b.preset);
    assert_eq!(
        b.picks_spent(&game().content.rules),
        11,
        "nine books and the Channeler"
    );
    g.create_wizard(P0, &b).map_err(|e| format!("{e:?}"))?;
    // His picture is his place in the list, as in the original.
    let place = g
        .content
        .rules
        .wizards
        .defs()
        .iter()
        .position(|w| w.id == "lo_pan")
        .ok_or("position is missing")?;
    assert_eq!(
        g.player(P0).ok_or("player is missing")?.portrait() as usize,
        place
    );
    let restored = g.snapshot().restore(g.content.clone())?;
    assert_eq!(
        restored.player(P0).ok_or("player is missing")?.portrait() as usize,
        place
    );
    assert!(g
        .wizard(P0)
        .ok_or("wizard is missing")?
        .has(Retort::new(8) /* channeler */));
    assert!(g.preset_build(9999).is_none());
    Ok(())
}

#[test]
fn eleven_books_know_every_common_spell_and_pick_two_uncommon_and_one_rare() -> TestResult {
    let mut g = game();
    let life = |g: &Game, rarity, n| nth(g, 3 /* life */, rarity, n);
    let (u0, u1, u2) = (
        life(&g, Rarity::Uncommon, 0),
        life(&g, Rarity::Uncommon, 1),
        life(&g, Rarity::Uncommon, 2),
    );
    let (r0, rare, r4) = (
        life(&g, Rarity::Rare, 0),
        life(&g, Rarity::Rare, 3),
        life(&g, Rarity::Rare, 4),
    );
    let very_rare = life(&g, Rarity::VeryRare, 0);
    let b = build(&[(RealmId::new(3 /* life */), 11)], &[]);
    let lib = &g.content.settings.spell_library;
    assert_eq!(
        b.spell_choices_of(lib, realm_id(3 /* life */), Rarity::Common),
        0,
        "the commons are all given"
    );
    assert_eq!(
        b.spell_choices_of(lib, realm_id(3 /* life */), Rarity::Uncommon),
        2
    );
    assert_eq!(
        b.spell_choices_of(lib, realm_id(3 /* life */), Rarity::Rare),
        1
    );
    // Too many, or the wrong sort, are refused.
    for bad in [vec![u0, u1, u2], vec![rare, r4]] {
        let mut b = b.clone();
        b.spells = bad;
        assert!(matches!(
            g.create_wizard(P0, &b),
            Err(BuildError::BadSpell(_))
        ));
    }
    let mut chosen = b.clone();
    chosen.spells = vec![u1, u2, rare];
    g.create_wizard(P0, &chosen).map_err(|e| format!("{e:?}"))?;
    let magic = g.wizard(P0).ok_or("wizard is missing")?;
    assert_eq!(magic.books_in(realm_id(3 /* life */)), 11);
    let known = |spell| magic.knows(spell);
    assert!(
        (0..10).all(|i| known(life(&g, Rarity::Common, i))),
        "every common"
    );
    assert!(known(u1) && known(u2) && known(rare));
    assert!(!known(u0) && !known(r0), "only what was picked");
    let researchable = |spell| magic.can_research(spell);
    assert!(researchable(u0) && researchable(very_rare) && researchable(r0));
    // Without a choice the realm's defaults fill in: still two uncommon and one rare.
    let mut g = game();
    g.create_wizard(P0, &b).map_err(|e| format!("{e:?}"))?;
    let magic = g.wizard(P0).ok_or("wizard is missing")?;
    let count = |r: Rarity| {
        magic
            .known()
            .iter()
            .filter(|&&s| {
                let d = g.content.rules.spells.get(s);
                d.realm.num() == 3 /* life */ && d.rarity == r
            })
            .count()
    };
    assert_eq!((count(Rarity::Uncommon), count(Rarity::Rare)), (2, 1));
    Ok(())
}

#[test]
fn ten_books_still_choose_nine_commons_and_nothing_else() {
    let b = build(&[(RealmId::new(3 /* life */), 10)], &[]);
    let lib = &game().content.settings.spell_library;
    assert_eq!(
        b.spell_choices_of(lib, realm_id(3 /* life */), Rarity::Common),
        9
    );
    assert_eq!(
        b.spell_choices_of(lib, realm_id(3 /* life */), Rarity::Uncommon),
        0
    );
    assert_eq!(
        b.spell_choices_of(lib, realm_id(3 /* life */), Rarity::Rare),
        0
    );
    assert_eq!(b.picks_spent(&game().content.rules), 10);
}

#[test]
fn eight_spells_are_on_offer_for_research_lowest_rarity_first() -> TestResult {
    use crate::wizard::magic::RESEARCH_SLOTS;
    let mut g = game();
    g.create_wizard(
        P0,
        &build(
            &[
                (RealmId::new(3 /* life */), 6),
                (RealmId::new(2 /* chaos */), 5),
            ],
            &[],
        ),
    )
    .map_err(|e| format!("{e:?}"))?;
    let magic = g.wizard(P0).ok_or("wizard is missing")?;
    assert_eq!(magic.candidates().len(), RESEARCH_SLOTS);
    assert!(magic.candidates().iter().all(|s| magic.can_research(*s)));
    // Lowest rarity first: a realm's uncommon spell is offered only once all its researchable
    // commons are on the offer.
    for realm in [3 /* life */, 2 /* chaos */] {
        let of_realm = |s: &&crate::rules::SpellId| {
            g.content.rules.spells.get(**s).realm == RealmId::new(realm)
        };
        let commons_left_out = magic
            .researchable()
            .iter()
            .filter(of_realm)
            .filter(|&&s| g.content.rules.spells.get(s).rarity == Rarity::Common)
            .any(|s| !magic.candidates().contains(s));
        let offered_other = magic
            .candidates()
            .iter()
            .filter(of_realm)
            .any(|&s| g.content.rules.spells.get(s).rarity != Rarity::Common);
        assert!(!(commons_left_out && offered_other), "{realm}");
    }
    // Only an offered spell can be researched.
    let researchable = magic.researchable();
    let outside = *researchable
        .iter()
        .find(|s| !magic.candidates().contains(s))
        .ok_or("find is missing")?;
    let pick = magic.candidates()[3];
    assert_eq!(
        g.set_research(P0, outside),
        Err(crate::wizard::magic::MagicError::NotAvailable)
    );
    g.set_research(P0, pick).map_err(|e| format!("{e:?}"))?;
    // Learning it takes it off the offer, and the offer is topped up again.
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::Research>()
        .remaining = 0;
    let events = g.process_research_all();
    assert!(events
        .iter()
        .any(|e| matches!(e, crate::game::Event::SpellLearned { spell, .. } if *spell == pick)));
    let after = g.wizard(P0).ok_or("wizard is missing")?;
    assert!(after.knows(pick) && !after.candidates().contains(&pick));
    assert_eq!(after.candidates().len(), RESEARCH_SLOTS);
    Ok(())
}

/// A ready-made wizard: no pick budget, so a test can have any books.
fn preset(books: &[(RealmId, u8)], retorts: &[Retort]) -> WizardBuild {
    WizardBuild {
        preset: true,
        ..build(books, retorts)
    }
}

fn spell_id(g: &Game, id: u16) -> crate::rules::SpellId {
    g.content
        .rules
        .spells
        .id_numbered(id)
        .expect("id_of succeeds")
}

/// How many of the spells in `set` are of this realm and rarity.
fn of_kind(
    g: &Game,
    set: &std::collections::BTreeSet<crate::rules::SpellId>,
    realm: u16,
    rarity: Rarity,
) -> usize {
    set.iter()
        .filter(|&&s| {
            let d = g.content.rules.spells.get(s);
            d.realm == RealmId::new(realm) && d.rarity == rarity
        })
        .count()
}

#[test]
fn a_realm_starts_with_the_first_default_commons_known() -> TestResult {
    // The original: n books, n - 1 commons known, taken from the realm's default list.
    let mut g = game();
    g.create_wizard(P0, &preset(&[(RealmId::new(0 /* nature */), 3)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    for id in [6 /* war_bears */, 7 /* stone_skin */] {
        assert!(m.known().contains(&spell_id(&g, id)), "{id}");
    }
    assert!(
        !m.known().contains(&spell_id(&g, 9 /* sprites */)),
        "the third is researchable"
    );
    assert_eq!(of_kind(&g, &m.known(), 0 /* nature */, Rarity::Common), 2);
    Ok(())
}

#[test]
fn one_book_knows_no_spell_but_opens_three_commons_and_an_uncommon() -> TestResult {
    let mut g = game();
    g.create_wizard(P0, &preset(&[(RealmId::new(2 /* chaos */), 1)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    assert_eq!(of_kind(&g, &m.known(), 2 /* chaos */, Rarity::Common), 0);
    assert_eq!(
        of_kind(&g, &m.researchable(), 2 /* chaos */, Rarity::Common),
        3
    );
    assert_eq!(
        of_kind(&g, &m.researchable(), 2 /* chaos */, Rarity::Uncommon),
        1
    );
    assert_eq!(
        of_kind(&g, &m.researchable(), 2 /* chaos */, Rarity::Rare),
        0
    );
    Ok(())
}

#[test]
fn the_books_open_more_of_the_realm_as_in_the_tables() -> TestResult {
    // (books, researchable commons, uncommons, rares, very rares) in the original's tables.
    for (books, expect) in [
        (2, (5, 2, 1, 0)),
        (5, (8, 5, 4, 3)),
        (7, (10, 8, 6, 5)),
        (8, (10, 10, 7, 6)),
        (9, (10, 10, 9, 7)),
        (10, (10, 10, 10, 10)),
    ] {
        let mut g = game();
        g.create_wizard(P0, &preset(&[(RealmId::new(3 /* life */), books)], &[]))
            .map_err(|e| format!("{e:?}"))?;
        let m = g.wizard(P0).ok_or("wizard is missing")?;
        let open = |r: Rarity| {
            of_kind(&g, &m.researchable(), 3 /* life */, r)
                + of_kind(&g, &m.known(), 3 /* life */, r)
        };
        assert_eq!(
            (
                open(Rarity::Common),
                open(Rarity::Uncommon),
                open(Rarity::Rare),
                open(Rarity::VeryRare)
            ),
            expect,
            "{books} books"
        );
    }
    Ok(())
}

#[test]
fn eleven_books_know_the_default_uncommons_and_rare() -> TestResult {
    let mut g = game();
    g.create_wizard(P0, &preset(&[(RealmId::new(1 /* sorcery */), 11)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    for id in [
        56, /* flight */
        60, /* phantom_beast */
        65, /* storm_giant */
    ] {
        assert!(m.known().contains(&spell_id(&g, id)), "{id}");
    }
    assert_eq!(of_kind(&g, &m.known(), 1 /* sorcery */, Rarity::Common), 10);
    assert_eq!(
        of_kind(
            &g,
            &m.researchable(),
            1, /* sorcery */
            Rarity::VeryRare
        ),
        10
    );
    Ok(())
}

#[test]
fn every_wizard_knows_magic_spirit_and_the_spell_of_return() -> TestResult {
    let mut g = game();
    g.create_wizard(P0, &preset(&[(RealmId::new(3 /* life */), 2)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    assert!(m.known().contains(&spell_id(&g, 201 /* magic_spirit */)));
    assert!(m.known().contains(&spell_id(&g, 214 /* spell_of_return */)));
    assert!(
        !m.known().contains(&spell_id(&g, 207 /* enchant_item */)),
        "not without the retort"
    );
    assert!(!m
        .researchable()
        .contains(&spell_id(&g, 213 /* spell_of_mastery */)));
    assert!(m
        .researchable()
        .contains(&spell_id(&g, 207 /* enchant_item */)));
    Ok(())
}

#[test]
fn an_artificer_also_knows_enchant_item_and_create_artifact() -> TestResult {
    let mut g = game();
    g.create_wizard(
        P0,
        &preset(
            &[
                (RealmId::new(3 /* life */), 2),
                (RealmId::new(2 /* chaos */), 2),
            ],
            &[Retort::new(17) /* artificer */],
        ),
    )
    .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    for id in [207 /* enchant_item */, 211 /* create_artifact */] {
        assert!(m.known().contains(&spell_id(&g, id)), "{id}");
        assert!(!m.researchable().contains(&spell_id(&g, id)), "{id}");
    }
    Ok(())
}
