//! The start menu and the save slots (tasks 86 and 145; `docs/screen_list.md`).
//!
//! Both follow the original: the menu is `Main_Menu_Screen` (Continue, Load Game, New Game, Hall
//! of Fame, Quit; Continue loads the auto-save, Load Game opens the slots) and the slots are
//! `Load_Screen` (eight slots, with Quit, Load, Save, Settings and OK). Both are drawn on the
//! original's 320 by 200 layout with its pictures, or with plain boxes when the art is missing.
//! Unlike the original, a slot's name is not typed: it shows the wizard, the turn and the date.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::font::Style;
use crate::input::Input;
use crate::page::Page;
use crate::ui;
use f4x_core::game::save::LoadError;
use f4x_core::text::{Args, Texts};
use f4x_data::save::{summarize_save, SaveError, SaveSummary};
use macroquad::prelude::*;

use crate::frame::Layout;
use crate::ui::GOLD;
use crate::view::{App, Screen, AUTOSAVE};

/// Save slots on the Load / Save screen.
pub const SLOTS: usize = 8;

/// Where slot `n` (from 0) is kept.
pub fn slot_path(n: usize) -> PathBuf {
    PathBuf::from(format!("saves/slot{}.ron", n + 1))
}

/// What one slot holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    Empty,
    Filled {
        summary: SaveSummary,
        /// Seconds since 1970 when the file was last written.
        saved_at: Option<u64>,
    },
    /// The file is there but cannot be read.
    Damaged,
    /// Written by another version of the game (or another save layout): `version` is the game
    /// version the file names, or empty when it names none.
    Incompatible {
        version: String,
    },
}

/// The game version a save file names (`game_version: "0.0.1"`), if it does.
fn game_version_of(path: &Path) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            let rest = text.split("game_version:").nth(1)?;
            let start = rest.find('"')? + 1;
            let len = rest[start..].find('"')?;
            Some(rest[start..start + len].to_string())
        })
        .unwrap_or_default()
}

impl Slot {
    fn read(path: &Path) -> Slot {
        if !path.exists() {
            return Slot::Empty;
        }
        match summarize_save(path) {
            Ok(summary) => Slot::Filled {
                summary,
                saved_at: std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
            },
            Err(SaveError::Load(LoadError::Version { .. } | LoadError::GameVersion { .. })) => {
                Slot::Incompatible {
                    version: game_version_of(path),
                }
            }
            Err(_) => Slot::Damaged,
        }
    }

    /// The row's text.
    pub fn label(&self, texts: &Texts, n: usize) -> String {
        let args = Args::new().count("slot", n + 1);
        match self {
            Slot::Empty => texts.get("menu.slot_empty", &args),
            Slot::Damaged => texts.get("menu.slot_damaged", &args),
            Slot::Incompatible { version } if version.is_empty() => {
                texts.get("menu.slot_incompatible_unknown", &args)
            }
            Slot::Incompatible { version } => {
                texts.get("menu.slot_incompatible", &args.text("version", version))
            }
            Slot::Filled { summary, saved_at } => {
                let when = saved_at.map(format_time).unwrap_or_default();
                let end = if summary.finished {
                    texts.get("menu.slot_ended", &Args::new())
                } else {
                    String::new()
                };
                let args = args
                    .text("wizard", &summary.wizard)
                    .count("turn", summary.turn)
                    .text("end", end)
                    .text("when", when);
                texts.get("menu.slot_filled", &args)
            }
        }
    }

    fn loadable(&self) -> bool {
        matches!(self, Slot::Filled { .. })
    }
}

/// `YYYY-MM-DD HH:MM` (UTC) for seconds since 1970.
pub fn format_time(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// State of the menu and the slots.
#[derive(Default)]
pub struct MenuState {
    /// An auto-save exists, so Continue is offered.
    pub can_continue: bool,
    pub slots: Vec<Slot>,
    pub selected: Option<usize>,
    /// The slots were opened from the start menu: nothing to save, and OK goes back to it.
    pub from_menu: bool,
    /// A Save onto a filled slot waits for a second click.
    pub confirm_overwrite: bool,
    /// What went wrong, shown at the bottom.
    pub message: String,
}

/// A row of the start menu.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    Continue,
    Load,
    New,
    Fame,
    Quit,
}

impl App {
    pub fn open_main_menu(&mut self) {
        self.menu.can_continue = Path::new(AUTOSAVE).exists();
        self.menu.slots = (0..SLOTS).map(|n| Slot::read(&slot_path(n))).collect();
        self.menu.message.clear();
        self.open_page(Screen::MainMenu, MainMenuPage);
    }

