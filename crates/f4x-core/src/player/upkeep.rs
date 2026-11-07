//! The wizard's choice of what to give up when the mana reserve cannot pay the upkeep.
//!
//! The turn end drops the highest upkeep by itself (the cheapest to replace first among equals) until the reserve covers the rest
//! (`magic::upkeep_plan`). A player who would like to choose asks [`Game::upkeep_shortfall`]
//! before ending the turn, and answers with `Command::ReleaseUpkeep`: what is released goes
//! at once, so the turn change finds the reserve enough (or drops only what is still short).
//!
//! Tests: in wizard/magic_game/tests.rs.

use crate::game::{Event, Game};
use crate::player::PlayerId;
use crate::spell::cast::{UpkeepItem, UpkeepKind};
use crate::wizard::magic::MagicError;
use f4x_def::half_int::HalfInt;

/// One thing the wizard pays mana for each turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpkeepLine {
    pub kind: UpkeepKind,
    /// What it costs each turn (a Channeler's half already taken).
    pub mana: HalfInt,
    /// What it takes to replace it (see `UpkeepItem::cost`).
    pub cost: i32,
}

/// The upkeep the next turn change cannot pay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortfall {
    /// The reserve the turn change will find, with this turn's mana income added.
    pub reserve: HalfInt,
    /// The upkeep it will ask for.
    pub owed: HalfInt,
    /// Everything that costs mana, oldest first; the turn change would give up from the end.
    pub lines: Vec<UpkeepLine>,
}

impl Shortfall {
    /// What the turn change would give up by itself (`magic::upkeep_release_order`): the
    /// highest upkeep first, then the cheapest to replace, then the newest, until the reserve
    /// covers the rest.
    pub fn default_release(&self) -> Vec<UpkeepKind> {
        let mut owed = self.owed;
        let mut out = Vec::new();
        let entries: Vec<(i32, i32)> = self.lines.iter().map(|l| (l.mana.raw(), l.cost)).collect();
        for i in crate::wizard::magic::upkeep_release_order(&entries) {
            if owed <= self.reserve {
                break;
            }
            out.push(self.lines[i].kind);
            owed -= self.lines[i].mana;
        }
        out
    }
}

impl Game {
    /// The shortfall the wizard faces at the next turn change, or `None` when the reserve
    /// (with the coming mana income) pays the whole upkeep.
    pub fn upkeep_shortfall(&self, player: PlayerId) -> Option<Shortfall> {
        let percent = self.wizard(player)?.upkeep_percent(&self.content.rules);
        let items = self.upkeep_items(player);
        let part =
            |mana: i32| HalfInt::from_raw(mana.saturating_mul(2).saturating_mul(percent) / 100);
        let owed = part(items.iter().map(|i| i.mana).sum::<i32>());
        // The turn change adds the mana income first (not for the wizard who stopped time).
        let income = if self.time_stopper() == Some(player) {
            HalfInt::ZERO
        } else {
            self.income(player).split.mana.max(HalfInt::ZERO)
        };
        let reserve = self.player(player)?.treasury().mana + income;
        if owed <= reserve {
            return None;
        }
        let lines = items
            .iter()
            .map(|i| UpkeepLine {
                kind: i.kind,
                mana: part(i.mana),
                cost: i.cost,
            })
            .collect();
        Some(Shortfall {
            reserve,
            owed,
            lines,
        })
    }

    /// Give up the chosen upkeep items now. Each must be something the wizard really pays for
    /// (otherwise nothing is released).
    pub fn release_upkeep(
        &mut self,
        player: PlayerId,
        kinds: &[UpkeepKind],
    ) -> Result<Vec<Event>, MagicError> {
        let items = self.upkeep_items(player);
        let mut chosen = Vec::new();
        for kind in kinds {
            let item = items
                .iter()
                .find(|i| i.kind == *kind)
                .ok_or(MagicError::NotAvailable)?;
            if !chosen.iter().any(|c: &UpkeepItem| c.kind == *kind) {
                chosen.push(*item);
            }
        }
        let mut events = Vec::new();
        if self.upkeep_shortfall(player).is_some() {
            events.push(Event::Shortage {
                player,
                what: crate::game::Shortage::Mana,
            });
        }
        for item in chosen {
            events.extend(self.drop_upkeep_item(player, item));
        }
        Ok(events)
    }
}
