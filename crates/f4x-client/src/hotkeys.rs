//! The keys of the overland map, by what they do (`content/hotkeys.ron`). The file lists the keys
//! of each action; the player's own `saves/hotkeys.ron` and the `hotkeys.ron` of a mod's content
//! folder replace the keys of the actions they name. The screens' own keys (Enter and Escape in a
//! question, the arrows in a list) stay as they are.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use macroquad::prelude::KeyCode;
use macroquad::prelude::{is_key_down, is_key_pressed};

/// Every action a key can do on the map; the names are those of `hotkeys.ron`.
pub(crate) const ACTIONS: [&str; 41] = [
    "end_turn",
    "game_menu",
    "quick_save",
    "quick_load",
    "undo",
    "console",
    "magic",
    "spellbook",
    "army_list",
    "advisors",
    "surveyor",
    "cartographer",
    "apprentice",
    "history",
    "astrologer",
    "chancellor",
    "tax_collector",
    "grand_vizier",
    "mirror",
    "zoom_in",
    "zoom_out",
    "other_plane",
    "centre",
    "scroll_left",
    "scroll_right",
    "scroll_up",
    "scroll_down",
    "unselect",
    "special",
    "plane_shift",
    "balance_farmers",
    "wait",
    "done",
    "move_down_left",
    "move_down",
    "move_down_right",
    "move_left",
    "move_right",
    "move_up_left",
    "move_up",
    "move_up_right",
];

/// The key names the file accepts.
const KEYS: &[(&str, KeyCode)] = &[
    ("A", KeyCode::A),
    ("B", KeyCode::B),
    ("C", KeyCode::C),
    ("D", KeyCode::D),
    ("E", KeyCode::E),
    ("F", KeyCode::F),
    ("G", KeyCode::G),
    ("H", KeyCode::H),
    ("I", KeyCode::I),
    ("J", KeyCode::J),
    ("K", KeyCode::K),
    ("L", KeyCode::L),
    ("M", KeyCode::M),
    ("N", KeyCode::N),
    ("O", KeyCode::O),
    ("P", KeyCode::P),
    ("Q", KeyCode::Q),
    ("R", KeyCode::R),
    ("S", KeyCode::S),
    ("T", KeyCode::T),
    ("U", KeyCode::U),
    ("V", KeyCode::V),
    ("W", KeyCode::W),
    ("X", KeyCode::X),
    ("Y", KeyCode::Y),
    ("Z", KeyCode::Z),
    ("Key0", KeyCode::Key0),
    ("Key1", KeyCode::Key1),
    ("Key2", KeyCode::Key2),
    ("Key3", KeyCode::Key3),
    ("Key4", KeyCode::Key4),
    ("Key5", KeyCode::Key5),
    ("Key6", KeyCode::Key6),
    ("Key7", KeyCode::Key7),
    ("Key8", KeyCode::Key8),
    ("Key9", KeyCode::Key9),
    ("F1", KeyCode::F1),
    ("F2", KeyCode::F2),
    ("F3", KeyCode::F3),
    ("F4", KeyCode::F4),
    ("F5", KeyCode::F5),
    ("F6", KeyCode::F6),
    ("F7", KeyCode::F7),
    ("F8", KeyCode::F8),
    ("F9", KeyCode::F9),
    ("F10", KeyCode::F10),
    ("F11", KeyCode::F11),
    ("F12", KeyCode::F12),
    ("Enter", KeyCode::Enter),
    ("Escape", KeyCode::Escape),
    ("Tab", KeyCode::Tab),
    ("Space", KeyCode::Space),
    ("Backspace", KeyCode::Backspace),
    ("Delete", KeyCode::Delete),
    ("Insert", KeyCode::Insert),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("PageUp", KeyCode::PageUp),
    ("PageDown", KeyCode::PageDown),
    ("Left", KeyCode::Left),
    ("Right", KeyCode::Right),
    ("Up", KeyCode::Up),
    ("Down", KeyCode::Down),
    ("Kp0", KeyCode::Kp0),
    ("Kp1", KeyCode::Kp1),
    ("Kp2", KeyCode::Kp2),
    ("Kp3", KeyCode::Kp3),
    ("Kp4", KeyCode::Kp4),
    ("Kp5", KeyCode::Kp5),
    ("Kp6", KeyCode::Kp6),
    ("Kp7", KeyCode::Kp7),
    ("Kp8", KeyCode::Kp8),
    ("Kp9", KeyCode::Kp9),
    ("KpAdd", KeyCode::KpAdd),
    ("KpSubtract", KeyCode::KpSubtract),
    ("KpEnter", KeyCode::KpEnter),
    ("Equal", KeyCode::Equal),
    ("Minus", KeyCode::Minus),
    ("Comma", KeyCode::Comma),
    ("Period", KeyCode::Period),
    ("Slash", KeyCode::Slash),
    ("Semicolon", KeyCode::Semicolon),
    ("Apostrophe", KeyCode::Apostrophe),
    ("LeftBracket", KeyCode::LeftBracket),
    ("RightBracket", KeyCode::RightBracket),
    ("Backslash", KeyCode::Backslash),
    ("GraveAccent", KeyCode::GraveAccent),
];

