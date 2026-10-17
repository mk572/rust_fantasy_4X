//! The question before the turn ends when the reserves cannot pay the upkeep: which lasting
//! spells and summoned creatures, which troops, ships and heroes to give up (tasks 62 and 292).
//! Without an answer the turn change lets go of the newest. A second page appears when released
//! heroes bring more items home than the armory holds: which to disband for mana.

use f4x_core::cast::UpkeepKind;
use f4x_core::shortfall::{ItemOverflow, TreasuryShortfall};
use f4x_core::upkeep::Shortfall;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::frame::{Align, Layout};
use crate::view::{App, HUMAN};

/// Row height and the most rows shown, in the original's pixels (the screen is 320 by 200).
const ROW: f32 = 10.0;
const SHOWN: usize = 11;

/// A line of the list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Line {
    Header(&'static str),
    Mana(usize),
    Troop(usize),
    Item(usize),
}

/// What is on the screen.
pub(crate) struct UpkeepPrompt {
    mana: Option<Shortfall>,
    mana_marked: Vec<bool>,
    troops: Option<TreasuryShortfall>,
    troop_marked: Vec<bool>,
    /// The second page: the items that do not fit, and which of them go.
    items: Option<ItemOverflow>,
    item_marked: Vec<bool>,
    cursor: usize,
}

impl UpkeepPrompt {
    /// Marks what the turn change would give up by itself, so Enter keeps the default.
    pub(crate) fn new(mana: Option<Shortfall>, troops: Option<TreasuryShortfall>) -> Self {
        let mana_marked = mana.as_ref().map_or_else(Vec::new, |m| {
            let default = m.default_release();
            m.lines.iter().map(|l| default.contains(&l.kind)).collect()
        });
        let troop_marked = troops.as_ref().map_or_else(Vec::new, |t| {
            t.troops
                .iter()
                .map(|l| t.default_release.contains(&l.unit))
                .collect()
        });
        let mut prompt = Self {
            mana,
            mana_marked,
            troops,
            troop_marked,
            items: None,
            item_marked: Vec::new(),
            cursor: 0,
        };
        prompt.cursor = prompt.first_selectable();
        prompt
    }

    fn lines(&self) -> Vec<Line> {
        let mut out = Vec::new();
        if let Some(items) = &self.items {
            out.push(Line::Header(
                "Too many items for the armory: mark what to disband for mana",
            ));
            out.extend((0..items.lines.len()).map(Line::Item));
            return out;
        }
        if let Some(m) = &self.mana {
            out.push(Line::Header("Mana"));
            out.extend((0..m.lines.len()).map(Line::Mana));
        }
        if let Some(t) = &self.troops {
            out.push(Line::Header("Gold and food"));
            out.extend((0..t.troops.len()).map(Line::Troop));
        }
        out
    }

    fn first_selectable(&self) -> usize {
        self.lines()
            .iter()
            .position(|l| !matches!(l, Line::Header(_)))
            .unwrap_or(0)
    }

    fn released_units(&self) -> Vec<hecs::Entity> {
        self.troops.as_ref().map_or_else(Vec::new, |t| {
            t.troops
                .iter()
                .zip(&self.troop_marked)
                .filter(|(_, &m)| m)
                .map(|(l, _)| l.unit)
                .collect()
        })
    }

    fn released_upkeep(&self) -> Vec<UpkeepKind> {
        self.mana.as_ref().map_or_else(Vec::new, |m| {
            m.lines
                .iter()
                .zip(&self.mana_marked)
                .filter(|(_, &x)| x)
                .map(|(l, _)| l.kind)
                .collect()
        })
    }

    fn scrapped(&self) -> Vec<f4x_core::items::armory::ItemSpot> {
        self.items.as_ref().map_or_else(Vec::new, |o| {
            o.lines
                .iter()
                .zip(&self.item_marked)
                .filter(|(_, &m)| m)
                .map(|(l, _)| l.spot)
                .collect()
        })
    }

