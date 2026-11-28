//! The raid job (task 646, job 5 of the dispatcher): the parties that clear the sites. It starts
//! from the sites the wizard knows (task 849): the closest first, each looked at once a turn, a
//! party only when the soldiers within range that have a way there are enough for it, with the
//! wizard's spells (`spellhelper::spell_power`). Soldiers standing apart gather first, on the way
//! of the one that arrives last (`assembly.rs`). A party out walks to the square beside its site
//! (the attack is `attack.rs`'s) while it is still enough for it, and else ends its raid. What a
//! site is worth and where the sites are is `site.rs`.

use crate::lever::Levered;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::world::serial::Serial;
use f4x_core::world::{Event, World};

use crate::assembly;
use crate::roles::{MusterKind, Role};
use crate::site::{expected_vs, lost_before, unarmed_share};
use crate::view::{distance, party, stack_strength, unit_strength};
use crate::Ai;
use f4x_core::unit::travel::Travel;
use f4x_master::Game;

pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // The parties out already, by the site each is set on: it keeps the site while it can get
    // there and is still strong enough, and else its soldiers are free again.
    let alive: Vec<Serial> = {
        let view = PlayerView::new(game, me);
        view.my_units().iter().map(|&u| view.serial_of(u)).collect()
    };
    ai.write(game)
        .retain_role(Role::Raid, |s| alive.contains(&s));
    let (mut parties, mut done) = {
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        let mut parties: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
        let mut done: Vec<Serial> = Vec::new();
        for u in view.my_units() {
            let serial = view.serial_of(u);
            if !tasks.roles.is(serial, Role::Raid) {
                continue;
            }
            let out = view
                .unit_position(u)
                .is_some_and(|p| view.city_at(p).is_none());
            match tasks.roles.objective_of(serial, Role::Raid) {
                Some(site) if out => match parties.iter_mut().find(|(t, _)| *t == site) {
                    Some((_, list)) => list.push(u),
                    None => parties.push((site, vec![u])),
                },
                // Back in a city, or set on nothing: the raid is over.
                _ => done.push(serial),
            }
        }
        (parties, done)
    };
    parties.sort_by_key(|(p, _)| (p.plane as u8, p.y, p.x));
    for (site, units) in parties {
        if game.game_over().is_some() {
            return;
        }
        let stays = {
            let view = PlayerView::new(game, me);
            crate::site::live(&view, site) && worth(ai, &view, &units, site)
        };
        if !(stays && set_off(ai, game, &units, site, events)) {
            let view = PlayerView::new(game, me);
            done.extend(units.iter().map(|&u| view.serial_of(u)));
        }
    }
    for &s in &done {
        ai.write(game).set_target(s, Role::Raid, None);
    }
    ai.write(game)
        .retain_role(Role::Raid, |s| !done.contains(&s));
    form(ai, game, events);
}

/// Does a party on `pos` stand beside the site?
fn beside(world: &World, me: PlayerId, pos: Position, site: Position) -> bool {
    distance(&PlayerView::new(world, me), pos, site) == Some(1)
}

/// Whether `units` are enough to go against the site: a party at least, strong enough with the
/// wizard's spells for the guards as seen or as they look, and much stronger than a party that
/// lost there.
fn worth(ai: &Ai, view: &PlayerView, units: &[hecs::Entity], site: Position) -> bool {
    if units.len() < view.levers().sites.site_party {
        return false;
    }
    let band = party(view, units);
    let strength = stack_strength(view, &band) + crate::spellhelper::spell_power(view);
    let needed = expected_vs(view, site, unarmed_share(view, &band));
    strength >= needed * view.levers().attack.attack_force
        && !lost_before(
            &ai.site_losses,
            site,
            strength,
            view.levers().sites.retry_edge,
        )
}

