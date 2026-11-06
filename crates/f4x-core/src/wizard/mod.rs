//! Creating a wizard: the pick budget, retort prerequisites and starting spells.
//!
//! The rules follow the original's custom-wizard screen: eleven picks buy spell books (one
//! each) and retorts (one each; two for Warlord, Channeler, Divine Power, Famous and Infernal
//! Power; three for Myrran), at most six retorts, and some retorts need books first. A realm
//! with `n` books lets the wizard choose `n - 1` common spells to start with; with eleven
//! (all the picks) it knows all ten common spells and chooses two uncommon and one rare.
//!
//! Tests: in wizard/tests.rs.

pub(crate) mod ai;
pub mod astrology;
pub mod entity;
pub mod fortress;
pub mod magic;
pub(crate) mod magic_command;
pub(crate) mod magic_game;
pub mod magic_income;
pub mod objective;
pub mod sheet;

use std::collections::{BTreeMap, BTreeSet};

use crate::game::setting::SpellLibraryDef;
use crate::game::Game;
use crate::player::PlayerId;
use crate::rules::{Rarity, RealmId, Rules, SpellId};
use crate::wizard::magic::{MagicError, Retort};

pub use f4x_def::settings::PICKS;
/// Most retorts a wizard may have.
pub const MAX_RETORTS: usize = 6;
/// Most books in one realm.
pub const MAX_BOOKS: u8 = 11;

/// What a wizard is made of, before the game starts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WizardBuild {
    pub name: String,
    /// For the messages that name the wizard (`docs/text.md`).
    pub gender: crate::text::Gender,
    /// Books by realm id.
    pub books: BTreeMap<RealmId, u8>,
    pub retorts: BTreeSet<Retort>,
    /// Chosen starting spells; realms with fewer than `books - 1` choices are filled from
    /// the realm's defaults.
    pub spells: Vec<SpellId>,
    /// The banner colour chosen (0 to 4); `None` keeps the player's default.
    pub banner: Option<u8>,
    /// The wizard picture chosen (0 to 13); `None` keeps the player's default. A ready-made
    /// wizard has the picture of its place in the rules' list, which is the original's order.
    pub portrait: Option<u8>,
    /// A ready-made wizard: exempt from the pick budget and retort prerequisites (the
    /// original's own Lo Pan spends twelve picks).
    pub preset: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// More than eleven picks spent.
    TooManyPicks {
        spent: u8,
    },
    /// Picks left over: a wizard must spend all of them.
    UnspentPicks {
        left: u8,
    },
    TooManyRetorts,
    /// The retort's prerequisites are not met.
    RetortNotAvailable(Retort),
    UnknownRealm(RealmId),
    TooManyBooks(RealmId),
    /// A chosen spell is not a common spell of one of the wizard's realms, or too many.
    BadSpell(SpellId),
    Magic(MagicError),
}

/// Picks a retort costs (its row in `retorts.ron`).
pub fn retort_cost(rules: &Rules, retort: Retort) -> u8 {
    rules.retorts.get(retort).cost
}

/// Every retort, in numeric order (the original's list): the creation screen, the AI and every
/// loop over the retorts go in this order.
pub fn all_retorts(rules: &Rules) -> impl Iterator<Item = Retort> + '_ {
    rules.retorts.iter().map(|(id, _)| id)
}

/// The books a retort needs before it may be picked (its `requires` in `retorts.ron`: the
/// original's `wsa_picklist_start` and the unlocks in its pick screen).
pub fn retort_available(rules: &Rules, retort: Retort, books: &BTreeMap<RealmId, u8>) -> bool {
    rules.retorts.get(retort).available(books)
}

impl WizardBuild {
    /// Picks spent on books and retorts.
    pub fn picks_spent(&self, rules: &Rules) -> u8 {
        let books: u8 = self.books.values().sum();
        books
            + self
                .retorts
                .iter()
                .map(|&r| retort_cost(rules, r))
                .sum::<u8>()
    }

    /// Picks still to spend (negative if over budget).
    pub fn picks_left(&self, rules: &Rules) -> i16 {
        i16::from(PICKS) - i16::from(self.picks_spent(rules))
    }

