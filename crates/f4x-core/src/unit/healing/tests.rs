use super::*;
use crate::city::economy::{Buildings, Enchantments};
use crate::city::City;
use crate::content::Content;
use crate::game::serial::Serial;
use crate::game::testutil::content_with_magic;
use crate::map::{Grid, Map, Plane, Position};
use crate::player::PlayerId;

use crate::rules::{BuildingId, Registry, Stat};
use hecs::World;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// A world with no game: units of 120 hit points (six figures of twenty), the abilities the
/// rules read, an Animists' Guild and the Stream of Life.
struct Rig {
    content: Content,
    map: Map,
    world: World,
    next: u64,
}

impl Rig {
    fn new() -> Rig {
        let mut c = content_with_magic();
        let mut units = c.rules.units.defs().to_vec();
        let mut base = units.iter().find(|u| u.id == "walker").unwrap().clone();
        base.stats.insert(Stat::Figures, 6);
        base.stats.insert(Stat::MaxHp, 20);
        for (id, ability) in [
            ("plain", None),
            ("healer", Some("healer")),
            ("regenerator", Some("regeneration")),
        ] {
            let mut u = base.clone();
            u.id = id.into();
            u.abilities.extend(ability.map(String::from));
            units.push(u);
        }
        c.rules.units = Registry::new(units);
        let mut buildings = c.rules.buildings.defs().to_vec();
        let mut guild = buildings[0].clone();
        guild.id = "animists_guild".into();
        guild.effects = vec![];
        buildings.push(guild);
        c.rules.buildings = Registry::new(buildings);
        let mut spells = c.rules.spells.defs().to_vec();
        let mut stream = spells
            .iter()
            .find(|s| s.id == "gaias_blessing")
            .unwrap()
            .clone();
        stream.id = "stream_of_life".into();
        spells.push(stream);
        c.rules.spells = Registry::new(spells);
        let grass = c.terrains.id_of("grassland").unwrap();
        let map = Map::new(Grid::new(12, 8, grass), Grid::new(12, 8, grass));
        Rig {
            content: c,
            map,
            world: World::new(),
            next: 1,
        }
    }

    /// The entity of a player, made when it is first asked for.
    fn player(&mut self, id: PlayerId) -> hecs::Entity {
        let found = self
            .world
            .query::<&PlayerId>()
            .iter()
            .find(|(_, p)| **p == id)
            .map(|(e, _)| e);
        found.unwrap_or_else(|| self.world.spawn(crate::player::bundle(id)))
    }

    fn unit(&mut self, kind: &str, owner: PlayerId, pos: Position, hurt: u32) -> hecs::Entity {
        let id = self.content.rules.units.id_of(kind).unwrap();
        let serial = Serial(self.next);
        self.next += 1;
        let owner = self.player(owner);
        let e = crate::unit::spawn(&mut self.world, &self.content, serial, owner, pos, id);
        self.world.insert_one(e, Hurt(hurt)).unwrap();
        e
    }

    fn city(&mut self, owner: PlayerId, pos: Position) -> hecs::Entity {
        let serial = Serial(self.next);
        self.next += 1;
        let owner = self.player(owner);
        self.world.spawn((
            serial,
            City {
                name: "Town".into(),
            },
            crate::player::Owner(owner),
            pos,
            Buildings::default(),
            Enchantments::default(),
        ))
    }

    fn build(&mut self, city: hecs::Entity, id: &str) {
        let b: BuildingId = self.content.rules.buildings.id_of(id).unwrap();
        self.world.get::<&mut Buildings>(city).unwrap().0.push(b);
    }

    fn enchant(&mut self, city: hecs::Entity, id: &str, caster: PlayerId) {
        let s = self.content.rules.spells.id_of(id).unwrap();
        self.world
            .get::<&mut Enchantments>(city)
            .unwrap()
            .0
            .push((s, caster));
    }

    /// What the plan says, and how many numbers it drew.
    fn plan(&self, seed: u64) -> (Vec<(hecs::Entity, u32)>, u64) {
        let units = UnitView::new(&self.world, &self.content);
        let economy = Economy::new(&self.world, Some(&self.map), &self.content);
        let mut rng = GameRng::new(seed);
        let plan = healing(&units, &economy, &mut rng);
        (plan, rng.draws())
    }

    fn after(&self, plan: &[(hecs::Entity, u32)], unit: hecs::Entity) -> Option<u32> {
        plan.iter().find(|(u, _)| *u == unit).map(|(_, h)| *h)
    }
}

#[test]
fn a_unit_in_the_field_mends_a_twentieth_of_its_hit_points() {
    let mut r = Rig::new();
    let u = r.unit("plain", P0, at(3, 3), 50);
    let (plan, draws) = r.plan(1);
    assert_eq!(r.after(&plan, u), Some(50 - 6));
    assert_eq!(draws, 1, "one draw for the odd points");
}

#[test]
fn a_city_mends_a_tenth_and_an_animists_guild_a_sixth() {
    let mut r = Rig::new();
    let city = r.city(P0, at(5, 5));
    let u = r.unit("plain", P0, at(5, 5), 50);
    assert_eq!(r.after(&r.plan(1).0, u), Some(50 - 12));
    r.build(city, "animists_guild");
    assert_eq!(r.after(&r.plan(1).0, u), Some(50 - 20));
}

#[test]
fn a_city_of_another_owner_counts_as_the_field() {
    let mut r = Rig::new();
    let city = r.city(P1, at(5, 5));
    r.build(city, "animists_guild");
    let u = r.unit("plain", P0, at(5, 5), 50);
    assert_eq!(r.after(&r.plan(1).0, u), Some(50 - 6));
}

#[test]
fn a_healer_in_the_stack_gives_a_fifth_more_to_its_own_side_only() {
    let mut r = Rig::new();
    let _healer = r.unit("healer", P0, at(3, 3), 0);
    let mine = r.unit("plain", P0, at(3, 3), 50);
    let theirs = r.unit("plain", P1, at(3, 3), 50);
    let elsewhere = r.unit("plain", P0, at(7, 7), 50);
    let (plan, _) = r.plan(1);
    assert_eq!(r.after(&plan, mine), Some(50 - 6 - 24));
    assert_eq!(r.after(&plan, theirs), Some(50 - 6));
    assert_eq!(r.after(&plan, elsewhere), Some(50 - 6));
}

#[test]
fn regeneration_and_the_stream_of_life_heal_completely_without_a_draw() {
    let mut r = Rig::new();
    let city = r.city(P0, at(5, 5));
    let field = r.unit("regenerator", P0, at(3, 3), 70);
    let home = r.unit("plain", P0, at(5, 5), 70);
    let outside = r.unit("plain", P0, at(3, 4), 70);
    r.enchant(city, "stream_of_life", P0);
    let (plan, draws) = r.plan(1);
    assert_eq!(r.after(&plan, field), Some(0));
    assert_eq!(r.after(&plan, home), Some(0));
    assert_eq!(r.after(&plan, outside), Some(70 - 6));
    assert_eq!(draws, 1, "only the unit outside draws");
}

#[test]
fn the_unhurt_and_the_dead_do_not_mend() {
    let mut r = Rig::new();
    let well = r.unit("plain", P0, at(3, 3), 0);
    let raised = r.unit("plain", P0, at(3, 4), 50);
    crate::unit::undead::make_undead(&mut r.world, raised);
    let (plan, draws) = r.plan(1);
    assert_eq!(r.after(&plan, well), None);
    assert_eq!(r.after(&plan, raised), None, "undead cannot be healed");
    assert_eq!(draws, 0);
}
