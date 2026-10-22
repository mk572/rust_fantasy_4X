//! The budget (task 188): what a player takes in and pays out at the turn change, and who is
//! let go when the gold or the food runs short. The sums are plain functions over figures
//! ([`Budget::compute`], [`settle`]), so they run in a test without a game; the `Game` side
//! gathers the figures from the cities and the troops and applies the result. The taxes and
//! farmers that balance an independent city's books are here too.

use crate::city::economy::{Farmers, Population};
use crate::game::{Event, Game};
use crate::player::{PlayerId, Tax};
use crate::rules::UnitKind;

/// A player's projected gold and food for the coming turn change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Budget {
    /// Taxes, trade and mines from all cities.
    pub city_gold: i32,
    pub building_upkeep: i32,
    pub noble_gold: i32,
    /// Gold from selling food the cities grow beyond what citizens and troops eat.
    pub food_gold: i32,
    /// Gold paid for troops, after fame.
    pub unit_upkeep: i32,
    pub food_surplus: i32,
    pub food_upkeep: i32,
}

/// What one city adds to the treasury.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CityYield {
    pub gold: i32,
    pub gold_upkeep: i32,
    pub food: i32,
    /// Citizens, who eat one food each.
    pub figures: i32,
}

/// What one unit costs the treasury.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Troop {
    pub id: hecs::Entity,
    pub food: i32,
    pub gold: i32,
    /// A noble hero brings in gold instead of costing it.
    pub noble_hero: bool,
}

impl Budget {
    /// Net change in gold.
    pub fn net_gold(&self) -> i32 {
        self.city_gold - self.building_upkeep + self.noble_gold + self.food_gold - self.unit_upkeep
    }

    /// The treasury from the player's cities and troops (in `player_units` order) and fame,
    /// which pays part of the troops' gold.
    pub fn compute(cities: &[CityYield], troops: &[Troop], fame: i32) -> Budget {
        let mut t = Budget::default();
        let mut sold_food_gold = 0;
        for city in cities {
            t.city_gold += city.gold;
            t.building_upkeep += city.gold_upkeep;
            let excess = (city.food - city.figures).max(0);
            sold_food_gold += excess / 2;
            t.food_surplus += excess;
        }
        t.noble_gold = 10 * troops.iter().filter(|u| u.noble_hero).count() as i32;
        t.food_upkeep = troops.iter().map(|u| u.food).sum();
        // Spare food is sold at two for one gold once the army has eaten.
        t.food_gold = (sold_food_gold - t.food_upkeep / 2).max(0);
        t.unit_upkeep = (troops.iter().map(|u| u.gold).sum::<i32>() - fame).max(0);
        t
    }
}

/// The outcome of a turn change for one player's gold and army.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    /// Units let go, the newest first.
    pub disbanded: Vec<hecs::Entity>,
    /// How many of the first `disbanded` went for lack of food; the rest, for lack of gold.
    pub for_food: usize,
    /// The gold left after income and upkeep.
    pub gold: i32,
}

/// The gold on hand once the turn's income and the buildings' upkeep are counted (not below 0).
pub fn gold_after_income(t: &Budget, gold: i32) -> i32 {
    (gold + t.city_gold - t.building_upkeep + t.noble_gold + t.food_gold).max(0)
}

/// Income, food and gold upkeep: the income is added, an army that eats more than the cities
/// grow is disbanded newest first, then more units go (newest first) until the gold covers the
/// upkeep of those left.
pub fn settle(t: &Budget, gold: i32, fame: i32, troops: &[Troop]) -> Settlement {
    let gold = gold_after_income(t, gold);
    let mut disbanded = Vec::new();
    let mut food_upkeep = t.food_upkeep;
    for unit in troops.iter().rev() {
        if food_upkeep <= t.food_surplus {
            break;
        }
        if unit.food > 0 {
            food_upkeep -= unit.food;
            disbanded.push(unit.id);
        }
    }
    let for_food = disbanded.len();
    let upkeep_without = |gone: &[hecs::Entity]| -> i32 {
        let total: i32 = troops
            .iter()
            .filter(|u| !gone.contains(&u.id))
            .map(|u| u.gold)
            .sum();
        (total - fame).max(0)
    };
    let mut upkeep = upkeep_without(&disbanded);
    for unit in troops.iter().rev() {
        if upkeep <= gold {
            break;
        }
        if !disbanded.contains(&unit.id) && unit.gold > 0 {
            disbanded.push(unit.id);
            upkeep = upkeep_without(&disbanded);
        }
    }
    Settlement {
        disbanded,
        for_food,
        gold: (gold - upkeep).max(0),
    }
}

