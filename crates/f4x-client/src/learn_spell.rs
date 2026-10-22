//! The animation of a spell just learned (`Learn_Spell_Animation`, `Spellbook.c`): the wizard in
//! the lab with the familiar, then the open library at the spell's page, where its entry is
//! decoded out of glowing text under the `specfx` 49 sparkle. With Spell Animations off the
//! original shows a box over the map instead: "You have mastered the spell of X".

use f4x_core::rules::SpellId;
use macroquad::prelude::*;

use crate::bigbook::BigMode;
use crate::event_popup::GREEN;
use crate::frame::Layout;
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
    spell: SpellId,
    started: f64,
    /// When the lab scene ends, in seconds from the start.
    lab_end: f64,
    /// The library page of the spell and its place on the page, if the library has it.
    place: Option<(usize, usize)>,
}

fn any_input() -> bool {
    is_mouse_button_pressed(MouseButton::Left)
        || is_key_pressed(KeyCode::Enter)
        || is_key_pressed(KeyCode::Escape)
        || is_key_pressed(KeyCode::Space)
}

impl App {
    /// For screenshots: announce a spell as learned, the animation `seconds` in.
    pub fn open_test_learn(&mut self, id: &str, seconds: f64) {
        let Some(spell) = self.game.content().rules.spells.id_of(id) else {
            eprintln!("unknown spell {id}");
            return;
        };
        self.big_ui.learned.push_back(spell);
        let anim = self.start_learn_anim(spell);
        self.big_ui.learn = Some(LearnAnim {
            started: get_time() - seconds,
            ..anim
        });
    }

    fn start_learn_anim(&mut self, spell: SpellId) -> LearnAnim {
        self.build_library_pages();
        self.big_ui.mode = BigMode::Apprentice;
        let library = self.big_ui.first_research;
        let place = self.big_ui.pages[..library]
            .iter()
            .enumerate()
            .find_map(|(page, p)| p.spells.iter().position(|&s| s == spell).map(|i| (page, i)));
        LearnAnim {
            spell,
            started: get_time(),
            lab_end: LAB_LOOPS * LAB_STEP,
            place,
        }
    }

    /// The oldest spell learned and not yet announced, over the whole screen until it is done.
    /// Returns whether it took the frame.
    pub(crate) fn learned_notice(&mut self) -> bool {
        let Some(&spell) = self.big_ui.learned.front() else {
            return false;
        };
        if !self.prefs.get(Pref::SpellAnimations) {
            return self.learned_box(spell);
        }
        let mut anim = match self.big_ui.learn.clone() {
            Some(a) if a.spell == spell => a,
            _ => self.start_learn_anim(spell),
        };
        let lay = Layout::new();
        let t = get_time() - anim.started;
        let pressed = any_input();
        if t < anim.lab_end {
            if pressed {
                anim.lab_end = t;
            }
            self.draw_lab(&lay, t);
        } else {
            let stage = ((t - anim.lab_end) / DECODE_STEP) as usize;
            let Some(place) = anim.place.filter(|_| stage < DECODE_STAGES && !pressed) else {
                self.big_ui.learned.pop_front();
                self.big_ui.learn = None;
                return true;
            };
            self.draw_decode(&lay, spell, place, stage);
        }
        self.big_ui.learn = Some(anim);
        true
    }

    /// The realm of the wizard's strongest books, the first of nature, sorcery, chaos, life and
    /// death on a tie (`Prim_Realm`): it colours the familiar.
    fn primary_realm(&self) -> usize {
        let books = |r: &str| self.game.magic(HUMAN).map_or(0, |m| m.books_in(r));
        let realms = ["nature", "sorcery", "chaos", "life", "death"];
        let mut best = 0;
        for (i, r) in realms.iter().enumerate() {
            if books(r) > books(realms[best]) {
                best = i;
            }
        }
        best
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
        self.clipped((0.0, 0.0, 320.0, 200.0), |app| {
            app.draw_pic(lay, "wizlab", 21, 0, 132.0, -3.0);
        });
    }

    /// `Learn_Spell_Animation_Decode_Draw`: the book open at the spell's page with its entry
    /// hidden, the glowing text over it, the real entry opening through a window, and the
    /// sparkle on top.
    fn draw_decode(&mut self, lay: &Layout, spell: SpellId, place: (usize, usize), stage: usize) {
        let (page, slot) = place;
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.big_ui.left = page & !1;
        self.draw_big_book(lay, view, Some(place));
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
            self.draw_big_entry(lay, view, spell, x, y, false, None);
        } else if stage >= 22 {
            let k = stage - 22;
            let (open, close) = (x + WINDOW_LEFT[k], x + WINDOW_RIGHT[k]);
            self.clipped((open, y, close - open, 37.0), |app| {
                app.draw_big_entry(lay, view, spell, x, y, false, None);
            });
            if stage < 27 {
                self.clipped((x, y, open - x, 37.0), |app| {
                    app.draw_big_entry(lay, view, spell, x, y, false, Some(glow));
                });
                self.clipped((close, y, x + 129.0 - close, 37.0), |app| {
                    app.draw_big_entry(lay, view, spell, x, y, false, Some(glow));
                });
            }
        } else {
            self.draw_big_entry(lay, view, spell, x, y, false, Some(glow));
        }
        if (16..29).contains(&stage) {
            let (sx, sy) = if on_right {
                (168.0, 24.0)
            } else {
                (20.0, 21.0)
            };
            self.draw_pic(lay, "specfx", 49, stage - 16, sx, sy + 37.0 * slot as f32);
        }
    }

    /// With animations off: "You have mastered the spell of X" in a box over the map.
    fn learned_box(&mut self, spell: SpellId) -> bool {
        let name = self.game.content().rules.spells.get(spell).name.clone();
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let lay = Layout::new();
        let text = format!("You have mastered the spell of {name}");
        let closed = self.notify2_box(&lay, 40.0, GREEN, &text, |app, lay, x| {
            app.draw_pic(lay, "spellscr", 60, 0, x, 40.0 + 9.0);
        });
        if closed {
            self.big_ui.learned.pop_front();
        }
        true
    }
}
