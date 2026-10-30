//! The offers made at the turn change: a hero for hire and a company of mercenaries (the
//! merchant only calls on a human wizard). The rules are the original's `AI_Accept_Hero` and
//! `AI_Accept_Mercenaries`, in `f4x_core::player::offer`.

use f4x_core::game::{Command, Event, Game};
use f4x_core::item::armory::ItemSpot;
use f4x_core::player::offer::{ai_accepts_hero, ai_accepts_item, ai_accepts_mercenaries, Offer};
use f4x_core::player::view::PlayerView;

use crate::Ai;

/// Put the items in the vault and on the award list on the heroes at the fortress that have a
/// free slot for them (moving an item to a hero elsewhere costs mana, so those wait). The one worth
/// most of those that fit is taken.
pub fn equip(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let (heroes, fortress) = {
        let view = PlayerView::new(game, ai.player);
        (view.my_heroes(), view.fortress())
    };
    for hero in heroes {
        let free = {
            let view = PlayerView::new(game, ai.player);
            if view.unit_position(hero) != fortress {
                continue;
            }
            view.hero_free_slots(hero)
        };
        for (slot, kind) in free {
            let from = {
                let view = PlayerView::new(game, ai.player);
                let armory = view.armory();
                // The fitting item worth most.
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
                vault
                    .chain(awards)
                    .filter(|(_, item)| kind.fits(item.kind))
                    .max_by_key(|(_, item)| item.cost)
                    .map(|(spot, _)| spot)
            };
            if let Some(from) = from {
                ai.send(
                    game,
                    Command::MoveItem {
                        player: ai.player,
                        from,
                        to: ItemSpot::Hero(hero, slot),
                        pay: false,
                    },
                    events,
                );
            }
        }
    }
}

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
            (offer, view.treasury().gold, view.turn())
        };
        let accept = match &offer {
            Offer::Hero { price, .. } => ai_accepts_hero(*price, gold),
            Offer::Mercenaries { price, .. } => ai_accepts_mercenaries(turn, *price, gold),
            Offer::Item { price, .. } | Offer::Resale { price, .. } => ai_accepts_item(
                *price,
                gold,
                PlayerView::new(game, ai.player).my_heroes().len(),
            ),
        };
        let command = if accept {
            Command::AcceptOffer {
                player: ai.player,
                index,
                name: None,
            }
        } else {
            Command::RejectOffer {
                player: ai.player,
                index,
            }
        };
        ai.send(game, command, events);
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
            .filter(|&p| Some(p) != game.human() && !game.is_neutral_player(p))
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
            let (index, price, heroes, gold) = {
                let view = PlayerView::new(game, player);
                let Some(index) = view
                    .offers()
                    .iter()
                    .position(|o| matches!(o, Offer::Resale { .. }))
                else {
                    continue;
                };
                let price = view.offers()[index].price();
                (index, price, view.my_heroes().len(), view.treasury().gold)
            };
            let command = if ai_accepts_item(price, gold, heroes) {
                Command::AcceptOffer {
                    player,
                    index,
                    name: None,
                }
            } else {
                Command::RejectOffer { player, index }
            };
            ai.send(game, command, &mut events);
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::game::setup::{new_game, NewGame};
    use f4x_core::player::offer::{OfferKind, AI_FIRST_MERCENARY_TURN};
    use f4x_core::player::PlayerId;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const P1: PlayerId = PlayerId(1);

    fn game() -> Result<Game, Box<dyn std::error::Error>> {
        let content = crate::testworld::content();
        Ok(new_game(
            &content,
            &NewGame {
                seed: 7,
                ..NewGame::default()
            },
        )?)
    }

    fn set_gold(g: &mut Game, player: PlayerId, gold: i32) -> TestResult {
        g.treasury_mut(player)
            .ok_or("the player has a treasury")?
            .gold = gold;
        Ok(())
    }

    /// Force a hero offer. A hero is only offered to a wizard whose fame exceeds its cost's step
    /// over 100, and the pick can come up empty, so it tries again.
    fn offer_hero(g: &mut Game) {
        g.set_fame(P1, 100);
        assert!(
            (0..500).any(|_| g.cheat_offer(P1, OfferKind::Hero).is_some()),
            "no hero was ever offered"
        );
    }

    #[test]
    fn a_computer_wizard_hires_the_hero_it_can_pay_for_and_turns_down_one_it_cannot() -> TestResult
    {
        let mut g = game()?;
        let ai = Ai::new(P1);
        set_gold(&mut g, P1, 5000)?;
        offer_hero(&mut g);
        let mut events = Vec::new();
        answer(&ai, &mut g, &mut events);
        assert!(
            events.iter().any(|e| matches!(e, Event::HeroJoined { .. })),
            "{events:?}"
        );
        assert!(g.offers(P1).is_empty());
        // With nothing to pay with, the offer is turned down, not left lying.
        set_gold(&mut g, P1, 0)?;
        offer_hero(&mut g);
        let mut events = Vec::new();
        answer(&ai, &mut g, &mut events);
        assert!(!events.iter().any(|e| matches!(e, Event::HeroJoined { .. })));
        assert!(g.offers(P1).is_empty());
        Ok(())
    }

    #[test]
    fn mercenaries_wait_for_the_first_mercenary_turn() -> TestResult {
        let mut g = game()?;
        let ai = Ai::new(P1);
        set_gold(&mut g, P1, 100_000)?;
        g.turn = AI_FIRST_MERCENARY_TURN - 10;
        let mut got = 0;
        for _ in 0..50 {
            if g.cheat_offer(P1, OfferKind::Mercenaries).is_some() {
                got += 1;
            }
        }
        assert!(
            got > 0,
            "the wizard needs a unit on a plane to be offered any"
        );
        let mut events = Vec::new();
        answer(&ai, &mut g, &mut events);
        assert!(!events
            .iter()
            .any(|e| matches!(e, Event::MercenariesHired { .. })));
        assert!(g.offers(P1).is_empty());
        g.turn = AI_FIRST_MERCENARY_TURN;
        g.cheat_offer(P1, OfferKind::Mercenaries);
        let mut events = Vec::new();
        answer(&ai, &mut g, &mut events);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::MercenariesHired { .. })));
        Ok(())
    }

    #[test]
    fn a_computer_wizard_buys_the_merchants_item_once_it_has_a_hero_and_puts_it_on_the_hero(
    ) -> TestResult {
        let mut g = game()?;
        let ai = Ai::new(P1);
        set_gold(&mut g, P1, 100_000)?;
        // Without a hero to carry it, the item is turned down.
        g.cheat_offer(P1, OfferKind::Item)
            .ok_or("an item is offered")?;
        let mut events = Vec::new();
        answer(&ai, &mut g, &mut events);
        assert!(g.offers(P1).is_empty());
        assert!(g.items().armory(P1).items().next().is_none());
        // With one, it is bought and the hero at the fortress takes what fits its slots.
        offer_hero(&mut g);
        answer(&ai, &mut g, &mut events);
        let hero = *PlayerView::new(&g, P1)
            .my_heroes()
            .first()
            .ok_or("the wizard hired a hero")?;
        let mut equipped = false;
        for _ in 0..40 {
            if g.cheat_offer(P1, OfferKind::Item).is_none() {
                continue;
            }
            answer(&ai, &mut g, &mut events);
            equip(&ai, &mut g, &mut events);
            if !g.items().carried(hero).is_empty() {
                equipped = true;
                break;
            }
        }
        assert!(equipped, "the hero never got an item");
        Ok(())
    }

    /// A copy of a sword of the shipped content.
    fn sword(g: &Game) -> Result<f4x_core::item::Item, &'static str> {
        g.content()
            .rules
            .items
            .iter()
            .map(|(_, def)| def)
            .find(|def| def.kind == f4x_core::rules::ItemKind::Sword)
            .map(f4x_core::item::Item::from_def)
            .ok_or("the shipped content has a sword")
    }

    #[test]
    fn a_computer_wizard_with_a_hero_and_the_gold_buys_a_resold_item_and_one_without_does_not(
    ) -> TestResult {
        use f4x_core::item::armory::Spot;
        let mut g = game()?;
        let seller = PlayerId(0);
        let sell = |g: &mut Game| -> Result<Vec<Event>, &'static str> {
            let item = sword(g)?;
            g.give_item(seller, item);
            let mut events = g.apply(Command::SellItem {
                player: seller,
                from: Spot::Vault(0),
            });
            events.extend(answer_resales(g));
            Ok(events)
        };
        // No hero to carry it: turned down, and the item is gone.
        set_gold(&mut g, P1, 5000)?;
        let events = sell(&mut g)?;
        assert!(g.offers(P1).is_empty());
        assert!(!events.iter().any(|e| matches!(e, Event::ItemMade { .. })));
        assert_eq!(g.items().armory(P1).vault.iter().flatten().count(), 0);
        // With a hero and gold it buys at the price the seller was paid.
        offer_hero(&mut g);
        let mut hired = Vec::new();
        answer(&Ai::new(P1), &mut g, &mut hired);
        assert!(!g.units().hero_units(P1).is_empty(), "{hired:?}");
        let gold = |g: &mut Game, player| g.treasury_mut(player).map(|t| t.gold);
        let (buyer_before, seller_before) = (gold(&mut g, P1), gold(&mut g, seller));
        let events = sell(&mut g)?;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemMade { player: P1, .. })),
            "{events:?}"
        );
        let paid = gold(&mut g, seller).zip(seller_before).map(|(a, b)| a - b);
        assert!(paid.is_some_and(|p| p > 0), "the seller was paid: {paid:?}");
        assert_eq!(
            gold(&mut g, P1),
            buyer_before.zip(paid).map(|(g, p)| g - p),
            "the buyer pays what the seller was paid"
        );
        assert_eq!(g.items().armory(P1).vault.iter().flatten().count(), 1);
        Ok(())
    }
}