    fn toggle(&mut self, line: Line) {
        let flip = |v: &mut Vec<bool>, i: usize| v[i] = !v[i];
        match line {
            Line::Mana(i) => flip(&mut self.mana_marked, i),
            Line::Troop(i) => flip(&mut self.troop_marked, i),
            Line::Item(i) => flip(&mut self.item_marked, i),
            Line::Header(_) => {}
        }
    }

    fn marked(&self, line: Line) -> bool {
        match line {
            Line::Mana(i) => self.mana_marked[i],
            Line::Troop(i) => self.troop_marked[i],
            Line::Item(i) => self.item_marked[i],
            Line::Header(_) => false,
        }
    }

    /// What the line says, and its price on the right.
    fn text(&self, line: Line) -> (String, String) {
        match line {
            Line::Mana(i) => {
                let l = &self.mana.as_ref().unwrap().lines[i];
                (l.label.clone(), format!("{} mana", l.mana))
            }
            Line::Troop(i) => {
                let l = &self.troops.as_ref().unwrap().troops[i];
                let mut label = l.label.clone();
                if l.hero {
                    label.push_str(" (hero: its items come home)");
                } else if l.ship && l.aboard > 0 {
                    label.push_str(&format!(" ({} aboard: sails for the shore)", l.aboard));
                }
                (label, format!("{} gold  {} food", l.gold, l.food))
            }
            Line::Item(i) => {
                let l = &self.items.as_ref().unwrap().lines[i];
                (l.label.clone(), format!("+{} mana", l.mana))
            }
            Line::Header(h) => (h.to_string(), String::new()),
        }
    }

    /// The line under the list: what is still missing after the marked ones go.
    fn status(&self) -> (String, bool) {
        if let Some(o) = &self.items {
            let n = self.item_marked.iter().filter(|&&m| m).count();
            return if n >= o.excess {
                ("Enough items marked.".to_string(), true)
            } else {
                (
                    format!("Mark {} more, or the game picks.", o.excess - n),
                    false,
                )
            };
        }
        let mut missing = Vec::new();
        if let Some(m) = &self.mana {
            let freed: i32 = m
                .lines
                .iter()
                .zip(&self.mana_marked)
                .filter(|(_, &x)| x)
                .map(|(l, _)| l.mana)
                .sum();
            let short = (m.owed - freed - m.reserve).max(0);
            if short > 0 {
                missing.push(format!("{short} mana"));
            }
        }
        if let Some(t) = &self.troops {
            let (food, gold) = t.missing(&self.released_units());
            if food > 0 {
                missing.push(format!("{food} food"));
            }
            if gold > 0 {
                missing.push(format!("{gold} gold"));
            }
        }
        if missing.is_empty() {
            ("Enough: the rest is paid.".to_string(), true)
        } else {
            (
                format!("Short by {}: the newest go.", missing.join(", ")),
                false,
            )
        }
    }
}

impl App {
    /// Ends the turn, first asking what to give up if a reserve cannot pay the upkeep.
    pub(crate) fn request_end_turn(&mut self) {
        let mana = self.game.upkeep_shortfall(HUMAN);
        let troops = self.game.treasury_shortfall(HUMAN);
        if (mana.is_some() || troops.is_some()) && self.upkeep_prompt.is_none() {
            self.upkeep_prompt = Some(UpkeepPrompt::new(mana, troops));
        } else {
            self.finish_turn();
        }
    }

    /// For screenshots: run the human out of gold and mana, with a hero and some troops to lose,
    /// and open the prompt.
    pub(crate) fn open_test_upkeep(&mut self) {
        let Some(at) = self.game.economy().cities().first().map(|c| c.2) else {
            return;
        };
        let rules = &self.game.content().rules;
        let troops: Vec<_> = ["high_men_spearmen", "high_men_swordsmen", "high_men_bowmen"]
            .iter()
            .filter_map(|id| rules.units.id_of(id))
            .collect();
        let spells: Vec<_> = rules
            .spells
            .defs()
            .iter()
            .filter(|s| s.upkeep > 0 && s.target == f4x_core::rules::SpellTarget::None)
            .take(2)
            .filter_map(|s| rules.spells.id_of(&s.id))
            .collect();
        for u in troops.iter().cycle().take(6) {
            self.game.spawn_unit(HUMAN, at, *u);
        }
        let _ = self.game.spawn_hero(HUMAN, at, 0);
        if let Some(p) = self.game.player_mut(HUMAN) {
            p.gold = 0;
            p.mana = 0;
            p.magic.global.extend(spells);
        }
        self.game.apply(Command::SetTaxRate {
            player: HUMAN,
            rate: 0,
        });
        self.request_end_turn();
    }

