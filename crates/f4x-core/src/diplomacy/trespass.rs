//! Trespass (`docs/diplomacy.md` section 6): units standing next to another's city. A settler and an
//! engineer do no harm there. Between the partners of a wizard pact each turn of it in a row is a
//! warning, and at the last one (`trespass_warnings` in `rules.ron`) the city's owner chooses: break
//! the treaty, declare war, or overlook it. Allies are warned too only under the setting
//! `allied_trespass_penalty`. The core only counts; the choice is a command of the owner's.
//!
//! Tests: in `tests/it/c9_diplomacy.rs`.

use std::collections::BTreeSet;

use super::{Diplomacy, DiplomacyError, DiplomacyEvent, Treaty};
use crate::player::view::PlayerView;
use crate::player::PlayerId;
use crate::rules::Stat;
use crate::world::World;

impl Diplomacy {
    /// `player` lets the last warning it gave `with` pass.
    pub(super) fn overlook(
        &mut self,
        view: &World,
        player: PlayerId,
        with: PlayerId,
    ) -> Result<(), DiplomacyError> {
        let last = view.content().rules.config.diplomacy.trespass_warnings;
        if self.stance(player, with).is_none_or(|s| s.trespass < last) {
            return Err(DiplomacyError::NothingPending);
        }
        self.stance_mut(player, with).trespass = 0;
        Ok(())
    }
}

impl World {
    /// Whether `unit` trespasses where it stands next to another's city: a settler and an engineer
    /// do not.
    pub fn unit_can_trespass(&self, unit: hecs::Entity) -> bool {
        !self.units().unit_is_settler(unit) && self.units().unit_stat(unit, Stat::Construction) == 0
    }

    /// The players not at war with `owner` whose units, as far as it sees them, stand within
    /// `trespass_distance` squares of one of its cities.
    pub fn trespassers(&self, owner: PlayerId) -> Vec<PlayerId> {
        let reach = self.content().rules.config.diplomacy.trespass_distance;
        let view = PlayerView::new(self, owner);
        let mut found = BTreeSet::new();
        for (_, city) in view.my_cities() {
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let units = view
                        .shift(city, dx, dy)
                        .and_then(|square| view.hostile_at(square));
                    for unit in units.into_iter().flatten() {
                        if self.unit_can_trespass(unit) {
                            found.extend(self.units().unit_owner(unit));
                        }
                    }
                }
            }
        }
        found
            .into_iter()
            .filter(|&p| p != owner && self.treaty(owner, p) != Treaty::War)
            .collect()
    }

    /// The treaty partners that have had their last warning from `owner`, which now chooses what to
    /// do about each: `BreakTreaty`, `DeclareWar` or `Overlook`. What is left open lapses with the
    /// owner's turn.
    pub fn trespass_choices(&self, owner: PlayerId) -> Vec<PlayerId> {
        let last = self.content().rules.config.diplomacy.trespass_warnings;
        self.player_ids()
            .into_iter()
            .filter(|&p| {
                self.diplomacy
                    .stance(owner, p)
                    .is_some_and(|s| s.trespass >= last)
            })
            .collect()
    }

    /// Whether `owner` is warned of the units of `intruder` near its cities: its partner in a
    /// wizard pact, and an ally under the setting `allied_trespass_penalty`.
    fn minds_trespass(&self, owner: PlayerId, intruder: PlayerId) -> bool {
        match self.treaty(owner, intruder) {
            Treaty::Pact => true,
            Treaty::Alliance => self.settings.rules.allied_trespass_penalty,
            Treaty::None | Treaty::War => false,
        }
    }

    /// The start of `owner`'s turn: a warning to each partner that trespasses, one more for each
    /// turn in a row; the count of one that has left starts again.
    pub(crate) fn warn_trespassers(&mut self, owner: PlayerId) {
        let partners: Vec<PlayerId> = self
            .player_ids()
            .into_iter()
            .filter(|&p| self.minds_trespass(owner, p))
            .collect();
        if partners.is_empty() {
            return;
        }
        let there = self.trespassers(owner);
        for intruder in partners {
            let stance = self.diplomacy.stance_mut(owner, intruder);
            if !there.contains(&intruder) {
                stance.trespass = 0;
                continue;
            }
            stance.trespass = stance.trespass.saturating_add(1);
            let warning = stance.trespass;
            self.report_diplomacy(DiplomacyEvent::Trespassing {
                owner,
                intruder,
                warning,
            });
        }
    }

    /// The end of `owner`'s turn: a choice it did not make lapses.
    pub(crate) fn lapse_trespass(&mut self, owner: PlayerId) {
        for intruder in self.trespass_choices(owner) {
            self.diplomacy.stance_mut(owner, intruder).trespass = 0;
        }
    }
}
