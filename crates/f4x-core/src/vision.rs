//! What a player can see right now (as opposed to merely having explored).

use crate::city::{PlayerId, Position};
use crate::map::Plane;
use crate::rules::stats::compute;
use crate::rules::Stat;
use crate::Game;

/// Sight radius of a city before walls, an Oracle or Nature's Eye raise it.
const CITY_SIGHT: i32 = 2;

impl Game {
    /// How far a city sees: 2, or more with an Oracle (4), City Walls (3) or Nature's Eye (4).
    pub fn city_sight(&self, city: hecs::Entity) -> i32 {
        compute(Stat::CitySight, CITY_SIGHT, &self.city_modifiers(city))
    }

    /// Visible tiles on one plane as a row-major grid of `width * height` flags: the
    /// squares around each of the player's units and cities.
    pub fn visibility(&self, player: PlayerId, plane: Plane) -> Vec<bool> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let (w, h) = (map.width() as i32, map.height() as i32);
        if self.has_global(player, "nature_awareness") {
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
                for dx in -radius..=radius {
                    let x = (at.x as i32 + dx).rem_euclid(w);
                    seen[(y * w + x) as usize] = true;
                }
            }
        };
        for unit in self.player_units(player) {
            if let Some(at) = self.unit_position(unit) {
                reveal(at, self.unit_stat(unit, Stat::Sight).max(1));
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
    use crate::map::{Grid, Map};
    use crate::testutil::content_with_units;

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
        let seen = g.visibility(p, Plane::Arcanus);
        // Sight 1 at the corner: columns 9, 0, 1 and rows 0, 1.
        assert_eq!(seen.iter().filter(|&&s| s).count(), 6);
        assert!(g.visibility(p, Plane::Myrror).iter().all(|&s| !s));
        assert!(g
            .visibility(PlayerId(1), Plane::Arcanus)
            .iter()
            .all(|&s| !s));
    }

    #[test]
    fn oracle_walls_and_natures_eye_widen_a_citys_sight() {
        use crate::rules::{ModOp, Modifier, Source};
        let c = crate::testutil::content_with_economy();
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
        g.apply(crate::Command::FoundCity {
            owner: PlayerId(0),
            pos: at,
            name: "A".into(),
            race: None,
            population: 3,
        });
        let city = g.city_at(at).unwrap();
        assert_eq!(g.city_sight(city), 2);
        let seen = |g: &Game| {
            g.visibility(PlayerId(0), Plane::Arcanus)
                .iter()
                .filter(|&&v| v)
                .count()
        };
        assert_eq!(seen(&g), 25);
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
            .get::<&mut crate::economy::Buildings>(city)
            .unwrap()
            .0
            .push(id);
        assert_eq!(g.city_sight(city), 4);
        assert_eq!(seen(&g), 81);
        g.process_globals();
        assert!(g.fog.is_explored(PlayerId(0), Plane::Arcanus, 14, 10));
        assert!(!g.fog.is_explored(PlayerId(0), Plane::Arcanus, 15, 10));
    }
}
