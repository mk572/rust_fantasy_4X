//! What a soak game measures about each wizard: counts taken from the game's events as they
//! happen, and the standing at a given moment. Used by `soak` (one line per game) and
//! `soak-versus` (the AI versions side by side).

use std::collections::BTreeSet;
use std::ops::AddAssign;

use f4x_core::city::production::ProductionItem;
use f4x_core::diplomacy::{DiplomacyEvent, Proposal};
use f4x_core::game::{Event, Game};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::player::PlayerId;
use f4x_core::rules::{MovementType, Stat, UnitDef};
use serde::{Deserialize, Serialize};

/// Whether a unit kind is a ship: it sails and carries land units.
fn is_ship(def: &UnitDef) -> bool {
    def.movement == MovementType::Sailing && def.stats.get(&Stat::Transport).is_some_and(|&t| t > 0)
}

/// Counts over a whole game for one wizard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Wizards the player has met.
    pub contacts_made: u32,
    /// Diplomatic actions the player started: proposals, warnings, gifts, broken treaties and wars.
    pub diplomacy_started: u32,
    pub refused_orders: u32,
    pub ships_built: u32,
    pub ships_lost: u32,
    /// Land units that stepped from a ship onto land.
    pub troops_landed: u32,
    /// Cities taken on a landmass where the wizard had landed troops.
    pub cities_by_sea: u32,
    /// 1 when the wizard landed troops at all.
    pub wizards_landed: u32,
    /// The turn of the wizard's first landing (summed over wizards, see `first_landing`).
    pub first_landing_turns: u32,
}

impl Metrics {
    /// Each count with its name, in the order a report shows them.
    pub fn rows(&self) -> [(&'static str, u32); 23] {
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
            ("wizards met", self.contacts_made),
            ("diplomacy started", self.diplomacy_started),
            ("refused orders", self.refused_orders),
            ("ships built", self.ships_built),
            ("ships lost", self.ships_lost),
            ("troops landed", self.troops_landed),
            ("cities by sea", self.cities_by_sea),
            ("wizards landed", self.wizards_landed),
        ]
    }

    /// The average turn of the first landing over the wizards who landed.
    pub fn first_landing(&self) -> Option<f64> {
        (self.wizards_landed > 0)
            .then(|| f64::from(self.first_landing_turns) / f64::from(self.wizards_landed))
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
            contacts_made,
            diplomacy_started,
            refused_orders,
            ships_built,
            ships_lost,
            troops_landed,
            cities_by_sea,
            wizards_landed,
            first_landing_turns,
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
        self.contacts_made += contacts_made;
        self.diplomacy_started += diplomacy_started;
        self.refused_orders += refused_orders;
        self.ships_built += ships_built;
        self.ships_lost += ships_lost;
        self.troops_landed += troops_landed;
        self.cities_by_sea += cities_by_sea;
        self.wizards_landed += wizards_landed;
        self.first_landing_turns += first_landing_turns;
    }
}

/// The counts of every player of a game, by player number. The last slot holds whoever has no
/// number among the players (the monsters that win a battle, say), so the totals add up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tally {
    players: Vec<Metrics>,
    /// The ships each player had when the game began.
    ships_at_start: Vec<u32>,
    /// The landmasses (plane, number) each player has landed troops on.
    landed_on: Vec<BTreeSet<(usize, u32)>>,
}

impl Tally {
    pub fn new(game: &Game) -> Self {
        let count = game
            .player_ids()
            .into_iter()
            .map(|p| usize::from(p.0) + 1)
            .max()
            .unwrap_or(0);
        let mut tally = Self {
            players: vec![Metrics::default(); count + 1],
            ships_at_start: vec![0; count + 1],
            landed_on: vec![BTreeSet::new(); count + 1],
        };
        tally.ships_at_start = tally.ships_now(game);
        tally
    }

