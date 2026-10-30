//! The item crafting screen (task 260), the original's `Item_Make_Screen` (`ItemMake.c`): the
//! wizard designs the item that Enchant Item or Create Artifact makes. Ten buttons for the kind,
//! the picture with its two arrows, the name box, the powers in three columns (the plain powers
//! in two, the special ones in the third), the cost and the OK button. Spell Charges opens the
//! list of spells that can be put in the item and then asks for one to four charges. The rules
//! are the core's (`item::crafting`: what may be chosen, what it costs, `Command::CraftItem`);
//! this file holds the choices. Positions are the original's.

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::ui;
use f4x_core::game::{Command, Event};
use f4x_core::item::crafting::{
    available_powers, check, cost, made_name, Craft, ItemDesign, MAX_CHARGES, MAX_POWERS,
    SPELL_CHARGES,
};
use f4x_core::rules::{CastContext, ItemKind, ItemPowerDef};
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::view::{App, Screen, HUMAN};

/// The kinds in the order of the buttons (`SPELLSCR.LBX` entries 14 to 23).
pub(crate) const KINDS: [ItemKind; 10] = [
    ItemKind::Sword,
    ItemKind::Mace,
    ItemKind::Axe,
    ItemKind::Bow,
    ItemKind::Staff,
    ItemKind::Wand,
    ItemKind::Misc,
    ItemKind::Shield,
    ItemKind::Chainmail,
    ItemKind::Platemail,
];
/// Letters in an item's name (`LEN_ITEM_MAKE_NAME`).
const NAME_LEN: usize = 29;
/// Rows in one column of the plain powers.
const COLUMN_ROWS: usize = 16;
/// Spells on one page of the spell list: two columns of fourteen.
const SPELLS_PER_PAGE: usize = 28;

/// What the screen remembers while it is open.
#[derive(Debug, Clone)]
pub(crate) struct CraftUi {
    pub craft: Craft,
    pub design: ItemDesign,
    /// The name box has the keyboard.
    pub typing: bool,
    /// The spell list for Spell Charges, while it is open.
    pub spells: Option<SpellPick>,
    /// The question whether to give up a design with no power.
    pub give_up: bool,
    /// The last refusal, shown until the next click.
    pub message: Option<String>,
}

/// The list of spells to put in an item.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpellPick {
    pub page: usize,
    /// The spell chosen, waiting for its number of charges.
    pub chosen: Option<(usize, String)>,
}

/// One entry of the power columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Row {
    Power(String),
    /// A gap between two groups.
    Gap,
}

/// The three columns of the screen for the powers on offer, given in the table's order: the
/// plain powers in groups with a gap between them, split in two columns of at most
/// [`COLUMN_ROWS`] rows, and the special powers in the third.
pub(crate) fn columns(powers: &[&ItemPowerDef]) -> [Vec<Row>; 3] {
    let mut plain: Vec<Row> = Vec::new();
    let mut last_group = None;
    for p in powers.iter().filter(|p| p.bonus.is_some()) {
        if last_group.is_some_and(|g| g != p.group) {
            plain.push(Row::Gap);
        }
        last_group = Some(p.group);
        plain.push(Row::Power(p.id.clone()));
    }
    let mut first = plain.len();
    let mut second = Vec::new();
    if plain.len() > COLUMN_ROWS {
        // Break after the last whole group that fits.
        first = plain[..=COLUMN_ROWS]
            .iter()
            .rposition(|r| *r == Row::Gap)
            .unwrap_or(COLUMN_ROWS);
        second = plain.split_off(first);
        if second.first() == Some(&Row::Gap) {
            second.remove(0);
        }
    }
    plain.truncate(first);
    let special = powers
        .iter()
        .filter(|p| p.bonus.is_none())
        .map(|p| Row::Power(p.id.clone()))
        .collect();
    [plain, second, special]
}

impl CraftUi {
    pub(crate) fn new(craft: Craft) -> CraftUi {
        let kind = KINDS[0];
        CraftUi {
            craft,
            design: ItemDesign {
                name: String::new(),
                kind,
                icon: kind.icon_range().0,
                powers: Vec::new(),
                charges: None,
            },
            typing: false,
            spells: None,
            give_up: false,
            message: None,
        }
    }

