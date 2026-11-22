//! Spells that reshape a map square: Raise Volcano and Change Terrain.
//!
//! A volcano is a mountain square with a volcano entity on it (`feature::entity`), owned by the
//! wizard who raised it; the owner gets one point of magic power per volcano each turn. Change Terrain turns a volcano back into
//! plain mountains.

use rand::RngCore;

use crate::city::economy::Buildings;
use crate::feature::def::FeatureKind;
use crate::game::{Event, Game};
use crate::map::terrain::TerrainClass;
use crate::map::Position;
use crate::player::PlayerId;
use crate::spell::target_error::TargetError;

/// The hook of the spells that act on one map square.
pub const TERRAIN_EFFECT: &str = "terrain_effect";

crate::hook::effect_enum! {
    /// What a spell with the `terrain_effect` hook does to the square it is aimed at: the
    /// argument of the hook (`Custom(name: "terrain_effect", args: ["corruption"])`).
    pub enum TerrainEffect in TERRAIN_EFFECT;
    ChangeTerrain = "change_terrain",
    RaiseVolcano = "raise_volcano",
    Corruption = "corruption",
    EnchantRoad = "enchant_road",
    WarpNode = "warp_node",
    Transmute = "transmute",
    EarthLore = "earth_lore",
    FloatingIsland = "floating_island",
}

impl TerrainEffect {
    /// The effect a spell's definition gives, if it has the hook.
    pub fn of(def: &crate::rules::SpellDef) -> Option<TerrainEffect> {
        TerrainEffect::named(def.hook_args(TERRAIN_EFFECT)?.first()?)
    }
}

impl Game {
    /// Volcanoes that add to `player`'s magic power, one each: the ones they own.
    pub fn volcano_count(&self, player: PlayerId) -> i32 {
        self.economy()
            .volcanoes()
            .iter()
            .filter(|&&(_, owner)| owner == player)
            .count() as i32
    }

    /// A volcano may rise on land that is not mountains or hills, or in the sea, and not on a node or any
    /// other site (minerals are fine: they are destroyed).
    pub fn check_raise_volcano(&self, at: Position) -> Result<(), TargetError> {
        let class = self
            .economy()
            .terrain_class_at(at)
            .ok_or(TargetError::OffMap)?;
        match class {
            TerrainClass::Mountains => return Err(TargetError::VolcanoOnMountain),
            TerrainClass::Hills => return Err(TargetError::VolcanoOnHill),
            _ => {}
        }
        if self
            .map
            .as_ref()
            .is_some_and(|m| m.plane(at.plane).has_river(at.x as i32, at.y as i32))
        {
            return Err(TargetError::VolcanoOnRiver);
        }
        self.economy()
            .feature_entity_at(at)
            .is_none()
            .then_some(())
            .ok_or(TargetError::VolcanoOnSite)
    }

    /// What Change Terrain turns the square into, or why it is not a valid target:
    /// deserts, forests, hills and swamps become grassland, grassland becomes forest,
    /// mountains become hills, and a volcano becomes plain mountains.
    pub fn change_terrain_result(&self, at: Position) -> Result<TerrainClass, TargetError> {
        let class = self
            .economy()
            .terrain_class_at(at)
            .ok_or(TargetError::OffMap)?;
        match class {
            TerrainClass::Desert
            | TerrainClass::Forest
            | TerrainClass::Hills
            | TerrainClass::Swamp => Ok(TerrainClass::Grassland),
            TerrainClass::Grassland => Ok(TerrainClass::Forest),
            TerrainClass::Mountains => Ok(TerrainClass::Hills),
            TerrainClass::Ocean | TerrainClass::Tundra => Err(TargetError::TerrainFixed),
            TerrainClass::SorceryNode | TerrainClass::NatureNode | TerrainClass::ChaosNode => {
                Err(TargetError::NodeFixed)
            }
        }
    }

