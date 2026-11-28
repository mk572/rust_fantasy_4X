//! Monsters (`Make_Monsters`, `AIDATA.c`): now and then a stack of monsters of a lair's realm
//! settles in that lair, and the monsters that lived there are driven out. The original makes
//! the new stack walk the land beside the lair; here it takes the lair at once and the evicted
//! guards roam as the monsters' units (`f4x_ai::NeutralAi`): they look for a stand that
//! nobody holds and settle in it, or attack the first city they find. A city that falls to them
//! is wrecked, and with luck turns into a lair of its own (`Rampage_Combat_City`, `Combat.c`);
//! the monsters that did it are gone from the map either way, into the ruins or the wilderness.
//!
//! Tests: `tests/it/c9_threat.rs`.

use crate::feature::def::FeatureKind;
use crate::feature::entity::Placement;
use crate::feature::lair::{candidates, create, Lair};
use crate::map::Position;
use crate::player::MONSTERS;
use crate::rules::{RealmId, UnitId};
use crate::unit::Unit;
use crate::world::rng::{GameRng, RngCore};
use crate::world::{Event, World};
use f4x_def::half_int::HalfInt;
use f4x_def::settings::MONSTER_START_TURN;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

// The realms that keep their monsters at home (`spared_realms`), the extra damage a rampage counts and the cheapest monster a budget buys are `threat` of `rules.ron`.

impl World {
    /// The realm of the creatures that guard a site, `None` for a site without guards.
    fn guard_realm_of(&self, guards: &[(UnitId, u32)]) -> Option<RealmId> {
        let rules = &self.content.rules;
        rules.units.get(guards.first()?.0).realm
    }

    /// The turn from which monsters leave their lairs: the setting on Average and harder, the
    /// original's 50 on Intro and Easy (patch 1.40 sets 40 only above Easy).
    pub(crate) fn monster_first_turn(&self) -> u32 {
        if self.difficulty() >= 2 {
            u32::from(self.settings.world.monster_start_turn)
        } else {
            u32::from(MONSTER_START_TURN)
        }
    }

