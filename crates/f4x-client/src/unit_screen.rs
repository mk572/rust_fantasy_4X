//! Drawing and handling the unit statistics sheet (task 140; the data is in `unit_sheet.rs`).
//!
//! The sheet is the original's `Unit_Statistics_Popup`: a teal panel (`unitview` 1) with the
//! unit on a diamond, its name, moves and upkeep, five rows of icons (melee, range, armour,
//! resistance, hits) and a list of abilities and lasting spells, eight to a page. On the map it
//! has the Dismiss and OK buttons on a second panel (`unitview` 2) to the right; in battle it
//! is the panel alone. Positions are the original's, from the panel's top-left (`UnitStat.c`).

use f4x_core::battle::Battle;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::unit_sheet::{hit_cells, special_picture, stat_cells, Cell, Row, Sheet, Source};
use crate::view::{App, Screen};

/// The panel's top-left on the map and in battle.
const MAP_AT: (f32, f32) = (31.0, 6.0);
const BATTLE_AT: (f32, f32) = (61.0, 6.0);
/// Icons in a stat row before it wraps to a second row.
const ROW: i32 = 15;
const PER_PAGE: usize = 8;

/// What the sheet remembers while it is open.
#[derive(Debug, Clone, Default)]
pub(crate) struct SheetUi {
    pub sheet: Option<Sheet>,
    pub back: Option<Screen>,
    /// The first line of the abilities page shown.
    pub first: usize,
    /// Dismiss was clicked and waits for a yes.
    pub confirm: bool,
}

impl App {
    /// Open the sheet of a unit on the map (right-click on it in the stack).
    pub(crate) fn open_unit_sheet(&mut self, unit: hecs::Entity) {
        if let Some(sheet) = self.overland_sheet(unit) {
            self.show_sheet(sheet);
        }
    }

    /// Open the sheet of a unit in the battle.
    pub(crate) fn open_battle_sheet(&mut self, battle: &Battle, id: usize) {
        if let Some(sheet) = self.battle_sheet(battle, id) {
            self.show_sheet(sheet);
        }
    }

    /// For the screenshot check: open the sheet of unit `n` of the starting stack, or of the
    /// test battle when one is on.
    pub fn open_test_sheet(&mut self, n: usize) {
        if let Some(battle) = self.game.battle().cloned() {
            self.open_battle_sheet(&battle, n);
            return;
        }
        if self.selection.is_empty() {
            self.select_first_stack();
        }
        if let Some(&unit) = self.selection.get(n) {
            self.open_unit_sheet(unit);
        }
    }

    fn show_sheet(&mut self, sheet: Sheet) {
        self.sheet_ui = SheetUi {
            sheet: Some(sheet),
            back: Some(self.screen),
            first: 0,
            confirm: false,
        };
        self.screen = Screen::UnitSheet;
    }

    fn close_sheet(&mut self) {
        self.screen = self.sheet_ui.back.take().unwrap_or(Screen::Map);
        self.sheet_ui.sheet = None;
    }

    pub(crate) fn unit_sheet_screen(&mut self) {
        let Some(sheet) = self.sheet_ui.sheet.clone() else {
            self.close_sheet();
            return;
        };
        let in_battle = matches!(sheet.source, Source::Battle(_));
        let (px, py) = if in_battle { BATTLE_AT } else { MAP_AT };
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let lay = Layout::new();

        self.draw_sheet_panel(&lay, &sheet, (px, py), !in_battle);

        // Buttons and leaving.
        let mut leave = is_key_pressed(KeyCode::Escape);
        if !in_battle {
            let ok = self.red_button(&lay, "OK", px + 222.0, py + 162.0);
            let dismiss = sheet.can_dismiss
                && !self.sheet_ui.confirm
                && self.red_button(&lay, "Dismiss", px + 222.0, py + 143.0);
            if !sheet.can_dismiss {
                self.red_button_face(&lay, "Dismiss", px + 222.0, py + 143.0, false);
            }
            if ok || (is_key_pressed(KeyCode::Enter) && !self.sheet_ui.confirm) {
                leave = true;
            }
            if dismiss || (sheet.can_dismiss && is_key_pressed(KeyCode::D)) {
                self.sheet_ui.confirm = true;
            }
        } else if is_key_pressed(KeyCode::Enter) {
            leave = true;
        }
        if self.sheet_ui.confirm {
            self.dismiss_confirmation(&lay, &sheet);
            return;
        }
        // A click away from the panel leaves too.
        let panel = lay.rect((px, py, if in_battle { 218.0 } else { 282.0 }, 184.0));
        if is_mouse_button_pressed(MouseButton::Left) && !panel.contains(mouse_position().into()) {
            leave = true;
        }
        if leave {
            self.close_sheet();
        }
    }

