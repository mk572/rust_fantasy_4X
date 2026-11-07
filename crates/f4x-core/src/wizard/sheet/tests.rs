use super::*;
use crate::game::testutil::shipped;
use crate::game::Game;
use crate::player::PlayerId;
use crate::rules::RealmId;
use crate::wizard::magic::{base_casting_skill, MagicError, PowerShares};
use f4x_def::testdir::TestResult;

const P0: PlayerId = PlayerId(0);

fn game_with(books: &[(RealmId, u8)], retorts: &[Retort]) -> Game {
    let mut g = Game::new(5);
    g.content = shipped();
    g.add_player(P0, "W");
    g.init_wizard(P0, books, retorts, None)
        .expect("init_wizard succeeds");
    g
}

fn spell(g: &Game, id: u16) -> SpellId {
    g.content
        .rules
        .spells
        .id_numbered(id)
        .expect("id_of succeeds")
}

#[test]
fn a_new_wizard_has_skill_from_books_and_an_archmage_more() -> TestResult {
    let g = game_with(
        &[
            (RealmId::new(3 /* life */), 3),
            (RealmId::new(2 /* chaos */), 2),
        ],
        &[],
    );
    let w = g.wizard(P0).ok_or("wizard is missing")?;
    assert_eq!(w.total_books(), 5);
    // Twice the books: ten points of target, reached by the triangular steps.
    assert_eq!(w.casting_skill(&f4x_def::testdir::shipped().rules), 10);
    assert_eq!(w.skill_left(), 10);
    let g = game_with(
        &[(RealmId::new(3 /* life */), 3)],
        &[Retort::new(10) /* archmage */],
    );
    assert_eq!(
        g.wizard(P0)
            .ok_or("wizard is missing")?
            .casting_skill(&f4x_def::testdir::shipped().rules),
        2 * (3 + 5)
    );
    let g = game_with(&[(RealmId::new(3 /* life */), 15)], &[]);
    assert_eq!(
        g.wizard(P0)
            .ok_or("wizard is missing")?
            .books_in(crate::game::testutil::realm(3 /* life */)),
        11,
        "no more than eleven"
    );
    Ok(())
}

#[test]
fn the_library_depends_on_the_seed_and_the_books_alone() -> TestResult {
    let library = |seed| {
        let mut g = game_with(
            &[
                (RealmId::new(3 /* life */), 4),
                (RealmId::new(2 /* chaos */), 2),
            ],
            &[],
        );
        let mut rng = GameRng::new(seed);
        let content = g.content.clone();
        let mut w = g.wizard_mut(P0).expect("wizard_mut succeeds");
        w.init_library(&content, &mut rng, None);
        w.refill_research(&content, &mut rng, 1);
        let v = w.view();
        (v.known(), v.researchable(), v.candidates())
    };
    let (a, b) = (library(5), library(5));
    assert_eq!(a, b);
    assert!(a.2.len() <= RESEARCH_SLOTS);
    assert!(a.2.iter().all(|s| a.1.contains(s)));
    assert!(a.0.is_disjoint(&a.1));
    Ok(())
}

#[test]
fn the_cost_of_a_spell_follows_the_books_the_retorts_and_where_it_is_cast() -> TestResult {
    let g = game_with(&[(RealmId::new(0 /* nature */), 1)], &[]);
    let strength = g
        .content
        .rules
        .spells
        .get(spell(&g, 4 /* giant_strength */));
    let base = strength.cost as i32;
    let bare = g.wizard(P0).ok_or("wizard is missing")?;
    assert_eq!(
        bare.casting_cost(&g.content.rules, strength, true, false),
        base
    );
    assert_eq!(
        bare.casting_cost(&g.content.rules, strength, false, false),
        base * 5,
        "five times as much away from a battle"
    );
    // Nine books of its realm: two steps of ten percent; the mastery adds fifteen.
    let g = game_with(
        &[(RealmId::new(0 /* nature */), 9)],
        &[Retort::new(3) /* nature_mastery */],
    );
    let deep = g.wizard(P0).ok_or("wizard is missing")?;
    assert_eq!(deep.cost_reduction(&g.content.rules, strength), 20 + 15);
    let cost = deep.casting_cost(&g.content.rules, strength, true, false);
    assert_eq!(cost, base - base * 35 / 100);
    // Evil Omens: half as much again for life and nature.
    assert_eq!(
        deep.casting_cost(&g.content.rules, strength, true, true),
        cost + cost / 2
    );
    Ok(())
}