/// Send the party against the site; one that stands beside it and cannot move on stays there,
/// set on it. Whether the party is set on the site now.
fn set_off(
    ai: &mut Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) -> bool {
    let me = ai.player;
    match assembly::march(ai, game, Role::Raid, units, site, events) {
        Ok(()) => true,
        Err(why) => {
            let near: Vec<(hecs::Entity, Position)> = {
                let view = PlayerView::new(game, me);
                units
                    .iter()
                    .filter_map(|&u| Some((u, view.unit_position(u)?)))
                    .filter(|&(_, p)| beside(game, me, p, site))
                    .collect()
            };
            for &(u, p) in &near {
                // The party attacks only the site it is sent against (`attack.rs`).
                assembly::take(ai, game, Role::Raid, &[u], p, site);
            }
            if near.is_empty() && !matches!(why, assembly::MarchError::Unreachable) {
                let detail = matches!(why, assembly::MarchError::Refused(_))
                    .then(|| assembly::march_detail(ai, game, units, site));
                ai.note_failure(
                    game,
                    format!("march to {site:?}"),
                    crate::with_detail(format!("{why:?}"), detail),
                );
            }
            !near.is_empty()
        }
    }
}

/// The strongest first.
fn by_strength(view: &PlayerView, units: &mut [hecs::Entity]) {
    units.sort_by(|&a, &b| {
        unit_strength(view, b)
            .total_cmp(&unit_strength(view, a))
            .then(view.serial_of(a).cmp(&view.serial_of(b)))
    });
}

/// New raiding parties, from the site's side: the sites the wizard knows, those closest to a free
/// soldier first, each looked at once. A site gets the strongest free soldiers within range that
/// have a way to it, if they are enough (`worth`): those standing together march at once, else
/// they gather first (`muster`). The war job has taken its soldiers before this runs, and the
/// garrisons are not in the pool, so nothing caps the parties. Returns whether any party left or
/// began to gather.
pub fn form(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) -> bool {
    let me = ai.player;
    release(ai, game, events);
    let mut free = crate::dispatch::soldiers(ai, game, |roles, s| roles.is_free(s));
    let sites: Vec<Position> = {
        let view = PlayerView::new(game, me);
        let range = view.levers().sites.range;
        let taken: Vec<Position> = view
            .tasks()
            .roles
            .objectives(Role::Raid)
            .into_iter()
            .chain(
                view.tasks()
                    .roles
                    .musters()
                    .into_iter()
                    .filter(|(_, m, _)| m.kind == MusterKind::Raid)
                    .filter_map(|(_, m, _)| m.objective),
            )
            .collect();
        let mut sites: Vec<(u32, Position)> = crate::site::known_sites(&view, &ai.tower_blocked)
            .into_iter()
            .filter(|site| !taken.contains(site))
            .filter_map(|site| {
                let nearest = free
                    .iter()
                    .filter_map(|&u| distance(&view, view.unit_position(u)?, site))
                    .min()?;
                (nearest <= range).then_some((nearest, site))
            })
            .collect();
        sites.sort_by_key(|&(d, p)| (d, p.plane as u8, p.y, p.x));
        sites.into_iter().map(|(_, p)| p).collect()
    };
    let mut travel = Travel::new(game, me, ai.crossing);
    let mut left = false;
    for site in sites {
        if game.game_over().is_some() || free.len() < game.levers().sites.site_party {
            break;
        }
        // The soldiers in range, if they would do should all of them have a way: asked before
        // any route is searched.
        let in_range: Vec<hecs::Entity> = {
            let view = PlayerView::new(game, me);
            let range = view.levers().sites.range;
            let mut units: Vec<hecs::Entity> = free
                .iter()
                .copied()
                .filter(|&u| {
                    view.unit_position(u)
                        .and_then(|p| distance(&view, p, site))
                        .is_some_and(|d| d <= range)
                })
                .collect();
            by_strength(&view, &mut units);
            if !worth(ai, &view, &units, site) {
                continue;
            }
            units
        };
        let Ok(square) = assembly::beside(ai, game, &mut travel, site, &in_range) else {
            continue;
        };
        // Those with a way, and the parties of them that stand together, nearest square first.
        let (able, together) = {
            let view = PlayerView::new(game, me);
            let able: Vec<hecs::Entity> = in_range
                .iter()
                .copied()
                .filter(|&u| {
                    view.unit_position(u) == Some(square)
                        || travel.trip(&view, &[u], square).is_some()
                })
                .collect();
            if !worth(ai, &view, &able, site) {
                continue;
            }
            let mut together = crate::military::stacks(&view, able.clone());
            together.retain(|(_, units)| worth(ai, &view, units, site));
            together.sort_by_key(|&(p, _)| (distance(&view, p, site), p.y, p.x));
            (able, together)
        };
        let gone = together.into_iter().find_map(|(_, mut units)| {
            let view = PlayerView::new(game, me);
            by_strength(&view, &mut units);
            units.truncate(view.max_battle_side());
            set_off(ai, game, &units, site, events).then_some(units)
        });
        let gone = match gone {
            Some(units) => units,
            None => match muster(ai, game, &mut travel, &able, site, square, events) {
                Some(units) => units,
                None => continue,
            },
        };
        free.retain(|u| !gone.contains(u));
        left = true;
    }
    left
}

