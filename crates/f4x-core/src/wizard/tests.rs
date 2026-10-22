use super::*;
use crate::game::testutil::content_with_magic;
use crate::player::Player;

const P0: PlayerId = PlayerId(0);

fn game() -> Game {
    let mut g = Game::new(5);
    g.content = content_with_magic();
    g.add_player(P0, Player::new(P0, "W"));
    g
}

/// The `n`th real spell of a realm and rarity, in the order of the game's spell list.
fn nth(g: &Game, realm: &str, rarity: Rarity, n: usize) -> crate::rules::SpellId {
    g.content
        .rules
        .spells
        .iter()
        .filter(|(_, d)| d.realm == realm && d.rarity == rarity)
        .nth(n)
        .map(|(id, _)| id)
        .expect("the game has ten spells of each realm and rarity")
}

fn build(books: &[(&str, u8)], retorts: &[Retort]) -> WizardBuild {
    WizardBuild {
        name: "Test".into(),
        books: books.iter().map(|&(r, n)| (r.to_string(), n)).collect(),
        retorts: retorts.iter().copied().collect(),
        ..Default::default()
    }
}

#[test]
fn retorts_cost_one_two_or_three_picks() {
    assert_eq!(retort_cost(Retort::Alchemy), 1);
    for r in [
        Retort::Warlord,
        Retort::Channeler,
        Retort::DivinePower,
        Retort::Famous,
        Retort::InfernalPower,
    ] {
        assert_eq!(retort_cost(r), 2, "{r:?}");
    }
    assert_eq!(retort_cost(Retort::Myrran), 3);
    assert_eq!(ALL_RETORTS.len(), 18);
}

#[test]
fn some_retorts_need_books() {
    let none = BTreeMap::new();
    let mut b: BTreeMap<String, u8> = BTreeMap::new();
    assert!(retort_available(Retort::Alchemy, &none));
    assert!(!retort_available(Retort::Archmage, &none));
    assert!(!retort_available(Retort::SageMaster, &none));
    b.insert("life".into(), 4);
    assert!(retort_available(Retort::DivinePower, &b));
    assert!(retort_available(Retort::Archmage, &b));
    assert!(retort_available(Retort::ManaFocusing, &b));
    assert!(!retort_available(Retort::InfernalPower, &b));
    assert!(!retort_available(Retort::SageMaster, &b), "one realm only");
    b.insert("death".into(), 1);
    assert!(retort_available(Retort::SageMaster, &b));
    assert!(!retort_available(Retort::Runemaster, &b));
    b.insert("death".into(), 2);
    b.insert("chaos".into(), 2);
    b.insert("life".into(), 2);
    assert!(
        retort_available(Retort::Runemaster, &b),
        "two books in three realms"
    );
    assert!(!retort_available(Retort::NodeMastery, &b));
    for r in ["chaos", "sorcery", "nature"] {
        b.insert(r.into(), 1);
    }
    assert!(retort_available(Retort::NodeMastery, &b));
}

#[test]
fn a_wizard_spends_exactly_eleven_picks() {
    let g = game();
    assert_eq!(
        g.check_wizard_build(&build(&[("life", 5), ("chaos", 5)], &[Retort::Alchemy])),
        Ok(())
    );
    assert_eq!(
        g.check_wizard_build(&build(&[("life", 5), ("chaos", 5)], &[])),
        Err(BuildError::UnspentPicks { left: 1 })
    );
    assert_eq!(
        g.check_wizard_build(&build(&[("life", 6), ("chaos", 5)], &[Retort::Alchemy])),
        Err(BuildError::TooManyPicks { spent: 12 })
    );
    assert_eq!(
        g.check_wizard_build(&build(&[("life", 9)], &[Retort::Warlord])),
        Ok(()),
        "warlord costs two"
    );
    assert_eq!(
        g.check_wizard_build(&build(&[("life", 3), ("chaos", 3)], &[Retort::Archmage])),
        Err(BuildError::RetortNotAvailable(Retort::Archmage))
    );
    assert_eq!(
        g.check_wizard_build(&build(&[("magic", 11)], &[])),
        Err(BuildError::UnknownRealm("magic".into()))
    );
}

