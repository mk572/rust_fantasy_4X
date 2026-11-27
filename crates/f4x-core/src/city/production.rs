//! What a city is building, buying it, and finishing it.
//!
//! The checks and the costs are methods of [`Economy`]: what a city may build, what
//! it costs, how far it is, what a purchase or a sale would do, what finishing an item gives
//! and what the auto-builder picks (`city/autobuild.rs`). They need only the world, the map, the
//! content and the players. `World` makes the changes that need more (the owner's gold, the new
//! unit, the treasury for the Vizier).

use crate::world::rng::GameRng;
use f4x_def::half_int::HalfInt;

use crate::city::economy::{Buildings, CityRace, Economy};
use crate::city::City;
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::{BuildingId, Stat, UnitId, UnitKind};
use crate::unit::Experience;
use crate::world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Production points gathered, in halves.
    pub progress: HalfInt,
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
    /// The city has sold a building this turn already.
    AlreadySold,
    /// Settlers need a city of two citizens: one stays.
    TooSmall,
}

impl ProductionError {
    /// The key of its sentence in `content/text`.
    pub fn key(self) -> &'static str {
        match self {
            Self::NotACity => "refuse_wizard.production_error_not_a_city",
            Self::AlreadyBuilt => "refuse_wizard.production_error_already_built",
            Self::WrongRace => "refuse_wizard.production_error_wrong_race",
            Self::MissingBuilding => "refuse_wizard.production_error_missing_building",
            Self::NothingToBuy => "refuse_wizard.production_error_nothing_to_buy",
            Self::NotEnoughGold => "refuse_wizard.production_error_not_enough_gold",
            Self::NotCoastal => "refuse_wizard.production_error_not_coastal",
            Self::NotBuilt => "refuse_wizard.production_error_not_built",
            Self::AlreadySold => "refuse_wizard.production_error_already_sold",
            Self::TooSmall => "refuse_wizard.production_error_too_small",
        }
    }
}

/// What a building brings when sold: a third of its cost, city walls half of that, at least 1
/// (`City_Sell_Building_Value`).
pub(crate) fn sale_value(cost: u32, walls: bool) -> u32 {
    let third = cost / 3;
    (if walls { third / 2 } else { third }).max(1)
}

/// The city has sold this building since the turn began: it may sell one a turn
/// (`sold_building` in the original, cleared when the turn changes). The city picture shows
/// rubble in its place until then (`bs_Removed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoldBuilding(pub BuildingId);

/// What buying the current item takes (`Economy::buy_plan`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuyPlan {
    pub cost: HalfInt,
    /// The production that is added, which completes the item.
    pub remaining: HalfInt,
    pub owner: PlayerId,
}

/// What this turn's production did to the current item (`Economy::advance_production`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Advance {
    /// Short of the cost: the progress to keep.
    Waiting(Production),
    /// Covered: what was built, with the progress as it stands, and where and for whom.
    Done {
        item: ProductionItem,
        production: Production,
        at: Position,
        owner: PlayerId,
    },
}

/// Take a building out of a city. Fails when the city has none or does not have this one.
pub(crate) fn remove_building(
    ecs: &hecs::World,
    city: hecs::Entity,
    building: BuildingId,
) -> Result<(), ProductionError> {
    let mut b = ecs
        .get::<&mut Buildings>(city)
        .map_err(|_| ProductionError::NotACity)?;
    let before = b.0.len();
    b.0.retain(|&x| x != building);
    if b.0.len() == before {
        return Err(ProductionError::NotBuilt);
    }
    Ok(())
}

