//! The screen that asks who fights when more than nine units could (`PLANNED.md` item 34): a
//! scrollable list of the units on the square, strongest first, one row each with the picture,
//! name, attack, defence and abilities, and a check box for the nine at most that go in. The
//! human sees it as the attacker (the units not chosen stay where they are with their moves) and
//! as the defender (the units not chosen sit the fight out).
//!
//! Tests: below. The list's first row is `scrolled`, a tick is `toggled`, the end of the pick is
//! `outcome`, and `new_fighter_pick` makes the list; the drawing is left to the screenshots.

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
        pick.first = scrolled(pick.first, pick.units.len(), input);

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
            toggle(&mut pick.picked, i, max);
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
        };
        outcome(pick, ok, back, input)
    }
}

/// The first row shown after a frame: the wheel and the arrow keys move it by one, the page
/// keys by a page, and it never passes the last page.
fn scrolled(first: usize, units: usize, input: &Input) -> usize {
    let last_first = units.saturating_sub(VISIBLE);
    let mut first = first;
    let wheel = input.mouse_wheel().1;
    if wheel > 0.0 || input.key_pressed(KeyCode::Up) {
        first = first.saturating_sub(1);
    }
    if wheel < 0.0 || input.key_pressed(KeyCode::Down) {
        first = (first + 1).min(last_first);
    }
    if input.key_pressed(KeyCode::PageUp) {
        first = first.saturating_sub(VISIBLE);
    }
    if input.key_pressed(KeyCode::PageDown) {
        first = (first + VISIBLE).min(last_first);
    }
    first.min(last_first)
}

/// A click on a row: a ticked unit is unticked, an unticked one is ticked while fewer than `max`
/// are.
fn toggle(picked: &mut [bool], i: usize, max: usize) {
    let count = picked.iter().filter(|&&p| p).count();
    if picked[i] {
        picked[i] = false;
    } else if count < max {
        picked[i] = true;
    }
}

