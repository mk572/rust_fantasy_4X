//! The text console in the client (F2): the same `f4x_console::Console` every other front end
//! uses, with an input line, scrollback and a command history.

use f4x_console::{Access, Console, Mode};
use macroquad::prelude::*;

use crate::ui::{GOLD, PANEL};
use crate::view::{App, Screen, HUMAN};

const MAX_LINES: usize = 500;

pub(crate) struct ConsoleUi {
    console: Console,
    input: String,
    lines: Vec<String>,
    /// Position in the console's history while stepping through it with the arrow keys.
    recall: Option<usize>,
    /// The screen to go back to.
    back: Screen,
}

impl Default for ConsoleUi {
    fn default() -> Self {
        Self {
            console: Console::new(HUMAN, Access::Full),
            input: String::new(),
            lines: vec!["Console. `help` lists the commands; `mode override` allows cheats.".into()],
            recall: None,
            back: Screen::Map,
        }
    }
}

impl App {
    pub(crate) fn toggle_console(&mut self) {
        if self.screen == Screen::Console {
            self.screen = self.console.back;
        } else {
            self.console.back = self.screen;
            self.screen = Screen::Console;
        }
    }

    fn run_console_line(&mut self) {
        let line = std::mem::take(&mut self.console.input);
        self.console.recall = None;
        if line.trim().is_empty() {
            return;
        }
        let prompt = if self.console.console.session.mode == Mode::Override {
            "override>"
        } else {
            "legal>"
        };
        self.console.lines.push(format!("{prompt} {line}"));
        let reply = self.console.console.execute(&mut self.game, &line);
        for l in reply.text().lines() {
            self.console.lines.push(l.to_string());
        }
        // A command may have replaced the game or moved units about.
        self.selection.clear();
        if self.console.console.session.quit {
            self.console.console.session.quit = false;
            self.screen = self.console.back;
        }
        let excess = self.console.lines.len().saturating_sub(MAX_LINES);
        self.console.lines.drain(..excess);
    }

    pub(crate) fn console_screen(&mut self) {
        while let Some(c) = get_char_pressed() {
            if !c.is_control() && c != '`' {
                self.console.input.push(c);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.console.input.pop();
        }
        if is_key_pressed(KeyCode::Enter) {
            self.run_console_line();
        }
        if is_key_pressed(KeyCode::Escape) {
            self.screen = self.console.back;
        }
        let history = self.console.console.history.len();
        if is_key_pressed(KeyCode::Up) && history > 0 {
            let i = self.console.recall.map_or(history - 1, |i| i.saturating_sub(1));
            self.console.recall = Some(i);
            self.console.input = self.console.console.history[i].0.clone();
        }
        if is_key_pressed(KeyCode::Down) {
            match self.console.recall {
                Some(i) if i + 1 < history => {
                    self.console.recall = Some(i + 1);
                    self.console.input = self.console.console.history[i + 1].0.clone();
                }
                _ => {
                    self.console.recall = None;
                    self.console.input.clear();
                }
            }
        }
        if is_key_pressed(KeyCode::Tab) {
            let prefix = self.console.input.clone();
            if !prefix.contains(' ') {
                let hits = self.console.console.complete(&prefix);
                match hits.as_slice() {
                    [one] => self.console.input = format!("{one} "),
                    [] => {}
                    many => self.console.lines.push(many.join("  ")),
                }
            }
        }

        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, PANEL);
        let size = 20.0;
        let rows = ((h - 70.0) / (size * 1.15)) as usize;
        let skip = self.console.lines.len().saturating_sub(rows);
        let mut y = 28.0;
        for line in &self.console.lines[skip..] {
            let color = if line.starts_with("error:") {
                Color::new(1.0, 0.5, 0.45, 1.0)
            } else if line.starts_with("override>") || line.starts_with("legal>") {
                GOLD
            } else {
                WHITE
            };
            draw_text(line, 16.0, y, size, color);
            y += size * 1.15;
        }
        let overriding = self.console.console.session.mode == Mode::Override;
        let (prompt, color) = if overriding {
            ("[OVERRIDE]>", Color::new(1.0, 0.4, 0.35, 1.0))
        } else {
            ("[legal]>", Color::new(0.5, 0.9, 0.5, 1.0))
        };
        let blink = if (get_time() * 2.0) as i64 % 2 == 0 { "_" } else { " " };
        draw_text(
            format!("{prompt} {}{blink}", self.console.input),
            16.0,
            h - 22.0,
            size,
            color,
        );
        draw_text("F2 / Esc: back to the game", w - 250.0, h - 22.0, 16.0, GRAY);
    }
}
