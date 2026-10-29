//! The animation of a global enchantment being cast (`WIZ_GlobalSpellAnim`, `OVL_DrawGlobalAnim`,
//! `Spells137.c`): the caster's face in the diplomacy mirror, a swirl clearing the glass, then
//! the face dissolving into the spell's own picture, over the map. It plays for every wizard's
//! cast. With Spell Animations off the mirror shows the spell's picture at once, with the line
//! "You have completed casting..." under it.

use crate::input::Input;
use f4x_core::player::PlayerId;
use f4x_core::rules::SpellId;
use f4x_core::text::Args;
use f4x_data::spell_art::Role;
use macroquad::prelude::*;

use crate::audio::Track;
use crate::frame::{Align, Layout};
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::view::App;

/// Where the mirror's frame goes (`start_x`, `start_y`); the face sits 12 in, the spell's
/// picture 10 and 8 in.
const MIRROR: (f32, f32) = (58.0, 28.0);
/// Two ticks of the original's 55 ms timer a stage.
const STAGE: f64 = 0.11;
/// The animation ends by itself after 120 stages.
const STAGES: usize = 120;

/// A global spell cast and not yet shown.
#[derive(Debug, Clone)]
pub(crate) struct GlobalCast {
    player: PlayerId,
    spell: SpellId,
    /// The picture in the mirror (`Role::Global` of the spell in `content/spell_art.ron`).
    picture: (String, usize),
    /// The spell's own piece of music, when it has one.
    music: Option<Track>,
    /// When it began, once it has.
    started: Option<f64>,
}

impl Modal for GlobalCast {
    fn kind(&self) -> &'static str {
        kind::GLOBAL_CAST
    }
    fn music(&self) -> Option<Track> {
        self.music
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.global_cast_frame(*self, input))
    }
}

fn any_input(input: &Input) -> bool {
    input.mouse_pressed(MouseButton::Left)
        || input.mouse_pressed(MouseButton::Right)
        || input.key_pressed(KeyCode::Enter)
        || input.key_pressed(KeyCode::Escape)
        || input.key_pressed(KeyCode::Space)
}

impl App {
    /// Remember a cast to show, if the spell is a global enchantment with a picture.
    pub(crate) fn announce_global_cast(&mut self, player: PlayerId, spell: SpellId) {
        self.enqueue_global_cast(player, spell, None);
    }

    fn enqueue_global_cast(&mut self, player: PlayerId, spell: SpellId, started: Option<f64>) {
        let id = &self.game.content().rules.spells.get(spell).id;
        let Some(picture) = self.art.spell_picture(id, &Role::Global) else {
            return;
        };
        let music = self
            .audio
            .table
            .global_music
            .get(id.as_str())
            .map(|&entry| Track::piece(u16::from(entry)));
        self.modals.enqueue(GlobalCast {
            player,
            spell,
            picture,
            music,
            started,
        });
    }

    /// For screenshots: show `id` cast by player `by`, `seconds` into the animation.
    pub fn open_test_global(&mut self, id: &str, by: u8, seconds: f64) {
        let Some(spell) = self.game.content().rules.spells.id_of(id) else {
            eprintln!("unknown spell {id}");
            return;
        };
        self.enqueue_global_cast(PlayerId(by), spell, Some(get_time() - seconds));
    }

    /// A global cast over the screen until it ends.
    pub(crate) fn global_cast_frame(
        &mut self,
        mut cast: GlobalCast,
        input: &Input,
    ) -> Option<GlobalCast> {
        let started = *cast.started.get_or_insert_with(get_time);
        let stage = ((get_time() - started) / STAGE) as usize;
        if stage >= STAGES || any_input(input) {
            return None;
        }
        let lay = Layout::new();
        let animated = self.prefs.get(Pref::SpellAnimations);
        self.draw_mirror(&lay, &cast, stage, animated);
        Some(cast)
    }

    fn draw_mirror(&mut self, lay: &Layout, cast: &GlobalCast, stage: usize, animated: bool) {
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (x, y) = MIRROR;
        let face = self
            .game
            .wizard(cast.player)
            .map_or(0, |w| usize::from(w.portrait()));
        let words = if cast.player == crate::view::HUMAN {
            self.text("spell.you_completed")
        } else {
            let args = self.wizard_arg(Args::new(), "wizard", cast.player);
            self.text_with("spell.rival_cast", &args)
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
            self.draw_pic(lay, &cast.picture.0, cast.picture.1, 0, x + 10.0, y + 8.0);
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
                (1, stage)
            } else {
                (2, stage - 10)
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
            self.draw_pic(lay, &cast.picture.0, cast.picture.1, 0, x + 10.0, y + 8.0);
            let frame = if stage < 30 {
                (stage - 20) % 4
            } else {
                stage - 30
            };
            self.draw_masked(lay, "moodwiz", face, 2, frame, inside);
            self.draw_pic(lay, "backgrnd", 18, 0, x, y);
        } else {
            self.draw_pic(lay, "backgrnd", 18, 0, x, y);
            self.draw_pic(lay, &cast.picture.0, cast.picture.1, 0, x + 10.0, y + 8.0);
            self.bm_text_outlined(view, 5, &name, line.0, line.1, Align::Center);
        }
    }

    /// Draw frame 0 of a picture through a frame of the first or second mask picture.
    fn draw_masked(
        &mut self,
        lay: &Layout,
        lbx: &str,
        entry: usize,
        mask: u8,
        mask_frame: usize,
        at: (f32, f32),
    ) {
        let Some((mask_lbx, mask_entry)) = self.art.shared_picture(&Role::Mask(mask)) else {
            return;
        };
        let Some(tex) = self
            .art
            .masked_picture(lbx, entry, 0, (&mask_lbx, mask_entry), mask_frame)
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
