//! The portrait picker (`Target_Wizard_Screen`): a spell aimed at a rival wizard (Spell Blast,
//! Drain Power, Subversion, Cruel Unminding) asks which of the wizards met and still alive it
//! is for, by their portraits on a stone box over the map. A click on a portrait or the number
//! key under it casts, Esc gives the spell up. The map click on one of the wizard's cities or
//! units (`App::aim_at`) still works for a spell already being aimed.

use crate::font::Style;
use crate::frame::{Align, Layout};
use crate::input::Input;
use crate::modal::Modal;
use crate::ui::{self, GOLD};
use crate::view::{App, Screen, HUMAN};
use f4x_core::game::Command;
use f4x_core::player::PlayerId;
use f4x_core::rules::exec::Target;
use f4x_core::rules::SpellId;
use macroquad::prelude::*;

/// The most portraits the box holds (a game has at most four rivals).
const SHOWN: usize = 4;
const GEM_STEP: f32 = 44.0;

/// The question: which spell, and whether it is a paid-up cast that waits for its target.
pub(crate) struct WizardPick {
    spell: SpellId,
    resolve: bool,
}

/// The key that picks the wizard in slot `n`.
pub(crate) fn slot_key(n: usize) -> KeyCode {
    [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4][n.min(3)]
}

/// Where portrait `n` of `count` sits (the left edge, in the original's pixels), the row centred
/// in the box that starts at `box_x` and is `box_w` wide.
pub(crate) fn gem_x(n: usize, count: usize, box_x: f32, box_w: f32) -> f32 {
    box_x + (box_w - GEM_STEP * count as f32) / 2.0 + GEM_STEP * n as f32
}

impl App {
    /// The wizards a spell can be aimed at: met, not defeated, not the monsters.
    pub(crate) fn pickable_wizards(&self) -> Vec<PlayerId> {
        self.game
            .player_ids()
            .into_iter()
            .filter(|&id| {
                id != HUMAN
                    && !self.game.is_neutral_player(id)
                    && self.game.have_met(HUMAN, id)
                    && self.game.player(id).is_some_and(|p| !p.is_defeated())
            })
            .take(SHOWN)
            .collect()
    }

    /// Ask which wizard `spell` is for; `resolve` for a spell paid over turns.
    pub(crate) fn open_wizard_pick(&mut self, spell: SpellId, resolve: bool) {
        self.screen = Screen::Map;
        if self.pickable_wizards().is_empty() {
            let key = if resolve {
                "spell.no_wizards_waits"
            } else {
                "spell.no_wizards"
            };
            self.log.push(self.text(key));
        } else {
            self.modals.open(WizardPick { spell, resolve });
        }
    }

    /// For screenshots: the picker for Subversion with the wizards met.
    pub(crate) fn open_test_wizard_pick(&mut self) {
        let Some(spell) = self.game.content().rules.spells.id_numbered(177)
        /* subversion */
        else {
            return;
        };
        for id in self.game.player_ids() {
            if id != HUMAN {
                self.game.make_contact(HUMAN, id);
            }
        }
        self.open_wizard_pick(spell, false);
    }
}

impl Modal for WizardPick {
    fn kind(&self) -> &'static str {
        "wizard_pick"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        let Self { spell, resolve } = *self;
        let wizards = app.pickable_wizards();
        if wizards.is_empty() {
            return None;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (bx, bw, bh) = (68.0, 185.0, 92.0);
        let by = ((crate::resolution::DESIGN_H - bh) / 2.0).floor();
        app.draw_stone_box(&lay, bx, by, bh);
        let name = app.game.content().rules.spells.get(spell).name.clone();
        let title = app.text_with(
            "spell.pick_wizard",
            &f4x_core::text::Args::new().text("spell", &name),
        );
        app.draw_styled(
            view,
            &Style::nearest(2, GOLD).align(Align::Center),
            &title,
            bx + bw / 2.0,
            by + 8.0,
        );
        let mut picked = None;
        for (n, &id) in wizards.iter().enumerate() {
            let x = gem_x(n, wizards.len(), bx, bw);
            let (portrait, banner, who) = app.game.player(id).map_or((0, 0, String::new()), |p| {
                (p.portrait(), p.banner(), p.name().to_string())
            });
            let rect = crate::art::gem_entry(app.art.wizard(portrait).gem, banner)
                .and_then(|entry| app.draw_pic(&lay, "lilwiz", entry, 0, x, by + 24.0))
                .unwrap_or_else(|| lay.rect((x, by + 24.0, 40.0, 40.0)));
            app.draw_styled(
                view,
                &Style::nearest(1, WHITE).align(Align::Center),
                &who,
                x + GEM_STEP / 2.0 - 2.0,
                by + 68.0,
            );
            if ui::hit(rect, true, input).clicked() || input.key_pressed(slot_key(n)) {
                picked = Some(id);
            }
        }
        if let Some(id) = picked {
            let target = Target::Player(id);
            let command = if resolve {
                Command::ResolveCast {
                    player: HUMAN,
                    target,
                }
            } else {
                Command::CastSpell {
                    player: HUMAN,
                    spell,
                    target: Some(target),
                    extra: 0,
                }
            };
            let events = app.game.apply(command);
            app.note(events);
            return None;
        }
        let cancel = app.text("ui.cancel");
        let cancel_at = (bx + 8.0, by + bh - 16.0);
        app.draw_styled(
            view,
            &Style::nearest(1, GOLD),
            &cancel,
            cancel_at.0,
            cancel_at.1,
        );
        let cancel_hit =
            ui::hit(lay.rect((cancel_at.0, cancel_at.1, 40.0, 9.0)), true, input).clicked();
        if cancel_hit || input.key_pressed(KeyCode::Escape) {
            let line = app.text("log.cast_cancelled");
            app.log.push(line);
            return None;
        }
        Some(Box::new(Self { spell, resolve }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portraits_are_centred_in_the_box() {
        // Two portraits in a box 185 wide starting at 68: equal room on both sides.
        let (a, b) = (gem_x(0, 2, 68.0, 185.0), gem_x(1, 2, 68.0, 185.0));
        assert_eq!(b - a, GEM_STEP);
        assert_eq!(a - 68.0, 253.0 - (b + GEM_STEP));
    }

    #[test]
    fn the_number_keys_pick_the_portraits_in_order() {
        assert_eq!(slot_key(0), KeyCode::Key1);
        assert_eq!(slot_key(3), KeyCode::Key4);
    }

    #[test]
    fn the_picker_has_its_texts() {
        let texts = crate::test_app::shipped::texts();
        for key in [
            "spell.pick_wizard",
            "spell.no_wizards",
            "spell.no_wizards_waits",
            "ui.cancel",
        ] {
            assert!(texts.has(key), "{key}");
        }
    }
}