    /// The ships of each player, by slot.
    fn ships_now(&self, game: &Game) -> Vec<u32> {
        let mut ships = vec![0; self.players.len()];
        for p in game.player_ids() {
            let slot = usize::from(p.0).min(ships.len() - 1);
            ships[slot] += game
                .units()
                .player_units(p)
                .iter()
                .filter(|&&u| game.units().unit_def(u).is_some_and(is_ship))
                .count() as u32;
        }
        ships
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
                Event::CityCaptured { city, from, to } => {
                    let by_sea = game
                        .economy()
                        .cities()
                        .iter()
                        .find(|(e, ..)| e == city)
                        .is_some_and(|(.., pos)| {
                            self.landed_on.get(usize::from(to.0)).is_some_and(|l| {
                                l.contains(&(pos.plane.index(), game.landmass_of(*pos)))
                            })
                        });
                    if let Some(m) = self.at(*to) {
                        m.cities_captured += 1;
                        m.cities_by_sea += u32::from(by_sea);
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
                        ProductionItem::Unit(unit) => {
                            m.units_built += 1;
                            m.ships_built +=
                                u32::from(is_ship(game.content().rules.units.get(*unit)));
                        }
                        ProductionItem::TradeGoods | ProductionItem::Housing => {}
                    }
                }
                Event::UnitsMoved { units, from, to } => {
                    let sea = |at| game.economy().terrain_class_at(at) == Some(TerrainClass::Ocean);
                    let Some(player) = units.first().and_then(|&u| game.units().unit_owner(u))
                    else {
                        continue;
                    };
                    let landing = sea(*from) && !sea(*to);
                    let troops = units
                        .iter()
                        .filter(|&&u| game.units().unit_def(u).is_some_and(|d| !is_ship(d)))
                        .count() as u32;
                    if !landing || troops == 0 {
                        continue;
                    }
                    let mass = (to.plane.index(), game.landmass_of(*to));
                    let Some(slot) = self.landed_on.get_mut(usize::from(player.0)) else {
                        continue;
                    };
                    slot.insert(mass);
                    let first = game.turn;
                    if let Some(m) = self.at(player) {
                        m.troops_landed += troops;
                        if m.wizards_landed == 0 {
                            m.wizards_landed = 1;
                            m.first_landing_turns = first;
                        }
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
                    self.bump(*attacker, |m| {
                        m.wars_declared += 1;
                        m.diplomacy_started += 1;
                    })
                }
                Event::Diplomacy(DiplomacyEvent::ContactMade { a, b }) => {
                    self.bump(*a, |m| m.contacts_made += 1);
                    self.bump(*b, |m| m.contacts_made += 1);
                }
                Event::Diplomacy(
                    DiplomacyEvent::ProposalMade(Proposal { from, .. })
                    | DiplomacyEvent::Warned { from, .. }
                    | DiplomacyEvent::GoldGiven { from, .. }
                    | DiplomacyEvent::SpellGiven { from, .. }
                    | DiplomacyEvent::TreatyBroken { breaker: from, .. },
                ) => self.bump(*from, |m| m.diplomacy_started += 1),
                Event::Rejected(_) => {
                    if let Some(p) = source {
                        self.bump(p, |m| m.refused_orders += 1);
                    }
                }
                _ => {}
            }
        }
        // A ship leaves no event of its own when a battle or the sea takes it, so count what is
        // missing from the fleet: the ones it started with and built, less those still afloat.
        let now = self.ships_now(game);
        for (slot, m) in self.players.iter_mut().enumerate() {
            m.ships_lost = (self.ships_at_start[slot] + m.ships_built).saturating_sub(now[slot]);
        }
    }

    fn bump(&mut self, player: PlayerId, change: impl FnOnce(&mut Metrics)) {
        if let Some(m) = self.at(player) {
            change(m);
        }
    }
}

/// How a wizard stands at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    use crate::testutil::shipped;
    use f4x_core::game::setup::{new_game, NewGame};
    use f4x_core::testdir::TestResult;

    /// A real two-wizard game: every city a wizard has at the end was there at the start, was
    /// founded or was taken, and none that was lost is still there.
    #[test]
    fn founded_cities_account_for_the_cities_a_wizard_ends_with() -> TestResult {
        let mut game = new_game(
            shipped(),
            &NewGame {
                seed: 1,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?;
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
        Ok(())
    }

    #[test]
    fn contacts_count_for_both_wizards_and_only_the_one_who_acts_starts_diplomacy() -> TestResult {
        let game = new_game(
            shipped(),
            &NewGame {
                seed: 1,
                all_ai: true,
                ..NewGame::with_wizards(2)
            },
        )?;
        let (a, b) = (PlayerId(0), PlayerId(1));
        let mut tally = Tally::new(&game);
        tally.record(
            &game,
            None,
            &[
                Event::Diplomacy(DiplomacyEvent::ContactMade { a, b }),
                Event::Diplomacy(DiplomacyEvent::Warned { from: a, to: b }),
                Event::Diplomacy(DiplomacyEvent::WarDeclared {
                    attacker: a,
                    defender: b,
                }),
            ],
        );
        assert_eq!(
            (tally.of(a).contacts_made, tally.of(b).contacts_made),
            (1, 1)
        );
        assert_eq!(tally.of(a).diplomacy_started, 2);
        assert_eq!(tally.of(b).diplomacy_started, 0);
        Ok(())
    }

    #[test]
    fn only_a_sailing_unit_with_room_is_a_ship() {
        let ships: Vec<_> = shipped()
            .rules
            .units
            .iter()
            .map(|(_, d)| d)
            .filter(|d| is_ship(d))
            .collect();
        assert!(ships.iter().any(|d| d.id == "galley"));
        assert!(ships.iter().all(|d| d.movement == MovementType::Sailing));
        assert!(!ships.iter().any(|d| d.id == "spearmen"));
    }
}