impl Economy<'_> {
    /// Whether the city may start building `item`.
    pub fn can_build(
        &self,
        city: hecs::Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        let rules = &self.content.rules;
        let race_id = self
            .ecs
            .get::<&CityRace>(city)
            .map(|r| r.0)
            .map_err(|_| ProductionError::NotACity)?;
        let race = rules.races.get(race_id);
        let built = self
            .ecs
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
                if !def.buildable_by(race_id) {
                    return Err(ProductionError::WrongRace);
                }
                if def.coastal && !self.city_is_coastal(city) {
                    return Err(ProductionError::NotCoastal);
                }
                self.requirements_met(&built, &def.requires)
            }
            ProductionItem::Unit(u) => {
                let def = rules.units.get(u);
                let allowed = match def.kind {
                    UnitKind::Normal => race.units.contains(&u),
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
                if self.costs_a_citizen(u)
                    && self
                        .ecs
                        .get::<&super::economy::Population>(city)
                        .map_or(0, |p| p.figures)
                        < 2
                {
                    return Err(ProductionError::TooSmall);
                }
                self.requirements_met(&built, &def.requires)
            }
        }
    }

    fn requirements_met(
        &self,
        built: &[BuildingId],
        requires: &[BuildingId],
    ) -> Result<(), ProductionError> {
        if requires.iter().all(|r| built.contains(r)) {
            Ok(())
        } else {
            Err(ProductionError::MissingBuilding)
        }
    }

    pub fn city_is_coastal(&self, city: hecs::Entity) -> bool {
        let (Some(map), Ok(at)) = (self.map, self.ecs.get::<&Position>(city).map(|p| *p)) else {
            return false;
        };
        let grid = map.plane(at.plane);
        (-1i32..=1).any(|dy| {
            (-1i32..=1).any(|dx| {
                grid.get(at.x as i32 + dx, at.y as i32 + dy)
                    .and_then(|t| self.content.terrains.get(t))
                    .is_some_and(|d| d.class == crate::map::terrain::TerrainClass::Ocean)
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
        self.ecs
            .get::<&Production>(city)
            .map(|p| *p)
            .unwrap_or_default()
    }

    /// Production still needed for the current item.
    pub fn production_remaining(&self, city: hecs::Entity) -> HalfInt {
        let p = self.city_production(city);
        match p.item {
            Some(item) => (HalfInt::from_int(self.item_cost(city, item) as i32) - p.progress)
                .max(HalfInt::ZERO),
            None => HalfInt::ZERO,
        }
    }

    /// Gold to finish the current item now: four times what is left from scratch, three
    /// times with less than half of the remainder already done, otherwise twice.
    pub fn buy_cost(&self, city: hecs::Entity) -> Option<HalfInt> {
        let p = self.city_production(city);
        let item = p.item?;
        if matches!(item, ProductionItem::TradeGoods | ProductionItem::Housing) {
            return None;
        }
        let remaining = self.production_remaining(city);
        if remaining == HalfInt::ZERO {
            return None;
        }
        Some(if p.progress == HalfInt::ZERO {
            remaining * 4
        } else if p.progress * 2 < remaining {
            remaining * 3
        } else {
            remaining * 2
        })
    }

    /// The built building that needs `building`, directly or down a chain of requirements:
    /// the one a sale is refused for (`City_Building_Has_Requirement`).
    pub fn first_dependant(&self, city: hecs::Entity, building: BuildingId) -> Option<BuildingId> {
        let rules = &self.content.rules;
        let mut needed = vec![building];
        let mut i = 0;
        while let Some(need) = needed.get(i).copied() {
            i += 1;
            for (id, b) in rules.buildings.iter() {
                if !b.requires.contains(&need) || needed.contains(&id) {
                    continue;
                }
                if self.has_building(city, id) {
                    return Some(id);
                }
                needed.push(id);
            }
        }
        None
    }

    /// Whether what the city is building needs `building`: selling it ends that production
    /// (`City_Building_Is_Currently_Required`).
    pub fn production_needs(&self, city: hecs::Entity, building: BuildingId) -> bool {
        let rules = &self.content.rules;
        match self.city_production(city).item {
            Some(ProductionItem::Building(b)) => {
                rules.buildings.get(b).requires.contains(&building)
            }
            Some(ProductionItem::Unit(u)) => rules.units.get(u).requires.contains(&building),
            _ => false,
        }
    }

    pub fn has_building(&self, city: hecs::Entity, building: BuildingId) -> bool {
        self.ecs
            .get::<&Buildings>(city)
            .is_ok_and(|b| b.0.contains(&building))
    }

    /// The weapons units trained in the city get (`Terrain.c`'s `City_Best_Weapon`): normal
    /// without an alchemists guild (built or replaced), magic with one, mithril or adamantium
    /// when the city works such an ore on a square not spoiled by corruption (adamantium wins).
    pub fn city_weapon_quality(&self, city: hecs::Entity) -> u8 {
        self.city_weapon_quality_with(city, false)
    }

    /// [`city_weapon_quality`](Self::city_weapon_quality), with the guild not needed when
    /// `without_guild` (Adamant Alchemy: the city's owner has Alchemy).
    pub fn city_weapon_quality_with(&self, city: hecs::Entity, without_guild: bool) -> u8 {
        use crate::unit::{WEAPON_ADAMANTIUM, WEAPON_MAGIC, WEAPON_MITHRIL};
        if !without_guild
            && !self.city_has_building(
                city,
                crate::rules::BuildingId::new(19), /* alchemists_guild */
            )
        {
            return 0;
        }
        let (Some(map), Ok(at)) = (self.map, self.ecs.get::<&Position>(city).map(|p| *p)) else {
            return WEAPON_MAGIC;
        };
        let grid = map.plane(at.plane);
        let mut quality = WEAPON_MAGIC;
        for (p, _) in self.city_area(at) {
            if self
                .map
                .is_some_and(|m| m.plane(p.plane).is_corrupted(p.x as i32, p.y as i32))
            {
                continue;
            }
            let id = grid
                .mineral(p.x as i32, p.y as i32)
                .and_then(|f| self.content.features.get(f))
                .map(|f| f.id.as_str());
            match id {
                Some("adamantium_ore") => quality = WEAPON_ADAMANTIUM,
                Some("mithril_ore") if quality != WEAPON_ADAMANTIUM => quality = WEAPON_MITHRIL,
                _ => {}
            }
        }
        quality
    }

    /// Experience a unit built in this city starts with (guilds that train recruits).
    pub fn city_starting_xp(&self, city: hecs::Entity) -> u32 {
        crate::rules::stats::compute(Stat::StartingXp, 0, &self.city_modifiers(city)).max(0) as u32
    }

    /// What buying the current item would take: the gold, the production it adds and who pays.
    pub(crate) fn buy_plan(&self, city: hecs::Entity) -> Result<BuyPlan, ProductionError> {
        let cost = self.buy_cost(city).ok_or(ProductionError::NothingToBuy)?;
        let owner = self.city_owner(city).ok_or(ProductionError::NotACity)?;
        Ok(BuyPlan {
            cost,
            remaining: self.production_remaining(city),
            owner,
        })
    }

    /// Whether the city has sold a building this turn.
    pub fn sold_this_turn(&self, city: hecs::Entity) -> bool {
        self.sold_building(city).is_some()
    }

    /// The building the city has sold this turn, which its picture shows as rubble.
    pub fn sold_building(&self, city: hecs::Entity) -> Option<BuildingId> {
        self.ecs.get::<&SoldBuilding>(city).ok().map(|s| s.0)
    }

    /// What selling `building` would bring, or why it cannot be sold.
    pub fn sale_price(
        &self,
        city: hecs::Entity,
        building: BuildingId,
    ) -> Result<u32, ProductionError> {
        self.sell_plan(city, building).map(|(_, cost)| cost)
    }

    /// Who is paid, and what, for selling a building: refused while a built building needs it.
    pub(crate) fn sell_plan(
        &self,
        city: hecs::Entity,
        building: BuildingId,
    ) -> Result<(PlayerId, u32), ProductionError> {
        if self.sold_this_turn(city) {
            return Err(ProductionError::AlreadySold);
        }
        if self.first_dependant(city, building).is_some() {
            return Err(ProductionError::MissingBuilding);
        }
        let def = self.content.rules.buildings.get(building);
        let cost = sale_value(def.cost, def.num == 35 /* city_walls */);
        let owner = self.city_owner(city).ok_or(ProductionError::NotACity)?;
        Ok((owner, cost))
    }

    /// This turn's production added to the current item: still short of its cost, or done.
    /// `None` when there is nothing to build (or only trade goods and housing).
    pub(crate) fn advance_production(
        &self,
        city: hecs::Entity,
        production: HalfInt,
    ) -> Option<Advance> {
        let mut p = self.city_production(city);
        let item = p.item?;
        if matches!(item, ProductionItem::TradeGoods | ProductionItem::Housing) {
            return None;
        }
        p.progress += production.max(HalfInt::ZERO);
        if p.progress < HalfInt::from_int(self.item_cost(city, item) as i32) {
            return Some(Advance::Waiting(p));
        }
        let at = *self.ecs.get::<&Position>(city).ok()?;
        let owner = self.city_owner(city)?;
        Some(Advance::Done {
            item,
            production: p,
            at,
            owner,
        })
    }

    /// What a unit finished in this city starts with: its experience and the quality of its
    /// weapons (a wizard with Alchemy gives every unit at least magic weapons: `StartingWeapons`).
    /// With `adamant_alchemy` (the setting) an Alchemy wizard's cities work the ores without the
    /// guild.
    pub(crate) fn new_unit_start(
        &self,
        city: hecs::Entity,
        owner: PlayerId,
        adamant_alchemy: bool,
    ) -> (u32, u8) {
        let alchemist = adamant_alchemy
            && self.wizard(owner).is_some_and(|m| {
                m.stat(&self.content.rules, crate::rules::Stat::StartingWeapons, 0) >= 1
            });
        let mut weapons = self.city_weapon_quality_with(city, alchemist);
        if let Some(m) = self.wizard(owner) {
            let better = m.stat(
                &self.content.rules,
                crate::rules::Stat::StartingWeapons,
                i32::from(weapons),
            );
            weapons = u8::try_from(better).unwrap_or(weapons);
        }
        (self.city_starting_xp(city), weapons)
    }
}

impl World {
    pub fn set_production(
        &mut self,
        city: hecs::Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        self.economy().can_build(city, item)?;
        let mut p = self.economy().city_production(city);
        p.item = Some(item);
        self.put(city, p);
        self.note_production_change(city);
        Ok(())
    }

    /// Pay to complete the current item at the next turn change.
    pub fn buy_production(&mut self, city: hecs::Entity) -> Result<HalfInt, ProductionError> {
        let plan = self.economy().buy_plan(city)?;
        let treasury = self
            .treasury_mut(plan.owner)
            .ok_or(ProductionError::NotACity)?;
        if treasury.gold < plan.cost {
            return Err(ProductionError::NotEnoughGold);
        }
        treasury.gold -= plan.cost;
        let mut p = self.economy().city_production(city);
        p.progress += plan.remaining;
        self.put(city, p);
        Ok(plan.cost)
    }

    /// A new turn of `player`: each of their cities may sell a building again.
    pub(crate) fn reopen_sales(&mut self, player: crate::player::PlayerId) {
        let sold: Vec<hecs::Entity> = self
            .ecs
            .query::<(&City, &SoldBuilding)>()
            .iter()
            .map(|(e, _)| e)
            .filter(|&e| self.economy().city_owner(e) == Some(player))
            .collect();
        for e in sold {
            self.strip::<SoldBuilding>(e);
        }
    }

    /// Sell a building for a third of its construction cost, unless a built building needs it
    /// (directly or through a chain of requirements) or the city has sold one this turn.
    pub fn sell_building(
        &mut self,
        city: hecs::Entity,
        building: BuildingId,
    ) -> Result<u32, ProductionError> {
        let (owner, cost) = self.economy().sell_plan(city, building)?;
        let ends_production = self.economy().production_needs(city, building);
        remove_building(&self.ecs, city, building)?;
        self.put(city, SoldBuilding(building));
        if ends_production {
            let mut p = self.economy().city_production(city);
            p.item = Some(ProductionItem::Housing);
            self.put(city, p);
        }
        if let Some(t) = self.treasury_mut(owner) {
            t.gold += cost as i32;
        }
        Ok(cost)
    }

    /// The current item when the city can no longer build it (a building it needs is gone, or
    /// the like): the city falls back to Trade Goods.
    pub(crate) fn production_impossible(&mut self, city: hecs::Entity) -> Option<ProductionItem> {
        let item = self.economy().city_production(city).item?;
        let gone = matches!(
            self.economy().can_build(city, item),
            Err(ProductionError::MissingBuilding
                | ProductionError::WrongRace
                | ProductionError::NotCoastal)
        );
        if !gone {
            return None;
        }
        let mut p = self.economy().city_production(city);
        p.item = Some(ProductionItem::TradeGoods);
        self.put(city, p);
        Some(item)
    }

    /// Add this turn's production and complete the item if it is covered.
    /// A unit that would be finished now but finds the square full (nine units): the unit waits
    /// with its cost paid and the production of this turn is not added. Returns the unit.
    pub(crate) fn production_blocked(
        &mut self,
        city: hecs::Entity,
        production: HalfInt,
    ) -> Option<ProductionItem> {
        let Some(Advance::Done {
            item: item @ ProductionItem::Unit(_),
            production: p,
            ..
        }) = self.economy().advance_production(city, production)
        else {
            return None;
        };
        let ProductionItem::Unit(u) = item else {
            return None;
        };
        let too_small = self.economy().costs_a_citizen(u) && self.city_population(city).figures < 2;
        if !too_small {
            return None;
        }
        self.put(city, p);
        Some(item)
    }

    /// Returns what was completed.
    pub fn apply_production(
        &mut self,
        city: hecs::Entity,
        production: HalfInt,
        rng: &mut GameRng,
    ) -> Option<ProductionItem> {
        let (item, mut p, at, owner) = match self.economy().advance_production(city, production)? {
            Advance::Waiting(p) => {
                self.put(city, p);
                return None;
            }
            Advance::Done {
                item,
                production,
                at,
                owner,
            } => (item, production, at, owner),
        };
        let cost = self.economy().item_cost(city, item);
        match item {
            ProductionItem::Building(b) => {
                if let Ok(mut buildings) = self.ecs.get::<&mut Buildings>(city) {
                    buildings.0.push(b);
                }
                self.autobuild_spent(city, item, cost);
                // The building's upkeep is in the net gold the choice below reads.
                self.note_production_change(city);
                // Surplus production is lost. The city has nothing in hand until its owner
                // chooses again, unless the auto-builder (a human's helper) picks the next item.
                p = Production {
                    item: self.autobuild_choice(city, owner, rng),
                    progress: HalfInt::ZERO,
                };
            }
            ProductionItem::Unit(u) => {
                let (xp, weapons) =
                    self.economy()
                        .new_unit_start(city, owner, self.settings.rules.adamant_alchemy);
                if self.economy().costs_a_citizen(u) {
                    let pop = self.city_population(city);
                    self.put(
                        city,
                        super::economy::Population {
                            figures: pop.figures.saturating_sub(1).max(1),
                            ..pop
                        },
                    );
                }
                let unit = self.spawn_unit(owner, at, u);
                self.doom_mastery_mutates(unit, owner, rng);
                self.put(unit, Experience(xp));
                if weapons > 0 {
                    self.put(unit, crate::unit::WeaponQuality(weapons));
                }
                p.progress = HalfInt::ZERO;
                self.autobuild_spent(city, item, cost);
                // The unit's upkeep (and the citizen it cost) is in the net gold the choice
                // below reads.
                self.note_production_change(city);
                // Likewise: nothing in hand, unless the auto-builder picks the next item.
                p.item = self.autobuild_choice(city, owner, rng);
            }
            ProductionItem::TradeGoods | ProductionItem::Housing => {}
        }
        self.put(city, p);
        Some(item)
    }

    /// Turning the Vizier on: cities with nothing to build (or housing) get a building now.
    pub(crate) fn vizier_fills_idle_cities(&mut self, player: PlayerId, rng: &mut GameRng) {
        for (city, c, _) in self.economy().cities() {
            if c.owner != player {
                continue;
            }
            let idle = matches!(
                self.economy().city_production(city).item,
                None | Some(ProductionItem::Housing)
            );
            if !idle {
                continue;
            }
            if let Some(item) = self.autobuild_choice(city, player, rng) {
                self.put(
                    city,
                    Production {
                        item: Some(item),
                        progress: HalfInt::ZERO,
                    },
                );
            }
        }
    }
}
