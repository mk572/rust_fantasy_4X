//! The lasting spells cast on a unit in a battle: the list of them is all that is added to what
//! the unit started the battle with (`unit/stats.rs` works the numbers out from it).
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::half_int::HalfInt;
use f4x_def::id::PlayerId;
use f4x_def::rules::stats::Modifier;
use f4x_def::rules::{Ability, SpellId};

use super::BattleUnit;

/// A lasting spell cast on a unit in the battle, with what it does to the unit's numbers: the
/// unit's list of these, in the order cast, is all that is added to its sheet
/// (`BattleUnit::lasting`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Lasting {
    pub spell: SpellId,
    pub caster: PlayerId,
    pub mods: Vec<Modifier>,
    pub grants: Vec<Ability>,
}

impl BattleUnit {
    /// A web closes round the unit: whole again, and the unit loses what it had left of its turn.
    pub fn web_closes(&mut self, web_hits: i32) {
        self.web_hits = web_hits;
        self.end_moves();
        self.attacked = true;
    }

    /// The lasting spells of the battle on the unit, and who cast each, in the order cast.
    pub fn combat_spells(&self) -> Vec<(SpellId, PlayerId)> {
        self.lasting.iter().map(|l| (l.spell, l.caster)).collect()
    }

    /// The unit's moves of the turn are gone, and so is the rest of what it could do.
    pub fn moves_gone(&self) -> bool {
        self.moves_left == HalfInt::ZERO
    }
}
