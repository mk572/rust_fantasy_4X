//! Drawing and handling the unit statistics sheet (task 140; the data is in `unit_sheet.rs`).
//!
//! The sheet is the original's `Unit_Statistics_Popup`: a teal panel (`unitview` 1) with the
//! unit on a diamond, its name, moves and upkeep, five rows of icons (melee, range, armour,
//! resistance, hits) and a list of abilities and lasting spells, eight to a page. On the map it
//! has the Dismiss and OK buttons on a second panel (`unitview` 2) to the right; in battle it
//! is the panel alone. Positions are the original's, from the panel's top-left (`UnitStat.c`).

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::ui;
use f4x_core::battle::Battle;
use f4x_core::game::Command;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::unit_sheet::{hit_cells, special_picture, stat_cells, Cell, Row, Sheet, Source};
use crate::view::{App, Screen};

/// The panel's top-left on the map and in battle.
const MAP_AT: (f32, f32) = (31.0, 6.0);
const BATTLE_AT: (f32, f32) = (61.0, 6.0);
/// Icons in a stat row before it wraps to a second row.
const ROW: i32 = 15;
const PER_PAGE: usize = 8;

/// What the abilities page of a sheet panel remembers, on the sheet screen and where the same
/// panel is shown inside another screen (production, the hire pop-ups).
#[derive(Debug, Clone, Default)]
pub(crate) struct SheetPanel {
    /// The first line of the abilities page shown.
    pub first: usize,
    /// Dismiss was clicked and waits for a yes.
    pub confirm: bool,
    /// A lasting spell of the viewer's was clicked and waits for a yes.
    pub turn_off: Option<TurnOff>,
}

/// The unit sheet screen.
#[derive(Debug, Clone)]
pub(crate) struct SheetUi {
    pub sheet: Sheet,
    /// The screen to return to.
    pub back: Screen,
    pub panel: SheetPanel,
    /// The units of the square the sheet was opened from, for the arrows to step through
    /// (empty: a sheet of one unit).
    pub group: Vec<hecs::Entity>,
}

/// Which of the sheet's panels is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelKind {
    /// With the small panel for the buttons to the right (the map, the hire pop-ups).
    WithButtons,
    /// The panel alone (in a battle).
    Bare,
    /// As the production screen shows it.
    Production,
}

/// The question "turn off this spell?" (`UV_Remove_Unit_Enchantment`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TurnOff {
    pub spell: f4x_core::rules::SpellId,
    pub name: String,
    /// Land units that drown when the spell goes (this one and perhaps others).
    pub drowned: usize,
}

impl App {
    /// Open the sheet of a unit on the map (right-click on it in the stack).
    pub(crate) fn open_unit_sheet(&mut self, unit: hecs::Entity) {
        self.open_unit_sheet_of(unit, Vec::new());
    }

    /// Open the sheet of `unit`, one of the `group` standing on a square: the Left and Right
    /// arrows (or the buttons) step through the group's sheets.
    pub(crate) fn open_unit_sheet_of(&mut self, unit: hecs::Entity, group: Vec<hecs::Entity>) {
        if let Some(sheet) = self.overland_sheet(unit) {
            let group = if group.len() > 1 { group } else { Vec::new() };
            self.show_sheet(sheet, group, None);
        }
    }

    /// Open the sheet of a unit in the battle.
    pub(crate) fn open_battle_sheet(&mut self, battle: &Battle, id: usize) {
        if let Some(sheet) = self.battle_sheet(battle, id) {
            self.show_sheet(sheet, Vec::new(), None);
        }
    }

