//! Override mutators: things the rules never allow a player to do, for debug screens and tests.
//! The console's override commands that used them are out for now (`reference/console-cheat/`).

use crate::map::{Position, TerrainId};
use crate::player::PlayerId;

use crate::game::Game;
use crate::rules::SpellId;

impl Game {
    /// Override commands that changed this game, oldest first.
    pub fn override_log(&self) -> &[String] {
        &self.override_log
    }

    /// Remove a unit from the game.
    pub fn cheat_remove_unit(&mut self, unit: hecs::Entity) -> bool {
        let there = self.world.contains(unit);
        self.remove_unit(unit, crate::hero::roster::Fate::Released);
        there
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
                crate::map::fog::sight_for(w.max(h)),
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

    /// Teach a wizard a spell.
    pub fn cheat_learn_spell(&mut self, player: PlayerId, spell: SpellId) -> bool {
        let Some(mut w) = self.wizard_mut(player) else {
            return false;
        };
        let mut spells = w.get_mut::<crate::wizard::sheet::KnownSpells>();
        spells.researchable.remove(&spell);
        spells.known.insert(spell);
        true
    }
}
