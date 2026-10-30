//! The Item screen (task 142), the original's `Item_Screen` (`ItemScrn.c`): the wizard's
//! heroes in two columns with their three item slots, the fortress vault below, the anvil, the
//! gold and mana, and the Alchemy and OK buttons, over the Army list. A click on an item picks
//! it up (it follows the mouse with its name), a click on another slot puts it there, swapping
//! with what is in the slot; the anvil destroys the item in hand for mana, and S sells it for gold. A right-click on an
//! item shows its card, on a hero's picture the hero's sheet. Items that wait on the award list
//! come up in the hand one at a time. The rules are the core's (`armory`, `Command::MoveItem`,
//! `Command::ScrapItem`); a move to a hero somewhere else asks to pay its mana first.
//! Positions are the original's.

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::ui;
use f4x_core::game::Command;
use f4x_core::item::armory::{ItemError, ItemSpot, Spot, TRANSFER_MANA, VAULT_SLOTS};
use f4x_core::item::Item;
use f4x_core::rules::SlotKind;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::view::{App, Screen, HUMAN};

/// The window's top-left corner.
const WINDOW: (f32, f32) = (17.0, 2.0);
/// Heroes shown at a time: a wizard may serve more, and the arrows scroll through them a page
/// at a time.
const HEROES: usize = 6;

/// What the screen remembers while it is open.
#[derive(Debug, Clone, Default)]
pub(crate) struct ItemUi {
    /// Where the item in the hand came from.
    pub held: Option<ItemSpot>,
    /// A move that costs mana and waits for a yes.
    pub confirm: Option<(ItemSpot, ItemSpot)>,
    /// The card of an item being looked at.
    pub card: Option<Item>,
    /// The last refusal, shown until the next click.
    pub message: Option<String>,
    /// The first vault slot shown: the vault has no limit and the screen shows [`VAULT_SLOTS`]
    /// of it at a time, a page at a time.
    pub vault_offset: usize,
    /// The first hero shown, always a multiple of [`HEROES`].
    pub hero_offset: usize,
}

/// Pages of heroes for `count` heroes.
pub(crate) fn hero_pages_for(count: usize) -> usize {
    count.div_ceil(HEROES).max(1)
}

/// Pages of the vault for `used` slots up to the last item: one slot more than that is always
/// there, so an item can be put down after the last one.
pub(crate) fn vault_pages_for(used: usize) -> usize {
    (used + 1).div_ceil(VAULT_SLOTS).max(1)
}

/// The picture of an empty slot: `itemisc` 19 sword, 20 bow, 21 weapon or staff, 22 wand,
/// 23 misc, 24 armor.
pub(crate) fn slot_icon(kind: SlotKind) -> usize {
    match kind {
        SlotKind::Sword => 19,
        SlotKind::Bow => 20,
        SlotKind::SwordOrStaff => 21,
        SlotKind::Staff => 22,
        SlotKind::Misc => 23,
        SlotKind::Armor => 24,
    }
}

/// The picture square of hero `slot` (`Item_Window_Picture_Coords`): 32 by 32.
pub(crate) fn hero_square(slot: usize) -> (f32, f32) {
    (
        WINDOW.0 + 14.0 + (slot % 2) as f32 * 135.0,
        WINDOW.1 + 12.0 + (slot / 2) as f32 * 46.0,
    )
}

/// The key of the text the original prints for a refused move; empty when it prints none.
pub(crate) fn refusal(error: ItemError) -> &'static str {
    match error {
        ItemError::WrongSlot => "item.wrong_slot",
        ItemError::NoMana => "item.no_mana",
        ItemError::WouldDrown => "item.would_drown",
        ItemError::NoSuchPlace | ItemError::Empty | ItemError::Transfer => "",
    }
}

impl App {
    pub(crate) fn open_item_screen(&mut self) {
        let mut ui = ItemUi::default();
        ui.pick_up_award(self);
        self.open_page(Screen::Items, ui);
    }

    /// The item at a spot.
    fn item_at(&self, spot: ItemSpot) -> Option<Item> {
        use f4x_core::item::armory::ItemWorld;
        match spot {
            ItemSpot::Vault(i) => self.game.items().armory(HUMAN).vault.get(i)?.clone(),
            ItemSpot::Award(i) => self.game.items().armory(HUMAN).awards.get(i).cloned(),
            ItemSpot::Hero(unit, slot) => self.game.items().hero_item(unit, slot),
        }
    }

