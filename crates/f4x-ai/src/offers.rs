//! The offers made at the turn change: a hero for hire and a company of mercenaries (the
//! merchant only calls on a human wizard). The rules are the original's `AI_Accept_Hero` and
//! `AI_Accept_Mercenaries`, in `f4x_core::offers`.

use f4x_core::items::armory::ItemSpot;
use f4x_core::offers::{ai_accepts_hero, ai_accepts_item, ai_accepts_mercenaries, Offer};
use f4x_core::player_view::PlayerView;
use f4x_core::{Command, Event, Game};

use crate::Ai;

/// Put the items in the vault and on the award list on the heroes at the fortress that have a
/// free slot for them (moving an item to a hero elsewhere costs mana, so those wait). The first
/// fit is taken; a better choice by the item's worth is task 222.
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
                let Some(armory) = view.armory() else { return };
                let vault = armory
                    .vault
                    .iter()
                    .position(|i| i.as_ref().is_some_and(|i| kind.fits(i.kind)))
                    .map(ItemSpot::Vault);
                vault.or_else(|| {
                    armory
                        .awards
                        .iter()
                        .position(|i| kind.fits(i.kind))
                        .map(ItemSpot::Award)
                })
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
            let (Some(offer), Some(player)) = (view.offers().get(index), view.player()) else {
                continue;
            };
            (offer.clone(), player.gold, view.turn())
        };
        let accept = match &offer {
            Offer::Hero { price, .. } => ai_accepts_hero(*price, gold),
            Offer::Mercenaries { price, .. } => ai_accepts_mercenaries(turn, *price, gold),
            Offer::Item { price, .. } => ai_accepts_item(
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

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::city::PlayerId;
    use f4x_core::offers::OfferKind;
    use f4x_core::setup::{new_game, NewGame};

    const P1: PlayerId = PlayerId(1);

    fn game() -> Game {
        let content = crate::testworld::content();
        new_game(
            &content,
            &NewGame {
                seed: 7,
                ..NewGame::default()
            },
        )
        .expect("a new game")
    }

    /// Force a hero offer; with little fame the pick can come up empty, so try again.
    fn offer_hero(g: &mut Game) {
        // A hero is only offered to a wizard whose fame exceeds its cost's step over 100.
        g.player_mut(P1).unwrap().fame = 100;
        assert!(
            (0..500).any(|_| g.cheat_offer(P1, OfferKind::Hero).is_some()),
            "no hero was ever offered"
        );
    }

    #[test]
    fn a_computer_wizard_hires_the_hero_it_can_pay_for_and_turns_down_one_it_cannot() {
        let mut g = game();
        let ai = Ai::new(P1);
        g.player_mut(P1).unwrap().gold = 5000;
        offer_hero(&mut g);
        let mut events = Vec::new();
        answer(&ai, &mut g, &mut events);
        assert!(
            events.iter().any(|e| matches!(e, Event::HeroJoined { .. })),
            "{events:?}"
        );
        assert!(g.offers(P1).is_empty());
        // With nothing to pay with, the offer is turned down, not left lying.
        g.player_mut(P1).unwrap().gold = 0;
        offer_hero(&mut g);
        let mut events = Vec::new();
        answer(&ai, &mut g, &mut events);
        assert!(!events.iter().any(|e| matches!(e, Event::HeroJoined { .. })));
        assert!(g.offers(P1).is_empty());
    }

    #[test]
    fn mercenaries_wait_for_turn_50() {
        let mut g = game();
        let ai = Ai::new(P1);
        g.player_mut(P1).unwrap().gold = 100_000;
        g.turn = 40;
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
        g.turn = 50;
        g.cheat_offer(P1, OfferKind::Mercenaries);
        let mut events = Vec::new();
        answer(&ai, &mut g, &mut events);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::MercenariesHired { .. })));
    }

    #[test]
    fn a_computer_wizard_buys_the_merchants_item_once_it_has_a_hero_and_puts_it_on_the_hero() {
        let mut g = game();
        let ai = Ai::new(P1);
        g.player_mut(P1).unwrap().gold = 100_000;
        // Without a hero to carry it, the item is turned down.
        g.cheat_offer(P1, OfferKind::Item).expect("an item");
        let mut events = Vec::new();
        answer(&ai, &mut g, &mut events);
        assert!(g.offers(P1).is_empty());
        assert!(g.items().armory(P1).unwrap().items().next().is_none());
        // With one, it is bought and the hero at the fortress takes what fits its slots.
        offer_hero(&mut g);
        answer(&ai, &mut g, &mut events);
        let hero = PlayerView::new(&g, P1).my_heroes()[0];
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
    }
}
