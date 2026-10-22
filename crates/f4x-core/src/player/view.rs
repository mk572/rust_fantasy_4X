//! What one player may know of the game: all of its own, and of the others only what it can see
//! or has explored. An AI wizard reads the game through this and plays through
//! [`Game::apply`], so it cannot see more than a human player does (task 181).
//!
//! The view borrows the game for as long as it lives, so it is made where it is read and let go
//! before a command is sent. It is cheap to make: what a player sees now (`visibility`) is worked
//! out the first time it is asked for.
//!
//! The rule it keeps: the *lists* are limited to what the player knows (its own units and
//! cities, the cities of others on squares it has explored, the units on squares it sees now,
//! the terrain, lairs and nodes of explored squares). A handle that comes out of a list (a unit
//! or a city) may then be asked about freely, as a player may look at a unit it sees.

use std::cell::OnceCell;

use crate::city::economy::CityEconomy;
use crate::city::production::{Production, ProductionError, ProductionItem};
use crate::content::{Content, TerrainClass};
use crate::diplomacy::Proposal;
use crate::game::serial::Serial;
use crate::map::{Plane, Position};
use crate::player::{PlayerId, Treasury};

use crate::player::finance::Budget;

use crate::game::Game;
use crate::rules::{SpellDef, SpellId, Stat, UnitDef};

pub struct PlayerView<'a> {
    game: &'a Game,
    me: PlayerId,
    seen: OnceCell<[Vec<bool>; 2]>,
}

impl<'a> PlayerView<'a> {
    pub fn new(game: &'a Game, me: PlayerId) -> Self {
        Self {
            game,
            me,
            seen: OnceCell::new(),
        }
    }

    pub fn me(&self) -> PlayerId {
        self.me
    }

    // ---- the game and the player's own state ---------------------------------------

