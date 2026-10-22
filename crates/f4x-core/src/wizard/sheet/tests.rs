use super::*;
use crate::game::testutil::content_with_magic;
use crate::game::Game;
use crate::player::PlayerId;
use crate::wizard::magic::{base_casting_skill, MagicError, PowerShares};

const P0: PlayerId = PlayerId(0);

fn game_with(books: &[(&str, u8)], retorts: &[Retort]) -> Game {
    let mut g = Game::new(5);
    g.content = content_with_magic();
    g.add_player(P0, "W");
    g.init_wizard(P0, books, retorts, None).unwrap();
    g
}

fn spell(g: &Game, id: &str) -> SpellId {
    g.content.rules.spells.id_of(id).unwrap()
}

#[test]
fn a_new_wizard_has_skill_from_books_and_an_archmage_more() {
    let g = game_with(&[("life", 3), ("chaos", 2)], &[]);
    let w = g.wizard(P0).unwrap();
    assert_eq!(w.total_books(), 5);
    // Twice the books: ten points of target, reached by the triangular steps.
    assert_eq!(w.casting_skill(), 10);
    assert_eq!(w.skill_left(), 10);
    let g = game_with(&[("life", 3)], &[Retort::Archmage]);
    assert_eq!(g.wizard(P0).unwrap().casting_skill(), 2 * (3 + 5));
    let g = game_with(&[("life", 15)], &[]);
    assert_eq!(
        g.wizard(P0).unwrap().books_in("life"),
        11,
        "no more than eleven"
    );
}

