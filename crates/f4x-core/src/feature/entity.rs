//! The terrain features that are entities (tasks 329 to 332): towers, magic nodes, encounter
//! sites and the volcanoes spells raise. Each has a [`Position`] and a creation number; a
//! `TerrainFeature` says which kind it is (all but a volcano), a [`Node`] and a [`Lair`] hold the
//! state of a node and of a guarded site (a node and a tower have both), and an `Owner` names the
//! player who owns the node, the tower or the volcano.
//!
//! Minerals are not entities: nothing happens when a unit enters their square, so they stay a
//! layer of the map ([`crate::map::Grid::mineral`]).
//!
//! The map keeps one index from a square to the feature entity on it (a tower is on both planes).
//! It is rebuilt on load and never saved.

use std::collections::HashMap;

use hecs::{Entity, World};

use crate::city::economy::Economy;
use crate::feature::def::{FeatureDef, FeatureKind};
use crate::feature::lair::Lair;
use crate::feature::node::Node;
use crate::game::serial::Serial;
use crate::game::Game;
use crate::map::{FeatureId, Position};
use crate::player::{self, Owner, PlayerId};

/// On a tower, a node or an encounter site: which feature of the content it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainFeature(pub FeatureId);

/// On a volcano a spell raised on a mountain square. Its owner gets a point of power a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volcano;

/// The square-to-entity index of a map. Two indexes are never different: the entities are what
/// a game is made of and this only finds them, so comparing maps ignores it.
#[derive(Debug, Clone, Default)]
pub struct FeatureIndex(HashMap<Position, Entity>);

impl PartialEq for FeatureIndex {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for FeatureIndex {}

impl FeatureIndex {
    pub fn get(&self, at: Position) -> Option<Entity> {
        self.0.get(&at).copied()
    }

    pub(crate) fn insert(&mut self, at: Position, entity: Entity) {
        self.0.insert(at, entity);
    }

    pub(crate) fn remove(&mut self, at: Position) {
        self.0.remove(&at);
    }
}

/// A feature the map generator or a test puts on the map, for the game to spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub at: Position,
    pub feature: FeatureId,
    pub node: Option<Node>,
    pub lair: Option<Lair>,
}

/// A node as the rules read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeInfo {
    pub entity: Entity,
    pub pos: Position,
    pub realm: String,
    pub power: u8,
    pub aura: Vec<(u32, u32)>,
    pub owner: Option<PlayerId>,
    pub guardian: bool,
    pub warped: bool,
}

/// A guarded site as the rules read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LairInfo {
    pub entity: Entity,
    pub pos: Position,
    /// Id of the site's feature: `tower`, a node, or an encounter site.
    pub site: String,
    pub intact: bool,
    pub guards: Vec<(String, u32)>,
    pub gold: u32,
    pub mana: u32,
    pub items: Vec<u32>,
    pub prisoner: bool,
    pub special: u8,
    /// The players who have fought for this site once.
    pub fought_by: Vec<PlayerId>,
}

impl LairInfo {
    /// Number of guard units.
    pub fn guard_count(&self) -> u32 {
        self.guards.iter().map(|g| g.1).sum()
    }
}

/// The player an `Owner` on `entity` names.
fn owner_of(world: &World, entity: Entity) -> Option<PlayerId> {
    let owner = world.get::<&Owner>(entity).ok()?;
    player::id_of(world, owner.0)
}

impl<'a> Economy<'a> {
    /// The feature entity on a square: a tower, node, encounter site or volcano.
    pub fn feature_entity_at(&self, at: Position) -> Option<Entity> {
        self.map?.feature_entity(at)
    }

    /// The feature of the content on a square: the one of the entity there, or else the mineral.
    pub fn feature_id_at(&self, at: Position) -> Option<FeatureId> {
        if let Some(e) = self.feature_entity_at(at) {
            if let Ok(f) = self.world.get::<&TerrainFeature>(e) {
                return Some(f.0);
            }
        }
        self.map?.plane(at.plane).mineral(at.x as i32, at.y as i32)
    }