    /// Open the save slots; `from_menu` when reached from the start menu.
    pub fn open_slots(&mut self, from_menu: bool) {
        self.menu.slots = (0..SLOTS).map(|n| Slot::read(&slot_path(n))).collect();
        self.menu.selected = None;
        self.menu.from_menu = from_menu;
        self.menu.confirm_overwrite = false;
        self.menu.message.clear();
        self.open_page(Screen::Slots, SlotsPage);
    }

    /// A button on the 320 by 200 layout: the original's picture (a different frame while the
    /// mouse is over it) or a plain box. True when clicked.
    #[allow(clippy::too_many_arguments)]
    fn menu_button(
        &mut self,
        lay: &Layout,
        rect: (f32, f32, f32, f32),
        art: Option<(&str, usize, f32, f32)>,
        label: &str,
        enabled: bool,
        hover_frame: bool,
        hover_shift: bool,
        input: &Input,
    ) -> bool {
        let r = lay.rect(rect);
        let state = ui::hit(r, enabled, input);
        let hover = state.hovered();
        let tint = if enabled {
            WHITE
        } else {
            Color::new(1.0, 1.0, 1.0, 0.4)
        };
        let (shift, frame) = (
            if hover_shift && state.held() {
                1.0
            } else {
                0.0
            },
            usize::from(!(hover_frame && hover)),
        );
        let drawn = art.and_then(|(lbx, entry, x, y)| {
            let frames_ok = self.art.picture(lbx, entry, frame).is_some();
            let frame = if frames_ok { frame } else { 0 };
            self.draw_pic_tinted(lay, lbx, entry, frame, x + shift, y + shift, tint)
        });
        if drawn.is_none() {
            let base = if !enabled {
                Color::new(0.2, 0.2, 0.2, 1.0)
            } else if hover {
                Color::new(0.45, 0.35, 0.2, 1.0)
            } else {
                Color::new(0.3, 0.22, 0.12, 1.0)
            };
            draw_rectangle(r.x, r.y, r.w, r.h, base);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.8, 0.65, 0.3, 1.0));
            let view = (lay.origin.x, lay.origin.y, lay.scale);
            let color = if enabled { WHITE } else { GRAY };
            self.draw_styled(
                view,
                &Style::nearest(1, color).align(crate::frame::Align::Center),
                label,
                rect.0 + rect.2 / 2.0,
                rect.1 + 1.5,
            );
        }
        state.clicked()
    }
}

/// Draw and run the start menu. Quit sets `quit`.
pub(crate) struct MainMenuPage;

impl Page for MainMenuPage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        clear_background(BLACK);
        let lay = Layout::new();
        let has_slots = app.menu.slots.iter().any(Slot::loadable);
        let mut rows = Vec::new();
        if has_slots {
            rows.push(Entry::Load);
        }
        if app.menu.can_continue {
            // Continue sits above Load, or takes its place.
            rows.insert(0, Entry::Continue);
        }
        rows.extend([Entry::New, Entry::Fame, Entry::Quit]);

        // The logo and the picture behind the entries.
        if app.draw_pic(&lay, "mainscrn", 0, 0, 0.0, 0.0).is_some() {
            app.draw_pic(&lay, "mainscrn", 5, 0, 0.0, 41.0);
        } else {
            let view = (lay.origin.x, lay.origin.y, lay.scale);
            app.draw_styled(
                view,
                &Style::nearest(5, GOLD).align(crate::frame::Align::Center),
                &app.text("menu.title"),
                160.0,
                60.0,
            );
        }

        // The entries: the original puts them 12 pixels apart from y = 141, and the "words" are
        // `vortex.lbx` pictures (1 Continue, 2 Hall of Fame, 3 Quit, 4 New Game, 5 Load Game).
        let mut chosen = None;
        let top = if rows.len() >= 5 {
            141.0
        } else {
            141.0 + 12.0 * (5 - rows.len()) as f32
        };
        for (i, &entry) in rows.iter().enumerate() {
            let (label, vortex, hotkey) = match entry {
                Entry::Continue => ("menu.continue", 1, KeyCode::C),
                Entry::Load => ("menu.load_game", 5, KeyCode::L),
                Entry::New => ("menu.new_game", 4, KeyCode::N),
                Entry::Fame => ("menu.hall_of_fame", 2, KeyCode::H),
                Entry::Quit => ("menu.quit", 3, KeyCode::Q),
            };
            let label = app.text(label);
            let y = top + 12.0 * i as f32;
            let enabled = true;
            let clicked = app.menu_button(
                &lay,
                (108.0, y - 2.0, 103.0, 12.0),
                Some(("vortex", vortex, 123.0, y)),
                &label,
                enabled,
                true,
                false,
                input,
            );
            if (clicked || (enabled && input.key_pressed(hotkey))) && chosen.is_none() {
                chosen = Some(entry);
            }
        }
        if input.key_pressed(KeyCode::Escape) {
            chosen = Some(Entry::Quit);
        }

        if !app.menu.message.is_empty() {
            let view = (lay.origin.x, lay.origin.y, lay.scale);
            let msg = app.menu.message.clone();
            app.draw_styled(
                view,
                &Style::nearest(1, RED).align(crate::frame::Align::Center),
                &msg,
                160.0,
                130.0,
            );
        }

        match chosen {
            Some(Entry::Continue) => {
                if let Err(e) = app.load_from(Path::new(AUTOSAVE)) {
                    app.menu.message = app.text_with(
                        "menu.could_not_continue",
                        &Args::new().text("why", e.to_string()),
                    );
                }
            }
            Some(Entry::Load) => app.open_slots(true),
            Some(Entry::New) => app.open_options(),
            Some(Entry::Fame) => app.open_hall_of_fame(),
            Some(Entry::Quit) => app.quit = true,
            None => {}
        }
    }
}

