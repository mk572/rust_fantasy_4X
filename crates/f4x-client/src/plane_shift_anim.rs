//! The transition of a Plane Shift on the map (`Cast_Plane_Shift_Draw`, `Spells132.c`): the view
//! goes to the other plane, where the stack now stands, and the square it left fades out over it.
//! The old square (its ground and its stack) is drawn once into a small off-screen picture and
//! drawn again at falling opacity; no screenshot of the screen is taken. It is skipped with Spell
//! Animations off, and a rival's shift shows only when the player may see rival spells.
//!
//! Tests: in this file.

use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::preferences::Pref;
use crate::view::{App, HUMAN};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use macroquad::prelude::*;

/// How long the fade lasts. The original takes 21 page flips.
const DURATION: f64 = 0.8;

#[derive(Debug, Clone)]
pub(crate) struct PlaneShiftAnim {
    from: Position,
    to: Position,
    units: Vec<hecs::Entity>,
    started: Option<f64>,
}

impl Modal for PlaneShiftAnim {
    fn kind(&self) -> &'static str {
        kind::PLANE_SHIFT_ANIM
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.plane_shift_frame(*self, input))
    }
}

/// How opaque the old square still is after `seconds`: 1 at the start, 0 at the end.
fn old_square_opacity(seconds: f64) -> f32 {
    (1.0 - seconds / DURATION).clamp(0.0, 1.0) as f32
}

impl App {
    /// Queue the transition of the stack `units` shifted from `from` to `to`.
    pub(crate) fn announce_plane_shift(
        &mut self,
        player: PlayerId,
        from: Position,
        to: Position,
        units: Vec<hecs::Entity>,
    ) {
        if !self.prefs.get(Pref::SpellAnimations) {
            return;
        }
        if player != HUMAN && !self.rival_spell_seen(player, Some(from), false) {
            return;
        }
        self.modals.enqueue(PlaneShiftAnim {
            from,
            to,
            units,
            started: None,
        });
    }

    fn plane_shift_frame(
        &mut self,
        mut anim: PlaneShiftAnim,
        input: &Input,
    ) -> Option<PlaneShiftAnim> {
        let first = anim.started.is_none();
        let started = *anim.started.get_or_insert_with(get_time);
        if first {
            // The map shows the plane the stack is on now.
            self.center_on(anim.to);
        }
        let seconds = get_time() - started;
        if seconds >= DURATION || input.dismisses() {
            return None;
        }
        let r = self.map_rect();
        let t = self.tile_size();
        let square = vec2(
            r.x + (anim.from.x as f32 - self.cam.x) * t.x,
            r.y + (anim.from.y as f32 - self.cam.y) * t.y,
        );
        // The square the stack left, composed off screen.
        let (w, h) = (t.x.ceil() as u32, t.y.ceil() as u32);
        let target = render_target(w, h);
        target.texture.set_filter(FilterMode::Nearest);
        set_camera(&Camera2D {
            zoom: vec2(2.0 / w as f32, 2.0 / h as f32),
            target: vec2(w as f32 / 2.0, h as f32 / 2.0),
            render_target: Some(target.clone()),
            ..Default::default()
        });
        clear_background(Color::new(0.0, 0.0, 0.0, 0.0));
        crate::map_draw::draw_square(
            &self.game,
            &mut self.art,
            anim.from.plane,
            anim.from.x,
            anim.from.y,
            0.0,
            0.0,
            t,
            (get_time() * 2.5) as usize,
        );
        self.draw_stack_icon(Vec2::ZERO, t, &anim.units, false);
        set_default_camera();
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        let dpi = screen_dpi_scale();
        gl.quad_gl.scissor(Some((
            (r.x * dpi) as i32,
            (r.y * dpi) as i32,
            (r.w * dpi) as i32,
            (r.h * dpi) as i32,
        )));
        draw_texture_ex(
            &target.texture,
            square.x,
            square.y,
            Color::new(1.0, 1.0, 1.0, old_square_opacity(seconds)),
            DrawTextureParams {
                dest_size: Some(t),
                ..Default::default()
            },
        );
        gl.flush();
        gl.quad_gl.scissor(None);
        Some(anim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::map::Plane;
    use f4x_def::testdir::TestResult;

    fn squares(app: &App) -> Result<(Position, Position, Vec<hecs::Entity>), &'static str> {
        let unit = *app
            .game
            .units()
            .player_units(HUMAN)
            .first()
            .ok_or("the human has no unit")?;
        let from = app
            .game
            .units()
            .unit_position(unit)
            .ok_or("the unit has no position")?;
        let to = Position {
            plane: match from.plane {
                Plane::Arcanus => Plane::Myrror,
                Plane::Myrror => Plane::Arcanus,
            },
            ..from
        };
        Ok((from, to, vec![unit]))
    }

    #[test]
    fn the_old_square_fades_from_opaque_to_clear() {
        assert_eq!(old_square_opacity(0.0), 1.0);
        assert!((old_square_opacity(DURATION / 2.0) - 0.5).abs() < 1e-6);
        assert_eq!(old_square_opacity(DURATION), 0.0);
        assert_eq!(old_square_opacity(DURATION * 3.0), 0.0, "never below clear");
    }

    #[test]
    fn a_plane_shift_fades_unless_spell_animations_are_off() -> TestResult {
        let mut app = App::for_test();
        let (from, to, units) = squares(&app)?;
        app.prefs.set(Pref::SpellAnimations, false);
        app.announce_plane_shift(HUMAN, from, to, units.clone());
        assert!(!app.modals.has(kind::PLANE_SHIFT_ANIM));
        app.prefs.set(Pref::SpellAnimations, true);
        app.announce_plane_shift(HUMAN, from, to, units);
        assert!(app.modals.has(kind::PLANE_SHIFT_ANIM));
        Ok(())
    }

    #[test]
    fn a_rivals_plane_shift_stays_unseen_without_detect_magic() -> TestResult {
        let mut app = App::for_test();
        let (from, to, units) = squares(&app)?;
        app.prefs.set(Pref::SpellAnimations, true);
        app.announce_plane_shift(PlayerId(1), from, to, units);
        assert!(!app.modals.has(kind::PLANE_SHIFT_ANIM));
        Ok(())
    }

    #[test]
    fn plane_shift_has_no_square_picture_besides_the_fade() -> TestResult {
        let mut app = App::for_test();
        let (from, ..) = squares(&app)?;
        app.prefs.set(Pref::SpellAnimations, true);
        let spell = app
            .game
            .content()
            .rules
            .spells
            .id_numbered(132 /* plane_shift */)
            .ok_or("no plane_shift")?;
        app.announce_square_spell(HUMAN, spell, from);
        assert!(!app.modals.has(kind::SQUARE_ANIM));
        Ok(())
    }
}
