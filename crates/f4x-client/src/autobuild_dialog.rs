//! The dialog that sets a city's auto-builder plan (docs/auto-builder.md), opened from the
//! city screen. Left-click a row for the next value, right-click for the previous one.

use crate::input::Input;
use crate::text::{Args, Texts};
use crate::ui;
use f4x_core::city::autobuild::{BuildPlan, Focus, Mix};
use f4x_core::game::Command;
use macroquad::prelude::*;

use crate::city_screen::{CityPage, Layout};
use crate::font::Ink;
use crate::font_ramp;
use crate::frame::{Align, Layout as FrameLayout};
use crate::view::{App, Screen};

/// The plan being edited, and whether the city had one when the dialog opened.
pub(crate) struct AutoDialog {
    pub plan: BuildPlan,
    pub had_plan: bool,
}

const MIXES: &[Mix] = &[Mix::Both, Mix::Buildings, Mix::Units];
/// Row height of the dialog's list, in the original's pixels.
const ROW: f32 = 10.0;
const POPULATIONS: std::ops::RangeInclusive<u32> = 2..=12;

/// The next or previous of `all` after `current`, wrapping.
fn step<T: Copy + PartialEq>(all: &[T], current: T, back: bool) -> T {
    let at = all.iter().position(|&x| x == current).unwrap_or(0);
    let len = all.len();
    all[(at + if back { len - 1 } else { 1 }) % len]
}

/// Balanced first, then the seven focuses.
fn primaries() -> Vec<Focus> {
    std::iter::once(Focus::Balanced)
        .chain(Focus::choosable())
        .collect()
}

/// What the button on the city screen says.
pub(crate) fn label(texts: &Texts, plan: Option<&BuildPlan>) -> String {
    let focus = |f: Focus| texts.get(f.key(), &Args::new());
    match plan {
        None => texts.get("autobuild.off", &Args::new()),
        Some(p) => match p.secondary {
            Some(s) if p.primary != Focus::Balanced => texts.get(
                "autobuild.label_two",
                &Args::new()
                    .text("primary", focus(p.primary))
                    .text("secondary", focus(s)),
            ),
            _ => texts.get(
                "autobuild.label",
                &Args::new().text("primary", focus(p.primary)),
            ),
        },
    }
}

impl App {
    /// The first city's screen with the dialog open on a plan (the `--autobuild` screenshot).
    pub(crate) fn open_test_autobuild(&mut self) {
        let Some(city) = self.game.economy().cities().first().map(|c| c.0) else {
            return;
        };
        let mut page = CityPage::new(city);
        page.autobuild = Some(AutoDialog {
            plan: BuildPlan {
                primary: Focus::Defense,
                secondary: Some(Focus::Gold),
                settlers: true,
                ..Default::default()
            },
            had_plan: true,
        });
        self.open_page(Screen::City(city), page);
    }

    /// The "Auto" button in the window where the production is chosen (`city_screen.rs`), at
    /// `(x, y)` and `width` wide; opens the dialog when clicked.
    pub(crate) fn draw_autobuild_button(
        &mut self,
        page: &mut CityPage,
        _lay: Layout,
        (x, y, width): (f32, f32, f32),
        input: &Input,
    ) {
        let city = page.city;
        let plan = self.game.autobuild_of(city).map(|a| a.plan);
        let own = plan.is_some();
        let text = label(self.texts(), plan.as_ref());
        let frame = FrameLayout::new();
        if self.red_button_sized(&frame, &text, x, y, width, input) {
            self.choosing = false;
            page.autobuild = Some(AutoDialog {
                plan: plan.unwrap_or_default(),
                had_plan: own,
            });
        }
    }

