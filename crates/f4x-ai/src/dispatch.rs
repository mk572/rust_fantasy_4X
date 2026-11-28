//! The job board (task 645, `docs/ai-design.md` section 14). A unit with nothing to do awaits
//! orders where it stands; the dispatcher, once at the end of the wizard's turn, goes through the
//! open jobs in a fixed order and hands each the waiting units it needs. What no job takes stays
//! waiting.
//!
//! The pool is the units that are free (`Roles::is_free`): no routine has a task for them and they
//! are not a city's garrison (`garrison::post`, which gives a city the soldiers its defence
//! needs, slow defensive ones first). A job asks the game whether it is open, takes its units from
//! the pool by the registry's claim (a unit holds one role, so two jobs never have the same unit),
//! and gives them back when it is done.
//!
//! The order of the checklist (before it and after it, the attacks of `attack.rs`: a stack beside
//! something it may attack and is strong enough for fights it):
//!
//! 0. a city in danger (the perimeter: the free soldiers go for the enemy in its zone, or join the
//!    garrison when it is the stronger);
//! 1. settlers to their sites, engineers to their roads, spirits to their nodes;
//! 2. escorts for the settlers and engineers whose way is not safe;
//! 3. scouts for unexplored land, the fast units first;
//! 4. the war: the rally, the army against an enemy city (task 646); at peace it asks for nothing
//!    and is skipped, at war it picks its soldiers before the raids do;
//! 5. raiding parties for a lair, from the soldiers the war left over (646);
//! 6. the other under-defended cities (646);
//!
//! Then come the ships and the colonies (task 647), the scouting of the ships left with nothing to
//! carry, and the audit that counts what is left.

use crate::Ai;
use f4x_core::player::view::PlayerView;
use f4x_core::unit::task_walk::Check;
use f4x_core::unit::travel::Travel;
use f4x_core::world::{Event, World};
use f4x_master::Game;

/// What waits for a job: the units of the pool, by kind (read by the jobs of task 646; for now
/// only the tests count it).
#[cfg(test)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Pool {
    /// Soldiers (fighters) standing free, in a city beyond its garrison or in the field.
    pub soldiers: usize,
    /// Free units that are not fighters and not ships: settlers, engineers, spirits.
    pub others: usize,
    /// Ships with no mission.
    pub ships: usize,
}

/// Count the pool as the registry holds it (`roles::register` has run).
#[cfg(test)]
pub fn pool(ai: &Ai, world: &World) -> Pool {
    use crate::view::{is_fighter, is_ship};
    let view = f4x_core::player::view::PlayerView::new(world, ai.player);
    let mut pool = Pool::default();
    for unit in view.my_units() {
        let serial = view.serial_of(unit);
        if is_ship(&view, unit) {
            pool.ships += usize::from(
                ai.tasks(world)
                    .roles
                    .is(serial, crate::roles::Role::TransportFree),
            );
        } else if ai.tasks(world).roles.is_free(serial) {
            if is_fighter(&view, unit) {
                pool.soldiers += 1;
            } else {
                pool.others += 1;
            }
        }
    }
    pool
}

/// The forces' tasks before the missions are looked over: a raider, a marcher or a perimeter
/// soldier with no walk is given the square to attack its objective from again (the one chosen
/// before while it is open), and one of a save from before the objective was recorded apart is
/// given both. The helper walks to what the task says.
fn restage(ai: &mut Ai, game: &mut Game) {
    use crate::assembly::MarchError;
    use crate::roles::Role;
    let lost: Vec<_> = {
        let view = PlayerView::new(game, ai.player);
        let tasks = view.tasks();
        view.my_units()
            .into_iter()
            .filter_map(|u| {
                let serial = view.serial_of(u);
                let task = tasks.roles.get(serial)?;
                if !matches!(task.role, Role::Raid | Role::March) {
                    return None;
                }
                let objective = tasks.roles.objective_of(serial, task.role)?;
                let here = view.unit_position(u)?;
                let arrived =
                    Some(here) == task.to || view.distance(here, objective).is_some_and(|d| d <= 1);
                let walks = view.unit_has_order(u) || task.crossing.is_some();
                (task.objective.is_none() || !(walks || arrived))
                    .then_some((u, serial, task.role, objective))
            })
            .collect()
    };
    for (unit, serial, role, objective) in lost {
        let mut travel = Travel::new(game, ai.player, ai.crossing);
        let to = match crate::assembly::beside(ai, game, &mut travel, objective, &[unit]) {
            Ok(square) => Some(square),
            // No way: the task keeps the objective itself, which the helper finds no way to
            // either, and the unit is tagged with what it needs to get there.
            Err(MarchError::Unreachable) => Some(objective),
            // No square to attack from: the task is given up.
            Err(_) => None,
        };
        ai.write(game).stage(serial, role, to, objective);
    }
}

