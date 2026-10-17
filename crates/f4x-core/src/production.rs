//! What a city is building, buying it, and finishing it.
//!
//! The checks and the costs are methods of [`Economy`] (task 187): what a city may build, what
//! it costs, how far it is, what a purchase or a sale would do, what finishing an item gives
//! and (task: auto-builder, `autobuild.rs`) what the auto-builder picks. They need only the world, the map, the content and the
//! players. `Game` makes the changes that need more (the owner's gold, the new unit, the
//! treasury for the Vizier) and keeps the old names as thin methods that ask the handle.

use hecs::World;

use crate::city::{City, PlayerId, Position};
use crate::economy::{Buildings, CityRace, Economy};
use crate::rules::{BuildingId, Stat, UnitId, UnitKind};
use crate::units::Experience;
use crate::Game;

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
    /// The city has sold a building this turn already.
    AlreadySold,
}

/// What a building brings when sold: a third of its cost, city walls half of that, at least 1
/// (`City_Sell_Building_Value`).
pub(crate) fn sale_value(cost: u32, walls: bool) -> u32 {
    let third = cost / 3;
    (if walls { third / 2 } else { third }).max(1)
}

/// The city has sold a building since the turn began: it may sell one a turn (`sold_building`
/// in the original, cleared when the turn changes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoldBuilding;

