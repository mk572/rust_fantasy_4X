//! The animation of a global enchantment being cast (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`,
//! `Spells137.c`): the caster's face in the diplomacy mirror, a swirl clearing the glass, then
//! the face dissolving into the spell's own picture, over the map. It plays for every wizard's
//! cast. With Spell Animations off the mirror shows the spell's picture at once, with the line
//! "You have completed casting..." under it.

use std::collections::VecDeque;

use f4x_core::player::PlayerId;
use f4x_core::rules::SpellId;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::preferences::Pref;
use crate::view::App;

/// Where the mirror's frame goes (`start_x`, `start_y`); the face sits 12 in, the spell's
/// picture 10 and 8 in.
const MIRROR: (f32, f32) = (58.0, 28.0);
/// Two ticks of the original's 55 ms timer a stage.
const STAGE: f64 = 0.11;
/// The animation ends by itself after 120 stages.
const STAGES: usize = 120;

/// The `specfx` picture of each global enchantment (15 plus the spell's `Param0`; the ones
/// with their own entries are Death Wish, Great Unsummoning and Awareness).
const PICTURES: [(&str, usize); 26] = [
    ("eternal_night", 15),
    ("evil_omens", 16),
    ("zombie_mastery", 17),
    ("aura_of_majesty", 18),
    ("wind_mastery", 19),
    ("suppress_magic", 20),
    ("time_stop", 21),
    ("nature_awareness", 22),
    ("natures_wrath", 23),
    ("herb_mastery", 24),
    ("chaos_surge", 25),
    ("doom_mastery", 26),
    ("great_wasting", 27),
    ("meteor_storm", 28),
    ("armageddon", 29),
    ("tranquility", 30),
    ("life_force", 31),
    ("crusade", 32),
    ("just_cause", 33),
    ("holy_arms", 34),
    ("planar_seal", 35),
    ("charm_of_life", 36),
    ("detect_magic", 37),
    ("death_wish", 38),
    ("great_unsummoning", 39),
    ("awareness", 56),
];

/// A global spell cast and not yet shown.
#[derive(Debug, Clone)]
pub(crate) struct GlobalCast {
    player: PlayerId,
    spell: SpellId,
    picture: usize,
    /// When it began, once it has.
    started: Option<f64>,
}

fn any_input() -> bool {
    is_mouse_button_pressed(MouseButton::Left)
        || is_mouse_button_pressed(MouseButton::Right)
        || is_key_pressed(KeyCode::Enter)
        || is_key_pressed(KeyCode::Escape)
        || is_key_pressed(KeyCode::Space)
}

impl App {
    /// Remember a cast to show, if the spell is a global enchantment with a picture.
    pub(crate) fn announce_global_cast(&mut self, player: PlayerId, spell: SpellId) {
        let id = &self.game.content().rules.spells.get(spell).id;
        let Some(&(_, picture)) = PICTURES.iter().find(|(name, _)| name == id) else {
            return;
        };
        self.global_casts.push_back(GlobalCast {
            player,
            spell,
            picture,
            started: None,
        });
    }

    /// For screenshots: show `id` cast by player `by`, `seconds` into the animation.
    pub fn open_test_global(&mut self, id: &str, by: u8, seconds: f64) {
        let Some(spell) = self.game.content().rules.spells.id_of(id) else {
            eprintln!("unknown spell {id}");
            return;
        };
        self.announce_global_cast(PlayerId(by), spell);
        if let Some(cast) = self.global_casts.back_mut() {
            cast.started = Some(get_time() - seconds);
        }
    }