    /// The sheet's panel with everything on it but the buttons: the unit on its diamond, name,
    /// moves, upkeep, the stat rows and the abilities list. `second_panel` adds the small panel
    /// for the buttons to the right (on the map, and in the hire pop-ups).
    pub(crate) fn draw_sheet_panel(
        &mut self,
        lay: &Layout,
        sheet: &Sheet,
        (px, py): (f32, f32),
        second_panel: bool,
    ) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        // The panels.
        if self.draw_pic(lay, "unitview", 1, 0, px, py).is_none() {
            let r = lay.rect((px, py, 218.0, 183.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.35, 0.35, 1.0));
        }
        if second_panel {
            self.draw_pic(lay, "unitview", 2, 0, px + 213.0, py + 133.0);
        }

        // The unit on its diamond.
        if sheet.water {
            self.draw_pic(lay, "unitview", 34, 0, px + 6.0, py + 18.0);
        } else {
            self.draw_pic(lay, "cityscap", 103, 0, px + 5.0, py + 5.0);
        }
        self.draw_sheet_figures(lay, sheet, px + 11.0, py - 3.0);

        // Name: a race's unit shows the race first.
        let name_x = px + 51.0;
        match &sheet.race {
            Some(race) => {
                let w = self.bm_text(view, 4, gold, race, name_x, py + 11.0, Align::Left);
                self.bm_text(
                    view,
                    4,
                    gold,
                    &sheet.name,
                    name_x + w + 5.0,
                    py + 11.0,
                    Align::Left,
                );
            }
            None => {
                self.bm_text(view, 4, gold, &sheet.name, name_x, py + 11.0, Align::Left);
            }
        }

        // Moves, and upkeep or damage.
        self.bm_text(view, 1, light, "Moves", name_x, py + 27.0, Align::Left);
        for i in 0..sheet.moves {
            self.draw_stat_icon(
                lay,
                sheet.move_icon,
                false,
                px + 84.0 + 8.0 * i as f32,
                py + 25.0,
            );
        }
        if let Some([gold_up, mana_up, food_up]) = sheet.upkeep {
            self.bm_text(view, 1, light, "Upkeep", name_x, py + 34.0, Align::Left);
            let (mut x, y) = (px + 85.0, py + 33.0);
            // Gold, then mana, then food, each a small icon from `backgrnd`.
            for (count, entry) in [(gold_up, 42), (mana_up, 43), (food_up, 40)] {
                for _ in 0..count.max(0) {
                    self.draw_pic(lay, "backgrnd", entry, 0, x, y);
                    x += 8.0;
                }
            }
        } else if let Some(damage) = sheet.damage {
            self.bm_text(view, 1, light, "Damage", name_x, py + 34.0, Align::Left);
            self.bm_text(
                view,
                1,
                light,
                &damage.to_string(),
                px + 90.0,
                py + 34.0,
                Align::Right,
            );
        }

        // The five rows.
        let rows: [(&str, f32, Option<Row>); 4] = [
            ("Melee", 47.0, Some(sheet.melee)),
            ("Range", 58.0, sheet.ranged),
            ("Armor", 68.0, Some(sheet.defense)),
            ("Resist", 79.0, Some(sheet.resist)),
        ];
        for (label, y, row) in rows {
            self.bm_text(view, 4, gold, label, px + 8.0, py + y, Align::Left);
            if let Some(row) = row {
                for cell in stat_cells(row, ROW) {
                    self.draw_cell(lay, row.icon, cell, px + 49.0, py + y);
                }
            }
        }
        self.bm_text(view, 4, gold, "Hits", px + 8.0, py + 90.0, Align::Left);
        let bonus = sheet.hits.gold;
        for cell in hit_cells(sheet.hits.value, bonus, sheet.hurt, ROW) {
            self.draw_cell(lay, sheet.hits.icon, cell, px + 49.0, py + 90.0);
        }

