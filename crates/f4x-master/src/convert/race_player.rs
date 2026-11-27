//! The upgrade of a save from before game version 0.0.6, where every independent city was a
//! player of its own: the cities of a race now belong to one player (task 522), numbered
//! `FIRST_RACE_PLAYER` plus the race. Each old city player is mapped to the player of its first
//! city's race; their treasuries are added up; their units, cities, nodes and items go with
//! them; the fog of the old players is dropped (the new player sees what its cities and units
//! see); a city player with no city is dropped with its units.
//!
//! Tests: below.

use f4x_core::player::race_player::FIRST_RACE_PLAYER;
use f4x_core::player::MONSTERS;
use f4x_core::world::save::{HeldSave, PlayerSave, Snapshot};
use std::collections::BTreeMap;

use crate::game::save::Plans;

/// The first game version with one player for each race.
pub(super) const SINCE: &str = "0.0.6";

/// Bring a world of an older game version, and the plans saved with it, to one player for each
/// race.
pub(super) fn upgrade(snapshot: &mut Snapshot, plans: &mut Plans) {
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
    let mut merged: BTreeMap<u8, PlayerSave> = BTreeMap::new();
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
    plans.retain(|(p, _)| !target.contains_key(&p.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::player::PlayerId;
    use f4x_core::world::testutil::{fresh, shipped, FRESH_WORLDS};
    use f4x_def::half_int::HalfInt;

    #[test]
    fn a_world_with_a_player_for_each_city_is_brought_to_one_for_each_race() {
        let two_cities = |base: &Snapshot| {
            base.players
                .iter()
                .find(|p| {
                    p.neutral
                        && p.id != MONSTERS.0
                        && p.id >= FIRST_RACE_PLAYER
                        && base.cities.iter().filter(|c| c.owner == p.id).count() >= 2
                })
                .cloned()
        };
        // A player of two independent cities, one of which gets a player of its own.
        let (base, race_player) = FRESH_WORLDS
            .iter()
            .find_map(|&(wizards, seed)| {
                let base = fresh(wizards, seed).expect("a fresh world").snapshot();
                two_cities(&base).map(|p| (base, p))
            })
            .expect("a fresh world with such a player");
        let city_at = base
            .cities
            .iter()
            .find(|c| c.owner == race_player.id)
            .expect("a city of it")
            .pos;
        // The old world: that city has a player of its own, with a treasury of its own.
        let own = (FIRST_RACE_PLAYER..MONSTERS.0)
            .find(|id| base.players.iter().all(|p| p.id != *id))
            .expect("a free player number");
        let mut old = base.clone();
        let mut extra = race_player.clone();
        extra.id = own;
        extra.gold = HalfInt::from_int(7);
        old.players.push(extra);
        old.cities
            .iter_mut()
            .find(|c| c.pos == city_at)
            .expect("the city")
            .owner = own;
        let mut plans = vec![(PlayerId(own), "old".to_string())];

        upgrade(&mut old, &mut plans);

        assert!(plans.is_empty(), "the plans of a city player stayed");
        let w = old.restore(shipped()).expect("the world restores");
        assert!(w.player(PlayerId(own)).is_none(), "the old player is there");
        let city = w.economy().city_at(city_at).expect("the city");
        assert_eq!(w.economy().city_owner(city), Some(PlayerId(race_player.id)));
        let gold = w
            .player(PlayerId(race_player.id))
            .expect("the player")
            .treasury()
            .gold;
        assert_eq!(gold, race_player.gold + HalfInt::from_int(7));
    }
}
