//! Stasis: every enemy unit on a map square is paralysed. The unit cannot move or, in a battle,
//! act (it is treated as asleep, as under Black Sleep) for at least the turn that follows the
//! cast. After that, each turn it rolls a resistance check at -5 and is freed if it resists
//! (the original frees it on the same roll; help topic 554 says -3).

use crate::battle::immunity::{resistance_failure, Realm};
use crate::rng::GameRng;
use crate::{Event, Game};

/// Hook of the spell.
pub const STASIS: &str = "stasis";
/// The resistance penalty of the roll that frees a unit.
const FREEING_MODIFIER: i32 = -5;

/// A unit held in stasis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stasis {
    /// Just put there: held through the next turn whatever it rolls.
    Fresh,
    /// Rolls to be freed at each turn change.
    Lingering,
}

impl Game {
    /// Whether the unit is held in stasis.
    pub fn unit_in_stasis(&self, unit: hecs::Entity) -> bool {
        self.world.get::<&Stasis>(unit).is_ok()
    }

    /// Stasis on one unit of the chosen square: enemy units only, and not those that Magic
    /// Immunity protects. Returns whether the unit is held now.
    pub(crate) fn put_in_stasis(
        &mut self,
        unit: hecs::Entity,
        caster: crate::city::PlayerId,
    ) -> bool {
        if self.units().unit_owner(unit) == Some(caster)
            || self.unit_in_stasis(unit)
            || self.units().unit_protected_from(unit, Some(Realm::Sorcery))
        {
            return false;
        }
        let _ = self.world.insert_one(unit, Stasis::Fresh);
        true
    }

    /// At a turn change: a lingering unit rolls to be freed, and a fresh one starts lingering.
    /// Call before the moves are refilled.
    pub(crate) fn process_stasis(&mut self) -> Vec<Event> {
        let mut held: Vec<(hecs::Entity, Stasis)> = self
            .world
            .query::<&Stasis>()
            .iter()
            .map(|(e, s)| (e, *s))
            .collect();
        held.sort_by_key(|&(e, _)| (self.units().serial_of(e), e.id()));
        let mut rng = std::mem::replace(&mut self.rng, GameRng::lent());
        let mut events = Vec::new();
        for (unit, state) in held {
            match state {
                Stasis::Fresh => {
                    let _ = self.world.insert_one(unit, Stasis::Lingering);
                }
                Stasis::Lingering => {
                    let resistance = self
                        .units()
                        .unit_resistance_against(unit, Some(Realm::Sorcery));
                    if resistance_failure(resistance, FREEING_MODIFIER, &mut rng) == 0 {
                        let _ = self.world.remove_one::<Stasis>(unit);
                        events.push(Event::StasisEnded { unit });
                    }
                }
            }
        }
        self.rng = rng;
        events
    }
}
