//! What the other crates (the client, the console, the AI, the learning code, the tools) may ask
//! of the game and set up in it, in one place.
//!
//! `World`'s state (`ecs`, `map`, `fog`, `content`, `player_index`) is private to the core, so
//! that its shape can change without the callers noticing. Outside the core a game
//! is *read* through these queries and the many `unit_*`, `city_*` and `player` ones next to
//! them, *played* through [`World::apply`], and *set up by hand* (a test, a scenario, a tool)
//! through the setters at the end, which the rules never use.
//!
//! Tests: below.

use crate::city::economy::{Buildings, CityRace, Farmers, Population};
use crate::map::{Map, Position};
use crate::player::PlayerId;
use f4x_def::content::Content;
use f4x_def::half_int::HalfInt;

use crate::rules::{BuildingId, RaceId, SpellId};
use crate::unit::{Experience, Hurt, Order};
use crate::world::World;

impl World {
    // ---- reading -----------------------------------------------------------------

    /// The definitions of the game (public rules data: units, spells, terrain...).
    pub fn content(&self) -> &Content {
        &self.content
    }

    /// The content, to build another world on without copying it.
    pub fn shared_content(&self) -> std::sync::Arc<Content> {
        self.content.clone()
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
        crate::map::Plane::all()
            .map(|pl| self.fog.explored_count(player, pl))
            .sum()
    }

    /// The random events that are going on.
    pub fn events(&self) -> &crate::world_event::Events {
        &self.events
    }

    /// A city's name.
    pub fn city_name(&self, city: hecs::Entity) -> Option<String> {
        self.ecs
            .get::<&crate::city::City>(city)
            .map(|c| c.name.clone())
            .ok()
    }

    /// The race of a city's citizens.
    pub fn city_race(&self, city: hecs::Entity) -> Option<RaceId> {
        self.ecs.get::<&CityRace>(city).map(|r| r.0).ok()
    }

    /// A city's people: whole citizens and tens of people toward the next (zero citizens is an
    /// outpost).
    pub fn city_population(&self, city: hecs::Entity) -> Population {
        self.ecs
            .get::<&Population>(city)
            .map(|p| *p)
            .unwrap_or_default()
    }

    /// Has the city just been taken, so that its new owner may still raze it?
    pub fn city_conquered(&self, city: hecs::Entity) -> bool {
        self.ecs
            .get::<&crate::city::conquest::Conquered>(city)
            .is_ok()
    }

    /// How many of a city's citizens farm.
    pub fn city_farmers(&self, city: hecs::Entity) -> u32 {
        self.ecs.get::<&Farmers>(city).map_or(0, |f| f.0)
    }

    /// Where a city stands.
    pub fn city_position(&self, city: hecs::Entity) -> Option<Position> {
        self.ecs.get::<&Position>(city).map(|p| *p).ok()
    }

    /// What is built in a city, in build order (replaced buildings included).
    pub fn city_buildings(&self, city: hecs::Entity) -> Vec<BuildingId> {
        self.ecs
            .get::<&Buildings>(city)
            .map(|b| b.0.clone())
            .unwrap_or_default()
    }

    /// Has the unit a standing order (a path it is walking)?
    pub fn unit_has_order(&self, unit: hecs::Entity) -> bool {
        crate::unit::ledger::has_order(&self.ecs, unit)
    }

    /// The unit's standing order.
    pub fn unit_order(&self, unit: hecs::Entity) -> Option<Order> {
        crate::unit::ledger::order_of(&self.ecs, unit)
    }

    /// The square the unit's standing order takes it to: the end of its path, the square of the
    /// unit it escorts, the last square of its road. `None` for a scout that has chosen none.
    pub fn unit_destination(&self, unit: hecs::Entity) -> Option<crate::map::Position> {
        match self.unit_order(unit)? {
            Order::GoTo(path) => path.last().copied(),
            Order::Scout(scouting) => scouting.trip.map(|t| t.to),
            Order::Meld(trip) | Order::Reinforce(trip) | Order::Settle { trip, .. } => {
                Some(trip.to)
            }
            Order::Escort(charge) => {
                let owner = self.player_entity(self.units().unit_owner(unit)?)?;
                let ledger = self
                    .ecs
                    .get::<&crate::unit::ledger::ArmyLedger>(owner)
                    .ok()?;
                self.units().unit_position(ledger.unit(charge)?)
            }
            Order::Road(work) => Some(work.rest.last().copied().unwrap_or(work.at)),
        }
    }

    /// Is the unit (an engineer) building a road?
    pub fn unit_building_road(&self, unit: hecs::Entity) -> bool {
        matches!(self.unit_order(unit), Some(Order::Road(_)))
    }

    /// Is the unit waiting for orders: moves left, and neither on standby nor under an order?
    /// What the main screen cycles through.
    pub fn unit_is_idle(&self, unit: hecs::Entity) -> bool {
        self.units().unit_moves_left(unit) > 0
            && self.units().unit_standby(unit).is_none()
            && !self.unit_has_order(unit)
    }

