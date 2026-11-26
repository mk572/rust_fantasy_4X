//! The language of the interface: which languages are installed, the player's choice (kept in
//! `settings/language.txt`, not in `saves/`, which holds the game saves, and not in the shipped
//! `client_settings.ron`, which the game never writes)
//! and the switch, which takes effect at once: the messages, the names, the help and the event
//! sentences are loaded again from the chosen language (`docs/text.md`).

use std::path::Path;

use crate::font::{Ink, Style};
use crate::font_ramp;
use crate::frame::Layout;
use crate::input::Input;
use crate::modal::Modal;
use crate::names::Names;
use crate::text::{load_texts, Args};
use crate::ui;
use crate::view::App;
use macroquad::prelude::*;

/// The player's choice, one line with the language's code.
pub(crate) const FILE: &str = "settings/language.txt";

/// An installed language: the folder of `text/` and its own name for itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed {
    pub code: String,
    pub name: String,
}

/// The languages found under `text/` of the layers (a folder with files), by code, each with the
/// name its own `language.name` gives (its code when it has none).
pub(crate) fn installed(layers: &[&Path]) -> Vec<Installed> {
    let mut codes: Vec<String> = layers
        .iter()
        .filter_map(|d| std::fs::read_dir(d.join("text")).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    codes.sort();
    codes.dedup();
    codes
        .into_iter()
        .filter_map(|code| {
            let texts = load_texts(layers, &code).ok()?;
            let name = if texts.has_own("language.name") {
                texts.get("language.name", &Args::new())
            } else {
                code.clone()
            };
            Some(Installed { code, name })
        })
        .collect()
}

/// The code the player chose before, if the file has one.
pub(crate) fn saved(path: &Path) -> Option<String> {
    let code = std::fs::read_to_string(path).ok()?.trim().to_string();
    (!code.is_empty()).then_some(code)
}

/// The language to start in: the player's choice when it is installed, else the settings'.
pub(crate) fn initial(layers: &[&Path], saved_choice: Option<String>, default: &str) -> String {
    let have = |code: &str| installed(layers).iter().any(|l| l.code == code);
    saved_choice
        .filter(|c| have(c))
        .unwrap_or_else(|| default.to_string())
}

impl App {
    /// Switch the interface to `code` at once: the messages, the names, the help and the event
    /// sentences. Nothing changes if any of them does not load. `lang_layers` and `layers` are
    /// the language files and the client's content (`main.rs`).
    pub(crate) fn switch_language(
        &mut self,
        lang_layers: &[&Path],
        layers: &[&Path],
        message_dir: &Path,
        code: &str,
    ) -> Result<(), String> {
        let texts = load_texts(lang_layers, code)?;
        let names = Names::load(lang_layers, code)?;
        let problems = names.problems(self.world.content());
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
        let help = f4x_def::load::load_help(&[lang_layers[0], layers[0]], code)
            .map_err(|e| e.to_string())?;
        let event_texts = crate::event_message::load(message_dir).unwrap_or_default();
        self.texts = texts;
        self.names = names;
        self.help = help;
        self.event_texts = event_texts;
        Ok(())
    }

    /// The switch the menu makes: from the installed folders, and the choice is remembered.
    pub(crate) fn choose_language(&mut self, code: &str) {
        let lang_layers = [crate::shipped(f4x_lang::LANG_CONTENT)];
        let layers = [crate::shipped(crate::CLIENT_CONTENT)];
        let message = crate::message_dir(code, "event_texts.ron");
        let lang_refs: Vec<&Path> = lang_layers.iter().map(|p| p.as_path()).collect();
        let layer_refs: Vec<&Path> = layers.iter().map(|p| p.as_path()).collect();
        match self.switch_language(&lang_refs, &layer_refs, &message, code) {
            Ok(()) => {
                let written = Path::new(FILE)
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(FILE, format!("{code}\n")));
                if let Err(e) = written {
                    eprintln!("could not remember the language: {e}");
                }
            }
            Err(e) => {
                self.menu.message =
                    self.text_with("menu.language_failed", &Args::new().text("why", e));
            }
        }
    }

    /// Open the list of installed languages over the start menu.
    pub(crate) fn open_language_list(&mut self) {
        let lang_layers = [crate::shipped(f4x_lang::LANG_CONTENT)];
        let refs: Vec<&Path> = lang_layers.iter().map(|p| p.as_path()).collect();
        let choices = installed(&refs);
        let current = self.texts.language().to_string();
        self.modals.open(LanguageList { choices, current });
    }
}

