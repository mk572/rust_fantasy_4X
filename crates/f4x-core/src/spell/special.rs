//! Special overland spells with no home of their own: Transmute, Earth Lore, Nature's Cures,
//! Word of Recall, Plane Shift (task 56) and Floating Island. Each is a hook the spell runs; the targets they
//! accept are checked by `check_target` in `spell/cast.rs`.

use crate::map::{Plane, Position};

use crate::game::{Event, Game};
use crate::rules::exec::Target;
use crate::spell::target_error::TargetError;
use crate::unit::Hurt;

pub const TRANSMUTE: &str = "transmute";
pub const EARTH_LORE: &str = "earth_lore";
pub const NATURES_CURES: &str = "natures_cures";
pub const WORD_OF_RECALL: &str = "word_of_recall";
pub const PLANE_SHIFT: &str = "plane_shift";
pub const FLOATING_ISLAND: &str = "floating_island";

/// What Transmute turns each mineral into. Adamantium and the crystals are left alone.
const TRANSMUTED: [(&str, &str); 6] = [
    ("iron_ore", "gold_ore"),
    ("gold_ore", "iron_ore"),
    ("coal", "gem"),
    ("gem", "coal"),
    ("silver_ore", "mithril_ore"),
    ("mithril_ore", "silver_ore"),
];

/// The block Earth Lore reveals, in tiles: the original's map window.
const LORE_WIDTH: u32 = 12;
const LORE_HEIGHT: u32 = 10;

impl Game {
    /// The mineral Transmute makes of the one on `at`, or why there is none it can change.
    pub fn transmute_result(&self, at: Position) -> Result<crate::map::FeatureId, TargetError> {
        let map = self.map.as_ref().ok_or(TargetError::OffMap)?;
        (|| {
            let here = map.plane(at.plane).mineral(at.x as i32, at.y as i32)?;
            let name = &self.content.features.get(here)?.id;
            let (_, to) = TRANSMUTED.iter().find(|(from, _)| from == name)?;
            self.content.features.id_of(to)
        })()
        .ok_or(TargetError::NothingToTransmute)
    }

    /// Floating Island needs a water square with no enemy units on it and room for the island:
    /// it is raised where the wizard aims.
    pub fn check_floating_island(
        &self,
        owner: crate::player::PlayerId,
        at: Position,
    ) -> Result<(), TargetError> {
        match self.economy().terrain_class_at(at) {
            None => return Err(TargetError::OffMap),
            Some(crate::map::terrain::TerrainClass::Ocean) => {}
            Some(_) => return Err(TargetError::NotWater),
        }
        if self.economy().hostile_at(at, owner).is_some() {
            return Err(TargetError::EnemyOnSquare);
        }
        if self.units().units_at(at).len() >= self.max_tile() {
            return Err(TargetError::SquareFull);
        }
        Ok(())
    }

    pub(crate) fn floating_island(&mut self, owner: crate::player::PlayerId, at: Position) {
        if self.check_floating_island(owner, at).is_err() {
            return;
        }
        if let Some(id) = self.content.rules.units.id_of("floating_island") {
            self.spawn_unit(owner, at, id);
        }
    }

    pub(crate) fn transmute(&mut self, at: Position) -> Vec<Event> {
        let Ok(to) = self.transmute_result(at) else {
            return Vec::new();
        };
        if let Some(map) = self.map.as_mut() {
            map.plane_mut(at.plane).set_mineral(at.x, at.y, Some(to));
        }
        vec![Event::TerrainChanged { pos: at }]
    }

    /// Earth Lore: the wizard explores the whole block around the target tile.
    pub(crate) fn earth_lore(&mut self, caster: crate::player::PlayerId, at: Position) {
        self.fog.explore_block(caster, at, LORE_WIDTH, LORE_HEIGHT);
    }

    /// Nature's Cures: every unit on the target's tile is healed in full, except creatures
    /// of the Death realm and the undead.
    pub(crate) fn natures_cures(&mut self, unit: hecs::Entity) {
        let Some(at) = self.units().unit_position(unit) else {
            return;
        };
        let healable: Vec<_> = self
            .units()
            .units_at(at)
            .into_iter()
            .filter(|&u| !self.units().unit_is_death(u))
            .collect();
        for u in healable {
            let _ = self.world.insert_one(u, Hurt(0));
        }
    }

