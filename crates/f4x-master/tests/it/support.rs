//! What the tests share: the saved fresh games of core, as worlds and played through a [`Game`].

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::world::testutil::sweep::around;
use f4x_core::world::testutil::{fresh, shipped, Table, FRESH_WORLDS};
use f4x_core::world::{Command, Event, World};
use f4x_def::half_int::HalfInt;
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

/// One round of play without an AI: every unit of the first `limit` tries one step to a
/// neighbouring square (the first the game accepts), then the turn ends. The commands go through
/// the game, so they are recorded and can be undone. The events come back, so two games played the
/// same way can be compared.
pub fn busy_round(game: &mut Game, limit: usize) -> Vec<Event> {
    let mut events = Vec::new();
    for u in game.units().all_units().into_iter().take(limit) {
        let Some(at) = game.units().unit_position(u) else {
            continue;
        };
        for to in around(game.world(), at) {
            let out = game.command(Command::MoveStack { units: vec![u], to });
            let refused = out.iter().any(|e| matches!(e, Event::Rejected(_)));
            events.extend(out);
            if !refused {
                break;
            }
        }
    }
    events.extend(game.play_round());
    events
}

/// Every fresh game as a world alone, with its name for a failure message.
pub fn worlds() -> Vec<(String, Table)> {
    FRESH_WORLDS
        .iter()
        .map(|&(w, s)| {
            let world = fresh(w, s).unwrap_or_else(|e| panic!("{e}"));
            (format!("{w} wizards, seed {s}"), world)
        })
        .collect()
}

/// [`text`] and the numbers drawn so far: all that playing changes, for a test that says two games
/// are the same or that nothing happened to one.
pub fn state(game: &Game) -> String {
    format!("{}draws {}\n", text(game), game.draws())
}

/// The events as text with the entity handles blanked (`170v1` becomes `E`): a loaded game numbers
/// its entities afresh, so two games that are the same differ in these alone. Creation numbers (the
/// saved text) are what is compared exactly.
pub fn shape(events: &[Event]) -> String {
    let text = format!("{events:?}");
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i > start && i < chars.len() && chars[i] == 'v' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && !chars[start.saturating_sub(1)].is_alphanumeric() {
                out.push('E');
                i = j;
                continue;
            }
        }
        i = start;
        out.push(chars[i]);
        i += 1;
    }
    out
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

/// The players that keep a treasury: the wizards.
pub fn wizards(w: &World) -> Vec<PlayerId> {
    w.player_ids()
        .into_iter()
        .filter(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
        .collect()
}

pub fn gold(w: &World, p: PlayerId) -> HalfInt {
    w.player(p).expect("a player").treasury().gold
}

pub fn mana(w: &World, p: PlayerId) -> HalfInt {
    w.player(p).expect("a player").treasury().mana
}

/// No city has anything in hand, so that no item is finished and changes the gold at a turn's end.
pub fn idle_cities(w: &mut World) {
    for (city, _, _) in w.economy().cities() {
        w.scenario_clear_production(city);
    }
}