    /// The definition of the feature on a square.
    pub fn feature_at(&self, at: Position) -> Option<&'a FeatureDef> {
        self.content.features.get(self.feature_id_at(at)?)
    }

    /// The kind of the feature on a square.
    pub fn feature_kind_at(&self, at: Position) -> Option<FeatureKind> {
        self.feature_at(at).map(|f| f.kind)
    }

    fn node_info(&self, entity: Entity) -> Option<NodeInfo> {
        let node = self.world.get::<&Node>(entity).ok()?;
        let pos = *self.world.get::<&Position>(entity).ok()?;
        Some(NodeInfo {
            entity,
            pos,
            realm: node.realm.clone(),
            power: node.power,
            aura: node.aura.clone(),
            owner: owner_of(self.world, entity),
            guardian: node.guardian,
            warped: node.warped,
        })
    }

    fn lair_info(&self, entity: Entity) -> Option<LairInfo> {
        let lair = self.world.get::<&Lair>(entity).ok()?;
        let pos = *self.world.get::<&Position>(entity).ok()?;
        let site = self
            .world
            .get::<&TerrainFeature>(entity)
            .ok()
            .and_then(|f| self.content.features.get(f.0).map(|d| d.id.clone()))?;
        Some(LairInfo {
            entity,
            pos,
            site,
            intact: lair.intact,
            guards: lair.guards.clone(),
            gold: lair.gold,
            mana: lair.mana,
            items: lair.items.clone(),
            prisoner: lair.prisoner,
            special: lair.special,
            fought_by: lair.fought_by.clone(),
        })
    }

    /// Every feature entity that has component `C`, oldest first.
    fn entities_with<C: hecs::Component>(&self) -> Vec<Entity> {
        let mut found: Vec<(Serial, Entity)> = self
            .world
            .query::<(&C, &Serial)>()
            .iter()
            .map(|(e, (_, s))| (*s, e))
            .collect();
        found.sort_by_key(|&(s, e)| (s, e.id()));
        found.into_iter().map(|(_, e)| e).collect()
    }

    /// The magic nodes, oldest first.
    pub fn nodes(&self) -> Vec<NodeInfo> {
        self.entities_with::<Node>()
            .into_iter()
            .filter_map(|e| self.node_info(e))
            .collect()
    }

    /// The node standing on a square.
    pub fn node_at(&self, at: Position) -> Option<NodeInfo> {
        let e = self.feature_entity_at(at)?;
        self.node_info(e)
    }

    /// The node whose aura covers a square (the node's own square included).
    pub fn node_covering(&self, at: Position) -> Option<NodeInfo> {
        let mut covering: Vec<(Serial, Entity)> = self
            .world
            .query::<(&Node, &Position, &Serial)>()
            .iter()
            .filter(|(_, (n, p, _))| p.plane == at.plane && n.aura.contains(&(at.x, at.y)))
            .map(|(e, (_, _, s))| (*s, e))
            .collect();
        covering.sort_by_key(|&(s, e)| (s, e.id()));
        self.node_info(covering.first()?.1)
    }

    /// Every guarded site, cleared ones included, oldest first.
    pub fn lairs(&self) -> Vec<LairInfo> {
        self.entities_with::<Lair>()
            .into_iter()
            .filter_map(|e| self.lair_info(e))
            .collect()
    }

    /// The intact guarded site on a square.
    pub fn lair_at(&self, at: Position) -> Option<LairInfo> {
        let e = self.feature_entity_at(at)?;
        self.lair_info(e).filter(|l| l.intact)
    }

    /// What guards the site on a square, if `player` knows: they have fought for it once, or a
    /// spell shows them the units there (Nature Awareness shows the whole map, Nature's Eye
    /// the squares round the city it is on). `None` when they do not know.
    pub fn lair_guards_known_to(
        &self,
        at: Position,
        player: PlayerId,
    ) -> Option<Vec<(String, u32)>> {
        let lair = self.lair_at(at)?;
        (lair.fought_by.contains(&player) || self.spell_shows_units(at, player))
            .then_some(lair.guards)
    }

    /// Does a spell of `player` show them the units on a square? Nature Awareness shows the
    /// whole world; Nature's Eye widens the sight of its city, and the units in it show.
    fn spell_shows_units(&self, at: Position, player: PlayerId) -> bool {
        if self.units().has_global(player, "nature_awareness") {
            return true;
        }
        let Some(map) = self.map else {
            return false;
        };
        let grid = map.plane(at.plane);
        self.cities().into_iter().any(|(city, info, pos)| {
            if info.owner != player
                || pos.plane != at.plane
                || !self.city_has_enchantment(city, "natures_eye")
            {
                return false;
            }
            let w = grid.width() as i32;
            let mut dx = (at.x as i32 - pos.x as i32).rem_euclid(w);
            if dx > w / 2 {
                dx -= w;
            }
            let dy = at.y as i32 - pos.y as i32;
            crate::map::fog::in_sight(dx, dy, self.city_sight(city), self.diagonal_extra)
        })
    }

    /// A guarded site by its entity.
    pub fn lair_of(&self, entity: Entity) -> Option<LairInfo> {
        self.lair_info(entity)
    }

    /// The squares of the towers, nodes and encounter sites; a tower is on both planes.
    pub fn site_squares(&self) -> Vec<Position> {
        let mut found = Vec::new();
        for (e, (f, pos)) in self.world.query::<(&TerrainFeature, &Position)>().iter() {
            let _ = e;
            found.push(*pos);
            if self.content.features.get(f.0).map(|d| d.kind) == Some(FeatureKind::Tower) {
                found.push(pos.on_other_plane());
            }
        }
        found
    }

    /// The player who last stood in the tower on a square.
    pub fn tower_owner(&self, at: Position) -> Option<PlayerId> {
        let e = self.feature_entity_at(at)?;
        let is_tower = self
            .world
            .get::<&TerrainFeature>(e)
            .ok()
            .and_then(|f| self.content.features.get(f.0))
            .is_some_and(|d| d.kind == FeatureKind::Tower);
        is_tower.then(|| owner_of(self.world, e)).flatten()
    }

    /// The player whose volcano stands on a square.
    pub fn volcano_at(&self, at: Position) -> Option<PlayerId> {
        let e = self.feature_entity_at(at)?;
        if !self.world.satisfies::<&Volcano>(e).unwrap_or(false) {
            return None;
        }
        owner_of(self.world, e)
    }

    /// Every volcano: its square and owner, oldest first.
    pub fn volcanoes(&self) -> Vec<(Position, PlayerId)> {
        self.entities_with::<Volcano>()
            .into_iter()
            .filter_map(|e| {
                let pos = *self.world.get::<&Position>(e).ok()?;
                Some((pos, owner_of(self.world, e)?))
            })
            .collect()
    }
}

