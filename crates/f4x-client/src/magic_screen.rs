//! The magic screen: the wizard overview, laid out like the original's `Magic_Screen`
//! (`MagicScr.c`) on its 320 by 200 pixels.
//!
//! Three staves set how the power base is split between mana, research and casting skill
//! (drag one; the lock above each holds its share), the four gems on top are the other wizards
//! (grey until diplomacy gives a wizard something to say), the panel on the right lists the
//! overland enchantments of every wizard, and the block of text below the staves has the
//! casting skill, the mana reserve, the power base and what is being cast and researched. The
//! Alchemy button opens the gold-and-power exchange. Spells are cast from the spell book
//! (`spellbook.rs`) and chosen for research in the big book (`bigbook.rs`).

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::ui;
use f4x_core::game::Command;
use f4x_core::player::{PlayerId, NEUTRAL_BANNER};
use f4x_core::rules::SpellId;
use f4x_core::text::Args;
use f4x_core::wizard::magic::Retort;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::ui::button;
use crate::view::{App, Screen, HUMAN};

/// The staves, in the order the split is kept: mana, research, skill.
const STAFF_X: [f32; 3] = [29.0, 75.0, 122.0];
/// The picture entries of each staff: empty, full, locked (`magic.lbx`).
const STAFF_ART: [(usize, usize, usize); 3] = [(7, 8, 15), (9, 10, 16), (11, 12, 17)];
/// Where each staff's picture goes (empty, full, lock), in the original's pixels.
const STAFF_AT: [[(f32, f32); 3]; 3] = [
    [(29.0, 83.0), (32.0, 102.0), (27.0, 81.0)],
    [(75.0, 85.0), (79.0, 102.0), (74.0, 81.0)],
    [(122.0, 83.0), (126.0, 102.0), (121.0, 81.0)],
];
/// The bottom of a full staff: a staff at `n` holds `n` pixels up from here.
const STAFF_FLOOR: f32 = 152.0;
/// A staff is full at fifty pixels; the split is kept in hundredths, two for each.
const STAFF_MAX: i32 = 50;

/// Move staff `changed` (0 mana, 1 research, 2 skill) to `new`, taking from or giving to the
/// others as the original does: nothing moves if the staff is locked or both others are; with
/// one other locked the free one takes the difference (the staff can only grow by what that
/// one has); with neither locked they share it, the odd point staying where it was. The three
/// always add up to fifty.
pub(crate) fn staff_adjust(pos: [i32; 3], locked: [bool; 3], changed: usize, new: i32) -> [i32; 3] {
    let new = new.clamp(0, STAFF_MAX);
    if locked[changed] {
        return pos;
    }
    // The original works out the other two in this order: skill, research, mana.
    let mut others: Vec<usize> = [2usize, 1, 0]
        .into_iter()
        .filter(|&i| i != changed)
        .collect();
    let (a, b) = (others.remove(0), others.remove(0));
    let d = new - pos[changed];
    let mut out = pos;
    match (locked[a], locked[b]) {
        (true, true) => {}
        (true, false) | (false, true) => {
            let free = if locked[a] { b } else { a };
            if d > 0 {
                let take = d.min(pos[free]);
                out[free] -= take;
                out[changed] += take;
            } else {
                out[free] -= d;
                out[changed] += d;
            }
        }
        (false, false) => {
            let half = d.abs() / 2;
            if d > 0 {
                let mut from_a = half.min(pos[a]);
                let from_b = (half + (half - from_a)).min(pos[b]);
                // What the second could not give comes from the first, if it has it.
                from_a = (2 * half - from_b).min(pos[a]).max(from_a);
                out[a] -= from_a;
                out[b] -= from_b;
                out[changed] += from_a + from_b;
            } else {
                out[a] += half;
                out[b] += half;
                out[changed] -= 2 * half;
            }
        }
    }
    out
}

