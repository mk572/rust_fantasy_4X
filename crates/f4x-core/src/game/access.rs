//! What the other crates (the client, the console, the AI, the learning code, the tools) may ask
//! of the game and set up in it, in one place.
//!
//! `Game`'s state (`world`, `map`, `fog`, `content`, `players`) is private to the core, so
//! that its shape can change without the callers noticing. Outside the core a game
//! is *read* through these queries and the many `unit_*`, `city_*` and `player` ones next to
//! them, *played* through [`Game::apply`], and *set up by hand* (a test, a scenario, a tool)
//! through the setters at the end, which the rules never use.
//!
//! Tests: below.

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

    /// Replace the map. The features on the old one go with it.
    pub fn set_map(&mut self, map: Map) {
        self.map = Some(map);
        self.despawn_all_features();
    }

    /// Replace the map with a generated one, and spawn its features.
    pub fn set_generated(&mut self, generated: crate::map::mapgen::Generated) {
        self.set_map(generated.map);
        self.spawn_features(generated.placements);
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

    /// Put a lasting spell on a city, as a scenario sets it up (no cost, no upkeep paid).
    pub fn scenario_enchant_city(&mut self, city: hecs::Entity, spell: SpellId, caster: PlayerId) {
        self.add_city_enchantment(city, spell, caster);
    }

    /// Start a lasting spell of a wizard on the whole world, as a scenario sets it up (no cost,
    /// no upkeep paid).
    pub fn scenario_add_global(&mut self, player: PlayerId, spell: SpellId) {
        self.add_global(player, spell);
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

    /// Found a city (or an outpost, with no figures) with no checks and no cost, as a scenario
    /// sets it up.
    pub fn scenario_found_city(
        &mut self,
        owner: PlayerId,
        pos: Position,
        name: &str,
        race: Option<crate::rules::RaceId>,
        figures: u32,
    ) -> hecs::Entity {
        self.spawn_city(owner, pos, name.into(), race, figures)
    }

    /// Set the moves a unit has left, in half-moves.
    pub fn scenario_set_moves_left(&mut self, unit: hecs::Entity, half_moves: u32) {
        let _ = self
            .world
            .insert_one(unit, crate::unit::MovesLeft(half_moves));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::setup::{new_game, NewGame};
    use crate::game::testutil::shipped;
    use crate::map::Plane;
    use crate::testdir::TestResult;
    use crate::world_event::EventKind;

    const P0: PlayerId = PlayerId(0);

    fn world() -> Result<Game, String> {
        new_game(
            &shipped(),
            &NewGame {
                seed: 6,
                all_ai: true,
                ..Default::default()
            },
        )
    }

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// A town with `figures` citizens on a square no one else stands on.
    fn town(g: &mut Game, figures: u32) -> Result<hecs::Entity, String> {
        let race = g.content().rules.races.id_of("high_elf");
        let free = (3..g.map().ok_or("no map")?.width() - 3)
            .map(|x| at(x, 20))
            .find(|&p| g.economy().city_at(p).is_none())
            .ok_or("no free square")?;
        Ok(g.scenario_found_city(P0, free, "Testville", race, figures))
    }

    #[test]
    fn a_city_is_read_back_as_it_was_set_up() -> TestResult {
        let mut g = world()?;
        let city = town(&mut g, 4)?;
        let elf = g.content().rules.races.id_of("high_elf");
        assert_eq!(g.city_name(city).as_deref(), Some("Testville"));
        assert_eq!(g.city_race(city), elf);
        assert_eq!(g.city_population(city).figures, 4);
        assert!(g.city_position(city).is_some());
        assert!(!g.city_conquered(city));
        assert!(g.city_enchantments(city).is_empty());
        g.scenario_set_citizens(city, 7);
        assert_eq!(g.city_population(city).figures, 7);
        // What is not a city reads as nothing.
        let soldier = g
            .content()
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("no cavalry")?;
        let unit = g.spawn_unit(P0, at(3, 3), soldier);
        assert_eq!(g.city_name(unit), None);
        assert_eq!(g.city_position(unit), g.units().unit_position(unit));
        assert_eq!(g.city_farmers(unit), 0);
        assert_eq!(g.city_population(unit).figures, 0);
        assert!(g.city_buildings(unit).is_empty());
        Ok(())
    }

    #[test]
    fn buildings_are_put_in_a_city_and_taken_out_in_build_order() -> TestResult {
        let mut g = world()?;
        let city = town(&mut g, 4)?;
        let id = |g: &Game, name: &str| g.content().rules.buildings.id_of(name);
        let (smithy, market) = (
            id(&g, "smithy").ok_or("no smithy")?,
            id(&g, "marketplace").ok_or("no marketplace")?,
        );
        let before = g.city_buildings(city);
        g.scenario_set_building(city, smithy, true);
        g.scenario_set_building(city, market, true);
        g.scenario_set_building(city, smithy, true);
        let built = g.city_buildings(city);
        assert_eq!(built.len(), before.len() + 2, "the smithy is not doubled");
        assert_eq!(built.last(), Some(&smithy), "set again, it goes last");
        g.scenario_set_building(city, smithy, false);
        assert!(!g.city_buildings(city).contains(&smithy));
        assert!(g.city_buildings(city).contains(&market));
        Ok(())
    }

    #[test]
    fn a_unit_set_up_by_hand_has_the_experience_wounds_and_moves_it_was_given() -> TestResult {
        let mut g = world()?;
        let soldier = g
            .content()
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("no cavalry")?;
        let unit = g.spawn_unit(P0, at(3, 3), soldier);
        g.scenario_set_experience(unit, 130);
        g.scenario_set_hurt(unit, 3);
        g.scenario_set_moves_left(unit, 1);
        assert_eq!(g.component::<Experience>(unit).map(|x| x.0), Some(130));
        assert_eq!(g.component::<Hurt>(unit).map(|h| h.0), Some(3));
        assert_eq!(g.units().unit_moves_left(unit), 1);
        assert!(g.entity_exists(unit));
        let count = g.entity_count();
        g.scenario_remove_unit(unit);
        assert!(!g.entity_exists(unit));
        assert_eq!(g.entity_count(), count - 1);
        Ok(())
    }

    #[test]
    fn a_unit_is_idle_only_with_moves_and_no_orders() -> TestResult {
        let mut g = world()?;
        let soldier = g
            .content()
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("no cavalry")?;
        let unit = g.spawn_unit(P0, at(3, 3), soldier);
        g.scenario_set_moves_left(unit, 2);
        assert!(g.unit_is_idle(unit));
        assert!(!g.unit_has_order(unit) && !g.unit_building_road(unit));
        g.scenario_set_moves_left(unit, 0);
        assert!(!g.unit_is_idle(unit), "no moves left");
        g.scenario_set_moves_left(unit, 2);
        let _ = g.world.insert_one(unit, Order::GoTo(vec![at(4, 3)]));
        assert!(g.unit_has_order(unit));
        assert!(!g.unit_is_idle(unit), "it is walking a path");
        Ok(())
    }

    #[test]
    fn a_global_spell_and_an_event_are_started_without_cost() -> TestResult {
        let mut g = world()?;
        let spell = g
            .content()
            .rules
            .spells
            .id_of("great_unsummoning")
            .or_else(|| g.content().rules.spells.id_of("armageddon"))
            .ok_or("no global spell")?;
        let mana = g.player(P0).map(|p| p.treasury().mana);
        g.scenario_add_global(P0, spell);
        assert!(g.wizard(P0).ok_or("no wizard")?.any_global(|s| s == spell));
        assert_eq!(g.player(P0).map(|p| p.treasury().mana), mana, "no cost");
        g.scenario_start_event(EventKind::GoodMoon);
        assert!(g.events().is_ongoing(EventKind::GoodMoon));
        Ok(())
    }

    #[test]
    fn a_casting_set_by_hand_replaces_the_one_in_progress() -> TestResult {
        let mut g = world()?;
        let spells = &g.content().rules.spells;
        let (a, b) = (
            spells.id_of("fire_bolt").ok_or("no fire_bolt")?,
            spells.id_of("healing").ok_or("no healing")?,
        );
        g.scenario_casting(P0, a, 30);
        g.scenario_casting(P0, b, 12);
        let casting = g.wizard(P0).and_then(|w| w.casting());
        assert_eq!(casting.map(|c| (c.spell, c.remaining)), Some((b, 12)));
        Ok(())
    }

    #[test]
    fn fleeing_is_made_safe_for_everyone_and_the_map_and_content_can_be_replaced() -> TestResult {
        let mut g = world()?;
        g.scenario_set_safe_flee(true);
        assert!(g.settings().rules.safe_flee.of(true));
        g.scenario_set_safe_flee(false);
        assert!(!g.settings().rules.safe_flee.of(true));
        let content = g.content().clone();
        g.set_content(content.clone());
        assert_eq!(g.content(), &content);
        let map = g.map().ok_or("no map")?.clone();
        g.set_map(map.clone());
        assert_eq!(g.map(), Some(&map));
        Ok(())
    }

    #[test]
    fn exploring_is_counted_for_the_player_who_did_it() -> TestResult {
        let mut g = world()?;
        let home = g
            .wizard(P0)
            .and_then(|w| w.fortress())
            .ok_or("no fortress")?;
        assert!(g.is_explored(P0, home));
        assert_eq!(g.explored_corners(P0, home), 15, "the start is explored");
        assert!(g.explored_squares(P0) > 0);
        g.reveal_map(P0);
        assert_eq!(
            g.explored_squares(P0),
            2 * g.map().ok_or("no map")?.width() as usize
                * g.map().ok_or("no map")?.height() as usize
        );
        Ok(())
    }
}
