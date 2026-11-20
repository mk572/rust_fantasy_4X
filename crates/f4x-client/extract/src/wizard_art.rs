//! Which pictures each ready-made wizard uses (`crates/f4x-client/content/wizard_art.ron`). A wizard's place in the
//! list is its slot, the number the game keeps as the wizard's portrait; every other number here is
//! an entry of the original's file that the role names (the comment in the file lists them). The
//! rules do not know this table; the client reads it, and checks the ids against the rules
//! (`asset_check`).

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

/// Where a wizard's portrait sits in its gem when the table says nothing (pixels from the gem's
/// top left corner; the gem is 41 x 43, the portrait 33 x 35).
pub const DEFAULT_FACE_AT: (usize, usize) = (4, 5);

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
    /// `magic`: the portrait of the wizard's gem (33 x 35, rounded corners), which the client puts
    /// on the diamond of the wizard's banner colour (`gem.rs`); the original's finished gems
    /// (`lilwiz`) are not drawn.
    pub face: usize,
    /// Where the portrait sits in the gem: pixels from its left and top edge. The original's artists
    /// placed each by hand, so it is measured for each wizard. (0, 0), which no gem has, means "not
    /// given": [`DEFAULT_FACE_AT`] (see [`WizardPictures::face_origin`]).
    pub face_at: (usize, usize),
    /// `diplomac`: the wizard fading into the mirror (five frames).
    pub appear: usize,
    /// `halofam`: the hall of fame.
    pub hall: usize,
    /// `spellscr`: the summon scene.
    pub summon: usize,
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
    /// Where the wizard's portrait sits in its gem: the table's place, or the default for a wizard
    /// that gives none (a custom one).
    pub fn face_origin(&self) -> (usize, usize) {
        if self.face_at == (0, 0) {
            DEFAULT_FACE_AT
        } else {
            self.face_at
        }
    }

    /// Every picture that is this wizard's alone, as `(file, entry, name)`: the name is the picture's
    /// place in the wizard's folder (`art/wizards/<id>/<name>`). `hands` is left out: the same
    /// hands go with several wizards.
    pub fn files(&self) -> Vec<(&'static str, usize, String)> {
        vec![
            ("wizards", self.portrait, "portrait".to_string()),
            ("moodwiz", self.mirror, "mirror".to_string()),
            ("diplomac", self.talk, "talk".to_string()),
            ("diplomac", self.appear, "appear".to_string()),
            ("magic", self.face, "face".to_string()),
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
        ]
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
        // The portraits of the gems are the pictures 18 to 31 of `magic`, Horus's and Tauron's the
        // other way round; each wizard has where its portrait sits, the same entry nowhere twice.
        assert_eq!(wizards[8].id, "tauron");
        assert_eq!(
            (wizards[8].pictures.face, wizards[10].pictures.face),
            (28, 26)
        );
        assert!(wizards.iter().all(|w| w.pictures.face_at != (0, 0)));
        assert_eq!(WizardPictures::default().face_origin(), DEFAULT_FACE_AT);
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
        assert_eq!(seen.len(), 14 * 17);
        let twice = parse_wizard_art(r#"[(id: "a", pictures: ()), (id: "a", pictures: ())]"#)?;
        assert_eq!(validate_wizard_art(&twice).len(), 1);
        Ok(())
    }
}
