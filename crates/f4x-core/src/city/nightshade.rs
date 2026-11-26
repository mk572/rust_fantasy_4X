//! Nightshade: a plant in a city's area that shields the city from enemy spells. Each plant
//! adds 100 dispel strength against a bad enchantment cast on the city, so a spell aimed at
//! the city can fizzle. Only a city with a building that allows its use counts the plants.
//!
//! In the original the count is a pseudo-enchantment, set each turn for cities with one of
//! the buildings below and used when a spell is cast at the city (`Spells129.c`). Its
//! mid-turn dispel of the city's own bad enchantments (`CTY_NightshadeDispel`) is an empty
//! stub there and is not modelled.

use crate::city::economy::Population;
use crate::player::PlayerId;
use crate::rules::BuildingId;
use crate::rules::{SpellId, SpellTarget};
use crate::world::World;

impl World {
    /// Nightshade plants the city can use: those in its area, if it has a building that allows
    /// them and is not an outpost.
    pub fn nightshade_count(&self, city: hecs::Entity) -> u32 {
        let (Some(at), Some(map)) = (
            self.ecs.get::<&crate::map::Position>(city).ok().map(|p| *p),
            self.map.as_ref(),
        ) else {
            return 0;
        };
        if self.ecs.get::<&Population>(city).map_or(0, |p| p.figures) == 0 {
            return 0;
        }
        let allowed = (self.content.rules.config.spell_effects.nightshade_buildings)
            .iter()
            .any(|&b| self.economy().has_building(city, BuildingId::new(b)));
        let Some(plant) = self.content.features.id_numbered(16)
        /* nightshade */
        else {
            return 0;
        };
        if !allowed {
            return 0;
        }
        let grid = map.plane(at.plane);
        self.economy()
            .city_area(at)
            .iter()
            .filter(|(p, _)| grid.mineral(p.x as i32, p.y as i32) == Some(plant))
            .count() as u32
    }

    /// Whether the plants turn `spell`, cast by `player` at `city`, away. Only a spell aimed at
    /// an enemy city (a bad city enchantment) is resisted; the chance is the plants' dispel
    /// strength over that strength plus the spell's dispel difficulty.
    pub(crate) fn nightshade_fizzles(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        city: hecs::Entity,
    ) -> bool {
        let def = self.content.rules.spells.get(spell);
        if def.target != SpellTarget::EnemyCity || self.economy().city_owner(city) == Some(player) {
            return false;
        }
        let strength = self.nightshade_count(city)
            * self.content.rules.config.spell_effects.nightshade_strength;
        if strength == 0 {
            return false;
        }
        // The caster's masteries make the spell harder to turn away, as for any dispel.
        let difficulty = self.dispel_difficulty(spell, Some(player));
        self.roll(100) as u64 <= u64::from(strength) * 100 / u64::from(strength + difficulty)
    }
}
