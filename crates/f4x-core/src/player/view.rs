//! What one player may know of the game: all of its own, and of the others only what it can see
//! or has explored. An AI wizard reads the game through this and plays through
//! [`World::apply`], so it cannot see more than a human player does.
//!
//! The view borrows the game for as long as it lives, so it is made where it is read and let go
//! before a command is sent. It is cheap to make: what a player sees now (`visibility`) is worked
//! out the first time it is asked for.
//!
//! The rule it keeps: the *lists* are limited to what the player knows (its own units and
//! cities, the cities of others on squares it has explored, the units on squares it sees now,
//! the terrain, lairs and nodes of explored squares). A handle that comes out of a list (a unit
//! or a city) may then be asked about freely, as a player may look at a unit it sees.

use crate::rules::Ability;
use f4x_def::half_int::HalfInt;
use std::cell::OnceCell;

use crate::city::economy::CityEconomy;
use crate::city::production::{Production, ProductionError, ProductionItem};
use crate::diplomacy::Proposal;
use crate::map::terrain::TerrainClass;
use crate::map::Position;
use crate::player::{PlayerId, Treasury};
use crate::world::serial::Serial;
use f4x_def::content::Content;

use crate::player::finance::Budget;

use crate::rules::{RealmId, SpellDef, SpellId, Stat, UnitDef, UnitId};
use crate::world::World;

/// What the player may read of another wizard's city: the enemy city screen of the original
/// (`Enemy_City_Screen`), which is what a human gets. The size and name, the race, the citizens,
/// the units standing in it (invisible ones left out), the enchantments with who cast each, and
/// the buildings. Not the production, the gold or the food: the screen shows none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CityReport {
    pub city: hecs::Entity,
    pub owner: PlayerId,
    pub position: Position,
    pub race: Option<crate::rules::RaceId>,
    /// The owner's fortress city (the screen calls it the capital).
    pub fortress: bool,
    /// Citizens in all (zero is an outpost).
    pub figures: u32,
    pub farmers: u32,
    pub workers: u32,
    pub rebels: u32,
    /// What is built, in build order (a replaced building stays in the list).
    pub buildings: Vec<crate::rules::BuildingId>,
    /// The lasting spells on the city, each with the wizard who cast it.
    pub enchantments: Vec<(SpellId, PlayerId)>,
    /// The owner's units standing in it, in creation order. Invisible units are not listed.
    pub garrison: Vec<hecs::Entity>,
    /// Does the owner's summoning circle stand in it? The cityscape draws it.
    pub summoning_circle: bool,
}

/// What the special tiles in a city's area give it, all of them counted (whoever else works a
/// tile too): what the city could specialize in.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CitySpecials {
    pub gold: f32,
    pub mana: f32,
    pub food: f32,
    /// Percent off the cost of military units (coal, iron).
    pub unit_cost_percent: f32,
    /// Nightshade plants.
    pub nightshade: u32,
    /// Percent of production the land itself gives (forest, hills, mountains, desert, nature and
    /// chaos nodes), over the whole area.
    pub terrain_production: f32,
    /// River squares (two food each, whatever they are laid on).
    pub rivers: u32,
}

impl CitySpecials {
    pub fn is_empty(&self) -> bool {
        *self == CitySpecials::default()
    }
}

pub struct PlayerView<'a> {
    world: &'a World,
    me: PlayerId,
    seen: [OnceCell<Vec<bool>>; 2],
    /// A number the caller works out once for this view (see [`PlayerView::memo`]).
    memo: OnceCell<Option<f32>>,
}

impl<'a> PlayerView<'a> {
    pub fn new(world: &'a World, me: PlayerId) -> Self {
        Self {
            world,
            me,
            seen: [OnceCell::new(), OnceCell::new()],
            memo: OnceCell::new(),
        }
    }

    pub fn me(&self) -> PlayerId {
        self.me
    }

    /// One number of the caller's (the AI's average unit strength), worked out by `compute` on
    /// the first call and kept for the life of the view. A view borrows the game, so the game
    /// cannot change under it: the kept number is the one a new call would give. The view holds
    /// one slot; every caller of one view must ask for the same number.
    pub fn memo(&self, compute: impl FnOnce() -> Option<f32>) -> Option<f32> {
        *self.memo.get_or_init(compute)
    }

    // ---- the game and the player's own state ---------------------------------------