    /// Where Word of Recall would send a unit of `player`: the summoning circle, if it has
    /// room. The original evicts the weakest unit of a full stack; here a full circle makes
    /// the spell unavailable.
    pub fn recall_destination(
        &self,
        player: crate::player::PlayerId,
        unit: hecs::Entity,
    ) -> Result<Position, TargetError> {
        let to = self
            .wizard(player)
            .and_then(|w| w.summon_at())
            .ok_or(TargetError::NoCircle)?;
        let there = self.units().units_at(to);
        (there.contains(&unit) || there.len() < self.max_tile())
            .then_some(to)
            .ok_or(TargetError::CircleFull)
    }

    pub(crate) fn word_of_recall(&mut self, caster: crate::player::PlayerId, unit: hecs::Entity) {
        if let Ok(to) = self.recall_destination(caster, unit) {
            let _ = self.world.insert_one(unit, to);
            self.explore(caster, to, crate::map::fog::sight_for(1));
        }
    }

    /// Where Plane Shift would take the stack `unit` stands in, or why it cannot go:
    /// the Planar Seal is up, an enemy holds the square on the other plane, the stack would
    /// not fit, or one of its units could not stand there.
    pub fn plane_shift_destination(&self, unit: hecs::Entity) -> Result<Position, TargetError> {
        if self.units().planar_seal_active() {
            return Err(TargetError::PlanarSeal);
        }
        let from = self
            .units()
            .unit_position(unit)
            .ok_or(TargetError::WrongTarget)?;
        let owner = self
            .units()
            .unit_owner(unit)
            .ok_or(TargetError::WrongTarget)?;
        let to = Position {
            plane: match from.plane {
                Plane::Arcanus => Plane::Myrror,
                Plane::Myrror => Plane::Arcanus,
            },
            ..from
        };
        let stack = self.units().units_at(from);
        if self.economy().hostile_at(to, owner).is_some() {
            return Err(TargetError::EnemyOnOtherPlane);
        }
        if self.units().units_at(to).len() + stack.len() > self.max_tile() {
            return Err(TargetError::OtherPlaneFull);
        }
        let traits = self.economy().stack_traits(&stack);
        stack
            .iter()
            .all(|&u| self.economy().step_cost(u, to, to, traits).is_ok())
            .then_some(to)
            .ok_or(TargetError::CannotStandThere)
    }

    pub(crate) fn plane_shift(&mut self, caster: crate::player::PlayerId, unit: hecs::Entity) {
        let Some(from) = self.units().unit_position(unit) else {
            return;
        };
        let Ok(to) = self.plane_shift_destination(unit) else {
            return;
        };
        for u in self.units().units_at(from) {
            let _ = self.world.insert_one(u, to);
        }
        self.explore(caster, to, crate::map::fog::sight_for(1));
    }

