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
//! [`Command::RazeCity`]: crate::game::Command::RazeCity

use crate::city::economy::{Buildings, Economy, Enchantments, Population};
use crate::city::production::{Production, ProductionItem};
use crate::city::City;
use crate::game::rng::GameRng;
use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::BuildingId;

/// Marks a city taken this turn by the player it holds, who may still raze it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conquered(pub hecs::Entity);

/// A city's size class: 1 (hamlet) for up to four citizens, one more for each four, at most 5;
/// 0 for an outpost.
pub fn city_size_class(citizens: u32) -> u32 {
    citizens.div_ceil(4).min(5)
}

/// The fame the loser loses and the winner gains when a city of size class `size` is kept:
/// the size, and the size less two (not below nothing).
pub(crate) fn fame_change(size: i32) -> (i32, i32) {
    (size, (size - 2).max(0))
}

/// The share of the loser's gold that a city loots: the treasury (not below nothing) times this
/// city's citizens over all the loser's citizens.
pub(crate) fn loot_share(gold: i32, citizens: u32, all_citizens: u32) -> i32 {
    if all_citizens == 0 {
        return 0;
    }
    (i64::from(gold.max(0)) * i64::from(citizens) / i64::from(all_citizens)) as i32
}

/// What the fighting cost a city.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Toll {
    pub citizens_lost: u32,
    pub buildings_lost: u32,
}

/// The fighting kills citizens and wrecks buildings: every citizen but one dies with a chance of
/// 10 percent plus twice the city damage (at most 50), and every building dies with a chance of
/// the damage plus 10 percent (at most 75) unless `destroy` says another building needs it. One
/// draw per citizen but one, then one per building, in that order.
pub(crate) fn fighting_toll(
    citizens: u32,
    city_damage: u32,
    built: &[BuildingId],
    rng: &mut GameRng,
    mut destroy: impl FnMut(BuildingId) -> bool,
) -> Toll {
    let damage = city_damage as i32;
    let percent = (damage * 2 + 10).min(50);
    let mut citizens_lost = 0;
    for _ in 0..citizens.saturating_sub(1) {
        if rng.roll(100) as i32 <= percent {
            citizens_lost += 1;
        }
    }
    let chance = (damage + 10).min(75);
    let mut buildings_lost = 0;
    for &b in built {
        if rng.roll(100) as i32 <= chance && destroy(b) {
            buildings_lost += 1;
        }
    }
    Toll {
        citizens_lost,
        buildings_lost,
    }
}

/// The gold razing a city brings: a tenth of the cost of each building.
pub(crate) fn razing_loot(costs: impl IntoIterator<Item = u32>) -> i32 {
    costs.into_iter().map(|c| (c / 10) as i32).sum()
}

/// The fame razing a city costs: the size class plus one.
pub(crate) fn razing_fame_loss(size: i32) -> i32 {
    size + 1
}

/// Destroy a building, unless another that is built needs it (also down a chain). Returns
/// whether it went.
pub(crate) fn destroy_building(
    world: &mut hecs::World,
    map: Option<&crate::map::Map>,
    content: &crate::content::Content,
    city: hecs::Entity,
    building: BuildingId,
) -> bool {
    if Economy::new(world, map, content).has_dependants(city, building) {
        return false;
    }
    match world.get::<&mut Buildings>(city) {
        Ok(mut b) => {
            let before = b.0.len();
            b.0.retain(|&x| x != building);
            b.0.len() < before
        }
        Err(_) => false,
    }
}

impl Economy<'_> {
    pub fn citizens(&self, city: hecs::Entity) -> u32 {
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
        let gold = crate::player::component::<crate::player::Treasury>(self.world, owner)
            .map_or(0, |t| t.gold);
        loot_share(gold, self.citizens(city), all)
    }
}

impl Game {
    /// `winner` takes `city` after the fighting, which did `city_damage` to it.
    pub fn conquer_city(
        &mut self,
        city: hecs::Entity,
        winner: PlayerId,
        city_damage: u32,
    ) -> Vec<Event> {
        let Some(loser) = self.economy().city_owner(city) else {
            return Vec::new();
        };
        if loser == winner {
            return Vec::new();
        }
        let citizens = self.economy().citizens(city);
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
        self.city_taken_relations(winner, loser);
        let size = city_size_class(citizens) as i32;
        let (fame_lost, fame_gained) = fame_change(size);
        let loot = self.economy().conquest_loot(city);
        let loser_is_wizard = !self.is_neutral_player(loser);
        if loser_is_wizard {
            self.add_fame(loser, -fame_lost);
        }
        self.add_fame(winner, fame_gained);
        if let Some(t) = self.treasury_mut(winner) {
            t.gold += loot;
        }
        if let Some(t) = self.treasury_mut(loser) {
            t.gold = (t.gold - loot).max(0);
        }

        // The fighting kills citizens and wrecks buildings.
        let built: Vec<_> = self
            .world
            .get::<&Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        let mut lost = Vec::new();
        let toll = fighting_toll(citizens, city_damage, &built, &mut self.rng, |b| {
            let gone = destroy_building(&mut self.world, self.map.as_ref(), &self.content, city, b);
            if gone {
                lost.push(b);
            }
            gone
        });
        if let Ok(mut pop) = self.world.get::<&mut Population>(city) {
            pop.figures -= toll.citizens_lost;
        }

        let mut events = vec![Event::CityLooted {
            city,
            gold: loot,
            citizens_lost: toll.citizens_lost,
            buildings_lost: lost,
            fame_lost: if loser_is_wizard { fame_lost } else { 0 },
            fame_gained,
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
        let farmers = self.economy().city_min_farmers(city);
        let _ = self
            .world
            .insert_one(city, crate::city::economy::Farmers(farmers));
        if let Some(by) = self.player_entity(winner) {
            let _ = self.world.insert_one(city, Conquered(by));
        }
        events
    }

    /// Destroy a building, unless another that is built needs it (also down a chain).
    pub(crate) fn destroy_building(&mut self, city: hecs::Entity, building: BuildingId) -> bool {
        destroy_building(
            &mut self.world,
            self.map.as_ref(),
            &self.content,
            city,
            building,
        )
    }

    /// The winner razes the city they have just taken: more fame lost, a tenth of the
    /// buildings' cost looted, and the city is gone.
    pub fn raze_city(&mut self, player: PlayerId, city: hecs::Entity) -> Vec<Event> {
        let Some(Conquered(by)) = self.world.get::<&Conquered>(city).ok().map(|c| *c) else {
            return vec![Event::Rejected(crate::game::Rejection::Raze(
                RazeError::NotJustTaken,
            ))];
        };
        if self.player_entity(player) != Some(by) || self.economy().city_owner(city) != Some(player)
        {
            return vec![Event::Rejected(crate::game::Rejection::Raze(
                RazeError::NotJustTaken,
            ))];
        }
        let size = city_size_class(self.economy().citizens(city)) as i32;
        let rules = &self.content.rules;
        let loot = self
            .world
            .get::<&Buildings>(city)
            .map(|b| razing_loot(b.0.iter().map(|&b| rules.buildings.get(b).cost)))
            .unwrap_or(0);
        let pos: Option<Position> = self.world.get::<&Position>(city).map(|p| *p).ok();
        let fame_lost = razing_fame_loss(size);
        self.add_fame(player, -fame_lost);
        if let Some(t) = self.treasury_mut(player) {
            t.gold += loot;
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
        events.push(Event::CityRazed {
            player,
            gold: loot,
            fame_lost,
        });
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

#[cfg(test)]
mod tests;
