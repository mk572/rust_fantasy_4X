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
//!
//! The layout lives here, with the battle; `f4x-core` gathers the facts from the game
//! (`World::battle_info`) and hands them to [`build`].
//!
//! Tests: none here, because `build` only lays facts out; the facts are read from real battles
//! in `f4x-core` (`battle/tests.rs`, `World::battle_info`).

use f4x_def::rules::realm::Realm;

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
    /// Nothing to show.
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
/// The help is the spell's own topic (198).
pub const EVIL_OMENS: InfoItem = item("battle_info.evil_omens", 198);

/// Every line the window can show; the client's `icon_art.ron` has a picture for each it draws one for.
pub const ALL: &[InfoItem] = &[
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

/// The wizard spells the window lists for one side.
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
