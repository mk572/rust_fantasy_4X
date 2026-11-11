//! What the roaming monsters (`monster.rs`) and the sites (`site.rs`) share: marching a party to a
//! site, assaulting it, striking a stack next to it. (The independent cities have no AI of their
//! own: the neutral player of each race plays as any AI does, task 522.)

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_def::rules::Stat;

use crate::view::distance;
use crate::{Ai, Attack};

/// How strong the guards of a site are, as `unit_strength` measures units, when the player has
/// fought for the site once; `None` when it does not know them.
pub(crate) fn guard_strength(view: &PlayerView, site: Position) -> Option<f32> {
    let rules = &view.content().rules;
    let guards = view.lair_guards(site)?;
    Some(
        guards
            .iter()
            .map(|(id, n)| {
                let def = rules.units.get(*id);
                let stat = |s: Stat| def.stats.get(&s).map_or(0, |v| v.floor()).max(0) as f32;
                let attack = stat(Stat::Melee).max(stat(Stat::Ranged));
                let hits = stat(Stat::MaxHp) * stat(Stat::Figures).max(1.0);
                (attack * 1.5 + stat(Stat::Defense) + 1.0) * hits * *n as f32
            })
            .sum(),
    )
}

/// The eight squares round `at`.
pub(crate) fn around(view: &PlayerView, at: Position) -> Vec<Position> {
    let mut out = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            if (dx, dy) != (0, 0) {
                out.extend(view.shift(at, dx, dy));
            }
        }
    }
    out
}

/// Send a party to the free square next to `site` that it can reach and is nearest to it.
pub(crate) fn march(
    ai: &Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) -> bool {
    march_with(
        &mut crate::view::ReachCache::default(),
        ai,
        game,
        units,
        site,
        events,
    )
}

/// [`march`] with answers to "can it walk there" kept by the caller for as long as nothing moves,
/// for parties that go through the same sites one after the other.
pub(crate) fn march_with(
    reached: &mut crate::view::ReachCache,
    ai: &Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) -> bool {
    let spots = {
        let view = PlayerView::new(game, ai.player);
        let from = view.unit_position(units[0]).unwrap_or(site);
        let open: Vec<Position> = around(&view, site)
            .into_iter()
            .filter(|&p| view.hostile_at(p).is_none() && !view.has_lair(p) && p != from)
            .collect();
        // One search that stops at the first square found tells whether any of them can be
        // reached: when none can (a site across the water) the party does not search once for
        // each square, each of them over the whole plane.
        if !reached.can_reach_round(&view, units, site, &open) {
            return false;
        }
        let mut spots = open;
        spots.sort_by_key(|&p| (distance(&view, from, p), p.y, p.x));
        spots
    };
    // The nearest square that can be reached, asked one at a time: the first that can is the one
    // the party is sent to, so the others need no search.
    for spot in spots {
        if !PlayerView::new(game, ai.player).can_reach(units, spot) {
            continue;
        }
        let moved = ai.send(
            game,
            Command::GoTo {
                units: units.to_vec(),
                to: spot,
            },
            events,
        );
        if moved {
            return true;
        }
    }
    false
}

/// Assault the site with the units that still have moves, and fight the battle out.
pub(crate) fn assault(
    ai: &Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) {
    let fighters: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, ai.player);
        units
            .iter()
            .copied()
            .filter(|&u| view.unit_moves_left(u) > 0)
            .collect()
    };
    if fighters.is_empty() {
        return;
    }
    // The guards cannot be seen from outside: a site that has none left refuses, and the stack
    // then walks onto it (below).
    let sent = ai.probe(
        game,
        Command::EnterSite {
            units: fighters.clone(),
            to: site,
        },
        events,
    );
    if !sent {
        // A site nobody guards (the guards cannot be seen from outside) is not fought for: the
        // stack walks onto it and takes the treasure.
        ai.send(
            game,
            Command::GoTo {
                units: fighters,
                to: site,
            },
            events,
        );
        return;
    }
    // The battle AI plays both sides; a battle that stalls ends with the active side fleeing.
    for _ in 0..20_000 {
        let Some(command) = game.next_battle_command() else {
            break;
        };
        if !ai.send(game, Command::Battle(command), events) {
            break;
        }
    }
    while game.in_battle() {
        if !ai.send(
            game,
            Command::Battle(f4x_core::battle::BattleCommand::Flee),
            events,
        ) {
            break;
        }
    }
}

/// Fight the battle the AI means to fight: the human defends in person when `ai` leaves the
/// defence to them (the host plays it out), else the battle AI plays both sides.
pub(crate) fn strike(
    ai: &mut Ai,
    game: &mut Game,
    attackers: Vec<hecs::Entity>,
    defenders: Vec<hecs::Entity>,
    events: &mut Vec<Event>,
) {
    let in_person = defenders
        .first()
        .and_then(|&d| game.units().unit_owner(d))
        .is_some_and(|owner| ai.defended_by.contains(&owner));
    if in_person {
        ai.attacks.push(Attack {
            attackers,
            defenders,
        });
    } else {
        ai.send(
            game,
            Command::AutoResolve {
                mana_budget: None,
                attackers,
                defenders,
            },
            events,
        );
    }
}