/// The box that lists the languages; a click on one switches to it, Esc or a click elsewhere
/// closes it.
pub(crate) struct LanguageList {
    choices: Vec<Installed>,
    current: String,
}

impl Modal for LanguageList {
    fn kind(&self) -> &'static str {
        "language"
    }

    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        let lay = Layout::new();
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let line = app.art.text_height(4) + 3.0;
        let paragraph = line * self.choices.len() as f32;
        let (x, y) = (
            68.0,
            ((crate::resolution::DESIGN_H - (paragraph + 34.0)) / 2.0).floor(),
        );
        app.draw_pic_rows(&lay, "resource", 0, x, y, y, y + paragraph + 12.0);
        app.draw_pic(&lay, "resource", 1, 0, x, y + paragraph + 10.0);
        let mut chosen = None;
        for (i, language) in self.choices.iter().enumerate() {
            let row_y = y + 9.0 + line * i as f32;
            let hit = ui::hit(lay.rect((x + 9.0, row_y, 166.0, line)), true, input);
            let lit = hit.hovered() || language.code == self.current;
            let ink = if lit {
                font_ramp::MENU_WORD_HOVER
            } else {
                font_ramp::NOTIFY_TEXT
            };
            app.draw_styled(
                view,
                &Style::inked(4, Ink::Set(ink))
                    .shadow()
                    .edge(crate::font::rgb(0, 0, 0)),
                &language.name,
                x + 9.0,
                row_y,
            );
            if hit.clicked() && chosen.is_none() {
                chosen = Some(language.code.clone());
            }
        }
        let cancel = app.slab_button(
            &lay,
            (crate::action_button_art::Slab::No, "ui.button_cancel"),
            (x + 101.0, y + paragraph + 15.0),
            input,
        );
        if let Some(code) = chosen {
            if code != self.current {
                app.choose_language(&code);
            }
            return None;
        }
        if cancel.clicked() || input.key_pressed(KeyCode::Escape) {
            return None;
        }
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::{TestDir, TestResult};

    fn shipped_lang() -> std::path::PathBuf {
        std::path::PathBuf::from(crate::test_app::LANG_CONTENT)
    }

    #[test]
    fn the_shipped_language_is_installed_under_its_own_name() {
        let lang = shipped_lang();
        let found = installed(&[lang.as_path()]);
        assert!(
            found.contains(&Installed {
                code: "en".into(),
                name: "English".into()
            }),
            "{found:?}"
        );
    }

    #[test]
    fn a_language_with_no_name_of_its_own_shows_its_code() -> TestResult {
        let dir = TestDir::new("languages");
        std::fs::create_dir_all(dir.join("text/xx"))?;
        std::fs::write(dir.join("text/xx/menu.ron"), r#"{ "menu.settings": "Xx" }"#)?;
        let found = installed(&[dir.as_ref()]);
        assert_eq!(
            found,
            vec![Installed {
                code: "xx".into(),
                name: "xx".into()
            }]
        );
        Ok(())
    }

    #[test]
    fn switching_the_language_changes_the_messages_and_the_names_at_once() -> TestResult {
        let dir = TestDir::new("language_switch");
        std::fs::create_dir_all(dir.join("text/xx/name"))?;
        std::fs::write(
            dir.join("text/xx/menu.ron"),
            r#"{ "menu.language": "Sprache", "menu.settings": "Einstellungen" }"#,
        )?;
        std::fs::write(
            dir.join("text/xx/name/unit.ron"),
            r#"{ 76: (text: "Zwergische Schwertkämpfer", gender: Masculine) }"#,
        )?;
        let lang = shipped_lang();
        let client = std::path::PathBuf::from(crate::test_app::CLIENT_CONTENT);
        let lang_layers = [lang.as_path(), dir.as_ref()];
        let mut app = App::for_test();
        assert_eq!(app.text("menu.settings"), "Settings");
        app.switch_language(
            &lang_layers,
            &[client.as_path()],
            &dir.join("no_events"),
            "xx",
        )?;
        assert_eq!(app.text("menu.settings"), "Einstellungen");
        assert_eq!(app.names.unit(76), "Zwergische Schwertkämpfer");

        // What the language lacks is English.
        assert_eq!(app.text("menu.quit"), "Quit");
        assert_eq!(app.names.unit(77), "Dwarven Halberdiers");
        // A folder that is not there is English all the way (the list only offers installed ones).
        app.switch_language(
            &lang_layers,
            &[client.as_path()],
            &dir.join("no_events"),
            "yy",
        )?;
        assert_eq!(app.text("menu.settings"), "Settings");
        Ok(())
    }

    #[test]
    fn german_is_listed_and_switches_the_menu_and_the_names() -> TestResult {
        let lang = shipped_lang();
        let layers = [lang.as_path()];
        let found = installed(&layers);
        assert!(
            found.contains(&Installed {
                code: "de".into(),
                name: "Deutsch".into()
            }),
            "{found:?}"
        );
        let client = std::path::PathBuf::from(crate::test_app::CLIENT_CONTENT);
        let mut app = App::for_test();
        app.switch_language(&layers, &[client.as_path()], &lang.join("none"), "de")?;
        assert_eq!(app.text("menu.language"), "Sprache");
        assert_eq!(app.names.race(4), "Zwerg");
        assert_eq!(app.names.race_adjective(4), "zwergisch");
        assert_eq!(
            app.names.building_term(35).gender,
            Some(f4x_def::text::Gender::Feminine)
        );
        assert_eq!(app.text("menu.quit_question"), "Zum Desktop zurückkehren?");
        // A unit is named whole, race included, and carries its number.
        assert_eq!(app.names.unit(76), "Zwergische Schwertkämpfer");
        assert!(app.names.unit_term(76).plural);
        // A made item's name takes the power in its own form: "Schwert der Flammen".
        let made = |suffix| {
            let mut item =
                f4x_core::item::Item::from_def(&app.world.content().rules.items.defs()[0]);
            item.name.clear();
            item.prefab = None;
            item.made = Some(f4x_core::item::crafting::MadeName {
                kind: f4x_core::item::crafting::TypeName::of(f4x_def::rules::ItemKind::Sword, 0),
                bonus: None,
                suffix: Some(suffix),
            });
            app.names.item_name(&item, app.texts())
        };
        let flaming = app
            .world
            .content()
            .rules
            .item_powers
            .id_numbered(33)
            .expect("the power exists");
        assert_eq!(
            made(f4x_core::item::crafting::Suffix::Special(flaming)),
            "Schwert der Flammen"
        );
        assert_eq!(
            made(f4x_core::item::crafting::Suffix::Strength),
            "Schwert der Stärke"
        );
        // Sentences take the case and the article the sentence asks for, and verbs agree.
        let say = |key: &str, args: &Args| app.text_with(key, args);
        let built = Args::new()
            .term("size", app.names.word("city_size.city"))
            .text("city", "Waldheim")
            .term("building", app.names.building_term(16));
        assert_eq!(
            say("city.built", &built),
            "Die Stadt Waldheim hat die Bibliothek fertiggestellt."
        );
        let sold = Args::new()
            .term("building", app.names.building_term(16))
            .count("gold", 60);
        assert_eq!(
            say("sell.ask", &sold),
            "Willst du die Bibliothek für 60 Gold zurückverkaufen?"
        );
        let cast = Args::new().term("spell", app.names.spell_term(96));
        assert_eq!(say("spell.you_cast", &cast), "Du wirkst den Feuerball");
        assert_eq!(
            say("spell.abort", &cast),
            "Willst du den Feuerball abbrechen?"
        );
        let slayer = Args::new()
            .term("item", app.names.item_term_of_num(3))
            .count("price", 90);
        assert!(say("offer.merchant", &slayer).contains("einen Drachentöter"));
        let hit = |attacker: u16, defender: u16| {
            Args::new()
                .term("attacker", app.names.unit_term(attacker))
                .term("defender", app.names.unit_term(defender))
                .count("dealt", 4)
                .text("kills", "")
                .text("taken", "")
        };
        assert_eq!(
            say("battle.hits", &hit(76, 2)),
            "Die Zwergischen Schwertkämpfer treffen den Weisen: 4 Schaden"
        );
        assert_eq!(
            say("battle.hits", &hit(2, 76)),
            "Der Weise trifft die Zwergischen Schwertkämpfer: 4 Schaden"
        );
        let size = app.names.word("city_size.city");
        let rift = Args::new().term("size", size).text("city", "Waldheim");
        assert_eq!(say("spell.city_scroll_title", &rift), "Stadt Waldheim");
        Ok(())
    }

    #[test]
    fn the_choice_is_read_back_and_a_missing_language_falls_back() -> TestResult {
        let dir = TestDir::new("language_choice");
        let file = dir.join("language.txt");
        assert_eq!(saved(&file), None);
        std::fs::write(&file, "en\n")?;
        assert_eq!(saved(&file), Some("en".to_string()));
        let lang = shipped_lang();
        let layers = [lang.as_path()];
        assert_eq!(initial(&layers, saved(&file), "en"), "en");
        assert_eq!(initial(&layers, Some("zz".into()), "en"), "en");
        Ok(())
    }
}