#[test]
fn the_power_is_split_by_the_percentages() -> TestResult {
    let mut g = game_with(&[(RealmId::new(3 /* life */), 3)], &[]);
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .set_split(40, 30, 30)
        .map_err(|e| format!("{e:?}"))?;
    let split = g.wizard(P0).ok_or("wizard is missing")?.split_power(
        &g.content.rules,
        f4x_def::half_int::HalfInt::from_int(100),
        6,
    );
    assert_eq!(
        (split.mana.floor(), split.skill, split.research),
        (40, 30, 30 + 6)
    );
    assert_eq!(
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .set_split(50, 50, 50),
        Err(MagicError::BadSplit)
    );
    // With research off its share goes to skill, with Mana Focusing mana grows a quarter.
    let mut w = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
    w.set_split(50, 0, 50).map_err(|e| format!("{e:?}"))?;
    w.get_mut::<Retorts>()
        .0
        .insert(Retort::new(11) /* mana_focusing */);
    let split = g.wizard(P0).ok_or("wizard is missing")?.split_power(
        &g.content.rules,
        f4x_def::half_int::HalfInt::from_int(10),
        0,
    );
    assert_eq!(
        (split.mana.floor(), split.skill, split.research),
        (5 + 1, 5, 0)
    );
    Ok(())
}

#[test]
fn the_spell_of_mastery_is_offered_last_and_costs_less_for_each_spell_known() -> TestResult {
    let mut g = game_with(&[(RealmId::new(3 /* life */), 1)], &[]);
    let content = g.content.clone();
    let mastery = spell(&g, 213 /* spell_of_mastery */);
    let mut rng = GameRng::new(3);
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .refill_research(&content, &mut rng, 5);
    assert!(
        !g.wizard(P0)
            .ok_or("wizard is missing")?
            .candidates()
            .contains(&mastery),
        "not while other spells remain"
    );
    let mut learned = 0;
    while g.wizard(P0).ok_or("wizard is missing")?.candidates() != [mastery] {
        let next = *g
            .wizard(P0)
            .ok_or("wizard is missing")?
            .candidates()
            .first()
            .expect("something on offer");
        let mut w = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
        w.learn(next);
        learned += 1;
        assert!(learned < 500, "the pool runs dry");
        w.refill_research(&content, &mut rng, 5);
    }
    let w = g.wizard(P0).ok_or("wizard is missing")?;
    let listed = content.rules.spells.get(mastery).research_cost as i32;
    let cost = w.research_cost(&content, mastery);
    // The starting spells do not count, only what was gained since.
    let half: i32 = w
        .known()
        .difference(&w.starting_spells())
        .map(|&s| content.rules.spells.get(s).research_cost as i32 / 2)
        .sum();
    assert!(half > 0, "spells were learned");
    assert!(cost < listed);
    assert_eq!(cost, (listed - half).max(0));
    assert_ne!(
        w.research_cost(&content, spell(&g, 125 /* healing */)),
        cost
    );
    Ok(())
}

