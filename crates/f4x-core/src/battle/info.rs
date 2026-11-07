//! The combat information window: every global and battlefield enchantment in force, split
//! between the two wizards (`Combat_Information_Window`, `Combat_Info_Effects_Base` and
//! `Combat_Info_Effects` in ReMoM `Combat.c`).
//!
//! The first section holds what belongs to the field itself: the node's dispelling and aura,
//! the besieged city's Cloud of Shadow and Heavenly Light, and the two spells that touch every
//! fight, Chaos Surge and Eternal Night. Under it each wizard's own spells follow: Crusade,
//! Holy Arms, Charm of Life and Zombie Mastery, the opponent's on the left and the viewer's on
//! the right.
//!
//! Divergence: the original lists neither Evil Omens nor Spell Wards (`OGBUG: INCONSISTENT`);
//! Evil Omens is shown here, among the common effects, since it changes what every spell
//! costs. The original's drawing loop stops short of the per-side cells (its bound is half the
//! rows); the window here shows all of them.

use super::immunity::Realm;
use super::Battle;
use crate::game::Game;
use crate::player::PlayerId;

/// One line of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoItem {
    /// The key of the name the window prints, in `content/text`.
    pub key: &'static str,
    /// The help topic (the original's `HLP_` number) a right-click opens.
    pub help: u16,
}

/// What the window shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BattleInfo {
    /// The field's effects, whoever's they are.
    pub common: Vec<InfoItem>,
    /// The opponent's own spells (left column) and its wizard's name, if it has one.
    pub opponent: Vec<InfoItem>,
    pub opponent_name: Option<String>,
    /// The viewer's own spells (right column) and name.
    pub viewer: Vec<InfoItem>,
    pub viewer_name: Option<String>,
}

impl BattleInfo {
    /// Nothing to show: the original then leaves the Info button locked.
    pub fn is_empty(&self) -> bool {
        self.common.is_empty() && self.opponent.is_empty() && self.viewer.is_empty()
    }

    /// The rows each side's list needs: the longer of the two.
    pub fn side_rows(&self) -> usize {
        self.opponent.len().max(self.viewer.len())
    }
}

const fn item(key: &'static str, help: u16) -> InfoItem {
    InfoItem { key, help }
}

pub const CRUSADE: InfoItem = item("battle_info.crusade", 622);
pub const HOLY_ARMS: InfoItem = item("battle_info.holy_arms", 623);
pub const CHARM_OF_LIFE: InfoItem = item("battle_info.charm_of_life", 624);
pub const ZOMBIE_MASTERY: InfoItem = item("battle_info.zombie_mastery", 625);
pub const DISPELS_NON_SORCERY: InfoItem = item("battle_info.dispels_non_sorcery", 612);
pub const DISPELS_NON_CHAOS: InfoItem = item("battle_info.dispels_non_chaos", 613);
pub const DISPELS_NON_NATURE: InfoItem = item("battle_info.dispels_non_nature", 614);
pub const SORCERY_AURA: InfoItem = item("battle_info.sorcery_node_aura", 615);
pub const CHAOS_AURA: InfoItem = item("battle_info.chaos_node_aura", 616);
pub const NATURE_AURA: InfoItem = item("battle_info.nature_node_aura", 617);
pub const CLOUD_OF_DARKNESS: InfoItem = item("battle_info.cloud_of_darkness", 618);
pub const HOLY_LIGHT: InfoItem = item("battle_info.holy_light", 619);
pub const CHAOS_SURGE: InfoItem = item("battle_info.chaos_surge", 620);
pub const ETERNAL_NIGHT: InfoItem = item("battle_info.eternal_night", 621);
/// Not in the original's window; the help is the spell's own topic (198).
pub const EVIL_OMENS: InfoItem = item("battle_info.evil_omens", 198);

