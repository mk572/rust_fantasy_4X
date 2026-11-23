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

// ---- crafting: Enchant Item and Create Artifact ----

use f4x_core::item::crafting::{self, Craft, CraftError, CraftingError, ItemDesign};
use f4x_def::rules::ItemKind;

const KINDS: [ItemKind; 9] = [
    ItemKind::Sword,
    ItemKind::Mace,
    ItemKind::Axe,
    ItemKind::Bow,
    ItemKind::Staff,
    ItemKind::Wand,
    ItemKind::Misc,
    ItemKind::Shield,
    ItemKind::Chainmail,
];

fn design(kind: ItemKind, powers: Vec<f4x_def::rules::ItemPowerId>) -> ItemDesign {
    ItemDesign {
        name: String::new(),
        kind,
        icon: 0,
        powers,
        charges: None,
    }
}

/// The powers `crafting::available_powers` lists, with their ids in the registry.
fn open_powers<'a>(
    kind: ItemKind,
    craft: Craft,
    magic: &f4x_core::wizard::entity::Wizard<'_>,
    rules: &'a f4x_def::rules::Rules,
) -> Vec<(
    f4x_def::rules::ItemPowerId,
    &'a f4x_def::rules::ItemPowerDef,
)> {
    let names: Vec<_> = crafting::available_powers(kind, craft, magic, rules)
        .into_iter()
        .map(|d| d.id.clone())
        .collect();
    rules
        .item_powers
        .iter()
        .filter(|(_, d)| names.contains(&d.id))
        .collect()
}

/// A wizard that knows both spells, with books to spare is not made: `p` as the world gave them.
fn maker(g: &mut Game) -> PlayerId {
    let p = g.wizards()[0];
    for c in [Craft::EnchantItem, Craft::CreateArtifact] {
        g.cheat_learn_spell(p, c.spell());
    }
    p
}

#[test]
fn a_design_is_checked_power_by_power_and_a_good_one_builds_an_item_worth_its_cost() {
    let (mut good, mut bad) = (0, 0);
    for (name, mut g) in worlds() {
        let p = maker(&mut g);
        let rules = g.content().rules.clone();
        let magic = g.wizard(p).expect("a wizard");
        for kind in KINDS {
            for craft in [Craft::EnchantItem, Craft::CreateArtifact] {
                let open: Vec<_> = open_powers(kind, craft, &magic, &rules)
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect();
                for (id, power) in rules.item_powers.iter() {
                    let one = design(kind, vec![id]);
                    let result = crafting::check(&one, craft, &magic, &rules);
                    if power.holds_spell {
                        assert_eq!(result, Err(CraftError::UnknownPower), "{name}: {power:?}");
                    } else if open.contains(&id) {
                        assert_eq!(result, Ok(()), "{name}: {kind:?} {craft:?} {id:?}");
                        let item = crafting::build(&one, &rules);
                        assert_eq!(item.cost, crafting::cost(&one, &rules), "{name}");
                        assert!(
                            item.cost > rules.config.item.base_cost(kind),
                            "{name}: a power adds nothing"
                        );
                        assert_eq!(item.kind, kind, "{name}");
                        good += 1;
                    } else {
                        let why = result.expect_err("a closed power is refused");
                        assert!(
                            matches!(
                                why,
                                CraftError::WrongKind
                                    | CraftError::TooCostly
                                    | CraftError::NeedsBooks
                            ),
                            "{name}: {why:?}"
                        );
                        bad += 1;
                    }
                }
            }
        }
    }
    assert!(good > 0 && bad > 0, "{good} good, {bad} bad");
}

