//! A time limit on every plan of the AI (task 515): a plan that is not resolved after
//! [`PLAN_TIMEOUT_TURNS`] turns is dropped, whatever it is, and the AI plans something else. A plan
//! that never resolves is a bug of the AI (an army that waits for a ship that is never built, a
//! landing on a coast nothing reaches); the clock does not find the bug, it only keeps the AI
//! from sitting on it for hundreds of turns. Shorten the constant as the bugs are fixed.
//!
//! Tests: in this file; the plans' own tests are in `military.rs`, `naval.rs` and `colony.rs`.

use f4x_core::map::Position;

use crate::Ai;

/// Turns a plan may stand before it is dropped.
pub const PLAN_TIMEOUT_TURNS: u32 = 100;

/// Turns a dropped target stays on the AI's list of failures before it may be tried again.
const FORGIVE_TURNS: u32 = 3 * PLAN_TIMEOUT_TURNS;

/// The kinds of plan the clock watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlanKind {
    /// The enemy city the army gathers and marches against (with the tower it crosses by).
    Campaign,
    /// The landing from a port against a city on another land mass.
    Landing,
    /// Settlers by sea.
    Colony,
    /// A unit out on a raid of a tower, node, encounter site or neutral city.
    Raid,
}

impl PlanKind {
    pub const ALL: [PlanKind; 4] = [
        PlanKind::Campaign,
        PlanKind::Landing,
        PlanKind::Colony,
        PlanKind::Raid,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PlanKind::Campaign => "campaign",
            PlanKind::Landing => "landing",
            PlanKind::Colony => "colony",
            PlanKind::Raid => "raid",
        }
    }

    fn slot(self) -> usize {
        self as usize
    }
}

/// What a plan is about, to tell a new plan from the one that stands: a square, or a unit's number.
type Key = u64;

fn square(p: Position) -> Key {
    (p.plane as u64) << 48 | u64::from(p.y) << 24 | u64::from(p.x)
}

/// When each standing plan began, and how many the clock has dropped.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlanClock {
    started: Vec<(PlanKind, Key, u32)>,
    dropped: [u32; 4],
    /// The turn the failure lists were last forgiven.
    #[serde(default)]
    forgiven: u32,
}

impl PlanClock {
    /// The plans of `kind` that stand now, by key: the clock forgets the ones that ended and
    /// returns the keys that have stood for too long. A plan seen for the first time starts now.
    fn tick(&mut self, kind: PlanKind, now: u32, standing: &[Key]) -> Vec<Key> {
        self.started
            .retain(|(k, key, _)| *k != kind || standing.contains(key));
        let mut expired = Vec::new();
        for &key in standing {
            match self
                .started
                .iter()
                .find(|(k, q, _)| *k == kind && *q == key)
            {
                Some(&(_, _, since)) if now.saturating_sub(since) >= PLAN_TIMEOUT_TURNS => {
                    expired.push(key);
                }
                Some(_) => {}
                None => self.started.push((kind, key, now)),
            }
        }
        for key in &expired {
            self.started.retain(|(k, q, _)| !(*k == kind && q == key));
            self.dropped[kind.slot()] += 1;
        }
        expired
    }

    /// The raiders in `party` have done something that counts (cleared a site): their time starts
    /// again at the next turn's tick.
    pub(crate) fn restart_raiders(&mut self, party: &[crate::Serial]) {
        self.started
            .retain(|(k, key, _)| !(*k == PlanKind::Raid && party.iter().any(|s| s.0 == *key)));
    }

    /// How many plans of `kind` the clock has dropped.
    pub fn dropped(&self, kind: PlanKind) -> u32 {
        self.dropped[kind.slot()]
    }
}

impl Ai {
    /// Drop every plan that has stood for [`PLAN_TIMEOUT_TURNS`]; its target goes on the list of
    /// failures, to be tried again later, and the AI chooses anew. Called at the start of a turn.
    pub(crate) fn drop_stale_plans(&mut self, turn: u32) {
        let mut clock = std::mem::take(&mut self.clock);
        if turn.saturating_sub(clock.forgiven) >= FORGIVE_TURNS {
            clock.forgiven = turn;
            self.naval_bad.clear();
            self.colony_bad.clear();
        }
        let campaign: Vec<Key> = self.campaign.map(square).into_iter().collect();
        for key in clock.tick(PlanKind::Campaign, turn, &campaign) {
            if let Some(target) = self.campaign.filter(|&c| square(c) == key) {
                self.campaign = None;
                self.crossing = None;
                self.march_target = None;
                self.marching.clear();
                self.unreachable.push(target);
                self.unreachable_since = turn;
            }
        }
        let landing: Vec<Key> = self.naval.map(|p| square(p.target)).into_iter().collect();
        for key in clock.tick(PlanKind::Landing, turn, &landing) {
            if let Some(plan) = self.naval.filter(|p| square(p.target) == key) {
                self.naval = None;
                self.naval_bad.push(plan.target);
            }
        }
        let colony: Vec<Key> = self.colony.map(|c| square(c.land)).into_iter().collect();
        for key in clock.tick(PlanKind::Colony, turn, &colony) {
            if let Some(c) = self.colony.filter(|c| square(c.land) == key) {
                self.colony = None;
                self.colony_bad.push(c.land);
            }
        }
        let raiders: Vec<Key> = self.raiders.iter().map(|s| s.0).collect();
        for key in clock.tick(PlanKind::Raid, turn, &raiders) {
            self.raiders.retain(|s| s.0 != key);
        }
        self.clock = clock;
    }