    pub(crate) fn finish_turn(&mut self) {
        self.end_turn();
        self.save_to(std::path::Path::new(crate::view::AUTOSAVE));
    }

    /// Send what the player chose, then end the turn.
    fn answer_upkeep(&mut self, prompt: &UpkeepPrompt) {
        let items = prompt.released_upkeep();
        if !items.is_empty() {
            let events = self.game.apply(Command::ReleaseUpkeep {
                player: HUMAN,
                items,
            });
            self.note(events);
        }
        let units = prompt.released_units();
        let scrap = prompt.scrapped();
        if !units.is_empty() {
            let events = self.game.apply(Command::ReleaseTroops {
                player: HUMAN,
                units,
                scrap,
            });
            self.note(events);
        }
        self.finish_turn();
    }

    /// One frame of the prompt; returns whether it took the frame.
    pub(crate) fn upkeep_prompt_frame(&mut self) -> bool {
        let Some(mut prompt) = self.upkeep_prompt.take() else {
            return false;
        };
        // The game may have changed under it (a building sold, a spell turned off): the prompt
        // goes if nothing is short any more.
        if self.game.upkeep_shortfall(HUMAN).is_none()
            && self.game.treasury_shortfall(HUMAN).is_none()
        {
            return true;
        }
        let lines = prompt.lines();
        let n = lines.len();
        let rows = n.min(SHOWN);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let (bx, bw) = (8.0, 224.0);
        let bh = 62.0 + rows as f32 * ROW;
        let by = ((200.0 - bh) / 2.0).floor();
        let first = prompt
            .cursor
            .saturating_sub(SHOWN - 1)
            .min(n.saturating_sub(rows));
        let row_rect = |i: usize| {
            lay.rect((
                bx + 4.0,
                by + 24.0 + (i - first) as f32 * ROW,
                bw - 8.0,
                ROW,
            ))
        };
        let yes_at = (bx + bw - 70.0, by + bh - 21.0);
        let no_at = (bx + bw - 138.0, by + bh - 21.0);
        let yes = lay.rect((yes_at.0, yes_at.1, 65.0, 17.0));
        let no = lay.rect((no_at.0, no_at.1, 65.0, 17.0));

        let mut confirm = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Y);
        let mut cancelled = is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::N);
        let selectable = |i: usize| !matches!(lines[i], Line::Header(_));
        if is_key_pressed(KeyCode::Down) {
            if let Some(i) = (prompt.cursor + 1..n).find(|&i| selectable(i)) {
                prompt.cursor = i;
            }
        }
        if is_key_pressed(KeyCode::Up) {
            if let Some(i) = (0..prompt.cursor).rev().find(|&i| selectable(i)) {
                prompt.cursor = i;
            }
        }
        if is_key_pressed(KeyCode::Space) {
            prompt.toggle(lines[prompt.cursor]);
        }
        let mouse = Vec2::from(mouse_position());
        if is_mouse_button_pressed(MouseButton::Left) {
            if yes.contains(mouse) {
                confirm = true;
            } else if no.contains(mouse) {
                cancelled = true;
            } else if let Some(i) =
                (first..first + rows).find(|&i| selectable(i) && row_rect(i).contains(mouse))
            {
                prompt.cursor = i;
                prompt.toggle(lines[i]);
            }
        }
        if cancelled {
            if prompt.items.is_some() {
                // Back to the first page.
                prompt.items = None;
                prompt.item_marked.clear();
                prompt.cursor = prompt.first_selectable();
                self.upkeep_prompt = Some(prompt);
            }
            return true;
        }
        if confirm {
            if prompt.items.is_none() {
                let released = prompt.released_units();
                if let Some(over) = self.game.item_overflow(HUMAN, &released) {
                    prompt.item_marked = over
                        .lines
                        .iter()
                        .map(|l| over.default_scrap.contains(&l.spot))
                        .collect();
                    prompt.items = Some(over);
                    prompt.cursor = prompt.first_selectable();
                    self.upkeep_prompt = Some(prompt);
                    return true;
                }
            }
            self.answer_upkeep(&prompt);
            return true;
        }

        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        // The box: the original's stone brown with a dark edge.
        let panel = lay.rect((bx, by, bw, bh));
        draw_rectangle(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            Color::new(0.36, 0.27, 0.20, 1.0),
        );
        draw_rectangle_lines(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            2.0 * lay.scale,
            Color::new(0.12, 0.08, 0.05, 1.0),
        );
        let shadow = Color::new(0.05, 0.1, 0.2, 1.0);
        let light = Color::new(0.97, 0.95, 0.85, 1.0);
        let gold = Color::new(0.95, 0.78, 0.30, 1.0);
        let red = Color::new(0.85, 0.20, 0.15, 1.0);
        let say = |app: &mut App, color: Color, text: &str, x: f32, y: f32, align: Align| {
            app.bm_text(view, 4, shadow, text, x + 1.0, y + 1.0, align);
            app.bm_text(view, 4, color, text, x, y, align);
        };
        let middle = bx + bw / 2.0;
        say(
            self,
            light,
            "Reserves cannot pay everything.",
            middle,
            by + 4.0,
            Align::Center,
        );
        say(
            self,
            gold,
            "Click to mark who goes.",
            middle,
            by + 14.0,
            Align::Center,
        );
        for (i, &line) in lines.iter().enumerate().skip(first).take(rows) {
            let y = by + 24.0 + (i - first) as f32 * ROW;
            let (label, price) = prompt.text(line);
            if let Line::Header(_) = line {
                say(self, gold, &label, bx + 8.0, y + 1.0, Align::Left);
                continue;
            }
            if i == prompt.cursor {
                let r = row_rect(i);
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.5, 0.38, 0.26, 1.0));
            }
            let marked = prompt.marked(line);
            let b = lay.rect((bx + 8.0, y + 1.0, 7.0, 7.0));
            draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.12, 0.08, 0.05, 1.0));
            if marked {
                draw_rectangle(
                    b.x + lay.scale,
                    b.y + lay.scale,
                    b.w - 2.0 * lay.scale,
                    b.h - 2.0 * lay.scale,
                    red,
                );
            }
            say(
                self,
                if marked { red } else { light },
                &label,
                bx + 19.0,
                y + 1.0,
                Align::Left,
            );
            say(
                self,
                if marked { red } else { light },
                &price,
                bx + bw - 8.0,
                y + 1.0,
                Align::Right,
            );
        }
        let (status, fine) = prompt.status();
        let ok_colour = if fine {
            Color::new(0.55, 0.9, 0.5, 1.0)
        } else {
            Color::new(1.0, 0.7, 0.3, 1.0)
        };
        say(
            self,
            ok_colour,
            &status,
            bx + 8.0,
            by + bh - 33.0,
            Align::Left,
        );
        if first + rows < n {
            say(
                self,
                gold,
                "More below",
                bx + 8.0,
                by + bh - 15.0,
                Align::Left,
            );
        }
        let down = is_mouse_button_down(MouseButton::Left);
        self.draw_pic(
            &lay,
            "resource",
            3,
            usize::from(down && yes.contains(mouse)),
            yes_at.0,
            yes_at.1,
        );
        self.draw_pic(
            &lay,
            "resource",
            4,
            usize::from(down && no.contains(mouse)),
            no_at.0,
            no_at.1,
        );
        self.upkeep_prompt = Some(prompt);
        true
    }
}
