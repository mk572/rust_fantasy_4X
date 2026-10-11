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

use f4x_core::city::PlayerId;
use f4x_core::magic::Retort;
use f4x_core::rules::SpellId;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
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

/// What the magic screen remembers while it is open.
#[derive(Debug, Clone, Default)]
pub(crate) struct MagicUi {
    pub pos: [i32; 3],
    pub locked: [bool; 3],
    pub drag: Option<usize>,
    /// The first overland enchantment shown, when there are more than eighteen.
    pub first: usize,
    pub alchemy: Option<AlchemyUi>,
    /// Asking whether to turn off one of the wizard's own enchantments.
    pub turn_off: Option<SpellId>,
}

impl App {
    pub fn open_magic(&mut self) {
        let (m, r, s) = {
            let m = self.game.magic(HUMAN);
            m.map_or((34, 33, 33), |m| (m.mana_pct, m.research_pct, m.skill_pct))
        };
        self.magic_ui = MagicUi {
            pos: staves_from_split(m, r, s),
            ..Default::default()
        };
        self.screen = Screen::Magic;
    }

    /// Every overland enchantment in play, with its wizard: the original lists all of them.
    fn overland_enchantments(&self) -> Vec<(SpellId, f4x_core::city::PlayerId)> {
        self.game
            .players
            .iter()
            .flat_map(|p| p.magic.global.iter().map(move |&s| (s, p.id)))
            .collect()
    }

