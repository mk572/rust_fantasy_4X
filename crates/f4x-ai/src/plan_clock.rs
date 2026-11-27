//! A time limit on every plan of the AI (task 515): a plan that is not resolved after
//! `plan_timeout_turns` turns is dropped, whatever it is, and the AI plans something else. A plan
//! that never resolves is a bug of the AI (an army that waits for a ship that is never built, a
//! landing on a coast nothing reaches); the clock does not find the bug, it only keeps the AI
//! from sitting on it for hundreds of turns. Shorten the time as the bugs are fixed.
//!
//! Tests: in this file; the plans' own tests are in `military.rs`, `naval.rs` and `colony.rs`.

use f4x_core::map::Position;
use f4x_def::rules::ai_strategy::PlanningLevers;

use crate::Ai;
use f4x_master::Game;

/// Turns a dropped target stays on the AI's list of failures before it may be tried again:
/// `forgive_plans` times the time of a plan.
fn forgive_turns(levers: &PlanningLevers) -> u32 {
    levers
        .forgive_plans
        .saturating_mul(levers.plan_timeout_turns)
}

/// The kinds of plan the clock watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlanKind {
    /// The enemy city the army gathers and marches against (with the tower it crosses by).
    Campaign,
    /// The landing from a port against a city on another land mass.
    Landing,
    /// A unit out on a raid of a tower, node, encounter site or neutral city.
    Raid,
    /// A scout on its way to a target (key: the scout and the target).
    Scout,
    /// The search for the rest of the world by ship (and the fighters it puts ashore).
    SeaScout,
    /// The scout on its way to an open tower, to change plane.
    Planar,
}

// The march of a campaign stack has no kind of its own: its target is the campaign's, which has
// its clock, and `military.rs` calls the stack back every `march_turns` and sends it out again.

impl PlanKind {
    pub const ALL: [PlanKind; 6] = [
        PlanKind::Campaign,
        PlanKind::Landing,
        PlanKind::Raid,
        PlanKind::Scout,
        PlanKind::SeaScout,
        PlanKind::Planar,
    ];