#[test]
fn enchant_item_refuses_what_create_artifact_takes_and_the_other_rules_of_a_design() {
    let (mut dear, mut grouped) = (0, 0);
    for (name, mut g) in worlds() {
        let p = maker(&mut g);
        let rules = g.content().rules.clone();
        let magic = g.wizard(p).expect("a wizard");
        let check = |d: &ItemDesign, c| crafting::check(d, c, &magic, &rules);
        for kind in KINDS {
            assert_eq!(
                check(&design(kind, vec![]), Craft::CreateArtifact),
                Err(CraftError::NoPowers),
                "{name}"
            );
            let artifact = open_powers(kind, Craft::CreateArtifact, &magic, &rules);
            let enchant = open_powers(kind, Craft::EnchantItem, &magic, &rules);
            // A power that costs too much for Enchant Item.
            if let Some(power) = artifact
                .iter()
                .find(|(id, _)| !enchant.iter().any(|(e, _)| e == id))
            {
                let d = design(kind, vec![power.0]);
                assert_eq!(
                    check(&d, Craft::EnchantItem),
                    Err(CraftError::TooCostly),
                    "{name}"
                );
                assert_eq!(check(&d, Craft::CreateArtifact), Ok(()), "{name}");
                dear += 1;
            }
            let Some(&(first, first_def)) = artifact.first() else {
                continue;
            };
            assert_eq!(
                check(&design(kind, vec![first, first]), Craft::CreateArtifact),
                Err(CraftError::Repeated),
                "{name}"
            );
            // Two powers of one group (but the free group).
            if let Some(second) = artifact.iter().find(|d| {
                d.0 != first
                    && d.1.group == first_def.group
                    && first_def.group != rules.config.item.free_group
            }) {
                assert_eq!(
                    check(&design(kind, vec![first, second.0]), Craft::CreateArtifact),
                    Err(CraftError::SameGroup),
                    "{name}"
                );
                grouped += 1;
            }
            // More powers than an item holds.
            let many: Vec<_> = artifact.iter().map(|(id, _)| *id).collect();
            let limit = rules.config.item.max_powers;
            if many.len() > limit {
                assert_eq!(
                    check(
                        &design(kind, many[..=limit].to_vec()),
                        Craft::CreateArtifact
                    ),
                    Err(CraftError::TooManyPowers),
                    "{name}"
                );
            }
        }
    }
    assert!(dear > 0 && grouped > 0, "{dear} dear, {grouped} grouped");
}

#[test]
fn a_wizard_with_the_skill_and_the_mana_makes_the_item_at_once_and_pays_its_price() {
    let mut made = 0;
    for (name, mut g) in worlds() {
        let p = maker(&mut g);
        let rules = g.content().rules.clone();
        let kind = ItemKind::Sword;
        let Some(power) = open_powers(
            kind,
            Craft::EnchantItem,
            &g.wizard(p).expect("a wizard"),
            &rules,
        )
        .first()
        .map(|(id, _)| *id) else {
            continue;
        };
        let d = design(kind, vec![power]);
        g.scenario_set_casting_skill(p, 600);
        g.wizard_mut(p).expect("a wizard").add_skill_left(5_000);
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
        let (m, n) = (mana(&g, p), armory_size(&g, p));
        let events = g.apply(Command::CraftItem {
            player: p,
            craft: Craft::EnchantItem,
            design: d.clone(),
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ItemMade { player, .. } if *player == p)),
            "{name}: {events:?}"
        );
        assert!(mana(&g, p) < m, "{name}: free");
        assert_eq!(armory_size(&g, p), n + 1, "{name}");
        assert!(
            g.wizard(p).expect("a wizard").casting().is_none(),
            "{name}: still casting"
        );
        made += 1;
    }
    assert!(made > 0, "no world made an item");
}

#[test]
fn a_wizard_who_cannot_pay_at_once_pays_over_turns_and_the_item_comes_when_it_is_paid() {
    let mut slow = 0;
    for (name, mut g) in worlds() {
        let p = maker(&mut g);
        let rules = g.content().rules.clone();
        let kind = ItemKind::Sword;
        let Some(power) = open_powers(
            kind,
            Craft::CreateArtifact,
            &g.wizard(p).expect("a wizard"),
            &rules,
        )
        .into_iter()
        .max_by_key(|(_, d)| d.cost)
        .map(|(id, _)| id) else {
            continue;
        };
        g.scenario_set_casting_skill(p, 1);
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(1);
        let n = armory_size(&g, p);
        let events = g.apply(Command::CraftItem {
            player: p,
            craft: Craft::CreateArtifact,
            design: design(kind, vec![power]),
        });
        if events.iter().any(|e| matches!(e, Event::ItemMade { .. })) {
            continue;
        }
        assert!(
            events.iter().all(|e| !matches!(e, Event::Rejected(_))),
            "{name}: {events:?}"
        );
        slow += 1;
        assert!(
            g.wizard(p).expect("a wizard").casting().is_some(),
            "{name}: not casting"
        );
        assert_eq!(
            armory_size(&g, p),
            n,
            "{name}: the item came before it was paid"
        );
        // A wizard casts one thing at a time.
        let second = g.apply(Command::CraftItem {
            player: p,
            craft: Craft::EnchantItem,
            design: design(kind, vec![power]),
        });
        assert!(
            second.iter().any(|e| matches!(
                e,
                Event::Rejected(Rejection::Craft(CraftingError::Magic(_)))
            )),
            "{name}: {second:?}"
        );
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(100_000);
        g.scenario_set_casting_skill(p, 600);
        let mut arrived = false;
        for _ in 0..200 {
            if g.end_player_turn(p)
                .iter()
                .any(|e| matches!(e, Event::ItemMade { .. }))
            {
                arrived = true;
                break;
            }
        }
        assert!(arrived, "{name}: the item never came");
        assert_eq!(armory_size(&g, p), n + 1, "{name}");
    }
    assert!(slow > 0, "no world paid over turns");
}

