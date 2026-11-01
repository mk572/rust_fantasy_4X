//! The extra-mana slider (`Spellbook_Mana_Adder_Screen`): a spell that gets stronger with mana
//! (Disenchant Area and True, Disjunction, Disjunction True, Spell Binding; see
//! `Game::spell_takes_extra_mana`) asks how much to put into it before it is cast. The slider
//! runs from nothing to as much again as the spell costs; "Additional Power: +N" shows the
//! amount. The keys Left and Right (or Down and Up) change it by one mana, a click or drag on the
//! bar sets it, OK (Enter) casts and Esc gives the spell up. The amount goes with
//! `Command::CastSpell`.

use crate::font::Style;
use crate::frame::{Align, Layout};
use crate::input::Input;
use crate::modal::Modal;
use crate::ui::{self, GOLD};
use crate::view::App;
use f4x_core::rules::SpellId;
use macroquad::prelude::*;

/// The bar in the box's own coordinates: where it starts, how long it is.
const BAR_X: f32 = 12.0;
const BAR_W: f32 = 100.0;
const BAR_Y: f32 = 22.0;

/// The question: which spell, and how much extra so far.
pub(crate) struct ExtraMana {
    spell: SpellId,
    extra: i32,
}

impl ExtraMana {
    pub(crate) fn new(spell: SpellId) -> Self {
        Self { spell, extra: 0 }
    }
}

/// The most extra mana a spell of `cost` takes: as much again as it costs.
pub(crate) fn most_extra(cost: i32) -> i32 {
    cost.max(0)
}

/// The extra mana for a click `along` pixels along a bar `BAR_W` long, for at most `most`.
pub(crate) fn extra_at(along: f32, most: i32) -> i32 {
    let share = (along / BAR_W).clamp(0.0, 1.0);
    (share * most as f32).round() as i32
}

/// Where the slider's star sits, in pixels along the bar, for `extra` of at most `most`.
pub(crate) fn thumb_at(extra: i32, most: i32) -> f32 {
    if most <= 0 {
        0.0
    } else {
        BAR_W * extra.clamp(0, most) as f32 / most as f32
    }
}

impl App {
    /// For screenshots: the slider of Disenchant Area, a third of the way along.
    pub(crate) fn open_test_extra_mana(&mut self) {
        let Some(spell) = self.game.content().rules.spells.id_numbered(204)
        /* disenchant_area */
        else {
            return;
        };
        let most = most_extra(self.game.content().rules.spells.get(spell).cost as i32);
        self.modals.open(ExtraMana {
            spell,
            extra: most / 3,
        });
    }
}

impl Modal for ExtraMana {
    fn kind(&self) -> &'static str {
        "extra_mana"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        let Self { spell, mut extra } = *self;
        let def = app.game.content().rules.spells.get(spell);
        let base = def.cost as i32;
        let most = most_extra(base);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);

        // The box, in the middle of the screen.
        let back = app.art_picture("spellscr", 5, 0);
        let (bw, bh) = back.as_ref().map_or((154.0, 48.0), |t| {
            let s = crate::resolution::logical_size(t);
            (s.x, s.y)
        });
        let (x, y) = (
            ((320.0 - bw) / 2.0).floor() + 4.0,
            ((200.0 - bh) / 2.0).floor() - 1.0,
        );
        if app
            .draw_pic(&lay, "spellscr", 5, 0, x - 4.0, y + 1.0)
            .is_none()
        {
            let r = lay.rect((x - 4.0, y + 1.0, bw, bh));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.06, 0.04, 0.95));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, GOLD);
        }

        let bar = lay.rect((x + BAR_X, y + BAR_Y - 3.0, BAR_W, 9.0));
        if input.mouse_down(MouseButton::Left) && bar.contains(input.mouse_position().into()) {
            extra = extra_at((input.mouse_position().0 - bar.x) / lay.scale, most);
        }
        let step = if input.key_down(KeyCode::LeftShift) || input.key_down(KeyCode::RightShift) {
            5
        } else {
            1
        };
        for (key, by) in [
            (KeyCode::Left, -step),
            (KeyCode::Down, -step),
            (KeyCode::Right, step),
            (KeyCode::Up, step),
        ] {
            if input.key_pressed(key) {
                extra = (extra + by).clamp(0, most);
            }
        }

        let label = app.text("spell.additional_power");
        let amount = format!("+{extra}");
        let style = Style::nearest(4, WHITE);
        app.draw_styled(view, &style, &label, x + 4.0, y + 8.0);
        app.draw_styled(
            view,
            &style.align(Align::Right),
            &amount,
            x + 144.0,
            y + 8.0,
        );
        // The bar fills up to the star.
        let at = thumb_at(extra, most);
        let filled = lay.rect((x + BAR_X, y + BAR_Y, at, 3.0));
        draw_rectangle(filled.x, filled.y, filled.w, filled.h, GOLD);
        let rest = lay.rect((x + BAR_X + at, y + BAR_Y, BAR_W - at, 3.0));
        draw_rectangle(
            rest.x,
            rest.y,
            rest.w,
            rest.h,
            Color::new(0.2, 0.15, 0.1, 1.0),
        );
        app.draw_pic(&lay, "spellscr", 3, 0, x + at + 5.0, y + 22.0);

        // OK on the right, Cancel on the left.
        let ok = app.draw_pic(&lay, "spellscr", 42, 0, x + 123.0, y + 18.0);
        let ok_hit = ok.is_some_and(|r| ui::hit(r, true, input).clicked());
        let cancel_text = app.text("ui.cancel");
        let cancel_at = (x + 4.0, y + bh + 3.0);
        app.draw_styled(
            view,
            &Style::nearest(1, GOLD),
            &cancel_text,
            cancel_at.0,
            cancel_at.1,
        );
        let cancel_hit =
            ui::hit(lay.rect((cancel_at.0, cancel_at.1, 40.0, 9.0)), true, input).clicked();

        if ok_hit || input.key_pressed(KeyCode::Enter) {
            app.cast_extra = extra;
            app.continue_cast(spell);
            return None;
        }
        if cancel_hit || input.key_pressed(KeyCode::Escape) {
            let line = app.text("log.cast_cancelled");
            app.log.push(line);
            return None;
        }
        Some(Box::new(Self { spell, extra }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_along_the_bar_sets_the_share_of_the_most() {
        assert_eq!(extra_at(0.0, 50), 0);
        assert_eq!(extra_at(BAR_W / 2.0, 50), 25);
        assert_eq!(extra_at(BAR_W * 3.0, 50), 50);
        assert_eq!(extra_at(-4.0, 50), 0);
    }

    #[test]
    fn the_star_sits_where_the_amount_is() {
        assert_eq!(thumb_at(0, 50), 0.0);
        assert_eq!(thumb_at(25, 50), BAR_W / 2.0);
        assert_eq!(thumb_at(99, 50), BAR_W);
        assert_eq!(thumb_at(5, 0), 0.0);
    }

    #[test]
    fn a_spell_takes_as_much_again_as_it_costs() {
        assert_eq!(most_extra(50), 50);
        assert_eq!(most_extra(-3), 0);
    }

    #[test]
    fn the_slider_has_its_texts() {
        let texts = crate::test_app::shipped::texts();
        for key in ["spell.additional_power", "ui.cancel", "log.cast_cancelled"] {
            assert!(texts.has(key), "{key}");
        }
    }
}
