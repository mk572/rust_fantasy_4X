//! Spells that reshape a map square: Raise Volcano and Change Terrain.
//!
//! A volcano is a mountain square with an owner (the wizard who raised it); the owner gets
//! one point of magic power per volcano each turn. Change Terrain turns a volcano back into
//! plain mountains.

use rand::RngCore;

use crate::city::{PlayerId, Position};
use crate::content::{FeatureKind, TerrainClass};
use crate::economy::Buildings;
use crate::{Event, Game};

pub const RAISE_VOLCANO: &str = "raise_volcano";
pub const CHANGE_TERRAIN: &str = "change_terrain";

/// Each building in a city under a new volcano collapses with this chance (percent).
const COLLAPSE_PERCENT: u32 = 15;

impl Game {
    /// Volcanoes `player` owns; each adds one to their magic power.
    pub fn volcano_count(&self, player: PlayerId) -> i32 {
        let Some(map) = self.map.as_ref() else {
            return 0;
        };
        crate::map::Plane::ALL
            .iter()
            .map(|&p| {
                map.plane(p)
                    .volcanoes()
                    .iter()
                    .filter(|&&v| v == player.0 + 1)
                    .count() as i32
            })
            .sum()
    }

    /// A volcano may rise on land that is not mountains or hills, and not on a node or any
    /// other site (minerals are fine: they are destroyed).
    pub fn can_raise_volcano(&self, at: Position) -> bool {
        let Some(class) = self.terrain_class_at(at) else {
            return false;
        };
        if matches!(
            class,
            TerrainClass::Ocean | TerrainClass::Mountains | TerrainClass::Hills
        ) {
            return false;
        }
        let Some(map) = self.map.as_ref() else {
            return false;
        };
        map.plane(at.plane)
            .feature(at.x as i32, at.y as i32)
            .and_then(|f| self.content.features.get(f))
            .is_none_or(|f| f.kind == FeatureKind::Mineral)
    }

    /// What Change Terrain turns the square into, or `None` if it is not a valid target:
    /// deserts, forests, hills and swamps become grassland, grassland becomes forest,
    /// mountains become hills, and a volcano becomes plain mountains.
    pub fn change_terrain_result(&self, at: Position) -> Option<TerrainClass> {
        let class = self.terrain_class_at(at)?;
        match class {
            TerrainClass::Desert
            | TerrainClass::Forest
            | TerrainClass::Hills
            | TerrainClass::Swamp => Some(TerrainClass::Grassland),
            TerrainClass::Grassland => Some(TerrainClass::Forest),
            TerrainClass::Mountains => Some(TerrainClass::Hills),
            TerrainClass::Ocean | TerrainClass::Tundra => None,
        }
    }

    fn volcano_at(&self, at: Position) -> Option<u8> {
        self.map
            .as_ref()?
            .plane(at.plane)
            .volcano(at.x as i32, at.y as i32)
    }