/// End a muster whose soldiers have come (or waited long enough): those on its square set off
/// against its site if they are enough, and else are free again, as are those still on the way.
/// A soldier of a muster that stands is sent on when it has no order.
pub(crate) fn release(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    if !ai.tasks(game).roles.any_muster(MusterKind::Raid) {
        return;
    }
    let me = ai.player;
    let musters = ai.tasks(game).roles.musters();
    for (party, muster, members) in musters {
        if muster.kind != MusterKind::Raid {
            continue;
        }
        let (over, there, idle) = {
            let view = PlayerView::new(game, me);
            let alive: Vec<_> = view
                .my_units()
                .into_iter()
                .filter(|&u| members.contains(&view.serial_of(u)))
                .collect();
            let mut there: Vec<_> = alive
                .iter()
                .copied()
                .filter(|&u| view.unit_position(u) == Some(muster.rally))
                .collect();
            let over = there.len() == alive.len()
                || game.round().saturating_sub(muster.since) > view.levers().sites.muster_turns;
            by_strength(&view, &mut there);
            there.truncate(view.max_battle_side());
            let idle: Vec<hecs::Entity> = alive
                .into_iter()
                .filter(|&u| !view.unit_has_order(u) && view.unit_position(u) != Some(muster.rally))
                .collect();
            (over, there, idle)
        };
        if over {
            ai.write(game).end_muster(party);
            let Some(site) = muster.objective else {
                continue;
            };
            let enough = {
                let view = PlayerView::new(game, me);
                crate::site::live(&view, site) && worth(ai, &view, &there, site)
            };
            if enough {
                set_off(ai, game, &there, site, events);
            }
            continue;
        }
        for u in idle {
            // One that cannot go on now is tried again next turn, until the muster's time is up.
            let _ = assembly::send(ai, game, &[u], muster.rally, events);
        }
    }
}

