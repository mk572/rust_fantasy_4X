//! Where a city's food, production, gold, power and research come from: the lines that add up to
//! the figure on the city screen.
//!
//! A line is a source and the amount it brings. The lines are worked out from the same inputs as
//! [`Economy::city_economy_with`] and each modifier is applied in turn, its line being what it
//! added, so the lines always sum to the figure; what no line explains (a computer wizard's
//! difficulty bonus, a cap) is the last line, [`Item::Adjustment`].
//!
//! Tests: below.

use super::*;
use crate::rules::{BuildingId, RaceId, SpellId};

/// The resources a city makes that the screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Food,
    Production,
    Gold,
    Power,
    Research,
}

/// What a line of the breakdown stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// The food or the half production the farmers make; the food they make by race.
    Farmers,
    /// The production the workers make.
    Workers,
    /// The gold of the tax rate.
    Taxes,
    /// A dwarvish city's taxes count twice.
    Dwarves,
    /// Mines in the area (gold, power).
    Mines,
    /// The trade of the site: shore, river and roads.
    Trade,
    /// The land's percent of the production.
    Terrain,
    /// Wild game in the area.
    WildGame,
    /// The power each citizen makes.
    Citizens,
    /// The gold a city making Trade Goods gets from its production.
    TradeGoods,
    /// The food the farmers make beyond what the land feeds, which is halved.
    BeyondLand,
    Race(RaceId),
    Building(BuildingId),
    Spell(SpellId),
    /// A modifier with no source of its own (a moon, a religious bonus).
    Other,
    /// What the lines above do not explain: a difficulty bonus, a cap.
    Adjustment,
}

/// One line of a breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    pub item: Item,
    pub amount: HalfInt,
}

/// The lines of one resource and what they add up to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakdown {
    pub resource: Resource,
    pub lines: Vec<Line>,
    /// The sum of the lines: the figure the city screen shows.
    pub total: HalfInt,
    /// What is taken off it: the food the citizens eat, the gold the buildings cost. Nothing for
    /// the others.
    pub upkeep: HalfInt,
}

fn item_of(source: Source) -> Item {
    match source {
        Source::Race(r) => Item::Race(r),
        Source::Building(b) => Item::Building(b),
        Source::Spell(s) => Item::Spell(s),
        _ => Item::Other,
    }
}

/// The modifiers of `stat` with the item each stands for.
fn tagged(stat: Stat, mods: &[Modifier]) -> Vec<(Item, Modifier)> {
    mods.iter()
        .filter(|m| m.stat == stat)
        .map(|m| (item_of(m.source), *m))
        .collect()
}

/// Add the lines of `mods` to `lines`: each is applied in turn (the adds, then the percents, then
/// the bounds) and its line is what the figure changed by. `eval` works the figure out from a
/// list of modifiers. Returns the figure with them all.
fn stepped(
    lines: &mut Vec<Line>,
    mods: &[(Item, Modifier)],
    eval: &dyn Fn(&[Modifier]) -> HalfInt,
) -> HalfInt {
    use crate::rules::ModOp;
    let mut applied: Vec<Modifier> = Vec::new();
    let mut before = eval(&applied);
    for op in [
        ModOp::Add,
        ModOp::Percent,
        ModOp::Min,
        ModOp::Max,
        ModOp::Set,
    ] {
        for (item, m) in mods.iter().filter(|(_, m)| m.op == op) {
            applied.push(*m);
            let now = eval(&applied);
            put(lines, *item, now - before);
            before = now;
        }
    }
    before
}

/// A line, joined to the one of the same item.
fn put(lines: &mut Vec<Line>, item: Item, amount: HalfInt) {
    if amount == HalfInt::ZERO {
        return;
    }
    match lines.iter_mut().find(|l| l.item == item) {
        Some(l) => l.amount += amount,
        None => lines.push(Line { item, amount }),
    }
}

