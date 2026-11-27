//! What the attacks on sites (`attack.rs`) and the site job (`site.rs`) share: the strength of a
//! site's guards, and the assault fought out. (The independent cities have no AI of their own: the
//! neutral player of each race plays as any AI does, task 522.)

use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::{Command, Event};

use crate::Ai;
use f4x_master::Game;

/// How strong the guards of a site are, as `unit_strength` measures units, when the player has
/// fought for the site once; `None` when it does not know them.
pub(crate) fn guard_strength(view: &PlayerView, site: Position) -> Option<f32> {
    guard_parts(view, site).map(|(all, _)| all)
}

/// The guards of a site as [`guard_strength`] measures them, and of that the part with weapon
/// immunity (`weapon_immunity`, 72): normal weapons barely hurt those.
pub(crate) fn guard_parts(view: &PlayerView, site: Position) -> Option<(f32, f32)> {
    let rules = &view.content().rules;
    let guards = view.lair_guards(site)?;
    let (mut all, mut immune) = (0.0, 0.0);
    for (id, n) in &guards {
        let def = rules.units.get(*id);
        let worth = f4x_battle::power::def_power(def, rules) * *n as f32;
        all += worth;
        if def.abilities.contains(&f4x_core::rules::Ability::new(
            72, /* weapon_immunity */
        )) {
            immune += worth;
        }
    }
    Some((all, immune))
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
    let sent = ai
        .probe(
            game,
            Command::EnterSite {
                units: fighters.clone(),
                to: site,
            },
            events,
        )
        .is_ok();
    if !sent {
        // A site nobody guards (the guards cannot be seen from outside) is not fought for: the
        // stack walks onto it and takes the treasure.
        ai.order(
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
        if !ai.send(game, Command::Battle(command), events).is_ok() {
            break;
        }
    }
    while game.in_battle() {
        if !ai
            .send(
                game,
                Command::Battle(f4x_battle::battle::BattleCommand::Flee),
                events,
            )
            .is_ok()
        {
            break;
        }
    }
}
