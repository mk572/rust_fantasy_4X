//! Class 9, the player: the treasury (alchemy, taxes) and the offers a wizard is made.

use crate::support::{content, state, text, worlds};
use f4x_core::map::Position;
use f4x_core::player::offer::{Offer, OfferError};
use f4x_core::player::PlayerId;
use f4x_core::wizard::magic::{MagicError, Retort};
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;
use f4x_def::testdir::sample;

fn wizards(w: &World) -> Vec<PlayerId> {
    w.player_ids()
        .into_iter()
        .filter(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
        .collect()
}

fn treasury(w: &World, p: PlayerId) -> (HalfInt, HalfInt) {
    let t = w.player(p).expect("a player").treasury();
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
    for (name, mut w) in worlds().into_iter().step_by(2) {
        for p in wizards(&w) {
            // Half for any wizard, all of it for one with the Alchemy retort (`retorts.ron`).
            let has = w.wizard(p).is_some_and(|wizard| {
                wizard.retorts().contains(&Retort::new(0 /* alchemy */))
            });
            let percent = if has { 100 } else { 50 };
            let t = w.treasury_mut(p).expect("a treasury");
            t.gold = HalfInt::from_int(1000);
            t.mana = HalfInt::from_int(1000);
            // Gold to mana.
            let events = w.apply(Command::Alchemy {
                player: p,
                amount: 100,
                gold_to_mana: true,
            });
            assert_eq!(refusal(&events), None, "{name}");
            assert_eq!(
                treasury(&w, p),
                (
                    HalfInt::from_int(900),
                    HalfInt::from_int(1000 + 100 * percent / 100)
                ),
                "{name}: {p:?}"
            );
            // And back.
            w.apply(Command::Alchemy {
                player: p,
                amount: 100,
                gold_to_mana: false,
            });
            let (gold, mana) = treasury(&w, p);
            assert_eq!(
                mana,
                HalfInt::from_int(1000 + 100 * percent / 100 - 100),
                "{name}"
            );
            assert_eq!(gold, HalfInt::from_int(900 + 100 * percent / 100), "{name}");
            // More than there is: refused, nothing changes. A negative amount changes nothing either.
            let before = treasury(&w, p);
            let (gold, mana) = before;
            for (amount, to_mana, why) in [
                (gold.floor() + 1, true, MagicError::NotEnoughGold),
                (mana.floor() + 1, false, MagicError::NotEnoughMana),
            ] {
                let events = w.apply(Command::Alchemy {
                    player: p,
                    amount,
                    gold_to_mana: to_mana,
                });
                assert_eq!(refusal(&events), Some(Rejection::Magic(why)), "{name}");
                assert_eq!(treasury(&w, p), before);
            }
            w.apply(Command::Alchemy {
                player: p,
                amount: -50,
                gold_to_mana: true,
            });
            assert_eq!(treasury(&w, p), before, "{name}: a negative amount");
        }
    }
}

#[test]
fn a_higher_tax_brings_more_gold_until_the_rebels_stop_paying_and_the_rate_is_held_to_the_table() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        let top = (w.content().rules.config.tax_unrest_percent.len() - 1) as u8;
        let mut last: std::collections::BTreeMap<usize, (HalfInt, u32)> = Default::default();
        for rate in 0..=top {
            assert_eq!(
                refusal(&w.apply(Command::SetTaxRate { player: p, rate })),
                None
            );
            for (i, (city, _)) in w.economy().cities_of(p).into_iter().enumerate() {
                let eco = w.economy().city_economy(city).expect("an economy");
                if let Some(&(g0, r0)) = last.get(&i) {
                    assert!(eco.rebels >= r0, "{name}: rate {rate} calms city {i}");
                    // Rebels pay no taxes, so a rate that makes more of them can bring less; while it
                    // makes none more, a higher rate brings more.
                    if eco.rebels == r0 {
                        assert!(
                            eco.gold >= g0,
                            "{name}: rate {rate} brings less gold than {}",
                            rate - 1
                        );
                    }
                }
                last.insert(i, (eco.gold, eco.rebels));
            }
        }
        // A rate over the table is held to its top.
        w.apply(Command::SetTaxRate {
            player: p,
            rate: u8::MAX,
        });
        let held = w.player_component::<f4x_core::player::Tax>(p).map(|t| t.0);
        assert_eq!(held, Some(top), "{name}");
    }
}

