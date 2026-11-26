//! The target wizard screen (`Target_Wizard_Screen`, `Spells137.c`): a spell aimed at a rival wizard
//! (Spell Blast, Drain Power, Subversion, Cruel Unminding) asks which of them it is for, on the
//! original's panel (`spellscr` 0, or 72 for Spell Blast) with four gem slots, two by two. A wizard
//! met and alive shows its gem, one not met the grey gem, a defeated one the broken grey gem. A click
//! on a gem or the number key under it casts, Cancel or Esc gives the spell up. The map click on one
//! of the wizard's cities or units (`App::aim_at`) still works for a spell already being aimed.
//!
//! The original has four rivals at most; here the list scrolls by a row of two with the arrows
//! beside the slots (`spellscr` 43 and 44), the mouse wheel and Up/Down (task 643).

use crate::font::Style;
use crate::frame::{Align, Layout};
use crate::input::Input;
use crate::modal::Modal;
use crate::scroll::{self, Window};
use crate::ui::{self, GOLD};
use crate::view::{App, Screen, HUMAN};
use f4x_core::player::PlayerId;
use f4x_core::rules::exec::Target;
use f4x_core::world::Command;
use f4x_def::rules::SpellId;
use macroquad::prelude::*;

/// The panel's top left corner, and the gem of the first slot, in the original's pixels; the next
/// slots are `SLOT_STEP` further across and down.
const PANEL: (f32, f32) = (45.0, 10.0);
const PANEL_SIZE: (f32, f32) = (166.0, 181.0);
const SLOT: (f32, f32) = (69.0, 47.0);
const SLOT_STEP: (f32, f32) = (77.0, 61.0);
const ACROSS: usize = 2;
const ROWS_IN_VIEW: usize = 2;
const SLOTS: usize = ACROSS * ROWS_IN_VIEW;
/// The gem is 41 wide; the name goes under it.
const GEM_W: f32 = 41.0;
const NAME_DOWN: f32 = 49.0;
/// Cancel, and the wider panel of Spell Blast has it elsewhere.
const CANCEL: (f32, f32) = (90.0, 165.0);
const CANCEL_BLAST: (f32, f32) = (128.0, 165.0);
/// The scroll arrows (7 x 9), at the right of the slots.
const ARROW_UP: (f32, f32) = (196.0, 52.0);
const ARROW_DOWN: (f32, f32) = (196.0, 140.0);
/// The title, over the panel's top, in two lines.
const TITLE_X: f32 = 128.0;
const TITLE_Y: f32 = 19.0;
const TITLE_LINE: f32 = 9.0;
const TITLE_FITS: usize = 26;

/// What a rival is to the one choosing: a wizard that can be picked, one not met yet (grey gem) or
/// one that is gone (broken gem).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Standing {
    Pickable,
    Unmet,
    Defeated,
}

/// The question: which spell, whether it is a paid-up cast that waits for its target, and the
/// row the list is scrolled to.
pub(crate) struct WizardPick {
    spell: SpellId,
    resolve: bool,
    first_row: usize,
}

/// The key that picks the wizard in slot `n`.
pub(crate) fn slot_key(n: usize) -> KeyCode {
    [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4][n.min(SLOTS - 1)]
}

/// Where the gem of slot `n` (0 to 3, row by row) is, in the original's pixels.
pub(crate) fn slot_at(n: usize) -> (f32, f32) {
    (
        SLOT.0 + SLOT_STEP.0 * (n % ACROSS) as f32,
        SLOT.1 + SLOT_STEP.1 * (n / ACROSS) as f32,
    )
}

/// The rows of two in a list of `count` wizards, and the window over them.
pub(crate) fn rows_window(count: usize, first_row: usize) -> Window {
    Window::new(first_row, ROWS_IN_VIEW, count.div_ceil(ACROSS))
}

/// The wizards in view for a window: indices into the list, at most four.
pub(crate) fn in_view(window: &Window, count: usize) -> Vec<usize> {
    (window.first * ACROSS..(window.first + window.shown) * ACROSS)
        .filter(|&n| n < count)
        .collect()
}

/// The title cut into two lines about the middle, at a space, so it fits the panel's top.
pub(crate) fn title_lines(title: &str) -> Vec<String> {
    if title.chars().count() <= TITLE_FITS {
        return vec![title.to_string()];
    }
    let middle = title.chars().count() / 2;
    let cut = title
        .char_indices()
        .filter(|(_, c)| *c == ' ')
        .min_by_key(|(i, _)| title[..*i].chars().count().abs_diff(middle))
        .map(|(i, _)| i);
    match cut {
        Some(i) => vec![title[..i].to_string(), title[i + 1..].to_string()],
        None => vec![title.to_string()],
    }
}

