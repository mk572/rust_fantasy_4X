//! What the domains (diplomacy, magic and the rest) ask of the game: who is in it, what a wizard
//! can see, how much an army costs. The questions answer in plain data, so a domain takes a
//! `&Game` that it only reads and never an entity or a component of its own. Tests of a domain
//! build a real game (`game/testutil.rs`) and run on it.

use crate::map::Position;
use crate::player::PlayerId;

use crate::game::Game;

/// Something of another player that a wizard can see: one of its cities, or a unit the wizard
/// can make out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sighting {
    pub owner: PlayerId,
    pub pos: Position,
}

/// A unit as the domains need to know it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitInfo {
    pub pos: Position,
    /// What the unit cost to make (the original's measure of its strength).
    pub cost: u32,
}

impl Game {
    /// The game's difficulty: 0 intro, 1 easy, 2 normal, 3 hard, 4 impossible.
    pub fn difficulty(&self) -> u8 {
        self.settings.basic.difficulty
    }

    /// A wizard's temper (`Chaotic` for a player without one).
    pub fn personality(&self, id: PlayerId) -> crate::wizard::objective::Personality {
        self.wizard(id).map(|w| w.personality()).unwrap_or_default()
    }

    /// A player that is a wizard: neither the monsters nor an independent city.
    pub fn is_wizard(&self, id: PlayerId) -> bool {
        self.has_player(id) && !self.is_neutral_player(id)
    }

    /// Another player's cities and units that `viewer` sees now: on squares it has in sight,
    /// the units only when it can make them out (an invisible one is hidden).
    pub fn foreign_sightings(&self, viewer: PlayerId) -> Vec<Sighting> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let width = map.width() as usize;
        let seen: Vec<Vec<bool>> = crate::map::Plane::ALL
            .iter()
            .map(|&plane| self.economy().visibility(viewer, plane))
            .collect();
        let sees = |p: Position| {
            seen.get(p.plane.index())
                .and_then(|grid| grid.get(p.y as usize * width + p.x as usize))
                .copied()
                .unwrap_or(false)
        };
        self.sightings_where(viewer, sees)
    }

    /// Another player's cities and units within `radius` squares of `at` (the same plane),
    /// the units only when `viewer` can make them out. The squares need not be in sight.
    pub fn foreign_sightings_near(
        &self,
        viewer: PlayerId,
        at: Position,
        radius: u32,
    ) -> Vec<Sighting> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let grid = map.plane(at.plane);
        self.sightings_where(viewer, |p| {
            p.plane == at.plane && grid.chebyshev((p.x, p.y), (at.x, at.y)) <= radius
        })
    }

    /// The units a player has.
    pub fn units_of(&self, player: PlayerId) -> Vec<UnitInfo> {
        self.units()
            .player_units(player)
            .into_iter()
            .filter_map(|u| {
                Some(UnitInfo {
                    pos: self.units().unit_position(u)?,
                    cost: self.units().unit_def(u)?.cost,
                })
            })
            .collect()
    }

    /// How many cities (outposts too) a player has.
    pub fn city_count(&self, player: PlayerId) -> usize {
        self.economy()
            .cities()
            .iter()
            .filter(|(_, c, _)| c.owner == player)
            .count()
    }

    /// The cities and visible units of other players on the squares `keep` accepts.
    fn sightings_where(&self, viewer: PlayerId, keep: impl Fn(Position) -> bool) -> Vec<Sighting> {
        let mut out = Vec::new();
        for (_, city, pos) in self.economy().cities() {
            if city.owner != viewer && keep(pos) {
                out.push(Sighting {
                    owner: city.owner,
                    pos,
                });
            }
        }
        for unit in self.units().all_units() {
            let (Some(owner), Some(pos)) = (
                self.units().unit_owner(unit),
                self.units().unit_position(unit),
            ) else {
                continue;
            };
            if owner != viewer && keep(pos) && self.economy().unit_visible_to(unit, viewer) {
                out.push(Sighting { owner, pos });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplomacy::army_strength;
    use crate::game::testutil::{num, shipped};
    use crate::map::Plane;
    use crate::spell::instance::SpellTarget;
    use crate::testdir::TestResult;

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);
    const P2: PlayerId = PlayerId(2);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn game() -> Game {
        let mut g = Game::new(3);
        g.content = shipped();
        let grass = g
            .content
            .terrains
            .id_numbered(num::terrain::GRASSLAND)
            .expect("id_of succeeds");
        g.map = Some(crate::map::Map::new(
            crate::map::Grid::new(30, 20, grass),
            crate::map::Grid::new(30, 20, grass),
        ));
        for id in [P0, P1, P2] {
            g.add_player(id, "W");
        }
        g
    }

    #[test]
    fn the_provided_questions_follow_from_the_players() -> TestResult {
        let mut g = game();
        g.add_neutral_player(PlayerId(7));
        let defeated = g.player_entity(P2).ok_or("player_entity is missing")?;
        g.world
            .insert_one(defeated, crate::player::Defeat { by: None })?;
        assert!(g.is_wizard(P0) && !g.is_wizard(PlayerId(7)));
        assert!(!g.is_wizard(crate::player::MONSTERS) && !g.is_wizard(PlayerId(9)));
        assert_eq!(g.living_wizards(), vec![P0, P1]);
        let (spell, id) = g
            .content
            .rules
            .spells
            .iter()
            .map(|(id, d)| (id, d.id.clone()))
            .next()
            .ok_or("next is missing")?;
        assert!(!g.has_global(P0, &id));
        let wizard = g.wizard(P0).ok_or("wizard is missing")?.entity();
        g.spawn_spell(spell, wizard, SpellTarget::Global);
        assert!(g.has_global(P0, &id));
        assert!(!g.has_global(P1, &id));
        Ok(())
    }

    #[test]
    fn the_game_answers_what_a_wizard_sees_and_has() -> TestResult {
        let mut g = Game::new(3);
        g.content = shipped();
        let grass = g
            .content
            .terrains
            .id_numbered(num::terrain::GRASSLAND)
            .ok_or("grassland is not in the content")?;
        g.map = Some(crate::map::Map::new(
            crate::map::Grid::new(30, 20, grass),
            crate::map::Grid::new(30, 20, grass),
        ));
        for id in [P0, P1, P2] {
            g.add_player(id, "W");
        }
        let walker = g
            .content
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?;
        g.spawn_unit(P1, at(11, 10), walker);
        g.spawn_unit(P2, at(25, 15), walker);
        let mine = g.spawn_unit(P0, at(10, 10), walker);
        let cost = g.units().unit_def(mine).ok_or("unit_def is missing")?.cost;
        // Within two squares of (10, 10), whether or not it is in sight.
        let around = g.foreign_sightings_near(P0, at(10, 10), 2);
        assert_eq!(
            around,
            vec![Sighting {
                owner: P1,
                pos: at(11, 10)
            }]
        );
        // What P0 sees now: its units see around them, so the near one and not the far one.
        let seen = g.foreign_sightings(P0);
        assert_eq!(
            seen,
            vec![Sighting {
                owner: P1,
                pos: at(11, 10)
            }],
            "{seen:?}"
        );
        assert_eq!(
            g.units_of(P0),
            vec![UnitInfo {
                pos: at(10, 10),
                cost
            }]
        );
        assert_eq!(army_strength(&g, P0), cost as i32);
        // Another viewpoint: P1 sees its own side's neighbours, not itself.
        assert!(g.foreign_sightings(P1).iter().all(|s| s.owner != P1));
        Ok(())
    }
}