/// The staves' positions for a split of percentages (mana, research, skill).
pub(crate) fn staves_from_split(mana: u8, research: u8, skill: u8) -> [i32; 3] {
    let (r, s) = (i32::from(research) / 2, i32::from(skill) / 2);
    let m = (i32::from(mana) / 2).max(STAFF_MAX - r - s);
    [m.clamp(0, STAFF_MAX), r, s]
}

/// The alchemy window's slider runs from 3 to 53 (`Alchemy_Popup`): gold or power it converts.
pub(crate) fn alchemy_amount(pool: i32, slider: i32) -> i32 {
    let mut rate = 50;
    if 50 * pool / rate > 999 {
        rate = 50 * pool / 999;
    }
    (pool * (slider - 3) / rate.max(1)).clamp(0, 999)
}

/// The alchemy window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AlchemyUi {
    /// Converting gold into power (true) or power into gold.
    pub gold_to_mana: bool,
    /// The slider, 3 to 53.
    pub slider: i32,
}

/// Where a wizard stands in the row of gems: those you can talk to first, then those not met
/// yet, then the gone ones (a stable sort keeps the order of the players within each).
fn gem_rank(gone: bool, met: bool) -> u8 {
    match (gone, met) {
        (false, true) => 0,
        (false, false) => 1,
        (true, _) => 2,
    }
}

/// What the magic screen remembers while it is open.
#[derive(Debug, Clone, Default)]
pub(crate) struct MagicUi {
    pub pos: [i32; 3],
    pub locked: [bool; 3],
    pub drag: Option<usize>,
    /// The first overland enchantment shown, when there are more than eighteen.
    pub first: usize,
    /// The first of the other wizards shown in the four gems, when there are more than four.
    pub first_wizard: usize,
    pub alchemy: Option<AlchemyUi>,
    /// Asking whether to turn off one of the wizard's own enchantments.
    pub turn_off: Option<SpellId>,
}

impl App {
    /// The magic screen as it opens: the staves at the wizard's power split.
    fn fresh_magic(&self) -> MagicUi {
        let (m, r, s) = self.game.wizard(HUMAN).map_or((34, 33, 33), |m| {
            let s = m.power_split();
            (s.mana, s.research, s.skill)
        });
        MagicUi {
            pos: staves_from_split(m, r, s),
            ..Default::default()
        }
    }

    pub fn open_magic(&mut self) {
        let ui = self.fresh_magic();
        self.open_page(Screen::Magic, ui);
    }

    /// The magic screen with the alchemy box open (from the item screen's Alchemy button).
    pub(crate) fn open_magic_alchemy(&mut self) {
        let mut ui = self.fresh_magic();
        ui.alchemy = Some(AlchemyUi {
            gold_to_mana: true,
            slider: 3,
        });
        self.open_page(Screen::Magic, ui);
    }

