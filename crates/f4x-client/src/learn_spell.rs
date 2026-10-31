//! The animation of a spell just learned (`Learn_Spell_Animation`, `Spellbook.c`): the wizard in
//! the lab with the familiar, then the open library at the spell's page, where its entry is
//! decoded out of glowing text under the sparkle (`Role::Sparkle`). With Spell Animations off the
//! original shows a box over the map instead: "You have mastered the spell of X".
//!
//! Tests: below. When a click or key ends a scene and when the decoding is over is `phase`, the
//! realm of the familiar is `strongest`; the drawing is left to the screenshots.

use crate::input::Input;
use f4x_core::rules::SpellId;
use f4x_core::text::Args;
use f4x_data::spell_art::Role;
use macroquad::prelude::*;

use crate::audio::{music, Track};
use crate::bigbook::BigBookUi;
use crate::event_popup::GREEN;
use crate::frame::Layout;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::view::{App, HUMAN};

/// One loop of the original waits two ticks of the PC timer (55 ms each).
const LAB_STEP: f64 = 0.11;
/// The lab scene runs 60 loops unless a key or click ends it.
const LAB_LOOPS: f64 = 60.0;
/// The decoding runs 45 stages; the original does not wait between them, so this is our pace.
const DECODE_STEP: f64 = 0.08;
const DECODE_STAGES: usize = 45;
/// The window that opens on the real entry from stage 22 to 27 (`g_spell_decode_x_r` and
/// `_l`): the entry shows between the two, the glowing text outside them.
const WINDOW_LEFT: [f32; 6] = [53.0, 45.0, 37.0, 16.0, 6.0, 6.0];
const WINDOW_RIGHT: [f32; 6] = [64.0, 71.0, 79.0, 101.0, 111.0, 111.0];
/// The colour of palette entry 198, the glowing text, cycles from the first to the second
/// (6-bit values) by 2 a stage for the first 15 stages (`Cycle_Palette_Color(198, ...)`).
const GLOW_FROM: [f32; 3] = [18.0, 14.0, 9.0];
const GLOW_TO: [f32; 3] = [38.0, 38.0, 53.0];

/// A learn-spell animation in progress.
#[derive(Debug, Clone)]
pub(crate) struct LearnAnim {
    started: f64,
    /// When the lab scene ends, in seconds from the start.
    lab_end: f64,
    /// The library page of the spell and its place on the page, if the library has it.
    place: Option<(usize, usize)>,
}

/// A spell the human has just learned, announced until the animation or the box is done.
pub(crate) struct Learned {
    spell: SpellId,
    /// The animation once it has started, and the library it plays in.
    anim: Option<LearnAnim>,
    book: BigBookUi,
}

impl Learned {
    pub(crate) fn new(spell: SpellId) -> Self {
        Self {
            spell,
            anim: None,
            book: BigBookUi::default(),
        }
    }

    /// Lay out the library and find the spell's place in it.
    fn start(&mut self, app: &mut App) -> LearnAnim {
        self.book = app.library_book().unwrap_or_default();
        let place = self.book.pages[..self.book.first_research]
            .iter()
            .enumerate()
            .find_map(|(page, p)| {
                p.spells
                    .iter()
                    .position(|&s| s == self.spell)
                    .map(|i| (page, i))
            });
        LearnAnim {
            started: get_time(),
            lab_end: LAB_LOOPS * LAB_STEP,
            place,
        }
    }
}

impl Modal for Learned {
    fn kind(&self) -> &'static str {
        kind::LEARNED
    }
    fn music(&self) -> Option<Track> {
        Some(Track::piece(music::NEW_SPELL))
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.learned_frame(*self, input))
    }
}

/// A left click, Enter, Esc or Space: what hurries the animation on.
fn any_input(input: &Input) -> bool {
    input.mouse_pressed(MouseButton::Left)
        || input.key_pressed(KeyCode::Enter)
        || input.key_pressed(KeyCode::Escape)
        || input.key_pressed(KeyCode::Space)
}

/// Where the animation is `t` seconds in.
#[derive(Debug, PartialEq)]
enum Phase {
    /// The lab scene, ending at this many seconds (now, if a key was pressed).
    Lab(f64),
    /// The decoding at this stage.
    Decode(usize),
    /// Over: all stages shown, a key pressed, or the spell has no place in the library.
    Over,
}

