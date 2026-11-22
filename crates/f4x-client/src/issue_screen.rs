//! The screen that lists what is wrong with the game's files or with a saved game (task 768,
//! `docs/content-policy.md`). It is drawn with the window's plain font and nothing from the game's
//! art, because it has to work when the content (or the pictures) cannot be loaded.
//!
//! The text of an issue is the message of its kind (`content_issue.<kind>`) with the details as
//! arguments; `f4x-def` never writes the sentence.
//!
//! Tests: below, for the text of every kind.

use f4x_def::load::{ContentIssue, IssueKind};
use f4x_lang::text::{Args, Texts};
use macroquad::prelude::*;

/// One issue as the screen shows it: where, and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub place: String,
    pub sentence: String,
}

/// What the screen shows.
#[derive(Debug, Clone)]
pub struct View {
    pub title: String,
    pub intro: String,
    pub rows: Vec<Row>,
    /// The buttons from left to right: the last is the one that confirms (it is on the right), the
    /// first of two cancels.
    pub buttons: Vec<String>,
}

/// The sentence of an issue: the message of its kind when the details are the ones that message
/// takes, else the details as they are.
pub fn sentence(issue: &ContentIssue, texts: &Texts) -> String {
    let d = |i: usize| issue.detail.get(i).cloned().unwrap_or_default();
    let key = match (issue.kind, issue.detail.len()) {
        // A bare issue has nothing to say beyond its kind: the kind's name is the detail.
        (_, 0) => {
            return texts.get(
                "content_issue.plain",
                &Args::new().text("d1", format!("{:?}", issue.kind)),
            )
        }
        (IssueKind::OutOfRange | IssueKind::Dangling, 3) | (IssueKind::Duplicate, 2) => {
            issue.kind.text_key()
        }
        (IssueKind::OutOfRange | IssueKind::Dangling | IssueKind::Duplicate, _) => {
            "content_issue.plain"
        }
        (_, 1) => issue.kind.text_key(),
        _ => "content_issue.plain",
    };
    let args = Args::new()
        .text("d1", d(0))
        .text("d2", d(1))
        .text("d3", d(2));
    texts.get(key, &args)
}

/// The rows for `issues`; `layer` names the folder a file came from (`content_issue.layer_*`).
pub fn rows(issues: &[ContentIssue], texts: &Texts, layer: impl Fn(&str) -> String) -> Vec<Row> {
    issues
        .iter()
        .map(|i| {
            let args = Args::new()
                .text("layer", layer(&i.file))
                .text("file", i.file.clone())
                .text("at", i.at.clone());
            let key = if i.at.is_empty() {
                "content_issue.where_file"
            } else {
                "content_issue.where"
            };
            Row {
                place: texts.get(key, &args),
                sentence: sentence(i, texts),
            }
        })
        .collect()
}

