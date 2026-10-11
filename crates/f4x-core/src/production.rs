//! What a city is building, buying it, and finishing it.

use crate::city::{City, PlayerId, Position};
use crate::economy::{Buildings, CityRace};
use crate::rules::{BuildingId, Stat, UnitId, UnitKind};
use crate::units::Experience;
use crate::Game;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionItem {
    Building(BuildingId),
    Unit(UnitId),
    /// Turn production into gold (two production for one gold).
    TradeGoods,
    /// Speed up population growth instead of building anything.
    Housing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Production {
    pub item: Option<ProductionItem>,
    pub progress: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionError {
    NotACity,
    AlreadyBuilt,
    /// The race cannot build it.
    WrongRace,
    /// A building it needs is missing.
    MissingBuilding,
    NothingToBuy,
    NotEnoughGold,
    /// Coastal cities only (ships).
    NotCoastal,
    NotBuilt,
}

impl Game {
    /// Whether the city may start building `item`.
    pub fn can_build(
        &self,
        city: hecs::Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        let rules = &self.content.rules;
        let race_id = self
            .world
            .get::<&CityRace>(city)
            .map(|r| r.0)
            .map_err(|_| ProductionError::NotACity)?;
        let race = rules.races.get(race_id);
        let built = self
            .world
            .get::<&Buildings>(city)
            .map(|b| b.0.clone())
            .map_err(|_| ProductionError::NotACity)?;
        match item {
            ProductionItem::TradeGoods | ProductionItem::Housing => Ok(()),
            ProductionItem::Building(b) => {
                let def = rules.buildings.get(b);
                if built.contains(&b) {
                    return Err(ProductionError::AlreadyBuilt);
                }
                if !def.races.is_empty() && !def.races.contains(&race.id) {
                    return Err(ProductionError::WrongRace);
                }
                self.requirements_met(&built, &def.requires)
            }
            ProductionItem::Unit(u) => {
                let def = rules.units.get(u);
                let allowed = match def.kind {
                    UnitKind::Normal => race.units.contains(&def.id),
                    UnitKind::Fantastic => true,
                    UnitKind::Hero | UnitKind::Summoned => false,
                };
                if !allowed {
                    return Err(ProductionError::WrongRace);
                }
                if def.movement == crate::rules::MovementType::Sailing
                    && !self.city_is_coastal(city)
                {
                    return Err(ProductionError::NotCoastal);
                }
                self.requirements_met(&built, &def.requires)
            }
        }
    }

    fn requirements_met(
        &self,
        built: &[BuildingId],
        requires: &[String],
    ) -> Result<(), ProductionError> {
        let rules = &self.content.rules;
        for r in requires {
            let ok = rules
                .buildings
                .id_of(r)
                .is_some_and(|id| built.contains(&id));
            if !ok {
                return Err(ProductionError::MissingBuilding);
            }
        }
        Ok(())
    }

    pub fn city_is_coastal(&self, city: hecs::Entity) -> bool {
        let (Some(map), Ok(at)) = (
            self.map.as_ref(),
            self.world.get::<&Position>(city).map(|p| *p),
        ) else {
            return false;
        };
        let grid = map.plane(at.plane);
        (-1i32..=1).any(|dy| {
            (-1i32..=1).any(|dx| {
                grid.get(at.x as i32 + dx, at.y as i32 + dy)
                    .and_then(|t| self.content.terrains.get(t))
                    .is_some_and(|d| d.class == crate::content::TerrainClass::Ocean)
            })
        })
    }

    /// Production points the item needs in this city.
    pub fn item_cost(&self, city: hecs::Entity, item: ProductionItem) -> u32 {
        let rules = &self.content.rules;
        match item {
            ProductionItem::Building(b) => rules.buildings.get(b).cost,
            ProductionItem::Unit(u) => {
                let base = rules.units.get(u).cost;
                let percent = self.city_economy(city).map_or(100, |e| e.unit_cost_percent);
                base * percent.clamp(50, 100) as u32 / 100
            }
            ProductionItem::TradeGoods | ProductionItem::Housing => 0,
        }
    }

    pub fn city_production(&self, city: hecs::Entity) -> Production {
        self.world
            .get::<&Production>(city)
            .map(|p| *p)
            .unwrap_or_default()
    }

    /// Production still needed for the current item.
    pub fn production_remaining(&self, city: hecs::Entity) -> u32 {
        let p = self.city_production(city);
        match p.item {
            Some(item) => self.item_cost(city, item).saturating_sub(p.progress),
            None => 0,
        }
    }

    /// Gold to finish the current item now: four times what is left from scratch, three
    /// times with less than half of the remainder already done, otherwise twice.
    pub fn buy_cost(&self, city: hecs::Entity) -> Option<u32> {
        let p = self.city_production(city);
        let item = p.item?;
        if matches!(item, ProductionItem::TradeGoods | ProductionItem::Housing) {
            return None;
        }
        let remaining = self.production_remaining(city);
        if remaining == 0 {
            return None;
        }
        Some(if p.progress == 0 {
            remaining * 4
        } else if p.progress < remaining / 2 {
            remaining * 3
        } else {
            remaining * 2
        })
    }

    pub fn set_production(
        &mut self,
        city: hecs::Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        self.can_build(city, item)?;
        let mut p = self.city_production(city);
        p.item = Some(item);
        let _ = self.world.insert_one(city, p);
        Ok(())
    }

    /// Pay to complete the current item at the next turn change.
    pub fn buy_production(&mut self, city: hecs::Entity) -> Result<u32, ProductionError> {
        let cost = self.buy_cost(city).ok_or(ProductionError::NothingToBuy)?;
        let owner = self.city_owner(city).ok_or(ProductionError::NotACity)?;
        let remaining = self.production_remaining(city);
        let player = self.player_mut(owner).ok_or(ProductionError::NotACity)?;
        if player.gold < cost as i32 {
            return Err(ProductionError::NotEnoughGold);
        }
        player.gold -= cost as i32;
        let mut p = self.city_production(city);
        p.progress += remaining;
        let _ = self.world.insert_one(city, p);
        Ok(cost)
    }

    /// Sell a building for its construction cost, unless a built building needs it (directly
    /// or through a chain of requirements).
    pub fn sell_building(
        &mut self,
        city: hecs::Entity,
        building: BuildingId,
    ) -> Result<u32, ProductionError> {
        let cost = self.content.rules.buildings.get(building).cost;
        let dependants = self.has_dependants(city, building);
        if dependants {
            return Err(ProductionError::MissingBuilding);
        }
        let owner = self.city_owner(city).ok_or(ProductionError::NotACity)?;
        {
            let mut b = self
                .world
                .get::<&mut Buildings>(city)
                .map_err(|_| ProductionError::NotACity)?;
            let before = b.0.len();
            b.0.retain(|&x| x != building);
            if b.0.len() == before {
                return Err(ProductionError::NotBuilt);
            }
        }
        if let Some(p) = self.player_mut(owner) {
            p.gold += cost as i32;
        }
        Ok(cost)
    }

    /// Whether a building that is built in the city needs `building`, directly or down a chain
    /// of requirements (an unbuilt link in the chain does not break it).
    pub fn has_dependants(&self, city: hecs::Entity, building: BuildingId) -> bool {
        let rules = &self.content.rules;
        let mut needed = vec![rules.buildings.get(building).id.clone()];
        let mut i = 0;
        while let Some(name) = needed.get(i).cloned() {
            i += 1;
            for b in rules.buildings.defs() {
                if !b.requires.contains(&name) || needed.contains(&b.id) {
                    continue;
                }
                if rules
                    .buildings
                    .id_of(&b.id)
                    .is_some_and(|id| self.has_building(city, id))
                {
                    return true;
                }
                needed.push(b.id.clone());
            }
        }
        false
    }

    pub fn has_building(&self, city: hecs::Entity, building: BuildingId) -> bool {
        self.world
            .get::<&Buildings>(city)
            .is_ok_and(|b| b.0.contains(&building))
    }

    /// Experience a unit built in this city starts with (guilds that train recruits).
    pub fn city_starting_xp(&self, city: hecs::Entity) -> u32 {
        crate::rules::stats::compute(Stat::StartingXp, 0, &self.city_modifiers(city)).max(0) as u32
    }

    /// Add this turn's production and complete the item if it is covered.
    /// Returns what was completed.
    pub fn apply_production(
        &mut self,
        city: hecs::Entity,
        production: i32,
    ) -> Option<ProductionItem> {
        let mut p = self.city_production(city);
        let item = p.item?;
        if matches!(item, ProductionItem::TradeGoods | ProductionItem::Housing) {
            return None;
        }
        p.progress += production.max(0) as u32;
        let cost = self.item_cost(city, item);
        if p.progress < cost {
            let _ = self.world.insert_one(city, p);
            return None;
        }
        let at = *self.world.get::<&Position>(city).ok()?;
        let owner = self.world.get::<&City>(city).ok()?.owner;
        match item {
            ProductionItem::Building(b) => {
                if let Ok(mut buildings) = self.world.get::<&mut Buildings>(city) {
                    buildings.0.push(b);
                }
                // Surplus production is lost; the city falls back to housing, unless the
                // Grand Vizier picks the next building.
                p = Production {
                    item: Some(
                        self.vizier_choice(city, owner)
                            .unwrap_or(ProductionItem::Housing),
                    ),
                    progress: 0,
                };
            }
            ProductionItem::Unit(u) => {
                let xp = self.city_starting_xp(city);
                let unit = self.spawn_unit(owner, at, u);
                let _ = self.world.insert_one(unit, Experience(xp));
                p.progress = 0;
            }
            ProductionItem::TradeGoods | ProductionItem::Housing => {}
        }
        let _ = self.world.insert_one(city, p);
        Some(item)
    }

    /// What the Grand Vizier has the city build next: the first building in the rules'
    /// `autobuild_order` that the city can build and whose upkeep the treasury can bear.
    /// `None` when the player has not asked for the Vizier or nothing is left to build.
    pub fn vizier_choice(&self, city: hecs::Entity, owner: PlayerId) -> Option<ProductionItem> {
        if !self.player(owner)?.grand_vizier {
            return None;
        }
        // An outpost builds nothing; housing is what makes it grow.
        if self.city_economy(city).is_none_or(|e| e.figures == 0) {
            return None;
        }
        let rules = &self.content.rules;
        let net = self.treasury(owner).net_gold();
        rules
            .config
            .autobuild_order
            .iter()
            .filter_map(|name| rules.buildings.id_of(name))
            .filter(|&b| self.can_build(city, ProductionItem::Building(b)).is_ok())
            .find(|&b| net >= rules.buildings.get(b).upkeep_gold as i32)
            .map(ProductionItem::Building)
    }

    /// Turning the Vizier on: cities with nothing to build (or housing) get a building now.
    pub(crate) fn vizier_fills_idle_cities(&mut self, player: PlayerId) {
        for (city, c, _) in self.cities() {
            if c.owner != player {
                continue;
            }
            let idle = matches!(
                self.city_production(city).item,
                None | Some(ProductionItem::Housing)
            );
            if !idle {
                continue;
            }
            if let Some(item) = self.vizier_choice(city, player) {
                let _ = self.world.insert_one(
                    city,
                    Production {
                        item: Some(item),
                        progress: 0,
                    },
                );
            }
        }
    }
}