    /// Run a unit-target special hook. Returns whether the hook was one of these.
    pub(crate) fn run_unit_special(
        &mut self,
        hook: &str,
        caster: crate::player::PlayerId,
        who: Target,
    ) -> bool {
        let Target::Unit(unit) = who else {
            return false;
        };
        match hook {
            NATURES_CURES => self.natures_cures(unit),
            WORD_OF_RECALL => self.word_of_recall(caster, unit),
            PLANE_SHIFT => self.plane_shift(caster, unit),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::def::{CityBonus, FeatureDef, FeatureKind, Features};
    use crate::game::testutil::content_with_magic;
    use crate::game::Command;
    use crate::map::{Grid, Map};
    use crate::player::PlayerId;

    const P0: PlayerId = PlayerId(0);

    fn at(plane: Plane, x: u32, y: u32) -> Position {
        Position { plane, x, y }
    }

    fn world() -> Game {
        let mut c = content_with_magic();
        let mut defs = c.features.defs().to_vec();
        for id in ["iron_ore", "gold_ore", "coal", "gem", "silver_ore"] {
            defs.push(FeatureDef {
                id: id.into(),
                name: id.into(),
                kind: FeatureKind::Mineral,
                city: CityBonus::default(),
                spawn: vec![],
            });
        }
        c.features = Features::new(defs);
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(3);
        g.content = c;
        g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
        g.add_player(P0, "W");
        g.treasury_mut(P0).unwrap().mana = 1000;
        g.init_wizard(P0, &[("life", 2)], &[], None).unwrap();
        g
    }

    fn cast(g: &mut Game, id: &str, target: Target) -> Vec<Event> {
        let s = g.content.rules.spells.id_of(id).unwrap();
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(s);
        g.wizard_mut(P0)
            .unwrap()
            .get_mut::<crate::wizard::sheet::Skill>()
            .left = 500;
        g.treasury_mut(P0).unwrap().mana = 1000;
        g.apply(Command::CastSpell {
            player: P0,
            spell: s,
            target: Some(target),
        })
    }

    fn walker(g: &mut Game, pos: Position) -> hecs::Entity {
        let id = g.content.rules.units.id_of("walker").unwrap();
        g.spawn_unit(P0, pos, id)
    }

    fn rejected(events: &[Event]) -> bool {
        events.iter().any(|e| matches!(e, Event::Rejected(_)))
    }

    #[test]
    fn transmute_swaps_each_mineral_with_its_partner_and_refuses_the_rest() {
        let mut g = world();
        let pairs = [
            ("iron_ore", "gold_ore"),
            ("gold_ore", "iron_ore"),
            ("coal", "gem"),
            ("gem", "coal"),
            ("silver_ore", "mithril_ore"),
            ("mithril_ore", "silver_ore"),
        ];
        for (i, (from, to)) in pairs.into_iter().enumerate() {
            let pos = at(Plane::Arcanus, i as u32, 3);
            let id = g.content.features.id_of(from).unwrap();
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set_mineral(pos.x, pos.y, Some(id));
            let events = cast(&mut g, "transmute", Target::Tile(pos));
            assert!(!rejected(&events), "{from}: {events:?}");
            let now = g
                .map
                .as_ref()
                .unwrap()
                .plane(Plane::Arcanus)
                .mineral(pos.x as i32, pos.y as i32);
            assert_eq!(now, g.content.features.id_of(to), "{from}");
        }
        let adamantium = g.content.features.id_of("adamantium_ore").unwrap();
        let pos = at(Plane::Arcanus, 10, 3);
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set_mineral(pos.x, pos.y, Some(adamantium));
        assert!(rejected(&cast(&mut g, "transmute", Target::Tile(pos))));
        assert!(rejected(&cast(
            &mut g,
            "transmute",
            Target::Tile(at(Plane::Arcanus, 20, 9))
        )));
    }

    #[test]
    fn earth_lore_explores_a_block_around_the_target() {
        let mut g = world();
        g.fog = crate::map::fog::Fog::new(24, 14);
        let target = at(Plane::Arcanus, 10, 7);
        cast(&mut g, "earth_lore", Target::Tile(target));
        assert!(g.is_explored(P0, at(Plane::Arcanus, 4, 2)));
        assert!(g.is_explored(P0, at(Plane::Arcanus, 15, 11)));
        assert!(!g.is_explored(P0, at(Plane::Arcanus, 16, 7)));
        assert!(!g.is_explored(P0, at(Plane::Arcanus, 3, 7)));
        assert!(!g.is_explored(P0, at(Plane::Arcanus, 10, 12)));
        assert!(!g.is_explored(P0, at(Plane::Myrror, 10, 7)));
    }

    #[test]
    fn natures_cures_heals_the_stack_but_not_the_dead_or_undead() {
        let mut g = world();
        let pos = at(Plane::Arcanus, 4, 4);
        let a = walker(&mut g, pos);
        let b = walker(&mut g, pos);
        let elsewhere = walker(&mut g, at(Plane::Arcanus, 6, 6));
        g.make_undead(b);
        for u in [a, b, elsewhere] {
            g.world.insert_one(u, Hurt(1)).unwrap();
        }
        let events = cast(&mut g, "natures_cures", Target::Unit(a));
        assert!(!rejected(&events), "{events:?}");
        assert_eq!(g.world.get::<&Hurt>(a).unwrap().0, 0);
        assert_eq!(g.world.get::<&Hurt>(b).unwrap().0, 1, "undead stay hurt");
        assert_eq!(g.world.get::<&Hurt>(elsewhere).unwrap().0, 1);
    }

    #[test]
    fn word_of_recall_takes_a_unit_to_the_summoning_circle() {
        let mut g = world();
        let u = walker(&mut g, at(Plane::Arcanus, 4, 4));
        assert!(
            rejected(&cast(&mut g, "word_of_recall", Target::Unit(u))),
            "no circle yet"
        );
        let home = at(Plane::Myrror, 9, 9);
        g.wizard_mut(P0)
            .unwrap()
            .put(Some(crate::wizard::fortress::SummonAt(home)));
        cast(&mut g, "word_of_recall", Target::Unit(u));
        assert_eq!(g.units().unit_position(u), Some(home));
        for _ in 0..crate::unit::MAX_TILE_LIMIT - 1 {
            walker(&mut g, home);
        }
        let far = walker(&mut g, at(Plane::Arcanus, 1, 1));
        assert!(rejected(&cast(&mut g, "word_of_recall", Target::Unit(far))));
    }

    #[test]
    fn plane_shift_moves_the_whole_stack_unless_blocked() {
        let mut g = world();
        let here = at(Plane::Arcanus, 4, 4);
        let a = walker(&mut g, here);
        let b = walker(&mut g, here);
        let over = at(Plane::Myrror, 4, 4);
        let events = cast(&mut g, "plane_shift", Target::Unit(a));
        assert!(!rejected(&events), "{events:?}");
        assert_eq!(g.units().unit_position(a), Some(over));
        assert_eq!(g.units().unit_position(b), Some(over));
        // Back again is refused while the Planar Seal is up.
        let other = PlayerId(1);
        g.add_player(other, "X");
        g.init_wizard(other, &[("life", 2)], &[], None).unwrap();
        let seal = g.content.rules.spells.id_of("planar_seal").unwrap();
        g.attach_enchantment(Target::Player(other), seal, other);
        assert!(rejected(&cast(&mut g, "plane_shift", Target::Unit(a))));
    }

    #[test]
    fn floating_island_is_raised_on_the_aimed_water_square_and_refused_on_land() {
        let mut g = world();
        let ocean = g.content.terrains.id_of("ocean").unwrap();
        let sea = at(Plane::Arcanus, 5, 5);
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set(sea.x, sea.y, ocean);
        let mut units = g.content.rules.units.defs().to_vec();
        if !units.iter().any(|u| u.id == "floating_island") {
            units.push(crate::game::testutil::real_unit("floating_island"));
            g.content.rules.units = crate::rules::Registry::new_sorted(units);
        }
        let land = at(Plane::Arcanus, 6, 5);
        let events = cast(&mut g, "floating_island", Target::Tile(land));
        assert!(rejected(&events), "{events:?}");
        assert_eq!(
            g.check_floating_island(P0, land),
            Err(TargetError::NotWater),
            "land is refused"
        );
        assert!(g.units().units_at(land).is_empty());
        // An enemy boat on the square bars it; a square already holding nine units is full.
        let boat = g.content.rules.units.id_of("walker").unwrap();
        g.add_player(PlayerId(1), "X");
        let enemy = g.spawn_unit(PlayerId(1), sea, boat);
        assert_eq!(
            g.check_floating_island(P0, sea),
            Err(TargetError::EnemyOnSquare)
        );
        g.world.despawn(enemy).unwrap();
        for _ in 0..crate::unit::MAX_TILE_LIMIT {
            g.spawn_unit(P0, sea, boat);
        }
        assert_eq!(
            g.check_floating_island(P0, sea),
            Err(TargetError::SquareFull)
        );
        for u in g.units().units_at(sea) {
            g.world.despawn(u).unwrap();
        }
        let events = cast(&mut g, "floating_island", Target::Tile(sea));
        assert!(!rejected(&events), "{events:?}");
        let here = g.units().units_at(sea);
        assert_eq!(here.len(), 1, "the island rises where it was aimed");
        assert_eq!(
            g.units().unit_def(here[0]).map(|d| d.id.as_str()),
            Some("floating_island")
        );
    }
}
