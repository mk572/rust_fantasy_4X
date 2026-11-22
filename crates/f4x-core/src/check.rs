//! The checker of a world: what must hold in a game whatever was done to it (`docs/f4x-core-test-design.md`,
//! class 2). Written from the rules, not from the code that keeps them, so a bug in a handler shows here.
//! It reads the game through its views and reports each broken invariant as a line of text, `rule: what`.
//!
//! Tests: in `tests/it/c2_invariant.rs`, which also breaks each rule on purpose to see it found.

use std::collections::{HashMap, HashSet};

use crate::game::Game;
use crate::map::Position;

/// Every invariant the world breaks, empty when it holds.
pub fn world_problems(g: &Game) -> Vec<String> {
    let mut bad = Vec::new();
    let mut say = |rule: &str, what: String| bad.push(format!("{rule}: {what}"));
    let Some(map) = g.map() else {
        // A game with no map holds no units or cities.
        if !g.units().all_units().is_empty() {
            say("map", "units in a game with no map".into());
        }
        return bad;
    };
    let players: HashSet<_> = g.player_ids().into_iter().collect();
    let on_map = |p: Position| p.x < map.width() && p.y < map.height();

    // Units: an owner that exists, a square on the map, room on the square, a number of their own.
    let units = g.units();
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
    let economy = g.economy();
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
        if g.is_neutral_player(p) {
            continue;
        }
        let Some(w) = g.wizard(p) else { continue };
        if let Some(at) = w.fortress() {
            let own = economy.city_at(at).and_then(|c| economy.city_owner(c));
            if own != Some(p) && g.player(p).is_some_and(|x| !x.is_defeated()) {
                say(
                    "fortress",
                    format!("{p:?}: the fortress at {at:?} is in {own:?}'s city or none"),
                );
            }
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::fresh;
    use crate::map::terrain::TerrainClass;
    use crate::player::Owner;
    use crate::unit::Hurt;

    fn world() -> Game {
        fresh(3, 7).expect("the fresh game")
    }

    /// Did the checker find the rule, and only it?
    fn only(g: &Game, rule: &str) {
        let bad = world_problems(g);
        assert!(!bad.is_empty(), "{rule} was not found");
        assert!(
            bad.iter().all(|b| b.starts_with(&format!("{rule}:"))),
            "{rule}: {bad:#?}"
        );
    }

    fn some_unit(g: &Game) -> hecs::Entity {
        g.units().all_units()[0]
    }

    #[test]
    fn a_world_that_holds_has_no_problem() {
        assert_eq!(world_problems(&world()), Vec::<String>::new());
    }

    #[test]
    fn a_unit_without_an_owner_is_found() {
        let mut g = world();
        let owner = g.units().unit_owner(some_unit(&g)).expect("an owner");
        let entity = g.player_entity(owner).expect("the player");
        g.world.despawn(entity).expect("despawn");
        let bad = world_problems(&g);
        assert!(bad.iter().any(|b| b.starts_with("unit-owner:")), "{bad:#?}");
    }

    #[test]
    fn a_unit_off_the_map_is_found() {
        let mut g = world();
        let u = some_unit(&g);
        let at = g.units().unit_position(u).expect("a square");
        g.put(u, Position { x: 10_000, ..at });
        only(&g, "unit-square");
    }

    #[test]
    fn a_crowded_square_is_found() {
        let mut g = world();
        let u = some_unit(&g);
        let (owner, at) = (
            g.units().unit_owner(u).expect("owner"),
            g.units().unit_position(u).expect("square"),
        );
        let kind = g.units().unit_kind_id(u).expect("a kind");
        while g.units().units_at(at).len() <= crate::unit::TILE_SANITY_LIMIT {
            g.spawn_unit(owner, at, kind);
        }
        only(&g, "tile-room");
    }

    #[test]
    fn two_units_with_one_creation_number_are_found() {
        let mut g = world();
        let all = g.units().all_units();
        let serial = g.units().serial_of(all[0]);
        g.put(all[1], serial);
        only(&g, "serial");
    }

    #[test]
    fn a_city_over_the_size_limit_is_found() {
        let mut g = world();
        let (city, ..) = g.economy().cities().remove(0);
        let over = g.economy().max_city_size() + 1;
        g.scenario_set_citizens(city, over);
        only(&g, "city-size");
    }

    #[test]
    fn a_unit_with_no_hit_points_is_found() {
        let mut g = world();
        let u = some_unit(&g);
        let (_, most) = g.units().unit_hits(u);
        g.put(u, Hurt(most));
        only(&g, "unit-dead");
    }

    #[test]
    fn two_cities_on_one_square_are_found() {
        let mut g = world();
        let (_, info, at) = g.economy().cities().remove(0);
        let owner = info.owner;
        g.scenario_found_city(owner, at, "Twin", None, 1);
        let bad = world_problems(&g);
        assert!(bad.iter().any(|b| b.starts_with("city-twice:")), "{bad:#?}");
    }

    #[test]
    fn a_city_in_the_sea_is_found() {
        let mut g = world();
        let (_, _, at) = g.economy().cities().remove(0);
        let sea = g
            .content()
            .terrains
            .first_of_class(TerrainClass::Ocean, at.plane)
            .expect("an ocean");
        g.scenario_set_terrain(at, sea);
        let bad = world_problems(&g);
        assert!(bad.iter().any(|b| b.starts_with("city-sea:")), "{bad:#?}");
    }

    #[test]
    fn a_fortress_in_another_wizards_city_is_found() {
        let mut g = world();
        let cities = g.economy().cities();
        let (mine, other) = {
            let first = &cities[0];
            let second = cities
                .iter()
                .find(|c| c.1.owner != first.1.owner)
                .expect("two owners");
            (first.0, second.1.owner)
        };
        let owner_entity = g.player_entity(other).expect("a player");
        g.put(mine, Owner(owner_entity));
        let bad = world_problems(&g);
        assert!(bad.iter().any(|b| b.starts_with("fortress:")), "{bad:#?}");
    }
}