/// Every line the window can show; the client's `icon_art.ron` has a picture for each it draws one for.
pub const ALL: [InfoItem; 15] = [
    CRUSADE,
    HOLY_ARMS,
    CHARM_OF_LIFE,
    ZOMBIE_MASTERY,
    DISPELS_NON_SORCERY,
    DISPELS_NON_CHAOS,
    DISPELS_NON_NATURE,
    SORCERY_AURA,
    CHAOS_AURA,
    NATURE_AURA,
    CLOUD_OF_DARKNESS,
    HOLY_LIGHT,
    CHAOS_SURGE,
    ETERNAL_NIGHT,
    EVIL_OMENS,
];

/// The wizard spells the window lists for one side, in the original's order.
#[derive(Debug, Clone, Copy, Default)]
pub struct WizardSpells {
    pub crusade: bool,
    pub holy_arms: bool,
    pub charm_of_life: bool,
    pub zombie_mastery: bool,
}

impl WizardSpells {
    fn items(self) -> Vec<InfoItem> {
        let mut out = Vec::new();
        for (on, i) in [
            (self.crusade, CRUSADE),
            (self.holy_arms, HOLY_ARMS),
            (self.charm_of_life, CHARM_OF_LIFE),
            (self.zombie_mastery, ZOMBIE_MASTERY),
        ] {
            if on {
                out.push(i);
            }
        }
        out
    }
}

/// What the window is built from.
#[derive(Debug, Clone, Default)]
pub struct InfoFacts {
    /// The realm of the node the fight is on (its centre structure) and of the node whose aura
    /// covers the square.
    pub node_site: Option<Realm>,
    pub node_aura: Option<Realm>,
    /// The besieged city, one the viewer does not own, has these.
    pub cloud_of_shadow: bool,
    pub heavenly_light: bool,
    pub chaos_surge: bool,
    pub eternal_night: bool,
    pub evil_omens: bool,
    pub opponent: Option<(String, WizardSpells)>,
    pub viewer: Option<(String, WizardSpells)>,
}

/// Lay the facts out as the window does.
pub fn build(facts: InfoFacts) -> BattleInfo {
    let mut common = Vec::new();
    match facts.node_site {
        Some(Realm::Sorcery) => common.push(DISPELS_NON_SORCERY),
        Some(Realm::Chaos) => common.push(DISPELS_NON_CHAOS),
        Some(Realm::Nature) => common.push(DISPELS_NON_NATURE),
        _ => {}
    }
    match facts.node_aura {
        Some(Realm::Sorcery) => common.push(SORCERY_AURA),
        Some(Realm::Nature) => common.push(NATURE_AURA),
        Some(Realm::Chaos) => common.push(CHAOS_AURA),
        _ => {}
    }
    for (on, i) in [
        (facts.cloud_of_shadow, CLOUD_OF_DARKNESS),
        (facts.heavenly_light, HOLY_LIGHT),
        (facts.chaos_surge, CHAOS_SURGE),
        (facts.eternal_night, ETERNAL_NIGHT),
        (facts.evil_omens, EVIL_OMENS),
    ] {
        if on {
            common.push(i);
        }
    }
    let (opponent_name, opponent) = split(facts.opponent);
    let (viewer_name, viewer) = split(facts.viewer);
    BattleInfo {
        common,
        opponent,
        opponent_name,
        viewer,
        viewer_name,
    }
}

fn split(side: Option<(String, WizardSpells)>) -> (Option<String>, Vec<InfoItem>) {
    match side {
        Some((name, spells)) => (Some(name), spells.items()),
        None => (None, Vec::new()),
    }
}

