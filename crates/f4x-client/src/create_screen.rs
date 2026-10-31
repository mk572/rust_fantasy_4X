//! The state of the wizard creation steps and the helpers they share. The steps themselves are
//! `newgame_flow.rs` (wizard, picture, name, race, banner) and `newgame_picks.rs` (picks and
//! spells); a ready-made wizard skips the picture, picks and spells.

use f4x_core::rules::{Rarity, SpellId};
use f4x_core::wizard::WizardBuild;

use crate::view::App;

/// Where the creation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Select Wizard: the fourteen ready-made ones, or Custom.
    Wizard,
    /// Select Picture (a custom wizard).
    Portrait,
    /// The wizard's name.
    Name,
    /// The books and retorts (a custom wizard).
    Picks,
    /// The starting spells of the realm at this place of `REALM_ORDER` (a custom wizard).
    Spells(usize),
    Race,
    Banner,
}

/// What the player has set up so far.
pub(crate) struct CreateState {
    /// Index into the rules' ready-made wizards; `None` once edited or for "Custom".
    pub preset: Option<usize>,
    pub build: WizardBuild,
    pub step: Step,
    /// A custom wizard goes through the picture, picks and spells steps.
    pub custom: bool,
    /// The wizard under the mouse on the lists (14 and up: "Custom").
    pub hover: usize,
    /// How tall each book on the shelf and in the rows is drawn (`Randomize_Book_Heights`).
    pub shelf: [u8; 11],
    pub rows: [u8; 11],
    /// A message in the warning box, which takes the input until it is closed.
    pub warn: Option<String>,
    /// A book drag whose refusal was just closed goes on being ignored until the button is let go
    /// (or the same refusal would come up again every frame).
    pub ignore_drag: bool,
    /// The race picked on the race step (an index into the rules' races), until OK is clicked.
    pub race_pick: Option<usize>,
}

impl App {
    /// Realm ids that hold spell books, in the rules' order (arcane has none).
    pub(crate) fn book_realms(&self) -> Vec<String> {
        self.game
            .content()
            .rules
            .realms
            .defs()
            .iter()
            .filter(|r| r.id != "arcane")
            .map(|r| r.id.clone())
            .collect()
    }
}
impl CreateState {
    pub(crate) fn pick_preset(&mut self, app: &mut App, index: Option<usize>) {
        match index {
            Some(i) => {
                let id = app.game.content().rules.wizards.defs()[i].id.clone();
                if let Some(build) = app.game.preset_build(&id) {
                    self.build = build;
                    self.preset = Some(i);
                }
            }
            None => {
                // "Custom" keeps what is there but under the full pick rules.
                self.build.preset = false;
                self.preset = None;
            }
        }
        self.build.spells.clear();
        self.refill_spells(app);
    }

    /// Keep the chosen starting spells within what the books allow, filling any gaps with
    /// the realm's defaults so the list always shows what the wizard would start with.
    fn refill_spells(&mut self, app: &mut App) {
        let realms = app.book_realms();
        let mut spells: Vec<SpellId> = Vec::new();
        for realm in &realms {
            for rarity in [Rarity::Common, Rarity::Uncommon, Rarity::Rare] {
                let want = self.build.spell_choices_of(
                    &app.game.content().settings.spell_library,
                    realm,
                    rarity,
                );
                let mut chosen: Vec<SpellId> = self
                    .build
                    .spells
                    .iter()
                    .copied()
                    .filter(|&s| {
                        let d = app.game.content().rules.spells.get(s);
                        d.realm == *realm && d.rarity == rarity
                    })
                    .collect();
                chosen.truncate(want);
                for s in f4x_core::wizard::magic::default_spells(app.game.content(), realm, rarity)
                    .into_iter()
                    .chain(f4x_core::wizard::magic::spells_of(
                        app.game.content(),
                        realm,
                        rarity,
                    ))
                {
                    if chosen.len() >= want {
                        break;
                    }
                    if !chosen.contains(&s) {
                        chosen.push(s);
                    }
                }
                spells.extend(chosen);
            }
        }
        self.build.spells = spells;
    }

    pub(crate) fn begin_game(&mut self, app: &mut App) {
        let mut build = self.build.clone();
        build.name = if build.name.trim().is_empty() {
            app.text("create.default_name")
        } else {
            build.name.trim().to_string()
        };
        app.human_build = Some(build);
        app.new_map(app.seed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_realms_of_magic_hold_books_and_arcane_does_not() {
        let app = App::for_test();
        let mut realms = app.book_realms();
        realms.sort();
        assert_eq!(realms, ["chaos", "death", "life", "nature", "sorcery"]);
    }

    #[test]
    fn a_ready_made_wizard_brings_its_books_and_spells_and_custom_keeps_them_under_the_full_rules()
    {
        let mut app = App::for_test();
        let mut state = CreateState::empty();
        state.pick_preset(&mut app, Some(0));
        assert_eq!(state.preset, Some(0));
        assert!(state.build.preset);
        let realms = app.book_realms();
        assert!(!state.build.spells.is_empty());
        for &s in &state.build.spells {
            let realm = &app.game.content().rules.spells.get(s).realm;
            assert!(realms.contains(realm), "{realm} holds no books");
        }
        let spells = state.build.spells.clone();
        state.pick_preset(&mut app, None);
        assert_eq!(state.preset, None);
        assert!(!state.build.preset);
        assert_eq!(
            state.build.spells, spells,
            "the same books call for the same spells"
        );
    }

    #[test]
    fn a_wizard_without_a_name_gets_the_default_one() {
        let mut app = App::for_test();
        let mut state = CreateState::empty();
        state.build.name = "   ".into();
        state.begin_game(&mut app);
        let default = app.text("create.default_name");
        assert_eq!(
            app.human_build.as_ref().map(|b| b.name.as_str()),
            Some(default.as_str())
        );
        state.build.name = "  Merlin ".into();
        state.begin_game(&mut app);
        assert_eq!(
            app.human_build.as_ref().map(|b| b.name.as_str()),
            Some("Merlin")
        );
    }
}
