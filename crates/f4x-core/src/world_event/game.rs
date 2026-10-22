//! Random events on the game: the draw each turn (`Determine_Event`) and the playing out of what
//! was drawn and of the events that last (`Event_Twiddle`). The state and the plain rules are in
//! `world_event/mod.rs`; `docs/events.md` has the whole model.
//!
//! The draw picks a kind and a victim and vetoes what cannot happen; what it arms is played out
//! at once, in the order of the table, together with the lasting events that were already going.

use crate::city::conquest::city_size_class;
use crate::city::economy::Population;
use crate::city::production::{Production, ProductionItem};
use crate::game::rng::GameRng;
use crate::item::make::{make_item, Grade};
use crate::map::{FeatureId, Position};
use crate::player::PlayerId;

use crate::game::{Event, Game};
use crate::rules::UnitKind;
use crate::world_event::{
    donation, fires, flat_end, pick_victim, pirates_take, victim_weights, world_end, EventKind,
    EventNotice, Voice, CITY_SIZE_NAMES, ORES,
};

/// A mineral square an event works on: the city whose area holds it, the square and the mineral.
#[derive(Debug, Clone, Copy)]
struct Deposit {
    city: hecs::Entity,
    at: Position,
    ore: FeatureId,
}

/// A square that could hold or lose a mineral: its city, the square and the minerals it could have, with weights.
type Candidate = (hecs::Entity, Position, Vec<(FeatureId, u32)>);

/// What a draw armed.
#[derive(Debug, Clone, Copy)]
struct Armed {
    kind: EventKind,
    victim: PlayerId,
    city: Option<hecs::Entity>,
    /// The neutral city of a marriage.
    other: Option<hecs::Entity>,
    deposit: Option<Deposit>,
    /// A lasting event starting this turn rather than going on.
    fresh: bool,
}

impl Game {
    /// The turn's events: a draw, if random events are on, and the playing out.
    pub(crate) fn process_events(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        let mut rng = std::mem::replace(&mut self.rng, GameRng::lent());
        let armed = if self.settings.events.random_events {
            self.determine_event(&mut rng)
        } else {
            None
        };
        self.twiddle_events(armed, &mut rng, &mut out);
        self.rng = rng;
        out
    }

    /// Make an event happen to a wizard now (an override, for tests and the console). The vetoes
    /// apply, except the wait until turn 150. Returns what the player is told, or why the event
    /// cannot happen.
    pub fn cheat_event(&mut self, kind: EventKind, victim: PlayerId) -> Result<Vec<Event>, String> {
        let turn = self.turn;
        self.turn = turn.max(150);
        let mut rng = std::mem::replace(&mut self.rng, GameRng::lent());
        let armed = self.try_event(kind, victim, &mut rng);
        self.turn = turn;
        let Some(armed) = armed else {
            self.rng = rng;
            return Err(format!(
                "{} cannot happen to that wizard now",
                kind.id().replace('_', " ")
            ));
        };
        let mut out = Vec::new();
        self.play_event(armed, &mut rng, &mut out);
        self.rng = rng;
        self.override_log.push(format!("event {}", kind.id()));
        Ok(out)
    }

    /// `Determine_Event`: whether an event is drawn this turn, and which.
    fn determine_event(&mut self, rng: &mut GameRng) -> Option<Armed> {
        let pressure = self.events.pressure(
            self.turn,
            self.settings.events.pressure_percent,
            self.settings.events.frequency,
        );
        if !fires(pressure, rng.roll(512)) {
            return None;
        }
        for _ in 0..5 {
            let kind = EventKind::ALL[rng.roll(18) as usize - 1];
            let Some(victim) = self.event_victim(kind.is_good(), rng) else {
                continue;
            };
            if let Some(armed) = self.try_event(kind, victim, rng) {
                self.events.drawn(self.turn);
                return Some(armed);
            }
        }
        None
    }

