//! The scripted battle AI (task 46, `docs/ai-design.md` section 6).
//!
//! [`baseline_command`] is the plain opponent the AI began as: every unit walks at the nearest
//! enemy and hits it. It stays as the yardstick the smarter [`next_command`] is measured against
//! (`f4x-learn`'s mirrored fights). The smarter one chooses targets by what an attack is worth
//! (damage dealt against what the target has left, and what it is worth), so a side focuses its
//! fire on the unit it can kill; shooters stay where they can shoot instead of walking into melee;
//! a defender inside a walled city stays behind the wall. Spells are chosen by the wizard's brain
//! with the whole game (`game.rs`), not here: a [`Battle`] alone knows nothing of the wizard.

use super::attack::Mode;
use super::immunity::{AttackNature, Immunities};
use super::map::{distance, in_city_area, Pos};
use super::{Battle, BattleCommand, BattleUnit, RangedKind, Side};
use crate::game::setting::MeleeVsFliers;

/// The nearest active enemy the unit's side can see (lowest id on ties).
fn closest_enemy<'a>(battle: &'a Battle, unit: &BattleUnit) -> Option<&'a BattleUnit> {
    battle
        .active_units(unit.side.other())
        .filter(|e| battle.seen_by(unit.side, e.id))
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

/// The plain AI: the next order for the side whose turn it is. Every order either spends
/// something of a unit's turn or ends the side's turn, so repeating it until the side changes
/// terminates.
pub fn baseline_command(battle: &Battle) -> BattleCommand {
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

// ---- the smarter AI ------------------------------------------------------------

/// How hard a unit hits and how much it can take, as one number: the one strength both the
/// battle AI (`worth`) and the strategic AI (`f4x_ai::view::unit_strength`) weigh units by.
pub fn strength(attack: i32, defense: i32, hit_points: i32) -> f32 {
    (attack.max(0) as f32 * 1.5 + defense.max(0) as f32 + 1.0) * hit_points.max(0) as f32
}

/// What a unit is worth in a fight: its strength times what it has left.
pub fn worth(u: &BattleUnit) -> f32 {
    strength(u.melee.max(u.ranged), u.defense, u.hit_points())
}

/// The damage (hit points) `attacker` is expected to do to `target` with one attack of kind
/// `ranged`: its dice that hit, less what the target's defence blocks. Zero when the target is
/// immune to what the attack is made of.
pub fn expected_damage(
    attacker: &BattleUnit,
    target: &BattleUnit,
    ranged: bool,
    vs_fliers: MeleeVsFliers,
) -> f32 {
    let (mode, strength) = if ranged {
        (Mode::Ranged, attacker.ranged)
    } else if target.flies && !attacker.flies {
        (Mode::Melee, vs_fliers.strength(attacker.melee))
    } else {
        (Mode::Melee, attacker.melee)
    };
    if strength <= 0 {
        return 0.0;
    }
    let nature = AttackNature::of_unit_attack(mode, &attacker.abilities, attacker.magical_weapon);
    if nature.stopped_by.meets(&Immunities::of(&target.abilities)) {
        return 0.0;
    }
    let p_hit = (0.3 + 0.1 * attacker.to_hit as f32).clamp(0.05, 0.95);
    let p_block = (0.3 + 0.1 * target.to_block as f32).clamp(0.05, 0.95);
    let hits = attacker.figures as f32 * strength as f32 * p_hit;
    let defence = if nature.armor_piercing {
        target.defense / 2
    } else {
        target.defense
    }
    .max(0) as f32;
    let blocked = attacker.figures.min(target.figures) as f32 * defence * p_block;
    (hits - blocked).max(hits * 0.1)
}

/// What an attack is worth: how much of the target it takes (a kill is worth all of it) times
/// what the target is worth, with a premium on the units that shoot back.
fn attack_value(
    attacker: &BattleUnit,
    target: &BattleUnit,
    ranged: bool,
    vs_fliers: MeleeVsFliers,
) -> f32 {
    let left = target.hit_points().max(1) as f32;
    let taken = (expected_damage(attacker, target, ranged, vs_fliers) / left).min(1.0);
    let threat = if can_shoot(target) { 1.2 } else { 1.0 };
    taken * worth(target) * threat
}

/// The next order for the side whose turn it is: the smarter AI.
pub fn next_command(battle: &Battle) -> BattleCommand {
    let side = battle.active;
    for unit in battle.active_units(side) {
        if unit.attacked && unit.moves_left == 0 {
            continue;
        }
        let enemies: Vec<&BattleUnit> = battle
            .active_units(side.other())
            .filter(|e| battle.seen_by(side, e.id))
            .collect();
        let Some(&nearest) = enemies
            .iter()
            .min_by_key(|e| (distance(unit.cell, e.cell), e.id))
        else {
            return BattleCommand::EndTurn;
        };
        if let Some(order) = wall_order(battle, unit, nearest) {
            return order;
        }
        let shooter = can_shoot(unit);
        let adjacent = enemies.iter().any(|e| distance(unit.cell, e.cell) == 1);
        if !unit.attacked {
            let melee = enemies
                .iter()
                .filter(|e| distance(unit.cell, e.cell) == 1)
                .map(|e| (attack_value(unit, e, false, battle.melee_vs_fliers), e.id))
                .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
            let shot = if shooter {
                enemies
                    .iter()
                    .filter(|e| !battle.blocked_by_darkness(unit.id, e.id))
                    .map(|e| (attack_value(unit, e, true, battle.melee_vs_fliers), e.id))
                    .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)))
            } else {
                None
            };
            match (melee, shot) {
                (Some((m, target)), Some((s, _))) if m >= s => {
                    return BattleCommand::Attack {
                        unit: unit.id,
                        target,
                    }
                }
                (_, Some((_, target))) => {
                    return BattleCommand::Shoot {
                        unit: unit.id,
                        target,
                    }
                }
                (Some((_, target)), None) => {
                    return BattleCommand::Attack {
                        unit: unit.id,
                        target,
                    }
                }
                (None, None) => {}
            }
        }
        // Shooters with shots left stay where they can shoot; so does a defender behind walls.
        let behind_walls = side == Side::Defender
            && battle.map.is_walled()
            && !battle.map.is_breached()
            && in_city_area(unit.cell);
        let has_shot = shooter
            && enemies
                .iter()
                .any(|e| !battle.blocked_by_darkness(unit.id, e.id));
        if unit.moves_left == 0 || (has_shot && !adjacent) || behind_walls {
            return BattleCommand::Done { unit: unit.id };
        }
        // Walk towards the enemy that is worth the most for the walk.
        let goal = *enemies
            .iter()
            .max_by(|a, b| {
                let value = |e: &&BattleUnit| {
                    attack_value(unit, e, false, battle.melee_vs_fliers)
                        / (1.0 + distance(unit.cell, e.cell) as f32)
                };
                value(a).total_cmp(&value(b)).then(b.id.cmp(&a.id))
            })
            .unwrap_or(&nearest);
        if let Some(to) = approach(battle, unit, goal) {
            return BattleCommand::Move { unit: unit.id, to };
        }
        return BattleCommand::Done { unit: unit.id };
    }
    BattleCommand::EndTurn
}

