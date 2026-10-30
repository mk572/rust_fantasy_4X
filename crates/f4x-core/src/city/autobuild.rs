//! The city auto-builder (PLANNED item 7; the rules and every weight are in `docs/auto-builder.md`).
//!
//! A city can have a [`BuildPlan`]: what to build (units, buildings or both) and what matters
//! (a primary focus and perhaps a secondary one). When the city finishes an item, [`choose`]
//! picks the next from the plan and a snapshot of the city ([`CityFacts`]); the choice is a pure
//! function of those two and the game's random numbers, so it is tested on fixed cities. The
//! scripted and learning AI set plans through the same data.
//!
//! Tests: in city/autobuild/tests.rs.

use serde::{Deserialize, Serialize};

use crate::city::production::ProductionItem;
use crate::content::Content;
use crate::game::Game;
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::effects::Effect;
use crate::rules::{BuildingId, ModOp, MovementType, Stat, UnitId, UnitKind};

/// Net gold of the treasury under which the city builds gold first.
pub const GOLD_FLOOR: i32 = 3;
/// The share of the primary and the secondary focus when there are two.
pub const PRIMARY_WEIGHT: f32 = 0.65;
pub const SECONDARY_WEIGHT: f32 = 0.35;
/// A city this close to the largest population its land feeds wants settlers (and food).
pub const NEAR_LIMIT: u32 = 2;
/// How far from the city open space and other settlers count, in squares.
pub const SETTLE_RADIUS: i32 = 9;
/// Open sites worth counting.
const MAX_SITES: u32 = 5;
/// The most settlers wanted at a time near one city.
const MAX_SETTLERS_WANTED: u32 = 2;
/// A rival's city this close raises the garrison.
const RIVAL_RANGE: u32 = 10;
/// The sharpness of the unit roll: a candidate is weighed `(score / best)^SHARPNESS`.
const SHARPNESS: i32 = 4;
/// Candidates below this share of the best score are not rolled for.
const ROLL_CUTOFF: f32 = 1.0 / 3.0;

/// What a city builds: only buildings, only units, or both with equal production on each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mix {
    Buildings,
    Units,
    Both,
}

/// What matters. `Balanced` weighs all the others equally and is only a primary focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Focus {
    Magic,
    Population,
    Production,
    Defense,
    Offense,
    Exploration,
    Gold,
    Balanced,
}

impl Focus {
    /// Every choosable focus, in the order of the affinity table.
    pub const ALL: [Focus; 7] = [
        Focus::Magic,
        Focus::Population,
        Focus::Production,
        Focus::Defense,
        Focus::Offense,
        Focus::Exploration,
        Focus::Gold,
    ];

    /// The key of the focus's name in `content/text/` (the console uses [`Focus::name`]).
    pub fn key(self) -> &'static str {
        match self {
            Focus::Magic => "autobuild.focus_magic",
            Focus::Population => "autobuild.focus_population",
            Focus::Production => "autobuild.focus_production",
            Focus::Defense => "autobuild.focus_defense",
            Focus::Offense => "autobuild.focus_offense",
            Focus::Exploration => "autobuild.focus_exploration",
            Focus::Gold => "autobuild.focus_gold",
            Focus::Balanced => "autobuild.focus_balanced",
        }
    }

    /// The word the console takes for the focus.
    pub fn name(self) -> &'static str {
        match self {
            Focus::Magic => "Magic",
            Focus::Population => "Population",
            Focus::Production => "Production",
            Focus::Defense => "Defense",
            Focus::Offense => "Offense",
            Focus::Exploration => "Exploration",
            Focus::Gold => "Gold",
            Focus::Balanced => "Balanced",
        }
    }

    /// The place in [`Focus::ALL`] (`None` for `Balanced`).
    fn index(self) -> Option<usize> {
        Focus::ALL.iter().position(|&f| f == self)
    }
}

impl Focus {
    /// A focus from its name, in any case.
    pub fn parse(text: &str) -> Option<Focus> {
        let t = text.to_lowercase();
        Focus::ALL
            .iter()
            .copied()
            .chain([Focus::Balanced])
            .find(|f| f.name().to_lowercase() == t)
    }
}