    /// The vetoes of a draw: `None` when the event cannot happen to this wizard now.
    fn try_event(&mut self, kind: EventKind, victim: PlayerId, rng: &mut GameRng) -> Option<Armed> {
        self.economy().fortress_city(victim)?;
        if self.events.is_ongoing(kind) {
            return None;
        }
        if kind.is_world() && self.events.world_event().is_some() {
            return None;
        }
        if kind == EventKind::Piracy && self.treasury(victim).map_or(0, |p| p.gold) < 100 {
            return None;
        }
        let (mut city, mut other) = (None, None);
        if kind.needs_city() {
            city = Some(self.pick_random_city(victim, rng)?);
            if kind == EventKind::Marriage {
                other = Some(self.pick_neutral_city(rng)?);
            }
        }
        if matches!(kind, EventKind::Marriage | EventKind::GreatMeteor) && self.turn < 150 {
            return None;
        }
        if kind == EventKind::Rebellion && !self.can_rebel(victim, city?) {
            return None;
        }
        let deposit = match kind {
            EventKind::Depletion => Some(self.find_deposit(victim, false, rng)?),
            EventKind::NewMinerals => Some(self.find_deposit(victim, true, rng)?),
            _ => None,
        };
        if kind == EventKind::Disjunction
            && self
                .players()
                .iter()
                .all(|(_, p)| p.magic.global.is_empty())
        {
            return None;
        }
        // A plague and a population boom do not share a city.
        let other_kind = match kind {
            EventKind::Plague => Some(EventKind::PopulationBoom),
            EventKind::PopulationBoom => Some(EventKind::Plague),
            _ => None,
        };
        if let (Some(other_kind), Some(c)) = (other_kind, city) {
            if self.events.on_city(other_kind, self.serial_of(c)) {
                return None;
            }
        }
        Some(Armed {
            kind,
            victim,
            city,
            other,
            deposit,
            fresh: kind.lasts(),
        })
    }

    /// `Get_Event_Victim`: the wizard an event of this kind falls on, weighted by power.
    fn event_victim(&mut self, good: bool, rng: &mut GameRng) -> Option<PlayerId> {
        let wizards = self.living_wizards();
        let powers: Vec<i32> = wizards
            .iter()
            .map(|&w| {
                let a = self.astrology(w);
                a.power + a.research + a.strength
            })
            .collect();
        let weights = victim_weights(&powers, good);
        let total: i32 = weights.iter().sum();
        if total < 1 {
            return None;
        }
        pick_victim(&weights, rng.roll(total as u32) as i32).map(|i| wizards[i])
    }

    /// A random city of the wizard that has people in it.
    fn pick_random_city(&self, owner: PlayerId, rng: &mut GameRng) -> Option<hecs::Entity> {
        let cities: Vec<_> = self.cities_with_people(|o| o == owner);
        (!cities.is_empty()).then(|| cities[rng.roll(cities.len() as u32) as usize - 1])
    }

    fn pick_neutral_city(&self, rng: &mut GameRng) -> Option<hecs::Entity> {
        let cities: Vec<_> = self.cities_with_people(|o| self.is_neutral_player(o));
        (!cities.is_empty()).then(|| cities[rng.roll(cities.len() as u32) as usize - 1])
    }

    fn cities_with_people(&self, owned_by: impl Fn(PlayerId) -> bool) -> Vec<hecs::Entity> {
        let mut cities: Vec<_> = self
            .economy()
            .cities()
            .into_iter()
            .filter(|(e, c, _)| {
                owned_by(c.owner)
                    && self
                        .world
                        .get::<&Population>(*e)
                        .is_ok_and(|p| p.figures > 0)
            })
            .map(|(e, ..)| e)
            .collect();
        self.sort_by_serial(&mut cities);
        cities
    }

    /// Whether a city can rise against its wizard: not the fortress city, no hero in it, and no
    /// more conjured creatures than normal units.
    fn can_rebel(&self, owner: PlayerId, city: hecs::Entity) -> bool {
        if self.economy().is_fortress_city(city) {
            return false;
        }
        let Some(pos) = self.city_position(city) else {
            return false;
        };
        let (mut normal, mut conjured) = (0, 0);
        for u in self.units().units_at(pos) {
            if self.units().unit_owner(u) != Some(owner) {
                continue;
            }
            match self.units().unit_def(u).map(|d| d.kind) {
                Some(UnitKind::Hero) => return false,
                Some(UnitKind::Normal) => normal += 1,
                _ => conjured += 1,
            }
        }
        conjured <= normal
    }

