//! The screen that picks a rival's overland enchantment (`Spell_Target_Global_Enchantment_Screen`,
//! `SPLMASTR.c`): Disjunction (and its True version) ends one, Spell Binding takes one over. The
//! original's panel (`spellscr` 1) has a row for each rival: its gem and up to three of its
//! enchantments in a brown box, with a pair of arrows when it has more. The panel is drawn in two
//! pieces so it is as tall as the rows need (up to four). A click on an enchantment casts, Esc
//! gives up; Up and Down move a highlight through the enchantments and Enter casts.
//!
//! The original has four rivals at most; here the rows scroll with the arrows at the panel's
//! right (`spellscr` 43 and 44), the mouse wheel and the keys when there are more (task 644).

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

const X: f32 = 50.0;
const PANEL_W: f32 = 210.0;
/// The rows of rivals in view at most, and the height of one.
const ROWS: usize = 4;
const ROW_H: f32 = 46.0;
/// The enchantments in view for one rival, the height of one and where the list starts in the row.
const SPELLS: usize = 3;
const SPELL_H: f32 = 13.0;
/// Where the pieces of a row are (from the panel's left edge and the row's top).
const GEM_AT: (f32, f32) = (8.0, 14.0);
const NAMES_AT: (f32, f32) = (76.0, 18.0);
const HIT_AT: (f32, f32) = (74.0, 16.0);
const HIT_W: f32 = 120.0;
const HIGHLIGHT_AT: (f32, f32) = (73.0, 16.0);
const SPELL_ARROW_X: f32 = 61.0;
const SPELL_ARROW_UP_Y: f32 = 19.0;
const SPELL_ARROW_DOWN_Y: f32 = 43.0;
/// The arrows for the rows of rivals, at the right of the panel.
const ROW_ARROW_X: f32 = 197.0;
/// The panel's top piece ends 56 pixels below its top plus a row for each row after the first; the
/// bottom border, 6 pixels, comes after that.
const TOP_PIECE: f32 = 56.0;
const BOTTOM_PIECE_FROM: f32 = 194.0;

/// The rival's row of the list: who, and which of their enchantments (the instances).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    pub(crate) who: PlayerId,
    pub(crate) spells: Vec<hecs::Entity>,
}

/// The rival's enchantments picked, and what is scrolled where.
pub(crate) struct Global {
    spell: SpellId,
    resolve: bool,
    /// The highlighted enchantment, counted over all the rows.
    row: usize,
    /// The first rival in view, and the first enchantment in view for each rival.
    first_rival: usize,
    firsts: Vec<usize>,
}

impl Global {
    pub(crate) fn new(spell: SpellId, resolve: bool) -> Self {
        Self {
            spell,
            resolve,
            row: 0,
            first_rival: 0,
            firsts: Vec::new(),
        }
    }
}

/// Where the panel starts down the screen for `rows` rows in view: it is centred by the original's
/// `(5 - players) x 23`, which is 23 for each row short of four.
pub(crate) fn panel_top(rows: usize) -> f32 {
    (ROWS.saturating_sub(rows)) as f32 * 23.0
}

/// The panel's height for `rows` rows: the top piece, a row for each row after the first, and the
/// bottom border.
pub(crate) fn panel_height(rows: usize) -> f32 {
    TOP_PIECE + ROW_H * rows.saturating_sub(1) as f32 + (200.0 - BOTTOM_PIECE_FROM)
}

/// The top of the row `n` (in view) of a panel that starts at `top`.
pub(crate) fn row_top(top: f32, n: usize) -> f32 {
    top + ROW_H * n as f32
}

/// The rectangle (x, y, w, h) of the enchantment `j` of the rival in row `n`.
pub(crate) fn spell_rect(top: f32, n: usize, j: usize) -> (f32, f32, f32, f32) {
    (
        X + HIT_AT.0,
        row_top(top, n) + HIT_AT.1 + SPELL_H * j as f32,
        HIT_W,
        SPELL_H,
    )
}

/// The enchantments of the rivals, grouped by rival in the order of the players, every rival wizard
/// with a row, the ones with none an empty one. `held` is what the game lists (`rival_globals`) with
/// its owner.
pub(crate) fn groups(
    rivals: &[(PlayerId, crate::wizard_pick::Standing)],
    held: &[(hecs::Entity, PlayerId)],
) -> Vec<Group> {
    rivals
        .iter()
        .map(|&(who, _)| Group {
            who,
            spells: held
                .iter()
                .filter(|&&(_, owner)| owner == who)
                .map(|&(i, _)| i)
                .collect(),
        })
        .collect()
}

/// The highlighted enchantment `row` (counted over all) as (rival, enchantment in the rival's list).
pub(crate) fn locate(groups: &[Group], row: usize) -> Option<(usize, usize)> {
    let mut left = row;
    for (g, group) in groups.iter().enumerate() {
        if left < group.spells.len() {
            return Some((g, left));
        }
        left -= group.spells.len();
    }
    None
}

