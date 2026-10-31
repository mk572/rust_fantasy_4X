//! The screen that asks who fights when more than nine units could (`PLANNED.md` item 34): a
//! scrollable list of the units on the square, strongest first, one row each with the picture,
//! name, attack, defence and abilities, and a check box for the nine at most that go in. The
//! human sees it as the attacker (the units not chosen stay where they are with their moves) and
//! as the defender (the units not chosen sit the fight out).
//!
//! Tests: none yet, because its decisions are made in the frame function that also draws, which
//! needs a window; they are to move into functions of `&Input` (see "UI test" in
//! `docs/rust_coding_rules.md`).

use crate::font::Style;
use crate::frame::Layout;
use crate::input::Input;
use crate::ui;
use crate::unit_sheet::special_picture;
use crate::view::App;
use f4x_core::text::Args;
use macroquad::prelude::*;

/// The panel on the original's 320 by 200 screen.
const LEFT: f32 = 12.0;
const WIDTH: f32 = 296.0;
const TOP: f32 = 8.0;
const HEAD: f32 = 26.0;
const ROW: f32 = 19.0;
const VISIBLE: usize = 7;
const BOTTOM: f32 = TOP + HEAD + ROW * VISIBLE as f32 + 28.0;
/// The room for abilities at the right end of a row.
const ICONS: usize = 6;

/// The state of the pick: the units strongest first, who is ticked, and the first row shown.
#[derive(Debug, Clone)]
pub(crate) struct FighterPick {
    pub units: Vec<hecs::Entity>,
    pub picked: Vec<bool>,
    pub first: usize,
    /// The human attacks (and may call it off); else the human defends.
    pub attacking: bool,
    /// The fight goes on as Auto when the pick is done.
    pub auto: bool,
}

/// What a frame of the pick decided.
pub(crate) enum PickStep {
    Picking(FighterPick),
    /// The ticked units, in the order of the list.
    Done(Vec<hecs::Entity>),
    /// Called off (the attacker only).
    Back,
}

/// The pick before an assault on a site's guards.
pub(crate) struct SitePick {
    pub pick: FighterPick,
    pub to: f4x_core::map::Position,
}

impl crate::modal::Modal for SitePick {
    fn kind(&self) -> &'static str {
        "site_pick"
    }
    fn frame(
        self: Box<Self>,
        app: &mut App,
        input: &Input,
    ) -> Option<Box<dyn crate::modal::Modal>> {
        let SitePick { pick, to } = *self;
        match app.fighter_pick_frame(pick, input) {
            PickStep::Picking(pick) => Some(Box::new(SitePick { pick, to })),
            PickStep::Back => None,
            PickStep::Done(units) => {
                app.begin_battle(f4x_core::game::Command::EnterSite { units, to });
                None
            }
        }
    }
}

impl App {
    /// The list for `units`, strongest first, with the preset's nine ticked.
    pub(crate) fn new_fighter_pick(
        &self,
        units: &[hecs::Entity],
        attacking: bool,
        auto: bool,
    ) -> FighterPick {
        let mut sorted = units.to_vec();
        sorted.sort_by(|&a, &b| {
            self.game
                .fighter_strength(b)
                .total_cmp(&self.game.fighter_strength(a))
        });
        let preset = self.game.fighter_preset(units);
        let picked = sorted
            .iter()
            .map(|u| preset[..f4x_core::unit::MAX_STACK.min(preset.len())].contains(u))
            .collect();
        FighterPick {
            units: sorted,
            picked,
            first: 0,
            attacking,
            auto,
        }
    }

