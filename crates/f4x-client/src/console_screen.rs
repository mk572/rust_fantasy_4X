//! The text console in the client (backquote key): the same `f4x_console::Console` every other front end
//! uses, with an input line, scrollback and a command history.

use crate::input::Input;
use crate::page::Page;
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
            self.open_page(Screen::Console, ConsolePage);
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

pub(crate) struct ConsolePage;

impl Page for ConsolePage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        for c in input.typed() {
            if !c.is_control() && c != '`' {
                app.console.input.push(c);
            }
        }
        if input.key_pressed(KeyCode::Backspace) {
            app.console.input.pop();
        }
        if input.key_pressed(KeyCode::Enter) {
            app.run_console_line();
        }
        if input.key_pressed(KeyCode::Escape) {
            app.screen = app.console.back;
        }
        let history = app.console.console.history.len();
        if input.key_pressed(KeyCode::Up) && history > 0 {
            let i = app
                .console
                .recall
                .map_or(history - 1, |i| i.saturating_sub(1));
            app.console.recall = Some(i);
            app.console.input = app.console.console.history[i].0.clone();
        }
        if input.key_pressed(KeyCode::Down) {
            match app.console.recall {
                Some(i) if i + 1 < history => {
                    app.console.recall = Some(i + 1);
                    app.console.input = app.console.console.history[i + 1].0.clone();
                }
                _ => {
                    app.console.recall = None;
                    app.console.input.clear();
                }
            }
        }
        if input.key_pressed(KeyCode::Tab) {
            let prefix = app.console.input.clone();
            if !prefix.contains(' ') {
                let hits = app.console.console.complete(&prefix);
                match hits.as_slice() {
                    [one] => app.console.input = format!("{one} "),
                    [] => {}
                    many => {
                        app.console.scroll = 0;
                        app.console.lines.push(many.join("  "));
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
        for line in &app.console.lines {
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
        let wheel = input.mouse_wheel().1;
        if wheel != 0.0 {
            let steps = (wheel.abs().ceil() as usize).max(1) * 3;
            app.console.scroll = if wheel > 0.0 {
                app.console.scroll.saturating_add(steps)
            } else {
                app.console.scroll.saturating_sub(steps)
            };
        }
        if input.key_pressed(KeyCode::PageUp) {
            app.console.scroll = app.console.scroll.saturating_add(rows.saturating_sub(1));
        }
        if input.key_pressed(KeyCode::PageDown) {
            app.console.scroll = app.console.scroll.saturating_sub(rows.saturating_sub(1));
        }
        if input.key_pressed(KeyCode::Home) {
            app.console.scroll = furthest;
        }
        if input.key_pressed(KeyCode::End) {
            app.console.scroll = 0;
        }
        app.console.scroll = app.console.scroll.min(furthest);
        let skip = furthest - app.console.scroll;
        let mut y = 28.0;
        for (row, color) in shown.iter().skip(skip).take(rows) {
            draw_text(row, 16.0, y, size, *color);
            y += size * 1.15;
        }
        if app.console.scroll > 0 {
            draw_text(
                "scrolled up: PageDown or End for the newest lines",
                w - 470.0,
                h - 44.0,
                16.0,
                GRAY,
            );
        }
        let (prompt, color) = match app.console.console.session.mode {
            Mode::Legal => ("[legal]>", Color::new(0.5, 0.9, 0.5, 1.0)),
            Mode::ReadOnly => ("[read-only]>", Color::new(0.6, 0.75, 1.0, 1.0)),
        };
        let blink = if (get_time() * 2.0) as i64 % 2 == 0 {
            "_"
        } else {
            " "
        };
        draw_text(
            format!("{prompt} {}{blink}", app.console.input),
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
