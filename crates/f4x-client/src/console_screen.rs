//! The text console in the client (backquote key): the same `f4x_console::Console` every other front end
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
    /// How many rows the text is scrolled up from its end (0 shows the newest lines).
    scroll: usize,
}

impl Default for ConsoleUi {
    fn default() -> Self {
        Self {
            console: Console::new(HUMAN, Access::Full),
            input: String::new(),
            lines: vec!["Console. `help` lists the commands.".into()],
            recall: None,
            back: Screen::Map,
            scroll: 0,
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
        let prompt = match self.console.console.session.mode {
            Mode::Legal => "legal>",
            Mode::ReadOnly => "readonly>",
        };
        self.console.scroll = 0;
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
            let i = self
                .console
                .recall
                .map_or(history - 1, |i| i.saturating_sub(1));
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
                    many => {
                        self.console.scroll = 0;
                        self.console.lines.push(many.join("  "));
                    }
                }
            }
        }

        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, PANEL);
        let size = 20.0;
        let rows = (((h - 70.0) / (size * 1.15)) as usize).max(1);
        // Long lines wrap; the rows are what scrolls.
        let mut shown: Vec<(String, Color)> = Vec::new();
        for line in &self.console.lines {
            let color = if line.starts_with("error:") {
                Color::new(1.0, 0.5, 0.45, 1.0)
            } else if line.starts_with("legal>") || line.starts_with("readonly>") {
                GOLD
            } else {
                WHITE
            };
            for row in wrap_line(line, w - 32.0, size) {
                shown.push((row, color));
            }
        }
        let furthest = shown.len().saturating_sub(rows);
        let wheel = mouse_wheel().1;
        if wheel != 0.0 {
            let steps = (wheel.abs().ceil() as usize).max(1) * 3;
            self.console.scroll = if wheel > 0.0 {
                self.console.scroll.saturating_add(steps)
            } else {
                self.console.scroll.saturating_sub(steps)
            };
        }
        if is_key_pressed(KeyCode::PageUp) {
            self.console.scroll = self.console.scroll.saturating_add(rows.saturating_sub(1));
        }
        if is_key_pressed(KeyCode::PageDown) {
            self.console.scroll = self.console.scroll.saturating_sub(rows.saturating_sub(1));
        }
        if is_key_pressed(KeyCode::Home) {
            self.console.scroll = furthest;
        }
        if is_key_pressed(KeyCode::End) {
            self.console.scroll = 0;
        }
        self.console.scroll = self.console.scroll.min(furthest);
        let skip = furthest - self.console.scroll;
        let mut y = 28.0;
        for (row, color) in shown.iter().skip(skip).take(rows) {
            draw_text(row, 16.0, y, size, *color);
            y += size * 1.15;
        }
        if self.console.scroll > 0 {
            draw_text(
                "scrolled up: PageDown or End for the newest lines",
                w - 470.0,
                h - 44.0,
                16.0,
                GRAY,
            );
        }
        let (prompt, color) = match self.console.console.session.mode {
            Mode::Legal => ("[legal]>", Color::new(0.5, 0.9, 0.5, 1.0)),
            Mode::ReadOnly => ("[read-only]>", Color::new(0.6, 0.75, 1.0, 1.0)),
        };
        let blink = if (get_time() * 2.0) as i64 % 2 == 0 {
            "_"
        } else {
            " "
        };
        draw_text(
            format!("{prompt} {}{blink}", self.console.input),
            16.0,
            h - 22.0,
            size,
            color,
        );
        draw_text(
            "` / Esc: back   PageUp / PageDown / wheel: scroll",
            w - 440.0,
            h - 22.0,
            16.0,
            GRAY,
        );
    }
}

/// `line` cut into rows that fit in `width` pixels at `size`, at spaces where it can.
fn wrap_line(line: &str, width: f32, size: f32) -> Vec<String> {
    let fits = |text: &str| measure_text(text, None, size as u16, 1.0).width <= width;
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in line.split_inclusive(' ') {
        if !row.is_empty() && !fits(&format!("{row}{word}")) {
            rows.push(std::mem::take(&mut row));
        }
        row.push_str(word);
        // A word longer than the row is cut where it overflows.
        while !fits(&row) && row.chars().count() > 1 {
            let cut = (1..row.chars().count())
                .rev()
                .find(|&n| fits(&row.chars().take(n).collect::<String>()))
                .unwrap_or(1);
            rows.push(row.chars().take(cut).collect());
            row = row.chars().skip(cut).collect();
        }
    }
    rows.push(row);
    rows
}
