//! The war plan (task 648, `docs/ai-design.md` section 15): the layer between the diplomacy that
//! decides on a war and the army that fights it. It rates every known enemy town twice, as the
//! war goal (the prize, changed rarely) and as the next target (the step now), keeps each choice
//! until something is clearly better (`goal_hold`, `next_hold`), and names the recon: the towns
//! with unexplored ground around them, which the scouts look at before others
//! (`scout::best_target_with`). What is not built yet is in the doc and task 648.
//!
//! Tests: at the end of this file, the ratings on plain data and the plan on the shipped rules.

use f4x_def::rules::ai_strategy::TargetLevers;

use crate::lever::Levered;
use std::collections::HashMap;

use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;

use crate::view::{distance, is_fighter};

/// One town of the enemy, as the AI knows it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Town {
    pub pos: Position,
    pub owner: PlayerId,
    pub fortress: bool,
    /// Citizens.
    pub figures: u32,
    pub buildings: usize,
    /// A wizard (not rebels or raiders).
    pub rival: bool,
    /// A wizard we are at war with.
    pub war: bool,
    /// The defence to beat, in the units of `fighting_strength`, walls counted.
    pub defence: f32,
    /// The squares from our nearest city (not across planes).
    pub from_us: Option<u32>,
    /// Can the army walk to a square beside it?
    pub on_foot: bool,
}

/// What the plan keeps between turns.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct WarPlan {
    /// The prize of the war.
    pub goal: Option<Position>,
    /// The town to attack now, on the way to the goal.
    pub next: Option<Position>,
    /// The towns with ground around them that the scouts have not seen.
    pub recon: Vec<Position>,
}

/// What the war asks of the army to take `target`: the strength that is `attack_force` times what
/// is expected there, so that the force gathered is one that attacks (`attack.rs`). The march
/// waits for it, the cities build towards it.
pub(crate) fn strength_wanted(
    view: &PlayerView,
    memory: &HashMap<Position, (f32, u32)>,
    turn: u32,
    target: Position,
) -> f32 {
    let force = view.levers().attack.attack_force;
    crate::military::expected_defence(view, memory, turn, target) * force + 1.0
}

/// How many average fighters a strength is, for the cities that build them: at least what a march
/// starts with, at most what a battle holds.
pub(crate) fn units_for(view: &PlayerView, strength: f32) -> usize {
    let levers = view.levers();
    ((strength / crate::military::average_strength(view).max(1.0)).ceil() as usize).clamp(
        levers.attack.min_march,
        levers.target.max_force.min(view.max_battle_side()),
    )
}

/// What a town is worth to take.
pub(crate) fn worth(levers: &TargetLevers, t: &Town) -> f32 {
    let mut worth =
        t.figures as f32 * levers.size_worth + t.buildings as f32 * levers.building_worth;
    if t.fortress {
        worth += levers.fortress_worth;
    }
    if t.war {
        worth += levers.war_worth;
    }
    if !t.rival {
        worth *= levers.neutral_share;
    }
    worth
}

/// How good the town is as the goal of the war.
pub(crate) fn goal_score(levers: &TargetLevers, t: &Town) -> f32 {
    let walk = t.from_us.unwrap_or(0) as f32 * levers.goal_distance;
    let across = if t.on_foot { 0.0 } else { levers.goal_across };
    worth(levers, t) - walk - across
}

/// How good the town is as the next target. `defence` is in average units of the AI's own fighters.
pub(crate) fn next_score(
    levers: &TargetLevers,
    t: &Town,
    average: f32,
    on_route: bool,
    foothold: bool,
) -> f32 {
    let walk = t.from_us.unwrap_or(40) as f32 * levers.next_distance;
    let held = t.defence / average.max(1.0) * levers.defence_squares;
    let mut score = levers.next_worth * worth(levers, t) - walk - held;
    if on_route {
        score += levers.route_bonus;
    }
    if foothold {
        score += levers.next_foothold;
    }
    score
}