    /// The oldest global cast not yet shown, over the map until it ends. Returns whether it took
    /// the frame.
    pub(crate) fn global_cast_frame(&mut self) -> bool {
        let Some(cast) = self.global_casts.front_mut() else {
            return false;
        };
        let started = *cast.started.get_or_insert_with(get_time);
        let cast = cast.clone();
        let stage = ((get_time() - started) / STAGE) as usize;
        if stage >= STAGES || any_input() {
            self.global_casts.pop_front();
            return true;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let lay = Layout::new();
        let animated = self.prefs.get(Pref::SpellAnimations);
        self.draw_mirror(&lay, &cast, stage, animated);
        true
    }

    fn draw_mirror(&mut self, lay: &Layout, cast: &GlobalCast, stage: usize, animated: bool) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (x, y) = MIRROR;
        let face = self
            .game
            .player(cast.player)
            .map_or(0, |p| usize::from(p.portrait));
        let words = if cast.player == crate::view::HUMAN {
            "You have completed casting...".to_string()
        } else {
            let who = self
                .game
                .player(cast.player)
                .map_or("A rival wizard".to_string(), |p| p.name.clone());
            format!("{who} has cast...")
        };
        let name = self
            .game
            .content()
            .rules
            .spells
            .get(cast.spell)
            .name
            .clone();
        let line = (x + 62.0, y + 133.0);
        if !animated {
            self.bm_text_outlined(view, 5, &words, line.0, line.1, Align::Center);
            self.draw_pic(lay, "backgrnd", 18, 0, x, y);
            self.draw_pic(lay, "specfx", cast.picture, 0, x + 10.0, y + 8.0);
            self.bm_text_outlined(view, 5, &name, line.0, line.1 + 20.0, Align::Center);
            return;
        }
        if stage < 34 {
            self.bm_text_outlined(view, 5, &words, line.0, line.1, Align::Center);
        }
        let inside = (x + 12.0, y + 12.0);
        if stage < 14 {
            // The wizard, under the glass pane that a swirl clears away: the first mask's ten
            // frames, then the second's.
            self.draw_pic(lay, "moodwiz", face, 0, inside.0, inside.1);
            let (mask, frame) = if stage < 10 {
                (54, stage)
            } else {
                (55, stage - 10)
            };
            self.draw_masked(lay, "diplomac", 1, mask, frame, inside);
            self.draw_pic(lay, "backgrnd", 18, 0, x, y);
        } else if stage < 20 {
            self.draw_pic(lay, "diplomac", 1, 0, inside.0, inside.1);
            self.draw_pic(lay, "moodwiz", face, 0, inside.0, inside.1);
            self.draw_pic(lay, "backgrnd", 18, 0, x, y);
        } else if stage < 34 {
            // The face dissolves in a swirl (the second mask over and over, restarted at 30)
            // out of the spell's picture.
            self.draw_pic(lay, "specfx", cast.picture, 0, x + 10.0, y + 8.0);
            let frame = if stage < 30 {
                (stage - 20) % 4
            } else {
                stage - 30
            };
            self.draw_masked(lay, "moodwiz", face, 55, frame, inside);
            self.draw_pic(lay, "backgrnd", 18, 0, x, y);
        } else {
            self.draw_pic(lay, "backgrnd", 18, 0, x, y);
            self.draw_pic(lay, "specfx", cast.picture, 0, x + 10.0, y + 8.0);
            self.bm_text_outlined(view, 5, &name, line.0, line.1, Align::Center);
        }
    }

    /// Draw frame 0 of a picture through a `specfx` mask frame.
    fn draw_masked(
        &mut self,
        lay: &Layout,
        lbx: &str,
        entry: usize,
        mask: usize,
        mask_frame: usize,
        at: (f32, f32),
    ) {
        let Some(tex) = self
            .art
            .masked_picture(lbx, entry, 0, mask, mask_frame)
            .cloned()
        else {
            return;
        };
        let p = lay.point(at.0, at.1);
        draw_texture_ex(
            &tex,
            p.x,
            p.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(tex.size() * lay.scale),
                ..Default::default()
            },
        );
    }
}

/// The queue of casts to show.
pub(crate) type GlobalCasts = VecDeque<GlobalCast>;
