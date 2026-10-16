//! The astrologer's comparison of the wizards and the history of their power (task 71): the
//! original's `Record_History` and `Astrologer_*` in `CITYCALC.c`. The Astrologer screen shows
//! the three scores, the History screen the power over the turns.

use std::collections::BTreeMap;

use crate::city::PlayerId;
use crate::Game;

/// Turns the history keeps; the oldest drop off after that.
pub const HISTORY_TURNS: usize = 288;

/// The three scores of the Astrologer screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Astrology {
    /// Magic power drawn each turn, less ten.
    pub power: i32,
    /// The army: gold upkeep plus twice the mana upkeep of the units, over five.
    pub strength: i32,
    /// The spells known, each worth its place in its realm, over eight.
    pub research: i32,
}

impl Game {
    /// The raw scores of one wizard.
    pub fn astrology(&self, player: PlayerId) -> Astrology {
        let upkeep: i32 = self
            .units()
            .player_units(player)
            .into_iter()
            .map(|u| {
                let mana = self.units().unit_def(u).map_or(0, |d| d.upkeep.mana as i32);
                self.units().unit_gold_upkeep(u) + 2 * mana
            })
            .sum();
        let rules = &self.content.rules;
        let mut seen: BTreeMap<&str, i32> = BTreeMap::new();
        let mut rank = BTreeMap::new();
        for (id, def) in rules.spells.iter() {
            let n = seen.entry(def.realm.as_str()).or_insert(0);
            *n += 1;
            rank.insert(id, *n);
        }
        let research: i32 = self
            .magic(player)
            .map_or(0, |m| m.known.iter().filter_map(|s| rank.get(s)).sum());
        Astrology {
            power: (self.income(player).power_base - 10).max(0),
            strength: upkeep / 5,
            research: research / 8,
        }
    }

    /// Every wizard's scores, each column cut back so its highest is 200 at most (what the
    /// Astrologer screen draws), and the power for the history: the three scores and the
    /// population over seven, the highest cut to 160.
    pub fn astrology_table(&self) -> Vec<(PlayerId, Astrology, u8)> {
        let wizards: Vec<PlayerId> = self
            .players
            .iter()
            .map(|p| p.id)
            .filter(|&id| self.is_wizard(id))
            .collect();
        let rows: Vec<(PlayerId, Astrology, i32)> = wizards
            .iter()
            .map(|&id| {
                let a = self.astrology(id);
                let pops: i32 = self
                    .economy()
                    .cities()
                    .iter()
                    .filter(|(_, c, _)| c.owner == id)
                    .map(|(e, ..)| {
                        self.world
                            .get::<&crate::economy::Population>(*e)
                            .map_or(0, |p| p.figures as i32)
                    })
                    .sum::<i32>()
                    / 10;
                (id, a, a.power + a.strength + a.research + pops)
            })
            .collect();
        let top = |f: fn(&Astrology) -> i32| rows.iter().map(|r| f(&r.1)).max().unwrap_or(0);
        let (max_power, max_strength, max_research) =
            (top(|a| a.power), top(|a| a.strength), top(|a| a.research));
        let cut = |v: i32, max: i32| if max > 200 { v * 200 / max } else { v };
        let max_total = rows.iter().map(|r| r.2).max().unwrap_or(0) / 7;
        rows.iter()
            .map(|&(id, a, total)| {
                let mut power = total / 7;
                if max_total > 160 {
                    power = power * 160 / max_total;
                }
                (
                    id,
                    Astrology {
                        power: cut(a.power, max_power),
                        strength: cut(a.strength, max_strength),
                        research: cut(a.research, max_research),
                    },
                    power.clamp(0, 255) as u8,
                )
            })
            .collect()
    }

    /// Add this turn's power to every wizard's history.
    pub(crate) fn record_history(&mut self) {
        for (id, _, power) in self.astrology_table() {
            if let Some(p) = self.players.iter_mut().find(|p| p.id == id) {
                if p.history.len() >= HISTORY_TURNS {
                    p.history.remove(0);
                }
                p.history.push(power);
            }
        }
    }
}

#[cfg(test)]
mod tests;
