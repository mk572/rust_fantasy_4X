use super::*;
use crate::game::testutil::shipped;
use crate::player::Treasury;
use crate::rules::RealmId;
use crate::wizard::magic::Retort;
use f4x_def::testdir::TestResult;

fn treasury() -> Treasury {
    Treasury {
        gold: f4x_def::half_int::HalfInt::from_int(100),
        mana: 20,
    }
}

#[test]
fn alchemy_is_two_for_one_without_the_retort_and_one_for_one_with_it() -> TestResult {
    let mut t = treasury();
    alchemy(50, &mut t, 10, true).map_err(|e| format!("{e:?}"))?;
    assert_eq!((t.gold.floor(), t.mana), (90, 25));
    alchemy(50, &mut t, 10, false).map_err(|e| format!("{e:?}"))?;
    assert_eq!((t.gold.floor(), t.mana), (95, 15));
    alchemy(100, &mut t, 10, true).map_err(|e| format!("{e:?}"))?;
    assert_eq!((t.gold.floor(), t.mana), (85, 25));
    Ok(())
}

#[test]
fn alchemy_refuses_what_the_wizard_cannot_pay_and_changes_nothing() {
    let mut t = treasury();
    assert_eq!(
        alchemy(50, &mut t, 101, true),
        Err(MagicError::NotEnoughGold)
    );
    assert_eq!(
        alchemy(50, &mut t, 21, false),
        Err(MagicError::NotEnoughMana)
    );
    assert_eq!((t.gold.floor(), t.mana), (100, 20));
}

#[test]
fn the_spell_being_cast_can_be_given_up_except_the_spell_of_return() -> TestResult {
    let mut g = crate::game::Game::new(3);
    g.content = shipped();
    g.add_player(crate::player::PlayerId(0), "W");
    let mut w = g
        .wizard_mut(crate::player::PlayerId(0))
        .ok_or("wizard_mut is missing")?;
    assert_eq!(cancel_cast(&mut w, true), Err(MagicError::NotAvailable));
    assert_eq!(cancel_cast(&mut w, false), Ok(()));
    Ok(())
}

#[test]
fn a_famous_wizard_starts_with_fame_and_every_wizard_with_a_library() -> TestResult {
    let mut g = crate::game::Game::new(3);
    g.content = shipped();
    let id = crate::player::PlayerId(0);
    g.add_player(id, "W");
    g.init_wizard(
        id,
        &[(RealmId::new(3 /* life */), 2)],
        &[Retort::new(13) /* famous */],
        None,
    )
    .map_err(|e| format!("{e:?}"))?;
    let w = g.wizard(id).ok_or("wizard is missing")?;
    assert_eq!(w.fame(), 10);
    assert_eq!(w.books_in(crate::game::testutil::realm(3 /* life */)), 2);
    assert!(!w.known().is_empty());
    g.init_wizard(id, &[(RealmId::new(3 /* life */), 2)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.wizard(id).ok_or("wizard is missing")?.fame(), 0);
    Ok(())
}