    pub(crate) fn magic_screen(&mut self) {
        let modal = self.magic_ui.alchemy.is_some() || self.magic_ui.turn_off.is_some();
        if !modal
            && (is_key_pressed(KeyCode::Escape)
                || is_key_pressed(KeyCode::M)
                || is_key_pressed(KeyCode::O))
        {
            self.screen = Screen::Map;
            return;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let full = lay.rect((0.0, 0.0, 320.0, 200.0));
        if self.draw_pic(&lay, "magic", 0, 0, 0.0, 0.0).is_none() {
            draw_rectangle(
                full.x,
                full.y,
                full.w,
                full.h,
                Color::new(0.18, 0.2, 0.3, 1.0),
            );
        }
        self.magic_staves(&lay, !modal);
        self.magic_text(&lay, view);
        self.magic_gems(&lay, view, !modal);
        self.magic_enchantments(&lay, view, !modal);

        // The Alchemy and OK buttons.
        let alchemy = self.draw_pic(&lay, "magic", 13, 0, 232.0, 181.0);
        let ok = self.draw_pic(&lay, "magic", 14, 0, 291.0, 181.0);
        let hit = |r: Option<Rect>| {
            r.is_some_and(|r| r.contains(mouse_position().into()))
                && is_mouse_button_pressed(MouseButton::Left)
        };
        if !modal {
            if hit(alchemy)
                || (alchemy.is_none()
                    && button(lay.rect((232.0, 181.0, 58.0, 15.0)), "Alchemy", true))
            {
                let gold_to_mana = true;
                self.magic_ui.alchemy = Some(AlchemyUi {
                    gold_to_mana,
                    slider: 3,
                });
            }
            if hit(ok) || (ok.is_none() && button(lay.rect((291.0, 181.0, 26.0, 15.0)), "OK", true))
            {
                self.screen = Screen::Map;
            }
        }
        if self.magic_ui.alchemy.is_some() {
            self.alchemy_popup(&lay, view);
        }
        if let Some(spell) = self.magic_ui.turn_off {
            let name = self.game.content.rules.spells.get(spell).name.clone();
            match self.confirm_box(&format!("Do you wish to cancel your {name} spell?")) {
                Some(true) => {
                    self.magic_ui.turn_off = None;
                    let events = self.game.apply(Command::TurnOffSpell {
                        player: HUMAN,
                        spell,
                    });
                    self.note(events);
                    self.magic_ui.first = 0;
                }
                Some(false) => self.magic_ui.turn_off = None,
                None => {}
            }
        }
    }

    /// The three staves: drawn empty above the line and full below it, locked or not, and
    /// dragged with the mouse (the share changes as the mouse moves and is kept on release).
    fn magic_staves(&mut self, lay: &Layout, active: bool) {
        let (mx, my) = mouse_position();
        let virtual_y = (my - lay.origin.y) / lay.scale;
        let virtual_x = (mx - lay.origin.x) / lay.scale;
        if active {
            // Locks.
            if is_mouse_button_pressed(MouseButton::Left) {
                for (i, &sx) in STAFF_X.iter().enumerate() {
                    let lock = (sx - 1.0, 82.0, sx + 12.0, 101.0);
                    if (lock.0..=lock.2).contains(&virtual_x)
                        && (lock.1..=lock.3).contains(&virtual_y)
                    {
                        self.magic_ui.locked[i] = !self.magic_ui.locked[i];
                    } else if (sx - 4.0..=sx + 18.0).contains(&virtual_x)
                        && (83.0..=158.0).contains(&virtual_y)
                    {
                        self.magic_ui.drag = Some(i);
                    }
                }
            }
            if let Some(i) = self.magic_ui.drag {
                if is_mouse_button_down(MouseButton::Left) {
                    let value = (STAFF_FLOOR - virtual_y).round() as i32;
                    let moved = staff_adjust(self.magic_ui.pos, self.magic_ui.locked, i, value);
                    self.magic_ui.pos = moved;
                } else {
                    self.magic_ui.drag = None;
                    self.commit_staves();
                }
            }
        }
        let pos = self.magic_ui.pos;
        let locked = self.magic_ui.locked;
        for i in 0..3 {
            let (empty, full, lock) = STAFF_ART[i];
            let fill = pos[i] as f32;
            let line = STAFF_FLOOR - fill;
            let (ex, ey) = STAFF_AT[i][0];
            let (fx, fy) = STAFF_AT[i][1];
            self.draw_pic_rows(lay, "magic", empty, ex, ey, 79.0, line);
            self.draw_pic_rows(lay, "magic", full, fx, fy, line, 166.0);
            if locked[i] {
                let (lx, ly) = STAFF_AT[i][2];
                self.draw_pic(lay, "magic", lock, 0, lx, ly);
            }
        }
    }

    /// Keep the staves' positions as the power split.
    fn commit_staves(&mut self) {
        let [m, r, s] = self.magic_ui.pos;
        let events = self.game.apply(Command::SetPowerSplit {
            player: HUMAN,
            mana: (2 * m) as u8,
            research: (2 * r) as u8,
            skill: (2 * s) as u8,
        });
        self.note(events);
    }

    /// The income under each staff and the block of facts below them.
    fn magic_text(&mut self, lay: &Layout, view: (f32, f32, f32)) {
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let gold = Color::new(0.95, 0.55, 0.1, 1.0);
        let split = if self.magic_ui.drag.is_some() {
            // While dragging show what the staves would give.
            let base = self.game.power_base(HUMAN);
            let [m, _, s] = self.magic_ui.pos;
            let mut p = self.game.power_split(HUMAN);
            p.mana = base * 2 * m / 100;
            p.skill = base * 2 * s / 100;
            p.research = base - p.mana - p.skill + self.game.city_research(HUMAN);
            p
        } else {
            self.game.power_split(HUMAN)
        };
        let magic = self.game.magic(HUMAN).cloned().unwrap_or_default();
        self.bm_text(
            view,
            2,
            gold,
            &format!("{} MP", split.mana),
            54.0,
            160.0,
            Align::Right,
        );
        if magic.researching.is_none() {
            self.bm_text(view, 2, gold, "No Spell", 67.0, 100.0, Align::Left);
        } else {
            self.bm_text(
                view,
                2,
                gold,
                &format!("{} RP", self.game.research_income(HUMAN)),
                101.0,
                160.0,
                Align::Right,
            );
        }
        self.bm_text(
            view,
            2,
            gold,
            &format!("{} SP", split.skill),
            148.0,
            160.0,
            Align::Right,
        );

        let skill = self.game.casting_skill(HUMAN);
        let reserve = self.game.player(HUMAN).map_or(0, |p| p.mana);
        let base = self.game.power_base(HUMAN);
        let name = |g: &f4x_core::Game, s: SpellId| g.content.rules.spells.get(s).name.clone();
        let casting = magic
            .casting
            .map_or("None".to_string(), |c| name(&self.game, c.spell));
        let researching = magic
            .researching
            .map_or("None".to_string(), |s| name(&self.game, s));
        let summon = self
            .game
            .player(HUMAN)
            .and_then(|p| p.summon_at)
            .and_then(|at| self.game.city_at(at))
            .map_or("None".to_string(), |c| self.city_name(c));
        let rows = [
            (
                "Casting Skill:",
                format!("{skill}({skill})"),
                "Casting:",
                casting,
            ),
            (
                "Magic Reserve:",
                reserve.to_string(),
                "Researching:",
                researching,
            ),
            ("Power Base:", base.to_string(), "Summon To:", summon),
        ];
        for (i, (l1, v1, l2, v2)) in rows.into_iter().enumerate() {
            let y = 177.0 + 8.0 * i as f32;
            self.bm_text(view, 1, light, l1, 5.0, y, Align::Left);
            self.bm_text(view, 1, light, &v1, 90.0, y, Align::Right);
            self.bm_text(view, 1, light, l2, 100.0, y, Align::Left);
            self.bm_text(view, 1, light, &v2, 156.0, y, Align::Left);
        }
        let _ = lay;
    }

    /// The other wizards' gems: grey until the wizard is met, then its picture in its banner
    /// colour with the word for how it feels about you and an icon for each treaty, and broken
    /// once the wizard is gone. A click on a met wizard's gem opens the diplomacy screen.
    fn magic_gems(&mut self, lay: &Layout, view: (f32, f32, f32), active: bool) {
        for i in 0..4 {
            self.draw_pic(lay, "magic", 6, 0, 24.0 + 77.0 * i as f32, 4.0);
        }
        let others: Vec<(PlayerId, u8, u8, bool)> = self
            .game
            .players
            .iter()
            .filter(|p| p.id != HUMAN && !p.neutral && p.id.0 != u8::MAX)
            .map(|p| (p.id, p.portrait, p.banner, p.defeated))
            .collect();
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
            let gem = self.draw_pic(
                lay,
                "lilwiz",
                usize::from(portrait) * 5 + usize::from(banner),
                0,
                x,
                4.0,
            );
            let rel = self.game.relation(HUMAN, id);
            let word = f4x_core::diplomacy::relation_word(rel);
            self.bm_text_set(
                view,
                0,
                3,
                word,
                45.0 + 77.0 * i as f32,
                53.0,
                Align::Center,
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
            let my_banner = self.game.player(HUMAN).map_or(0, |p| p.banner);
            show(self, mine, my_banner);
            for &other in gems.iter().filter(|&&o| o != id) {
                if self.game.have_met(HUMAN, other) {
                    let t = self.game.treaty(id, other);
                    let colour = self.game.player(other).map_or(0, |p| p.banner);
                    show(self, t, colour);
                }
            }
            if active
                && is_mouse_button_pressed(MouseButton::Left)
                && gem.is_some_and(|r| r.contains(mouse_position().into()))
            {
                self.open_diplomacy(id);
            }
        }
    }

    /// The overland enchantments panel: one column of up to nine, or two of up to eighteen
    /// with a "More..." that turns the page. A click on one of your own offers to turn it off.
    fn magic_enchantments(&mut self, lay: &Layout, view: (f32, f32, f32), active: bool) {
        let list = self.overland_enchantments();
        let two_columns = list.len() >= 10;
        let per_page = if two_columns { 18 } else { 9 };
        if self.magic_ui.first >= list.len() {
            self.magic_ui.first = 0;
        }
        let first = self.magic_ui.first;
        for (i, &(spell, owner)) in list.iter().skip(first).take(per_page).enumerate() {
            let name = self.game.content.rules.spells.get(spell).name.clone();
            let color = self.banner_color(owner.0);
            let (x, y, style) = if two_columns {
                (
                    171.0 + 69.0 * (i % 2) as f32,
                    81.0 + 9.0 * (i / 2) as f32,
                    0,
                )
            } else {
                (171.0, 80.0 + 10.0 * i as f32, 2)
            };
            self.bm_text(view, style, color, &name, x, y, Align::Left);
            let rect = if two_columns {
                lay.rect((x - 2.0, y - 1.0, 65.0, 8.0))
            } else {
                lay.rect((x - 2.0, y - 1.0, 120.0, 9.0))
            };
            if active
                && owner == HUMAN
                && is_mouse_button_pressed(MouseButton::Left)
                && rect.contains(mouse_position().into())
            {
                self.magic_ui.turn_off = Some(spell);
            }
        }
        if list.len() > 18 {
            self.bm_text(view, 0, WHITE, "More...", 286.0, 163.0, Align::Left);
            if active
                && is_mouse_button_pressed(MouseButton::Left)
                && lay
                    .rect((286.0, 163.0, 21.0, 7.0))
                    .contains(mouse_position().into())
            {
                self.magic_ui.first += 18;
                if self.magic_ui.first >= list.len() {
                    self.magic_ui.first = 0;
                }
            }
        }
    }

    /// The alchemy window at (80, 60): drag the star along the bar to pick how much, the arrow
    /// button turns the exchange round, OK does it.
    fn alchemy_popup(&mut self, lay: &Layout, view: (f32, f32, f32)) {
        let Some(mut ui) = self.magic_ui.alchemy else {
            return;
        };
        let (px, py) = (80.0, 60.0);
        let gold = self.game.player(HUMAN).map_or(0, |p| p.gold);
        let mana = self.game.player(HUMAN).map_or(0, |p| p.mana);
        let skilled = self
            .game
            .magic(HUMAN)
            .is_some_and(|m| m.has(Retort::Alchemy));
        let divisor = if skilled { 1 } else { 2 };
        let pool = if ui.gold_to_mana { gold } else { mana };
        if pool <= 1 {
            ui.slider = 3;
        }
        let amount = alchemy_amount(pool, ui.slider);
        let (mx, my) = mouse_position();
        let vx = (mx - lay.origin.x) / lay.scale;
        let vy = (my - lay.origin.y) / lay.scale;
        // The slider bar.
        if is_mouse_button_down(MouseButton::Left)
            && (px + 46.0..=px + 108.0).contains(&vx)
            && (py + 22.0..=py + 34.0).contains(&vy)
            && pool > 1
        {
            ui.slider = ((vx - (px + 50.0)).round() as i32 + 3).clamp(3, 53);
        }
        self.draw_pic(lay, "magic", 52, 0, px - 5.0, py);
        if !ui.gold_to_mana {
            self.draw_pic(lay, "magic", 59, 0, px + 7.0, py + 10.0);
        }
        // The bar's shaded part up to the star, then the star.
        self.draw_pic_rows(lay, "magic", 57, px + 50.0, py + 25.0, 0.0, 1000.0);
        self.draw_pic(lay, "magic", 58, 0, px + 47.0 + ui.slider as f32, py + 25.0);
        let light = Color::new(0.95, 0.93, 0.85, 1.0);
        let (gold_amount, power_amount) = if ui.gold_to_mana {
            (amount, amount / divisor)
        } else {
            (amount / divisor, amount)
        };
        self.bm_text(
            view,
            1,
            light,
            &gold_amount.to_string(),
            px + 28.0,
            py + 26.0,
            Align::Right,
        );
        self.bm_text(view, 1, light, "GP", px + 40.0, py + 26.0, Align::Right);
        self.bm_text(
            view,
            1,
            light,
            &power_amount.to_string(),
            px + 130.0,
            py + 26.0,
            Align::Right,
        );
        self.bm_text(view, 1, light, "PP", px + 142.0, py + 26.0, Align::Right);
        // Cancel, the arrow that turns the exchange, and OK.
        let click = |app: &mut App, entry: usize, x: f32, y: f32| {
            let r = app.draw_pic(lay, "magic", entry, 0, x, y);
            r.is_some_and(|r| r.contains(mouse_position().into()))
                && is_mouse_button_pressed(MouseButton::Left)
        };
        let cancel = click(self, 53, px + 13.0, py + 39.0);
        let turn = click(
            self,
            if ui.gold_to_mana { 56 } else { 55 },
            px + 66.0,
            py + 39.0,
        );
        let ok = click(self, 54, px + 96.0, py + 39.0);
        if turn || is_key_pressed(KeyCode::Space) {
            ui.gold_to_mana = !ui.gold_to_mana;
            ui.slider = 3;
        }
        if ok && amount > 0 {
            let events = self.game.apply(Command::Alchemy {
                player: HUMAN,
                amount,
                gold_to_mana: ui.gold_to_mana,
            });
            self.note(events);
            self.magic_ui.alchemy = None;
            return;
        }
        if cancel || ok || is_key_pressed(KeyCode::Escape) {
            self.magic_ui.alchemy = None;
            return;
        }
        self.magic_ui.alchemy = Some(ui);
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
    pub(crate) fn confirm_box(&mut self, text: &str) -> Option<bool> {
        let (w, h) = (screen_width(), screen_height());
        let (bw, bh) = (w.min(560.0), 130.0);
        let (x, y) = ((w - bw) / 2.0, (h - bh) / 2.0);
        draw_rectangle(x, y, bw, bh, Color::new(0.08, 0.06, 0.04, 0.96));
        draw_rectangle_lines(x, y, bw, bh, 3.0, crate::ui::GOLD);
        crate::frame::text_centered(text, w / 2.0, y + 45.0, 22.0, WHITE);
        let yes = button(
            Rect::new(x + bw / 2.0 - 110.0, y + 70.0, 100.0, 36.0),
            "Yes",
            true,
        );
        let no = button(
            Rect::new(x + bw / 2.0 + 10.0, y + 70.0, 100.0, 36.0),
            "No",
            true,
        );
        if yes || is_key_pressed(KeyCode::Y) || is_key_pressed(KeyCode::Enter) {
            return Some(true);
        }
        if no || is_key_pressed(KeyCode::N) || is_key_pressed(KeyCode::Escape) {
            return Some(false);
        }
        None
    }
}

#[cfg(test)]
mod tests {
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
