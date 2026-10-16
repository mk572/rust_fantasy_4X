//! Magic Vortex: a whirling object on the battlefield. At the start of each turn it moves three
//! squares at random, and its owner may move it one more square on their own turn. Each time it
//! comes to a square, a unit standing there (not one with Magic Immunity or Righteousness) takes
//! 5 points of damage whatever its defence, every unit next to the square has a one in three
//! chance of being struck by a lightning bolt, and a vortex over the city area adds 5 to the
//! city's damage. At most ten vortices can be on a battlefield. Units cannot walk through one.
//!
//! This follows the original's `Vortex_Combat_Round` and `Vortex_Move_And_Attack`. In the
//! original the computer's vortices never get the controlled move; here a computer wizard's
//! don't either (nothing plays it), but a human can.

use super::attack::unblockable;
use super::immunity::{AttackNature, Immunities, Realm};
use super::map::{self, Pos};
use super::{Battle, BattleError, BattleEvent, Side};
use crate::rules::effects::DamageTraits;

/// Hook of the spell.
pub const MAGIC_VORTEX: &str = "magic_vortex";
/// The most vortices on one battlefield.
pub const MAX_VORTICES: usize = 10;
/// Moves a vortex makes by itself at the start of a turn.
const RANDOM_MOVES: usize = 3;
/// Damage to the unit the vortex lands on, and the strength of the bolts at its neighbours.
const CRUSH_DAMAGE: i32 = 5;
const BOLT_STRENGTH: i32 = 5;
/// City damage for each move over the city area.
const CITY_DAMAGE: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vortex {
    pub cell: Pos,
    pub owner: Side,
    /// It has had its controlled move this turn.
    pub steered: bool,
}

impl Battle {
    /// Whether anything stops a unit walking onto `cell`: a unit or a vortex stands there.
    pub fn is_occupied(&self, cell: Pos) -> bool {
        self.unit_at(cell).is_some() || self.vortex_at(cell).is_some()
    }

    pub fn vortex_at(&self, cell: Pos) -> Option<usize> {
        self.vortices.iter().position(|v| v.cell == cell)
    }

    /// Whether a vortex can be put on `cell`: room for another, and the square is free ground.
    pub fn can_make_vortex(&self, cell: Pos) -> bool {
        self.vortices.len() < MAX_VORTICES && self.map.is_playable(cell) && !self.is_occupied(cell)
    }

    pub(super) fn make_vortex(&mut self, side: Side, cell: Pos) -> BattleEvent {
        self.vortices.push(Vortex {
            cell,
            owner: side,
            steered: false,
        });
        BattleEvent::VortexMade { cell }
    }

    /// The start of a turn: every vortex moves three times at random.
    pub(super) fn vortex_round(&mut self) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        for index in 0..self.vortices.len() {
            for _ in 0..RANDOM_MOVES {
                let from = self.vortices[index].cell;
                let mut to = from;
                for _ in 0..50 {
                    let dx = self.roll(3) as i32 - 2;
                    let dy = self.roll(3) as i32 - 2;
                    let next = (from.0 + dx, from.1 + dy);
                    if next != from && self.map.is_playable(next) && self.vortex_at(next).is_none()
                    {
                        to = next;
                        break;
                    }
                }
                if to != from {
                    events.extend(self.vortex_move(index, to));
                }
            }
        }
        events
    }

    /// A new turn for `side`: its vortices can be steered again.
    pub(super) fn vortex_new_turn(&mut self, side: Side) {
        for v in self.vortices.iter_mut().filter(|v| v.owner == side) {
            v.steered = false;
        }
    }

    /// The owner moves a vortex one square on their turn.
    pub(super) fn steer_vortex(
        &mut self,
        index: usize,
        to: Pos,
    ) -> Result<Vec<BattleEvent>, BattleError> {
        let v = *self.vortices.get(index).ok_or(BattleError::NoSuchUnit)?;
        if v.owner != self.active || v.steered {
            return Err(BattleError::NotYours);
        }
        if map::distance(v.cell, to) != 1 || !self.map.is_playable(to) {
            return Err(BattleError::Unreachable);
        }
        if self.vortex_at(to).is_some() {
            return Err(BattleError::Unreachable);
        }
        self.vortices[index].steered = true;
        Ok(self.vortex_move(index, to))
    }

    /// Move the vortex to `to` and let it strike.
    fn vortex_move(&mut self, index: usize, to: Pos) -> Vec<BattleEvent> {
        self.vortices[index].cell = to;
        let mut events = vec![BattleEvent::VortexMoved { index, to }];
        if self.map.is_city() && map::in_city_area(to) {
            self.city_damage += CITY_DAMAGE;
        }
        let bolt = DamageTraits {
            armor_piercing: true,
            ..Default::default()
        };
        let bolt_nature = AttackNature {
            stopped_by: Immunities {
                magic: true,
                ..Default::default()
            },
            realm: Some(Realm::Chaos),
            armor_piercing: false,
        };
        for id in 0..self.units.len() {
            if !self.units[id].is_active() {
                continue;
            }
            let cell = self.units[id].cell;
            if cell == to {
                let u = &self.units[id];
                if !u.has("magic_immunity") && !u.has_enchantment("righteousness") {
                    let mut state = u.defender_state();
                    let lost = unblockable(&mut state, CRUSH_DAMAGE);
                    if lost.points > 0 {
                        self.units[id].set_from(&state);
                        self.units[id].record(lost);
                        events.push(BattleEvent::FieldStruck {
                            unit: id,
                            dealt: lost.points,
                            kills: lost.kills,
                        });
                        if !self.units[id].is_active() {
                            events.push(BattleEvent::UnitDied { unit: id });
                            continue;
                        }
                    }
                }
            }
            if cell != to && map::distance(cell, to) == 1 && self.roll(3) == 1 {
                let dmg = self.spell_damage(id, BOLT_STRENGTH, &bolt, bolt_nature);
                events.extend(self.spell_events_for_damage(id, dmg));
            }
        }
        events
    }
}