impl Mix {
    /// A mix from its name, in any case ("both" also as "50/50" or "mixed").
    pub fn parse(text: &str) -> Option<Mix> {
        match text.to_lowercase().as_str() {
            "buildings" | "building" => Some(Mix::Buildings),
            "units" | "unit" => Some(Mix::Units),
            "both" | "mixed" | "50/50" => Some(Mix::Both),
            _ => None,
        }
    }

    /// The key of the mix's name in `content/text/` (the console uses [`Mix::name`]).
    pub fn key(self) -> &'static str {
        match self {
            Mix::Buildings => "autobuild.mix_buildings",
            Mix::Units => "autobuild.mix_units",
            Mix::Both => "autobuild.mix_both",
        }
    }

    /// The word the console takes for the mix.
    pub fn name(self) -> &'static str {
        match self {
            Mix::Buildings => "Buildings",
            Mix::Units => "Units",
            Mix::Both => "Both",
        }
    }
}

/// What the player asks of a city's auto-builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildPlan {
    pub mix: Mix,
    pub primary: Focus,
    pub secondary: Option<Focus>,
    /// Whether the city may build settlers.
    pub settlers: bool,
    /// The city's population must be at least this to build settlers.
    pub min_settler_pop: u32,
    /// Turn the auto-builder off and tell the player when a threat comes (docs section 9).
    pub event_prompt: bool,
    /// Settle without waiting for the city to near its population limit (the AI's plans ask
    /// for this; the dialog does not offer it).
    #[serde(default)]
    pub eager_settlers: bool,
}

impl Default for BuildPlan {
    fn default() -> Self {
        BuildPlan {
            mix: Mix::Both,
            primary: Focus::Balanced,
            secondary: None,
            settlers: false,
            min_settler_pop: 4,
            event_prompt: false,
            eager_settlers: false,
        }
    }
}

impl BuildPlan {
    /// The plan of a player with the Grand Vizier on and a city with no plan of its own:
    /// buildings only, balanced, as the Vizier always chose.
    pub fn vizier() -> BuildPlan {
        BuildPlan {
            mix: Mix::Buildings,
            ..BuildPlan::default()
        }
    }

    /// The weight of each focus, in the order of [`Focus::ALL`], adding up to 1.
    pub fn weights(&self) -> [f32; 7] {
        let mut w = [0.0; 7];
        let (primary, secondary) = match self.secondary {
            Some(s) if s != self.primary && s != Focus::Balanced => {
                (PRIMARY_WEIGHT, Some((s, SECONDARY_WEIGHT)))
            }
            _ => (1.0, None),
        };
        match self.primary.index() {
            Some(i) => w[i] += primary,
            None => w = [primary / 7.0; 7],
        }
        if let Some((s, share)) = secondary {
            if let Some(i) = s.index() {
                w[i] += share;
            }
        }
        w
    }
}

/// The component on a city with a plan: the plan and the production spent on each side so far
/// (which `Mix::Both` balances).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoBuild {
    pub plan: BuildPlan,
    pub spent_units: u32,
    pub spent_buildings: u32,
}

impl AutoBuild {
    pub fn new(plan: BuildPlan) -> AutoBuild {
        AutoBuild {
            plan,
            spent_units: 0,
            spent_buildings: 0,
        }
    }
}

// ---- the affinity table --------------------------------------------------------------------