    /// How many plans of each kind the clock has dropped, for the soak's summary.
    pub fn plans_dropped(&self) -> Vec<(&'static str, u32)> {
        PlanKind::ALL
            .iter()
            .map(|&k| (k.name(), self.clock.dropped(k)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};

    fn ai_at(turn: u32) -> (Ai, u32) {
        (Ai::new(P0), turn)
    }

    #[test]
    fn a_campaign_is_dropped_after_the_time_and_its_target_is_set_aside() {
        let (mut ai, turn) = ai_at(10);
        let target = at(9, 9);
        ai.campaign = Some(target);
        ai.crossing = Some(at(1, 1));
        for t in turn..turn + PLAN_TIMEOUT_TURNS {
            ai.drop_stale_plans(t);
            assert_eq!(ai.campaign, Some(target), "turn {t}");
        }
        ai.drop_stale_plans(turn + PLAN_TIMEOUT_TURNS);
        assert_eq!(ai.campaign, None);
        assert_eq!(ai.crossing, None);
        assert!(ai.unreachable.contains(&target));
        assert_eq!(ai.clock.dropped(PlanKind::Campaign), 1);
    }

    #[test]
    fn a_new_plan_starts_its_own_clock() {
        let (mut ai, turn) = ai_at(10);
        ai.campaign = Some(at(9, 9));
        ai.drop_stale_plans(turn);
        ai.drop_stale_plans(turn + 60);
        ai.campaign = Some(at(8, 8));
        ai.drop_stale_plans(turn + 61);
        ai.drop_stale_plans(turn + PLAN_TIMEOUT_TURNS + 20);
        assert_eq!(
            ai.campaign,
            Some(at(8, 8)),
            "the new target has stood 80 turns"
        );
        assert_eq!(ai.clock.dropped(PlanKind::Campaign), 0);
    }

    #[test]
    fn a_landing_is_dropped_and_its_target_goes_on_the_failure_list() {
        let (mut ai, turn) = ai_at(10);
        let plan = crate::naval::Plan {
            target: at(9, 9),
            land: at(8, 9),
            sea: at(7, 9),
            port: at(COAST_CITY.0, COAST_CITY.1),
        };
        ai.naval = Some(plan);
        ai.drop_stale_plans(turn);
        ai.drop_stale_plans(turn + PLAN_TIMEOUT_TURNS);
        assert_eq!(ai.naval, None);
        assert_eq!(ai.naval_bad, vec![plan.target]);
        assert_eq!(ai.clock.dropped(PlanKind::Landing), 1);
    }

    #[test]
    fn a_colony_voyage_is_dropped() {
        let (mut ai, turn) = ai_at(10);
        let c = crate::colony::Colony {
            port: at(1, 1),
            sea: at(2, 2),
            land: at(3, 3),
        };
        ai.colony = Some(c);
        ai.drop_stale_plans(turn);
        ai.drop_stale_plans(turn + PLAN_TIMEOUT_TURNS);
        assert_eq!(ai.colony, None);
        assert_eq!(ai.colony_bad, vec![c.land]);
        assert_eq!(ai.clock.dropped(PlanKind::Colony), 1);
    }

    #[test]
    fn a_raider_out_for_the_time_is_called_back_and_a_fresh_one_is_not() {
        let mut g = game(COAST_CITY, 8);
        let units = spawn(&mut g, P0, at(6, 3), 105, 2);
        let mut ai = Ai::new(P0);
        let view = f4x_core::player::view::PlayerView::new(&g, P0);
        let old = view.serial_of(units[0]);
        ai.raiders = vec![old];
        ai.drop_stale_plans(10);
        let fresh = view.serial_of(units[1]);
        ai.raiders.push(fresh);
        ai.drop_stale_plans(10 + PLAN_TIMEOUT_TURNS - 1);
        assert_eq!(ai.raiders, vec![old, fresh]);
        ai.drop_stale_plans(10 + PLAN_TIMEOUT_TURNS);
        assert_eq!(ai.raiders, vec![fresh]);
        assert_eq!(ai.clock.dropped(PlanKind::Raid), 1);
    }

    #[test]
    fn the_failure_lists_are_forgiven_after_a_while() {
        let (mut ai, _) = ai_at(10);
        ai.naval_bad.push(at(9, 9));
        ai.colony_bad.push(at(3, 3));
        ai.drop_stale_plans(10);
        assert!(!ai.naval_bad.is_empty());
        ai.drop_stale_plans(10 + FORGIVE_TURNS);
        assert!(ai.naval_bad.is_empty() && ai.colony_bad.is_empty());
    }
}
