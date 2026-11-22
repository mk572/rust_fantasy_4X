//! Class 9, the player: the treasury (alchemy, taxes) and the offers a wizard is made.

use crate::support::{content, text, worlds};
use f4x_core::game::save::Snapshot;
use f4x_core::game::{Command, Event, Game, Rejection};
use f4x_core::player::offer::{Offer, OfferError};
use f4x_core::player::PlayerId;
use f4x_core::wizard::magic::{MagicError, Retort};
use f4x_def::half_int::HalfInt;

fn wizards(g: &Game) -> Vec<PlayerId> {
    g.player_ids()
        .into_iter()
        .filter(|&p| !g.is_neutral_player(p) && g.wizard(p).is_some())
        .collect()
}

fn treasury(g: &Game, p: PlayerId) -> (HalfInt, HalfInt) {
    let t = g.player(p).expect("a player").treasury();
    (t.gold, t.mana)
}

fn refusal(events: &[Event]) -> Option<Rejection> {
    events.iter().find_map(|e| match e {
        Event::Rejected(r) => Some(*r),
        _ => None,
    })
}

#[test]
fn alchemy_turns_gold_into_mana_and_back_at_the_rate_of_the_wizard_and_refuses_what_is_not_there() {
    let alchemy = Retort::new(0 /* alchemy */);
    for (name, mut g) in worlds().into_iter().step_by(2) {
        for p in wizards(&g) {
            // Half for any wizard, all of it for one with the Alchemy retort (`retorts.ron`).
            let has = g.wizard(p).is_some_and(|w| w.retorts().contains(&alchemy));
            let percent = if has { 100 } else { 50 };
            let t = g.treasury_mut(p).expect("a treasury");
            t.gold = HalfInt::from_int(1000);
            t.mana = HalfInt::from_int(1000);
            // Gold to mana.
            let events = g.apply(Command::Alchemy {
                player: p,
                amount: 100,
                gold_to_mana: true,
            });
            assert_eq!(refusal(&events), None, "{name}");
            assert_eq!(
                treasury(&g, p),
                (
                    HalfInt::from_int(900),
                    HalfInt::from_int(1000 + 100 * percent / 100)
                ),
                "{name}: {p:?}"
            );
            // And back.
            g.apply(Command::Alchemy {
                player: p,
                amount: 100,
                gold_to_mana: false,
            });
            let (gold, mana) = treasury(&g, p);
            assert_eq!(
                mana,
                HalfInt::from_int(1000 + 100 * percent / 100 - 100),
                "{name}"
            );
            assert_eq!(gold, HalfInt::from_int(900 + 100 * percent / 100), "{name}");
            // More than there is: refused, nothing changes. A negative amount changes nothing either.
            let before = treasury(&g, p);
            let (gold, mana) = before;
            for (amount, to_mana, why) in [
                (gold.floor() + 1, true, MagicError::NotEnoughGold),
                (mana.floor() + 1, false, MagicError::NotEnoughMana),
            ] {
                let events = g.apply(Command::Alchemy {
                    player: p,
                    amount,
                    gold_to_mana: to_mana,
                });
                assert_eq!(refusal(&events), Some(Rejection::Magic(why)), "{name}");
                assert_eq!(treasury(&g, p), before);
            }
            g.apply(Command::Alchemy {
                player: p,
                amount: -50,
                gold_to_mana: true,
            });
            assert_eq!(treasury(&g, p), before, "{name}: a negative amount");
        }
    }
}

#[test]
fn a_higher_tax_brings_more_gold_until_the_rebels_stop_paying_and_the_rate_is_held_to_the_table() {
    for (name, mut g) in worlds().into_iter().step_by(2) {
        let p = wizards(&g)[0];
        let top = (g.content().rules.config.tax_unrest_percent.len() - 1) as u8;
        let mut last: Option<(HalfInt, u32)> = None;
        for rate in 0..=top {
            assert_eq!(
                refusal(&g.apply(Command::SetTaxRate { player: p, rate })),
                None
            );
            let (mut gold, mut rebels) = (HalfInt::ZERO, 0);
            for (city, _) in g.economy().cities_of(p) {
                let eco = g.economy().city_economy(city).expect("an economy");
                gold += eco.gold;
                rebels += eco.rebels;
            }
            if let Some((g0, r0)) = last {
                assert!(
                    gold >= g0,
                    "{name}: rate {rate} brings less gold than {}",
                    rate - 1
                );
                assert!(rebels >= r0, "{name}: rate {rate} calms the cities");
            }
            last = Some((gold, rebels));
        }
        // A rate over the table is held to its top.
        g.apply(Command::SetTaxRate {
            player: p,
            rate: u8::MAX,
        });
        let held = g.player_component::<f4x_core::player::Tax>(p).map(|t| t.0);
        assert_eq!(held, Some(top), "{name}");
        let (_, rebels_top) = last.expect("a rate");
        let _ = rebels_top;
    }
}