    /// How many pages of [`VAULT_SLOTS`] the vault has: one more slot than the last item is
    /// always there, so an item can be put down after the last one.
    fn vault_pages(&self) -> usize {
        let used = self
            .game
            .items()
            .armory(HUMAN)
            .vault
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |last| last + 1);
        vault_pages_for(used)
    }

    /// Draw an item's icon with its name at the hand.
    fn draw_item_in_hand(&mut self, lay: &Layout, item: &Item, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (mx, my) = input.mouse_position();
        let (x, y) = (
            (mx - lay.origin.x) / lay.scale - 5.0,
            (my - lay.origin.y) / lay.scale - 5.0,
        );
        self.draw_pic(lay, "items", usize::from(item.icon), 0, x, y);
        let name = item.display_name(self.game.content());
        self.draw_styled(view, &Style::outlined(1), &name, x + 20.0, y + 4.0);
    }

    /// Draw a triangle arrow of the scroll buttons at `at` (11 by 18) pointing left (`dir < 0`)
    /// or right; true when the mouse is on it and it is `enabled`.
    fn scroll_arrow(
        &self,
        lay: &Layout,
        at: (f32, f32),
        dir: i32,
        enabled: bool,
        input: &Input,
    ) -> bool {
        let (ax, ay) = at;
        let (x, y, scale) = (lay.origin.x, lay.origin.y, lay.scale);
        let tip = if dir < 0 { ax } else { ax + 9.0 };
        let base = if dir < 0 { ax + 9.0 } else { ax };
        let color = if enabled {
            Color::new(0.95, 0.85, 0.5, 1.0)
        } else {
            Color::new(0.4, 0.36, 0.25, 1.0)
        };
        draw_triangle(
            vec2(x + tip * scale, y + (ay + 9.0) * scale),
            vec2(x + base * scale, y + (ay + 1.0) * scale),
            vec2(x + base * scale, y + (ay + 17.0) * scale),
            color,
        );
        ui::hit(lay.rect((ax - 1.0, ay, 11.0, 18.0)), enabled, input).hovered()
    }

    /// One of the screen's two-frame buttons (`armylist` 7 and 8); true when clicked.
    fn item_button(&mut self, lay: &Layout, entry: usize, x: f32, y: f32, input: &Input) -> bool {
        self.army_button(lay, entry, x, y, true, input)
    }
}
impl ItemUi {
    /// The next item on the award list comes into the hand (`Process_Item_Pool`).
    fn pick_up_award(&mut self, app: &mut App) {
        if self.held.is_none() && !app.game.items().armory(HUMAN).awards.is_empty() {
            self.held = Some(ItemSpot::Award(0));
        }
    }

    /// Whether `hero`'s square is the one the item in the hand comes from.
    fn same_location(&self, app: &App, hero: hecs::Entity) -> bool {
        let source = match self.held {
            Some(ItemSpot::Hero(unit, _)) => app.game.units().unit_position(unit),
            _ => app.game.wizard(HUMAN).and_then(|w| w.fortress()),
        };
        source.is_some() && source == app.game.units().unit_position(hero)
    }

    /// Put the item in the hand at `to` (or pick up what is there).
    fn item_click(&mut self, app: &mut App, to: ItemSpot) {
        self.message = None;
        match self.held {
            None => {
                if app.item_at(to).is_some() {
                    self.held = Some(to);
                }
            }
            Some(from) if from == to => self.held = None,
            Some(from) => self.try_move(app, from, to, false),
        }
    }

