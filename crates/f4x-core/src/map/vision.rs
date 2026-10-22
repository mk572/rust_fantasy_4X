//! What a player can see right now (as opposed to merely having explored), worked out by the
//! `Economy` handle (world, map, content, players) without a `Game` (task 193).

use crate::city::economy::Economy;
use crate::map::{Plane, Position};
use crate::player::PlayerId;

use crate::rules::stats::compute;
use crate::rules::Stat;

/// Sight radius of a city before walls, an Oracle or Nature's Eye raise it.
const CITY_SIGHT: i32 = 2;

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
                        && grid.chebyshev((p.x, p.y), (at.x, at.y)) <= units.unit_sight(u) as u32
                })
        })
    }

    /// How far a city sees: 2, or more with an Oracle (4), City Walls (3) or Nature's Eye (4).
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
        let mut reveal = |at: Position, radius: i32| {
            if at.plane != plane {
                return;
            }
            for dy in -radius..=radius {
                let y = at.y as i32 + dy;
                if !(0..h).contains(&y) {
                    continue;
                }
                for dx in (-radius..=radius).filter(|&dx| crate::map::fog::in_sight(dx, dy, radius))
                {
                    let x = (at.x as i32 + dx).rem_euclid(w);
                    seen[(y * w + x) as usize] = true;
                }
            }
        };
        let units = self.units();
        for unit in units.player_units(player) {
            if let Some(at) = units.unit_position(unit) {
                reveal(at, units.unit_sight(unit));
            }
        }
        for (entity, city, at) in self.cities() {
            if city.owner == player {
                reveal(at, self.city_sight(entity));
            }
        }
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_units;
    use crate::game::Game;
    use crate::map::{Grid, Map};

    #[test]
    fn units_and_cities_see_around_them() {
        let c = content_with_units();
        let grass = c.terrains.id_of("grassland").unwrap();
        let map = Map::new(Grid::new(10, 8, grass), Grid::new(10, 8, grass));
        let mut g = Game::new(1);
        g.content = c;
        g.map = Some(map);
        let walker = g.content.rules.units.id_of("walker").unwrap();
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
    }

    #[test]
    fn oracle_walls_and_natures_eye_widen_a_citys_sight() {
        use crate::rules::{ModOp, Modifier, Source};
        let c = crate::game::testutil::content_with_economy();
        let grass = c.terrains.id_of("grassland").unwrap();
        let map = Map::new(Grid::new(30, 20, grass), Grid::new(30, 20, grass));
        let mut g = Game::new(1);
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
        let city = g.economy().city_at(at).unwrap();
        assert_eq!(g.economy().city_sight(city), 2);
        let seen = |g: &Game| {
            g.economy()
                .visibility(PlayerId(0), Plane::Arcanus)
                .iter()
                .filter(|&&v| v)
                .count()
        };
        // A 5 by 5 block without its four corners.
        assert_eq!(seen(&g), 21);
        // The sources are Min modifiers, so the best one counts and they do not add up.
        let mods = [
            Modifier::new(Stat::CitySight, ModOp::Min, 3, Source::Other),
            Modifier::new(Stat::CitySight, ModOp::Min, 4, Source::Other),
        ];
        assert_eq!(compute(Stat::CitySight, 2, &mods), 4);
        assert_eq!(compute(Stat::CitySight, 2, &mods[..1]), 3);
        // A real building with the modifier widens what the city sees and explores.
        let mut oracle = g.content.rules.buildings.defs()[0].clone();
        oracle.id = "oracle".into();
        oracle.effects = vec![crate::rules::effects::EffectRule {
            when: crate::rules::effects::Trigger::Passive,
            condition: crate::rules::effects::Condition::Always,
            target: crate::rules::effects::TargetSel::This,
            effect: crate::rules::effects::Effect::Modify {
                stat: Stat::CitySight,
                op: ModOp::Min,
                value: 4,
            },
        }];
        let mut defs = g.content.rules.buildings.defs().to_vec();
        defs.push(oracle);
        let id = crate::rules::BuildingId::new((defs.len() - 1) as u16);
        g.content.rules.buildings = crate::rules::Registry::new(defs);
        g.world
            .get::<&mut crate::city::economy::Buildings>(city)
            .unwrap()
            .0
            .push(id);
        assert_eq!(g.economy().city_sight(city), 4);
        assert_eq!(seen(&g), 61);
        g.process_globals();
        assert!(g.fog.is_explored(PlayerId(0), Plane::Arcanus, 14, 10));
        assert!(!g.fog.is_explored(PlayerId(0), Plane::Arcanus, 15, 10));
    }

    #[test]
    fn invisible_units_hide_from_everyone_but_those_who_see_through_illusions() {
        let mut c = content_with_units();
        let mut units = c.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            match u.id.as_str() {
                "spirit" => u.abilities.push("invisibility".into()),
                "mage" => u.abilities.push("illusion_immunity".into()),
                _ => {}
            }
        }
        c.rules.units = crate::rules::Registry::new(units);
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(1);
        g.content = c;
        g.map = Some(Map::new(Grid::new(20, 10, grass), Grid::new(20, 10, grass)));
        let at = |x| Position {
            plane: Plane::Arcanus,
            x,
            y: 5,
        };
        let id = |g: &Game, n: &str| g.content.rules.units.id_of(n).unwrap();
        let (spirit, mage) = (id(&g, "spirit"), id(&g, "mage"));
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
    }

    #[test]
    fn the_handle_sees_without_a_game() {
        let c = content_with_units();
        let grass = c.terrains.id_of("grassland").unwrap();
        let map = Map::new(Grid::new(10, 8, grass), Grid::new(10, 8, grass));
        let walker = c.rules.units.id_of("walker").unwrap();
        let mut world = hecs::World::new();
        crate::unit::spawn(
            &mut world,
            &c,
            &[],
            crate::game::serial::Serial(1),
            PlayerId(0),
            Position {
                plane: Plane::Arcanus,
                x: 5,
                y: 4,
            },
            walker,
        );
        let economy = Economy::new(&world, Some(&map), &c, &[]);
        let seen = economy.visibility(PlayerId(0), Plane::Arcanus);
        assert_eq!(seen.iter().filter(|&&s| s).count(), 9);
        assert!(economy
            .visibility(PlayerId(1), Plane::Arcanus)
            .iter()
            .all(|&s| !s));
    }
}