#[test]
fn research_runs_from_choice_to_a_known_spell() -> TestResult {
    let mut g = game_with(
        &[(RealmId::new(3 /* life */), 3)],
        &[Retort::new(7) /* sage_master */],
    );
    let content = g.content.clone();
    let mut rng = GameRng::new(3);
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .refill_research(&content, &mut rng, 5);
    let pick = *g
        .wizard(P0)
        .ok_or("wizard is missing")?
        .candidates()
        .first()
        .expect("something on offer");
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .choose_research(&content, pick)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .choose_research(&content, pick),
        Err(MagicError::AlreadyResearching)
    );
    let cost = content.rules.spells.get(pick).research_cost as i32;
    assert_eq!(
        g.wizard(P0)
            .ok_or("wizard is missing")?
            .research_remaining(),
        cost
    );
    // A sage master's research goes a quarter further.
    let split = PowerShares {
        mana: f4x_def::half_int::HalfInt::from_int(0),
        skill: 0,
        research: 100,
    };
    assert!(
        g.wizard(P0)
            .ok_or("wizard is missing")?
            .research_income(&content, &split)
            >= 125
    );
    let mut turns = 0;
    loop {
        let income = g
            .wizard(P0)
            .ok_or("wizard is missing")?
            .research_income(&content, &split);
        let mut w = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
        if w.finish_research().is_some() {
            break;
        }
        w.take_power(&f4x_def::testdir::shipped().rules, &split, income);
        turns += 1;
        assert!(turns < 100);
    }
    let w = g.wizard(P0).ok_or("wizard is missing")?;
    assert!(w.knows(pick) && !w.can_research(pick) && !w.candidates().contains(&pick));
    assert_eq!(w.researching(), None);
    Ok(())
}

#[test]
fn casting_is_paid_over_turns_and_cancelled_without_a_refund() -> TestResult {
    let mut g = game_with(&[(RealmId::new(3 /* life */), 3)], &[]);
    let strength = spell(&g, 4 /* giant_strength */);
    assert_eq!(
        g.wizard(P0)
            .ok_or("wizard is missing")?
            .check_can_cast(strength),
        Err(MagicError::NotAvailable)
    );
    let mut w = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
    w.get_mut::<KnownSpells>().known.insert(strength);
    let v = w.view();
    assert_eq!(v.check_can_cast(strength), Ok(()));
    assert_eq!(v.instant_budget(100), v.skill_left());
    assert_eq!(v.instant_budget(2), 2, "the reserve limits it");
    g.begin_casting(P0, strength, 40);
    let mut w = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
    assert_eq!(
        w.view().check_can_cast(strength),
        Err(MagicError::AlreadyCasting)
    );
    let skill = w.view().casting_skill(&f4x_def::testdir::shipped().rules);
    // The skill left from last turn pays first; the nominal skill pays only if it finishes the
    // spell, which it cannot here. What is paid comes out of the reserve.
    let reserve = w.pay_casting(
        &f4x_def::testdir::shipped().rules,
        f4x_def::half_int::HalfInt::from_int(1000),
    );
    assert_eq!(
        w.view().casting().ok_or("casting is missing")?.remaining,
        40 - skill
    );
    assert_eq!(reserve, 1000 - skill);
    assert_eq!(
        w.view().skill_left(),
        skill,
        "the nominal skill is there for next turn"
    );
    assert_eq!(w.view().casting_paid(), None);
    // With one turn's skill to go the second part finishes it, borrowed from the next turn.
    g.scenario_casting(P0, strength, skill);
    let mut w = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
    w.get_mut::<Skill>().left = 0;
    let reserve = w.pay_casting(
        &f4x_def::testdir::shipped().rules,
        f4x_def::half_int::HalfInt::from_int(1000),
    );
    assert_eq!(w.view().casting_paid(), Some(strength));
    assert_eq!(reserve, 1000 - skill);
    assert_eq!(w.view().skill_left(), 0, "borrowed and paid back at once");
    w.cancel_casting();
    assert!(w.view().casting().is_none());
    Ok(())
}

