//! What a soak game measures about each wizard: counts taken from the game's events as they
//! happen, and the standing at a given moment. Used by `soak` (one line per game) and
//! `soak-versus` (the AI versions side by side).

use std::collections::{BTreeMap, BTreeSet};
use std::ops::AddAssign;

use f4x_battle::battle::BattleEvent;
use f4x_core::city::production::ProductionItem;
use f4x_core::diplomacy::{DiplomacyEvent, Proposal, ProposalKind, Treaty};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::player::PlayerId;
use f4x_core::world::{Event, World};
use f4x_def::rules::{CastContext, MovementType, Rules, Stat, UnitDef};
use serde::{Deserialize, Serialize};

/// Whether a unit kind is a ship: it sails and carries land units.
fn is_ship(def: &UnitDef) -> bool {
    def.movement == MovementType::Sailing && def.stats.get(&Stat::Transport).is_some_and(|&t| t > 0)
}

/// A war between two wizards is stalled when no battle and no city taken between them is this many
/// turns old (task 561; the plan clock of task 515 is 100 turns too).
const STALLED_WAR_TURNS: u32 = 100;

/// What the other side of a battle is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Wizard,
    /// A neutral city: the player of a race.
    Neutral,
    /// A lair, tower or node: the monsters.
    Lair,
}

fn kind_of(player: PlayerId) -> Kind {
    if player == f4x_core::player::MONSTERS {
        Kind::Lair
    } else if player.0 >= f4x_core::player::race_player::FIRST_RACE_PLAYER {
        Kind::Neutral
    } else {
        Kind::Wizard
    }
}

/// The key of two players in the tables of wars: the lower number first.
fn pair(a: PlayerId, b: PlayerId) -> (u8, u8) {
    (a.0.min(b.0), a.0.max(b.0))
}

/// Counts over a whole game for one wizard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metrics {
    pub cities_founded: u32,
    pub cities_captured: u32,
    pub cities_lost: u32,
    pub battles_won: u32,
    pub battles_lost: u32,
    pub units_built: u32,
    /// Units lost outside a battle: no food, no gold, no mana, wounds (`UnitLost`).
    pub units_lost: u32,
    /// Units of the army that fell in a battle or were lost fleeing from one.
    pub units_lost_in_battle: u32,
    pub buildings_built: u32,
    pub spells_cast: u32,
    pub spells_learned: u32,
    pub heroes_joined: u32,
    pub mercenaries_hired: u32,
    pub items_made: u32,
    pub items_sold: u32,
    pub items_scrapped: u32,
    pub wars_declared: u32,
    /// Wizards the player has met.
    pub contacts_made: u32,
    /// Diplomatic actions the player started: proposals, warnings, gifts, broken treaties and wars.
    pub diplomacy_started: u32,
    /// The kinds of diplomatic action, each counted for the wizard that started it
    /// (`diplomacy_started` is their total, with the wars declared). The answers are counted for
    /// the proposer; a treaty joined is counted for both wizards.
    pub peace_offers: u32,
    pub pact_offers: u32,
    pub alliance_offers: u32,
    pub war_requests: u32,
    pub alliance_break_requests: u32,
    /// Trades offered (a spell swap, gold, an item or a unit), and the ones answered yes: all, those
    /// with gold, and those with an item or a unit.
    pub spell_exchanges: u32,
    /// Trades answered with a trade of the other's own (a negotiation round).
    pub trade_counters: u32,
    pub trades_done: u32,
    pub trades_gold: u32,
    pub trades_goods: u32,
    pub threats: u32,
    pub demands: u32,
    pub warnings: u32,
    pub gold_gifts: u32,
    pub spell_gifts: u32,
    pub treaties_broken: u32,
    pub proposals_accepted: u32,
    pub proposals_refused: u32,
    pub treaties_joined: u32,
    /// What the AI said it did and why (`DiplomacyCommand::Remark`, task 510), counted for the
    /// wizard that said it: wars begun on a much weaker neighbour, gifts and pacts offered to a
    /// much stronger one, alliances asked against the leader, peace offered by a wizard tired of war.
    pub dominant_wars: u32,
    pub appeasements: u32,
    pub coalition_asks: u32,
    pub weary_peace_offers: u32,
    pub refused_orders: u32,
    /// Orders the game refused because of a pact or an alliance (an attack or a move onto a partner): the AI
    /// declares war first, so this stays at zero (task 776).
    pub treaty_refusals: u32,
    pub ships_built: u32,
    pub ships_lost: u32,
    /// Land units that stepped from a ship onto land.
    pub troops_landed: u32,
    /// Cities taken on a landmass where the wizard had landed troops.
    pub cities_by_sea: u32,
    /// 1 when the wizard landed troops at all.
    pub wizards_landed: u32,
    /// The turn of the wizard's first landing (summed over wizards, see `first_landing`).
    pub first_landing_turns: u32,
    /// Land units that changed plane (a tower, a gate or Plane Shift).
    pub plane_crossings: u32,
    /// Plane Shift casts that carried a stack over.
    pub plane_shifts: u32,
    /// Sites cleared of their guards or taken empty, and of those, towers of wisdom.
    pub sites_cleared: u32,
    pub towers_cleared: u32,
    /// Turns the wizard had a spell to research and its research income was nil.
    pub research_stalled: u32,
    /// Items finished by paying gold (`Command::BuyProduction`), and the gold they cost.
    pub production_rushed: u32,
    pub gold_rushed: u32,
    /// Rival wizards whose last city this wizard took (`Tally::defeats` says which).
    pub wizards_eliminated: u32,
    /// Cities taken on the plane the wizard did not start on (`Tally::far_captures` says which).
    pub cities_captured_far_plane: u32,
    /// Marches the AI sent against a city, the soldiers on them, the marches called off for
    /// going on too long without a fight, and those dropped because the target changed
    /// (task 436).
    pub marches_launched: u32,
    pub march_units: u32,
    pub marches_called_off: u32,
    pub marches_dropped: u32,
    /// Times the war plan swapped its goal, or its next target, for another town.
    pub plan_goal_changes: u32,
    pub plan_next_changes: u32,
    /// Turns the marches called off for going on too long had run, and the soldiers on them
    /// (task 436: why a march is called off).
    pub march_called_off_turns: u32,
    pub march_called_off_units: u32,
    /// Turns at war with a rival wizard (each rival counts), and those of them in which the two
    /// had fought no battle and taken no city for `STALLED_WAR_TURNS` turns (task 561).
    pub war_turns: u32,
    pub stalled_war_turns: u32,
    /// Soldiers the end-of-turn audit found with no role (sent home), summed over turns.
    pub units_adrift: u32,
    /// Ships and other units (settlers, engineers, spirits) with no role, summed over turns.
    pub units_unassigned: u32,
    /// Units that came into being during the turn with no registry entry (summoned, hired); they
    /// await orders, and the code that makes them should register them (task 645).
    pub units_unregistered: u32,
    /// Units whose mission was over (a rival cleared the lair, the order stopped) and that were
    /// reset to awaiting orders by the dispatcher.
    pub missions_reset: u32,
    /// Units tagged for a ship that had not yet taken them aboard, summed over turns (unit-turns
    /// of waiting for a ship, task 647).
    pub waiting_for_ship: u32,
    /// Ships that held the post of a bridge (on their way to it or parked), summed over turns
    /// (ship-turns of a bridge, task 738).
    pub bridge_ships: u32,
    /// Units tagged for a bridge that did not yet lead to their target, summed over turns
    /// (unit-turns of waiting for a bridge, task 738).
    pub waiting_for_bridge: u32,
    /// Cities with no unit in them but engineers, settlers, mana spirits and ships, summed over
    /// rounds (city-turns undefended).
    pub undefended_cities: u32,
    /// Settlers that left the map without founding a city: killed (in a battle, or otherwise gone
    /// with no word from the game), let go for lack of food, let go for lack of gold.
    pub settlers_killed: u32,
    pub settlers_starved: u32,
    pub settlers_unpaid: u32,
    /// Units of any kind let go for lack of food (the settlers among them too).
    pub units_starved: u32,
    /// Battles by who attacked and who the other side was: a wizard, a neutral city (a race's
    /// player) or a lair (the monsters). Counted for the wizard on each side.
    pub attack_won_wizard: u32,
    pub attack_lost_wizard: u32,
    pub defense_won_wizard: u32,
    pub defense_lost_wizard: u32,
    pub attack_won_neutral: u32,
    pub attack_lost_neutral: u32,
    pub defense_won_neutral: u32,
    pub defense_lost_neutral: u32,
    pub attack_won_lair: u32,
    pub attack_lost_lair: u32,
    pub defense_won_lair: u32,
    pub defense_lost_lair: u32,
}

/// The counts that mean nothing for a neutral player: it has no magic, so no spells cast or
/// learned and no research. It follows the wizards' rules for everything else (heroes, items,
/// diplomacy, trades).
const WIZARD_ONLY: &[&str] = &["spells cast", "spells learned", "research stalled turns"];

impl Metrics {
    /// The counts that apply to a neutral player, in the order of [`Metrics::rows`]: all but the
    /// ones only a wizard has (`WIZARD_ONLY`).
    pub fn neutral_rows(&self) -> Vec<(&'static str, u32)> {
        self.rows()
            .into_iter()
            .filter(|(label, _)| !WIZARD_ONLY.contains(label))
            .collect()
    }