    /// How many of the four places the design uses (the charges take one).
    pub(crate) fn used(&self) -> usize {
        self.design.powers.len() + usize::from(self.design.charges.is_some())
    }

    /// Choose a kind: the picture is the kind's first, and the powers the new kind cannot
    /// have go.
    pub(crate) fn set_kind(&mut self, kind: ItemKind, allowed: &[String]) {
        self.design.kind = kind;
        self.design.icon = kind.icon_range().0;
        self.design.powers.retain(|p| allowed.contains(p));
        if !allowed.iter().any(|p| p == SPELL_CHARGES) {
            self.design.charges = None;
        }
    }

    /// Step the picture through the kind's range, round at both ends.
    pub(crate) fn step_icon(&mut self, forward: bool) {
        let (first, last) = self.design.kind.icon_range();
        let at = self.design.picture();
        self.design.icon = match (forward, at) {
            (true, a) if a >= last => first,
            (true, a) => a + 1,
            (false, a) if a <= first => last,
            (false, a) => a - 1,
        };
    }
}

/// Choose or drop the power `id`. A plain power replaces the one of its group; `Err` names the
/// text of the reason it could not be chosen.
pub(crate) fn toggle_power(
    design: &mut ItemDesign,
    power: &ItemPowerDef,
    groups: &dyn Fn(&str) -> Option<u8>,
) -> Result<(), &'static str> {
    if let Some(at) = design.powers.iter().position(|p| *p == power.id) {
        design.powers.remove(at);
        return Ok(());
    }
    // A power of the same group (but the special powers) goes out for this one.
    let same = (power.group != 9)
        .then(|| {
            design
                .powers
                .iter()
                .position(|p| groups(p) == Some(power.group))
        })
        .flatten();
    if let Some(at) = same {
        design.powers.remove(at);
    }
    if design.powers.len() + usize::from(design.charges.is_some()) >= MAX_POWERS {
        return Err("craft.too_many");
    }
    design.powers.push(power.id.clone());
    Ok(())
}

impl App {
    /// Open the screen for Enchant Item or Create Artifact.
    pub(crate) fn open_craft(&mut self, craft: Craft) {
        self.open_page(Screen::Craft, CraftUi::new(craft));
    }