    /// A square in the area of one of the wizard's cities that holds a mineral (to deplete) or
    /// could (to find), and the mineral.
    fn find_deposit(&self, owner: PlayerId, new: bool, rng: &mut GameRng) -> Option<Deposit> {
        let map = self.map.as_ref()?;
        let rules = &self.content;
        let ores: Vec<_> = ORES
            .iter()
            .filter_map(|(id, _)| rules.features.id_of(id))
            .collect();
        let mut found: Vec<Candidate> = Vec::new();
        for city in self.cities_with_people(|o| o == owner) {
            let Some(at) = self.city_position(city) else {
                continue;
            };
            for (sq, _) in self.economy().city_area(at) {
                let grid = map.plane(sq.plane);
                let feature = grid.feature(sq.x as i32, sq.y as i32);
                if new {
                    let Some(class) = self.economy().terrain_class_at(sq) else {
                        continue;
                    };
                    if feature.is_some() || class == crate::content::TerrainClass::Ocean {
                        continue;
                    }
                    let options: Vec<_> = rules
                        .features
                        .weighted(crate::content::FeatureKind::Mineral, class, sq.plane)
                        .into_iter()
                        .filter(|(f, _)| ores.contains(f))
                        .collect();
                    if !options.is_empty() {
                        found.push((city, sq, options));
                    }
                } else if let Some(f) = feature.filter(|f| ores.contains(f)) {
                    found.push((city, sq, vec![(f, 1)]));
                }
            }
        }
        if found.is_empty() {
            return None;
        }
        let (city, at, options) = found[rng.roll(found.len() as u32) as usize - 1].clone();
        let total: u32 = options.iter().map(|(_, w)| w).sum();
        let mut left = rng.roll(total);
        let ore = options
            .iter()
            .find(|(_, w)| {
                if left <= *w {
                    true
                } else {
                    left -= w;
                    false
                }
            })
            .map(|(f, _)| *f)?;
        Some(Deposit { city, at, ore })
    }

    /// `Event_Twiddle`: play out the event just drawn and move the lasting ones on.
    fn twiddle_events(&mut self, armed: Option<Armed>, rng: &mut GameRng, out: &mut Vec<Event>) {
        let mut plays: Vec<Armed> = Vec::new();
        for l in self.events.ongoing().copied().collect::<Vec<_>>() {
            if armed.is_some_and(|a| a.kind == l.kind) {
                continue;
            }
            let city = l.city.and_then(|s| self.units().entity_of_serial(s));
            plays.push(Armed {
                kind: l.kind,
                victim: l
                    .victim
                    .and_then(|e| crate::player::id_of(&self.world, e))
                    .unwrap_or(PlayerId(0)),
                city,
                other: None,
                deposit: None,
                fresh: false,
            });
        }
        plays.extend(armed);
        plays.sort_by_key(|a| a.kind);
        for play in plays {
            self.play_event(play, rng, out);
        }
    }