    /// For the screenshot check: open the sheet of unit `n` of the starting stack, or of the
    /// test battle when one is on. `spell` is put on the unit first (cast by you), and `ask`
    /// clicks its line so the question to turn it off is open.
    pub fn open_test_sheet(&mut self, n: usize, spell: Option<&str>, ask: bool) {
        if let Some(battle) = self.game.battle().cloned() {
            self.open_battle_sheet(&battle, n);
            return;
        }
        if self.selection.is_empty() {
            self.select_first_stack();
        }
        let Some(&unit) = self.selection.get(n) else {
            return;
        };
        if spell == Some("temp") {
            // Two temporary effects, to see their lines (task 152).
            let who = f4x_core::rules::exec::Target::Unit(unit);
            let plus = f4x_core::rules::Modifier::new(
                f4x_core::rules::Stat::Melee,
                f4x_core::rules::ModOp::Add,
                2,
                f4x_core::rules::Source::Other,
            );
            self.game.add_temp_modifier(who, plus, Some(3));
            self.game.grant_temp_ability(who, "first_strike", None);
        }
        let spell = spell.and_then(|id| self.game.content().rules.spells.id_of(id));
        if let Some(spell) = spell {
            let who = f4x_core::rules::exec::Target::Unit(unit);
            self.game.attach_enchantment(who, spell, crate::view::HUMAN);
        }
        let Some(sheet) = self.overland_sheet(unit) else {
            return;
        };
        let turn_off = match (spell, ask) {
            (Some(spell), true) => Some(TurnOff {
                spell,
                name: self.game.content().rules.spells.get(spell).name.clone(),
                drowned: self.game.drowned_if_off(unit, spell),
            }),
            _ => None,
        };
        self.show_sheet(sheet, Vec::new(), turn_off);
    }

    fn show_sheet(&mut self, sheet: Sheet, group: Vec<hecs::Entity>, turn_off: Option<TurnOff>) {
        let ui = SheetUi {
            sheet,
            back: self.screen,
            panel: SheetPanel {
                turn_off,
                ..Default::default()
            },
            group,
        };
        self.open_page(Screen::UnitSheet, ui);
    }

    /// The sheet's panel with everything on it but the buttons: the unit on its diamond, name,
    /// moves, upkeep, the stat rows and the abilities list. `second_panel` adds the small panel
    /// for the buttons to the right (on the map, and in the hire pop-ups).
    pub(crate) fn draw_sheet_panel(
        &mut self,
        lay: &Layout,
        sheet: &Sheet,
        at: (f32, f32),
        second_panel: bool,
        panel: &mut SheetPanel,
        input: &Input,
    ) {
        let kind = if second_panel {
            PanelKind::WithButtons
        } else {
            PanelKind::Bare
        };
        self.draw_sheet_panel_as(lay, sheet, at, kind, panel, input);
    }

    /// The same panel as the production screen shows it (`Thing_View_Draw` called from the
    /// production screen): the small background, everything four pixels to the right, the top
    /// block higher, a stat row ten icons long and no Dismiss panel. The caller adds the cost.
    pub(crate) fn draw_production_sheet(
        &mut self,
        lay: &Layout,
        sheet: &Sheet,
        at: (f32, f32),
        panel: &mut SheetPanel,
        input: &Input,
    ) {
        self.draw_sheet_panel_as(lay, sheet, at, PanelKind::Production, panel, input);
    }