    /// The ids of the powers the wizard may put on an item of `kind`.
    fn craft_allowed(&self, craft: Craft, kind: ItemKind) -> Vec<String> {
        let rules = &self.game.content().rules;
        self.game
            .wizard(HUMAN)
            .map(|m| {
                available_powers(kind, craft, &m, rules)
                    .iter()
                    .map(|p| p.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The spells that can be put in an item: those the wizard knows and can cast in battle.
    fn charge_spells(&self) -> Vec<(String, String)> {
        let rules = &self.game.content().rules;
        let Some(m) = self.game.wizard(HUMAN) else {
            return Vec::new();
        };
        m.known()
            .into_iter()
            .map(|s| rules.spells.get(s))
            .filter(|d| d.contexts.contains(&CastContext::Battle))
            .map(|d| (d.id.clone(), d.name.clone()))
            .collect()
    }

    /// What the cast costs the wizard: the item's value less the reduction for the spell.
    fn craft_price(&self, ui: &CraftUi) -> i32 {
        let rules = &self.game.content().rules;
        let value = cost(&ui.design, rules) as i32;
        let reduction = rules
            .spells
            .id_of(ui.craft.spell())
            .and_then(|id| {
                self.game
                    .wizard(HUMAN)
                    .map(|m| m.cost_reduction(rules.spells.get(id)))
            })
            .unwrap_or(0);
        value - value * reduction / 100
    }

    /// The name the box shows: the one typed, else the default one.
    fn craft_name(&self, ui: &CraftUi) -> String {
        if ui.typing || !ui.design.name.is_empty() {
            return ui.design.name.clone();
        }
        made_name(&ui.design, &self.game.content().rules).compose(self.game.content())
    }

    /// The name box lets go: a name left as the default one goes back to being made from the
    /// parts, in the language of the texts.
    fn leave_name(&self, ui: &mut CraftUi) {
        ui.typing = false;
        let default = {
            let mut bare = ui.design.clone();
            bare.name.clear();
            made_name(&bare, &self.game.content().rules).compose(self.game.content())
        };
        if ui.design.name == default || ui.design.name.trim().is_empty() {
            ui.design.name.clear();
        }
    }

    /// OK with a design: ask the core to start casting. A refusal stays on the screen.
    fn finish_craft(&mut self, ui: &mut CraftUi) {
        let rules = self.game.content().rules.clone();
        let wizard_ok = self
            .game
            .wizard(HUMAN)
            .map(|m| check(&ui.design, ui.craft, &m, &rules));
        if let Some(Err(e)) = wizard_ok {
            ui.message = Some(self.text(e.key()));
            return;
        }
        self.leave_name(ui);
        let events = self.game.apply(Command::CraftItem {
            player: HUMAN,
            craft: ui.craft,
            design: ui.design.clone(),
        });
        let refused = events.iter().find_map(|e| match e {
            Event::Rejected(why) => Some(self.text(why.key())),
            _ => None,
        });
        match refused {
            Some(text) => ui.message = Some(text),
            None => {
                self.note(events);
                self.screen = Screen::Map;
            }
        }
    }

    /// The list of spells for Spell Charges, over the screen (`Item_Make_Screen_Spellbook_Popup`).
    fn craft_spell_list(&mut self, lay: &Layout, ui: &mut CraftUi, input: &Input) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (bx, by) = (28.0, 12.0);
        let spells = self.charge_spells();
        let click = input.mouse_pressed(MouseButton::Left);
        self.draw_pic(lay, "spells", 0, 0, bx, by);
        let title = self.text("craft.choose_spell");
        self.draw_styled(
            view,
            &Style::nearest(4, Color::new(0.95, 0.88, 0.6, 1.0)).center(),
            &title,
            160.0,
            2.0,
        );
        let Some(pick) = ui.spells.as_mut() else {
            return;
        };
        let pages = spells.len().div_ceil(SPELLS_PER_PAGE).max(1);
        pick.page = pick.page.min(pages - 1);
        if pick.page > 0 {
            let r = self.draw_pic(lay, "spells", 1, 0, bx + 13.0, by + 4.0);
            if r.is_some_and(|r| ui::hit(r, click, input).clicked()) {
                pick.page -= 1;
            }
        }
        if pick.page + 1 < pages {
            let r = self.draw_pic(lay, "spells", 2, 0, bx + 258.0, by + 4.0);
            if r.is_some_and(|r| ui::hit(r, click, input).clicked()) {
                pick.page += 1;
            }
        }
        let ink = Color::new(0.2, 0.12, 0.05, 1.0);
        let lit = Color::new(0.7, 0.2, 0.1, 1.0);
        let first = pick.page * SPELLS_PER_PAGE;
        let mut chosen = None;
        for (i, (id, name)) in spells.iter().skip(first).take(SPELLS_PER_PAGE).enumerate() {
            let (x, y) = (
                bx + 30.0 + (i / 14) as f32 * 130.0,
                by + 7.0 + (i % 14) as f32 * 10.0,
            );
            let picked = pick.chosen.as_ref().is_some_and(|(at, _)| *at == i);
            self.draw_styled(
                view,
                &Style::nearest(1, if picked { lit } else { ink }),
                name,
                x,
                y,
            );
            if pick.chosen.is_none()
                && ui::hit(lay.rect((x - 2.0, y - 1.0, 105.0, 10.0)), click, input).clicked()
            {
                chosen = Some((i, id.clone()));
            }
        }
        if chosen.is_some() {
            pick.chosen = chosen;
        }
        // The click that is none of the spells and is not on the charges closes the list.
        let mut done = None;
        if let Some((at, id)) = pick.chosen.clone() {
            let (col, row) = (at / 14, (at % 14) as f32);
            let row = row.clamp(1.0, 11.0);
            let charges_x = bx + 155.0 - 140.0 * col as f32;
            let charges_y = by + row * 10.0 - 4.0;
            self.draw_pic(lay, "spellscr", 37, 0, charges_x, charges_y);
            let word = self.text("craft.charges");
            self.draw_styled(
                view,
                &Style::nearest(1, Color::new(0.95, 0.88, 0.6, 1.0)).center(),
                &word,
                bx + 204.0 - 140.0 * col as f32,
                by + row * 10.0 + 1.0,
            );
            for n in 0..MAX_CHARGES {
                let (x, y) = (
                    bx + 158.0 + 23.0 * n as f32 - 140.0 * col as f32,
                    by + 11.0 + row * 10.0 + 10.0 * col as f32,
                );
                let r = self.draw_pic(lay, "spellscr", 38 + n as usize, 0, x, y);
                if r.is_some_and(|r| ui::hit(r, click, input).clicked()) {
                    done = Some((id.clone(), n + 1));
                }
            }
        }
        let leave = click
            && pick.chosen.is_none()
            && !ui::hit(lay.rect((bx, by, 290.0, 170.0)), true, input).hovered()
            || input.key_pressed(KeyCode::Escape);
        if let Some(charges) = done {
            ui.design.charges = Some(charges);
            ui.spells = None;
        } else if leave {
            ui.spells = None;
        }
    }
}

impl Page for CraftUi {
    fn frame(&mut self, app: &mut App, input: &Input) {
        let ui = self;
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        clear_background(BLACK);
        app.draw_pic(&lay, "spellscr", 13, 0, 0.0, 0.0);
        let busy = ui.spells.is_some() || ui.give_up;
        let click = input.mouse_pressed(MouseButton::Left) && !busy;
        let allowed = app.craft_allowed(ui.craft, ui.design.kind);

        // The ten kinds.
        for (i, kind) in KINDS.into_iter().enumerate() {
            let (x, y) = (156.0 + 33.0 * (i % 5) as f32, 3.0 + 15.0 * (i / 5) as f32);
            let on = kind == ui.design.kind;
            let entry = if on { 25 + i } else { 14 + i };
            let pressed = !busy && input.mouse_down(MouseButton::Left);
            let rect = app.draw_pic(&lay, "spellscr", entry, usize::from(pressed), x, y);
            if rect.is_some_and(|r| ui::hit(r, click, input).clicked()) {
                ui.set_kind(kind, &app.craft_allowed(ui.craft, kind));
                ui.message = None;
            }
        }

        // The picture and its arrows.
        app.draw_pic(&lay, "items", usize::from(ui.design.picture()), 0, 4.0, 5.0);
        for (forward, x, entry) in [(false, 4.0, 35), (true, 17.0, 36)] {
            let rect = app.draw_pic(&lay, "spellscr", entry, 0, x, 24.0);
            if rect.is_some_and(|r| ui::hit(r, click, input).clicked()) {
                ui.step_icon(forward);
            }
        }

        // The name box.
        let name_box = lay.rect((30.0, 13.0, 118.0, 10.0));
        if input.mouse_pressed(MouseButton::Left) && !busy {
            let inside = ui::hit(name_box, true, input).hovered();
            if inside && !ui.typing {
                ui.design.name = app.craft_name(ui);
                ui.typing = true;
            } else if !inside && ui.typing {
                app.leave_name(ui);
            }
        }
        if ui.typing && !busy {
            for c in input.typed() {
                if !c.is_control() && ui.design.name.chars().count() < NAME_LEN {
                    ui.design.name.push(c);
                }
            }
            if input.key_pressed(KeyCode::Backspace) {
                ui.design.name.pop();
            }
            if input.key_pressed(KeyCode::Enter) {
                app.leave_name(ui);
            }
        }
        let shown = if ui.typing {
            format!("{}_", ui.design.name)
        } else {
            app.craft_name(ui)
        };
        app.draw_styled(
            view,
            &Style::nearest(1, Color::new(0.95, 0.88, 0.6, 1.0)),
            &shown,
            33.0,
            15.0,
        );

        // The powers.
        let rules = app.game.content().rules.clone();
        let on_offer: Vec<&ItemPowerDef> = rules
            .item_powers
            .defs()
            .iter()
            .filter(|p| allowed.contains(&p.id))
            .collect();
        let group_of = |id: &str| rules.item_powers.by_name(id).map(|p| p.group);
        let [first, second, special] = columns(&on_offer);
        for (col, rows) in [first, second, special].into_iter().enumerate() {
            let x = 10.0 + 85.0 * col as f32 + if col == 2 { 17.0 } else { 0.0 };
            let mut y = 41.0;
            for row in rows {
                let Row::Power(id) = row else {
                    y += 3.0;
                    continue;
                };
                let Some(power) = rules.item_powers.by_name(&id) else {
                    continue;
                };
                let charges = id == SPELL_CHARGES;
                let chosen = if charges {
                    ui.design.charges.is_some()
                } else {
                    ui.design.powers.contains(&id)
                };
                let color = if chosen {
                    Color::new(1.0, 0.95, 0.55, 1.0)
                } else {
                    Color::new(0.62, 0.64, 0.8, 1.0)
                };
                if charges && chosen {
                    let (spell, n) = ui.design.charges.clone().unwrap_or_default();
                    let spell = rules
                        .spells
                        .by_name(&spell)
                        .map_or(spell.clone(), |s| s.name.clone());
                    let line = app.text_with(
                        "item.name_charges",
                        &Args::new().text("spell", &spell).count("n", n as i32),
                    );
                    app.draw_styled(view, &Style::nearest(3, color), &line, x, y);
                } else if col == 2 {
                    app.draw_styled(view, &Style::nearest(3, color), &power.name, x, y);
                } else {
                    // "+2 Attack": the number right-aligned, the rest after it.
                    let (num, rest) = power.name.split_once(' ').unwrap_or(("", &power.name));
                    app.draw_styled(view, &Style::nearest(3, color).right(), num, x + 10.0, y);
                    app.draw_styled(view, &Style::nearest(3, color), rest, x + 14.0, y);
                }
                let width = if col == 2 { 98.0 } else { 78.0 };
                if ui::hit(lay.rect((x, y - 2.0, width, 9.0)), click, input).clicked() {
                    ui.message = None;
                    if charges {
                        if ui.design.charges.is_some() {
                            ui.design.charges = None;
                        } else if ui.used() >= MAX_POWERS {
                            ui.message = Some(app.text("craft.too_many"));
                        } else {
                            ui.spells = Some(SpellPick::default());
                        }
                    } else if let Err(key) = toggle_power(&mut ui.design, power, &group_of) {
                        ui.message = Some(app.text(key));
                    }
                }
                y += 11.0;
            }
        }

        // The cost and the OK button.
        let price = app.craft_price(ui);
        let cost_label = app.text("craft.cost");
        let ink = Color::new(0.95, 0.88, 0.6, 1.0);
        let at = app.draw_styled(view, &Style::nearest(1, ink), &cost_label, 198.0, 185.0);
        app.draw_styled(
            view,
            &Style::nearest(1, ink),
            &price.to_string(),
            198.0 + at + 4.0,
            185.0,
        );
        let ok_rect = app.draw_pic(&lay, "spellscr", 24, 0, 281.0, 180.0);
        if let Some(message) = &ui.message {
            app.draw_styled(
                view,
                &Style::nearest(1, Color::new(1.0, 0.5, 0.4, 1.0)).center(),
                message,
                160.0,
                170.0,
            );
        }
        let ok = !busy
            && !ui.typing
            && (ok_rect.is_some_and(|r| ui::hit(r, click, input).clicked())
                || input.key_pressed(KeyCode::Escape));
        if ok {
            if ui.used() == 0 {
                ui.give_up = true;
            } else {
                app.finish_craft(ui);
            }
        }

        if ui.spells.is_some() {
            app.craft_spell_list(&lay, ui, input);
        }
        if ui.give_up {
            let question = app.text("craft.give_up");
            match app.confirm_box(&question, input) {
                Some(true) => {
                    app.screen = Screen::Map;
                }
                Some(false) => ui.give_up = false,
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::rules::PowerBonus;

    fn power(id: &str, group: u8, bonus: Option<(PowerBonus, i32)>) -> ItemPowerDef {
        ItemPowerDef {
            num: 0,
            id: id.into(),
            name: id.into(),
            group,
            cost: 50,
            kinds: vec![ItemKind::Sword],
            realm: None,
            books: 0,
            bonus,
            spell: None,
            attack: None,
        }
    }

    fn group(powers: &[ItemPowerDef]) -> impl Fn(&str) -> Option<u8> + '_ {
        |id| powers.iter().find(|p| p.id == id).map(|p| p.group)
    }

    #[test]
    fn a_plain_power_replaces_the_one_of_its_group() {
        let all = [
            power("a1", 0, Some((PowerBonus::Attack, 1))),
            power("a2", 0, Some((PowerBonus::Attack, 2))),
            power("d1", 2, Some((PowerBonus::Defense, 1))),
        ];
        let mut ui = CraftUi::new(Craft::EnchantItem);
        let g = group(&all);
        toggle_power(&mut ui.design, &all[0], &g).unwrap();
        toggle_power(&mut ui.design, &all[2], &g).unwrap();
        toggle_power(&mut ui.design, &all[1], &g).unwrap();
        assert_eq!(ui.design.powers, ["d1", "a2"]);
        toggle_power(&mut ui.design, &all[1], &g).unwrap();
        assert_eq!(ui.design.powers, ["d1"]);
    }

    #[test]
    fn four_places_are_all_there_are_and_the_charges_take_one() {
        let all: Vec<_> = (0..5).map(|i| power(&format!("s{i}"), 9, None)).collect();
        let mut ui = CraftUi::new(Craft::CreateArtifact);
        ui.design.charges = Some(("fire_bolt".into(), 2));
        let g = group(&all);
        for p in &all[..3] {
            toggle_power(&mut ui.design, p, &g).unwrap();
        }
        assert_eq!(ui.used(), 4);
        assert_eq!(
            toggle_power(&mut ui.design, &all[3], &g),
            Err("craft.too_many")
        );
    }

    #[test]
    fn the_picture_steps_round_the_kinds_range() {
        let mut ui = CraftUi::new(Craft::EnchantItem);
        let (first, last) = ItemKind::Sword.icon_range();
        ui.step_icon(false);
        assert_eq!(ui.design.icon, last);
        ui.step_icon(true);
        assert_eq!(ui.design.icon, first);
        ui.step_icon(true);
        assert_eq!(ui.design.icon, first + 1);
    }

    #[test]
    fn a_new_kind_keeps_the_powers_it_can_have() {
        let mut ui = CraftUi::new(Craft::EnchantItem);
        ui.design.powers = vec!["a1".into(), "d1".into()];
        ui.design.charges = Some(("fire_bolt".into(), 1));
        ui.set_kind(ItemKind::Shield, &["d1".to_string()]);
        assert_eq!(ui.design.powers, ["d1"]);
        assert_eq!(ui.design.charges, None);
        assert_eq!(ui.design.icon, ItemKind::Shield.icon_range().0);
    }

    #[test]
    fn plain_powers_split_in_two_columns_at_a_group_and_the_special_ones_stand_apart() {
        let mut all: Vec<ItemPowerDef> = Vec::new();
        for g in 0..6u8 {
            for n in 0..4 {
                all.push(power(
                    &format!("p{g}{n}"),
                    g,
                    Some((PowerBonus::Attack, n + 1)),
                ));
            }
        }
        all.push(power("flaming", 9, None));
        let refs: Vec<_> = all.iter().collect();
        let [a, b, c] = columns(&refs);
        assert!(a.len() <= COLUMN_ROWS);
        assert_eq!(a.last().map(|r| *r != Row::Gap), Some(true));
        assert_eq!(
            a.len() + b.len(),
            4 * 6 + 5 - 1 /* the gap at the break */
        );
        assert_eq!(b.first().map(|r| *r != Row::Gap), Some(true));
        assert_eq!(c, [Row::Power("flaming".into())]);
    }
}
