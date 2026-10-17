//! Spells cast in a battle, as the original plays them (`Combat_Spell_Animation`): the spell's
//! picture on its target, a bolt flying in, a stroke of lightning, or the screen tinted in the
//! realm's colour, under the line that says who cast what.

use super::*;
use crate::frame::Layout;
use crate::spell_fx::{self, SpellFx};
use battle_anim::{BOLT_FRAME, SPELL_FRAME};
use f4x_core::rules::effects::Effect;

impl App {
    /// Turn a cast into the animation the original plays for it: `caster` is the wizard's side
    /// or the casting unit, `before` the battle as it stood.
    pub(super) fn spell_animation(
        &mut self,
        before: &Battle,
        spell: SpellId,
        caster: Result<Side, usize>,
    ) -> Option<Anim> {
        let (id, realm, summons, name) = {
            let def = self.game.content().rules.spells.get(spell);
            let summons = def
                .effects
                .iter()
                .any(|e| matches!(e.effect, Effect::Summon { .. }));
            (def.id.clone(), def.realm.clone(), summons, def.name.clone())
        };
        let fx = spell_fx::fx_for(&id, &realm, summons);
        let side = match caster {
            Ok(side) => side,
            Err(unit) => before.unit(unit)?.side,
        };
        let line = match caster {
            Ok(side) => format!(
                "{} has cast {name}",
                self.player_label(before.player_of(side))
            ),
            Err(unit) => {
                let u = before.unit(unit)?;
                let verb = if u.figures_start > 1 { "have" } else { "has" };
                format!("The {} {verb} cast {name}", u.name)
            }
        };
        // Spells that strike a side's units play over each of them, a few frames apart.
        let cells: Vec<(Pos, usize)> = match spell_fx::strikes_a_side(&id) {
            Some(enemies) => {
                let target = if enemies { side.other() } else { side };
                before
                    .active_units(target)
                    .map(|u| (u.cell, (u.id * 3) % 5))
                    .collect()
            }
            None => self
                .battle_ui
                .cast_target
                .into_iter()
                .map(|c| (c, 0))
                .collect(),
        };
        let frames = match fx {
            SpellFx::Cell { lbx, entry, .. } => self.art.frames(lbx, entry).max(1),
            SpellFx::Bolt { bolt, .. } => bolt.frames(),
            SpellFx::Lightning | SpellFx::Tint { .. } => 1,
        };
        // Without a square to play on only the tint plays (a message, for the rest).
        let plays = matches!(fx, SpellFx::Tint { .. }) || !cells.is_empty();
        Some(Anim::Spell {
            fx: if plays {
                fx
            } else {
                SpellFx::Tint { realm: 5 }
            },
            frames,
            cells: if plays { cells } else { Vec::new() },
            line,
        })
    }

    /// The point on the screen (in the grid's native pixels) of a square for the pictures of
    /// spells (`Combat_Grid_Screen_Coordinates` with an offset of 4 and 4).
    fn spell_point(pos: Pos) -> (f32, f32) {
        let (x, y) = cell_origin(pos);
        (x + 14.0, y + 8.0)
    }

    /// Draw the picture of the spell being played.
    pub(super) fn draw_spell_fx(&mut self, (ox, oy, scale): (f32, f32, f32)) {
        let Some((
            Anim::Spell {
                fx, frames, cells, ..
            },
            t,
        )) = self.battle_ui.playing.clone()
        else {
            return;
        };
        let place = |app: &mut App, lbx: &str, entry: usize, frame: usize, at: (f32, f32)| {
            if let Some(tex) = app.art.picture(lbx, entry, frame) {
                draw_texture_ex(
                    tex,
                    ox + at.0 * scale,
                    oy + at.1 * scale,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(tex.size() * scale),
                        ..Default::default()
                    },
                );
            }
        };
        match fx {
            SpellFx::Cell { lbx, entry, big } => {
                for (pos, delay) in cells {
                    let frame = (t / SPELL_FRAME) as isize - delay as isize;
                    if frame < 0 || frame as usize >= frames {
                        continue;
                    }
                    let (px, py) = Self::spell_point(pos);
                    let from = if big { (28.0, 30.0) } else { (14.0, 25.0) };
                    place(self, lbx, entry, frame as usize, (px - from.0, py - from.1));
                }
            }
            SpellFx::Bolt { lbx, entry, bolt } => {
                let Some(&(pos, _)) = cells.first() else {
                    return;
                };
                let (px, py) = Self::spell_point(pos);
                let (rest, start, step) = bolt.path();
                let rest = (px + rest.0, py + rest.1);
                // Eleven quick frames of flight, then the burst where it landed.
                let flight = 11.0 * BOLT_FRAME;
                let frame_num = if t < flight {
                    (t / BOLT_FRAME) as usize
                } else {
                    11 + ((t - flight) / SPELL_FRAME) as usize
                };
                if frame_num >= frames {
                    return;
                }
                let picture = match bolt {
                    spell_fx::Bolt::Fireball => frame_num,
                    spell_fx::Bolt::Doom => frame_num / 4,
                    _ if frame_num == frames - 1 => 3,
                    _ => frame_num % 3,
                };
                let at = if frame_num > 10 {
                    rest
                } else {
                    (
                        rest.0 + start.0 + step.0 * frame_num as f32,
                        rest.1 + start.1 + step.1 * frame_num as f32,
                    )
                };
                place(self, lbx, entry, picture, at);
            }
            SpellFx::Lightning => {
                let Some(&(pos, _)) = cells.first() else {
                    return;
                };
                // A pause, two flickering frames of the stroke, a pause.
                if (0.28..0.5).contains(&t) {
                    let (px, py) = Self::spell_point(pos);
                    let frame = (t * 20.0) as usize % 4;
                    place(self, "cmbtfx", 24, frame, (px, py - 199.0));
                }
            }
            SpellFx::Tint { .. } => {}
        }
    }

    /// The tint of a battlefield spell over the whole screen, and the line telling who cast what
    /// (`Combat_Cast_Spell_Message`) over the map's top.
    pub(super) fn draw_spell_overlay(&mut self) {
        let Some((Anim::Spell { fx, line, .. }, t)) = self.battle_ui.playing.clone() else {
            return;
        };
        let lay = Layout::new();
        if let SpellFx::Tint { realm } = fx {
            let (r, g, b) = spell_fx::realm_tint(realm);
            // Up to 40 percent of the colour and back again.
            let phase = (t / 0.9).clamp(0.0, 1.0);
            let strength = 0.4 * (1.0 - (2.0 * phase - 1.0).abs());
            draw_rectangle(
                0.0,
                0.0,
                screen_width(),
                screen_height(),
                Color::new(r, g, b, strength),
            );
        }
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let width = self.art.text_width(1, &line);
        let (x1, x2) = (160.0 - width / 2.0 - 5.0, 165.0 + width / 2.0);
        Self::draw_hud_box(&lay, (x1, 5.0, x2 - x1, 11.0));
        let ink = self.art.minimap().named("scan_text", WHITE);
        self.flat_text(view, 1, &line, (160.0, 7.0), Align::Center, ink, BLACK);
    }
}