    /// Every overland enchantment in play, with its wizard: the original lists all of them.
    fn overland_enchantments(&self) -> Vec<(SpellId, f4x_core::player::PlayerId)> {
        self.game
            .player_ids()
            .into_iter()
            .flat_map(|id| {
                self.game
                    .wizard(id)
                    .map(|w| w.global())
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |s| (s, id))
            })
            .collect()
    }

    /// The other wizards' gems: grey until the wizard is met, then its picture in its banner
    /// colour with the word for how it feels about you and an icon for each treaty, and broken
    /// once the wizard is gone. A click on a met wizard's gem opens the diplomacy screen, a
    /// right-click the wizard's sheet. The wizards you can talk to come first, then those not
    /// met yet, then the gone ones; with more than four the wheel, the arrows beside the gems
    /// and the left and right keys scroll through them.
    fn magic_gems(
        &mut self,
        lay: &Layout,
        view: (f32, f32, f32),
        active: bool,
        input: &Input,
        first_wizard: &mut usize,
    ) {
        for i in 0..4 {
            self.draw_pic(lay, "magic", 6, 0, 24.0 + 77.0 * i as f32, 4.0);
        }
        let mut others: Vec<(PlayerId, u8, u8, bool)> = self
            .game
            .player_ids()
            .into_iter()
            .filter(|id| *id != HUMAN && !self.game.is_neutral_player(*id))
            .map(|id| {
                let p = self.game.player(id);
                (
                    id,
                    p.map_or(0, |p| p.portrait()),
                    p.map_or(NEUTRAL_BANNER, |p| p.banner()),
                    p.is_some_and(|p| p.is_defeated()),
                )
            })
            .collect();
        others.sort_by_key(|&(id, _, _, gone)| gem_rank(gone, self.game.have_met(HUMAN, id)));
        let last_first = others.len().saturating_sub(4);
        *first_wizard = (*first_wizard).min(last_first);
        if last_first > 0 && active {
            let strip = lay.rect((24.0, 4.0, 77.0 * 3.0 + 40.0, 56.0));
            let mouse: Vec2 = input.mouse_position().into();
            let step =
                |first: usize, by: i32| (first as i32 + by).clamp(0, last_first as i32) as usize;
            let wheel = input.mouse_wheel().1;
            if wheel != 0.0 && strip.contains(mouse) {
                *first_wizard = step(*first_wizard, if wheel > 0.0 { -1 } else { 1 });
            }
            if input.key_pressed(KeyCode::Left) {
                *first_wizard = step(*first_wizard, -1);
            }
            if input.key_pressed(KeyCode::Right) {
                *first_wizard = step(*first_wizard, 1);
            }
            for (right, x) in [(false, 12.0), (true, 308.0)] {
                let enabled = if right {
                    *first_wizard < last_first
                } else {
                    *first_wizard > 0
                };
                let r = lay.rect((x - 5.0, 22.0, 10.0, 16.0));
                let (cx, cy, h, d) = (
                    r.x + r.w / 2.0,
                    r.y + r.h / 2.0,
                    r.h / 2.0,
                    if right { 1.0 } else { -1.0 },
                );
                let colour = if enabled {
                    WHITE
                } else {
                    Color::new(1.0, 1.0, 1.0, 0.25)
                };
                draw_triangle(
                    vec2(cx + d * r.w / 2.0, cy),
                    vec2(cx - d * r.w / 2.0, cy - h),
                    vec2(cx - d * r.w / 2.0, cy + h),
                    colour,
                );
                if enabled && ui::hit(r, true, input).clicked() {
                    *first_wizard = step(*first_wizard, if right { 1 } else { -1 });
                }
            }
        }
        let others: Vec<_> = others.into_iter().skip(*first_wizard).collect();
        let gems: Vec<PlayerId> = others.iter().take(4).map(|o| o.0).collect();
        for (i, (id, portrait, banner, gone)) in others.into_iter().take(4).enumerate() {
            let x = 24.0 + 77.0 * i as f32;
            if gone {
                self.draw_pic(lay, "magic", 51, 0, x - 1.0, 3.0);
                continue;
            }
            if !self.game.have_met(HUMAN, id) {
                continue;
            }
            let gem = crate::art::gem_entry(portrait, banner)
                .and_then(|entry| self.draw_pic(lay, "lilwiz", entry, 0, x, 4.0));
            let rel = self.game.relation(HUMAN, id);
            let word = self.text(f4x_core::diplomacy::relation_key(rel));
            self.draw_styled(
                view,
                &Style::set(0, 3).center(),
                &word,
                45.0 + 77.0 * i as f32,
                53.0,
            );
            // The treaty with you, then those with the other wizards shown: a scroll for a
            // pact, a peace symbol for an alliance, crossed swords for war, in its banner colour.
            let mut icon_y = 2.0;
            let icon_x = 68.0 + 77.0 * i as f32;
            let mut show = |app: &mut Self, treaty: f4x_core::diplomacy::Treaty, colour: u8| {
                use f4x_core::diplomacy::Treaty as T;
                let n = match treaty {
                    T::None => return,
                    T::Pact => 0,
                    T::Alliance => 1,
                    T::War => 2,
                };
                app.draw_pic(
                    lay,
                    "magic",
                    60 + n + usize::from(colour) * 3,
                    0,
                    icon_x,
                    icon_y,
                );
                icon_y += 12.0;
            };
            let mine = self.game.treaty(HUMAN, id);
            let my_banner = self.game.wizard(HUMAN).map_or(0, |w| w.banner());
            show(self, mine, my_banner);
            for &other in gems.iter().filter(|&&o| o != id) {
                if self.game.have_met(HUMAN, other) {
                    let t = self.game.treaty(id, other);
                    let colour = self.game.wizard(other).map_or(0, |w| w.banner());
                    show(self, t, colour);
                }
            }
            let over = gem.is_some_and(|r| ui::hit(r, true, input).hovered());
            if active && over && input.mouse_pressed(MouseButton::Left) {
                self.open_diplomacy(id);
            }
            if active && over && input.mouse_pressed(MouseButton::Right) {
                self.open_wizard(id);
            }
        }
    }

    /// A picture whose rows between `top` and `bottom` (in the original's pixels) are drawn:
    /// the part of a staff above or below the line its share reaches.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_pic_rows(
        &mut self,
        lay: &Layout,
        lbx: &str,
        entry: usize,
        x: f32,
        y: f32,
        top: f32,
        bottom: f32,
    ) {
        let Some(tex) = self.art_picture(lbx, entry, 0) else {
            return;
        };
        let h = tex.height();
        let (from, to) = (top.max(y), bottom.min(y + h));
        if to <= from {
            return;
        }
        let at = lay.point(x, from);
        draw_texture_ex(
            &tex,
            at.x,
            at.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(tex.width() * lay.scale, (to - from) * lay.scale)),
                source: Some(Rect::new(0.0, from - y, tex.width(), to - from)),
                ..Default::default()
            },
        );
    }

    /// A question with Yes and No (or Y and N); `Some` once the player has answered.
    pub(crate) fn confirm_box(&mut self, text: &str, input: &Input) -> Option<bool> {
        let (w, h) = (screen_width(), screen_height());
        let (bw, bh) = (w.min(560.0), 130.0);
        let (x, y) = ((w - bw) / 2.0, (h - bh) / 2.0);
        draw_rectangle(x, y, bw, bh, Color::new(0.08, 0.06, 0.04, 0.96));
        draw_rectangle_lines(x, y, bw, bh, 3.0, crate::ui::GOLD);
        crate::frame::text_centered(text, w / 2.0, y + 45.0, 22.0, WHITE);
        let yes = button(
            Rect::new(x + bw / 2.0 - 110.0, y + 70.0, 100.0, 36.0),
            &self.text("ui.yes"),
            true,
            input,
        );
        let no = button(
            Rect::new(x + bw / 2.0 + 10.0, y + 70.0, 100.0, 36.0),
            &self.text("ui.no"),
            true,
            input,
        );
        if yes || input.key_pressed(KeyCode::Y) || input.key_pressed(KeyCode::Enter) {
            return Some(true);
        }
        if no || input.key_pressed(KeyCode::N) || input.key_pressed(KeyCode::Escape) {
            return Some(false);
        }
        None
    }
}
impl MagicUi {
    /// The three staves: drawn empty above the line and full below it, locked or not, and
    /// dragged with the mouse (the share changes as the mouse moves and is kept on release).
    fn magic_staves(&mut self, app: &mut App, lay: &Layout, active: bool, input: &Input) {
        let (_, my) = input.mouse_position();
        let virtual_y = (my - lay.origin.y) / lay.scale;
        if active {
            // Locks.
            if input.mouse_pressed(MouseButton::Left) {
                for (i, &sx) in STAFF_X.iter().enumerate() {
                    let lock = lay.rect((sx - 1.0, 82.0, 13.0, 19.0));
                    let staff = lay.rect((sx - 4.0, 83.0, 22.0, 75.0));
                    if ui::hit(lock, true, input).clicked() {
                        self.locked[i] = !self.locked[i];
                    } else if ui::hit(staff, true, input).clicked() {
                        self.drag = Some(i);
                    }
                }
            }
            if let Some(i) = self.drag {
                if input.mouse_down(MouseButton::Left) {
                    let value = (STAFF_FLOOR - virtual_y).round() as i32;
                    let moved = staff_adjust(self.pos, self.locked, i, value);
                    self.pos = moved;
                } else {
                    self.drag = None;
                    self.commit_staves(app);
                }
            }
        }
        let pos = self.pos;
        let locked = self.locked;
        for i in 0..3 {
            let (empty, full, lock) = STAFF_ART[i];
            let fill = pos[i] as f32;
            let line = STAFF_FLOOR - fill;
            let (ex, ey) = STAFF_AT[i][0];
            let (fx, fy) = STAFF_AT[i][1];
            app.draw_pic_rows(lay, "magic", empty, ex, ey, 79.0, line);
            app.draw_pic_rows(lay, "magic", full, fx, fy, line, 166.0);
            if locked[i] {
                let (lx, ly) = STAFF_AT[i][2];
                app.draw_pic(lay, "magic", lock, 0, lx, ly);
            }
        }
    }