impl Game {
    /// Set the tax rate to the lowest that leaves the treasury no worse off each turn, or to the
    /// best there is when none does. How an independent city is run; wizards choose their own.
    pub(crate) fn balance_taxes(&mut self, player: PlayerId) {
        let rates = self.content.rules.config.tax_unrest_percent.len() as u8;
        let Some(start) = self.player_component::<Tax>(player).map(|t| t.0) else {
            return;
        };
        let mut best = (i32::MIN, start);
        for rate in 0..rates {
            if let Some(t) = self.player_component_mut::<Tax>(player) {
                t.0 = rate;
            }
            let net = self.budget(player).net_gold();
            if net >= 0 {
                best = (net, rate);
                break;
            }
            if net > best.0 {
                best = (net, rate);
            }
        }
        if let Some(t) = self.player_component_mut::<Tax>(player) {
            t.0 = best.1;
        }
    }

    /// Put just enough citizens on farms that the player's cities feed their citizens
    /// and their army, preferring the cities that farm best. Does nothing where there
    /// is not enough land: those cities farm with everyone.
    pub fn balance_farmers(&mut self, player: PlayerId) {
        let mut mine: Vec<hecs::Entity> = self
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == player)
            .map(|(e, ..)| e)
            .collect();
        self.sort_by_serial(&mut mine);
        for &city in &mine {
            let min = self.economy().city_min_farmers(city);
            let _ = self.world.insert_one(city, Farmers(min));
        }
        let units = self.units();
        let upkeep: i32 = units
            .player_units(player)
            .iter()
            .map(|&u| units.unit_food_upkeep(u))
            .sum();
        // Each extra farmer yields food beyond what he eats; add them where it pays most.
        loop {
            let surplus = self.budget(player).food_surplus;
            if surplus >= upkeep {
                break;
            }
            let best = mine
                .iter()
                .copied()
                .filter_map(|c| {
                    let farmers = self.world.get::<&Farmers>(c).map_or(0, |f| f.0);
                    let figures = self.world.get::<&Population>(c).map_or(0, |p| p.figures);
                    if farmers >= figures {
                        return None;
                    }
                    let now = self.economy().city_economy_with(c, farmers)?;
                    let more = self.economy().city_economy_with(c, farmers + 1)?;
                    Some((more.food - now.food, c, farmers))
                })
                .filter(|(gain, ..)| *gain > 0)
                .max_by_key(|(gain, c, _)| (*gain, std::cmp::Reverse(self.serial_of(*c))));
            let Some((_, city, farmers)) = best else {
                break;
            };
            let _ = self.world.insert_one(city, Farmers(farmers + 1));
        }
    }

    /// The figures of a player's troops that the treasury needs, in the order of `player_units`.
    pub(crate) fn troops(&self, player: PlayerId) -> Vec<Troop> {
        let units = self.units();
        units
            .player_units(player)
            .into_iter()
            .map(|id| Troop {
                id,
                food: units.unit_food_upkeep(id),
                gold: units.unit_gold_upkeep(id),
                noble_hero: units.unit_has_ability(id, "noble")
                    && units.unit_def(id).is_some_and(|d| d.kind == UnitKind::Hero),
            })
            .collect()
    }

    /// What a player will take in and pay out at the next turn change.
    pub fn budget(&self, player: PlayerId) -> Budget {
        let cities: Vec<CityYield> = self
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == player)
            .filter_map(|(city, ..)| self.economy().city_economy(city))
            .map(|eco| CityYield {
                gold: eco.gold,
                gold_upkeep: eco.gold_upkeep,
                food: eco.food,
                figures: eco.figures as i32,
            })
            .collect();
        let fame = self.fame(player);
        Budget::compute(&cities, &self.troops(player), fame)
    }

    /// Income, food and gold upkeep for one player; units go when they cannot be paid for
    /// (`release_one`: heroes come home, ships with passengers sail for the shore).
    pub(crate) fn settle_treasury(&mut self, player: PlayerId) -> Vec<Event> {
        let t = self.budget(player);
        let troops = self.troops(player);
        let fame = self.fame(player);
        let Some(treasury) = self.treasury_mut(player) else {
            return Vec::new();
        };
        let settled = settle(&t, treasury.gold, fame, &troops);
        treasury.gold = settled.gold;
        let mut events = Vec::new();
        let short = |cause: crate::game::LossCause| Event::Shortage {
            player,
            what: match cause {
                crate::game::LossCause::Food => crate::game::Shortage::Food,
                _ => crate::game::Shortage::Gold,
            },
        };
        if settled.for_food > 0 {
            events.push(short(crate::game::LossCause::Food));
        }
        if settled.disbanded.len() > settled.for_food {
            events.push(short(crate::game::LossCause::Gold));
        }
        for (i, unit) in settled.disbanded.iter().enumerate() {
            let cause = if i < settled.for_food {
                crate::game::LossCause::Food
            } else {
                crate::game::LossCause::Gold
            };
            events.extend(self.release_one(player, *unit, cause));
        }
        if settled.disbanded.is_empty() {
            return Vec::new();
        }
        events.push(Event::UnitsDisbanded {
            player,
            units: settled.disbanded.len() as u32,
        });
        events
    }
}

#[cfg(test)]
mod tests;