    /// Each count with its name, in the order a report shows them.
    pub fn rows(&self) -> [(&'static str, u32); 92] {
        [
            ("cities founded", self.cities_founded),
            ("cities captured", self.cities_captured),
            ("cities lost", self.cities_lost),
            ("battles won", self.battles_won),
            ("battles lost", self.battles_lost),
            ("units built", self.units_built),
            ("units lost outside battle", self.units_lost),
            ("units lost in battle", self.units_lost_in_battle),
            ("buildings built", self.buildings_built),
            ("spells cast", self.spells_cast),
            ("spells learned", self.spells_learned),
            ("heroes joined", self.heroes_joined),
            ("mercenaries hired", self.mercenaries_hired),
            ("items made", self.items_made),
            ("items sold", self.items_sold),
            ("items scrapped", self.items_scrapped),
            ("wars declared", self.wars_declared),
            ("wizards met", self.contacts_made),
            ("diplomacy started", self.diplomacy_started),
            ("peace offers", self.peace_offers),
            ("pact offers", self.pact_offers),
            ("alliance offers", self.alliance_offers),
            ("war requests", self.war_requests),
            ("alliance-break requests", self.alliance_break_requests),
            ("trades offered", self.spell_exchanges),
            ("trade counters", self.trade_counters),
            ("trades done", self.trades_done),
            ("trades with gold", self.trades_gold),
            ("trades of items, units or heroes", self.trades_goods),
            ("threats", self.threats),
            ("tribute demands", self.demands),
            ("warnings", self.warnings),
            ("gold gifts", self.gold_gifts),
            ("spell gifts", self.spell_gifts),
            ("treaties broken", self.treaties_broken),
            ("proposals accepted", self.proposals_accepted),
            ("proposals refused or lapsed", self.proposals_refused),
            ("pacts and alliances joined", self.treaties_joined),
            ("wars on a weaker neighbour", self.dominant_wars),
            ("appeasements", self.appeasements),
            ("coalition requests", self.coalition_asks),
            ("war-weary peace offers", self.weary_peace_offers),
            ("refused orders", self.refused_orders),
            ("refused by a treaty", self.treaty_refusals),
            ("ships built", self.ships_built),
            ("ships lost", self.ships_lost),
            ("troops landed", self.troops_landed),
            ("cities by sea", self.cities_by_sea),
            ("wizards landed", self.wizards_landed),
            ("plane crossings", self.plane_crossings),
            ("plane shifts", self.plane_shifts),
            ("sites cleared", self.sites_cleared),
            ("towers cleared", self.towers_cleared),
            ("research stalled turns", self.research_stalled),
            ("production rushed", self.production_rushed),
            ("gold paid for rushing", self.gold_rushed),
            ("wizards eliminated", self.wizards_eliminated),
            ("marches launched", self.marches_launched),
            ("soldiers sent on marches", self.march_units),
            ("marches called off", self.marches_called_off),
            ("marches dropped for another target", self.marches_dropped),
            ("war plan goal changes", self.plan_goal_changes),
            ("war plan next-target changes", self.plan_next_changes),
            ("turns of marches called off", self.march_called_off_turns),
            (
                "soldiers on marches called off",
                self.march_called_off_units,
            ),
            ("turns at war with a wizard", self.war_turns),
            ("turns of stalled war", self.stalled_war_turns),
            ("units adrift (no role)", self.units_adrift),
            ("ships and other units with no role", self.units_unassigned),
            (
                "units made in the turn with no registry entry",
                self.units_unregistered,
            ),
            (
                "missions over, unit reset to awaiting orders",
                self.missions_reset,
            ),
            ("unit-turns waiting for a ship", self.waiting_for_ship),
            ("ship-turns held as a bridge", self.bridge_ships),
            ("unit-turns waiting for a bridge", self.waiting_for_bridge),
            ("city-turns undefended", self.undefended_cities),
            ("settlers lost in battle", self.settlers_killed),
            ("settlers let go for lack of food", self.settlers_starved),
            ("settlers let go for lack of gold", self.settlers_unpaid),
            ("units let go for lack of food", self.units_starved),
            ("attacks won on a wizard", self.attack_won_wizard),
            ("attacks lost on a wizard", self.attack_lost_wizard),
            ("defenses won against a wizard", self.defense_won_wizard),
            ("defenses lost against a wizard", self.defense_lost_wizard),
            ("attacks won on a neutral city", self.attack_won_neutral),
            ("attacks lost on a neutral city", self.attack_lost_neutral),
            (
                "defenses won against a neutral city",
                self.defense_won_neutral,
            ),
            (
                "defenses lost against a neutral city",
                self.defense_lost_neutral,
            ),
            ("attacks won on a lair", self.attack_won_lair),
            ("attacks lost on a lair", self.attack_lost_lair),
            ("defenses won against a lair", self.defense_won_lair),
            ("defenses lost against a lair", self.defense_lost_lair),
            (
                "cities captured on the other plane",
                self.cities_captured_far_plane,
            ),
        ]
    }

    /// The average turn of the first landing over the wizards who landed.
    pub fn first_landing(&self) -> Option<f64> {
        (self.wizards_landed > 0)
            .then(|| f64::from(self.first_landing_turns) / f64::from(self.wizards_landed))
    }
}

/// What the wizards of one or more games did with one spell (see [`Tally::spell_rows`]); each count
/// is in wizards, one per wizard and game, except `casts`, which is every cast.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellRow {
    /// Wizards whose books let them research it (it was open to research, or they learned it).
    pub could: u32,
    /// Wizards that got it by research, trade or gift (not a starting spell).
    pub learned: u32,
    /// Wizards that knew it at the end, a starting spell too.
    pub knew: u32,
    /// Wizards that cast it, on the map or in a battle.
    pub casters: u32,
    /// Casts by wizards, on the map and in battles (a unit's own spell is not one).
    pub casts: u32,
}

impl SpellRow {
    /// Casts per wizard that knew the spell (and per game: a wizard is counted once a game).
    pub fn casts_per_knower(&self) -> f64 {
        if self.knew == 0 {
            0.0
        } else {
            f64::from(self.casts) / f64::from(self.knew)
        }
    }
}

/// The spell rows of several games added up.
pub fn add_spell_rows<'a>(
    rows: impl IntoIterator<Item = &'a BTreeMap<String, SpellRow>>,
) -> BTreeMap<String, SpellRow> {
    let mut sum: BTreeMap<String, SpellRow> = BTreeMap::new();
    for one in rows {
        for (spell, r) in one {
            let t = sum.entry(spell.clone()).or_default();
            t.could += r.could;
            t.learned += r.learned;
            t.knew += r.knew;
            t.casters += r.casters;
            t.casts += r.casts;
        }
    }
    sum
}

/// A table with a row for each spell of the rules: wizards that could research it, learned it,
/// knew it at the end, cast it, and the casts per wizard that knew it.
pub fn spell_table(rules: &Rules, rows: &BTreeMap<String, SpellRow>) -> String {
    let mut out = format!(
        "{:<28} {:>6} {:>8} {:>6} {:>7} {:>7} {:>9}\n",
        "spell", "could", "learned", "knew", "cast", "casts", "per knower"
    );
    for (_, def) in rules.spells.iter() {
        let r = rows.get(&def.id).copied().unwrap_or_default();
        out.push_str(&format!(
            "{:<28} {:>6} {:>8} {:>6} {:>7} {:>7} {:>9.2}\n",
            def.id,
            r.could,
            r.learned,
            r.knew,
            r.casters,
            r.casts,
            r.casts_per_knower()
        ));
    }
    out
}

/// The tries to cast a spell that came to nothing, by the spell's id and then by what stopped it
/// (see [`Tally::cast_failures`]).
pub type CastFailures = BTreeMap<String, BTreeMap<String, u32>>;

/// The failures of several games added up.
pub fn add_failures<'a>(all: impl IntoIterator<Item = &'a CastFailures>) -> CastFailures {
    let mut sum = CastFailures::new();
    for one in all {
        for (spell, reasons) in one {
            for (why, n) in reasons {
                *sum.entry(spell.clone())
                    .or_default()
                    .entry(why.clone())
                    .or_default() += n;
            }
        }
    }
    sum
}

