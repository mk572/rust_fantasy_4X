//! The game's own mouse pointer on every screen, drawn by the frame loop as the original's
//! `Set_Mouse_List` does: a finger by default, an hourglass while the computer plays, the winged
//! boot over a square the selected stack can walk to and the red cross over one it cannot, and the
//! wand while a spell waits for its target. The battle screen picks its own (`battle_screen/pointer.rs`)
//! and draws through `draw_image`. Without the art the system pointer stays.

use std::sync::atomic::{AtomicBool, Ordering};

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
    pub(crate) fn draw_pointer_image(&mut self, image: usize) {
        let Some(tex) = self.art.cursor(image).cloned() else {
            set_system_pointer(true);
            return;
        };
        set_system_pointer(false);
        let at: Vec2 = mouse_position().into();
        draw_texture_ex(
            &tex,
            at.x,
            at.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(tex.size() * Layout::new().scale),
                ..Default::default()
            },
        );
    }

    /// Draw the pointer for the frame just built. The battle screen draws its own.
    pub(crate) fn draw_pointer(&mut self) {
        if matches!(self.screen, Screen::Battle) && self.game.in_battle() {
            return;
        }
        let image = self.overland_pointer();
        self.draw_pointer_image(image);
    }

    fn overland_pointer(&mut self) -> usize {
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
        let Some(pos) = self.tile_at(mouse_position().into()) else {
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
