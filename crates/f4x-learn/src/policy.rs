//! Simple policies: a random one that explores, and the plain scripted battle AI of the core.

use f4x_core::battle::{ai, Battle, BattleCommand};
use rand::Rng;

/// The class of a command, for choosing a kind of action before a particular one.
fn class(c: &BattleCommand) -> usize {
    match c {
        BattleCommand::Move { .. } => 0,
        BattleCommand::Attack { .. } => 1,
        BattleCommand::Shoot { .. } => 2,
        BattleCommand::AttackWall { .. } | BattleCommand::ShootWall { .. } => 3,
        BattleCommand::Cast { .. } => 4,
        BattleCommand::Done { .. } => 5,
        BattleCommand::EndTurn => 6,
        BattleCommand::Flee => 7,
    }
}

/// A random command: first a kind of action (all kinds that are possible are equally likely,
/// and fleeing is rare), then one of that kind. This ends turns and fights in a few hundred
/// steps, where a draw over every command (hundreds of moves to each attack) would not.
pub fn random_command<R: Rng>(actions: &[BattleCommand], rng: &mut R) -> BattleCommand {
    let mut present = [false; 8];
    for c in actions {
        present[class(c)] = true;
    }
    let kinds: Vec<usize> = (0..8)
        .filter(|&k| present[k] && (k != 7 || rng.gen_bool(0.01)))
        .collect();
    let kind = kinds
        .get(rng.gen_range(0..kinds.len().max(1)))
        .copied()
        .unwrap_or(6);
    let of_kind: Vec<&BattleCommand> = actions.iter().filter(|c| class(c) == kind).collect();
    of_kind
        .get(rng.gen_range(0..of_kind.len().max(1)))
        .map_or(BattleCommand::EndTurn, |c| (*c).clone())
}

/// The core's plain battle AI.
pub fn scripted_command(battle: &Battle) -> BattleCommand {
    ai::next_command(battle)
}
