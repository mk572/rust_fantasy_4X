//! The upgrade of a save from before game version 0.0.6, where every independent city was a
//! player of its own: the cities of a race now belong to one player (task 522), numbered
//! `FIRST_RACE_PLAYER` plus the race. Each old city player is mapped to the player of its first
//! city's race; their treasuries are added up; their units, cities, nodes and items go with
//! them; the fog of the old players is dropped (the new player sees what its cities and units
//! see); a city player with no city is dropped with its units.
//!
//! Tests: in game/save/tests.rs.

use super::{HeldSave, Snapshot};
use crate::player::race_player::FIRST_RACE_PLAYER;
use crate::player::MONSTERS;
use std::collections::BTreeMap;

/// Bring a snapshot of an older game version to one player for each race.
pub(super) fn upgrade(snapshot: &mut Snapshot) {
    let old: Vec<u8> = snapshot
        .players
        .iter()
        .filter(|p| p.neutral && p.id != MONSTERS.0 && p.id >= FIRST_RACE_PLAYER)
        .map(|p| p.id)
        .collect();
    let mut target: BTreeMap<u8, Option<u8>> = BTreeMap::new();
    for &id in &old {
        let race = snapshot
            .cities
            .iter()
            .filter(|c| c.owner == id)
            .find_map(|c| c.race);
        target.insert(
            id,
            race.map(|r| {
                FIRST_RACE_PLAYER
                    .saturating_add(r as u8)
                    .min(MONSTERS.0 - 1)
            }),
        );
    }
    let map = |id: u8| -> Option<u8> {
        match target.get(&id) {
            Some(to) => *to,
            None => Some(id),
        }
    };
    // The players: one for each target, with the gold and mana of the old ones added up.
    let mut merged: BTreeMap<u8, super::PlayerSave> = BTreeMap::new();
    snapshot.players.retain_mut(|p| {
        let Some(&Some(to)) = target.get(&p.id) else {
            return !target.contains_key(&p.id);
        };
        match merged.get_mut(&to) {
            Some(into) => {
                into.gold += p.gold;
                into.mana += p.mana;
            }
            None => {
                let mut first = p.clone();
                first.id = to;
                first.offers.clear();
                merged.insert(to, first);
            }
        }
        false
    });
    snapshot.players.extend(merged.into_values());
    // Whatever belonged to a dropped player goes with it.
    snapshot.units.retain_mut(|u| match map(u.owner) {
        Some(to) => {
            u.owner = to;
            true
        }
        None => false,
    });
    for c in &mut snapshot.cities {
        if let Some(to) = map(c.owner) {
            c.owner = to;
        }
        c.conquered = c.conquered.and_then(map);
    }
    for f in &mut snapshot.features {
        f.owner = f.owner.and_then(map);
    }
    snapshot.items.retain_mut(|i| {
        let remap = |p: &mut u8| match map(*p) {
            Some(to) => {
                *p = to;
                true
            }
            None => false,
        };
        match &mut i.held {
            HeldSave::Vault(p, _)
            | HeldSave::Award(p)
            | HeldSave::Crafting(p)
            | HeldSave::Offered(p) => remap(p),
            HeldSave::Hero(..) => true,
        }
    });
    snapshot
        .fog
        .players
        .retain(|(p, ..)| !target.contains_key(p));
    snapshot.plans.retain(|(p, _)| !target.contains_key(&p.0));
}
