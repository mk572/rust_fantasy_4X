//! How cities grow, outposts included, and how a city is founded (task 188). The sums are plain
//! functions over the people and the random numbers ([`grow_population`], [`outpost_step`],
//! [`outpost_shrink_chance`]); the `Game` side reads the city and applies the result.

use crate::city::economy::{Buildings, CityRace, Economy, Enchantments, Farmers, Population};
use crate::city::production::{Production, ProductionItem};
use crate::city::City;
use crate::game::rng::GameRng;
use crate::game::Game;
use crate::map::Position;
use crate::player::PlayerId;

/// Apply `delta_tens` to a city's people: tens of 100 make a citizen (up to `cap`); a city
/// that loses people drops a citizen but never below one, and never below zero tens.
pub(crate) fn grow_population(mut pop: Population, delta_tens: i32, cap: u32) -> Population {
    let mut tens = pop.tens as i32 + delta_tens;
    let mut figures = pop.figures as i32;
    while tens >= 100 && figures < cap as i32 {
        tens -= 100;
        figures += 1;
    }
    while tens < 0 {
        if figures > 1 {
            figures -= 1;
            tens += 100;
        } else {
            tens = 0;
        }
    }
    pop.figures = figures as u32;
    pop.tens = tens.clamp(0, 99) as u32;
    pop
}

/// The chance (percent) that an outpost loses people in a turn: 5, and more for each curse on
/// it. `enchanted` says whether the outpost has the enchantment of that name.
pub(crate) fn outpost_shrink_chance(enchanted: impl Fn(&str) -> bool) -> i32 {
    let mut shrink = 5;
    for (name, extra) in [
        ("evil_presence", 5),
        ("pestilence", 10),
        ("famine", 10),
        ("chaos_rift", 10),
    ] {
        if enchanted(name) {
            shrink += extra;
        }
    }
    shrink
}

/// One turn of an outpost: by chance it gains up to three tens of people, and by another chance
/// it loses up to two; at 100 tens it becomes a city of one. The draws are in this order.
pub(crate) fn outpost_step(
    mut pop: Population,
    grow: i32,
    shrink: i32,
    rng: &mut GameRng,
) -> Population {
    if rng.roll(100) as i32 <= grow {
        pop.tens += rng.roll(3);
    }
    if rng.roll(100) as i32 <= shrink {
        pop.tens = pop.tens.saturating_sub(rng.roll(2));
    }
    if pop.tens >= 100 {
        pop.figures = 1;
        pop.tens -= 100;
    }
    pop
}

impl Game {
    /// Divide a city's population into whole citizens and tens, applying `delta_tens`.
    pub(crate) fn apply_growth(&mut self, city: hecs::Entity, delta_tens: i32) -> (u32, u32) {
        let pop = self
            .world
            .get::<&Population>(city)
            .map(|p| *p)
            .unwrap_or_default();
        let grown = grow_population(pop, delta_tens, self.content.rules.config.max_city_size);
        let _ = self.world.insert_one(city, grown);
        (pop.figures, grown.figures)
    }

    /// Outposts grow a few tens of people at a time, by chance.
    pub(crate) fn grow_outpost(&mut self, city: hecs::Entity) -> bool {
        if self.world.get::<&Position>(city).is_err() {
            return false;
        }
        let grow = self.economy().outpost_growth_chance(city);
        let shrink = outpost_shrink_chance(|name| self.economy().city_has_enchantment(city, name));
        let pop = self
            .world
            .get::<&Population>(city)
            .map(|p| *p)
            .unwrap_or_default();
        let pop = outpost_step(pop, grow, shrink, &mut self.rng);
        let _ = self.world.insert_one(city, pop);
        pop.figures > 0
    }

    /// Found a city (or outpost, with no population) with all its economic components.
    pub(crate) fn spawn_city(
        &mut self,
        owner: PlayerId,
        pos: Position,
        name: String,
        race: Option<crate::rules::RaceId>,
        figures: u32,
    ) -> hecs::Entity {
        let serial = self.take_serial();
        let city = self.world.spawn((
            serial,
            City { name },
            crate::player::Owner(self.player_entity(owner).expect("a city has an owner")),
            pos,
            Population { figures, tens: 0 },
            Farmers(0),
            Buildings::default(),
            Enchantments::default(),
            Production {
                item: Some(ProductionItem::Housing),
                progress: 0,
            },
        ));
        if let Some(r) = race {
            let _ = self.world.insert_one(city, CityRace(r));
            if self
                .wizard(owner)
                .is_some_and(|w| w.capital_race().is_none())
            {
                self.wizard_set(owner, crate::wizard::entity::CapitalRace(Some(r)));
            }
        }
        let farmers = self.economy().city_min_farmers(city);
        let _ = self.world.insert_one(city, Farmers(farmers));
        if !self.is_neutral_player(owner) {
            let sight = self.economy().city_sight(city).max(0) as u32;
            self.explore(owner, pos, sight);
        }
        city
    }
}

impl Economy<'_> {
    /// The chance (percent) that an outpost gains people this turn: the land's food, the race,
    /// the minerals and wild game in reach, and Gaia's Blessing and Stream of Life. The
    /// original added a difficulty modifier here and ignored wild game's food; we do neither
    /// (the computer wizards' bonus is applied at the end).
    pub(crate) fn outpost_growth_chance(&self, city: hecs::Entity) -> i32 {
        let Some(at) = self.world.get::<&Position>(city).ok().map(|p| *p) else {
            return 0;
        };
        let race = self.world.get::<&CityRace>(city).ok().map(|r| r.0);
        let mut grow = race.map_or(10, |r| {
            i32::from(self.content.rules.races.get(r).outpost_growth)
        });
        if let Some(grid) = self.map.map(|m| m.plane(at.plane)) {
            // Food from the land, wild game included (it counts in the city's food).
            grow += self
                .city_economy_with(city, 0)
                .map_or(0, |e| e.max_size as i32);
            for (p, _) in self.city_area(at) {
                if let Some(f) = grid
                    .feature(p.x as i32, p.y as i32)
                    .and_then(|f| self.content.features.get(f))
                {
                    grow += f.city.outpost_growth;
                }
            }
        }
        let enchanted = |name: &str| self.city_has_enchantment(city, name);
        if enchanted("gaias_blessing") {
            grow += 20;
        }
        if enchanted("stream_of_life") {
            grow += 10;
        }
        // A computer wizard's difficulty bonus (the original applied it here, too).
        let owner = self.city_owner(city);
        let bonus = owner
            .and_then(|o| crate::player::component::<crate::player::Bonus>(self.world, o))
            .map_or(100, |b| b.outpost_growth);
        crate::player::Bonus::of(bonus, grow)
    }
}

#[cfg(test)]
mod tests;
