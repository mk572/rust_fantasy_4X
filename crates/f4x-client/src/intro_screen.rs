//! The original's opening movie (task 85): `Draw_Logos` in `INTRO.c`. Thirteen scenes of
//! `INTRO.LBX`, each a short animation of 320 by 200 frames, with a fade out before it and a
//! fade in on its first frame, the Simtex copyright line over scene 2, and a few sound effects at
//! fixed frames. Any key or click skips it. The pictures are the author's own copy of the game
//! (`assets-local/screens/intro/`) and the sounds come from `SOUNDFX`; without them the movie is
//! skipped. The movie's song (`music/112_intro`) plays under it, a piece that ends when the movie does
//! (`App::want_music`).
//!
//! The show is made as a list of beats up front (`beats`), one picture held for a number of the
//! original's 55 ms ticks at a darkness, so the timing can be tested without a window.

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use macroquad::prelude::*;

use crate::audio::{fx, Key};
use crate::frame::Layout;
use crate::preferences::Pref;
use crate::view::{App, Screen};

/// Frames of each scene (`IntroAnim_FrameCounts`).
const FRAMES: [usize; 13] = [15, 12, 30, 18, 25, 32, 24, 32, 30, 37, 30, 11, 32];

/// One timer tick of the original's: the BIOS clock, 55 ms.
const TICK: f64 = 0.055;

/// A frame takes three ticks.
const FRAME_TICKS: u8 = 3;

/// The `SOUNDFX` entries the movie plays (`SFX_IntroT1` and so on).
const INTRO_T1: u16 = 115;
const INTRO_M: u16 = 4;
const MERLIN_WORK: u16 = 116;
const INTRO_T2: u16 = 1;
const LIGHTNING: u16 = 27;
const UNKNOWN_118: u16 = 118;
const INTRO_T3: u16 = 3;

/// The original's palette fades: the darkness in percent at each step, 55 ms apart.
fn fade_out_steps(scene: usize) -> Vec<u8> {
    let (first, step) = match scene {
        1 | 3 => (5, 5),
        2 => (10, 10),
        _ => (50, 50),
    };
    (first..=100).step_by(step).collect()
}

fn fade_in_steps(scene: usize) -> Vec<u8> {
    if scene > 2 {
        vec![100, 50, 0]
    } else {
        vec![10, 8, 6, 4, 2, 0]
    }
}

/// The sound that starts on a frame (`Draw_Logos`), if any.
fn sound_at(scene: usize, frame: usize) -> Option<u16> {
    match (scene, frame) {
        (4, 0) => Some(INTRO_T1),
        (5, 10) => Some(INTRO_M),
        (6, 0) => Some(MERLIN_WORK),
        (7, 2) => Some(INTRO_T2),
        (9, 0) => Some(UNKNOWN_118),
        (11, 2) => Some(INTRO_T3),
        (8, 3) | (8, 9) | (10, 2) => Some(LIGHTNING),
        _ => None,
    }
}

/// One picture held for some ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Beat {
    /// The picture: scene and frame; `None` is a black screen.
    pub picture: Option<(usize, usize)>,
    /// How dark it is, in percent (0 is the picture as it is).
    pub dark: u8,
    pub ticks: u8,
    pub sound: Option<u16>,
    /// The copyright line is over the picture.
    pub copyright: bool,
}

/// The whole show, in order.
pub(crate) fn beats() -> Vec<Beat> {
    let mut out = Vec::new();
    let mut last = None;
    for (scene, &count) in FRAMES.iter().enumerate() {
        for dark in fade_out_steps(scene) {
            out.push(Beat {
                picture: last,
                dark,
                ticks: 1,
                sound: None,
                copyright: false,
            });
        }
        for frame in 0..count {
            let picture = Some((scene, frame));
            let copyright = scene == 2 && frame > 6;
            if frame == 0 {
                // The first frame is drawn dark and fades in; its sound starts with it.
                for (i, dark) in fade_in_steps(scene).into_iter().enumerate() {
                    out.push(Beat {
                        picture,
                        dark,
                        ticks: 1,
                        sound: if i == 0 { sound_at(scene, 0) } else { None },
                        copyright,
                    });
                }
            }
            out.push(Beat {
                picture,
                dark: 0,
                ticks: FRAME_TICKS,
                sound: if frame == 0 {
                    None
                } else {
                    sound_at(scene, frame)
                },
                copyright,
            });
            last = picture;
        }
    }
    out
}

/// Where the movie is.
#[derive(Debug, Clone, Default)]
pub(crate) struct IntroState {
    beats: Vec<Beat>,
    at: usize,
    /// When the beat on show began, in seconds.
    since: f64,
    /// The beat's sound has been started.
    sounded: bool,
}

impl App {
    /// Start the movie when it is wanted and the pictures are there; else the start menu.
    pub fn open_intro(&mut self) {
        if self.prefs.get(Pref::PlayIntro) && self.art_picture("intro", 0, 0).is_some() {
            self.open_page(Screen::Intro, IntroState::start());
        } else {
            self.open_main_menu();
        }
    }