impl App {
    /// The rivals the panel lists: every wizard but the human, the monsters left out.
    pub(crate) fn opponent_wizards(&self) -> Vec<(PlayerId, Standing)> {
        self.world
            .player_ids()
            .into_iter()
            .filter(|&id| id != HUMAN && self.world.is_wizard(id))
            .map(|id| {
                let standing = if self.world.player(id).is_some_and(|p| p.is_defeated()) {
                    Standing::Defeated
                } else if self.world.have_met(HUMAN, id) {
                    Standing::Pickable
                } else {
                    Standing::Unmet
                };
                (id, standing)
            })
            .collect()
    }

    /// The wizards a spell can be aimed at: met, not defeated, not the monsters.
    pub(crate) fn pickable_wizards(&self) -> Vec<PlayerId> {
        self.opponent_wizards()
            .into_iter()
            .filter(|&(_, s)| s == Standing::Pickable)
            .map(|(id, _)| id)
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
            self.modals.open(WizardPick {
                spell,
                resolve,
                first_row: 0,
            });
        }
    }

    /// For screenshots: the picker for Subversion with the wizards met.
    pub(crate) fn open_test_wizard_pick(&mut self) {
        let Some(spell) = self
            .world
            .content()
            .rules
            .spells
            .defs()
            .iter()
            .find(|d| f4x_core::spell::wizard::WizardEffect::Subversion.is_in(d))
            .and_then(|d| self.world.content().rules.spells.id_numbered(d.num))
        else {
            return;
        };
        for id in self.world.player_ids() {
            if id != HUMAN {
                self.world.make_contact(HUMAN, id);
            }
        }
        self.open_wizard_pick(spell, false);
    }

    fn draw_target_panel(&mut self, lay: &Layout, blast: bool) {
        let entry = if blast { 72 } else { 0 };
        if self
            .draw_pic(lay, "spellscr", entry, 0, PANEL.0, PANEL.1)
            .is_none()
        {
            let r = lay.rect((PANEL.0, PANEL.1, PANEL_SIZE.0, PANEL_SIZE.1));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.06, 0.04, 0.95));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, GOLD);
        }
    }

    /// The gem of a rival in its standing: its own, the grey one, or the broken one.
    fn draw_rival_gem(
        &mut self,
        lay: &Layout,
        id: PlayerId,
        standing: Standing,
        at: (f32, f32),
    ) -> Rect {
        let (portrait, banner) = self
            .world
            .player(id)
            .map_or((0, 0), |p| (p.portrait(), p.banner()));
        let drawn = match standing {
            // A wizard whose gem cannot be made (no portrait picture, no banner colour) shows the
            // grey one rather than a hole.
            Standing::Pickable => self
                .draw_gem(lay, portrait, banner, at.0, at.1)
                .or_else(|| self.draw_pic(lay, "magic", 6, 0, at.0, at.1)),
            Standing::Unmet => self.draw_pic(lay, "magic", 6, 0, at.0, at.1),
            Standing::Defeated => self.draw_pic(lay, "magic", 51, 0, at.0, at.1),
        };
        drawn.unwrap_or_else(|| lay.rect((at.0, at.1, GEM_W, 43.0)))
    }

    /// One scroll arrow, shown only when the list can move that way; true when it was clicked.
    fn list_arrow(
        &mut self,
        lay: &Layout,
        entry: usize,
        at: (f32, f32),
        enabled: bool,
        input: &Input,
    ) -> bool {
        if !enabled {
            return false;
        }
        let rect = lay.rect((at.0, at.1, 7.0, 9.0));
        let frame = usize::from(ui::hit(rect, true, input) != ui::Hit::None);
        self.draw_pic(lay, "spellscr", entry, frame, at.0, at.1);
        ui::hit(rect, true, input).clicked()
    }
}

impl Modal for WizardPick {
    fn kind(&self) -> &'static str {
        "wizard_pick"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        let Self {
            spell,
            resolve,
            first_row,
        } = *self;
        let rivals = app.opponent_wizards();
        if !rivals.iter().any(|&(_, s)| s == Standing::Pickable) {
            return None;
        }
        let def = app.world.content().rules.spells.get(spell);
        let blast = f4x_core::spell::wizard::WizardEffect::SpellBlast.is_in(def);
        let name_term = app.names.spell_term(def.num);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        app.draw_target_panel(&lay, blast);

        let title = app.text_with(
            "spell.pick_wizard",
            &crate::text::Args::new().term("spell", name_term.clone()),
        );
        for (n, line) in title_lines(&title).iter().enumerate() {
            app.draw_styled(
                view,
                &Style::nearest(1, WHITE).align(Align::Center),
                line,
                TITLE_X,
                TITLE_Y + TITLE_LINE * n as f32,
            );
        }