    /// Raise Volcano: a volcano on land that can take one.
    pub(crate) fn raise_volcano(
        &mut self,
        owner: PlayerId,
        at: Position,
        rng: &mut dyn RngCore,
    ) -> Vec<Event> {
        if self.check_raise_volcano(at).is_err() {
            return Vec::new();
        }
        self.make_volcano(owner, at, rng, false)
    }

    /// Whether Armageddon may put a volcano on `at`: any square that is not a volcano already
    /// and holds no node or other site, water and cities included (the original keeps to land and
    /// away from the caster's cities; the sea is rarer, see `world.rs`).
    pub fn can_armageddon_volcano(&self, at: Position) -> bool {
        self.economy().terrain_class_at(at).is_some()
            && self.economy().feature_entity_at(at).is_none()
    }

    /// One of Armageddon's volcanoes, on land or sea (`can_armageddon_volcano`).
    pub(crate) fn armageddon_volcano(
        &mut self,
        owner: PlayerId,
        at: Position,
        rng: &mut dyn RngCore,
    ) -> Vec<Event> {
        if !self.can_armageddon_volcano(at) {
            return Vec::new();
        }
        self.make_volcano(owner, at, rng, true)
    }

    /// The square becomes a volcano of `owner`'s: mountains with an owner, its old feature gone
    /// and, one time in five, a new mineral. Ships on the square sink. A city on the square
    /// loses a tenth and a half of its buildings (a quarter of its buildings, units and
    /// citizens in Armageddon's case); the squares around it are not touched.
    fn make_volcano(
        &mut self,
        owner: PlayerId,
        at: Position,
        rng: &mut dyn RngCore,
        armageddon: bool,
    ) -> Vec<Event> {
        // Everything that can refuse is looked at before the map is touched: a volcano of a
        // player that does not exist, or on a plane with no mountains in the content, changes
        // nothing and is not announced.
        if self.player_entity(owner).is_none() {
            return Vec::new();
        }
        let Some(mountains) = self
            .content
            .terrains
            .first_of_class(TerrainClass::Mountains, at.plane)
        else {
            return Vec::new();
        };
        let mut events = Vec::new();
        let mineral =
            if rng.next_u32() % 100 < self.content.rules.config.spell_effects.mineral_percent {
                let weights = self.content.features.weighted(
                    FeatureKind::Mineral,
                    TerrainClass::Mountains,
                    at.plane,
                );
                let total: u32 = weights.iter().map(|&(_, w)| w).sum();
                if total == 0 {
                    None
                } else {
                    let mut roll = rng.next_u32() % total;
                    weights.iter().find_map(|&(id, w)| {
                        if roll < w {
                            Some(id)
                        } else {
                            roll -= w;
                            None
                        }
                    })
                }
            } else {
                None
            };
        // A ship cannot sail on mountains: it goes down, and those aboard are left on the land.
        let ships: Vec<hecs::Entity> = self
            .units()
            .units_at(at)
            .into_iter()
            .filter(|&u| {
                self.units()
                    .unit_def(u)
                    .is_some_and(|d| d.movement == crate::rules::MovementType::Sailing)
            })
            .collect();
        if let Some(map) = self.map.as_mut() {
            let grid = map.plane_mut(at.plane);
            grid.set(at.x, at.y, mountains);
            grid.set_mineral(at.x, at.y, mineral);
        }
        let raised = self.spawn_volcano(owner, at);
        debug_assert!(
            raised.is_some(),
            "the owner was looked at before the map changed"
        );
        for ship in ships {
            self.remove_unit(ship, crate::hero::Fate::Slain);
        }
        for unit in self.units().units_at(at) {
            self.strip::<crate::unit::transport::Aboard>(unit);
        }
        events.push(Event::VolcanoRaised {
            player: owner,
            pos: at,
        });
        let Some(city) = self.economy().city_at(at) else {
            return events;
        };
        if armageddon {
            events.extend(self.devastate_city(city, owner, rng));
            return events;
        }
        let mut lost = 0;
        if let Ok(mut b) = self.world.get::<&mut Buildings>(city) {
            let before = b.0.len();
            b.0.retain(|_| {
                rng.next_u32() % 100 >= self.content.rules.config.spell_effects.collapse_percent
            });
            lost = before - b.0.len();
        }
        if lost > 0 {
            events.push(Event::BuildingsCollapsed {
                city,
                count: lost as u32,
            });
        }
        events
    }