/// The phase at `t` seconds, given when the lab scene ends, the spell's place in the library and
/// whether a key or click came this frame.
fn phase(t: f64, lab_end: f64, place: Option<(usize, usize)>, pressed: bool) -> Phase {
    if t < lab_end {
        return Phase::Lab(if pressed { t } else { lab_end });
    }
    let stage = ((t - lab_end) / DECODE_STEP) as usize;
    if place.is_none() || stage >= DECODE_STAGES || pressed {
        Phase::Over
    } else {
        Phase::Decode(stage)
    }
}

/// The first of `books` with the most (nature, sorcery, chaos, life, death in that order on a tie).
fn strongest(books: [i32; 5]) -> usize {
    let mut best = 0;
    for (i, &n) in books.iter().enumerate() {
        if n > books[best] {
            best = i;
        }
    }
    best
}

impl App {
    /// The realm of the wizard's strongest books, the first of nature, sorcery, chaos, life and
    /// death on a tie (`Prim_Realm`): it colours the familiar.
    fn primary_realm(&self) -> usize {
        let books = |r: &str| self.game.wizard(HUMAN).map_or(0, |m| m.books_in(r));
        strongest(["nature", "sorcery", "chaos", "life", "death"].map(books))
    }

    /// `Learn_Spell_Animation_Draw`: the wizard's lab, the wizard on the podium, the familiar
    /// and the blue column.
    fn draw_lab(&mut self, lay: &Layout, t: f64) {
        clear_background(BLACK);
        let wizard = self
            .game
            .wizard(HUMAN)
            .map_or(0, |w| usize::from(w.portrait()));
        let familiar = 14 + self.primary_realm();
        let frames = self.art.frames("wizlab", familiar).max(1);
        self.draw_pic(lay, "wizlab", 19, 0, 0.0, 0.0);
        self.draw_pic(lay, "wizlab", 20, 0, 149.0, 133.0);
        self.draw_pic(lay, "wizlab", wizard, 0, 69.0, 75.0);
        let frame = (t / LAB_STEP) as usize % frames;
        self.draw_pic(lay, "wizlab", familiar, frame, 190.0, 160.0);
        self.clipped(crate::resolution::SCREEN, |app| {
            app.draw_pic(lay, "wizlab", 21, 0, 132.0, -3.0);
        });
    }

    /// With animations off: "You have mastered the spell of X" in a box over the map.
    /// True while the box stays.
    fn learned_box(&mut self, spell: SpellId, input: &Input) -> bool {
        let name = self.game.content().rules.spells.get(spell).name.clone();
        let lay = Layout::new();
        let text = self.text_with("spell.mastered", &Args::new().text("spell", &name));
        let closed = self.notify2_box(
            &lay,
            40.0,
            GREEN,
            &text,
            |app, lay, x| {
                app.draw_shared_pic(lay, &Role::Learned, 0, x, 40.0 + 9.0);
            },
            input,
        );
        !closed
    }
}
impl App {
    /// For screenshots: announce a spell as learned, the animation `seconds` in.
    pub fn open_test_learn(&mut self, id: &str, seconds: f64) {
        let Some(spell) = self.game.content().rules.spells.id_of(id) else {
            eprintln!("unknown spell {id}");
            return;
        };
        let mut learned = Learned::new(spell);
        let anim = learned.start(self);
        learned.anim = Some(LearnAnim {
            started: get_time() - seconds,
            ..anim
        });
        self.modals.enqueue(learned);
    }

    /// A spell learned, over the whole screen until its animation is done; what is left of it
    /// while it plays.
    fn learned_frame(&mut self, mut learned: Learned, input: &Input) -> Option<Learned> {
        let spell = learned.spell;
        if !self.prefs.get(Pref::SpellAnimations) {
            return self.learned_box(spell, input).then_some(learned);
        }
        let mut anim = match learned.anim.take() {
            Some(a) => a,
            None => learned.start(self),
        };
        let lay = Layout::new();
        let t = get_time() - anim.started;
        let pressed = any_input(input);
        match phase(t, anim.lab_end, anim.place, pressed) {
            Phase::Lab(end) => {
                anim.lab_end = end;
                self.draw_lab(&lay, t);
            }
            Phase::Decode(stage) => {
                let place = anim.place?;
                learned.book.draw_decode(self, &lay, spell, place, stage);
            }
            Phase::Over => return None,
        }
        learned.anim = Some(anim);
        Some(learned)
    }
}

