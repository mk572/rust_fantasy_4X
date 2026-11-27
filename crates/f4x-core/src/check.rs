//! The checker of a world: what must hold in a game whatever was done to it (`docs/f4x-core-test-design.md`,
//! class 2). Written from the rules, not from the code that keeps them, so a bug in a handler shows here.
//! It reads the game through its views and reports each broken invariant as a line of text, `rule: what`.
//!
//! Tests: here, which break each rule on purpose to see it found; over the fresh worlds in
//! `tests/it/c2_invariant.rs` and turn after turn in `f4x-master/tests/it/invariant.rs`.

use std::collections::{HashMap, HashSet};

use crate::map::Position;
use crate::player::PlayerId;
use crate::world::World;

/// Where a player's army ledger differs from the units that name the player as their owner,
/// oldest first.
pub(crate) fn army_problems(w: &World) -> Vec<String> {
    let units = w.units();
    let mut owned: HashMap<PlayerId, Vec<hecs::Entity>> = HashMap::new();
    for u in units.all_units() {
        if let Some(owner) = units.unit_owner(u) {
            owned.entry(owner).or_default().push(u);
        }
    }
    let mut players = w.player_ids();
    players.push(crate::player::MONSTERS);
    players
        .into_iter()
        .filter_map(|p| {
            let listed = units.player_units(p);
            let have = owned.remove(&p).unwrap_or_default();
            (listed != have).then(|| format!("army: {p:?} lists {listed:?} and has {have:?}"))
        })
        .collect()
}

/// Every invariant the world breaks, empty when it holds.
pub fn world_problems(w: &World) -> Vec<String> {
    let mut bad = Vec::new();
    let mut say = |rule: &str, what: String| bad.push(format!("{rule}: {what}"));
    let Some(map) = w.map() else {
        // A game with no map holds no units or cities.
        if !w.units().all_units().is_empty() {
            say("map", "units in a game with no map".into());
        }
        return bad;
    };
    // The monsters' owner is a player of the world that `player_ids` does not list.
    let mut players: HashSet<_> = w.player_ids().into_iter().collect();
    if w.player_entity(crate::player::MONSTERS).is_some() {
        players.insert(crate::player::MONSTERS);
    }
    let on_map = |p: Position| p.x < map.width() && p.y < map.height();

    // Units: an owner that exists, a square on the map, room on the square, a number of their own.
    let units = w.units();
    let mut serials = HashSet::new();
    let mut per_square: HashMap<Position, usize> = HashMap::new();
    for u in units.all_units() {
        match units.unit_owner(u) {
            Some(o) if players.contains(&o) => {}
            owner => say("unit-owner", format!("{u:?} belongs to {owner:?}")),
        }
        match units.unit_position(u) {
            Some(at) if on_map(at) => *per_square.entry(at).or_default() += 1,
            at => say("unit-square", format!("{u:?} stands at {at:?}")),
        }
        if !serials.insert(units.serial_of(u)) {
            say("serial", format!("{u:?} shares its creation number"));
        }
        // A unit that has taken all its hit points is dead and gone, not still standing.
        let (now, most) = units.unit_hits(u);
        if most > 0 && now == 0 {
            say("unit-dead", format!("{u:?} stands with no hit points"));
        }
    }
    for (at, n) in &per_square {
        // The rules let a square hold more than `max_tile` (units put there, not walked); the
        // game never keeps more than the sanity limit.
        if *n > crate::unit::TILE_SANITY_LIMIT {
            say(
                "tile-room",
                format!(
                    "{n} units at {at:?}, at most {}",
                    crate::unit::TILE_SANITY_LIMIT
                ),
            );
        }
    }

    // Cities: an owner that exists, one to a square, on land, within the size limit, a number of their own.
    let economy = w.economy();
    let mut city_squares = HashSet::new();
    for (city, info, at) in economy.cities() {
        if !players.contains(&info.owner) {
            say(
                "city-owner",
                format!("{} belongs to {:?}", info.name, info.owner),
            );
        }
        if !on_map(at) {
            say("city-square", format!("{} stands at {at:?}", info.name));
            continue;
        }
        if !city_squares.insert(at) {
            say("city-twice", format!("two cities at {at:?}"));
        }
        if economy.terrain_class_at(at) == Some(crate::map::terrain::TerrainClass::Ocean) {
            say(
                "city-sea",
                format!("{} stands in the sea at {at:?}", info.name),
            );
        }
        if economy
            .city_economy(city)
            .is_some_and(|e| e.figures > economy.max_city_size())
        {
            say(
                "city-size",
                format!(
                    "{} has more citizens than the size limit {}",
                    info.name,
                    economy.max_city_size()
                ),
            );
        }
        if !serials.insert(units.serial_of(city)) {
            say(
                "serial",
                format!("{} shares its creation number", info.name),
            );
        }
    }

    // Players: the units and cities named above are theirs; a wizard has its fortress where a city of theirs is.
    for &p in &players {
        if w.is_neutral_player(p) {
            continue;
        }
        let Some(wizard) = w.wizard(p) else { continue };
        if let Some(at) = wizard.fortress() {
            let own = economy.city_at(at).and_then(|c| economy.city_owner(c));
            if own != Some(p) && w.player(p).is_some_and(|x| !x.is_defeated()) {
                say(
                    "fortress",
                    format!("{p:?}: the fortress at {at:?} is in {own:?}'s city or none"),
                );
            }
        }
    }
    // The ledgers: each player's lists its units and no others.
    bad.extend(army_problems(w));

    bad
}

