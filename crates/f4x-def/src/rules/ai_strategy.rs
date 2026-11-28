//! The levers of the strategic AI (`crates/f4x-def/content/ai_strategy.ron`, whose comments say
//! what each steers). AI tuning, not rules of the game.
//!
//! Tests: at the end of this file; the shipped file's refusals in `tests/it/c1_content.rs`.

use serde::{Deserialize, Serialize};

use super::lever::{allows, levers, wanted, Lever, OverrideError};
use crate::issue::{ContentIssue, Issues};
use crate::map::TerrainClass;

const FILE: &str = "ai_strategy.ron";

levers!(
    /// What several jobs read.
    SharedLevers {
        near_city_radius: i32,
        city_defence_factor: f32,
        rival_range: u32,
        river_worth: f32,
        mana_reserve: i32,
    }
);

levers!(
    /// When the armies, parties and monsters attack.
    AttackLevers {
        attack_force: f32,
        guess: f32,
        memory_turns: u32,
        march_turns: u32,
        unreachable_turns: u32,
        threat_ratio: f32,
        monster_edge: f32,
        strike_share: f32,
        min_march: usize,
        min_crossing: usize,
        perimeter_site_range: i32,
        perimeter_reach: u32,
    }
);

levers!(
    /// Which town a campaign marches on (the war plan, and the choice of a landing or a crossing).
    TargetLevers {
        route_slack: u32,
        route_bonus: f32,
        defence_squares: f32,
        fortress_bonus: f32,
        fortress_worth: f32,
        size_worth: f32,
        building_worth: f32,
        war_worth: f32,
        neutral_share: f32,
        goal_distance: f32,
        goal_across: f32,
        next_distance: f32,
        next_foothold: f32,
        next_worth: f32,
        goal_hold: f32,
        next_hold: f32,
        recon_radius: i32,
        recon_towns: usize,
        recon_bonus: f32,
        recon_near: u32,
        max_force: usize,
    }
);

levers!(
    /// How many soldiers stand in a city.
    GarrisonLevers {
        garrison_fortress: usize,
        garrison_city: usize,
        garrison_front_fortress: usize,
        garrison_front_city: usize,
        garrison_war_fortress: usize,
        build_garrison: f32,
        late_spare: usize,
        fast_moves: i32,
    }
);

levers!(
    /// Scouting.
    ScoutingLevers {
        scouts: u32,
        walk_cost: f32,
        stuck_turns: u32,
        keep_apart: u32,
        sea_party: usize,
        blind_share: f32,
    }
);

levers!(
    /// Lairs, nodes and towers: the parties that take them.
    SiteHuntLevers {
        range: u32,
        immune_discount: f32,
        retry_edge: f32,
        site_party: usize,
        muster_turns: u32,
    }
);

levers!(
    /// What a place for a city is worth, in points.
    CitySiteLevers {
        shared_share: f32,
        production: f32,
        mana: f32,
        gold: f32,
        food: f32,
        unit_cost: f32,
        near_own: f32,
        near_own_step: f32,
        enemy_range: u32,
        enemy_peace: f32,
        enemy_war: f32,
        sea_cost: f32,
        crossing_sites_counted: usize,
        refresh_turns: u32,
        refresh_squares: usize,
        sea_limit: usize,
        land_limit: usize,
    }
    lists {
        /// What a square of each terrain is worth to the city that works it (food and growth).
        /// A class not listed is worth nothing.
        land_values: Vec<(TerrainClass, f32)>,
    }
);

levers!(
    /// Taxes, gold, settlers and ships.
    EconomyLevers {
        max_tax_rate: u8,
        net: i32,
        ships: usize,
        max_ships: usize,
        idle_surplus: usize,
        rich_gold: i32,
        tall_below: f32,
        wide_above: f32,
        gold_reserve_base: i32,
        gold_reserve_income: i32,
    }
);

levers!(
    /// What the cities build.
    BuildingLevers {
        gold_floor: i32,
        rich_enough: i32,
        land_production_min: f32,
        rivers_min: u32,
        large_city: u32,
    }
);

levers!(
    /// The forge cities and the dismissal of cheap units late in the game.
    ForgeLevers {
        forge_turn: u32,
        cities_per_forge: usize,
        most_forges: usize,
        forge_rich_gold: i32,
        most_forges_rich: usize,
        low_tier_percent: u32,
    }
);

