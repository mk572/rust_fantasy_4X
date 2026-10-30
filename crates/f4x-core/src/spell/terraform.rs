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

pub const RAISE_VOLCANO: &str = "raise_volcano";
pub const CHANGE_TERRAIN: &str = "change_terrain";

/// Each building in a city under a new volcano collapses with this chance (percent).
const COLLAPSE_PERCENT: u32 = 15;
/// A new volcano gets a mineral on its square with this chance (percent).
const MINERAL_PERCENT: u32 = 20;
/// A volcano cools into mountains with this chance each turn (the original's code rolls
/// `Random(100) < 3`, which is two in a hundred; its comment says three).
const COOL_PERCENT: u32 = 2;
/// Armageddon's volcano under a city: the chance (percent) each building, unit and citizen is lost.
const ARMAGEDDON_CITY_PERCENT: u32 = 25;

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
        let Some(mountains) = self
            .content
            .terrains
            .first_of_class(TerrainClass::Mountains, at.plane)
        else {
            return Vec::new();
        };
        let mut events = Vec::new();
        let mineral = if rng.next_u32() % 100 < MINERAL_PERCENT {
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
        self.spawn_volcano(owner, at);
        for ship in ships {
            self.remove_unit(ship, crate::hero::Fate::Slain);
        }
        for unit in self.units().units_at(at) {
            let _ = self
                .world
                .remove_one::<crate::unit::transport::Aboard>(unit);
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
            b.0.retain(|_| rng.next_u32() % 100 >= COLLAPSE_PERCENT);
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
        let Some(spell) = self.content.rules.spells.id_of("armageddon") else {
            return Vec::new();
        };
        let Ok(at) = self.world.get::<&Position>(city).map(|p| *p) else {
            return Vec::new();
        };
        let victim = self.economy().city_owner(city);
        let mut buildings_lost = 0;
        if let Ok(mut b) = self.world.get::<&mut Buildings>(city) {
            let before = b.0.len();
            b.0.retain(|_| rng.next_u32() % 100 >= ARMAGEDDON_CITY_PERCENT);
            buildings_lost = (before - b.0.len()) as u32;
        }
        let mut units_lost = 0;
        for unit in self.units().units_at(at) {
            if rng.next_u32() % 100 < ARMAGEDDON_CITY_PERCENT {
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
                .filter(|_| rng.next_u32() % 100 < ARMAGEDDON_CITY_PERCENT)
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
            if rng.next_u32() % 100 < COOL_PERCENT {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_magic;
    use crate::game::Command;
    use crate::map::Position;
    use crate::map::{Grid, Map, Plane};
    use crate::rules::exec::Target;
    use crate::testdir::TestResult;

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
        let grass = c.terrains.id_of("grassland").expect("id_of succeeds");
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.add_player(P0, "W");
        g.treasury_mut(P0).expect("treasury_mut succeeds").mana = 1000;
        g.init_wizard(P0, &[("life", 2)], &[], None)
            .expect("init_wizard succeeds");
        g
    }

    fn cast(g: &mut Game, id: &str, pos: Position) -> Vec<Event> {
        let s = g.content.rules.spells.id_of(id).expect("id_of succeeds");
        g.wizard_mut(P0)
            .expect("value succeeds")
            .get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(s);
        g.wizard_mut(P0)
            .expect("value succeeds")
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 500;
        g.treasury_mut(P0).expect("treasury_mut succeeds").mana = 1000;
        g.apply(Command::CastSpell {
            player: P0,
            spell: s,
            target: Some(Target::Tile(pos)),
        })
    }

    #[test]
    fn a_volcano_makes_mountains_destroys_minerals_and_adds_power() -> TestResult {
        let mut g = world();
        let gold = g
            .content
            .features
            .id_of("gold")
            .ok_or("gold is not in the content")?;
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set_mineral(4, 4, Some(gold));
        let before = g.income(P0).power_base;
        let events = cast(&mut g, "raise_volcano", at(4, 4));
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::VolcanoRaised { .. })));
        let grid = g
            .map
            .as_ref()
            .ok_or("as_ref is missing")?
            .plane(Plane::Arcanus);
        assert_eq!(g.economy().volcano_at(at(4, 4)), Some(PlayerId(0)));
        assert_eq!(grid.mineral(4, 4), None);
        assert_eq!(
            g.economy().terrain_class_at(at(4, 4)),
            Some(TerrainClass::Mountains)
        );
        assert_eq!(g.volcano_count(P0), 1);
        assert_eq!(g.income(P0).power_base, before + 1);
        Ok(())
    }

    #[test]
    fn a_volcano_cannot_rise_on_a_river_and_a_node_cannot_be_changed() -> TestResult {
        let mut g = world();
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set_river(4, 4, true);
        assert_eq!(
            g.check_raise_volcano(at(4, 4)),
            Err(TargetError::VolcanoOnRiver)
        );
        for class in [
            TerrainClass::SorceryNode,
            TerrainClass::NatureNode,
            TerrainClass::ChaosNode,
        ] {
            let id = g
                .content
                .terrains
                .first_of_class(class, Plane::Arcanus)
                .ok_or("first_of_class is missing")?;
            g.map
                .as_mut()
                .ok_or("as_mut is missing")?
                .plane_mut(Plane::Arcanus)
                .set(6, 6, id);
            assert_eq!(
                g.change_terrain_result(at(6, 6)),
                Err(TargetError::NodeFixed)
            );
        }
        Ok(())
    }

    #[test]
    fn volcanoes_cannot_rise_on_mountains_hills_or_nodes() -> TestResult {
        let mut g = world();
        let hills = g
            .content
            .terrains
            .first_of_class(TerrainClass::Hills, Plane::Arcanus)
            .ok_or("first_of_class is missing")?;
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set(2, 2, hills);
        g.put_feature(at(3, 3), "sorcery_node");
        assert_eq!(
            g.check_raise_volcano(at(2, 2)),
            Err(TargetError::VolcanoOnHill)
        );
        assert_eq!(
            g.check_raise_volcano(at(3, 3)),
            Err(TargetError::VolcanoOnSite)
        );
        assert_eq!(g.check_raise_volcano(at(5, 5)), Ok(()));
        let s = g
            .content
            .rules
            .spells
            .id_of("raise_volcano")
            .ok_or("raise_volcano is not in the content")?;
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(s);
        g.wizard_mut(P0)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 500;
        let events = g.apply(Command::CastSpell {
            player: P0,
            spell: s,
            target: Some(Target::Tile(at(2, 2))),
        });
        assert!(
            events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        assert_eq!(
            g.player(P0).ok_or("player is missing")?.treasury().mana,
            1000,
            "refused before paying"
        );
        Ok(())
    }

    #[test]
    fn a_volcano_under_a_city_can_collapse_buildings() -> TestResult {
        let mut g = world();
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(8, 8),
            name: "T".into(),
            race: g.content.rules.races.id_of("high_elf"),
            population: 3,
        });
        let city = g.economy().city_at(at(8, 8)).ok_or("city_at is missing")?;
        let ids: Vec<_> = (0..g.content.rules.buildings.defs().len().min(8))
            .map(|i| crate::rules::BuildingId::new(i as u16))
            .collect();
        g.world.get::<&mut Buildings>(city)?.0 = ids.clone();
        let mut rng = rand_pcg::Pcg64Mcg::new(1);
        let events = g.raise_volcano(P0, at(8, 8), &mut rng);
        let left = g.world.get::<&Buildings>(city)?.0.len();
        assert!(left <= ids.len());
        assert_eq!(
            events
                .iter()
                .any(|e| matches!(e, Event::BuildingsCollapsed { .. })),
            left < ids.len()
        );
        Ok(())
    }

    #[test]
    fn change_terrain_follows_the_help_table() -> TestResult {
        let mut g = world();
        let terr = |g: &Game, c| {
            g.content
                .terrains
                .first_of_class(c, Plane::Arcanus)
                .expect("value succeeds")
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
                .ok_or("as_mut is missing")?
                .plane_mut(Plane::Arcanus)
                .set(6, 6, id);
            assert_eq!(g.change_terrain_result(at(6, 6)), Ok(to), "{from:?}");
        }
        for class in [TerrainClass::Ocean, TerrainClass::Tundra] {
            let id = terr(&g, class);
            g.map
                .as_mut()
                .ok_or("as_mut is missing")?
                .plane_mut(Plane::Arcanus)
                .set(6, 6, id);
            assert_eq!(
                g.change_terrain_result(at(6, 6)),
                Err(TargetError::TerrainFixed)
            );
        }
        Ok(())
    }

    #[test]
    fn change_terrain_turns_a_volcano_into_plain_mountains() {
        let mut g = world();
        cast(&mut g, "raise_volcano", at(4, 4));
        assert_eq!(g.volcano_count(P0), 1);
        cast(&mut g, "change_terrain", at(4, 4));
        assert_eq!(g.volcano_count(P0), 0);
        assert_eq!(
            g.economy().terrain_class_at(at(4, 4)),
            Some(TerrainClass::Mountains)
        );
        cast(&mut g, "change_terrain", at(4, 4));
        assert_eq!(
            g.economy().terrain_class_at(at(4, 4)),
            Some(TerrainClass::Hills)
        );
    }
}