#[test]
fn the_library_depends_on_the_seed_and_the_books_alone() {
    let library = |seed| {
        let mut g = game_with(&[("life", 4), ("chaos", 2)], &[]);
        let mut rng = GameRng::new(seed);
        let content = g.content.clone();
        let mut w = g.wizard_mut(P0).unwrap();
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
}

#[test]
fn the_cost_of_a_spell_follows_the_books_the_retorts_and_where_it_is_cast() {
    let g = game_with(&[("nature", 1)], &[]);
    let strength = g.content.rules.spells.get(spell(&g, "giant_strength"));
    let base = strength.cost as i32;
    let bare = g.wizard(P0).unwrap();
    assert_eq!(bare.casting_cost(strength, true, false), base);
    assert_eq!(
        bare.casting_cost(strength, false, false),
        base * 5,
        "five times as much away from a battle"
    );
    // Nine books of its realm: two steps of ten percent; the mastery adds fifteen.
    let g = game_with(&[("nature", 9)], &[Retort::NatureMastery]);
    let deep = g.wizard(P0).unwrap();
    assert_eq!(deep.cost_reduction(strength), 20 + 15);
    let cost = deep.casting_cost(strength, true, false);
    assert_eq!(cost, base - base * 35 / 100);
    // Evil Omens: half as much again for life and nature.
    assert_eq!(deep.casting_cost(strength, true, true), cost + cost / 2);
}

#[test]
fn the_power_is_split_by_the_percentages() {
    let mut g = game_with(&[("life", 3)], &[]);
    g.wizard_mut(P0).unwrap().set_split(40, 30, 30).unwrap();
    let split = g.wizard(P0).unwrap().split_power(100, 6);
    assert_eq!((split.mana, split.skill, split.research), (40, 30, 30 + 6));
    assert_eq!(
        g.wizard_mut(P0).unwrap().set_split(50, 50, 50),
        Err(MagicError::BadSplit)
    );
    // With research off its share goes to skill, with Mana Focusing mana grows a quarter.
    let mut w = g.wizard_mut(P0).unwrap();
    w.set_split(50, 0, 50).unwrap();
    w.get_mut::<Retorts>().0.insert(Retort::ManaFocusing);
    let split = g.wizard(P0).unwrap().split_power(10, 0);
    assert_eq!((split.mana, split.skill, split.research), (5 + 1, 5, 0));
}

#[test]
fn the_spell_of_mastery_is_offered_last_and_costs_less_for_each_spell_known() {
    let mut g = game_with(&[("life", 1)], &[]);
    let content = g.content.clone();
    let mastery = spell(&g, "spell_of_mastery");
    let mut rng = GameRng::new(3);
    g.wizard_mut(P0)
        .unwrap()
        .refill_research(&content, &mut rng, 5);
    assert!(
        !g.wizard(P0).unwrap().candidates().contains(&mastery),
        "not while other spells remain"
    );
    let mut learned = 0;
    while g.wizard(P0).unwrap().candidates() != [mastery] {
        let next = *g
            .wizard(P0)
            .unwrap()
            .candidates()
            .first()
            .expect("something on offer");
        let mut w = g.wizard_mut(P0).unwrap();
        w.learn(next);
        learned += 1;
        assert!(learned < 500, "the pool runs dry");
        w.refill_research(&content, &mut rng, 5);
    }
    let w = g.wizard(P0).unwrap();
    let listed = content.rules.spells.get(mastery).research_cost as i32;
    let cost = w.research_cost(&content, mastery);
    let half: i32 = w
        .known()
        .iter()
        .map(|&s| content.rules.spells.get(s).research_cost as i32 / 2)
        .sum();
    assert!(cost < listed);
    assert_eq!(cost, (listed - half).max(0));
    assert_ne!(w.research_cost(&content, spell(&g, "healing")), cost);
}

#[test]
fn research_runs_from_choice_to_a_known_spell() {
    let mut g = game_with(&[("life", 3)], &[Retort::SageMaster]);
    let content = g.content.clone();
    let mut rng = GameRng::new(3);
    g.wizard_mut(P0)
        .unwrap()
        .refill_research(&content, &mut rng, 5);
    let pick = *g
        .wizard(P0)
        .unwrap()
        .candidates()
        .first()
        .expect("something on offer");
    g.wizard_mut(P0)
        .unwrap()
        .choose_research(&content, pick)
        .unwrap();
    assert_eq!(
        g.wizard_mut(P0).unwrap().choose_research(&content, pick),
        Err(MagicError::AlreadyResearching)
    );
    let cost = content.rules.spells.get(pick).research_cost as i32;
    assert_eq!(g.wizard(P0).unwrap().research_remaining(), cost);
    // A sage master's research goes a quarter further.
    let split = PowerShares {
        mana: 0,
        skill: 0,
        research: 100,
    };
    assert!(g.wizard(P0).unwrap().research_income(&content, &split) >= 125);
    let mut turns = 0;
    loop {
        let income = g.wizard(P0).unwrap().research_income(&content, &split);
        let mut w = g.wizard_mut(P0).unwrap();
        if w.finish_research().is_some() {
            break;
        }
        w.take_power(&split, income);
        turns += 1;
        assert!(turns < 100);
    }
    let w = g.wizard(P0).unwrap();
    assert!(w.knows(pick) && !w.can_research(pick) && !w.candidates().contains(&pick));
    assert_eq!(w.researching(), None);
}

#[test]
fn casting_is_paid_over_turns_and_cancelled_without_a_refund() {
    let mut g = game_with(&[("life", 3)], &[]);
    let strength = spell(&g, "giant_strength");
    assert_eq!(
        g.wizard(P0).unwrap().check_can_cast(strength),
        Err(MagicError::NotAvailable)
    );
    let mut w = g.wizard_mut(P0).unwrap();
    w.get_mut::<KnownSpells>().known.insert(strength);
    let v = w.view();
    assert_eq!(v.check_can_cast(strength), Ok(()));
    assert_eq!(v.instant_budget(100), v.skill_left());
    assert_eq!(v.instant_budget(2), 2, "the reserve limits it");
    g.begin_casting(P0, strength, 40);
    let mut w = g.wizard_mut(P0).unwrap();
    assert_eq!(
        w.view().check_can_cast(strength),
        Err(MagicError::AlreadyCasting)
    );
    let skill = w.view().casting_skill();
    // The skill left from last turn pays first; the nominal skill pays only if it finishes the
    // spell, which it cannot here. What is paid comes out of the reserve.
    let reserve = w.pay_casting(1000);
    assert_eq!(w.view().casting().unwrap().remaining, 40 - skill);
    assert_eq!(reserve, 1000 - skill);
    assert_eq!(
        w.view().skill_left(),
        skill,
        "the nominal skill is there for next turn"
    );
    assert_eq!(w.view().casting_paid(), None);
    // With one turn's skill to go the second part finishes it, borrowed from the next turn.
    g.scenario_casting(P0, strength, skill);
    let mut w = g.wizard_mut(P0).unwrap();
    w.get_mut::<Skill>().left = 0;
    let reserve = w.pay_casting(1000);
    assert_eq!(w.view().casting_paid(), Some(strength));
    assert_eq!(reserve, 1000 - skill);
    assert_eq!(w.view().skill_left(), 0, "borrowed and paid back at once");
    w.cancel_casting();
    assert!(w.view().casting().is_none());
}

#[test]
fn global_spells_can_be_turned_off_only_if_they_are_on() {
    let mut g = game_with(&[], &[]);
    let s = spell(&g, "giant_strength");
    assert_eq!(g.turn_off(P0, s), Err(MagicError::NotAvailable));
    g.add_global(P0, s);
    assert_eq!(g.turn_off(P0, s), Ok(()));
    assert!(g.wizard(P0).unwrap().global().is_empty());
}

#[test]
fn power_reaches_the_reserve_the_research_and_the_skill() {
    let mut g = game_with(&[("life", 2)], &[Retort::Archmage]);
    let content = g.content.clone();
    let mut rng = GameRng::new(3);
    let target = spell(&g, "healing");
    let mut w = g.wizard_mut(P0).unwrap();
    w.refill_research(&content, &mut rng, 1);
    *w.get_mut::<Research>() = Research {
        spell: Some(target),
        remaining: 10,
    };
    let points = w.view().skill_points();
    let split = PowerShares {
        mana: 7,
        research: 4,
        skill: 4,
    };
    assert_eq!(w.take_power(&split, 4), 7);
    assert_eq!(w.view().research_remaining(), 6);
    assert_eq!(
        w.view().skill_points() - points,
        6,
        "an archmage gets half as much again"
    );
    assert_eq!(base_casting_skill(0, false), 0);
}
