//! Override mutators: things the rules never allow a player to do. Only the console's
//! override commands (`docs/console.md`) call these; every use is written to the game's
//! override log, which is saved, so a game that was tampered with can be told apart.

use crate::city::{PlayerId, Position};
use crate::economy::Buildings;
use crate::map::TerrainId;
use crate::rules::{BuildingId, SpellId};
use crate::{Event, Game};

const NEUTRAL: PlayerId = PlayerId(u8::MAX);

impl Game {
    /// Note that an override command changed the game.
    pub fn record_override(&mut self, line: &str) {
        self.override_log
            .push(format!("turn {}: {line}", self.turn));
    }

    /// Override commands that changed this game, oldest first.
    pub fn override_log(&self) -> &[String] {
        &self.override_log
    }

    /// Remove a unit from the game.
    pub fn cheat_remove_unit(&mut self, unit: hecs::Entity) -> bool {
        let there = self.world.contains(unit);
        self.remove_unit(unit, crate::roster::Fate::Released);
        there
    }

    /// Heal a unit completely and give it all its moves back.
    pub fn cheat_restore_unit(&mut self, unit: hecs::Entity) {
        let moves = self.units().unit_max_moves(unit);
        let _ = self.world.insert_one(unit, crate::units::Hurt(0));
        let _ = self.world.insert_one(unit, crate::units::MovesLeft(moves));
    }

    /// Put a unit on a square, wherever it stands and whatever is there.
    pub fn cheat_move_unit(&mut self, unit: hecs::Entity, to: Position) {
        let _ = self.world.insert_one(unit, to);
    }

    /// Explore every square of both planes for a player, without meeting anyone (for
    /// screenshots and tests).
    pub fn reveal_map(&mut self, player: PlayerId) {
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let (w, h) = (map.width(), map.height());
        for plane in crate::map::Plane::ALL {
            self.fog.explore(
                player,
                Position {
                    plane,
                    x: w / 2,
                    y: h / 2,
                },
                w.max(h),
            );
        }
    }

    /// Replace a square's terrain.
    pub fn cheat_set_terrain(&mut self, pos: Position, terrain: TerrainId) -> bool {
        let Some(map) = self.map.as_mut() else {
            return false;
        };
        let grid = map.plane_mut(pos.plane);
        if pos.x >= grid.width() || pos.y >= grid.height() {
            return false;
        }
        grid.set(pos.x, pos.y, terrain);
        true
    }

    /// Put a river on a square, or take it off.
    pub fn cheat_set_river(&mut self, pos: Position, river: bool) -> bool {
        let Some(map) = self.map.as_mut() else {
            return false;
        };
        let grid = map.plane_mut(pos.plane);
        if pos.x >= grid.width() || pos.y >= grid.height() {
            return false;
        }
        grid.set_river(pos.x, pos.y, river);
        true
    }

    /// Put a feature (or none) on a square.
    pub fn cheat_set_feature(
        &mut self,
        pos: Position,
        feature: Option<crate::map::FeatureId>,
    ) -> bool {
        let Some(map) = self.map.as_mut() else {
            return false;
        };
        let grid = map.plane_mut(pos.plane);
        if pos.x >= grid.width() || pos.y >= grid.height() {
            return false;
        }
        grid.set_feature(pos.x, pos.y, feature);
        true
    }

    /// Give a city a building at no cost and without its requirements.
    pub fn cheat_add_building(&mut self, city: hecs::Entity, building: BuildingId) -> bool {
        if self.economy().city_owner(city).is_none() {
            return false;
        }
        let mut list = self
            .world
            .get::<&Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        if !list.contains(&building) {
            list.push(building);
        }
        self.world.insert_one(city, Buildings(list)).is_ok()
    }

    /// Teach a wizard a spell.
    pub fn cheat_learn_spell(&mut self, player: PlayerId, spell: SpellId) -> bool {
        let Some(m) = self.magic_mut(player) else {
            return false;
        };
        m.researchable.remove(&spell);
        m.known.insert(spell);
        true
    }

    /// Reveal both planes to a player.
    pub fn cheat_reveal(&mut self, player: PlayerId) {
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let (w, h) = (map.width(), map.height());
        for plane in crate::map::Plane::ALL {
            self.fog.explore(
                player,
                Position {
                    plane,
                    x: w / 2,
                    y: h / 2,
                },
                w.max(h),
            );
        }
    }

    /// Defeat a wizard on the spot: their cities turn neutral and the usual aftermath
    /// follows, ending the game if that leaves one wizard (or the human is the one).
    pub fn cheat_eliminate(&mut self, player: PlayerId, by: PlayerId) -> Vec<Event> {
        if self.player(player).is_none_or(|p| p.defeated) {
            return Vec::new();
        }
        for (city, c, _) in self.economy().cities() {
            if c.owner == player {
                if let Ok(mut c) = self.world.get::<&mut crate::city::City>(city) {
                    c.owner = NEUTRAL;
                }
            }
        }
        self.defeat(player, by)
    }
}