    fn try_move(&mut self, app: &mut App, from: ItemSpot, to: ItemSpot, pay: bool) {
        // An item put in the vault that comes from the award list leaves the list shorter; an
        // item in the hand stays there while the swapped one is picked up in its place (the
        // original's exchange), which the core does as a swap.
        let before = app.item_at(to);
        let events = app.game.apply(Command::MoveItem {
            player: HUMAN,
            from,
            to,
            pay,
        });
        let error = events.iter().find_map(|e| match e {
            f4x_core::game::Event::Rejected(f4x_core::game::Rejection::Item(err)) => Some(*err),
            _ => None,
        });
        match error {
            Some(ItemError::Transfer) => self.confirm = Some((from, to)),
            Some(err) => {
                let key = refusal(err);
                self.message = Some(if key.is_empty() {
                    String::new()
                } else {
                    app.text(key)
                });
            }
            None => {
                // What was in the target slot is now where the item came from: pick it up.
                self.held = before.map(|_| from);
                app.note(events);
            }
        }
        self.pick_up_award(app);
    }

    /// The anvil: destroy the item in the hand if it is in the vault or on the award list.
    fn scrap_held(&mut self, app: &mut App) {
        let spot = match self.held {
            Some(ItemSpot::Vault(i)) => Spot::Vault(i),
            Some(ItemSpot::Award(i)) => Spot::Award(i),
            _ => return,
        };
        let events = app.game.apply(Command::ScrapItem {
            player: HUMAN,
            from: spot,
        });
        app.note(events);
        self.held = None;
        self.pick_up_award(app);
    }

    /// Sell the item in the hand if it is in the vault or on the award list: half its value in
    /// gold now, and the computer wizards are asked in turn whether they buy it.
    fn sell_held(&mut self, app: &mut App) {
        let spot = match self.held {
            Some(ItemSpot::Vault(i)) => Spot::Vault(i),
            Some(ItemSpot::Award(i)) => Spot::Award(i),
            _ => return,
        };
        let mut events = app.game.apply(Command::SellItem {
            player: HUMAN,
            from: spot,
        });
        events.extend(f4x_ai::answer_resales(&mut app.game));
        app.note(events);
        self.held = None;
        self.pick_up_award(app);
    }

    /// The item card and the question about paying for a move.
    fn item_modals(&mut self, app: &mut App, lay: &Layout, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if let Some(item) = self.card.clone() {
            app.draw_item_card(lay, &item, 50.0, 60.0);
            if input.mouse_pressed(MouseButton::Left)
                || input.mouse_pressed(MouseButton::Right)
                || input.key_pressed(KeyCode::Escape)
            {
                self.card = None;
            }
        }
        if let Some((from, to)) = self.confirm {
            app.draw_pic(lay, "newgame", 44, 0, 67.0, 40.0);
            app.draw_pic(lay, "newgame", 45, 0, 67.0, 152.0);
            let text = app.text_with(
                "item.transfer_ask",
                &Args::new().count("mana", TRANSFER_MANA),
            );
            let lines = app.bm_wrap(1, &text, 160.0);
            for (i, line) in lines.iter().enumerate() {
                app.draw_styled(
                    view,
                    &Style::nearest(1, Color::new(0.93, 0.88, 0.68, 1.0)).center(),
                    line,
                    160.0,
                    80.0 + i as f32 * 8.0,
                );
            }
            if input.key_pressed(KeyCode::Y) || input.key_pressed(KeyCode::Enter) {
                self.confirm = None;
                self.try_move(app, from, to, true);
            } else if input.key_pressed(KeyCode::N) || input.key_pressed(KeyCode::Escape) {
                self.confirm = None;
            }
        }
    }
}

