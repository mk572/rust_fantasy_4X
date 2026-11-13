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

/// Turns a settler may take to reach the site it chose and found its city (it needs far less
/// than a campaign: a few dozen squares on foot).
pub const SETTLER_TIMEOUT_TURNS: u32 = 30;

/// Turns a scout may walk to one target before the target is given up (a scout that finds new
/// squares gets new targets and so a new clock).
pub const SCOUT_TIMEOUT_TURNS: u32 = 50;

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
    /// A settler on its way to the site it chose (key: the settler and the site).
    Settler,
    /// A scout on its way to a target (key: the scout and the target).
    Scout,
    /// The search for the rest of the world by ship (and the fighters it puts ashore).
    SeaScout,
    /// The scout on its way to an open tower, to change plane.
    Planar,
}

// The march of a campaign stack has no kind of its own: its target is the campaign's, which has
// its clock, and `military.rs` calls the stack back every `MARCH_TURNS` and sends it out again.

impl PlanKind {
    pub const ALL: [PlanKind; 8] = [
        PlanKind::Campaign,
        PlanKind::Landing,
        PlanKind::Colony,
        PlanKind::Raid,
        PlanKind::Settler,
        PlanKind::Scout,
        PlanKind::SeaScout,
        PlanKind::Planar,
    ];

    /// Turns a plan of this kind may stand.
    pub fn timeout(self) -> u32 {
        match self {
            PlanKind::Settler => SETTLER_TIMEOUT_TURNS,
            PlanKind::Scout => SCOUT_TIMEOUT_TURNS,
            _ => PLAN_TIMEOUT_TURNS,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PlanKind::Campaign => "campaign",
            PlanKind::Landing => "landing",
            PlanKind::Colony => "colony",
            PlanKind::Raid => "raid",
            PlanKind::Settler => "settler",
            PlanKind::Scout => "scout",
            PlanKind::SeaScout => "sea scout",
            PlanKind::Planar => "planar",
        }
    }

    fn slot(self) -> usize {
        self as usize
    }
}

/// What a plan is about, to tell a new plan from the one that stands: a square, a unit's number,
/// or a unit and a square (a new target of the unit is a new plan).
type Key = (u64, u64);

fn square(p: Position) -> Key {
    (
        0,
        (p.plane as u64) << 48 | u64::from(p.y) << 24 | u64::from(p.x),
    )
}

fn unit(s: crate::Serial) -> Key {
    (s.0, 0)
}

fn unit_going(s: crate::Serial, to: Position) -> Key {
    (s.0, square(to).1)
}

/// When each standing plan began, and how many the clock has dropped.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlanClock {
    started: Vec<(PlanKind, Key, u32)>,
    dropped: Vec<u32>,
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
                Some(&(_, _, since)) if now.saturating_sub(since) >= kind.timeout() => {
                    expired.push(key);
                }
                Some(_) => {}
                None => self.started.push((kind, key, now)),
            }
        }
        for key in &expired {
            self.started.retain(|(k, q, _)| !(*k == kind && q == key));
            if self.dropped.len() <= kind.slot() {
                self.dropped.resize(PlanKind::ALL.len(), 0);
            }
            self.dropped[kind.slot()] += 1;
        }
        expired
    }

    /// The raiders in `party` have done something that counts (cleared a site): their time starts
    /// again at the next turn's tick.
    pub(crate) fn restart_raiders(&mut self, party: &[crate::Serial]) {
        self.started
            .retain(|(k, key, _)| !(*k == PlanKind::Raid && party.iter().any(|s| s.0 == key.0)));
    }

    /// How many plans of `kind` the clock has dropped.
    pub fn dropped(&self, kind: PlanKind) -> u32 {
        self.dropped.get(kind.slot()).copied().unwrap_or(0)
    }
}

/// What the AI has to undo in the game after a drop: the units whose orders are cleared (they
/// would walk on to the target given up), and whether the ships on the sea scouting are called
/// back.
#[derive(Debug, Default)]
pub(crate) struct Dropped {
    pub release: Vec<crate::Serial>,
    pub sea: bool,
}

