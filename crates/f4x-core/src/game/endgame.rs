//! How a game ends, and what it was worth (task 70; the rules are in `docs/game_end.md`).
//!
//! A game ends in one of two ways: the last wizard standing wins by **conquest** (or the human
//! wizard is defeated, which ends it at once), or a wizard finishes the **Spell of Mastery**.
//! Once it has ended every command is refused. A defeated wizard's belongings are cleared away
//! first (`aftermath`, task 82).

use serde::{Deserialize, Serialize};

use crate::game::{Event, Game};
use crate::player::PlayerId;

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
    /// Citizens in the wizard's cities, half a point each (in thousands, as the cities count).
    pub townsfolk: i32,
    /// The citizens themselves, before they are halved into points (the score screen names them).
    pub people: i32,
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
            people: 0,
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
        self.player_ids()
            .into_iter()
            .filter(|&id| self.is_wizard(id) && !self.is_defeated(id))
            .collect()
    }

    /// The final score of `player` as the game stands.
    /// The wizards `player` has beaten: those it defeated and, once it has cast the Spell of
    /// Mastery, every wizard still standing, who count as banished too.
    pub fn banished_wizards(&self, player: PlayerId) -> Vec<PlayerId> {
        let mastery = self
            .game_over
            .is_some_and(|g| g.kind == EndKind::SpellOfMastery && g.winner == player);
        self.player_ids()
            .into_iter()
            .filter(|&id| {
                self.defeat_by(id) == Some(player)
                    || (mastery && self.is_wizard(id) && id != player && !self.is_defeated(id))
            })
            .collect()
    }

    pub fn score(&self, player: PlayerId) -> Score {
        if !self.has_player(player) {
            return Score::default();
        }
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
                    .get::<&crate::city::economy::Population>(*e)
                    .map_or(0, |pop| pop.figures)
            })
            .sum();
        let conquests = self.banished_wizards(player).len();
        let time = if self.wizard(player).is_some_and(|w| w.fortress().is_some())
            && mastery_by.is_none_or(|w| w == player)
        {
            2000 - self.turn as i32 * 2
        } else {
            0
        };
        Score {
            spells: self.wizard(player).map_or(0, |w| w.known().len()) as i32,
            townsfolk: (townsfolk / 2) as i32,
            people: townsfolk as i32,
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

    /// The announcement that `player` has started casting `spell`, if it is the Spell of
    /// Mastery.
    pub(crate) fn mastery_announcement(
        &self,
        player: PlayerId,
        spell: crate::rules::SpellId,
    ) -> Vec<Event> {
        if self.spell_has_hook(spell, SPELL_OF_MASTERY) {
            vec![Event::MasteryStarted { caster: player }]
        } else {
            Vec::new()
        }
    }

    /// The wizards now casting the Spell of Mastery and the mana each still has to pay. Every
    /// wizard is told when one starts, so this is public knowledge.
    pub fn mastery_casters(&self) -> Vec<(PlayerId, i32)> {
        self.player_ids()
            .into_iter()
            .filter_map(|id| {
                let c = self.wizard(id)?.casting()?;
                self.spell_has_hook(c.spell, SPELL_OF_MASTERY)
                    .then_some((id, c.remaining))
            })
            .collect()
    }

    /// The Spell of Mastery is finished: the caster wins at once.
    pub fn win_by_mastery(&mut self, caster: PlayerId) -> Vec<Event> {
        self.finish(caster, EndKind::SpellOfMastery)
    }

    /// Called after a wizard is defeated: ends the game when the human wizard has fallen or
    /// only one wizard is left.
    pub(crate) fn check_conquest(&mut self, conqueror: PlayerId) -> Vec<Event> {
        let human_down = self
            .human()
            .is_some_and(|h| self.has_player(h) && self.is_defeated(h));
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
            self.remove_unit(u, crate::hero::roster::Fate::Gone);
        }
        // Their enchantments on other wizards' cities and units.
        for (city, ..) in self.economy().cities() {
            if let Ok(mut list) = self
                .world
                .get::<&mut crate::city::economy::Enchantments>(city)
            {
                list.0.retain(|&(_, caster)| caster != loser);
            }
        }
        for u in self.units().all_units() {
            if let Ok(mut list) = self.world.get::<&mut crate::unit::UnitEnchantments>(u) {
                list.0.retain(|&(_, caster)| caster != loser);
            }
        }
        if let Some(map) = self.map.as_mut() {
            for i in 0..map.nodes().len() {
                if let Some(n) = map.node_mut(i) {
                    if n.owner == Some(loser) {
                        n.owner = None;
                        n.guardian = false;
                    }
                }
            }
        }
        let loot = self.treasury(loser).map_or(0, |t| t.gold / 2);
        if let Some(t) = self.treasury_mut(loser) {
            t.gold -= loot;
        }
        if let Some(mut w) = self.wizard_mut(loser) {
            let mut magic = w.get_mut::<crate::wizard::magic::Magic>();
            magic.global.clear();
            magic.casting = None;
        }
        if let Some(t) = self.treasury_mut(winner) {
            t.gold += loot;
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
            people: 40,
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