#[test]
fn at_most_six_retorts() {
    let g = game();
    let seven = [
        Retort::Alchemy,
        Retort::Conjurer,
        Retort::Artificer,
        Retort::Charismatic,
        Retort::Warlord,
        Retort::Famous,
        Retort::Channeler,
    ];
    assert_eq!(
        g.check_wizard_build(&build(&[], &seven)),
        Err(BuildError::TooManyRetorts)
    );
}

#[test]
fn starting_spells_are_chosen_then_defaulted() {
    let mut g = game();
    let (c3, c7) = (
        nth(&g, "life", Rarity::Common, 3),
        nth(&g, "life", Rarity::Common, 7),
    );
    let mut b = build(&[("life", 4), ("chaos", 6)], &[Retort::Alchemy]);
    b.spells = vec![c7, c3];
    g.create_wizard(P0, &b).unwrap();
    let m = g.magic(P0).unwrap();
    // Four life books: three commons; two were chosen, one is filled in.
    let life: Vec<_> = m
        .known
        .iter()
        .filter(|&&s| g.content.rules.spells.get(s).realm == "life")
        .collect();
    assert_eq!(life.len(), 3);
    assert!(m.known.contains(&c7) && m.known.contains(&c3));
    // Six chaos books: five commons, all from the pool.
    let chaos = m
        .known
        .iter()
        .filter(|&&s| g.content.rules.spells.get(s).realm == "chaos")
        .count();
    assert_eq!(chaos, 5);
    assert_eq!(g.player(P0).unwrap().name, "Test");
}

