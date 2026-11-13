//! Magic nodes of a new world: the aura and power of each node the placement put on the map.
//!
//! A node's power is the number of squares its aura covers (5-10 on Arcanus, 10-20 on Myrror).
//! What a node does in play (melding, income) is `f4x_core::feature::node`.
//!
//! Tests: below.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::place::Placed;
use f4x_core::feature::def::FeatureKind;
use f4x_core::feature::node::Node;
use f4x_core::map::{Map, Plane, Position};
use f4x_def::content::Content;

/// Squares of a node's aura: its own tile, then up to eight of the squares around it, then
/// the rest from the ring two squares out; fewer if the map edge leaves no room.
fn make_aura(
    rng: &mut Pcg64Mcg,
    map: &Map,
    plane: Plane,
    x: u32,
    y: u32,
    power: u8,
) -> Vec<(u32, u32)> {
    let (w, h) = (map.width() as i32, map.height() as i32);
    let mut aura = vec![(x, y)];
    for radius in [1i32, 2] {
        let mut ring: Vec<(u32, u32)> = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                let ny = y as i32 + dy;
                if !(0..h).contains(&ny) {
                    continue;
                }
                ring.push(((x as i32 + dx).rem_euclid(w) as u32, ny as u32));
            }
        }
        while aura.len() < usize::from(power) && !ring.is_empty() {
            let i = rng.gen_range(0..ring.len());
            aura.push(ring.swap_remove(i));
        }
    }
    let _ = plane;
    aura
}

/// Make the `Node` of every node feature on the map, each with the square it stands on.
pub fn generate_nodes(
    seed: u64,
    map: &Map,
    placed: &Placed,
    content: &Content,
) -> Vec<(Position, Node)> {
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x4E4F_4445_535F_5057);
    let mut nodes = Vec::new();
    for plane in Plane::all() {
        let mut found = Vec::new();
        for y in 0..map.height() {
            for x in 0..map.width() {
                let Some(def) = placed
                    .get(plane, x as i32, y as i32)
                    .and_then(|f| content.features.get(f))
                else {
                    continue;
                };
                if def.kind == FeatureKind::Node {
                    if let Some(realm) = def.realm {
                        found.push((x, y, realm));
                    }
                }
            }
        }
        for (x, y, realm) in found {
            let power: u8 = match plane {
                Plane::Arcanus => 5 + rng.gen_range(0..6),
                Plane::Myrror => 10 + rng.gen_range(0..11),
            };
            let aura = make_aura(&mut rng, map, plane, x, y, power);
            nodes.push((
                Position { plane, x, y },
                Node {
                    realm,
                    power,
                    aura,
                    guardian: false,
                    warped: false,
                },
            ));
        }
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::map::Grid;
    use f4x_def::testdir::{shipped, TestResult};

    /// The terrain and feature numbers of the shipped content (`f4x_core::game::testutil::num`).
    const GRASSLAND: u16 = 4;
    const SORCERY_NODE: u16 = 20;

    fn build(seed: u64) -> Result<Vec<(Position, Node)>, String> {
        let c = shipped();
        let grass = c
            .terrains
            .id_numbered(GRASSLAND)
            .ok_or("grassland is not in the content")?;
        let node = c
            .features
            .id_numbered(SORCERY_NODE)
            .ok_or("sorcery_node is not in the content")?;
        let map = Map::new(Grid::new(30, 20, grass), Grid::new(30, 20, grass));
        let mut placed = Placed::new(30, 20);
        for p in Plane::all() {
            placed.set(p, 10, 10, node);
        }
        Ok(generate_nodes(seed, &map, &placed, c))
    }

    #[test]
    fn a_node_gets_power_and_an_aura_around_its_own_square() -> TestResult {
        for (at, n) in build(5)? {
            let range = match at.plane {
                Plane::Arcanus => 5..=10,
                Plane::Myrror => 10..=20,
            };
            assert!(range.contains(&n.power), "{at:?}: {}", n.power);
            assert_eq!(n.aura.len(), usize::from(n.power));
            assert_eq!(n.aura[0], (10, 10));
            let mut sorted = n.aura.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), n.aura.len(), "no square twice");
            assert!(n
                .aura
                .iter()
                .all(|&(x, y)| x.abs_diff(10).max(y.abs_diff(10)) <= 2));
            assert!(!n.guardian && !n.warped);
        }
        Ok(())
    }

    #[test]
    fn generation_is_deterministic_and_myrror_nodes_are_stronger() -> TestResult {
        assert_eq!(build(5)?, build(5)?);
        let nodes = build(5)?;
        let power = |plane| {
            nodes
                .iter()
                .find(|(p, _)| p.plane == plane)
                .map(|(_, n)| n.power)
        };
        assert!(matches!(power(Plane::Arcanus), Some(5..=10)));
        assert!(matches!(power(Plane::Myrror), Some(10..=20)));
        Ok(())
    }
}