/// A key with the modifiers it needs. Control (or Cmd) must be held exactly when the chord names
/// it, so M and Ctrl+M are two different keys; Shift and Alt are only asked for when named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Chord {
    pub key: KeyCode,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Chord {
    /// `"Ctrl+Z"`, `"F10"`, `"Shift+Left"`.
    pub fn parse(text: &str) -> Result<Chord, String> {
        let mut chord = Chord {
            key: KeyCode::Unknown,
            ctrl: false,
            shift: false,
            alt: false,
        };
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let (key, mods) = parts.split_last().ok_or("an empty key")?;
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" | "control" => chord.ctrl = true,
                "shift" => chord.shift = true,
                "alt" => chord.alt = true,
                _ => return Err(format!("{text:?}: {m:?} is not Ctrl, Shift or Alt")),
            }
        }
        chord.key = KEYS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|&(_, k)| k)
            .ok_or_else(|| format!("{text:?}: {key:?} is not a key"))?;
        Ok(chord)
    }

    fn modifiers_held() -> (bool, bool, bool) {
        (
            is_key_down(KeyCode::LeftControl)
                || is_key_down(KeyCode::RightControl)
                || is_key_down(KeyCode::LeftSuper)
                || is_key_down(KeyCode::RightSuper),
            is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift),
            is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt),
        )
    }

    fn modifiers_ok(&self) -> bool {
        let (ctrl, shift, alt) = Self::modifiers_held();
        self.ctrl == ctrl && (!self.shift || shift) && (!self.alt || alt)
    }

    pub fn pressed(&self) -> bool {
        is_key_pressed(self.key) && self.modifiers_ok()
    }

    pub fn down(&self) -> bool {
        is_key_down(self.key) && self.modifiers_ok()
    }

    /// How it is shown to the player: `Ctrl+Z`, `Numpad 1`, `Page Up`.
    pub fn label(&self) -> String {
        let name = KEYS
            .iter()
            .find(|&&(_, k)| k == self.key)
            .map_or("?", |&(n, _)| n);
        let key = match name {
            n if n.starts_with("Kp") && n.len() == 3 => format!("Numpad {}", &n[2..]),
            "KpAdd" => "Numpad +".into(),
            "KpSubtract" => "Numpad -".into(),
            "KpEnter" => "Numpad Enter".into(),
            "Equal" => "=".into(),
            "Minus" => "-".into(),
            "Comma" => ",".into(),
            "Period" => ".".into(),
            "Slash" => "/".into(),
            "Semicolon" => ";".into(),
            "Apostrophe" => "'".into(),
            "LeftBracket" => "[".into(),
            "RightBracket" => "]".into(),
            "Backslash" => "\\".into(),
            "GraveAccent" => "Backquote".into(),
            "PageUp" => "Page Up".into(),
            "PageDown" => "Page Down".into(),
            n if n.starts_with("Key") => n[3..].to_string(),
            n => n.to_string(),
        };
        let mut label = String::new();
        if self.ctrl {
            label += "Ctrl+";
        }
        if self.shift {
            label += "Shift+";
        }
        if self.alt {
            label += "Alt+";
        }
        label + &key
    }
}

/// The keys of every action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hotkeys {
    keys: HashMap<&'static str, Vec<Chord>>,
}

/// The file as shipped, so the game has its keys wherever it is started from.
const SHIPPED: &str = include_str!("../../../content/hotkeys.ron");

impl Default for Hotkeys {
    fn default() -> Self {
        Self::from_layers(&[SHIPPED]).expect("content/hotkeys.ron is valid")
    }
}

fn action_name(name: &str) -> Option<&'static str> {
    ACTIONS.iter().find(|a| **a == name).copied()
}

