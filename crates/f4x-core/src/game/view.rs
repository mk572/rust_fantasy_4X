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
    /// What the unit cost to make.
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
        near.sort_by_key(|&(unit, ..)| self.units().serial_of(unit));
        for (unit, owner, pos) in near {
            if self.economy().unit_visible_to(unit, viewer) {
                out.push(Sighting { owner, pos });
            }
        }
        out
    }
}
