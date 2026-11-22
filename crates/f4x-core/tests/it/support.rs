//! What the tests share: the fresh games the matrix runs over.

use f4x_core::game::testutil::fresh;
use f4x_core::game::Game;

/// The saved fresh games: (computer wizards, seed). The files are `tests/saves/fresh_w*_s*.save`.
pub const WORLDS: [(u8, u64); 4] = [(2, 7), (3, 3), (3, 7), (3, 11)];

/// Every fresh game of the matrix, with its name for a failure message.
pub fn worlds() -> Vec<(String, Game)> {
    WORLDS
        .iter()
        .map(|&(w, s)| {
            let game = fresh(w, s).unwrap_or_else(|e| panic!("{e}"));
            (format!("{w} wizards, seed {s}"), game)
        })
        .collect()
}

use f4x_core::game::{Command, Event};
use f4x_core::map::Position;

/// The content the games are loaded and restored against.
pub fn content() -> f4x_def::content::Content {
    f4x_core::game::testutil::shipped()
}

/// The text of a game: what a save would write (the checksum line aside, which only follows from it).
pub fn text(g: &Game) -> String {
    g.snapshot().to_text().expect("a game writes as text")
}

/// The eight squares round `at`, east and west wrapping, those off the top or bottom left out.
pub fn around(g: &Game, at: Position) -> Vec<Position> {
    let map = g.map().expect("a map");
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
/// neighbouring square (the first the game accepts), then the turn ends. The events come back, so two
/// games played the same way can be compared. A scripted walk, not a player: it explores, draws
/// random numbers and fights, which is what the persistence tests need to have happened.
pub fn busy_round(g: &mut Game, limit: usize) -> Vec<Event> {
    let mut events = Vec::new();
    for u in g.units().all_units().into_iter().take(limit) {
        let Some(at) = g.units().unit_position(u) else {
            continue;
        };
        for to in around(g, at) {
            let out = g.apply(Command::MoveStack { units: vec![u], to });
            let refused = out.iter().any(|e| matches!(e, Event::Rejected(_)));
            events.extend(out);
            if !refused {
                break;
            }
        }
    }
    events.extend(g.apply(Command::EndTurn));
    events
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
