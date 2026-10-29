//! The Army list (task 138), the original's `ArmyList_Screen` (`ArmyList.c`): every stack of the
//! wizard, six rows of up to nine units on the original's stone picture (`armylist.lbx`), the
//! unit under the mouse named below with its lasting spells, a small map showing where it
//! stands, and the army's upkeep. A left-click on a unit selects its stack and goes to it on the
//! map; a right-click opens the unit's sheet. U and D (or the arrows, or the wheel) scroll, Esc
//! or OK leaves. Positions are the original's.

use crate::font::Style;
use crate::input::Input;
use crate::ui;
use f4x_core::map::Position;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::{Disabled, Layout};
use crate::view::{App, Screen, HUMAN};

/// Rows of stacks on the screen.
const ROWS: usize = 6;
/// Where the first unit square is, and the steps between squares and rows.
const START: (f32, f32) = (77.0, 24.0);
const STEP: (f32, f32) = (18.0, 22.0);
/// The small map's place and size.
const MAP: (f32, f32, f32, f32) = (85.0, 163.0, 49.0, 33.0);

/// What the list remembers while it is open.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ArmyUi {
    /// The first stack shown.
    pub first: usize,
    /// The unit under the mouse (stack, place in it), kept when the mouse is elsewhere.
    pub scanned: (usize, usize),
}

/// Group units into stacks by square, in the order of their first unit.
pub(crate) fn group_stacks<T: Copy>(units: &[(T, Position)]) -> Vec<Vec<T>> {
    let mut stacks: Vec<(Position, Vec<T>)> = Vec::new();
    for &(unit, at) in units {
        match stacks.iter_mut().find(|(p, _)| *p == at) {
            Some((_, list)) => list.push(unit),
            None => stacks.push((at, vec![unit])),
        }
    }
    stacks.into_iter().map(|(_, list)| list).collect()
}

impl App {
    /// The wizard's stacks: units on the same square, nine at most each.
    fn army_stacks(&self) -> Vec<Vec<hecs::Entity>> {
        let units = self.game.units();
        let placed: Vec<_> = units
            .player_units(HUMAN)
            .into_iter()
            .filter_map(|u| Some((u, units.unit_position(u)?)))
            .collect();
        group_stacks(&placed)
    }

    /// Gold, mana and food the army costs each turn.
    fn army_upkeep(&self) -> [i32; 3] {
        let units = self.game.units();
        let rules = &self.game.content().rules;
        let mut out = [0; 3];
        for u in units.player_units(HUMAN) {
            out[0] += units.unit_gold_upkeep(u);
            out[2] += units.unit_food_upkeep(u);
            if let Some(def) = units.unit_def(u) {
                out[1] += def.upkeep.mana as i32;
            }
            for (spell, caster) in units.unit_enchantments(u) {
                if caster == HUMAN {
                    out[1] += rules.spells.get(spell).upkeep as i32;
                }
            }
        }
        out
    }

    pub(crate) fn open_army_list(&mut self) {
        self.army_ui = ArmyUi::default();
        self.screen = Screen::ArmyList;
    }

