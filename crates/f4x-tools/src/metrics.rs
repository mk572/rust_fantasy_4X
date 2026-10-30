//! What a soak game measures about each wizard: counts taken from the game's events as they
//! happen, and the standing at a given moment. Used by `soak` (one line per game) and
//! `soak-versus` (the AI versions side by side).

use std::ops::AddAssign;

use f4x_core::city::production::ProductionItem;
use f4x_core::diplomacy::DiplomacyEvent;
use f4x_core::game::{Event, Game};
use f4x_core::player::PlayerId;

/// Counts over a whole game for one wizard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Metrics {
    pub cities_founded: u32,
    pub cities_captured: u32,
    pub cities_lost: u32,
    pub battles_won: u32,
    pub battles_lost: u32,
    pub units_built: u32,
    pub units_lost: u32,
    pub buildings_built: u32,
    pub spells_cast: u32,
    pub spells_learned: u32,
    pub heroes_joined: u32,
    pub mercenaries_hired: u32,
    pub items_made: u32,
    pub items_sold: u32,
    pub wars_declared: u32,
    pub refused_orders: u32,
}

impl Metrics {
    /// Each count with its name, in the order a report shows them.
    pub fn rows(&self) -> [(&'static str, u32); 16] {
        [
            ("cities founded", self.cities_founded),
            ("cities captured", self.cities_captured),
            ("cities lost", self.cities_lost),
            ("battles won", self.battles_won),
            ("battles lost", self.battles_lost),
            ("units built", self.units_built),
            ("units lost", self.units_lost),
            ("buildings built", self.buildings_built),
            ("spells cast", self.spells_cast),
            ("spells learned", self.spells_learned),
            ("heroes joined", self.heroes_joined),
            ("mercenaries hired", self.mercenaries_hired),
            ("items made", self.items_made),
            ("items sold", self.items_sold),
            ("wars declared", self.wars_declared),
            ("refused orders", self.refused_orders),
        ]
    }
}

impl AddAssign for Metrics {
    fn add_assign(&mut self, other: Metrics) {
        let Metrics {
            cities_founded,
            cities_captured,
            cities_lost,
            battles_won,
            battles_lost,
            units_built,
            units_lost,
            buildings_built,
            spells_cast,
            spells_learned,
            heroes_joined,
            mercenaries_hired,
            items_made,
            items_sold,
            wars_declared,
            refused_orders,
        } = other;
        self.cities_founded += cities_founded;
        self.cities_captured += cities_captured;
        self.cities_lost += cities_lost;
        self.battles_won += battles_won;
        self.battles_lost += battles_lost;
        self.units_built += units_built;
        self.units_lost += units_lost;
        self.buildings_built += buildings_built;
        self.spells_cast += spells_cast;
        self.spells_learned += spells_learned;
        self.heroes_joined += heroes_joined;
        self.mercenaries_hired += mercenaries_hired;
        self.items_made += items_made;
        self.items_sold += items_sold;
        self.wars_declared += wars_declared;
        self.refused_orders += refused_orders;
    }
}

/// The counts of every player of a game, by player number. The last slot holds whoever has no
/// number among the players (the monsters that win a battle, say), so the totals add up.
#[derive(Debug, Clone)]
pub struct Tally {
    players: Vec<Metrics>,
}

impl Tally {
    pub fn new(game: &Game) -> Self {
        let count = game
            .player_ids()
            .into_iter()
            .map(|p| usize::from(p.0) + 1)
            .max()
            .unwrap_or(0);
        Self {
            players: vec![Metrics::default(); count + 1],
        }
    }

    pub fn of(&self, player: PlayerId) -> Metrics {
        self.players
            .get(usize::from(player.0))
            .copied()
            .unwrap_or_default()
    }

    /// The counts of all players together.
    pub fn total(&self) -> Metrics {
        let mut sum = Metrics::default();
        for m in &self.players {
            sum += *m;
        }
        sum
    }

    fn at(&mut self, player: PlayerId) -> Option<&mut Metrics> {
        let last = self.players.len() - 1;
        self.players.get_mut(usize::from(player.0).min(last))
    }

