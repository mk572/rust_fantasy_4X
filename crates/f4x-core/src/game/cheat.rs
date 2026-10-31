//! Override mutators: things the rules never allow a player to do, for debug screens and tests.
//! The console's override commands are out (`reference/console-cheat/`).
//!
//! Tests: below.

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
        self.remove_unit(unit, crate::hero::Fate::Released);
        there
    }

    /// Kill a unit as a battle would: a hero is then dead and can be raised.
    pub fn cheat_slay_unit(&mut self, unit: hecs::Entity) -> bool {
        let there = self.world.contains(unit);
        self.remove_unit(unit, crate::hero::Fate::Slain);
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
                self.settings.basic.diagonal_extra_cost,
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

    /// Put a feature (or none) on a square: a mineral goes in the map layer, a tower, node or
    /// encounter site becomes an entity (with no guards or treasure, and a node with no aura).
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
        grid.set_mineral(pos.x, pos.y, None);
        if let Some(old) = map.feature_entity(pos) {
            self.despawn_feature(old);
        }
        let Some(id) = feature else {
            return true;
        };
        let kind = self.content.features.get(id).map(|d| d.kind);
        if kind == Some(crate::feature::def::FeatureKind::Mineral) {
            if let Some(map) = self.map.as_mut() {
                map.plane_mut(pos.plane).set_mineral(pos.x, pos.y, Some(id));
            }
        } else {
            self.spawn_feature(crate::feature::entity::Placement {
                at: pos,
                feature: id,
                node: None,
                lair: None,
            });
        }
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

#[cfg(test)]
mod tests {
    use crate::game::setup::{new_game, NewGame};
    use crate::game::testutil::{num, shipped};
    use crate::game::Game;
    use crate::hero::HeroState;
    use crate::map::{Plane, Position};
    use crate::player::PlayerId;
    use crate::testdir::TestResult;

    const P0: PlayerId = PlayerId(0);

    fn world() -> Result<Game, String> {
        new_game(
            &shipped(),
            &NewGame {
                seed: 4,
                all_ai: true,
                ..Default::default()
            },
        )
    }

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    #[test]
    fn a_unit_is_removed_or_slain_once_and_a_hero_slain_is_dead_but_released_is_free() -> TestResult
    {
        let mut g = world()?;
        let soldier = g
            .content()
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("no cavalry")?;
        let plain = g.spawn_unit(P0, at(3, 3), soldier);
        assert!(g.cheat_remove_unit(plain));
        assert!(!g.world.contains(plain));
        assert!(!g.cheat_remove_unit(plain), "nothing left to remove");
        let other = g.spawn_unit(P0, at(3, 3), soldier);
        assert!(g.cheat_slay_unit(other));
        assert!(!g.world.contains(other));

        let (dead, freed) = (
            g.spawn_hero(P0, at(4, 4), 0)
                .map_err(|e| format!("{e:?}"))?,
            g.spawn_hero(P0, at(4, 5), 1)
                .map_err(|e| format!("{e:?}"))?,
        );
        assert!(g.cheat_slay_unit(dead));
        assert!(g.cheat_remove_unit(freed));
        assert_eq!(g.units().hero_state(dead), Some(HeroState::Dead));
        assert_eq!(g.units().hero_state(freed), Some(HeroState::Free));
        Ok(())
    }

    #[test]
    fn a_unit_moves_wherever_it_is_told() -> TestResult {
        let mut g = world()?;
        let soldier = g
            .content()
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("no cavalry")?;
        let unit = g.spawn_unit(P0, at(3, 3), soldier);
        g.cheat_move_unit(unit, at(20, 12));
        assert_eq!(g.units().unit_position(unit), Some(at(20, 12)));
        Ok(())
    }

    #[test]
    fn terrain_and_rivers_change_on_the_map_and_not_off_it() -> TestResult {
        let mut g = world()?;
        let (w, h) = g
            .map
            .as_ref()
            .map(|m| (m.width(), m.height()))
            .ok_or("no map")?;
        let hills = g
            .content()
            .terrains
            .id_numbered(num::terrain::HILLS)
            .ok_or("no hills")?;
        let spot = at(10, 10);
        assert!(g.cheat_set_terrain(spot, hills));
        let grid = |g: &Game| g.map.as_ref().map(|m| m.plane(Plane::Arcanus).clone());
        let after = grid(&g).ok_or("no map")?;
        assert_eq!(after.get(10, 10), Some(hills));
        assert!(g.cheat_set_river(spot, true));
        assert!(grid(&g).ok_or("no map")?.has_river(10, 10));
        assert!(g.cheat_set_river(spot, false));
        assert!(!grid(&g).ok_or("no map")?.has_river(10, 10));
        for off in [at(w, 3), at(3, h), at(w + 5, h + 5)] {
            assert!(!g.cheat_set_terrain(off, hills), "{off:?}");
            assert!(!g.cheat_set_river(off, true), "{off:?}");
            assert!(!g.cheat_set_feature(off, None), "{off:?}");
        }
        Ok(())
    }

    #[test]
    fn a_feature_put_on_a_square_replaces_what_was_there() -> TestResult {
        let mut g = world()?;
        let id = |n| crate::map::FeatureId(n);
        let spot = at(12, 12);
        // A mineral goes in the map layer.
        assert!(g.cheat_set_feature(spot, Some(id(num::feature::GOLD_ORE))));
        let mineral = |g: &Game| {
            g.map
                .as_ref()
                .and_then(|m| m.plane(Plane::Arcanus).mineral(12, 12))
        };
        assert_eq!(mineral(&g), Some(id(num::feature::GOLD_ORE)));
        assert!(g
            .map
            .as_ref()
            .ok_or("no map")?
            .feature_entity(spot)
            .is_none());
        // A tower is an entity, and the mineral under it goes.
        assert!(g.cheat_set_feature(spot, Some(id(num::feature::TOWER))));
        assert_eq!(mineral(&g), None);
        let tower = g
            .map
            .as_ref()
            .ok_or("no map")?
            .feature_entity(spot)
            .ok_or("the tower has no entity")?;
        assert!(g.world.contains(tower));
        // Another feature takes its place and the old entity is gone; none clears the square.
        assert!(g.cheat_set_feature(spot, Some(id(num::feature::WILD_GAME))));
        assert!(!g.world.contains(tower));
        assert!(g.cheat_set_feature(spot, None));
        assert!(g
            .map
            .as_ref()
            .ok_or("no map")?
            .feature_entity(spot)
            .is_none());
        assert_eq!(mineral(&g), None);
        Ok(())
    }

    #[test]
    fn a_wizard_is_taught_a_spell_and_a_missing_one_is_refused() -> TestResult {
        let mut g = world()?;
        let spell = g
            .content()
            .rules
            .spells
            .id_of("fire_bolt")
            .ok_or("no fire_bolt")?;
        assert!(g.cheat_learn_spell(P0, spell));
        assert!(g.wizard(P0).ok_or("no wizard")?.knows(spell));
        assert!(!g.cheat_learn_spell(PlayerId(77), spell), "no such wizard");
        Ok(())
    }

    #[test]
    fn revealing_the_map_explores_it_for_one_player_only() -> TestResult {
        let mut g = world()?;
        let (other, p) = (PlayerId(1), P0);
        let seen = |g: &Game, who| g.fog.explored_count(who, Plane::Arcanus);
        let (before, theirs) = (seen(&g, p), seen(&g, other));
        g.reveal_map(p);
        assert!(seen(&g, p) > before);
        assert_eq!(seen(&g, other), theirs);
        let (w, h) = g
            .map
            .as_ref()
            .map(|m| (m.width(), m.height()))
            .ok_or("no map")?;
        assert_eq!(seen(&g, p) as u32, w * h, "every square of the plane");
        assert!(g.override_log().is_empty(), "revealing is not logged");
        Ok(())
    }
}
