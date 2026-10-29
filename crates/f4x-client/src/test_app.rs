//! An `App` for the tests of click flows: the real content and a game, no window and no pictures.
//! Anything that draws needs a window; the tests call what decides (`Input`, the modal stack, the
//! events a screen turns into notices) and leave the drawing to the screenshots.

use std::path::{Path, PathBuf};

use crate::art::Art;
use crate::view::App;

impl App {
    /// A new game on the shipped content, silent, with an art folder that is not there.
    pub(crate) fn for_test() -> App {
        let dir = Path::new("../../content");
        let content = f4x_data::load_content(dir).unwrap();
        let spell_art = f4x_data::spell_art::load_spell_art(&[dir], &content.rules).unwrap();
        let art = Art::load(PathBuf::from("no-art-in-tests"), spell_art);
        App::new(content, art, 1).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;
    use crate::view::HUMAN;
    use macroquad::prelude::KeyCode;

    #[test]
    fn the_resign_question_stays_until_a_key_answers_it() {
        let mut app = App::for_test();
        assert!(app.resign_answer(&Input::default()));
        assert!(app.resign_answer(&Input::key(KeyCode::A)));
        for key in [KeyCode::Escape, KeyCode::N, KeyCode::Enter] {
            assert!(!app.resign_answer(&Input::key(key)), "{key:?}");
        }
    }

    #[test]
    fn y_closes_the_resign_question_but_the_rules_refuse_while_the_fortress_stands() {
        let mut app = App::for_test();
        assert!(!app.resign_answer(&Input::key(KeyCode::Y)));
        assert_eq!(app.game.player(HUMAN).and_then(|p| p.defeated_by()), None);
    }
}
