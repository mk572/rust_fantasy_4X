//! Override mutators: things the rules never allow a player to do. The console's override
//! commands (`docs/console.md`) and the debug screens call these; every console use is written to
//! the game's override log, which is saved, so a game that was tampered with can be told apart.
//!
//! Tests: `tests/it/c9_world.rs`.

use crate::map::{Plane, Position, TerrainId};
use crate::player::PlayerId;
use crate::world::rng::GameRng;

use crate::city::economy::Buildings;
use crate::rules::{BuildingId, RealmId, SpellId};
use crate::wizard::magic::Retort;
use crate::world::{Event, World};
use std::collections::{BTreeMap, BTreeSet};

use crate::player::MONSTERS;

impl World {
    /// Note that an override command changed the game. The caller, which knows the round, says in
    /// the line when it was.
    pub fn record_override(&mut self, line: &str) {
        self.override_log.push(line.to_string());
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
        let there = self.ecs.contains(unit);
        self.remove_unit(unit, crate::hero::Fate::Released);
        there
    }

    /// Kill a unit as a battle would: a hero is then dead and can be raised.
    pub fn cheat_slay_unit(&mut self, unit: hecs::Entity) -> bool {
        let there = self.ecs.contains(unit);
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
        if !self.ecs.contains(unit) {
            return false;
        }
        self.attach_enchantment(crate::rules::exec::Target::Unit(unit), spell, caster);
        true
    }

    /// Set a city's citizens (the whole number; the fraction is kept).
    pub fn cheat_set_citizens(&mut self, city: hecs::Entity, citizens: u32) -> bool {
        self.change_population(city, |p| p.figures = citizens)
    }