/// The towns the AI knows of the enemy: every city of another player it has explored, but those
/// it may not attack (`attack::may_target`: a pact, an alliance, a peace period), those in
/// `skip` (no march found a way to them) and, when a wizard is about to win with the Spell of
/// Mastery and one of its towns is known, the towns of anyone else.
pub(crate) fn towns(
    view: &PlayerView,
    memory: &HashMap<Position, (f32, u32)>,
    turn: u32,
    capital: Position,
    skip: &[Position],
    focus: Option<PlayerId>,
) -> Vec<Town> {
    let known = view.known_cities();
    let focus = focus.filter(|&f| {
        known
            .iter()
            .any(|&(_, owner, p)| owner == f && !skip.contains(&p))
    });
    let walker = view
        .garrison(capital)
        .into_iter()
        .find(|&u| is_fighter(view, u));
    let reach = walker.map(|w| view.reachable(&[w]));
    let mine: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
    let at_war = view.rivals_at_war();
    let mut towns = Vec::new();
    for (city, owner, pos) in known {
        if skip.contains(&pos) || focus.is_some_and(|f| owner != f) {
            continue;
        }
        let rival = view.is_rival_wizard(owner);
        if !crate::attack::may_target(view, owner) {
            continue;
        }
        let Some(report) = view.city_report(city) else {
            continue;
        };
        let on_foot = match &reach {
            None => true,
            Some(set) => crate::military::neighbours(view, pos)
                .into_iter()
                .any(|s| view.hostile_at(s).is_none() && set.contains(&s)),
        };
        towns.push(Town {
            pos,
            owner,
            fortress: report.fortress,
            figures: report.figures,
            buildings: report.buildings.len(),
            rival,
            war: at_war.contains(&owner),
            defence: crate::military::expected_defence(view, memory, turn, pos),
            from_us: mine.iter().filter_map(|&m| distance(view, m, pos)).min(),
            on_foot,
        });
    }
    towns
}

/// The better of two scores, a tie going to the square that comes first (the choice must not
/// depend on the order the towns came in).
fn better(a: (f32, Position), b: (f32, Position)) -> bool {
    a.0 > b.0 || (a.0 == b.0 && (a.1.y, a.1.x) < (b.1.y, b.1.x))
}

/// Revise the plan from the towns and return the town to march on, `None` when the army cannot
/// walk to any (the goal is then across; the caller sets it aside for the ships). `origin` is the
/// city the army gathers in.
pub(crate) fn revise(
    plan: &mut WarPlan,
    view: &PlayerView,
    towns: &[Town],
    origin: Position,
) -> Option<Position> {
    if towns.is_empty() {
        *plan = WarPlan::default();
        return None;
    }
    let levers = &view.levers().target;
    // The goal: the best town, kept while it is not clearly bettered.
    let best_goal = towns
        .iter()
        .map(|t| (goal_score(levers, t), t.pos))
        .reduce(|a, b| if better(b, a) { b } else { a })?;
    let held = plan
        .goal
        .and_then(|g| towns.iter().find(|t| t.pos == g))
        .map(|t| (goal_score(levers, t), t.pos));
    let goal = match held {
        Some(h) if best_goal.0 <= h.0 + levers.goal_hold => h.1,
        _ => best_goal.1,
    };
    plan.goal = Some(goal);
    let goal_town = towns.iter().find(|t| t.pos == goal)?;

    // The foothold: when the goal cannot be walked to, the town that can, nearest to it.
    let foothold = (!goal_town.on_foot)
        .then(|| {
            towns
                .iter()
                .filter(|t| t.on_foot)
                .filter_map(|t| distance(view, t.pos, goal).map(|d| (d, t.pos)))
                .min_by_key(|&(d, p)| (d, p.y, p.x))
                .map(|(_, p)| p)
        })
        .flatten();
    let direct = distance(view, origin, goal);
    let average = crate::military::average_strength(view);
    let score = |t: &Town| {
        let on_route = t.pos == goal
            || match (
                direct,
                distance(view, origin, t.pos),
                distance(view, t.pos, goal),
            ) {
                (Some(direct), Some(to), Some(on)) => to + on <= direct + levers.route_slack,
                _ => false,
            };
        next_score(levers, t, average, on_route, foothold == Some(t.pos))
    };
    let walkable = || towns.iter().filter(|t| t.on_foot);
    let best_next = walkable()
        .map(|t| (score(t), t.pos))
        .reduce(|a, b| if better(b, a) { b } else { a });
    let held = plan
        .next
        .and_then(|n| walkable().find(|t| t.pos == n))
        .map(|t| (score(t), t.pos));
    plan.next = match (best_next, held) {
        (Some(best), Some(h)) if best.0 <= h.0 + levers.next_hold => Some(h.1),
        (Some(best), _) => Some(best.1),
        (None, _) => None,
    };

    // The recon: the goal, the next target and the other towns with unexplored ground around.
    let mut order: Vec<&Town> = towns.iter().collect();
    order.sort_by_key(|t| {
        (
            t.pos != goal,
            Some(t.pos) != plan.next,
            t.from_us.unwrap_or(u32::MAX),
            t.pos.y,
            t.pos.x,
        )
    });
    plan.recon = order
        .into_iter()
        .filter(|t| unexplored_around(view, t.pos))
        .map(|t| t.pos)
        .take(levers.recon_towns)
        .collect();
    plan.next
}

