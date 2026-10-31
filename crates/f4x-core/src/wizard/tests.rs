use super::*;
use crate::game::testutil::shipped;
use crate::testdir::TestResult;

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
    let serial = g.take_serial();
    let owner = g.player_entity(town).ok_or("player_entity is missing")?;
    g.world.spawn((
        crate::city::City {
            name: "Free Town".into(),
        },
        crate::player::Owner(owner),
        serial,
    ));
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
fn starting_spells_are_chosen_then_defaulted() -> TestResult {
    let mut g = game();
    let (c3, c7) = (
        nth(&g, "life", Rarity::Common, 3),
        nth(&g, "life", Rarity::Common, 7),
    );
    let mut b = build(&[("life", 4), ("chaos", 6)], &[Retort::Alchemy]);
    b.spells = vec![c7, c3];
    g.create_wizard(P0, &b).map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    // Four life books: three commons; two were chosen, one is filled in.
    let known = m.known();
    let life: Vec<_> = known
        .iter()
        .filter(|&&s| g.content.rules.spells.get(s).realm == "life")
        .collect();
    assert_eq!(life.len(), 3);
    assert!(m.knows(c7) && m.knows(c3));
    // Six chaos books: five commons, all from the pool.
    let chaos = m
        .known()
        .iter()
        .filter(|&&s| g.content.rules.spells.get(s).realm == "chaos")
        .count();
    assert_eq!(chaos, 5);
    assert_eq!(g.player(P0).ok_or("player is missing")?.name(), "Test");
    Ok(())
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
fn a_wizard_chooses_a_banner_and_it_is_saved() -> TestResult {
    let mut g = game();
    let mut b = build(&[("life", 5), ("chaos", 5)], &[Retort::Alchemy]);
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
    let b = g.preset_build("lo_pan").ok_or("preset_build is missing")?;
    assert!(b.preset);
    assert_eq!(b.picks_spent(), 11, "nine books and the Channeler");
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
        .has(Retort::Channeler));
    assert!(g.preset_build("nobody").is_none());
    Ok(())
}

#[test]
fn eleven_books_know_every_common_spell_and_pick_two_uncommon_and_one_rare() -> TestResult {
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
    let lib = &g.content.settings.spell_library;
    assert_eq!(
        b.spell_choices_of(lib, "life", Rarity::Common),
        0,
        "the commons are all given"
    );
    assert_eq!(b.spell_choices_of(lib, "life", Rarity::Uncommon), 2);
    assert_eq!(b.spell_choices_of(lib, "life", Rarity::Rare), 1);
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
    assert_eq!(magic.books_in("life"), 11);
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
                d.realm == "life" && d.rarity == r
            })
            .count()
    };
    assert_eq!((count(Rarity::Uncommon), count(Rarity::Rare)), (2, 1));
    Ok(())
}

#[test]
fn ten_books_still_choose_nine_commons_and_nothing_else() {
    let b = build(&[("life", 10)], &[]);
    let lib = &game().content.settings.spell_library;
    assert_eq!(b.spell_choices_of(lib, "life", Rarity::Common), 9);
    assert_eq!(b.spell_choices_of(lib, "life", Rarity::Uncommon), 0);
    assert_eq!(b.spell_choices_of(lib, "life", Rarity::Rare), 0);
    assert_eq!(b.picks_spent(), 10);
}