/// One line per spell that failed, the most failures first: `spell total: reason n, reason n`,
/// then one line per reason over all spells.
pub fn failure_lines(failures: &CastFailures) -> String {
    let mut spells: Vec<(&String, u32, &BTreeMap<String, u32>)> = failures
        .iter()
        .map(|(s, r)| (s, r.values().sum(), r))
        .collect();
    spells.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let mut out = String::new();
    let mut by_reason: BTreeMap<&String, u32> = BTreeMap::new();
    for (spell, total, reasons) in spells {
        let mut why: Vec<_> = reasons.iter().collect();
        why.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        out.push_str(&format!(
            "  {spell} {total}: {}\n",
            why.iter()
                .map(|(w, n)| format!("{w} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for (w, n) in reasons {
            *by_reason.entry(w).or_default() += n;
        }
    }
    let mut all: Vec<_> = by_reason.into_iter().collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    out.push_str("  by reason: ");
    out.push_str(
        &all.iter()
            .map(|(w, n)| format!("{w} {n}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push('\n');
    out
}

/// The turns wizards had a situation for a kind of spell and knew none of the kind, and how many
/// wizards (a wizard counts once a game), by the kind's name (`Ai::unmet_spell_wants`).
pub type UnmetWants = BTreeMap<String, (u32, u32)>;

/// The wants of several games added up.
pub fn add_unmet<'a>(all: impl IntoIterator<Item = &'a UnmetWants>) -> UnmetWants {
    let mut sum = UnmetWants::new();
    for one in all {
        for (kind, (turns, wizards)) in one {
            let t = sum.entry(kind.clone()).or_default();
            t.0 += turns;
            t.1 += wizards;
        }
    }
    sum
}

/// One line per kind, the most turns first: `kind: turns (wizards)`.
pub fn unmet_line(unmet: &UnmetWants) -> String {
    let mut all: Vec<_> = unmet.iter().collect();
    all.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    all.iter()
        .map(|(k, (turns, wizards))| format!("{k} {turns} turns ({wizards} wizards)"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The casts of several games added up: `casts` is each tally's [`Tally::casts`].
pub fn add_casts<'a>(
    casts: impl IntoIterator<Item = &'a BTreeMap<String, u32>>,
) -> BTreeMap<String, u32> {
    let mut sum = BTreeMap::new();
    for one in casts {
        for (spell, n) in one {
            *sum.entry(spell.clone()).or_default() += n;
        }
    }
    sum
}

/// The ids of the spells that can be cast on the map and are not in `casts`.
pub fn never_cast(rules: &Rules, casts: &BTreeMap<String, u32>) -> Vec<String> {
    never_cast_in(rules, casts, CastContext::Overland)
}

/// The ids of the spells that can be cast in `context` and are not in `casts`.
pub fn never_cast_in(
    rules: &Rules,
    casts: &BTreeMap<String, u32>,
    context: CastContext,
) -> Vec<String> {
    rules
        .spells
        .iter()
        .map(|(_, def)| def)
        .filter(|def| def.contexts.contains(&context) && !casts.contains_key(&def.id))
        .map(|def| def.id.clone())
        .collect()
}

/// The spells some wizard knew and nobody cast, with the number of wizards that knew each, as
/// `id wizards` pairs in the order of the overland spells of the rules: where a spell never gets
/// its situation (or no intent), as against a spell nobody learned.
pub fn known_never_cast(
    rules: &Rules,
    known: &BTreeMap<String, u32>,
    casts: &BTreeMap<String, u32>,
    context: CastContext,
) -> String {
    rules
        .spells
        .iter()
        .map(|(_, def)| def)
        .filter(|d| d.contexts.contains(&context) && !casts.contains_key(&d.id))
        .filter_map(|d| known.get(&d.id).map(|n| format!("{} {n}", d.id)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `id count` pairs, the most cast first.
pub fn casts_line(casts: &BTreeMap<String, u32>) -> String {
    let mut all: Vec<_> = casts.iter().collect();
    all.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    all.iter()
        .map(|(spell, n)| format!("{spell} {n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl AddAssign for Metrics {
    fn add_assign(&mut self, other: Metrics) {
        let Metrics {
            cities_founded,
            cities_captured,
            cities_lost,
            battles_won,
            battles_lost,
            units_built,
            units_lost,
            units_lost_in_battle,
            buildings_built,
            spells_cast,
            spells_learned,
            heroes_joined,
            mercenaries_hired,
            items_made,
            items_sold,
            items_scrapped,
            wars_declared,
            contacts_made,
            diplomacy_started,
            peace_offers,
            pact_offers,
            alliance_offers,
            war_requests,
            alliance_break_requests,
            spell_exchanges,
            trade_counters,
            trades_done,
            trades_gold,
            trades_goods,
            threats,
            demands,
            warnings,
            gold_gifts,
            spell_gifts,
            treaties_broken,
            proposals_accepted,
            proposals_refused,
            treaties_joined,
            dominant_wars,
            appeasements,
            coalition_asks,
            weary_peace_offers,
            refused_orders,
            treaty_refusals,
            ships_built,
            ships_lost,
            troops_landed,
            cities_by_sea,
            wizards_landed,
            first_landing_turns,
            plane_crossings,
            plane_shifts,
            sites_cleared,
            towers_cleared,
            research_stalled,
            production_rushed,
            gold_rushed,
            wizards_eliminated,
            cities_captured_far_plane,
            marches_launched,
            march_units,
            marches_called_off,
            marches_dropped,
            plan_goal_changes,
            plan_next_changes,
            march_called_off_turns,
            march_called_off_units,
            war_turns,
            stalled_war_turns,
            units_adrift,
            units_unassigned,
            units_unregistered,
            missions_reset,
            waiting_for_ship,
            bridge_ships,
            waiting_for_bridge,
            undefended_cities,
            settlers_killed,
            settlers_starved,
            settlers_unpaid,
            units_starved,
            attack_won_wizard,
            attack_lost_wizard,
            defense_won_wizard,
            defense_lost_wizard,
            attack_won_neutral,
            attack_lost_neutral,
            defense_won_neutral,
            defense_lost_neutral,
            attack_won_lair,
            attack_lost_lair,
            defense_won_lair,
            defense_lost_lair,
        } = other;
        self.cities_founded += cities_founded;
        self.cities_captured += cities_captured;
        self.cities_lost += cities_lost;
        self.battles_won += battles_won;
        self.battles_lost += battles_lost;
        self.units_built += units_built;
        self.units_lost += units_lost;
        self.units_lost_in_battle += units_lost_in_battle;
        self.buildings_built += buildings_built;
        self.spells_cast += spells_cast;
        self.spells_learned += spells_learned;
        self.heroes_joined += heroes_joined;
        self.mercenaries_hired += mercenaries_hired;
        self.items_made += items_made;
        self.items_sold += items_sold;
        self.items_scrapped += items_scrapped;
        self.wars_declared += wars_declared;
        self.contacts_made += contacts_made;
        self.diplomacy_started += diplomacy_started;
        self.peace_offers += peace_offers;
        self.pact_offers += pact_offers;
        self.alliance_offers += alliance_offers;
        self.war_requests += war_requests;
        self.alliance_break_requests += alliance_break_requests;
        self.spell_exchanges += spell_exchanges;
        self.trade_counters += trade_counters;
        self.trades_done += trades_done;
        self.trades_gold += trades_gold;
        self.trades_goods += trades_goods;
        self.threats += threats;
        self.demands += demands;
        self.warnings += warnings;
        self.gold_gifts += gold_gifts;
        self.spell_gifts += spell_gifts;
        self.treaties_broken += treaties_broken;
        self.proposals_accepted += proposals_accepted;
        self.proposals_refused += proposals_refused;
        self.treaties_joined += treaties_joined;
        self.dominant_wars += dominant_wars;
        self.appeasements += appeasements;
        self.coalition_asks += coalition_asks;
        self.weary_peace_offers += weary_peace_offers;
        self.refused_orders += refused_orders;
        self.treaty_refusals += treaty_refusals;
        self.ships_built += ships_built;
        self.ships_lost += ships_lost;
        self.troops_landed += troops_landed;
        self.cities_by_sea += cities_by_sea;
        self.wizards_landed += wizards_landed;
        self.plane_crossings += plane_crossings;
        self.plane_shifts += plane_shifts;
        self.sites_cleared += sites_cleared;
        self.towers_cleared += towers_cleared;
        self.research_stalled += research_stalled;
        self.production_rushed += production_rushed;
        self.gold_rushed += gold_rushed;
        self.wizards_eliminated += wizards_eliminated;
        self.cities_captured_far_plane += cities_captured_far_plane;
        self.marches_launched += marches_launched;
        self.march_units += march_units;
        self.marches_called_off += marches_called_off;
        self.marches_dropped += marches_dropped;
        self.plan_goal_changes += plan_goal_changes;
        self.plan_next_changes += plan_next_changes;
        self.march_called_off_turns += march_called_off_turns;
        self.march_called_off_units += march_called_off_units;
        self.war_turns += war_turns;
        self.stalled_war_turns += stalled_war_turns;
        self.units_adrift += units_adrift;
        self.units_unassigned += units_unassigned;
        self.units_unregistered += units_unregistered;
        self.missions_reset += missions_reset;
        self.waiting_for_ship += waiting_for_ship;
        self.bridge_ships += bridge_ships;
        self.waiting_for_bridge += waiting_for_bridge;
        self.undefended_cities += undefended_cities;
        self.settlers_killed += settlers_killed;
        self.settlers_starved += settlers_starved;
        self.settlers_unpaid += settlers_unpaid;
        self.units_starved += units_starved;
        self.attack_won_wizard += attack_won_wizard;
        self.attack_lost_wizard += attack_lost_wizard;
        self.defense_won_wizard += defense_won_wizard;
        self.defense_lost_wizard += defense_lost_wizard;
        self.attack_won_neutral += attack_won_neutral;
        self.attack_lost_neutral += attack_lost_neutral;
        self.defense_won_neutral += defense_won_neutral;
        self.defense_lost_neutral += defense_lost_neutral;
        self.attack_won_lair += attack_won_lair;
        self.attack_lost_lair += attack_lost_lair;
        self.defense_won_lair += defense_won_lair;
        self.defense_lost_lair += defense_lost_lair;
        self.first_landing_turns += first_landing_turns;
    }
}

/// The counts of every player of a game, by player number. The last slot holds whoever has no
/// number among the players (the monsters that win a battle, say), so the totals add up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tally {
    players: Vec<Metrics>,
    /// The ships each player had when the game began.
    ships_at_start: Vec<u32>,
    /// The landmasses (plane, number) each player has landed troops on.
    landed_on: Vec<BTreeSet<(usize, u32)>>,
    /// How often each spell was cast by anyone, by the spell's id.
    #[serde(default)]
    casts: BTreeMap<String, u32>,
    /// The same for the spells cast in battles, by a wizard or by a unit (a hero, a creature, an
    /// item's charge): `Event::SpellCast` is the map's, a battle has its own events.
    #[serde(default)]
    battle_casts: BTreeMap<String, u32>,
    /// The mana a wizard paid from its reserve for each spell cast in battle, by the spell's id
    /// (a unit's own mana is not counted: it is not the reserve).
    #[serde(default)]
    battle_mana: BTreeMap<String, i64>,
    /// How many wizards know each spell at the end of the game, by the spell's id.
    #[serde(default)]
    known: BTreeMap<String, u32>,
    /// What each spell's wizards did with it, by the spell's id (see [`Tally::note_known`]).
    #[serde(default)]
    spell_rows: BTreeMap<String, SpellRow>,
    /// The tries to cast a spell that came to nothing, by the spell's id and then by what stopped
    /// it, with where it happened (`map` or `battle`): a command the game refused (not enough
    /// mana, no target, banished), a spell that failed or fizzled when it came to its target, a
    /// battle spell countered, a map spell suppressed.
    #[serde(default)]
    cast_failures: CastFailures,
    /// The situations that called for a kind of spell the wizard did not know (see [`UnmetWants`]).
    #[serde(default)]
    unmet_wants: UnmetWants,
    /// The casts of one game by wizard (player number) and spell id; folded into `spell_rows` by
    /// `note_known`.
    #[serde(skip)]
    cast_by: BTreeMap<(u8, String), u32>,
    /// Per wizard (by player number): the mana spent over the game (net of what Alchemy added),
    /// and the pool after the last round.
    #[serde(default)]
    mana_spent: Vec<i64>,
    #[serde(default)]
    mana_left: Vec<i64>,
    /// Who took the last city each player lost, by player number: the taker of a wizard's last
    /// city is the one who defeated it.
    #[serde(default)]
    last_taker: BTreeMap<u8, u8>,
    /// The turn of the last battle or city taken between two players (the lower number first),
    /// and the turn each war between wizards began, for the stalled-war count.
    #[serde(default)]
    last_fight: BTreeMap<(u8, u8), u32>,
    /// The attacker, the defender and the square of the battle being fought.
    open_battle: Option<(PlayerId, PlayerId, String)>,
    /// The attacks a wizard lost on something that is no wizard (monsters, a neutral city), by
    /// wizard and place: the count.
    #[serde(skip)]
    lost_on_others: BTreeMap<(u8, String), u32>,
    #[serde(default)]
    war_since: BTreeMap<(u8, u8), u32>,
    /// The wizards defeated: the turn, the wizard, and who took its last city (`None` when it
    /// lost its last city some other way, razed, destroyed or lost to rebels).
    #[serde(default)]
    defeats: Vec<(u32, u8, Option<u8>)>,
    /// The wars declared: the turn, the wizard who declared and the one it declared on.
    #[serde(default)]
    wars: Vec<(u32, u8, u8)>,
    /// The plane (index) each wizard started on, by player number.
    #[serde(default)]
    home_plane: Vec<Option<usize>>,
    /// Cities taken on the plane the taker did not start on: the turn, the taker, the former
    /// owner and the plane (index).
    #[serde(default)]
    far_captures: Vec<(u32, u8, u8, usize)>,
    /// Each city (outposts too) found undefended at a sample: the turn, the owner and the city's
    /// square.
    #[serde(default)]
    undefended: Vec<(u32, u8, f4x_core::map::Position)>,
    /// The settlers on the map at the last sample, with their owner and square, to see which one
    /// has gone, and the settlers the game let go of for lack of food or gold since then (by owner).
    #[serde(skip)]
    settlers_seen: Vec<(
        f4x_core::world::serial::Serial,
        PlayerId,
        f4x_core::map::Position,
    )>,
    #[serde(skip)]
    settlers_let_go: Vec<(PlayerId, f4x_core::world::event::LossCause)>,
    /// The cities founded since the last sample, with their owner and square.
    #[serde(skip)]
    founded_now: Vec<(PlayerId, f4x_core::map::Position)>,
}

impl Tally {
    pub fn new(world: &World) -> Self {
        // The neutral players of the races are made when their first city turns neutral, so
        // leave a slot for each of them: they must not be counted with the monsters.
        let races = f4x_core::player::race_player::FIRST_RACE_PLAYER as usize
            + world.content().rules.races.iter().count();
        let count = world
            .player_ids()
            .into_iter()
            .map(|p| usize::from(p.0) + 1)
            .max()
            .unwrap_or(0)
            .max(races);
        let mut tally = Self {
            players: vec![Metrics::default(); count + 1],
            ships_at_start: vec![0; count + 1],
            landed_on: vec![BTreeSet::new(); count + 1],
            casts: BTreeMap::new(),
            battle_casts: BTreeMap::new(),
            spell_rows: BTreeMap::new(),
            cast_by: BTreeMap::new(),
            cast_failures: CastFailures::new(),
            unmet_wants: UnmetWants::new(),
            battle_mana: BTreeMap::new(),
            known: BTreeMap::new(),
            mana_spent: vec![0; count + 1],
            mana_left: vec![0; count + 1],
            last_taker: BTreeMap::new(),
            last_fight: BTreeMap::new(),
            open_battle: None,
            lost_on_others: BTreeMap::new(),
            war_since: BTreeMap::new(),
            defeats: Vec::new(),
            wars: Vec::new(),
            home_plane: (0..=count)
                .map(|k| {
                    world
                        .wizard(PlayerId(k as u8))
                        .and_then(|w| w.fortress())
                        .map(|at| at.plane.index())
                })
                .collect(),
            far_captures: Vec::new(),
            undefended: Vec::new(),
            settlers_seen: Vec::new(),
            settlers_let_go: Vec::new(),
            founded_now: Vec::new(),
        };
        tally.mana_left = tally.mana_now(world);
        tally.ships_at_start = tally.ships_now(world);
        tally
    }

    /// The ships of each player, by slot.
    fn ships_now(&self, world: &World) -> Vec<u32> {
        let mut ships = vec![0; self.players.len()];
        for p in world.player_ids() {
            let slot = usize::from(p.0).min(ships.len() - 1);
            ships[slot] += world
                .units()
                .player_units(p)
                .iter()
                .filter(|&&u| world.units().unit_def(u).is_some_and(is_ship))
                .count() as u32;
        }
        ships
    }

    /// The mana pool of each player, by slot.
    fn mana_now(&self, world: &World) -> Vec<i64> {
        let mut mana = vec![0; self.players.len()];
        for p in world.player_ids() {
            if world.wizard(p).is_some() {
                let slot = usize::from(p.0).min(mana.len() - 1);
                mana[slot] = world.player(p).map_or(0, |q| q.treasury().mana.floor()) as i64;
            }
        }
        mana
    }

    /// Note the mana of every wizard after a round: what it spent is the pool before, plus the
    /// income the turn change paid, less the pool now.
    pub fn sample_mana(&mut self, world: &World, round: u32) {
        self.sample_wars(world, round);
        self.sample_settlers(world);
        self.sample_cities(world, round);
        let now = self.mana_now(world);
        for p in world.player_ids() {
            if world.wizard(p).is_none() {
                continue;
            }
            let slot = usize::from(p.0).min(now.len() - 1);
            let income = world.income(p);
            let paid = (income.split.mana - income.mana_upkeep).floor() as i64;
            // Casting the Spell of Mastery takes all the power by design (the AI's split is
            // 50/0/50 then): research does not count as stalled.
            let mastery = world.wizard(p).is_some_and(|w| {
                w.casting().is_some_and(|c| {
                    world
                        .content()
                        .rules
                        .spells
                        .get(c.spell)
                        .has_hook(f4x_core::spell::special::SPELL_OF_MASTERY)
                })
            });
            // No research is no fault of the wizard's split when it is defeated, when the Mana Short
            // event dries up every source of power, or while it casts the Spell of Return.
            let blameless = world.player(p).is_some_and(|q| q.is_defeated())
                || world.events().mana_is_short()
                || world.casting_return(p);
            if !mastery
                && !blameless
                && world.wizard(p).is_some_and(|w| w.researching().is_some())
                && income.research <= 0
            {
                self.bump(p, |m| m.research_stalled += 1);
            }
            let before = self.mana_left.get(slot).copied().unwrap_or(0);
            if let Some(spent) = self.mana_spent.get_mut(slot) {
                *spent += (before + paid - now[slot]).max(0);
            }
        }
        self.mana_left = now;
    }

    /// Count a turn of war for each pair of wizards at war, and a stalled one when neither a
    /// battle nor a city taken between them is newer than `STALLED_WAR_TURNS` turns.
    /// A settler that was on the map at the last sample and is not now either founded a city on
    /// its square, was let go for lack of food or gold (the game said so), or was killed.
    fn sample_settlers(&mut self, world: &World) {
        use f4x_core::world::event::LossCause;
        let now: Vec<(
            f4x_core::world::serial::Serial,
            PlayerId,
            f4x_core::map::Position,
        )> = world
            .units()
            .all_units()
            .into_iter()
            .filter(|&u| world.units().unit_is_settler(u))
            .filter_map(|u| {
                Some((
                    world.units().serial_of(u),
                    world.units().unit_owner(u)?,
                    world.units().unit_position(u)?,
                ))
            })
            .collect();
        let gone: Vec<_> = self
            .settlers_seen
            .iter()
            .filter(|(u, ..)| !now.iter().any(|(n, ..)| n == u))
            .copied()
            .collect();
        let mut let_go = std::mem::take(&mut self.settlers_let_go);
        let mut founded = std::mem::take(&mut self.founded_now);
        for (_, owner, at) in gone {
            if let Some(i) = founded.iter().position(|&(p, pos)| p == owner && pos == at) {
                founded.remove(i);
                continue; // it founded a city
            }
            let cause = let_go
                .iter()
                .position(|&(p, _)| p == owner)
                .map(|i| let_go.remove(i).1);
            self.bump(owner, |m| match cause {
                Some(LossCause::Food) => m.settlers_starved += 1,
                Some(_) => m.settlers_unpaid += 1,
                None => m.settlers_killed += 1,
            });
        }
        self.settlers_seen = now;
    }

    /// Count each city (a neutral one too: it is the neutrals' share) that holds no unit but
    /// engineers, settlers, mana spirits and ships (the units that cannot defend it), once per round, and keep which city on which turn. An
    /// outpost is a city, so it counts.
    fn sample_cities(&mut self, world: &World, round: u32) {
        // A ship defends nothing, and neither do the workers.
        let defends = |u| {
            world
                .units()
                .unit_def(u)
                .is_some_and(|d| !d.is_worker() && !is_ship(d))
        };
        for (_, info, at) in world.economy().cities() {
            let held = world
                .economy()
                .army_at(at)
                .into_iter()
                .any(|u| world.units().unit_owner(u) == Some(info.owner) && defends(u));
            if !held {
                self.bump(info.owner, |m| m.undefended_cities += 1);
                self.undefended.push((round, info.owner.0, at));
            }
        }
    }

    fn sample_wars(&mut self, world: &World, round: u32) {
        let wizards: Vec<PlayerId> = world
            .player_ids()
            .into_iter()
            .filter(|&p| world.wizard(p).is_some())
            .collect();
        for (i, &a) in wizards.iter().enumerate() {
            for &b in &wizards[i + 1..] {
                let key = pair(a, b);
                if world.treaty(a, b) != f4x_core::diplomacy::Treaty::War {
                    self.war_since.remove(&key);
                    continue;
                }
                let since = *self.war_since.entry(key).or_insert(round);
                let last = self.last_fight.get(&key).copied().unwrap_or(0).max(since);
                let stalled = round.saturating_sub(last) >= STALLED_WAR_TURNS;
                for p in [a, b] {
                    self.bump(p, |m| {
                        m.war_turns += 1;
                        m.stalled_war_turns += u32::from(stalled);
                    });
                }
            }
        }
    }

    /// The wizards defeated, in order: the turn, the wizard and who took its last city.
    pub fn defeats(&self) -> &[(u32, u8, Option<u8>)] {
        &self.defeats
    }

    /// The places where each wizard lost the most attacks on what is no wizard (monsters, neutral
    /// cities): wizard, place, count; at most `n` for each wizard, the worst first.
    pub fn worst_places(&self, n: usize) -> Vec<(u8, String, u32)> {
        let mut by: BTreeMap<u8, Vec<(String, u32)>> = BTreeMap::new();
        for ((p, place), c) in &self.lost_on_others {
            by.entry(*p).or_default().push((place.clone(), *c));
        }
        let mut out = Vec::new();
        for (p, mut v) in by {
            v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            out.extend(v.into_iter().take(n).map(|(place, c)| (p, place, c)));
        }
        out
    }

    /// The wars declared, in order: the turn, the wizard who declared and the one it declared on.
    pub fn wars(&self) -> &[(u32, u8, u8)] {
        &self.wars
    }

    /// The cities taken on the other plane than the taker started on, in order: the turn, the
    /// taker, the former owner and the plane (index).
    pub fn far_captures(&self) -> &[(u32, u8, u8, usize)] {
        &self.far_captures
    }

    /// The cities found undefended, in order: the turn, the owner and the city's square.
    pub fn undefended(&self) -> &[(u32, u8, f4x_core::map::Position)] {
        &self.undefended
    }

    /// The mana each wizard spent over the game and holds now, by player number.
    pub fn mana(&self) -> (&[i64], &[i64]) {
        (&self.mana_spent, &self.mana_left)
    }

    /// How often each spell was cast, by the spell's id; a spell never cast is not in it.
    pub fn casts(&self) -> &BTreeMap<String, u32> {
        &self.casts
    }

    /// The reserve mana paid for each spell cast in battle by a wizard, by the spell's id.
    pub fn battle_mana(&self) -> &BTreeMap<String, i64> {
        &self.battle_mana
    }

    /// How many wizards know each spell, by the spell's id (see [`Tally::note_known`]).
    pub fn known(&self) -> &BTreeMap<String, u32> {
        &self.known
    }

    /// Count the wizards of the game that know each spell: call it once, at the end.
    pub fn note_known(&mut self, world: &World) {
        for p in world.player_ids() {
            let Some(w) = world.wizard(p) else { continue };
            let known = w.known();
            let starting = w.starting_spells();
            let open = w.researchable();
            for &spell in known.iter().chain(open.iter()) {
                let id = world.content().rules.spells.get(spell).id.clone();
                let knew = known.contains(&spell);
                if knew {
                    *self.known.entry(id.clone()).or_default() += 1;
                }
                let r = self.spell_rows.entry(id.clone()).or_default();
                if knew {
                    r.knew += 1;
                }
                if !starting.contains(&spell) {
                    r.could += 1;
                    if knew {
                        r.learned += 1;
                    }
                }
            }
            for ((who, id), n) in &self.cast_by {
                if *who == p.0 {
                    let r = self.spell_rows.entry(id.clone()).or_default();
                    r.casters += 1;
                    r.casts += n;
                }
            }
        }
    }

    /// What the wizards did with each spell, by the spell's id (filled by [`Tally::note_known`]).
    pub fn spell_rows(&self) -> &BTreeMap<String, SpellRow> {
        &self.spell_rows
    }

    /// Add the count of a wizard whose AI saw situations for a spell it did not know, by the
    /// kind of spell and the turns.
    pub fn note_unmet(&mut self, wants: &[(&'static str, u32)]) {
        for &(kind, turns) in wants.iter().filter(|(_, n)| *n > 0) {
            let t = self.unmet_wants.entry(kind.to_string()).or_default();
            t.0 += turns;
            t.1 += 1;
        }
    }

    /// The situations that called for a kind of spell the wizard did not know.
    pub fn unmet_wants(&self) -> &UnmetWants {
        &self.unmet_wants
    }

    /// The tries to cast a spell that came to nothing, by the spell and the reason.
    pub fn cast_failures(&self) -> &CastFailures {
        &self.cast_failures
    }

    fn fail(&mut self, world: &World, spell: f4x_def::rules::SpellId, context: &str, why: String) {
        let id = world.content().rules.spells.get(spell).id.clone();
        *self
            .cast_failures
            .entry(id)
            .or_default()
            .entry(format!("{context}: {why}"))
            .or_default() += 1;
    }

    /// Count a command the AI sent and the game refused, when it was a try to cast a spell: the
    /// refused-order log's `command` and `reason` texts (the command's debug text names the spell
    /// by its number, `spell: Id(93)`; a `ResolveCast` names none).
    pub fn note_refused_cast(&mut self, world: &World, command: &str, reason: &str) {
        let context = if command.starts_with("CastSpell") || command.starts_with("ResolveCast") {
            "map"
        } else if command.starts_with("Battle(Cast") || command.starts_with("Battle(UnitCast") {
            "battle"
        } else {
            return;
        };
        let name = command
            .split("spell: ")
            .nth(1)
            .and_then(|rest| rest.split([',', ' ']).next())
            .and_then(|key| {
                world
                    .content()
                    .rules
                    .spells
                    .iter()
                    .find(|(id, _)| format!("{id:?}") == key)
                    .map(|(_, d)| d.id.clone())
            })
            .unwrap_or_else(|| "(resolving a cast)".into());
        *self
            .cast_failures
            .entry(name)
            .or_default()
            .entry(format!("{context}: refused, {reason}"))
            .or_default() += 1;
    }

    /// How often each spell was cast in a battle, by the spell's id; a spell never cast is not in it.
    pub fn battle_casts(&self) -> &BTreeMap<String, u32> {
        &self.battle_casts
    }

    pub fn of(&self, player: PlayerId) -> Metrics {
        self.players
            .get(usize::from(player.0))
            .copied()
            .unwrap_or_default()
    }

    /// The counts of all players together.
    pub fn total(&self) -> Metrics {
        let mut sum = Metrics::default();
        for m in &self.players {
            sum += *m;
        }
        sum
    }

    /// The battle just ended, by who attacked and what the other side was.
    fn count_by_role(&mut self, winner: PlayerId, loser: PlayerId) {
        let Some((attacker, _, place)) = self.open_battle.take() else {
            return;
        };
        if attacker == loser && kind_of(winner) != Kind::Wizard {
            *self.lost_on_others.entry((attacker.0, place)).or_default() += 1;
        }
        for (me, other, won) in [(winner, loser, true), (loser, winner, false)] {
            let by_me = me == attacker;
            let Some(m) = self.at(me) else { continue };
            let counter = match (by_me, won, kind_of(other)) {
                (true, true, Kind::Wizard) => &mut m.attack_won_wizard,
                (true, false, Kind::Wizard) => &mut m.attack_lost_wizard,
                (false, true, Kind::Wizard) => &mut m.defense_won_wizard,
                (false, false, Kind::Wizard) => &mut m.defense_lost_wizard,
                (true, true, Kind::Neutral) => &mut m.attack_won_neutral,
                (true, false, Kind::Neutral) => &mut m.attack_lost_neutral,
                (false, true, Kind::Neutral) => &mut m.defense_won_neutral,
                (false, false, Kind::Neutral) => &mut m.defense_lost_neutral,
                (true, true, Kind::Lair) => &mut m.attack_won_lair,
                (true, false, Kind::Lair) => &mut m.attack_lost_lair,
                (false, true, Kind::Lair) => &mut m.defense_won_lair,
                (false, false, Kind::Lair) => &mut m.defense_lost_lair,
            };
            *counter += 1;
        }
    }

    fn at(&mut self, player: PlayerId) -> Option<&mut Metrics> {
        let last = self.players.len() - 1;
        self.players.get_mut(usize::from(player.0).min(last))
    }

    /// Count `events`. `source` is the wizard whose orders caused them (a city founded or a
    /// building finished has no player of its own: its owner counts), none for the turn change.
    pub fn record(
        &mut self,
        world: &World,
        round: u32,
        source: Option<PlayerId>,
        events: &[Event],
    ) {
        let owner = |city| world.economy().city_owner(city).or(source);
        for event in events {
            match event {
                Event::CityFounded { city, pos } => {
                    if let Some(p) = owner(*city) {
                        self.founded_now.push((p, *pos));
                    }
                    if let Some(m) = owner(*city).and_then(|p| self.at(p)) {
                        m.cities_founded += 1;
                    }
                }
                Event::CityCaptured { city, from, to } => {
                    self.last_fight.insert(pair(*from, *to), round);
                    self.last_taker.insert(from.0, to.0);
                    let at = world
                        .economy()
                        .cities()
                        .iter()
                        .find(|(e, ..)| e == city)
                        .map(|(.., pos)| *pos);
                    let by_sea = at.is_some_and(|pos| {
                        self.landed_on.get(usize::from(to.0)).is_some_and(|l| {
                            l.contains(&(pos.plane.index(), world.landmass_of(pos)))
                        })
                    });
                    let far = at.is_some_and(|pos| {
                        self.home_plane
                            .get(usize::from(to.0))
                            .copied()
                            .flatten()
                            .is_some_and(|home| home != pos.plane.index())
                    });
                    if far {
                        if let Some(pos) = at {
                            self.far_captures
                                .push((round, to.0, from.0, pos.plane.index()));
                        }
                    }
                    if let Some(m) = self.at(*to) {
                        m.cities_captured += 1;
                        m.cities_by_sea += u32::from(by_sea);
                        m.cities_captured_far_plane += u32::from(far);
                    }
                    if let Some(m) = self.at(*from) {
                        m.cities_lost += 1;
                    }
                }
                // The taker razes the city it has just won: it is gone from the taker's count.
                Event::CityRazed { player, .. } => {
                    if let Some(m) = self.at(*player) {
                        m.cities_lost += 1;
                    }
                }
                Event::BattleStarted {
                    attacker,
                    defender,
                    site,
                } => {
                    let kind = if world.economy().city_at(*site).is_some() {
                        "city"
                    } else {
                        "site"
                    };
                    self.open_battle = Some((
                        *attacker,
                        *defender,
                        format!("{kind} {site:?} held by p{}", defender.0),
                    ));
                }
                Event::BattleCasualties { player, units }
                | Event::UnitsLostFleeing { player, units } => {
                    self.bump(*player, |m| m.units_lost_in_battle += units)
                }
                Event::BattleEnded { winner, loser } => {
                    self.last_fight.insert(pair(*winner, *loser), round);
                    self.count_by_role(*winner, *loser);
                    if let Some(m) = self.at(*winner) {
                        m.battles_won += 1;
                    }
                    if let Some(m) = self.at(*loser) {
                        m.battles_lost += 1;
                    }
                }
                Event::ProductionCompleted { city, item } => {
                    let Some(m) = owner(*city).and_then(|p| self.at(p)) else {
                        continue;
                    };
                    match item {
                        ProductionItem::Building(_) => m.buildings_built += 1,
                        ProductionItem::Unit(unit) => {
                            m.units_built += 1;
                            m.ships_built +=
                                u32::from(is_ship(world.content().rules.units.get(*unit)));
                        }
                        ProductionItem::TradeGoods | ProductionItem::Housing => {}
                    }
                }
                Event::MarchLaunched { player, units, .. } => self.bump(*player, |m| {
                    m.marches_launched += 1;
                    m.march_units += units;
                }),
                Event::WarPlanChanged { player, goal } => self.bump(*player, |m| {
                    if *goal {
                        m.plan_goal_changes += 1;
                    } else {
                        m.plan_next_changes += 1;
                    }
                }),
                Event::MarchCalledOff {
                    player,
                    target_changed,
                    units,
                    turns,
                    ..
                } => self.bump(*player, |m| {
                    if *target_changed {
                        m.marches_dropped += 1;
                    } else {
                        m.marches_called_off += 1;
                        m.march_called_off_turns += turns;
                        m.march_called_off_units += units;
                    }
                }),
                Event::RolesAudited {
                    player,
                    adrift,
                    unassigned,
                    unregistered,
                    reset,
                    waiting,
                    bridges,
                    waiting_bridge,
                } => self.bump(*player, |m| {
                    m.units_adrift += adrift;
                    m.units_unassigned += unassigned;
                    m.units_unregistered += unregistered;
                    m.missions_reset += reset;
                    m.waiting_for_ship += waiting;
                    m.bridge_ships += bridges;
                    m.waiting_for_bridge += waiting_bridge;
                }),
                Event::PlaneShifted { player, units, .. } => self.bump(*player, |m| {
                    m.plane_shifts += 1;
                    m.plane_crossings += units.len() as u32;
                }),
                Event::SiteCleared { player, site, .. } => {
                    let tower = world
                        .content()
                        .features
                        .get(*site)
                        .is_some_and(|f| f.kind == f4x_def::feature::FeatureKind::Tower);
                    self.bump(*player, |m| {
                        m.sites_cleared += 1;
                        m.towers_cleared += u32::from(tower);
                    })
                }
                Event::UnitsMoved { units, from, to } if from.plane != to.plane => {
                    let own = units.first().and_then(|&u| world.units().unit_owner(u));
                    if let Some(p) = own {
                        self.bump(p, |m| m.plane_crossings += units.len() as u32);
                    }
                }
                Event::UnitsMoved { units, from, to } => {
                    let sea =
                        |at| world.economy().terrain_class_at(at) == Some(TerrainClass::Ocean);
                    let Some(player) = units.first().and_then(|&u| world.units().unit_owner(u))
                    else {
                        continue;
                    };
                    let landing = sea(*from) && !sea(*to);
                    let troops = units
                        .iter()
                        .filter(|&&u| world.units().unit_def(u).is_some_and(|d| !is_ship(d)))
                        .count() as u32;
                    if !landing || troops == 0 {
                        continue;
                    }
                    let mass = (to.plane.index(), world.landmass_of(*to));
                    let Some(slot) = self.landed_on.get_mut(usize::from(player.0)) else {
                        continue;
                    };
                    slot.insert(mass);
                    let first = round;
                    if let Some(m) = self.at(player) {
                        m.troops_landed += troops;
                        if m.wizards_landed == 0 {
                            m.wizards_landed = 1;
                            m.first_landing_turns = first;
                        }
                    }
                }
                Event::UnitLost {
                    player,
                    unit,
                    cause,
                } => {
                    self.bump(*player, |m| {
                        m.units_lost += 1;
                        if *cause == f4x_core::world::event::LossCause::Food {
                            m.units_starved += 1;
                        }
                    });
                    if world.content().rules.units.get(*unit).is_settler() {
                        self.settlers_let_go.push((*player, *cause));
                    }
                }
                Event::SpellCast { player, spell } => {
                    let id = world.content().rules.spells.get(*spell).id.clone();
                    *self.cast_by.entry((player.0, id.clone())).or_default() += 1;
                    *self.casts.entry(id).or_default() += 1;
                    self.bump(*player, |m| m.spells_cast += 1)
                }
                Event::Battle(BattleEvent::SpellCast {
                    side, spell, mana, ..
                }) => {
                    let id = world.content().rules.spells.get(*spell).id.clone();
                    if let Some((attacker, defender, _)) = &self.open_battle {
                        let caster = match side {
                            f4x_battle::side::Side::Attacker => attacker,
                            f4x_battle::side::Side::Defender => defender,
                        };
                        *self.cast_by.entry((caster.0, id.clone())).or_default() += 1;
                    }
                    *self.battle_mana.entry(id.clone()).or_default() += mana.floor() as i64;
                    *self.battle_casts.entry(id).or_default() += 1;
                }
                Event::Battle(BattleEvent::UnitSpellCast { spell, .. }) => {
                    *self
                        .battle_casts
                        .entry(world.content().rules.spells.get(*spell).id.clone())
                        .or_default() += 1;
                }
                Event::SpellFailed { spell, reason, .. } => self.fail(
                    world,
                    *spell,
                    "map",
                    format!("failed at its target, {reason:?}"),
                ),
                Event::SpellFizzled { spell, .. } => self.fail(
                    world,
                    *spell,
                    "map",
                    "fizzled, the city's nightshade".into(),
                ),
                Event::NoTargetForCast { spell, .. } => {
                    self.fail(world, *spell, "map", "nothing to aim at".into())
                }
                Event::SpellSuppressed { spell, counter, .. } => {
                    let by = world.content().rules.spells.get(*counter).id.clone();
                    self.fail(world, *spell, "map", format!("suppressed by {by}"))
                }
                Event::Battle(BattleEvent::SpellCountered { spell, .. }) => {
                    self.fail(world, *spell, "battle", "countered by the enemy".into())
                }
                Event::SpellLearned { player, .. } => self.bump(*player, |m| m.spells_learned += 1),
                Event::HeroJoined { player, .. } => self.bump(*player, |m| m.heroes_joined += 1),
                Event::MercenariesHired { player, .. } => {
                    self.bump(*player, |m| m.mercenaries_hired += 1)
                }
                Event::ItemMade { player, .. } => self.bump(*player, |m| m.items_made += 1),
                Event::ItemSold { player, .. } => self.bump(*player, |m| m.items_sold += 1),
                Event::ItemScrapped { player, .. } => self.bump(*player, |m| m.items_scrapped += 1),
                Event::Diplomacy(DiplomacyEvent::WarDeclared { attacker, defender }) => {
                    self.wars.push((round, attacker.0, defender.0));
                    self.bump(*attacker, |m| {
                        m.wars_declared += 1;
                        m.diplomacy_started += 1;
                    })
                }
                Event::Diplomacy(DiplomacyEvent::ContactMade { a, b }) => {
                    self.bump(*a, |m| m.contacts_made += 1);
                    self.bump(*b, |m| m.contacts_made += 1);
                }
                Event::Diplomacy(DiplomacyEvent::ProposalMade(Proposal { from, kind, .. })) => self
                    .bump(*from, |m| {
                        m.diplomacy_started += 1;
                        match kind {
                            ProposalKind::Pact => m.pact_offers += 1,
                            ProposalKind::Alliance => m.alliance_offers += 1,
                            ProposalKind::Peace => m.peace_offers += 1,
                            ProposalKind::WarOn(_) => m.war_requests += 1,
                            ProposalKind::BreakAllianceWith(_) => m.alliance_break_requests += 1,
                            ProposalKind::Trade { .. } => m.spell_exchanges += 1,
                            ProposalKind::Threat => m.threats += 1,
                            ProposalKind::Demand { .. } => m.demands += 1,
                        }
                    }),
                Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
                    proposal, accepted, ..
                }) => self.bump(proposal.from, |m| {
                    if *accepted {
                        m.proposals_accepted += 1;
                        if let ProposalKind::Trade { give, get } = proposal.kind {
                            m.trades_done += 1;
                            m.trades_gold += u32::from(give.gold > 0 || get.gold > 0);
                            m.trades_goods += u32::from(
                                give.item.is_some()
                                    || give.unit.is_some()
                                    || get.item.is_some()
                                    || get.unit.is_some()
                                    || give.hero.is_some()
                                    || get.hero.is_some(),
                            );
                        }
                    } else {
                        m.proposals_refused += 1;
                    }
                }),
                Event::Diplomacy(DiplomacyEvent::ProposalCountered { new, .. }) => {
                    self.bump(new.from, |m| m.trade_counters += 1)
                }
                Event::Diplomacy(DiplomacyEvent::TreatyChanged { a, b, treaty }) => {
                    if matches!(treaty, Treaty::Pact | Treaty::Alliance) {
                        self.bump(*a, |m| m.treaties_joined += 1);
                        self.bump(*b, |m| m.treaties_joined += 1);
                    }
                }
                Event::Diplomacy(DiplomacyEvent::Remarked { from, why, .. }) => {
                    self.bump(*from, |m| match why {
                        f4x_core::diplomacy::Remark::DominantWar => m.dominant_wars += 1,
                        f4x_core::diplomacy::Remark::Appeasing => m.appeasements += 1,
                        f4x_core::diplomacy::Remark::Coalition => m.coalition_asks += 1,
                        f4x_core::diplomacy::Remark::WarWeary => m.weary_peace_offers += 1,
                    })
                }
                Event::Diplomacy(DiplomacyEvent::Warned { from, .. }) => self.bump(*from, |m| {
                    m.diplomacy_started += 1;
                    m.warnings += 1;
                }),
                Event::Diplomacy(DiplomacyEvent::GoldGiven { from, .. }) => self.bump(*from, |m| {
                    m.diplomacy_started += 1;
                    m.gold_gifts += 1;
                }),
                Event::Diplomacy(DiplomacyEvent::SpellGiven { from, .. }) => {
                    self.bump(*from, |m| {
                        m.diplomacy_started += 1;
                        m.spell_gifts += 1;
                    })
                }
                Event::Diplomacy(DiplomacyEvent::TreatyBroken { breaker, .. }) => {
                    self.bump(*breaker, |m| {
                        m.diplomacy_started += 1;
                        m.treaties_broken += 1;
                    })
                }
                Event::ProductionBought { player, cost, .. } => self.bump(*player, |m| {
                    m.production_rushed += 1;
                    m.gold_rushed += *cost;
                }),
                // A standing order that stopped on a refused step counts as a refused order.
                Event::WizardDefeated { player } => {
                    let killer = self.last_taker.get(&player.0).copied();
                    self.defeats.push((round, player.0, killer));
                    if let Some(k) = killer.map(PlayerId).filter(|&k| world.is_wizard(k)) {
                        self.bump(k, |m| m.wizards_eliminated += 1);
                    }
                }
                Event::OrderFailed { player, why, .. } => {
                    self.bump(*player, |m| m.refused_orders += 1);
                    if matches!(why, f4x_core::unit::movement::MoveError::Treaty) {
                        self.bump(*player, |m| m.treaty_refusals += 1);
                    }
                }
                Event::Rejected(why) => {
                    if let Some(p) = source {
                        self.bump(p, |m| m.refused_orders += 1);
                        if matches!(
                            why,
                            f4x_core::world::Rejection::Move(
                                f4x_core::unit::movement::MoveError::Treaty
                            ) | f4x_core::world::Rejection::BattleStart(
                                f4x_core::battle::world::BattleStartError::Treaty
                            )
                        ) {
                            self.bump(p, |m| m.treaty_refusals += 1);
                        }
                    }
                }
                _ => {}
            }
        }
        // A ship leaves no event of its own when a battle or the sea takes it, so count what is
        // missing from the fleet: the ones it started with and built, less those still afloat.
        let now = self.ships_now(world);
        for (slot, m) in self.players.iter_mut().enumerate() {
            m.ships_lost = (self.ships_at_start[slot] + m.ships_built).saturating_sub(now[slot]);
        }
    }

    fn bump(&mut self, player: PlayerId, change: impl FnOnce(&mut Metrics)) {
        if let Some(m) = self.at(player) {
            change(m);
        }
    }
}

/// How a wizard stands at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standing {
    pub cities: usize,
    pub buildings: usize,
    pub units: usize,
    pub gold: i32,
    pub mana: i32,
    pub spells_known: usize,
    pub explored: usize,
    pub fame: i32,
    pub score: i32,
}

impl Standing {
    pub fn of(world: &World, player: PlayerId, round: u32) -> Self {
        let cities: Vec<_> = world
            .economy()
            .cities()
            .iter()
            .filter(|(_, c, _)| c.owner == player)
            .map(|(e, ..)| *e)
            .collect();
        let treasury = world.player(player).map(|p| p.treasury());
        Self {
            cities: cities.len(),
            buildings: cities.iter().map(|&c| world.city_buildings(c).len()).sum(),
            units: world.units().player_units(player).len(),
            gold: treasury.map_or(0, |t| t.gold.floor()),
            mana: treasury.map_or(0, |t| t.mana.floor()),
            spells_known: world.wizard(player).map_or(0, |w| w.known().len()),
            explored: world.explored_squares(player),
            fame: world.wizard(player).map_or(0, |w| w.fame()),
            score: world.score(player, round).total(),
        }
    }

    /// Each value with its name, in the order a report shows them.
    pub fn rows(&self) -> [(&'static str, i64); 9] {
        [
            ("cities", self.cities as i64),
            ("buildings", self.buildings as i64),
            ("units", self.units as i64),
            ("gold", i64::from(self.gold)),
            ("mana", i64::from(self.mana)),
            ("spells known", self.spells_known as i64),
            ("explored squares", self.explored as i64),
            ("fame", i64::from(self.fame)),
            ("score", i64::from(self.score)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contestant::{self, Version};
    use crate::testutil::shipped;
    use f4x_def::testdir::TestResult;
    use f4x_newgame::setup::{new_game, NewGame};

    /// A real two-wizard game: every city a wizard has at the end was there at the start, was
    /// founded or was taken, and none that was lost is still there.
    #[test]
    fn founded_cities_account_for_the_cities_a_wizard_ends_with() -> TestResult {
        let mut world = f4x_master::Game::from(new_game(
            shipped(),
            &NewGame {
                seed: 4,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?);
        let players = [PlayerId(0), PlayerId(1)];
        let start: Vec<usize> = players
            .iter()
            .map(|&p| Standing::of(&world, p, world.round()).cities)
            .collect();
        let mut ais: Vec<_> = players
            .iter()
            .map(|&p| contestant::make(Version::New, p))
            .collect();
        let mut neutrals = contestant::Neutrals::new(Version::New);
        let mut tally = Tally::new(&world);
        for _ in 0..40 {
            let (turns, rest, _) =
                contestant::play_round_timed(&mut world, &mut ais, &mut neutrals);
            for (player, events) in &turns {
                tally.record(&world, world.round(), Some(*player), events);
            }
            tally.record(&world, world.round(), None, &rest);
        }
        assert!(tally.total().cities_founded > 0, "nobody founded a city");
        for (k, &p) in players.iter().enumerate() {
            let m = tally.of(p);
            let ends = Standing::of(&world, p, world.round()).cities;
            assert_eq!(
                start[k] + m.cities_founded as usize + m.cities_captured as usize
                    - m.cities_lost as usize,
                ends,
                "p{}",
                p.0
            );
        }
        Ok(())
    }

    /// A settler that leaves the map is counted by what took it: killed when the game says
    /// nothing, let go for lack of food or gold when the game says so; one that stays or founds
    /// a city is no failure.
    #[test]
    fn a_settler_that_leaves_the_map_is_counted_by_what_took_it() -> TestResult {
        use f4x_core::world::event::LossCause;
        let content = shipped();
        let mut world = f4x_master::Game::from(new_game(
            content,
            &NewGame {
                seed: 4,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?);
        let kind = content
            .rules
            .units
            .defs()
            .iter()
            .find(|d| d.is_settler())
            .and_then(|d| content.rules.units.id_numbered(d.num))
            .ok_or("no settler in the content")?;
        let home = world
            .wizard(PlayerId(0))
            .and_then(|w| w.fortress())
            .ok_or("no fortress")?;
        let mut made = Vec::new();
        for _ in 0..3 {
            world.command(f4x_core::world::Command::CreateUnit {
                owner: PlayerId(0),
                pos: home,
                unit: kind,
            });
        }
        for u in world.units().units_at(home) {
            if world.units().unit_is_settler(u) {
                made.push(u);
            }
        }
        assert_eq!(made.len(), 3);
        let mut tally = Tally::new(&world);
        tally.sample_settlers(&world);
        // One vanishes with no word, one is let go for lack of food, one stays.
        world.scenario_remove_unit(made[0]);
        world.scenario_remove_unit(made[1]);
        tally.record(
            &world,
            world.round(),
            None,
            &[Event::UnitLost {
                player: PlayerId(0),
                unit: kind,
                cause: LossCause::Food,
            }],
        );
        tally.sample_settlers(&world);
        let m = tally.of(PlayerId(0));
        assert_eq!(
            (m.settlers_killed, m.settlers_starved, m.settlers_unpaid),
            (1, 1, 0)
        );
        // Nothing more is counted while the last one stays.
        tally.sample_settlers(&world);
        assert_eq!(tally.of(PlayerId(0)).settlers_killed, 1);
        // The last one founds a city where it stands: that is no failure.
        let city = world.economy().city_at(home).ok_or("no capital")?;
        world.scenario_remove_unit(made[2]);
        tally.record(
            &world,
            world.round(),
            None,
            &[Event::CityFounded { city, pos: home }],
        );
        tally.sample_settlers(&world);
        assert_eq!(tally.of(PlayerId(0)).settlers_killed, 1);
        Ok(())
    }

    /// A city that holds only settlers, engineers and mana spirits is undefended: it is counted
    /// each time the tally samples; a unit that fights in it ends the count.
    #[test]
    fn a_city_with_no_unit_but_workers_is_counted_as_undefended() -> TestResult {
        let content = shipped();
        let mut world = f4x_master::Game::from(new_game(
            content,
            &NewGame {
                seed: 4,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?);
        let home = world
            .wizard(PlayerId(0))
            .and_then(|w| w.fortress())
            .ok_or("no fortress")?;
        let kind_of = |keep: &dyn Fn(&f4x_def::rules::UnitDef) -> bool| {
            content
                .rules
                .units
                .defs()
                .iter()
                .find(|d| keep(d))
                .and_then(|d| content.rules.units.id_numbered(d.num))
                .ok_or("no such unit in the content")
        };
        // Empty the city of everything it holds.
        for u in world.units().units_at(home) {
            world.scenario_remove_unit(u);
        }
        let mut tally = Tally::new(&world);
        tally.sample_cities(&world, world.round());
        assert_eq!(tally.of(PlayerId(0)).undefended_cities, 1, "an empty city");
        let ours: Vec<_> = tally.undefended().iter().filter(|u| u.1 == 0).collect();
        assert_eq!(ours, [&(world.round(), 0, home)], "turn and square");
        assert!(
            tally.undefended().iter().any(|u| u.1 >= 100),
            "a neutral city with no garrison counts too: {:?}",
            tally.undefended()
        );
        // A settler is no defender.
        let settler = kind_of(&|d| d.is_settler())?;
        world.command(f4x_core::world::Command::CreateUnit {
            owner: PlayerId(0),
            pos: home,
            unit: settler,
        });
        tally.sample_cities(&world, world.round());
        assert_eq!(tally.of(PlayerId(0)).undefended_cities, 2, "a settler only");
        // Nor is a ship.
        let ship = kind_of(&|d| is_ship(d))?;
        world.command(f4x_core::world::Command::CreateUnit {
            owner: PlayerId(0),
            pos: home,
            unit: ship,
        });
        tally.sample_cities(&world, world.round());
        assert_eq!(tally.of(PlayerId(0)).undefended_cities, 3, "a ship too");
        // A soldier is.
        let soldier = kind_of(&|d| d.is_fighter())?;
        world.command(f4x_core::world::Command::CreateUnit {
            owner: PlayerId(0),
            pos: home,
            unit: soldier,
        });
        tally.sample_cities(&world, world.round());
        assert_eq!(tally.of(PlayerId(0)).undefended_cities, 3, "defended now");
        Ok(())
    }

    /// A neutral player is shown all but the magic: no spells or research, but the heroes, the
    /// diplomacy, the battles, the cities and the city-turns without a defender. Every name in the wizard-only list is a real
    /// row.
    #[test]
    fn a_neutral_player_is_shown_the_rows_that_apply_to_it() {
        let m = Metrics::default();
        let all: Vec<&str> = m.rows().iter().map(|r| r.0).collect();
        for label in WIZARD_ONLY {
            assert!(all.contains(label), "no such row: {label}");
        }
        let neutral: Vec<&str> = m.neutral_rows().iter().map(|r| r.0).collect();
        assert!(!neutral.contains(&"spells cast"));
        for kept in [
            "units built",
            "battles won",
            "heroes joined",
            "trades done",
            "wars declared",
            "city-turns undefended",
        ] {
            assert!(neutral.contains(&kept), "{kept}");
        }
    }

    /// A city taken on the plane the taker did not start on is a far capture, with its turn,
    /// taker, former owner and plane; one taken on the home plane is not.
    #[test]
    fn a_city_taken_on_the_other_plane_is_a_far_capture() -> TestResult {
        let world = f4x_master::Game::from(new_game(
            shipped(),
            &NewGame {
                seed: 4,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?);
        let (city, plane) = world
            .economy()
            .cities()
            .iter()
            .find(|(e, ..)| world.economy().city_owner(*e) == Some(PlayerId(1)))
            .map(|(e, .., pos)| (*e, pos.plane.index()))
            .ok_or("player 1 has no city")?;
        let taken = |home: usize| -> Result<Tally, Box<dyn std::error::Error>> {
            let mut tally = Tally::new(&world);
            tally.home_plane[0] = Some(home);
            let event = Event::CityCaptured {
                city,
                from: PlayerId(1),
                to: PlayerId(0),
            };
            tally.record(&world, world.round(), None, &[event]);
            Ok(tally)
        };
        let far = taken(1 - plane)?;
        assert_eq!(far.far_captures(), &[(world.round(), 0, 1, plane)]);
        assert_eq!(far.of(PlayerId(0)).cities_captured_far_plane, 1);
        let home = taken(plane)?;
        assert!(home.far_captures().is_empty());
        assert_eq!(home.of(PlayerId(0)).cities_captured_far_plane, 0);
        assert_eq!(home.of(PlayerId(0)).cities_captured, 1);
        Ok(())
    }

    /// The AI's role audit is counted per wizard, summed over turns.
    #[test]
    fn the_role_audit_is_counted() -> TestResult {
        let world = f4x_master::Game::from(new_game(
            shipped(),
            &NewGame {
                seed: 4,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?);
        let player = PlayerId(0);
        let mut tally = Tally::new(&world);
        let audit = |adrift, unassigned, waiting| Event::RolesAudited {
            player,
            adrift,
            unassigned,
            unregistered: 0,
            reset: 0,
            waiting,
            bridges: 1,
            waiting_bridge: waiting + 1,
        };
        tally.record(
            &world,
            world.round(),
            None,
            &[audit(2, 4, 3), audit(1, 5, 2)],
        );
        let m = tally.of(player);
        assert_eq!((m.units_adrift, m.units_unassigned), (3, 9));
        assert_eq!(m.waiting_for_ship, 5);
        assert_eq!((m.bridge_ships, m.waiting_for_bridge), (2, 7));
        Ok(())
    }

    /// The AI's march events are counted per wizard: launches with their soldiers, marches called
    /// off for time, marches dropped for another target.
    #[test]
    fn marches_launched_and_called_off_are_counted() -> TestResult {
        let world = f4x_master::Game::from(new_game(
            shipped(),
            &NewGame {
                seed: 4,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?);
        let at = f4x_core::map::Position {
            plane: f4x_core::map::Plane::Arcanus,
            x: 3,
            y: 3,
        };
        let player = PlayerId(0);
        let mut tally = Tally::new(&world);
        let events = [
            Event::MarchLaunched {
                player,
                target: at,
                units: 9,
            },
            Event::MarchLaunched {
                player,
                target: at,
                units: 4,
            },
            Event::MarchCalledOff {
                player,
                target: at,
                units: 9,
                turns: 30,
                target_changed: false,
            },
            Event::MarchCalledOff {
                player,
                target: at,
                units: 4,
                turns: 2,
                target_changed: true,
            },
        ];
        tally.record(&world, world.round(), None, &events);
        let m = tally.of(player);
        assert_eq!(
            (
                m.marches_launched,
                m.march_units,
                m.marches_called_off,
                m.marches_dropped
            ),
            (2, 13, 1, 1)
        );
        assert_eq!(tally.of(PlayerId(1)).marches_launched, 0);
        Ok(())
    }

    #[test]
    fn contacts_count_for_both_wizards_and_only_the_one_who_acts_starts_diplomacy() -> TestResult {
        let world = f4x_master::Game::from(new_game(
            shipped(),
            &NewGame {
                seed: 1,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?);
        let (a, b) = (PlayerId(0), PlayerId(1));
        let mut tally = Tally::new(&world);
        tally.record(
            &world,
            world.round(),
            None,
            &[
                Event::Diplomacy(DiplomacyEvent::ContactMade { a, b }),
                Event::Diplomacy(DiplomacyEvent::Warned { from: a, to: b }),
                Event::Diplomacy(DiplomacyEvent::WarDeclared {
                    attacker: a,
                    defender: b,
                }),
            ],
        );
        assert_eq!(
            (tally.of(a).contacts_made, tally.of(b).contacts_made),
            (1, 1)
        );
        assert_eq!(tally.of(a).diplomacy_started, 2);
        assert_eq!(tally.of(b).diplomacy_started, 0);
        assert_eq!(tally.of(a).warnings, 1);
        Ok(())
    }

    #[test]
    fn each_kind_of_diplomatic_action_has_its_own_count_and_all_add_up_to_the_total() -> TestResult
    {
        let world = f4x_master::Game::from(new_game(
            shipped(),
            &NewGame {
                seed: 1,
                all_ai: true,
                ..NewGame::with_wizards(3)
            },
        )?);
        let (a, b, c) = (PlayerId(0), PlayerId(1), PlayerId(2));
        let offer = |kind| {
            Event::Diplomacy(DiplomacyEvent::ProposalMade(Proposal {
                from: a,
                to: b,
                kind,
                round: 0,
            }))
        };
        let spell = world
            .content()
            .rules
            .spells
            .iter()
            .map(|(id, _)| id)
            .next()
            .ok_or("no spells")?;
        let mut tally = Tally::new(&world);
        tally.record(
            &world,
            world.round(),
            None,
            &[
                offer(ProposalKind::Peace),
                offer(ProposalKind::Pact),
                offer(ProposalKind::Alliance),
                offer(ProposalKind::WarOn(c)),
                offer(ProposalKind::BreakAllianceWith(c)),
                offer(ProposalKind::Trade {
                    give: f4x_core::diplomacy::Goods::spell(spell),
                    get: f4x_core::diplomacy::Goods::spell(spell),
                }),
                offer(ProposalKind::Threat),
                offer(ProposalKind::Demand { gold: 5 }),
                Event::Diplomacy(DiplomacyEvent::GoldGiven {
                    from: a,
                    to: b,
                    gold: 5,
                }),
                Event::Diplomacy(DiplomacyEvent::SpellGiven {
                    from: a,
                    to: b,
                    spell,
                }),
                Event::Diplomacy(DiplomacyEvent::TreatyBroken {
                    breaker: a,
                    victim: b,
                    treaty: Treaty::Pact,
                }),
                Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
                    proposal: Proposal {
                        from: a,
                        to: b,
                        kind: ProposalKind::Peace,
                        round: 0,
                    },
                    accepted: true,
                    lapsed: false,
                }),
                Event::Diplomacy(DiplomacyEvent::TreatyChanged {
                    a,
                    b,
                    treaty: Treaty::Pact,
                }),
            ],
        );
        let m = tally.of(a);
        let kinds = [
            m.peace_offers,
            m.pact_offers,
            m.alliance_offers,
            m.war_requests,
            m.alliance_break_requests,
            m.spell_exchanges,
            m.threats,
            m.demands,
            m.gold_gifts,
            m.spell_gifts,
            m.treaties_broken,
        ];
        assert_eq!(kinds, [1; 11]);
        assert_eq!(m.diplomacy_started, 11);
        assert_eq!((m.proposals_accepted, m.proposals_refused), (1, 0));
        assert_eq!((m.treaties_joined, tally.of(b).treaties_joined), (1, 1));
        Ok(())
    }

    #[test]
    fn only_a_sailing_unit_with_room_is_a_ship() {
        let ships: Vec<_> = shipped()
            .rules
            .units
            .iter()
            .map(|(_, d)| d)
            .filter(|d| is_ship(d))
            .collect();
        assert!(ships.iter().any(|d| d.id == "galley"));
        assert!(ships.iter().all(|d| d.movement == MovementType::Sailing));
        assert!(!ships.iter().any(|d| d.id == "spearmen"));
    }

    /// The cast counts add up across games, are listed most-cast first, and a map spell that is
    /// not among them is reported as never cast.
    #[test]
    fn casts_are_added_listed_and_the_missing_map_spells_reported() {
        let one = BTreeMap::from([("heroism".to_string(), 2), ("bless".to_string(), 1)]);
        let two = BTreeMap::from([("heroism".to_string(), 3)]);
        let sum = add_casts([&one, &two]);
        assert_eq!(sum["heroism"], 5);
        assert_eq!(casts_line(&sum), "heroism 5, bless 1");
        let missing = never_cast(&shipped().rules, &sum);
        assert!(!missing.iter().any(|s| s == "heroism" || s == "bless"));
        assert!(missing.iter().any(|s| s == "transmute"), "{missing:?}");
        assert!(
            !missing.iter().any(|s| s == "magic_vortex"),
            "a battle-only spell is not a map spell"
        );
    }

    /// A refused cast and the failures that come as events are counted by spell and reason; a
    /// command that is no cast is not counted.
    #[test]
    fn failed_casts_are_counted_by_spell_and_reason() -> TestResult {
        let rules = &shipped().rules;
        let (id, def) = rules.spells.iter().next().ok_or("no spells")?;
        let world = f4x_master::Game::from(new_game(
            shipped(),
            &NewGame {
                seed: 4,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?);
        let mut tally = Tally::new(&world);
        let command =
            format!("CastSpell {{ player: PlayerId(1), spell: {id:?}, target: None, extra: 0 }}");
        tally.note_refused_cast(&world, &command, "Magic(NotEnoughMana)");
        tally.note_refused_cast(&world, &command, "Magic(NotEnoughMana)");
        tally.note_refused_cast(&world, "GoTo { units: [] }", "Move(Treaty)");
        tally.record(
            &world,
            world.round(),
            None,
            &[Event::NoTargetForCast {
                player: PlayerId(1),
                spell: id,
            }],
        );
        let row = &tally.cast_failures()[&def.id];
        assert_eq!(row["map: refused, Magic(NotEnoughMana)"], 2);
        assert_eq!(row["map: nothing to aim at"], 1);
        assert_eq!(tally.cast_failures().len(), 1, "a move is not a cast");
        Ok(())
    }

    /// A wizard's and a unit's casts in a battle are counted by the spell, apart from the map's
    /// casts, and a battle spell that is not among them is reported as never cast.
    #[test]
    fn battle_casts_are_counted_apart_from_the_maps() -> TestResult {
        let rules = &shipped().rules;
        let spell = |id: &str| {
            rules
                .spells
                .iter()
                .find(|(_, d)| d.id == id)
                .map(|(i, _)| i)
                .ok_or(format!("no spell {id}"))
        };
        let world = f4x_master::Game::from(new_game(
            shipped(),
            &NewGame {
                seed: 4,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?);
        let mut tally = Tally::new(&world);
        let bolt = spell("fire_bolt")?;
        let side = f4x_battle::Side::Attacker;
        tally.record(
            &world,
            world.round(),
            None,
            &[
                Event::Battle(BattleEvent::SpellCast {
                    side,
                    spell: bolt,
                    skill: 5,
                    mana: f4x_def::half_int::HalfInt::from_int(5),
                }),
                Event::Battle(BattleEvent::SpellCast {
                    side,
                    spell: bolt,
                    skill: 5,
                    mana: f4x_def::half_int::HalfInt::from_int(5),
                }),
                Event::Battle(BattleEvent::UnitSpellCast {
                    unit: 0,
                    spell: spell("web")?,
                    mana: 0,
                }),
            ],
        );
        assert_eq!(tally.battle_casts()["fire_bolt"], 2);
        assert_eq!(tally.battle_casts()["web"], 1);
        assert!(tally.casts().is_empty(), "a battle cast is not a map cast");
        let never = never_cast_in(rules, tally.battle_casts(), CastContext::Battle);
        assert!(!never.iter().any(|s| s == "fire_bolt" || s == "web"));
        assert!(never.iter().any(|s| s == "magic_vortex"), "{never:?}");
        Ok(())
    }
}
