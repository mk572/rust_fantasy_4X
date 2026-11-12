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
    /// The game's difficulty: 0 intro, 1 easy, 2 normal, 3 hard, 4 extreme, 5 impossible.
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

    /// A player that takes part in diplomacy: it can meet, talk, make treaties and declare war.
    /// The wizards and the neutral player of each race (task 522); not the monsters. Every
    /// diplomacy rule asks this and not [`Game::is_wizard`] (the spell parts of diplomacy keep
    /// asking for the wizard itself).
    pub fn takes_part_in_diplomacy(&self, id: PlayerId) -> bool {
        self.is_wizard(id) || self.race_of_player(id).is_some()
    }

    /// Another player's cities and units that `viewer` sees now: on squares it has in sight,
    /// the units only when it can make them out (an invisible one is hidden).
    pub fn foreign_sightings(&self, viewer: PlayerId) -> Vec<Sighting> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let width = map.width() as usize;
        let seen: Vec<Vec<bool>> = crate::map::Plane::all()
            .map(|plane| self.economy().visibility(viewer, plane))
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

    /// The owners of [`Game::foreign_sightings_near`], once each, in player order: what a move
    /// asks for to see whom the mover meets. The same answer without building the list of
    /// sightings, and the test of sight only for an owner not yet found.
    pub fn foreign_owners_near(
        &self,
        viewer: PlayerId,
        at: Position,
        radius: u32,
    ) -> Vec<PlayerId> {
        let Some(map) = self.map.as_ref() else {
            return Vec::new();
        };
        let grid = map.plane(at.plane);
        let near = |p: &Position| {
            p.plane == at.plane && grid.chebyshev((p.x, p.y), (at.x, at.y)) <= radius
        };
        let mut met: Vec<PlayerId> = Vec::new();
        let mut owner_ids: Vec<(hecs::Entity, Option<PlayerId>)> = Vec::new();
        let mut id_of = |owner: hecs::Entity| -> Option<PlayerId> {
            match owner_ids.iter().find(|(e, _)| *e == owner) {
                Some((_, id)) => *id,
                None => {
                    let id = crate::player::id_of(&self.world, owner);
                    owner_ids.push((owner, id));
                    id
                }
            }
        };
        for (_, (_, pos, owner)) in self
            .world
            .query::<(&crate::city::City, &Position, &crate::player::Owner)>()
            .iter()
        {
            if !near(pos) {
                continue;
            }
            if let Some(id) = id_of(owner.0).filter(|&id| id != viewer && !met.contains(&id)) {
                met.push(id);
            }
        }
        for (unit, (_, pos, owner)) in self
            .world
            .query::<(&crate::unit::Unit, &Position, &crate::player::Owner)>()
            .iter()
        {
            if !near(pos) {
                continue;
            }
            let Some(id) = id_of(owner.0) else { continue };
            if id != viewer && !met.contains(&id) && self.economy().unit_visible_to(unit, viewer) {
                met.push(id);
            }
        }
        met.sort();
        met
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
        // The units in the area first, in a stable order, then the (dearer) test of sight.
        let mut near: Vec<(hecs::Entity, PlayerId, Position)> = self
            .units()
            .unsorted_units()
            .into_iter()
            .filter_map(|unit| {
                let owner = self.units().unit_owner(unit)?;
                let pos = self.units().unit_position(unit)?;
                (owner != viewer && keep(pos)).then_some((unit, owner, pos))
            })
            .collect();
        near.sort_by_key(|&(unit, ..)| self.serial_of(unit));
        for (unit, owner, pos) in near {
            if self.economy().unit_visible_to(unit, viewer) {
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
    use f4x_def::testdir::TestResult;

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
        // Until the neutral players of task 522, diplomacy is between wizards.
        assert!(g.takes_part_in_diplomacy(P0) && !g.takes_part_in_diplomacy(PlayerId(7)));
        assert!(!g.takes_part_in_diplomacy(crate::player::MONSTERS));
        let (spell, id) = g
            .content
            .rules
            .spells
            .iter()
            .map(|(id, _)| (id, id))
            .next()
            .ok_or("next is missing")?;
        assert!(!g.has_global(P0, id));
        let wizard = g.wizard(P0).ok_or("wizard is missing")?.entity();
        g.spawn_spell(spell, wizard, SpellTarget::Global);
        assert!(g.has_global(P0, id));
        assert!(!g.has_global(P1, id));
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
            .id_numbered(107)
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
        // The owners alone, as the moves ask for them, are those of the sightings, once each.
        for radius in [0, 2, 5, 20] {
            let mut owners: Vec<PlayerId> = g
                .foreign_sightings_near(P0, at(10, 10), radius)
                .into_iter()
                .map(|s| s.owner)
                .collect();
            owners.sort();
            owners.dedup();
            assert_eq!(g.foreign_owners_near(P0, at(10, 10), radius), owners);
        }
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