/// Break `text` into lines no wider than `width` pixels at `size`.
fn wrap(text: &str, size: u16, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && measure_text(&candidate, None, size, 1.0).width > width {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

/// Show the screen until a button is chosen; the answer is the button's place in `view.buttons`.
/// Enter takes the last button, Escape the first of two (the last when there is one).
pub async fn show(view: &View) -> usize {
    let mut scroll = 0.0_f32;
    loop {
        if let Some(i) = frame(view, &mut scroll) {
            return i;
        }
        next_frame().await;
    }
}

/// Draw the screen once and read the keys and the mouse: the chosen button, if one was.
pub fn frame(view: &View, scroll: &mut f32) -> Option<usize> {
    clear_background(Color::new(0.08, 0.07, 0.1, 1.0));
    let margin = 24.0;
    let width = screen_width() - 2.0 * margin;
    let button_h = 40.0;
    let bottom = screen_height() - margin - button_h - 16.0;
    let mut y = margin + 24.0;
    draw_text(
        &view.title,
        margin,
        y,
        34.0,
        Color::new(0.95, 0.8, 0.4, 1.0),
    );
    y += 18.0;
    for line in wrap(&view.intro, 20, width) {
        y += 24.0;
        draw_text(&line, margin, y, 20.0, WHITE);
    }
    y += 20.0;
    // The list scrolls with the wheel, the arrows and the page keys.
    let mut lines: Vec<(String, Color)> = Vec::new();
    for row in &view.rows {
        if !row.place.is_empty() {
            for l in wrap(&row.place, 18, width) {
                lines.push((l, Color::new(0.6, 0.75, 0.95, 1.0)));
            }
        }
        for l in wrap(&row.sentence, 20, width - 20.0) {
            lines.push((format!("    {l}"), WHITE));
        }
        lines.push((String::new(), WHITE));
    }
    let visible = ((bottom - y) / 22.0).floor().max(1.0);
    let most = (lines.len() as f32 - visible).max(0.0);
    *scroll -= mouse_wheel().1 * 2.0;
    if is_key_pressed(KeyCode::Down) {
        *scroll += 1.0;
    }
    if is_key_pressed(KeyCode::Up) {
        *scroll -= 1.0;
    }
    if is_key_pressed(KeyCode::PageDown) {
        *scroll += visible - 1.0;
    }
    if is_key_pressed(KeyCode::PageUp) {
        *scroll -= visible - 1.0;
    }
    *scroll = scroll.clamp(0.0, most);
    for (i, (line, colour)) in lines.iter().enumerate().skip(*scroll as usize) {
        let ly = y + (i as f32 - scroll.floor()) * 22.0;
        if ly > bottom {
            break;
        }
        draw_text(line, margin, ly, 20.0, *colour);
    }
    // The buttons, the confirming one on the right.
    let (bw, gap) = (210.0, 14.0);
    let mut chosen = None;
    let n = view.buttons.len();
    for (i, label) in view.buttons.iter().enumerate() {
        let x = screen_width() - margin - (n - i) as f32 * bw - (n - 1 - i) as f32 * gap;
        let rect = Rect::new(x, screen_height() - margin - button_h, bw, button_h);
        let (mx, my) = mouse_position();
        let hot = rect.contains(vec2(mx, my));
        draw_rectangle(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            if hot {
                Color::new(0.35, 0.3, 0.2, 1.0)
            } else {
                Color::new(0.22, 0.2, 0.18, 1.0)
            },
        );
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, GRAY);
        let size = measure_text(label, None, 22, 1.0);
        draw_text(
            label,
            rect.x + (rect.w - size.width) / 2.0,
            rect.y + 27.0,
            22.0,
            WHITE,
        );
        if hot && is_mouse_button_pressed(MouseButton::Left) {
            chosen = Some(i);
        }
    }
    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
        chosen = Some(n.saturating_sub(1));
    }
    if is_key_pressed(KeyCode::Escape) {
        chosen = Some(0);
    }
    chosen
}

/// What happens when the player answers a [`Dialog`] with its last button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Then {
    /// Nothing more: the list was only to be read.
    Nothing,
    /// Load the save again without the check of the content files.
    LoadAnyway(std::path::PathBuf),
}

/// A list of problems or notes about a load that waits for the player's answer.
#[derive(Debug, Clone)]
pub struct Dialog {
    pub view: View,
    pub then: Then,
    pub scroll: f32,
}

/// The folder a row of a load names: the save, or the game's files.
fn load_layer(file: &str, texts: &Texts) -> String {
    let key = if file == "save" {
        "content_issue.layer_save"
    } else {
        "content_issue.layer_shipped"
    };
    texts.get(key, &Args::new())
}

impl Dialog {
    /// A save that was refused. When the only trouble is content files that are not the ones the
    /// game was made with, the player may load it anyway.
    pub fn refused_load(issues: &[ContentIssue], texts: &Texts, path: &std::path::Path) -> Dialog {
        let only_changed_files =
            !issues.is_empty() && issues.iter().all(|i| i.file == "save" && i.at == "content");
        let get = |key: &str| texts.get(key, &Args::new());
        let (intro, buttons, then) = if only_changed_files {
            (
                "content_issue.intro_load_changed",
                vec![
                    get("content_issue.cancel"),
                    get("content_issue.load_anyway"),
                ],
                Then::LoadAnyway(path.to_path_buf()),
            )
        } else {
            (
                "content_issue.intro_load_refused",
                vec![get("content_issue.back")],
                Then::Nothing,
            )
        };
        Dialog {
            view: View {
                title: get("content_issue.title_load_refused"),
                intro: get(intro),
                rows: rows(issues, texts, |f| load_layer(f, texts)),
                buttons,
            },
            then,
            scroll: 0.0,
        }
    }

    /// What the player should know about a game that loaded.
    pub fn notes(issues: &[ContentIssue], texts: &Texts) -> Dialog {
        let get = |key: &str| texts.get(key, &Args::new());
        Dialog {
            view: View {
                title: get("content_issue.title_load_notes"),
                intro: get("content_issue.intro_load_notes"),
                rows: rows(issues, texts, |f| load_layer(f, texts)),
                buttons: vec![get("content_issue.continue")],
            },
            then: Then::Nothing,
            scroll: 0.0,
        }
    }

    /// One frame of the dialog: the answer (the place of the button), when there is one.
    pub fn frame(&mut self) -> Option<usize> {
        frame(&self.view, &mut self.scroll)
    }
}

