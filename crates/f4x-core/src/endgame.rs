//! How a game ends, and what it was worth (task 70; the rules are in `docs/game_end.md`).
//!
//! A game ends in one of two ways: the last wizard standing wins by **conquest** (or the human
//! wizard is defeated, which ends it at once), or a wizard finishes the **Spell of Mastery**.
//! Once it has ended every command is refused. A defeated wizard's belongings are cleared away
//! first (`aftermath`, task 82).

use serde::{Deserialize, Serialize};

use crate::city::PlayerId;
use crate::{Event, Game};

/// The custom spell hook of the Spell of Mastery.
pub const SPELL_OF_MASTERY: &str = "spell_of_mastery";

/// The neutral player and monsters are not wizards.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndKind {
    /// The winner took the last rival's last city, or the human wizard was defeated.
    Conquest,
    /// The winner finished casting the Spell of Mastery.
    SpellOfMastery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameOver {
    pub winner: PlayerId,
    pub kind: EndKind,
    /// The turn it ended on.
    pub turn: u32,
}

/// A wizard's final score and where it came from (`End_Of_Game_Score_Draw`). The total is scaled
/// by the game's score percent (the original's difficulty scale: 50% on Intro to 300%).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Score {
    /// Spells known, 1 point each.
    pub spells: i32,
    /// Citizens in the wizard's cities, half a point each.
    pub townsfolk: i32,
    /// Wizards defeated, 50 points each.
    pub conquests: i32,
    /// Fame, 2 points each.
    pub fame: i32,
    /// `2000 - 2 * turn` while the fortress stands (can be negative late in the game).
    pub time: i32,
    /// 250 for casting the Spell of Mastery.
    pub mastery: i32,
    /// The scale of the total, in percent (the game's `scoring.score_percent`).
    pub percent: i32,
}

impl Default for Score {
    fn default() -> Self {
        Self {
            spells: 0,
            townsfolk: 0,
            conquests: 0,
            fame: 0,
            time: 0,
            mastery: 0,
            percent: 100,
        }
    }
}

impl Score {
    pub fn total(&self) -> i32 {
        let sum =
            self.spells + self.townsfolk + self.conquests + self.fame + self.time + self.mastery;
        sum * self.percent / 100
    }
}

/// The original's difficulty scaling of the final score: Intro halves it, Easy takes three
/// quarters, Normal leaves it, Hard doubles and Impossible triples it (levels 0 to 4).
pub fn scale_for_difficulty(score: i32, level: u8) -> i32 {
    match level {
        0 => score / 2,
        1 => score * 3 / 4,
        3 => score * 2,
        4 => score * 3,
        _ => score,
    }
}

impl Game {
    /// Wizards that have not been defeated.
    pub fn living_wizards(&self) -> Vec<PlayerId> {
        self.players
            .iter()
            .filter(|p| self.is_wizard(p.id) && !p.defeated)
            .map(|p| p.id)
            .collect()
    }

    /// The final score of `player` as the game stands.
    pub fn score(&self, player: PlayerId) -> Score {
        let Some(p) = self.player(player) else {
            return Score::default();
        };
        let mastery_by = self
            .game_over
            .filter(|g| g.kind == EndKind::SpellOfMastery)
            .map(|g| g.winner);
        let townsfolk: u32 = self
            .economy()
            .cities()
            .iter()
            .filter(|(_, c, _)| c.owner == player)
            .map(|(e, ..)| {
                self.world
                    .get::<&crate::economy::Population>(*e)
                    .map_or(0, |pop| pop.figures)
            })
            .sum();
        let mut conquests = self
            .players
            .iter()
            .filter(|q| q.defeated_by == Some(player))
            .count();
        if mastery_by == Some(player) {
            // Casting the spell counts every wizard still standing as beaten.
            conquests += self
                .players
                .iter()
                .filter(|q| self.is_wizard(q.id) && q.id != player && !q.defeated)
                .count();
        }
        let time = if p.fortress.is_some() && mastery_by.is_none_or(|w| w == player) {
            2000 - self.turn as i32 * 2
        } else {
            0
        };
        Score {
            spells: p.magic.known.len() as i32,
            townsfolk: (townsfolk / 2) as i32,
            conquests: conquests as i32 * 50,
            fame: self.fame(player) * 2,
            time,
            mastery: if mastery_by == Some(player) { 250 } else { 0 },
            percent: i32::from(self.settings.scoring.score_percent),
        }
    }

