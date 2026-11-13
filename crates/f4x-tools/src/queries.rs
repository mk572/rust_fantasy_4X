//! `f4x-tools queries --from SAVE [--player N] [--repeat N]`: the time of the game queries the
//! client asks for in every frame (the map, the units, the summary panel, the pointer), on a
//! saved game, to see which of them a late game cannot afford at 60 frames a second (task 476).
//!
//! Tests: in the module below (a small game, every query answers).

use std::path::PathBuf;
use std::time::Instant;

use f4x_core::game::Game;
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;

pub const USAGE: &str =
    "usage: f4x-tools queries --from SAVE [--player N] [--repeat N] [--content DIR]";

/// Run each query `repeat` times and print the microseconds one call took.
pub fn run(args: &[String]) -> Result<(), String> {
    let (mut from, mut player, mut repeat, mut dir) = (
        None::<PathBuf>,
        0u8,
        20u32,
        PathBuf::from(f4x_def::SHIPPED_CONTENT),
    );
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--from" => from = Some(value("--from")?.into()),
            "--player" => player = value("--player")?.parse().map_err(|_| "bad player")?,
            "--repeat" => repeat = value("--repeat")?.parse().map_err(|_| "bad repeat")?,
            "--content" => dir = value("--content")?.into(),
            other => return Err(format!("unknown option {other:?}\n{USAGE}")),
        }
    }
    let path = from.ok_or(USAGE)?;
    let content =
        f4x_def::load::load_content_layers(&[&dir, std::path::Path::new(f4x_def::SHIPPED_CONTENT)])
            .map_err(|e| e.to_string())?;
    let (game, _) = f4x_core::game::save::load_game(&path, content)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let human = PlayerId(player);
    println!(
        "{}: {} units, {} cities, player {player}",
        path.display(),
        game.units().all_units().len(),
        game.economy().cities().len()
    );
    for (name, micros) in measure(&game, human, repeat.max(1)) {
        println!("{micros:9.0} us  {name}");
    }
    Ok(())
}

/// The queries of a frame, each timed: name and microseconds a call.
pub fn measure(game: &Game, human: PlayerId, repeat: u32) -> Vec<(&'static str, f64)> {
    let plane = Plane::Arcanus;
    let mut out = Vec::new();
    let mut time = |name: &'static str, mut work: Box<dyn FnMut() + '_>| {
        let started = Instant::now();
        for _ in 0..repeat {
            work();
        }
        out.push((
            name,
            started.elapsed().as_secs_f64() * 1e6 / f64::from(repeat),
        ));
    };
    time(
        "map: vision of the player's plane (economy().visibility)",
        Box::new(|| {
            std::hint::black_box(game.economy().visibility(human, plane));
        }),
    );
    time(
        "map and minimap: every city with its name (economy().cities)",
        Box::new(|| {
            std::hint::black_box(game.economy().cities());
        }),
    );
    let visible = game.economy().visibility(human, plane);
    let width = game.map().map_or(1, |m| m.width());
    time(
        "map: the stacks to draw (all_units, owner, position, visible to the player)",
        Box::new(|| {
            let mut stacks = 0;
            for unit in game.units().all_units() {
                let Some(at) = game.units().unit_position(unit) else {
                    continue;
                };
                if at.plane != plane {
                    continue;
                }
                let mine = game.units().unit_owner(unit) == Some(human);
                if !mine
                    && (!visible[(at.y * width + at.x) as usize]
                        || !game.economy().unit_visible_to(unit, human))
                {
                    continue;
                }
                stacks += 1;
            }
            std::hint::black_box(stacks);
        }),
    );
    time(
        "summary panel: budget",
        Box::new(|| {
            std::hint::black_box(game.budget(human));
        }),
    );
    time(
        "summary panel: income (mana and research)",
        Box::new(|| {
            std::hint::black_box(game.income(human));
        }),
    );
    let mine = game.units().player_units(human);
    let stack: Vec<_> = mine.first().copied().into_iter().collect();
    let far = stack
        .first()
        .and_then(|&u| game.units().unit_position(u))
        .map(|p| Position {
            plane: p.plane,
            x: (p.x + width / 2) % width,
            y: p.y,
        });
    if let Some(far) = far {
        time(
            "pointer: the path to the square under the cursor (find_path, once a frame)",
            Box::new(|| {
                std::hint::black_box(game.find_path(&stack, far));
            }),
        );
        time(
            "pointer: the units on the square under the cursor (units_at)",
            Box::new(|| {
                std::hint::black_box(game.units().units_at(far));
            }),
        );
    }
    time(
        "map: the player's own units (player_units)",
        Box::new(|| {
            std::hint::black_box(game.units().player_units(human));
        }),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::game::setup::{new_game, NewGame};

    #[test]
    fn every_frame_query_is_timed_on_a_small_game() -> Result<(), String> {
        let content =
            f4x_def::load::load_content_layers(&[std::path::Path::new(f4x_def::SHIPPED_CONTENT)])
                .map_err(|e| e.to_string())?;
        let game = new_game(&content, &NewGame::with_wizards(2))?;
        let times = measure(&game, PlayerId(0), 1);
        assert!(times.len() >= 8, "{times:?}");
        assert!(times.iter().all(|(_, t)| t.is_finite() && *t >= 0.0));
        Ok(())
    }
}
