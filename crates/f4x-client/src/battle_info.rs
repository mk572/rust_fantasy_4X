//! The combat information window (`Combat_Information_Window_Draw` in ReMoM `Combat.c`): the
//! effects in force on the field and each wizard's own spells, over the battle. Any key or
//! click closes it.

use f4x_core::battle::info::{BattleInfo, InfoItem};
use macroquad::prelude::*;

use crate::frame::Align;
use crate::view::App;

/// The window's left edge, and the cell columns, in the original's 320 by 200 pixels.
const LEFT: f32 = 50.0;
const COLUMN: f32 = 112.0;
/// The picture's width (`compix.lbx` 56 to 58).
const WIDTH: f32 = 234.0;
const TEXT_STYLE: usize = 2;
const TEXT: Color = Color::new(0.98, 0.86, 0.45, 1.0);

/// Where the window's cells go: the cell's column and row, the common items first.
struct Layout {
    items: usize,
    rows: usize,
    top: f32,
}

impl Layout {
    fn new(info: &BattleInfo) -> Self {
        let common = info.common.len();
        let items = common + common % 2;
        let rows = info.side_rows();
        let base = if items > 0 {
            (items / 2) as f32 * 19.0 + 9.0
        } else {
            4.0
        };
        let height = 10.0 + rows as f32 * 20.0 + base + if rows > 0 { 21.0 } else { 0.0 };
        Layout {
            items,
            rows,
            top: 100.0 - (height / 2.0).floor(),
        }
    }

    /// The bottom of the top strip, which is also where the middle begins.
    fn top_end(&self) -> f32 {
        self.top
            + if self.items > 0 {
                9.0 + (self.items / 2) as f32 * 19.0
            } else {
                4.0
            }
    }

    fn mid_end(&self) -> f32 {
        self.top_end()
            + if self.rows > 0 {
                self.rows as f32 * 20.0 + 21.0
            } else {
                0.0
            }
    }

    /// The top-left of cell `idx` (common cells first, then each side's, left column the
    /// opponent's).
    fn cell(&self, idx: usize) -> (f32, f32) {
        let offset = if idx >= self.items && self.items > 0 {
            31.0
        } else if self.items > 0 {
            10.0
        } else {
            26.0
        };
        (
            LEFT + (idx % 2) as f32 * COLUMN + 11.0,
            self.top + offset + (idx / 2) as f32 * 19.0,
        )
    }
}

