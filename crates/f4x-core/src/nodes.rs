//! Magic nodes: generation, ownership by melding, and the power they yield.
//!
//! A node's power is the number of squares its aura covers (5-10 on Arcanus, 10-20 on
//! Myrror). Each owned node gives `power * (strength + 1) / 2` mana power, doubled by the
//! owner's mastery of its realm and again by Node Mastery; a warped node costs five instead.
//! `strength` is the game's node setting: 0 weak, 1 normal, 2 powerful.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::city::{PlayerId, Position};
use crate::content::{Content, FeatureKind};
use crate::magic::Retort;
use crate::map::{Map, Node, Plane};
use crate::{Event, Game};

/// Why a meld did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeldError {
    /// No units, or they are not all one player's on one tile.
    BadSelection,
    /// There is no node here.
    NoNode,
    /// None of the units can meld.
    NoMelder,
    /// The node is warped and cannot be melded with.
    Warped,
}

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

/// Create a `Node` for every node feature on the map.
pub fn generate_nodes(seed: u64, map: &mut Map, content: &Content) {
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x4E4F_4445_535F_5057);
    let mut nodes = Vec::new();
    for plane in Plane::ALL {
        let grid = map.plane(plane);
        let mut found = Vec::new();
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                let Some(def) = grid
                    .feature(x as i32, y as i32)
                    .and_then(|f| content.features.get(f))
                else {
                    continue;
                };
                if def.kind == FeatureKind::Node {
                    let realm = def.id.strip_suffix("_node").unwrap_or(&def.id).to_string();
                    found.push((x, y, realm));
                }
            }
        }
        for (x, y, realm) in found {
            let power: u8 = match plane {
                Plane::Arcanus => 5 + rng.gen_range(0..6),
                Plane::Myrror => 10 + rng.gen_range(0..11),
            };
            let aura = make_aura(&mut rng, map, plane, x, y, power);
            nodes.push(Node {
                plane,
                x,
                y,
                realm,
                power,
                aura,
                owner: None,
                guardian: false,
                warped: false,
            });
        }
    }
    map.set_nodes(nodes);
}

impl Game {
    /// Power one node yields to its owner this turn.
    pub fn node_power(&self, node: &Node) -> i32 {
        if node.warped {
            return -5;
        }
        let mut power = i32::from(node.power) * (i32::from(self.magic_strength) + 1) / 2;
        let Some(owner) = node.owner else {
            return 0;
        };
        if let Some(m) = self.magic(PlayerId(owner)) {
            let mastery = match node.realm.as_str() {
                "sorcery" => m.has(Retort::SorceryMastery),
                "nature" => m.has(Retort::NatureMastery),
                "chaos" => m.has(Retort::ChaosMastery),
                _ => false,
            };
            if mastery {
                power *= 2;
            }
            if m.has(Retort::NodeMastery) {
                power *= 2;
            }
        }
        power
    }

    /// Power from all the nodes `player` owns.
    pub fn node_income(&self, player: PlayerId) -> i32 {
        self.map.as_ref().map_or(0, |m| {
            m.nodes()
                .iter()
                .filter(|n| n.owner == Some(player.0))
                .map(|n| self.node_power(n))
                .sum()
        })
    }

    /// The node standing on `at`, if any.
    pub fn node_at(&self, at: Position) -> Option<&Node> {
        let m = self.map.as_ref()?;
        m.nodes().get(m.node_at(at.plane, at.x, at.y)?)
    }