#[test]
fn global_spells_can_be_turned_off_only_if_they_are_on() -> TestResult {
    let mut g = game_with(&[], &[]);
    let s = spell(&g, 4 /* giant_strength */);
    assert_eq!(g.turn_off(P0, s), Err(MagicError::NotAvailable));
    g.add_global(P0, s);
    assert_eq!(g.turn_off(P0, s), Ok(()));
    assert!(g.wizard(P0).ok_or("wizard is missing")?.global().is_empty());
    Ok(())
}

#[test]
fn power_reaches_the_reserve_the_research_and_the_skill() -> TestResult {
    let mut g = game_with(
        &[(RealmId::new(3 /* life */), 2)],
        &[Retort::new(10) /* archmage */],
    );
    let content = g.content.clone();
    let mut rng = GameRng::new(3);
    let target = spell(&g, 125 /* healing */);
    let mut w = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
    w.refill_research(&content, &mut rng, 1);
    *w.get_mut::<Research>() = Research {
        spell: Some(target),
        remaining: 10,
    };
    let points = w.view().skill_points();
    let split = PowerShares {
        mana: f4x_def::half_int::HalfInt::from_int(7),
        research: 4,
        skill: 4,
    };
    assert_eq!(
        w.take_power(&f4x_def::testdir::shipped().rules, &split, 4),
        7
    );
    assert_eq!(w.view().research_remaining(), 6);
    assert_eq!(
        w.view().skill_points() - points,
        6,
        "an archmage gets half as much again"
    );
    assert_eq!(base_casting_skill(0, 0), 0);
    Ok(())
}

#[test]
fn a_starting_spell_does_not_lower_the_cost_of_the_spell_of_mastery() -> TestResult {
    let g = game_with(&[(RealmId::new(3 /* life */), 4)], &[]);
    let mastery = spell(&g, 213 /* spell_of_mastery */);
    let w = g.wizard(P0).ok_or("wizard is missing")?;
    assert!(!w.starting_spells().is_empty());
    assert_eq!(w.starting_spells(), w.known());
    let listed = g.content.rules.spells.get(mastery).research_cost as i32;
    assert_eq!(w.research_cost(&g.content, mastery), listed);
    Ok(())
}

#[test]
fn eleven_books_of_one_realm_take_3000_off_the_spell_of_mastery() -> TestResult {
    let mastery_of = |books: u8| -> Result<i32, Box<dyn std::error::Error>> {
        let g = game_with(&[(RealmId::new(3 /* life */), books)], &[]);
        let mastery = spell(&g, 213 /* spell_of_mastery */);
        Ok(g.wizard(P0)
            .ok_or("wizard is missing")?
            .research_cost(&g.content, mastery))
    };
    assert_eq!(mastery_of(10)? - mastery_of(11)?, MASTERY_DISCOUNT);
    Ok(())
}

#[test]
fn a_found_book_opens_the_spells_its_realms_table_row_allows() -> TestResult {
    let mut g = game_with(&[(RealmId::new(3 /* life */), 2)], &[]);
    let content = g.content.clone();
    let life = RealmId::new(3);
    let mut rng = GameRng::new(11);
    let count = |g: &Game, rarity: Rarity| -> usize {
        let w = g.wizard(P0).expect("wizard succeeds");
        let (known, open) = (w.known(), w.researchable());
        spells_of(&content, life, rarity)
            .iter()
            .filter(|s| known.contains(s) || open.contains(s))
            .count()
    };
    for books in 3..=13u8 {
        let mut w = g.wizard_mut(P0).ok_or("wizard_mut is missing")?;
        w.get_mut::<Books>().0.insert(life, books);
        w.open_book_spells(&content, &mut rng, life);
        let row = SpellLibraryDef::row(&content.settings.spell_library.researchable, books);
        for (rarity, limit) in [
            (Rarity::Common, row.common),
            (Rarity::Uncommon, row.uncommon),
            (Rarity::Rare, row.rare),
            (Rarity::VeryRare, row.very_rare),
        ] {
            let all = spells_of(&content, life, rarity).len();
            assert_eq!(
                count(&g, rarity),
                usize::from(limit).min(all),
                "{books} books, {rarity:?}"
            );
        }
    }
    // Thirteen books: all of the realm.
    let all = [
        Rarity::Common,
        Rarity::Uncommon,
        Rarity::Rare,
        Rarity::VeryRare,
    ]
    .iter()
    .map(|&r| spells_of(&content, life, r).len())
    .sum::<usize>();
    let w = g.wizard(P0).ok_or("wizard is missing")?;
    let mine = w
        .known()
        .union(&w.researchable())
        .filter(|s| content.rules.spells.get(**s).realm == life)
        .count();
    assert_eq!(mine, all);
    Ok(())
}