impl Economy<'_> {
    /// Where `resource` of `city` comes from, with `farmers` farming as the city has them.
    pub fn city_breakdown(&self, city: hecs::Entity, resource: Resource) -> Option<Breakdown> {
        let farmers = self.ecs.get::<&Farmers>(city).map_or(0, |f| f.0);
        let eco = self.city_economy_with(city, farmers)?;
        let at = *self.ecs.get::<&Position>(city).ok()?;
        let owner = self.city_owner(city)?;
        let race_id = self.ecs.get::<&CityRace>(city).ok().map(|r| r.0);
        let rules = &self.content.rules;
        let race: Option<&RaceDef> = race_id.map(|r| rules.races.get(r));
        let mods = self.city_modifiers(city);
        let figures = eco.figures;
        let ctx = self.square_ctx(city)?;
        let land = self.land_of(city, at, ctx)?;
        let terrain_food = land.terrain_food.floor_half();
        let (farmers, workers, rebels) = (eco.farmers, eco.workers, eco.rebels);

        let per_worker = self.worker_production(race);
        let mut lines: Vec<Line> = Vec::new();
        let mut upkeep = HalfInt::ZERO;
        match resource {
            Resource::Food => {
                let rate = self.farmer_food(race);
                put(&mut lines, Item::Farmers, rate * farmers as i32);
                // What each farmer's food gains by the buildings, for all the farmers.
                let mut rate_lines = Vec::new();
                stepped(&mut rate_lines, &tagged(Stat::FarmerFood, &mods), &|m| {
                    compute_half(Stat::FarmerFood, rate, m)
                });
                for l in rate_lines {
                    put(&mut lines, l.item, l.amount * farmers as i32);
                }
                let farmer_rate = compute_half(Stat::FarmerFood, rate, &mods);
                let base = farmer_rate * farmers as i32;
                let food = stepped(&mut lines, &tagged(Stat::CityFood, &mods), &|m| {
                    compute_half(Stat::CityFood, base, m)
                });
                if food > terrain_food {
                    let kept = terrain_food + (food - terrain_food) / 2;
                    put(&mut lines, Item::BeyondLand, kept - food);
                }
                stepped(&mut lines, &tagged(Stat::CityFoodFlat, &mods), &|m| {
                    compute_half(Stat::CityFoodFlat, HalfInt::ZERO, m)
                });
                put(&mut lines, Item::WildGame, land.wild_food);
                upkeep = HalfInt::from_int(figures as i32);
            }
            Resource::Production => {
                let worked = per_worker * workers as i32;
                let farmed = HalfInt::HALF * farmers as i32;
                put(&mut lines, Item::Workers, worked);
                put(&mut lines, Item::Farmers, farmed);
                let base = worked + farmed;
                let mut tagged_mods = tagged(Stat::CityProduction, &mods);
                tagged_mods.push((
                    Item::Terrain,
                    Modifier::new(
                        Stat::CityProduction,
                        crate::rules::ModOp::Percent,
                        land.production_percent,
                        Source::Other,
                    ),
                ));
                stepped(&mut lines, &tagged_mods, &|m| {
                    compute_half(Stat::CityProduction, base, m)
                });
            }
            Resource::Gold => {
                let tax_rate = crate::player::component::<crate::player::Tax>(self.ecs, owner)
                    .map_or(1, |t| t.0) as usize;
                let taxes = HalfInt::HALF * ((figures as i32 - rebels as i32) * tax_rate as i32);
                put(&mut lines, Item::Taxes, taxes);
                if ctx.dwarf {
                    put(&mut lines, Item::Dwarves, taxes);
                }
                put(&mut lines, Item::Mines, land.mine_gold);
                let base = if ctx.dwarf { taxes * 2 } else { taxes } + land.mine_gold;
                let mut tagged_mods = tagged(Stat::CityGold, &mods);
                tagged_mods.push((
                    Item::Trade,
                    Modifier::new(
                        Stat::CityGold,
                        crate::rules::ModOp::Percent,
                        self.site_trade_percent(city, at, race_id, figures),
                        Source::Other,
                    ),
                ));
                stepped(&mut lines, &tagged_mods, &|m| {
                    compute_half(Stat::CityGold, base, m)
                });
                if let Ok(p) = self.ecs.get::<&crate::city::production::Production>(city) {
                    if p.item == Some(crate::city::production::ProductionItem::TradeGoods) {
                        let base = per_worker * workers as i32 + HalfInt::HALF * farmers as i32;
                        let mut production_mods = mods.clone();
                        production_mods.push(Modifier::new(
                            Stat::CityProduction,
                            crate::rules::ModOp::Percent,
                            land.production_percent,
                            Source::Other,
                        ));
                        let made = compute_half(Stat::CityProduction, base, &production_mods);
                        put(&mut lines, Item::TradeGoods, made);
                    }
                }
                upkeep = eco.gold_upkeep;
            }
            Resource::Power => {
                let per = race.map_or(HalfInt::ZERO, |r| r.mana_per_citizen) * figures as i32;
                put(&mut lines, Item::Citizens, per);
                put(&mut lines, Item::Mines, land.mine_mana);
                let base = per + land.mine_mana;
                stepped(&mut lines, &tagged(Stat::CityMana, &mods), &|m| {
                    compute_half(Stat::CityMana, base, m).max(HalfInt::ZERO)
                });
            }
            Resource::Research => {
                stepped(&mut lines, &tagged(Stat::CityResearch, &mods), &|m| {
                    HalfInt::from_int(compute(Stat::CityResearch, 0, m))
                });
            }
        }
        let total = match resource {
            Resource::Food => eco.food,
            Resource::Production => eco.production,
            Resource::Gold => eco.gold,
            Resource::Power => eco.mana,
            Resource::Research => HalfInt::from_int(eco.research),
        };
        let sum: HalfInt = lines.iter().map(|l| l.amount).sum();
        put(&mut lines, Item::Adjustment, total - sum);
        Some(Breakdown {
            resource,
            lines,
            total,
            upkeep,
        })
    }
}
