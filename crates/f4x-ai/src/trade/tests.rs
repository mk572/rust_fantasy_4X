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

fn proposal(from: PlayerId, to: PlayerId, give: Goods, get: Goods, round: u8) -> Proposal {
    Proposal {
        from,
        to,
        kind: ProposalKind::Trade { give, get },
        round,
    }
}

#[test]
fn an_open_ask_is_answered_with_a_price_the_asker_would_take() -> TestResult {
    let (g, dear) = world()?;
    // P1 wants the spell and names no price: P0 names one.
    let ask = proposal(P1, P0, Goods::NONE, Goods::spell(dear), 0);
    let Reply::Counter(ProposalKind::Trade { give, get }) = respond(&g, P0, ask) else {
        return Err("a price is named".into());
    };
    assert_eq!(give, Goods::spell(dear));
    let worth = f4x_core::diplomacy::trade::worth(&g, P0, give);
    assert!(
        get.gold * 100 >= worth * margin(&g, P0, P1),
        "{get:?} for a spell worth {worth}"
    );
    assert_eq!(verdict(&g, P1, P0, give, get), Verdict::Accepted);
    Ok(())
}

#[test]
fn a_short_offer_is_countered_with_more_and_the_last_round_only_accepts_or_refuses() -> TestResult {
    let (g, dear) = world()?;
    let worth = f4x_core::diplomacy::trade::worth(&g, P0, Goods::spell(dear));
    let short = proposal(P1, P0, Goods::gold(worth * 80 / 100), Goods::spell(dear), 0);
    let Reply::Counter(ProposalKind::Trade { give, get }) = respond(&g, P0, short) else {
        return Err("a counter".into());
    };
    assert_eq!(give, Goods::spell(dear));
    assert!(get.gold > worth * 80 / 100, "{get:?}");
    // The same offer on the last round is turned down, and a fair one is taken.
    let last = proposal(
        P1,
        P0,
        Goods::gold(worth * 80 / 100),
        Goods::spell(dear),
        MAX_ROUNDS - 1,
    );
    assert_eq!(respond(&g, P0, last), Reply::Reject);
    let fair = proposal(
        P1,
        P0,
        Goods::gold(worth * 110 / 100),
        Goods::spell(dear),
        MAX_ROUNDS - 1,
    );
    assert_eq!(respond(&g, P0, fair), Reply::Accept);
    Ok(())
}

#[test]
fn an_offer_with_nothing_asked_is_answered_with_what_the_ai_would_give() -> TestResult {
    let (g, dear) = world()?;
    // P1 offers 720 gold and asks nothing: P0 names the spell (the one thing it can hand over).
    let offer = proposal(P1, P0, Goods::gold(720), Goods::NONE, 0);
    let Reply::Counter(ProposalKind::Trade { give, get }) = respond(&g, P0, offer) else {
        return Err("an offer is named".into());
    };
    assert_eq!((give.spell, get.gold), (Some(dear), 720));
    Ok(())
}
