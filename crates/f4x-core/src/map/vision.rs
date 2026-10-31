//! What a player can see right now (as opposed to merely having explored), worked out by the
//! `Economy` handle (world, map, content, players) without a `Game`.

use crate::city::economy::Economy;
use crate::map::{Plane, Position};
use crate::player::PlayerId;

use crate::rules::stats::compute;
use crate::rules::Stat;

/// Sight of a city (half-squares) before walls, an Oracle or Nature's Eye raise it.
const CITY_SIGHT: i32 = 5;

impl Economy<'_> {
    /// Can `viewer` make out `unit`? One's own units always, others unless invisible; an
    /// invisible unit shows to a viewer who has a unit immune to illusions in sight of it.
    pub fn unit_visible_to(&self, unit: hecs::Entity, viewer: PlayerId) -> bool {
        let units = self.units();
        if units.unit_owner(unit) == Some(viewer) || !units.unit_has_ability(unit, "invisibility") {
            return true;
        }
        let Some(at) = units.unit_position(unit) else {
            return true;
        };
        let Some(map) = self.map else {
            return true;
        };
        let grid = map.plane(at.plane);
        units.player_units(viewer).into_iter().any(|u| {
            units.unit_has_ability(u, "illusion_immunity")
                && units.unit_position(u).is_some_and(|p| {
                    p.plane == at.plane
                        && grid.chebyshev((p.x, p.y), (at.x, at.y)) * 2
                            <= units.unit_sight(u) as u32
                })
        })
    }

    /// How far a city sees, in half-squares: 5, or more with an Oracle (9), City Walls (7) or
    /// Nature's Eye (9).
    pub fn city_sight(&self, city: hecs::Entity) -> i32 {
        compute(Stat::CitySight, CITY_SIGHT, &self.city_modifiers(city))
    }

    /// Visible tiles on one plane as a row-major grid of `width * height` flags: the
    /// squares around each of the player's units and cities.
    pub fn visibility(&self, player: PlayerId, plane: Plane) -> Vec<bool> {
        let Some(map) = self.map else {
            return Vec::new();
        };
        let (w, h) = (map.width() as i32, map.height() as i32);
        if self.units().has_global(player, "nature_awareness") {
            return vec![true; (w * h) as usize];
        }
        let mut seen = vec![false; (w * h) as usize];
        let mut reveal = |at: Position, sight: i32| {
            if at.plane != plane {
                return;
            }
            let radius = crate::map::fog::squares_seen(sight.max(0) as u32) as i32;
            for dy in -radius..=radius {
                let y = at.y as i32 + dy;
                if !(0..h).contains(&y) {
                    continue;
                }
                for dx in (-radius..=radius)
                    .filter(|&dx| crate::map::fog::in_sight(dx, dy, sight, self.diagonal_extra))
                {
                    let x = (at.x as i32 + dx).rem_euclid(w);
                    seen[(y * w + x) as usize] = true;
                }
            }
        };
        let units = self.units();
        for unit in units.player_units(player) {
            // The sight is the dear part (it adds up the unit's items and spells): ask only for
            // the units on this plane.
            if let Some(at) = units.unit_position(unit).filter(|at| at.plane == plane) {
                reveal(at, units.unit_sight(unit));
            }
        }
        for (entity, city, at) in self.cities() {
            if city.owner == player && at.plane == plane {
                reveal(at, self.city_sight(entity));
            }
        }
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::{num, shipped};
    use crate::game::Game;
    use crate::map::{Grid, Map};
    use crate::testdir::TestResult;

    #[test]
    fn units_and_cities_see_around_them() -> TestResult {
        let c = shipped();
        let grass = c
            .terrains
            .id_numbered(num::terrain::GRASSLAND)
            .ok_or("grassland is not in the content")?;
        let map = Map::new(Grid::new(10, 8, grass), Grid::new(10, 8, grass));
        let mut g = Game::new(1);
        for id in (0..3).map(PlayerId) {
            g.add_player(id, "");
        }
        g.content = c;
        g.map = Some(map);
        let walker = g
            .content
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?;
        let p = PlayerId(0);
        g.spawn_unit(
            p,
            Position {
                plane: Plane::Arcanus,
                x: 0,
                y: 0,
            },
            walker,
        );
        let seen = g.economy().visibility(p, Plane::Arcanus);
        // Sight 1 at the corner: columns 9, 0, 1 and rows 0, 1.
        assert_eq!(seen.iter().filter(|&&s| s).count(), 6);
        assert!(g.economy().visibility(p, Plane::Myrror).iter().all(|&s| !s));
        assert!(g
            .economy()
            .visibility(PlayerId(1), Plane::Arcanus)
            .iter()
            .all(|&s| !s));
        Ok(())
    }

    #[test]
    fn oracle_walls_and_natures_eye_widen_a_citys_sight() -> TestResult {
        use crate::rules::{ModOp, Modifier, Source};
        let c = crate::game::testutil::shipped();
        let grass = c
            .terrains
            .id_numbered(num::terrain::GRASSLAND)
            .ok_or("grassland is not in the content")?;
        let map = Map::new(Grid::new(30, 20, grass), Grid::new(30, 20, grass));
        let mut g = Game::new(1);
        for id in (0..3).map(PlayerId) {
            g.add_player(id, "");
        }
        g.content = c;
        g.map = Some(map);
        let at = Position {
            plane: Plane::Arcanus,
            x: 10,
            y: 10,
        };
        g.apply(crate::game::Command::FoundCity {
            owner: PlayerId(0),
            pos: at,
            name: "A".into(),
            race: None,
            population: 3,
        });
        let city = g.economy().city_at(at).ok_or("city_at is missing")?;
        assert_eq!(g.economy().city_sight(city), 5);
        let seen = |g: &Game| {
            g.economy()
                .visibility(PlayerId(0), Plane::Arcanus)
                .iter()
                .filter(|&&v| v)
                .count()
        };
        // A 5 by 5 block without its four corners when a diagonal step costs extra, else whole.
        g.settings.basic.diagonal_extra_cost = true;
        assert_eq!(seen(&g), 21);
        g.settings.basic.diagonal_extra_cost = false;
        assert_eq!(seen(&g), 25);
        g.settings.basic.diagonal_extra_cost = true;
        // The sources are Min modifiers, so the best one counts and they do not add up.
        let mods = [
            Modifier::new(Stat::CitySight, ModOp::Min, 7, Source::Other),
            Modifier::new(Stat::CitySight, ModOp::Min, 9, Source::Other),
        ];
        assert_eq!(compute(Stat::CitySight, 5, &mods), 9);
        assert_eq!(compute(Stat::CitySight, 5, &mods[..1]), 7);
        // A real building with the modifier widens what the city sees and explores.
        let id = g
            .content
            .rules
            .buildings
            .id_of("oracle")
            .ok_or("oracle is not in the content")?;
        g.world
            .get::<&mut crate::city::economy::Buildings>(city)?
            .0
            .push(id);
        assert_eq!(g.economy().city_sight(city), 9);
        assert_eq!(seen(&g), 61);
        g.process_globals();
        assert!(g.fog.is_explored(PlayerId(0), Plane::Arcanus, 14, 10));
        assert!(!g.fog.is_explored(PlayerId(0), Plane::Arcanus, 15, 10));
        Ok(())
    }

    #[test]
    fn invisible_units_hide_from_everyone_but_those_who_see_through_illusions() -> TestResult {
        let mut c = shipped();
        let mut units = c.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            match u.id.as_str() {
                "magic_spirit" => u.abilities.push("invisibility".into()),
                "wind_mage" => u.abilities.push("illusion_immunity".into()),
                _ => {}
            }
        }
        c.rules.units = crate::rules::Registry::new_sorted(units);
        let grass = c
            .terrains
            .id_numbered(num::terrain::GRASSLAND)
            .ok_or("grassland is not in the content")?;
        let mut g = Game::new(1);
        for id in (0..3).map(PlayerId) {
            g.add_player(id, "");
        }
        g.content = c;
        g.map = Some(Map::new(Grid::new(20, 10, grass), Grid::new(20, 10, grass)));
        let at = |x| Position {
            plane: Plane::Arcanus,
            x,
            y: 5,
        };
        let id = |g: &Game, n: &str| g.content.rules.units.id_of(n).expect("id_of succeeds");
        let (spirit, mage) = (id(&g, "magic_spirit"), id(&g, "wind_mage"));
        let ghost = g.spawn_unit(PlayerId(0), at(5), spirit);
        assert!(
            g.economy().unit_visible_to(ghost, PlayerId(0)),
            "its owner sees it"
        );
        assert!(!g.economy().unit_visible_to(ghost, PlayerId(1)));
        let _far = g.spawn_unit(PlayerId(1), at(15), mage);
        assert!(
            !g.economy().unit_visible_to(ghost, PlayerId(1)),
            "too far away"
        );
        let _near = g.spawn_unit(PlayerId(1), at(6), mage);
        assert!(
            g.economy().unit_visible_to(ghost, PlayerId(1)),
            "an illusion-immune unit is close"
        );
        Ok(())
    }

    #[test]
    fn the_handle_sees_without_a_game() -> TestResult {
        let c = shipped();
        let grass = c
            .terrains
            .id_numbered(num::terrain::GRASSLAND)
            .ok_or("grassland is not in the content")?;
        let map = Map::new(Grid::new(10, 8, grass), Grid::new(10, 8, grass));
        let walker = c
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?;
        let mut world = hecs::World::new();
        let owner = world.spawn(crate::player::bundle(PlayerId(0)));
        crate::unit::spawn(
            &mut world,
            &c,
            crate::game::serial::Serial(1),
            owner,
            Position {
                plane: Plane::Arcanus,
                x: 5,
                y: 4,
            },
            walker,
        );
        let economy = Economy::new(&world, Some(&map), &c);
        let seen = economy.visibility(PlayerId(0), Plane::Arcanus);
        assert_eq!(seen.iter().filter(|&&s| s).count(), 9);
        assert!(economy
            .visibility(PlayerId(1), Plane::Arcanus)
            .iter()
            .all(|&s| !s));
        Ok(())
    }
}