#[test]
fn eight_spells_are_on_offer_for_research_lowest_rarity_first() -> TestResult {
    use crate::wizard::magic::RESEARCH_SLOTS;
    let mut g = game();
    g.create_wizard(P0, &build(&[("life", 6), ("chaos", 5)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    let magic = g.wizard(P0).ok_or("wizard is missing")?;
    assert_eq!(magic.candidates().len(), RESEARCH_SLOTS);
    assert!(magic.candidates().iter().all(|s| magic.can_research(*s)));
    // Lowest rarity first: a realm's uncommon spell is offered only once all its researchable
    // commons are on the offer.
    for realm in ["life", "chaos"] {
        let of_realm = |s: &&crate::rules::SpellId| g.content.rules.spells.get(**s).realm == realm;
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
    let events = g.process_magic();
    assert!(events
        .iter()
        .any(|e| matches!(e, crate::game::Event::SpellLearned { spell, .. } if *spell == pick)));
    let after = g.wizard(P0).ok_or("wizard is missing")?;
    assert!(after.knows(pick) && !after.candidates().contains(&pick));
    assert_eq!(after.candidates().len(), RESEARCH_SLOTS);
    Ok(())
}

/// A ready-made wizard: no pick budget, so a test can have any books.
fn preset(books: &[(&str, u8)], retorts: &[Retort]) -> WizardBuild {
    WizardBuild {
        preset: true,
        ..build(books, retorts)
    }
}

fn spell_id(g: &Game, id: &str) -> crate::rules::SpellId {
    g.content.rules.spells.id_of(id).expect("id_of succeeds")
}

/// How many of the spells in `set` are of this realm and rarity.
fn of_kind(
    g: &Game,
    set: &std::collections::BTreeSet<crate::rules::SpellId>,
    realm: &str,
    rarity: Rarity,
) -> usize {
    set.iter()
        .filter(|&&s| {
            let d = g.content.rules.spells.get(s);
            d.realm == realm && d.rarity == rarity
        })
        .count()
}

#[test]
fn a_realm_starts_with_the_first_default_commons_known() -> TestResult {
    // The original: n books, n - 1 commons known, taken from the realm's default list.
    let mut g = game();
    g.create_wizard(P0, &preset(&[("nature", 3)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    for id in ["war_bears", "stone_skin"] {
        assert!(m.known().contains(&spell_id(&g, id)), "{id}");
    }
    assert!(
        !m.known().contains(&spell_id(&g, "sprites")),
        "the third is researchable"
    );
    assert_eq!(of_kind(&g, &m.known(), "nature", Rarity::Common), 2);
    Ok(())
}

#[test]
fn one_book_knows_no_spell_but_opens_three_commons_and_an_uncommon() -> TestResult {
    let mut g = game();
    g.create_wizard(P0, &preset(&[("chaos", 1)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    assert_eq!(of_kind(&g, &m.known(), "chaos", Rarity::Common), 0);
    assert_eq!(of_kind(&g, &m.researchable(), "chaos", Rarity::Common), 3);
    assert_eq!(of_kind(&g, &m.researchable(), "chaos", Rarity::Uncommon), 1);
    assert_eq!(of_kind(&g, &m.researchable(), "chaos", Rarity::Rare), 0);
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
        g.create_wizard(P0, &preset(&[("life", books)], &[]))
            .map_err(|e| format!("{e:?}"))?;
        let m = g.wizard(P0).ok_or("wizard is missing")?;
        let open = |r: Rarity| {
            of_kind(&g, &m.researchable(), "life", r) + of_kind(&g, &m.known(), "life", r)
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
    g.create_wizard(P0, &preset(&[("sorcery", 11)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    for id in ["flight", "phantom_beast", "storm_giant"] {
        assert!(m.known().contains(&spell_id(&g, id)), "{id}");
    }
    assert_eq!(of_kind(&g, &m.known(), "sorcery", Rarity::Common), 10);
    assert_eq!(
        of_kind(&g, &m.researchable(), "sorcery", Rarity::VeryRare),
        10
    );
    Ok(())
}

#[test]
fn every_wizard_knows_magic_spirit_and_the_spell_of_return() -> TestResult {
    let mut g = game();
    g.create_wizard(P0, &preset(&[("life", 2)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    assert!(m.known().contains(&spell_id(&g, "magic_spirit")));
    assert!(m.known().contains(&spell_id(&g, "spell_of_return")));
    assert!(
        !m.known().contains(&spell_id(&g, "enchant_item")),
        "not without the retort"
    );
    assert!(!m.researchable().contains(&spell_id(&g, "spell_of_mastery")));
    assert!(m.researchable().contains(&spell_id(&g, "enchant_item")));
    Ok(())
}

#[test]
fn an_artificer_also_knows_enchant_item_and_create_artifact() -> TestResult {
    let mut g = game();
    g.create_wizard(
        P0,
        &preset(&[("life", 2), ("chaos", 2)], &[Retort::Artificer]),
    )
    .map_err(|e| format!("{e:?}"))?;
    let m = g.wizard(P0).ok_or("wizard is missing")?;
    for id in ["enchant_item", "create_artifact"] {
        assert!(m.known().contains(&spell_id(&g, id)), "{id}");
        assert!(!m.researchable().contains(&spell_id(&g, id)), "{id}");
    }
    Ok(())
}

#[test]
fn a_computer_wizard_knows_a_spell_of_its_main_realm_and_the_human_does_not() -> TestResult {
    // Most books wins; on a tie the first realm in the order nature, sorcery, chaos, life, death.
    for (books, spell) in [
        (vec![("nature", 4), ("life", 2)], "sprites"),
        (vec![("sorcery", 3), ("chaos", 3)], "nagas"),
        (vec![("chaos", 1), ("life", 5)], "guardian_spirit"),
        (vec![("death", 2), ("life", 2)], "guardian_spirit"),
        (vec![("death", 3), ("life", 2)], "ghouls"),
    ] {
        let mut g = game();
        g.settings.ai.wizards.guaranteed_summoning_spell = true;
        g.create_wizard(P0, &preset(&books, &[]))
            .map_err(|e| format!("{e:?}"))?;
        g.give_main_realm_spell(P0);
        assert!(
            g.wizard(P0)
                .ok_or("wizard is missing")?
                .known()
                .contains(&spell_id(&g, spell)),
            "{spell}"
        );
    }
    let mut g = game();
    g.create_wizard(P0, &preset(&[("nature", 1)], &[]))
        .map_err(|e| format!("{e:?}"))?;
    g.set_human(Some(P0));
    g.give_main_realm_spell(P0);
    assert!(!g
        .wizard(P0)
        .ok_or("wizard is missing")?
        .known()
        .contains(&spell_id(&g, "sprites")));
    Ok(())
}

#[test]
fn from_hard_on_a_computer_wizard_may_research_an_uncommon_summoning_spell() -> TestResult {
    for (difficulty, open) in [(2, false), (3, true), (4, true)] {
        let mut g = game();
        g.settings.basic.difficulty = difficulty;
        g.create_wizard(P0, &preset(&[("nature", 1)], &[]))
            .map_err(|e| format!("{e:?}"))?;
        g.give_main_realm_spell(P0);
        let m = g.wizard(P0).ok_or("wizard is missing")?;
        let basilisk = spell_id(&g, "basilisk");
        assert_eq!(m.can_research(basilisk), open, "difficulty {difficulty}");
        assert!(!m.knows(basilisk));
    }
    Ok(())
}
