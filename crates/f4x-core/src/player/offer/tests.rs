use super::*;
use crate::game::testutil::{content_with_economy, unit};
use crate::hero::game::tests::{at, world as hero_world, P0, P1};
use crate::item::armory::Placed;
use crate::rules::{Registry, UnitKind};

/// The hero world with a fortress for both wizards, one human, two races (elves on Arcanus,
/// trolls on Myrror), three kinds of unit a mercenary company may be and three it may not, a
/// prefab item and a plain power for the merchant.
fn world() -> Game {
    let mut g = hero_world();
    let races = content_with_economy().rules.races;
    let mut elf = races.by_name("elf").unwrap().clone();
    elf.units.clear();
    let mut troll = elf.clone();
    troll.id = "troll".into();
    troll.name = "troll".into();
    troll.myrror = true;
    let mut draconian = troll.clone();
    draconian.id = "draconian".into();
    draconian.name = "draconian".into();
    g.content.rules.races = Registry::new(vec![elf, troll, draconian]);
    let mut units = g.content.rules.units.defs().to_vec();
    // Only the units added below are ordinary race units.
    units.retain(|u| u.kind != UnitKind::Normal);
    let mut add = |id: &str, race: &str, cost: u32, abilities: &[&str]| {
        let mut u = unit(id, crate::rules::MovementType::Walking, 1, abilities);
        u.kind = UnitKind::Normal;
        u.race = Some(race.into());
        u.cost = cost;
        units.push(u);
    };
    add("elf_spear", "elf", 20, &[]);
    add("troll_spear", "troll", 30, &[]);
    add("settler", "elf", 30, &["create_outpost"]);
    add("elf_engineers", "elf", 40, &[]);
    add("draconian_spear", "draconian", 30, &[]);
    add("draconian_engineers", "draconian", 40, &[]);
    g.content.rules.units = Registry::new(units);
    // Each wizard has one unit on Arcanus, so Arcanus is the plane they can be offered.
    let scout = g.content.rules.units.id_of("elf_spear").unwrap();
    g.spawn_unit(P0, at(2, 3), scout);
    g.spawn_unit(P1, at(8, 3), scout);
    g.set_human(Some(P0));
    g.turn = 40;
    for (i, id) in g.player_ids().into_iter().take(2).enumerate() {
        g.treasury_mut(id).unwrap().gold = 5000;
        g.wizard_mut(id)
            .unwrap()
            .put(Some(crate::wizard::fortress::Fortress(at(
                1 + i as u32 * 6,
                1,
            ))));
        g.set_fame(id, 190);
    }
    g
}

fn offers_of(g: &Game, p: PlayerId, f: impl Fn(&Offer) -> bool) -> Vec<usize> {
    g.offers(p)
        .iter()
        .enumerate()
        .filter(|(_, o)| f(o))
        .map(|(i, _)| i)
        .collect()
}

/// Roll the turn's offers until `wanted` is among player 0's; the game is left with them.
fn roll_until(g: &mut Game, wanted: impl Fn(&Offer) -> bool) -> usize {
    for _ in 0..3000 {
        g.determine_offers();
        if let Some(&i) = offers_of(g, P0, &wanted).first() {
            return i;
        }
    }
    panic!("the offer never came");
}

#[test]
fn the_chances_follow_fame_the_retort_and_the_computer_bonus() {
    // 2 + fame/25, doubled for a Famous wizard, at most 10.
    assert_eq!(item_chance(0, false), 2);
    assert_eq!(item_chance(50, false), 4);
    assert_eq!(item_chance(50, true), 8);
    assert_eq!(item_chance(500, true), 10);
    // 1 + fame/20; the computer wizards get ten more on top of the cap.
    assert_eq!(mercenary_chance(40, false, false), 3);
    assert_eq!(mercenary_chance(400, true, false), 10);
    assert_eq!(mercenary_chance(400, true, true), 20);
    // 3 + fame/25, plus ten for the computer, divided by (heroes + 1) / 2 + 1.
    assert_eq!(hero_chance(0, false, false, 0), 3);
    assert_eq!(hero_chance(0, false, true, 0), 13);
    assert_eq!(hero_chance(500, true, true, 0), 20);
    assert_eq!(hero_chance(500, true, true, 1), 10);
    assert_eq!(hero_chance(500, true, true, 4), 20 / 3);
    assert_eq!(hero_chance(500, true, true, 5), 5);
}