    /// One frame of the screen.
    pub(crate) fn fighter_pick_frame(&mut self, mut pick: FighterPick, input: &Input) -> PickStep {
        let max = f4x_core::unit::MAX_STACK;
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let count = pick.picked.iter().filter(|&&p| p).count();
        let last_first = pick.units.len().saturating_sub(VISIBLE);

        // The panel.
        let panel = lay.rect((LEFT, TOP, WIDTH, BOTTOM - TOP));
        draw_rectangle(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            Color::new(0.1, 0.08, 0.07, 0.97),
        );
        draw_rectangle_lines(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            2.0 * lay.scale,
            Color::new(0.75, 0.6, 0.3, 1.0),
        );
        let title = self.text(if pick.attacking {
            "attack.pick_attackers"
        } else {
            "attack.pick_defenders"
        });
        self.draw_styled(
            view,
            &Style::nearest(4, light).center(),
            &title,
            LEFT + WIDTH / 2.0,
            TOP + 5.0,
        );
        let line = self.text_with(
            "attack.pick_count",
            &Args::new().count("n", count).count("max", max),
        );
        self.draw_styled(
            view,
            &Style::nearest(1, light).right(),
            &line,
            LEFT + WIDTH - 8.0,
            TOP + 14.0,
        );

        // Scrolling: the wheel, the arrow keys and the page keys.
        let wheel = input.mouse_wheel().1;
        if wheel > 0.0 || input.key_pressed(KeyCode::Up) {
            pick.first = pick.first.saturating_sub(1);
        }
        if wheel < 0.0 || input.key_pressed(KeyCode::Down) {
            pick.first = (pick.first + 1).min(last_first);
        }
        if input.key_pressed(KeyCode::PageUp) {
            pick.first = pick.first.saturating_sub(VISIBLE);
        }
        if input.key_pressed(KeyCode::PageDown) {
            pick.first = (pick.first + VISIBLE).min(last_first);
        }
        pick.first = pick.first.min(last_first);

        // The rows.
        let body = TOP + HEAD;
        let mut toggled = None;
        for (i, &unit) in pick.units.iter().enumerate().skip(pick.first).take(VISIBLE) {
            let y = body + ROW * (i - pick.first) as f32;
            let row = lay.rect((LEFT + 4.0, y, WIDTH - 14.0, ROW - 1.0));
            let state = ui::hit(row, true, input);
            if pick.picked[i] {
                draw_rectangle(row.x, row.y, row.w, row.h, Color::new(0.3, 0.22, 0.1, 0.8));
            } else if state.hovered() {
                draw_rectangle(row.x, row.y, row.w, row.h, Color::new(1.0, 1.0, 1.0, 0.1));
            }
            if state.clicked() {
                toggled = Some(i);
            }
            // The check box.
            let boxr = lay.rect((LEFT + 8.0, y + 5.0, 9.0, 9.0));
            draw_rectangle_lines(boxr.x, boxr.y, boxr.w, boxr.h, lay.scale, light);
            if pick.picked[i] {
                draw_rectangle(
                    boxr.x + 2.0 * lay.scale,
                    boxr.y + 2.0 * lay.scale,
                    boxr.w - 4.0 * lay.scale,
                    boxr.h - 4.0 * lay.scale,
                    GOLD_CHECK,
                );
            }
            self.draw_army_unit(&lay, unit, LEFT + 22.0, y);
            let Some(sheet) = self.overland_sheet(unit) else {
                continue;
            };
            self.draw_styled(
                view,
                &Style::nearest(2, light),
                &sheet.name,
                LEFT + 44.0,
                y + 5.0,
            );
            // The attack: the better of melee and range, with its icon.
            let ranged = sheet.ranged.unwrap_or_default();
            let (attack, attack_icon) = if ranged.value > sheet.melee.value {
                (ranged.value, ranged.icon)
            } else {
                (sheet.melee.value, sheet.melee.icon)
            };
            for (k, (value, icon)) in [
                (attack, attack_icon),
                (sheet.defense.value, sheet.defense.icon),
            ]
            .into_iter()
            .enumerate()
            {
                let x = LEFT + 128.0 + 24.0 * k as f32;
                self.draw_styled(
                    view,
                    &Style::nearest(1, light).right(),
                    &value.to_string(),
                    x,
                    y + 7.0,
                );
                self.draw_pic(&lay, "unitview", 13 + icon, 0, x + 1.0, y + 5.0);
            }
            // The abilities, as the icons of the unit sheet.
            let icons = sheet.specials.iter().filter_map(|s| s.icon).take(ICONS);
            for (k, icon) in icons.enumerate() {
                let (lbx, entry) = special_picture(icon);
                self.draw_pic(&lay, lbx, entry, 0, LEFT + 178.0 + 18.0 * k as f32, y + 1.0);
            }
        }
        if let Some(i) = toggled {
            if pick.picked[i] {
                pick.picked[i] = false;
            } else if count < max {
                pick.picked[i] = true;
            }
        }

        // The scroll bar.
        if pick.units.len() > VISIBLE {
            let track = lay.rect((LEFT + WIDTH - 8.0, body, 4.0, ROW * VISIBLE as f32));
            draw_rectangle(
                track.x,
                track.y,
                track.w,
                track.h,
                Color::new(1.0, 1.0, 1.0, 0.15),
            );
            let share = VISIBLE as f32 / pick.units.len() as f32;
            let at = pick.first as f32 / pick.units.len() as f32;
            draw_rectangle(
                track.x,
                track.y + track.h * at,
                track.w,
                track.h * share,
                light,
            );
        }

        // The buttons.
        let count = pick.picked.iter().filter(|&&p| p).count();
        let by = BOTTOM - 20.0;
        let ok_label = self.text("attack.pick_ok");
        let ok = count > 0 && self.red_button(&lay, &ok_label, LEFT + WIDTH - 60.0, by, input);
        let back = pick.attacking && {
            let label = self.text("attack.cancel");
            self.red_button(&lay, &label, LEFT + 10.0, by, input)
                || input.key_pressed(KeyCode::Escape)
        };
        if back {
            PickStep::Back
        } else if ok || (count > 0 && input.key_pressed(KeyCode::Enter)) {
            let chosen = pick
                .units
                .iter()
                .zip(&pick.picked)
                .filter(|(_, &p)| p)
                .map(|(&u, _)| u)
                .collect();
            PickStep::Done(chosen)
        } else {
            PickStep::Picking(pick)
        }
    }
}

const GOLD_CHECK: Color = Color::new(0.95, 0.8, 0.3, 1.0);