/// How much each building serves each focus, 0 to 3, in the order of [`Focus::ALL`]: magic,
/// population, production, defense, offense, exploration, gold. A test keeps it in step with the
/// content: every building has a row.
pub const AFFINITY: &[(&str, [u8; 7])] = &[
    ("builders_hall", [0, 1, 1, 0, 0, 0, 0]),
    ("granary", [0, 2, 0, 0, 0, 0, 0]),
    ("farmers_market", [0, 3, 0, 0, 0, 0, 0]),
    ("foresters_guild", [0, 1, 2, 0, 0, 0, 0]),
    ("shrine", [1, 1, 0, 0, 0, 0, 0]),
    ("temple", [2, 2, 0, 0, 0, 0, 0]),
    ("parthenon", [2, 2, 0, 0, 0, 0, 0]),
    ("cathedral", [3, 2, 0, 0, 0, 0, 0]),
    ("oracle", [1, 2, 0, 1, 0, 1, 0]),
    ("library", [2, 0, 0, 0, 0, 0, 0]),
    ("sages_guild", [2, 0, 0, 0, 0, 0, 0]),
    ("university", [3, 0, 0, 0, 0, 0, 0]),
    ("wizards_guild", [3, 0, 0, 0, 0, 0, 0]),
    ("alchemists_guild", [1, 0, 0, 0, 1, 0, 2]),
    ("animists_guild", [2, 2, 0, 0, 1, 0, 0]),
    ("sawmill", [0, 0, 2, 0, 0, 0, 0]),
    ("miners_guild", [0, 0, 3, 0, 0, 0, 0]),
    ("mechanicians_guild", [0, 0, 3, 0, 0, 0, 0]),
    ("marketplace", [0, 0, 0, 0, 0, 0, 2]),
    ("bank", [0, 0, 0, 0, 0, 0, 3]),
    ("merchants_guild", [0, 0, 0, 0, 0, 0, 3]),
    ("barracks", [0, 0, 0, 1, 1, 0, 0]),
    ("smithy", [0, 0, 0, 0, 1, 0, 0]),
    ("armory", [0, 0, 0, 1, 2, 0, 0]),
    ("fighters_guild", [0, 0, 0, 1, 3, 0, 0]),
    ("armorers_guild", [0, 0, 0, 2, 2, 0, 0]),
    ("war_college", [0, 0, 0, 2, 3, 0, 0]),
    ("stables", [0, 0, 0, 0, 2, 1, 0]),
    ("fantastic_stable", [0, 0, 0, 0, 3, 0, 0]),
    ("city_walls", [0, 0, 0, 3, 0, 0, 0]),
    ("ship_wrights_guild", [0, 0, 0, 0, 0, 2, 0]),
    ("ship_yard", [0, 0, 0, 0, 0, 2, 0]),
    ("maritime_guild", [0, 0, 0, 0, 0, 3, 1]),
];

fn affinity(id: &str) -> [u8; 7] {
    AFFINITY
        .iter()
        .find(|(name, _)| *name == id)
        .map_or([0; 7], |&(_, a)| a)
}

/// The fewest defenders a city of this size wants (docs section 4).
pub fn garrison_minimum(figures: u32) -> u32 {
    match figures {
        0 => 0,
        1..=2 => 1,
        3..=5 => 2,
        6..=8 => 3,
        _ => 4,
    }
}

// ---- what the choice needs to know of the city ---------------------------------------------

/// A snapshot of a city and its surroundings: all `choose` needs besides the plan.
#[derive(Debug, Clone, Default)]
pub struct CityFacts {
    pub figures: u32,
    /// The largest population the land and the buildings feed.
    pub max_size: u32,
    pub food: i32,
    pub rebels: u32,
    /// The treasury's net gold per turn.
    pub net_gold: i32,
    /// Fighting Normal units of the owner standing in the city.
    pub defenders: u32,
    /// A rival wizard's city is known within range.
    pub rival_near: bool,
    pub built: Vec<BuildingId>,
    /// Not built, open to the city's race and coast, whatever it still needs.
    pub possible: Vec<BuildingId>,
    /// The part of `possible` the city may start now.
    pub buildable: Vec<BuildingId>,
    /// Normal units the city may build.
    pub units: Vec<UnitId>,
    /// Places a new city could stand, near this one (at most five).
    pub open_sites: u32,
    /// Settlers alive or being built near this city.
    pub settlers_near: u32,
    /// Engineers and ships without an attack that the player has, and the player's cities.
    pub engineers: u32,
    pub transports: u32,
    pub cities: u32,
}

fn stat(d: &crate::rules::UnitDef, s: Stat) -> f32 {
    d.stats.get(&s).copied().unwrap_or(0) as f32
}

fn is_settler(d: &crate::rules::UnitDef) -> bool {
    d.abilities.iter().any(|a| a == "create_outpost")
}

fn is_combat(d: &crate::rules::UnitDef) -> bool {
    stat(d, Stat::Melee) > 0.0 || stat(d, Stat::Ranged) > 0.0
}

fn is_engineer(d: &crate::rules::UnitDef) -> bool {
    stat(d, Stat::Construction) > 0.0
}