/// A game in which `p` has been made the offers.
fn with_offers(g: &Game, p: PlayerId, offers: Vec<Offer>) -> Game {
    let mut s: Snapshot = Snapshot::from_text(&text(g)).expect("reads");
    let slot = s
        .players
        .iter_mut()
        .find(|x| x.id == p.0)
        .expect("the player");
    slot.offers = offers;
    s.restore(content()).expect("restores")
}

#[test]
fn mercenaries_offered_are_hired_for_their_price_or_turned_down_for_nothing() {
    let (_, g) = worlds().remove(0);
    let p = wizards(&g)[0];
    let unit = g
        .content()
        .rules
        .units
        .defs()
        .iter()
        .find(|d| d.hireable && d.kind == f4x_def::rules::UnitKind::Normal && !d.is_settler())
        .map(|d| f4x_core::rules::UnitId::new(d.num))
        .expect("a unit for hire");
    let offer = Offer::Mercenaries {
        unit,
        count: 2,
        level: 1,
        price: 90,
    };
    let mut g = with_offers(&g, p, vec![offer.clone()]);
    g.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(100);
    let units_before = g.units().player_units(p).len();
    // With too little gold the offer stands, the game as it was.
    g.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(89);
    let was = text(&g);
    assert_eq!(
        refusal(&g.apply(Command::AcceptOffer {
            player: p,
            index: 0,
            name: None
        })),
        Some(Rejection::Offer(OfferError::NoGold))
    );
    assert_eq!(text(&g), was);
    // With enough, two units of the kind join and the price is paid.
    g.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(100);
    let events = g.apply(Command::AcceptOffer {
        player: p,
        index: 0,
        name: None,
    });
    let hired = events
        .iter()
        .find_map(|e| match e {
            Event::MercenariesHired { units, .. } => Some(units.clone()),
            _ => None,
        })
        .expect("MercenariesHired");
    assert_eq!(hired.len(), 2);
    assert_eq!(g.units().player_units(p).len(), units_before + 2);
    for u in hired {
        assert_eq!(g.units().unit_owner(u), Some(p));
        assert_eq!(g.units().unit_def(u).map(|d| d.num), Some(unit.num()));
    }
    assert_eq!(treasury(&g, p).0, HalfInt::from_int(10));
    // The offer is answered: asking again finds nothing.
    assert_eq!(
        refusal(&g.apply(Command::AcceptOffer {
            player: p,
            index: 0,
            name: None
        })),
        Some(Rejection::Offer(OfferError::NoOffer))
    );
    // Turned down, it costs nothing and goes.
    let (_, g0) = worlds().remove(0);
    let mut g = with_offers(&g0, p, vec![offer]);
    let gold = treasury(&g, p).0;
    assert_eq!(
        refusal(&g.apply(Command::RejectOffer {
            player: p,
            index: 0
        })),
        None
    );
    assert_eq!(treasury(&g, p).0, gold);
    assert_eq!(
        refusal(&g.apply(Command::RejectOffer {
            player: p,
            index: 0
        })),
        Some(Rejection::Offer(OfferError::NoOffer))
    );
}

#[test]
fn an_offer_that_is_not_answered_lapses_at_the_turn_change() {
    let (_, g0) = worlds().remove(0);
    let p = wizards(&g0)[0];
    let unit = g0
        .content()
        .rules
        .units
        .defs()
        .iter()
        .find(|d| d.hireable)
        .map(|d| f4x_core::rules::UnitId::new(d.num))
        .expect("a unit");
    let mut g = with_offers(
        &g0,
        p,
        vec![Offer::Mercenaries {
            unit,
            count: 1,
            level: 1,
            price: 1,
        }],
    );
    g.apply(Command::EndTurn);
    // What is waiting now was rolled at this turn change, not kept from before.
    let kept = g
        .player_component::<f4x_core::player::Offers>(p)
        .map(|o| {
            o.0.iter().any(|x| {
                matches!(
                    x,
                    Offer::Mercenaries {
                        price: 1,
                        count: 1,
                        ..
                    }
                )
            })
        })
        .unwrap_or(false);
    assert!(!kept, "an offer from the last turn is still waiting");
}