/// The digest of a game: its saved text cut at the fields of the snapshot, each field hashed (FNV-1a,
/// sixteen hex digits). Two games that are the same have the same digest, and a game that drifted shows
/// which fields moved (`units`, `cities`, `draws`) without a whole text to compare. The pinned digests of
/// `tests/it/c8_drift.rs` are made from it (`docs/f4x-core-test-design.md`, class 8).
pub fn digest(w: &World) -> Vec<(String, String)> {
    let text = w.snapshot().to_text().expect("a game writes as text");
    // The first line carries the checksum of the rest; the fields start at four spaces.
    let body = text.split_once('\n').map_or("", |(_, b)| b);
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in body.lines() {
        let key = line
            .strip_prefix("    ")
            .filter(|rest| !rest.starts_with(' '))
            .and_then(|rest| rest.split_once(':'))
            .map(|(k, _)| k)
            .filter(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        match (key, fields.last_mut()) {
            (Some(k), _) => fields.push((k.to_string(), format!("{line}\n"))),
            (None, Some((_, text))) => {
                text.push_str(line);
                text.push('\n');
            }
            (None, None) => {}
        }
    }
    // The versions are no drift of the game: a release moves every pin otherwise.
    fields
        .into_iter()
        .filter(|(k, _)| !matches!(k.as_str(), "version" | "game_version"))
        .map(|(k, text)| {
            let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
            });
            (k, format!("{hash:016x}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::terrain::TerrainClass;
    use crate::player::Owner;
    use crate::unit::Hurt;
    use crate::world::testutil::fresh;

    fn world() -> World {
        fresh(3, 7).expect("the fresh game").world
    }

    /// Did the checker find the rule, and only it?
    fn only(w: &World, rule: &str) {
        let bad = world_problems(w);
        assert!(!bad.is_empty(), "{rule} was not found");
        assert!(
            bad.iter().all(|b| b.starts_with(&format!("{rule}:"))),
            "{rule}: {bad:#?}"
        );
    }

    fn some_unit(w: &World) -> hecs::Entity {
        w.units().all_units()[0]
    }

    #[test]
    fn a_world_that_holds_has_no_problem() {
        assert_eq!(world_problems(&world()), Vec::<String>::new());
    }

    #[test]
    fn a_unit_without_an_owner_is_found() {
        let mut w = world();
        let owner = w.units().unit_owner(some_unit(&w)).expect("an owner");
        let entity = w.player_entity(owner).expect("the player");
        w.ecs.despawn(entity).expect("despawn");
        let bad = world_problems(&w);
        assert!(bad.iter().any(|b| b.starts_with("unit-owner:")), "{bad:#?}");
    }

    #[test]
    fn a_unit_that_changed_hands_past_the_ledger_is_found() {
        let mut w = world();
        let u = some_unit(&w);
        let owner = w.units().unit_owner(u).expect("an owner");
        let other = w
            .player_ids()
            .into_iter()
            .find(|&p| p != owner)
            .expect("another player");
        let entity = w.player_entity(other).expect("the player");
        w.put(u, Owner(entity));
        only(&w, "army");
    }

    #[test]
    fn a_unit_off_the_map_is_found() {
        let mut w = world();
        let u = some_unit(&w);
        let at = w.units().unit_position(u).expect("a square");
        w.put(u, Position { x: 10_000, ..at });
        only(&w, "unit-square");
    }

    #[test]
    fn a_crowded_square_is_found() {
        let mut w = world();
        let u = some_unit(&w);
        let (owner, at) = (
            w.units().unit_owner(u).expect("owner"),
            w.units().unit_position(u).expect("square"),
        );
        let kind = w.units().unit_kind_id(u).expect("a kind");
        while w.units().units_at(at).len() <= crate::unit::TILE_SANITY_LIMIT {
            w.spawn_unit(owner, at, kind);
        }
        only(&w, "tile-room");
    }

    #[test]
    fn two_units_with_one_creation_number_are_found() {
        let mut w = world();
        let all = w.units().all_units();
        let serial = w.units().serial_of(all[0]);
        w.put(all[1], serial);
        only(&w, "serial");
    }

    #[test]
    fn a_city_over_the_size_limit_is_found() {
        let mut w = world();
        let (city, ..) = w.economy().cities().remove(0);
        let over = w.economy().max_city_size() + 1;
        w.scenario_set_citizens(city, over);
        only(&w, "city-size");
    }

    #[test]
    fn a_unit_with_no_hit_points_is_found() {
        let mut w = world();
        let u = some_unit(&w);
        let (_, most) = w.units().unit_hits(u);
        w.put(u, Hurt(most));
        only(&w, "unit-dead");
    }

    #[test]
    fn two_cities_on_one_square_are_found() {
        let mut w = world();
        let (_, info, at) = w.economy().cities().remove(0);
        let owner = info.owner;
        w.scenario_found_city(owner, at, "Twin", None, 1);
        let bad = world_problems(&w);
        assert!(bad.iter().any(|b| b.starts_with("city-twice:")), "{bad:#?}");
    }

    #[test]
    fn a_city_in_the_sea_is_found() {
        let mut w = world();
        let (_, _, at) = w.economy().cities().remove(0);
        let sea = w
            .content()
            .terrains
            .first_of_class(TerrainClass::Ocean, at.plane)
            .expect("an ocean");
        w.scenario_set_terrain(at, sea);
        let bad = world_problems(&w);
        assert!(bad.iter().any(|b| b.starts_with("city-sea:")), "{bad:#?}");
    }

    #[test]
    fn a_fortress_in_another_wizards_city_is_found() {
        let mut w = world();
        let cities = w.economy().cities();
        let (mine, other) = {
            let first = &cities[0];
            let second = cities
                .iter()
                .find(|c| c.1.owner != first.1.owner)
                .expect("two owners");
            (first.0, second.1.owner)
        };
        let owner_entity = w.player_entity(other).expect("a player");
        w.put(mine, Owner(owner_entity));
        let bad = world_problems(&w);
        assert!(bad.iter().any(|b| b.starts_with("fortress:")), "{bad:#?}");
    }
}