        // The abilities and spells, a page at a time.
        let total = sheet.specials.len();
        let multipage = total > PER_PAGE;
        let mut first = self.sheet_ui.first.min(total.saturating_sub(1));
        let shown = PER_PAGE.min(total - first.min(total));
        for (i, special) in sheet.specials.iter().skip(first).take(shown).enumerate() {
            let (col, row) = (i / 4, i % 4);
            let x = px + 8.0 + 107.0 * col as f32;
            let y = py + 108.0 + 17.0 * row as f32;
            self.draw_pic(lay, "unitview", 5 + i, 0, x - 1.0, y - 1.0);
            if let Some(icon) = special.icon {
                let (lbx, entry) = special_picture(icon);
                self.draw_pic(lay, lbx, entry, 0, x, y);
            }
            self.bm_text(
                view,
                1,
                light,
                &special.name,
                x + 19.0,
                y + 5.0,
                Align::Left,
            );
        }
        if multipage {
            let up = self.arrow_button(lay, 3, px + 204.0, py + 105.0);
            let down = self.arrow_button(lay, 4, px + 204.0, py + 168.0);
            if (up || is_key_pressed(KeyCode::PageUp)) && first > 0 {
                first = first.saturating_sub(PER_PAGE);
            }
            if (down || is_key_pressed(KeyCode::PageDown)) && first + PER_PAGE < total {
                first += PER_PAGE;
            }
            self.sheet_ui.first = first;
        }
    }

    /// Ask whether to dismiss the unit; Yes sends the command and leaves.
    fn dismiss_confirmation(&mut self, lay: &Layout, sheet: &Sheet) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let r = lay.rect((60.0, 70.0, 200.0, 56.0));
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.06, 0.18, 0.2, 0.98));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(0.8, 0.65, 0.3, 1.0));
        self.bm_text(
            view,
            1,
            light,
            &format!("Do you wish to disband the unit of {}?", sheet.name),
            160.0,
            78.0,
            Align::Center,
        );
        let yes = self.red_button(lay, "Yes", 92.0, 100.0);
        let no = self.red_button(lay, "No", 180.0, 100.0);
        if yes || is_key_pressed(KeyCode::Y) {
            if let Source::Overland(unit) = sheet.source {
                let events = self.game.apply(Command::DismissUnit { unit });
                self.note(events);
                self.selection.retain(|&u| u != unit);
            }
            self.sheet_ui.confirm = false;
            self.close_sheet();
        } else if no || is_key_pressed(KeyCode::N) || is_key_pressed(KeyCode::Escape) {
            self.sheet_ui.confirm = false;
        }
    }

    /// The unit's figures on the diamond; the overland picture stands in without the combat
    /// figures.
    fn draw_sheet_figures(&mut self, lay: &Layout, sheet: &Sheet, x: f32, y: f32) {
        let count = sheet.figures.max(1) as usize;
        let figure = sheet.unit_type.and_then(|ty| {
            // The sheet shows the figure facing east, standing.
            self.art.figure(ty, 2, 2, sheet.banner).cloned()
        });
        let Some(tex) = figure else {
            return;
        };
        let mut spots: Vec<(i32, i32)> = (0..count)
            .map(|i| crate::unit_sheet::figure_spot(count, i))
            .collect();
        // Back figures first.
        spots.sort_by_key(|&(_, fy)| fy);
        for (fx, fy) in spots {
            let at = lay.point(x + fx as f32, y + fy as f32);
            draw_texture_ex(
                &tex,
                at.x,
                at.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(tex.size() * lay.scale),
                    ..Default::default()
                },
            );
        }
    }

    /// One 9 by 8 stat icon (`unitview` 13 on, gold ones 35 on).
    fn draw_stat_icon(&mut self, lay: &Layout, icon: usize, gold: bool, x: f32, y: f32) {
        let entry = if gold { 35 + icon } else { 13 + icon };
        self.draw_pic(lay, "unitview", entry, 0, x, y);
    }

    fn draw_cell(&mut self, lay: &Layout, icon: usize, cell: Cell, x: f32, y: f32) {
        let (cx, cy) = (x + cell.x as f32, y + cell.y as f32);
        self.draw_stat_icon(lay, icon, cell.gold, cx, cy);
        if cell.dark {
            let r = lay.rect((cx, cy, 9.0, 8.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.55));
        }
    }

    /// The up and down arrows of the abilities list (`unitview` 3 and 4); true when clicked.
    fn arrow_button(&mut self, lay: &Layout, entry: usize, x: f32, y: f32) -> bool {
        let down = self.art.picture("unitview", entry, 0).cloned();
        let Some(tex) = down else {
            return false;
        };
        let r = lay.rect((x, y, tex.width(), tex.height()));
        let hover = r.contains(mouse_position().into());
        let frame = usize::from(hover && is_mouse_button_down(MouseButton::Left));
        self.draw_pic(lay, "unitview", entry, frame, x, y);
        hover && is_mouse_button_pressed(MouseButton::Left)
    }

    /// A red button of the original (`backgrnd` 24) with its label; true when clicked.
    pub(crate) fn red_button(&mut self, lay: &Layout, label: &str, x: f32, y: f32) -> bool {
        self.red_button_face(lay, label, x, y, true)
    }

    fn red_button_face(
        &mut self,
        lay: &Layout,
        label: &str,
        x: f32,
        y: f32,
        enabled: bool,
    ) -> bool {
        let (w, h) = (49.0, 12.0);
        let r = lay.rect((x, y, w, h));
        let hover = enabled && r.contains(mouse_position().into());
        let down = hover && is_mouse_button_down(MouseButton::Left);
        let tint = if enabled {
            WHITE
        } else {
            Color::new(1.0, 1.0, 1.0, 0.5)
        };
        if self
            .draw_pic_tinted(lay, "backgrnd", 24, usize::from(down), x, y, tint)
            .is_none()
        {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.5, 0.15, 0.1, 1.0));
        }
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let color = if enabled {
            Color::new(0.95, 0.93, 0.85, 1.0)
        } else {
            Color::new(0.6, 0.6, 0.6, 1.0)
        };
        self.bm_text(
            view,
            1,
            color,
            label,
            x + w / 2.0,
            y + 3.0 + f32::from(u8::from(down)),
            Align::Center,
        );
        hover && is_mouse_button_pressed(MouseButton::Left)
    }
}