    pub(crate) fn army_screen(&mut self, input: &Input) {
        let stacks = self.army_stacks();
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let gold = Color::new(0.95, 0.75, 0.25, 1.0);
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        if self.draw_pic(&lay, "armylist", 0, 0, 0.0, 0.0).is_none() {
            let r = lay.rect((0.0, 0.0, 320.0, 200.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.2, 0.2, 0.3, 1.0));
        }
        let count = stacks.len();
        let mut ui = self.army_ui;
        ui.first = ui.first.min(count.saturating_sub(ROWS));

        // Title and upkeep.
        let owner = self
            .game
            .wizard(HUMAN)
            .map_or_else(String::new, |w| w.name());
        let title = self.text_with("army.title", &Args::new().text("owner", owner));
        self.draw_styled(view, &Style::outlined(4).center(), &title, 160.0, 7.0);
        let upkeep_label = self.text("army.upkeep");
        self.draw_styled(
            view,
            &Style::nearest(1, light).center(),
            &upkeep_label,
            30.0,
            162.0,
        );
        let upkeep = self.army_upkeep();
        for (value, y) in upkeep.into_iter().zip([173.0, 183.0, 192.0]) {
            self.draw_styled(
                view,
                &Style::nearest(1, light).center(),
                &value.to_string(),
                45.0,
                y,
            );
        }

        // Scrolling: the arrows, U and D, the wheel.
        let can_up = ui.first > 0;
        let can_down = ui.first + ROWS < count;
        let mut scroll = 0i32;
        for x in [250.0, 60.0] {
            if self.army_button(&lay, 1, x, 26.0, can_up, input) {
                scroll -= 1;
            }
            if self.army_button(&lay, 2, x, 139.0, can_down, input) {
                scroll += 1;
            }
        }
        if input.key_pressed(KeyCode::U) {
            scroll -= 1;
        }
        if input.key_pressed(KeyCode::D) {
            scroll += 1;
        }
        scroll -= input.mouse_wheel().1.signum() as i32;
        ui.first = (ui.first as i32 + scroll.signum()).clamp(0, count.saturating_sub(ROWS) as i32)
            as usize;

        // The stacks.
        let mut left = None;
        let mut right = None;
        for row in 0..ROWS {
            let Some(stack) = stacks.get(ui.first + row) else {
                break;
            };
            for (i, &unit) in stack.iter().take(9).enumerate() {
                let (x, y) = (START.0 + STEP.0 * i as f32, START.1 + STEP.1 * row as f32);
                let cell = lay.rect((x + 1.0, y + 1.0, 18.0, 15.0));
                let state = ui::hit(cell, true, input);
                if state.hovered() {
                    ui.scanned = (ui.first + row, i);
                    if state.clicked() {
                        left = Some(unit);
                    }
                    if input.mouse_pressed(MouseButton::Right) {
                        right = Some(unit);
                    }
                }
                self.draw_army_unit(&lay, unit, x, y);
                if ui.scanned == (ui.first + row, i) {
                    let h = lay.rect((x + 1.0, y + 2.0, 18.0, 14.0));
                    draw_rectangle(h.x, h.y, h.w, h.h, Color::new(1.0, 1.0, 1.0, 0.25));
                }
            }
        }

        // The unit under the mouse: its name, its lasting spells and where it is.
        let scanned = stacks
            .get(ui.scanned.0)
            .and_then(|s| s.get(ui.scanned.1))
            .copied();
        if let Some(unit) = scanned {
            let name = self.army_unit_name(unit).to_uppercase();
            self.draw_styled(view, &Style::nearest(1, gold).center(), &name, 188.0, 161.0);
            let names: Vec<String> = self
                .game
                .units()
                .unit_enchantments(unit)
                .into_iter()
                .take(4)
                .map(|(s, _)| self.game.content().rules.spells.get(s).name.clone())
                .collect();
            let step = if names.len() < 4 { 7.0 } else { 6.0 };
            for (i, n) in names.iter().enumerate() {
                self.draw_styled(
                    view,
                    &Style::nearest(1, light),
                    n,
                    144.0,
                    172.0 + step * i as f32,
                );
            }
            if let Some(at) = self.game.units().unit_position(unit) {
                self.draw_reduced_map(&lay, at, MAP);
            }
        }

        // Buttons and leaving.
        let items = self.army_button(&lay, 3, 273.0, 163.0, true, input);
        let ok = self.army_button(&lay, 4, 273.0, 182.0, true, input);
        self.army_ui = ui;
        if items {
            self.open_item_screen();
        } else if ok || input.key_pressed(KeyCode::Escape) {
            self.screen = Screen::Map;
        } else if let Some(unit) = left {
            self.army_go_to(unit);
        } else if let Some(unit) = right {
            self.open_unit_sheet(unit);
        }
    }

    /// Select the unit's stack and show it on the map.
    fn army_go_to(&mut self, unit: hecs::Entity) {
        if let Some(at) = self.game.units().unit_position(unit) {
            self.select_at(at);
            self.center_on(at);
        }
        self.screen = Screen::Map;
    }

    /// "Race Name" for a race's unit, the unit's name otherwise.
    fn army_unit_name(&self, unit: hecs::Entity) -> String {
        let Some(def) = self.game.units().unit_def(unit) else {
            return String::new();
        };
        let race = def.race.as_deref().and_then(|r| {
            self.game
                .content()
                .rules
                .races
                .by_name(r)
                .map(|r| r.name.clone())
        });
        match race {
            Some(r) => format!("{r} {}", def.name),
            None => def.name.clone(),
        }
    }