    fn play_event(&mut self, play: Armed, rng: &mut GameRng, out: &mut Vec<Event>) {
        let Armed {
            kind,
            victim,
            city,
            other,
            deposit,
            fresh,
        } = play;
        let mut notice = EventNotice::new(kind, Voice::Started);
        match kind {
            EventKind::GreatMeteor => {
                let Some(city) = city.filter(|&c| self.economy().city_owner(c) == Some(victim))
                else {
                    return;
                };
                let (units, buildings, people) = self.strike_city(city, rng, true);
                notice.lost_units = units;
                notice.lost_buildings = buildings;
                notice.lost_population = people;
            }
            EventKind::Gift => {
                let item = make_item(
                    rng,
                    &self.content.rules,
                    Grade::Artifact,
                    0,
                    &mut self.prefab_taken,
                );
                notice.item = item.name.clone();
                self.give_item(victim, item);
            }
            EventKind::Disjunction => {
                for (_, p) in self.world.query_mut::<&mut crate::player::Player>() {
                    p.magic.global.clear();
                }
                notice.voice = Voice::Happened;
            }
            EventKind::Marriage => {
                let (Some(neutral), Some(_)) = (other, city) else {
                    return;
                };
                self.marry(neutral, victim);
                notice.city = Some(neutral);
            }
            EventKind::Earthquake => {
                let Some(city) = city else {
                    return;
                };
                let (units, buildings, people) = self.strike_city(city, rng, false);
                if units + buildings + people == 0 {
                    return;
                }
                notice.lost_units = units;
                notice.lost_buildings = buildings;
                notice.lost_population = people;
            }
            EventKind::Piracy => {
                let gold = self.treasury(victim).map_or(0, |p| p.gold);
                let take = pirates_take(gold, rng.roll(21)).min(gold);
                if let Some(t) = self.treasury_mut(victim) {
                    t.gold = (t.gold - take).max(0);
                }
                notice.number = take;
            }
            EventKind::Plague => {
                let Some(city) = city else {
                    self.events.end(kind);
                    return;
                };
                if !self.play_plague(city, victim, fresh, rng, &mut notice) {
                    return;
                }
            }
            EventKind::Rebellion => {
                let Some(city) = city else {
                    return;
                };
                self.make_city_neutral(city, victim);
            }
            EventKind::Donation => {
                let gold = donation(rng.roll(100));
                if let Some(t) = self.treasury_mut(victim) {
                    t.gold += gold;
                }
                notice.number = gold;
            }
            EventKind::Depletion | EventKind::NewMinerals => {
                let Some(d) = deposit else {
                    return;
                };
                let feature = (kind == EventKind::NewMinerals).then_some(d.ore);
                if let Some(map) = self.map.as_mut() {
                    map.plane_mut(d.at.plane)
                        .set_feature(d.at.x, d.at.y, feature);
                }
                let id = self
                    .content
                    .features
                    .get(d.ore)
                    .map_or("", |f| f.id.as_str());
                notice.ore = ORES
                    .iter()
                    .find(|(f, _)| *f == id)
                    .map_or(String::new(), |(_, w)| w.to_string());
                notice.city = Some(d.city);
            }
            EventKind::PopulationBoom => {
                if !self.play_boom(city, victim, fresh, rng, &mut notice) {
                    return;
                }
            }
            _ => {
                if !self.play_world_event(kind, fresh, rng, &mut notice) {
                    return;
                }
            }
        }
        if (kind.is_world() || kind == EventKind::Disjunction) && notice.voice != Voice::Ended {
            notice.voice = Voice::Happened;
        }
        self.announce(notice, city, victim, out);
    }

    /// Fill in the notice's names and decide whether the player hears of it at all: a world event
    /// to everyone, one that fell on the human as it happened to them, one that fell on a
    /// wizard they have met as news, and the others not.
    fn announce(
        &mut self,
        mut notice: EventNotice,
        city: Option<hecs::Entity>,
        victim: PlayerId,
        out: &mut Vec<Event>,
    ) {
        let world = notice.kind.is_world() || notice.kind == EventKind::Disjunction;
        let human = self.human();
        if !world {
            notice.player = Some(victim);
            notice.player_name = self.player_name(victim);
            let heard = match human {
                Some(h) if h == victim => {
                    if notice.voice != Voice::Ended {
                        notice.voice = Voice::Started;
                    }
                    true
                }
                Some(h) => {
                    notice.voice = if notice.voice == Voice::Ended {
                        return;
                    } else {
                        Voice::Elsewhere
                    };
                    self.diplomacy.have_met(h, victim)
                }
                None => false,
            };
            if !heard {
                return;
            }
        } else if human.is_none() {
            return;
        }
        let city = notice.city.or(city);
        if let Some(c) = city {
            notice.city = Some(c);
            notice.city_name = self.city_name(c).unwrap_or_default();
            let people = self.world.get::<&Population>(c).map_or(0, |p| p.figures);
            notice.city_size = CITY_SIZE_NAMES[city_size_class(people) as usize].to_string();
        }
        out.push(Event::RandomEvent(notice));
    }