    /// Spells of a rarity the wizard may choose in a realm: the `chosen` row of the library
    /// table for the realm's books. The rest of what the wizard knows are defaults.
    pub fn spell_choices_of(
        &self,
        library: &SpellLibraryDef,
        realm: RealmId,
        rarity: Rarity,
    ) -> usize {
        let row = SpellLibraryDef::row(
            &library.chosen,
            self.books.get(&realm).copied().unwrap_or(0),
        );
        usize::from(match rarity {
            Rarity::Common => row.common,
            Rarity::Uncommon => row.uncommon,
            Rarity::Rare => row.rare,
            Rarity::VeryRare => row.very_rare,
        })
    }
}

impl Game {
    /// Check a build against the pick rules (not the starting spells).
    pub fn check_wizard_build(&self, build: &WizardBuild) -> Result<(), BuildError> {
        for (&realm, &n) in &build.books {
            if n > 0 && self.content.rules.realms.id_numbered(realm.num()).is_none() {
                return Err(BuildError::UnknownRealm(realm));
            }
            if n > MAX_BOOKS {
                return Err(BuildError::TooManyBooks(realm));
            }
        }
        if build.preset {
            return Ok(());
        }
        if build.retorts.len() > MAX_RETORTS {
            return Err(BuildError::TooManyRetorts);
        }
        for &r in &build.retorts {
            if !retort_available(&self.content.rules, r, &build.books) {
                return Err(BuildError::RetortNotAvailable(r));
            }
        }
        let spent = build.picks_spent(&self.content.rules);
        if spent > PICKS {
            return Err(BuildError::TooManyPicks { spent });
        }
        if spent < PICKS {
            return Err(BuildError::UnspentPicks {
                left: PICKS - spent,
            });
        }
        Ok(())
    }

    /// Set up `player` as the wizard `build` describes: books, retorts, casting skill and
    /// starting spells (chosen ones first, then defaults, then any common spell).
    pub fn create_wizard(
        &mut self,
        player: PlayerId,
        build: &WizardBuild,
    ) -> Result<(), BuildError> {
        self.check_wizard_build(build)?;
        // Chosen spells must be of a realm the wizard has books in, and of a rarity and number
        // the books allow.
        let mut per_realm: BTreeMap<(Option<RealmId>, Rarity), Vec<SpellId>> = BTreeMap::new();
        for &s in &build.spells {
            let def = self.content.rules.spells.get(s);
            let realm = Some(def.realm);
            let key = (realm, def.rarity);
            let room = realm.map_or(0, |realm| {
                build.spell_choices_of(&self.content.settings.spell_library, realm, def.rarity)
            });
            let list = per_realm.entry(key).or_default();
            if list.contains(&s) || list.len() >= room {
                return Err(BuildError::BadSpell(s));
            }
            list.push(s);
        }
        let books: Vec<(RealmId, u8)> = build.books.iter().map(|(&r, &n)| (r, n)).collect();
        let retorts: Vec<Retort> = build.retorts.iter().copied().collect();
        self.init_wizard(player, &books, &retorts, Some(&build.spells))
            .map_err(BuildError::Magic)?;
        if !build.name.is_empty() {
            self.wizard_set(player, entity::Name(build.name.clone()));
        }
        self.wizard_set(player, build.gender);
        if let Some(b) = build.banner {
            self.wizard_set(
                player,
                entity::Banner(b.min(crate::player::BANNER_CHOICES - 1)),
            );
        }
        if let Some(b) = build.portrait {
            self.wizard_set(
                player,
                entity::Portrait(b.min(crate::player::PORTRAITS - 1)),
            );
        }
        Ok(())
    }

    /// A ready-made wizard from the rules as a build.
    pub fn preset_build(&self, num: u16) -> Option<WizardBuild> {
        let place = self
            .content
            .rules
            .wizards
            .defs()
            .iter()
            .position(|w| w.num == num)?;
        let def = &self.content.rules.wizards.defs()[place];
        let retorts = def.retorts.iter().copied().collect();
        Some(WizardBuild {
            name: String::new(),
            gender: def.gender,
            books: def.books.iter().copied().collect(),
            retorts,
            spells: Vec::new(),
            banner: None,
            portrait: u8::try_from(place)
                .ok()
                .filter(|&p| p < crate::player::PORTRAITS),
            preset: true,
        })
    }
}

#[cfg(test)]
mod tests;