#[test]
fn a_company_has_a_level_a_size_and_a_price() {
    assert_eq!(mercenary_level(95, 0), 3);
    assert_eq!(mercenary_level(50, 15), 2);
    assert_eq!(mercenary_level(10, 0), 1);
    assert_eq!(mercenary_level(91, 0), 3);
    assert_eq!(mercenary_level(90, 0), 2);
    assert_eq!(mercenary_count(95, 0), 3);
    assert_eq!(mercenary_count(61, 0), 2);
    assert_eq!(mercenary_count(60, 0), 1);
    // 20 gold units, level 3: 20 * 6 / 2 = 60 each.
    assert_eq!(mercenary_price(20, 3, 2, false), 120);
    assert_eq!(mercenary_price(20, 3, 2, true), 60);
    assert_eq!(item_price(500, false), 1500);
    assert_eq!(item_price(500, true), 750);
}

#[test]
fn the_computer_wizards_answer_by_gold_and_turn() {
    // A hero at nine tenths of the gold is hired, one dearer is not.
    assert!(ai_accepts_hero(90, 100));
    assert!(!ai_accepts_hero(91, 100));
    // The original's arithmetic would have taken a price a ninth over the gold.
    assert!(!ai_accepts_hero(110, 100));
    assert!(ai_accepts_mercenaries(50, 50, 100));
    assert!(!ai_accepts_mercenaries(49, 50, 100));
    assert!(!ai_accepts_mercenaries(60, 51, 100));
}

#[test]
fn a_company_is_drawn_from_the_races_of_the_planes_the_wizard_is_on() {
    let g = world();
    let rules = &g.content.rules;
    let ids = |arcanus, myrror| {
        let mut v: Vec<_> = mercenary_pool(rules, arcanus, myrror)
            .iter()
            .map(|u| u.id.clone())
            .collect();
        v.sort();
        v
    };
    // Never settlers, boats, heroes or summoned creatures. Engineers are offered like any other
    // unit, except the Draconian ones: Draconians fly and need no roads.
    assert_eq!(
        ids(true, false),
        vec!["elf_engineers".to_string(), "elf_spear".to_string()]
    );
    assert_eq!(
        ids(false, true),
        vec!["draconian_spear".to_string(), "troll_spear".to_string()]
    );
    assert_eq!(ids(true, true).len(), 4);
    assert!(ids(false, false).is_empty());
}

#[test]
fn only_wizards_with_a_standing_fortress_are_offered_anything() {
    let mut g = world();
    g.wiz_mut(0)
        .put(Some(crate::wizard::fortress::Banishment { by: None }));
    g.wiz_mut(1).put::<crate::wizard::fortress::Fortress>(None);
    for _ in 0..300 {
        g.determine_offers();
    }
    assert!(g.offers(P0).is_empty() && g.offers(P1).is_empty());
    assert!(g.offers(PlayerId(9)).is_empty());
}

#[test]
fn mercenaries_come_after_turn_30_to_a_wizard_who_can_pay_and_arrive_with_their_experience() {
    let mut g = world();
    g.turn = 29;
    for _ in 0..400 {
        g.determine_offers();
        assert!(offers_of(&g, P0, |o| matches!(o, Offer::Mercenaries { .. })).is_empty());
    }
    g.turn = 40;
    let i = roll_until(&mut g, |o| matches!(o, Offer::Mercenaries { .. }));
    let Offer::Mercenaries {
        unit,
        count,
        level,
        price,
    } = g.offers(P0)[i].clone()
    else {
        unreachable!()
    };
    // Only the elves' units: the wizard is on Arcanus only.
    assert!(
        ["elf_spear", "elf_engineers"].contains(&unit.as_str()),
        "{unit}"
    );
    assert!((1..=3).contains(&count) && (1..=3).contains(&level));
    let (arcanus, myrror) = g.planes_of(P0);
    assert_eq!((arcanus, myrror), (true, false));
    let gold = g.tr(0).gold;
    let before = g.units().units_at(at(1, 1)).len();
    let events = g.accept_offer(P0, i, None).unwrap();
    assert_eq!(g.tr(0).gold, gold - price);
    let Some(Event::MercenariesHired { units, .. }) = events.first() else {
        panic!("{events:?}");
    };
    assert_eq!(units.len(), count as usize);
    assert_eq!(g.units().units_at(at(1, 1)).len(), before + count as usize);
    let xp = g.world.get::<&Experience>(units[0]).unwrap().0;
    assert_eq!(
        crate::hero::kind::level_of(
            &g.content.rules,
            g.content.rules.units.id_of(&unit).unwrap(),
            xp
        ),
        // The test unit's path has two levels only.
        usize::from(level).min(1)
    );
    // The offer is used up.
    assert_eq!(g.accept_offer(P0, i, None), Err(OfferError::NoOffer));
}

