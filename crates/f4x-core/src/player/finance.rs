//! The budget: what a player takes in and pays out at the turn change, and who is
//! let go when the gold or the food runs short. The sums are plain functions over figures
//! ([`Budget::compute`], [`settle`]), so they run in a test without a game; the `Game` side
//! gathers the figures from the cities and the troops and applies the result. The taxes and
//! farmers that balance an independent city's books are here too.
//!
//! Tests: in player/finance/tests.rs.

use crate::city::economy::{Farmers, Population};
use crate::game::{Event, Game};
use crate::player::{PlayerId, Tax};
use crate::rules::Ability;
use crate::rules::UnitKind;
use f4x_def::half_int::HalfInt;

/// The figures a kept budget is made of: see [`Game::kept_budget`].
pub(crate) struct BudgetParts {
    pub(crate) owner: PlayerId,
    cities: Vec<(hecs::Entity, CityYield)>,
    troops: Vec<Troop>,
    /// Cities that completed something since the last asking.
    dirty: Vec<hecs::Entity>,
}

/// Set `F4X_CHECK_BUDGET` to work the kept budget out afresh each time and compare.
fn check_budget() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("F4X_CHECK_BUDGET").is_some())
}

/// A player's projected gold and food for the coming turn change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Budget {
    /// Taxes, trade and mines from all cities.
    pub city_gold: HalfInt,
    pub building_upkeep: HalfInt,
    pub noble_gold: HalfInt,
    /// Gold from selling food the cities grow beyond what citizens and troops eat.
    pub food_gold: HalfInt,
    /// Gold paid for troops, after fame.
    pub unit_upkeep: HalfInt,
    pub food_surplus: HalfInt,
    pub food_upkeep: HalfInt,
}

/// What one city adds to the treasury.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CityYield {
    pub gold: HalfInt,
    pub gold_upkeep: HalfInt,
    pub food: HalfInt,
    /// Citizens, who eat one food each.
    pub figures: i32,
}

/// What one unit costs the treasury.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Troop {
    pub id: hecs::Entity,
    /// The food it eats (the unit table's `upkeep.food`).
    pub food: HalfInt,
    pub gold: HalfInt,
    /// A noble hero brings in gold instead of costing it.
    pub noble_hero: bool,
    /// What it cost to make (of two units that cost the same upkeep, the cheaper goes first).
    pub cost: u32,
    /// A hero, or a ship with passengers aboard: the last to be let go.
    pub last: bool,
}

impl Budget {
    /// Net change in gold.
    pub fn net_gold(&self) -> HalfInt {
        self.city_gold - self.building_upkeep + self.noble_gold + self.food_gold - self.unit_upkeep
    }

    /// The treasury from the player's cities and troops (in `player_units` order) and fame,
    /// which pays part of the troops' gold.
    pub fn compute(cities: &[CityYield], troops: &[Troop], fame: i32) -> Budget {
        let mut t = Budget::default();
        for city in cities {
            t.city_gold += city.gold;
            t.building_upkeep += city.gold_upkeep;
            let excess = (city.food - HalfInt::from_int(city.figures)).max(HalfInt::ZERO);
            t.food_surplus += excess;
        }
        t.noble_gold =
            HalfInt::from_int(10 * troops.iter().filter(|u| u.noble_hero).count() as i32);
        t.food_upkeep = troops.iter().map(|u| u.food).sum();
        // The empire's spare food is sold at two for one gold once the army has eaten: the original
        // rounded each city and the army down on their own, here only the total is rounded (to a
        // half gold).
        t.food_gold = HalfInt::from_raw(((t.food_surplus - t.food_upkeep).raw() / 2).max(0));
        t.unit_upkeep = (troops.iter().map(|u| u.gold).sum::<HalfInt>() - HalfInt::from_int(fame))
            .max(HalfInt::ZERO);
        t
    }
}

/// The outcome of a turn change for one player's gold and army.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    /// Units let go, in the order they went (`release_order`).
    pub disbanded: Vec<hecs::Entity>,
    /// How many of the first `disbanded` went for lack of food; the rest, for lack of gold.
    pub for_food: usize,
    /// The gold left after income and upkeep.
    pub gold: HalfInt,
}

/// The gold on hand once the turn's income and the buildings' upkeep are counted (not below 0).
pub fn gold_after_income(t: &Budget, gold: HalfInt) -> HalfInt {
    (gold + t.city_gold - t.building_upkeep + t.noble_gold + t.food_gold).max(HalfInt::ZERO)
}