/// What buying the current item takes (`Economy::buy_plan`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuyPlan {
    pub cost: u32,
    /// The production that is added, which completes the item.
    pub remaining: u32,
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
    world: &World,
    city: hecs::Entity,
    building: BuildingId,
) -> Result<(), ProductionError> {
    let mut b = world
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
                if def.coastal && !self.city_is_coastal(city) {
                    return Err(ProductionError::NotCoastal);
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
        let (Some(map), Ok(at)) = (self.map, self.world.get::<&Position>(city).map(|p| *p)) else {
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

    /// Whether a building that is built in the city needs `building`, directly or down a chain
    /// of requirements (an unbuilt link in the chain does not break it).
    pub fn has_dependants(&self, city: hecs::Entity, building: BuildingId) -> bool {
        self.first_dependant(city, building).is_some()
    }

    /// The built building that needs `building`, directly or down a chain of requirements:
    /// the one a sale is refused for (`City_Building_Has_Requirement`).
    pub fn first_dependant(&self, city: hecs::Entity, building: BuildingId) -> Option<BuildingId> {
        let rules = &self.content.rules;
        let mut needed = vec![rules.buildings.get(building).id.clone()];
        let mut i = 0;
        while let Some(name) = needed.get(i).cloned() {
            i += 1;
            for b in rules.buildings.defs() {
                if !b.requires.contains(&name) || needed.contains(&b.id) {
                    continue;
                }
                if let Some(id) = rules
                    .buildings
                    .id_of(&b.id)
                    .filter(|&id| self.has_building(city, id))
                {
                    return Some(id);
                }
                needed.push(b.id.clone());
            }
        }
        None
    }

    /// Whether what the city is building needs `building`: selling it ends that production
    /// (`City_Building_Is_Currently_Required`).
    pub fn production_needs(&self, city: hecs::Entity, building: BuildingId) -> bool {
        let rules = &self.content.rules;
        let name = &rules.buildings.get(building).id;
        match self.city_production(city).item {
            Some(ProductionItem::Building(b)) => rules.buildings.get(b).requires.contains(name),
            Some(ProductionItem::Unit(u)) => rules.units.get(u).requires.contains(name),
            _ => false,
        }
    }

    pub fn has_building(&self, city: hecs::Entity, building: BuildingId) -> bool {
        self.world
            .get::<&Buildings>(city)
            .is_ok_and(|b| b.0.contains(&building))
    }

    /// The weapons units trained in the city get (`Terrain.c`'s `City_Best_Weapon`): normal
    /// without an alchemists guild (built or replaced), magic with one, mithril or adamantium
    /// when the city works such an ore on a square not spoiled by corruption (adamantium wins).
    pub fn city_weapon_quality(&self, city: hecs::Entity) -> u8 {
        use crate::units::{WEAPON_ADAMANTIUM, WEAPON_MAGIC, WEAPON_MITHRIL};
        if !self.city_has_building(city, "alchemists_guild") {
            return 0;
        }
        let (Some(map), Ok(at)) = (self.map, self.world.get::<&Position>(city).map(|p| *p)) else {
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
                .feature(p.x as i32, p.y as i32)
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
        self.world.get::<&SoldBuilding>(city).is_ok()
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
        if self.has_dependants(city, building) {
            return Err(ProductionError::MissingBuilding);
        }
        let def = self.content.rules.buildings.get(building);
        let cost = sale_value(def.cost, def.id == "city_walls");
        let owner = self.city_owner(city).ok_or(ProductionError::NotACity)?;
        Ok((owner, cost))
    }

    /// This turn's production added to the current item: still short of its cost, or done.
    /// `None` when there is nothing to build (or only trade goods and housing).
    pub(crate) fn advance_production(
        &self,
        city: hecs::Entity,
        production: i32,
    ) -> Option<Advance> {
        let mut p = self.city_production(city);
        let item = p.item?;
        if matches!(item, ProductionItem::TradeGoods | ProductionItem::Housing) {
            return None;
        }
        p.progress += production.max(0) as u32;
        if p.progress < self.item_cost(city, item) {
            return Some(Advance::Waiting(p));
        }
        let at = *self.world.get::<&Position>(city).ok()?;
        let owner = self.world.get::<&City>(city).ok()?.owner;
        Some(Advance::Done {
            item,
            production: p,
            at,
            owner,
        })
    }

    /// What a unit finished in this city starts with: its experience and the quality of its
    /// weapons (a wizard with Alchemy gives every unit at least magic weapons).
    pub(crate) fn new_unit_start(&self, city: hecs::Entity, owner: PlayerId) -> (u32, u8) {
        let mut weapons = self.city_weapon_quality(city);
        if weapons == 0
            && self
                .magic(owner)
                .is_some_and(|m| m.has(crate::magic::Retort::Alchemy))
        {
            weapons = crate::units::WEAPON_MAGIC;
        }
        (self.city_starting_xp(city), weapons)
    }
}

impl Game {
    pub fn set_production(
        &mut self,
        city: hecs::Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        self.economy().can_build(city, item)?;
        let mut p = self.economy().city_production(city);
        p.item = Some(item);
        let _ = self.world.insert_one(city, p);
        Ok(())
    }

    /// Pay to complete the current item at the next turn change.
    pub fn buy_production(&mut self, city: hecs::Entity) -> Result<u32, ProductionError> {
        let plan = self.economy().buy_plan(city)?;
        let player = self
            .player_mut(plan.owner)
            .ok_or(ProductionError::NotACity)?;
        if player.gold < plan.cost as i32 {
            return Err(ProductionError::NotEnoughGold);
        }
        player.gold -= plan.cost as i32;
        let mut p = self.economy().city_production(city);
        p.progress += plan.remaining;
        let _ = self.world.insert_one(city, p);
        Ok(plan.cost)
    }

    /// A new turn: every city may sell a building again.
    pub(crate) fn reopen_sales(&mut self) {
        let sold: Vec<hecs::Entity> = self
            .world
            .query::<(&City, &SoldBuilding)>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        for e in sold {
            let _ = self.world.remove_one::<SoldBuilding>(e);
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
        remove_building(&self.world, city, building)?;
        let _ = self.world.insert_one(city, SoldBuilding);
        if ends_production {
            let mut p = self.economy().city_production(city);
            p.item = Some(ProductionItem::Housing);
            let _ = self.world.insert_one(city, p);
        }
        if let Some(p) = self.player_mut(owner) {
            p.gold += cost as i32;
        }
        Ok(cost)
    }

    /// Add this turn's production and complete the item if it is covered.
    /// Returns what was completed.
    pub fn apply_production(
        &mut self,
        city: hecs::Entity,
        production: i32,
    ) -> Option<ProductionItem> {
        let (item, mut p, at, owner) = match self.economy().advance_production(city, production)? {
            Advance::Waiting(p) => {
                let _ = self.world.insert_one(city, p);
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
                if let Ok(mut buildings) = self.world.get::<&mut Buildings>(city) {
                    buildings.0.push(b);
                }
                self.autobuild_spent(city, item, cost);
                // Surplus production is lost; the city falls back to housing, unless the
                // auto-builder picks the next item.
                p = Production {
                    item: Some(
                        self.autobuild_choice(city, owner)
                            .unwrap_or(ProductionItem::Housing),
                    ),
                    progress: 0,
                };
            }
            ProductionItem::Unit(u) => {
                let (xp, weapons) = self.economy().new_unit_start(city, owner);
                let unit = self.spawn_unit(owner, at, u);
                self.doom_mastery_mutates(unit, owner);
                let _ = self.world.insert_one(unit, Experience(xp));
                if weapons > 0 {
                    let _ = self
                        .world
                        .insert_one(unit, crate::units::WeaponQuality(weapons));
                }
                p.progress = 0;
                self.autobuild_spent(city, item, cost);
                // A unit is built over and over, unless the auto-builder picks the next item.
                if let Some(next) = self.autobuild_choice(city, owner) {
                    p.item = Some(next);
                }
            }
            ProductionItem::TradeGoods | ProductionItem::Housing => {}
        }
        let _ = self.world.insert_one(city, p);
        Some(item)
    }

    /// Turning the Vizier on: cities with nothing to build (or housing) get a building now.
    pub(crate) fn vizier_fills_idle_cities(&mut self, player: PlayerId) {
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
            if let Some(item) = self.autobuild_choice(city, player) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::{Enchantments, Farmers, Population};
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::testutil::content_with_economy;

    /// A one-city world with the elves' content, and no `Game`.
    struct Rig {
        world: World,
        map: Map,
        content: crate::content::Content,
        players: Vec<Player>,
        city: hecs::Entity,
    }

    impl Rig {
        fn new() -> Rig {
            let content = content_with_economy();
            let grass = content.terrains.id_of("grassland").unwrap();
            let map = Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass));
            let mut world = World::new();
            let at = Position {
                plane: Plane::Arcanus,
                x: 10,
                y: 7,
            };
            let city = world.spawn((
                City {
                    name: "Town".into(),
                    owner: PlayerId(0),
                },
                at,
                Population {
                    figures: 4,
                    tens: 0,
                },
                Farmers(0),
                Buildings::default(),
                Enchantments::default(),
                Production::default(),
                CityRace(content.rules.races.id_of("elf").unwrap()),
            ));
            Rig {
                world,
                map,
                content,
                players: vec![Player::new(PlayerId(0), "Merlin")],
                city,
            }
        }

        fn eco(&self) -> Economy<'_> {
            Economy::new(&self.world, Some(&self.map), &self.content, &self.players)
        }

        fn building(&self, id: &str) -> BuildingId {
            self.content.rules.buildings.id_of(id).unwrap()
        }
    }

    #[test]
    fn what_a_city_may_build_follows_its_race_and_its_buildings() {
        let rig = Rig::new();
        let spear = ProductionItem::Unit(rig.content.rules.units.id_of("spear").unwrap());
        let barracks = rig.building("barracks");
        assert_eq!(
            rig.eco().can_build(rig.city, spear),
            Err(ProductionError::MissingBuilding)
        );
        rig.world
            .get::<&mut Buildings>(rig.city)
            .unwrap()
            .0
            .push(barracks);
        assert_eq!(rig.eco().can_build(rig.city, spear), Ok(()));
        assert_eq!(
            rig.eco()
                .can_build(rig.city, ProductionItem::Building(barracks)),
            Err(ProductionError::AlreadyBuilt)
        );
    }

    #[test]
    fn costs_and_purchases_are_worked_out_from_the_city_alone() {
        let rig = Rig::new();
        let barracks = ProductionItem::Building(rig.building("barracks"));
        rig.world.get::<&mut Production>(rig.city).unwrap().item = Some(barracks);
        let eco = rig.eco();
        assert_eq!(eco.item_cost(rig.city, barracks), 30);
        assert_eq!(eco.production_remaining(rig.city), 30);
        assert_eq!(eco.buy_cost(rig.city), Some(120), "four times from scratch");
        rig.world.get::<&mut Production>(rig.city).unwrap().progress = 10;
        assert_eq!(rig.eco().buy_cost(rig.city), Some(40), "twice what is left");
        let plan = rig.eco().buy_plan(rig.city).unwrap();
        assert_eq!(
            (plan.cost, plan.remaining, plan.owner),
            (40, 20, PlayerId(0))
        );
    }

    #[test]
    fn production_is_added_until_the_item_is_covered() {
        let rig = Rig::new();
        let barracks = rig.building("barracks");
        rig.world.get::<&mut Production>(rig.city).unwrap().item =
            Some(ProductionItem::Building(barracks));
        match rig.eco().advance_production(rig.city, 12).unwrap() {
            Advance::Waiting(p) => assert_eq!(p.progress, 12),
            other => panic!("{other:?}"),
        }
        rig.world.get::<&mut Production>(rig.city).unwrap().progress = 25;
        match rig.eco().advance_production(rig.city, 10).unwrap() {
            Advance::Done { item, owner, .. } => {
                assert_eq!(item, ProductionItem::Building(barracks));
                assert_eq!(owner, PlayerId(0));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_building_that_another_needs_cannot_be_sold() {
        let rig = Rig::new();
        let (barracks, smithy) = (rig.building("barracks"), rig.building("smithy"));
        let armory = rig.building("armory");
        rig.world
            .get::<&mut Buildings>(rig.city)
            .unwrap()
            .0
            .extend([barracks, smithy, armory]);
        assert_eq!(
            rig.eco().sell_plan(rig.city, smithy),
            Err(ProductionError::MissingBuilding)
        );
        assert_eq!(rig.eco().first_dependant(rig.city, smithy), Some(armory));
        assert_eq!(rig.eco().sell_plan(rig.city, armory), Ok((PlayerId(0), 26)));
        assert_eq!(remove_building(&rig.world, rig.city, armory), Ok(()));
        assert_eq!(
            remove_building(&rig.world, rig.city, armory),
            Err(ProductionError::NotBuilt)
        );
    }

    #[test]
    fn a_sale_brings_a_third_walls_a_sixth_and_never_less_than_one() {
        assert_eq!(sale_value(30, false), 10);
        assert_eq!(sale_value(200, false), 66);
        assert_eq!(sale_value(150, true), 25);
        assert_eq!(sale_value(2, false), 1);
        assert_eq!(sale_value(0, true), 1);
    }
}