    /// Count `events`. `source` is the wizard whose orders caused them (a city founded or a
    /// building finished has no player of its own: its owner counts), none for the turn change.
    pub fn record(&mut self, game: &Game, source: Option<PlayerId>, events: &[Event]) {
        let owner = |city| game.economy().city_owner(city).or(source);
        for event in events {
            match event {
                Event::CityFounded { city, .. } => {
                    if let Some(m) = owner(*city).and_then(|p| self.at(p)) {
                        m.cities_founded += 1;
                    }
                }
                Event::CityCaptured { from, to, .. } => {
                    if let Some(m) = self.at(*to) {
                        m.cities_captured += 1;
                    }
                    if let Some(m) = self.at(*from) {
                        m.cities_lost += 1;
                    }
                }
                Event::BattleEnded { winner, loser } => {
                    if let Some(m) = self.at(*winner) {
                        m.battles_won += 1;
                    }
                    if let Some(m) = self.at(*loser) {
                        m.battles_lost += 1;
                    }
                }
                Event::ProductionCompleted { city, item } => {
                    let Some(m) = owner(*city).and_then(|p| self.at(p)) else {
                        continue;
                    };
                    match item {
                        ProductionItem::Building(_) => m.buildings_built += 1,
                        ProductionItem::Unit(_) => m.units_built += 1,
                        ProductionItem::TradeGoods | ProductionItem::Housing => {}
                    }
                }
                Event::UnitLost { player, .. } => self.bump(*player, |m| m.units_lost += 1),
                Event::SpellCast { player, .. } => self.bump(*player, |m| m.spells_cast += 1),
                Event::SpellLearned { player, .. } => self.bump(*player, |m| m.spells_learned += 1),
                Event::HeroJoined { player, .. } => self.bump(*player, |m| m.heroes_joined += 1),
                Event::MercenariesHired { player, .. } => {
                    self.bump(*player, |m| m.mercenaries_hired += 1)
                }
                Event::ItemMade { player, .. } => self.bump(*player, |m| m.items_made += 1),
                Event::ItemSold { player, .. } => self.bump(*player, |m| m.items_sold += 1),
                Event::Diplomacy(DiplomacyEvent::WarDeclared { attacker, .. }) => {
                    self.bump(*attacker, |m| m.wars_declared += 1)
                }
                Event::Rejected(_) => {
                    if let Some(p) = source {
                        self.bump(p, |m| m.refused_orders += 1);
                    }
                }
                _ => {}
            }
        }
    }

    fn bump(&mut self, player: PlayerId, change: impl FnOnce(&mut Metrics)) {
        if let Some(m) = self.at(player) {
            change(m);
        }
    }
}

/// How a wizard stands at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Standing {
    pub cities: usize,
    pub buildings: usize,
    pub units: usize,
    pub gold: i32,
    pub mana: i32,
    pub spells_known: usize,
    pub explored: usize,
    pub fame: i32,
    pub score: i32,
}

impl Standing {
    pub fn of(game: &Game, player: PlayerId) -> Self {
        let cities: Vec<_> = game
            .economy()
            .cities()
            .iter()
            .filter(|(_, c, _)| c.owner == player)
            .map(|(e, ..)| *e)
            .collect();
        let treasury = game.player(player).map(|p| p.treasury());
        Self {
            cities: cities.len(),
            buildings: cities.iter().map(|&c| game.city_buildings(c).len()).sum(),
            units: game.units().player_units(player).len(),
            gold: treasury.map_or(0, |t| t.gold),
            mana: treasury.map_or(0, |t| t.mana),
            spells_known: game.wizard(player).map_or(0, |w| w.known().len()),
            explored: game.explored_squares(player),
            fame: game.wizard(player).map_or(0, |w| w.fame()),
            score: game.score(player).total(),
        }
    }

    /// Each value with its name, in the order a report shows them.
    pub fn rows(&self) -> [(&'static str, i64); 9] {
        [
            ("cities", self.cities as i64),
            ("buildings", self.buildings as i64),
            ("units", self.units as i64),
            ("gold", i64::from(self.gold)),
            ("mana", i64::from(self.mana)),
            ("spells known", self.spells_known as i64),
            ("explored squares", self.explored as i64),
            ("fame", i64::from(self.fame)),
            ("score", i64::from(self.score)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contestant::{self, Version};
    use f4x_core::game::setup::{new_game, NewGame};

    /// A real two-wizard game: every city a wizard has at the end was there at the start, was
    /// founded or was taken, and none that was lost is still there.
    #[test]
    fn founded_cities_account_for_the_cities_a_wizard_ends_with() {
        let content = f4x_data::load_content_layers(&[std::path::Path::new("../../content")])
            .expect("content");
        let mut game = new_game(
            &content,
            &NewGame {
                seed: 1,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )
        .expect("game");
        let players = [PlayerId(0), PlayerId(1)];
        let start: Vec<usize> = players
            .iter()
            .map(|&p| Standing::of(&game, p).cities)
            .collect();
        let mut ais: Vec<_> = players
            .iter()
            .map(|&p| contestant::make(Version::New, p))
            .collect();
        let mut tally = Tally::new(&game);
        for _ in 0..40 {
            let (turns, rest) = contestant::play_round(&mut game, &mut ais, Version::New);
            for (player, events) in &turns {
                tally.record(&game, Some(*player), events);
            }
            tally.record(&game, None, &rest);
        }
        assert!(tally.total().cities_founded > 0, "nobody founded a city");
        for (k, &p) in players.iter().enumerate() {
            let m = tally.of(p);
            let ends = Standing::of(&game, p).cities;
            assert_eq!(
                start[k] + m.cities_founded as usize + m.cities_captured as usize
                    - m.cities_lost as usize,
                ends,
                "p{}",
                p.0
            );
        }
    }
}
