//! Class 9, items: the armory (the vault and the award list), the item screen's moves, the slots of a hero,
//! the price of carrying an item to a hero far away, the anvil and the sale.

use crate::support::{around, worlds};
use f4x_core::game::{Command, Event, Game, Rejection};
use f4x_core::item::armory::{ItemError, ItemSpot, Placed, Spot};
use f4x_core::item::Item;
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_def::half_int::HalfInt;

/// The shipped items, as items.
fn items(g: &Game) -> Vec<Item> {
    g.content()
        .rules
        .items
        .defs()
        .iter()
        .map(Item::from_def)
        .collect()
}

fn mana(g: &Game, p: PlayerId) -> HalfInt {
    g.player(p).expect("a player").treasury().mana
}

fn gold(g: &Game, p: PlayerId) -> HalfInt {
    g.player(p).expect("a player").treasury().gold
}

fn armory_size(g: &Game, p: PlayerId) -> usize {
    g.items().armory(p).items().count()
}

fn item_error(events: &[Event]) -> Option<ItemError> {
    events.iter().find_map(|e| match e {
        Event::Rejected(Rejection::Item(why)) => Some(*why),
        _ => None,
    })
}

fn move_item(g: &mut Game, p: PlayerId, from: ItemSpot, to: ItemSpot, pay: bool) -> Vec<Event> {
    g.apply(Command::MoveItem {
        player: p,
        from,
        to,
        pay,
    })
}

#[test]
fn items_given_to_a_wizard_fill_the_vault_then_the_award_list_and_none_is_made_or_lost_on_the_way()
{
    for (name, mut g) in worlds() {
        let p = g.wizards()[0];
        let all = items(&g);
        let (mut vaulted, mut awaiting, mut lost) = (0usize, 0usize, 0usize);
        let mut last_vault = None;
        for item in all.iter().cycle().take(40) {
            match g.give_item(p, item.clone()) {
                Placed::Vault(i) => {
                    assert!(
                        awaiting == 0 && lost == 0,
                        "{name}: the vault after the awards"
                    );
                    assert!(
                        last_vault.is_none_or(|l| i > l),
                        "{name}: slot {i} after {last_vault:?}"
                    );
                    last_vault = Some(i);
                    vaulted += 1;
                }
                Placed::Awards => {
                    assert_eq!(lost, 0, "{name}: an award after a loss");
                    awaiting += 1;
                }
                Placed::Lost => lost += 1,
            }
        }
        assert_eq!(vaulted + awaiting + lost, 40, "{name}");
        assert_eq!(
            armory_size(&g, p),
            vaulted + awaiting,
            "{name}: an item was lost or made"
        );
        assert!(vaulted > 0, "{name}");
    }
}

#[test]
fn an_item_moves_between_the_vault_slots_and_a_refusal_changes_nothing() {
    for (name, mut g) in worlds() {
        let p = g.wizards()[0];
        let all = items(&g);
        g.give_item(p, all[0].clone());
        g.give_item(p, all[1].clone());
        let before = crate::support::text(&g);
        // From and to the same slot, nothing changes; from an empty slot, a slot that is not there, into the award list.
        assert_eq!(
            item_error(&move_item(
                &mut g,
                p,
                ItemSpot::Vault(0),
                ItemSpot::Vault(0),
                false
            )),
            None,
            "{name}"
        );
        assert_eq!(
            item_error(&move_item(
                &mut g,
                p,
                ItemSpot::Vault(3),
                ItemSpot::Vault(0),
                false
            )),
            Some(ItemError::Empty),
            "{name}"
        );
        assert_eq!(
            item_error(&move_item(
                &mut g,
                p,
                ItemSpot::Vault(0),
                ItemSpot::Award(0),
                false
            )),
            Some(ItemError::WrongSlot),
            "{name}: the award list takes nothing"
        );
        assert_eq!(
            item_error(&move_item(
                &mut g,
                p,
                ItemSpot::Vault(0),
                ItemSpot::Hero(hecs::Entity::DANGLING, 0),
                false
            )),
            Some(ItemError::NoSuchPlace),
            "{name}"
        );
        assert_eq!(
            crate::support::text(&g),
            before,
            "{name}: a refusal changed the game"
        );
        // Two items in two slots swap.
        let (a, b) = (
            g.items().armory(p).vault[0].clone(),
            g.items().armory(p).vault[1].clone(),
        );
        assert_eq!(
            item_error(&move_item(
                &mut g,
                p,
                ItemSpot::Vault(0),
                ItemSpot::Vault(1),
                false
            )),
            None,
            "{name}"
        );
        let now = g.items().armory(p);
        assert_eq!(
            (now.vault[0].clone(), now.vault[1].clone()),
            (b, a),
            "{name}"
        );
    }
}

/// A free hero in the army of `p` at `pos`, and the kind's slots.
fn hero_at(g: &mut Game, p: PlayerId, pos: Position) -> Option<hecs::Entity> {
    let kinds = g.content().rules.hero_templates.len();
    (0..kinds).find_map(|k| g.spawn_hero(p, pos, k).ok())
}

