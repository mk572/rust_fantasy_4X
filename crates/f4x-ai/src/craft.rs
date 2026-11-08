//! Making items for the heroes: Enchant Item and Create Artifact. When a hero has an empty slot
//! and no item in the armory fits it, the wizard designs an item for that slot, with the
//! dearest powers its books and mana allow (a power's mana cost is the table's measure of its
//! worth), and starts casting it. The finished item goes to the armory, and `offer::equip` puts it
//! on the hero.

use f4x_core::game::{Command, Event, Game};
use f4x_core::item::crafting::{self, Craft, ItemDesign};
use f4x_core::player::view::PlayerView;
use f4x_def::rules::{ItemKind, PowerBonus, SlotKind};

use crate::Ai;

/// Mana the wizard keeps back from a craft, for its other spells.
const RESERVE: i32 = 200;
/// The most an item is worth, whatever the mana: a craft is paid over turns and blocks the
/// casting of other spells.
const MAX_VALUE: i32 = 800;
/// Powers on one item (the game allows four; the charges are not used).
const POWERS: usize = 3;

/// The kind of item the slot takes that suits the hero's trade.
fn kind_for(slot: SlotKind) -> ItemKind {
    match slot {
        SlotKind::Sword | SlotKind::SwordOrStaff => ItemKind::Sword,
        SlotKind::Bow => ItemKind::Bow,
        SlotKind::Staff => ItemKind::Staff,
        SlotKind::Armor => ItemKind::Platemail,
        SlotKind::Misc => ItemKind::Misc,
    }
}

/// Is the power worth putting on a hero's item? The bonuses to casting and to the save against
/// spells need a caster, the rest all help whoever carries them.
fn useful(bonus: Option<PowerBonus>) -> bool {
    !matches!(bonus, Some(PowerBonus::SpellSkill | PowerBonus::SpellSave))
}

/// The item to make for an empty slot, for at most `budget` mana of value.
fn design(
    game: &Game,
    me: f4x_core::player::PlayerId,
    craft: Craft,
    slot: SlotKind,
    budget: i32,
) -> Option<ItemDesign> {
    let rules = &game.content().rules;
    let magic = game.wizard(me)?;
    let kind = kind_for(slot);
    let mut powers = crafting::available_powers(kind, craft, &magic, rules);
    powers.retain(|p| {
        f4x_def::rules::ItemPowerId::new(p.num) != crafting::SPELL_CHARGES
            && useful(p.bonus.map(|b| b.0))
    });
    // The dearest first.
    powers.sort_by_key(|p| std::cmp::Reverse(p.cost));
    let mut design = ItemDesign {
        name: String::new(),
        kind,
        icon: 0,
        powers: Vec::new(),
        charges: None,
    };
    let mut groups = Vec::new();
    for p in powers {
        if design.powers.len() == POWERS {
            break;
        }
        // The free group (the special powers) combines; the others take one power each.
        if p.group != 9 && groups.contains(&p.group) {
            continue;
        }
        let mut trial = design.clone();
        trial.powers.push(f4x_def::rules::ItemPowerId::new(p.num));
        if crafting::cost(&trial, rules) as i32 > budget {
            continue;
        }
        if p.group != 9 {
            groups.push(p.group);
        }
        design = trial;
    }
    (!design.powers.is_empty()).then_some(design)
}

/// Start an item for a hero that has an empty slot, if the armory has nothing for it.
pub fn run(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let choice = {
        let view = PlayerView::new(game, me);
        let Some(magic) = view.wizard() else {
            return;
        };
        if magic.casting().is_some() {
            return;
        }
        let rules = &game.content().rules;
        // Create Artifact takes any power; Enchant Item the cheap ones.
        let Some(craft) = [Craft::CreateArtifact, Craft::EnchantItem]
            .into_iter()
            .find(|c| magic.knows(c.spell()))
        else {
            return;
        };
        let mana = view.treasury().mana.floor();
        let budget = (mana - RESERVE).min(MAX_VALUE);
        if budget <= 0 {
            return;
        }
        // The slots still empty, less the items in the armory that wait for a hero.
        let waiting = {
            let armory = view.armory();
            armory.vault.iter().flatten().count() + armory.awards.len()
        };
        let slots: Vec<SlotKind> = view
            .my_heroes()
            .into_iter()
            .flat_map(|h| view.hero_free_slots(h))
            .map(|(_, k)| k)
            .collect();
        let _ = rules;
        let Some(&slot) = slots.get(waiting) else {
            return;
        };
        design(game, me, craft, slot, budget).map(|d| (craft, d))
    };
    if let Some((craft, design)) = choice {
        ai.send(
            game,
            Command::CraftItem {
                player: me,
                craft,
                design,
            },
            events,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{fresh_game, P1};
    use f4x_core::player::view::PlayerView;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn rich(g: &mut Game) -> TestResult {
        let t = g.treasury_mut(P1).ok_or("player 1 has a treasury")?;
        t.gold = f4x_def::half_int::HalfInt::from_int(50_000);
        t.mana = f4x_def::half_int::HalfInt::from_int(5_000);
        Ok(())
    }

    #[test]
    fn a_wizard_that_knows_create_artifact_makes_an_item_for_a_hero_with_an_empty_slot(
    ) -> TestResult {
        let mut g = fresh_game()?;
        let ai = Ai::new(P1);
        rich(&mut g)?;
        g.hire_hero(P1, 1).map_err(|e| format!("{e:?}"))?;
        let items = |g: &Game| {
            let armory = PlayerView::new(g, P1).armory();
            armory.vault.iter().flatten().count() + armory.awards.len()
        };
        // Not known: nothing.
        let mut events = Vec::new();
        run(&ai, &mut g, &mut events);
        assert_eq!(items(&g), 0, "{events:?}");
        g.cheat_learn_spell(P1, Craft::CreateArtifact.spell());
        run(&ai, &mut g, &mut events);
        let begun = g
            .wizard(P1)
            .ok_or("a wizard")?
            .casting()
            .is_some_and(|c| c.spell == Craft::CreateArtifact.spell());
        assert!(
            begun || items(&g) > 0,
            "an item is made or begun: {events:?}"
        );
        Ok(())
    }
}
