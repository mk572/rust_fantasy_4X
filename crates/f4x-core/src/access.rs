//! What the other crates (the client, the console, the AI, the learning code, the tools) may ask
//! of the game and set up in it, in one place.
//!
//! `Game`'s state (`world`, `map`, `fog`, `content`, `players`) is private to the core, so
//! that its shape can change without the callers noticing (task 181). Outside the core a game
//! is *read* through these queries and the many `unit_*`, `city_*` and `player` ones next to
//! them, *played* through [`Game::apply`], and *set up by hand* (a test, a scenario, a tool)
//! through the setters at the end, which the rules never use.

use crate::city::{PlayerId, Position};
use crate::content::Content;
use crate::economy::{Buildings, CityRace, Enchantments, Farmers, Population};
use crate::map::Map;
use crate::player::Player;
use crate::rules::{BuildingId, RaceId, SpellId};
use crate::units::{Experience, Hurt, Order};
use crate::Game;

impl Game {
    // ---- reading -----------------------------------------------------------------

    /// The definitions of the game (public rules data: units, spells, terrain...).
    pub fn content(&self) -> &Content {
        &self.content
    }

    /// The map, once there is one.
    pub fn map(&self) -> Option<&Map> {
        self.map.as_ref()
    }

    /// Every player, wizards and independent cities' players, in the order they were added.
    pub fn players(&self) -> &[Player] {
        &self.players
    }

    /// Has `player` explored the square (as opposed to seeing it now: see `visibility`)?
    pub fn is_explored(&self, player: PlayerId, at: Position) -> bool {
        self.fog.is_explored(player, at.plane, at.x, at.y)
    }

    /// The explored corners of a square (see `Fog::corners`): 0 unexplored, 15 explored.
    pub fn explored_corners(&self, player: PlayerId, at: Position) -> u8 {
        self.fog.corners(player, at.plane, at.x, at.y)
    }

    /// How many squares the player has explored, on both planes.
    pub fn explored_squares(&self, player: PlayerId) -> usize {
        crate::map::Plane::ALL
            .iter()
            .map(|&pl| self.fog.explored_count(player, pl))
            .sum()
    }

    /// A city's name.
    pub fn city_name(&self, city: hecs::Entity) -> Option<String> {
        self.world
            .get::<&crate::city::City>(city)
            .map(|c| c.name.clone())
            .ok()
    }

    /// The race of a city's citizens.
    pub fn city_race(&self, city: hecs::Entity) -> Option<RaceId> {
        self.world.get::<&CityRace>(city).map(|r| r.0).ok()
    }

    /// A city's people: whole citizens and tens of people toward the next (zero citizens is an
    /// outpost).
    pub fn city_population(&self, city: hecs::Entity) -> Population {
        self.world
            .get::<&Population>(city)
            .map(|p| *p)
            .unwrap_or_default()
    }

    /// Has the city just been taken, so that its new owner may still raze it?
    pub fn city_conquered(&self, city: hecs::Entity) -> bool {
        self.world.get::<&crate::conquest::Conquered>(city).is_ok()
    }

    /// How many of a city's citizens farm.
    pub fn city_farmers(&self, city: hecs::Entity) -> u32 {
        self.world.get::<&Farmers>(city).map_or(0, |f| f.0)
    }

    /// Where a city stands.
    pub fn city_position(&self, city: hecs::Entity) -> Option<Position> {
        self.world.get::<&Position>(city).map(|p| *p).ok()
    }

    /// What is built in a city, in build order (replaced buildings included).
    pub fn city_buildings(&self, city: hecs::Entity) -> Vec<BuildingId> {
        self.world
            .get::<&Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default()
    }

    /// The enchantments on a city, with the wizard who cast each.
    pub fn city_enchantments(&self, city: hecs::Entity) -> Vec<(SpellId, PlayerId)> {
        self.world
            .get::<&Enchantments>(city)
            .map(|e| e.0.clone())
            .unwrap_or_default()
    }

    /// Has the unit a standing order (a path it is walking)?
    pub fn unit_has_order(&self, unit: hecs::Entity) -> bool {
        self.world.get::<&Order>(unit).is_ok()
    }

    // ---- for tests and tools --------------------------------------------------------

    /// A component of an entity, read-only. For tests and tools that check what the rules did to
    /// a unit or a city; the game's own code and the other crates use the named queries above
    /// and the `unit_*` and `city_*` ones.
    #[doc(hidden)]
    pub fn component<T: hecs::Component>(&self, entity: hecs::Entity) -> Option<hecs::Ref<'_, T>> {
        self.world.get::<&T>(entity).ok()
    }

    /// Does the entity (a unit or a city) still exist?
    #[doc(hidden)]
    pub fn entity_exists(&self, entity: hecs::Entity) -> bool {
        self.world.contains(entity)
    }

    /// How many entities (units and cities) there are.
    #[doc(hidden)]
    pub fn entity_count(&self) -> usize {
        self.world.len() as usize
    }

    // ---- setting up by hand ------------------------------------------------------

    /// Replace the game's definitions.
    pub fn set_content(&mut self, content: Content) {
        self.content = content;
    }

    /// Replace the map.
    pub fn set_map(&mut self, map: Map) {
        self.map = Some(map);
    }

    /// Add a player.
    pub fn add_player(&mut self, player: Player) {
        self.players.push(player);
    }

    /// Give a unit experience points, as a scenario sets it up (no level checks).
    pub fn scenario_set_experience(&mut self, unit: hecs::Entity, xp: u32) {
        let _ = self.world.insert_one(unit, Experience(xp));
    }

    /// Wound a unit by this many hit points (over all figures), as a scenario sets it up.
    pub fn scenario_set_hurt(&mut self, unit: hecs::Entity, hurt: u32) {
        let _ = self.world.insert_one(unit, Hurt(hurt));
    }

    /// Set the number of citizens of a city, as a scenario sets it up.
    pub fn scenario_set_citizens(&mut self, city: hecs::Entity, figures: u32) {
        if let Ok(mut p) = self.world.get::<&mut Population>(city) {
            p.figures = figures;
        }
    }

    /// Put a lasting spell on a unit, as a scenario sets it up (no cost, no upkeep paid).
    pub fn scenario_enchant_unit(&mut self, unit: hecs::Entity, spell: SpellId, caster: PlayerId) {
        let mut list = self
            .world
            .get::<&crate::units::UnitEnchantments>(unit)
            .map(|l| l.0.clone())
            .unwrap_or_default();
        list.push((spell, caster));
        let _ = self
            .world
            .insert_one(unit, crate::units::UnitEnchantments(list));
    }

    /// Make fleeing from a battle safe or risky for everyone (the `safe_flee` setting).
    pub fn scenario_set_safe_flee(&mut self, safe: bool) {
        self.settings.rules.safe_flee = crate::settings::Split::both(safe);
    }

    /// Remove a unit from the game.
    pub fn scenario_remove_unit(&mut self, unit: hecs::Entity) {
        self.remove_unit(unit, crate::roster::Fate::Released);
    }

    /// Put a building in a city, or take it out, as a scenario sets it up (no cost, no
    /// requirements).
    pub fn scenario_set_building(&mut self, city: hecs::Entity, building: BuildingId, built: bool) {
        if let Ok(mut b) = self.world.get::<&mut Buildings>(city) {
            b.0.retain(|&x| x != building);
            if built {
                b.0.push(building);
            }
        }
    }
}
