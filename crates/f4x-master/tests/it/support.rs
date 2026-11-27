//! What the replay and undo tests share: the saved fresh games of core, played through a [`Game`].

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::world::testutil::{fresh, shipped, FRESH_WORLDS};
use f4x_core::world::{Command, Event, World};
use f4x_def::rules::{UnitId, UnitKind};
use f4x_master::Game;

/// Every fresh game, with its name for a failure message.
pub fn games() -> Vec<(String, Game)> {
    FRESH_WORLDS
        .iter()
        .map(|&(w, s)| {
            let world = fresh(w, s).unwrap_or_else(|e| panic!("{e}"));
            (format!("{w} wizards, seed {s}"), Game::from(world))
        })
        .collect()
}

/// The content the games are loaded and restored against.
pub fn content() -> f4x_def::content::Content {
    shipped()
}

/// The text of a world: what a save would write.
pub fn text(w: &World) -> String {
    w.snapshot().to_text().expect("a game writes as text")
}

/// The squares round `at`, east and west wrapping, those off the top or bottom left out.
fn around(w: &World, at: Position) -> Vec<Position> {
    let map = w.map().expect("a map");
    let mut out = Vec::new();
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let y = at.y as i32 + dy;
            if (dx, dy) != (0, 0) && (0..map.height() as i32).contains(&y) {
                out.push(Position {
                    x: (at.x as i32 + dx).rem_euclid(map.width() as i32) as u32,
                    y: y as u32,
                    plane: at.plane,
                });
            }
        }
    }
    out
}

/// One round of play without an AI: every unit of the first `limit` tries one step to a
/// neighbouring square (the first the game accepts), then the turn ends. The commands go through
/// the game, so they are recorded and can be undone.
pub fn busy_round(game: &mut Game, limit: usize) {
    for u in game.units().all_units().into_iter().take(limit) {
        let Some(at) = game.units().unit_position(u) else {
            continue;
        };
        for to in around(game.world(), at) {
            let out = game.command(Command::MoveStack { units: vec![u], to });
            if !out.iter().any(|e| matches!(e, Event::Rejected(_))) {
                break;
            }
        }
    }
    game.play_round();
}

/// A command that changes the game, draws no random number and explores nothing: the tax rate of a
/// wizard. The world as it was before comes back.
pub fn a_change_that_can_be_taken_back(game: &mut Game) -> Option<String> {
    let player = game
        .player_ids()
        .into_iter()
        .find(|&p| !game.is_neutral_player(p) && game.wizard(p).is_some())?;
    let before = text(game.world());
    for rate in 0..=10u8 {
        let out = game.command(Command::SetTaxRate { player, rate });
        if !out.iter().any(|e| matches!(e, Event::Rejected(_))) && text(game.world()) != before {
            return Some(before);
        }
    }
    None
}

/// Put everyone on the farms, so that balancing them changes something.
pub fn all_farm(game: &mut Game) {
    for (city, ..) in game.economy().cities() {
        game.command(Command::SetFarmers {
            city,
            farmers: u32::MAX,
        });
    }
}

/// Two soldiers of `a` beside two of `b`, who is the human.
pub struct Field {
    pub game: Game,
    pub a: PlayerId,
    pub b: PlayerId,
    pub attackers: Vec<hecs::Entity>,
    pub defenders: Vec<hecs::Entity>,
}

pub fn field() -> Field {
    let (_, mut game) = games().remove(0);
    let wizards: Vec<PlayerId> = game
        .turn_players()
        .iter()
        .copied()
        .filter(|&p| game.is_wizard(p))
        .collect();
    let (a, b) = (wizards[0], wizards[1]);
    game.world_mut().set_human(Some(b));
    let map = game.map().expect("a map");
    let free = |p: Position| {
        game.units().units_at(p).is_empty()
            && game.economy().terrain_class_at(p) == Some(TerrainClass::Grassland)
            && game.economy().city_at(p).is_none()
    };
    let plane = game
        .units()
        .player_units(a)
        .first()
        .and_then(|&u| game.units().unit_position(u))
        .expect("a unit")
        .plane;
    let (here, there) = (1..map.height() - 1)
        .flat_map(|y| (1..map.width() - 1).map(move |x| Position { x, y, plane }))
        .map(|p| (p, Position { x: p.x + 1, ..p }))
        .find(|&(p, q)| free(p) && free(q))
        .expect("two squares of grassland");
    let normal = game.content().rules.units.defs().iter();
    let kind = normal
        .filter(|d| d.kind == UnitKind::Normal && d.race.is_some())
        .map(|d| UnitId::new(d.num))
        .next()
        .expect("a unit of a race");
    let mut stack = |owner, pos| -> Vec<hecs::Entity> {
        (0..2)
            .map(|_| game.world_mut().spawn_unit(owner, pos, kind))
            .collect()
    };
    let attackers = stack(a, here);
    let defenders = stack(b, there);
    Field {
        game,
        a,
        b,
        attackers,
        defenders,
    }
}
