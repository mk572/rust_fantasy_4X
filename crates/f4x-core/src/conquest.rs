//! What happens to a city that falls: the fame and gold it changes, the citizens and buildings
//! the fighting costs, and the choice to raze it.
//!
//! The rules follow the original's `End_Of_Combat` and `Combat_Results_Scroll`:
//!
//! - **An outpost** (no citizens) is destroyed when taken.
//! - **Any other city is kept** at first. The loser loses fame equal to the city's size (1 for a
//!   hamlet to 5), the winner gains the size less two (not below nothing), and the winner
//!   loots a share of the loser's gold: the loser's treasury times this city's citizens over all
//!   the loser's citizens (an independent city has its own treasury, which is all of it).
//! - **The fighting hurts the city.** Every citizen but one dies with a chance of 10 percent
//!   plus twice the *city damage*, at most 50 percent, and every building that nothing else needs
//!   is destroyed with a chance of the city damage plus 10 percent, at most 75 percent. City
//!   damage is one for each of the winner's units standing inside the city at the start of each
//!   turn of the battle (`Battle::city_damage`), so a long siege inside the walls costs more.
//! - **The winner may then raze it** ([`Command::RazeCity`]) until their turn ends: another
//!   fame penalty of the size plus one, a tenth of the cost of each building as gold, and the
//!   city is gone. The wizard whose fortress it was is banished either way.
//!
//! [`Command::RazeCity`]: crate::Command::RazeCity

use crate::city::{City, PlayerId, Position};
use crate::economy::{Buildings, Enchantments, Population};
use crate::production::{Production, ProductionItem};
use crate::{Event, Game};

/// Marks a city taken this turn by the player it holds, who may still raze it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conquered(pub PlayerId);

/// A city's size class: 1 (hamlet) for up to four citizens, one more for each four, at most 5;
/// 0 for an outpost.
pub fn city_size_class(citizens: u32) -> u32 {
    citizens.div_ceil(4).min(5)
}

impl Game {
    fn citizens(&self, city: hecs::Entity) -> u32 {
        self.world.get::<&Population>(city).map_or(0, |p| p.figures)
    }