/// A ship or other unit with no attack that the exploration focus may like.
fn is_transport(d: &crate::rules::UnitDef) -> bool {
    !is_combat(d) && !is_settler(d) && !is_engineer(d)
}

fn effect_values(content: &Content, b: BuildingId, wanted: &[Stat]) -> Vec<(Stat, ModOp, i32)> {
    content
        .rules
        .buildings
        .get(b)
        .effects
        .iter()
        .filter_map(|r| match r.effect {
            Effect::Modify { stat, op, value } if wanted.contains(&stat) => Some((stat, op, value)),
            _ => None,
        })
        .collect()
}

fn adds_food(content: &Content, b: BuildingId) -> bool {
    effect_values(
        content,
        b,
        &[Stat::CityFood, Stat::CityFoodFlat, Stat::FarmerFood],
    )
    .iter()
    .any(|&(_, _, v)| v > 0)
}

fn lowers_unrest(content: &Content, b: BuildingId) -> bool {
    effect_values(content, b, &[Stat::CityUnrest])
        .iter()
        .any(|&(_, _, v)| v < 0)
}

fn adds_gold(content: &Content, b: BuildingId) -> bool {
    effect_values(content, b, &[Stat::CityGold])
        .iter()
        .any(|&(_, _, v)| v > 0)
}

// ---- scoring buildings ---------------------------------------------------------------------

/// `V(b)`: the building's worth to the plan's focuses.
fn value(content: &Content, plan: &BuildPlan, b: BuildingId) -> f32 {
    let a = affinity(&content.rules.buildings.get(b).id);
    plan.weights()
        .iter()
        .zip(a)
        .map(|(w, a)| w * f32::from(a))
        .sum()
}

/// `W(b)`: the value plus half the best of what the building opens, two levels down.
fn worth(content: &Content, plan: &BuildPlan, f: &CityFacts, b: BuildingId, depth: u32) -> f32 {
    let own = value(content, plan, b);
    if depth >= 2 {
        return own;
    }
    let id = &content.rules.buildings.get(b).id;
    let opened = f
        .possible
        .iter()
        .filter(|&&d| {
            d != b
                && content
                    .rules
                    .buildings
                    .get(d)
                    .requires
                    .iter()
                    .any(|r| r == id)
        })
        .map(|&d| worth(content, plan, f, d, depth + 1))
        .fold(0.0, f32::max);
    own + 0.5 * opened
}

fn building_score(content: &Content, plan: &BuildPlan, f: &CityFacts, b: BuildingId) -> f32 {
    let def = content.rules.buildings.get(b);
    let mut need = 1.0;
    if adds_food(content, b) && (f.food <= 0 || f.figures + 1 >= f.max_size) {
        need = 2.0;
    }
    if lowers_unrest(content, b) && f.rebels > 0 {
        need = 3.0;
    }
    worth(content, plan, f, b, 0) * need
        / (def.cost.max(1) as f32).sqrt()
        / (1.0 + 0.1 * def.upkeep_gold as f32)
}

/// The buildings the city can start whose upkeep the treasury bears, best first (ties by the
/// rules' `autobuild_order`, then by the content's order).
fn ranked_buildings(content: &Content, plan: &BuildPlan, f: &CityFacts) -> Vec<(BuildingId, f32)> {
    let order = &content.rules.config.autobuild_order;
    let place = |b: BuildingId| {
        let id = &content.rules.buildings.get(b).id;
        order.iter().position(|o| o == id).unwrap_or(usize::MAX)
    };
    let mut v: Vec<(BuildingId, f32)> = f
        .buildable
        .iter()
        .copied()
        .filter(|&b| f.net_gold >= content.rules.buildings.get(b).upkeep_gold as i32)
        .map(|b| (b, building_score(content, plan, f, b)))
        .collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1).then(place(a.0).cmp(&place(b.0))));
    v
}

// ---- scoring units -------------------------------------------------------------------------

