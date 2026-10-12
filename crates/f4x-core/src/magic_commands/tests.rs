use super::*;
use crate::city::PlayerId;
use crate::testutil::content_with_magic;

fn player() -> Player {
    let mut p = Player::new(PlayerId(0), "Merlin");
    p.gold = 100;
    p.mana = 20;
    p
}

#[test]
fn alchemy_is_two_for_one_without_the_retort_and_one_for_one_with_it() {
    let mut p = player();
    alchemy(&mut p, 10, true).unwrap();
    assert_eq!((p.gold, p.mana), (90, 25));
    alchemy(&mut p, 10, false).unwrap();
    assert_eq!((p.gold, p.mana), (95, 15));
    p.magic.retorts.insert(Retort::Alchemy);
    alchemy(&mut p, 10, true).unwrap();
    assert_eq!((p.gold, p.mana), (85, 25));
}

#[test]
fn alchemy_refuses_what_the_wizard_cannot_pay_and_changes_nothing() {
    let mut p = player();
    assert_eq!(alchemy(&mut p, 101, true), Err(MagicError::NotEnoughGold));
    assert_eq!(alchemy(&mut p, 21, false), Err(MagicError::NotEnoughMana));
    assert_eq!((p.gold, p.mana), (100, 20));
}

#[test]
fn the_spell_being_cast_can_be_given_up_except_the_spell_of_return() {
    let mut p = player();
    assert_eq!(cancel_cast(&mut p, true), Err(MagicError::NotAvailable));
    assert_eq!(cancel_cast(&mut p, false), Ok(()));
}

#[test]
fn a_famous_wizard_starts_with_fame_and_every_wizard_with_a_library() {
    let content = content_with_magic();
    let mut rng = GameRng::new(3);
    let mut p = player();
    found_wizard(
        &mut p,
        &content,
        &mut rng,
        &[("life", 2)],
        &[Retort::Famous],
        None,
    );
    assert_eq!(p.fame, 10);
    assert_eq!(p.magic.books_in("life"), 2);
    assert!(!p.magic.known.is_empty());
    found_wizard(&mut p, &content, &mut rng, &[("life", 2)], &[], None);
    assert_eq!(p.fame, 0);
}