    pub(crate) fn raise_volcano(
        &mut self,
        owner: PlayerId,
        at: Position,
        rng: &mut dyn RngCore,
    ) -> Vec<Event> {
        if !self.can_raise_volcano(at) {
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
        if let Some(map) = self.map.as_mut() {
            let grid = map.plane_mut(at.plane);
            grid.set(at.x, at.y, mountains);
            grid.set_feature(at.x, at.y, None);
            grid.set_volcano(at.x, at.y, Some(owner.0));
        }
        events.push(Event::VolcanoRaised {
            player: owner,
            pos: at,
        });
        if let Some(city) = self.city_at(at) {
            let mut lost = 0;
            if let Ok(mut b) = self.world.get::<&mut Buildings>(city) {
                let before = b.0.len();
                b.0.retain(|_| rng.next_u32() % 100 >= COLLAPSE_PERCENT);
                lost = before - b.0.len();
            }
            if lost > 0 {
                events.push(Event::BuildingsCollapsed {
                    city,
                    count: lost as u32,
                });
            }
        }
        events
    }

    pub(crate) fn change_terrain_spell(&mut self, at: Position) -> Vec<Event> {
        let Some(new) = self.change_terrain_result(at) else {
            return Vec::new();
        };
        // A volcano is mountains with an owner: removing the owner is the whole change.
        if self.volcano_at(at).is_some() {
            if let Some(map) = self.map.as_mut() {
                map.plane_mut(at.plane).set_volcano(at.x, at.y, None);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::exec::Target;
    use crate::testutil::content_with_magic;
    use crate::Command;

    const P0: PlayerId = PlayerId(0);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn world() -> Game {
        let c = content_with_magic();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        let mut p = Player::new(P0, "W");
        p.mana = 1000;
        g.players.push(p);
        g.init_wizard(P0, &[("life", 2)], &[], None).unwrap();
        g
    }

    fn cast(g: &mut Game, id: &str, pos: Position) -> Vec<Event> {
        let s = g.content.rules.spells.id_of(id).unwrap();
        g.magic_mut(P0).unwrap().known.insert(s);
        g.magic_mut(P0).unwrap().skill_left = 500;
        g.player_mut(P0).unwrap().mana = 1000;
        g.apply(Command::CastSpell {
            player: P0,
            spell: s,
            target: Some(Target::Tile(pos)),
        })
    }

    #[test]
    fn a_volcano_makes_mountains_destroys_minerals_and_adds_power() {
        let mut g = world();
        let gold = g.content.features.id_of("gold").unwrap();
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_feature(4, 4, Some(gold));
        let before = g.income(P0).power_base;
        let events = cast(&mut g, "raise_volcano", at(4, 4));
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::VolcanoRaised { .. })));
        let grid = g.map.as_ref().unwrap().plane(Plane::Arcanus);
        assert_eq!(grid.volcano(4, 4), Some(0));
        assert_eq!(grid.feature(4, 4), None);
        assert_eq!(g.terrain_class_at(at(4, 4)), Some(TerrainClass::Mountains));
        assert_eq!(g.volcano_count(P0), 1);
        assert_eq!(g.income(P0).power_base, before + 1);
    }

    #[test]
    fn volcanoes_cannot_rise_on_mountains_hills_or_nodes() {
        let mut g = world();
        let hills = g
            .content
            .terrains
            .first_of_class(TerrainClass::Hills, Plane::Arcanus)
            .unwrap();
        let node = g.content.features.id_of("node").unwrap();
        {
            let grid = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
            grid.set(2, 2, hills);
            grid.set_feature(3, 3, Some(node));
        }
        assert!(!g.can_raise_volcano(at(2, 2)));
        assert!(!g.can_raise_volcano(at(3, 3)));
        assert!(g.can_raise_volcano(at(5, 5)));
        let s = g.content.rules.spells.id_of("raise_volcano").unwrap();
        g.magic_mut(P0).unwrap().known.insert(s);
        g.magic_mut(P0).unwrap().skill_left = 500;
        let events = g.apply(Command::CastSpell {
            player: P0,
            spell: s,
            target: Some(Target::Tile(at(2, 2))),
        });
        assert!(
            events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        assert_eq!(g.player(P0).unwrap().mana, 1000, "refused before paying");
    }

    #[test]
    fn a_volcano_under_a_city_can_collapse_buildings() {
        let mut g = world();
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(8, 8),
            name: "T".into(),
            race: g.content.rules.races.id_of("elf"),
            population: 3,
        });
        let city = g.city_at(at(8, 8)).unwrap();
        let ids: Vec<_> = (0..g.content.rules.buildings.defs().len().min(8))
            .map(|i| crate::rules::BuildingId::new(i as u16))
            .collect();
        g.world.get::<&mut Buildings>(city).unwrap().0 = ids.clone();
        let mut rng = rand_pcg::Pcg64Mcg::new(1);
        let events = g.raise_volcano(P0, at(8, 8), &mut rng);
        let left = g.world.get::<&Buildings>(city).unwrap().0.len();
        assert!(left <= ids.len());
        assert_eq!(
            events
                .iter()
                .any(|e| matches!(e, Event::BuildingsCollapsed { .. })),
            left < ids.len()
        );
    }

    #[test]
    fn change_terrain_follows_the_help_table() {
        let mut g = world();
        let terr = |g: &Game, c| {
            g.content
                .terrains
                .first_of_class(c, Plane::Arcanus)
                .unwrap()
        };
        for (from, to) in [
            (TerrainClass::Desert, TerrainClass::Grassland),
            (TerrainClass::Forest, TerrainClass::Grassland),
            (TerrainClass::Hills, TerrainClass::Grassland),
            (TerrainClass::Swamp, TerrainClass::Grassland),
            (TerrainClass::Grassland, TerrainClass::Forest),
            (TerrainClass::Mountains, TerrainClass::Hills),
        ] {
            let id = terr(&g, from);
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set(6, 6, id);
            assert_eq!(g.change_terrain_result(at(6, 6)), Some(to), "{from:?}");
        }
        for class in [TerrainClass::Ocean, TerrainClass::Tundra] {
            let id = terr(&g, class);
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set(6, 6, id);
            assert_eq!(g.change_terrain_result(at(6, 6)), None);
        }
    }

    #[test]
    fn change_terrain_turns_a_volcano_into_plain_mountains() {
        let mut g = world();
        cast(&mut g, "raise_volcano", at(4, 4));
        assert_eq!(g.volcano_count(P0), 1);
        cast(&mut g, "change_terrain", at(4, 4));
        assert_eq!(g.volcano_count(P0), 0);
        assert_eq!(g.terrain_class_at(at(4, 4)), Some(TerrainClass::Mountains));
        cast(&mut g, "change_terrain", at(4, 4));
        assert_eq!(g.terrain_class_at(at(4, 4)), Some(TerrainClass::Hills));
    }
}