/// How many enchantments there are in all.
pub(crate) fn total(groups: &[Group]) -> usize {
    groups.iter().map(|g| g.spells.len()).sum()
}

/// The first enchantment in view for a rival with `count`, so that `item` is in view.
pub(crate) fn keep_in_view(first: usize, item: usize, count: usize) -> usize {
    Window::new(first, SPELLS, count)
        .moved(if item < first {
            item as i32 - first as i32
        } else if item >= first + SPELLS {
            (item + 1 - SPELLS - first) as i32
        } else {
            0
        })
        .first
}

impl App {
    /// The enchantments of the rivals with their owners, in the order the game lists them.
    fn rival_enchantments(&self) -> Vec<(hecs::Entity, PlayerId)> {
        let spells = self.game.spells();
        self.game
            .rival_globals(HUMAN)
            .into_iter()
            .filter_map(|i| Some((i, spells.caster_player(i)?)))
            .collect()
    }

    /// Whether `spell` takes the enchantment over (Spell Binding) and not ends it.
    fn is_binding(&self, spell: SpellId) -> bool {
        f4x_core::spell::dispel::CounterEffect::SpellBinding
            .is_in(self.game.content().rules.spells.get(spell))
    }

    /// The panel in two pieces, the top and the bottom border, so that it is `rows` rows tall.
    fn draw_global_panel(&mut self, lay: &Layout, rows: usize) {
        let top = panel_top(rows);
        let split = top + TOP_PIECE + ROW_H * rows.saturating_sub(1) as f32;
        if self.art_picture("spellscr", 1, 0).is_some() {
            self.draw_pic_rows(lay, "spellscr", 1, X, top, top, split);
            let bottom_y =
                top + ROW_H * rows.saturating_sub(1) as f32 - (BOTTOM_PIECE_FROM - TOP_PIECE);
            self.draw_pic_rows(lay, "spellscr", 1, X, bottom_y, split, 1000.0);
        } else {
            let r = lay.rect((X, top, PANEL_W, panel_height(rows)));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.06, 0.04, 0.95));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, GOLD);
        }
    }

    /// An arrow button at `at` (7 x 9 or the resource arrows' own size); true when it was clicked.
    pub(crate) fn arrow(
        &mut self,
        lay: &Layout,
        (lbx, entry): (&str, usize),
        at: (f32, f32),
        input: &Input,
    ) -> bool {
        let rect = lay.rect((at.0, at.1, 9.0, 9.0));
        let hovered = ui::hit(rect, true, input) != ui::Hit::None;
        self.draw_pic(lay, lbx, entry, usize::from(hovered), at.0, at.1);
        ui::hit(rect, true, input).clicked()
    }

    fn global_pick_frame(&mut self, mut pick: Global, input: &Input) -> Option<Global> {
        let rivals = self.opponent_wizards();
        let held = self.rival_enchantments();
        let groups = groups(&rivals, &held);
        let count = total(&groups);
        if count == 0 || input.key_pressed(KeyCode::Escape) {
            self.log.push(self.text("log.cast_cancelled"));
            return None;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        pick.firsts.resize(groups.len(), 0);
        pick.row = pick.row.min(count - 1);
        let rows_in_view = groups.len().min(ROWS);
        let top = panel_top(rows_in_view);
        let mut window = Window::new(pick.first_rival, ROWS, groups.len());

        // The highlight moves with Up and Down and the view follows it.
        let mut moved = false;
        if input.key_pressed(KeyCode::Up) && pick.row > 0 {
            pick.row -= 1;
            moved = true;
        }
        if input.key_pressed(KeyCode::Down) && pick.row + 1 < count {
            pick.row += 1;
            moved = true;
        }
        if window.scrolls() && !moved {
            window = window.moved(scroll::step(input.mouse_wheel().1, false, false));
        }
        if let Some((g, item)) = locate(&groups, pick.row) {
            if moved {
                let by = if g < window.first {
                    g as i32 - window.first as i32
                } else if g >= window.first + ROWS {
                    (g + 1 - ROWS - window.first) as i32
                } else {
                    0
                };
                window = window.moved(by);
                pick.firsts[g] = keep_in_view(pick.firsts[g], item, groups[g].spells.len());
            }
        }

        self.draw_global_panel(&lay, rows_in_view);
        let spell_name_term = self
            .names
            .spell_term(self.game.content().rules.spells.get(pick.spell).num);
        let key = if self.is_binding(pick.spell) {
            "spell.select_to_bind"
        } else {
            "spell.select_to_disjunct"
        };
        let title = self.text_with(
            key,
            &crate::text::Args::new().term("spell", spell_name_term.clone()),
        );
        self.draw_styled(
            view,
            &Style::nearest(1, WHITE).align(Align::Center),
            &title,
            X + 105.0,
            top + 3.0,
        );

        // The arrows for the rows of rivals, when there are more than four.
        if window.scrolls() {
            let mut by = 0;
            if window.can_up()
                && self.arrow(&lay, ("spellscr", 43), (X + ROW_ARROW_X, top + 16.0), input)
            {
                by -= 1;
            }
            if window.can_down()
                && self.arrow(
                    &lay,
                    ("spellscr", 44),
                    (X + ROW_ARROW_X, top + panel_height(rows_in_view) - 22.0),
                    input,
                )
            {
                by += 1;
            }
            window = window.moved(by);
        }

        let mut chosen = None;
        let mut flat = 0;
        let before: Vec<usize> = groups
            .iter()
            .scan(0, |sum, g| {
                let at = *sum;
                *sum += g.spells.len();
                Some(at)
            })
            .collect();
        for (slot, g) in window.rows().enumerate() {
            let group = &groups[g];
            let y = row_top(top, slot);
            let standing = rivals[g].1;
            let (portrait, banner) = self
                .game
                .player(group.who)
                .map_or((0, 0), |p| (p.portrait(), p.banner()));
            // A rival still met or with enchantments shows its own gem, the grey one when not, the
            // broken one when gone.
            let own =
                standing == crate::wizard_pick::Standing::Pickable || !group.spells.is_empty();
            let gem = (X + GEM_AT.0, y + GEM_AT.1);
            if own {
                if self
                    .draw_gem(&lay, portrait, banner, gem.0, gem.1)
                    .is_none()
                {
                    self.draw_pic(&lay, "magic", 6, 0, gem.0, gem.1);
                }
            } else if standing == crate::wizard_pick::Standing::Unmet {
                self.draw_pic(&lay, "magic", 6, 0, gem.0, gem.1);
            } else {
                self.draw_pic(&lay, "magic", 51, 0, gem.0, gem.1);
            }
            // The rival's own list scrolls with a pair of arrows when it has more than three.
            let spell_window = Window::new(pick.firsts[g], SPELLS, group.spells.len());
            if spell_window.scrolls() {
                let mut by = 0;
                if spell_window.can_up()
                    && self.arrow(
                        &lay,
                        ("resource", 32),
                        (X + SPELL_ARROW_X, y + SPELL_ARROW_UP_Y),
                        input,
                    )
                {
                    by -= 1;
                }
                if spell_window.can_down()
                    && self.arrow(
                        &lay,
                        ("resource", 33),
                        (X + SPELL_ARROW_X, y + SPELL_ARROW_DOWN_Y),
                        input,
                    )
                {
                    by += 1;
                }
                pick.firsts[g] = spell_window.moved(by).first;
            }
            flat = before[g];
            let shown = Window::new(pick.firsts[g], SPELLS, group.spells.len());
            for (j, item) in shown.rows().enumerate() {
                let rect = lay.rect(spell_rect(top, slot, j));
                let hit = ui::hit(rect, true, input);
                let selected = flat + item == pick.row;
                if hit != ui::Hit::None {
                    pick.row = flat + item;
                }
                if selected || hit != ui::Hit::None {
                    self.draw_pic(
                        &lay,
                        "spellscr",
                        7,
                        0,
                        X + HIGHLIGHT_AT.0,
                        y + HIGHLIGHT_AT.1 + SPELL_H * j as f32,
                    );
                }
                let spells = self.game.spells();
                let name = spells
                    .def(group.spells[item])
                    .map(|id| {
                        self.names
                            .spell(self.game.content().rules.spells.get(id).num)
                            .to_string()
                    })
                    .unwrap_or_default();
                let ink = if selected { GOLD } else { WHITE };
                self.draw_styled(
                    view,
                    &Style::nearest(1, ink).align(Align::Left),
                    &name,
                    X + NAMES_AT.0,
                    y + NAMES_AT.1 + SPELL_H * j as f32,
                );
                if hit.clicked() {
                    chosen = Some(group.spells[item]);
                }
            }
        }
        let _ = flat;
        if chosen.is_none() && input.key_pressed(KeyCode::Enter) {
            chosen = locate(&groups, pick.row).map(|(g, i)| groups[g].spells[i]);
        }
        if let Some(instance) = chosen {
            let target = Target::Spell(instance);
            let command = if pick.resolve {
                Command::ResolveCast {
                    player: HUMAN,
                    target,
                }
            } else {
                Command::CastSpell {
                    extra: self.chosen_extra(pick.spell),
                    player: HUMAN,
                    spell: pick.spell,
                    target: Some(target),
                }
            };
            let events = self.game.apply(command);
            self.note(events);
            return None;
        }
        pick.first_rival = window.first;
        Some(pick)
    }

    /// For screenshots: Disjunction against the rivals, each given a few global enchantments (the
    /// first four rows of the list get 1, 4, 2 and 0 so the arrows of a long list show).
    pub(crate) fn open_test_global_pick(&mut self) {
        let rules = &self.game.content().rules;
        let Some(spell) = rules.spells.id_numbered(210 /* disjunction */) else {
            return;
        };
        let globals: Vec<SpellId> = [59, 118, 119, 120, 121]
            .into_iter()
            .filter_map(|n| rules.spells.id_numbered(n))
            .collect();
        let rivals: Vec<PlayerId> = self
            .game
            .player_ids()
            .into_iter()
            .filter(|&id| id != HUMAN && self.game.is_wizard(id))
            .collect();
        for (&id, count) in rivals.iter().zip([1, 4, 2, 0]) {
            self.game.world_mut().make_contact(HUMAN, id);
            for &g in globals.iter().take(count) {
                self.game.world_mut().scenario_add_global(id, g);
            }
        }
        self.open_global_pick(spell, false);
    }

    /// Ask which of the rivals' global enchantments `spell` is for; `resolve` for a spell paid over
    /// turns. Says so in the log when there is none.
    pub(crate) fn open_global_pick(&mut self, spell: SpellId, resolve: bool) {
        self.screen = Screen::Map;
        if self.game.rival_globals(HUMAN).is_empty() {
            let key = if resolve {
                "spell.no_globals_waits"
            } else {
                "spell.no_globals"
            };
            self.log.push(self.text(key));
        } else {
            self.modals.open(Global::new(spell, resolve));
        }
    }
}