        // The list scrolls a row of two at a time.
        let mut window = rows_window(rivals.len(), first_row);
        if window.scrolls() {
            let by = scroll::step(
                input.mouse_wheel().1,
                input.key_pressed(KeyCode::Up),
                input.key_pressed(KeyCode::Down),
            );
            let up = app.list_arrow(&lay, 43, ARROW_UP, window.can_up(), input);
            let down = app.list_arrow(&lay, 44, ARROW_DOWN, window.can_down(), input);
            window = window.moved(by - i32::from(up) + i32::from(down));
        }

        let mut picked = None;
        for (slot, &n) in in_view(&window, rivals.len()).iter().enumerate() {
            let (id, standing) = rivals[n];
            let at = slot_at(slot);
            let rect = app.draw_rival_gem(&lay, id, standing, at);
            let who = app
                .world
                .player(id)
                .map_or_else(String::new, |p| p.name().to_string());
            app.draw_styled(
                view,
                &Style::nearest(1, WHITE).align(Align::Center),
                &who,
                at.0 + GEM_W / 2.0,
                at.1 + NAME_DOWN,
            );
            let clicked = ui::hit(rect, standing == Standing::Pickable, input).clicked();
            if standing == Standing::Pickable && (clicked || input.key_pressed(slot_key(slot))) {
                picked = Some(id);
            }
        }
        // A slot with no wizard in it shows the broken gem.
        for slot in in_view(&window, rivals.len()).len()..SLOTS {
            let at = slot_at(slot);
            app.draw_pic(&lay, "magic", 51, 0, at.0, at.1);
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
            let events = app.world.apply(command);
            app.note(events);
            return None;
        }

        let cancel_at = if blast { CANCEL_BLAST } else { CANCEL };
        let cancel = lay.rect((cancel_at.0, cancel_at.1, 74.0, 17.0));
        let held = ui::hit(cancel, true, input);
        let drawn = app.draw_pic(
            &lay,
            "spellscr",
            71,
            usize::from(held == ui::Hit::Pressed),
            cancel_at.0,
            cancel_at.1,
        );
        if drawn.is_none() {
            let text = app.text("ui.cancel");
            app.draw_styled(
                view,
                &Style::nearest(1, GOLD).align(Align::Center),
                &text,
                cancel_at.0 + 37.0,
                cancel_at.1 + 4.0,
            );
        }
        if held.clicked() || input.key_pressed(KeyCode::Escape) {
            let line = app.text("log.cast_cancelled");
            app.log.push(line);
            return None;
        }
        Some(Box::new(Self {
            spell,
            resolve,
            first_row: window.first,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slots_are_two_by_two_as_the_originals() {
        assert_eq!(slot_at(0), (69.0, 47.0));
        assert_eq!(slot_at(1), (146.0, 47.0));
        assert_eq!(slot_at(2), (69.0, 108.0));
        assert_eq!(slot_at(3), (146.0, 108.0));
        // The gems and their names stay inside the panel.
        for n in 0..SLOTS {
            let (x, y) = slot_at(n);
            assert!(x + GEM_W <= PANEL.0 + PANEL_SIZE.0 && y + 43.0 <= PANEL.1 + PANEL_SIZE.1);
        }
        assert!(ARROW_UP.0 + 7.0 <= PANEL.0 + PANEL_SIZE.0 && ARROW_DOWN.1 + 9.0 <= CANCEL.1);
    }

    #[test]
    fn four_rivals_or_fewer_fit_and_more_scroll_a_row_of_two_at_a_time() {
        for count in 0..=4 {
            assert!(!rows_window(count, 0).scrolls(), "{count}");
        }
        let five = rows_window(5, 0);
        assert!(five.scrolls() && five.can_down() && !five.can_up());
        assert_eq!(in_view(&five, 5), vec![0, 1, 2, 3]);
        let scrolled = five.moved(1);
        // The last row has one wizard only.
        assert_eq!(in_view(&scrolled, 5), vec![2, 3, 4]);
        assert!(!scrolled.can_down() && scrolled.can_up());
        // Ten wizards: five rows, the window never leaves them.
        let ten = rows_window(10, 99);
        assert_eq!(in_view(&ten, 10), vec![6, 7, 8, 9]);
    }

    #[test]
    fn a_long_title_is_cut_at_a_space_near_the_middle() {
        assert_eq!(title_lines("Cast Drain Power"), vec!["Cast Drain Power"]);
        let lines = title_lines("Cast Cruel Unminding on which wizard?");
        assert_eq!(lines, vec!["Cast Cruel Unminding", "on which wizard?"]);
        assert!(lines.iter().all(|l| l.chars().count() <= TITLE_FITS));
        assert_eq!(
            title_lines("Unbrokenwordthatistoolongtofitanywhere"),
            vec!["Unbrokenwordthatistoolongtofitanywhere"]
        );
    }

    #[test]
    fn the_number_keys_pick_the_slots_in_order() {
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