#[test]
fn mercenaries_are_not_offered_to_a_wizard_who_cannot_pay_even_at_the_discount() {
    let mut g = world();
    g.tr_mut(0).gold = 0;
    for _ in 0..500 {
        g.determine_offers();
        assert!(offers_of(&g, P0, |o| matches!(o, Offer::Mercenaries { .. })).is_empty());
    }
}

#[test]
fn a_hero_for_hire_joins_under_the_name_the_player_gives() {
    let mut g = world();
    let i = roll_until(&mut g, |o| matches!(o, Offer::Hero { .. }));
    let Offer::Hero { kind, price } = g.offers(P0)[i] else {
        unreachable!()
    };
    let gold = g.tr(0).gold;
    let events = g.accept_offer(P0, i, Some("Rex".into())).unwrap();
    assert!(matches!(events[..], [Event::HeroJoined { kind: k, .. }] if k == kind));
    assert_eq!(g.tr(0).gold, gold - price);
    assert_eq!(g.units().hero_type_name(kind).as_deref(), Some("Rex"));
    assert_eq!(g.units().hero_units(P0).len(), 1);
}

#[test]
fn a_prisoner_in_a_cleared_lair_is_a_hero_who_joins_for_nothing() {
    let mut g = world();
    g.player_component_mut::<crate::player::Offers>(P0)
        .unwrap()
        .0
        .clear();
    let site = g.put_lair(
        crate::map::Position {
            plane: crate::map::Plane::Arcanus,
            x: 5,
            y: 5,
        },
        "lair",
        crate::feature::lair::Lair {
            intact: true,
            guards: Vec::new(),
            gold: 0,
            mana: 0,
            items: Vec::new(),
            prisoner: true,
            special: 0,
            fought_by: Vec::new(),
        },
    );
    let events = g.claim_lair(site, P0);
    let Some(Event::SiteCleared { loot, .. }) = events.first() else {
        panic!("{events:?}");
    };
    assert!(loot.prisoner);
    assert!(matches!(g.offers(P0)[0], Offer::Hero { price: 0, .. }));
    let gold = g.tr(0).gold;
    let joined = g.accept_offer(P0, 0, None).unwrap();
    assert!(matches!(joined[..], [Event::HeroJoined { .. }]));
    assert_eq!(g.tr(0).gold, gold, "a freed prisoner costs nothing");
    assert_eq!(g.units().hero_units(P0).len(), 1);
}

#[test]
fn a_hero_the_wizard_cannot_pay_for_stays_on_offer() {
    let mut g = world();
    let i = roll_until(&mut g, |o| matches!(o, Offer::Hero { .. }));
    g.tr_mut(0).gold = 0;
    assert_eq!(g.accept_offer(P0, i, None), Err(OfferError::NoGold));
    assert!(!g.offers(P0).is_empty());
    assert!(g.units().hero_units(P0).is_empty());
}

#[test]
fn every_wizard_meets_the_merchant_and_the_item_goes_to_the_armory() {
    let mut g = world();
    // The computer wizard (player 1) gets the same offers as the human.
    let mut computer_met_him = false;
    for _ in 0..400 {
        g.determine_offers();
        computer_met_him |= !offers_of(&g, P1, |o| matches!(o, Offer::Item { .. })).is_empty();
    }
    assert!(computer_met_him);
    let i = roll_until(&mut g, |o| matches!(o, Offer::Item { .. }));
    let Offer::Item { item, price } = g.offers(P0)[i].clone() else {
        unreachable!()
    };
    assert_eq!(price, item_price(item.cost, false));
    let gold = g.tr(0).gold;
    let events = g.accept_offer(P0, i, None).unwrap();
    assert_eq!(g.tr(0).gold, gold - price);
    assert!(matches!(
        events[..],
        [Event::ItemMade {
            placed: Placed::Vault(_),
            ..
        }]
    ));
}

