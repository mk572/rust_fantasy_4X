//! The scripted battle player (`docs/ai-design.md` section 6): [`order`] moves and attacks with the
//! units, [`cast`] chooses the wizard's spell once a turn and falls back on the orders. The game
//! asks for the next command through [`install`], which names [`cast::next_command`] as the
//! scripted player of every side without a brain of its own (`World::set_battle_brain`).
//!
//! Tests: in the modules.

pub mod cast;
mod lever;
pub mod order;

use f4x_core::battle::world::set_battle_ai;

/// Make this the scripted battle player of the game (behaviour the game only asks for;
/// calling again does nothing). `Ai::new`, `NeutralAi` and the apps call it, so any battle the
/// AI or an auto-resolve plays has its commands.
pub fn install() {
    set_battle_ai(cast::next_command);
}
