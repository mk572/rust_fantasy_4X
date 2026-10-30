use super::*;
use crate::city::economy::Buildings;
use crate::city::City;
use crate::content::Content;
use crate::game::serial::Serial;
use crate::game::testutil::content_with_magic;
use crate::map::{Grid, Map, Plane, Position};
use crate::player::PlayerId;

use crate::rules::{BuildingId, Registry};
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

/// A world with no game: the gnoll wolf riders (twenty hit points, no abilities), the healer
/// hero and the war trolls (they regenerate), an Animists' Guild and the Stream of Life.
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
        for id in ["gnoll_wolf_riders", "healer", "troll_war_trolls"] {
            crate::game::testutil::add_real_unit(&mut units, id);
        }
        c.rules.units = Registry::new_sorted(units);
        let grass = c.terrains.id_of("grassland").expect("id_of succeeds");
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
        let id = self
            .content
            .rules
            .units
            .id_of(kind)
            .expect("id_of succeeds");
        let serial = Serial(self.next);
        self.next += 1;
        let owner = self.player(owner);
        let e = crate::unit::spawn(&mut self.world, &self.content, serial, owner, pos, id);
        self.world
            .insert_one(e, Hurt(hurt))
            .expect("insert_one succeeds");
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
        ))
    }

    fn build(&mut self, city: hecs::Entity, id: &str) {
        let b: BuildingId = self
            .content
            .rules
            .buildings
            .id_of(id)
            .expect("id_of succeeds");
        self.world
            .get::<&mut Buildings>(city)
            .expect("value succeeds")
            .0
            .push(b);
    }

    fn enchant(&mut self, city: hecs::Entity, id: &str, caster: PlayerId) {
        let s = self.content.rules.spells.id_of(id).expect("id_of succeeds");
        let player = self.player(caster);
        let wizard = self
            .world
            .spawn((crate::wizard::entity::PlayerLink(player),));
        let serial = Serial(self.next);
        self.next += 1;
        crate::spell::instance::spawn_instance(
            &mut self.world,
            s,
            wizard,
            crate::spell::instance::SpellTarget::City(city),
            serial,
        );
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
    let u = r.unit("gnoll_wolf_riders", P0, at(3, 3), 15);
    let (plan, draws) = r.plan(1);
    assert_eq!(r.after(&plan, u), Some(15 - 1));
    assert_eq!(draws, 1, "one draw for the points left over");
}

#[test]
fn a_city_mends_a_tenth_and_an_animists_guild_a_sixth() {
    let mut r = Rig::new();
    let city = r.city(P0, at(5, 5));
    let u = r.unit("gnoll_wolf_riders", P0, at(5, 5), 15);
    assert_eq!(r.after(&r.plan(1).0, u), Some(15 - 2));
    r.build(city, "animists_guild");
    assert_eq!(
        r.after(&r.plan(1).0, u),
        Some(15 - 4),
        "20 / 6 is 3, and seed 1 wins the odd point"
    );
}

#[test]
fn a_city_of_another_owner_counts_as_the_field() {
    let mut r = Rig::new();
    let city = r.city(P1, at(5, 5));
    r.build(city, "animists_guild");
    let u = r.unit("gnoll_wolf_riders", P0, at(5, 5), 15);
    assert_eq!(r.after(&r.plan(1).0, u), Some(15 - 1));
}

#[test]
fn a_healer_in_the_stack_gives_a_fifth_more_to_its_own_side_only() {
    let mut r = Rig::new();
    let _healer = r.unit("healer", P0, at(3, 3), 0);
    let mine = r.unit("gnoll_wolf_riders", P0, at(3, 3), 15);
    let theirs = r.unit("gnoll_wolf_riders", P1, at(3, 3), 15);
    let elsewhere = r.unit("gnoll_wolf_riders", P0, at(7, 7), 15);
    let (plan, _) = r.plan(1);
    assert_eq!(r.after(&plan, mine), Some(15 - 1 - 4));
    assert_eq!(r.after(&plan, theirs), Some(15 - 1));
    assert_eq!(r.after(&plan, elsewhere), Some(15 - 1));
}

#[test]
fn regeneration_and_the_stream_of_life_heal_completely_without_a_draw() {
    let mut r = Rig::new();
    let city = r.city(P0, at(5, 5));
    let field = r.unit("troll_war_trolls", P0, at(3, 3), 15);
    let home = r.unit("gnoll_wolf_riders", P0, at(5, 5), 15);
    let outside = r.unit("gnoll_wolf_riders", P0, at(3, 4), 15);
    r.enchant(city, "stream_of_life", P0);
    let (plan, draws) = r.plan(1);
    assert_eq!(r.after(&plan, field), Some(0));
    assert_eq!(r.after(&plan, home), Some(0));
    assert_eq!(r.after(&plan, outside), Some(15 - 1));
    assert_eq!(draws, 1, "only the unit outside draws");
}

#[test]
fn the_unhurt_and_the_dead_do_not_mend() {
    let mut r = Rig::new();
    let well = r.unit("gnoll_wolf_riders", P0, at(3, 3), 0);
    let raised = r.unit("gnoll_wolf_riders", P0, at(3, 4), 15);
    crate::unit::undead::make_undead(&mut r.world, raised);
    let (plan, draws) = r.plan(1);
    assert_eq!(r.after(&plan, well), None);
    assert_eq!(r.after(&plan, raised), None, "undead cannot be healed");
    assert_eq!(draws, 0);
}