/// The strongest of `able` (they have a way to `square`, beside the site) gather on the way of
/// the one that arrives last, or on that square: one such muster at a time, and none while the
/// empire is being built. Returns the soldiers that set out, if they are enough for the site.
fn muster(
    ai: &mut Ai,
    game: &mut Game,
    travel: &mut Travel,
    able: &[hecs::Entity],
    site: Position,
    square: Position,
    events: &mut Vec<f4x_core::world::Event>,
) -> Option<Vec<hecs::Entity>> {
    let me = ai.player;
    if ai.tasks(game).roles.any_muster(MusterKind::Raid) {
        return None;
    }
    let members: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        if crate::phase::phase(&view, game.round()) == crate::phase::Phase::Build {
            return None;
        }
        let mut members: Vec<_> = able
            .iter()
            .copied()
            .filter(|&u| !view.unit_has_order(u))
            .collect();
        members.truncate(view.max_battle_side());
        if !worth(ai, &view, &members, site) {
            return None;
        }
        members
    };
    let rally = assembly::rally(ai, game, travel, site, square, &members).unwrap_or(square);
    let serials: Vec<Serial> = {
        let view = PlayerView::new(game, me);
        members.iter().map(|&u| view.serial_of(u)).collect()
    };
    ai.write(game)
        .begin_muster(MusterKind::Raid, rally, Some(site), &serials)?;
    // One that cannot set out is no part of the muster.
    let mut stayed: Vec<Serial> = Vec::new();
    let mut gone: Vec<hecs::Entity> = Vec::new();
    for (u, serial) in members.into_iter().zip(serials) {
        let at = PlayerView::new(game, me).unit_position(u);
        if !ai.tasks(game).roles.is(serial, Role::Muster) {
            continue;
        }
        if at != Some(rally) && assembly::send(ai, game, &[u], rally, events).is_err() {
            stayed.push(serial);
        } else {
            gone.push(u);
        }
    }
    let enough = worth(ai, &PlayerView::new(game, me), &gone, site);
    if !enough {
        stayed.extend({
            let view = PlayerView::new(game, me);
            gone.iter().map(|&u| view.serial_of(u)).collect::<Vec<_>>()
        });
    }
    ai.write(game).release_many(&stayed, Role::Muster);
    ai.write(game).tidy_musters();
    enough.then_some(gone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{
        at, game, spawn, spawn_site, spawn_site_looking, spawn_tower, COAST_CITY, P0,
    };
    use crate::view::is_fighter;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The west wizard with `paladins` in its city at (10, 6) and an encounter site of the
    /// given guards two squares west. `known`: the wizard has fought for the site before.
    fn world(
        paladins: usize,
        guards: Vec<(f4x_def::rules::UnitId, u32)>,
        known: bool,
    ) -> Result<Game, Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.treasury_mut(P0).ok_or("player 0 has a treasury")?.gold =
            f4x_def::half_int::HalfInt::from_int(1_000_000);
        spawn(
            &mut w,
            P0,
            at(10, 6),
            113, /* high_men_paladins */
            paladins,
        );
        spawn_site(
            &mut w,
            at(8, 6),
            guards,
            if known { vec![P0] } else { Vec::new() },
        )?;
        Ok(w)
    }

    /// Whether every soldier of the wizard stands in its city.
    fn all_home(w: &World) -> bool {
        w.units()
            .player_units(P0)
            .iter()
            .all(|&u| w.units().unit_position(u) == Some(at(10, 6)))
    }

    fn turns(w: &mut Game, ai: &mut Ai, n: usize) -> Vec<Event> {
        let mut events = Vec::new();
        w.balance_farmers(P0);
        for _ in 0..n {
            // As the dispatcher does it: the attacks before the jobs and after them.
            crate::attack::run(ai, w, &mut events);
            run(ai, w, &mut events);
            crate::attack::run(ai, w, &mut events);
            w.play_round();
            w.balance_farmers(P0);
        }
        events
    }

    #[test]
    fn a_strong_party_next_to_an_encounter_site_clears_it() -> TestResult {
        let mut w = world(
            5,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        let events = turns(&mut w, &mut ai, 10);
        let gold: u32 = events
            .iter()
            .filter_map(|e| match e {
                Event::SiteCleared { player, loot, .. } if *player == P0 => Some(loot.gold),
                _ => None,
            })
            .sum();
        assert_eq!(gold, 300, "the hoard is the wizard's");
        assert!(w.economy().lair_at(at(8, 6)).is_none());
        Ok(())
    }

    #[test]
    fn a_party_keeps_the_site_it_is_set_on_while_another_becomes_nearer() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let guards = || vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)];
        spawn_site(&mut w, at(4, 4), guards(), Vec::new())?;
        spawn_site(&mut w, at(4, 9), guards(), Vec::new())?;
        w.reveal_map(P0);
        let party = spawn(&mut w, P0, at(6, 7), 113 /* high_men_paladins */, 5);
        let mut ai = Ai::new(P0);
        let members: Vec<Serial> = {
            let view = PlayerView::new(&w, P0);
            party.iter().map(|&u| view.serial_of(u)).collect()
        };
        ai.write(&mut w).assign(crate::roles::Role::Raid, members);
        // The party is set on the far site (4, 4) though (4, 9) is nearer to where it stands.
        let raiders = ai.tasks(&w).roles.members(crate::roles::Role::Raid);
        for s in raiders {
            ai.write(&mut w)
                .set_target(s, crate::roles::Role::Raid, Some(at(4, 4)));
        }
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let here = w
            .units()
            .unit_position(party[0])
            .ok_or("the party is on the map")?;
        assert!(here.y < 7, "it went towards (4, 4), not (4, 9): {here:?}");
        assert!(ai
            .tasks(&w)
            .roles
            .objectives(crate::roles::Role::Raid)
            .iter()
            .all(|&t| t == at(4, 4)));
        Ok(())
    }

    #[test]
    fn a_party_whose_only_site_cannot_be_reached_goes_home_and_the_raid_ends() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // A tower on the east island: in range, but over the sea from the party.
        spawn_tower(&mut w, at(30, 9), true)?;
        w.reveal_map(P0);
        let party = spawn(&mut w, P0, at(5, 8), 113 /* high_men_paladins */, 5);
        let mut ai = Ai::new(P0);
        let members: Vec<Serial> = {
            let view = PlayerView::new(&w, P0);
            party.iter().map(|&u| view.serial_of(u)).collect()
        };
        ai.write(&mut w).assign(crate::roles::Role::Raid, members);
        let mut events = Vec::new();
        for _ in 0..4 {
            run(&mut ai, &mut w, &mut events);
            w.play_round();
        }
        assert!(
            ai.tasks(&w)
                .roles
                .targets(crate::roles::Role::Raid)
                .is_empty(),
            "no target it can walk to"
        );
        assert!(
            !ai.tasks(&w).roles.any(crate::roles::Role::Raid),
            "the raid ended at the city: {:?}",
            ai.tasks(&w).roles
        );
        Ok(())
    }

    #[test]
    fn a_party_with_no_site_left_ends_the_raid_where_it_stands_on_either_plane() -> TestResult {
        use f4x_core::map::Plane;
        for plane in [Plane::Arcanus, Plane::Myrror] {
            let mut w = game(COAST_CITY, 8 /* high_men */);
            w.reveal_map(P0);
            let spot = Position { plane, ..at(5, 8) };
            let party = spawn(&mut w, P0, spot, 113 /* high_men_paladins */, 4);
            let mut ai = Ai::new(P0);
            ai.record_refusals(true);
            let members: Vec<Serial> = {
                let view = PlayerView::new(&w, P0);
                party.iter().map(|&u| view.serial_of(u)).collect()
            };
            ai.write(&mut w).assign(crate::roles::Role::Raid, members);
            let mut events = Vec::new();
            run(&mut ai, &mut w, &mut events);
            assert!(
                !ai.tasks(&w).roles.any(crate::roles::Role::Raid),
                "{plane:?}"
            );
            assert!(ai.take_refusals().is_empty(), "{plane:?}: no refused order");
            assert!(
                party
                    .iter()
                    .all(|&u| w.units().unit_position(u) == Some(spot)),
                "{plane:?}: nobody is sent anywhere"
            );
        }
        Ok(())
    }

    #[test]
    fn a_march_that_cannot_be_ordered_says_why() -> TestResult {
        use crate::assembly::{march, MarchError};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut w, at(30, 9), true)?;
        w.reveal_map(P0);
        let party = spawn(&mut w, P0, at(5, 8), 113 /* high_men_paladins */, 3);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        assert_eq!(
            march(
                &mut ai,
                &mut w,
                crate::roles::Role::Raid,
                &party,
                at(30, 9),
                &mut events
            ),
            Err(MarchError::Unreachable),
            "over the sea"
        );
        assert_eq!(
            march(
                &mut ai,
                &mut w,
                crate::roles::Role::Raid,
                &[],
                at(30, 9),
                &mut events
            ),
            Err(MarchError::NoUnits)
        );
        // A site on the party's own island can be marched on.
        let site = at(3, 3);
        spawn_site(&mut w, site, Vec::new(), Vec::new())?;
        assert_eq!(
            march(
                &mut ai,
                &mut w,
                crate::roles::Role::Raid,
                &party,
                site,
                &mut events
            ),
            Ok(())
        );
        Ok(())
    }

    #[test]
    fn a_weak_party_leaves_a_site_whose_guards_it_knows_to_be_too_strong() -> TestResult {
        // The guards are as many great drakes as it takes for them to be worth three times the
        // party in the measure of the rules (`battle::power`): the test is about the decision to
        // leave a site that is too strong by that measure, not about which unit is stronger.
        const PARTY: usize = 4;
        let drake = f4x_def::rules::UnitId::new(164 /* great_drake */);
        let content = game(COAST_CITY, 8 /* high_men */);
        let rules = &content.content().rules;
        let one_paladin = f4x_battle::power::def_power(
            rules.units.get(f4x_def::rules::UnitId::new(
                113, /* high_men_paladins */
            )),
            rules,
        );
        let one_drake = f4x_battle::power::def_power(rules.units.get(drake), rules);
        let guards = (3.0 * PARTY as f32 * one_paladin / one_drake)
            .ceil()
            .max(1.0) as u32;
        let mut w = world(PARTY, vec![(drake, guards)], true)?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 4);
        assert!(!ai.tasks(&w).roles.any(crate::roles::Role::Raid));
        assert!(w.economy().lair_at(at(8, 6)).is_some());
        assert!(all_home(&w));
        Ok(())
    }

    #[test]
    fn a_site_a_party_lost_on_is_left_alone_until_a_party_is_much_stronger() -> TestResult {
        // The guards are weak by the measure (one spearman against a party of paladins), so a
        // party would go: it stays home only because a party as strong as this one lost its
        // assault on the site before.
        let guards = vec![(f4x_def::rules::UnitId::new(104 /* high_men_spearmen */), 1)];
        let mut w = world(4, guards.clone(), true)?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 1);
        assert!(
            ai.tasks(&w).roles.any(crate::roles::Role::Raid),
            "the party goes when nothing is held against the site"
        );
        let mut w = world(4, guards.clone(), true)?;
        let mut ai = Ai::new(P0);
        ai.site_losses.push((at(8, 6), f32::MAX / 2.0));
        turns(&mut w, &mut ai, 4);
        assert!(!ai.tasks(&w).roles.any(crate::roles::Role::Raid));
        assert!(w.economy().lair_at(at(8, 6)).is_some());
        assert!(all_home(&w));
        Ok(())
    }

    #[test]
    fn a_party_with_normal_weapons_leaves_guards_that_weapons_barely_hurt_unless_a_spell_arms_it(
    ) -> TestResult {
        use crate::site::{expected_vs, unarmed_share};
        let rules = &game(COAST_CITY, 8).content().rules.clone();
        let ability = rules
            .abilities
            .id_numbered(72 /* weapon_immunity */)
            .ok_or("no weapon immunity")?;
        let immune = rules
            .units
            .defs()
            .iter()
            .find(|d| d.abilities.contains(&ability))
            .map(|d| f4x_def::rules::UnitId::new(d.num))
            .ok_or("no unit has weapon immunity")?;
        // The most guards the party would take if its weapons hurt them, and no more.
        let mut chosen = None;
        for k in 1..60u32 {
            let w = world(4, vec![(immune, k)], true)?;
            let view = PlayerView::new(&w, P0);
            let party = w.units().player_units(P0);
            let strength = stack_strength(&view, &party);
            let armed =
                expected_vs(&view, at(8, 6), 0.0) * w.levers().attack.attack_force <= strength;
            let unarmed = unarmed_share(&view, &party);
            let blocked =
                expected_vs(&view, at(8, 6), unarmed) * w.levers().attack.attack_force > strength;
            if armed && blocked {
                chosen = Some(k);
            }
        }
        let k = chosen.ok_or("no number of guards is beatable only with magical weapons")?;
        let mut w = world(4, vec![(immune, k)], true)?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 4);
        assert!(
            !ai.tasks(&w).roles.any(crate::roles::Role::Raid),
            "paladins with normal weapons go against guards they cannot hurt"
        );
        // With Flame Blade known and the casting skill and mana for four casts, the same party goes.
        let mut w = world(4, vec![(immune, k)], true)?;
        let blade = w
            .content()
            .rules
            .spells
            .id_numbered(94)
            .ok_or("no flame blade")?;
        w.cheat_learn_spell(P0, blade);
        w.wizard_mut(P0)
            .ok_or("no wizard")?
            .get_mut::<f4x_core::wizard::sheet::Skill>()
            .points = 1000;
        w.treasury_mut(P0).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(400);
        let mut ai = Ai::new(P0);
        let said = turns(&mut w, &mut ai, 1);
        // Whoever falls there takes the task along: that the party went shows in the battle.
        assert!(
            said.iter().any(|e| matches!(
                e,
                Event::BattleStarted { attacker, site, .. }
                    if *attacker == P0 && *site == at(8, 6)
            )),
            "the party goes when a spell can arm it: {said:?}"
        );
        Ok(())
    }

    #[test]
    fn a_city_keeps_its_garrison_and_waits_for_a_party() -> TestResult {
        // One spare fighter beyond the single guard is not a party.
        let mut w = world(
            2,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 4);
        assert!(!ai.tasks(&w).roles.any(crate::roles::Role::Raid));
        assert!(w.economy().lair_at(at(8, 6)).is_some());
        Ok(())
    }

    /// Soldiers who muster for the campaign hold that role; a raid takes only free soldiers, so the
    /// same soldiers go on a raid when they are free and do not when they muster.
    #[test]
    fn a_raid_does_not_take_the_soldiers_of_a_campaign_muster() -> TestResult {
        let guards = vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)];
        let city = at(10, 6);
        let serials = |w: &World| {
            let view = PlayerView::new(w, P0);
            view.garrison(city)
                .into_iter()
                .filter(|&u| is_fighter(&view, u))
                .map(|u| view.serial_of(u))
                .collect::<Vec<_>>()
        };
        // Control: free, they raid.
        let mut w = world(6, guards.clone(), false)?;
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(
            ai.tasks(&w).roles.any(crate::roles::Role::Raid),
            "free soldiers raid"
        );
        // Mustering for the campaign, they stay out of it.
        let mut w = world(6, guards, false)?;
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let members = serials(&w);
        assert!(ai
            .write(&mut w)
            .begin_muster(
                crate::roles::MusterKind::Campaign,
                city,
                Some(at(4, 4)),
                &members,
            )
            .is_some());
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(
            !ai.tasks(&w).roles.any(crate::roles::Role::Raid),
            "{:?}",
            ai.tasks(&w).roles
        );
        Ok(())
    }

    #[test]
    fn the_survivors_are_free_soldiers_again_where_the_raid_ended() -> TestResult {
        let mut w = world(
            6,
            vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            false,
        )?;
        let mut ai = Ai::new(P0);
        turns(&mut w, &mut ai, 14);
        assert!(
            w.economy().lair_at(at(8, 6)).is_none(),
            "the site is cleared"
        );
        assert!(
            !ai.tasks(&w).roles.any(crate::roles::Role::Raid),
            "{:?}",
            ai.tasks(&w).roles
        );
        // No walk home is ordered: the soldiers stand free for the jobs that need them.
        let view = PlayerView::new(&w, P0);
        assert!(w
            .units()
            .player_units(P0)
            .into_iter()
            .all(|u| ai.tasks(&w).roles.is_free(view.serial_of(u))));
        Ok(())
    }

    #[test]
    fn an_army_in_the_field_with_nothing_to_march_on_raids_the_nearest_tower() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(11, 6), 113 /* high_men_paladins */, 6);
        spawn_tower(&mut w, at(8, 6), true)?;
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        assert!(form(&mut ai, &mut w, &mut events), "{events:?}");
        assert_eq!(ai.tasks(&w).roles.count(crate::roles::Role::Raid), 6);
        Ok(())
    }

    /// A game with the paladins on the squares given, each `(x, y, how many)`, a site of zombies
    /// on each of `sites` that looks `looks` paladins strong, and the whole map known.
    fn field(
        paladins: &[(u32, u32, usize)],
        sites: &[(Position, f32)],
    ) -> Result<(Game, Vec<hecs::Entity>), Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.set_round(200); // past the build-up, when parties may gather
        w.treasury_mut(P0).ok_or("player 0 has a treasury")?.gold =
            f4x_def::half_int::HalfInt::from_int(1_000_000);
        let mut units = Vec::new();
        for &(x, y, n) in paladins {
            units.extend(spawn(
                &mut w,
                P0,
                at(x, y),
                113, /* high_men_paladins */
                n,
            ));
        }
        let one = unit_strength(&PlayerView::new(&w, P0), units[0]);
        for &(pos, looks) in sites {
            spawn_site_looking(
                &mut w,
                pos,
                vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
                Vec::new(),
                Some((looks * one) as u32),
            )?;
        }
        w.reveal_map(P0);
        w.balance_farmers(P0);
        Ok((w, units))
    }

    #[test]
    fn the_closest_site_is_taken_first_though_another_is_weaker() -> TestResult {
        let (near, far) = (at(4, 6), at(2, 3));
        let (mut w, _) = field(&[(7, 6, 5)], &[(near, 2.0), (far, 0.5)])?;
        let mut ai = Ai::new(P0);
        assert!(form(&mut ai, &mut w, &mut Vec::new()));
        let bound = ai.tasks(&w).roles.objectives(Role::Raid);
        assert!(
            !bound.is_empty() && bound.iter().all(|&t| t == near),
            "{bound:?}"
        );
        Ok(())
    }

    #[test]
    fn soldiers_standing_apart_gather_for_a_site_that_needs_them_all() -> TestResult {
        let site = at(3, 6);
        // Two and two paladins, neither pair enough for guards that look three strong.
        let (mut w, units) = field(&[(8, 4, 2), (8, 9, 2)], &[(site, 3.0)])?;
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        assert!(form(&mut ai, &mut w, &mut events), "{events:?}");
        let musters = ai.tasks(&w).roles.musters();
        assert_eq!(musters.len(), 1, "{:?}", ai.tasks(&w).roles);
        assert_eq!(musters[0].1.objective, Some(site));
        assert_eq!(musters[0].2.len(), units.len());
        assert!(!ai.tasks(&w).roles.any(Role::Raid));
        // They come together and set off.
        let mut set_off = false;
        for _ in 0..12 {
            w.play_round();
            w.balance_farmers(P0);
            run(&mut ai, &mut w, &mut events);
            set_off |= ai.tasks(&w).roles.count(Role::Raid) >= 3;
            if set_off {
                break;
            }
        }
        assert!(set_off, "{:?}", ai.tasks(&w).roles);
        let bound = ai.tasks(&w).roles.objectives(Role::Raid);
        assert!(bound.iter().all(|&t| t == site), "{bound:?}");
        Ok(())
    }

    #[test]
    fn no_party_forms_that_would_not_be_enough_for_the_site() -> TestResult {
        let (mut w, units) = field(&[(8, 4, 2), (8, 9, 2)], &[(at(3, 6), 40.0)])?;
        let mut ai = Ai::new(P0);
        assert!(!form(&mut ai, &mut w, &mut Vec::new()));
        assert!(ai.tasks(&w).roles.musters().is_empty());
        let view = PlayerView::new(&w, P0);
        assert!(units
            .iter()
            .all(|&u| ai.tasks(&w).roles.is_free(view.serial_of(u))));
        Ok(())
    }

    #[test]
    fn a_muster_that_ran_out_of_time_with_too_few_does_not_set_off() -> TestResult {
        let site = at(3, 6);
        let (mut w, units) = field(&[(8, 4, 2), (8, 9, 2)], &[(site, 3.0)])?;
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let two: Vec<Serial> = {
            let view = PlayerView::new(&w, P0);
            units[..2].iter().map(|&u| view.serial_of(u)).collect()
        };
        ai.write(&mut w)
            .begin_muster(MusterKind::Raid, at(8, 4), Some(site), &two)
            .ok_or("no muster")?;
        release(&mut ai, &mut w, &mut Vec::new());
        assert!(
            ai.tasks(&w).roles.musters().is_empty(),
            "all of it is there"
        );
        assert!(!ai.tasks(&w).roles.any(Role::Raid), "two are not enough");
        Ok(())
    }
}