    /// Draw the dialog and handle it. Returns true while it is open.
    ///
    /// The box is the stone of the original's confirmation boxes (`resource.lbx` 0), the text its
    /// readable font 1 in the colours of those boxes, the buttons the red ones of the production
    /// screen.
    pub(crate) fn draw_autobuild_dialog(&mut self, page: &mut CityPage, input: &Input) -> bool {
        let city = page.city;
        let Some(mut d) = page.autobuild.take() else {
            return false;
        };
        let lay = FrameLayout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let stone_w = self
            .art_picture("resource", 0, 0)
            .map_or(186.0, |t| crate::resolution::logical_w(&t));
        let (bw, bh) = (stone_w, 138.0);
        let (bx, by) = (
            ((crate::resolution::DESIGN_W - bw) / 2.0).floor(),
            ((crate::resolution::DESIGN_H - bh) / 2.0).floor(),
        );
        self.draw_stone_box(&lay, bx, by, bh);
        let gold = Ink::Set(font_ramp::NOTIFY_TEXT);
        let light = Ink::Set(font_ramp::DIALOG_TEXT);
        let grey = Ink::Flat(Color::new(0.55, 0.5, 0.45, 1.0));
        let say = |app: &mut App, ink: Ink, font: usize, text: &str, x: f32, y: f32, align| {
            app.stone_text(view, font, ink, text, x, y, align);
        };
        say(
            self,
            gold,
            4,
            &self.text("autobuild.title"),
            bx + bw / 2.0,
            by + 6.0,
            Align::Center,
        );
        say(
            self,
            light,
            1,
            &self.text("autobuild.help"),
            bx + bw / 2.0,
            by + 17.0,
            Align::Center,
        );
        let left = input.mouse_pressed(MouseButton::Left);
        let right = input.mouse_pressed(MouseButton::Right);
        let balanced = d.plan.primary == Focus::Balanced;
        let yes_no = |on: bool| self.text(if on { "autobuild.yes" } else { "autobuild.no" });
        let secondary = match d.plan.secondary {
            Some(s) if !balanced => self.text(s.key()),
            _ => self.text("autobuild.none"),
        };
        let rows: [(String, String, bool); 6] = [
            (
                self.text("autobuild.builds"),
                self.text(d.plan.mix.key()),
                true,
            ),
            (
                self.text("autobuild.primary"),
                self.text(d.plan.primary.key()),
                true,
            ),
            (self.text("autobuild.secondary"), secondary, !balanced),
            (
                self.text("autobuild.settlers"),
                yes_no(d.plan.settlers),
                true,
            ),
            (
                self.text("autobuild.settlers_pop"),
                d.plan.min_settler_pop.to_string(),
                d.plan.settlers,
            ),
            (
                self.text("autobuild.switch_off"),
                yes_no(d.plan.event_prompt),
                true,
            ),
        ];
        let mut action: Option<usize> = None;
        for (i, (name, value, enabled)) in rows.iter().enumerate() {
            let y = by + 29.0 + i as f32 * ROW;
            let r = lay.rect((bx + 6.0, y - 1.0, bw - 12.0, ROW));
            let hover = ui::hit(r, true, input).hovered();
            if hover && *enabled {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.5, 0.38, 0.26, 1.0));
            }
            say(
                self,
                if *enabled { light } else { grey },
                1,
                name,
                bx + 9.0,
                y,
                Align::Left,
            );
            say(
                self,
                if *enabled { gold } else { grey },
                1,
                value,
                bx + bw - 9.0,
                y,
                Align::Right,
            );
            if hover && *enabled && (left || right) {
                action = Some(i);
            }
        }
        if let Some(i) = action {
            let back = right;
            match i {
                0 => d.plan.mix = step(MIXES, d.plan.mix, back),
                1 => d.plan.primary = step(&primaries(), d.plan.primary, back),
                2 => {
                    let mut all: Vec<Option<Focus>> = vec![None];
                    all.extend(
                        Focus::choosable()
                            .filter(|&f| f != d.plan.primary)
                            .map(Some),
                    );
                    d.plan.secondary = step(&all, d.plan.secondary, back);
                }
                3 => d.plan.settlers = !d.plan.settlers,
                4 => {
                    let p = d.plan.min_settler_pop as i32 + if back { -1 } else { 1 };
                    d.plan.min_settler_pop =
                        (p.max(*POPULATIONS.start() as i32) as u32).min(*POPULATIONS.end());
                }
                _ => d.plan.event_prompt = !d.plan.event_prompt,
            }
        }
        let hint_at = by + 29.0 + 6.0 * ROW + 4.0;
        let line = self.art.text_height(1) + 1.0;
        let hint = self.bm_wrap(1, &hint(self.texts(), &d.plan), bw - 20.0);
        for (k, l) in hint.iter().take(3).enumerate() {
            say(
                self,
                light,
                1,
                l,
                bx + 10.0,
                hint_at + line * k as f32,
                Align::Left,
            );
        }
        let labels = [
            (self.text("ui.ok"), true),
            (self.text("autobuild.button_off"), d.had_plan),
            (self.text("autobuild.button_cancel"), true),
        ];
        let mut chosen: Option<usize> = None;
        for (k, (name, enabled)) in labels.iter().enumerate() {
            // OK on the right, Cancel on the left.
            let x = bx + 10.0 + (2 - k) as f32 * 57.0;
            if self.red_button_face_sized(&lay, name, (x, by + bh - 24.0), *enabled, 52.0, input) {
                chosen = Some(k);
            }
        }
        if input.key_pressed(KeyCode::Escape) {
            chosen = Some(2);
        }
        if input.key_pressed(KeyCode::Enter) {
            chosen = Some(0);
        }
        match chosen {
            Some(0) => {
                let events = self.game.apply(Command::SetAutoBuild {
                    city,
                    plan: Some(d.plan),
                });
                self.note(events);
            }
            Some(1) => {
                let events = self.game.apply(Command::SetAutoBuild { city, plan: None });
                self.note(events);
            }
            Some(_) => {}
            None => {
                page.autobuild = Some(d);
            }
        }
        true
    }
}