    // ---- for tests and tools --------------------------------------------------------

    /// A component of an entity, read-only. For tests and tools that check what the rules did to
    /// a unit or a city; the game's own code and the other crates use the named queries above
    /// and the `unit_*` and `city_*` ones.
    #[doc(hidden)]
    pub fn component<T: hecs::Component>(&self, entity: hecs::Entity) -> Option<hecs::Ref<'_, T>> {
        self.ecs.get::<&T>(entity).ok()
    }

    /// Does the entity (a unit or a city) still exist?
    #[doc(hidden)]
    pub fn entity_exists(&self, entity: hecs::Entity) -> bool {
        self.ecs.contains(entity)
    }

    /// How many entities (units and cities) there are.
    #[doc(hidden)]
    pub fn entity_count(&self) -> usize {
        self.ecs.len() as usize
    }

    // ---- setting up by hand ------------------------------------------------------

    /// Replace the game's definitions. The game's settings become the level of the new-game
    /// choices of that content (so a modded content plays its own figures, not the shipped ones);
    /// a caller that wants others sets them after. Empty content (no levels) leaves them.
    pub fn set_content(&mut self, content: impl Into<std::sync::Arc<Content>>) {
        self.content = content.into();
        if let Some(settings) = self.content.settings.default_game() {
            self.settings = settings;
        }
    }

    /// The definitions, for a test that changes one (a copy is made if a battle shares them).
    #[cfg(test)]
    pub(crate) fn content_mut(&mut self) -> &mut Content {
        std::sync::Arc::make_mut(&mut self.content)
    }

    /// The definitions of the game as the shared handle a battle keeps.
    pub fn content_arc(&self) -> std::sync::Arc<Content> {
        self.content.clone()
    }

    /// Hand the game its name lists (a loaded game has none until this is called).
    pub fn set_names(&mut self, names: f4x_def::names::NameBundle) {
        self.names = names;
    }

    /// The name lists the game draws from.
    pub fn names(&self) -> &f4x_def::names::NameBundle {
        &self.names
    }

    /// The name of the `index`th city of a race (from 0): its list, or `<race id>_city_NN`.
    pub fn city_name_for(&self, race: crate::rules::RaceId, index: usize) -> String {
        let def = self.content.rules.races.get(race);
        self.names.city(def.num, &def.id, index)
    }

    /// Replace the map. The features on the old one go with it.
    pub fn set_map(&mut self, map: Map) {
        self.map = Some(map);
        self.despawn_all_features();
    }

    // ---- starting a game (`f4x_newgame`) -----------------------------------------

    /// A new game on a made map, with the game's own copy of the content and the name lists, the
    /// settings applied to the content, and the map's features spawned. No players yet.
    pub fn begin_world(
        settings: crate::world::setting::GameSettings,
        content: &Content,
        names: f4x_def::names::NameBundle,
        map: Map,
        placements: Vec<crate::feature::entity::Placement>,
    ) -> World {
        let mut world = World::new();
        world.set_content(std::sync::Arc::new(content.clone()));
        world.settings = settings;
        world.apply_content_settings();
        world.names = names;
        world.set_map(map);
        world.spawn_features(placements);
        world.add_race_players();
        world
    }

    /// Make what `player` has explored only the land within `radius` of `centre`.
    pub fn restrict_view(&mut self, player: PlayerId, centre: Position, radius: HalfInt) {
        self.fog.forget(player);
        self.fog.explore(
            player,
            centre,
            radius,
            self.settings.basic.diagonal_extra_cost,
        );
    }

    /// Give a unit experience points, as a scenario sets it up (no level checks).
    pub fn scenario_set_experience(&mut self, unit: hecs::Entity, xp: u32) {
        self.put(unit, Experience(xp));
    }

    /// Scenario setup: a hero rolled elsewhere (`hero::kind::roll_one`) enters the game, free
    /// and with no experience, so that `spawn_hero` can bring it into a wizard's army. Returns
    /// its entity.
    pub fn scenario_make_hero(&mut self, hero: crate::hero::Hero) -> hecs::Entity {
        self.make_hero(hero, crate::hero::HeroState::Free, 0)
    }

    /// Scenario setup: change the battle on the table (its map, where its units stand), as a
    /// scenario sets it up. Does nothing when there is no battle.
    pub fn scenario_edit_battle(&mut self, change: impl FnOnce(&mut crate::battle::Battle)) {
        if let Some(a) = self.battle.as_mut() {
            change(&mut a.battle);
        }
    }

    /// Scenario setup: the player's units get `bonus` levels beyond what experience gives,
    /// the way the real game gives them: the first from the Warlord retort, the second from
    /// the Crusade spell (a normal unit reaches Ultra-Elite with one and Champion with both).
    pub fn scenario_level_bonus(&mut self, player: PlayerId, bonus: usize) {
        if bonus >= 1 {
            if let Some(mut w) = self.wizard_mut(player) {
                w.get_mut::<crate::wizard::sheet::Retorts>()
                    .0
                    .insert(crate::wizard::magic::Retort::new(1 /* warlord */));
            }
        }
        if bonus >= 2 {
            if let Some(crusade) = self.content.rules.spells.id_numbered(158 /* crusade */) {
                self.add_global(player, crusade);
            }
        }
    }

    /// Give the wizard of `side` in the battle on the table `mana` to cast with and the same
    /// to spend, with a casting skill of ten (a test or a scenario setting up a fight).
    pub fn scenario_battle_caster(&mut self, side: crate::battle::Side, mana: i32) {
        if let Some(a) = self.battle.as_mut() {
            a.battle.set_caster(
                side,
                f4x_battle::battlemagic::caster::BattleCaster::new(mana, mana, 10),
            );
        }
    }

    /// Scenario setup: change the game's settings (a test world is made with the defaults).
    pub fn scenario_change_settings(
        &mut self,
        change: impl FnOnce(&mut crate::world::setting::GameSettings),
    ) {
        change(&mut self.settings);
    }

    /// Wound a unit by this many hit points (over all figures), as a scenario sets it up.
    pub fn scenario_set_hurt(&mut self, unit: hecs::Entity, hurt: u32) {
        self.put(unit, Hurt(hurt));
    }

    /// Scenario setup: the city has nothing in hand, as when its item has just been finished.
    pub fn scenario_clear_production(&mut self, city: hecs::Entity) {
        let mut p = self.economy().city_production(city);
        p.item = None;
        self.put(city, p);
    }

    /// Set the number of citizens of a city, as a scenario sets it up.
    pub fn scenario_set_citizens(&mut self, city: hecs::Entity, figures: u32) {
        if let Ok(mut p) = self.ecs.get::<&mut Population>(city) {
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

    /// Make every war this many rounds older (younger, if negative), as a scenario that skips
    /// rounds sets it up.
    pub fn scenario_age_wars(&mut self, rounds: i64) {
        self.diplomacy.age_wars(rounds);
    }

    /// Set the spell a wizard is casting and the mana still owed, as a scenario sets it up (no
    /// checks; a cast already in progress is replaced).
    pub fn scenario_casting(&mut self, player: PlayerId, spell: SpellId, owed: i32) {
        if let Some(mut w) = self.wizard_mut(player) {
            w.clear_casting();
        }
        self.begin_casting(player, spell, owed);
    }

    /// [`World::scenario_casting`] for a spell with `extra` mana put into it (a dispel's strength).
    pub fn scenario_casting_with(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        owed: i32,
        extra: i32,
    ) {
        if let Some(mut w) = self.wizard_mut(player) {
            w.clear_casting();
        }
        self.begin_casting_with(player, spell, owed, extra);
    }

    /// Give a wizard this casting skill (the points that make it, none left over), as a scenario
    /// sets it up.
    pub fn scenario_set_casting_skill(&mut self, player: PlayerId, skill: i32) {
        if let Some(mut w) = self.wizard_mut(player) {
            w.get_mut::<crate::wizard::sheet::Skill>().points =
                crate::wizard::magic::skill_points_for(skill, 0);
        }
    }

    /// Make fleeing from a battle safe or risky for everyone (the `safe_flee` setting).
    pub fn scenario_set_safe_flee(&mut self, safe: bool) {
        self.settings.rules.safe_flee = crate::world::setting::Split::both(safe);
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

    /// Set the moves a unit has left.
    pub fn scenario_set_moves_left(&mut self, unit: hecs::Entity, moves: HalfInt) {
        self.put(unit, crate::unit::MovesLeft(moves));
    }

    /// Put a road on a square, or take it off, as an engineer's work would leave it.
    pub fn scenario_set_road(&mut self, at: crate::map::Position, road: bool) {
        if let Some(map) = self.map.as_mut() {
            map.plane_mut(at.plane).set_road(at.x, at.y, road);
        }
    }

    /// Change the terrain of a square, as the setup of a game does when it builds an island out.
    pub fn scenario_set_terrain(
        &mut self,
        at: crate::map::Position,
        terrain: crate::map::TerrainId,
    ) {
        if let Some(map) = self.map.as_mut() {
            map.plane_mut(at.plane).set(at.x, at.y, terrain);
        }
    }

    /// Enchant the roads within two squares of `at`, as the spell does.
    pub fn scenario_enchant_roads(&mut self, at: crate::map::Position) {
        self.enchant_roads(at);
    }

    /// Remove a unit from the game.
    pub fn scenario_remove_unit(&mut self, unit: hecs::Entity) {
        self.remove_unit(unit, crate::hero::Fate::Released);
    }

    /// Put a building in a city, or take it out, as a scenario sets it up (no cost, no
    /// requirements).
    pub fn scenario_set_building(&mut self, city: hecs::Entity, building: BuildingId, built: bool) {
        if let Ok(mut b) = self.ecs.get::<&mut Buildings>(city) {
            b.0.retain(|&x| x != building);
            if built {
                b.0.push(building);
            }
        }
    }
}
