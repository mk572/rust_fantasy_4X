//! The offers made at the turn change: a hero for hire and a company of mercenaries (the
//! merchant only calls on a human wizard). The rules are the original's `AI_Accept_Hero` and
//! `AI_Accept_Mercenaries`, in `f4x_core::player::offer`.

use f4x_core::item::armory::ItemSpot;
use f4x_core::item::Item;
use f4x_core::player::offer::{ai_accepts_hero, ai_accepts_item, ai_accepts_mercenaries, Offer};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::world::{Command, Event, World};
use f4x_def::half_int::HalfInt;

use crate::Ai;
use f4x_master::Game;

/// Mana the wizard must hold to send an item to a hero away from the fortress.
const FAR_RESERVE: i32 = 120;

/// What an item is worth to this hero, in gold: its value, half as much again when it suits the
/// hero's trade (a caster likes spell skill and charges, a fighter attack and defence) and half
/// when it does not.
pub(crate) fn use_value(world: &World, hero: hecs::Entity, item: &Item) -> i32 {
    let caster = world.units().hero_mana(hero) > 0;
    let b = &item.bonus;
    let magic = b.spell_skill + b.spell_save + i32::from(item.charges.is_some());
    let melee = b.attack + b.to_hit + b.defense + b.resistance;
    let cost = item.cost as i32;
    match (caster, magic > 0, melee > 0) {
        (true, true, _) | (false, _, true) => cost * USE_SUITS_PCT / 100,
        (false, true, false) => cost * USE_MISFIT_PCT / 100,
        _ => cost,
    }
}

/// The percent of an item's value a hero it suits and a hero it does not suit count it for.
const USE_SUITS_PCT: i32 = 150;
const USE_MISFIT_PCT: i32 = 50;
/// An item that takes the place of the one a hero carries must be worth this percent of it.
const BETTER_PCT: i32 = 130;
/// The unusable items the vault may hold before the wizard scraps the cheapest.
const KEEP_UNUSABLE: usize = 2;

/// Put the items of the vault and the award list on the heroes that use them best: a free slot
/// takes the item of the most use to that hero that fits, and a slot already taken gives its item
/// up for one worth at least [`BETTER_PCT`] percent of it (the old one goes back to the vault).
/// Moving an item to a hero away from the fortress costs mana (the item screen's 20), which the
/// wizard pays when it has [`FAR_RESERVE`] to spare.
pub fn equip(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    use f4x_core::item::armory::ItemWorld;
    let (heroes, fortress) = {
        let view = PlayerView::new(game, ai.player);
        (view.my_heroes(), view.fortress())
    };
    for hero in heroes {
        let mut far = false;
        let slots = {
            let view = PlayerView::new(game, ai.player);
            if view.unit_position(hero) != fortress {
                far = true;
                if view.treasury().mana.floor() < FAR_RESERVE {
                    continue;
                }
            }
            game.items()
                .hero_slots(ai.player, hero)
                .map_or_else(Vec::new, |s| s.into_iter().enumerate().collect::<Vec<_>>())
        };
        for (slot, kind) in slots {
            let from = {
                let carried = game
                    .items()
                    .carried(hero)
                    .into_iter()
                    .find(|(s, _)| usize::from(*s) == slot)
                    .map(|(_, item)| use_value(game, hero, &item));
                let armory = game.items().armory(ai.player);
                let vault = armory
                    .vault
                    .iter()
                    .enumerate()
                    .filter_map(|(i, item)| Some((ItemSpot::Vault(i), item.as_ref()?)));
                let awards = armory
                    .awards
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (ItemSpot::Award(i), item));
                // The fitting item of most use to this hero, if it beats what the slot holds.
                vault
                    .chain(awards)
                    .filter(|(_, item)| kind.fits(item.kind))
                    .map(|(spot, item)| (spot, use_value(game, hero, item)))
                    .max_by_key(|&(_, value)| value)
                    .filter(|&(_, value)| carried.is_none_or(|c| value * 100 >= c * BETTER_PCT))
                    .map(|(spot, _)| spot)
            };
            if let Some(from) = from {
                ai.order(
                    game,
                    Command::MoveItem {
                        player: ai.player,
                        from,
                        to: ItemSpot::Hero(hero, slot),
                        pay: far,
                    },
                    events,
                );
            }
        }
    }
    scrap(ai, game, events);
}