#[test]
fn a_heros_slot_takes_the_items_its_kind_fits_and_refuses_the_others() {
    let stride = if f4x_def::testdir::deep() { 1 } else { 4 };
    let (mut taken, mut refused) = (0, 0);
    for (name, mut g) in worlds() {
        let p = g.wizards()[0];
        let fortress = g
            .wizard(p)
            .expect("a wizard")
            .fortress()
            .expect("a fortress");
        let Some(hero) = hero_at(&mut g, p, fortress) else {
            continue;
        };
        let slots = g
            .units()
            .unit_def(hero)
            .and_then(|d| d.hero.as_ref())
            .expect("a hero")
            .kind
            .slots();
        for item in items(&g).into_iter().step_by(stride) {
            for (i, slot) in slots.iter().enumerate() {
                let from = match g.give_item(p, item.clone()) {
                    Placed::Vault(i) => ItemSpot::Vault(i),
                    Placed::Awards => ItemSpot::Award(0),
                    Placed::Lost => continue,
                };
                let events = move_item(&mut g, p, from, ItemSpot::Hero(hero, i), false);
                let error = item_error(&events);
                if error.is_none() {
                    // Back where it came from, so the next try starts from the same armory.
                    move_item(
                        &mut g,
                        p,
                        ItemSpot::Hero(hero, i),
                        ItemSpot::Vault(0),
                        false,
                    );
                }
                let spot = match from {
                    ItemSpot::Vault(_) => Spot::Vault(0),
                    _ => Spot::Award(0),
                };
                let _ = g.scrap_item(p, spot);
                match (slot.fits(item.kind), error) {
                    (true, None) => taken += 1,
                    (false, Some(ItemError::WrongSlot)) => refused += 1,
                    (fits, other) => panic!(
                        "{name}: {:?} in slot {i} ({slot:?}): fits {fits}, got {other:?}",
                        item.kind
                    ),
                }
            }
        }
    }
    assert!(taken > 0 && refused > 0, "{taken} taken, {refused} refused");
}

/// A hero of `p` on a square next to their fortress, with a sword-slot item in the vault to give it.
fn far_hero(g: &mut Game, p: PlayerId) -> Option<(hecs::Entity, usize)> {
    let fortress = g.wizard(p)?.fortress()?;
    let pos = around(g, fortress).into_iter().find(|&q| q != fortress)?;
    let hero = hero_at(g, p, pos)?;
    let slots = g.units().unit_def(hero)?.hero.as_ref()?.kind.slots();
    let slot = 0;
    let kind = items(g).into_iter().find(|i| slots[slot].fits(i.kind))?;
    g.give_item(p, kind);
    Some((hero, slot))
}

#[test]
fn carrying_an_item_to_a_hero_away_from_the_fortress_costs_mana_only_when_the_wizard_agrees() {
    let mut tried = 0;
    for (name, mut g) in worlds() {
        let p = g.wizards()[0];
        let Some((hero, slot)) = far_hero(&mut g, p) else {
            continue;
        };
        tried += 1;
        let price = g.content().rules.config.item.transfer_mana;
        let to = ItemSpot::Hero(hero, slot);
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(price + 50);
        let before = mana(&g, p);
        assert_eq!(
            item_error(&move_item(&mut g, p, ItemSpot::Vault(0), to, false)),
            Some(ItemError::Transfer),
            "{name}"
        );
        assert_eq!(mana(&g, p), before, "{name}: a refusal cost mana");
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(price - 1);
        assert_eq!(
            item_error(&move_item(&mut g, p, ItemSpot::Vault(0), to, true)),
            Some(ItemError::NoMana),
            "{name}"
        );
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(price + 50);
        assert_eq!(
            item_error(&move_item(&mut g, p, ItemSpot::Vault(0), to, true)),
            None,
            "{name}"
        );
        assert_eq!(
            mana(&g, p),
            HalfInt::from_int(50),
            "{name}: the price is the rule's"
        );
        assert!(g.items().hero_item_entity(hero, slot).is_some(), "{name}");
        // Back into the vault is free.
        assert_eq!(
            item_error(&move_item(&mut g, p, to, ItemSpot::Vault(0), false)),
            None,
            "{name}"
        );
        assert_eq!(mana(&g, p), HalfInt::from_int(50), "{name}");
    }
    assert!(tried > 0, "no world had a hero to try");
}

#[test]
fn the_anvil_pays_half_the_value_in_mana_and_the_item_is_gone() {
    for (name, mut g) in worlds() {
        let p = g.wizards()[0];
        let item = items(&g)
            .into_iter()
            .find(|i| i.anvil_mana() > 0)
            .expect("a costly item");
        g.give_item(p, item.clone());
        let (m, n) = (mana(&g, p), armory_size(&g, p));
        let events = g.apply(Command::ScrapItem {
            player: p,
            from: Spot::Vault(0),
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemScrapped { .. })),
            "{name}: {events:?}"
        );
        assert_eq!(mana(&g, p), m + item.anvil_mana(), "{name}");
        assert_eq!(armory_size(&g, p), n - 1, "{name}");
        let again = g.apply(Command::ScrapItem {
            player: p,
            from: Spot::Vault(0),
        });
        assert_eq!(item_error(&again), Some(ItemError::Empty), "{name}");
        assert_eq!(
            mana(&g, p),
            m + item.anvil_mana(),
            "{name}: an empty slot paid"
        );
    }
}

#[test]
fn a_sale_pays_gold_at_once_and_the_item_is_offered_on_or_lost() {
    for (name, mut g) in worlds() {
        let p = g.wizards()[0];
        let item = items(&g)
            .into_iter()
            .find(|i| i.anvil_mana() > 0)
            .expect("a costly item");
        g.give_item(p, item.clone());
        let (gld, n) = (gold(&g, p), armory_size(&g, p));
        let events = g.apply(Command::SellItem {
            player: p,
            from: Spot::Vault(0),
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemSold { player, gold, .. }
                if *player == p && *gold == item.anvil_mana())),
            "{name}: {events:?}"
        );
        assert_eq!(gold(&g, p), gld + item.anvil_mana(), "{name}");
        assert_eq!(
            armory_size(&g, p),
            n - 1,
            "{name}: the item stayed with the seller"
        );
        let again = g.apply(Command::SellItem {
            player: p,
            from: Spot::Vault(0),
        });
        assert_eq!(item_error(&again), Some(ItemError::Empty), "{name}");
    }
}