impl Ai {
    /// Drop every plan that has stood for its time ([`PLAN_TIMEOUT_TURNS`], or less for a settler
    /// and a scout); its target goes on the list of failures, to be tried again later, and the AI
    /// chooses anew. Called at the start of a turn.
    pub(crate) fn drop_stale_plans(&mut self, turn: u32) -> Dropped {
        let mut out = Dropped::default();
        let mut clock = std::mem::take(&mut self.clock);
        if turn.saturating_sub(clock.forgiven) >= FORGIVE_TURNS {
            clock.forgiven = turn;
            self.naval_bad.clear();
            self.colony_bad.clear();
            self.bad_sites.clear();
            self.bad_scout_targets.clear();
            if std::mem::take(&mut self.sea_gave_up) {
                self.sea_done = false;
            }
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
        let raiders: Vec<Key> = self.raiders.iter().map(|&s| unit(s)).collect();
        for key in clock.tick(PlanKind::Raid, turn, &raiders) {
            self.raiders.retain(|s| s.0 != key.0);
        }
        let settlers: Vec<Key> = self
            .settler_targets
            .iter()
            .map(|(&s, &p)| unit_going(s, p))
            .collect();
        for key in clock.tick(PlanKind::Settler, turn, &settlers) {
            let found = self
                .settler_targets
                .iter()
                .find(|(&s, &p)| unit_going(s, p) == key)
                .map(|(&s, &p)| (s, p));
            if let Some((serial, site)) = found {
                self.settler_targets.remove(&serial);
                self.settler_last.remove(&serial);
                self.bad_sites.push(site);
                out.release.push(serial);
            }
        }
        let scouts: Vec<Key> = self
            .scout_targets
            .iter()
            .map(|(&s, &p)| unit_going(s, p))
            .collect();
        for key in clock.tick(PlanKind::Scout, turn, &scouts) {
            let found = self
                .scout_targets
                .iter()
                .find(|(&s, &p)| unit_going(s, p) == key)
                .map(|(&s, &p)| (s, p));
            if let Some((serial, target)) = found {
                self.scout_targets.remove(&serial);
                self.scout_last.remove(&serial);
                self.bad_scout_targets.push(target);
                out.release.push(serial);
            }
        }
        let sea: Vec<Key> = if self.sea_scouting {
            vec![(0, 0)]
        } else {
            vec![]
        };
        if !clock.tick(PlanKind::SeaScout, turn, &sea).is_empty() {
            self.sea_scouting = false;
            self.sea_done = true;
            self.sea_gave_up = true;
            self.sea_port = None;
            self.sea_party.clear();
            out.sea = true;
        }
        let planar: Vec<Key> = self.planar.map(unit).into_iter().collect();
        for key in clock.tick(PlanKind::Planar, turn, &planar) {
            if let Some(serial) = self.planar.filter(|&s| unit(s) == key) {
                self.planar = None;
                self.scouts.retain(|&s| s != serial);
                if let Some(tower) = self.planar_tower.take() {
                    self.bad_scout_targets.push(tower);
                }
                out.release.push(serial);
            }
        }
        self.clock = clock;
        out
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
    fn a_campaign_dropped_also_ends_its_march() {
        let (mut ai, turn) = ai_at(10);
        ai.campaign = Some(at(9, 9));
        ai.march_target = Some(at(9, 9));
        ai.marching = vec![crate::Serial(7)];
        ai.drop_stale_plans(turn);
        ai.drop_stale_plans(turn + PLAN_TIMEOUT_TURNS);
        assert!(ai.marching.is_empty() && ai.march_target.is_none());
    }

    #[test]
    fn a_settler_that_does_not_get_there_gives_its_site_up_and_is_released() {
        let (mut ai, turn) = ai_at(10);
        let s = crate::Serial(5);
        ai.settler_targets.insert(s, at(9, 9));
        ai.settler_last.insert(s, at(1, 1));
        ai.drop_stale_plans(turn);
        let early = ai.drop_stale_plans(turn + SETTLER_TIMEOUT_TURNS - 1);
        assert!(early.release.is_empty() && ai.settler_targets.contains_key(&s));
        let out = ai.drop_stale_plans(turn + SETTLER_TIMEOUT_TURNS);
        assert_eq!(out.release, vec![s]);
        assert!(ai.settler_targets.is_empty() && ai.settler_last.is_empty());
        assert_eq!(ai.bad_sites, vec![at(9, 9)]);
        assert_eq!(ai.clock.dropped(PlanKind::Settler), 1);
    }

    #[test]
    fn a_settler_that_keeps_getting_new_sites_is_never_dropped() {
        let (mut ai, turn) = ai_at(10);
        let s = crate::Serial(5);
        for t in 0..3 * SETTLER_TIMEOUT_TURNS {
            // A new site every twenty turns: each is a new plan with its own clock.
            ai.settler_targets.insert(s, at(2 + t / 20, 2));
            let out = ai.drop_stale_plans(turn + t);
            assert!(out.release.is_empty(), "turn {t}");
        }
        assert_eq!(ai.clock.dropped(PlanKind::Settler), 0);
    }

    #[test]
    fn a_scout_walking_to_one_target_too_long_gives_it_up_but_one_with_new_targets_does_not() {
        let (mut ai, turn) = ai_at(10);
        let (walker, finder) = (crate::Serial(1), crate::Serial(2));
        ai.scout_targets.insert(walker, at(9, 9));
        let mut released = Vec::new();
        for t in 0..=SCOUT_TIMEOUT_TURNS {
            ai.scout_targets.insert(finder, at(2 + t / 10, 3));
            released.extend(ai.drop_stale_plans(turn + t).release);
        }
        assert_eq!(released, vec![walker]);
        assert_eq!(ai.bad_scout_targets, vec![at(9, 9)]);
        assert!(ai.scout_targets.contains_key(&finder));
        assert_eq!(ai.clock.dropped(PlanKind::Scout), 1);
    }

    #[test]
    fn a_sea_scouting_that_never_ends_is_called_off_and_tried_again_later() {
        let (mut ai, turn) = ai_at(10);
        ai.sea_scouting = true;
        ai.sea_port = Some(at(1, 1));
        ai.sea_party = vec![crate::Serial(3)];
        ai.drop_stale_plans(turn);
        assert!(!ai.drop_stale_plans(turn + PLAN_TIMEOUT_TURNS - 1).sea);
        assert!(ai.drop_stale_plans(turn + PLAN_TIMEOUT_TURNS).sea);
        assert!(
            !ai.sea_scouting && ai.sea_done && ai.sea_port.is_none() && ai.sea_party.is_empty()
        );
        assert_eq!(ai.clock.dropped(PlanKind::SeaScout), 1);
        // The failure lists are forgiven after a while, and with them this.
        ai.drop_stale_plans(turn + PLAN_TIMEOUT_TURNS + FORGIVE_TURNS);
        assert!(!ai.sea_done);
    }

    #[test]
    fn a_sea_scouting_that_found_nothing_stays_over_when_the_lists_are_forgiven() {
        let (mut ai, turn) = ai_at(10);
        ai.sea_done = true;
        ai.drop_stale_plans(turn);
        ai.drop_stale_plans(turn + FORGIVE_TURNS);
        assert!(ai.sea_done);
    }

    #[test]
    fn a_planar_scout_that_never_reaches_the_tower_is_let_go_and_the_tower_set_aside() {
        let (mut ai, turn) = ai_at(10);
        let s = crate::Serial(4);
        ai.planar = Some(s);
        ai.planar_tower = Some(at(5, 5));
        ai.scouts = vec![s];
        ai.drop_stale_plans(turn);
        let out = ai.drop_stale_plans(turn + PLAN_TIMEOUT_TURNS);
        assert_eq!(out.release, vec![s]);
        assert!(ai.planar.is_none() && ai.scouts.is_empty());
        assert_eq!(ai.bad_scout_targets, vec![at(5, 5)]);
        assert_eq!(ai.clock.dropped(PlanKind::Planar), 1);
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