levers!(
    /// Casting overland: which spell, when, and the mana it takes.
    MagicLevers {
        rate_affinity: f32,
        rate_upkeep: f32,
        mana_per_soldier: f32,
        most_soldiers: f32,
        mana_wanted_percent: i32,
        most_paying_turns: i32,
        dear_spell: i32,
        max_casting_turns: i32,
        saving_turns: i32,
        army_cap_base: usize,
        army_cap_city: usize,
        explore_floor: u32,
        cure_percent: u32,
        recall_distance: u32,
        recall_percent: u32,
        lycanthropy_edge: f32,
        min_shift: usize,
        shift_squares: usize,
        gate_far: u32,
        gates_max: usize,
        gold_kept: i32,
        race_lead: f32,
    }
);

levers!(
    /// How the power is split between mana, research and skill, in percent.
    SplitLevers {
        base_mana: i32,
        base_research: i32,
        base_skill: i32,
        split_step: i32,
        least_mana: i32,
        least_research: i32,
        least_skill: i32,
        mana_horizon: i32,
        rich_reserve: i32,
        mastery_mana: u8,
        mastery_research: u8,
        mastery_skill: u8,
        done_mana: u8,
        done_research: u8,
        done_skill: u8,
        racing_mana: u8,
        racing_research: u8,
        racing_skill: u8,
    }
);

levers!(
    /// Items: crafting, buying and handing them out.
    ItemLevers {
        craft_max_value: i32,
        craft_powers: usize,
        use_suits_percent: i32,
        use_misfit_percent: i32,
        better_percent: i32,
        keep_unusable: usize,
        worth_percent: i32,
    }
);

levers!(
    /// The mana spent in a battle.
    BattleSpendLevers {
        hold_turns: i32,
        no_limit: f32,
        sure_ratio: f32,
        even_ratio: f32,
        least_need: f32,
        home_range: u32,
    }
);

levers!(
    /// Engineers, roads and bridges.
    EngineerLevers {
        linked_cities: usize,
        link_range: u32,
        cities_per_engineer: usize,
        road_stretch: usize,
        engineer_escort_range: u32,
        bridge_keep_turns: u32,
    }
);

levers!(
    /// Ships and boarding.
    TransportLevers {
        min_load: usize,
        candidates: usize,
        straggler_turns: u32,
        merge_range: u32,
        shore_range: i32,
    }
);

levers!(
    /// Wounded units, escorts and spirits.
    CareLevers {
        rest_percent: u32,
        healer_range: u32,
        rest_city_range: u32,
        escort_enemy_near: i32,
        rest_enemy_near: i32,
        weak_hero: f32,
        meld_patience: u32,
    }
);

levers!(
    /// The phases of a game and how long a plan stands.
    PlanningLevers {
        early_turn: u32,
        late_turn: u32,
        plan_timeout_turns: u32,
        scout_timeout_turns: u32,
        forgive_plans: u32,
    }
);

levers!(
    /// What the two groups below share: power, the leader, factors and self-interest (`statecraft.rs`).
    DiplomacyLevers {
        on_factors: bool,
        on_arms: bool,
        on_trade: bool,
        city_power: i32,
        dominant_army: i32,
        dominant_cities: i32,
        dominant_min_cities: usize,
        leader_margin: i32,
        coalition_bonus: i32,
        fame_per_point: i32,
        fame_cap: i32,
        charismatic: i32,
        same_race: i32,
        shared_realm: i32,
        shared_cap: i32,
        opposed_realm: i32,
        broken_memory: i32,
        interest_target: i32,
        interest_leader: i32,
        audience_refused_below: i32,
        gift_gold: i32,
        gift_least_gold: i32,
        friend_gift_least_gold: i32,
    }
);

levers!(
    /// Trades.
    TradeLevers {
        margin_none: i32,
        margin_pact: i32,
        margin_alliance: i32,
        margin_per_relation: i32,
        margin_per_goodwill: i32,
        margin_least: i32,
        margin_most: i32,
        least_price: i32,
    }
);

