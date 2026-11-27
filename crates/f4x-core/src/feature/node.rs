//! Magic nodes: generation, ownership by melding, and the power they yield.
//!
//! A node's power is the number of squares its aura covers (5-10 on Arcanus, 10-20 on
//! Myrror). Each owned node gives `power * (strength + 1) / 2` mana power, doubled by the
//! owner's mastery of its realm and again by Node Mastery; a warped node costs five instead.
//! `strength` is the game's node setting: 0 weak, 1 normal, 2 powerful.

use crate::rules::{Ability, RealmId};
use crate::world::rng::GameRng;
use serde::{Deserialize, Serialize};

use crate::feature::entity::NodeInfo;
use crate::player::PlayerId;

use crate::world::{Event, World};

/// A magic node: a component of the feature entity on the node's square, with an aura of squares
/// around it. A wizard who melds a spirit into it owns it (the entity's `Owner`) and draws power
/// from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// The realm of the node's magic: "sorcery", "nature" or "chaos".
    pub realm: RealmId,
    /// Strength: how many squares the aura covers, and so the power the node yields.
    pub power: u8,
    /// The squares of the aura, the node's own tile first.
    pub aura: Vec<(u32, u32)>,
    /// A Guardian Spirit holds the node: rivals who meld there only succeed one time in four.
    #[serde(default)]
    pub guardian: bool,
    /// A warped node drains its owner of five power instead of giving any.
    #[serde(default)]
    pub warped: bool,
}

/// Why a meld did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeldError {
    /// No units, or they are not all one player's on one tile.
    BadSelection,
    /// There is no node here.
    NoNode,
    /// None of the units can meld.
    NoMelder,
    /// The node is warped and cannot be melded with.
    Warped,
    /// The player owns the node and already guards it (`Unit_Action_Special_Meld` gives no
    /// meld there).
    AlreadyGuarded,
}

impl MeldError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::BadSelection => "refuse_map.meld_error_bad_selection",
            Self::NoNode => "refuse_map.meld_error_no_node",
            Self::NoMelder => "refuse_map.meld_error_no_melder",
            Self::Warped => "refuse_map.meld_error_warped",
            Self::AlreadyGuarded => "refuse_map.meld_error_already_guarded",
        }
    }
}

impl World {
    /// Power one node yields to its owner this turn.
    pub fn node_power(&self, node: &NodeInfo) -> i32 {
        if node.warped {
            return -5;
        }
        let mut power =
            i32::from(node.power) * (i32::from(self.settings.basic.magic_strength) + 1) / 2;
        power = self.events.node_power(node.realm, power);
        let Some(owner) = node.owner else {
            return 0;
        };
        if let Some(m) = self.wizard(owner) {
            let facts = crate::rules::exec::Facts {
                realm: Some(node.realm),
                ..Default::default()
            };
            let doublings = crate::wizard::retort::stat_with(
                &self.content.rules,
                &m.retorts(),
                crate::rules::Stat::NodePowerDoublings,
                0,
                &facts,
            );
            power <<= doublings.clamp(0, 8);
        }
        power
    }

    /// Power from all the nodes `player` owns.
    pub fn node_income(&self, player: PlayerId) -> i32 {
        self.economy()
            .nodes()
            .iter()
            .filter(|n| n.owner == Some(player))
            .map(|n| self.node_power(n))
            .sum()
    }

    /// Meld a spirit into the node the stack stands on. The spirit is used up; a Guardian
    /// Spirit is preferred and leaves the node guarded. Against a rival's guarded node the
    /// meld succeeds one time in four, and the spirit dies either way.
    pub fn meld(
        &mut self,
        units: &[hecs::Entity],
        rng: &mut GameRng,
    ) -> Result<Vec<Event>, MeldError> {
        let first = *units.first().ok_or(MeldError::BadSelection)?;
        let at = self
            .units()
            .unit_position(first)
            .ok_or(MeldError::BadSelection)?;
        let owner = self
            .units()
            .unit_owner(first)
            .ok_or(MeldError::BadSelection)?;
        if units.iter().any(|&u| {
            self.units().unit_position(u) != Some(at) || self.units().unit_owner(u) != Some(owner)
        }) {
            return Err(MeldError::BadSelection);
        }
        let node = self.economy().node_at(at).ok_or(MeldError::NoNode)?;
        if node.warped {
            return Err(MeldError::Warped);
        }
        if node.owner == Some(owner) && node.guardian {
            return Err(MeldError::AlreadyGuarded);
        }
        let melders: Vec<hecs::Entity> = units
            .iter()
            .copied()
            .filter(|&u| {
                self.units()
                    .unit_has_ability(u, Ability::new(42 /* meld */))
            })
            .collect();
        let guardian = melders.iter().copied().find(|&u| {
            self.units()
                .unit_def(u)
                .is_some_and(|d| d.num == 176 /* guardian_spirit */)
        });
        let melder = guardian
            .or(melders.last().copied())
            .ok_or(MeldError::NoMelder)?;
        let taken = match node.owner {
            None => true,
            Some(o) if o == owner => true,
            Some(_) if !node.guardian => true,
            Some(_) => rng.roll(100) <= 25,
        };
        crate::unit::ledger::record(&self.ecs, melder, crate::unit::ledger::Departure::UsedUp);
        self.despawn_with_spells(melder);
        let mut events = Vec::new();
        if taken {
            if let Some(player) = self.player_entity(owner) {
                self.put(node.entity, crate::player::Owner(player));
            }
            if let Ok(mut n) = self.ecs.get::<&mut Node>(node.entity) {
                // A rival's guardian is gone with the successful meld; ours stands again.
                n.guardian = guardian.is_some();
            }
            events.push(Event::NodeMelded {
                player: owner,
                pos: at,
            });
        } else {
            events.push(Event::MeldFailed {
                player: owner,
                pos: at,
            });
        }
        Ok(events)
    }
}
