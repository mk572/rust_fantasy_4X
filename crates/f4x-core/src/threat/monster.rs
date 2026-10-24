//! Monsters (`Make_Monsters`, `AIDATA.c`): now and then a stack of monsters of a lair's realm
//! settles in that lair, and the monsters that lived there are driven out. The original makes
//! the new stack walk the land beside the lair; here it takes the lair at once and the evicted
//! guards roam as the monsters' units (`f4x_ai::play_neutrals`): they look for a stand that
//! nobody holds and settle in it, or attack the first city they find. A city that falls to them
//! is wrecked, and with luck turns into a lair of its own (`Rampage_Combat_City`, `Combat.c`);
//! the monsters that did it are gone from the map either way, into the ruins or the wilderness.

use crate::feature::def::FeatureKind;
use crate::feature::entity::Placement;
use crate::feature::lair::{candidates, Lair};
use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::MONSTERS;
use crate::unit::{Unit, MAX_STACK};

use super::MONSTER_FIRST_TURN;

/// The realm that keeps its monsters at home.
const LIFE: &str = "life";

/// Extra damage the fighting toll counts when monsters win a city: it makes the share of
/// citizens killed 50 percent and the chance of a building falling 50 more than the battle's.
const RAMPAGE_DAMAGE: u32 = 40;

/// The cheapest monster a budget still buys.
const CHEAPEST: i32 = 25;

impl Game {
    /// The realm of the creatures that guard a site, `None` for a site without guards.
    fn guard_realm_of(&self, guards: &[(String, u32)]) -> Option<String> {
        let rules = &self.content.rules;
        let first = rules.units.id_of(&guards.first()?.0)?;
        rules.units.get(first).realm.clone()
    }

