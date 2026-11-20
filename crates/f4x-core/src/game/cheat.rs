//! Override mutators: things the rules never allow a player to do. The console's override
//! commands (`docs/console.md`) and the debug screens call these; every console use is written to
//! the game's override log, which is saved, so a game that was tampered with can be told apart.
//!
//! Tests: below.

use crate::map::{Plane, Position, TerrainId};
use crate::player::PlayerId;

use crate::city::economy::Buildings;
use crate::game::{Event, Game};
use crate::rules::{BuildingId, SpellId};

const MONSTERS: PlayerId = PlayerId(u8::MAX);

impl Game {
    /// Note that an override command changed the game.
    pub fn record_override(&mut self, line: &str) {
        self.override_log
            .push(format!("turn {}: {line}", self.turn));
    }

    /// Whether the game ever used the console's cheat mode (switching it on is logged, and the log
    /// is saved with the game): such a game does not go in the Hall of Fame.
    pub fn cheated(&self) -> bool {
        !self.override_log.is_empty()
    }

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

    /// Put a lasting spell on a unit at no cost, as if `caster` had cast it.
    pub fn cheat_enchant_unit(
        &mut self,
        unit: hecs::Entity,
        spell: SpellId,
        caster: PlayerId,
    ) -> bool {
        if !self.world.contains(unit) {
            return false;
        }
        self.attach_enchantment(crate::rules::exec::Target::Unit(unit), spell, caster);
        true
    }

    /// Set a city's citizens (the whole number; the fraction is kept).
    pub fn cheat_set_citizens(&mut self, city: hecs::Entity, citizens: u32) -> bool {
        match self
            .world
            .get::<&mut crate::city::economy::Population>(city)
        {
            Ok(mut p) => {
                p.figures = citizens;
                true
            }
            Err(_) => false,
        }
    }

    /// Heal a unit completely and give it all its moves back.
    pub fn cheat_restore_unit(&mut self, unit: hecs::Entity) {
        let moves = self.units().unit_max_moves(unit);
        let _ = self.world.insert_one(unit, crate::unit::Hurt(0));
        let _ = self.world.insert_one(unit, crate::unit::MovesLeft(moves));
    }

    /// Put a unit on a square, wherever it stands and whatever is there.
    pub fn cheat_move_unit(&mut self, unit: hecs::Entity, to: Position) {
        self.put(unit, to);
    }