    /// How much gold taking `city` loots from its owner: the owner's gold in proportion to the
    /// city's share of their citizens.
    pub fn conquest_loot(&self, city: hecs::Entity) -> i32 {
        let Some(owner) = self.city_owner(city) else {
            return 0;
        };
        let all: u32 = self
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == owner)
            .map(|(e, ..)| self.citizens(e))
            .sum();
        let gold = self.player(owner).map_or(0, |p| p.gold).max(0);
        if all == 0 {
            return 0;
        }
        (i64::from(gold) * i64::from(self.citizens(city)) / i64::from(all)) as i32
    }

    /// `winner` takes `city` after the fighting, which did `city_damage` to it.
    pub fn conquer_city(
        &mut self,
        city: hecs::Entity,
        winner: PlayerId,
        city_damage: u32,
    ) -> Vec<Event> {
        let Some(loser) = self.city_owner(city) else {
            return Vec::new();
        };
        if loser == winner {
            return Vec::new();
        }
        let citizens = self.citizens(city);
        if citizens == 0 {
            // An outpost is destroyed, not held.
            let mut events = self.capture_city(city, winner);
            if let Ok(e) = self.destroy_city(city) {
                events.extend(e);
            } else {
                let _ = self.world.despawn(city);
            }
            return events;
        }
        let size = city_size_class(citizens) as i32;
        let loot = self.conquest_loot(city);
        let loser_is_wizard = !self.is_neutral_player(loser);
        if loser_is_wizard {
            if let Some(p) = self.player_mut(loser) {
                p.fame = (p.fame - size).max(0);
            }
        }
        if let Some(p) = self.player_mut(winner) {
            p.fame += (size - 2).max(0);
            p.gold += loot;
        }
        if let Some(p) = self.player_mut(loser) {
            p.gold = (p.gold - loot).max(0);
        }

        // The fighting kills citizens and wrecks buildings.
        let damage = city_damage as i32;
        let percent = (damage * 2 + 10).min(50);
        let mut lost = 0;
        for _ in 0..citizens.saturating_sub(1) {
            if self.roll(100) as i32 <= percent {
                lost += 1;
            }
        }
        if let Ok(mut pop) = self.world.get::<&mut Population>(city) {
            pop.figures -= lost;
        }
        let chance = (damage + 10).min(75);
        let mut destroyed = 0;
        let built: Vec<_> = self
            .world
            .get::<&Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        for b in built {
            if self.roll(100) as i32 <= chance && self.destroy_building(city, b) {
                destroyed += 1;
            }
        }

        let mut events = vec![Event::CityLooted {
            city,
            gold: loot,
            citizens_lost: lost,
            buildings_lost: destroyed,
        }];
        events.extend(self.capture_city(city, winner));
        // The old owner's spells on the city end, and the new owner starts afresh.
        if let Ok(mut en) = self.world.get::<&mut Enchantments>(city) {
            en.0.retain(|&(_, caster)| caster != loser);
        }
        let _ = self.world.insert_one(
            city,
            Production {
                item: Some(ProductionItem::TradeGoods),
                progress: 0,
            },
        );
        let farmers = self.city_min_farmers(city);
        let _ = self
            .world
            .insert_one(city, crate::economy::Farmers(farmers));
        let _ = self.world.insert_one(city, Conquered(winner));
        events
    }

    /// Destroy a building, unless another that is built needs it (also down a chain).
    fn destroy_building(&mut self, city: hecs::Entity, building: crate::rules::BuildingId) -> bool {
        let needed = self.has_dependants(city, building);
        if needed {
            return false;
        }
        match self.world.get::<&mut Buildings>(city) {
            Ok(mut b) => {
                let before = b.0.len();
                b.0.retain(|&x| x != building);
                b.0.len() < before
            }
            Err(_) => false,
        }
    }

    /// The winner razes the city they have just taken: more fame lost, a tenth of the
    /// buildings' cost looted, and the city is gone.
    pub fn raze_city(&mut self, player: PlayerId, city: hecs::Entity) -> Vec<Event> {
        let Some(Conquered(by)) = self.world.get::<&Conquered>(city).ok().map(|c| *c) else {
            return vec![Event::Rejected(crate::Rejection::Raze(
                RazeError::NotJustTaken,
            ))];
        };
        if by != player || self.city_owner(city) != Some(player) {
            return vec![Event::Rejected(crate::Rejection::Raze(
                RazeError::NotJustTaken,
            ))];
        }
        let size = city_size_class(self.citizens(city)) as i32;
        let rules = &self.content.rules;
        let loot: i32 = self
            .world
            .get::<&Buildings>(city)
            .map(|b| {
                b.0.iter()
                    .map(|&b| (rules.buildings.get(b).cost / 10) as i32)
                    .sum()
            })
            .unwrap_or(0);
        let pos: Option<Position> = self.world.get::<&Position>(city).map(|p| *p).ok();
        if let Some(p) = self.player_mut(player) {
            p.fame = (p.fame - (size + 1)).max(0);
            p.gold += loot;
        }
        let mut events = Vec::new();
        match self.destroy_city(city) {
            Ok(e) => events.extend(e),
            Err(_) => {
                let _ = self.world.despawn(city);
                if let Some(pos) = pos {
                    events.push(Event::CityDestroyed { pos });
                }
            }
        }
        events.push(Event::CityRazed { player, gold: loot });
        events
    }

    /// The window to raze a taken city closes when its taker's turn ends.
    pub(crate) fn close_raze_windows(&mut self) {
        let marked: Vec<hecs::Entity> = self
            .world
            .query::<(&City, &Conquered)>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        for e in marked {
            let _ = self.world.remove_one::<Conquered>(e);
        }
    }
}

/// Why a raze was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RazeError {
    /// The city was not taken by this player this turn.
    NotJustTaken,
}
