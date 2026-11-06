//! An `App` for the tests of click flows: the real content and a game, no window and no pictures.
//! Anything that draws needs a window; the tests call what decides (`Input`, the modal stack, the
//! events a screen turns into notices) and leave the drawing to the screenshots.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use f4x_core::game::setting::SettingsDef;
use f4x_def::content::Content;
use f4x_def::text::Texts;

use f4x_extract::spell_art::SpellArt;
use f4x_extract::unit_art::UnitArt;
use f4x_extract::wizard_art::WizardArt;

use crate::art::Art;
use crate::view::App;

pub(crate) const CONTENT: &str = "../../content";
/// The client's own tables (`crates/f4x-client/content`).
pub(crate) const CLIENT_CONTENT: &str = "content";

/// The files of `content/`, each parsed once for the whole test run.
pub(crate) mod shipped {
    use super::*;

    pub(crate) fn content() -> &'static Content {
        f4x_def::testdir::shipped()
    }

    pub(crate) fn texts() -> &'static Texts {
        static CELL: OnceLock<Texts> = OnceLock::new();
        CELL.get_or_init(|| {
            f4x_def::load::load_texts(&[Path::new(CLIENT_CONTENT)], "en")
                .expect("the shipped texts load")
        })
    }

    pub(crate) fn settings() -> &'static SettingsDef {
        static CELL: OnceLock<SettingsDef> = OnceLock::new();
        CELL.get_or_init(|| {
            f4x_def::load::load_settings(&[Path::new(CONTENT)]).expect("the shipped settings load")
        })
    }

    pub(crate) fn spell_art() -> &'static SpellArt {
        static CELL: OnceLock<SpellArt> = OnceLock::new();
        CELL.get_or_init(|| {
            f4x_extract::spell_art::load_spell_art(&[Path::new(CLIENT_CONTENT)])
                .expect("the shipped spell art loads")
        })
    }

    pub(crate) fn wizard_art() -> &'static Vec<WizardArt> {
        static CELL: OnceLock<Vec<WizardArt>> = OnceLock::new();
        CELL.get_or_init(|| {
            f4x_extract::wizard_art::load_wizard_art(&[Path::new(CLIENT_CONTENT)])
                .expect("the shipped wizard art loads")
        })
    }

    pub(crate) fn names() -> &'static crate::names::Names {
        static CELL: OnceLock<crate::names::Names> = OnceLock::new();
        CELL.get_or_init(|| {
            crate::names::Names::load(&[Path::new(CLIENT_CONTENT)], "en")
                .expect("the shipped names load")
        })
    }

    pub(crate) fn help() -> &'static f4x_def::help::Help {
        static CELL: OnceLock<f4x_def::help::Help> = OnceLock::new();
        CELL.get_or_init(|| {
            f4x_def::load::load_help(&[Path::new(CLIENT_CONTENT)], "en")
                .expect("the shipped help loads")
        })
    }

    pub(crate) fn icon_art() -> &'static crate::icon_art::IconArt {
        static ART: OnceLock<crate::icon_art::IconArt> = OnceLock::new();
        ART.get_or_init(|| {
            crate::icon_art::load_icon_art(&[Path::new(CLIENT_CONTENT)])
                .expect("the shipped icon art loads")
        })
    }

    pub(crate) fn unit_art() -> &'static Vec<UnitArt> {
        static CELL: OnceLock<Vec<UnitArt>> = OnceLock::new();
        CELL.get_or_init(|| {
            f4x_extract::unit_art::load_unit_art(&[Path::new(CLIENT_CONTENT)])
                .expect("the shipped unit art loads")
        })
    }
}

impl App {
    /// A new game on the shipped content, silent, with an art folder that is not there.
    pub(crate) fn for_test() -> App {
        let art = Art::load(
            PathBuf::from("no-art-in-tests"),
            shipped::spell_art().clone(),
            shipped::unit_art().clone(),
            shipped::wizard_art().clone(),
            shipped::icon_art().clone(),
        );
        let mut app = App::new(shipped::content().clone(), art, 1)
            .expect("a new game starts on the shipped content");
        app.names = shipped::names().clone();
        app
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
