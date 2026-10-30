//! Helpers for tests that need features on a map.
//!
//! Tests: none, because this is test support.

use hecs::Entity;

use crate::feature::def::FeatureKind;
use crate::feature::entity::Placement;
use crate::feature::lair::Lair;
use crate::feature::node::Node;
use crate::game::Game;
use crate::map::Position;

impl Game {
    /// Put the feature called `id` on a square: a mineral in the map layer, anything else as an
    /// entity with no guards and no node data. Returns the entity (none for a mineral).
    pub(crate) fn put_feature(&mut self, at: Position, id: &str) -> Option<Entity> {
        let feature = self
            .content
            .features
            .id_of(id)
            .expect("a feature of that id");
        if self.content.features.get(feature).map(|d| d.kind) == Some(FeatureKind::Mineral) {
            self.map
                .as_mut()
                .expect("a map")
                .plane_mut(at.plane)
                .set_mineral(at.x, at.y, Some(feature));
            return None;
        }
        Some(self.spawn_feature(Placement {
            at,
            feature,
            node: None,
            lair: None,
        }))
    }

    /// Put a guarded site (a tower, a node or an encounter site) on a square.
    pub(crate) fn put_lair(&mut self, at: Position, id: &str, lair: Lair) -> Entity {
        let feature = self
            .content
            .features
            .id_of(id)
            .expect("a feature of that id");
        self.spawn_feature(Placement {
            at,
            feature,
            node: None,
            lair: Some(lair),
        })
    }

    /// Put a node with the given data on a square, owned by `owner`.
    pub(crate) fn put_node_data(
        &mut self,
        at: Position,
        id: &str,
        node: Node,
        owner: Option<crate::player::PlayerId>,
    ) -> Entity {
        let feature = self
            .content
            .features
            .id_of(id)
            .expect("a feature of that id");
        let e = self.spawn_feature(Placement {
            at,
            feature,
            node: Some(node),
            lair: None,
        });
        if let Some(owner) = owner {
            self.set_feature_owner(e, owner);
        }
        e
    }

    /// Put a node on a square, with an aura made from `seed` as map generation does.
    pub(crate) fn put_node(&mut self, at: Position, id: &str, seed: u64) -> Entity {
        let feature = self
            .content
            .features
            .id_of(id)
            .expect("a feature of that id");
        let mut placed = crate::feature::place::Placed::new(
            self.map.as_ref().expect("a map").width(),
            self.map.as_ref().expect("a map").height(),
        );
        placed.set(at.plane, at.x, at.y, feature);
        let nodes = crate::feature::node::generate_nodes(
            seed,
            self.map.as_ref().expect("a map"),
            &placed,
            &self.content,
        );
        let node: Node = nodes.into_iter().next().expect("a node").1;
        self.spawn_feature(Placement {
            at,
            feature,
            node: Some(node),
            lair: None,
        })
    }
}