    /// Meld a spirit into the node the stack stands on. The spirit is used up; a Guardian
    /// Spirit is preferred and leaves the node guarded. Against a rival's guarded node the
    /// meld succeeds one time in four, and the spirit dies either way.
    pub fn meld(&mut self, units: &[hecs::Entity]) -> Result<Vec<Event>, MeldError> {
        let first = *units.first().ok_or(MeldError::BadSelection)?;
        let at = self.unit_position(first).ok_or(MeldError::BadSelection)?;
        let owner = self.unit_owner(first).ok_or(MeldError::BadSelection)?;
        if units
            .iter()
            .any(|&u| self.unit_position(u) != Some(at) || self.unit_owner(u) != Some(owner))
        {
            return Err(MeldError::BadSelection);
        }
        let index = {
            let map = self.map.as_ref().ok_or(MeldError::NoNode)?;
            map.node_at(at.plane, at.x, at.y).ok_or(MeldError::NoNode)?
        };
        let node = self
            .map
            .as_ref()
            .map(|m| m.nodes()[index].clone())
            .ok_or(MeldError::NoNode)?;
        if node.warped {
            return Err(MeldError::Warped);
        }
        let melders: Vec<hecs::Entity> = units
            .iter()
            .copied()
            .filter(|&u| self.unit_has_ability(u, "meld"))
            .collect();
        let guardian = melders
            .iter()
            .copied()
            .find(|&u| self.unit_def(u).is_some_and(|d| d.id == "guardian_spirit"));
        let melder = guardian
            .or(melders.last().copied())
            .ok_or(MeldError::NoMelder)?;
        let taken = match node.owner {
            None => true,
            Some(o) if o == owner.0 => true,
            Some(_) if !node.guardian => true,
            Some(_) => self.roll(100) <= 25,
        };
        let _ = self.world.despawn(melder);
        let mut events = Vec::new();
        if taken {
            if let Some(n) = self.map.as_mut().and_then(|m| m.node_mut(index)) {
                n.owner = Some(owner.0);
                // A rival's guardian is gone with the successful meld; ours stands again.
                n.guardian = guardian.is_some();
            }
            events.push(Event::NodeMelded {
                player: owner,
                pos: at,
            });
        } else {
            events.push(Event::MeldFailed {
                player: owner,
                pos: at,
            });
        }
        Ok(events)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::map::Grid;
    use crate::player::Player;
    use crate::rules::{Registry, UnitKind};
    use crate::testutil::content_with_magic;
    use crate::Command;

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// A map with one node at (6, 6), and a spirit unit that can meld.
    fn world() -> Game {
        world_seeded(9)
    }

    pub(crate) fn world_seeded(seed: u64) -> Game {
        let mut c = content_with_magic();
        let mut units = c.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            if u.id == "spirit" {
                u.abilities.push("meld".into());
                u.kind = UnitKind::Summoned;
                u.realm = Some("arcane".into());
            }
            if u.id == "mage" {
                u.id = "guardian_spirit".into();
                u.abilities.push("meld".into());
            }
        }
        c.rules.units = Registry::new(units);
        let grass = c.terrains.id_of("grassland").unwrap();
        let node = c.features.id_of("node").unwrap();
        let mut map = Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass));
        map.plane_mut(Plane::Arcanus).set_feature(6, 6, Some(node));
        generate_nodes(1, &mut map, &c);
        let mut g = Game::new(seed);
        g.content = c;
        g.map = Some(map);
        for id in [P0, P1] {
            let mut p = Player::new(id, "W");
            p.mana = 100;
            g.players.push(p);
            g.init_wizard(id, &[("life", 2)], &[], None).unwrap();
        }
        g
    }

    pub(crate) fn spirit(g: &mut Game, player: PlayerId, pos: Position, id: &str) -> hecs::Entity {
        let u = g.content.rules.units.id_of(id).unwrap();
        g.spawn_unit(player, pos, u)
    }

    #[test]
    fn nodes_get_power_and_an_aura() {
        let g = world();
        let nodes = g.map.as_ref().unwrap().nodes();
        assert_eq!(nodes.len(), 1);
        let n = &nodes[0];
        assert!(
            (5..=10).contains(&n.power),
            "Arcanus nodes are 5 to 10: {}",
            n.power
        );
        assert_eq!(n.aura.len(), usize::from(n.power));
        assert_eq!(n.aura[0], (6, 6));
        let mut sorted = n.aura.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), n.aura.len(), "no square twice");
        assert!(n
            .aura
            .iter()
            .all(|&(x, y)| x.abs_diff(6).max(y.abs_diff(6)) <= 2));
        assert_eq!(n.owner, None);
        let map = g.map.as_ref().unwrap();
        assert_eq!(map.node_covering(Plane::Arcanus, 6, 6), Some(0));
        assert_eq!(map.node_covering(Plane::Arcanus, 20, 2), None);
    }

    #[test]
    fn generation_is_deterministic_and_myrror_nodes_are_stronger() {
        let c = content_with_magic();
        let grass = c.terrains.id_of("grassland").unwrap();
        let node = c.features.id_of("node").unwrap();
        let build = || {
            let mut map = Map::new(Grid::new(30, 20, grass), Grid::new(30, 20, grass));
            for p in Plane::ALL {
                map.plane_mut(p).set_feature(10, 10, Some(node));
            }
            generate_nodes(5, &mut map, &c);
            map
        };
        assert_eq!(build(), build());
        let map = build();
        let power = |plane| map.nodes().iter().find(|n| n.plane == plane).unwrap().power;
        assert!((5..=10).contains(&power(Plane::Arcanus)));
        assert!((10..=20).contains(&power(Plane::Myrror)));
    }

    #[test]
    fn melding_a_spirit_takes_the_node_and_uses_the_spirit_up() {
        let mut g = world();
        let s = spirit(&mut g, P0, at(6, 6), "spirit");
        let events = g.apply(Command::Meld { units: vec![s] });
        assert!(
            events.iter().any(|e| matches!(e, Event::NodeMelded { .. })),
            "{events:?}"
        );
        assert!(
            g.world.get::<&crate::units::Unit>(s).is_err(),
            "the spirit is gone"
        );
        assert_eq!(g.map.as_ref().unwrap().nodes()[0].owner, Some(0));
        assert!(!g.map.as_ref().unwrap().nodes()[0].guardian);
    }

    #[test]
    fn only_spirits_on_a_node_can_meld() {
        let mut g = world();
        let off = spirit(&mut g, P0, at(10, 6), "spirit");
        assert_eq!(g.meld(&[off]), Err(MeldError::NoNode));
        let walker = spirit(&mut g, P0, at(6, 6), "walker");
        assert_eq!(g.meld(&[walker]), Err(MeldError::NoMelder));
        assert_eq!(g.meld(&[]), Err(MeldError::BadSelection));
        let theirs = spirit(&mut g, P1, at(6, 6), "spirit");
        assert_eq!(g.meld(&[walker, theirs]), Err(MeldError::BadSelection));
        g.map.as_mut().unwrap().node_mut(0).unwrap().warped = true;
        assert_eq!(g.meld(&[theirs]), Err(MeldError::Warped));
    }

    #[test]
    fn nodes_pay_power_by_strength_and_mastery() {
        let mut g = world();
        let s = spirit(&mut g, P0, at(6, 6), "spirit");
        g.meld(&[s]).unwrap();
        let power = i32::from(g.map.as_ref().unwrap().nodes()[0].power);
        let base = g.income(P0).power_base;
        assert_eq!(g.node_income(P0), power, "normal strength: one per square");
        assert_eq!(g.income(P1).power_base, 0);
        g.magic_strength = 2;
        assert_eq!(g.node_income(P0), power * 3 / 2, "powerful: one and a half");
        g.magic_strength = 0;
        assert_eq!(g.node_income(P0), power / 2, "weak: half");
        g.magic_strength = 1;
        g.magic_mut(P0).unwrap().retorts.insert(Retort::NodeMastery);
        assert_eq!(g.node_income(P0), power * 2);
        assert_eq!(
            g.income(P0).power_base,
            base + power,
            "node mastery doubled it"
        );
        g.map.as_mut().unwrap().node_mut(0).unwrap().warped = true;
        assert_eq!(g.node_income(P0), -5);
    }

    #[test]
    fn a_guarded_node_resists_rivals_three_times_in_four() {
        let mut g = world();
        let guard = spirit(&mut g, P0, at(6, 6), "guardian_spirit");
        g.meld(&[guard]).unwrap();
        assert!(g.map.as_ref().unwrap().nodes()[0].guardian);
        // Many rivals try; the roll is seeded, so count successes over several nodes of fresh
        // games: with a guardian about a quarter win.
        let mut wins = 0;
        for seed in 0..200 {
            let mut g2 = world_seeded(seed);
            let guard = spirit(&mut g2, P0, at(6, 6), "guardian_spirit");
            g2.meld(&[guard]).unwrap();
            let rival = spirit(&mut g2, P1, at(6, 6), "spirit");
            let events = g2.meld(&[rival]).unwrap();
            if events.iter().any(|e| matches!(e, Event::NodeMelded { .. })) {
                wins += 1;
                assert_eq!(g2.map.as_ref().unwrap().nodes()[0].owner, Some(1));
                assert!(
                    !g2.map.as_ref().unwrap().nodes()[0].guardian,
                    "their guardian is gone"
                );
            } else {
                assert_eq!(g2.map.as_ref().unwrap().nodes()[0].owner, Some(0));
            }
        }
        assert!((25..=75).contains(&wins), "about a quarter of 200: {wins}");
        // An unguarded node falls to any rival.
        let mut g3 = world();
        let mine = spirit(&mut g3, P0, at(6, 6), "spirit");
        g3.meld(&[mine]).unwrap();
        let rival = spirit(&mut g3, P1, at(6, 6), "spirit");
        g3.meld(&[rival]).unwrap();
        assert_eq!(g3.map.as_ref().unwrap().nodes()[0].owner, Some(1));
    }
}
