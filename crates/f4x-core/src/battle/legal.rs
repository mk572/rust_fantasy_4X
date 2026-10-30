//! Every command the side to play may give now (without spells, which need the rules: see
//! `Game::legal_battle_casts`). The console's battle queries and the learning environment
//! (`docs/learning-ai.md` section 4) use the same list; a test checks it against
//! [`Battle::apply`] for every command that could be written.
//!
//! Tests: in battle/tests.rs.

use super::map::{distance, GRID_H, GRID_W};
use super::{Battle, BattleCommand, RangedKind, Side};

impl Battle {
    /// The commands of the active side that `apply` accepts now: for each unit that can still
    /// act, its moves, melee attacks, shots and wall attacks, then `Done` for each, `EndTurn`
    /// and `Flee`. Empty once the battle is won.
    pub fn legal_commands(&self) -> Vec<BattleCommand> {
        if self.winner.is_some() {
            return Vec::new();
        }
        let side = self.active;
        let mut out = Vec::new();
        for u in self.active_units(side) {
            let unit = u.id;
            let mut moves: Vec<_> = self.reachable(unit).into_keys().collect();
            moves.sort_unstable();
            if u.moves_left > 0 {
                out.extend(moves.into_iter().map(|to| BattleCommand::Move { unit, to }));
            }
            if !u.attacked {
                let shoots = matches!(u.ranged_kind, RangedKind::Missile | RangedKind::Magic)
                    && u.ranged > 0
                    && u.ammo > 0;
                for t in self
                    .active_units(side.other())
                    .filter(|t| self.seen_by(side, t.id))
                {
                    if distance(u.cell, t.cell) == 1 {
                        out.push(BattleCommand::Attack { unit, target: t.id });
                    }
                    if shoots && !self.blocked_by_darkness(unit, t.id) {
                        out.push(BattleCommand::Shoot { unit, target: t.id });
                    }
                }
                if side == Side::Attacker && u.has("wall_crusher") {
                    for y in 0..GRID_H {
                        for x in 0..GRID_W {
                            let cell = (x, y);
                            if !self.map.has_wall(cell) {
                                continue;
                            }
                            if distance(u.cell, cell) == 1 {
                                out.push(BattleCommand::AttackWall { unit, cell });
                            }
                            if shoots {
                                out.push(BattleCommand::ShootWall { unit, cell });
                            }
                        }
                    }
                }
            }
            out.push(BattleCommand::Done { unit });
        }
        out.push(BattleCommand::EndTurn);
        out.push(BattleCommand::Flee);
        out
    }
}