    /// Run the monsters' clock and, when it runs out, let a stack take over a random lair.
    pub(super) fn make_monsters(&mut self, round: u32, rng: &mut GameRng) -> Vec<Event> {
        let d = u32::from(self.difficulty());
        let spared = self.content.rules.config.threat.spared_realms.clone();
        let zones: Vec<_> = self
            .economy()
            .lairs()
            .into_iter()
            .filter(|l| l.intact && !l.guards.is_empty())
            .filter(|l| {
                self.guard_realm_of(&l.guards)
                    .is_some_and(|r| !spared.contains(&r.num()))
            })
            .collect();
        if zones.is_empty() {
            return Vec::new();
        }
        self.threat.monster_clock += rng.roll(d + 1);
        if self.threat.monster_clock < 50u32.saturating_sub(5 * d) {
            return Vec::new();
        }
        self.threat.monster_clock = 0;
        if round < self.monster_first_turn() {
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
                cities
                    .iter()
                    .any(|&(_, p)| self.same_land(&lands, l.pos, p))
            })
            .collect();
        if near_players.is_empty() {
            return Vec::new();
        }
        let zone = near_players[rng.roll(near_players.len() as u32) as usize - 1].clone();
        let Some(realm) = self.guard_realm_of(&zone.guards) else {
            return Vec::new();
        };
        let Some(exile_square) = self.adjacent_free_square(zone.pos) else {
            return Vec::new();
        };
        let mut budget = ((rng.roll(d + 1) + rng.roll(d + 1)) * round / 5) as i32;
        // On a landmass that holds a computer wizard's cities and none of the human's the budget is
        // halved.
        let human = self.human();
        let on_land = |p: Position| self.same_land(&lands, zone.pos, p);
        let human_here = cities.iter().any(|&(o, p)| Some(o) == human && on_land(p));
        let ai_here = cities.iter().any(|&(o, p)| Some(o) != human && on_land(p));
        if ai_here && !human_here {
            budget /= 2;
        }
        let list = self.monster_list(budget, realm, rng);
        if list.is_empty() {
            return Vec::new();
        }
        let evicted = self.evict_guards(zone.entity, zone.pos, exile_square);
        let guards = tally(&list);
        if let Ok(mut lair) = self.ecs.get::<&mut Lair>(zone.entity) {
            lair.guards = guards;
            lair.fought_by.clear();
            lair.look(&self.content, zone.pos);
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
    fn monster_list(&mut self, budget: i32, realm: RealmId, rng: &mut GameRng) -> Vec<UnitId> {
        let pool = candidates(&self.content, realm);
        let mut left = budget.max(0);
        let mut list = Vec::new();
        if pool.is_empty() {
            return list;
        }
        for _ in 0..2000 {
            if left < self.content.rules.config.threat.cheapest_monster
                || list.len() >= self.max_battle_side()
            {
                break;
            }
            let (id, cost) = &pool[rng.roll(pool.len() as u32) as usize - 1];
            if *cost <= left {
                left -= cost;
                list.push(*id);
            }
        }
        list
    }

    /// Make the guards of a lair real units of the monsters on `to`. Returns how many.
    fn evict_guards(&mut self, lair: hecs::Entity, at: Position, to: Position) -> u32 {
        let guards = self
            .ecs
            .get::<&Lair>(lair)
            .map(|l| l.guards.clone())
            .unwrap_or_default();
        let mut evicted = 0;
        for (kind, count) in guards {
            for _ in 0..count {
                let unit = self.spawn_unit(MONSTERS, to, kind);
                let sight = self.units().unit_sight(unit);
                self.explore(MONSTERS, to, sight);
                evicted += 1;
            }
        }
        self.explore(MONSTERS, at, crate::map::fog::sight_for(2));
        evicted
    }

    /// A stack of the monsters has come to `to`: if nobody holds the stand there, they settle in
    /// it (the units become its guards, with a new hoard).
    pub(crate) fn occupy_zone(
        &mut self,
        units: &[hecs::Entity],
        to: Position,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let Some(zone) = self.vacant_zone(to) else {
            return Vec::new();
        };
        let mut guards: Vec<UnitId> = Vec::new();
        for &u in units {
            if let Some(kind) = self.units().unit_kind_id(u) {
                guards.push(kind);
            }
        }
        if guards.is_empty() {
            return Vec::new();
        }
        let Some(site) = self.economy().lair_of(zone).map(|l| l.site) else {
            return Vec::new();
        };
        for &u in units {
            self.despawn_with_spells(u);
        }
        let Some(kind) = self.content.features.get(site).map(create::site_type) else {
            return Vec::new();
        };
        let style = self.lair_style();
        let treasure = i32::from(self.settings.world.lair_treasure_percent.min(500));
        if let Ok(mut lair) = self.ecs.get::<&mut Lair>(zone) {
            lair.intact = true;
            lair.guards = tally(&guards);
            let mut rng = Pcg64Mcg::seed_from_u64(rng.next_u64());
            create::restock(
                &mut rng,
                &self.content,
                style,
                &mut lair,
                kind,
                to.plane,
                treasure,
            );
            lair.fought_by.clear();
            lair.look(&self.content, to);
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
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let Some(owner) = self.economy().city_owner(city) else {
            return Vec::new();
        };
        let Some(pos) = self.ecs.get::<&Position>(city).ok().map(|p| *p) else {
            return Vec::new();
        };
        let name = self
            .ecs
            .get::<&crate::city::City>(city)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let citizens = self.economy().citizens(city);
        let built: Vec<_> = self
            .ecs
            .get::<&crate::city::economy::Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        // A rampage is hard on a city: 50 percent are added to the losses of both kinds.
        let toll = crate::city::conquest::fighting_toll(
            citizens,
            city_damage + self.content.rules.config.threat.rampage_damage,
            &built,
            rng,
            |b| {
                crate::city::conquest::destroy_building(
                    &mut self.ecs,
                    self.map.as_ref(),
                    &self.content,
                    city,
                    b,
                )
                .is_ok()
            },
        );
        self.change_population(city, |p| p.figures -= toll.citizens_lost);
        let ruins = self.content.features.id_numbered(18) /* ruins */;
        let ruined = rng.roll(2) == 2
            && !self.economy().is_fortress_city(city)
            && self.settings.world.lairs_enabled;
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
        let gold = self.economy().conquest_loot(city).max(HalfInt::ZERO);
        if let Some(t) = self.treasury_mut(owner) {
            t.gold = (t.gold - gold).max(HalfInt::ZERO);
        }
        // The two dearest kinds among the monsters stay to guard the ruins.
        let rules = &self.content.rules;
        let mut kinds: Vec<(UnitId, u32)> = Vec::new();
        for &m in monsters {
            if let Some(kind) = self.ecs.get::<&Unit>(m).ok().map(|u| u.0) {
                match kinds.iter_mut().find(|(id, _)| *id == kind) {
                    Some((_, n)) => *n += 1,
                    None => kinds.push((kind, 1)),
                }
            }
        }
        kinds.sort_by_key(|(id, _)| std::cmp::Reverse(rules.units.get(*id).cost));
        kinds.truncate(2);
        for &m in monsters {
            self.despawn_with_spells(m);
        }
        let mut events = self.remove_city(city, rng);
        self.spawn_feature(Placement {
            at: pos,
            feature,
            node: None,
            lair: Some(Lair {
                intact: true,
                guards: kinds,
                gold: gold.floor() as u32,
                mana: 0,
                items: Vec::new(),
                prisoner: false,
                special: 0,
                fought_by: Vec::new(),
                hurt: Vec::new(),
                apparent: None,
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
fn tally(list: &[UnitId]) -> Vec<(UnitId, u32)> {
    let mut out: Vec<(UnitId, u32)> = Vec::new();
    for id in list {
        match out.iter_mut().find(|(k, _)| k == id) {
            Some((_, n)) => *n += 1,
            None => out.push((*id, 1)),
        }
    }
    out
}