    /// Keep the staves' positions as the power split.
    fn commit_staves(&mut self, app: &mut App) {
        let [m, r, s] = self.pos;
        let events = app.game.apply(Command::SetPowerSplit {
            player: HUMAN,
            mana: (2 * m) as u8,
            research: (2 * r) as u8,
            skill: (2 * s) as u8,
        });
        app.note(events);
    }

    /// The income under each staff and the block of facts below them.
    fn magic_text(&mut self, app: &mut App, lay: &Layout, view: (f32, f32, f32)) {
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let gold = Color::new(0.95, 0.55, 0.1, 1.0);
        let income = app.game.income(HUMAN);
        let split = if self.drag.is_some() {
            // While dragging show what the staves would give.
            let base = income.power_base;
            let [m, _, s] = self.pos;
            let mut p = income.split;
            p.mana = base * 2 * m / 100;
            p.skill = base * 2 * s / 100;
            p.research = base - p.mana - p.skill + income.city_research;
            p
        } else {
            income.split
        };
        let researching_spell = app.game.wizard(HUMAN).and_then(|m| m.researching());
        let casting_spell = app.game.wizard(HUMAN).and_then(|m| m.casting());
        app.draw_styled(
            view,
            &Style::nearest(2, gold).right(),
            &app.text_with("magic.mp", &Args::new().count("n", split.mana)),
            54.0,
            160.0,
        );
        app.draw_styled(
            view,
            &Style::nearest(2, gold).right(),
            &app.text_with("magic.rp", &Args::new().count("n", income.research)),
            101.0,
            160.0,
        );
        app.draw_styled(
            view,
            &Style::nearest(2, gold).right(),
            &app.text_with("magic.sp", &Args::new().count("n", split.skill)),
            148.0,
            160.0,
        );

        let skill = app.game.wizard(HUMAN).map_or(0, |m| m.casting_skill());
        let reserve = app.game.player(HUMAN).map_or(0, |p| p.treasury().mana);
        let base = income.power_base;
        let name =
            |g: &f4x_core::game::Game, s: SpellId| g.content().rules.spells.get(s).name.clone();
        let casting =
            casting_spell.map_or_else(|| app.text("magic.none"), |c| name(&app.game, c.spell));
        let researching =
            researching_spell.map_or_else(|| app.text("magic.none"), |s| name(&app.game, s));
        let summon = app
            .game
            .wizard(HUMAN)
            .and_then(|w| w.summon_at())
            .and_then(|at| app.game.economy().city_at(at))
            .map_or_else(|| app.text("magic.none"), |c| app.city_name(c));
        let rows = [
            (
                app.text("magic.casting_skill"),
                format!("{}({skill})", skill + app.game.hero_casting_skill(HUMAN)),
                app.text("magic.casting"),
                casting,
            ),
            (
                app.text("magic.reserve"),
                reserve.to_string(),
                app.text("magic.researching"),
                researching,
            ),
            (
                app.text("magic.power_base"),
                base.to_string(),
                app.text("magic.summon_to"),
                summon,
            ),
        ];
        for (i, (l1, v1, l2, v2)) in rows.into_iter().enumerate() {
            let y = 177.0 + 8.0 * i as f32;
            app.draw_styled(view, &Style::nearest(1, light), &l1, 5.0, y);
            app.draw_styled(view, &Style::nearest(1, light).right(), &v1, 90.0, y);
            app.draw_styled(view, &Style::nearest(1, light), &l2, 100.0, y);
            app.draw_styled(view, &Style::nearest(1, light), &v2, 156.0, y);
        }
        let _ = lay;
    }

