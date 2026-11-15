//! The setup in windows: the system's own dialogs for the questions (`rfd`: a folder picker and
//! message boxes, with Cancel on the left and the confirming button on the right as the system
//! lays them out) and a small window for the work (`macroquad`).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

use f4x_extract::paths::Roots;
use f4x_lang::text::Args;
use f4x_setup::flow::{self, Stopped, Ui};
use f4x_setup::say::Say;
use macroquad::prelude::*;
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

struct Dialogs<'a> {
    say: &'a Say,
}

impl Dialogs<'_> {
    /// A message box with a confirming and a cancelling button; `true` when confirmed.
    fn ask(&self, level: MessageLevel, text: &str, confirm_key: &str) -> bool {
        let confirm = self.say.plain(confirm_key);
        let result = MessageDialog::new()
            .set_level(level)
            .set_title(self.say.plain("setup.title"))
            .set_description(text)
            .set_buttons(MessageButtons::OkCancelCustom(
                confirm.clone(),
                self.say.plain("setup.cancel"),
            ))
            .show();
        matches!(result, MessageDialogResult::Ok)
            || matches!(&result, MessageDialogResult::Custom(label) if *label == confirm)
    }
}

impl Ui for Dialogs<'_> {
    fn welcome(&mut self, folder: &Path) -> bool {
        let text = self.say.get(
            "setup.welcome",
            &Args::new().text("folder", folder.display().to_string()),
        );
        self.ask(MessageLevel::Info, &text, "setup.start")
    }

    fn choose_folder(&mut self, start: Option<&Path>) -> Option<PathBuf> {
        let mut dialog = FileDialog::new().set_title(self.say.plain("setup.choose"));
        if let Some(dir) = start.and_then(Path::parent) {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_folder()
    }

    fn try_again(&mut self, problem: &str) -> bool {
        let text = self
            .say
            .get("setup.try_again", &Args::new().text("problem", problem));
        self.ask(MessageLevel::Error, &text, "setup.choose_other")
    }
}

/// What the work window shows.
enum Progress {
    Working(String),
    Finished(Result<String, String>),
}

/// Choose the folder in dialogs, then run the work behind a progress window.
pub fn run(say: Say, roots: Roots) -> Result<(), String> {
    let folder = {
        let mut ui = Dialogs { say: &say };
        match flow::choose_original(&mut ui, &say, &roots.data, None) {
            Ok(folder) => folder,
            Err(Stopped::Cancelled | Stopped::Declined(_)) => return Ok(()),
        }
    };
    let (sender, receiver) = mpsc::channel();
    let work_say = Say::load(&roots.game)?;
    let (data, game) = (roots.data.clone(), roots.game.clone());
    thread::spawn(move || {
        let result = flow::run(&work_say, &folder, &data, &game, &mut |sentence| {
            let _ = sender.send(Progress::Working(sentence.to_string()));
        });
        let _ = sender.send(Progress::Finished(result));
    });
    let outcome = Arc::new(Mutex::new(Ok(())));
    let conf = Conf {
        window_title: say.plain("setup.title"),
        window_width: 720,
        window_height: 260,
        window_resizable: false,
        ..Default::default()
    };
    macroquad::Window::from_config(conf, window(say, receiver, Arc::clone(&outcome)));
    let result = outcome.lock().map_err(|e| e.to_string())?.clone();
    result
}

/// Draw `text` wrapped to `width` characters, from `y`; returns the next free line's y.
fn paragraph(text: &str, x: f32, mut y: f32, width: usize, size: f32, colour: Color) -> f32 {
    for line in text.lines() {
        let mut current = String::new();
        for word in line.split_whitespace() {
            if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
                draw_text(&current, x, y, size, colour);
                y += size * 1.25;
                current.clear();
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        draw_text(&current, x, y, size, colour);
        y += size * 1.25;
    }
    y
}

async fn window(say: Say, receiver: Receiver<Progress>, outcome: Arc<Mutex<Result<(), String>>>) {
    // The window stays until the work is over: closing it would leave half the pictures made.
    prevent_quit();
    let mut sentence = String::new();
    let mut finished: Option<Result<String, String>> = None;
    let button = Rect::new(560.0, 200.0, 130.0, 36.0);
    loop {
        loop {
            match receiver.try_recv() {
                Ok(Progress::Working(text)) => sentence = text,
                Ok(Progress::Finished(result)) => finished = Some(result),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        clear_background(Color::from_rgba(24, 26, 38, 255));
        let ink = Color::from_rgba(235, 228, 205, 255);
        let gold = Color::from_rgba(240, 196, 80, 255);
        draw_text(say.plain("setup.title"), 30.0, 50.0, 34.0, gold);
        match &finished {
            None => {
                paragraph(&sentence, 30.0, 100.0, 70, 24.0, ink);
                paragraph(&say.plain("setup.working"), 30.0, 170.0, 70, 20.0, gold);
                // A bar that moves, since the stages have no count.
                let t = get_time() as f32;
                let x = 30.0 + ((t * 0.8).sin() * 0.5 + 0.5) * 540.0;
                draw_rectangle(30.0, 215.0, 660.0, 8.0, Color::from_rgba(50, 54, 76, 255));
                draw_rectangle(x, 215.0, 120.0, 8.0, gold);
            }
            Some(result) => {
                let (text, colour) = match result {
                    Ok(done) => (done.as_str(), ink),
                    Err(problem) => (problem.as_str(), Color::from_rgba(240, 120, 110, 255)),
                };
                paragraph(text, 30.0, 100.0, 70, 22.0, colour);
                draw_rectangle(
                    button.x,
                    button.y,
                    button.w,
                    button.h,
                    Color::from_rgba(70, 60, 110, 255),
                );
                let label = say.plain("setup.close");
                draw_text(&label, button.x + 34.0, button.y + 25.0, 24.0, ink);
                let (mx, my) = mouse_position();
                let clicked =
                    is_mouse_button_pressed(MouseButton::Left) && button.contains(vec2(mx, my));
                if clicked
                    || is_key_pressed(KeyCode::Enter)
                    || is_key_pressed(KeyCode::Escape)
                    || is_quit_requested()
                {
                    if let Ok(mut slot) = outcome.lock() {
                        *slot = result.as_ref().map(|_| ()).map_err(String::clone);
                    }
                    break;
                }
            }
        }
        next_frame().await;
    }
}