#[test]
fn crafting_without_the_spell_is_refused_and_changes_nothing() {
    for (name, mut g) in worlds() {
        let p = g.wizards()[0];
        let rules = g.content().rules.clone();
        let d = design(
            ItemKind::Sword,
            vec![rules.item_powers.iter().next().expect("a power").0],
        );
        let before = crate::support::text(&g);
        let events = g.apply(Command::CraftItem {
            player: p,
            craft: Craft::EnchantItem,
            design: d,
        });
        let known = g
            .wizard(p)
            .expect("a wizard")
            .knows(Craft::EnchantItem.spell());
        if known {
            continue;
        }
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Rejected(Rejection::Craft(CraftingError::Magic(_)))
            )),
            "{name}: {events:?}"
        );
        assert_eq!(crate::support::text(&g), before, "{name}");
    }
}

#[test]
fn an_item_that_keeps_a_hero_afloat_cannot_be_taken_away_over_the_sea() {
    use f4x_core::map::terrain::TerrainClass;
    let mut tried = 0;
    for (name, mut g) in worlds() {
        let p = g.wizards()[0];
        let rules = g.content().rules.clone();
        // Items that let their bearer cross the sea (flying, water walking, wind walking).
        let floating: Vec<Item> = items(&g)
            .into_iter()
            .filter(|i| {
                i.granted_spells(&rules).iter().any(|s| {
                    [
                        8,  /* water_walking */
                        56, /* flight */
                        63, /* wind_walking */
                    ]
                    .contains(&s.num())
                })
            })
            .collect();
        let fortress = g
            .wizard(p)
            .expect("a wizard")
            .fortress()
            .expect("a fortress");
        let sea = (1..4)
            .flat_map(|r| (-r..=r).flat_map(move |dx| (-r..=r).map(move |dy| (dx, dy))))
            .filter_map(|(dx, dy)| {
                let (x, y) = (fortress.x as i32 + dx, fortress.y as i32 + dy);
                (x >= 0 && y >= 0).then_some(Position {
                    x: x as u32,
                    y: y as u32,
                    ..fortress
                })
            })
            .find(|&q| g.economy().terrain_class_at(q) == Some(TerrainClass::Ocean));
        let Some(sea) = sea else {
            continue;
        };
        // A hero kind with a slot for one of them, and the item.
        let pick = (0..rules.hero_templates.len()).find_map(|k| {
            let entity = g.units().hero_entity(k)?;
            let template = rules.hero_templates.get(k)?;
            let slots = rules
                .units
                .try_get(template.unit)?
                .hero
                .as_ref()?
                .kind
                .slots();
            let free = g.units().hero_state(entity) == Some(f4x_core::hero::HeroState::Free);
            floating
                .iter()
                .find_map(|i| (0..3).find(|&s| slots[s].fits(i.kind)).map(|s| (i, s)))
                .filter(|_| free)
                .map(|(i, s)| (k, i.clone(), s))
        });
        let Some((kind, float, slot)) = pick else {
            continue;
        };
        let Ok(hero) = g.spawn_hero(p, sea, kind) else {
            continue;
        };
        g.treasury_mut(p).expect("a treasury").mana = HalfInt::from_int(10_000);
        let Placed::Vault(at) = g.give_item(p, float.clone()) else {
            continue;
        };
        let to = ItemSpot::Hero(hero, slot);
        assert_eq!(
            item_error(&move_item(&mut g, p, ItemSpot::Vault(at), to, true)),
            None,
            "{name}"
        );
        tried += 1;
        let before = crate::support::text(&g);
        assert_eq!(
            item_error(&move_item(&mut g, p, to, ItemSpot::Vault(at), true)),
            Some(ItemError::WouldDrown),
            "{name}: the hero would drown"
        );
        assert_eq!(
            crate::support::text(&g),
            before,
            "{name}: a refusal changed the game"
        );
    }
    assert!(
        tried > 0,
        "no world had a hero at sea with an item that floats"
    );
}
