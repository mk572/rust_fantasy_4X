//! The wizard's choice of what to give up when the mana reserve cannot pay the upkeep.
//!
//! The turn change drops the newest items by itself until the reserve covers the rest
//! (`magic::upkeep_plan`). A player who would like to choose asks [`Game::upkeep_shortfall`]
//! before ending the turn, and answers with `Command::ReleaseUpkeep`: what is released goes
//! at once, so the turn change finds the reserve enough (or drops only what is still short).

use crate::game::{Event, Game};
use crate::player::PlayerId;
use crate::spell::cast::{UpkeepItem, UpkeepKind};
use crate::wizard::magic::MagicError;

/// One thing the wizard pays mana for each turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpkeepLine {
    pub kind: UpkeepKind,
    /// What it costs each turn (a Channeler's half already taken).
    pub mana: i32,
    /// "Haste on Spearmen", "Flying Fortress on Aldaria", "Skeleton".
    pub label: String,
}

/// The upkeep the next turn change cannot pay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortfall {
    /// The reserve the turn change will find, with this turn's mana income added.
    pub reserve: i32,
    /// The upkeep it will ask for.
    pub owed: i32,
    /// Everything that costs mana, oldest first; the turn change would give up from the end.
    pub lines: Vec<UpkeepLine>,
}

impl Shortfall {
    /// What the turn change would give up by itself: the newest first until the reserve covers
    /// the rest.
    pub fn default_release(&self) -> Vec<UpkeepKind> {
        let mut owed = self.owed;
        let mut out = Vec::new();
        for line in self.lines.iter().rev() {
            if owed <= self.reserve {
                break;
            }
            out.push(line.kind);
            owed -= line.mana;
        }
        out
    }
}

impl Game {
    /// The shortfall the wizard faces at the next turn change, or `None` when the reserve
    /// (with the coming mana income) pays the whole upkeep.
    pub fn upkeep_shortfall(&self, player: PlayerId) -> Option<Shortfall> {
        let divisor = self.magic(player)?.upkeep_divisor();
        let items = self.upkeep_items(player);
        let owed: i32 = items.iter().map(|i| i.mana).sum::<i32>() / divisor;
        // The turn change adds the mana income first (not for the wizard who stopped time).
        let income = if self.time_stopper() == Some(player) {
            0
        } else {
            self.income(player).split.mana.max(0)
        };
        let reserve = self.player(player)?.mana.saturating_add(income);
        if owed <= reserve {
            return None;
        }
        let lines = items
            .iter()
            .map(|i| UpkeepLine {
                kind: i.kind,
                mana: i.mana / divisor,
                label: self.upkeep_label(i),
            })
            .collect();
        Some(Shortfall {
            reserve,
            owed,
            lines,
        })
    }

    fn upkeep_label(&self, item: &UpkeepItem) -> String {
        let spells = &self.content.rules.spells;
        match item.kind {
            UpkeepKind::Global(s) => spells.get(s).name.clone(),
            UpkeepKind::City(c, s) => {
                let city = self.city_name(c).unwrap_or_default();
                format!("{} on {city}", spells.get(s).name)
            }
            UpkeepKind::UnitSpell(u, s) => {
                let unit = self.units().unit_def(u).map_or("?", |d| d.name.as_str());
                format!("{} on {unit}", spells.get(s).name)
            }
            UpkeepKind::Unit(u) => self
                .units()
                .unit_def(u)
                .map_or_else(|| "?".to_string(), |d| d.name.clone()),
        }
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