impl Hotkeys {
    /// The texts of `hotkeys.ron` files, the first the base and each later one replacing the keys
    /// of the actions it names. An unknown action or key is an error, so a typo is not silent.
    pub fn from_layers(layers: &[&str]) -> Result<Hotkeys, String> {
        let mut keys: HashMap<&'static str, Vec<Chord>> = HashMap::new();
        for text in layers {
            let file: BTreeMap<String, Vec<String>> =
                ron::from_str(text).map_err(|e| format!("hotkeys.ron: {e}"))?;
            for (name, list) in file {
                let action =
                    action_name(&name).ok_or_else(|| format!("hotkeys.ron: no action {name:?}"))?;
                let chords = list
                    .iter()
                    .map(|k| Chord::parse(k))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("hotkeys.ron, {name}: {e}"))?;
                keys.insert(action, chords);
            }
        }
        Ok(Hotkeys { keys })
    }

    /// The shipped keys, then the `hotkeys.ron` of each folder (a mod's content, then the
    /// player's own), skipping the ones that are not there.
    pub fn load(folders: &[&Path]) -> Result<Hotkeys, String> {
        let mut texts = vec![SHIPPED.to_string()];
        for folder in folders {
            if let Ok(text) = std::fs::read_to_string(folder.join("hotkeys.ron")) {
                texts.push(text);
            }
        }
        let layers: Vec<&str> = texts.iter().map(String::as_str).collect();
        Self::from_layers(&layers)
    }

    fn chords(&self, action: &str) -> &[Chord] {
        self.keys.get(action).map_or(&[], Vec::as_slice)
    }

    /// An action's key went down this frame.
    pub fn pressed(&self, action: &str) -> bool {
        self.chords(action).iter().any(Chord::pressed)
    }

    /// A move key went down. With Shift held, a key that also scrolls the map is left to the
    /// map (the arrows move the selected stack, or scroll the map with Shift).
    pub fn pressed_moving(&self, action: &str, shift: bool) -> bool {
        self.chords(action).iter().any(|c| {
            c.pressed()
                && !(shift
                    && ["scroll_left", "scroll_right", "scroll_up", "scroll_down"]
                        .iter()
                        .any(|s| self.uses_key(s, c.key)))
        })
    }

    /// An action's key is held.
    pub fn down(&self, action: &str) -> bool {
        self.chords(action).iter().any(Chord::down)
    }

    /// The keys of `action`, as the player reads them: `Enter or N`.
    pub fn label(&self, action: &str) -> String {
        let labels: Vec<String> = self.chords(action).iter().map(Chord::label).collect();
        if labels.is_empty() {
            String::new()
        } else {
            labels.join(" or ")
        }
    }

    /// Whether `key` (with no modifier) is one of the keys of `action`.
    pub fn uses_key(&self, action: &str, key: KeyCode) -> bool {
        self.chords(action).iter().any(|c| c.key == key)
    }

    /// Every action and its chords, for checks.
    #[cfg(test)]
    pub fn all(&self) -> impl Iterator<Item = (&&'static str, &Vec<Chord>)> {
        self.keys.iter()
    }
}

/// Replace `{key:action}` in a help text with the keys of that action.
pub(crate) fn fill_keys(text: &str, hotkeys: &Hotkeys) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{key:") {
        out += &rest[..start];
        let after = &rest[start + 5..];
        match after.find('}') {
            Some(end) => {
                out += &hotkeys.label(&after[..end]);
                rest = &after[end + 1..];
            }
            None => {
                out += &rest[start..];
                rest = "";
            }
        }
    }
    out + rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chord_is_parsed_and_shown() {
        let c = Chord::parse("Ctrl+Z").unwrap();
        assert!(c.ctrl && !c.shift && c.key == KeyCode::Z);
        assert_eq!(c.label(), "Ctrl+Z");
        assert_eq!(Chord::parse("kp1").unwrap().label(), "Numpad 1");
        assert_eq!(Chord::parse("PageUp").unwrap().label(), "Page Up");
        assert_eq!(Chord::parse("GraveAccent").unwrap().label(), "Backquote");
        assert_eq!(Chord::parse("Key5").unwrap().label(), "5");
        assert!(Chord::parse("Ctrl+Hyper").is_err());
        assert!(Chord::parse("Meta+Z").is_err());
    }

    #[test]
    fn the_shipped_file_names_every_action_with_a_key_and_no_two_actions_share_one() {
        let keys = Hotkeys::default();
        for name in ACTIONS.iter() {
            assert!(!keys.chords(name).is_empty(), "{name} has no key");
        }
        // Arrows are shared on purpose: they scroll or move the stack, depending on a selection.
        let shared_on_purpose = ["scroll_", "move_"];
        let mut seen: HashMap<(KeyCode, bool), &str> = HashMap::new();
        for (action, chords) in keys.all() {
            for c in chords {
                if let Some(other) = seen.insert((c.key, c.ctrl), action) {
                    let both = |p: &str| action.starts_with(p) && other.starts_with(p);
                    let scroll_and_move = [action, &other]
                        .iter()
                        .all(|a| shared_on_purpose.iter().any(|p| a.starts_with(p)));
                    assert!(
                        both("move_") || both("scroll_") || scroll_and_move,
                        "{action} and {other} share {}",
                        c.label()
                    );
                }
            }
        }
    }

    #[test]
    fn a_later_file_replaces_only_the_actions_it_names_and_a_typo_is_an_error() {
        let keys =
            Hotkeys::from_layers(&[SHIPPED, r#"{"end_turn": ["Space"], "magic": []}"#]).unwrap();
        assert_eq!(keys.label("end_turn"), "Space");
        assert_eq!(keys.label("magic"), "");
        assert_eq!(keys.label("spellbook"), "S");
        assert!(Hotkeys::from_layers(&[SHIPPED, r#"{"end_trun": ["N"]}"#]).is_err());
        assert!(Hotkeys::from_layers(&[SHIPPED, r#"{"end_turn": ["Nn"]}"#]).is_err());
    }

    #[test]
    fn help_text_shows_the_keys_in_force() {
        let keys = Hotkeys::from_layers(&[SHIPPED, r#"{"done": ["F"]}"#]).unwrap();
        assert_eq!(
            fill_keys(
                "Press {key:done} or {key:end_turn}. {key:nothing} {key:",
                &keys
            ),
            "Press F or Enter or N.  {key:"
        );
    }
}
