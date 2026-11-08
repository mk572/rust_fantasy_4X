//! Nature's Wrath striking a wizard who cast a death or chaos spell (`Call_Forth_The_Force_Of_Nature`,
//! `Spells129.c`): the mirror's frame over the map with the spell's picture in it, "Nature's Wrath
//! strikes you" under it, the screen shaking for the first stages, and the original's music entry 27
//! (`Force_of_Nature`). The original never names a spell "Force of Nature"; that is this scene's
//! name in the sound table (`docs/sound.md`).
//!
//! It shows for the human when Nature's Wrath strikes them, and for a rival's strike with Enemy
//! Spells on; Overland Spell Events off hides both (the original plays it every time).
//!
//! Tests: below. The drawing is left to the screenshots (`--wrath`).

use crate::audio::{music, Track};
use crate::font::{rgb, Effect, Style};
use crate::frame::Layout;
use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::text::Args;
use crate::view::{App, HUMAN};
use f4x_core::player::PlayerId;
use f4x_extract::spell_art::Role;
use macroquad::prelude::*;

/// Where the mirror's frame goes (`FLIC_Draw(58, 33, ...)`); the picture sits 10 and 8 in.
const MIRROR: (f32, f32) = (58.0, 33.0);
/// The paragraph's left edge, top and width (`Print_Paragraph(45, 166, 152, ...)`).
const PARAGRAPH: (f32, f32, f32) = (45.0, 166.0, 152.0);
/// Two ticks of the original's 55 ms timer a stage.
const STAGE: f64 = 0.11;
/// The scene ends by itself after 120 stages; the first 16 shake.
const STAGES: usize = 120;
const SHAKING: usize = 16;

/// A strike and not yet shown.
#[derive(Debug, Clone)]
pub(crate) struct WrathScene {
    /// The picture in the frame (`Role::Global` of Nature's Wrath).
    picture: (String, usize),
    text: String,
    /// When it began, once it has.
    started: Option<f64>,
}

impl Modal for WrathScene {
    fn kind(&self) -> &'static str {
        kind::WRATH_SCENE
    }
    fn music(&self) -> Option<Track> {
        Some(Track::piece(music::FORCE_OF_NATURE))
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.wrath_frame(*self, input))
    }
}

/// How far the frame is pushed at a shaking stage: one to four across and one to three down
/// (the original copies the screen at a random offset of that size).
fn shake(stage: usize) -> (f32, f32) {
    if stage >= SHAKING {
        return (0.0, 0.0);
    }
    (1.0 + (stage * 7 % 4) as f32, 1.0 + (stage * 5 % 3) as f32)
}

impl App {
    /// Remember a strike of Nature's Wrath on `victim` to show.
    pub(crate) fn announce_wrath(&mut self, victim: PlayerId) {
        if (victim != HUMAN && !self.prefs.get(Pref::EnemySpells))
            || !self.prefs.get(Pref::OverlandSpellEvents)
        {
            return;
        }
        self.enqueue_wrath(victim, None);
    }

    fn enqueue_wrath(&mut self, victim: PlayerId, started: Option<f64>) {
        let Some(picture) = self.art.spell_picture("natures_wrath", &Role::Global) else {
            return;
        };
        let text = if victim == HUMAN {
            self.text("spell.wrath_strikes_you")
        } else {
            let args = self.wizard_arg(Args::new(), "wizard", victim);
            self.text_with("spell.wrath_strikes", &args)
        };
        self.modals.enqueue(WrathScene {
            picture,
            text,
            started,
        });
    }

    /// For screenshots: the strike on player `victim`, `seconds` into the scene.
    pub fn open_test_wrath(&mut self, victim: u8, seconds: f64) {
        self.enqueue_wrath(PlayerId(victim), Some(get_time() - seconds));
    }

    fn wrath_frame(&mut self, mut scene: WrathScene, input: &Input) -> Option<WrathScene> {
        let started = *scene.started.get_or_insert_with(get_time);
        let stage = ((get_time() - started) / STAGE) as usize;
        if stage >= STAGES || input.dismisses() {
            return None;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (dx, dy) = shake(stage);
        let (x, y) = MIRROR;
        self.draw_pic(&lay, "backgrnd", 18, 0, x + dx, y + dy);
        self.draw_pic(
            &lay,
            &scene.picture.0,
            scene.picture.1,
            0,
            x + 10.0 + dx,
            y + 8.0 + dy,
        );
        let style = Style::set(5, 0)
            .effect(Effect::Outline)
            .edge(rgb(0, 0, 0))
            .center();
        let (left, top, width) = PARAGRAPH;
        let line = self.art.text_height(5) + 1.0;
        for (i, words) in self.bm_wrap(5, &scene.text, width).iter().enumerate() {
            self.draw_styled(
                view,
                &style,
                words,
                left + width / 2.0 + dx,
                top + line * i as f32 + dy,
            );
        }
        Some(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::TestResult;

    #[test]
    fn the_screen_shakes_only_for_the_first_stages() {
        for stage in 0..SHAKING {
            let (dx, dy) = shake(stage);
            assert!((1.0..=4.0).contains(&dx) && (1.0..=3.0).contains(&dy));
        }
        assert_eq!(shake(SHAKING), (0.0, 0.0));
    }

    #[test]
    fn a_strike_shows_by_the_switches() -> TestResult {
        let mut app = App::for_test();
        let rival = PlayerId(1);
        app.prefs.set(Pref::OverlandSpellEvents, false);
        app.announce_wrath(HUMAN);
        assert!(!app.modals.has(kind::WRATH_SCENE), "events off");
        app.prefs.set(Pref::OverlandSpellEvents, true);
        app.prefs.set(Pref::EnemySpells, false);
        app.announce_wrath(rival);
        assert!(
            !app.modals.has(kind::WRATH_SCENE),
            "a rival's needs Enemy Spells"
        );
        app.announce_wrath(HUMAN);
        assert!(app.modals.has(kind::WRATH_SCENE), "the human's strike does");
        Ok(())
    }
}