    /// Open the movie at the start of `scene`, for a screenshot (`--intro`).
    pub fn open_intro_at(&mut self, scene: usize) {
        let mut movie = IntroState::start();
        let at = movie
            .beats
            .iter()
            .position(|b| b.picture == Some((scene, 0)) && b.dark == 0);
        movie.at = at.unwrap_or(0);
        movie.since = f64::MAX;
        self.open_page(Screen::Intro, movie);
    }
}
impl IntroState {
    /// The movie from its first beat, starting now.
    fn start() -> IntroState {
        IntroState {
            beats: beats(),
            at: 0,
            since: get_time(),
            sounded: false,
        }
    }

    /// Go to the start menu and silence the movie's sounds: the long ones would play on over
    /// the screens after it.
    fn leave_intro(&mut self, app: &mut App) {
        for sound in self.beats.iter().filter_map(|b| b.sound) {
            app.audio.stop_effect(Some(fx(sound)));
        }
        app.open_main_menu();
    }
}

impl Page for IntroState {
    fn frame(&mut self, app: &mut App, input: &Input) {
        clear_background(BLACK);
        let skip = input.any_key_pressed() || input.mouse_pressed(MouseButton::Left);
        let now = get_time();
        let Some(beat) = self.beats.get(self.at).copied() else {
            return self.leave_intro(app);
        };
        if skip {
            return self.leave_intro(app);
        }
        if !self.sounded {
            self.sounded = true;
            let key: Option<Key> = beat.sound.map(fx);
            app.audio.effect(key);
        }
        if now - self.since >= f64::from(beat.ticks) * TICK {
            self.since = now;
            self.at += 1;
            self.sounded = false;
        }
        let lay = Layout::new();
        if let Some((scene, frame)) = beat.picture {
            let shade = 1.0 - f32::from(beat.dark) / 100.0;
            let tint = Color::new(shade, shade, shade, 1.0);
            app.draw_pic_tinted(&lay, "intro", scene, frame, 0.0, 0.0, tint);
        }
        if beat.copyright {
            let view = (lay.origin.x, lay.origin.y, lay.scale);
            let text = app.text("intro.copyright");
            app.draw_styled(
                view,
                &Style::nearest(0, WHITE).center(),
                &text,
                160.0,
                192.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks(beats: &[Beat]) -> u32 {
        beats.iter().map(|b| u32::from(b.ticks)).sum()
    }

    #[test]
    fn the_show_shows_every_frame_of_every_scene_at_full_brightness() {
        let beats = beats();
        for (scene, &count) in FRAMES.iter().enumerate() {
            for frame in 0..count {
                let held = beats
                    .iter()
                    .filter(|b| b.picture == Some((scene, frame)) && b.dark == 0)
                    .count();
                // The first frame of a scene reaches full brightness at the end of its fade in
                // and then holds, so it has two beats.
                let want = if frame == 0 { 2 } else { 1 };
                assert_eq!(held, want, "scene {scene} frame {frame}");
            }
        }
    }

    #[test]
    fn the_first_scene_fades_in_from_black_and_each_scene_fades_the_last_out() {
        let beats = beats();
        assert_eq!(beats[0].picture, None);
        assert_eq!(beats[0].dark, 50);
        let first_scene_start = beats.iter().position(|b| b.picture.is_some()).unwrap();
        assert_eq!(beats[first_scene_start].picture, Some((0, 0)));
        assert_eq!(beats[first_scene_start].dark, 10);
        // Scene 1 fades scene 0's last frame out in twenty steps of five percent.
        let out: Vec<_> = beats
            .iter()
            .filter(|b| b.picture == Some((0, 14)) && b.dark > 0)
            .map(|b| b.dark)
            .collect();
        assert_eq!(out, (1..=20).map(|i| i * 5).collect::<Vec<u8>>());
    }

    #[test]
    fn the_sounds_start_at_the_originals_frames_and_the_copyright_over_scene_two() {
        let beats = beats();
        let sounds: Vec<_> = beats.iter().filter_map(|b| b.sound).collect();
        assert_eq!(
            sounds,
            [
                INTRO_T1,
                INTRO_M,
                MERLIN_WORK,
                INTRO_T2,
                LIGHTNING,
                LIGHTNING,
                UNKNOWN_118,
                LIGHTNING,
                INTRO_T3
            ]
        );
        let lines = beats.iter().filter(|b| b.copyright).count();
        assert!(lines > 0);
        assert!(beats
            .iter()
            .filter(|b| b.copyright)
            .all(|b| matches!(b.picture, Some((2, f)) if f > 6)));
    }

    #[test]
    fn the_whole_show_lasts_about_a_minute() {
        let seconds = f64::from(ticks(&beats())) * TICK;
        assert!((30.0..120.0).contains(&seconds), "{seconds} s");
    }
}