/// Break up the items that no hero of the wizard could carry, the cheapest first, once more than
/// [`KEEP_UNUSABLE`] of them lie in the armory (a hero hired later might use them, and a
/// neighbour might buy them: `crate::trade`), for mana.
fn scrap(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    use f4x_core::item::armory::{ItemWorld, Spot};
    loop {
        let doomed = {
            let heroes = PlayerView::new(game, ai.player).my_heroes();
            if heroes.is_empty() {
                return;
            }
            let kinds: Vec<_> = heroes
                .iter()
                .filter_map(|&h| game.items().hero_slots(ai.player, h))
                .flatten()
                .collect();
            let armory = game.items().armory(ai.player);
            let vault = armory
                .vault
                .iter()
                .enumerate()
                .filter_map(|(i, item)| Some((Spot::Vault(i), item.as_ref()?)));
            let awards = armory
                .awards
                .iter()
                .enumerate()
                .map(|(i, item)| (Spot::Award(i), item));
            let mut unusable: Vec<(Spot, u32)> = vault
                .chain(awards)
                .filter(|(_, item)| !kinds.iter().any(|k| k.fits(item.kind)))
                .map(|(spot, item)| (spot, item.cost))
                .collect();
            if unusable.len() <= KEEP_UNUSABLE {
                return;
            }
            unusable.sort_by_key(|&(_, cost)| cost);
            unusable[0].0
        };
        if !ai
            .send(
                game,
                Command::ScrapItem {
                    player: ai.player,
                    from: doomed,
                },
                events,
            )
            .is_ok()
        {
            return;
        }
    }
}

/// Whether an item on offer is worth its price to the wizard: it can afford it (the price is at
/// most half its gold), and the item is worth to it, with the heroes it has to carry it, at least
/// [`WORTH_PCT`] percent of the price.
fn item_wanted(world: &World, player: PlayerId, item: &Item, price: i32, gold: HalfInt) -> bool {
    let heroes = PlayerView::new(world, player).my_heroes().len();
    ai_accepts_item(price, gold, heroes)
        && f4x_core::diplomacy::trade::item_worth(world, player, item) * MARKUP * 100
            >= price * WORTH_PCT
}

/// What an item must be worth to the wizard to buy it, in percent of the price without the
/// merchant's markup (`MARKUP`, `player::offer::item_price`): a hero with a free slot for it makes
/// it worth more than that, a hero with a taken slot as much, no hero half.
const WORTH_PCT: i32 = 100;
const MARKUP: i32 = 3;

/// Answer every offer waiting: hire or decline. The last offer goes first, so that an answer
/// does not move the ones still to be given.
pub fn answer(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let count = PlayerView::new(game, ai.player).offers().len();
    for index in (0..count).rev() {
        let (offer, gold, turn) = {
            let view = PlayerView::new(game, ai.player);
            let Some(offer) = view.offers().get(index).cloned() else {
                continue;
            };
            (offer, view.treasury().gold, game.round())
        };
        let accept = match &offer {
            Offer::Hero { price, .. } => ai_accepts_hero(*price, gold),
            // A summoned hero is already paid for in mana.
            Offer::Summoned { .. } => true,
            Offer::Mercenaries { price, .. } => {
                ai_accepts_mercenaries(&game.content().rules.config.offer, turn, *price, gold)
            }
            Offer::Item { item, price } | Offer::Resale { item, price, .. } => {
                item_wanted(game, ai.player, item, *price, gold)
            }
        };
        let command = if accept {
            Command::AcceptOffer {
                player: ai.player,
                index,
                name: None,
                at: crate::wizardry::best_circle(&PlayerView::new(game, ai.player)),
            }
        } else {
            Command::RejectOffer {
                player: ai.player,
                index,
            }
        };
        // An offer it cannot take (no room for the hero at the summoning circle, say) is
        // declined, and does not wait to be refused again each turn.
        if !ai.send(game, command, events).is_ok() && accept {
            ai.order(
                game,
                Command::RejectOffer {
                    player: ai.player,
                    index,
                },
                events,
            );
        }
    }
}