/// The order in which units are let go for one kind of upkeep (`upkeep`: the food or the gold a
/// unit costs): the highest upkeep first, since those are the likely cause of the shortage, then
/// the cheapest to make, then the newest; heroes and loaded ships last. Indexes into `troops`.
pub fn release_order<K: Ord>(troops: &[Troop], upkeep: impl Fn(&Troop) -> K) -> Vec<usize> {
    let mut order: Vec<usize> = (0..troops.len()).collect();
    order.sort_by_key(|&i| {
        let t = &troops[i];
        (
            t.last,
            std::cmp::Reverse(upkeep(t)),
            t.cost,
            std::cmp::Reverse(i),
        )
    });
    order
}

/// Income, food and gold upkeep: the income is added, an army that eats more than the cities
/// grow is disbanded in `release_order` by food, then more units go in `release_order` by gold
/// until the gold covers the upkeep of those left.
pub fn settle(t: &Budget, gold: HalfInt, fame: i32, troops: &[Troop]) -> Settlement {
    let gold = gold_after_income(t, gold);
    let mut disbanded = Vec::new();
    let mut eating: HalfInt = troops.iter().map(|u| u.food).sum();
    for &i in &release_order(troops, |u| u.food) {
        let unit = &troops[i];
        if eating <= t.food_surplus {
            break;
        }
        if unit.food > HalfInt::ZERO {
            eating -= unit.food;
            disbanded.push(unit.id);
        }
    }
    let for_food = disbanded.len();
    let upkeep_without = |gone: &[hecs::Entity]| -> HalfInt {
        let total: HalfInt = troops
            .iter()
            .filter(|u| !gone.contains(&u.id))
            .map(|u| u.gold)
            .sum();
        (total - HalfInt::from_int(fame)).max(HalfInt::ZERO)
    };
    let mut upkeep = upkeep_without(&disbanded);
    for &i in &release_order(troops, |u| u.gold) {
        let unit = &troops[i];
        if upkeep <= gold {
            break;
        }
        if !disbanded.contains(&unit.id) && unit.gold > HalfInt::ZERO {
            disbanded.push(unit.id);
            upkeep = upkeep_without(&disbanded);
        }
    }
    Settlement {
        disbanded,
        for_food,
        gold: (gold - upkeep).max(HalfInt::ZERO),
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
        let mut best = (HalfInt::from_raw(i32::MIN), start);
        for rate in 0..rates {
            if let Some(t) = self.player_component_mut::<Tax>(player) {
                t.0 = rate;
            }
            let net = self.budget(player).net_gold();
            if net >= HalfInt::ZERO {
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
        let mine: Vec<hecs::Entity> = self
            .economy()
            .cities_of(player)
            .into_iter()
            .map(|(e, _)| e)
            .collect();
        for &city in &mine {
            let min = self.economy().city_min_farmers(city);
            let _ = self.world.insert_one(city, Farmers(min));
        }
        let units = self.units();
        let upkeep = units.food_of(&units.player_units(player));
        // Each extra farmer yields food beyond what he eats; add them where it pays most.
        // The surplus is the sum of what each city grows beyond its citizens' needs
        // (`Budget::compute`). Only the city that gets a farmer changes, so each city's
        // economy is worked out once for its farmers now and once for one more, and only the
        // city that took a farmer is worked out again.
        struct Slot {
            city: hecs::Entity,
            farmers: u32,
            figures: u32,
            /// The city's food with `farmers`, and with one more (None: no gain to be had).
            food: Option<HalfInt>,
            more: Option<HalfInt>,
            excess: HalfInt,
        }
        let judge = |g: &Game, city: hecs::Entity, farmers: u32| -> Slot {
            let figures = g.world.get::<&Population>(city).map_or(0, |p| p.figures);
            let now = g.economy().city_economy_with(city, farmers);
            let more = if farmers < figures {
                g.economy().city_economy_with(city, farmers + 1)
            } else {
                None
            };
            Slot {
                city,
                farmers,
                figures,
                excess: now.as_ref().map_or(HalfInt::ZERO, |eco| {
                    (eco.food - HalfInt::from_int(eco.figures as i32)).max(HalfInt::ZERO)
                }),
                food: now.map(|e| e.food),
                more: more.map(|e| e.food),
            }
        };
        let mut slots: Vec<Slot> = mine
            .iter()
            .map(|&c| {
                let farmers = self.world.get::<&Farmers>(c).map_or(0, |f| f.0);
                judge(self, c, farmers)
            })
            .collect();
        loop {
            let surplus: HalfInt = slots.iter().map(|s| s.excess).sum();
            if surplus >= upkeep {
                break;
            }
            let best = slots
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    let gain = s.more? - s.food?;
                    (s.farmers < s.figures && gain > HalfInt::ZERO).then_some((gain, i))
                })
                .max_by_key(|&(gain, i)| (gain, std::cmp::Reverse(self.serial_of(slots[i].city))));
            let Some((_, i)) = best else {
                break;
            };
            let (city, farmers) = (slots[i].city, slots[i].farmers + 1);
            let _ = self.world.insert_one(city, Farmers(farmers));
            slots[i] = judge(self, city, farmers);
        }
    }

    /// What one unit costs the treasury.
    fn troop_of(&self, id: hecs::Entity) -> Troop {
        let units = self.units();
        Troop {
            id,
            food: units.unit_food(id),
            gold: units.unit_gold_upkeep(id),
            noble_hero: units.unit_has_ability(id, Ability::new(96))
                && units.unit_def(id).is_some_and(|d| d.kind == UnitKind::Hero),
            cost: units.unit_def(id).map_or(0, |d| d.cost),
            last: units.hero_kind(id).is_some()
                || (self.economy().is_ship(id) && !self.economy().passengers_of(id).is_empty()),
        }
    }

    /// The figures of a player's troops that the treasury needs, in the order of `player_units`.
    pub(crate) fn troops(&self, player: PlayerId) -> Vec<Troop> {
        self.units()
            .player_units(player)
            .into_iter()
            .map(|id| self.troop_of(id))
            .collect()
    }

    /// What one city adds to the treasury (none for a city that has no economy).
    fn city_yield_of(&self, city: hecs::Entity) -> Option<CityYield> {
        let eco = self.economy().city_economy(city)?;
        Some(CityYield {
            gold: eco.gold,
            gold_upkeep: eco.gold_upkeep,
            food: eco.food,
            figures: eco.figures as i32,
        })
    }

    /// What a player will take in and pay out at the next turn change.
    pub fn budget(&self, player: PlayerId) -> Budget {
        let cities: Vec<CityYield> = self
            .economy()
            .cities_of(player)
            .into_iter()
            .filter_map(|(city, _)| self.city_yield_of(city))
            .collect();
        let fame = self.fame(player);
        Budget::compute(&cities, &self.troops(player), fame)
    }

    /// [`Game::budget`] while a player's cities build (`turn.rs`), asked for the sake of the
    /// auto-builder of `city`. The figures it is made of are kept from one asking to the next;
    /// what a completed item changes is the city that completed it (flagged by
    /// [`Game::mark_budget_dirty`]) and one more unit, so a budget costs the economy of the
    /// cities that changed and the upkeep of the new units, not the whole empire's.
    /// `F4X_CHECK_BUDGET` works the budget out afresh each time and compares (slow).
    pub(crate) fn kept_budget(&self, player: PlayerId, city: hecs::Entity) -> Budget {
        let mut kept = self.budget_parts.borrow_mut();
        if kept.as_ref().is_none_or(|p| p.owner != player) {
            *kept = Some(BudgetParts {
                owner: player,
                cities: self
                    .economy()
                    .cities_of(player)
                    .into_iter()
                    .filter_map(|(city, _)| Some((city, self.city_yield_of(city)?)))
                    .collect(),
                troops: self.troops(player),
                dirty: Vec::new(),
            });
        }
        let parts = kept.as_mut().expect("the parts were just made");
        // The city that builds and the ones that completed something since the last asking.
        let mut stale = std::mem::take(&mut parts.dirty);
        stale.push(city);
        for c in stale {
            let now = self.city_yield_of(c);
            match (now, parts.cities.iter().position(|(k, _)| *k == c)) {
                (Some(y), Some(i)) => parts.cities[i].1 = y,
                (Some(y), None) => parts.cities.push((c, y)),
                (None, Some(i)) => {
                    parts.cities.remove(i);
                }
                (None, None) => {}
            }
        }
        // Units made since: the order of `player_units` is the order of the serial numbers, so
        // the new ones come last. Anything else starts again.
        if self.units().player_unit_count(player) != parts.troops.len() {
            let now = self.units().player_units(player);
            let known = parts.troops.len();
            if now.len() < known || parts.troops.iter().zip(&now).any(|(t, &u)| t.id != u) {
                parts.troops = now.iter().map(|&u| self.troop_of(u)).collect();
            } else {
                for &unit in &now[known..] {
                    parts.troops.push(self.troop_of(unit));
                }
            }
        }
        let yields: Vec<CityYield> = parts.cities.iter().map(|(_, y)| *y).collect();
        let budget = Budget::compute(&yields, &parts.troops, self.fame(player));
        if check_budget() {
            assert_eq!(
                budget,
                self.budget(player),
                "the kept budget is out of date"
            );
        }
        budget
    }

    /// `city` of `player` completed an item, or changed what it builds, while the kept budget is
    /// in use: its yield is worked out again at the next asking.
    pub(crate) fn mark_budget_dirty(&self, player: PlayerId, city: hecs::Entity) {
        if let Some(parts) = self.budget_parts.borrow_mut().as_mut() {
            if parts.owner == player {
                parts.dirty.push(city);
            }
        }
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
