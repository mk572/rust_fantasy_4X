//! Spells that work on the whole world or hit a city from afar: the global enchantments Meteor
//! Storm, Great Wasting and Armageddon (once a turn, while kept going), Call the Void, and
//! Nature's Wrath (when another wizard casts a death or chaos spell).
//!
//! Tests: in spell/wizard/tests.rs.

use crate::rules::damage_type::DamageType;
use rand::SeedableRng;

use crate::map::{Plane, Position};
use crate::player::PlayerId;
use crate::world::rng::GameRng;
use f4x_def::rules::attack_nature::AttackNature;
use f4x_def::rules::realm::Realm;

use crate::rules::effects::DamageTraits;
use crate::unit::Hurt;
use crate::world::{Event, World};

impl World {
    /// One turn of the world-scale enchantments every wizard keeps going.
    pub(crate) fn process_world_enchantments(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let players: Vec<PlayerId> = crate::player::ids(&self.ecs);
        for player in players {
            if self.has_global_effect(
                player,
                crate::spell::global_enchantment::GlobalEffect::GreatWasting,
            ) {
                events.extend(self.great_wasting(player));
            }
            if self.has_global_effect(
                player,
                crate::spell::global_enchantment::GlobalEffect::Armageddon,
            ) {
                events.extend(self.armageddon(player));
            }
            if self.has_global_effect(
                player,
                crate::spell::global_enchantment::GlobalEffect::MeteorStorm,
            ) {
                events.extend(self.meteor_storm(player));
            }
        }
        events
    }

    /// Whether `at` lies within the safe distance of one of `player`'s cities.
    fn near_city_of(&self, player: PlayerId, at: Position) -> bool {
        let Some(map) = self.map.as_ref() else {
            return false;
        };
        let width = map.width() as i32;
        self.economy()
            .cities()
            .into_iter()
            .filter(|(_, c, p)| c.owner == player && p.plane == at.plane)
            .any(|(_, _, p)| {
                let dx = (p.x as i32 - at.x as i32).abs();
                let dx = dx.min(width - dx);
                dx.max((p.y as i32 - at.y as i32).abs())
                    <= self.content.rules.config.spell_effects.safe_distance
            })
    }

    /// Up to `3..=6` random squares on either plane that pass `fit`, and with `spare_cities`
    /// not within two squares of `player`'s cities.
    fn pick_squares(
        &mut self,
        player: PlayerId,
        spare_cities: bool,
        sea_percent: u32,
        fit: impl Fn(&World, Position) -> bool,
    ) -> Vec<Position> {
        let Some((w, h)) = self.map.as_ref().map(|m| (m.width(), m.height())) else {
            return Vec::new();
        };
        let wanted = self.content.rules.config.spell_effects.min_squares
            + self.roll(self.content.rules.config.spell_effects.square_spread)
            - 1;
        let mut found: Vec<Position> = Vec::new();
        for _ in 0..wanted {
            for _ in 0..self.content.rules.config.spell_effects.scatter_tries {
                let at = Position {
                    plane: if self.roll(2) == 1 {
                        Plane::Arcanus
                    } else {
                        Plane::Myrror
                    },
                    x: self.roll(w) - 1,
                    y: self.roll(h) - 1,
                };
                if !found.contains(&at)
                    && (!spare_cities || !self.near_city_of(player, at))
                    && fit(self, at)
                {
                    let sea = self.economy().terrain_class_at(at)
                        == Some(crate::map::terrain::TerrainClass::Ocean);
                    if sea && self.roll(100) > sea_percent {
                        continue;
                    }
                    found.push(at);
                    break;
                }
            }
        }
        found
    }

    /// Great Wasting: any land squares, the caster's own cities' too, are corrupted (the caster
    /// is proof against it, see `city/economy.rs`).
    fn great_wasting(&mut self, player: PlayerId) -> Vec<Event> {
        let squares = self.pick_squares(player, false, 100, |w, at| w.check_corrupt(at).is_ok());
        squares
            .into_iter()
            .flat_map(|at| self.corrupt(at))
            .collect()
    }

    /// Armageddon: volcanoes rise on any square, the caster's cities included and the sea too
    /// (seldom), owned by the caster, who gains a point of power from each.
    fn armageddon(&mut self, player: PlayerId) -> Vec<Event> {
        let squares = self.pick_squares(
            player,
            false,
            self.content.rules.config.spell_effects.sea_percent,
            |w, at| w.can_armageddon_volcano(at),
        );
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(self.next_u64() ^ 0xA2E6);
        squares
            .into_iter()
            .flat_map(|at| self.armageddon_volcano(player, at, &mut rng))
            .collect()
    }

    /// Meteor Storm: every unit outside a city is struck, the caster's too, and buildings of
    /// other wizards' cities may fall. The caster's cities are safe.
    fn meteor_storm(&mut self, player: PlayerId) -> Vec<Event> {
        let Some(spell) = self.content.rules.spells.id_numbered(113)
        /* meteor_storm */
        else {
            return Vec::new();
        };
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(self.next_u64() ^ 0x3E7E);
        let nature = AttackNature::magical(Realm::Chaos, &[DamageType::Fire]);
        let traits = DamageTraits::default();
        let mut units_lost = 0;
        for unit in self.units().all_units() {
            let Some(at) = self.units().unit_position(unit) else {
                continue;
            };
            if self.economy().city_at(at).is_some() {
                continue;
            }
            if self.spell_hit_overland_unit(
                unit,
                self.content.rules.config.spell_effects.meteor_strength,
                &traits,
                nature,
                &mut rng,
            ) {
                units_lost += 1;
            }
        }
        let mut buildings_lost = 0;
        for (city, c, _) in self.economy().cities() {
            if c.owner == player {
                continue;
            }
            let built = self.buildings_of(city);
            for b in built {
                if self.roll(100)
                    <= self
                        .content
                        .rules
                        .config
                        .spell_effects
                        .meteor_building_percent
                    && self.destroy_building(city, b).is_ok()
                {
                    buildings_lost += 1;
                }
            }
        }
        vec![Event::WorldSpellStruck {
            spell,
            victim: None,
            units_lost,
            buildings_lost,
            population_lost: 0,
        }]
    }