/// The escorts that fell behind their charges (the stack was split by a fight, or the charge was
/// ordered on by another job) go to where the charges stand now. Run after the jobs that move the
/// charges, so they follow to the new squares.
pub fn repair_escorts(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    check(ai, game, Check::Escorts, events);
}

/// Have the movement helper look the tasks over.
fn check(ai: &Ai, game: &mut Game, check: Check, events: &mut Vec<Event>) {
    let command = f4x_core::world::Command::CheckTasks {
        player: ai.player,
        check,
        door: ai.crossing,
    };
    // Nothing of it is a refused order: a walk the game will not take changes the task.
    let _ = ai.probe(game, command, events);
}

/// Before any new job is looked for: the helper gives up the missions that can no longer be
/// fulfilled (a rival cleared the lair first, the ground was explored by another) and sends on the
/// units that lost their walk; those it gives up await orders where they stand, so the jobs below
/// see them in the pool. Returns how many were reset.
pub fn revalidate(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) -> u32 {
    use crate::roles::Role;
    restage(ai, game);
    // The settlers' sites: one with a city on it, or that takes none, or that no way leads to
    // any more (worked out on the day of the site check only, `city_site.rs`).
    let mut unfit_sites = Vec::new();
    let mut ended = Vec::new();
    let held: Vec<_> = {
        let view = PlayerView::new(game, ai.player);
        let tasks = view.tasks();
        view.my_units()
            .into_iter()
            .filter_map(|u| {
                let serial = view.serial_of(u);
                let task = tasks.roles.get(serial)?;
                if let (Role::Settle, Some(site)) = (task.role, task.to) {
                    let unreached = ai.sites.rebuilt_on(game.round())
                        && crate::city_site::site_reach(&view, site).is_none();
                    if unreached {
                        unfit_sites.push(serial);
                    }
                    if view.city_at(site).is_some() {
                        ended.push(crate::settler_fate::SettlerFate::SiteTaken);
                    } else if unreached || !view.can_found_city(site) {
                        ended.push(crate::settler_fate::SettlerFate::SiteUnusable);
                    }
                }
                (!task.role.is_base()).then_some((serial, task.role))
            })
            .collect()
    };
    for fate in ended {
        ai.settler_fates.note(fate);
    }
    check(ai, game, Check::Missions { unfit_sites }, events);
    let count = {
        let tasks = ai.tasks(game);
        held.iter()
            .filter(|&&(s, role)| {
                !tasks.roles.is(s, role) && tasks.roles.is(s, Role::AwaitingOrders)
            })
            .count() as u32
    };
    ai.missions_reset += count;
    count
}

/// The fighters that are rested and stand somewhere, of the roles `role_ok` allows.
pub(crate) fn soldiers(
    ai: &Ai,
    world: &World,
    role_ok: impl Fn(&crate::roles::Registry, f4x_core::world::serial::Serial) -> bool,
) -> Vec<hecs::Entity> {
    let view = f4x_core::player::view::PlayerView::new(world, ai.player);
    let tasks = view.tasks();
    view.my_units()
        .into_iter()
        .filter(|&u| crate::view::is_fighter(&view, u) && view.unit_position(u).is_some())
        .filter(|&u| !crate::recover::needs_rest(&view, u))
        .filter(|&u| role_ok(&tasks.roles, view.serial_of(u)))
        .collect()
}

/// The soldiers of the pool that the danger job (job 0) may take: the free ones, those that
/// gather for the campaign and those that hold the perimeter already.
fn free_soldiers(ai: &Ai, world: &World) -> Vec<hecs::Entity> {
    use crate::roles::{MusterKind, Role};
    soldiers(ai, world, |roles, s| {
        roles.is_free(s) || roles.is(s, Role::Perimeter) || roles.in_muster(s, MusterKind::Campaign)
    })
}