/// The end of a frame: the attacker backs out on the button or Esc; Ok or Enter takes the ticked
/// units, if there are any; otherwise the pick goes on.
fn outcome(pick: FighterPick, ok: bool, back: bool, input: &Input) -> PickStep {
    let count = pick.picked.iter().filter(|&&p| p).count();
    if back || (pick.attacking && input.key_pressed(KeyCode::Escape)) {
        PickStep::Back
    } else if (ok || input.key_pressed(KeyCode::Enter)) && count > 0 {
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

const GOLD_CHECK: Color = Color::new(0.95, 0.8, 0.3, 1.0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::HUMAN;
    use f4x_core::testdir::TestResult;

    /// A pick of `n` units, the first `ticked` of them ticked.
    fn pick_of(n: usize, ticked: usize, attacking: bool) -> FighterPick {
        let mut world = hecs::World::new();
        FighterPick {
            units: (0..n).map(|_| world.spawn(())).collect(),
            picked: (0..n).map(|i| i < ticked).collect(),
            first: 0,
            attacking,
            auto: false,
        }
    }

    fn wheel(dy: f32) -> Input {
        Input {
            wheel: (0.0, dy),
            ..Input::default()
        }
    }

    #[test]
    fn the_list_scrolls_by_a_row_or_a_page_and_stops_at_the_ends() {
        let units = VISIBLE + 5;
        assert_eq!(scrolled(0, units, &Input::key(KeyCode::Down)), 1);
        assert_eq!(scrolled(0, units, &Input::key(KeyCode::Up)), 0);
        assert_eq!(scrolled(3, units, &Input::key(KeyCode::Up)), 2);
        assert_eq!(scrolled(0, units, &Input::key(KeyCode::PageDown)), 5);
        assert_eq!(scrolled(5, units, &Input::key(KeyCode::Down)), 5);
        assert_eq!(scrolled(5, units, &Input::key(KeyCode::PageUp)), 0);
        assert_eq!(scrolled(0, units, &wheel(-1.0)), 1, "the wheel down");
        assert_eq!(scrolled(2, units, &wheel(1.0)), 1, "the wheel up");
    }

    #[test]
    fn a_list_that_fits_does_not_scroll() {
        for key in [KeyCode::Down, KeyCode::PageDown, KeyCode::Up] {
            assert_eq!(scrolled(0, VISIBLE, &Input::key(key)), 0, "{key:?}");
        }
        assert_eq!(scrolled(4, 3, &Input::default()), 0, "a shrunk list");
    }

    #[test]
    fn a_click_unticks_a_ticked_unit_and_ticks_another_only_below_the_most() {
        let mut picked = vec![true, false, false];
        toggle(&mut picked, 0, 2);
        assert_eq!(picked, [false, false, false]);
        toggle(&mut picked, 1, 2);
        toggle(&mut picked, 2, 2);
        assert_eq!(picked, [false, true, true]);
        let mut full = vec![true, true, false];
        toggle(&mut full, 2, 2);
        assert_eq!(full, [true, true, false], "no more than the most");
    }

    #[test]
    fn ok_or_enter_takes_the_ticked_units_in_list_order() -> TestResult {
        let mut pick = pick_of(4, 0, true);
        pick.picked = vec![false, true, false, true];
        let want = vec![pick.units[1], pick.units[3]];
        for (ok, key) in [(true, None), (false, Some(KeyCode::Enter))] {
            let input = key.map_or_else(Input::default, Input::key);
            let PickStep::Done(chosen) = outcome(pick.clone(), ok, false, &input) else {
                return Err("the pick is not done".into());
            };
            assert_eq!(chosen, want);
        }
        Ok(())
    }

    #[test]
    fn nothing_ticked_cannot_be_taken() {
        let pick = pick_of(3, 0, true);
        assert!(matches!(
            outcome(pick.clone(), true, false, &Input::default()),
            PickStep::Picking(_)
        ));
        assert!(matches!(
            outcome(pick, false, false, &Input::key(KeyCode::Enter)),
            PickStep::Picking(_)
        ));
    }

    #[test]
    fn only_the_attacker_can_call_the_fight_off() {
        let esc = Input::key(KeyCode::Escape);
        assert!(matches!(
            outcome(pick_of(3, 2, true), false, false, &esc),
            PickStep::Back
        ));
        assert!(matches!(
            outcome(pick_of(3, 2, true), false, true, &Input::default()),
            PickStep::Back
        ));
        assert!(matches!(
            outcome(pick_of(3, 2, false), false, false, &esc),
            PickStep::Picking(_)
        ));
    }

    #[test]
    fn the_list_is_strongest_first_and_ticks_the_nine_the_preset_takes() -> TestResult {
        let mut app = crate::view::App::for_test();
        let pos = app
            .game
            .units()
            .player_units(HUMAN)
            .first()
            .and_then(|&u| app.game.units().unit_position(u))
            .ok_or("the human has no unit")?;
        let rules = &app.game.content().rules;
        let kinds = [
            "barbarian_spearmen",
            "barbarian_bowmen",
            "barbarian_cavalry",
        ]
        .map(|name| {
            rules
                .units
                .id_of(name)
                .ok_or("a unit is not in the content")
        });
        for i in 0..12 {
            app.game.spawn_unit(HUMAN, pos, kinds[i % 3]?);
        }
        let units = app.own_units_at(pos);
        assert!(units.len() > f4x_core::unit::MAX_STACK);
        let pick = app.new_fighter_pick(&units, true, false);
        assert_eq!(pick.units.len(), units.len());
        let strength: Vec<f32> = pick
            .units
            .iter()
            .map(|&u| app.game.fighter_strength(u))
            .collect();
        assert!(strength.windows(2).all(|w| w[0] >= w[1]), "{strength:?}");
        assert_eq!(
            pick.picked.iter().filter(|&&p| p).count(),
            f4x_core::unit::MAX_STACK
        );
        Ok(())
    }
}