    /// The overland enchantments panel: one column of up to nine, or two of up to eighteen
    /// with a "More..." that turns the page. A click on one of your own offers to turn it off.
    fn magic_enchantments(
        &mut self,
        app: &mut App,
        lay: &Layout,
        view: (f32, f32, f32),
        active: bool,
        input: &Input,
    ) {
        let list = app.overland_enchantments();
        let two_columns = list.len() >= 10;
        let per_page = if two_columns { 18 } else { 9 };
        if self.first >= list.len() {
            self.first = 0;
        }
        let first = self.first;
        for (i, &(spell, owner)) in list.iter().skip(first).take(per_page).enumerate() {
            let name = app.game.content().rules.spells.get(spell).name.clone();
            let color = app.banner_color(owner.0);
            let (x, y, style) = if two_columns {
                (
                    171.0 + 69.0 * (i % 2) as f32,
                    81.0 + 9.0 * (i / 2) as f32,
                    0,
                )
            } else {
                (171.0, 80.0 + 10.0 * i as f32, 2)
            };
            app.draw_styled(view, &Style::nearest(style, color), &name, x, y);
            let rect = if two_columns {
                lay.rect((x - 2.0, y - 1.0, 65.0, 8.0))
            } else {
                lay.rect((x - 2.0, y - 1.0, 120.0, 9.0))
            };
            if active && owner == HUMAN && ui::hit(rect, true, input).clicked() {
                self.turn_off = Some(spell);
            }
        }
        if list.len() > 18 {
            let more = app.text("magic.more");
            app.draw_styled(view, &Style::nearest(0, WHITE), &more, 286.0, 163.0);
            if active && ui::hit(lay.rect((286.0, 163.0, 21.0, 7.0)), true, input).clicked() {
                self.first += 18;
                if self.first >= list.len() {
                    self.first = 0;
                }
            }
        }
    }