impl BigBookUi {
    /// `Learn_Spell_Animation_Decode_Draw`: the book open at the spell's page with its entry
    /// hidden, the glowing text over it, the real entry opening through a window, and the
    /// sparkle on top.
    fn draw_decode(
        &mut self,
        app: &mut App,
        lay: &Layout,
        spell: SpellId,
        place: (usize, usize),
        stage: usize,
    ) {
        let (page, slot) = place;
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.left = page & !1;
        self.draw_big_book(app, lay, view, Some(place));
        let on_right = page % 2 == 1;
        let (x, y) = (
            if on_right { 173.0 } else { 25.0 },
            28.0 + 37.0 * slot as f32,
        );
        let cycled = stage.min(15) as f32 * 2.0;
        let glow = Color::new(
            (GLOW_FROM[0] + cycled).min(GLOW_TO[0]) / 63.0,
            (GLOW_FROM[1] + cycled).min(GLOW_TO[1]) / 63.0,
            (GLOW_FROM[2] + cycled).min(GLOW_TO[2]) / 63.0,
            1.0,
        );
        if stage >= 28 {
            app.draw_big_entry(lay, view, spell, x, y, false, None);
        } else if stage >= 22 {
            let k = stage - 22;
            let (open, close) = (x + WINDOW_LEFT[k], x + WINDOW_RIGHT[k]);
            app.clipped((open, y, close - open, 37.0), |app| {
                app.draw_big_entry(lay, view, spell, x, y, false, None);
            });
            if stage < 27 {
                app.clipped((x, y, open - x, 37.0), |app| {
                    app.draw_big_entry(lay, view, spell, x, y, false, Some(glow));
                });
                app.clipped((close, y, x + 129.0 - close, 37.0), |app| {
                    app.draw_big_entry(lay, view, spell, x, y, false, Some(glow));
                });
            }
        } else {
            app.draw_big_entry(lay, view, spell, x, y, false, Some(glow));
        }
        if (16..29).contains(&stage) {
            let (sx, sy) = if on_right {
                (168.0, 24.0)
            } else {
                (20.0, 21.0)
            };
            app.draw_shared_pic(lay, &Role::Sparkle, stage - 16, sx, sy + 37.0 * slot as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_left_click_enter_escape_or_space_hurries_the_scene_on() {
        assert!(any_input(&Input::click(MouseButton::Left, 1.0, 1.0)));
        for key in [KeyCode::Enter, KeyCode::Escape, KeyCode::Space] {
            assert!(any_input(&Input::key(key)), "{key:?}");
        }
        assert!(!any_input(&Input::key(KeyCode::A)));
        assert!(!any_input(&Input::click(MouseButton::Right, 1.0, 1.0)));
        assert!(!any_input(&Input::default()));
    }

    #[test]
    fn the_lab_runs_its_time_and_a_key_ends_it_at_once() {
        let end = LAB_LOOPS * LAB_STEP;
        assert_eq!(phase(1.0, end, Some((0, 0)), false), Phase::Lab(end));
        assert_eq!(phase(1.0, end, Some((0, 0)), true), Phase::Lab(1.0));
    }

    #[test]
    fn the_decoding_runs_its_stages_then_ends() {
        let place = Some((2, 1));
        let end = 5.0;
        assert_eq!(phase(end, end, place, false), Phase::Decode(0));
        assert_eq!(
            phase(end + DECODE_STEP * 10.5, end, place, false),
            Phase::Decode(10)
        );
        let last = end + DECODE_STEP * (DECODE_STAGES - 1) as f64 + DECODE_STEP / 2.0;
        assert_eq!(
            phase(last, end, place, false),
            Phase::Decode(DECODE_STAGES - 1)
        );
        let after = end + DECODE_STEP * (DECODE_STAGES as f64 + 0.5);
        assert_eq!(phase(after, end, place, false), Phase::Over);
    }

    #[test]
    fn a_key_ends_the_decoding_and_a_spell_with_no_page_has_none() {
        assert_eq!(phase(6.0, 5.0, Some((0, 0)), true), Phase::Over);
        assert_eq!(phase(6.0, 5.0, None, false), Phase::Over);
    }

    #[test]
    fn the_familiar_takes_the_realm_of_the_most_books_and_the_first_on_a_tie() {
        assert_eq!(strongest([0, 0, 0, 0, 0]), 0);
        assert_eq!(strongest([1, 3, 3, 0, 0]), 1);
        assert_eq!(strongest([1, 1, 1, 1, 4]), 4);
        assert_eq!(strongest([2, 1, 0, 2, 2]), 0);
    }
}