/// The name of the folder a content file is read from: the mod's when it has the file, else the
/// shipped one (`content_issue.layer_*`).
pub fn layer_name(file: &str, mod_dir: Option<&std::path::Path>, texts: &Texts) -> String {
    let key = if mod_dir.is_some_and(|d| d.join(file).exists()) {
        "content_issue.layer_mod"
    } else {
        "content_issue.layer_shipped"
    };
    texts.get(key, &Args::new())
}

/// A row for a failure that is not an issue of a content file (a file of the client, the pictures):
/// the words as they are.
pub fn plain_row(words: String) -> Row {
    Row {
        place: String::new(),
        sentence: words,
    }
}

/// Show the problems and stop: the game cannot start. `automated` runs (a screenshot, a sound test)
/// print them and stop without waiting for a click.
pub async fn stop(texts: &Texts, automated: bool, title: &str, intro: &str, rows: Vec<Row>) -> ! {
    let view = View {
        title: texts.get(title, &Args::new()),
        intro: texts.get(intro, &Args::new()),
        rows,
        buttons: vec![texts.get("content_issue.quit", &Args::new())],
    };
    for row in &view.rows {
        eprintln!("{}: {}", row.place, row.sentence);
    }
    if !automated {
        show(&view).await;
    }
    std::process::exit(1);
}

/// Show what the game goes on without, and wait for Continue.
pub async fn tell(texts: &Texts, automated: bool, title: &str, intro: &str, rows: Vec<Row>) {
    let view = View {
        title: texts.get(title, &Args::new()),
        intro: texts.get(intro, &Args::new()),
        rows,
        buttons: vec![texts.get("content_issue.continue", &Args::new())],
    };
    for row in &view.rows {
        eprintln!("{}: {}", row.place, row.sentence);
    }
    if !automated {
        show(&view).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::load::ContentIssue;

    fn texts() -> Texts {
        f4x_lang::text::load_texts(
            &[std::path::Path::new(&format!(
                "{}/../{}",
                env!("CARGO_MANIFEST_DIR"),
                "f4x-lang/content"
            ))],
            "en",
        )
        .expect("the messages load")
    }

    #[test]
    fn every_kind_of_issue_is_told_in_words_with_its_values() {
        let t = texts();
        let issue = |kind, d: &[&str]| {
            ContentIssue::new("units.ron", "unit 3", kind).with(d.iter().copied())
        };
        for (kind, d, want) in [
            (IssueKind::Missing, &["cost"][..], "Missing: cost"),
            (
                IssueKind::Unreadable,
                &["no such file"],
                "Cannot be read: no such file",
            ),
            (IssueKind::Corrupt, &["line 3"], "Not valid: line 3"),
            (
                IssueKind::WrongType,
                &["a number"],
                "Wrong kind of value: a number",
            ),
            (
                IssueKind::OutOfRange,
                &["cost", "0", "above 0"],
                "cost is 0; it must be above 0",
            ),
            (
                IssueKind::Dangling,
                &["race", "race", "9"],
                "race names a race that does not exist: 9",
            ),
            (
                IssueKind::Duplicate,
                &["the number", "5"],
                "the number 5 is used twice",
            ),
            (
                IssueKind::UnknownKey,
                &["colour"],
                "A key that nothing uses: colour",
            ),
            (IssueKind::Conflict, &["a contradiction"], "a contradiction"),
        ] {
            assert_eq!(sentence(&issue(kind, d), &t), want, "{kind:?}");
        }
    }

    #[test]
    fn details_that_do_not_fit_the_message_are_shown_as_they_are() {
        let t = texts();
        // A syntax error is an OutOfRange or Duplicate kind with one detail: the ron message.
        let one = ContentIssue::new("a.ron", "", IssueKind::OutOfRange).with(["too big"]);
        assert_eq!(sentence(&one, &t), "too big");
        let bare = ContentIssue::new("a.ron", "", IssueKind::Corrupt);
        assert!(!sentence(&bare, &t).is_empty());
    }

    #[test]
    fn the_place_names_the_layer_the_file_and_where_in_it() {
        let t = texts();
        let a = ContentIssue::new("units.ron", "unit 3", IssueKind::Missing).with(["cost"]);
        let b = ContentIssue::new("battle.ron", "", IssueKind::Missing).with(["the file"]);
        let r = rows(&[a, b], &t, |f| {
            if f == "units.ron" {
                "mod".into()
            } else {
                "game files".into()
            }
        });
        assert_eq!(r[0].place, "mod, units.ron, unit 3");
        assert_eq!(r[1].place, "game files, battle.ron");
    }
}