/// Is any square within `recon_radius` of `at` not yet explored?
fn unexplored_around(view: &PlayerView, at: Position) -> bool {
    let radius = view.levers().target.recon_radius;
    (-radius..=radius).any(|dy| {
        (-radius..=radius).any(|dx| view.shift(at, dx, dy).is_some_and(|p| !view.explored(p)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levers() -> TargetLevers {
        crate::testworld::shipped().rules.ai_strategy.target
    }
    use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0, P1};

    fn town(x: u32, figures: u32) -> Town {
        Town {
            pos: at(x, 5),
            owner: P1,
            fortress: false,
            figures,
            buildings: 0,
            rival: true,
            war: true,
            defence: 20.0,
            from_us: Some(x),
            on_foot: true,
        }
    }

    #[test]
    fn the_fortress_is_worth_more_than_a_bigger_town() {
        let mut fortress = town(10, 5);
        fortress.fortress = true;
        let big = town(10, 12);
        assert!(worth(&levers(), &fortress) > worth(&levers(), &big));
        assert!(goal_score(&levers(), &fortress) > goal_score(&levers(), &big));
    }

    #[test]
    fn a_good_town_on_the_same_plane_comes_before_a_fortress_the_army_cannot_walk_to() {
        let mut fortress = town(10, 8);
        fortress.fortress = true;
        fortress.on_foot = false;
        fortress.from_us = None;
        let juicy = town(14, 9);
        assert!(
            goal_score(&levers(), &juicy) > goal_score(&levers(), &fortress),
            "{} against {}",
            goal_score(&levers(), &juicy),
            goal_score(&levers(), &fortress)
        );
    }

    #[test]
    fn a_rebel_town_is_worth_less_than_a_wizards_of_the_same_size() {
        let wizard = town(10, 8);
        let mut rebels = wizard.clone();
        rebels.rival = false;
        rebels.war = false;
        assert!(worth(&levers(), &rebels) < worth(&levers(), &wizard));
    }

    #[test]
    fn a_weak_town_behind_the_front_is_the_next_target_before_a_strong_near_one() {
        let mut near = town(8, 6);
        near.defence = 80.0;
        let mut back = town(14, 6);
        back.defence = 10.0;
        let average = 10.0;
        // 6 squares more to walk, 7 average units less to fight: the weak town wins.
        assert!(
            next_score(&levers(), &back, average, false, false)
                > next_score(&levers(), &near, average, false, false)
        );
    }

    #[test]
    fn the_way_to_the_goal_and_the_foothold_raise_the_next_target() {
        let t = town(10, 6);
        let base = next_score(&levers(), &t, 10.0, false, false);
        assert!(next_score(&levers(), &t, 10.0, true, false) > base);
        assert!(next_score(&levers(), &t, 10.0, false, true) > base);
    }

    /// The plan on a real map: the enemy's capital on the other island is no walk away, so the
    /// plan has a goal but no target to march on, and it names the capital for the ships.
    #[test]
    fn a_goal_across_the_sea_gives_no_town_to_march_on() -> Result<(), Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            104, /* spearmen */
            2,
        );
        let view = PlayerView::new(&w, P0);
        let origin = at(COAST_CITY.0, COAST_CITY.1);
        let list = towns(&view, &HashMap::new(), w.round(), origin, &[], None);
        assert_eq!(list.len(), 1, "{list:?}");
        assert_eq!(list[0].pos, at(ENEMY_CITY.0, ENEMY_CITY.1));
        assert!(!list[0].on_foot);
        let mut plan = WarPlan::default();
        assert_eq!(revise(&mut plan, &view, &list, origin), None);
        assert_eq!(plan.goal, Some(at(ENEMY_CITY.0, ENEMY_CITY.1)));
        assert_eq!(plan.next, None);
        Ok(())
    }

    /// A town that is not explored around is a recon target; an explored one is not.
    #[test]
    fn a_town_with_unexplored_ground_around_it_is_a_recon_target(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.explore(
            P0,
            at(ENEMY_CITY.0, ENEMY_CITY.1),
            f4x_def::half_int::HalfInt::from_raw(2),
        );
        spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 2);
        let view = PlayerView::new(&w, P0);
        let origin = at(COAST_CITY.0, COAST_CITY.1);
        let list = towns(&view, &HashMap::new(), w.round(), origin, &[], None);
        let mut plan = WarPlan::default();
        revise(&mut plan, &view, &list, origin);
        assert_eq!(plan.recon, vec![at(ENEMY_CITY.0, ENEMY_CITY.1)]);
        drop(view);
        w.reveal_map(P0);
        let view = PlayerView::new(&w, P0);
        let list = towns(&view, &HashMap::new(), w.round(), origin, &[], None);
        revise(&mut plan, &view, &list, origin);
        assert!(plan.recon.is_empty(), "{:?}", plan.recon);
        Ok(())
    }

    /// The choice is kept against a slightly better town and given up for a clearly better one.
    #[test]
    fn the_plan_keeps_its_target_until_another_is_clearly_better(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let w = game(COAST_CITY, 8);
        let view = PlayerView::new(&w, P0);
        let origin = at(COAST_CITY.0, COAST_CITY.1);
        let (a, b) = (town(12, 6), town(14, 6));
        let mut plan = WarPlan::default();
        revise(&mut plan, &view, &[a.clone(), b.clone()], origin);
        let first = plan.next.ok_or("a target")?;
        // The other town gets a little weaker: not enough to switch.
        let mut weaker = if first == a.pos { b.clone() } else { a.clone() };
        weaker.defence -= 1.0;
        let kept = [
            if first == a.pos { a.clone() } else { b.clone() },
            weaker.clone(),
        ];
        revise(&mut plan, &view, &kept, origin);
        assert_eq!(plan.next, Some(first));
        // Much weaker: it is taken.
        let mut much = weaker;
        much.defence = 0.0;
        much.from_us = Some(1);
        revise(
            &mut plan,
            &view,
            &[if first == a.pos { a } else { b }, much.clone()],
            origin,
        );
        assert_eq!(plan.next, Some(much.pos));
        Ok(())
    }

    /// The strength the war asks for grows with the defence expected, and the cities build at
    /// least a march's three and at most what a battle holds.
    #[test]
    fn the_strength_wanted_follows_the_expected_defence_and_the_builders_count_is_bounded() {
        let w = game(COAST_CITY, 8 /* high_men */);
        let view = PlayerView::new(&w, P0);
        let target = at(4, 4);
        let weak = HashMap::from([(target, (1.0, 2))]);
        let strong =
            HashMap::from([(target, (crate::military::average_strength(&view) * 30.0, 2))]);
        let (a, b) = (
            strength_wanted(&view, &weak, 2, target),
            strength_wanted(&view, &strong, 2, target),
        );
        assert!(b > a);
        assert_eq!(
            units_for(&view, a),
            crate::testworld::shipped()
                .rules
                .ai_strategy
                .attack
                .min_march
        );
        assert_eq!(
            units_for(&view, b),
            levers().max_force.min(view.max_battle_side()),
            "bounded by what a battle holds"
        );
    }
}
