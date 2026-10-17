//! The battle screen's mouse pointer, drawn by the game as the original does
//! (`Assign_Mouse_Images`, `Combat_Spell_Target_Screen_Draw`): a finger by default, a winged boot
//! on a square the unit can walk to, crossed swords on an enemy it can strike, a bow on one it can
//! shoot, a wand on a square a spell can be aimed at, and a red cross where nothing can be done.

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::frame::Layout;

/// The pictures of `cursors/NN.png` (the original's `crsr_*`).
const FINGER: usize = 1;
const WAND: usize = 2;
const RED_CROSS: usize = 3;
const BOW: usize = 4;
const SWORDS: usize = 5;
const BOOT: usize = 7;

static HIDDEN: AtomicBool = AtomicBool::new(false);

/// Show or hide the system pointer, only calling the window when it changes.
fn set_system_pointer(shown: bool) {
    if HIDDEN.swap(!shown, Ordering::Relaxed) == shown {
        show_mouse(shown);
    }
}

/// Give the system pointer back when another screen is up.
pub(crate) fn show_system_pointer() {
    set_system_pointer(true);
}

impl App {
    /// What the pointer should look like over the battle.
    fn pointer_image(&self, battle: &Battle, my_turn: bool, hovered: Option<Pos>) -> usize {
        if !self.on_map(mouse_position().into()) {
            return FINGER;
        }
        if let Some(aim) = &self.battle_ui.aiming {
            let aimable = hovered.is_some_and(|cell| self.spell_aim_ok(battle, aim, cell));
            return if aimable { WAND } else { RED_CROSS };
        }
        if !my_turn || self.battle_ui.spell_list {
            return FINGER;
        }
        let Some(cell) = hovered else {
            return RED_CROSS;
        };
        let sel = self
            .battle_ui
            .selected
            .and_then(|i| battle.unit(i))
            .filter(|u| unfinished(u));
        let strike = |u: &BattleUnit| {
            if u.attacked {
                RED_CROSS
            } else if distance(u.cell, cell) == 1 {
                SWORDS
            } else if can_shoot(u) {
                BOW
            } else {
                RED_CROSS
            }
        };
        match (Self::seen_unit_at(battle, cell), sel) {
            (Some(target), _) if target.side == battle.active => FINGER,
            (Some(_), Some(u)) => strike(u),
            (Some(_), None) => RED_CROSS,
            (None, Some(u))
                if battle.map.has_wall(cell)
                    && u.has("wall_crusher")
                    && u.side == Side::Attacker =>
            {
                strike(u)
            }
            (None, Some(u)) if battle.reachable(u.id).contains_key(&cell) => BOOT,
            _ => RED_CROSS,
        }
    }

    /// Draw the pointer on top of everything, hiding the system's.
    pub(super) fn draw_battle_pointer(
        &mut self,
        battle: &Battle,
        my_turn: bool,
        hovered: Option<Pos>,
    ) {
        let image = self.pointer_image(battle, my_turn, hovered);
        let Some(tex) = self.art.cursor(image).cloned() else {
            set_system_pointer(true);
            return;
        };
        set_system_pointer(false);
        let lay = Layout::new();
        let at: Vec2 = mouse_position().into();
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