impl Game {
    /// The combat information window for `viewer`, a wizard in the battle being fought.
    pub fn battle_info(&self, viewer: PlayerId) -> Option<BattleInfo> {
        let battle: &Battle = self.battle()?;
        let opponent = if battle.attacker == viewer {
            battle.defender
        } else {
            battle.attacker
        };
        let wizard = |p: PlayerId| {
            if self.is_neutral_player(p) {
                return None;
            }
            let name = self.wizard(p)?.name();
            Some((
                name,
                WizardSpells {
                    crusade: self.has_global_effect(
                        p,
                        crate::spell::global_enchantment::GlobalEffect::Crusade,
                    ),
                    holy_arms: self.has_global_effect(
                        p,
                        crate::spell::global_enchantment::GlobalEffect::HolyArms,
                    ),
                    charm_of_life: self.has_global_effect(
                        p,
                        crate::spell::global_enchantment::GlobalEffect::CharmOfLife,
                    ),
                    zombie_mastery: self.has_global_effect(
                        p,
                        crate::spell::global_enchantment::GlobalEffect::ZombieMastery,
                    ),
                },
            ))
        };
        // The city's own spells are listed only against a city that is not the viewer's: the
        // viewer sees theirs among their own enchantments.
        let city = battle
            .site
            .and_then(|s| self.economy().city_at(s))
            .filter(|&c| self.economy().city_owner(c) != Some(viewer));
        let on_city = |effect: crate::spell::city::CityEffect| {
            city.is_some_and(|c| self.economy().city_has_effect(c, effect))
        };
        Some(build(InfoFacts {
            node_site: battle.node_site,
            node_aura: battle.node_aura,
            cloud_of_shadow: on_city(crate::spell::city::CityEffect::CloudOfShadow),
            heavenly_light: on_city(crate::spell::city::CityEffect::HeavenlyLight),
            chaos_surge: self
                .any_global_effect(crate::spell::global_enchantment::GlobalEffect::ChaosSurge),
            eternal_night: self
                .any_global_effect(crate::spell::global_enchantment::GlobalEffect::EternalNight),
            evil_omens: self
                .any_global_effect(crate::spell::global_enchantment::GlobalEffect::EvilOmens),
            opponent: wizard(opponent),
            viewer: wizard(viewer),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spells(crusade: bool, zombie: bool) -> Option<(String, WizardSpells)> {
        Some((
            "Merlin".into(),
            WizardSpells {
                crusade,
                zombie_mastery: zombie,
                ..Default::default()
            },
        ))
    }

    #[test]
    fn nothing_in_force_leaves_the_window_empty() {
        let info = build(InfoFacts {
            opponent: spells(false, false),
            viewer: spells(false, false),
            ..Default::default()
        });
        assert!(info.is_empty());
        assert_eq!(info.side_rows(), 0);
    }

    #[test]
    fn the_field_comes_first_in_the_originals_order() {
        let info = build(InfoFacts {
            node_site: Some(Realm::Chaos),
            node_aura: Some(Realm::Chaos),
            cloud_of_shadow: true,
            heavenly_light: true,
            chaos_surge: true,
            eternal_night: true,
            evil_omens: true,
            ..Default::default()
        });
        let names: Vec<_> = info.common.iter().map(|i| i.key).collect();
        assert_eq!(
            names,
            [
                "battle_info.dispels_non_chaos",
                "battle_info.chaos_node_aura",
                "battle_info.cloud_of_darkness",
                "battle_info.holy_light",
                "battle_info.chaos_surge",
                "battle_info.eternal_night",
                "battle_info.evil_omens",
            ]
        );
        assert_eq!(info.common[0], DISPELS_NON_CHAOS);
        assert_eq!(info.common[1].help, 616);
    }

    #[test]
    fn each_wizards_spells_go_on_their_own_side() {
        let info = build(InfoFacts {
            opponent: spells(true, false),
            viewer: spells(true, true),
            ..Default::default()
        });
        assert_eq!(info.opponent.len(), 1);
        assert_eq!(info.viewer.len(), 2);
        assert_eq!(info.viewer[1], ZOMBIE_MASTERY);
        assert_eq!(info.side_rows(), 2);
        assert_eq!(info.opponent_name.as_deref(), Some("Merlin"));
    }

    #[test]
    fn a_side_without_a_wizard_has_no_heading() {
        let info = build(InfoFacts {
            viewer: spells(false, false),
            ..Default::default()
        });
        assert_eq!(info.opponent_name, None);
        assert!(info.viewer_name.is_some());
    }
}
