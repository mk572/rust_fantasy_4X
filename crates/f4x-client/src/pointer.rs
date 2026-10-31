//! The game's own mouse pointer on every screen, drawn by the frame loop as the original's
//! `Set_Mouse_List` does: a finger by default, an hourglass while the computer plays, the winged
//! boot over a square the selected stack can walk to and the red cross over one it cannot, and the
//! wand while a spell waits for its target. The battle screen picks its own (`battle_screen/pointer.rs`)
//! and draws through `draw_image`. Without the art the system pointer stays.
//!
//! Tests: below. Which pointer a frame wants is `overland_pointer`; showing it needs a window.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::input::Input;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::view::{App, Screen};

/// The pictures of `cursors/NN.png` (the original's `crsr_*`).
pub(crate) const FINGER: usize = 1;
pub(crate) const WAND: usize = 2;
pub(crate) const RED_CROSS: usize = 3;
pub(crate) const BOW: usize = 4;
pub(crate) const SWORDS: usize = 5;
pub(crate) const HOURGLASS: usize = 6;
pub(crate) const BOOT: usize = 7;

static HIDDEN: AtomicBool = AtomicBool::new(false);

/// Show or hide the system pointer, only calling the window when it changes.
fn set_system_pointer(shown: bool) {
    if HIDDEN.swap(!shown, Ordering::Relaxed) == shown {
        show_mouse(shown);
    }
}

impl App {
    /// Draw the original's pointer `image` at the mouse on top of everything, hiding the system's;
    /// the system pointer stays when the picture is missing.
    pub(crate) fn draw_pointer_image(&mut self, image: usize, input: &Input) {
        let Some(tex) = self.art.cursor(image).cloned() else {
            set_system_pointer(true);
            return;
        };
        set_system_pointer(false);
        let at: Vec2 = input.mouse_position().into();
        draw_texture_ex(
            &tex,
            at.x,
            at.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(crate::resolution::logical_size(&tex) * Layout::new().scale),
                ..Default::default()
            },
        );
    }

    /// Draw the pointer for the frame just built. The battle screen draws its own.
    pub(crate) fn draw_pointer(&mut self, input: &Input) {
        if matches!(self.screen, Screen::Battle) && self.game.in_battle() {
            return;
        }
        let image = self.overland_pointer(input);
        self.draw_pointer_image(image, input);
    }

    fn overland_pointer(&mut self, input: &Input) -> usize {
        if self.turn_pending > 0 {
            return HOURGLASS;
        }
        if !matches!(self.screen, Screen::Map) || self.game.in_battle() {
            return FINGER;
        }
        if self.targeting.is_some() {
            return WAND;
        }
        if self.selection.is_empty() || self.road_plan.is_some() || self.surveyor {
            return FINGER;
        }
        let Some(pos) = self.tile_at(input.mouse_position().into()) else {
            return FINGER;
        };
        let key = (pos, self.selection.clone());
        if self.pointer_walk.as_ref().map(|(k, _)| k) != Some(&key) {
            let walk = self.game.find_path(&self.selection, pos).is_some();
            self.pointer_walk = Some((key, walk));
        }
        match self.pointer_walk {
            Some((_, true)) => BOOT,
            _ => RED_CROSS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::HUMAN;
    use f4x_core::testdir::TestResult;

    fn over_the_map(app: &App) -> Input {
        let r = app.map_rect();
        Input {
            mouse: (r.x + r.w / 2.0, r.y + r.h / 2.0),
            ..Input::default()
        }
    }

    #[test]
    fn the_hourglass_shows_while_the_computer_plays() {
        let mut app = App::for_test();
        app.turn_pending = 1;
        let input = over_the_map(&app);
        assert_eq!(app.overland_pointer(&input), HOURGLASS);
    }

    #[test]
    fn off_the_map_screen_the_pointer_is_the_finger() {
        let mut app = App::for_test();
        app.screen = Screen::Magic;
        let input = over_the_map(&app);
        assert_eq!(app.overland_pointer(&input), FINGER);
    }

    #[test]
    fn the_wand_shows_while_a_spell_waits_for_its_target() -> TestResult {
        let mut app = App::for_test();
        let spell = app
            .game
            .content()
            .rules
            .spells
            .id_of("fireball")
            .ok_or("fireball is not in the content")?;
        app.targeting = Some(crate::spellbook::Targeting {
            spell,
            resolve: false,
        });
        let input = over_the_map(&app);
        assert_eq!(app.overland_pointer(&input), WAND);
        Ok(())
    }

    #[test]
    fn with_nothing_selected_the_pointer_is_the_finger() {
        let mut app = App::for_test();
        let input = over_the_map(&app);
        assert_eq!(app.overland_pointer(&input), FINGER);
    }

    #[test]
    fn a_selected_stack_gets_the_boot_where_it_can_walk_and_the_cross_where_not() -> TestResult {
        let mut app = App::for_test();
        let unit = *app
            .game
            .units()
            .player_units(HUMAN)
            .first()
            .ok_or("the human has no unit")?;
        app.selection = vec![unit];
        let rect = app.map_rect();
        let (mut boots, mut crosses) = (0, 0);
        for i in 0..40 {
            for j in 0..20 {
                let input = Input {
                    mouse: (
                        rect.x + rect.w * (i as f32 + 0.5) / 40.0,
                        rect.y + rect.h * (j as f32 + 0.5) / 20.0,
                    ),
                    ..Input::default()
                };
                let Some(pos) = app.tile_at(input.mouse_position().into()) else {
                    continue;
                };
                let walk = app.game.find_path(&app.selection, pos).is_some();
                let want = if walk { BOOT } else { RED_CROSS };
                assert_eq!(app.overland_pointer(&input), want, "{pos:?}");
                if walk {
                    boots += 1;
                } else {
                    crosses += 1;
                }
            }
        }
        assert!(boots > 0 && crosses > 0, "boots {boots}, crosses {crosses}");
        Ok(())
    }

    #[test]
    fn a_planned_road_or_the_surveyor_keeps_the_finger() -> TestResult {
        let mut app = App::for_test();
        let unit = *app
            .game
            .units()
            .player_units(HUMAN)
            .first()
            .ok_or("the human has no unit")?;
        app.selection = vec![unit];
        app.surveyor = true;
        let input = over_the_map(&app);
        assert_eq!(app.overland_pointer(&input), FINGER);
        Ok(())
    }
}
