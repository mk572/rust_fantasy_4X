//! Turn a [`BattleScenario`] into a game with its battle begun.

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;
use f4x_def::content::Content;
use f4x_newgame::setup::{new_game, NewGame};

use f4x_core::world::{Command, Event, World};

use crate::scenario::{BattleScenario, Ground, UnitPick, WizardSpec};

/// The attacking wizard and the defending one.
pub const ATTACKER: PlayerId = PlayerId(0);
pub const DEFENDER: PlayerId = PlayerId(1);

fn walkable(world: &World, at: Position) -> bool {
    world.economy().city_at(at).is_none()
        && world
            .economy()
            .terrain_class_at(at)
            .is_some_and(|c| c != TerrainClass::Ocean && !c.is_mountain())
}

fn around(at: Position) -> impl Iterator<Item = Position> {
    const D: [(i32, i32); 8] = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
    ];
    D.into_iter().filter_map(move |(dx, dy)| {
        let x = at.x as i32 + dx;
        let y = at.y as i32 + dy;
        (x >= 0 && y >= 0).then_some(Position {
            plane: at.plane,
            x: x as u32,
            y: y as u32,
        })
    })
}

/// Two neighbouring squares of open land, found by looking round `from`.
fn field_pair(world: &World, from: Position, start: usize) -> Option<(Position, Position)> {
    let mut rings = vec![from];
    for r in 1..8u32 {
        for dy in -(r as i32)..=r as i32 {
            for dx in -(r as i32)..=r as i32 {
                if dx.unsigned_abs().max(dy.unsigned_abs()) == r {
                    let (x, y) = (from.x as i32 + dx, from.y as i32 + dy);
                    if x >= 0 && y >= 0 {
                        rings.push(Position {
                            plane: from.plane,
                            x: x as u32,
                            y: y as u32,
                        });
                    }
                }
            }
        }
    }
    let n = rings.len();
    (0..n).find_map(|k| {
        let a = rings[(k + start) % n];
        if !walkable(world, a) {
            return None;
        }
        around(a)
            .filter(|&b| walkable(world, b))
            .find(|b| b.x == a.x + 1 || b.x + 1 == a.x)
            .map(|b| (a, b))
    })
}

fn set_wizard(world: &mut World, player: PlayerId, spec: &Option<WizardSpec>) {
    let ids: Vec<_> = spec
        .iter()
        .flat_map(|w| w.spells.iter())
        .filter_map(|&s| world.content().rules.spells.id_numbered(s))
        .collect();
    let (mana, points) = spec
        .as_ref()
        .map_or((0, 0), |w| (w.mana, w.skill_points as i32));
    if let Some(t) = world.treasury_mut(player) {
        t.mana = f4x_def::half_int::HalfInt::from_int(mana);
    }
    let Some(mut w) = world.wizard_mut(player) else {
        return;
    };
    w.get_mut::<f4x_core::wizard::sheet::KnownSpells>()
        .known
        .clear();
    w.get_mut::<f4x_core::wizard::sheet::Skill>().points = points;
    if spec.is_some() {
        w.get_mut::<f4x_core::wizard::sheet::KnownSpells>()
            .known
            .extend(ids);
    }
}

fn spawn_army(
    world: &mut World,
    owner: PlayerId,
    at: Position,
    army: &[UnitPick],
) -> Vec<hecs::Entity> {
    let mut out = Vec::new();
    for pick in army {
        let Some(id) = world.content().rules.units.id_numbered(pick.unit) else {
            continue;
        };
        let u = world.spawn_unit(owner, at, id);
        world.scenario_set_experience(u, pick.xp);
        world.scenario_set_hurt(u, pick.hurt);
        out.push(u);
    }
    out
}

/// Build the game of a scenario and begin its battle: the attacker is wizard 0, the defender
/// wizard 1.
pub fn build(content: &Content, s: &BattleScenario) -> Result<World, String> {
    f4x_ai::battle::install();
    let mut world = new_game(
        content,
        &NewGame {
            seed: s.seed,
            all_ai: true,
            ..NewGame::default()
        },
    )?;
    world.set_undo(false);
    for u in world.units().all_units() {
        world.scenario_remove_unit(u);
    }
    set_wizard(&mut world, ATTACKER, &s.attacker_wizard);
    set_wizard(&mut world, DEFENDER, &s.defender_wizard);
    let capital = world
        .economy()
        .cities()
        .into_iter()
        .find(|(_, c, _)| c.owner == DEFENDER)
        .map(|(e, _, p)| (e, p))
        .ok_or("the defender has no city")?;
    let (from, to) = match s.ground {
        Ground::City { walls } => {
            let (city, at) = capital;
            let wall = f4x_def::rules::BuildingId::new(35); // city_walls
            world.scenario_set_building(city, wall, walls);
            let from = around(at)
                .find(|&p| walkable(&world, p))
                .ok_or("no land beside the city")?;
            (from, at)
        }
        Ground::Field => field_pair(&world, capital.1, (s.seed % 97) as usize)
            .ok_or("no open ground near the city")?,
    };
    debug_assert!(from.plane == Plane::Arcanus || from.plane == Plane::Myrror);
    let attackers = spawn_army(&mut world, ATTACKER, from, &s.attackers);
    let defenders = spawn_army(&mut world, DEFENDER, to, &s.defenders);
    let events = world.apply(Command::BeginBattle {
        attackers,
        defenders,
    });
    if let Some(Event::Rejected(r)) = events.iter().find(|e| matches!(e, Event::Rejected(_))) {
        return Err(format!("the battle was refused: {r:?}"));
    }
    if !world.in_battle() {
        return Err("no battle began".into());
    }
    Ok(world)
}