    /// The definitions of the game, which are public.
    pub fn content(&self) -> &'a Content {
        self.game.content()
    }

    /// The turn the game is at.
    pub fn turn(&self) -> u32 {
        self.game.turn
    }

    /// Has the game ended?
    pub fn game_over(&self) -> bool {
        self.game.game_over.is_some()
    }

    /// The player's gold and mana.
    pub fn treasury(&self) -> Treasury {
        self.game
            .treasury(self.me)
            .unwrap_or(Treasury { gold: 0, mana: 0 })
    }

    /// Has the player lost everything?
    pub fn is_defeated(&self) -> bool {
        self.game.is_defeated(self.me)
    }

    /// The offers made to the player at the turn change that wait for an answer.
    pub fn offers(&self) -> Vec<crate::player::offer::Offer> {
        self.game.offers(self.me)
    }

    /// The player's wizard, if it has one.
    pub fn wizard(&self) -> Option<crate::wizard::entity::Wizard<'a>> {
        self.game.wizard(self.me)
    }

    /// What the player's income and expenses come to this turn.
    pub fn budget(&self) -> Budget {
        self.game.budget(self.me)
    }

    /// The spells on offer for research.
    pub fn research_choices(&self) -> Vec<SpellId> {
        self.game
            .wizard(self.me)
            .map(|w| w.candidates())
            .unwrap_or_default()
    }

    /// What casting a spell overland costs the player.
    pub fn casting_cost(&self, spell: SpellId) -> i32 {
        self.game.casting_cost(self.me, spell, false)
    }

    /// The other wizards casting the Spell of Mastery and the mana each has still to pay.
    pub fn rival_mastery_casters(&self) -> Vec<(PlayerId, i32)> {
        let mut found = self.game.mastery_casters();
        found.retain(|&(p, _)| p != self.me);
        found
    }

    /// Is the player another wizard (not the neutral player or the monsters)?
    pub fn is_rival_wizard(&self, other: PlayerId) -> bool {
        other != self.me && self.game.is_wizard(other)
    }

    /// What the player's power makes this turn: the split, the research and the mana upkeep.
    pub fn income(&self) -> crate::wizard::magic_income::Income {
        self.game.income(self.me)
    }

    /// What researching a spell would cost the player (the Spell of Mastery's depends on the
    /// spells it knows).
    pub fn research_cost(&self, spell: SpellId) -> i32 {
        self.game
            .wizard(self.me)
            .map_or(0, |m| m.research_cost(self.game.content(), spell))
    }

    /// A spell that has been paid for over several turns and now waits for its target (or takes
    /// effect at the turn's end).
    pub fn paid_spell(&self) -> Option<SpellId> {
        self.game.wizard(self.me).and_then(|m| m.casting_paid())
    }

    /// Does casting the spell ask for a target?
    pub fn spell_needs_target(&self, spell: SpellId) -> bool {
        self.game.spell_needs_target(self.me, spell)
    }

    /// Would the game accept the spell aimed at the target? (The rules the cast would be checked
    /// by; nothing is cast.)
    pub fn can_aim(&self, spell: SpellId, target: Option<crate::rules::exec::Target>) -> bool {
        self.game.check_target(self.me, spell, target).is_ok()
    }

    /// The lasting spells on one of the player's units.
    pub fn unit_enchantments(&self, unit: hecs::Entity) -> Vec<SpellId> {
        self.game
            .units()
            .unit_enchantments(unit)
            .into_iter()
            .map(|(s, _)| s)
            .collect()
    }

    /// The lasting spells on a city.
    pub fn city_enchantments(&self, city: hecs::Entity) -> Vec<SpellId> {
        self.game
            .city_enchantments(city)
            .into_iter()
            .map(|(s, _)| s)
            .collect()
    }

    /// The proposals other wizards have made to the player and it has not answered.
    pub fn proposals_to_me(&self) -> Vec<Proposal> {
        self.game
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
        self.game.units().hero_units(self.me)
    }

    /// The player's vault and the items waiting after a reward.
    pub fn armory(&self) -> Option<hecs::Ref<'a, crate::item::armory::Armory>> {
        self.game.items().armory(self.me)
    }

    /// The empty slots of one of the player's heroes: the slot number and what it takes.
    pub fn hero_free_slots(&self, hero: hecs::Entity) -> Vec<(usize, crate::rules::SlotKind)> {
        use crate::item::armory::ItemWorld;
        let items = self.game.items();
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
        self.game.wizard(self.me).and_then(|w| w.fortress())
    }

    pub fn my_units(&self) -> Vec<hecs::Entity> {
        self.game.units().player_units(self.me)
    }

    /// The player's cities (outposts too) and where they stand, oldest first.
    pub fn my_cities(&self) -> Vec<(hecs::Entity, Position)> {
        self.game
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == self.me)
            .map(|(e, _, p)| (e, p))
            .collect()
    }

    pub fn serial_of(&self, entity: hecs::Entity) -> Serial {
        self.game.serial_of(entity)
    }

    pub fn sort_by_serial(&self, entities: &mut [hecs::Entity]) {
        self.game.sort_by_serial(entities);
    }

    pub fn unit_def(&self, unit: hecs::Entity) -> Option<&'a UnitDef> {
        self.game.units().unit_def(unit)
    }

    pub fn unit_position(&self, unit: hecs::Entity) -> Option<Position> {
        self.game.units().unit_position(unit)
    }

    pub fn unit_stat(&self, unit: hecs::Entity, stat: Stat) -> i32 {
        self.game.units().unit_stat(unit, stat)
    }

    /// Movement points the unit has left this turn.
    pub fn unit_moves_left(&self, unit: hecs::Entity) -> u32 {
        self.game.units().unit_moves_left(unit)
    }

    /// `(current, maximum)` hit points over all figures.
    pub fn unit_hits(&self, unit: hecs::Entity) -> (u32, u32) {
        self.game.units().unit_hits(unit)
    }

    pub fn unit_has_ability(&self, unit: hecs::Entity, ability: &str) -> bool {
        self.game.units().unit_has_ability(unit, ability)
    }

    /// Is the unit walking a path it was ordered to?
    pub fn unit_has_order(&self, unit: hecs::Entity) -> bool {
        self.game.unit_has_order(unit)
    }

    pub fn unit_food_upkeep(&self, unit: hecs::Entity) -> i32 {
        self.game.units().unit_food_upkeep(unit)
    }

    pub fn city_production(&self, city: hecs::Entity) -> Production {
        self.game.economy().city_production(city)
    }

    pub fn can_build(
        &self,
        city: hecs::Entity,
        item: ProductionItem,
    ) -> Result<(), ProductionError> {
        self.game.economy().can_build(city, item)
    }

    pub fn buy_cost(&self, city: hecs::Entity) -> Option<u32> {
        self.game.economy().buy_cost(city)
    }

    pub fn city_economy(&self, city: hecs::Entity) -> Option<CityEconomy> {
        self.game.economy().city_economy(city)
    }

    /// The units standing in a city or on a square.
    pub fn garrison(&self, at: Position) -> Vec<hecs::Entity> {
        self.game.economy().garrison(at)
    }

    // ---- what it knows of the world ------------------------------------------------

    /// Has the player explored the square?
    pub fn explored(&self, at: Position) -> bool {
        self.game.is_explored(self.me, at)
    }

    /// Does the player see the square now?
    pub fn sees(&self, at: Position) -> bool {
        let seen = self.seen.get_or_init(|| {
            [
                self.game.economy().visibility(self.me, Plane::Arcanus),
                self.game.economy().visibility(self.me, Plane::Myrror),
            ]
        });
        let width = self.game.map().map_or(0, |m| m.width());
        seen[at.plane.index()]
            .get((at.y * width + at.x) as usize)
            .copied()
            .unwrap_or(false)
    }

    /// The cities of other players on squares the player has explored.
    pub fn known_cities(&self) -> Vec<(hecs::Entity, PlayerId, Position)> {
        self.game
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, p)| c.owner != self.me && self.explored(*p))
            .map(|(e, c, p)| (e, c.owner, p))
            .collect()
    }

    /// The city on a square the player has explored.
    pub fn city_at(&self, at: Position) -> Option<hecs::Entity> {
        self.explored(at)
            .then(|| self.game.economy().city_at(at))
            .flatten()
    }

    pub fn city_owner(&self, city: hecs::Entity) -> Option<PlayerId> {
        self.game.economy().city_owner(city)
    }

    /// Hostile units or a hostile city on a square the player sees now (`None` if there is
    /// nothing, or the square is out of sight: the player does not know). The units are those
    /// it can make out.
    pub fn hostile_at(&self, at: Position) -> Option<Vec<hecs::Entity>> {
        if !self.sees(at) {
            return None;
        }
        let units = self.game.economy().hostile_at(at, self.me)?;
        Some(
            units
                .into_iter()
                .filter(|&u| self.game.economy().unit_visible_to(u, self.me))
                .collect(),
        )
    }

    /// The class of terrain on an explored square.
    pub fn terrain_class(&self, at: Position) -> Option<TerrainClass> {
        self.explored(at)
            .then(|| self.game.economy().terrain_class_at(at))
            .flatten()
    }

    /// Is there a river on an explored square?
    pub fn has_river(&self, at: Position) -> bool {
        self.explored(at)
            && self
                .game
                .map()
                .is_some_and(|m| m.plane(at.plane).has_river(at.x as i32, at.y as i32))
    }

    /// Is an explored square under a magic node's influence?
    pub fn in_node(&self, at: Position) -> bool {
        self.explored(at)
            && self
                .game
                .map()
                .is_some_and(|m| m.node_covering(at.plane, at.x, at.y).is_some())
    }

    /// Is there a guarded site on an explored square?
    pub fn has_lair(&self, at: Position) -> bool {
        self.explored(at) && self.game.economy().lair_at(at).is_some()
    }

    /// Could the player found a city on this explored square?
    pub fn can_found_city(&self, at: Position) -> bool {
        self.explored(at) && self.game.can_found_city(at).is_ok()
    }

    /// The square `dx`, `dy` away from `from`: the map wraps east and west and ends at the poles
    /// (`None` there).
    pub fn shift(&self, from: Position, dx: i32, dy: i32) -> Option<Position> {
        let grid = self.game.map()?.plane(from.plane);
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
        let map = self.game.map()?;
        Some(map.plane(a.plane).chebyshev((a.x, a.y), (b.x, b.y)))
    }

    /// Is there a way for the stack to the square? (The game's own path check: the one an order
    /// to go there would use.)
    pub fn can_reach(&self, units: &[hecs::Entity], to: Position) -> bool {
        self.game.find_path(units, to).is_some()
    }

    /// Every square the stack can walk to from where it stands (see `Game::reachable_squares`):
    /// one search, for asking about many squares.
    pub fn reachable(&self, units: &[hecs::Entity]) -> std::collections::HashSet<Position> {
        self.game.reachable_squares(units)
    }

    /// A spell's definition (the content is public).
    pub fn spell(&self, spell: SpellId) -> &'a SpellDef {
        self.game.content().rules.spells.get(spell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_magic;
    use crate::game::Command;
    use crate::map::{Grid, Map};

    const ME: PlayerId = PlayerId(0);
    const THEM: PlayerId = PlayerId(1);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn game() -> Game {
        let c = content_with_magic();
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(5);
        g.set_content(c);
        g.set_map(Map::new(Grid::new(30, 20, grass), Grid::new(30, 20, grass)));
        for id in [ME, THEM] {
            g.add_player(id, "W");
        }
        let race = g.content().rules.races.id_of("elf");
        for (owner, pos) in [(ME, at(5, 5)), (THEM, at(20, 10)), (THEM, at(8, 5))] {
            g.apply(Command::FoundCity {
                owner,
                pos,
                name: "Town".into(),
                race,
                population: 3,
            });
        }
        g
    }

    #[test]
    fn a_player_knows_its_own_things_and_only_the_cities_it_has_explored() {
        let mut g = game();
        let walker = g.content().rules.units.id_of("walker").unwrap();
        let mine = g.spawn_unit(ME, at(5, 5), walker);
        g.spawn_unit(THEM, at(20, 11), walker);
        let view = PlayerView::new(&g, ME);
        assert_eq!(view.my_units(), vec![mine]);
        assert_eq!(view.my_cities().len(), 1);
        // Nobody has explored anything but the surroundings of its own city and unit.
        let known: Vec<_> = view
            .known_cities()
            .into_iter()
            .map(|(_, o, p)| (o, p))
            .collect();
        assert!(!known.iter().any(|&(_, p)| p == at(20, 10)), "{known:?}");
        assert!(!view.explored(at(20, 10)) && !view.sees(at(20, 10)));
        assert_eq!(view.city_at(at(20, 10)), None);
        assert_eq!(view.hostile_at(at(20, 11)), None, "out of sight: not known");
        assert_eq!(view.terrain_class(at(20, 10)), None);
        assert!(
            !view.can_found_city(at(25, 15)),
            "unexplored land is not a site"
        );
        drop(view);
        // Once the player has explored the square it knows the city; seeing it is another thing.
        g.explore(ME, at(20, 10), 5);
        let view = PlayerView::new(&g, ME);
        assert!(view
            .known_cities()
            .iter()
            .any(|&(_, o, p)| o == THEM && p == at(20, 10)));
        assert!(view.terrain_class(at(20, 10)).is_some());
        assert!(view.city_at(at(20, 10)).is_some());
        assert!(!view.sees(at(20, 10)), "explored is not seen");
        assert_eq!(view.hostile_at(at(20, 10)), None);
    }

    #[test]
    fn a_square_in_sight_shows_its_hostile_units() {
        let mut g = game();
        let walker = g.content().rules.units.id_of("walker").unwrap();
        g.spawn_unit(ME, at(5, 5), walker);
        let foe = g.spawn_unit(THEM, at(6, 5), walker);
        let view = PlayerView::new(&g, ME);
        assert!(view.sees(at(6, 5)));
        assert_eq!(view.hostile_at(at(6, 5)), Some(vec![foe]));
        assert_eq!(view.hostile_at(at(6, 6)), None, "nothing there");
        // A path search is a search of what the stack can walk to: one flood answers for all.
        let reach = view.reachable(&view.my_units());
        assert!(
            reach.contains(&at(5, 5)) && !reach.contains(&at(6, 5)),
            "the foe's square is shut"
        );
        assert_eq!(view.distance(at(5, 5), at(8, 5)), Some(3));
        assert_eq!(
            view.shift(at(0, 0), -1, 0),
            Some(at(29, 0)),
            "the map wraps"
        );
        assert_eq!(view.shift(at(3, 0), 0, -1), None, "and ends at the pole");
    }
}
