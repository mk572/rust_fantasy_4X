//! A whole game on the shipped content (`content/`), through the public interface only.

use std::sync::OnceLock;

use f4x_core::content::Content;
use f4x_core::game::setup::{new_game, NewGame};
use f4x_core::game::{Command, Event, Game};
use f4x_core::testdir::TestResult;

fn shipped() -> &'static Content {
    static CONTENT: OnceLock<Content> = OnceLock::new();
    CONTENT.get_or_init(|| {
        f4x_data::load_content(std::path::Path::new("../../content"))
            .expect("the shipped content loads")
    })
}

fn game(seed: u64) -> Result<Game, String> {
    new_game(
        shipped(),
        &NewGame {
            seed,
            all_ai: true,
            ..Default::default()
        },
    )
}

fn end_turns(g: &mut Game, n: usize) -> Vec<Event> {
    (0..n).flat_map(|_| g.apply(Command::EndTurn)).collect()
}

#[test]
fn turns_pass_on_a_new_game_without_a_refusal() -> TestResult {
    let mut g = game(3)?;
    let events = end_turns(&mut g, 20);
    let refused: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Rejected(_)))
        .collect();
    assert!(refused.is_empty(), "{refused:?}");
    Ok(())
}

#[test]
fn a_saved_game_loads_into_the_same_game() -> TestResult {
    let mut g = game(5)?;
    end_turns(&mut g, 10);
    let text = g.snapshot().to_text()?;
    let loaded = f4x_core::game::save::Snapshot::from_text(&text)?.restore(shipped().clone())?;
    assert_eq!(loaded.snapshot().to_text()?, text);
    Ok(())
}

#[test]
fn the_same_seed_and_commands_give_the_same_game() -> TestResult {
    let play = || -> Result<String, Box<dyn std::error::Error>> {
        let mut g = game(9)?;
        end_turns(&mut g, 15);
        Ok(g.snapshot().to_text()?)
    };
    assert_eq!(play()?, play()?);
    Ok(())
}
