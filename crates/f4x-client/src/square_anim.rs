//! The animation of a spell that lands on a unit or a square of the map (`Spell_Animation_Screen`,
//! `SBookScr.c`): the map scrolls to the square, then the spell's picture (its own, or its
//! realm's `specfx` picture) plays on it, at least twelve stages. It is skipped with Spell
//! Animations off, as the original does. A rival's cast shows with Enemy Spells on and the
//! square explored.

use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::view::{App, HUMAN};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::rules::{SpellId, SpellTarget};
use f4x_data::spell_art::Role;
use macroquad::prelude::*;

/// Two ticks of the original's 55 ms timer a stage.
const STAGE: f64 = 0.11;
/// The picture plays at least this many stages.
const MIN_STAGES: usize = 12;

#[derive(Debug, Clone)]
pub(crate) struct SquareAnim {
    at: Position,
    picture: (String, usize),
    started: Option<f64>,
}

impl Modal for SquareAnim {
    fn kind(&self) -> &'static str {
        kind::SQUARE_ANIM
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.square_anim_frame(*self, input))
    }
}

impl App {
    /// Queue the animation of a spell that landed on the square `at`.
    pub(crate) fn announce_square_spell(&mut self, player: PlayerId, spell: SpellId, at: Position) {
        if !self.prefs.get(Pref::SpellAnimations) {
            return;
        }
        if player != HUMAN && !self.rival_spell_seen(player, Some(at), false) {
            return;
        }
        if let Some(anim) = self.square_anim_for(spell, at, None) {
            self.modals.enqueue(anim);
        }
    }

    fn square_anim_for(
        &self,
        spell: SpellId,
        at: Position,
        started: Option<f64>,
    ) -> Option<SquareAnim> {
        let def = self.game.content().rules.spells.get(spell);
        // The summons, city spells and global enchantments have scenes of their own.
        if !matches!(
            def.target,
            SpellTarget::OwnUnit
                | SpellTarget::EnemyUnit
                | SpellTarget::AnyUnit
                | SpellTarget::Tile
        ) || crate::summon_scene::summoned_by(&def.effects).is_some()
        {
            return None;
        }
        let picture = self
            .art
            .spell_picture(&def.id, &Role::Cast)
            .or_else(|| {
                self.art
                    .spell_picture(&def.id, &Role::Realm(def.realm.clone()))
            })
            .or_else(|| self.art.shared_picture(&Role::Realm(def.realm.clone())))?;
        Some(SquareAnim {
            at,
            picture,
            started,
        })
    }

    /// For screenshots: the animation of `id` on the first city's square.
    pub fn open_test_square(&mut self, id: &str, seconds: f64) {
        let Some(spell) = self.game.content().rules.spells.id_of(id) else {
            eprintln!("unknown spell {id}");
            return;
        };
        let Some(city) = self.game.economy().cities().first().map(|c| c.0) else {
            return;
        };
        let at = self.city_position(city);
        match self.square_anim_for(spell, at, Some(get_time() - seconds)) {
            Some(anim) => self.modals.enqueue(anim),
            None => eprintln!("{id} has no square animation"),
        }
    }

    pub(crate) fn square_anim_frame(
        &mut self,
        mut anim: SquareAnim,
        input: &Input,
    ) -> Option<SquareAnim> {
        let first = anim.started.is_none();
        let started = *anim.started.get_or_insert_with(get_time);
        if first {
            self.center_on(anim.at);
        }
        let frames = self.art.frames(&anim.picture.0, anim.picture.1).max(1);
        let stages = MIN_STAGES.max(frames);
        let stage = ((get_time() - started) / STAGE) as usize;
        if stage >= stages || crate::global_spell::any_input(input) {
            return None;
        }
        let r = self.map_rect();
        let t = self.tile_size();
        let square = vec2(
            r.x + (anim.at.x as f32 - self.cam.x) * t.x,
            r.y + (anim.at.y as f32 - self.cam.y) * t.y,
        );
        let tex = self
            .art
            .picture(&anim.picture.0, anim.picture.1, stage % frames)
            .cloned();
        if let Some(tex) = tex {
            // The original draws it 20 pixels left of and 18 above the square's corner.
            let k = t.x / 20.0;
            draw_texture_ex(
                &tex,
                square.x - t.x,
                square.y - t.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(crate::resolution::logical_size(&tex) * k),
                    ..Default::default()
                },
            );
        }
        Some(anim)
    }
}
