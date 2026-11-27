//! Attacks that wait for the player attacked (`docs/turn-structure.md` 10.4).
//!
//! An attack on the human player, sent by another on its turn, is not fought at once: the game
//! holds it, saved by the creation numbers of the units, until the defender answers
//! (`Command::Defend`: who fights, and in person or left to the computer). The master asks the
//! defender before the attacker's turn ends. An attack on a computer player is fought as it is
//! sent.
//!
//! Tests: in tests/it/attack.rs.

use f4x_core::battle::world::BattleStartError;
use f4x_core::player::PlayerId;
use f4x_core::world::{Command, Event};
use f4x_def::id::Serial;
use serde::{Deserialize, Serialize};

use super::Game;

/// A held attack as it is saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldAttack {
    attackers: Vec<Serial>,
    defenders: Vec<Serial>,
}

/// An attack that waits for its defender's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attack {
    pub attackers: Vec<hecs::Entity>,
    /// The stack attacked.
    pub defenders: Vec<hecs::Entity>,
    pub defender: PlayerId,
}

impl Game {
    /// The oldest attack that waits for an answer and can still be fought: its units are all
    /// there, on neighbouring squares, and no treaty has come between.
    pub fn waiting_attack(&self) -> Option<Attack> {
        self.state.attacks.iter().find_map(|a| self.standing(a))
    }

    fn standing(&self, held: &HeldAttack) -> Option<Attack> {
        let units = self.world.units();
        let find = |serials: &[Serial]| -> Option<Vec<hecs::Entity>> {
            serials.iter().map(|&s| units.entity_of_serial(s)).collect()
        };
        let attackers = find(&held.attackers)?;
        let defenders = find(&held.defenders)?;
        self.world.check_attack(&attackers, &defenders).ok()?;
        let defender = units.unit_owner(*defenders.first()?)?;
        Some(Attack {
            attackers,
            defenders,
            defender,
        })
    }

    /// The stacks of `command`, when it is an attack the game has to hold: another player's on
    /// the human.
    pub(super) fn attack_to_hold<'a>(
        &self,
        command: &'a Command,
    ) -> Option<(&'a [hecs::Entity], &'a [hecs::Entity])> {
        let (attackers, defenders) = match command {
            Command::Anyway(inner) => return self.attack_to_hold(inner),
            Command::BeginBattle {
                attackers,
                defenders,
            }
            | Command::AutoResolve {
                attackers,
                defenders,
                ..
            } => (attackers, defenders),
            _ => return None,
        };
        let owner = |stack: &[hecs::Entity]| {
            let first = stack.first()?;
            self.world.units().unit_owner(*first)
        };
        let human = self.world.human()?;
        (owner(defenders) == Some(human) && owner(attackers) != Some(human))
            .then_some((attackers, defenders))
    }

    /// Hold an attack until its defender answers. The attackers move no further this turn.
    pub(super) fn hold_attack(
        &mut self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
    ) -> Result<(), BattleStartError> {
        self.world.check_attack(attackers, defenders)?;
        let units = self.world.units();
        let serials = |stack: &[hecs::Entity]| stack.iter().map(|&u| units.serial_of(u)).collect();
        let held = HeldAttack {
            attackers: serials(attackers),
            defenders: serials(defenders),
        };
        if !self.state.attacks.contains(&held) {
            self.state.attacks.push(held);
        }
        self.world.commit_attackers(attackers);
        Ok(())
    }

    /// Which held attack `command` answers: it names the attackers, and fighters of the stack
    /// attacked. `None` for a command that is no answer.
    pub(super) fn answered_attack(&self, command: &Command) -> Option<usize> {
        let (attackers, fighters) = match command {
            Command::Anyway(inner) => return self.answered_attack(inner),
            Command::Defend {
                attackers,
                fighters,
                ..
            } => (attackers, fighters),
            _ => return None,
        };
        self.state.attacks.iter().position(|held| {
            self.standing(held).is_some_and(|attack| {
                attack.attackers == *attackers
                    && !fighters.is_empty()
                    && fighters.iter().all(|u| attack.defenders.contains(u))
            })
        })
    }

    /// Nobody answers `attack`: the computer fights it for the defender. An attack that cannot
    /// be fought is dropped. For the master, and a front end that cannot ask.
    pub fn leave_attack(&mut self, attack: &Attack) -> Vec<Event> {
        let events = self.apply(
            attack.defender,
            Command::Defend {
                attackers: attack.attackers.clone(),
                fighters: attack.defenders.clone(),
                auto: true,
                mana: None,
            },
        );
        // Still there: the world refused to fight it.
        let held = &self.state.attacks;
        let stuck = held
            .iter()
            .position(|h| self.standing(h).as_ref() == Some(attack));
        if let Some(at) = stuck {
            self.state.attacks.remove(at);
        }
        events
    }
}
