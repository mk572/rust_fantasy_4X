//! The box at the top right of the battle screen with what the selected unit has beyond its
//! numbers: the spells it can cast (one click casts, or aims) and its abilities. It is made of
//! the Info window's pictures (`compix` 56 to 58) cut to size, and set in the original's font.

use super::*;
use crate::frame::Layout;
use crate::unit_sheet::special_picture;
use f4x_core::battle::spell::BattleSpell;

const LEFT: f32 = 228.0;
const WIDTH: f32 = 90.0;
const TOP: f32 = 4.0;
const ROW: f32 = 19.0;
/// What the Info window's pictures give: the width of their frame at each side, and the rows
/// of the top edge and the bottom edge.
const SIDE: f32 = 6.0;
const EDGE_TOP: f32 = 9.0;
const EDGE_BOTTOM: f32 = 8.0;
/// The most rows that fit between the top of the screen and the panel.
const MOST_ROWS: usize = 7;

/// One row of the box.
enum Row {
    /// A spell the unit can cast.
    Spell(BattleSpell, String),
    /// Summon a demon, with the number left.
    Demon(u32),
    /// An ability or lasting spell: its picture and name.
    Ability(Option<usize>, String),
}

impl App {
    /// A piece of one of the Info window's strips, `width` wide, stretched in the middle so that
    /// its frame stays the same at the sides.
    fn draw_strip(&mut self, lay: &Layout, entry: usize, rows: (f32, f32), at: (f32, f32), w: f32) {
        let Some(tex) = self.art.picture("compix", entry, 0).cloned() else {
            return;
        };
        let (src_y, h) = rows;
        let full = tex.width();
        let pieces = [
            (0.0, SIDE, at.0),
            (SIDE, w - 2.0 * SIDE, at.0 + SIDE),
            (full - SIDE, SIDE, at.0 + w - SIDE),
        ];
        for (src_x, width, x) in pieces {
            let to = lay.point(x, at.1);
            draw_texture_ex(
                &tex,
                to.x,
                to.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(width * lay.scale, h * lay.scale)),
                    source: Some(Rect::new(
                        src_x,
                        src_y,
                        width.min(full - 2.0 * SIDE).max(SIDE),
                        h,
                    )),
                    ..Default::default()
                },
            );
        }
    }

    /// The box's background: the top edge, the middle repeated, the bottom edge.
    fn draw_specials_box(&mut self, lay: &Layout, height: f32) {
        self.draw_strip(lay, 58, (0.0, EDGE_TOP), (LEFT, TOP), WIDTH);
        let middle = self
            .art
            .picture("compix", 57, 0)
            .map_or(100.0, |t| t.height());
        let body = height - EDGE_TOP - EDGE_BOTTOM;
        let mut done = 0.0;
        while done < body {
            let rows = (body - done).min(middle);
            self.draw_strip(lay, 57, (0.0, rows), (LEFT, TOP + EDGE_TOP + done), WIDTH);
            done += rows;
        }
        self.draw_strip(
            lay,
            56,
            (0.0, EDGE_BOTTOM),
            (LEFT, TOP + height - EDGE_BOTTOM),
            WIDTH,
        );
    }

    /// Draw the box for the selected unit and take a click on one of its spells.
    pub(super) fn draw_unit_specials(&mut self, battle: &Battle, my_turn: bool) {
        self.battle_ui.box_rect = None;
        let Some(id) = self.battle_ui.selected else {
            return;
        };
        let Some(unit) = battle.unit(id) else {
            return;
        };
        let mut rows: Vec<Row> = Vec::new();
        for s in self.game.battle_unit_spells(id) {
            let name = self.game.content().rules.spells.get(s.spell).name.clone();
            let label = if s.cost > 0 {
                self.text_with(
                    "battle.spell_row",
                    &Args::new().text("spell", &name).count("cost", s.cost),
                )
            } else {
                name
            };
            rows.push(Row::Spell(
                BattleSpell {
                    spell: s.spell,
                    cost: s.cost,
                    takes_extra_mana: s.takes_extra_mana,
                    target: s.target,
                    castable: s.castable,
                },
                label,
            ));
        }
        if unit.casting.demons > 0 {
            rows.push(Row::Demon(unit.casting.demons));
        }
        for special in self
            .battle_sheet(battle, id)
            .map(|s| s.specials)
            .unwrap_or_default()
        {
            rows.push(Row::Ability(special.icon, special.name));
        }
        rows.truncate(MOST_ROWS);
        if rows.is_empty() {
            return;
        }
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let height = EDGE_TOP + EDGE_BOTTOM + rows.len() as f32 * ROW;
        self.battle_ui.box_rect = Some(lay.rect((LEFT, TOP, WIDTH, height)));
        self.draw_specials_box(&lay, height);

        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let gold = Color::new(0.98, 0.86, 0.45, 1.0);
        let dim = Color::new(0.6, 0.55, 0.5, 1.0);
        let free =
            my_turn && self.battle_ui.aiming.is_none() && self.battle_ui.fallen_list.is_none();
        let mouse: Vec2 = mouse_position().into();
        let mut cast: Option<BattleSpell> = None;
        let mut summon = false;
        for (i, row) in rows.iter().enumerate() {
            let (x, y) = (LEFT + SIDE + 1.0, TOP + EDGE_TOP + i as f32 * ROW);
            let cell = lay.rect((x - 1.0, y, WIDTH - 2.0 * SIDE, ROW - 1.0));
            let (text, colour, icon, usable) = match row {
                Row::Spell(spell, label) => (label.clone(), gold, None, spell.castable && free),
                Row::Demon(left) => (
                    self.text_with("battle.demon_row", &Args::new().count("n", *left)),
                    gold,
                    None,
                    free && self.game.can_summon_demon(id),
                ),
                Row::Ability(icon, name) => (name.clone(), light, *icon, false),
            };
            let clickable = !matches!(row, Row::Ability(..));
            let hover = usable && cell.contains(mouse);
            if hover {
                draw_rectangle(
                    cell.x,
                    cell.y,
                    cell.w,
                    cell.h,
                    Color::new(1.0, 1.0, 1.0, 0.18),
                );
            }
            if let Some(icon) = icon {
                let (lbx, entry) = special_picture(icon);
                self.draw_pic(&lay, lbx, entry, 0, x, y + 1.0);
            }
            let text_x = if icon.is_some() { x + 19.0 } else { x + 2.0 };
            let colour = if clickable && !usable { dim } else { colour };
            self.bm_text(view, 1, colour, &text, text_x, y + 6.0, Align::Left);
            if hover && is_mouse_button_pressed(MouseButton::Left) {
                match row {
                    Row::Spell(spell, _) => cast = Some(*spell),
                    Row::Demon(_) => summon = true,
                    Row::Ability(..) => {}
                }
            }
        }
        if let Some(spell) = cast {
            self.battle_ui.spell_unit = Some(id);
            self.pick_spell(&spell);
        } else if summon {
            self.battle_ui.spell_list = false;
            self.battle_ui.spell_unit = None;
            self.battle_order(BattleCommand::SummonDemon { unit: id });
            self.battle_ui.selected = None;
        }
    }
}