impl App {
    /// Draw the window; true once it is dismissed.
    pub(crate) fn draw_battle_info(&mut self, info: &BattleInfo) -> bool {
        let (w, h) = (screen_width(), screen_height());
        let s = (w / 320.0).min(h / 200.0);
        let view = ((w - 320.0 * s) / 2.0, (h - 200.0 * s) / 2.0, s);
        let (ox, oy, _) = view;
        let at = |x: f32, y: f32| (ox + x * s, oy + y * s);
        let lay = Layout::new(info);

        // The three strips, each cut off where the next begins (the original clips them).
        let strip = |app: &mut App, entry: usize, y: f32, height: f32| {
            let Some(tex) = app.art.picture("compix", entry, 0).cloned() else {
                return;
            };
            let height = height.min(tex.height());
            if height <= 0.0 {
                return;
            }
            let (px, py) = at(LEFT, y);
            draw_texture_ex(
                &tex,
                px,
                py,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(WIDTH * s, height * s)),
                    source: Some(Rect::new(0.0, 0.0, WIDTH, height)),
                    ..Default::default()
                },
            );
        };
        if app_has_art(self) {
            let top_end = lay.top_end();
            strip(self, 58, lay.top, top_end - lay.top);
            // More rows than the top strip is tall: the middle fills the gap.
            let top_h = self
                .art
                .picture("compix", 58, 0)
                .map_or(0.0, |t| t.height());
            if top_end - lay.top > top_h {
                strip(self, 57, lay.top + top_h, top_end - lay.top - top_h);
            }
            strip(self, 57, top_end, lay.mid_end() - top_end);
            strip(self, 56, lay.mid_end() - 2.0, 8.0);
        } else {
            let (px, py) = at(LEFT, lay.top);
            draw_rectangle(
                px,
                py,
                WIDTH * s,
                (lay.mid_end() + 6.0 - lay.top) * s,
                Color::new(0.12, 0.09, 0.07, 0.95),
            );
            draw_rectangle_lines(
                px,
                py,
                WIDTH * s,
                (lay.mid_end() + 6.0 - lay.top) * s,
                2.0,
                GRAY,
            );
        }

        if lay.rows > 0 {
            let heading = if lay.items > 0 {
                15.0 + (lay.items / 2) as f32 * 19.0
            } else {
                11.0
            };
            if let Some(name) = &info.opponent_name {
                self.bm_text(
                    view,
                    TEXT_STYLE,
                    TEXT,
                    &format!("{name} Spells"),
                    LEFT + 54.0,
                    lay.top + heading,
                    Align::Center,
                );
            }
            if let Some(name) = &info.viewer_name {
                self.bm_text(
                    view,
                    TEXT_STYLE,
                    TEXT,
                    &format!("{name} Spells"),
                    LEFT + 170.0,
                    lay.top + heading,
                    Align::Center,
                );
            }
        }

        // Every cell has its box; the ones with an effect also its picture and name.
        let mut cells: Vec<Option<&InfoItem>> = info.common.iter().map(Some).collect();
        cells.resize(lay.items, None);
        for row in 0..lay.rows {
            cells.push(info.opponent.get(row));
            cells.push(info.viewer.get(row));
        }
        for (idx, cell) in cells.into_iter().enumerate() {
            let (x, y) = lay.cell(idx);
            if let Some(tex) = self.art.picture("compix", 59, 0).cloned() {
                let (px, py) = at(x - 1.0, y - 1.0);
                draw_texture_ex(
                    &tex,
                    px,
                    py,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(tex.size() * s),
                        ..Default::default()
                    },
                );
            }
            let Some(item) = cell else {
                continue;
            };
            if let Some(tex) = item
                .icon
                .and_then(|i| self.art.picture("compix", 42 + usize::from(i), 0).cloned())
            {
                let (px, py) = at(x, y);
                draw_texture_ex(
                    &tex,
                    px,
                    py,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(tex.size() * s),
                        ..Default::default()
                    },
                );
            }
            self.bm_text(
                view,
                TEXT_STYLE,
                TEXT,
                item.name,
                x + 20.0,
                y + 5.0,
                Align::Left,
            );
        }

        is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Space)
            || is_mouse_button_pressed(MouseButton::Left)
            || is_mouse_button_pressed(MouseButton::Right)
    }
}

/// Whether the window's pictures were extracted.
fn app_has_art(app: &mut App) -> bool {
    app.art.picture("compix", 58, 0).is_some() && app.art.picture("compix", 57, 0).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::battle::info::{InfoFacts, WizardSpells};

    #[test]
    fn the_window_is_centred_on_the_screens_middle_line() {
        let info = f4x_core::battle::info::build(InfoFacts {
            chaos_surge: true,
            eternal_night: true,
            ..Default::default()
        });
        let lay = Layout::new(&info);
        // One row of common cells: a window 9 + 19 high plus the 10 margin.
        assert_eq!(lay.items, 2);
        assert_eq!(lay.top, 100.0 - 19.0);
        assert_eq!(lay.cell(0), (61.0, lay.top + 10.0));
        assert_eq!(lay.cell(1), (173.0, lay.top + 10.0));
    }

    #[test]
    fn each_sides_cells_follow_the_common_ones() {
        let side = |crusade| {
            Some((
                "W".to_string(),
                WizardSpells {
                    crusade,
                    ..Default::default()
                },
            ))
        };
        let info = f4x_core::battle::info::build(InfoFacts {
            chaos_surge: true,
            opponent: side(true),
            viewer: side(true),
            ..Default::default()
        });
        let lay = Layout::new(&info);
        assert_eq!((lay.items, lay.rows), (2, 1));
        // First side cell: after the one common row, 31 below the window top plus a row.
        assert_eq!(lay.cell(2), (61.0, lay.top + 31.0 + 19.0));
    }
}