    /// Explore every square of both planes for a player, without meeting anyone (for
    /// screenshots and tests).
    pub fn reveal_map(&mut self, player: PlayerId) {
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let (w, h) = (map.width(), map.height());
        for plane in crate::map::Plane::all() {
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
        let Some(mut w) = self.wizard_mut(player) else {
            return false;
        };
        let mut spells = w.get_mut::<crate::wizard::sheet::KnownSpells>();
        spells.researchable.remove(&spell);
        spells.known.insert(spell);
        true
    }

    /// Defeat a wizard now, as if `winner` had beaten them (for tests and the console).
    pub fn cheat_defeat_player(&mut self, loser: PlayerId, winner: PlayerId) -> Vec<Event> {
        self.defeat(loser, winner, crate::wizard::fortress::Fall::Resigned)
    }

    /// Reveal both planes to a player.
    pub fn cheat_reveal(&mut self, player: PlayerId) {
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let (w, h) = (map.width(), map.height());
        for plane in crate::map::Plane::all() {
            let centre = Position {
                plane,
                x: w / 2,
                y: h / 2,
            };
            self.fog.explore_block(player, centre, w, h);
        }
    }

    /// Defeat a wizard on the spot: their cities turn neutral and the usual aftermath
    /// follows, ending the game if that leaves one wizard (or the human is the one).
    pub fn cheat_eliminate(&mut self, player: PlayerId, by: PlayerId) -> Vec<Event> {
        if !self.has_player(player) || self.player(player).is_none_or(|p| p.is_defeated()) {
            return Vec::new();
        }
        for (city, c, _) in self.economy().cities() {
            if c.owner == player {
                self.set_owner(city, MONSTERS);
            }
        }
        self.defeat(player, by, crate::wizard::fortress::Fall::Resigned)
    }

    /// Put an encounter site or a tower on a square with guards and treasure made as a new world
    /// makes them (`feature::lair::create`, with the game's lair settings): `strong` picks the
    /// budget of a strong site over a weak one (a tower has the one budget). Replaces whatever
    /// feature stands there.
    pub fn cheat_add_lair(
        &mut self,
        pos: Position,
        feature: crate::map::FeatureId,
        strong: bool,
    ) -> Result<(), String> {
        use crate::feature::def::FeatureKind;
        use crate::feature::lair::create;
        use rand::{RngCore, SeedableRng};
        let def = self
            .content
            .features
            .get(feature)
            .ok_or("no such feature")?;
        let kind = def.kind;
        if !matches!(kind, FeatureKind::Encounter | FeatureKind::Tower) {
            return Err("only an encounter site or a tower has a lair".into());
        }
        let site = create::site_type(def);
        let on_map = self.map.as_ref().is_some_and(|m| {
            let grid = m.plane(pos.plane);
            pos.x < grid.width() && pos.y < grid.height()
        });
        if !on_map {
            return Err("that square is off the map".into());
        }
        let style = crate::rules::lair::LairStyle {
            stronger: self.settings.rules.stronger_lairs,
            treasure: self.settings.rules.treasure_patch_150,
        };
        let guards = if style.stronger {
            let table = &self.content.rules.lairs.patch_150.guard_percent;
            table
                .get(usize::from(self.settings.basic.difficulty))
                .copied()
                .unwrap_or(100)
        } else {
            i32::from(self.settings.world.lair_guard_percent.min(300))
        };
        let treasure = i32::from(self.settings.world.lair_treasure_percent.min(500));
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(self.rng.next_u64());
        let (plane, budget) = if kind == FeatureKind::Tower {
            let budget = create::tower_budget(&mut rng, &self.content, style);
            (Plane::Arcanus, budget)
        } else {
            let budget = create::site_budget(&mut rng, &self.content, pos.plane, strong, style);
            (pos.plane, budget)
        };
        let lair = create::create(
            &mut rng,
            &self.content,
            plane,
            site,
            budget,
            guards,
            treasure,
            style,
        );
        self.cheat_set_feature(pos, None);
        self.spawn_feature(crate::feature::entity::Placement {
            at: pos,
            feature,
            node: None,
            lair: Some(lair),
        });
        Ok(())
    }

    /// Take the site with a lair off a square, guards, treasure and all. False when there is none.
    pub fn cheat_remove_lair(&mut self, pos: Position) -> bool {
        let Some(site) = self.map.as_ref().and_then(|m| m.feature_entity(pos)) else {
            return false;
        };
        if !self
            .world
            .satisfies::<&crate::feature::lair::Lair>(site)
            .unwrap_or(false)
        {
            return false;
        }
        self.despawn_feature(site);
        true
    }

    /// Hire a hero type for `player` at no cost: it appears on the wizard's fortress, or in a city
    /// of theirs. The hero must be free or dead (a resurrection).
    pub fn cheat_hire_hero(
        &mut self,
        player: PlayerId,
        kind: usize,
    ) -> Result<hecs::Entity, String> {
        let at = self
            .wizard(player)
            .and_then(|w| w.fortress())
            .or_else(|| {
                self.economy()
                    .cities_of(player)
                    .first()
                    .map(|&(_, pos)| pos)
            })
            .ok_or("that wizard has no fortress or city to arrive in")?;
        self.spawn_hero(player, at, kind)
            .map_err(|e| format!("{e:?}"))
    }

    /// Roll a hero type's abilities and spells anew (`hero::kind::roll_one`); its name, level and
    /// items stay. Works for a hero in service and for a free one.
    pub fn cheat_reroll_hero(&mut self, kind: usize) -> Result<(), String> {
        let hero = self.units().hero_entity(kind).ok_or("no such hero")?;
        let name = self
            .world
            .get::<&crate::hero::Hero>(hero)
            .map_err(|_| "no such hero")?
            .name
            .clone();
        let template = self
            .content
            .rules
            .hero_templates
            .get(kind)
            .ok_or("no such hero")?;
        let mut rolled = crate::hero::kind::roll_one(
            &mut self.rng,
            &self.content.rules,
            &self.names,
            kind,
            template,
        );
        rolled.name = name;
        self.put(hero, rolled);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::game::testutil::fresh;
    use crate::game::Game;
    use crate::hero::HeroState;
    use crate::map::{Plane, Position};
    use crate::player::PlayerId;
    use f4x_def::testdir::TestResult;

    const P0: PlayerId = PlayerId(0);

    fn world() -> Result<Game, String> {
        fresh(2, 7)
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
            .id_numbered(107)
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
            .id_numbered(107)
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
            .id_numbered(5 /* hills */)
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
        assert!(g.cheat_set_feature(spot, Some(id(10 /* gold_ore */))));
        let mineral = |g: &Game| {
            g.map
                .as_ref()
                .and_then(|m| m.plane(Plane::Arcanus).mineral(12, 12))
        };
        assert_eq!(mineral(&g), Some(id(10 /* gold_ore */)));
        assert!(g
            .map
            .as_ref()
            .ok_or("no map")?
            .feature_entity(spot)
            .is_none());
        // A tower is an entity, and the mineral under it goes.
        assert!(g.cheat_set_feature(spot, Some(id(21 /* tower */))));
        assert_eq!(mineral(&g), None);
        let tower = g
            .map
            .as_ref()
            .ok_or("no map")?
            .feature_entity(spot)
            .ok_or("the tower has no entity")?;
        assert!(g.world.contains(tower));
        // Another feature takes its place and the old entity is gone; none clears the square.
        assert!(g.cheat_set_feature(spot, Some(id(22 /* wild_game */))));
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
            .id_numbered(83)
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