/// The three profiles of a unit, before they are divided by the best candidate's.
fn raw_profiles(d: &crate::rules::UnitDef) -> [f32; 3] {
    let hp = stat(d, Stat::MaxHp);
    let figures = stat(d, Stat::Figures).max(1.0);
    let ranged = stat(d, Stat::Ranged);
    let attack = stat(d, Stat::Melee).max(ranged * 1.2);
    let moves = stat(d, Stat::Moves);
    let offense = figures * (attack * 1.5 + moves * 0.5 + hp * 0.3);
    let defense = figures
        * (stat(d, Stat::Defense) * 1.5
            + hp * 0.5
            + stat(d, Stat::Resistance) * 0.1
            + stat(d, Stat::Melee) * 0.5);
    let flying =
        matches!(d.movement, MovementType::Flying) || d.abilities.iter().any(|a| a == "flying");
    let sailing = matches!(d.movement, MovementType::Sailing);
    let exploration = moves
        + (stat(d, Stat::Sight) - 1.0) * 0.25
        + if flying { 2.0 } else { 0.0 }
        + if sailing { 1.5 } else { 0.0 }
        + stat(d, Stat::Transport) * 0.3
        + stat(d, Stat::Construction) * 1.5
        + if is_settler(d) { 1.0 } else { 0.0 };
    [offense, defense, exploration]
}

fn max_profiles(content: &Content, candidates: &[UnitId]) -> [f32; 3] {
    let mut best = [f32::EPSILON; 3];
    for &u in candidates {
        let p = raw_profiles(content.rules.units.get(u));
        for i in 0..3 {
            best[i] = best[i].max(p[i]);
        }
    }
    best
}