    /// The definitions of the game, which are public.
    pub fn content(&self) -> &'a Content {
        self.world.content()
    }

    /// Has the game ended?
    /// The name of the `index`th city of a race (see [`World::city_name_for`]).
    pub fn city_name_for(&self, race: crate::rules::RaceId, index: usize) -> String {
        self.world.city_name_for(race, index)
    }

    /// The player's gold and mana.
    pub fn treasury(&self) -> Treasury {
        self.world.player(self.me).map_or(
            Treasury {
                gold: HalfInt::ZERO,
                mana: HalfInt::ZERO,
            },
            |p| p.treasury(),
        )
    }

    /// Has the player lost everything?
    pub fn is_defeated(&self) -> bool {
        self.world.player(self.me).is_some_and(|p| p.is_defeated())
    }

    /// The offers made to the player at the turn change that wait for an answer.
    pub fn offers(&self) -> Vec<crate::player::offer::Offer> {
        self.world.offers(self.me)
    }

    /// The player's wizard, if it has one.
    pub fn wizard(&self) -> Option<crate::wizard::entity::Wizard<'a>> {
        self.world.wizard(self.me)
    }

    /// What the player's income and expenses come to this turn.
    pub fn budget(&self) -> Budget {
        self.world.budget(self.me)
    }

    /// The spells on offer for research.
    pub fn research_choices(&self) -> Vec<SpellId> {
        self.world
            .wizard(self.me)
            .map(|w| w.candidates())
            .unwrap_or_default()
    }

    /// What casting a spell overland costs the player.
    pub fn casting_cost(&self, spell: SpellId) -> i32 {
        self.world.casting_cost(self.me, spell, false)
    }

    /// The other wizards casting the Spell of Mastery and the mana each has still to pay.
    pub fn rival_mastery_casters(&self) -> Vec<(PlayerId, i32)> {
        let mut found = self.world.mastery_casters();
        found.retain(|&(p, _)| p != self.me);
        found
    }

    /// Whether the Spell of Mastery is open to the player (`RuleSettings::spell_of_mastery`).
    pub fn mastery_open(&self) -> bool {
        self.world.mastery_open_to(self.me)
    }

    /// Is the player another wizard (not the neutral player or the monsters)?
    pub fn is_rival_wizard(&self, other: PlayerId) -> bool {
        other != self.me && self.world.is_wizard(other)
    }

    /// Is the player another one that takes part in diplomacy: a wizard or the neutral player of a
    /// race (not the monsters)? A computer player talks to, quarrels with and makes treaties with
    /// these; what needs the wizard itself (the spells) still asks for it.
    pub fn is_diplomat(&self, other: PlayerId) -> bool {
        other != self.me
            && self.world.takes_part_in_diplomacy(other)
            && !self.world.is_dormant(other)
    }

    /// Whether a wizard the player is at war with has a book in one of `realms` (the books of a
    /// wizard are on show in the diplomacy screen).
    pub fn rival_at_war_has_books(&self, realms: &[RealmId]) -> bool {
        self.rivals_at_war().into_iter().any(|p| {
            self.world
                .wizard(p)
                .is_some_and(|w| realms.iter().any(|&r| w.books_in(r) > 0))
        })
    }

    /// The wizards still in the game that the player is at war with.
    pub fn rivals_at_war(&self) -> Vec<PlayerId> {
        self.world
            .player_ids()
            .into_iter()
            .filter(|&p| {
                self.is_rival_wizard(p)
                    && self.world.player(p).is_some_and(|q| !q.is_defeated())
                    && self.treaty_with(p) == crate::diplomacy::Treaty::War
            })
            .collect()
    }

    /// The treaty the player has with another wizard (a player knows its own treaties).
    pub fn treaty_with(&self, other: PlayerId) -> crate::diplomacy::Treaty {
        self.world.treaty(self.me, other)
    }

    /// Whether a peace period still runs between the player and another: it has promised not to
    /// attack.
    pub fn peace_period_with(&self, other: PlayerId) -> bool {
        self.world
            .diplomacy
            .stance(self.me, other)
            .is_some_and(|s| s.peace_duration > 0)
    }

    /// What the player's power makes this turn: the split, the research and the mana upkeep.
    pub fn income(&self) -> crate::wizard::magic_income::Income {
        self.world.income(self.me)
    }

    /// What researching a spell would cost the player (the Spell of Mastery's depends on the
    /// spells it knows).
    pub fn research_cost(&self, spell: SpellId) -> i32 {
        self.world
            .wizard(self.me)
            .map_or(0, |m| m.research_cost(self.world.content(), spell))
    }

    /// A spell that has been paid for over several turns and now waits for its target (or takes
    /// effect at the turn's end).
    pub fn paid_spell(&self) -> Option<SpellId> {
        self.world.wizard(self.me).and_then(|m| m.casting_paid())
    }

    /// Does casting the spell ask for a target?
    pub fn spell_needs_target(&self, spell: SpellId) -> bool {
        self.world.spell_needs_target(self.me, spell)
    }

    /// Would the game accept the spell aimed at the target? (The rules the cast would be checked
    /// by; nothing is cast.)
    pub fn can_aim(&self, spell: SpellId, target: Option<crate::rules::exec::Target>) -> bool {
        self.world.check_target(self.me, spell, target).is_ok()
    }

    /// Is there a free hero a hero-bringing spell could find for the player? (`true` for any other
    /// spell.)
    pub fn hero_to_summon(&self, spell: SpellId) -> bool {
        self.world.units().hero_to_summon(self.me, spell)
    }

    /// The lasting spells on one of the player's units.
    pub fn unit_enchantments(&self, unit: hecs::Entity) -> Vec<SpellId> {
        self.world
            .units()
            .unit_enchantments(unit)
            .into_iter()
            .map(|(s, _)| s)
            .collect()
    }

    /// The lasting spells on a city.
    pub fn city_enchantments(&self, city: hecs::Entity) -> Vec<SpellId> {
        self.world
            .spells()
            .city_enchantments(city)
            .into_iter()
            .map(|(s, _)| s)
            .collect()
    }

    /// The lasting spells on a city that another wizard cast.
    pub fn city_curses(&self, city: hecs::Entity) -> Vec<SpellId> {
        self.world
            .spells()
            .city_enchantments(city)
            .into_iter()
            .filter(|&(_, caster)| caster != self.me)
            .map(|(s, _)| s)
            .collect()
    }

    /// The lasting spells on the whole world that wizards at war with the player keep going, with
    /// the instance to aim Disjunction at.
    pub fn globals_of_enemies(&self) -> Vec<(SpellId, hecs::Entity)> {
        let spells = self.world.spells();
        self.world
            .rival_globals(self.me)
            .into_iter()
            .filter(|&i| {
                spells
                    .caster_player(i)
                    .is_some_and(|o| self.treaty_with(o) == crate::diplomacy::Treaty::War)
            })
            .filter_map(|i| spells.def(i).map(|s| (s, i)))
            .collect()
    }

    /// How much of a spell the wizard pays for in a turn: its casting skill and the mana of its
    /// heroes at the fortress. A spell costing more takes more turns.
    pub fn casting_power(&self) -> i32 {
        self.world
            .wizard(self.me)
            .map_or(0, |w| w.casting_skill(&self.world.content().rules))
            + self.world.hero_casting_skill(self.me)
    }

    /// The proposals other wizards have made to the player and it has not answered.
    pub fn proposals_to_me(&self) -> Vec<Proposal> {
        self.world
            .diplomacy
            .proposals()
            .iter()
            .filter(|p| p.to == self.me)
            .copied()
            .collect()
    }

    // ---- its own units and cities --------------------------------------------------

    /// The player's heroes.
    pub fn my_heroes(&self) -> Vec<hecs::Entity> {
        self.world.units().hero_units(self.me)
    }

    /// The player's vault and the items waiting after a reward.
    pub fn armory(&self) -> crate::item::armory::Armory {
        self.world.items().armory(self.me)
    }

    /// The empty slots of one of the player's heroes: the slot number and what it takes.
    pub fn hero_free_slots(&self, hero: hecs::Entity) -> Vec<(usize, crate::rules::SlotKind)> {
        use crate::item::armory::ItemWorld;
        let items = self.world.items();
        let Some(slots) = items.hero_slots(self.me, hero) else {
            return Vec::new();
        };
        let used: Vec<u8> = items.carried(hero).iter().map(|(s, _)| *s).collect();
        slots
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !used.contains(&(*i as u8)))
            .collect()
    }

    /// The player's own fortress.
    pub fn fortress(&self) -> Option<Position> {
        self.world.wizard(self.me).and_then(|w| w.fortress())
    }

    /// What the player has set its units to do (`Command::SetTasks`); `None` until it has.
    pub fn my_tasks(&self) -> Option<crate::unit::task::Tasks> {
        self.world.player_tasks(self.me)
    }

    pub fn my_units(&self) -> Vec<hecs::Entity> {
        self.world.units().player_units(self.me)
    }

    /// The player's cities (outposts too) and where they stand, oldest first.
    pub fn my_cities(&self) -> Vec<(hecs::Entity, Position)> {
        self.world.economy().cities_of(self.me)
    }

    pub fn serial_of(&self, entity: hecs::Entity) -> Serial {
        self.world.units().serial_of(entity)
    }

    pub fn sort_by_serial(&self, entities: &mut [hecs::Entity]) {
        self.world.units().sort_by_serial(entities);
    }

    pub fn unit_def(&self, unit: hecs::Entity) -> Option<&'a UnitDef> {
        self.world.units().unit_def(unit)
    }

    pub fn unit_position(&self, unit: hecs::Entity) -> Option<Position> {
        self.world.units().unit_position(unit)
    }

    pub fn unit_stat(&self, unit: hecs::Entity, stat: Stat) -> i32 {
        self.world.units().unit_stat(unit, stat)
    }

    /// The movement the unit has in a turn.
    pub fn unit_max_moves(&self, unit: hecs::Entity) -> HalfInt {
        self.world.units().unit_max_moves(unit)
    }

    /// How far the unit sees, in squares.
    pub fn unit_sight(&self, unit: hecs::Entity) -> HalfInt {
        self.world.units().unit_sight(unit)
    }

    /// Movement points the unit has left this turn.
    pub fn unit_moves_left(&self, unit: hecs::Entity) -> HalfInt {
        self.world.units().unit_moves_left(unit)
    }

    /// `(current, maximum)` hit points over all figures.
    pub fn unit_hits(&self, unit: hecs::Entity) -> (u32, u32) {
        self.world.units().unit_hits(unit)
    }

    /// Whether the unit is a walker, by nature and by what it carries or has cast on it
    /// (`Economy::is_walker`): the only kind a ship fetches or a land route is planned for.
    pub fn is_walker(&self, unit: hecs::Entity) -> bool {
        self.world.economy().is_walker(unit)
    }

    /// [`PlayerView::is_walker`] for the unit in the stack it stands in
    /// (`Economy::is_walker_in_stack`): a stack with a Wind Walker is carried through the air.
    pub fn is_walker_in_stack(&self, unit: hecs::Entity) -> bool {
        self.world.economy().is_walker_in_stack(unit)
    }

    pub fn unit_has_ability(&self, unit: hecs::Entity, ability: Ability) -> bool {
        self.world.units().unit_has_ability(unit, ability)
    }

    /// Whether one of the unit's abilities sets the flag (`Units::unit_has_flag`).
    pub fn unit_has_flag(
        &self,
        unit: hecs::Entity,
        flag: crate::rules::unit_flag::UnitFlag,
    ) -> bool {
        self.world.units().unit_has_flag(unit, flag)
    }

    /// Whether the unit can found a city.
    pub fn unit_is_settler(&self, unit: hecs::Entity) -> bool {
        self.world.units().unit_is_settler(unit)
    }

    /// Whether the unit's weapons are magical (heroes, fantastic creatures, a flaming or blessed
    /// weapon): they hurt what normal weapons barely scratch.
    pub fn unit_has_magical_weapon(&self, unit: hecs::Entity) -> bool {
        self.world.units().unit_has_magical_weapon(unit)
    }

    /// How quickly the unit gets to a square (`unit::mobility`).
    pub fn unit_mobility(&self, unit: hecs::Entity) -> f32 {
        self.world.units().unit_mobility(unit)
    }

    /// How quickly a party gets to a square: the slowest unit's pace, unless the stack is carried.
    pub fn party_mobility(&self, units: &[hecs::Entity]) -> f32 {
        self.world.units().party_mobility(units)
    }

    /// What the unit adds to a battle at full health (`battle::power::unit_power`).
    pub fn unit_power(&self, unit: hecs::Entity) -> f32 {
        self.world.units().unit_power(unit)
    }

    /// What the unit would add to a battle with the spell lasting on it (`UnitView::unit_power_with_spell`).
    pub fn unit_power_with_spell(&self, unit: hecs::Entity, spell: SpellId) -> f32 {
        self.world.units().unit_power_with_spell(unit, spell)
    }

    /// Is the unit walking a path it was ordered to?
    pub fn unit_has_order(&self, unit: hecs::Entity) -> bool {
        self.world.unit_has_order(unit)
    }

    /// The square the unit's standing order walks to (the end of its path).
    pub fn unit_destination(&self, unit: hecs::Entity) -> Option<Position> {
        self.world.unit_destination(unit)
    }

    /// The magic nodes on explored squares: where each stands, who holds it, whether a Guardian
    /// Spirit does, whether it is warped.
    pub fn nodes(&self) -> Vec<crate::feature::entity::NodeInfo> {
        self.world
            .economy()
            .nodes()
            .into_iter()
            .filter(|n| self.explored(n.pos))
            .collect()
    }

    /// Is the engineer at work on a road?
    pub fn unit_builds_road(&self, unit: hecs::Entity) -> bool {
        self.world.unit_building_road(unit)
    }

    /// The unit's standing order.
    pub fn unit_order(&self, unit: hecs::Entity) -> Option<crate::unit::Order> {
        self.world.unit_order(unit)
    }

    /// Whether a road can be built on the square (not on water): a way that crosses water (a ship's
    /// square) is no way for a road.
    pub fn road_possible(&self, at: Position) -> bool {
        self.world.turns_to_build_road(at).is_some()
    }

    /// Is there a road on an explored square?
    pub fn has_road(&self, at: Position) -> bool {
        self.explored(at)
            && self
                .world
                .map()
                .is_some_and(|m| m.plane(at.plane).has_road(at.x as i32, at.y as i32))
    }

    /// The squares of the way the stack would walk to `to` (the start left out), `None` when it
    /// has none.
    pub fn path(&self, units: &[hecs::Entity], to: Position) -> Option<Vec<Position>> {
        self.world.find_path(units, to)
    }

    /// The way the stack would walk from `from` (its square or another) to `to`, each square
    /// with the turns until it stands there (`World::way`).
    pub fn way(
        &self,
        units: &[hecs::Entity],
        from: Position,
        to: Position,
    ) -> Option<Vec<(Position, u32)>> {
        self.world.way(units, from, to)
    }

    /// The units aboard a ship (they stay aboard in a port until they step ashore).
    pub fn passengers_of(&self, ship: hecs::Entity) -> Vec<hecs::Entity> {
        self.world.economy().passengers_of(ship)
    }

    /// The food a unit eats per turn.
    pub fn unit_food(&self, unit: hecs::Entity) -> f4x_def::half_int::HalfInt {
        self.world.units().unit_food(unit)
    }

    /// The food these units eat together per turn.
    pub fn food_of(&self, units: &[hecs::Entity]) -> f4x_def::half_int::HalfInt {
        self.world.units().food_of(units)
    }

    /// The race of a city.
    pub fn city_race(&self, city: hecs::Entity) -> Option<f4x_def::rules::RaceId> {
        self.world.economy().city_race(city)
    }

    /// The player's own people: a wizard's capital race, an independent race player's race.
    pub fn my_race(&self) -> Option<f4x_def::rules::RaceId> {
        self.wizard()
            .and_then(|w| w.capital_race())
            .or_else(|| self.world.race_of_player(self.me))
    }

    /// What one of the player's own cities has built (nothing for another's: see `city_report`).
    pub fn city_buildings(&self, city: hecs::Entity) -> Vec<crate::rules::BuildingId> {
        if !self.is_my_city(city) {
            return Vec::new();
        }
        self.world.city_buildings(city)
    }

    /// Is the city the player's own? What a city makes, builds and costs is read of its own
    /// cities only; of another wizard's the player has the `city_report`.
    fn is_my_city(&self, city: hecs::Entity) -> bool {
        self.world.economy().city_owner(city) == Some(self.me)
    }

    /// What one of the player's cities is building (nothing for another's: see `city_report`).
    pub fn city_production(&self, city: hecs::Entity) -> Production {
        if !self.is_my_city(city) {
            return Production::default();
        }
        self.world.economy().city_production(city)
    }

    pub fn can_build(
        &self,
        city: hecs::Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        if !self.is_my_city(city) {
            return Err(ProductionError::NotACity);
        }
        self.world.economy().can_build(city, item)
    }

    pub fn buy_cost(&self, city: hecs::Entity) -> Option<HalfInt> {
        if !self.is_my_city(city) {
            return None;
        }
        self.world.economy().buy_cost(city)
    }

    /// The figures of one of the player's cities (food, production, gold, ...); `None` for
    /// another's, of which the player has the citizens in `city_report`.
    pub fn city_economy(&self, city: hecs::Entity) -> Option<CityEconomy> {
        if !self.is_my_city(city) {
            return None;
        }
        self.world.economy().city_economy(city)
    }

    /// The food the player's cities would add if half of their workers went to the fields: what
    /// an army that eats more can still be fed with, at the cost of production (the balance of
    /// farmers puts people there as the food is needed).
    pub fn farming_headroom(&self) -> f4x_def::half_int::HalfInt {
        let economy = self.world.economy();
        self.my_cities()
            .into_iter()
            .filter_map(|(city, _)| {
                let now = economy.city_economy(city)?;
                let more = economy.city_economy_with(city, now.farmers + now.workers / 2)?;
                Some((more.food - now.food).max(f4x_def::half_int::HalfInt::ZERO))
            })
            .sum()
    }

    /// The units the player knows to stand in a city or on a square: nothing for a square it
    /// has not explored; its own units there; and another player's only when the square is in
    /// sight now and the unit can be made out (as `hostile_at`). Of another wizard's city it has
    /// explored but does not see, `city_report` lists the units, as the enemy city screen does.
    pub fn garrison(&self, at: Position) -> Vec<hecs::Entity> {
        if !self.explored(at) {
            return Vec::new();
        }
        let sees = self.sees(at);
        let mut units = self.world.economy().garrison(at);
        units.retain(|&u| {
            self.world.units().unit_owner(u) == Some(self.me)
                || (sees && self.world.economy().unit_visible_to(u, self.me))
        });
        units
    }

    /// The game's difficulty: 0 intro, 1 easy, 2 normal, 3 hard, 4 extreme, 5 impossible.
    pub fn difficulty(&self) -> u8 {
        self.world.difficulty()
    }

    // ---- what it knows of the world ------------------------------------------------

    /// Does a diagonal step cost more than a straight one, in movement and in sight (the setting
    /// `diagonal_extra_cost`)?
    pub fn diagonal_extra_cost(&self) -> bool {
        self.world.settings.basic.diagonal_extra_cost
    }

    /// Has the player explored the square?
    pub fn explored(&self, at: Position) -> bool {
        self.world.is_explored(self.me, at)
    }

    /// The share (0 to 1) of a plane's squares the player has explored.
    pub fn explored_share(&self, plane: crate::map::Plane) -> f32 {
        let Some(grid) = self.world.map().map(|m| m.plane(plane)) else {
            return 0.0;
        };
        let (w, h) = (grid.width(), grid.height());
        let seen = (0..h)
            .flat_map(|y| (0..w).map(move |x| Position { plane, x, y }))
            .filter(|&p| self.explored(p))
            .count();
        seen as f32 / (w * h).max(1) as f32
    }

    /// Does the player see the square now?
    pub fn sees(&self, at: Position) -> bool {
        if let Some(sees) = self.world.cached_sees(self.me, at) {
            return sees;
        }
        let seen = self.seen[at.plane.index()]
            .get_or_init(|| self.world.economy().visibility(self.me, at.plane));
        let width = self.world.map().map_or(0, |m| m.width());
        seen.get((at.y * width + at.x) as usize)
            .copied()
            .unwrap_or(false)
    }

    /// The cities of other players on squares the player has explored.
    pub fn known_cities(&self) -> Vec<(hecs::Entity, PlayerId, Position)> {
        self.world
            .economy()
            .city_places(|owner, p| owner != self.me && self.explored(p))
    }

    /// Is the city its owner's fortress city (the wizard's seat)? Asked of a city the player has
    /// explored.
    pub fn is_fortress_city(&self, city: hecs::Entity) -> bool {
        self.world.economy().is_fortress_city(city)
    }

    /// The city on a square the player has explored.
    pub fn city_at(&self, at: Position) -> Option<hecs::Entity> {
        self.explored(at)
            .then(|| self.world.economy().city_at(at))
            .flatten()
    }

    pub fn city_owner(&self, city: hecs::Entity) -> Option<PlayerId> {
        self.world.economy().city_owner(city)
    }

    /// What the enemy city screen shows of a city of another player on a square the player has
    /// explored (`CityReport`); `None` for its own city, a city on a square it has not explored
    /// (the screen says "No Report") and a handle that is no city. It is read live: what the
    /// player explored once it may look at, as the human may, seen now or not.
    pub fn city_report(&self, city: hecs::Entity) -> Option<CityReport> {
        let economy = self.world.economy();
        let owner = economy.city_owner(city)?;
        let position = self.world.city_position(city)?;
        if owner == self.me || !self.explored(position) {
            return None;
        }
        let figures = economy.city_economy(city).unwrap_or_default();
        let units = self.world.units();
        let mut garrison: Vec<hecs::Entity> = units
            .units_at(position)
            .into_iter()
            .filter(|&u| units.unit_owner(u) == Some(owner))
            .filter(|&u| !units.unit_has_ability(u, Ability::new(33 /* invisibility */)))
            .collect();
        self.sort_by_serial(&mut garrison);
        Some(CityReport {
            city,
            owner,
            position,
            race: economy.city_race(city),
            fortress: economy.is_fortress_city(city),
            figures: figures.figures,
            farmers: figures.farmers,
            workers: figures.workers,
            rebels: figures.rebels,
            buildings: self.world.city_buildings(city),
            enchantments: self.world.spells().city_enchantments(city),
            garrison,
            summoning_circle: self.world.wizard(owner).and_then(|w| w.summon_at())
                == Some(position),
        })
    }

    /// The special tiles in the area of one of the player's cities; `None` for another's.
    pub fn city_specials(&self, city: hecs::Entity) -> Option<CitySpecials> {
        if !self.is_my_city(city) {
            return None;
        }
        let at = self.world.city_position(city)?;
        Some(self.site_specials(at))
    }

    /// The special tiles in the area a city founded at `at` would work, as far as the player has
    /// explored them (what is not explored counts for nothing).
    pub fn site_specials(&self, at: Position) -> CitySpecials {
        let mut out = CitySpecials::default();
        let Some(map) = self.world.map() else {
            return out;
        };
        let grid = map.plane(at.plane);
        let features = &self.world.content().features;
        let nightshade = features.id_numbered(16); // nightshade
        for (p, _) in self.world.economy().city_area(at) {
            if !self.explored(p) {
                continue;
            }
            if grid.has_river(p.x as i32, p.y as i32) {
                out.rivers += 1;
            }
            out.terrain_production +=
                self.world
                    .economy()
                    .square_production(grid, p.x as i32, p.y as i32, false) as f32;
            let Some(id) = grid.mineral(p.x as i32, p.y as i32) else {
                continue;
            };
            if Some(id) == nightshade {
                out.nightshade += 1;
            }
            let Some(def) = features.get(id) else {
                continue;
            };
            out.gold += def.city.gold.as_f32();
            out.mana += def.city.mana.as_f32();
            out.food += def.city.food.as_f32();
            out.unit_cost_percent += def.city.unit_cost_percent as f32;
        }
        out
    }

    /// Width and height of a plane of the map.
    pub fn map_size(&self) -> Option<(u32, u32)> {
        self.world.map().map(|m| (m.width(), m.height()))
    }

    /// The least distance between two cities in this game (the `min_city_distance` setting).
    pub fn min_city_distance(&self) -> u32 {
        u32::from(self.world.settings().rules.min_city_distance)
    }

    /// The size at which a city stops growing in this game (the `max_city_size` setting).
    pub fn max_city_size(&self) -> u32 {
        u32::from(self.world.settings().rules.max_city_size)
    }

    /// How many units may stand on `at` when a stack walks in (`World::tile_limit`).
    pub fn tile_limit(&self, at: crate::map::Position) -> usize {
        self.world.tile_limit(at)
    }

    /// Most units that may stand on one square in this game.
    pub fn max_tile(&self) -> usize {
        self.world.max_tile()
    }

    /// Whether the unit keeps order in a city it stands in: only normal units and heroes do
    /// (half a rebel each; fantastic creatures defend but do not police).
    pub fn is_garrison(&self, unit: hecs::Entity) -> bool {
        self.world.units().is_garrison(unit)
    }

    /// Most units one side fights with in a battle in this game.
    pub fn max_battle_side(&self) -> usize {
        self.world.max_battle_side()
    }

    /// Hostile units or a hostile city on a square the player sees now (`None` if there is
    /// nothing, or the square is out of sight: the player does not know). The units are those
    /// it can make out.
    pub fn hostile_at(&self, at: Position) -> Option<Vec<hecs::Entity>> {
        if !self.sees(at) {
            return None;
        }
        let units = self.world.economy().hostile_at(at, self.me)?;
        Some(
            units
                .into_iter()
                .filter(|&u| self.world.economy().unit_visible_to(u, self.me))
                .collect(),
        )
    }

    /// The units of wizards at war with the player on the squares within `radius` of `at` that
    /// it sees now (the units it can make out), as one stack per square.
    pub fn enemies_near(&self, at: Position, radius: i32) -> Vec<(Position, Vec<hecs::Entity>)> {
        let at_war = self.rivals_at_war();
        let mut found = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let Some(square) = self.shift(at, dx, dy) else {
                    continue;
                };
                let Some(units) = self.hostile_at(square) else {
                    continue;
                };
                let units: Vec<_> = units
                    .into_iter()
                    .filter(|&u| {
                        self.world
                            .units()
                            .unit_owner(u)
                            .is_some_and(|o| at_war.contains(&o))
                    })
                    .collect();
                if !units.is_empty() {
                    found.push((square, units));
                }
            }
        }
        found
    }

    /// The class of terrain on an explored square.
    pub fn terrain_class(&self, at: Position) -> Option<TerrainClass> {
        self.explored(at)
            .then(|| self.world.economy().terrain_class_at(at))
            .flatten()
    }

    /// Is there a river on an explored square?
    pub fn has_river(&self, at: Position) -> bool {
        self.explored(at)
            && self
                .world
                .map()
                .is_some_and(|m| m.plane(at.plane).has_river(at.x as i32, at.y as i32))
    }

    /// Is there an enchanted road on an explored square (every road on Myrror is one)?
    pub fn has_enchanted_road(&self, at: Position) -> bool {
        self.explored(at)
            && self.world.map().is_some_and(|m| {
                let grid = m.plane(at.plane);
                (at.plane == crate::map::Plane::Myrror && grid.has_road(at.x as i32, at.y as i32))
                    || grid.has_enchanted_road(at.x as i32, at.y as i32)
            })
    }

    /// Is an explored square under a magic node's influence?
    pub fn in_node(&self, at: Position) -> bool {
        self.explored(at) && self.world.economy().node_covering(at).is_some()
    }

    /// Is there a guarded site on an explored square?
    pub fn has_lair(&self, at: Position) -> bool {
        self.explored(at) && self.world.economy().lair_at(at).is_some()
    }

    /// The towers the player has seen, on the planes it has seen them on, and whether each still
    /// has its guards. A stack standing on a tower with none can change plane.
    pub fn towers(&self) -> Vec<(Position, bool)> {
        let economy = self.world.economy();
        let mut found = Vec::new();
        for lair in economy.lairs() {
            if !economy.is_tower(lair.pos) {
                continue;
            }
            for plane in crate::map::Plane::all() {
                let at = Position { plane, ..lair.pos };
                if self.explored(at) {
                    found.push((at, lair.intact));
                }
            }
        }
        found
    }

    /// Could the stack change plane where it stands?
    pub fn can_shift_plane(&self, units: &[hecs::Entity]) -> bool {
        // Off a tower the game refuses a crossing into a square held by an enemy or one without
        // room (planar travel, an Astral Gate), so this asks the same.
        let Ok(to) = self.world.economy().can_shift_plane(units) else {
            return false;
        };
        // In a tower the stack is on both planes already: nobody else can be there.
        if units
            .first()
            .and_then(|&u| self.world.units().unit_position(u))
            .is_some_and(|from| self.world.economy().tower_mirror(from).is_some())
        {
            return true;
        }
        self.world.economy().hostile_at(to, self.me).is_none()
            && self.world.units().units_at(to).len() + units.len() <= self.world.tile_limit(to)
    }

    /// The explored stands nobody holds: a lair without guards, or a cleared node or tower with
    /// no owner. Monsters settle in them.
    pub fn vacant_zones(&self) -> Vec<Position> {
        self.world
            .economy()
            .lairs()
            .into_iter()
            .map(|l| l.pos)
            .filter(|&p| self.explored(p) && self.world.vacant_zone(p).is_some())
            .collect()
    }

    /// What guards the site on a square, once the player has fought for it (otherwise the
    /// player does not know).
    pub fn lair_guards(&self, at: Position) -> Option<Vec<(UnitId, u32)>> {
        self.world.economy().lair_guards_known_to(at, self.me)
    }

    /// Could the player found a city on this explored square?
    pub fn can_found_city(&self, at: Position) -> bool {
        self.explored(at) && self.world.can_found_city(at).is_ok()
    }

    /// The square `dx`, `dy` away from `from`: the map wraps east and west and ends at the poles
    /// (`None` there).
    pub fn shift(&self, from: Position, dx: i32, dy: i32) -> Option<Position> {
        let grid = self.world.map()?.plane(from.plane);
        let y = from.y as i32 + dy;
        (0..grid.height() as i32).contains(&y).then(|| Position {
            plane: from.plane,
            x: grid.wrap_x(from.x as i32 + dx),
            y: y as u32,
        })
    }

    /// King-move distance with the east-west wrap; `None` across planes.
    pub fn distance(&self, a: Position, b: Position) -> Option<u32> {
        if a.plane != b.plane {
            return None;
        }
        let map = self.world.map()?;
        Some(map.plane(a.plane).chebyshev((a.x, a.y), (b.x, b.y)))
    }

    /// Is there a way for the stack to the square? (The game's own path check: the one an order
    /// to go there would use.)
    pub fn can_reach(&self, units: &[hecs::Entity], to: Position) -> bool {
        self.world.find_path(units, to).is_some()
    }

    /// Every square the stack can walk to from where it stands (see `World::reachable_squares`):
    /// one search, for asking about many squares.
    pub fn reachable(&self, units: &[hecs::Entity]) -> crate::fast::FastSet<Position> {
        self.world.reachable_squares(units)
    }

    /// Whether the stack can walk to any of the squares (one search that stops at the first).
    pub fn reaches_any(&self, units: &[hecs::Entity], targets: &[Position]) -> bool {
        self.world.reaches_any(units, targets)
    }

    /// The squares the stack can walk to when it reaches none of `targets` (`None` when it
    /// reaches one): see `World::unreached_region`.
    pub fn unreached_region(
        &self,
        units: &[hecs::Entity],
        targets: &[Position],
    ) -> Option<crate::fast::FastSet<Position>> {
        self.world.unreached_region(units, targets)
    }

    /// What decides the squares a stack can walk to: stacks with an equal signature reach the
    /// same squares, so a caller can search once for them all.
    pub fn reach_signature(
        &self,
        units: &[hecs::Entity],
    ) -> Option<crate::unit::pathing::ReachSignature> {
        self.world.reach_signature(units)
    }

    /// A spell's definition (the content is public).
    pub fn spell(&self, spell: SpellId) -> &'a SpellDef {
        self.world.content().rules.spells.get(spell)
    }
}
