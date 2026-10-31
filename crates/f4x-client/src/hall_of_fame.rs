//! The Hall of Fame, the original's `Hall_Of_Fame_Screen` (`SCORE.c`): the ten best
//! finished games with the wizard's name, race and score, kept between games in
//! `saves/hall_of_fame.txt` (the original keeps them in `MAGIC.SET`). Shown from the start menu
//! and after the score screen of a game that has ended. A click or Esc leaves.

use std::path::Path;

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use f4x_core::text::Args;
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::view::{App, Screen};

/// Where the table is kept.
pub(crate) const FILE: &str = "saves/hall_of_fame.txt";
/// Entries in the table (`NUM_SCORES`).
pub(crate) const SCORES: usize = 10;
/// The score the original's percentage is taken of.
const BEST_POSSIBLE: i64 = 8000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub name: String,
    /// The race of the wizard's capital.
    pub race: String,
    pub score: i32,
}

/// The table in the file: one `score<TAB>name<TAB>race` line each, best first.
pub(crate) fn load(path: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut table: Vec<Entry> = text
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let score = parts.next()?.trim().parse().ok()?;
            let name = parts.next()?.to_string();
            let race = parts.next().unwrap_or_default().to_string();
            Some(Entry { name, race, score })
        })
        .collect();
    table.sort_by_key(|e| std::cmp::Reverse(e.score));
    table.truncate(SCORES);
    table
}

pub(crate) fn save(path: &Path, table: &[Entry]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = table
        .iter()
        .map(|e| format!("{}\t{}\t{}\n", e.score, clean(&e.name), clean(&e.race)))
        .collect();
    std::fs::write(path, text)
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n'], " ")
}

/// Put `entry` in the table if it beats a score in it (a tie goes below the old score, as in
/// the original); returns its place.
pub(crate) fn insert(table: &mut Vec<Entry>, entry: Entry) -> Option<usize> {
    let at = table
        .iter()
        .position(|e| e.score < entry.score)
        .or((table.len() < SCORES).then_some(table.len()))?;
    table.insert(at, entry);
    table.truncate(SCORES);
    Some(at)
}

/// "Barbarian" as "Barbarians": the table says "of the Barbarians". A race the rules do not know
/// (a table kept from another set of rules) stays as it is written.
pub(crate) fn plural(
    races: &f4x_core::rules::Registry<f4x_core::rules::RaceDef>,
    race: &str,
) -> String {
    races.defs().iter().find(|r| r.name == race).map_or_else(
        || race.to_string(),
        |r| {
            if r.plural.is_empty() {
                r.name.clone()
            } else {
                r.plural.clone()
            }
        },
    )
}

/// The share of the best possible score, in percent.
pub(crate) fn percent(score: i32) -> i64 {
    i64::from(score) * 100 / BEST_POSSIBLE
}

impl App {
    /// Show the table; `from_menu` returns to the start menu, otherwise the game has ended and
    /// the start menu follows too (the original goes back to it).
    pub(crate) fn open_hall_of_fame(&mut self) {
        let table = load(Path::new(FILE));
        self.open_page(Screen::HallOfFame, HallPage { table });
    }
}

/// The table as it was when the screen opened.
struct HallPage {
    table: Vec<Entry>,
}

impl Page for HallPage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if app.draw_pic(&lay, "halofam", 0, 0, 0.0, 0.0).is_none() {
            let r = lay.rect(crate::resolution::SCREEN);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.08, 0.06, 1.0));
        }
        let gold = Color::new(0.95, 0.85, 0.5, 1.0);
        let ink = Color::new(0.95, 0.8, 0.45, 1.0);
        let title = app.text("hall.title");
        app.draw_styled(view, &Style::outlined(5).center(), &title, 160.0, 7.0);
        let (x, mut y) = (72.0, 42.0);
        for (i, e) in self.table.iter().enumerate() {
            let rank = if i == 0 {
                app.text("hall.master")
            } else {
                app.text_with("hall.rank", &Args::new().count("n", i + 1))
            };
            app.draw_styled(view, &Style::nearest(2, gold).right(), &rank, x, y);
            let who = app.text_with(
                "hall.of_the",
                &Args::new()
                    .text("name", &e.name)
                    .text("race", plural(&app.game.content().rules.races, &e.race)),
            );
            app.draw_styled(view, &Style::nearest(2, ink), &who, x + 9.0, y);
            app.draw_styled(
                view,
                &Style::nearest(2, ink).right(),
                &e.score.to_string(),
                x + 170.0,
                y,
            );
            let share = format!("({}%)", percent(e.score));
            app.draw_styled(view, &Style::nearest(2, ink).right(), &share, x + 198.0, y);
            y += 13.0;
        }
        if input.key_pressed(KeyCode::Escape)
            || input.key_pressed(KeyCode::Enter)
            || input.mouse_pressed(MouseButton::Left)
        {
            app.open_main_menu();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, score: i32) -> Entry {
        Entry {
            name: name.into(),
            race: "Gnoll".into(),
            score,
        }
    }

    #[test]
    fn a_new_score_goes_above_the_lower_ones_and_a_tie_below() {
        let mut t = vec![e("a", 900), e("b", 500)];
        assert_eq!(insert(&mut t, e("c", 700)), Some(1));
        assert_eq!(insert(&mut t, e("d", 700)), Some(2));
        let names: Vec<_> = t.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["a", "c", "d", "b"]);
    }

    #[test]
    fn the_table_keeps_ten_and_a_low_score_misses_a_full_one() {
        let mut t: Vec<Entry> = (1..=10).map(|i| e("x", i * 100)).collect();
        t.sort_by_key(|x| std::cmp::Reverse(x.score));
        assert_eq!(insert(&mut t, e("low", 50)), None);
        assert_eq!(insert(&mut t, e("top", 5000)), Some(0));
        assert_eq!(t.len(), SCORES);
        assert_eq!(t.last().map(|x| x.score), Some(200));
    }

    #[test]
    fn the_file_round_trips_and_a_damaged_line_is_skipped() -> f4x_core::testdir::TestResult {
        let dir = f4x_core::testdir::TestDir::new("hof");
        let path = dir.join("hall.txt");
        let table = vec![e("Merlin", 1200), e("Raven", 800)];
        save(&path, &table)?;
        let mut text = std::fs::read_to_string(&path)?;
        text.push_str("garbage\n");
        std::fs::write(&path, text)?;
        assert_eq!(load(&path), table);
        Ok(())
    }

    #[test]
    fn races_are_made_plural_for_the_table() {
        let races = &crate::test_app::shipped::content().rules.races;
        assert_eq!(plural(races, "Barbarian"), "Barbarians");
        assert_eq!(plural(races, "High Elf"), "High Elves");
        assert_eq!(plural(races, "Lizardman"), "Lizardmen");
        assert_eq!(plural(races, "High Men"), "High Men");
        assert_eq!(plural(races, "Unknown"), "Unknown");
    }

    #[test]
    fn the_percentage_is_of_eight_thousand() {
        assert_eq!(percent(8000), 100);
        assert_eq!(percent(1200), 15);
    }
}
