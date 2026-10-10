//! Creating a wizard: the pick budget, retort prerequisites and starting spells.
//!
//! The rules follow the original's custom-wizard screen: eleven picks buy spell books (one
//! each) and retorts (one each; two for Warlord, Channeler, Divine Power, Famous and Infernal
//! Power; three for Myrran), at most six retorts, and some retorts need books first. A realm
//! with `n` books lets the wizard choose `n - 1` common spells to start with.

use std::collections::{BTreeMap, BTreeSet};

use crate::city::PlayerId;
use crate::magic::{MagicError, Retort};
use crate::rules::{Rarity, SpellId};
use crate::Game;

/// Picks the wizard spends on books and retorts.
pub const PICKS: u8 = 11;
/// Most retorts a wizard may have.
pub const MAX_RETORTS: usize = 6;
/// Most books in one realm.
pub const MAX_BOOKS: u8 = 10;

/// What a wizard is made of, before the game starts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WizardBuild {
    pub name: String,
    /// Books by realm id.
    pub books: BTreeMap<String, u8>,
    pub retorts: BTreeSet<Retort>,
    /// Chosen starting spells; realms with fewer than `books - 1` choices are filled from
    /// the realm's defaults.
    pub spells: Vec<SpellId>,
    /// The banner colour chosen (0 to 4); `None` keeps the player's default.
    pub banner: Option<u8>,
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
    UnknownRealm(String),
    TooManyBooks(String),
    /// A chosen spell is not a common spell of one of the wizard's realms, or too many.
    BadSpell(SpellId),
    Magic(MagicError),
}

/// Picks a retort costs.
pub fn retort_cost(retort: Retort) -> u8 {
    match retort {
        Retort::Warlord
        | Retort::Channeler
        | Retort::DivinePower
        | Retort::Famous
        | Retort::InfernalPower => 2,
        Retort::Myrran => 3,
        _ => 1,
    }
}

/// Every retort, in the order the creation screen lists them.
pub const ALL_RETORTS: [Retort; 18] = [
    Retort::Alchemy,
    Retort::Warlord,
    Retort::Channeler,
    Retort::Archmage,
    Retort::Artificer,
    Retort::Conjurer,
    Retort::SageMaster,
    Retort::Myrran,
    Retort::DivinePower,
    Retort::Famous,
    Retort::Runemaster,
    Retort::Charismatic,
    Retort::ChaosMastery,
    Retort::NatureMastery,
    Retort::SorceryMastery,
    Retort::InfernalPower,
    Retort::ManaFocusing,
    Retort::NodeMastery,
];

/// Display name of a retort.
pub fn retort_name(retort: Retort) -> &'static str {
    match retort {
        Retort::Alchemy => "Alchemy",
        Retort::Warlord => "Warlord",
        Retort::ChaosMastery => "Chaos Mastery",
        Retort::NatureMastery => "Nature Mastery",
        Retort::SorceryMastery => "Sorcery Mastery",
        Retort::Channeler => "Channeler",
        Retort::Archmage => "Archmage",
        Retort::Artificer => "Artificer",
        Retort::Conjurer => "Conjurer",
        Retort::SageMaster => "Sage Master",
        Retort::Myrran => "Myrran",
        Retort::DivinePower => "Divine Power",
        Retort::InfernalPower => "Infernal Power",
        Retort::Famous => "Famous",
        Retort::Runemaster => "Runemaster",
        Retort::Charismatic => "Charismatic",
        Retort::ManaFocusing => "Mana Focusing",
        Retort::NodeMastery => "Node Mastery",
    }
}

/// The books a retort needs before it may be picked (the original's `wsa_picklist_start`
/// and the unlocks in its pick screen).
pub fn retort_available(retort: Retort, books: &BTreeMap<String, u8>) -> bool {
    let n = |realm: &str| books.get(realm).copied().unwrap_or(0);
    let realms = ["life", "death", "chaos", "nature", "sorcery"];
    let realms_with = |min: u8| realms.iter().filter(|r| n(r) >= min).count();
    let mastery_book = realms.iter().any(|r| n(r) >= 4);
    match retort {
        Retort::Alchemy
        | Retort::Warlord
        | Retort::Channeler
        | Retort::Artificer
        | Retort::Conjurer
        | Retort::Myrran
        | Retort::Famous
        | Retort::Charismatic => true,
        Retort::SageMaster => realms_with(1) >= 2,
        Retort::Runemaster => realms_with(2) >= 3,
        Retort::Archmage | Retort::ManaFocusing => mastery_book,
        Retort::ChaosMastery => n("chaos") >= 4,
        Retort::NatureMastery => n("nature") >= 4,
        Retort::SorceryMastery => n("sorcery") >= 4,
        Retort::InfernalPower => n("death") >= 4,
        Retort::DivinePower => n("life") >= 4,
        Retort::NodeMastery => n("chaos") >= 1 && n("sorcery") >= 1 && n("nature") >= 1,
    }
}

