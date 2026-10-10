//! City buildings and the production queue, plus per-player upkeep.

use std::collections::BTreeSet;

use hecs::Entity;

use crate::city::{City, PlayerId};
use crate::rules::effects::TargetSel;
use crate::rules::stats::{compute, passive_modifiers_for, Modifier, Source, Stat};
use crate::rules::{BuildingId, UnitId, UnitKind, Upkeep};
use crate::units::{Owner, Unit};
use crate::{Event, Game};

/// Buildings a city has completed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CityBuildings(pub BTreeSet<BuildingId>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionItem {
    Unit(UnitId),
    Building(BuildingId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueEntry {
    pub item: ProductionItem,
    /// Production points spent so far.
    pub progress: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProductionQueue(pub Vec<QueueEntry>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionError {
    NoSuchCity,
    NotYours,
    /// The city's race cannot build it, or it is not a buildable kind of unit.
    NotAvailable,
    AlreadyBuilt,
    AlreadyQueued,
    MissingRequirement,
    BadIndex,
}

/// Modifiers aimed at a city: aimed at `This` or `City`, from its race and buildings.
const CITY_TARGETS: [TargetSel; 2] = [TargetSel::This, TargetSel::City];

impl Game {
    pub fn city_buildings(&self, city: Entity) -> BTreeSet<BuildingId> {
        self.world
            .get::<&CityBuildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default()
    }

    pub fn production_queue(&self, city: Entity) -> Vec<QueueEntry> {
        self.world
            .get::<&ProductionQueue>(city)
            .map(|q| q.0.clone())
            .unwrap_or_default()
    }

    /// Everything modifying this city's stats right now: its race and its buildings.
    /// (Lasting spells join here once casting exists.)
    pub fn city_modifiers(&self, city: Entity) -> Vec<Modifier> {
        let rules = &self.content.rules;
        let Ok(c) = self.world.get::<&City>(city) else {
            return Vec::new();
        };
        let mut mods = passive_modifiers_for(
            &rules.races.get(c.race).effects,
            Source::Race(c.race),
            &CITY_TARGETS,
        );
        for b in self.city_buildings(city) {
            mods.extend(passive_modifiers_for(
                &rules.buildings.get(b).effects,
                Source::Building(b),
                &CITY_TARGETS,
            ));
        }
        mods
    }

    pub fn city_stat(&self, city: Entity, stat: Stat, base: i32) -> i32 {
        compute(stat, base, &self.city_modifiers(city))
    }

    /// Production points the city makes per turn.
    pub fn city_production(&self, city: Entity) -> i32 {
        self.city_stat(
            city,
            Stat::CityProduction,
            self.content.rules.config.base_city_production,
        )
    }

    /// What `item` costs in production points when built in `city`.
    pub fn item_cost(&self, city: Entity, item: ProductionItem) -> u32 {
        let rules = &self.content.rules;
        match item {
            ProductionItem::Building(b) => rules.buildings.get(b).cost,
            ProductionItem::Unit(u) => {
                let base = rules.units.get(u).cost as i32;
                self.city_stat(city, Stat::UnitCost, base).max(1) as u32
            }
        }
    }

    /// Whether `city` may add `item` to its queue now.
    pub fn check_can_queue(
        &self,
        city: Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        let rules = &self.content.rules;
        let race = self
            .world
            .get::<&City>(city)
            .map_err(|_| ProductionError::NoSuchCity)?
            .race;
        let built = self.city_buildings(city);
        let queue = self.production_queue(city);
        let requires_met = |req: &[String]| {
            req.iter().all(|name| {
                rules
                    .buildings
                    .id_of(name)
                    .is_some_and(|id| built.contains(&id))
            })
        };
        match item {
            ProductionItem::Unit(u) => {
                let def = rules.units.get(u);
                let race_name = &rules.races.get(race).id;
                if def.kind != UnitKind::Normal || def.race.as_ref() != Some(race_name) {
                    return Err(ProductionError::NotAvailable);
                }
                if !requires_met(&def.requires) {
                    return Err(ProductionError::MissingRequirement);
                }
            }
            ProductionItem::Building(b) => {
                let def = rules.buildings.get(b);
                if !def.races.is_empty() && !def.races.contains(&rules.races.get(race).id) {
                    return Err(ProductionError::NotAvailable);
                }
                if built.contains(&b) {
                    return Err(ProductionError::AlreadyBuilt);
                }
                if queue.iter().any(|e| e.item == item) {
                    return Err(ProductionError::AlreadyQueued);
                }
                if !requires_met(&def.requires) {
                    return Err(ProductionError::MissingRequirement);
                }
            }
        }
        Ok(())
    }

    fn city_owned_by(&self, city: Entity, by: PlayerId) -> Result<(), ProductionError> {
        let c = self
            .world
            .get::<&City>(city)
            .map_err(|_| ProductionError::NoSuchCity)?;
        if c.owner == by {
            Ok(())
        } else {
            Err(ProductionError::NotYours)
        }
    }

    pub(crate) fn queue_production(
        &mut self,
        by: PlayerId,
        city: Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        self.city_owned_by(city, by)?;
        self.check_can_queue(city, item)?;
        let mut q = self
            .world
            .get::<&mut ProductionQueue>(city)
            .map_err(|_| ProductionError::NoSuchCity)?;
        q.0.push(QueueEntry { item, progress: 0 });
        Ok(())
    }

    pub(crate) fn dequeue_production(
        &mut self,
        by: PlayerId,
        city: Entity,
        index: usize,
    ) -> Result<(), ProductionError> {
        self.city_owned_by(city, by)?;
        let mut q = self
            .world
            .get::<&mut ProductionQueue>(city)
            .map_err(|_| ProductionError::NoSuchCity)?;
        if index >= q.0.len() {
            return Err(ProductionError::BadIndex);
        }
        q.0.remove(index);
        Ok(())
    }

    /// Spend each city's production on the head of its queue, completing items.
    /// Surplus carries over to the next item; a unit waits while its city's tile
    /// is full.
    pub(crate) fn process_production(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for (city, c, pos) in self.cities() {
            let mut available = self.city_production(city).max(0) as u32;
            while let Some(entry) = self.production_queue(city).first().copied() {
                let cost = self.item_cost(city, entry.item);
                let progress = entry.progress + available;
                let blocked = matches!(entry.item, ProductionItem::Unit(_))
                    && self.units_at(pos).len() as u32 >= self.content.rules.config.max_stack;
                if progress < cost || blocked {
                    if let Ok(mut q) = self.world.get::<&mut ProductionQueue>(city) {
                        q.0[0].progress = progress.min(cost);
                    }
                    break;
                }
                if let Ok(mut q) = self.world.get::<&mut ProductionQueue>(city) {
                    q.0.remove(0);
                }
                available = progress - cost;
                match entry.item {
                    ProductionItem::Unit(def) => {
                        let unit = self.spawn_unit(c.owner, def, pos);
                        events.push(Event::UnitTrained { city, unit, pos });
                    }
                    ProductionItem::Building(building) => {
                        if let Ok(mut b) = self.world.get::<&mut CityBuildings>(city) {
                            b.0.insert(building);
                        }
                        events.push(Event::BuildingCompleted { city, building });
                    }
                }
            }
        }
        events
    }

    /// Gold, food and mana a player pays each turn for units and buildings.
    pub fn player_upkeep(&self, player: PlayerId) -> Upkeep {
        let rules = &self.content.rules;
        let mut total = Upkeep::default();
        for (_, (unit, owner)) in self.world.query::<(&Unit, &Owner)>().iter() {
            if owner.0 == player {
                let u = rules.units.get(unit.def).upkeep;
                total.gold += u.gold;
                total.food += u.food;
                total.mana += u.mana;
            }
        }
        for (city, c, _) in self.cities() {
            if c.owner == player {
                total.gold += self
                    .city_buildings(city)
                    .iter()
                    .map(|&b| rules.buildings.get(b).upkeep_gold)
                    .sum::<u32>();
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Plane::Arcanus;
    use crate::testutil::*;
    use crate::units::MovesLeft;
    use crate::{Command, Rejection};

    fn queue(
        g: &mut Game,
        by: PlayerId,
        city: Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        match g.apply(Command::QueueProduction { by, city, item })[..] {
            [Event::ProductionQueued { .. }] => Ok(()),
            [Event::Rejected(Rejection::Production(e))] => Err(e),
            ref o => panic!("{o:?}"),
        }
    }

    fn unit(g: &Game, n: &str) -> ProductionItem {
        ProductionItem::Unit(unit_id(g, n))
    }
    fn building(g: &Game, n: &str) -> ProductionItem {
        ProductionItem::Building(building_id(g, n))
    }

    fn turns(g: &mut Game, n: u32) -> Vec<Event> {
        (0..n).flat_map(|_| g.apply(Command::EndTurn)).collect()
    }

    #[test]
    fn availability_follows_race_and_requirements() {
        let mut g = game();
        let city = found(&mut g, P0, "sunfolk", at(Arcanus, 3, 3));
        let (spear, axe, pike, hero) = (
            unit(&g, "spearman"),
            unit(&g, "axeman"),
            unit(&g, "pikeman"),
            unit(&g, "hero"),
        );
        let (barracks, forge, armoury) = (
            building(&g, "barracks"),
            building(&g, "forge"),
            building(&g, "armoury"),
        );
        assert_eq!(queue(&mut g, P0, city, spear), Ok(()));
        assert_eq!(queue(&mut g, P0, city, spear), Ok(()), "units may repeat");
        assert_eq!(
            queue(&mut g, P0, city, axe),
            Err(ProductionError::NotAvailable),
            "other race"
        );
        assert_eq!(
            queue(&mut g, P0, city, hero),
            Err(ProductionError::NotAvailable),
            "heroes are not built"
        );
        assert_eq!(
            queue(&mut g, P0, city, pike),
            Err(ProductionError::MissingRequirement)
        );
        assert_eq!(
            queue(&mut g, P0, city, forge),
            Err(ProductionError::NotAvailable),
            "race-restricted building"
        );
        assert_eq!(
            queue(&mut g, P0, city, armoury),
            Err(ProductionError::MissingRequirement)
        );
        assert_eq!(
            queue(&mut g, P1, city, barracks),
            Err(ProductionError::NotYours)
        );
        assert_eq!(queue(&mut g, P0, city, barracks), Ok(()));
        assert_eq!(
            queue(&mut g, P0, city, barracks),
            Err(ProductionError::AlreadyQueued)
        );
        assert_eq!(
            queue(&mut g, P0, Entity::DANGLING, barracks),
            Err(ProductionError::NoSuchCity)
        );
        assert_eq!(g.production_queue(city).len(), 3);
    }

    #[test]
    fn dequeue_removes_entries() {
        let mut g = game();
        let city = found(&mut g, P0, "sunfolk", at(Arcanus, 3, 3));
        let spearman = unit(&g, "spearman");
        queue(&mut g, P0, city, spearman).unwrap();
        let bad = g.apply(Command::DequeueProduction {
            by: P0,
            city,
            index: 5,
        });
        assert_eq!(
            bad,
            vec![Event::Rejected(Rejection::Production(
                ProductionError::BadIndex
            ))]
        );
        let ok = g.apply(Command::DequeueProduction {
            by: P0,
            city,
            index: 0,
        });
        assert_eq!(ok, vec![Event::ProductionDequeued { city, index: 0 }]);
        assert!(g.production_queue(city).is_empty());
    }

    #[test]
    fn building_completes_after_enough_turns_and_unlocks_units() {
        let mut g = game();
        let city = found(&mut g, P0, "sunfolk", at(Arcanus, 3, 3));
        let barracks = building_id(&g, "barracks");
        queue(&mut g, P0, city, ProductionItem::Building(barracks)).unwrap();
        assert_eq!(g.city_production(city), 2);
        let events = turns(&mut g, 9);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::BuildingCompleted { .. })),
            "20 cost / 2 per turn"
        );
        let events = turns(&mut g, 1);
        assert!(events.contains(&Event::BuildingCompleted {
            city,
            building: barracks
        }));
        assert!(g.city_buildings(city).contains(&barracks));
        assert!(g.production_queue(city).is_empty());
        let pikeman_item = unit(&g, "pikeman");
        assert_eq!(
            queue(&mut g, P0, city, pikeman_item),
            Ok(()),
            "barracks unlocks pikemen"
        );
        assert_eq!(
            queue(&mut g, P0, city, ProductionItem::Building(barracks)),
            Err(ProductionError::AlreadyBuilt)
        );
    }

    #[test]
    fn units_appear_in_the_city_with_full_moves_and_surplus_carries_over() {
        let mut g = game();
        let pos = at(Arcanus, 3, 3);
        let city = found(&mut g, P0, "sunfolk", pos);
        let spear = unit_id(&g, "spearman");
        queue(&mut g, P0, city, ProductionItem::Unit(spear)).unwrap();
        queue(&mut g, P0, city, ProductionItem::Unit(spear)).unwrap();
        g.world.get::<&mut ProductionQueue>(city).unwrap().0[0].progress = 29;
        let events = turns(&mut g, 1);
        let trained: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::UnitTrained { .. }))
            .collect();
        assert_eq!(trained.len(), 1);
        let units = g.units_at(pos);
        assert_eq!(units.len(), 1);
        assert_eq!(g.unit_owner(units[0]), Some(P0));
        assert_eq!(g.world.get::<&MovesLeft>(units[0]).unwrap().0, 2);
        let q = g.production_queue(city);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].progress, 1, "29 + 2 - 30 carried to the next item");
    }

    #[test]
    fn buildings_and_race_change_city_modifiers_but_only_for_the_city() {
        let mut g = game();
        let city = found(&mut g, P0, "ironclans", at(Arcanus, 3, 3));
        assert_eq!(g.city_production(city), 3, "base 2 + race 1");
        g.world
            .get::<&mut CityBuildings>(city)
            .unwrap()
            .0
            .insert(building_id(&g, "forge"));
        assert_eq!(g.city_production(city), 5);
        // Armoury halves unit cost; its Owner-targeted mana bonus is not a city modifier.
        g.world
            .get::<&mut CityBuildings>(city)
            .unwrap()
            .0
            .insert(building_id(&g, "armoury"));
        let axe = ProductionItem::Unit(unit_id(&g, "axeman"));
        assert_eq!(g.item_cost(city, axe), 20, "40 * 50%");
        assert!(g
            .city_modifiers(city)
            .iter()
            .all(|m| m.stat != Stat::ManaIncome));
        let bld = ProductionItem::Building(building_id(&g, "barracks"));
        assert_eq!(g.item_cost(city, bld), 20, "buildings are not discounted");
    }

    #[test]
    fn a_full_city_tile_holds_back_new_units() {
        let mut g = game();
        let pos = at(Arcanus, 3, 3);
        let city = found(&mut g, P0, "sunfolk", pos);
        let spear = unit_id(&g, "spearman");
        let fillers: Vec<_> = (0..9).map(|_| g.spawn_unit(P0, spear, pos)).collect();
        queue(&mut g, P0, city, ProductionItem::Unit(spear)).unwrap();
        g.world.get::<&mut ProductionQueue>(city).unwrap().0[0].progress = 30;
        let events = turns(&mut g, 1);
        assert!(!events
            .iter()
            .any(|e| matches!(e, Event::UnitTrained { .. })));
        assert_eq!(
            g.production_queue(city)[0].progress,
            30,
            "progress is kept, not overflowed"
        );
        g.apply(Command::DisbandUnit {
            by: P0,
            unit: fillers[0],
        });
        let events = turns(&mut g, 1);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::UnitTrained { .. })));
    }

    #[test]
    fn upkeep_sums_units_and_buildings_per_player() {
        let mut g = game();
        let city = found(&mut g, P0, "sunfolk", at(Arcanus, 3, 3));
        let pos = at(Arcanus, 6, 6);
        let (spear, bat) = (unit_id(&g, "spearman"), unit_id(&g, "bat"));
        g.spawn_unit(P0, spear, pos);
        g.spawn_unit(P0, bat, pos);
        g.spawn_unit(P1, spear, pos);
        for b in ["barracks", "armoury"] {
            g.world
                .get::<&mut CityBuildings>(city)
                .unwrap()
                .0
                .insert(building_id(&g, b));
        }
        let up = g.player_upkeep(P0);
        assert_eq!((up.gold, up.food, up.mana), (1 + 1 + 2, 0, 1));
        assert_eq!(g.player_upkeep(P1).gold, 1);
    }
}