    /// The alchemy window at (80, 60): drag the star along the bar to pick how much, the arrow
    /// button turns the exchange round, OK does it.
    fn alchemy_popup(&mut self, app: &mut App, lay: &Layout, view: (f32, f32, f32), input: &Input) {
        let Some(mut ui) = self.alchemy else {
            return;
        };
        let (px, py) = (80.0, 60.0);
        let gold = app.game.player(HUMAN).map_or(0, |p| p.treasury().gold);
        let mana = app.game.player(HUMAN).map_or(0, |p| p.treasury().mana);
        let skilled = app
            .game
            .wizard(HUMAN)
            .is_some_and(|m| m.has(Retort::Alchemy));
        let divisor = if skilled { 1 } else { 2 };
        let pool = if ui.gold_to_mana { gold } else { mana };
        if pool <= 1 {
            ui.slider = 3;
        }
        let amount = alchemy_amount(pool, ui.slider);
        let (mx, _) = input.mouse_position();
        let vx = (mx - lay.origin.x) / lay.scale;
        // The slider bar.
        let bar = lay.rect((px + 46.0, py + 22.0, 62.0, 12.0));
        if ui::hit(bar, pool > 1, input).held() {
            ui.slider = ((vx - (px + 50.0)).round() as i32 + 3).clamp(3, 53);
        }
        app.draw_pic(lay, "magic", 52, 0, px - 5.0, py);
        if !ui.gold_to_mana {
            app.draw_pic(lay, "magic", 59, 0, px + 7.0, py + 10.0);
        }
        // The bar's shaded part up to the star, then the star.
        app.draw_pic_rows(lay, "magic", 57, px + 50.0, py + 25.0, 0.0, 1000.0);
        app.draw_pic(lay, "magic", 58, 0, px + 47.0 + ui.slider as f32, py + 25.0);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let (gold_amount, power_amount) = if ui.gold_to_mana {
            (amount, amount / divisor)
        } else {
            (amount / divisor, amount)
        };
        app.draw_styled(
            view,
            &Style::nearest(1, light).right(),
            &gold_amount.to_string(),
            px + 28.0,
            py + 26.0,
        );
        app.draw_styled(
            view,
            &Style::nearest(1, light).right(),
            "GP",
            px + 40.0,
            py + 26.0,
        );
        app.draw_styled(
            view,
            &Style::nearest(1, light).right(),
            &power_amount.to_string(),
            px + 130.0,
            py + 26.0,
        );
        app.draw_styled(
            view,
            &Style::nearest(1, light).right(),
            "PP",
            px + 142.0,
            py + 26.0,
        );
        // Cancel, the arrow that turns the exchange, and OK.
        let click = |app: &mut App, entry: usize, x: f32, y: f32| {
            let r = app.draw_pic(lay, "magic", entry, 0, x, y);
            r.is_some_and(|r| ui::hit(r, true, input).clicked())
        };
        let cancel = click(app, 53, px + 13.0, py + 39.0);
        let turn = click(
            app,
            if ui.gold_to_mana { 56 } else { 55 },
            px + 66.0,
            py + 39.0,
        );
        let ok = click(app, 54, px + 96.0, py + 39.0);
        if turn || input.key_pressed(KeyCode::Space) {
            ui.gold_to_mana = !ui.gold_to_mana;
            ui.slider = 3;
        }
        if ok && amount > 0 {
            let events = app.game.apply(Command::Alchemy {
                player: HUMAN,
                amount,
                gold_to_mana: ui.gold_to_mana,
            });
            app.note(events);
            self.alchemy = None;
            return;
        }
        if cancel || ok || input.key_pressed(KeyCode::Escape) {
            self.alchemy = None;
            return;
        }
        self.alchemy = Some(ui);
    }
}