    /// Run the monsters' clock and, when it runs out, let a stack take over a random lair.
    pub(super) fn make_monsters(&mut self) -> Vec<Event> {
        let d = u32::from(self.difficulty());
        let zones: Vec<_> = self
            .economy()
            .lairs()
            .into_iter()
            .filter(|l| l.intact && !l.guards.is_empty())
            .filter(|l| self.guard_realm_of(&l.guards).is_some_and(|r| r != LIFE))
            .collect();
        if zones.is_empty() {
            return Vec::new();
        }
        self.threat.monster_clock += self.roll(d + 1);
        if self.threat.monster_clock < 50u32.saturating_sub(5 * d) {
            return Vec::new();
        }
        self.threat.monster_clock = 0;
        if self.turn < MONSTER_FIRST_TURN {
            return Vec::new();
        }
        // A lair on a landmass where a player has a city.
        let lands = self.all_landmasses();
        let cities: Vec<(crate::player::PlayerId, Position)> = self
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| !self.is_neutral_player(c.owner))
            .map(|(_, c, p)| (c.owner, p))
            .collect();
        let near_players: Vec<_> = zones
            .into_iter()
            .filter(|l| {
                let land = self.landmass_at(&lands, l.pos);
                land != 0
                    && cities
                        .iter()
                        .any(|&(_, p)| self.landmass_at(&lands, p) == land)
            })
            .collect();
        if near_players.is_empty() {
            return Vec::new();
        }
        let zone = near_players[self.roll(near_players.len() as u32) as usize - 1].clone();
        let Some(realm) = self.guard_realm_of(&zone.guards) else {
            return Vec::new();
        };
        let Some(exile_square) = self.adjacent_free_square(zone.pos) else {
            return Vec::new();
        };
        let mut budget = ((self.roll(d + 1) + self.roll(d + 1)) * self.turn / 5) as i32;
        // On a landmass that holds a computer wizard's cities and none of the human's the
        // budget is halved (the original's "AI home continent").
        let land = self.landmass_at(&lands, zone.pos);
        let human = self.human();
        let on_land = |p: Position| self.landmass_at(&lands, p) == land;
        let human_here = cities.iter().any(|&(o, p)| Some(o) == human && on_land(p));
        let ai_here = cities.iter().any(|&(o, p)| Some(o) != human && on_land(p));
        if ai_here && !human_here {
            budget /= 2;
        }
        let list = self.monster_list(budget, &realm);
        if list.is_empty() {
            return Vec::new();
        }
        let evicted = self.evict_guards(zone.entity, zone.pos, exile_square);
        let guards = tally(&list);
        if let Ok(mut lair) = self.world.get::<&mut Lair>(zone.entity) {
            lair.guards = guards;
            lair.fought_by.clear();
        }
        vec![Event::MonstersSettled {
            pos: zone.pos,
            site: zone.site,
            count: list.len() as u32,
            evicted,
        }]
    }

    /// The monsters a budget buys (`Make_Monster_List`): random creatures of the realm that cost
    /// no more than what is left, as long as 25 is left, nine at most.
    fn monster_list(&mut self, budget: i32, realm: &str) -> Vec<String> {
        let pool = candidates(&self.content, realm);
        let mut left = budget.max(0);
        let mut list = Vec::new();
        if pool.is_empty() {
            return list;
        }
        for _ in 0..2000 {
            if left < CHEAPEST || list.len() >= MAX_STACK {
                break;
            }
            let (id, cost) = &pool[self.roll(pool.len() as u32) as usize - 1];
            if *cost <= left {
                left -= cost;
                list.push(id.clone());
            }
        }
        list
    }

    /// Make the guards of a lair real units of the monsters on `to`. Returns how many.
    fn evict_guards(&mut self, lair: hecs::Entity, at: Position, to: Position) -> u32 {
        let guards = self
            .world
            .get::<&Lair>(lair)
            .map(|l| l.guards.clone())
            .unwrap_or_default();
        let mut evicted = 0;
        for (id, count) in guards {
            let Some(kind) = self.content.rules.units.id_of(&id) else {
                continue;
            };
            for _ in 0..count {
                let unit = self.spawn_unit(MONSTERS, to, kind);
                let sight = self.units().unit_sight(unit) as u32;
                self.explore(MONSTERS, to, sight);
                evicted += 1;
            }
        }
        self.explore(MONSTERS, at, crate::map::fog::sight_for(2));
        evicted
    }

    /// A stack of the monsters has come to `to`: if nobody holds the stand there, they settle in
    /// it (the units become its guards).
    pub(crate) fn occupy_zone(&mut self, units: &[hecs::Entity], to: Position) -> Vec<Event> {
        let Some(zone) = self.vacant_zone(to) else {
            return Vec::new();
        };
        let mut guards: Vec<String> = Vec::new();
        for &u in units {
            if let Some(def) = self.units().unit_def(u) {
                guards.push(def.id.clone());
            }
        }
        if guards.is_empty() {
            return Vec::new();
        }
        for &u in units {
            self.despawn_with_spells(u);
        }
        let site = self
            .economy()
            .lair_of(zone)
            .map(|l| l.site)
            .unwrap_or_default();
        if let Ok(mut lair) = self.world.get::<&mut Lair>(zone) {
            lair.intact = true;
            lair.guards = tally(&guards);
            lair.gold = 0;
            lair.mana = 0;
            lair.items.clear();
            lair.prisoner = false;
            lair.special = 0;
            lair.fought_by.clear();
        }
        vec![Event::MonstersSettled {
            pos: to,
            site,
            count: guards.len() as u32,
            evicted: 0,
        }]
    }

    /// The site on a square that stands empty: a lair without guards, or a node or a tower that
    /// was cleared and has no owner.
    pub fn vacant_zone(&self, at: Position) -> Option<hecs::Entity> {
        let economy = self.economy();
        let entity = economy.feature_entity_at(at)?;
        let lair = economy.lair_of(entity)?;
        if !lair.guards.is_empty()
            || !self
                .units()
                .units_at(at)
                .iter()
                .all(|&u| self.units().unit_owner(u) == Some(MONSTERS))
        {
            return None;
        }
        match economy.feature_kind_at(at)? {
            FeatureKind::Encounter => lair.intact.then_some(entity),
            FeatureKind::Node => economy
                .node_at(at)
                .is_none_or(|n| n.owner.is_none())
                .then_some(entity),
            FeatureKind::Tower => economy.tower_owner(at).is_none().then_some(entity),
            _ => None,
        }
    }

    /// The monsters have beaten the defenders of a city, or walked into an empty one: the
    /// fighting costs it citizens and buildings, but the monsters do not hold it. Half the time
    /// (never at a wizard's fortress) it is razed and the survivors settle in the ruins, with
    /// the city's gold as their hoard; otherwise they vanish into the wilderness
    /// (`Rampage_Combat_City`).
    pub(crate) fn rampage_city(
        &mut self,
        city: hecs::Entity,
        city_damage: u32,
        monsters: &[hecs::Entity],
    ) -> Vec<Event> {
        let Some(owner) = self.economy().city_owner(city) else {
            return Vec::new();
        };
        let Some(pos) = self.world.get::<&Position>(city).ok().map(|p| *p) else {
            return Vec::new();
        };
        let name = self
            .world
            .get::<&crate::city::City>(city)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let citizens = self.economy().citizens(city);
        let built: Vec<_> = self
            .world
            .get::<&crate::city::economy::Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        // A rampage is hard on a city: the original adds 50 percent to the losses of both kinds.
        let toll = crate::city::conquest::fighting_toll(
            citizens,
            city_damage + RAMPAGE_DAMAGE,
            &built,
            &mut self.rng,
            |b| {
                crate::city::conquest::destroy_building(
                    &mut self.world,
                    self.map.as_ref(),
                    &self.content,
                    city,
                    b,
                )
            },
        );
        if let Ok(mut pop) = self
            .world
            .get::<&mut crate::city::economy::Population>(city)
        {
            pop.figures -= toll.citizens_lost;
        }
        let ruins = self.content.features.id_of("ruins");
        let ruined = self.roll(2) == 2 && !self.economy().is_fortress_city(city);
        let (Some(feature), true) = (ruins, ruined) else {
            for &m in monsters {
                self.despawn_with_spells(m);
            }
            return vec![Event::CityRampaged {
                pos,
                name,
                owner,
                ruined: false,
            }];
        };
        let gold = self.economy().conquest_loot(city).max(0);
        if let Some(t) = self.treasury_mut(owner) {
            t.gold = (t.gold - gold).max(0);
        }
        // The two dearest kinds among the monsters stay to guard the ruins.
        let rules = &self.content.rules;
        let mut kinds: Vec<(String, u32)> = Vec::new();
        for &m in monsters {
            if let Some(def) = self
                .world
                .get::<&Unit>(m)
                .ok()
                .map(|u| rules.units.get(u.0))
            {
                match kinds.iter_mut().find(|(id, _)| *id == def.id) {
                    Some((_, n)) => *n += 1,
                    None => kinds.push((def.id.clone(), 1)),
                }
            }
        }
        let cost = |id: &str| rules.units.id_of(id).map_or(0, |k| rules.units.get(k).cost);
        kinds.sort_by_key(|(id, _)| std::cmp::Reverse(cost(id)));
        kinds.truncate(2);
        for &m in monsters {
            self.despawn_with_spells(m);
        }
        let mut events = match self.destroy_city(city) {
            Ok(e) => e,
            Err(_) => {
                self.despawn_with_spells(city);
                vec![Event::CityDestroyed { pos }]
            }
        };
        self.spawn_feature(Placement {
            at: pos,
            feature,
            node: None,
            lair: Some(Lair {
                intact: true,
                guards: kinds,
                gold: gold as u32,
                mana: 0,
                items: Vec::new(),
                prisoner: false,
                special: 0,
                fought_by: Vec::new(),
            }),
        });
        events.push(Event::CityRampaged {
            pos,
            name,
            owner,
            ruined: true,
        });
        events
    }
}

/// A list of unit ids as a guard list: each kind once with its count, in the order met.
fn tally(list: &[String]) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for id in list {
        match out.iter_mut().find(|(k, _)| k == id) {
            Some((_, n)) => *n += 1,
            None => out.push((id.clone(), 1)),
        }
    }
    out
}
