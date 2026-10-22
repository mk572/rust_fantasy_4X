use super::*;
use crate::game::testutil::content_with_magic;
use crate::player::Treasury;

fn player() -> (Player, Treasury) {
    (
        Player::new(),
        Treasury {
            gold: 100,
            mana: 20,
        },
    )
}

#[test]
fn alchemy_is_two_for_one_without_the_retort_and_one_for_one_with_it() {
    let (mut p, mut t) = player();
    alchemy(&p, &mut t, 10, true).unwrap();
    assert_eq!((t.gold, t.mana), (90, 25));
    alchemy(&p, &mut t, 10, false).unwrap();
    assert_eq!((t.gold, t.mana), (95, 15));
    p.magic.retorts.insert(Retort::Alchemy);
    alchemy(&p, &mut t, 10, true).unwrap();
    assert_eq!((t.gold, t.mana), (85, 25));
}

#[test]
fn alchemy_refuses_what_the_wizard_cannot_pay_and_changes_nothing() {
    let (p, mut t) = player();
    assert_eq!(
        alchemy(&p, &mut t, 101, true),
        Err(MagicError::NotEnoughGold)
    );
    assert_eq!(
        alchemy(&p, &mut t, 21, false),
        Err(MagicError::NotEnoughMana)
    );
    assert_eq!((t.gold, t.mana), (100, 20));
}

#[test]
fn the_spell_being_cast_can_be_given_up_except_the_spell_of_return() {
    let (mut p, _) = player();
    assert_eq!(cancel_cast(&mut p, true), Err(MagicError::NotAvailable));
    assert_eq!(cancel_cast(&mut p, false), Ok(()));
}

#[test]
fn a_famous_wizard_starts_with_fame_and_every_wizard_with_a_library() {
    let content = content_with_magic();
    let mut rng = GameRng::new(3);
    let (mut p, _) = player();
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