    fn draw_sheet_panel_as(
        &mut self,
        lay: &Layout,
        sheet: &Sheet,
        (px, py): (f32, f32),
        kind: PanelKind,
        panel: &mut SheetPanel,
        input: &Input,
    ) {
        let second_panel = kind == PanelKind::WithButtons;
        let production = kind == PanelKind::Production;
        let (ox, row_length) = if production { (4.0, 10) } else { (0.0, ROW) };
        let (name_y, moves_y, upkeep_y) = if production {
            (7.0, 20.0, 27.0)
        } else {
            (11.0, 27.0, 34.0)
        };
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        // The panels.
        if self
            .draw_pic(lay, "unitview", usize::from(!production), 0, px, py)
            .is_none()
        {
            let r = lay.rect((px, py, 218.0, 183.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.35, 0.35, 1.0));
        }
        if second_panel {
            self.draw_pic(lay, "unitview", 2, 0, px + 213.0, py + 133.0);
        }

        // The unit on its diamond.
        if sheet.water {
            self.draw_pic(lay, "unitview", 34, 0, px + ox + 6.0, py + 18.0);
        } else {
            self.draw_pic(lay, "cityscap", 103, 0, px + ox + 5.0, py + 5.0);
        }
        self.draw_sheet_figures(lay, sheet, px + ox + 11.0, py - 3.0);

        // Name: a race's unit shows the race first.
        let name_x = px + ox + 51.0;
        match &sheet.race {
            Some(race) => {
                let w = self.draw_styled(view, &Style::nearest(4, gold), race, name_x, py + name_y);
                self.draw_styled(
                    view,
                    &Style::nearest(4, gold),
                    &sheet.name,
                    name_x + w + 5.0,
                    py + name_y,
                );
            }
            None => {
                self.draw_styled(
                    view,
                    &Style::nearest(4, gold),
                    &sheet.name,
                    name_x,
                    py + name_y,
                );
            }
        }

        // Moves, and upkeep or damage.
        let moves = self.text("unit.moves");
        self.draw_styled(
            view,
            &Style::nearest(1, light),
            &moves,
            name_x,
            py + moves_y,
        );
        for i in 0..sheet.moves {
            self.draw_stat_icon(
                lay,
                sheet.move_icon,
                false,
                px + ox + 84.0 + 8.0 * i as f32,
                py + moves_y - 2.0,
            );
        }
        if let Some([gold_up, mana_up, food_up]) = sheet.upkeep {
            let word = self.text("army.upkeep");
            self.draw_styled(
                view,
                &Style::nearest(1, light),
                &word,
                name_x,
                py + upkeep_y,
            );
            let (mut x, y) = (px + ox + 85.0, py + upkeep_y - 1.0);
            // Gold, then mana, then food, each a small icon from `backgrnd`.
            for (count, entry) in [(gold_up, 42), (mana_up, 43), (food_up, 40)] {
                for _ in 0..count.max(0) {
                    self.draw_pic(lay, "backgrnd", entry, 0, x, y);
                    x += 8.0;
                }
            }
        } else if let Some(damage) = sheet.damage {
            let word = self.text("unit.damage");
            self.draw_styled(
                view,
                &Style::nearest(1, light),
                &word,
                name_x,
                py + upkeep_y,
            );
            self.draw_styled(
                view,
                &Style::nearest(1, light).right(),
                &damage.to_string(),
                px + 90.0,
                py + upkeep_y,
            );
        }

        // The five rows.
        let rows: [(String, f32, Option<Row>); 4] = [
            (self.text("unit.melee"), 47.0, Some(sheet.melee)),
            (self.text("unit.range"), 58.0, sheet.ranged),
            (self.text("unit.armor"), 68.0, Some(sheet.defense)),
            (self.text("unit.resist"), 79.0, Some(sheet.resist)),
        ];
        for (label, y, row) in rows {
            self.draw_styled(
                view,
                &Style::nearest(4, gold),
                &label,
                px + ox + 8.0,
                py + y,
            );
            if let Some(row) = row {
                for cell in stat_cells(row, row_length) {
                    self.draw_cell(lay, row.icon, cell, px + ox + 49.0, py + y);
                }
            }
        }
        let hits = self.text("unit.hits");
        self.draw_styled(
            view,
            &Style::nearest(4, gold),
            &hits,
            px + ox + 8.0,
            py + 90.0,
        );
        let bonus = sheet.hits.gold;
        for cell in hit_cells(sheet.hits.value, bonus, sheet.hurt, row_length) {
            self.draw_cell(lay, sheet.hits.icon, cell, px + ox + 49.0, py + 90.0);
        }

        // The abilities and spells, a page at a time.
        let total = sheet.specials.len();
        let multipage = total > PER_PAGE;
        let mut first = panel.first.min(total.saturating_sub(1));
        let shown = PER_PAGE.min(total - first.min(total));
        for (i, special) in sheet.specials.iter().skip(first).take(shown).enumerate() {
            let (col, row) = (i / 4, i % 4);
            let x = px + ox + 8.0 + 107.0 * col as f32;
            let y = py + 108.0 + 17.0 * row as f32;
            self.draw_pic(lay, "unitview", 5 + i, 0, x - 1.0, y - 1.0);
            if let Some(icon) = special.icon {
                let (lbx, entry) = special_picture(icon);
                self.draw_pic(lay, lbx, entry, 0, x, y);
            }
            self.draw_styled(
                view,
                &Style::nearest(1, light),
                &special.name,
                x + 19.0,
                y + 5.0,
            );
            let asking = panel.confirm || panel.turn_off.is_some();
            if let (true, false, Some(spell), Source::Overland(unit)) =
                (special.removable, asking, special.spell, sheet.source)
            {
                let line = lay.rect((x - 1.0, y - 1.0, 105.0, 17.0));
                if ui::hit(line, true, input).clicked() {
                    let drowned = self.game.drowned_if_off(unit, spell);
                    panel.turn_off = Some(TurnOff {
                        spell,
                        name: special.name.clone(),
                        drowned,
                    });
                }
            }
        }
        if multipage {
            let up = self.arrow_button(
                lay,
                3,
                px + if production { 156.0 } else { 204.0 },
                py + 105.0,
                input,
            );
            let down = self.arrow_button(
                lay,
                4,
                px + if production { 156.0 } else { 204.0 },
                py + 168.0,
                input,
            );
            if (up || input.key_pressed(KeyCode::PageUp)) && first > 0 {
                first = first.saturating_sub(PER_PAGE);
            }
            if (down || input.key_pressed(KeyCode::PageDown)) && first + PER_PAGE < total {
                first += PER_PAGE;
            }
            panel.first = first;
        }
    }

