//! The place in the round as an old save names it: by the number of a seat. The wizards had the
//! first seats, in the order of their ids; one more seat after them played every player without a
//! wizard, all of them together.
//!
//! Tests: in tests/it/convert.rs.

use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::world::save::PlayerSave;
use serde::Deserialize;

use crate::{turn_order, Phase};

#[derive(Deserialize)]
#[serde(rename = "State")]
struct OldState {
    phase: OldPhase,
}

#[derive(Deserialize)]
#[serde(rename = "Phase")]
enum OldPhase {
    RoundStart,
    TurnStart(usize),
    Acting(usize),
    TurnEnd(usize),
    RoundEnd,
    Over,
}

/// The place in the round that the old text `flow` names in a world of these `players`; `None`
/// when it cannot be read or names a seat there was not.
///
/// A save made while the shared seat was playing had started every turn of its players: it goes
/// on in the last turn of the round, the monsters', so that no turn is started twice.
pub(super) fn phase(flow: &str, players: &[PlayerSave]) -> Option<Phase> {
    let state: OldState = ron::from_str(flow).ok()?;
    let ids = |neutral: bool| {
        let mut ids: Vec<PlayerId> = players
            .iter()
            .filter(|p| p.neutral == neutral && p.id != MONSTERS.0)
            .map(|p| PlayerId(p.id))
            .collect();
        ids.sort_by_key(|&p| turn_order(p));
        ids
    };
    let wizards = ids(false);
    let shared = wizards.len();
    let wizard = |seat: usize| wizards.get(seat).copied();
    Some(match state.phase {
        OldPhase::RoundStart => Phase::RoundStart,
        OldPhase::TurnStart(seat) if seat == shared => {
            Phase::TurnStart(ids(true).first().copied().unwrap_or(MONSTERS))
        }
        OldPhase::TurnStart(seat) => Phase::TurnStart(wizard(seat)?),
        OldPhase::Acting(seat) if seat == shared => Phase::Acting(MONSTERS),
        OldPhase::Acting(seat) => Phase::Acting(wizard(seat)?),
        OldPhase::TurnEnd(seat) if seat == shared => Phase::TurnEnd(MONSTERS),
        OldPhase::TurnEnd(seat) => Phase::TurnEnd(wizard(seat)?),
        OldPhase::RoundEnd => Phase::RoundEnd,
        OldPhase::Over => Phase::Over,
    })
}