    /// A volcano from Armageddon rises under the city: each building, each unit in it and each
    /// citizen is lost with one chance in four. A city never loses its last citizen.
    fn devastate_city(
        &mut self,
        city: hecs::Entity,
        caster: PlayerId,
        rng: &mut dyn RngCore,
    ) -> Vec<Event> {
        let Some(spell) = self.content.rules.spells.id_numbered(120)
        /* armageddon */
        else {
            return Vec::new();
        };
        let Ok(at) = self.world.get::<&Position>(city).map(|p| *p) else {
            return Vec::new();
        };
        let victim = self.economy().city_owner(city);
        let mut buildings_lost = 0;
        if let Ok(mut b) = self.world.get::<&mut Buildings>(city) {
            let before = b.0.len();
            b.0.retain(|_| {
                rng.next_u32() % 100
                    >= self
                        .content
                        .rules
                        .config
                        .spell_effects
                        .armageddon_city_percent
            });
            buildings_lost = (before - b.0.len()) as u32;
        }
        let mut units_lost = 0;
        for unit in self.units().units_at(at) {
            if rng.next_u32() % 100
                < self
                    .content
                    .rules
                    .config
                    .spell_effects
                    .armageddon_city_percent
            {
                self.remove_unit(unit, crate::hero::Fate::Slain);
                units_lost += 1;
            }
        }
        let mut population_lost = 0;
        if let Ok(mut pop) = self
            .world
            .get::<&mut crate::city::economy::Population>(city)
        {
            let dead = (0..pop.figures.saturating_sub(1))
                .filter(|_| {
                    rng.next_u32() % 100
                        < self
                            .content
                            .rules
                            .config
                            .spell_effects
                            .armageddon_city_percent
                })
                .count() as u32;
            pop.figures -= dead;
            population_lost = dead;
        }
        let _ = caster;
        vec![Event::WorldSpellStruck {
            spell,
            victim,
            units_lost,
            buildings_lost,
            population_lost,
        }]
    }

    /// Each turn every volcano has a small chance of cooling into plain mountains.
    pub(crate) fn cool_volcanoes(&mut self, rng: &mut dyn RngCore) {
        for (at, _) in self.economy().volcanoes() {
            if rng.next_u32() % 100 < self.content.rules.config.spell_effects.cool_percent {
                self.remove_volcano(at);
            }
        }
    }

    /// The volcano on a square goes out: the square stays plain mountains.
    pub(crate) fn remove_volcano(&mut self, at: Position) {
        if self.economy().volcano_at(at).is_some() {
            if let Some(e) = self.economy().feature_entity_at(at) {
                self.despawn_feature(e);
            }
        }
    }

    pub(crate) fn change_terrain_spell(&mut self, at: Position) -> Vec<Event> {
        let Ok(new) = self.change_terrain_result(at) else {
            return Vec::new();
        };
        // A volcano is mountains with an owner: removing the owner is the whole change.
        if self.economy().volcano_at(at).is_some() {
            self.remove_volcano(at);
            return vec![Event::TerrainChanged { pos: at }];
        }
        let Some(id) = self.content.terrains.first_of_class(new, at.plane) else {
            return Vec::new();
        };
        if let Some(map) = self.map.as_mut() {
            map.plane_mut(at.plane).set(at.x, at.y, id);
        }
        vec![Event::TerrainChanged { pos: at }]
    }
}