/// Where a situation fight is held instead of the scenario's own ground (the situations of
/// `examples/situations.rs`): on a terrain class, or at a site of the map by its feature number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spot {
    Class(TerrainClass),
    Site(u16),
}

fn neighbour_land(world: &World, at: Position) -> Option<Position> {
    around(at).find(|&p| walkable(world, p) && world.economy().lair_at(p).is_none())
}

/// A square of the class with land beside it, the `nth` one found (wrapping), for the defenders.
fn square_of_class(world: &World, class: TerrainClass, nth: usize) -> Option<(Position, Position)> {
    let map = world.map()?;
    let mut found = Vec::new();
    for plane in [Plane::Arcanus, Plane::Myrror] {
        for y in 0..map.height() {
            for x in 0..map.width() {
                let at = Position { plane, x, y };
                if world.economy().terrain_class_at(at) == Some(class)
                    && world.economy().city_at(at).is_none()
                    && world.economy().lair_at(at).is_none()
                {
                    if let Some(from) = neighbour_land(world, at) {
                        found.push((from, at));
                    }
                }
            }
        }
    }
    (!found.is_empty()).then(|| found[nth % found.len()])
}

/// The `nth` intact site with the feature number, and land beside it.
fn square_of_site(world: &World, feature: u16, nth: usize) -> Option<(Position, Position)> {
    let found: Vec<(Position, Position)> = world
        .economy()
        .lairs()
        .into_iter()
        .filter(|l| l.intact && l.site.0 == feature && !l.guards.is_empty())
        .filter_map(|l| neighbour_land(world, l.pos).map(|from| (from, l.pos)))
        .collect();
    (!found.is_empty()).then(|| found[nth % found.len()])
}

/// Like [`build`], but the fight is held on `spot` (the nth such square of the map), and with
/// `hero` the attackers have a hero carrying an item. At a site the defenders are the site's own
/// guards and the scenario's defenders are not used. `Err` when the map has no such square.
pub fn build_at(
    content: &Content,
    s: &BattleScenario,
    spot: &Spot,
    nth: usize,
    hero: bool,
) -> Result<World, String> {
    f4x_ai::battle::install();
    let mut world = new_game(
        content,
        &NewGame {
            seed: s.seed,
            all_ai: true,
            ..NewGame::default()
        },
    )?;
    world.set_undo(false);
    for u in world.units().all_units() {
        world.scenario_remove_unit(u);
    }
    set_wizard(&mut world, ATTACKER, &s.attacker_wizard);
    set_wizard(&mut world, DEFENDER, &s.defender_wizard);
    let (from, to) = match spot {
        Spot::Class(class) => square_of_class(&world, *class, nth),
        Spot::Site(feature) => square_of_site(&world, *feature, nth),
    }
    .ok_or("the map has no such square")?;
    let mut attackers = spawn_army(&mut world, ATTACKER, from, &s.attackers);
    if hero {
        attackers.extend(spawn_hero_with_item(&mut world, from, s.seed));
    }
    let events = match spot {
        Spot::Class(_) => {
            let defenders = spawn_army(&mut world, DEFENDER, to, &s.defenders);
            world.apply(Command::BeginBattle {
                attackers,
                defenders,
            })
        }
        Spot::Site(_) => world.apply(Command::EnterSite {
            units: attackers,
            to,
        }),
    };
    if let Some(Event::Rejected(r)) = events.iter().find(|e| matches!(e, Event::Rejected(_))) {
        return Err(format!("the battle was refused: {r:?}"));
    }
    if !world.in_battle() {
        return Err("no battle began".into());
    }
    Ok(world)
}

/// A free hero (the `seed`th kind that is available) joins the attacker at `at`, with a random
/// item in the first slot it fits.
fn spawn_hero_with_item(world: &mut World, at: Position, seed: u64) -> Option<hecs::Entity> {
    use f4x_core::item::armory::ItemSpot;
    let kinds = world.units().hero_states().len();
    let hero = (0..kinds)
        .map(|k| (k + seed as usize) % kinds)
        .find_map(|k| world.spawn_hero(ATTACKER, at, k).ok())?;
    let item = {
        let rules = world.content().rules.clone();
        let mut taken = Vec::new();
        let mut rng = f4x_core::world::rng::GameRng::new(seed ^ 0x1713);
        f4x_core::item::make::make_item(
            &mut rng,
            &rules,
            f4x_core::item::make::Grade::Any,
            0,
            &mut taken,
        )
    };
    if let f4x_core::item::armory::Placed::Vault(i) = world.give_item(ATTACKER, item) {
        for slot in 0..3 {
            if world
                .move_item(
                    ATTACKER,
                    ItemSpot::Vault(i),
                    ItemSpot::Hero(hero, slot),
                    false,
                )
                .is_ok()
            {
                break;
            }
        }
    }
    Some(hero)
}