/// One turn of the jobs, in the order of the checklist.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    // A spare ship a unit waits for stops scouting first, so it counts as free below.
    crate::scout::recall_spare_ships(ai, game, events);
    crate::roles::register(ai, game);
    // The sweep of the ship ties (and the sanity check of the passengers), before any job moves
    // one of them.
    crate::pickup::tidy_unit_registry(ai, game);
    crate::assembly::tidy(ai, game);
    // The attacks come first: no unit has used a move or got an order yet.
    crate::attack::run(ai, game, events);
    // First the units whose mission is over, so the jobs below see them waiting.
    revalidate(ai, game, events);
    crate::recover::run(ai, game, events);
    crate::garrison::thin_out(ai, game, events);
    // 0. a city in danger: the free soldiers deal with the enemy in its zone, before anything else
    {
        let free = free_soldiers(ai, game);
        crate::perimeter::run(ai, game, free, events);
    }
    // 0b. an own city that holds less than it wants: the nearest free soldiers go in
    crate::reinforce::run(ai, game, events);
    // 0b. an own city that holds less than it wants: the nearest free soldiers go in
    // 1. settlers, engineers, spirits
    crate::expand::settlers(ai, game, events);
    crate::road::run(ai, game, events);
    crate::meld::run(ai, game, events);
    // 2. escorts
    crate::escort::run(ai, game, events);
    repair_escorts(ai, game, events);
    // 3. scouts
    crate::scout::scout(ai, game, events);
    // 4. the war (task 646): the army takes the soldiers its target needs, none at peace
    crate::war::run(ai, game, events);
    // 5. raiding parties from the soldiers the war left over
    crate::raid::run(ai, game, events);
    // The ships (task 647): the bridges first, since a free ship goes to the bridge before it
    // scouts (task 739)
    crate::bridge::run(ai, game, events);
    crate::naval::run(ai, game, events);
    crate::pickup::run(ai, game, events);
    // What the ship jobs left free scouts.
    crate::scout::spare_ships(ai, game, events);
    // The orders a full square stopped go again, now that the units that could move have moved.
    ai.retry_blocked(game, events);
    // And last, for the units that arrived with moves left.
    crate::attack::run(ai, game, events);
    crate::roles::audit(ai, game, events);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
    use f4x_master::Game;

    /// A city keeps the soldiers its defence needs; the rest, and a lone soldier in the field,
    /// are the pool.
    #[test]
    fn the_pool_is_the_soldiers_beyond_the_garrison_and_the_idle_others() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        spawn(&mut w, P0, city, 104 /* high_men_spearmen */, 3);
        spawn(&mut w, P0, at(8, 6), 104, 1);
        spawn(&mut w, P0, at(9, 6), 111 /* high_men_settlers */, 1);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let wanted =
            crate::view::garrison_wanted(&f4x_core::player::view::PlayerView::new(&w, P0), city);
        let counted = pool(&ai, &w);
        assert_eq!(counted.soldiers, 3 - wanted + 1, "{counted:?}");
        assert_eq!(counted.others, 1);
        assert_eq!(counted.ships, 0);
    }

    /// A unit with nothing to do stays where it is: the turn sends it nowhere.
    #[test]
    fn a_waiting_soldier_with_no_job_is_left_where_it_stands() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        // The city holds the one soldier its defence wants: the lone one is not needed there.
        spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 1);
        let lone = spawn(&mut w, P0, at(8, 6), 104, 1)[0];
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        // The map is fully known, so there is nothing to scout; nobody is at war.
        assert!(!view.unit_has_order(lone), "{events:?}");
        assert_eq!(view.unit_position(lone), Some(at(8, 6)));
    }

    /// With the same soldiers and the same lair, a raid party forms at peace, and at war the
    /// campaign holds the soldiers and the lair is left alone. (This does not tell the order of
    /// the two jobs: the raid needs a few turns to form, the muster none; the order is in
    /// `run`.)
    #[test]
    fn at_war_the_soldiers_gather_for_the_campaign_and_leave_the_lair_alone(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::roles::{MusterKind, Role};
        let build = |war: bool| -> Result<(Game, Ai), Box<dyn std::error::Error>> {
            let mut w = game(COAST_CITY, 8 /* high_men */);
            if war {
                // A rival's town that P0 knows and is at war with.
                w.make_contact(P0, P1);
                let town = at(4, 4);
                w.command(f4x_core::world::Command::FoundCity {
                    owner: P1,
                    pos: town,
                    name: "Town".into(),
                    race: w.content().rules.races.id_numbered(8),
                    population: 8,
                });
                w.explore(P0, town, f4x_def::half_int::HalfInt::from_raw(3));
                w.declare_war(P0, P1);
            }
            spawn(
                &mut w,
                P0,
                at(COAST_CITY.0, COAST_CITY.1),
                113, /* paladins */
                8,
            );
            crate::testworld::spawn_site(
                &mut w,
                at(COAST_CITY.0 - 2, COAST_CITY.1),
                vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
                Vec::new(),
            )?;
            w.reveal_map(P0);
            Ok((w, Ai::new(P0)))
        };
        // Whether a raid party was formed in any of the turns, and whether the campaign held
        // soldiers in any (either may be over by the end).
        let turns = |w: &mut Game, ai: &mut Ai, turns_run: usize| {
            let (mut raided, mut campaigned) = (false, false);
            for _ in 0..turns_run {
                w.balance_farmers(P0);
                run(ai, w, &mut Vec::new());
                raided |= ai.tasks(w).roles.any(Role::Raid);
                campaigned |= ai.tasks(w).roles.any_muster(MusterKind::Campaign)
                    || ai.tasks(w).roles.any(Role::March);
                w.play_round();
            }
            (raided, campaigned)
        };
        let (mut w, mut ai) = build(false)?;
        assert!(turns(&mut w, &mut ai, 6).0, "at peace the soldiers raid");
        let (mut w, mut ai) = build(true)?;
        // While the campaign is on: the town falls after a few turns.
        let (raided, campaigned) = turns(&mut w, &mut ai, 3);
        assert!(!raided, "at war the lair is a distraction");
        assert!(campaigned, "at war the campaign holds them");
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        for s in ai.tasks(&w).roles.members(Role::Raid) {
            assert!(!ai.tasks(&w).roles.in_muster(s, MusterKind::Campaign));
            let _ = &view;
        }
        Ok(())
    }

    /// A scout whose ground was explored meanwhile is given up and awaits orders; one still on
    /// its way keeps its mission, and one with no walk to its target keeps it too, tagged with
    /// the ship it needs.
    #[test]
    fn a_unit_whose_mission_is_over_awaits_orders_and_one_that_cannot_walk_waits_for_a_ship() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let target = at(30, 10);
        let walker = spawn(&mut w, P0, at(9, 6), 104, 1)[0];
        let stopped = spawn(&mut w, P0, at(9, 7), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let (a, b) = (view.serial_of(walker), view.serial_of(stopped));
        assert!(!view.explored(target), "unexplored ground to scout");
        for s in [a, b] {
            let _ = ai.write(&mut w).claim(s, Role::Scout, Some(target));
        }
        let mut events = Vec::new();
        ai.send(
            &mut w,
            f4x_core::world::Command::GoTo {
                units: vec![walker],
                to: at(5, 9),
            },
            &mut events,
        )
        .expect("the walker can walk");
        // `walker` has an order and valid ground; `stopped` has no order and no walk leads to
        // its target (across the sea): it keeps its mission and waits for a ship.
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0);
        assert!(ai.tasks(&w).roles.is(a, Role::Scout));
        assert!(ai.tasks(&w).roles.is(b, Role::Scout));
        assert_eq!(
            ai.tasks(&w).roles.crossing_of(b),
            Some(crate::roles::Crossing::Sea)
        );
        assert_eq!(ai.tasks(&w).roles.crossing_of(a), None);
        // The ground becomes known: both missions are over, the flag goes with the role.
        w.reveal_map(P0);
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 2);
        assert!(!ai.tasks(&w).roles.is(a, Role::Scout));
        assert!(!ai.tasks(&w).roles.is(b, Role::Scout));
        assert_eq!(ai.tasks(&w).roles.crossing_of(b), None);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        assert!(!view.unit_has_order(walker));
        assert_eq!(ai.missions_reset, 2);
    }

    /// A unit that lost its walk (a strike moved it, a battle) but whose target is in the
    /// registry is sent on again, not given up.
    #[test]
    fn a_unit_that_lost_its_walk_is_sent_on_to_the_target_in_the_registry() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let target = at(5, 9);
        let scout = spawn(&mut w, P0, at(9, 7), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(scout);
        assert!(
            !f4x_core::player::view::PlayerView::new(&w, P0).explored(target),
            "unexplored ground to scout"
        );
        let _ = ai.write(&mut w).claim(serial, Role::Scout, Some(target));
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        assert!(!view.unit_has_order(scout), "no walk");
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0, "not given up");
        assert!(ai.tasks(&w).roles.is(serial, Role::Scout));
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        assert!(
            view.unit_has_order(scout) || view.unit_position(scout) != Some(at(9, 7)),
            "walking again: {events:?}"
        );
    }

    /// A sea scout that arrives at its target has explored it by arriving. It keeps the scouting
    /// (it is the one that puts its party ashore, `scout::scout`), where a walker on the same
    /// ground is done and gives the mission up.
    #[test]
    fn a_ship_that_arrived_at_its_scouting_target_keeps_the_role_and_puts_its_party_ashore() {
        use crate::roles::Role;
        use f4x_def::half_int::HalfInt;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // Off the east island's west shore, as in `scout`'s own landing test.
        let sea = at(27, 9);
        let boat = spawn(&mut w, P0, sea, 36 /* galley */, 1)[0];
        let walker = spawn(&mut w, P0, sea, 104 /* high_men_spearmen */, 1)[0];
        w.explore(P0, sea, HalfInt::from_raw(3));
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let (ship, rider) = (view.serial_of(boat), view.serial_of(walker));
        assert!(view.explored(sea), "arriving explored the target");
        let _ = ai.write(&mut w).claim(ship, Role::Scout, Some(sea));
        let _ = ai.write(&mut w).claim(rider, Role::SeaParty, None);
        ai.sea_scouting = true;
        assert_eq!(revalidate(&mut ai, &mut w, &mut Vec::new()), 0);
        assert!(
            ai.tasks(&w).roles.is(ship, Role::Scout),
            "the ship goes on scouting"
        );
        crate::scout::scout(&mut ai, &mut w, &mut Vec::new());
        assert!(
            !ai.tasks(&w).roles.is(rider, Role::SeaParty),
            "the rider went ashore"
        );
        assert_ne!(w.units().unit_position(walker), Some(sea));
    }

    /// A target on the other plane needs a crossing of the planes; the unit keeps its role and its
    /// target and is not sent again every turn.
    #[test]
    fn a_unit_whose_target_is_on_the_other_plane_is_tagged_with_a_plane_crossing() {
        use crate::roles::{Crossing, Role};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let mut target = at(5, 9);
        target.plane = f4x_core::map::Plane::Myrror;
        let scout = spawn(&mut w, P0, at(9, 7), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let serial = view.serial_of(scout);
        assert!(!view.explored(target));
        let _ = ai.write(&mut w).claim(serial, Role::Scout, Some(target));
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0);
        assert_eq!(
            ai.tasks(&w).roles.crossing_of(serial),
            Some(Crossing::Plane)
        );
        assert_eq!(
            ai.tasks(&w).roles.needing(Crossing::Plane),
            vec![(serial, target)]
        );
        // A new target clears the tag.
        ai.write(&mut w)
            .set_target(serial, Role::Scout, Some(at(5, 9)));
        assert_eq!(ai.tasks(&w).roles.crossing_of(serial), None);
    }

    /// A unit that travels between the planes by itself shifts and walks on: it is not tagged.
    #[test]
    fn a_unit_with_planar_travel_shifts_itself_instead_of_waiting_for_a_crossing() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let mut target = at(5, 9);
        target.plane = f4x_core::map::Plane::Myrror;
        let demon = spawn(&mut w, P0, at(9, 7), 171 /* shadow_demons */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(demon);
        let _ = ai.write(&mut w).claim(serial, Role::Scout, Some(target));
        let mut events = Vec::new();
        revalidate(&mut ai, &mut w, &mut events);
        assert_eq!(
            ai.tasks(&w).roles.crossing_of(serial),
            None,
            "no crossing to wait for"
        );
        assert_eq!(
            w.units().unit_position(demon).map(|p| p.plane),
            Some(f4x_core::map::Plane::Myrror),
            "it is across"
        );
    }

    /// A settler whose site no walk leads to waits for a ship with its site kept; the settler job
    /// does not choose another site for it.
    #[test]
    fn a_settler_with_no_walk_to_its_site_waits_for_a_ship_and_keeps_the_site() {
        use crate::roles::{Crossing, Role};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        // A site over the sea: a place for a city that no walk leads to.
        let site = crate::city_site::scan(&f4x_core::player::view::PlayerView::new(&w, P0))
            .into_iter()
            .find(|s| s.reach == crate::city_site::Reach::Sea)
            .expect("a site over the sea")
            .at;
        let settler = spawn(&mut w, P0, at(9, 7), 111 /* high_men_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(settler);
        let _ = ai.write(&mut w).claim(serial, Role::Settle, Some(site));
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0);
        assert_eq!(ai.tasks(&w).roles.crossing_of(serial), Some(Crossing::Sea));
        crate::expand::settlers(&mut ai, &mut w, &mut events);
        assert_eq!(
            ai.tasks(&w).roles.target_of(serial, Role::Settle),
            Some(site),
            "{events:?}"
        );
        assert_eq!(
            ai.tasks(&w).roles.needing(Crossing::Sea),
            vec![(serial, site)]
        );
    }

    /// A settler whose site now holds a city (a rival settled first) is given up, and the soak
    /// is told that is why.
    #[test]
    fn a_settler_whose_site_a_rival_settled_first_is_given_up_for_that_reason() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = at(ENEMY_CITY.0, ENEMY_CITY.1);
        let settler = spawn(&mut w, P0, at(9, 7), 111 /* high_men_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(settler);
        let _ = ai.write(&mut w).claim(serial, Role::Settle, Some(site));
        assert_eq!(revalidate(&mut ai, &mut w, &mut Vec::new()), 1);
        let said: Vec<_> = ai
            .settler_failures()
            .into_iter()
            .filter(|&(_, n)| n > 0)
            .collect();
        assert_eq!(said, [("site settled by someone else", 1)]);
    }

    /// A settler whose site can no longer take a city (it lies by a city of its own) loses the site
    /// and the role together and awaits orders.
    #[test]
    fn a_settler_whose_site_cannot_take_a_city_loses_the_role_with_the_site() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = at(COAST_CITY.0 + 1, COAST_CITY.1);
        let settler = spawn(&mut w, P0, at(9, 7), 111 /* high_men_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(settler);
        let _ = ai.write(&mut w).claim(serial, Role::Settle, Some(site));
        assert_eq!(revalidate(&mut ai, &mut w, &mut Vec::new()), 1);
        assert_eq!(
            ai.tasks(&w).roles.role_of(serial),
            Some(Role::AwaitingOrders)
        );
        assert!(ai.tasks(&w).roles.targets(Role::Settle).is_empty());
    }

    /// An engineer that is not building and has no walk goes on to its square, one that is
    /// building is left alone, and an escort apart from its charge walks to it.
    #[test]
    fn an_engineer_and_an_escort_that_lost_their_walk_are_sent_on() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = crate::city_site::scan(&f4x_core::player::view::PlayerView::new(&w, P0))
            .first()
            .expect("a site")
            .at;
        let engineer = spawn(&mut w, P0, at(9, 7), 110 /* high_men_engineers */, 1)[0];
        let charge = spawn(&mut w, P0, at(9, 6), 104, 1)[0];
        let escort = spawn(&mut w, P0, at(7, 6), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let (e, c, s) = (
            view.serial_of(engineer),
            view.serial_of(charge),
            view.serial_of(escort),
        );
        let _ = ai.write(&mut w).claim(e, Role::Road, Some(at(5, 9)));
        let _ = ai.write(&mut w).claim(c, Role::Settle, Some(site));
        let _ = ai.write(&mut w).claim(s, Role::Escort, None);
        ai.write(&mut w).group(&[s], Role::Escort, 1, Some(c));
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let moved = |u| view.unit_has_order(u) || view.unit_position(u) != Some(at(9, 7));
        assert!(moved(engineer), "the engineer walks again");
        assert_eq!(view.unit_position(escort), Some(at(7, 6)));
        repair_escorts(&mut ai, &mut w, &mut events);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        assert!(
            view.unit_has_order(escort) || view.unit_position(escort) != Some(at(7, 6)),
            "the escort follows its charge"
        );
        assert!(ai.tasks(&w).roles.is(s, Role::Escort));
    }

    /// A march of an old save has the target itself as its square: it is given the square to
    /// attack from beside it, and the target becomes the objective.
    #[test]
    fn a_march_of_an_old_save_is_given_a_square_beside_its_target() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let target = at(4, 4);
        let unit = spawn(&mut w, P0, at(8, 9), 113 /* high_men_paladins */, 1)[0];
        let mut ai = Ai::new(P0);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(unit);
        let _ = ai.write(&mut w).claim(serial, Role::March, Some(target));
        revalidate(&mut ai, &mut w, &mut Vec::new());
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let tasks = ai.tasks(&w);
        assert_eq!(tasks.roles.objective_of(serial, Role::March), Some(target));
        let from = tasks.roles.target_of(serial, Role::March);
        assert!(
            from.is_some_and(|p| view.distance(p, target) == Some(1)),
            "{from:?}"
        );
        assert!(view.unit_has_order(unit) || view.unit_position(unit) != Some(at(8, 9)));
    }
}
