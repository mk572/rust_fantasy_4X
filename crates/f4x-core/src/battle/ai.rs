//! A plain opponent for battles: every unit walks at the nearest enemy and hits it.
//!
//! This only lets the other side of a fight play; spell casting, target choice, retreating and
//! holding positions belong to the real battle AI (task 46).

use super::map::{distance, Pos};
use super::{Battle, BattleCommand, BattleUnit, RangedKind, Side};

/// The nearest active enemy (lowest id on ties).
fn closest_enemy<'a>(battle: &'a Battle, unit: &BattleUnit) -> Option<&'a BattleUnit> {
    battle
        .active_units(unit.side.other())
        .min_by_key(|e| (distance(unit.cell, e.cell), e.id))
}

fn can_shoot(unit: &BattleUnit) -> bool {
    matches!(unit.ranged_kind, RangedKind::Missile | RangedKind::Magic)
        && unit.ranged > 0
        && unit.ammo > 0
}

/// The squares on the cheapest way from the unit to `to`, round the walls (by their gate, or a
/// breach), ignoring other units, in order from the first step. Empty if there is no way.
fn route(battle: &Battle, unit: &BattleUnit, to: Pos) -> Vec<Pos> {
    let reach = battle
        .map
        .reachable_as(unit.cell, 10_000, unit.mover(), &|_| false);
    let mut path = Vec::new();
    let mut at = to;
    while at != unit.cell {
        let Some(&(_, from)) = reach.get(&at) else {
            return Vec::new();
        };
        path.push(at);
        at = from;
    }
    path.reverse();
    path
}

/// A wall crusher of the attackers goes for the wall when the city has one: the standing wall
/// section nearest the enemy it can get to, struck from next to it. `None` if it should fight as
/// usual (no wall, a breach already, or an enemy within reach).
fn wall_order(battle: &Battle, unit: &BattleUnit, enemy: &BattleUnit) -> Option<BattleCommand> {
    if unit.side != Side::Attacker || !unit.has("wall_crusher") || unit.attacked {
        return None;
    }
    let walls = battle.map.standing_walls();
    if walls.is_empty() || battle.map.is_breached() || distance(unit.cell, enemy.cell) == 1 {
        return None;
    }
    // Strike the nearest wall section from where the unit stands.
    if let Some(&cell) = walls
        .iter()
        .filter(|&&w| distance(unit.cell, w) == 1)
        .min_by_key(|&&w| (distance(w, enemy.cell), w))
    {
        return Some(BattleCommand::AttackWall {
            unit: unit.id,
            cell,
        });
    }
    // Otherwise walk next to the wall section closest to the enemy.
    let target = *walls
        .iter()
        .min_by_key(|&&w| (distance(w, enemy.cell), w))?;
    let to = battle
        .reachable(unit.id)
        .into_iter()
        .filter(|&(p, _)| distance(p, target) == 1)
        .min_by_key(|&(p, cost)| (distance(p, enemy.cell), cost, p))
        .map(|(p, _)| p)?;
    (unit.moves_left > 0).then_some(BattleCommand::Move { unit: unit.id, to })
}

/// The next order for the side whose turn it is. Every order either spends something of a
/// unit's turn or ends the side's turn, so repeating it until the side changes terminates.
pub fn next_command(battle: &Battle) -> BattleCommand {
    let side = battle.active;
    for unit in battle.active_units(side) {
        if unit.attacked && unit.moves_left == 0 {
            continue;
        }
        let Some(enemy) = closest_enemy(battle, unit) else {
            return BattleCommand::EndTurn;
        };
        if let Some(order) = wall_order(battle, unit, enemy) {
            return order;
        }
        if !unit.attacked {
            if distance(unit.cell, enemy.cell) == 1 {
                return BattleCommand::Attack {
                    unit: unit.id,
                    target: enemy.id,
                };
            }
            if can_shoot(unit) && !battle.blocked_by_darkness(unit.id, enemy.id) {
                return BattleCommand::Shoot {
                    unit: unit.id,
                    target: enemy.id,
                };
            }
        }
        // Walk as far towards the enemy as the unit's moves go. Where a wall is in the way, "towards"
        // is along the way round it, not as the crow flies.
        if unit.moves_left > 0 {
            if battle.map.is_walled() {
                let reach = battle.reachable(unit.id);
                let furthest = route(battle, unit, enemy.cell)
                    .into_iter()
                    .rfind(|p| reach.contains_key(p));
                if let Some(to) = furthest {
                    return BattleCommand::Move { unit: unit.id, to };
                }
            } else {
                let here = distance(unit.cell, enemy.cell);
                let best: Option<(Pos, u32)> = battle
                    .reachable(unit.id)
                    .into_iter()
                    .min_by_key(|&(p, cost)| (distance(p, enemy.cell), cost, p));
                if let Some((to, _)) = best {
                    if distance(to, enemy.cell) < here {
                        return BattleCommand::Move { unit: unit.id, to };
                    }
                }
            }
        }
        return BattleCommand::Done { unit: unit.id };
    }
    BattleCommand::EndTurn
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::map::BattleMap;
    use crate::battle::{BattleUnit, Side};
    use crate::city::PlayerId;

    fn duel(a: Pos, d: Pos) -> Battle {
        let units = vec![
            BattleUnit::simple(Side::Attacker, "a", 1, 5, 5, 2).at(a),
            BattleUnit::simple(Side::Defender, "d", 1, 5, 5, 2).at(d),
        ];
        Battle::new(PlayerId(0), PlayerId(1), BattleMap::open(), units, 1)
    }

    #[test]
    fn walks_towards_the_enemy_then_attacks() {
        let mut b = duel((3, 5), (10, 5));
        let first = next_command(&b);
        assert!(matches!(first, BattleCommand::Move { unit: 0, .. }));
        b.apply(first).unwrap();
        assert!(distance(b.units[0].cell, (10, 5)) < 7);
    }

    #[test]
    fn attacks_when_adjacent() {
        let b = duel((5, 5), (6, 5));
        assert_eq!(
            next_command(&b),
            BattleCommand::Attack { unit: 0, target: 1 }
        );
    }

    #[test]
    fn a_whole_fight_ends() {
        let mut b = duel((3, 5), (10, 5));
        for _ in 0..2000 {
            if b.winner.is_some() {
                break;
            }
            let c = next_command(&b);
            b.apply(c).unwrap();
        }
        assert!(b.winner.is_some());
    }
}
