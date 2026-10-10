//! What a player can see right now (as opposed to merely having explored).

use crate::city::{PlayerId, Position};
use crate::map::Plane;
use crate::rules::Stat;
use crate::Game;

/// Sight radius of a city; walls and an oracle would extend it.
const CITY_SIGHT: i32 = 2;

impl Game {
    /// Visible tiles on one plane as a row-major grid of `width * height` flags: the
    /// squares around each of the player's units and cities.
    pub fn visibility(&self, player: PlayerId, plane: Plane) -> Vec<bool> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let (w, h) = (map.width() as i32, map.height() as i32);
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
        for (_, city, at) in self.cities() {
            if city.owner == player {
                reveal(at, CITY_SIGHT);
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
}
