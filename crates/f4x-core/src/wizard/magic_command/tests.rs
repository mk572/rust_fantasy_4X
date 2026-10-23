use super::*;
use crate::game::testutil::content_with_magic;
use crate::player::Treasury;
use crate::wizard::magic::Retort;

fn treasury() -> Treasury {
    Treasury {
        gold: 100,
        mana: 20,
    }
}

#[test]
fn alchemy_is_two_for_one_without_the_retort_and_one_for_one_with_it() {
    let mut t = treasury();
    alchemy(false, &mut t, 10, true).unwrap();
    assert_eq!((t.gold, t.mana), (90, 25));
    alchemy(false, &mut t, 10, false).unwrap();
    assert_eq!((t.gold, t.mana), (95, 15));
    alchemy(true, &mut t, 10, true).unwrap();
    assert_eq!((t.gold, t.mana), (85, 25));
}

#[test]
fn alchemy_refuses_what_the_wizard_cannot_pay_and_changes_nothing() {
    let mut t = treasury();
    assert_eq!(
        alchemy(false, &mut t, 101, true),
        Err(MagicError::NotEnoughGold)
    );
    assert_eq!(
        alchemy(false, &mut t, 21, false),
        Err(MagicError::NotEnoughMana)
    );
    assert_eq!((t.gold, t.mana), (100, 20));
}

#[test]
fn the_spell_being_cast_can_be_given_up_except_the_spell_of_return() {
    let mut g = crate::game::Game::new(3);
    g.content = content_with_magic();
    g.add_player(crate::player::PlayerId(0), "W");
    let mut w = g.wizard_mut(crate::player::PlayerId(0)).unwrap();
    assert_eq!(cancel_cast(&mut w, true), Err(MagicError::NotAvailable));
    assert_eq!(cancel_cast(&mut w, false), Ok(()));
}

#[test]
fn a_famous_wizard_starts_with_fame_and_every_wizard_with_a_library() {
    let mut g = crate::game::Game::new(3);
    g.content = content_with_magic();
    let id = crate::player::PlayerId(0);
    g.add_player(id, "W");
    g.init_wizard(id, &[("life", 2)], &[Retort::Famous], None)
        .unwrap();
    let w = g.wizard(id).unwrap();
    assert_eq!(w.fame(), 10);
    assert_eq!(w.books_in("life"), 2);
    assert!(!w.known().is_empty());
    g.init_wizard(id, &[("life", 2)], &[], None).unwrap();
    assert_eq!(g.wizard(id).unwrap().fame(), 0);
}
