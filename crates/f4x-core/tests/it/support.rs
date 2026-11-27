//! What the tests share: the fresh games the matrix runs over.

use f4x_core::map::Position;
use f4x_core::world::testutil::{fresh, Table};
use f4x_core::world::{Command, Event, World};

/// The saved fresh games: (computer wizards, seed). The files are `tests/saves/fresh_w*_s*.save`.
pub use f4x_core::world::testutil::FRESH_WORLDS as WORLDS;

/// Every fresh game of the matrix, with its name for a failure message.
pub fn worlds() -> Vec<(String, Table)> {
    WORLDS
        .iter()
        .map(|&(w, s)| {
            let world = fresh(w, s).unwrap_or_else(|e| panic!("{e}"));
            (format!("{w} wizards, seed {s}"), world)
        })
        .collect()
}

pub use f4x_core::world::testutil::sweep::{around, commands};

/// The content the games are loaded and restored against.
pub fn content() -> f4x_def::content::Content {
    f4x_core::world::testutil::shipped()
}

/// The text of a game: what a save would write (the checksum line aside, which only follows from it).
pub fn text(w: &World) -> String {
    w.snapshot().to_text().expect("a game writes as text")
}

/// [`text`] and the numbers drawn so far: all that playing changes, for a test that says two worlds
/// are the same or that nothing happened to one.
pub fn state(w: &Table) -> String {
    format!("{}draws {}\n", text(w), w.draws())
}

/// A city of `owner` on the first square near their first city that the game lets one be founded on.
pub fn another_city(
    w: &mut Table,
    owner: f4x_core::player::PlayerId,
    population: u32,
) -> Option<hecs::Entity> {
    let (first, at) = w.economy().cities_of(owner).first().copied()?;
    let race = w.economy().city_race(first);
    for dx in -6i32..=6 {
        for dy in -6i32..=6 {
            let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
            if x < 0 || y < 0 {
                continue;
            }
            let pos = Position {
                x: x as u32,
                y: y as u32,
                plane: at.plane,
            };
            let events = w.apply(Command::FoundCity {
                owner,
                pos,
                name: "Another".to_owned(),
                race,
                population,
            });
            if events.iter().all(|e| !matches!(e, Event::Rejected(_))) {
                return w.economy().city_at(pos);
            }
        }
    }
    None
}