impl Page for ItemUi {
    fn frame(&mut self, app: &mut App, input: &Input) {
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (wx, wy) = WINDOW;
        app.draw_pic(&lay, "armylist", 0, 0, 0.0, 0.0);
        app.draw_pic(&lay, "armylist", 5, 0, wx, wy);
        let busy = self.card.is_some() || self.confirm.is_some();
        let left = input.mouse_pressed(MouseButton::Left) && !busy;
        let right = input.mouse_pressed(MouseButton::Right) && !busy;
        let heroes = app.game.units().hero_units(HUMAN);
        let held = self.held.is_some();
        let ink = Color::new(0.95, 0.85, 0.5, 1.0);
        let mut card_at = None;

        // The heroes, their pictures and their slots.
        let hero_pages = hero_pages_for(heroes.len());
        self.hero_offset = self.hero_offset.min((hero_pages - 1) * HEROES);
        let shown = heroes.iter().skip(self.hero_offset).take(HEROES);
        for (i, &hero) in shown.enumerate() {
            let (x1, y1) = hero_square(i);
            app.draw_pic(&lay, "armylist", 6, 0, x1, y1);
            let portrait = app
                .game
                .units()
                .unit_def(hero)
                .and_then(|d| d.hero.as_ref().map(|h| h.portrait as usize));
            if let Some(p) = portrait {
                app.draw_pic(&lay, "portrait", p, 0, x1 + 1.0, y1 + 1.0);
            }
            let name = app.game.units().hero_name(hero).unwrap_or_default();
            app.draw_styled(view, &Style::outlined(1), &name, x1 + 37.0, y1);
            if held {
                if self.same_location(app, hero) {
                    app.draw_styled(
                        view,
                        &Style::nearest(1, Color::new(0.6, 0.9, 0.5, 1.0)),
                        &app.text("item.same_location"),
                        x1 + 37.0,
                        y1 + 7.0,
                    );
                } else {
                    app.draw_styled(
                        view,
                        &Style::nearest(1, Color::new(0.95, 0.5, 0.4, 1.0)),
                        &app.text("item.teleport"),
                        x1 + 37.0,
                        y1 + 7.0,
                    );
                }
            }
            if right && ui::hit(lay.rect((x1, y1, 32.0, 32.0)), true, input).hovered() {
                app.open_unit_sheet(hero);
                return;
            }
            let kinds = app
                .game
                .units()
                .unit_def(hero)
                .and_then(|d| d.hero.as_ref().map(|h| h.kind.slots()))
                .unwrap_or([SlotKind::Misc; 3]);
            for (slot, &kind) in kinds.iter().enumerate() {
                let (sx, sy) = (x1 + 40.0 + slot as f32 * 29.0, y1 + 15.0);
                let spot = ItemSpot::Hero(hero, slot);
                match app.item_at(spot) {
                    Some(item) => {
                        app.draw_pic(&lay, "items", usize::from(item.icon), 0, sx, sy);
                    }
                    None => {
                        app.draw_pic(&lay, "itemisc", slot_icon(kind), 0, sx + 1.0, sy + 2.0);
                    }
                }
                if ui::hit(lay.rect((sx, sy + 1.0, 22.0, 18.0)), true, input).hovered() {
                    if left {
                        self.item_click(app, spot);
                    } else if right {
                        card_at = app.item_at(spot);
                    }
                }
            }
        }

        // More heroes than fit: an arrow on each side of the page number scrolls six at a time.
        if hero_pages > 1 {
            let page = self.hero_offset / HEROES;
            for (dir, ax) in [(-1i32, wx + 88.0), (1, wx + 124.0)] {
                let enabled = if dir < 0 {
                    page > 0
                } else {
                    page + 1 < hero_pages
                };
                if app.scroll_arrow(&lay, (ax, wy + 142.0), dir, enabled, input) && left {
                    let page = if dir < 0 { page - 1 } else { page + 1 };
                    self.hero_offset = page * HEROES;
                }
            }
            let label = format!("{}/{}", page + 1, hero_pages);
            app.draw_styled(
                view,
                &Style::outlined(1).center(),
                &label,
                wx + 111.0,
                wy + 147.0,
            );
        }

        // The vault: four slots of it at a time, with an arrow on each side to scroll a page.
        let pages = app.vault_pages();
        self.vault_offset = self.vault_offset.min((pages - 1) * VAULT_SLOTS);
        let page = self.vault_offset / VAULT_SLOTS;
        for (dir, ax) in [(-1i32, wx + 40.0), (1, wx + 139.0)] {
            let enabled = if dir < 0 { page > 0 } else { page + 1 < pages };
            if app.scroll_arrow(&lay, (ax, wy + 170.0), dir, enabled, input) && left {
                let page = if dir < 0 { page - 1 } else { page + 1 };
                self.vault_offset = page * VAULT_SLOTS;
            }
        }
        for slot in 0..VAULT_SLOTS {
            let (sx, sy) = (wx + 52.0 + slot as f32 * 21.0, wy + 170.0);
            let spot = ItemSpot::Vault(self.vault_offset + slot);
            if let Some(item) = app.item_at(spot) {
                app.draw_pic(&lay, "items", usize::from(item.icon), 0, sx, sy);
            }
            if ui::hit(lay.rect((sx, sy + 2.0, 20.0, 16.0)), true, input).hovered() {
                if left {
                    self.item_click(app, spot);
                } else if right {
                    card_at = app.item_at(spot);
                }
            }
        }

        // The gold and the mana, the anvil, the buttons.
        let (gold, mana) = app
            .game
            .player(HUMAN)
            .map_or((0, 0), |p| (p.treasury().gold, p.treasury().mana));
        let style = if gold > 19999 || mana > 19999 { 0 } else { 1 };
        app.draw_styled(
            view,
            &Style::outlined(style).right(),
            &gold.to_string(),
            wx + 166.0,
            wy + 164.0,
        );
        app.draw_styled(
            view,
            &Style::outlined(style).right(),
            &mana.to_string(),
            wx + 200.0,
            wy + 164.0,
        );
        app.draw_styled(view, &Style::outlined(0), "GP", wx + 169.0, wy + 164.0);
        app.draw_styled(view, &Style::outlined(0), "MP", wx + 203.0, wy + 164.0);
        let alchemy = !held && app.item_button(&lay, 7, wx + 219.0, wy + 155.0, input);
        let ok = !held && app.item_button(&lay, 8, wx + 219.0, wy + 175.0, input);
        if left && ui::hit(lay.rect((wx + 9.0, wy + 155.0, 39.0, 34.0)), true, input).hovered() {
            self.scrap_held(app);
        }
        // S sells the item in the hand for half its value in gold (task 294).
        if held && input.key_pressed(KeyCode::S) {
            self.sell_held(app);
        }
        if let Some(item) = card_at {
            self.card = Some(item);
        }

        // The item in the hand follows the mouse.
        if let Some(spot) = self.held {
            match app.item_at(spot) {
                Some(item) => app.draw_item_in_hand(&lay, &item, input),
                None => self.held = None,
            }
        }
        if let Some(message) = self.message.clone() {
            app.draw_styled(view, &Style::outlined(1).center(), &message, 160.0, 150.0);
        }
        self.item_modals(app, &lay, input);

        if alchemy {
            app.open_magic();
            app.magic_ui.alchemy = Some(crate::magic_screen::AlchemyUi {
                gold_to_mana: true,
                slider: 3,
            });
        } else if !busy && !held && (ok || input.key_pressed(KeyCode::Escape)) {
            app.open_page(Screen::ArmyList, crate::army_screen::ArmyUi::default());
        }
        let _ = ink;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vault_grows_a_page_when_the_last_slot_is_full() {
        assert_eq!(vault_pages_for(0), 1);
        assert_eq!(vault_pages_for(3), 1, "slot four is still free");
        assert_eq!(
            vault_pages_for(4),
            2,
            "four items need a free slot after them"
        );
        assert_eq!(vault_pages_for(7), 2);
        assert_eq!(vault_pages_for(8), 3);
    }

    #[test]
    fn heroes_scroll_six_to_a_page() {
        assert_eq!(hero_pages_for(0), 1);
        assert_eq!(hero_pages_for(6), 1);
        assert_eq!(hero_pages_for(7), 2);
        assert_eq!(hero_pages_for(35), 6);
    }

    #[test]
    fn heroes_stand_in_two_columns_of_three() {
        assert_eq!(hero_square(0), (31.0, 14.0));
        assert_eq!(hero_square(1), (166.0, 14.0));
        assert_eq!(hero_square(5), (166.0, 106.0));
    }

    #[test]
    fn every_slot_kind_has_its_own_picture() {
        let kinds = [
            SlotKind::Sword,
            SlotKind::Bow,
            SlotKind::SwordOrStaff,
            SlotKind::Staff,
            SlotKind::Armor,
            SlotKind::Misc,
        ];
        let mut icons: Vec<_> = kinds.into_iter().map(slot_icon).collect();
        icons.sort_unstable();
        icons.dedup();
        assert_eq!(icons.len(), 6);
    }

    #[test]
    fn the_refusals_have_words_and_the_transfer_has_none_here() {
        assert_eq!(refusal(ItemError::WouldDrown), "item.would_drown");
        assert!(refusal(ItemError::Transfer).is_empty());
    }
}