#[test]
fn an_offered_item_is_an_entity_that_is_kept_bought_or_withdrawn() {
    use crate::item::entity::HeldBy;
    let held = |g: &Game, by: HeldBy| g.items().count_held(|h| h == by);
    let mut g = world();
    let i = roll_until(&mut g, |o| matches!(o, Offer::Item { .. }));
    assert_eq!(held(&g, HeldBy::Offered(P0)), 1);
    // Bought, it is the wizard's.
    g.accept_offer(P0, i, None).unwrap();
    assert_eq!(held(&g, HeldBy::Offered(P0)), 0);
    assert_eq!(g.items().armory(P0).items().count(), 1);
    let i = roll_until(&mut g, |o| matches!(o, Offer::Item { .. }));
    // Turned down, the item is gone.
    g.reject_offer(P0, i).unwrap();
    assert_eq!(held(&g, HeldBy::Offered(P0)), 0);
    assert_eq!(
        g.items().count_held(|_| true),
        1,
        "only the bought one is left"
    );
}

#[test]
fn the_price_is_tested_after_the_charismatic_discount() {
    let mut g = world();
    g.wizard_mut(crate::player::PlayerId(0))
        .unwrap()
        .get_mut::<crate::wizard::sheet::Retorts>()
        .0
        .insert(Retort::Charismatic);
    // Hardly any gold: the full price could never be paid, the half price can be.
    for _ in 0..3000 {
        g.tr_mut(0).gold = 100;
        g.determine_offers();
        for offer in g.offers(P0) {
            if let Offer::Item { item, price } = offer {
                assert_eq!(price, item.cost as i32 * 3 / 2);
                assert!(price <= 100);
                return;
            }
        }
    }
    panic!("no cheap item was offered");
}

#[test]
fn a_rejected_or_lapsed_item_can_be_offered_again() {
    let mut g = world();
    // One prefab item only, so the taken list shows plainly.
    let mut items = g.content.rules.items.defs().to_vec();
    items.clear();
    items.push(crate::rules::ItemDef {
        id: "only".into(),
        name: "only".into(),
        slot: crate::rules::ItemSlot::Weapon,
        kind: crate::rules::ItemKind::Sword,
        cost: 10,
        icon: 0,
        effects: Vec::new(),
        spell_skill: 0,
        spell_save: 0,
        powers: Vec::new(),
        charges: None,
        flag: false,
    });
    g.content.rules.items = Registry::new(items);
    let i = roll_until(
        &mut g,
        |o| matches!(o, Offer::Item { item, .. } if item.prefab.is_some()),
    );
    assert_eq!(g.prefab_taken, vec!["only".to_string()]);
    g.reject_offer(P0, i).unwrap();
    assert!(g.prefab_taken.is_empty());
    // An unanswered one lapses at the next turn change.
    let i = roll_until(
        &mut g,
        |o| matches!(o, Offer::Item { item, .. } if item.prefab.is_some()),
    );
    assert!(i < g.offers(P0).len());
    g.player_component_mut::<crate::player::Offers>(P0)
        .unwrap()
        .0
        .clear();
    assert_eq!(g.reject_offer(P0, 0), Err(OfferError::NoOffer));
}

#[test]
fn the_turn_change_makes_the_offers_and_a_save_keeps_them() {
    let mut g = world();
    roll_until(&mut g, |o| matches!(o, Offer::Hero { .. }));
    // A save wants a city wherever there is a fortress; this world has none.
    g.wiz_mut(0).put::<crate::wizard::fortress::Fortress>(None);
    g.wiz_mut(1).put::<crate::wizard::fortress::Fortress>(None);
    let text = ron::to_string(&g.snapshot()).unwrap();
    let back = crate::game::save::Snapshot::from_text(&text)
        .unwrap()
        .restore(g.content.clone())
        .unwrap();
    assert_eq!(back.offers(P0), g.offers(P0));
    assert_eq!(back.prefab_taken, g.prefab_taken);
}

#[test]
fn the_commands_accept_and_reject() {
    let mut g = world();
    let i = roll_until(&mut g, |o| matches!(o, Offer::Hero { .. }));
    let events = g.apply(crate::game::Command::RejectOffer {
        player: P0,
        index: i,
    });
    assert!(events.is_empty(), "{events:?}");
    let events = g.apply(crate::game::Command::AcceptOffer {
        player: P0,
        index: 99,
        name: None,
    });
    assert!(matches!(
        events[..],
        [Event::Rejected(crate::game::Rejection::Offer(
            OfferError::NoOffer
        ))]
    ));
}