levers!(
    /// Feelings, peace, friends and enemies (`diplo_b*.rs`, `docs/ai-diplomacy-b.md`).
    DiplomacyBLevers {
        offer: i32,
        accept: i32,
        cool: u32,
        battle_lost: i32,
        battle_won: i32,
        city_lost: i32,
        city_taken: i32,
        drift_per_ten: i32,
        gap: i32,
        fear_pct: i32,
        friend_pct: i32,
        prey_pct: i32,
        prey_distance: u32,
        prey: i32,
        two_fronts: i32,
        young_age: u32,
        young: i32,
        mood_div: i32,
        near_per_square: i32,
        ruth_raze: i32,
        ruth_war: i32,
        ruth_broken: i32,
        ruth_fortress: i32,
        ruth_destroyed: i32,
        ruth_half: u32,
        ruth_cap: i32,
        mastery: i32,
        leader: i32,
        review: u32,
        hold: u32,
        keep: i32,
        enemy_min: i32,
        declare: i32,
        busy_army_x: i32,
        war_damp: i32,
        prey_value: i32,
        at_war: i32,
        leader_enemy: i32,
        mastery_enemy: i32,
        hostile_pct: i32,
        strike_fear: i32,
        from_turn: u32,
        too_strong_pct: i32,
        stance_friend_pct: i32,
        common_enemy: i32,
        grovel_pct: i32,
        league: i32,
        pact_min: i32,
        ally_min: i32,
        gift_min: i32,
        court_cool: u32,
        pact_accept: i32,
        ally_accept: i32,
        war_accept: i32,
        break_accept: i32,
        warn_below: i32,
        break_below: i32,
        war_below: i32,
        warn_cool: u32,
        demand_cool: u32,
        gift_cool: u32,
        trade_cool: u32,
        gift_friend: i32,
        yield_fear: i32,
        ally_war: i32,
        pact_growth: i32,
        alliance_growth: i32,
        trespass_break_below: i32,
        trespass_war_below: i32,
        counter_reach: i32,
        gold_per_point: i32,
        call_cool: u32,
        call_enemy_pct: i32,
        call_price_penalty: i32,
        map_refuse_pct: i32,
        map_least_news: usize,
        map_cool: u32,
        pact_cool: u32,
    }
);

levers!(
    /// Whether a captured city is razed, in points that speak for razing (`raze.rs`).
    RazeLevers {
        maniacal: i32,
        ruthless: i32,
        chaotic: i32,
        aggressive: i32,
        race_step: i32,
        land_fair: i32,
        land_pct: i32,
        land_cap: i32,
        citizen: i32,
        built_per_point: i32,
        size_cap: i32,
        alone: i32,
        exposed: i32,
        exposed_cap: i32,
        walls: i32,
        unheld: i32,
        threshold: i32,
        reach: u32,
    }
);