/// The best defender among the candidates: the Defense profile alone.
fn best_defender(content: &Content, f: &CityFacts) -> Option<UnitId> {
    let pool: Vec<UnitId> = f
        .units
        .iter()
        .copied()
        .filter(|&u| {
            let d = content.rules.units.get(u);
            is_combat(d) && !is_settler(d) && d.movement != MovementType::Sailing
        })
        .collect();
    let best = max_profiles(content, &pool);
    pool.into_iter()
        .map(|u| {
            let d = content.rules.units.get(u);
            let s = raw_profiles(d)[1] / best[1] / (d.cost.max(1) as f32).sqrt();
            (u, s)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(u, _)| u)
}

/// The units the plan may build with their scores, best first. Settlers and engineers join only
/// under the exploration focus (and settlers only where `settlers_ok`).
fn ranked_units(
    content: &Content,
    plan: &BuildPlan,
    f: &CityFacts,
    settlers_ok: bool,
) -> Vec<(UnitId, f32)> {
    let w = plan.weights();
    let [magic, pop, prod, def, off, exp, gold] = w;
    let off_weight = off + 0.5 * (magic + pop + prod + gold);
    let exploring = exp > 0.0;
    let pool: Vec<UnitId> = f
        .units
        .iter()
        .copied()
        .filter(|&u| {
            let d = content.rules.units.get(u);
            if is_settler(d) {
                exploring && settlers_ok
            } else if is_engineer(d) {
                exploring && f.engineers < f.cities.div_ceil(3).max(1)
            } else if is_transport(d) {
                exploring && f.transports < f.cities.div_ceil(4).max(1)
            } else {
                true
            }
        })
        .collect();
    let best = max_profiles(content, &pool);
    let mut v: Vec<(UnitId, f32)> = pool
        .into_iter()
        .map(|u| {
            let d = content.rules.units.get(u);
            let p = raw_profiles(d);
            let fit = off_weight * p[0] / best[0] + def * p[1] / best[1] + exp * p[2] / best[2];
            (u, fit / (d.cost.max(1) as f32).sqrt())
        })
        .filter(|&(_, s)| s > 0.0)
        .collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

/// A weighted roll among the best units: `(score / best)^4`, the weak ones dropped.
fn roll_unit(ranked: &[(UnitId, f32)], roll: &mut dyn FnMut(u32) -> u32) -> Option<UnitId> {
    let best = ranked.first()?.1;
    let weighted: Vec<(UnitId, u32)> = ranked
        .iter()
        .filter(|&&(_, s)| s >= best * ROLL_CUTOFF)
        .map(|&(u, s)| {
            (
                u,
                ((s / best).powi(SHARPNESS) * 1000.0).round().max(1.0) as u32,
            )
        })
        .collect();
    let total: u32 = weighted.iter().map(|&(_, w)| w).sum();
    let mut at = roll(total);
    for &(u, w) in &weighted {
        if at <= w {
            return Some(u);
        }
        at -= w;
    }
    weighted.last().map(|&(u, _)| u)
}

// ---- the choice ----------------------------------------------------------------------------

/// Whether the plan wants settlers now (docs section 6).
pub fn wants_settlers(plan: &BuildPlan, f: &CityFacts) -> bool {
    plan.settlers
        && f.figures >= plan.min_settler_pop
        && (plan.eager_settlers || f.figures + NEAR_LIMIT >= f.max_size)
        && f.settlers_near < f.open_sites.min(MAX_SETTLERS_WANTED)
}

/// The cheapest settlers the city can build.
fn cheapest_settler(content: &Content, f: &CityFacts) -> Option<UnitId> {
    f.units
        .iter()
        .copied()
        .filter(|&u| is_settler(content.rules.units.get(u)))
        .min_by_key(|&u| content.rules.units.get(u).cost)
}

/// A building that adds gold, the best of those the city may build.
fn gold_building(content: &Content, f: &CityFacts) -> Option<BuildingId> {
    let plan = BuildPlan {
        mix: Mix::Buildings,
        primary: Focus::Gold,
        ..BuildPlan::default()
    };
    ranked_buildings(content, &plan, f)
        .into_iter()
        .map(|(b, _)| b)
        .find(|&b| adds_gold(content, b))
}

/// What the city builds next (docs section 3). `roll(n)` gives a number in `1..=n`.
pub fn choose(
    content: &Content,
    auto: &AutoBuild,
    f: &CityFacts,
    roll: &mut dyn FnMut(u32) -> u32,
) -> ProductionItem {
    let plan = &auto.plan;
    let units_ok = plan.mix != Mix::Buildings;
    let buildings_ok = plan.mix != Mix::Units;

    // 1. Garrison first.
    let mut minimum = garrison_minimum(f.figures);
    if minimum > 0 {
        minimum += u32::from(f.rival_near);
        if plan.primary == Focus::Defense {
            minimum += 2;
        } else if plan.secondary == Some(Focus::Defense) {
            minimum += 1;
        }
    }
    if units_ok && f.defenders < minimum {
        if let Some(u) = best_defender(content, f) {
            return ProductionItem::Unit(u);
        }
    }

    let ranked = if buildings_ok {
        ranked_buildings(content, plan, f)
    } else {
        Vec::new()
    };

    // 2. Unrest.
    if buildings_ok && f.rebels > 0 {
        if let Some(&(b, _)) = ranked.iter().find(|&&(b, _)| lowers_unrest(content, b)) {
            return ProductionItem::Building(b);
        }
    }

    // 3. The gold floor.
    if buildings_ok && f.net_gold < GOLD_FLOOR {
        return match gold_building(content, f) {
            Some(b) => ProductionItem::Building(b),
            None => ProductionItem::TradeGoods,
        };
    }

    // 4. Settlers.
    let settlers_ok = wants_settlers(plan, f);
    if units_ok && settlers_ok {
        if let Some(u) = cheapest_settler(content, f) {
            return ProductionItem::Unit(u);
        }
    }

    // 5. The mix: the side that is behind, then the best of it.
    let building = ranked
        .iter()
        .find(|&&(_, s)| s > 0.0)
        .map(|&(b, _)| ProductionItem::Building(b));
    let unit = if units_ok {
        let units = ranked_units(content, plan, f, settlers_ok);
        roll_unit(&units, roll).map(ProductionItem::Unit)
    } else {
        None
    };
    let units_behind = auto.spent_units < auto.spent_buildings;
    let pick = match plan.mix {
        Mix::Buildings => building,
        Mix::Units => unit,
        Mix::Both if units_behind => unit.or(building),
        Mix::Both => building.or(unit),
    };
    if let Some(item) = pick {
        return item;
    }

    // 6. The end of the list: gold buildings, housing where the city can grow, trade goods.
    if buildings_ok {
        if let Some(b) = gold_building(content, f) {
            return ProductionItem::Building(b);
        }
    }
    if plan.weights()[1] > 0.0 && f.figures < f.max_size {
        return ProductionItem::Housing;
    }
    ProductionItem::TradeGoods
}

// ---- gathering the facts from a game -------------------------------------------------------

impl Game {
    /// The auto-builder of a city: its own plan, or the Vizier's default when its owner has the
    /// Vizier on.
    pub fn autobuild_of(&self, city: hecs::Entity) -> Option<AutoBuild> {
        if let Ok(a) = self.world.get::<&AutoBuild>(city) {
            return Some(*a);
        }
        let owner = self.economy().city_owner(city)?;
        self.player(owner)
            .is_some_and(|p| p.has_grand_vizier())
            .then(|| AutoBuild::new(BuildPlan::vizier()))
    }

    /// The facts about a city that the choice needs. `None` for an outpost (it builds nothing).
    pub fn autobuild_facts(&self, city: hecs::Entity, owner: PlayerId) -> Option<CityFacts> {
        let eco = self.economy();
        let e = eco.city_economy(city).filter(|e| e.figures > 0)?;
        let content = &self.content;
        let at = *self.world.get::<&Position>(city).ok()?;
        let view = crate::player::view::PlayerView::new(self, owner);
        let race = self
            .world
            .get::<&crate::city::economy::CityRace>(city)
            .ok()
            .map(|r| content.rules.races.get(r.0).id.clone());
        let built = self
            .world
            .get::<&crate::city::economy::Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        let coastal = eco.city_is_coastal(city);
        let possible: Vec<BuildingId> = content
            .rules
            .buildings
            .iter()
            .filter(|(id, d)| {
                !built.contains(id)
                    && (d.races.is_empty() || race.as_ref().is_some_and(|r| d.races.contains(r)))
                    && (!d.coastal || coastal)
            })
            .map(|(id, _)| id)
            .collect();
        let buildable: Vec<BuildingId> = possible
            .iter()
            .copied()
            .filter(|&b| eco.can_build(city, ProductionItem::Building(b)).is_ok())
            .collect();
        let units: Vec<UnitId> = content
            .rules
            .units
            .iter()
            .filter(|(id, d)| {
                d.kind == UnitKind::Normal
                    && d.cost > 0
                    && eco.can_build(city, ProductionItem::Unit(*id)).is_ok()
            })
            .map(|(id, _)| id)
            .collect();
        let defenders = eco
            .garrison(at)
            .into_iter()
            .filter(|&u| self.units().unit_owner(u) == Some(owner))
            .filter(|&u| {
                self.units()
                    .unit_def(u)
                    .is_some_and(|d| d.kind == UnitKind::Normal && is_combat(d) && !is_settler(d))
            })
            .count() as u32;
        let rival_near = view.known_cities().into_iter().any(|(_, o, p)| {
            view.is_rival_wizard(o) && view.distance(at, p).is_some_and(|d| d <= RIVAL_RANGE)
        });
        let my_units = view.my_units();
        let count = |pred: &dyn Fn(&crate::rules::UnitDef) -> bool| {
            my_units
                .iter()
                .filter_map(|&u| view.unit_def(u))
                .filter(|d| pred(d))
                .count() as u32
        };
        let cities = view.my_cities();
        let mut facts = CityFacts {
            figures: e.figures,
            max_size: e.max_size,
            food: e.food,
            rebels: e.rebels,
            net_gold: self.budget(owner).net_gold(),
            defenders,
            rival_near,
            built,
            possible,
            buildable,
            units,
            engineers: count(&is_engineer),
            transports: count(&is_transport),
            cities: cities.len() as u32,
            ..CityFacts::default()
        };
        // Open space matters only to a city that may build settlers soon.
        let plan_wants = self.autobuild_of(city).is_some_and(|a| {
            a.plan.settlers
                && (a.plan.eager_settlers || facts.figures + NEAR_LIMIT >= facts.max_size)
        });
        if plan_wants {
            facts.open_sites = open_sites(&view, at);
            let near = |p: Option<Position>| {
                p.is_some_and(|p| {
                    view.distance(at, p)
                        .is_some_and(|d| d <= SETTLE_RADIUS as u32)
                })
            };
            let alive = my_units
                .iter()
                .filter(|&&u| view.unit_has_ability(u, "create_outpost"))
                .filter(|&&u| near(view.unit_position(u)))
                .count();
            let building = cities
                .iter()
                .filter(|&&(c, p)| c != city && near(Some(p)))
                .filter(|&&(c, _)| {
                    matches!(
                        view.city_production(c).item,
                        Some(ProductionItem::Unit(u)) if is_settler(content.rules.units.get(u))
                    )
                })
                .count();
            facts.settlers_near = (alive + building) as u32;
        }
        Some(facts)
    }

    /// What the auto-builder has the city build next, `None` when the city has no plan (and its
    /// owner no Vizier) or is an outpost.
    pub fn autobuild_choice(
        &mut self,
        city: hecs::Entity,
        owner: PlayerId,
    ) -> Option<ProductionItem> {
        let auto = self.autobuild_of(city)?;
        let facts = self.autobuild_facts(city, owner)?;
        let mut rolls = |n: u32| self.rng.roll(n);
        Some(choose(&self.content, &auto, &facts, &mut rolls))
    }

    /// Give a city a plan, or take it away with `None`. A city with nothing to build starts on
    /// the plan's choice at once; one with work in hand finishes it first.
    pub fn set_autobuild(
        &mut self,
        city: hecs::Entity,
        plan: Option<BuildPlan>,
    ) -> Result<(), crate::city::production::ProductionError> {
        if self.world.get::<&crate::city::City>(city).is_err() {
            return Err(crate::city::production::ProductionError::NotACity);
        }
        match plan {
            Some(plan) => {
                // A new plan keeps what the city has spent so far.
                let mut auto = AutoBuild::new(plan);
                if let Ok(old) = self.world.get::<&AutoBuild>(city) {
                    auto.spent_units = old.spent_units;
                    auto.spent_buildings = old.spent_buildings;
                }
                let _ = self.world.insert_one(city, auto);
                self.autobuild_refresh(city);
            }
            None => {
                let _ = self.world.remove_one::<AutoBuild>(city);
            }
        }
        Ok(())
    }

    /// A city whose auto-builder has nothing in hand (nothing, housing or trade goods, which
    /// never finish) looks again: something may have become worth building.
    pub(crate) fn autobuild_refresh(&mut self, city: hecs::Entity) {
        let idle = matches!(
            self.economy().city_production(city).item,
            None | Some(ProductionItem::Housing) | Some(ProductionItem::TradeGoods)
        );
        let Some(owner) = self.economy().city_owner(city) else {
            return;
        };
        if !idle {
            return;
        }
        if let Some(item) = self.autobuild_choice(city, owner) {
            let mut p = self.economy().city_production(city);
            if p.item != Some(item) {
                p.item = Some(item);
                p.progress = 0;
                let _ = self.world.insert_one(city, p);
            }
        }
    }

    /// Note what a finished item cost, so `Mix::Both` can keep the sides level.
    pub(crate) fn autobuild_spent(&mut self, city: hecs::Entity, item: ProductionItem, cost: u32) {
        if let Ok(mut a) = self.world.get::<&mut AutoBuild>(city) {
            match item {
                ProductionItem::Unit(_) => a.spent_units += cost,
                ProductionItem::Building(_) => a.spent_buildings += cost,
                _ => {}
            }
        }
    }
}

/// How many places a new city could stand within reach of `at`: explored, free of lairs and
/// nodes, at least the minimum distance from every city and from each other. At most five.
pub fn open_sites(view: &crate::player::view::PlayerView, at: Position) -> u32 {
    let spacing = view.content().rules.config.min_city_distance;
    let mut picked: Vec<Position> = Vec::new();
    'search: for dy in -SETTLE_RADIUS..=SETTLE_RADIUS {
        for dx in -SETTLE_RADIUS..=SETTLE_RADIUS {
            let Some(p) = view.shift(at, dx, dy) else {
                continue;
            };
            if !view.can_found_city(p) || view.has_lair(p) || view.in_node(p) {
                continue;
            }
            if picked
                .iter()
                .any(|&q| view.distance(q, p).is_some_and(|d| d < spacing))
            {
                continue;
            }
            picked.push(p);
            if picked.len() as u32 >= MAX_SITES {
                break 'search;
            }
        }
    }
    picked.len() as u32
}

#[cfg(test)]
mod tests;
