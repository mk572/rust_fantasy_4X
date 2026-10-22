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

    pub(crate) fn pick_preset(&mut self, index: Option<usize>) {
        match index {
            Some(i) => {
                let id = self.game.content().rules.wizards.defs()[i].id.clone();
                if let Some(build) = self.game.preset_build(&id) {
                    self.create.build = build;
                    self.create.preset = Some(i);
                }
            }
            None => {
                // "Custom" keeps what is there but under the full pick rules.
                self.create.build.preset = false;
                self.create.preset = None;
            }
        }
        self.create.build.spells.clear();
        self.refill_spells();
    }

    /// Keep the chosen starting spells within what the books allow, filling any gaps with
    /// the realm's defaults so the list always shows what the wizard would start with.
    fn refill_spells(&mut self) {
        let realms = self.book_realms();
        let mut spells: Vec<SpellId> = Vec::new();
        for realm in &realms {
            for rarity in [Rarity::Common, Rarity::Uncommon, Rarity::Rare] {
                let want = self.create.build.spell_choices_of(
                    &self.game.content().settings.spell_library,
                    realm,
                    rarity,
                );
                let mut chosen: Vec<SpellId> = self
                    .create
                    .build
                    .spells
                    .iter()
                    .copied()
                    .filter(|&s| {
                        let d = self.game.content().rules.spells.get(s);
                        d.realm == *realm && d.rarity == rarity
                    })
                    .collect();
                chosen.truncate(want);
                for s in f4x_core::wizard::magic::default_spells(self.game.content(), realm, rarity)
                    .into_iter()
                    .chain(f4x_core::wizard::magic::spells_of(
                        self.game.content(),
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
        self.create.build.spells = spells;
    }

    pub(crate) fn begin_game(&mut self) {
        let mut build = self.create.build.clone();
        build.name = if build.name.trim().is_empty() {
            "You".into()
        } else {
            build.name.trim().to_string()
        };
        self.human_build = Some(build);
        self.new_map(self.seed);
    }
}