    /// Heal a unit completely and give it all its moves back.
    pub fn cheat_restore_unit(&mut self, unit: hecs::Entity) {
        let moves = self.units().unit_max_moves(unit);
        let _ = self.ecs.insert_one(unit, crate::unit::Hurt(0));
        let _ = self.ecs.insert_one(unit, crate::unit::MovesLeft(moves));
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

    /// Whether the terrain of a square allows a feature (`FeatureDef::stands_on`); a tower needs
    /// it on both planes. The error says what is in the way.
    pub fn cheat_feature_fits(
        &self,
        pos: Position,
        feature: crate::map::FeatureId,
    ) -> Result<(), String> {
        let def = self
            .content
            .features
            .get(feature)
            .ok_or("no such feature")?;
        let map = self.map.as_ref().ok_or("there is no map")?;
        let planes: Vec<Plane> = if def.kind == crate::feature::def::FeatureKind::Tower {
            Plane::all().collect()
        } else {
            vec![pos.plane]
        };
        for plane in planes {
            let terrain = map
                .plane(plane)
                .get(pos.x as i32, pos.y as i32)
                .and_then(|t| self.content.terrains.get(t))
                .ok_or("that square is off the map")?;
            if !def.stands_on(terrain.class, plane) {
                return Err(format!(
                    "{} cannot stand on {} ({plane:?})",
                    def.id, terrain.id
                ));
            }
        }
        Ok(())
    }

    /// Put a feature (or none) on a square, whatever its terrain: a mineral goes in the map layer,
    /// a tower, node or encounter site becomes an entity (with no guards or treasure, and a node
    /// with no aura). A tower takes the square of both planes.
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
        let kind = feature.and_then(|id| self.content.features.get(id).map(|d| d.kind));
        self.clear_for(pos, kind);
        let Some(id) = feature else {
            return true;
        };
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

    /// Make room on a square for a feature of `kind`: take off its mineral and feature entity,
    /// for a tower on both planes.
    fn clear_for(&mut self, pos: Position, kind: Option<crate::feature::def::FeatureKind>) {
        let tower = kind == Some(crate::feature::def::FeatureKind::Tower);
        for at in [Some(pos), tower.then(|| pos.on_other_plane())]
            .into_iter()
            .flatten()
        {
            let Some(map) = self.map.as_mut() else {
                return;
            };
            map.plane_mut(at.plane).set_mineral(at.x, at.y, None);
            if let Some(old) = map.feature_entity(at) {
                self.despawn_feature(old);
            }
        }
    }

    /// Give a city a building at no cost and without its requirements.
    pub fn cheat_add_building(&mut self, city: hecs::Entity, building: BuildingId) -> bool {
        if self.economy().city_owner(city).is_none() {
            return false;
        }
        let mut list = self
            .ecs
            .get::<&Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        if !list.contains(&building) {
            list.push(building);
        }
        self.ecs.insert_one(city, Buildings(list)).is_ok()
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

    /// Teach a wizard no more of a spell: it is no longer known, and goes back to what the wizard
    /// can research when the books allow it. False when there is no such wizard or it did not
    /// know the spell.
    pub fn cheat_forget_spell(&mut self, player: PlayerId, spell: SpellId) -> bool {
        let allowed = {
            let def = self.content.rules.spells.get(spell);
            self.wizard(player).is_some_and(|w| w.books_allow(def))
        };
        let Some(mut w) = self.wizard_mut(player) else {
            return false;
        };
        let mut spells = w.get_mut::<crate::wizard::sheet::KnownSpells>();
        spells.starting.remove(&spell);
        let knew = spells.known.remove(&spell);
        if knew && allowed {
            spells.researchable.insert(spell);
        }
        knew
    }

    /// Change a wizard's books and retorts with `change` (given what they are now), then work out
    /// the casting skill again ([`crate::wizard::sheet::WizardMut::set_sheet`]). False when there
    /// is no such wizard or `change` says nothing changed.
    fn cheat_change_sheet(
        &mut self,
        player: PlayerId,
        change: impl FnOnce(&mut BTreeMap<RealmId, u8>, &mut BTreeSet<Retort>) -> bool,
    ) -> bool {
        let rules = &self.content.rules;
        let Some(entity) = self
            .player_index
            .get(&player)
            .and_then(|&e| crate::wizard::entity::of_player(&self.ecs, e))
        else {
            return false;
        };
        let mut w = crate::wizard::sheet::WizardMut::new(&mut self.ecs, entity);
        let (mut books, mut retorts) = (w.view().books(), w.view().retorts());
        if !change(&mut books, &mut retorts) {
            return false;
        }
        w.set_sheet(rules, books, retorts);
        true
    }

    /// Set the books a wizard has in a realm (at most the game's most for a realm; none removes
    /// them); the casting skill follows.
    pub fn cheat_set_books(&mut self, player: PlayerId, realm: RealmId, books: u8) -> bool {
        let books = books.min(self.settings.wizards.max_books_per_realm);
        self.cheat_change_sheet(player, |all, _| {
            all.insert(realm, books);
            true
        })
    }

    /// Give a wizard a retort, or take it away; the casting skill follows. False when there is no
    /// such wizard, or it already had (or did not have) the retort.
    pub fn cheat_set_retort(&mut self, player: PlayerId, retort: Retort, on: bool) -> bool {
        self.cheat_change_sheet(player, |_, retorts| {
            if on {
                retorts.insert(retort)
            } else {
                retorts.remove(&retort)
            }
        })
    }

    /// Defeat a wizard now, as if `winner` had beaten them (for tests and the console).
    pub fn cheat_defeat_player(
        &mut self,
        loser: PlayerId,
        winner: PlayerId,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        self.defeat(loser, winner, crate::wizard::fortress::Fall::Resigned, rng)
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
    pub fn cheat_eliminate(
        &mut self,
        player: PlayerId,
        by: PlayerId,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        if !self.has_player(player) || self.player(player).is_none_or(|p| p.is_defeated()) {
            return Vec::new();
        }
        for (city, c, _) in self.economy().cities() {
            if c.owner == player {
                self.set_owner(city, MONSTERS);
            }
        }
        self.defeat(player, by, crate::wizard::fortress::Fall::Resigned, rng)
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
        rng: &mut GameRng,
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
            no_heroes: !self.settings.world.heroes_enabled,
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
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(rng.next_u64());
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
        self.clear_for(pos, Some(kind));
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
            .ecs
            .satisfies::<&crate::feature::lair::Lair>(site)
            .unwrap_or(false)
        {
            return false;
        }
        self.despawn_feature(site);
        true
    }

    /// Hire a hero type for `player` at no cost: it appears on `at`, any square of the map, or without
    /// one on the wizard's fortress, or in a city of theirs. The hero must be free or dead (a
    /// resurrection).
    pub fn cheat_hire_hero(
        &mut self,
        player: PlayerId,
        kind: usize,
        at: Option<Position>,
    ) -> Result<hecs::Entity, String> {
        if let Some(at) = at {
            let on_map = self.map.as_ref().is_some_and(|m| {
                i64::from(at.x) < i64::from(m.width()) && i64::from(at.y) < i64::from(m.height())
            });
            if !on_map {
                return Err(format!("{at:?} is not on the map"));
            }
        }
        let at = at
            .or_else(|| self.wizard(player).and_then(|w| w.fortress()))
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
    pub fn cheat_reroll_hero(&mut self, kind: usize, rng: &mut GameRng) -> Result<(), String> {
        let hero = self.units().hero_entity(kind).ok_or("no such hero")?;
        let name = self
            .ecs
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
        let mut rolled =
            crate::hero::kind::roll_one(rng, &self.content.rules, &self.names, kind, template);
        rolled.name = name;
        self.put(hero, rolled);
        Ok(())
    }
}
