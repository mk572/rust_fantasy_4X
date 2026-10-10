//! A plain opponent for battles: every unit walks at the nearest enemy and hits it.
//!
//! This only lets the other side of a fight play; spell casting, target choice, retreating and
//! holding positions belong to the real battle AI (task 46).

use super::map::{distance, Pos};
use super::{Battle, BattleCommand, BattleUnit, RangedKind};

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
        if !unit.attacked {
            if distance(unit.cell, enemy.cell) == 1 {
                return BattleCommand::Attack {
                    unit: unit.id,
                    target: enemy.id,
                };
            }
            if can_shoot(unit) {
                return BattleCommand::Shoot {
                    unit: unit.id,
                    target: enemy.id,
                };
            }
        }
        // Walk to the reachable square closest to the enemy, if that is closer than here.
        let here = distance(unit.cell, enemy.cell);
        let best: Option<(Pos, u32)> = battle
            .reachable(unit.id)
            .into_iter()
            .min_by_key(|&(p, cost)| (distance(p, enemy.cell), cost, p));
        if let Some((to, _)) = best {
            if unit.moves_left > 0 && distance(to, enemy.cell) < here {
                return BattleCommand::Move { unit: unit.id, to };
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