/// A game in which `p` has been made the offers.
fn with_offers(w: &Table, p: PlayerId, offers: Vec<Offer>) -> Table {
    let mut s: Snapshot = Snapshot::from_text(&text(w)).expect("reads");
    let slot = s
        .players
        .iter_mut()
        .find(|x| x.id == p.0)
        .expect("the player");
    slot.offers = offers;
    Table::new(s.restore(content()).expect("restores"), w.rng.clone())
}

#[test]
fn mercenaries_offered_are_hired_for_their_price_or_turned_down_for_nothing() {
    let (_, w) = worlds().remove(0);
    let p = wizards(&w)[0];
    let unit = w
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
    let mut w = with_offers(&w, p, vec![offer.clone()]);
    w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(100);
    let units_before = w.units().player_units(p).len();
    // With too little gold the offer stands, the game as it was.
    w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(89);
    let was = state(&w);
    assert_eq!(
        refusal(&w.apply(Command::AcceptOffer {
            player: p,
            index: 0,
            name: None,
            at: None
        })),
        Some(Rejection::Offer(OfferError::NoGold))
    );
    assert_eq!(state(&w), was);
    // With enough, two units of the kind join and the price is paid.
    w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(100);
    let events = w.apply(Command::AcceptOffer {
        player: p,
        index: 0,
        name: None,
        at: None,
    });
    let hired = events
        .iter()
        .find_map(|e| match e {
            Event::MercenariesHired { units, .. } => Some(units.clone()),
            _ => None,
        })
        .expect("MercenariesHired");
    assert_eq!(hired.len(), 2);
    assert_eq!(w.units().player_units(p).len(), units_before + 2);
    for u in hired {
        assert_eq!(w.units().unit_owner(u), Some(p));
        assert_eq!(w.units().unit_def(u).map(|d| d.num), Some(unit.num()));
    }
    assert_eq!(treasury(&w, p).0, HalfInt::from_int(10));
    // The offer is answered: asking again finds nothing.
    assert_eq!(
        refusal(&w.apply(Command::AcceptOffer {
            player: p,
            index: 0,
            name: None,
            at: None
        })),
        Some(Rejection::Offer(OfferError::NoOffer))
    );
    // Turned down, it costs nothing and goes.
    let (_, g0) = worlds().remove(0);
    let mut w = with_offers(&g0, p, vec![offer]);
    let gold = treasury(&w, p).0;
    assert_eq!(
        refusal(&w.apply(Command::RejectOffer {
            player: p,
            index: 0
        })),
        None
    );
    assert_eq!(treasury(&w, p).0, gold);
    assert_eq!(
        refusal(&w.apply(Command::RejectOffer {
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
    let w = with_offers(
        &g0,
        p,
        vec![Offer::Mercenaries {
            unit,
            count: 1,
            level: 1,
            price: 1,
        }],
    );
    let mut w = w;
    w.end_player_turn(p);
    w.start_player_turn(p, 2);
    // What is waiting now was rolled at this turn change, not kept from before.
    let kept = w
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

#[test]
fn with_several_circles_a_hired_hero_and_mercenaries_come_to_the_circle_asked_for() {
    use crate::c9_spell::two_cities;
    use f4x_core::rules::UnitId;
    let (_, mut w) = worlds().remove(0);
    w.scenario_change_settings(|s| s.rules.multiple_summoning_circles = true);
    let (p, first, second, _) = two_cities(&mut w);
    w.move_summoning_circle(p, second).expect("moved");
    let circles = w.wizard(p).expect("a wizard").summon_circles();
    assert_eq!(circles.len(), 2);
    let (a, b) = (circles[0], circles[1]);
    let _ = first;
    let unit = w
        .content()
        .rules
        .units
        .defs()
        .iter()
        .find(|d| d.hireable && d.kind == f4x_def::rules::UnitKind::Normal && !d.is_settler())
        .map(|d| UnitId::new(d.num))
        .expect("a unit for hire");
    let kind = (0..w.content().rules.hero_templates.len())
        .find(|&k| {
            w.units()
                .hero_entity(k)
                .is_some_and(|h| w.units().hero_state(h) == Some(f4x_core::hero::HeroState::Free))
        })
        .expect("a free hero");
    for offer in [
        Offer::Mercenaries {
            unit,
            count: 2,
            level: 1,
            price: 1,
        },
        Offer::Hero { kind, price: 1 },
    ] {
        let mut w = with_offers(&w, p, vec![offer.clone()]);
        w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(100_000);
        let accept = |w: &mut Table, at| {
            w.apply(Command::AcceptOffer {
                player: p,
                index: 0,
                name: None,
                at,
            })
        };
        let was = state(&w);
        // No circle named, or one that is none of the wizard's: refused, nothing changes.
        let elsewhere = Position {
            x: a.x.wrapping_add(1),
            ..a
        };
        for at in [None, Some(elsewhere)] {
            assert_eq!(
                refusal(&accept(&mut w, at)),
                Some(Rejection::Offer(OfferError::Circle)),
                "{offer:?} at {at:?}"
            );
            assert_eq!(state(&w), was);
        }
        // The second circle: whoever joins appears there, and not at the first.
        let (here_a, here_b) = (w.units().units_at(a).len(), w.units().units_at(b).len());
        let events = accept(&mut w, Some(b));
        assert_eq!(refusal(&events), None, "{events:?}");
        assert!(
            w.units().units_at(b).len() > here_b,
            "{offer:?}: nobody came to {b:?}"
        );
        assert_eq!(
            w.units().units_at(a).len(),
            here_a,
            "{offer:?}: someone came to {a:?}"
        );
    }
}

#[test]
fn with_several_circles_resurrection_asks_for_the_circle_the_hero_comes_back_to() {
    use crate::c9_spell::two_cities;
    use f4x_core::rules::exec::Target;
    use f4x_core::spell::hero::HeroSummon;
    let (_, mut w) = worlds().remove(0);
    w.scenario_change_settings(|s| s.rules.multiple_summoning_circles = true);
    let (p, _, second, _) = two_cities(&mut w);
    w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(100_000);
    let kind = (0..w.content().rules.hero_templates.len())
        .find(|&k| w.hire_hero(p, k).is_ok())
        .expect("a hero can be hired");
    let hero = w.units().hero_entity(kind).expect("the hero");
    w.cheat_slay_unit(hero);
    let spell = HeroSummon::Resurrection
        .spell_in(&w.content().rules)
        .expect("a Resurrection spell");
    assert!(
        w.units().raisable_heroes().contains(&kind),
        "the hero is not dead"
    );
    // One circle: the dead hero is the target. Two: the circle is named too.
    assert_eq!(w.check_target(p, spell, Some(Target::Hero(kind))), Ok(()));
    w.move_summoning_circle(p, second).expect("moved");
    let circles = w.wizard(p).expect("a wizard").summon_circles();
    assert!(w.check_target(p, spell, Some(Target::Hero(kind))).is_err());
    let elsewhere = Position {
        x: circles[0].x.wrapping_add(1),
        ..circles[0]
    };
    assert!(w
        .check_target(p, spell, Some(Target::HeroAt(kind, elsewhere)))
        .is_err());
    for &at in &circles {
        assert_eq!(
            w.check_target(p, spell, Some(Target::HeroAt(kind, at))),
            Ok(())
        );
    }
    // And the hero comes back to the circle named.
    w.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
    w.scenario_casting(p, spell, 0);
    let at = circles[1];
    let events = w.apply(Command::ResolveCast {
        player: p,
        target: Target::HeroAt(kind, at),
    });
    assert_eq!(refusal(&events), None, "{events:?}");
    let back = w.units().hero_entity(kind).expect("the hero");
    assert_eq!(w.units().unit_position(back), Some(at));
}

/// Without heroes and items the only offers are mercenaries.
#[test]
fn without_heroes_nobody_is_offered_a_hero_or_an_item() {
    for heroes in [true, false] {
        let (mut hired, mut items, mut mercenaries) = (0, 0, 0);
        for (name, mut w) in worlds() {
            w.scenario_change_settings(|g| g.world.heroes_enabled = heroes);
            let wizards = w.wizards();
            for &p in &wizards {
                w.set_fame(p, 200);
            }
            for round in 100..100 + sample(1500, 300) as u32 {
                for &p in &wizards {
                    w.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(100_000);
                    for event in w.start_player_turn(p, round) {
                        if let Event::OfferMade { offer, .. } = event {
                            match offer {
                                Offer::Hero { .. } => hired += 1,
                                Offer::Item { .. } => items += 1,
                                Offer::Mercenaries { .. } => mercenaries += 1,
                                _ => panic!("{name}: {offer:?}"),
                            }
                        }
                    }
                }
            }
        }
        let what = format!("heroes {heroes}: {hired} heroes, {items} items, {mercenaries}");
        assert!(mercenaries > 0, "{what}");
        if heroes {
            assert!(hired > 0 && items > 0, "{what}");
        } else {
            assert!(hired == 0 && items == 0, "{what}");
        }
    }
}
