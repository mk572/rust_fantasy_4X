//! Which pictures each ready-made wizard uses (`crates/f4x-client/content/wizard_art.ron`). A wizard's place in the
//! list is its slot, the number the game keeps as the wizard's portrait; every other number here is
//! an entry of the original's file that the role names (the comment in the file lists them). The
//! rules do not know this table; the client reads it, and checks the ids against the rules
//! (`asset_check`).

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

/// The gems a wizard has in `lilwiz`: the original's five colours at `gem` ..`gem` + 5, and the five
/// more of `make_banner_colours.py` at `NEW_GEMS_AT` + `gem` ..; ten in all, numbered like the banners.
pub const GEM_COLOURS: usize = 10;
pub const ORIGINAL_GEMS: usize = 5;
pub const NEW_GEMS_AT: usize = 100;

/// The entries of one wizard, one for each place the original draws or plays it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct WizardPictures {
    /// `wizards`: the new-game picker.
    pub portrait: usize,
    /// `moodwiz`: the diplomacy room and the spell animations.
    pub mirror: usize,
    /// `diplomac`: the wizard talking.
    pub talk: usize,
    /// `wizlab`: the wizard in the lab.
    pub lab: usize,
    /// `lilwiz`: the first of the wizard's gems (one for each banner colour).
    pub gem: usize,
    /// `halofam`: the hall of fame.
    pub hall: usize,
    /// `spellscr`: the summon scene.
    pub summon: usize,
    /// The wizard's place in the original's music order.
    pub music: u16,
    /// `win`: the arms rising.
    pub win: usize,
    /// `win`: the hands held up.
    pub hands: usize,
    /// `lose`: the fall into limbo.
    pub lose: usize,
    /// `spellose`: the sphere and the wizard in it.
    pub sphere: usize,
    pub sphere_figure: usize,
    /// `conquest`: the wizard standing, its pieces coming back, and seizing the other.
    pub stand: usize,
    pub pieces: usize,
    pub seize: usize,
    /// `splmastr`: the parade, the wizard and a rival passing by.
    pub parade: usize,
    pub parade_rival: usize,
}

impl WizardPictures {
    /// Every picture that is this wizard's alone, as `(file, entry, name)`: the name is the picture's
    /// place in the wizard's folder (`art/wizards/<id>/<name>`). `hands` is left out: the same
    /// hands go with several wizards.
    pub fn files(&self) -> Vec<(&'static str, usize, String)> {
        let mut files = vec![
            ("wizards", self.portrait, "portrait".to_string()),
            ("moodwiz", self.mirror, "mirror".to_string()),
            ("diplomac", self.talk, "talk".to_string()),
            ("wizlab", self.lab, "lab".to_string()),
            ("halofam", self.hall, "hall".to_string()),
            ("spellscr", self.summon, "summon".to_string()),
            ("win", self.win, "win".to_string()),
            ("lose", self.lose, "lose".to_string()),
            ("spellose", self.sphere, "sphere".to_string()),
            ("spellose", self.sphere_figure, "sphere_figure".to_string()),
            ("conquest", self.stand, "stand".to_string()),
            ("conquest", self.pieces, "pieces".to_string()),
            ("conquest", self.seize, "seize".to_string()),
            ("splmastr", self.parade, "parade".to_string()),
            ("splmastr", self.parade_rival, "parade_rival".to_string()),
        ];
        // The gems, one for each banner colour: the original's five, then the five more.
        for colour in 0..GEM_COLOURS {
            let entry = if colour < ORIGINAL_GEMS {
                self.gem + colour
            } else {
                NEW_GEMS_AT + self.gem + colour - ORIGINAL_GEMS
            };
            files.push(("lilwiz", entry, format!("gem_{colour}")));
        }
        files
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WizardArt {
    pub id: String,
    pub pictures: WizardPictures,
}

pub fn parse_wizard_art(src: &str) -> Result<Vec<WizardArt>, String> {
    ron::from_str(src).map_err(|e| format!("wizard_art.ron: {e}"))
}

/// Read `wizard_art.ron` from the first of `dirs` that has it and check the table by itself.
pub fn load_wizard_art(dirs: &[&Path]) -> Result<Vec<WizardArt>, String> {
    let path = dirs
        .iter()
        .map(|d| d.join("wizard_art.ron"))
        .find(|p| p.exists())
        .ok_or_else(|| "wizard_art.ron: not found".to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let wizards = parse_wizard_art(&text)?;
    let problems = validate_wizard_art(&wizards);
    if problems.is_empty() {
        Ok(wizards)
    } else {
        Err(format!(
            "invalid wizard_art.ron:\n  - {}",
            problems.join("\n  - ")
        ))
    }
}

/// Problems with the table by itself: a wizard twice.
pub fn validate_wizard_art(wizards: &[WizardArt]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    wizards
        .iter()
        .filter(|w| !seen.insert(&w.id))
        .map(|w| format!("wizard_art: \"{}\" twice", w.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{content_dir, TestResult};

    #[test]
    fn the_shipped_table_reads_and_a_repeat_is_reported() -> TestResult {
        let wizards = load_wizard_art(&[&content_dir()])?;
        assert_eq!(wizards.len(), 14);
        assert_eq!(wizards[0].id, "merlin");
        assert_eq!(wizards[13].pictures.parade, 27);
        assert_eq!(wizards[1].pictures.hands, 20);
        // No two pictures of the wizards share a file and an entry, and each has its own name.
        let mut seen = BTreeSet::new();
        for w in &wizards {
            for (lbx, entry, name) in w.pictures.files() {
                assert!(
                    seen.insert((lbx, entry)),
                    "{} {lbx} {entry} ({name}) is used twice",
                    w.id
                );
            }
        }
        assert_eq!(seen.len(), 14 * 25);
        let twice = parse_wizard_art(r#"[(id: "a", pictures: ()), (id: "a", pictures: ())]"#)?;
        assert_eq!(validate_wizard_art(&twice).len(), 1);
        Ok(())
    }
}