    fn finish(&mut self, winner: PlayerId, kind: EndKind) -> Vec<Event> {
        if self.game_over.is_some() {
            return Vec::new();
        }
        let over = GameOver {
            winner,
            kind,
            turn: self.turn,
        };
        self.game_over = Some(over);
        vec![Event::GameOver {
            winner,
            kind,
            turn: self.turn,
        }]
    }

    /// The Spell of Mastery is finished: the caster wins at once.
    pub fn win_by_mastery(&mut self, caster: PlayerId) -> Vec<Event> {
        self.finish(caster, EndKind::SpellOfMastery)
    }

    /// Called after a wizard is defeated: ends the game when the human wizard has fallen or
    /// only one wizard is left.
    pub(crate) fn check_conquest(&mut self, conqueror: PlayerId) -> Vec<Event> {
        let human_down = self
            .human
            .and_then(|h| self.player(h))
            .is_some_and(|p| p.defeated);
        let living = self.living_wizards();
        if human_down {
            return self.finish(conqueror, EndKind::Conquest);
        }
        match living.as_slice() {
            [last] => self.finish(*last, EndKind::Conquest),
            [] => self.finish(conqueror, EndKind::Conquest),
            _ => Vec::new(),
        }
    }

    /// Clear away a defeated wizard's holdings (task 82): their heroes, creatures and remaining
    /// units go, their enchantments come off every city and unit, their spells stop, their
    /// nodes are freed, and the conqueror takes half their gold.
    pub(crate) fn aftermath(&mut self, loser: PlayerId, winner: PlayerId) {
        // A defeated wizard has no city left, so none of their units is garrisoned: they are
        // all dismissed (the original turns garrisoned normal units neutral first).
        for u in self.units().player_units(loser) {
            self.remove_unit(u, crate::roster::Fate::Gone);
        }
        // Their enchantments on other wizards' cities and units.
        for (city, ..) in self.economy().cities() {
            if let Ok(mut list) = self.world.get::<&mut crate::economy::Enchantments>(city) {
                list.0.retain(|&(_, caster)| caster != loser);
            }
        }
        for u in self.units().all_units() {
            if let Ok(mut list) = self.world.get::<&mut crate::units::UnitEnchantments>(u) {
                list.0.retain(|&(_, caster)| caster != loser);
            }
        }
        self.timers.retain(|t| t.caster != loser);
        if let Some(map) = self.map.as_mut() {
            for i in 0..map.nodes().len() {
                if let Some(n) = map.node_mut(i) {
                    if n.owner == Some(loser.0) {
                        n.owner = None;
                        n.guardian = false;
                    }
                }
            }
        }
        let loot = self.player(loser).map_or(0, |p| p.gold / 2);
        if let Some(p) = self.player_mut(loser) {
            p.gold -= loot;
            p.magic.global.clear();
            p.magic.casting = None;
            p.defeated_by = Some(winner);
        }
        if let Some(p) = self.player_mut(winner) {
            p.gold += loot;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_scales_the_score() {
        assert_eq!(scale_for_difficulty(1000, 0), 500);
        assert_eq!(scale_for_difficulty(1000, 1), 750);
        assert_eq!(scale_for_difficulty(1000, 2), 1000);
        assert_eq!(scale_for_difficulty(1000, 3), 2000);
        assert_eq!(scale_for_difficulty(1000, 4), 3000);
    }

    #[test]
    fn the_total_adds_every_part() {
        let s = Score {
            spells: 10,
            townsfolk: 20,
            conquests: 50,
            fame: 4,
            time: -6,
            mastery: 250,
            percent: 100,
        };
        assert_eq!(s.total(), 328);
        assert_eq!(Score { percent: 50, ..s }.total(), 164);
        assert_eq!(Score { percent: 300, ..s }.total(), 984);
    }
}
