use super::*;
use crate::testworld::{game, COAST_CITY, P0, P1};
use f4x_core::diplomacy::DiplomacyCommand;
use f4x_core::game::Command;
use f4x_core::rules::{RealmId, SpellId};
use f4x_def::half_int::HalfInt;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const LIFE: u16 = 3;

/// Two wizards with five books of life that have met. P0 knows the dearest life spell that a
/// treasury of 1000 gold can buy and P1 no
/// life spell at all; P1 has 1000 gold.
fn world() -> Result<(Game, SpellId), Box<dyn std::error::Error>> {
    let mut g = game(COAST_CITY, 8 /* high_men */);
    let life = RealmId::new(LIFE);
    let dear = g
        .content()
        .rules
        .spells
        .iter()
        .filter(|(_, d)| d.realm == life && d.research_cost <= 600)
        .max_by_key(|(_, d)| (d.research_cost, std::cmp::Reverse(d.num)))
        .map(|(id, _)| id)
        .ok_or("a life spell")?;
    g.init_wizard(P0, &[(life, 5)], &[], Some(&[]))
        .map_err(|e| format!("{e:?}"))?;
    g.wizard_mut(P0)
        .ok_or("a wizard")?
        .get_mut::<f4x_core::wizard::sheet::KnownSpells>()
        .known
        .insert(dear);
    g.init_wizard(P1, &[(life, 5)], &[], Some(&[]))
        .map_err(|e| format!("{e:?}"))?;
    g.make_contact(P0, P1);
    g.treasury_mut(P1).ok_or("treasury")?.gold = HalfInt::from_int(1000);
    g.treasury_mut(P0).ok_or("treasury")?.gold = HalfInt::from_int(1000);
    Ok((g, dear))
}

/// Make the two allies (the proposal and its answer, as a human and a rival would).
fn ally(g: &mut Game) {
    g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
        from: P0,
        to: P1,
        kind: ProposalKind::Alliance,
    }));
    g.apply(Command::Diplomacy(DiplomacyCommand::Answer {
        player: P1,
        from: P0,
        accept: true,
    }));
    assert_eq!(g.treaty(P0, P1), Treaty::Alliance);
}

#[test]
fn an_ally_takes_a_trade_a_little_short_of_fair_and_a_stranger_does_not() -> TestResult {
    let (mut g, dear) = world()?;
    let worth = f4x_core::diplomacy::trade::worth(&g, P1, Goods::spell(dear));
    // P1 is asked to give the spell for 92 percent of its worth in gold, a little short of fair:
    // a stranger says no and an ally yes.
    let short = Goods::gold(worth * 92 / 100);
    assert_eq!(
        verdict(&g, P1, P0, short, Goods::spell(dear)),
        Verdict::Refused
    );
    ally(&mut g);
    assert!(margin(&g, P1, P0) <= 92);
    assert_eq!(
        verdict(&g, P1, P0, short, Goods::spell(dear)),
        Verdict::Accepted
    );
    Ok(())
}

#[test]
fn the_margin_eases_with_a_treaty_and_with_goodwill() -> TestResult {
    let (mut g, _) = world()?;
    let stranger = margin(&g, P1, P0);
    g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
        from: P0,
        to: P1,
        kind: ProposalKind::Pact,
    }));
    g.apply(Command::Diplomacy(DiplomacyCommand::Answer {
        player: P1,
        from: P0,
        accept: true,
    }));
    let pact = margin(&g, P1, P0);
    assert!(pact < stranger, "{pact} < {stranger}");
    ally(&mut g);
    assert!(margin(&g, P1, P0) < pact);
    Ok(())
}

#[test]
fn a_spell_is_sold_for_gold_to_a_wizard_that_has_the_books_and_not_to_one_that_has_not(
) -> TestResult {
    let (mut g, dear) = world()?;
    ally(&mut g);
    let Some(ProposalKind::Trade { give, get }) = offer(&g, P0, P1) else {
        return Err("an allied wizard with a spell to sell makes an offer".into());
    };
    assert_eq!(give.spell, Some(dear));
    assert!(get.gold > 0 && get.spell.is_none(), "{get:?}");
    // The price is one the buyer takes.
    assert_eq!(verdict(&g, P1, P0, give, get), Verdict::Accepted);
    // No books of the realm: nothing to sell it.
    g.wizard_mut(P1)
        .ok_or("a wizard")?
        .get_mut::<f4x_core::wizard::sheet::Books>()
        .0
        .clear();
    assert_eq!(offer(&g, P0, P1), None);
    Ok(())
}

#[test]
fn a_spell_is_bought_when_the_price_is_one_the_seller_takes() -> TestResult {
    let (mut g, dear) = world()?;
    ally(&mut g);
    // The other way round: P1 is the one with the spell.
    g.wizard_mut(P0)
        .ok_or("a wizard")?
        .get_mut::<f4x_core::wizard::sheet::KnownSpells>()
        .known
        .remove(&dear);
    g.wizard_mut(P1)
        .ok_or("a wizard")?
        .get_mut::<f4x_core::wizard::sheet::KnownSpells>()
        .known
        .insert(dear);
    let Some(ProposalKind::Trade { give, get }) = offer(&g, P0, P1) else {
        return Err("a wizard with gold to spare buys a spell it lacks".into());
    };
    assert_eq!(get.spell, Some(dear));
    assert!(give.gold > 0);
    assert_eq!(verdict(&g, P1, P0, give, get), Verdict::Accepted);
    // Without the gold to spare there is no purchase.
    g.treasury_mut(P0).ok_or("treasury")?.gold = HalfInt::from_int(10);
    assert_eq!(offer(&g, P0, P1), None);
    Ok(())
}

#[test]
fn no_trade_with_an_enemy_and_nothing_to_give_an_enemys_friend() -> TestResult {
    let (mut g, _) = world()?;
    g.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
        player: P0,
        on: P1,
    }));
    assert_eq!(offer(&g, P0, P1), None);
    assert_eq!(spell_to_give(&g, P0, P1), None);
    Ok(())
}