/// The square to walk to to get closer to `goal`, if there is one: the furthest along the way
/// round the walls where there are walls, else the reachable square nearest to it.
fn approach(battle: &Battle, unit: &BattleUnit, goal: &BattleUnit) -> Option<Pos> {
    if battle.map.is_walled() {
        let reach = battle.reachable(unit.id);
        return route(battle, unit, goal.cell)
            .into_iter()
            .rfind(|p| reach.contains_key(p));
    }
    let here = distance(unit.cell, goal.cell);
    battle
        .reachable(unit.id)
        .into_iter()
        .min_by_key(|&(p, cost)| (distance(p, goal.cell), cost, p))
        .filter(|&(p, _)| distance(p, goal.cell) < here)
        .map(|(p, _)| p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::map::BattleMap;
    use crate::battle::{BattleUnit, Side};
    use crate::player::PlayerId;

    fn duel(a: Pos, d: Pos) -> Battle {
        let units = vec![
            BattleUnit::simple(Side::Attacker, "a", 1, 5, 5, 2).at(a),
            BattleUnit::simple(Side::Defender, "d", 1, 5, 5, 2).at(d),
        ];
        Battle::new(PlayerId(0), PlayerId(1), BattleMap::open(), units, 1)
    }

    fn shooter(side: Side, cell: Pos) -> BattleUnit {
        let mut u = BattleUnit::simple(side, "archer", 3, 1, 1, 1).at(cell);
        u.ranged = 3;
        u.ammo = 5;
        u.ranged_kind = RangedKind::Missile;
        u
    }

    #[test]
    fn a_unit_hits_the_enemy_it_can_finish_rather_than_the_first_one() {
        // Two enemies stand next to the attacker: a full-strength unit and a nearly dead one of
        // the same kind. The wounded one gives the bigger share of a kill.
        let mut strong = BattleUnit::simple(Side::Defender, "strong", 3, 3, 3, 2).at((6, 5));
        let mut weak = BattleUnit::simple(Side::Defender, "weak", 3, 3, 3, 2).at((6, 6));
        weak.figures = 1;
        weak.front_damage = 2;
        strong.id = 0;
        let units = vec![
            BattleUnit::simple(Side::Attacker, "a", 3, 3, 3, 2).at((5, 5)),
            strong,
            weak,
        ];
        let b = Battle::new(PlayerId(0), PlayerId(1), BattleMap::open(), units, 1);
        assert_eq!(
            next_command(&b),
            BattleCommand::Attack { unit: 0, target: 2 }
        );
        // The plain AI takes the lowest id of the nearest.
        assert_eq!(
            baseline_command(&b),
            BattleCommand::Attack { unit: 0, target: 1 }
        );
    }

    #[test]
    fn a_shooter_with_shots_left_stays_put_instead_of_walking_into_melee() {
        let units = vec![
            shooter(Side::Attacker, (3, 5)),
            BattleUnit::simple(Side::Defender, "d", 3, 3, 3, 2).at((12, 5)),
        ];
        let mut b = Battle::new(PlayerId(0), PlayerId(1), BattleMap::open(), units, 1);
        assert_eq!(
            next_command(&b),
            BattleCommand::Shoot { unit: 0, target: 1 }
        );
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .unwrap();
        assert_eq!(next_command(&b), BattleCommand::Done { unit: 0 });
        // The plain AI walks up after shooting.
        assert!(matches!(
            baseline_command(&b),
            BattleCommand::Move { unit: 0, .. }
        ));
    }

    #[test]
    fn a_shooter_out_of_shots_walks_up_to_the_enemy() {
        let mut archer = shooter(Side::Attacker, (3, 5));
        archer.ammo = 0;
        let units = vec![
            archer,
            BattleUnit::simple(Side::Defender, "d", 3, 3, 3, 2).at((12, 5)),
        ];
        let b = Battle::new(PlayerId(0), PlayerId(1), BattleMap::open(), units, 1);
        assert!(matches!(
            next_command(&b),
            BattleCommand::Move { unit: 0, .. }
        ));
    }

    #[test]
    fn a_defender_behind_walls_holds_its_ground() {
        let units = vec![
            BattleUnit::simple(Side::Attacker, "a", 3, 3, 3, 2).at((3, 12)),
            BattleUnit::simple(Side::Defender, "d", 3, 3, 3, 2).at((13, 11)),
        ];
        let mut b = Battle::new(
            PlayerId(0),
            PlayerId(1),
            BattleMap::open().with_city(true),
            units,
            1,
        );
        b.apply(BattleCommand::EndTurn).unwrap();
        assert_eq!(b.active, Side::Defender);
        assert_eq!(next_command(&b), BattleCommand::Done { unit: 1 });
    }

    #[test]
    fn a_walker_expects_nothing_from_a_flier_in_melee_unless_the_setting_allows_it() {
        let walker = BattleUnit::simple(Side::Attacker, "walker", 4, 2, 6, 1);
        let mut bat = BattleUnit::simple(Side::Defender, "bat", 4, 2, 6, 1);
        bat.flies = true;
        let expect = |rule| expected_damage(&walker, &bat, false, rule);
        assert_eq!(expect(MeleeVsFliers::Zero), 0.0);
        assert!(expect(MeleeVsFliers::Half) > 0.0);
        assert!(expect(MeleeVsFliers::Full) > expect(MeleeVsFliers::Half));
    }

    #[test]
    fn expected_damage_falls_with_defence_and_is_nothing_against_an_immunity() {
        let a = BattleUnit::simple(Side::Attacker, "a", 4, 1, 4, 1);
        let soft = BattleUnit::simple(Side::Defender, "soft", 4, 1, 1, 0);
        let hard = BattleUnit::simple(Side::Defender, "hard", 4, 1, 1, 4);
        assert!(
            expected_damage(&a, &soft, false, MeleeVsFliers::default())
                > expected_damage(&a, &hard, false, MeleeVsFliers::default())
        );
        let mut stone = BattleUnit::simple(Side::Defender, "stone", 4, 1, 1, 0);
        stone.abilities.push("weapon_immunity".into());
        assert_eq!(
            expected_damage(&a, &stone, false, MeleeVsFliers::default()),
            0.0
        );
        let mut magic = a.clone();
        magic.magical_weapon = true;
        assert!(expected_damage(&magic, &stone, false, MeleeVsFliers::default()) > 0.0);
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