impl WizardBuild {
    /// Picks spent on books and retorts.
    pub fn picks_spent(&self) -> u8 {
        let books: u8 = self.books.values().sum();
        books + self.retorts.iter().map(|&r| retort_cost(r)).sum::<u8>()
    }

    /// Picks still to spend (negative if over budget).
    pub fn picks_left(&self) -> i16 {
        i16::from(PICKS) - i16::from(self.picks_spent())
    }

    /// Common spells the wizard may choose in a realm: one fewer than the books.
    pub fn spell_choices(&self, realm: &str) -> usize {
        usize::from(
            self.books
                .get(realm)
                .copied()
                .unwrap_or(0)
                .saturating_sub(1),
        )
    }
}

impl Game {
    /// Check a build against the pick rules (not the starting spells).
    pub fn check_wizard_build(&self, build: &WizardBuild) -> Result<(), BuildError> {
        for (realm, &n) in &build.books {
            if n > 0 && self.content.rules.realms.id_of(realm).is_none() {
                return Err(BuildError::UnknownRealm(realm.clone()));
            }
            if n > MAX_BOOKS {
                return Err(BuildError::TooManyBooks(realm.clone()));
            }
        }
        if build.preset {
            return Ok(());
        }
        if build.retorts.len() > MAX_RETORTS {
            return Err(BuildError::TooManyRetorts);
        }
        for &r in &build.retorts {
            if !retort_available(r, &build.books) {
                return Err(BuildError::RetortNotAvailable(r));
            }
        }
        let spent = build.picks_spent();
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

    /// The spells a realm offers at the start: its common spells in file order.
    pub fn starting_spell_pool(&self, realm: &str) -> Vec<SpellId> {
        self.content
            .rules
            .spells
            .iter()
            .filter(|(_, d)| d.realm == realm && d.rarity == Rarity::Common)
            .map(|(id, _)| id)
            .collect()
    }

    /// The realm's default starting spells that are common ones, in default order.
    pub fn default_starting_spells(&self, realm: &str) -> Vec<SpellId> {
        let Some(def) = self
            .content
            .rules
            .realms
            .defs()
            .iter()
            .find(|r| r.id == realm)
        else {
            return Vec::new();
        };
        let pool = self.starting_spell_pool(realm);
        def.default_spells
            .iter()
            .filter_map(|s| self.content.rules.spells.id_of(s))
            .filter(|s| pool.contains(s))
            .collect()
    }

    /// Set up `player` as the wizard `build` describes: books, retorts, casting skill and
    /// starting spells (chosen ones first, then defaults, then any common spell).
    pub fn create_wizard(
        &mut self,
        player: PlayerId,
        build: &WizardBuild,
    ) -> Result<(), BuildError> {
        self.check_wizard_build(build)?;
        // Chosen spells must be commons of a realm the wizard has books in, within the limit.
        let mut per_realm: BTreeMap<String, Vec<SpellId>> = BTreeMap::new();
        for &s in &build.spells {
            let def = self.content.rules.spells.get(s);
            let realm = def.realm.clone();
            let ok = def.rarity == Rarity::Common
                && build.spell_choices(&realm) > 0
                && !per_realm.get(&realm).is_some_and(|v| v.contains(&s));
            if !ok {
                return Err(BuildError::BadSpell(s));
            }
            let list = per_realm.entry(realm.clone()).or_default();
            if list.len() >= build.spell_choices(&realm) {
                return Err(BuildError::BadSpell(s));
            }
            list.push(s);
        }
        let mut start = Vec::new();
        for realm in build.books.keys() {
            let want = build.spell_choices(realm);
            let mut chosen = per_realm.remove(realm).unwrap_or_default();
            for s in self.default_starting_spells(realm) {
                if chosen.len() >= want {
                    break;
                }
                if !chosen.contains(&s) {
                    chosen.push(s);
                }
            }
            for s in self.starting_spell_pool(realm) {
                if chosen.len() >= want {
                    break;
                }
                if !chosen.contains(&s) {
                    chosen.push(s);
                }
            }
            start.extend(chosen);
        }
        let books: Vec<(&str, u8)> = build.books.iter().map(|(r, &n)| (r.as_str(), n)).collect();
        let retorts: Vec<Retort> = build.retorts.iter().copied().collect();
        self.init_wizard_with(player, &books, &retorts, Some(&start))
            .map_err(BuildError::Magic)?;
        if let Some(p) = self.player_mut(player) {
            if !build.name.is_empty() {
                p.name = build.name.clone();
            }
            if let Some(b) = build.banner {
                p.banner = b.min(crate::player::BANNER_CHOICES - 1);
            }
        }
        Ok(())
    }

    /// A ready-made wizard from the rules as a build.
    pub fn preset_build(&self, id: &str) -> Option<WizardBuild> {
        let def = self
            .content
            .rules
            .wizards
            .defs()
            .iter()
            .find(|w| w.id == id)?;
        Some(WizardBuild {
            name: def.name.clone(),
            books: def.books.iter().cloned().collect(),
            retorts: def.retorts.iter().copied().collect(),
            spells: Vec::new(),
            banner: None,
            preset: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::Player;
    use crate::testutil::content_with_magic;

    const P0: PlayerId = PlayerId(0);

    fn game() -> Game {
        let mut g = Game::new(5);
        g.content = content_with_magic();
        g.players.push(Player::new(P0, "W"));
        g
    }

    fn build(books: &[(&str, u8)], retorts: &[Retort]) -> WizardBuild {
        WizardBuild {
            name: "Test".into(),
            books: books.iter().map(|&(r, n)| (r.to_string(), n)).collect(),
            retorts: retorts.iter().copied().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn retorts_cost_one_two_or_three_picks() {
        assert_eq!(retort_cost(Retort::Alchemy), 1);
        for r in [
            Retort::Warlord,
            Retort::Channeler,
            Retort::DivinePower,
            Retort::Famous,
            Retort::InfernalPower,
        ] {
            assert_eq!(retort_cost(r), 2, "{r:?}");
        }
        assert_eq!(retort_cost(Retort::Myrran), 3);
        assert_eq!(ALL_RETORTS.len(), 18);
    }

    #[test]
    fn some_retorts_need_books() {
        let none = BTreeMap::new();
        let mut b: BTreeMap<String, u8> = BTreeMap::new();
        assert!(retort_available(Retort::Alchemy, &none));
        assert!(!retort_available(Retort::Archmage, &none));
        assert!(!retort_available(Retort::SageMaster, &none));
        b.insert("life".into(), 4);
        assert!(retort_available(Retort::DivinePower, &b));
        assert!(retort_available(Retort::Archmage, &b));
        assert!(retort_available(Retort::ManaFocusing, &b));
        assert!(!retort_available(Retort::InfernalPower, &b));
        assert!(!retort_available(Retort::SageMaster, &b), "one realm only");
        b.insert("death".into(), 1);
        assert!(retort_available(Retort::SageMaster, &b));
        assert!(!retort_available(Retort::Runemaster, &b));
        b.insert("death".into(), 2);
        b.insert("chaos".into(), 2);
        b.insert("life".into(), 2);
        assert!(
            retort_available(Retort::Runemaster, &b),
            "two books in three realms"
        );
        assert!(!retort_available(Retort::NodeMastery, &b));
        for r in ["chaos", "sorcery", "nature"] {
            b.insert(r.into(), 1);
        }
        assert!(retort_available(Retort::NodeMastery, &b));
    }

    #[test]
    fn a_wizard_spends_exactly_eleven_picks() {
        let g = game();
        assert_eq!(
            g.check_wizard_build(&build(&[("life", 5), ("chaos", 5)], &[Retort::Alchemy])),
            Ok(())
        );
        assert_eq!(
            g.check_wizard_build(&build(&[("life", 5), ("chaos", 5)], &[])),
            Err(BuildError::UnspentPicks { left: 1 })
        );
        assert_eq!(
            g.check_wizard_build(&build(&[("life", 6), ("chaos", 5)], &[Retort::Alchemy])),
            Err(BuildError::TooManyPicks { spent: 12 })
        );
        assert_eq!(
            g.check_wizard_build(&build(&[("life", 9)], &[Retort::Warlord])),
            Ok(()),
            "warlord costs two"
        );
        assert_eq!(
            g.check_wizard_build(&build(&[("life", 3), ("chaos", 3)], &[Retort::Archmage])),
            Err(BuildError::RetortNotAvailable(Retort::Archmage))
        );
        assert_eq!(
            g.check_wizard_build(&build(&[("magic", 11)], &[])),
            Err(BuildError::UnknownRealm("magic".into()))
        );
    }

    #[test]
    fn at_most_six_retorts() {
        let g = game();
        let seven = [
            Retort::Alchemy,
            Retort::Conjurer,
            Retort::Artificer,
            Retort::Charismatic,
            Retort::Warlord,
            Retort::Famous,
            Retort::Channeler,
        ];
        assert_eq!(
            g.check_wizard_build(&build(&[], &seven)),
            Err(BuildError::TooManyRetorts)
        );
    }

    #[test]
    fn starting_spells_are_chosen_then_defaulted() {
        let mut g = game();
        let (c3, c7) = (
            g.content.rules.spells.id_of("life_c3").unwrap(),
            g.content.rules.spells.id_of("life_c7").unwrap(),
        );
        let mut b = build(&[("life", 4), ("chaos", 6)], &[Retort::Alchemy]);
        b.spells = vec![c7, c3];
        g.create_wizard(P0, &b).unwrap();
        let m = g.magic(P0).unwrap();
        // Four life books: three commons; two were chosen, one is filled in.
        let life: Vec<_> = m
            .known
            .iter()
            .filter(|&&s| g.content.rules.spells.get(s).realm == "life")
            .collect();
        assert_eq!(life.len(), 3);
        assert!(m.known.contains(&c7) && m.known.contains(&c3));
        // Six chaos books: five commons, all from the pool.
        let chaos = m
            .known
            .iter()
            .filter(|&&s| g.content.rules.spells.get(s).realm == "chaos")
            .count();
        assert_eq!(chaos, 5);
        assert_eq!(g.player(P0).unwrap().name, "Test");
    }

    #[test]
    fn bad_spell_choices_are_refused() {
        let mut g = game();
        let uncommon = g.content.rules.spells.id_of("life_u0").unwrap();
        let common = g.content.rules.spells.id_of("life_c0").unwrap();
        let other = g.content.rules.spells.id_of("chaos_c0").unwrap();
        let mut b = build(&[("life", 5), ("chaos", 5)], &[Retort::Alchemy]);
        b.spells = vec![uncommon];
        assert_eq!(g.create_wizard(P0, &b), Err(BuildError::BadSpell(uncommon)));
        b.spells = vec![common, common];
        assert_eq!(g.create_wizard(P0, &b), Err(BuildError::BadSpell(common)));
        let mut one_book = build(&[("life", 1), ("chaos", 9)], &[Retort::Alchemy]);
        one_book.spells = vec![common];
        assert_eq!(
            g.create_wizard(P0, &one_book),
            Err(BuildError::BadSpell(common)),
            "one book leaves no choices"
        );
        b.spells = vec![other, common];
        assert!(g.create_wizard(P0, &b).is_ok());
    }

    #[test]
    fn a_wizard_chooses_a_banner_and_it_is_saved() {
        let mut g = game();
        let mut b = build(&[("life", 5), ("chaos", 5)], &[Retort::Alchemy]);
        assert_eq!(g.player(P0).unwrap().banner, 0, "player 0 starts blue");
        b.banner = Some(3);
        g.create_wizard(P0, &b).unwrap();
        assert_eq!(g.player(P0).unwrap().banner, 3);
        b.banner = Some(9);
        g.create_wizard(P0, &b).unwrap();
        assert_eq!(
            g.player(P0).unwrap().banner,
            4,
            "kept within the five choices"
        );
        let content = g.content.clone();
        let back = g.snapshot().restore(content).unwrap();
        assert_eq!(back.player(P0).unwrap().banner, 4);
        assert_eq!(
            Player::new(PlayerId(u8::MAX), "n").banner,
            crate::player::NEUTRAL_BANNER
        );
    }

    #[test]
    fn presets_come_from_the_rules() {
        let mut g = game();
        g.content.rules.wizards = crate::rules::Registry::new(vec![crate::rules::WizardDef {
            id: "lo_pan".into(),
            name: "Lo Pan".into(),
            description: String::new(),
            books: vec![("life".into(), 5), ("chaos".into(), 5)],
            retorts: vec![Retort::Channeler],
        }]);
        let b = g.preset_build("lo_pan").unwrap();
        assert!(b.preset);
        assert_eq!(b.picks_spent(), 12, "the original's Lo Pan is over budget");
        g.create_wizard(P0, &b).unwrap();
        assert!(g.magic(P0).unwrap().has(Retort::Channeler));
        assert!(g.preset_build("nobody").is_none());
    }
}
