//! Turn a [`BattleScenario`] into a game with its battle begun.

use f4x_core::content::{Content, TerrainClass};
use f4x_core::game::setup::{new_game, NewGame};
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;

use f4x_core::game::{Command, Event, Game};

use crate::scenario::{BattleScenario, Ground, UnitPick, WizardSpec};

/// The attacking wizard and the defending one.
pub const ATTACKER: PlayerId = PlayerId(0);
pub const DEFENDER: PlayerId = PlayerId(1);

fn walkable(game: &Game, at: Position) -> bool {
    game.economy().city_at(at).is_none()
        && !matches!(
            game.economy().terrain_class_at(at),
            None | Some(TerrainClass::Ocean) | Some(TerrainClass::Mountains)
        )
}

fn around(at: Position) -> impl Iterator<Item = Position> {
    const D: [(i32, i32); 8] = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
    ];
    D.into_iter().filter_map(move |(dx, dy)| {
        let x = at.x as i32 + dx;
        let y = at.y as i32 + dy;
        (x >= 0 && y >= 0).then_some(Position {
            plane: at.plane,
            x: x as u32,
            y: y as u32,
        })
    })
}

/// Two neighbouring squares of open land, found by looking round `from`.
fn field_pair(game: &Game, from: Position, start: usize) -> Option<(Position, Position)> {
    let mut rings = vec![from];
    for r in 1..8u32 {
        for dy in -(r as i32)..=r as i32 {
            for dx in -(r as i32)..=r as i32 {
                if dx.unsigned_abs().max(dy.unsigned_abs()) == r {
                    let (x, y) = (from.x as i32 + dx, from.y as i32 + dy);
                    if x >= 0 && y >= 0 {
                        rings.push(Position {
                            plane: from.plane,
                            x: x as u32,
                            y: y as u32,
                        });
                    }
                }
            }
        }
    }
    let n = rings.len();
    (0..n).find_map(|k| {
        let a = rings[(k + start) % n];
        if !walkable(game, a) {
            return None;
        }
        around(a)
            .filter(|&b| walkable(game, b))
            .find(|b| b.x == a.x + 1 || b.x + 1 == a.x)
            .map(|b| (a, b))
    })
}

fn set_wizard(game: &mut Game, player: PlayerId, spec: &Option<WizardSpec>) {
    let ids: Vec<_> = spec
        .iter()
        .flat_map(|w| w.spells.iter())
        .filter_map(|s| game.content().rules.spells.id_of(s))
        .collect();
    let Some((p, t)) = game.player_treasury_mut(player) else {
        return;
    };
    p.magic.known.clear();
    match spec {
        Some(w) => {
            t.mana = w.mana;
            p.magic.skill_points = w.skill_points as _;
            p.magic.known.extend(ids);
        }
        None => {
            t.mana = 0;
            p.magic.skill_points = 0;
        }
    }
}

fn spawn_army(
    game: &mut Game,
    owner: PlayerId,
    at: Position,
    army: &[UnitPick],
) -> Vec<hecs::Entity> {
    let mut out = Vec::new();
    for pick in army {
        let Some(id) = game.content().rules.units.id_of(&pick.unit) else {
            continue;
        };
        let u = game.spawn_unit(owner, at, id);
        game.scenario_set_experience(u, pick.xp);
        game.scenario_set_hurt(u, pick.hurt);
        out.push(u);
    }
    out
}

/// Build the game of a scenario and begin its battle: the attacker is wizard 0, the defender
/// wizard 1.
pub fn build(content: &Content, s: &BattleScenario) -> Result<Game, String> {
    let mut game = new_game(
        content,
        &NewGame {
            seed: s.seed,
            all_ai: true,
            ..NewGame::default()
        },
    )?;
    game.set_undo(false);
    for u in game.units().all_units() {
        game.scenario_remove_unit(u);
    }
    set_wizard(&mut game, ATTACKER, &s.attacker_wizard);
    set_wizard(&mut game, DEFENDER, &s.defender_wizard);
    let capital = game
        .economy()
        .cities()
        .into_iter()
        .find(|(_, c, _)| c.owner == DEFENDER)
        .map(|(e, _, p)| (e, p))
        .ok_or("the defender has no city")?;
    let (from, to) = match s.ground {
        Ground::City { walls } => {
            let (city, at) = capital;
            let wall = game.content().rules.buildings.id_of("city_walls");
            if let Some(wall) = wall {
                game.scenario_set_building(city, wall, walls);
            }
            let from = around(at)
                .find(|&p| walkable(&game, p))
                .ok_or("no land beside the city")?;
            (from, at)
        }
        Ground::Field => field_pair(&game, capital.1, (s.seed % 97) as usize)
            .ok_or("no open ground near the city")?,
    };
    debug_assert!(from.plane == Plane::Arcanus || from.plane == Plane::Myrror);
    let attackers = spawn_army(&mut game, ATTACKER, from, &s.attackers);
    let defenders = spawn_army(&mut game, DEFENDER, to, &s.defenders);
    let events = game.apply(Command::BeginBattle {
        attackers,
        defenders,
    });
    if let Some(Event::Rejected(r)) = events.iter().find(|e| matches!(e, Event::Rejected(_))) {
        return Err(format!("the battle was refused: {r:?}"));
    }
    if !game.in_battle() {
        return Err("no battle began".into());
    }
    Ok(game)
}