#[test]
fn bad_spell_choices_are_refused() {
    let mut g = game();
    let uncommon = nth(&g, "life", Rarity::Uncommon, 0);
    let common = nth(&g, "life", Rarity::Common, 0);
    let other = nth(&g, "chaos", Rarity::Common, 0);
    let mut b = build(&[("life", 5), ("chaos", 5)], &[Retort::Alchemy]);
    b.spells = vec![uncommon];
    assert_eq!(g.create_wizard(P0, &b), Err(BuildError::BadSpell(uncommon)));
    b.spells = vec![common, common];
    assert_eq!(g.create_wizard(P0, &b), Err(BuildError::BadSpell(common)));
    let mut one_book = build(&[("life", 1), ("chaos", 9)], &[Retort::Alchemy]);
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
fn a_wizard_chooses_a_banner_and_it_is_saved() {
    let mut g = game();
    let mut b = build(&[("life", 5), ("chaos", 5)], &[Retort::Alchemy]);
    assert_eq!(g.player(P0).unwrap().banner, 0, "player 0 starts blue");
    b.banner = Some(3);
    g.create_wizard(P0, &b).unwrap();
    assert_eq!(g.player(P0).unwrap().banner, 3);
    b.banner = Some(9);
    g.create_wizard(P0, &b).unwrap();
    assert_eq!(
        g.player(P0).unwrap().banner,
        4,
        "kept within the five choices"
    );
    let content = g.content.clone();
    let back = g.snapshot().restore(content).unwrap();
    assert_eq!(back.player(P0).unwrap().banner, 4);
    assert_eq!(
        Player::new(PlayerId(u8::MAX), "n").banner,
        crate::player::NEUTRAL_BANNER
    );
}

#[test]
fn presets_come_from_the_rules() {
    let mut g = game();
    g.content.rules.wizards = crate::rules::Registry::new(vec![crate::rules::WizardDef {
        id: "lo_pan".into(),
        name: "Lo Pan".into(),
        description: String::new(),
        books: vec![("life".into(), 5), ("chaos".into(), 5)],
        retorts: vec![Retort::Channeler],
    }]);
    let b = g.preset_build("lo_pan").unwrap();
    assert!(b.preset);
    assert_eq!(b.picks_spent(), 12, "the original's Lo Pan is over budget");
    g.create_wizard(P0, &b).unwrap();
    // His picture is his place in the list, as in the original.
    let place = g
        .content
        .rules
        .wizards
        .defs()
        .iter()
        .position(|w| w.id == "lo_pan")
        .unwrap();
    assert_eq!(g.player(P0).unwrap().portrait as usize, place);
    let restored = g.snapshot().restore(g.content.clone()).unwrap();
    assert_eq!(restored.player(P0).unwrap().portrait as usize, place);
    assert!(g.magic(P0).unwrap().has(Retort::Channeler));
    assert!(g.preset_build("nobody").is_none());
}

#[test]
fn eleven_books_know_every_common_spell_and_pick_two_uncommon_and_one_rare() {
    let mut g = game();
    let life = |g: &Game, rarity, n| nth(g, "life", rarity, n);
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
    let b = build(&[("life", 11)], &[]);
    assert_eq!(b.spell_choices("life"), 0, "the commons are all given");
    assert_eq!(b.spell_choices_of("life", Rarity::Uncommon), 2);
    assert_eq!(b.spell_choices_of("life", Rarity::Rare), 1);
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
    g.create_wizard(P0, &chosen).unwrap();
    let magic = (*g.magic(P0).unwrap()).clone();
    assert_eq!(magic.books_in("life"), 11);
    let known = |spell| magic.known.contains(&spell);
    assert!(
        (0..10).all(|i| known(life(&g, Rarity::Common, i))),
        "every common"
    );
    assert!(known(u1) && known(u2) && known(rare));
    assert!(!known(u0) && !known(r0), "only what was picked");
    let researchable = |spell| magic.researchable.contains(&spell);
    assert!(researchable(u0) && researchable(very_rare) && researchable(r0));
    // Without a choice the realm's defaults fill in: still two uncommon and one rare.
    let mut g = game();
    g.create_wizard(P0, &b).unwrap();
    let magic = (*g.magic(P0).unwrap()).clone();
    let count = |r: Rarity| {
        magic
            .known
            .iter()
            .filter(|&&s| g.content.rules.spells.get(s).rarity == r)
            .count()
    };
    assert_eq!((count(Rarity::Uncommon), count(Rarity::Rare)), (2, 1));
}

#[test]
fn ten_books_still_choose_nine_commons_and_nothing_else() {
    let b = build(&[("life", 10)], &[]);
    assert_eq!(b.spell_choices("life"), 9);
    assert_eq!(b.spell_choices_of("life", Rarity::Uncommon), 0);
    assert_eq!(b.spell_choices_of("life", Rarity::Rare), 0);
    assert_eq!(b.picks_spent(), 10);
}

#[test]
fn eight_spells_are_on_offer_for_research_lowest_rarity_first() {
    use crate::wizard::magic::RESEARCH_SLOTS;
    let mut g = game();
    g.create_wizard(P0, &build(&[("life", 6), ("chaos", 5)], &[]))
        .unwrap();
    let magic = (*g.magic(P0).unwrap()).clone();
    assert_eq!(magic.candidates.len(), RESEARCH_SLOTS);
    assert!(magic
        .candidates
        .iter()
        .all(|s| magic.researchable.contains(s)));
    // Lowest rarity first: a realm's uncommon spell is offered only once all its researchable
    // commons are on the offer.
    for realm in ["life", "chaos"] {
        let of_realm = |s: &&crate::rules::SpellId| g.content.rules.spells.get(**s).realm == realm;
        let commons_left_out = magic
            .researchable
            .iter()
            .filter(of_realm)
            .filter(|&&s| g.content.rules.spells.get(s).rarity == Rarity::Common)
            .any(|s| !magic.candidates.contains(s));
        let offered_other = magic
            .candidates
            .iter()
            .filter(of_realm)
            .any(|&s| g.content.rules.spells.get(s).rarity != Rarity::Common);
        assert!(!(commons_left_out && offered_other), "{realm}");
    }
    // Only an offered spell can be researched.
    let outside = *magic
        .researchable
        .iter()
        .find(|s| !magic.candidates.contains(s))
        .unwrap();
    assert_eq!(
        g.set_research(P0, outside),
        Err(crate::wizard::magic::MagicError::NotAvailable)
    );
    let pick = magic.candidates[3];
    g.set_research(P0, pick).unwrap();
    // Learning it takes it off the offer, and the offer is topped up again.
    g.magic_mut(P0).unwrap().research_remaining = 0;
    let events = g.process_magic();
    assert!(events
        .iter()
        .any(|e| matches!(e, crate::game::Event::SpellLearned { spell, .. } if *spell == pick)));
    let after = g.magic(P0).unwrap();
    assert!(after.known.contains(&pick) && !after.candidates.contains(&pick));
    assert_eq!(after.candidates.len(), RESEARCH_SLOTS);
}