    /// Meteor and earthquake on a city: the units, buildings and citizens lost.
    fn strike_city(
        &mut self,
        city: hecs::Entity,
        rng: &mut GameRng,
        meteor: bool,
    ) -> (u32, u32, u32) {
        if meteor {
            return self.call_the_void(city, rng);
        }
        // The earthquake kills no citizens (`Apply_Damage_To_City(city, 0, 15)`).
        match self.earthquake(city, rng) {
            Event::EarthquakeStruck {
                units_lost,
                buildings_lost,
                ..
            } => (units_lost, buildings_lost, 0),
            _ => (0, 0, 0),
        }
    }

    /// A neutral city and the units in it go to the wizard, who sets it to Trade Goods and sees
    /// the land round it.
    fn marry(&mut self, city: hecs::Entity, to: PlayerId) {
        let Some(pos) = self.city_position(city) else {
            return;
        };
        let from = self.economy().city_owner(city);
        for u in self.units().units_at(pos) {
            if self.units().unit_owner(u) == from {
                self.set_owner(u, to);
            }
        }
        self.set_owner(city, to);
        let _ = self.world.insert_one(
            city,
            Production {
                item: Some(ProductionItem::TradeGoods),
                progress: 0,
            },
        );
        self.explore(to, pos, 3);
    }

    /// The plague on a city for a turn. False when the plague is over and nobody is told (it
    /// ended with a city that is gone).
    fn play_plague(
        &mut self,
        city: hecs::Entity,
        victim: PlayerId,
        fresh: bool,
        rng: &mut GameRng,
        notice: &mut EventNotice,
    ) -> bool {
        let kind = EventKind::Plague;
        let figures = self.world.get::<&Population>(city).map_or(0, |p| p.figures);
        if rng.roll(10) < figures && figures > 2 {
            self.set_figures(city, figures - 1);
        }
        if fresh {
            let serial = self.serial_of(city);
            self.events
                .start(kind, self.player_entity(victim), Some(serial));
            return true;
        }
        let turns = self.events.get(kind).map_or(0, |l| l.turns);
        let mut over = flat_end(rng.roll(20), turns);
        self.events.age(kind);
        if self.world.get::<&Population>(city).map_or(0, |p| p.figures) < 2 {
            over = true;
            self.set_figures(city, 2);
        }
        if over {
            self.events.end(kind);
            notice.voice = Voice::Ended;
            return true;
        }
        false
    }

    /// The population boom on a city for a turn. False while it goes on quietly.
    fn play_boom(
        &mut self,
        city: Option<hecs::Entity>,
        victim: PlayerId,
        fresh: bool,
        rng: &mut GameRng,
        notice: &mut EventNotice,
    ) -> bool {
        let kind = EventKind::PopulationBoom;
        if fresh {
            let serial = city.map(|c| self.serial_of(c));
            self.events.start(kind, self.player_entity(victim), serial);
            return true;
        }
        let turns = self.events.get(kind).map_or(0, |l| l.turns);
        let over = flat_end(rng.roll(20), turns);
        self.events.age(kind);
        if over {
            self.events.end(kind);
            notice.voice = Voice::Ended;
        }
        over
    }

    /// A moon, a conjunction or Mana Short for a turn. False while it goes on quietly.
    fn play_world_event(
        &mut self,
        kind: EventKind,
        fresh: bool,
        rng: &mut GameRng,
        notice: &mut EventNotice,
    ) -> bool {
        if fresh {
            self.events.start(kind, None, None);
            return true;
        }
        let turns = self.events.get(kind).map_or(0, |l| l.turns);
        let over = world_end(rng.roll(20), turns);
        self.events.age(kind);
        if over {
            self.events.end(kind);
            notice.voice = Voice::Ended;
        }
        over
    }

    fn set_figures(&mut self, city: hecs::Entity, figures: u32) {
        let pop = Population {
            figures,
            ..self.city_population(city)
        };
        let _ = self.world.insert_one(city, pop);
    }
}

#[cfg(test)]
mod tests;