/// The computer wizards answer the items resold to them now, one after the other
/// round the cycle until the item is taken or nobody is left. Returns the events.
pub fn answer_resales(game: &mut Game) -> Vec<Event> {
    let mut events = Vec::new();
    // Every refusal passes the item on, so the cycle is as long as the number of wizards.
    for _ in 0..game.player_ids().len() {
        let waiting: Vec<_> = game
            .player_ids()
            .into_iter()
            .filter(|&p| Some(p) != game.human())
            .filter(|&p| {
                PlayerView::new(game, p)
                    .offers()
                    .iter()
                    .any(|o| matches!(o, Offer::Resale { .. }))
            })
            .collect();
        if waiting.is_empty() {
            break;
        }
        for player in waiting {
            let ai = Ai::new(player);
            let (index, price, item, gold) = {
                let view = PlayerView::new(game, player);
                let Some(index) = view
                    .offers()
                    .iter()
                    .position(|o| matches!(o, Offer::Resale { .. }))
                else {
                    continue;
                };
                let Offer::Resale { item, price, .. } = view.offers()[index].clone() else {
                    continue;
                };
                (index, price, item, view.treasury().gold)
            };
            let command = if item_wanted(game, player, &item, price, gold) {
                Command::AcceptOffer {
                    player,
                    index,
                    name: None,
                    at: None,
                }
            } else {
                Command::RejectOffer { player, index }
            };
            ai.order(game, command, &mut events);
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::player::offer::OfferKind;
    use f4x_core::player::PlayerId;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const P1: PlayerId = PlayerId(1);

    fn game() -> Result<Game, Box<dyn std::error::Error>> {
        crate::testworld::fresh_game()
    }

    fn set_gold(w: &mut Game, player: PlayerId, gold: i32) -> TestResult {
        w.treasury_mut(player)
            .ok_or("the player has a treasury")?
            .gold = f4x_def::half_int::HalfInt::from_int(gold);
        Ok(())
    }

    /// Force a hero offer. A hero is only offered to a wizard whose fame exceeds its cost's step
    /// over 100, and the pick can come up empty, so it tries again.
    fn offer_hero(w: &mut Game) {
        w.set_fame(P1, 100);
        let round = w.turn;
        assert!(
            (0..500).any(|_| w.cheat_offer(P1, OfferKind::Hero, round).is_some()),
            "no hero was ever offered"
        );
    }

    #[test]
    fn a_computer_wizard_hires_the_hero_it_can_pay_for_and_turns_down_one_it_cannot() -> TestResult
    {
        let mut w = game()?;
        let ai = Ai::new(P1);
        set_gold(&mut w, P1, 5000)?;
        offer_hero(&mut w);
        let mut events = Vec::new();
        answer(&ai, &mut w, &mut events);
        assert!(
            events.iter().any(|e| matches!(e, Event::HeroJoined { .. })),
            "{events:?}"
        );
        assert!(w.offers(P1).is_empty());
        // With nothing to pay with, the offer is turned down, not left lying.
        set_gold(&mut w, P1, 0)?;
        offer_hero(&mut w);
        let mut events = Vec::new();
        answer(&ai, &mut w, &mut events);
        assert!(!events.iter().any(|e| matches!(e, Event::HeroJoined { .. })));
        assert!(w.offers(P1).is_empty());
        Ok(())
    }

    #[test]
    fn mercenaries_wait_for_the_first_mercenary_turn() -> TestResult {
        let mut w = game()?;
        let ai = Ai::new(P1);
        set_gold(&mut w, P1, 100_000)?;
        let first = w.content().rules.config.offer.ai_first_turn;
        w.turn = first - 10;
        let mut got = 0;
        for _ in 0..50 {
            let round = w.turn;
            if w.cheat_offer(P1, OfferKind::Mercenaries, round).is_some() {
                got += 1;
            }
        }
        assert!(
            got > 0,
            "the wizard needs a unit on a plane to be offered any"
        );
        let mut events = Vec::new();
        answer(&ai, &mut w, &mut events);
        assert!(!events
            .iter()
            .any(|e| matches!(e, Event::MercenariesHired { .. })));
        assert!(w.offers(P1).is_empty());
        w.turn = first;
        let round = w.turn;
        w.cheat_offer(P1, OfferKind::Mercenaries, round);
        let mut events = Vec::new();
        answer(&ai, &mut w, &mut events);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::MercenariesHired { .. })));
        Ok(())
    }

    #[test]
    fn a_computer_wizard_buys_the_merchants_item_once_it_has_a_hero_and_puts_it_on_the_hero(
    ) -> TestResult {
        let mut w = game()?;
        let ai = Ai::new(P1);
        set_gold(&mut w, P1, 100_000)?;
        let round = w.turn;
        // Without a hero to carry it, the item is turned down.
        w.cheat_offer(P1, OfferKind::Item, round)
            .ok_or("an item is offered")?;
        let mut events = Vec::new();
        answer(&ai, &mut w, &mut events);
        assert!(w.offers(P1).is_empty());
        assert!(w.items().armory(P1).items().next().is_none());
        // With one, it is bought and the hero at the fortress takes what fits its slots.
        offer_hero(&mut w);
        answer(&ai, &mut w, &mut events);
        let hero = *PlayerView::new(&w, P1)
            .my_heroes()
            .first()
            .ok_or("the wizard hired a hero")?;
        let mut equipped = false;
        for _ in 0..40 {
            let round = w.turn;
            if w.cheat_offer(P1, OfferKind::Item, round).is_none() {
                continue;
            }
            answer(&ai, &mut w, &mut events);
            equip(&ai, &mut w, &mut events);
            if !w.items().carried(hero).is_empty() {
                equipped = true;
                break;
            }
        }
        assert!(equipped, "the hero never got an item");
        Ok(())
    }

    #[test]
    fn a_hero_away_from_the_fortress_gets_the_item_when_the_mana_pays_the_move() -> TestResult {
        let mut w = game()?;
        let ai = Ai::new(P1);
        set_gold(&mut w, P1, 100_000)?;
        let hero = w.hire_hero(P1, 1).map_err(|e| format!("{e:?}"))?;
        let fortress = PlayerView::new(&w, P1).fortress().ok_or("a fortress")?;
        let away = crate::testworld::at(fortress.x + 3, fortress.y);
        w.cheat_move_unit(hero, away);
        let item = sword(&w)?;
        w.give_item(P1, item);
        let mana = |w: &mut Game, m: i32| -> TestResult {
            w.treasury_mut(P1).ok_or("a treasury")?.mana = f4x_def::half_int::HalfInt::from_int(m);
            Ok(())
        };
        mana(&mut w, FAR_RESERVE - 1)?;
        equip(&ai, &mut w, &mut Vec::new());
        assert!(
            w.items().carried(hero).is_empty(),
            "too little mana: it waits"
        );
        mana(&mut w, FAR_RESERVE + 100)?;
        equip(&ai, &mut w, &mut Vec::new());
        assert!(
            !w.items().carried(hero).is_empty(),
            "the far hero takes the item"
        );
        Ok(())
    }

    /// A copy of a sword of the shipped content.
    fn sword(w: &World) -> Result<f4x_core::item::Item, &'static str> {
        w.content()
            .rules
            .items
            .iter()
            .map(|(_, def)| def)
            .find(|def| def.kind == f4x_def::rules::ItemKind::Sword)
            .map(f4x_core::item::Item::from_def)
            .ok_or("the shipped content has a sword")
    }

    #[test]
    fn a_computer_wizard_with_a_hero_and_the_gold_buys_a_resold_item_and_one_without_does_not(
    ) -> TestResult {
        use f4x_core::item::armory::Spot;
        let mut w = game()?;
        let seller = PlayerId(0);
        let sell = |w: &mut Game| -> Result<Vec<Event>, &'static str> {
            let item = sword(w)?;
            w.give_item(seller, item);
            let mut events = w.apply(Command::SellItem {
                player: seller,
                from: Spot::Vault(0),
            });
            events.extend(answer_resales(w));
            Ok(events)
        };
        // No hero to carry it: turned down, and the item is gone.
        set_gold(&mut w, P1, 5000)?;
        let events = sell(&mut w)?;
        assert!(w.offers(P1).is_empty());
        assert!(!events.iter().any(|e| matches!(e, Event::ItemMade { .. })));
        assert_eq!(w.items().armory(P1).vault.iter().flatten().count(), 0);
        // With a hero and gold it buys at the price the seller was paid.
        offer_hero(&mut w);
        let mut hired = Vec::new();
        answer(&Ai::new(P1), &mut w, &mut hired);
        assert!(!w.units().hero_units(P1).is_empty(), "{hired:?}");
        let gold = |w: &mut Game, player| w.treasury_mut(player).map(|t| t.gold);
        let (buyer_before, seller_before) = (gold(&mut w, P1), gold(&mut w, seller));
        let events = sell(&mut w)?;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemMade { player: P1, .. })),
            "{events:?}"
        );
        let paid = gold(&mut w, seller).zip(seller_before).map(|(a, b)| a - b);
        assert!(paid.is_some_and(|p| p > 0), "the seller was paid: {paid:?}");
        assert_eq!(
            gold(&mut w, P1),
            buyer_before.zip(paid).map(|(g, p)| g - p),
            "the buyer pays what the seller was paid"
        );
        assert_eq!(w.items().armory(P1).vault.iter().flatten().count(), 1);
        Ok(())
    }

    /// The shipped item numbered `num`.
    fn shipped_item(num: u16) -> f4x_core::item::Item {
        let content = crate::testworld::shipped();
        let id = content
            .rules
            .items
            .id_numbered(num)
            .expect("the content has the item");
        f4x_core::item::Item::from_def(content.rules.items.get(id))
    }

    /// A wizard with a hired hero of this kind (which slots it has): the offer comes from the
    /// game's random stream, so the stream is moved on until the hero is the kind asked for.
    fn with_hero_of(
        kind: f4x_def::rules::HeroKind,
    ) -> Result<(Game, Ai, hecs::Entity), Box<dyn std::error::Error>> {
        for burn in 0..200 {
            let mut w = game()?;
            for _ in 0..burn {
                w.roll(2);
            }
            set_gold(&mut w, P1, 100_000)?;
            offer_hero(&mut w);
            let ai = Ai::new(P1);
            answer(&ai, &mut w, &mut Vec::new());
            let Some(&hero) = PlayerView::new(&w, P1).my_heroes().first() else {
                continue;
            };
            let of_kind = w
                .units()
                .unit_def(hero)
                .and_then(|d| d.hero.as_ref())
                .is_some_and(|h| h.kind == kind);
            if of_kind {
                return Ok((w, ai, hero));
            }
        }
        Err("no hero of that kind was offered".into())
    }

    #[test]
    fn a_hero_gives_up_an_item_for_one_that_suits_it_a_good_deal_better() -> TestResult {
        // A sword needs a warrior's slots; which hero is offered depends on the random stream.
        let (mut w, ai, hero) = with_hero_of(f4x_def::rules::HeroKind::Warrior)?;
        let (base, little, much) = (
            shipped_item(1 /* sword_of_mallana */),
            shipped_item(160 /* sword_of_cannae, as good */),
            shipped_item(163 /* the_revenge_of_orculs */),
        );
        let (base_cost, much_cost) = (base.cost, much.cost);
        w.give_item(P1, base);
        equip(&ai, &mut w, &mut Vec::new());
        assert_eq!(w.items().carried(hero).len(), 1);
        // An equal sword is no reason to change.
        w.give_item(P1, little);
        equip(&ai, &mut w, &mut Vec::new());
        assert_eq!(w.items().carried(hero)[0].1.cost, base_cost);
        // A good deal better is, and the old one goes back to the vault.
        w.give_item(P1, much);
        equip(&ai, &mut w, &mut Vec::new());
        assert_eq!(w.items().carried(hero)[0].1.cost, much_cost);
        assert!(w.items().armory(P1).items().any(|i| i.cost == base_cost));
        Ok(())
    }

    #[test]
    fn items_no_hero_can_carry_are_scrapped_for_mana_but_two_are_kept() -> TestResult {
        let (mut w, ai, _) = with_hero_of(f4x_def::rules::HeroKind::Warrior)?;
        // The hired hero is a warrior: its slots take swords, armour and trinkets, not staves.
        let staves: Vec<_> = [40, 41, 128, 204].map(shipped_item).into();
        let mut costs: Vec<u32> = staves.iter().map(|i| i.cost).collect();
        costs.sort();
        for staff in staves {
            w.give_item(P1, staff);
        }
        let mana = w.player(P1).ok_or("a player")?.treasury().mana;
        equip(&ai, &mut w, &mut Vec::new());
        let left: Vec<u32> = w.items().armory(P1).items().map(|i| i.cost).collect();
        assert_eq!(left.len(), 2, "{left:?}");
        assert!(
            left.iter().all(|&c| c >= costs[2]),
            "the cheapest go first: {left:?}"
        );
        assert!(w.player(P1).ok_or("a player")?.treasury().mana > mana);
        Ok(())
    }
}