/// One line on what the plan will do.
fn hint(texts: &Texts, plan: &BuildPlan) -> String {
    texts.get(
        match plan.mix {
            Mix::Both => "autobuild.hint_both",
            Mix::Buildings => "autobuild.hint_buildings",
            Mix::Units => "autobuild.hint_units",
        },
        &Args::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_app::shipped;

    #[test]
    fn stepping_wraps_in_both_directions() {
        let all = [1, 2, 3];
        assert_eq!(step(&all, 1, false), 2);
        assert_eq!(step(&all, 3, false), 1);
        assert_eq!(step(&all, 1, true), 3);
        assert_eq!(step(&all, 2, true), 1);
        assert_eq!(
            step(&all, 9, false),
            2,
            "an unknown value starts from the first"
        );
    }

    #[test]
    fn the_first_primary_is_balanced_and_the_other_seven_follow() {
        let all = primaries();
        assert_eq!(all[0], Focus::Balanced);
        assert_eq!(all[1..], Focus::choosable().collect::<Vec<_>>());
    }

    #[test]
    fn the_button_names_the_plan_and_a_second_focus_only_beside_a_real_first() {
        let texts = shipped::texts();
        assert_eq!(label(texts, None), "Auto: off");
        let plan = |primary, secondary| BuildPlan {
            primary,
            secondary,
            ..BuildPlan::default()
        };
        let magic = label(texts, Some(&plan(Focus::Magic, None)));
        assert_eq!(
            magic,
            format!("Auto: {}", texts.get(Focus::Magic.key(), &Args::new()))
        );
        let two = label(texts, Some(&plan(Focus::Magic, Some(Focus::Gold))));
        assert!(two.starts_with(&magic) && two.contains(" / "), "{two}");
        assert_eq!(
            label(texts, Some(&plan(Focus::Balanced, Some(Focus::Gold)))),
            label(texts, Some(&plan(Focus::Balanced, None))),
            "a balanced plan has no second focus"
        );
    }
}