    /// The unit's figures on the diamond; the overland picture stands in without the combat
    /// figures.
    fn draw_sheet_figures(&mut self, lay: &Layout, sheet: &Sheet, x: f32, y: f32) {
        let count = sheet.figures.max(1) as usize;
        if let Some(ty) = sheet.unit_type {
            self.draw_figures(lay, ty, count, sheet.banner, x, y);
        }
    }

    /// `figures` of unit type `unit_type` standing on a diamond at `(x, y)`
    /// (`Draw_Unit_Figure`): facing east, back figures first.
    pub(crate) fn draw_figures(
        &mut self,
        lay: &Layout,
        unit_type: usize,
        figures: usize,
        banner: u8,
        x: f32,
        y: f32,
    ) {
        let Some(tex) = self.art.figure(unit_type, 2, 2, banner).cloned() else {
            return;
        };
        let mut spots: Vec<(i32, i32)> = (0..figures)
            .map(|i| crate::unit_sheet::figure_spot(figures, i))
            .collect();
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
    fn arrow_button(&mut self, lay: &Layout, entry: usize, x: f32, y: f32, input: &Input) -> bool {
        self.pic_button_on(lay, ("unitview", entry), (x, y), input)
    }

    /// A red button of the original (`backgrnd` 24) with its label; true when clicked.
    pub(crate) fn red_button(
        &mut self,
        lay: &Layout,
        label: &str,
        x: f32,
        y: f32,
        input: &Input,
    ) -> bool {
        self.red_button_face(lay, label, x, y, true, input)
    }

    /// A red button of another width (the picture is stretched).
    fn red_button_sized(
        &mut self,
        lay: &Layout,
        label: &str,
        x: f32,
        y: f32,
        w: f32,
        input: &Input,
    ) -> bool {
        self.red_button_face_sized(lay, label, (x, y), true, w, input)
    }

    fn red_button_face(
        &mut self,
        lay: &Layout,
        label: &str,
        x: f32,
        y: f32,
        enabled: bool,
        input: &Input,
    ) -> bool {
        self.red_button_face_sized(lay, label, (x, y), enabled, 49.0, input)
    }

    fn red_button_face_sized(
        &mut self,
        lay: &Layout,
        label: &str,
        (x, y): (f32, f32),
        enabled: bool,
        w: f32,
        input: &Input,
    ) -> bool {
        let h = 12.0;
        let r = lay.rect((x, y, w, h));
        let state = ui::hit(r, enabled, input);
        let down = state.held();
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
        self.draw_styled(
            view,
            &Style::nearest(1, color).center(),
            label,
            x + w / 2.0,
            y + 3.0 + f32::from(u8::from(down)),
        );
        state.clicked()
    }
}
impl SheetUi {
    /// Show the sheet of the unit `step` places on in the group (wrapping round).
    fn step_sheet(&mut self, app: &mut App, step: isize) {
        let Source::Overland(current) = self.sheet.source else {
            return;
        };
        let group = self.group.clone();
        let Some(at) = group.iter().position(|&u| u == current) else {
            return;
        };
        let next = group[(at as isize + step).rem_euclid(group.len() as isize) as usize];
        if let Some(sheet) = app.overland_sheet(next) {
            self.sheet = sheet;
            self.panel.first = 0;
        }
    }

