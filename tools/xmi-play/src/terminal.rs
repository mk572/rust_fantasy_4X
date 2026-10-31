//! Raw keyboard input without a dependency: `stty` switches the terminal, reads time out after 0.1 s.
//!
//! Tests: none, because it switches the real terminal with `stty`.

use std::io::Read;
use std::process::Command;

#[derive(PartialEq)]
pub enum Key {
    Up,
    Down,
    PageUp,
    PageDown,
    Enter,
    Esc,
    /// `q` or Ctrl-C.
    Quit,
    Other,
    /// Nothing pressed within the time-out.
    None,
}

fn stty(args: &[&str]) -> Option<String> {
    let out = Command::new("stty")
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Raw mode while alive; the saved settings come back on drop.
pub struct Raw(String);

impl Raw {
    pub fn enter() -> Result<Raw, String> {
        let saved = stty(&["-g"]).ok_or("the menu needs a terminal (stty failed)")?;
        stty(&["-icanon", "-echo", "-isig", "min", "0", "time", "1"]).ok_or("stty failed")?;
        print!("\x1b[?25l"); // hide the cursor
        Ok(Raw(saved))
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        print!("\x1b[?25h\x1b[0m\r\n");
        stty(&[&self.0]);
    }
}

pub fn key() -> Key {
    let mut buf = [0u8; 8];
    let n = std::io::stdin().read(&mut buf).unwrap_or(0);
    match &buf[..n] {
        [] => Key::None,
        [0x1b] => Key::Esc,
        [0x1b, b'[', b'A', ..] => Key::Up,
        [0x1b, b'[', b'B', ..] => Key::Down,
        [0x1b, b'[', b'5', b'~', ..] => Key::PageUp,
        [0x1b, b'[', b'6', b'~', ..] => Key::PageDown,
        [b'\r' | b'\n', ..] => Key::Enter,
        [b'q' | 3, ..] => Key::Quit,
        _ => Key::Other,
    }
}

/// Terminal height in lines.
pub fn rows() -> usize {
    stty(&["size"])
        .and_then(|s| s.split_whitespace().next().and_then(|r| r.parse().ok()))
        .unwrap_or(24)
}
