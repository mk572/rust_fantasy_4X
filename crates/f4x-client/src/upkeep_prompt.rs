//! The question before the turn ends when the mana reserve cannot pay the upkeep: which lasting
//! spells and summoned creatures to give up (task 62). Without it the turn change gives up the
//! newest ones.

use f4x_core::cast::UpkeepKind;
use f4x_core::upkeep::Shortfall;
use f4x_core::Command;
use macroquad::prelude::*;

use crate::ui::GOLD;
use crate::view::{App, HUMAN};

const ROW: f32 = 26.0;
const SHOWN: usize = 12;

/// What is on the screen: the shortfall, a mark per line, and the line under the cursor.
pub(crate) struct UpkeepPrompt {
    short: Shortfall,
    marked: Vec<bool>,
    cursor: usize,
}

impl UpkeepPrompt {
    /// Marks what the turn change would give up by itself (the newest first), so Enter keeps
    /// the default.
    pub(crate) fn new(short: Shortfall) -> Self {
        let default = short.default_release();
        let marked = short
            .lines
            .iter()
            .map(|l| default.contains(&l.kind))
            .collect();
        Self {
            short,
            marked,
            cursor: 0,
        }
    }

    /// The mana still missing after the marked items are given up.
    fn missing(&self) -> i32 {
        let freed: i32 = self
            .short
            .lines
            .iter()
            .zip(&self.marked)
            .filter(|(_, &m)| m)
            .map(|(l, _)| l.mana)
            .sum();
        (self.short.owed - freed - self.short.reserve).max(0)
    }

    fn chosen(&self) -> Vec<UpkeepKind> {
        self.short
            .lines
            .iter()
            .zip(&self.marked)
            .filter(|(_, &m)| m)
            .map(|(l, _)| l.kind)
            .collect()
    }
}

impl App {
    /// Ends the turn, first asking what to give up if the reserve cannot pay the mana upkeep.
    pub(crate) fn request_end_turn(&mut self) {
        match self.game.upkeep_shortfall(HUMAN) {
            Some(short) if self.upkeep_prompt.is_none() => {
                self.upkeep_prompt = Some(UpkeepPrompt::new(short));
            }
            _ => self.finish_turn(),
        }
    }

    pub(crate) fn finish_turn(&mut self) {
        self.end_turn();
        self.save_to(std::path::Path::new(crate::view::AUTOSAVE));
    }

    /// One frame of the prompt; returns whether it took the frame.
    pub(crate) fn upkeep_prompt_frame(&mut self) -> bool {
        let Some(mut prompt) = self.upkeep_prompt.take() else {
            return false;
        };
        // The game may have changed under it (a spell released by hand): ask again if so.
        if self.game.upkeep_shortfall(HUMAN).is_none() {
            return true;
        }
        let n = prompt.short.lines.len();
        let (w, h) = (screen_width(), screen_height());
        let rows = n.min(SHOWN);
        let (bw, bh) = (620.0, 150.0 + rows as f32 * ROW);
        let (bx, by) = (w / 2.0 - bw / 2.0, h / 2.0 - bh / 2.0);
        let first = prompt
            .cursor
            .saturating_sub(SHOWN - 1)
            .min(n.saturating_sub(rows));
        let row_rect = |i: usize| {
            Rect::new(
                bx + 20.0,
                by + 80.0 + (i - first) as f32 * ROW,
                bw - 40.0,
                ROW,
            )
        };
        let ok = Rect::new(bx + bw - 250.0, by + bh - 40.0, 110.0, 28.0);
        let cancel = Rect::new(bx + bw - 130.0, by + bh - 40.0, 110.0, 28.0);

        let mut confirm = is_key_pressed(KeyCode::Enter);
        let mut cancelled = is_key_pressed(KeyCode::Escape);
        if is_key_pressed(KeyCode::Down) {
            prompt.cursor = (prompt.cursor + 1).min(n - 1);
        }
        if is_key_pressed(KeyCode::Up) {
            prompt.cursor = prompt.cursor.saturating_sub(1);
        }
        if is_key_pressed(KeyCode::Space) {
            prompt.marked[prompt.cursor] = !prompt.marked[prompt.cursor];
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            let at: Vec2 = mouse_position().into();
            if ok.contains(at) {
                confirm = true;
            } else if cancel.contains(at) {
                cancelled = true;
            } else if let Some(i) = (first..first + rows).find(|&i| row_rect(i).contains(at)) {
                prompt.cursor = i;
                prompt.marked[i] = !prompt.marked[i];
            }
        }
        if cancelled {
            return true;
        }
        if confirm {
            let items = prompt.chosen();
            if !items.is_empty() {
                let events = self.game.apply(Command::ReleaseUpkeep {
                    player: HUMAN,
                    items,
                });
                self.note(events);
            }
            self.finish_turn();
            return true;
        }

        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        draw_rectangle(bx, by, bw, bh, Color::new(0.08, 0.06, 0.04, 0.96));
        draw_rectangle_lines(bx, by, bw, bh, 3.0, GOLD);
        crate::frame::text_centered(
            "Your mana reserve cannot pay for everything.",
            w / 2.0,
            by + 34.0,
            24.0,
            WHITE,
        );
        let short = &prompt.short;
        crate::frame::text_centered(
            &format!(
                "Upkeep {} mana, reserve {}. Mark what to give up.",
                short.owed, short.reserve
            ),
            w / 2.0,
            by + 62.0,
            20.0,
            GOLD,
        );
        for i in first..first + rows {
            let r = row_rect(i);
            if i == prompt.cursor {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.25, 0.2, 0.1, 0.8));
            }
            let line = &short.lines[i];
            let mark = if prompt.marked[i] { "[x]" } else { "[ ]" };
            let color = if prompt.marked[i] { RED } else { WHITE };
            draw_text(
                format!("{mark} {}", line.label),
                r.x + 8.0,
                r.y + 19.0,
                20.0,
                color,
            );
            let cost = format!("{} mana", line.mana);
            let tw = measure_text(&cost, None, 20, 1.0).width;
            draw_text(&cost, r.x + r.w - tw - 8.0, r.y + 19.0, 20.0, color);
        }
        let missing = prompt.missing();
        let status = if missing == 0 {
            "Enough: the reserve pays the rest.".to_string()
        } else {
            format!("{missing} mana still missing: the game will give up the newest.")
        };
        draw_text(
            &status,
            bx + 20.0,
            by + bh - 20.0,
            18.0,
            if missing == 0 { GREEN } else { ORANGE },
        );
        for (r, label) in [(ok, "Enter: OK"), (cancel, "Esc: Back")] {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.2, 0.15, 0.08, 1.0));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, GOLD);
            crate::frame::text_centered(label, r.x + r.w / 2.0, r.y + 20.0, 18.0, WHITE);
        }
        self.upkeep_prompt = Some(prompt);
        true
    }
}
