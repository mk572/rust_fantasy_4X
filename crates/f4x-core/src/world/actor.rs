//! Whose command it is: the player a [`Command`] acts for, read from the command itself (the
//! player or owner it names, the owner of the units or the city it names, or the side to play
//! in the battle). `f4x-master` uses it to refuse a command its sender may not give.
//!
//! Tests: `tests/it/c9_world.rs`, and `f4x-master`, `tests/it/sender.rs`.

use crate::player::PlayerId;
use crate::world::{Command, World};

impl World {
    /// The player a command acts for, or `None` when it names nobody: the units or the city it
    /// orders are gone, or it is a battle order and no battle is being fought. The world
    /// refuses such a command with its reason.
    pub fn command_actor(&self, command: &Command) -> Option<PlayerId> {
        let first =
            |units: &[hecs::Entity]| units.first().and_then(|&u| self.units().unit_owner(u));
        let city = |c: hecs::Entity| self.economy().city_owner(c);
        match command {
            Command::Anyway(inner) => self.command_actor(inner),
            Command::Battle(_) => self.battle().map(|b| b.player_of(b.active)),
            Command::Diplomacy(talk) => Some(talk.speaker()),
            Command::CreateUnit { owner, .. } | Command::FoundCity { owner, .. } => Some(*owner),
            Command::MoveStack { units, .. }
            | Command::GoTo { units, .. }
            | Command::GiveOrder { units, .. }
            | Command::ClearOrders { units }
            | Command::SetStandby { units, .. }
            | Command::EnterSite { units, .. }
            | Command::Purify { units }
            | Command::BuildRoad { units, .. }
            | Command::Meld { units }
            | Command::ShiftPlane { units }
            | Command::EarthGate { units, .. } => first(units),
            Command::Defend { fighters, .. } => first(fighters),
            Command::BeginBattle { attackers, .. } | Command::AutoResolve { attackers, .. } => {
                first(attackers)
            }
            Command::DismissUnit { unit }
            | Command::SetBridge { unit, .. }
            | Command::SettleOutpost { unit, .. } => first(&[*unit]),
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
            | Command::BalanceFarmers { player }
            | Command::Task { player, .. }
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

    /// Whether `command` is `player`'s to give: it acts for no other player, and every unit it
    /// orders is that player's. What names nobody ([`World::command_actor`]) is left to the
    /// world to refuse.
    pub fn may_command(&self, player: PlayerId, command: &Command) -> bool {
        self.command_actor(command).is_none_or(|p| p == player)
            && ordered_units(command)
                .iter()
                .all(|&u| self.units().unit_owner(u).is_none_or(|p| p == player))
    }
}

/// The units a command orders about as a group (the enemies it names are not among them).
/// Commands that name their player as well (`ReleaseTroops`) are left to the world, which
/// checks each unit against that player.
fn ordered_units(command: &Command) -> &[hecs::Entity] {
    match command {
        Command::Anyway(inner) => ordered_units(inner),
        Command::MoveStack { units, .. }
        | Command::GoTo { units, .. }
        | Command::GiveOrder { units, .. }
        | Command::ClearOrders { units }
        | Command::SetStandby { units, .. }
        | Command::EnterSite { units, .. }
        | Command::Purify { units }
        | Command::BuildRoad { units, .. }
        | Command::Meld { units }
        | Command::ShiftPlane { units }
        | Command::EarthGate { units, .. } => units,
        Command::BeginBattle { attackers, .. } | Command::AutoResolve { attackers, .. } => {
            attackers
        }
        Command::Defend { fighters, .. } => fighters,
        _ => &[],
    }
}