/// Draw and run the Load / Save screen.
pub(crate) struct SlotsPage;

impl Page for SlotsPage {
    fn frame(&mut self, app: &mut App, input: &Input) {
        clear_background(BLACK);
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        if app.draw_pic(&lay, "load", 0, 0, 0.0, 0.0).is_none() {
            let r = lay.rect(crate::resolution::SCREEN);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.07, 0.06, 1.0));
            app.draw_styled(
                view,
                &Style::nearest(5, GOLD).align(crate::frame::Align::Center),
                &app.text("menu.load_save"),
                160.0,
                20.0,
            );
        }

        // The slots: rows 15 pixels apart from y = 47.
        for n in 0..SLOTS {
            let y = 47.0 + 15.0 * n as f32;
            let slot = app.menu.slots.get(n).cloned().unwrap_or(Slot::Empty);
            let r = lay.rect((43.0, y, 229.0, 12.0));
            let picked = app.menu.selected == Some(n);
            if slot != Slot::Empty {
                // The original lays a text-field picture behind a filled slot.
                if app.draw_pic(&lay, "load", 9, 0, 43.0, y).is_none() {
                    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.2, 0.15, 0.1, 1.0));
                }
            } else if app.art.picture("load", 9, 0).is_none() {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.4, 0.3, 0.2, 1.0));
            }
            if picked && app.draw_pic(&lay, "load", 10, 0, 36.0, y).is_none() {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, GOLD);
            }
            let color = if picked {
                WHITE
            } else {
                Color::new(0.85, 0.8, 0.6, 1.0)
            };
            app.draw_styled(
                view,
                &Style::nearest(1, color).align(crate::frame::Align::Left),
                &slot.label(app.texts(), n),
                45.0,
                y + 1.0,
            );
            if ui::hit(r, true, input).clicked() {
                if app.menu.selected != Some(n) {
                    app.menu.confirm_overwrite = false;
                    app.menu.message.clear();
                }
                app.menu.selected = Some(n);
            }
        }

        let selected = app.menu.selected;
        let selected_slot = selected.and_then(|n| app.menu.slots.get(n).cloned());
        let can_load = selected_slot.as_ref().is_some_and(Slot::loadable);
        let in_game = !app.menu.from_menu;
        let can_save = in_game && selected.is_some() && app.game.game_over.is_none();

        // Buttons along y = 171: Quit, Load, Save, Settings, OK (the original's positions).
        let quit_label = app.text(if app.menu.from_menu {
            "menu.back"
        } else {
            "menu.quit"
        });
        let quit = app.menu_button(
            &lay,
            (43.0, 171.0, 39.0, 13.0),
            (!app.menu.from_menu).then_some(("load", 2, 43.0, 171.0)),
            &quit_label,
            true,
            false,
            true,
            input,
        );
        let load = app.menu_button(
            &lay,
            (83.0, 171.0, 38.0, 13.0),
            Some(("load", if can_load { 1 } else { 6 }, 83.0, 171.0)),
            &app.text("menu.load"),
            can_load,
            false,
            true,
            input,
        );
        let save = app.menu_button(
            &lay,
            (122.0, 171.0, 38.0, 13.0),
            Some(("load", if can_save { 3 } else { 7 }, 122.0, 171.0)),
            &app.text("menu.save"),
            can_save,
            false,
            true,
            input,
        );
        let settings = app.menu_button(
            &lay,
            (172.0, 171.0, 58.0, 13.0),
            Some(("load", 12, 172.0, 171.0)),
            &app.text("menu.settings"),
            true,
            false,
            true,
            input,
        );
        let ok = app.menu_button(
            &lay,
            (231.0, 171.0, 40.0, 13.0),
            Some(("load", 4, 231.0, 171.0)),
            &app.text("ui.ok"),
            true,
            false,
            true,
            input,
        ) || input.key_pressed(KeyCode::Escape);

        if app.menu.confirm_overwrite {
            app.draw_styled(
                view,
                &Style::nearest(1, GOLD).align(crate::frame::Align::Center),
                &app.text("menu.confirm_overwrite"),
                160.0,
                155.0,
            );
        } else if !app.menu.message.is_empty() {
            let msg = app.menu.message.clone();
            app.draw_styled(
                view,
                &Style::nearest(1, RED).align(crate::frame::Align::Center),
                &msg,
                160.0,
                155.0,
            );
        }

        if settings {
            app.open_settings();
        } else if load {
            if let Some(n) = selected {
                match app.load_from(&slot_path(n)) {
                    Ok(()) => {
                        app.log.push(
                            app.text_with("menu.loaded_slot", &Args::new().count("slot", n + 1)),
                        );
                    }
                    Err(e) => {
                        app.menu.message = app.text_with(
                            "menu.could_not_load",
                            &Args::new().text("why", e.to_string()),
                        )
                    }
                }
            }
        } else if save {
            if let Some(n) = selected {
                let filled = app.menu.slots.get(n).is_some_and(|s| *s != Slot::Empty);
                if filled && !app.menu.confirm_overwrite {
                    app.menu.confirm_overwrite = true;
                } else {
                    app.menu.confirm_overwrite = false;
                    app.save_to(&slot_path(n));
                    app.menu.slots[n] = Slot::read(&slot_path(n));
                    if !app.menu.slots[n].loadable() {
                        app.menu.message = app.text("menu.could_not_save");
                    } else {
                        app.screen = Screen::Map;
                    }
                }
            }
        } else if quit {
            if app.menu.from_menu {
                app.open_main_menu();
            } else {
                // Leaving the game keeps it as the one Continue offers.
                if app.game.game_over.is_none() {
                    app.save_to(Path::new(AUTOSAVE));
                }
                app.open_main_menu();
            }
        } else if ok {
            if app.menu.from_menu {
                app.open_main_menu();
            } else {
                app.screen = Screen::Map;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_written_as_utc_dates() {
        assert_eq!(format_time(0), "1970-01-01 00:00");
        assert_eq!(
            format_time(951_782_400 + 3_600 * 13 + 60 * 5),
            "2000-02-29 13:05"
        );
        assert_eq!(format_time(1_760_179_800), "2025-10-11 10:50");
    }

    #[test]
    fn slots_have_their_own_files() {
        assert_eq!(slot_path(0), PathBuf::from("saves/slot1.ron"));
        assert_eq!(slot_path(SLOTS - 1), PathBuf::from("saves/slot8.ron"));
    }

    #[test]
    fn a_slot_row_names_the_wizard_the_turn_and_the_time() {
        let filled = Slot::Filled {
            summary: SaveSummary {
                wizard: "Merlin".into(),
                turn: 41,
                finished: false,
            },
            saved_at: Some(0),
        };
        let t = f4x_data::load_texts(&[std::path::Path::new("../../content")], "en").unwrap();
        assert_eq!(filled.label(&t, 2), "3.  Merlin, turn 41  1970-01-01 00:00");
        assert_eq!(Slot::Empty.label(&t, 0), "1.  (empty)");
        assert!(Slot::Damaged.label(&t, 7).contains("damaged"));
        assert!(filled.loadable() && !Slot::Empty.loadable() && !Slot::Damaged.loadable());
    }

    #[test]
    fn reading_slots_tells_empty_from_damaged() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-saves/slots");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Slot::read(&dir.join("none.ron")), Slot::Empty);
        std::fs::write(dir.join("bad.ron"), "nope").unwrap();
        assert_eq!(Slot::read(&dir.join("bad.ron")), Slot::Damaged);
    }

    #[test]
    fn a_save_of_another_game_version_is_incompatible_not_damaged() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-saves/slots");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old.ron"), "(version: 1, game_version: \"0.0.1\")").unwrap();
        let slot = Slot::read(&dir.join("old.ron"));
        assert_eq!(
            slot,
            Slot::Incompatible {
                version: "0.0.1".into()
            }
        );
        let t = f4x_data::load_texts(&[std::path::Path::new("../../content")], "en").unwrap();
        assert_eq!(
            slot.label(&t, 0),
            "1.  Incompatible save (game version 0.0.1)"
        );
        assert!(!slot.loadable());
    }
}