levers!(
    /// One objective's numbers.
    ObjectiveLevers {
        cities_wanted: usize,
        strike_base: usize,
        split_shift: usize,
        like_share: f32,
        variety_every: u64,
        boost_summon: i32,
        boost_unit_buff: i32,
        boost_city_buff: i32,
        boost_global_buff: i32,
        boost_curse: i32,
    }
);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveTable {
    pub pragmatist: ObjectiveLevers,
    pub militarist: ObjectiveLevers,
    pub theurgist: ObjectiveLevers,
    pub perfectionist: ObjectiveLevers,
    pub expansionist: ObjectiveLevers,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct StrategyAiRules {
    pub shared: SharedLevers,
    pub attack: AttackLevers,
    pub target: TargetLevers,
    pub garrison: GarrisonLevers,
    pub scouting: ScoutingLevers,
    pub sites: SiteHuntLevers,
    pub city_site: CitySiteLevers,
    pub economy: EconomyLevers,
    pub building: BuildingLevers,
    pub forge: ForgeLevers,
    pub magic: MagicLevers,
    pub split: SplitLevers,
    pub items: ItemLevers,
    pub battle_spend: BattleSpendLevers,
    pub engineers: EngineerLevers,
    pub transport: TransportLevers,
    pub care: CareLevers,
    pub planning: PlanningLevers,
    pub diplomacy: DiplomacyLevers,
    pub trade: TradeLevers,
    pub diplomacy_b: DiplomacyBLevers,
    pub raze: RazeLevers,
    pub objectives: ObjectiveTable,
}

/// The levers that are a share, so at most 1.
const AT_MOST_ONE: &[&str] = &[
    "shared_share",
    "neutral_share",
    "tall_below",
    "wide_above",
    "no_limit",
    "least_need",
    "immune_discount",
    "blind_share",
];

/// The levers that may be below zero.
const SIGNED: &[&str] = &[
    "audience_refused_below",
    "warn_below",
    "break_below",
    "war_below",
    "trespass_break_below",
    "trespass_war_below",
];

/// The levers that must be more than 0: the AI divides by them, steps by them, or needs a range.
const ABOVE_ZERO: &[&str] = &[
    "immune_discount",
    "mana_per_soldier",
    "most_paying_turns",
    "split_step",
    "cities_per_forge",
    "cities_per_engineer",
    "fame_per_point",
    "mood_div",
    "built_per_point",
    "enemy_range",
];

impl StrategyAiRules {
    /// Every lever, with its group, the prefix of its override name and its name.
    pub fn each(&mut self, f: &mut dyn FnMut(&'static str, &'static str, &'static str, Lever<'_>)) {
        self.shared.each(&mut |n, l| f("shared", "", n, l));
        self.attack.each(&mut |n, l| f("attack", "", n, l));
        self.target.each(&mut |n, l| f("target", "", n, l));
        self.garrison.each(&mut |n, l| f("garrison", "", n, l));
        self.scouting.each(&mut |n, l| f("scouting", "", n, l));
        self.sites.each(&mut |n, l| f("sites", "", n, l));
        self.city_site
            .each(&mut |n, l| f("city_site", "CITY_SITE_", n, l));
        self.economy.each(&mut |n, l| f("economy", "", n, l));
        self.building.each(&mut |n, l| f("building", "", n, l));
        self.forge.each(&mut |n, l| f("forge", "", n, l));
        self.magic.each(&mut |n, l| f("magic", "", n, l));
        self.split.each(&mut |n, l| f("split", "", n, l));
        self.items.each(&mut |n, l| f("items", "", n, l));
        self.battle_spend
            .each(&mut |n, l| f("battle_spend", "", n, l));
        self.engineers.each(&mut |n, l| f("engineers", "", n, l));
        self.transport.each(&mut |n, l| f("transport", "", n, l));
        self.care.each(&mut |n, l| f("care", "", n, l));
        self.planning.each(&mut |n, l| f("planning", "", n, l));
        self.diplomacy
            .each(&mut |n, l| f("diplomacy", "DIPLO_", n, l));
        self.trade
            .each(&mut |n, l| f("trade", "DIPLO_TRADE_", n, l));
        self.diplomacy_b
            .each(&mut |n, l| f("diplomacy_b", "DIPLO_B_", n, l));
        self.raze.each(&mut |n, l| f("raze", "RAZE_", n, l));
        self.objectives
            .pragmatist
            .each(&mut |n, l| f("pragmatist", "PRAGMATIST_", n, l));
        self.objectives
            .militarist
            .each(&mut |n, l| f("militarist", "MILITARIST_", n, l));
        self.objectives
            .theurgist
            .each(&mut |n, l| f("theurgist", "THEURGIST_", n, l));
        self.objectives
            .perfectionist
            .each(&mut |n, l| f("perfectionist", "PERFECTIONIST_", n, l));
        self.objectives
            .expansionist
            .each(&mut |n, l| f("expansionist", "EXPANSIONIST_", n, l));
    }

    /// Set the lever whose override name is `name` (in any case) to what `text` says. `CITIES` and
    /// `STRIKE` set a number of every objective. Nothing changes on an error.
    pub fn set(&mut self, name: &str, text: &str) -> Result<(), OverrideError> {
        let every_objective = [("CITIES", "cities_wanted"), ("STRIKE", "strike_base")]
            .into_iter()
            .find(|(short, _)| short.eq_ignore_ascii_case(name))
            .map(|(_, lever)| lever);
        let mut next = self.clone();
        let mut result = Err(OverrideError::UnknownLever);
        next.each(&mut |_, prefix, lever_name, lever| {
            let named = match every_objective {
                Some(wanted) => lever_name == wanted,
                None => {
                    name.get(..prefix.len())
                        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
                        && name[prefix.len()..].eq_ignore_ascii_case(lever_name)
                }
            };
            if !named || result == Err(OverrideError::BadValue) {
                return;
            }
            result = lever.take(text, |v| ok(lever_name, v));
        });
        result?;
        *self = next;
        Ok(())
    }

    /// The problems of the numbers: one that its lever cannot take.
    pub fn problems(&self) -> Vec<ContentIssue> {
        let mut p = Issues::new();
        let mut copy = self.clone();
        copy.each(&mut |group, _, name, lever| {
            let v = lever.value();
            if !ok(name, v) {
                p.out_of_range(FILE, group, name, v, wanted(name, AT_MOST_ONE, ABOVE_ZERO));
            }
        });
        for (class, v) in self
            .city_site
            .land_values
            .iter()
            .filter(|(_, v)| !v.is_finite() || *v < 0.0)
        {
            p.out_of_range(
                FILE,
                "land_values",
                &format!("{class:?}"),
                v,
                "a number of 0 or more",
            );
        }
        p.into_vec()
    }
}

/// Whether the lever `name` may be `v`.
fn ok(name: &str, v: f64) -> bool {
    allows(name, v, SIGNED.contains(&name), AT_MOST_ONE, ABOVE_ZERO)
}

impl CitySiteLevers {
    /// What a square of `class` is worth (food and growth); nothing for no square.
    pub fn land_value(&self, class: Option<TerrainClass>) -> f32 {
        class
            .and_then(|c| self.land_values.iter().find(|(k, _)| *k == c))
            .map_or(0.0, |&(_, v)| v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> StrategyAiRules {
        crate::testdir::shipped().rules.ai_strategy.clone()
    }

    /// The prefix and the name of every lever.
    fn names() -> Vec<(&'static str, &'static str)> {
        let mut found = Vec::new();
        shipped().each(&mut |_, prefix, name, _| found.push((prefix, name)));
        found
    }

    fn values(mut r: StrategyAiRules) -> Vec<(String, f64)> {
        let mut found = Vec::new();
        r.each(&mut |_, prefix, name, lever| {
            found.push((format!("{prefix}{}", name.to_uppercase()), lever.value()))
        });
        found
    }

    #[test]
    fn a_lever_has_an_override_name_of_its_own() {
        let mut all: Vec<String> = values(shipped()).into_iter().map(|(n, _)| n).collect();
        let count = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), count, "two levers share an override's name");
    }

    #[test]
    fn the_checked_names_are_names_of_levers() {
        for checked in AT_MOST_ONE.iter().chain(ABOVE_ZERO).chain(SIGNED) {
            assert!(
                names().iter().any(|(_, n)| n == checked),
                "{checked} is no lever"
            );
        }
    }

    #[test]
    fn an_override_sets_the_lever_it_names_and_no_other() {
        let before = values(shipped());
        for (override_name, _) in &before {
            let mut after = shipped();
            // 1 is a value every lever may take.
            assert_eq!(after.set(override_name, "1"), Ok(()), "{override_name}");
            for ((name, was), (_, is)) in before.iter().zip(values(after)) {
                let expected = if name == override_name { 1.0 } else { *was };
                assert_eq!(is, expected, "{override_name} moved {name}");
            }
        }
    }

    #[test]
    fn the_short_names_set_a_number_of_every_objective() {
        for (short, lever) in [("CITIES", "CITIES_WANTED"), ("strike", "STRIKE_BASE")] {
            let mut after = shipped();
            assert_eq!(after.set(short, "1"), Ok(()), "{short}");
            for ((name, was), (_, is)) in values(shipped()).iter().zip(values(after)) {
                let expected = if name.ends_with(lever) { 1.0 } else { *was };
                assert_eq!(is, expected, "{short}: {name}");
            }
        }
    }

    #[test]
    fn a_name_without_its_prefix_or_a_value_out_of_range_is_not_taken() {
        let mut levers = shipped();
        assert_eq!(levers.set("OFFER", "1"), Err(OverrideError::UnknownLever));
        assert_eq!(
            levers.set("NO_SUCH_LEVER", "1"),
            Err(OverrideError::UnknownLever)
        );
        assert_eq!(
            levers.set("ATTACK_FORCE", "-1"),
            Err(OverrideError::BadValue)
        );
        assert_eq!(levers.set("CITIES", "-1"), Err(OverrideError::BadValue));
        assert_eq!(levers, shipped());
        assert_eq!(levers.set("diplo_b_war_below", "-90"), Ok(()));
        assert_eq!(levers.diplomacy_b.war_below, -90);
    }
}