    /// Turns a plan of this kind may stand. A scout's is the time it may walk to one target
    /// before the target is given up (a scout that finds new squares gets new targets and so a
    /// new clock).
    pub fn timeout(self, levers: &PlanningLevers) -> u32 {
        match self {
            PlanKind::Scout => levers.scout_timeout_turns,
            _ => levers.plan_timeout_turns,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PlanKind::Campaign => "campaign",
            PlanKind::Landing => "landing",
            PlanKind::Raid => "raid",
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
    fn tick(
        &mut self,
        levers: &PlanningLevers,
        kind: PlanKind,
        now: u32,
        standing: &[Key],
    ) -> Vec<Key> {
        self.started
            .retain(|(k, key, _)| *k != kind || standing.contains(key));
        let mut expired = Vec::new();
        for &key in standing {
            match self
                .started
                .iter()
                .find(|(k, q, _)| *k == kind && *q == key)
            {
                Some(&(_, _, since)) if now.saturating_sub(since) >= kind.timeout(levers) => {
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

    /// The clock dropped one plan of `kind`.
    fn count(&mut self, kind: PlanKind) {
        if self.dropped.len() <= kind.slot() {
            self.dropped.resize(PlanKind::ALL.len(), 0);
        }
        self.dropped[kind.slot()] += 1;
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
    /// Drop every plan that has stood for its time (`plan_timeout_turns`, or less for a scout); its target goes on the list of failures, to be tried again later, and the AI
    /// chooses anew. Called at the start of a turn.
    pub(crate) fn drop_stale_plans(&mut self, game: &mut Game, levers: &PlanningLevers) -> Dropped {
        let turn = game.round();
        let mut out = Dropped::default();
        let mut clock = std::mem::take(&mut self.clock);
        if turn.saturating_sub(clock.forgiven) >= forgive_turns(levers) {
            clock.forgiven = turn;
            self.naval_bad.clear();
            self.bad_scout_targets.clear();
            if std::mem::take(&mut self.sea_gave_up) {
                self.sea_done = false;
            }
        }
        let campaign: Vec<Key> = self.campaign.map(square).into_iter().collect();
        for key in clock.tick(levers, PlanKind::Campaign, turn, &campaign) {
            if let Some(target) = self.campaign.filter(|&c| square(c) == key) {
                self.campaign = None;
                self.crossing = None;
                self.march_target = None;
                self.write(game).release_role(crate::roles::Role::March);
                self.unreachable.push(target);
                self.unreachable_since = turn;
            }
        }
        let landing: Vec<Key> = self.naval.map(|p| square(p.target)).into_iter().collect();
        for key in clock.tick(levers, PlanKind::Landing, turn, &landing) {
            if let Some(plan) = self.naval.filter(|p| square(p.target) == key) {
                self.naval = None;
                self.naval_bad.push(plan.target);
            }
        }
        // The plans of single units: each unit's registry entry says when its plan began.
        let raids = self.expired_plans(game, levers, crate::roles::Role::Raid, PlanKind::Raid);
        for (serial, ..) in raids {
            self.write(game)
                .retain_role(crate::roles::Role::Raid, |s| s != serial);
            clock.count(PlanKind::Raid);
        }
        let scouts = self.expired_plans(game, levers, crate::roles::Role::Scout, PlanKind::Scout);
        for (serial, target) in scouts.into_iter().filter(|(_, to)| to.is_some()) {
            self.write(game)
                .set_target(serial, crate::roles::Role::Scout, None);
            self.write(game).clear_track(serial);
            if let Some(target) = target {
                self.bad_scout_targets.push(target);
            }
            out.release.push(serial);
            clock.count(PlanKind::Scout);
        }
        let sea: Vec<Key> = if self.sea_scouting {
            vec![(0, 0)]
        } else {
            vec![]
        };
        if !clock
            .tick(levers, PlanKind::SeaScout, turn, &sea)
            .is_empty()
        {
            self.sea_scouting = false;
            self.sea_done = true;
            self.sea_gave_up = true;
            self.sea_port = None;
            self.write(game).release_role(crate::roles::Role::SeaParty);
            out.sea = true;
        }
        let planar = self.expired_plans(
            game,
            levers,
            crate::roles::Role::PlanarScout,
            PlanKind::Planar,
        );
        for (serial, ..) in planar {
            self.write(game)
                .release(serial, crate::roles::Role::PlanarScout);
            if let Some(tower) = self.planar_tower.take() {
                self.bad_scout_targets.push(tower);
            }
            out.release.push(serial);
            clock.count(PlanKind::Planar);
        }
        self.clock = clock;
        out
    }

    /// The units holding `role` whose plan has stood for the time of `kind`, with their target.
    fn expired_plans(
        &self,
        game: &Game,
        levers: &PlanningLevers,
        role: crate::roles::Role,
        kind: PlanKind,
    ) -> Vec<(crate::Serial, Option<Position>)> {
        let now = game.round();
        self.tasks(game)
            .roles
            .plans(role)
            .into_iter()
            .filter(|&(_, planned, _)| now.saturating_sub(planned) >= kind.timeout(levers))
            .map(|(s, _, to)| (s, to))
            .collect()
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
    use crate::roles::Role;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};
    use f4x_core::player::view::PlayerView;
    use f4x_core::world::serial::Serial;

    fn planning() -> PlanningLevers {
        crate::testworld::levers().planning
    }

    fn ai_at(turn: u32) -> (Game, Ai, u32) {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.set_round(turn);
        (w, Ai::new(P0), turn)
    }

    /// The clock run in the round `round`.
    fn run_in(ai: &mut Ai, w: &mut Game, round: u32) -> Dropped {
        w.set_round(round);
        ai.drop_stale_plans(w, &planning())
    }

    /// The numbers of `count` new units.
    fn soldiers(w: &mut Game, count: usize) -> Vec<Serial> {
        let units = spawn(w, P0, at(6, 3), 105 /* high_men_swordsmen */, count);
        let view = PlayerView::new(w, P0);
        units.iter().map(|&u| view.serial_of(u)).collect()
    }

    #[test]
    fn a_campaign_is_dropped_after_the_time_and_its_target_is_set_aside() {
        let (mut w, mut ai, turn) = ai_at(10);
        let target = at(9, 9);
        ai.campaign = Some(target);
        ai.crossing = Some(at(1, 1));
        for t in turn..turn + planning().plan_timeout_turns {
            run_in(&mut ai, &mut w, t);
            assert_eq!(ai.campaign, Some(target), "turn {t}");
        }
        run_in(&mut ai, &mut w, turn + planning().plan_timeout_turns);
        assert_eq!(ai.campaign, None);
        assert_eq!(ai.crossing, None);
        assert!(ai.unreachable.contains(&target));
        assert_eq!(ai.clock.dropped(PlanKind::Campaign), 1);
    }

    #[test]
    fn a_new_plan_starts_its_own_clock() {
        let (mut w, mut ai, turn) = ai_at(10);
        ai.campaign = Some(at(9, 9));
        run_in(&mut ai, &mut w, turn);
        run_in(&mut ai, &mut w, turn + 60);
        ai.campaign = Some(at(8, 8));
        run_in(&mut ai, &mut w, turn + 61);
        run_in(&mut ai, &mut w, turn + planning().plan_timeout_turns + 20);
        assert_eq!(
            ai.campaign,
            Some(at(8, 8)),
            "the new target has stood 80 turns"
        );
        assert_eq!(ai.clock.dropped(PlanKind::Campaign), 0);
    }

    #[test]
    fn a_landing_is_dropped_and_its_target_goes_on_the_failure_list() {
        let (mut w, mut ai, turn) = ai_at(10);
        let plan = crate::naval::Plan {
            target: at(9, 9),
            land: at(8, 9),
            sea: at(7, 9),
            port: at(COAST_CITY.0, COAST_CITY.1),
        };
        ai.naval = Some(plan);
        run_in(&mut ai, &mut w, turn);
        run_in(&mut ai, &mut w, turn + planning().plan_timeout_turns);
        assert_eq!(ai.naval, None);
        assert_eq!(ai.naval_bad, vec![plan.target]);
        assert_eq!(ai.clock.dropped(PlanKind::Landing), 1);
    }

    #[test]
    fn a_raider_out_for_the_time_is_called_back_and_a_fresh_one_is_not() {
        let (mut w, mut ai, turn) = ai_at(10);
        let units = soldiers(&mut w, 2);
        let (old, fresh) = (units[0], units[1]);
        let _ = ai.write(&mut w).claim(old, Role::Raid, None);
        run_in(&mut ai, &mut w, turn);
        let late = turn + planning().plan_timeout_turns - 1;
        w.set_round(late);
        let _ = ai.write(&mut w).claim(fresh, Role::Raid, None);
        run_in(&mut ai, &mut w, late);
        assert_eq!(ai.tasks(&w).roles.members(Role::Raid), vec![old, fresh]);
        run_in(&mut ai, &mut w, late + 1);
        assert_eq!(ai.tasks(&w).roles.members(Role::Raid), vec![fresh]);
        assert_eq!(ai.clock.dropped(PlanKind::Raid), 1);
    }

    #[test]
    fn a_campaign_dropped_also_ends_its_march() {
        let (mut w, mut ai, turn) = ai_at(10);
        let marcher = soldiers(&mut w, 1)[0];
        ai.campaign = Some(at(9, 9));
        ai.march_target = Some(at(9, 9));
        let _ = ai.write(&mut w).claim(marcher, Role::March, None);
        run_in(&mut ai, &mut w, turn);
        run_in(&mut ai, &mut w, turn + planning().plan_timeout_turns);
        assert!(!ai.tasks(&w).roles.any(Role::March) && ai.march_target.is_none());
    }

    #[test]
    fn a_scout_walking_to_one_target_too_long_gives_it_up_but_one_with_new_targets_does_not() {
        let (mut w, mut ai, turn) = ai_at(10);
        let units = soldiers(&mut w, 2);
        let (walker, finder) = (units[0], units[1]);
        let _ = ai.write(&mut w).claim(walker, Role::Scout, Some(at(9, 9)));
        let _ = ai.write(&mut w).claim(finder, Role::Scout, None);
        let mut released = Vec::new();
        for t in 0..=planning().scout_timeout_turns {
            w.set_round(turn + t);
            ai.write(&mut w)
                .set_target(finder, Role::Scout, Some(at(2 + t / 10, 3)));
            released.extend(run_in(&mut ai, &mut w, turn + t).release);
        }
        assert_eq!(released, vec![walker]);
        assert_eq!(ai.bad_scout_targets, vec![at(9, 9)]);
        assert!(ai.tasks(&w).roles.target_of(finder, Role::Scout).is_some());
        assert_eq!(ai.clock.dropped(PlanKind::Scout), 1);
    }

    #[test]
    fn a_sea_scouting_that_never_ends_is_called_off_and_tried_again_later() {
        let (mut w, mut ai, turn) = ai_at(10);
        let rider = soldiers(&mut w, 1)[0];
        ai.sea_scouting = true;
        ai.sea_port = Some(at(1, 1));
        let _ = ai.write(&mut w).claim(rider, Role::SeaParty, None);
        run_in(&mut ai, &mut w, turn);
        assert!(!run_in(&mut ai, &mut w, turn + planning().plan_timeout_turns - 1).sea);
        assert!(run_in(&mut ai, &mut w, turn + planning().plan_timeout_turns).sea);
        assert!(
            !ai.sea_scouting
                && ai.sea_done
                && ai.sea_port.is_none()
                && !ai.tasks(&w).roles.any(Role::SeaParty)
        );
        assert_eq!(ai.clock.dropped(PlanKind::SeaScout), 1);
        // The failure lists are forgiven after a while, and with them this.
        run_in(
            &mut ai,
            &mut w,
            turn + planning().plan_timeout_turns + forgive_turns(&planning()),
        );
        assert!(!ai.sea_done);
    }

    #[test]
    fn a_sea_scouting_that_found_nothing_stays_over_when_the_lists_are_forgiven() {
        let (mut w, mut ai, turn) = ai_at(10);
        ai.sea_done = true;
        run_in(&mut ai, &mut w, turn);
        run_in(&mut ai, &mut w, turn + forgive_turns(&planning()));
        assert!(ai.sea_done);
    }

    #[test]
    fn a_planar_scout_that_never_reaches_the_tower_is_let_go_and_the_tower_set_aside() {
        let (mut w, mut ai, turn) = ai_at(10);
        let s = soldiers(&mut w, 1)[0];
        let _ = ai.write(&mut w).claim(s, Role::PlanarScout, None);
        ai.planar_tower = Some(at(5, 5));
        run_in(&mut ai, &mut w, turn);
        let out = run_in(&mut ai, &mut w, turn + planning().plan_timeout_turns);
        assert_eq!(out.release, vec![s]);
        assert!(ai.tasks(&w).roles.planar().is_none() && ai.tasks(&w).roles.scouting().is_empty());
        assert_eq!(ai.bad_scout_targets, vec![at(5, 5)]);
        assert_eq!(ai.clock.dropped(PlanKind::Planar), 1);
    }

    #[test]
    fn the_failure_lists_are_forgiven_after_a_while() {
        let (mut w, mut ai, turn) = ai_at(10);
        ai.naval_bad.push(at(9, 9));
        run_in(&mut ai, &mut w, turn);
        assert!(!ai.naval_bad.is_empty());
        run_in(&mut ai, &mut w, turn + forgive_turns(&planning()));
        assert!(ai.naval_bad.is_empty());
    }
}