#[test]
fn a_wizards_books_decide_which_spells_it_may_hold() -> TestResult {
    let g = game_with(
        &[
            (RealmId::new(3 /* life */), 1),
            (RealmId::new(0 /* nature */), 2),
        ],
        &[],
    );
    let w = g.wizard(P0).ok_or("wizard is missing")?;
    let spells = &g.content.rules.spells;
    let allowed = |realm: u16, rarity: Rarity| -> Vec<bool> {
        spells
            .iter()
            .filter(|(_, d)| d.realm.num() == realm && d.rarity == rarity)
            .map(|(_, d)| w.books_allow(d))
            .collect()
    };
    assert!(allowed(3, Rarity::Uncommon).iter().all(|&a| a));
    assert!(allowed(3, Rarity::Rare).iter().all(|&a| !a));
    assert!(allowed(0, Rarity::Rare).iter().all(|&a| a));
    assert!(allowed(0, Rarity::VeryRare).iter().all(|&a| !a));
    assert!(
        allowed(2, Rarity::Common).iter().all(|&a| !a),
        "no chaos book"
    );
    assert!(
        allowed(5, Rarity::Rare).iter().all(|&a| a),
        "arcane needs no book"
    );
    Ok(())
}

#[test]
fn the_arcane_spells_are_offered_in_the_wikis_three_rarities() {
    let c = shipped();
    let rarity_of = |n: u16| {
        c.rules
            .spells
            .get(c.rules.spells.id_numbered(n).expect("id succeeds"))
            .rarity
    };
    // Magic Spirit, Dispel Magic, Summoning Circle | Disenchant Area, Recall Hero, Detect Magic,
    // Enchant Item, Summon Hero | Awareness, Disjunction, Create Artifact, Summon Champion.
    for n in 201..=203 {
        assert_eq!(rarity_of(n), Rarity::Common, "{n}");
    }
    for n in 204..=208 {
        assert_eq!(rarity_of(n), Rarity::Uncommon, "{n}");
    }
    for n in 209..=212 {
        assert_eq!(rarity_of(n), Rarity::Rare, "{n}");
    }
}

#[test]
fn with_the_patch_setting_only_so_many_arcane_spells_are_on_offer_at_once() -> TestResult {
    let mut g = game_with(&[(RealmId::new(3 /* life */), 4)], &[]);
    let mut content = g.content.clone();
    content.settings.spell_library.arcane_candidate_cap = 4;
    let mut rng = GameRng::new(3);
    let arcane_on_offer = |g: &Game| {
        g.wizard(P0)
            .expect("wizard succeeds")
            .candidates()
            .iter()
            .filter(|&&s| g.content.rules.spells.get(s).realm == RealmId::new(5))
            .count()
    };
    // Without a cap the arcane spells fill the eight places that life leaves free.
    let plain = g.content.clone();
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .refill_research(&plain, &mut rng, 5);
    assert!(arcane_on_offer(&g) > 0);
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .get_mut::<Candidates>()
        .0
        .clear();
    g.wizard_mut(P0)
        .ok_or("wizard_mut is missing")?
        .refill_research(&content, &mut rng, 5);
    assert!(arcane_on_offer(&g) <= 4);
    Ok(())
}
