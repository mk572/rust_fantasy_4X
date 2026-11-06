//! Whose command it is: the player a [`Command`] acts for, read from the command itself (the
//! player or owner it names, or the owner of the units or the city it names). `f4x-master`
//! uses it to refuse a command for a player whose turn it is not.
//!
//! Tests: in actor/tests.rs.

use crate::game::{Command, Game};
use crate::player::PlayerId;

impl Game {
    /// The player a command acts for, or `None` when the command names none (the round's own
    /// work, a battle's commands, diplomacy between two wizards, and entities that are gone).
    pub fn command_actor(&self, command: &Command) -> Option<PlayerId> {
        let first =
            |units: &[hecs::Entity]| units.first().and_then(|&u| self.units().unit_owner(u));
        let city = |c: hecs::Entity| self.economy().city_owner(c);
        match command {
            Command::EndTurn
            | Command::Battle(_)
            | Command::Diplomacy(_)
            | Command::AutoResolve { .. } => None,
            Command::CreateUnit { owner, .. } | Command::FoundCity { owner, .. } => Some(*owner),
            Command::MoveStack { units, .. }
            | Command::GoTo { units, .. }
            | Command::ClearOrders { units }
            | Command::SetStandby { units, .. }
            | Command::EnterSite { units, .. }
            | Command::Purify { units }
            | Command::BuildRoad { units, .. }
            | Command::Meld { units }
            | Command::ShiftPlane { units }
            | Command::EarthGate { units, .. } => first(units),
            Command::BeginBattle { attackers, .. } => first(attackers),
            Command::DismissUnit { unit } | Command::SettleOutpost { unit, .. } => first(&[*unit]),
            Command::SetProduction { city: c, .. }
            | Command::BuyProduction { city: c }
            | Command::SetFarmers { city: c, .. }
            | Command::SetAutoBuild { city: c, .. }
            | Command::SellBuilding { city: c, .. }
            | Command::RenameCity { city: c, .. } => city(*c),
            Command::TurnOffUnitSpell { player, .. }
            | Command::MoveItem { player, .. }
            | Command::CraftItem { player, .. }
            | Command::ScrapItem { player, .. }
            | Command::SellItem { player, .. }
            | Command::AcceptOffer { player, .. }
            | Command::RejectOffer { player, .. }
            | Command::PlaceFortress { player, .. }
            | Command::Resign { player }
            | Command::RazeCity { player, .. }
            | Command::SetTaxRate { player, .. }
            | Command::SetGrandVizier { player, .. }
            | Command::SetPowerSplit { player, .. }
            | Command::ReleaseUpkeep { player, .. }
            | Command::ReleaseTroops { player, .. }
            | Command::SetResearch { player, .. }
            | Command::CastSpell { player, .. }
            | Command::ResolveCast { player, .. }
            | Command::CancelCast { player }
            | Command::TurnOffSpell { player, .. }
            | Command::TurnOffCitySpell { player, .. }
            | Command::Alchemy { player, .. } => Some(*player),
        }
    }
}

#[cfg(test)]
mod tests;