    fn close_sheet(&mut self, app: &mut App) {
        app.screen = self.back;
    }

    /// Ask whether to dismiss the unit (the original's confirmation box); Yes sends the
    /// command and leaves.
    fn dismiss_confirmation(&mut self, app: &mut App, lay: &Layout, sheet: &Sheet, input: &Input) {
        let text = app.text_with("unit.disband", &Args::new().text("unit", &sheet.name));
        match app.confirmation_box(lay, &text, input) {
            Some(true) => {
                if let Source::Overland(unit) = sheet.source {
                    let events = app.game.apply(Command::DismissUnit { unit });
                    app.note(events);
                    app.selection.retain(|&u| u != unit);
                }
                self.panel.confirm = false;
                self.close_sheet(app);
            }
            Some(false) => self.panel.confirm = false,
            None => {}
        }
    }

    /// Ask whether to turn off a lasting spell of the wizard's; Yes sends the command. The
    /// question and the warning are the original's words.
    fn turn_off_confirmation(&mut self, app: &mut App, lay: &Layout, sheet: &Sheet, input: &Input) {
        let (Some(ask), Source::Overland(unit)) = (self.panel.turn_off.clone(), sheet.source)
        else {
            self.panel.turn_off = None;
            return;
        };
        let mut text = app.text_with("sell.turn_off", &Args::new().text("spell", &ask.name));
        match ask.drowned {
            0 => {}
            1 => text.push_str(&app.text("unit.dies_one")),
            _ => text.push_str(&app.text("unit.dies_many")),
        }
        match app.confirmation_box(lay, &text, input) {
            Some(true) => {
                let events = app.game.apply(Command::TurnOffUnitSpell {
                    player: crate::view::HUMAN,
                    unit,
                    spell: ask.spell,
                });
                app.note(events);
                self.panel.turn_off = None;
                // The unit may be gone; otherwise the sheet is drawn again without the line.
                if app.game.units().unit_position(unit).is_none() {
                    app.selection.retain(|&u| u != unit);
                    self.close_sheet(app);
                } else if let Some(fresh) = app.overland_sheet(unit) {
                    self.sheet = fresh;
                    self.panel.first = 0;
                }
            }
            Some(false) => self.panel.turn_off = None,
            None => {}
        }
    }
}

impl Page for SheetUi {
    fn melee_icon(&self) -> Option<usize> {
        Some(self.sheet.melee.icon)
    }

    /// The help topic of the ability or spell line at `(x, y)` (320 by 200 coordinates) of the
    /// open sheet, with the line's area: a right-click on it opens the topic named like it
    /// (`UV_Set_Specials_Help_Fields`).
    fn help_at(&self, app: &App, x: i32, y: i32) -> Option<(u16, (f32, f32, f32, f32))> {
        let sheet = &self.sheet;
        let (px, py) = if matches!(sheet.source, Source::Battle(_)) {
            BATTLE_AT
        } else {
            MAP_AT
        };
        let first = self.panel.first.min(sheet.specials.len().saturating_sub(1));
        for (i, special) in sheet.specials.iter().skip(first).take(PER_PAGE).enumerate() {
            let (col, row) = (i / 4, i % 4);
            let left = px + 7.0 + 107.0 * col as f32;
            let top = py + 107.0 + 17.0 * row as f32;
            let area = (left, top, 105.0, 17.0);
            let inside = (x as f32 >= area.0 && (x as f32) < area.0 + area.2)
                && (y as f32 >= area.1 && (y as f32) < area.1 + area.3);
            if inside {
                let topic = app.help.topic_for(special.subject.as_deref()?)?;
                return Some((topic.index, area));
            }
        }
        None
    }