    /// One unit's square: its picture on its banner's colour, a damage bar under it.
    pub(crate) fn draw_army_unit(&mut self, lay: &Layout, unit: hecs::Entity, x: f32, y: f32) {
        let owner = self.game.units().unit_owner(unit).map_or(0, |o| o.0);
        let c = self.banner_color(owner);
        let frame = lay.rect((x + 1.0, y + 1.0, 18.0, 16.0));
        draw_rectangle(
            frame.x,
            frame.y,
            frame.w,
            frame.h,
            Color::new(c.r * 0.5, c.g * 0.3, c.b * 0.3, 1.0),
        );
        let art = self.game.units().unit_def(unit).and_then(|d| d.art.clone());
        if let Some((lbx, entry)) = art {
            self.draw_pic(lay, &lbx, entry as usize, 0, x + 2.0, y + 2.0);
        }
        let (hp, max) = self.game.units().unit_hits(unit);
        if hp < max && max > 0 {
            let bar = lay.rect((x + 2.0, y + 15.0, 16.0 * hp as f32 / max as f32, 1.0));
            draw_rectangle(bar.x, bar.y, bar.w, bar.h, RED);
        }
    }

    /// The small map: the explored squares round the unit, the unit's square blinking.
    pub(crate) fn draw_reduced_map(
        &mut self,
        lay: &Layout,
        at: Position,
        area: (f32, f32, f32, f32),
    ) {
        let r = lay.rect(area);
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.05, 1.0));
        let Some(map) = self.game.map() else {
            return;
        };
        let grid = map.plane(at.plane);
        let (gw, gh) = (grid.width() as i32, grid.height() as i32);
        let (w, h) = (area.2 as i32, area.3 as i32);
        // The window is centred on the unit, kept inside the map up and down and wrapping
        // sideways.
        let top = (at.y as i32 - h / 2).clamp(0, (gh - h).max(0));
        let left = at.x as i32 - w / 2;
        let content = self.game.content();
        let px = lay.scale;
        for dy in 0..h.min(gh) {
            for dx in 0..w {
                let (x, y) = ((left + dx).rem_euclid(gw) as u32, (top + dy) as u32);
                let pos = Position {
                    plane: at.plane,
                    x,
                    y,
                };
                if !self.game.is_explored(HUMAN, pos) {
                    continue;
                }
                let class = grid
                    .get(x as i32, y as i32)
                    .and_then(|t| content.terrains.get(t))
                    .map(|d| d.class);
                use f4x_core::map::terrain::TerrainClass as T;
                let c = match class {
                    Some(T::Ocean) => Color::new(0.1, 0.2, 0.6, 1.0),
                    Some(T::Mountains) => Color::new(0.6, 0.6, 0.6, 1.0),
                    Some(T::Desert) => Color::new(0.8, 0.7, 0.4, 1.0),
                    Some(T::Tundra) => Color::new(0.85, 0.9, 0.95, 1.0),
                    Some(_) => Color::new(0.2, 0.5, 0.2, 1.0),
                    None => continue,
                };
                draw_rectangle(r.x + dx as f32 * px, r.y + dy as f32 * px, px, px, c);
            }
        }
        if ((get_time() * 4.0) as u32).is_multiple_of(2) {
            let (mx, my) = (at.x as i32 - left, at.y as i32 - top);
            draw_rectangle(r.x + mx as f32 * px, r.y + my as f32 * px, px, px, WHITE);
        }
    }

    /// One of the list's two-frame buttons (`armylist` 1 to 4); true when clicked. A disabled
    /// one shows its second frame.
    pub(crate) fn army_button(
        &mut self,
        lay: &Layout,
        entry: usize,
        x: f32,
        y: f32,
        enabled: bool,
        input: &Input,
    ) -> bool {
        let disabled = if entry <= 2 {
            Disabled::Frame(1)
        } else {
            Disabled::Fade(0.5)
        };
        self.pic_button(lay, ("armylist", entry), (x, y), (enabled, disabled), input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::map::Plane;

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    #[test]
    fn units_on_one_square_make_one_stack_in_order_of_first_appearance() {
        let units = [(1, at(2, 2)), (2, at(5, 5)), (3, at(2, 2)), (4, at(5, 5))];
        assert_eq!(group_stacks(&units), vec![vec![1, 3], vec![2, 4]]);
        let on_myrror = Position {
            plane: Plane::Myrror,
            ..at(2, 2)
        };
        assert_eq!(group_stacks(&[(1, at(2, 2)), (2, on_myrror)]).len(), 2);
    }
}