impl Game {
    /// Spawn the feature a placement describes and enter it in the map's index. A tower is one
    /// entity found on both planes.
    pub fn spawn_feature(&mut self, placement: Placement) -> Entity {
        let serial = self.take_serial();
        self.spawn_feature_with(serial, placement)
    }

    /// [`Game::spawn_feature`] with a creation number already taken (a load).
    pub(crate) fn spawn_feature_with(&mut self, serial: Serial, placement: Placement) -> Entity {
        let kind = self.content.features.get(placement.feature).map(|d| d.kind);
        let e = self
            .world
            .spawn((TerrainFeature(placement.feature), placement.at, serial));
        if let Some(node) = placement.node {
            let _ = self.world.insert_one(e, node);
        }
        if let Some(lair) = placement.lair {
            let _ = self.world.insert_one(e, lair);
        }
        self.index_feature(e, placement.at, kind == Some(FeatureKind::Tower));
        e
    }

    /// Spawn the volcano of `owner` on a square.
    pub(crate) fn spawn_volcano(&mut self, owner: PlayerId, at: Position) -> Option<Entity> {
        let serial = self.take_serial();
        self.spawn_volcano_with(serial, owner, at)
    }

    pub(crate) fn spawn_volcano_with(
        &mut self,
        serial: Serial,
        owner: PlayerId,
        at: Position,
    ) -> Option<Entity> {
        let player = self.player_entity(owner)?;
        let e = self.world.spawn((Volcano, at, serial, Owner(player)));
        self.index_feature(e, at, false);
        Some(e)
    }

    /// Hand a node, tower or volcano to a player.
    pub(crate) fn set_feature_owner(&mut self, feature: Entity, owner: PlayerId) {
        if let Some(player) = self.player_entity(owner) {
            let _ = self.world.insert_one(feature, Owner(player));
        }
    }

    /// Remove a feature entity and its entry in the index.
    pub(crate) fn despawn_feature(&mut self, entity: Entity) {
        let Ok(at) = self.world.get::<&Position>(entity).map(|p| *p) else {
            return;
        };
        let both = self
            .world
            .get::<&TerrainFeature>(entity)
            .ok()
            .and_then(|f| self.content.features.get(f.0).map(|d| d.kind))
            == Some(FeatureKind::Tower);
        if let Some(map) = self.map.as_mut() {
            map.unindex_feature(at, both);
        }
        let _ = self.world.despawn(entity);
    }

    fn index_feature(&mut self, entity: Entity, at: Position, both_planes: bool) {
        if let Some(map) = self.map.as_mut() {
            map.index_feature(entity, at, both_planes);
        }
    }

    /// Spawn the features of a generated map.
    pub fn spawn_features(&mut self, placements: Vec<Placement>) {
        for p in placements {
            self.spawn_feature(p);
        }
    }

    /// Despawn every tower, node, encounter site and volcano.
    pub(crate) fn despawn_all_features(&mut self) {
        let found: Vec<Entity> = self
            .world
            .query::<()>()
            .with::<&Position>()
            .iter()
            .map(|(e, ())| e)
            .filter(|&e| {
                self.world.satisfies::<&TerrainFeature>(e).unwrap_or(false)
                    || self.world.satisfies::<&Volcano>(e).unwrap_or(false)
            })
            .collect();
        for e in found {
            let _ = self.world.despawn(e);
        }
    }
}