    fn frame(&mut self, app: &mut App, input: &Input) {
        let sheet = self.sheet.clone();
        let in_battle = matches!(sheet.source, Source::Battle(_));
        let (px, py) = if in_battle { BATTLE_AT } else { MAP_AT };
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let lay = Layout::new();

        // A box that opens this frame waits for the next, so its own click does not answer it.
        let (was_confirming, was_turning_off) = (self.panel.confirm, self.panel.turn_off.is_some());
        app.draw_sheet_panel(&lay, &sheet, (px, py), !in_battle, &mut self.panel, input);

        // Buttons and leaving.
        let mut leave = input.key_pressed(KeyCode::Escape);
        if !in_battle {
            let (ok_label, dismiss_label) = (app.text("ui.ok"), app.text("unit.dismiss"));
            let ok = app.red_button(&lay, &ok_label, px + 222.0, py + 162.0, input);
            let dismiss = sheet.can_dismiss
                && app.red_button(&lay, &dismiss_label, px + 222.0, py + 143.0, input)
                && !self.panel.confirm
                && self.panel.turn_off.is_none();
            if !sheet.can_dismiss {
                app.red_button_face(&lay, &dismiss_label, px + 222.0, py + 143.0, false, input);
            }
            let asking = self.panel.confirm || self.panel.turn_off.is_some();
            if ok || (input.key_pressed(KeyCode::Enter) && !asking) {
                leave = true;
            }
            if dismiss || (sheet.can_dismiss && input.key_pressed(KeyCode::D)) {
                self.panel.confirm = true;
            }
            if self.group.len() > 1 && !asking {
                let n = self.group.len();
                let at = self
                    .group
                    .iter()
                    .position(|&u| matches!(sheet.source, Source::Overland(s) if s == u))
                    .map_or(0, |i| i + 1);
                let view = (lay.origin.x, lay.origin.y, lay.scale);
                app.draw_styled(
                    view,
                    &Style::nearest(1, Color::new(0.95, 0.93, 0.85, 1.0)).center(),
                    &format!("{at}/{n}"),
                    px + 246.5,
                    py + 108.0,
                );
                let back = app.red_button_sized(&lay, "<", px + 222.0, py + 121.0, 24.0, input);
                let forward = app.red_button_sized(&lay, ">", px + 247.0, py + 121.0, 24.0, input);
                if back || input.key_pressed(KeyCode::Left) {
                    self.step_sheet(app, -1);
                } else if forward || input.key_pressed(KeyCode::Right) {
                    self.step_sheet(app, 1);
                }
            }
        } else if input.key_pressed(KeyCode::Enter) {
            leave = true;
        }
        if was_confirming {
            self.dismiss_confirmation(app, &lay, &sheet, input);
            return;
        }
        if was_turning_off {
            self.turn_off_confirmation(app, &lay, &sheet, input);
            return;
        }
        if self.panel.confirm || self.panel.turn_off.is_some() {
            return;
        }
        // A click away from the panel leaves too.
        let panel = lay.rect((px, py, if in_battle { 218.0 } else { 282.0 }, 184.0));
        if input.mouse_pressed(MouseButton::Left) && !ui::hit(panel, true, input).hovered() {
            leave = true;
        }
        if leave {
            self.close_sheet(app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sheet_returns_to_the_screen_it_was_opened_over_and_that_screen_keeps_its_page() {
        let mut app = App::for_test();
        app.open_army_list();
        app.select_first_stack();
        let unit = app.selection[0];
        app.open_unit_sheet(unit);
        assert!(matches!(app.screen, Screen::UnitSheet));
        let page = app.pages.get(&Screen::UnitSheet).unwrap();
        assert!(page.melee_icon().is_some());
        // Opening the sheet over the army list left that list's page where it was, for the
        // sheet to return to.
        assert!(app.pages.get(&Screen::ArmyList).is_some());
    }
}
