//! What the other crates (the client, the console, the AI, the learning code, the tools) may ask
//! of the game and set up in it, in one place.
//!
//! `Game`'s state (`world`, `map`, `fog`, `content`, `players`) is private to the core, so
//! that its shape can change without the callers noticing (task 181). Outside the core a game
//! is *read* through these queries and the many `unit_*`, `city_*` and `player` ones next to
//! them, *played* through [`Game::apply`], and *set up by hand* (a test, a scenario, a tool)
//! through the setters at the end, which the rules never use.

use crate::city::economy::{Buildings, CityRace, Farmers, Population};
use crate::content::Content;
use crate::map::{Map, Position};
use crate::player::PlayerId;

use crate::game::Game;
use crate::rules::{BuildingId, RaceId, SpellId};
use crate::unit::{Experience, Hurt, Order};

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

    /// The random events that are going on.
    pub fn events(&self) -> &crate::world_event::Events {
        &self.events
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
        self.world
            .get::<&crate::city::conquest::Conquered>(city)
            .is_ok()
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
        self.spells().city_enchantments(city)
    }

    /// Has the unit a standing order (a path it is walking)?
    pub fn unit_has_order(&self, unit: hecs::Entity) -> bool {
        self.world.get::<&Order>(unit).is_ok()
    }

    /// Is the unit (an engineer) building a road?
    pub fn unit_building_road(&self, unit: hecs::Entity) -> bool {
        self.world.get::<&crate::map::road::RoadWork>(unit).is_ok()
    }

    /// Is the unit waiting for orders: moves left, and not on standby, walking a path or
    /// building a road? What the main screen cycles through.
    pub fn unit_is_idle(&self, unit: hecs::Entity) -> bool {
        self.units().unit_moves_left(unit) > 0
            && self.units().unit_standby(unit).is_none()
            && self.world.get::<&Order>(unit).is_err()
            && self.world.get::<&crate::map::road::RoadWork>(unit).is_err()
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
        self.add_unit_enchantment(unit, spell, caster);
    }

    /// Set the spell a wizard is casting and the mana still owed, as a scenario sets it up (no
    /// checks; a cast already in progress is replaced).
    pub fn scenario_casting(&mut self, player: PlayerId, spell: SpellId, owed: i32) {
        if let Some(mut w) = self.wizard_mut(player) {
            w.clear_casting();
        }
        self.begin_casting(player, spell, owed);
    }

    /// Make fleeing from a battle safe or risky for everyone (the `safe_flee` setting).
    pub fn scenario_set_safe_flee(&mut self, safe: bool) {
        self.settings.rules.safe_flee = crate::game::setting::Split::both(safe);
    }

    /// Start a lasting event (a moon, a conjunction, mana short) that falls on nobody, as a
    /// scenario sets it up.
    pub fn scenario_start_event(&mut self, event: crate::world_event::EventKind) {
        self.events.start(event, None, None);
    }

    /// Remove a unit from the game.
    pub fn scenario_remove_unit(&mut self, unit: hecs::Entity) {
        self.remove_unit(unit, crate::hero::Fate::Released);
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