impl Page for MagicUi {
    fn frame(&mut self, app: &mut App, input: &Input) {
        let modal = self.alchemy.is_some() || self.turn_off.is_some();
        if !modal
            && (input.key_pressed(KeyCode::Escape)
                || input.key_pressed(KeyCode::M)
                || input.key_pressed(KeyCode::O))
        {
            app.screen = Screen::Map;
            return;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let full = lay.rect((0.0, 0.0, 320.0, 200.0));
        if app.draw_pic(&lay, "magic", 0, 0, 0.0, 0.0).is_none() {
            draw_rectangle(
                full.x,
                full.y,
                full.w,
                full.h,
                Color::new(0.18, 0.2, 0.3, 1.0),
            );
        }
        self.magic_staves(app, &lay, !modal, input);
        self.magic_text(app, &lay, view);
        app.magic_gems(&lay, view, !modal, input, &mut self.first_wizard);
        self.magic_enchantments(app, &lay, view, !modal, input);

        // The Alchemy and OK buttons.
        let alchemy = app.draw_pic(&lay, "magic", 13, 0, 232.0, 181.0);
        let ok = app.draw_pic(&lay, "magic", 14, 0, 291.0, 181.0);
        let hit = |r: Option<Rect>| r.is_some_and(|r| ui::hit(r, true, input).clicked());
        if !modal {
            if hit(alchemy)
                || (alchemy.is_none()
                    && button(
                        lay.rect((232.0, 181.0, 58.0, 15.0)),
                        &app.text("magic.alchemy"),
                        true,
                        input,
                    ))
            {
                let gold_to_mana = true;
                self.alchemy = Some(AlchemyUi {
                    gold_to_mana,
                    slider: 3,
                });
            }
            if hit(ok)
                || (ok.is_none()
                    && button(
                        lay.rect((291.0, 181.0, 26.0, 15.0)),
                        &app.text("ui.ok"),
                        true,
                        input,
                    ))
            {
                app.screen = Screen::Map;
            }
        }
        if self.alchemy.is_some() {
            self.alchemy_popup(app, &lay, view, input);
        }
        if let Some(spell) = self.turn_off {
            let name = app.game.content().rules.spells.get(spell).name.clone();
            let question = app.text_with("magic.cancel_spell", &Args::new().text("spell", &name));
            match app.confirm_box(&question, input) {
                Some(true) => {
                    self.turn_off = None;
                    let events = app.game.apply(Command::TurnOffSpell {
                        player: HUMAN,
                        spell,
                    });
                    app.note(events);
                    self.first = 0;
                }
                Some(false) => self.turn_off = None,
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn contactable_wizards_come_before_unmet_and_gone_ones() {
        let mut wizards = [(true, true), (false, false), (false, true), (true, false)];
        wizards.sort_by_key(|&(gone, met)| gem_rank(gone, met));
        assert_eq!(wizards[0], (false, true));
        assert_eq!(wizards[1], (false, false));
        assert!(wizards[2].0 && wizards[3].0);
    }

    use super::*;

    #[test]
    fn the_staves_always_add_up_to_fifty() {
        for locked in [
            [false; 3],
            [true, false, false],
            [false, true, false],
            [false, false, true],
            [true, true, false],
        ] {
            for changed in 0..3 {
                for new in 0..=60 {
                    let out = staff_adjust([17, 17, 16], locked, changed, new);
                    assert_eq!(out.iter().sum::<i32>(), 50, "{locked:?} {changed} {new}");
                    assert!(out.iter().all(|&v| (0..=50).contains(&v)));
                    for i in 0..3 {
                        if locked[i] {
                            assert_eq!(out[i], [17, 17, 16][i], "a locked staff stays");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_free_pair_share_what_a_staff_takes_and_gives() {
        // Skill up four: research and mana give two each.
        assert_eq!(staff_adjust([17, 17, 16], [false; 3], 2, 20), [15, 15, 20]);
        // An odd point stays where it was: skill up three moves two.
        assert_eq!(staff_adjust([17, 17, 16], [false; 3], 2, 19), [16, 16, 18]);
        // Skill down four: research and mana take two each.
        assert_eq!(staff_adjust([17, 17, 16], [false; 3], 2, 12), [19, 19, 12]);
        // With nothing left in one of them the other pays.
        assert_eq!(staff_adjust([0, 30, 20], [false; 3], 2, 40), [0, 10, 40]);
    }

    #[test]
    fn a_locked_staff_holds_and_the_free_one_takes_the_difference() {
        // Research locked: mana pays for skill, to the last point it has.
        assert_eq!(
            staff_adjust([17, 17, 16], [false, true, false], 2, 20),
            [13, 17, 20]
        );
        assert_eq!(
            staff_adjust([5, 17, 28], [false, true, false], 2, 40),
            [0, 17, 33]
        );
        // Both others locked: nothing moves.
        assert_eq!(
            staff_adjust([17, 17, 16], [true, true, false], 2, 30),
            [17, 17, 16]
        );
    }

    #[test]
    fn a_split_becomes_staves_and_back() {
        assert_eq!(staves_from_split(34, 33, 33), [18, 16, 16]);
        assert_eq!(staves_from_split(100, 0, 0), [50, 0, 0]);
        assert_eq!(staves_from_split(0, 0, 100), [0, 0, 50]);
    }

    #[test]
    fn the_alchemy_slider_converts_up_to_everything() {
        assert_eq!(alchemy_amount(100, 3), 0);
        assert_eq!(alchemy_amount(100, 53), 100);
        assert_eq!(alchemy_amount(100, 28), 50);
        assert_eq!(alchemy_amount(5000, 53), 999);
    }
}