impl Modal for Global {
    fn kind(&self) -> &'static str {
        "global"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        app.global_pick_frame(*self, input)
            .map(|g| Box::new(g) as Box<dyn Modal>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wizard_pick::Standing;

    fn entity(n: u32) -> hecs::Entity {
        hecs::Entity::from_bits((1u64 << 32) | u64::from(n)).unwrap_or(hecs::Entity::DANGLING)
    }

    #[test]
    fn the_panel_shrinks_by_a_row_and_sits_lower_for_each_row_short_of_four() {
        assert_eq!((panel_top(4), panel_height(4)), (0.0, 200.0));
        assert_eq!((panel_top(3), panel_height(3)), (23.0, 154.0));
        assert_eq!((panel_top(1), panel_height(1)), (69.0, 62.0));
        // The panel stays on the screen.
        for rows in 1..=4 {
            assert!(panel_top(rows) + panel_height(rows) <= 200.0, "{rows}");
        }
    }

    #[test]
    fn the_enchantments_of_a_rival_are_boxes_of_three_in_its_row() {
        let (x, y, w, h) = spell_rect(0.0, 0, 0);
        assert_eq!((x, y, w, h), (124.0, 16.0, 120.0, 13.0));
        assert_eq!(spell_rect(0.0, 1, 2).1, 46.0 + 16.0 + 26.0);
        // The third box of a row ends above the next row's first.
        assert!(spell_rect(0.0, 0, 2).1 + SPELL_H <= spell_rect(0.0, 1, 0).1);
    }

    #[test]
    fn every_rival_has_a_row_and_the_highlight_walks_over_the_enchantments() {
        let rivals = [
            (PlayerId(1), Standing::Pickable),
            (PlayerId(2), Standing::Unmet),
            (PlayerId(3), Standing::Pickable),
        ];
        let held = [
            (entity(1), PlayerId(1)),
            (entity(2), PlayerId(3)),
            (entity(3), PlayerId(3)),
        ];
        let g = groups(&rivals, &held);
        assert_eq!(g.len(), 3);
        assert!(g[1].spells.is_empty());
        assert_eq!(total(&g), 3);
        assert_eq!(locate(&g, 0), Some((0, 0)));
        assert_eq!(locate(&g, 1), Some((2, 0)));
        assert_eq!(locate(&g, 2), Some((2, 1)));
        assert_eq!(locate(&g, 3), None);
    }

    #[test]
    fn a_rivals_list_follows_the_highlight() {
        assert_eq!(keep_in_view(0, 2, 5), 0);
        assert_eq!(keep_in_view(0, 3, 5), 1);
        assert_eq!(keep_in_view(0, 4, 5), 2);
        assert_eq!(keep_in_view(2, 0, 5), 0);
        assert_eq!(keep_in_view(0, 1, 2), 0);
    }

    #[test]
    fn the_screen_has_its_texts() {
        let texts = crate::test_app::shipped::texts();
        for key in [
            "spell.select_to_disjunct",
            "spell.select_to_bind",
            "spell.no_globals",
            "spell.no_globals_waits",
        ] {
            assert!(texts.has(key), "{key}");
        }
    }
}