    /// Call the Void on a city: the land around it is half corrupted, every unit inside takes 10
    /// points of damage whatever its defence, each citizen but the first has an even chance of
    /// dying (`Apply_Call_The_Void`: a city is never emptied), and each building has an even
    /// chance of falling. Returns the units, buildings and citizens lost.
    pub(crate) fn call_the_void(
        &mut self,
        city: hecs::Entity,
        rng: &mut GameRng,
    ) -> (u32, u32, u32) {
        let Ok(at) = self.ecs.get::<&Position>(city).map(|p| *p) else {
            return (0, 0, 0);
        };
        let width = self.map.as_ref().map_or(1, |m| m.width()) as i32;
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let y = at.y as i32 + dy;
                if y < 0 || y >= self.map.as_ref().map_or(0, |m| m.height()) as i32 {
                    continue;
                }
                let sq = Position {
                    plane: at.plane,
                    x: (at.x as i32 + dx).rem_euclid(width) as u32,
                    y: y as u32,
                };
                if rng.roll(100) <= self.content.rules.config.spell_effects.void_corrupt_percent {
                    self.corrupt(sq);
                }
            }
        }
        let mut units_lost = 0;
        for unit in self.units().units_at(at) {
            if self.hurt_directly(
                unit,
                self.content.rules.config.spell_effects.void_unit_damage,
            ) {
                units_lost += 1;
            }
        }
        let citizens = self.city_population(city).figures;
        let citizens_lost = (1..citizens)
            .filter(|_| {
                rng.roll(100) <= self.content.rules.config.spell_effects.void_citizen_percent
            })
            .count() as u32;
        let mut buildings_lost = 0;
        for b in self.buildings_of(city) {
            if rng.roll(100)
                <= self
                    .content
                    .rules
                    .config
                    .spell_effects
                    .void_building_percent
                && self.destroy_building(city, b).is_ok()
            {
                buildings_lost += 1;
            }
        }
        if citizens_lost > 0 {
            if let Ok(mut pop) = self.ecs.get::<&mut crate::city::economy::Population>(city) {
                pop.figures -= citizens_lost;
            }
        }
        (units_lost, buildings_lost, citizens_lost)
    }

    fn buildings_of(&self, city: hecs::Entity) -> Vec<crate::rules::BuildingId> {
        self.ecs
            .get::<&crate::city::economy::Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default()
    }

    /// Damage that ignores defence: the unit is destroyed if it takes all its hit points.
    fn hurt_directly(&mut self, unit: hecs::Entity, amount: u32) -> bool {
        let (left, _) = self.units().unit_hits(unit);
        let hurt = self.ecs.get::<&Hurt>(unit).map_or(0, |h| h.0);
        let dealt = amount.min(left);
        self.put(unit, Hurt(hurt + dealt));
        if dealt >= left {
            self.remove_unit(unit, crate::hero::Fate::Slain);
            return true;
        }
        false
    }

    /// Nature's Wrath: `caster` has cast a death or chaos spell while another wizard keeps
    /// Nature's Wrath going, so all the caster's cities shake.
    pub(crate) fn natures_wrath_strikes(
        &mut self,
        caster: PlayerId,
        spell: crate::rules::SpellId,
    ) -> Vec<Event> {
        let realm = self.content.rules.spells.get(spell).realm.num();
        if realm != 4 /* death */ && realm != 2
        /* chaos */
        {
            return Vec::new();
        }
        if !self.player_ids().into_iter().any(|p| {
            p != caster
                && self.has_global_effect(
                    p,
                    crate::spell::global_enchantment::GlobalEffect::NaturesWrath,
                )
        }) {
            return Vec::new();
        }
        let Some(wrath) = self.content.rules.spells.id_numbered(40)
        /* natures_wrath */
        else {
            return Vec::new();
        };
        let cities: Vec<_> = self
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == caster)
            .map(|(e, ..)| e)
            .collect();
        let mut rng = std::mem::replace(&mut self.rng, GameRng::lent());
        let (mut units_lost, mut buildings_lost) = (0, 0);
        for city in cities {
            // The cities come from the economy: one that cannot be shaken is skipped.
            if let Ok((u, b)) = self.shake_city(
                city,
                self.content.rules.config.spell_effects.wrath_unit_percent,
                self.content
                    .rules
                    .config
                    .spell_effects
                    .wrath_building_percent,
                &mut rng,
            ) {
                units_lost += u;
                buildings_lost += b;
            }
        }
        self.rng = rng;
        vec![Event::WorldSpellStruck {
            spell: wrath,
            victim: Some(caster),
            units_lost,
            buildings_lost,
            population_lost: 0,
        }]
    }
}
