//! The spell instance: a spell cast that lasts, as an entity (`docs/ecs-structure.md` section 3.4).
//!
//! An instant spell does its work and leaves nothing behind. A spell that lasts (an enchantment,
//! a cast in progress) is an entity with [`SpellCast`], [`Caster`], [`SpellTarget`] and a
//! [`Serial`], made by [`Game::spawn_spell`]. It has no timer: a spell of the game is either instant
//! or lasts until it is dispelled or its caster loses it (`SpellDef::duration`). It goes when it
//! is dispelled or cancelled ([`Game::despawn_spell`]), when its target goes
//! ([`Game::despawn_with_spells`], used wherever a unit, city or feature is removed) and when its
//! caster is defeated ([`Game::despawn_spells_of`]). Queries come back oldest first, so that
//! anything the player or the AI can see happens in a fixed order.

use crate::game::serial::Serial;
use crate::game::Game;
use crate::player::PlayerId;
use crate::rules::SpellId;
use crate::wizard::entity::PlayerLink;

/// Which spell this is: its definition in `content/spells.ron`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellCast {
    pub def: SpellId,
}

/// The wizard entity that cast the spell and pays for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caster(pub hecs::Entity);

/// What the spell is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellTarget {
    /// The whole world (Evil Omens and the other global enchantments).
    Global,
    Wizard(hecs::Entity),
    City(hecs::Entity),
    Unit(hecs::Entity),
    Feature(hecs::Entity),
    /// The battle being fought; the instance is in the battle's own world.
    Battlefield,
}

impl SpellTarget {
    /// The entity the spell is on, for the targets that are entities.
    pub fn entity(self) -> Option<hecs::Entity> {
        match self {
            SpellTarget::Wizard(e)
            | SpellTarget::City(e)
            | SpellTarget::Unit(e)
            | SpellTarget::Feature(e) => Some(e),
            SpellTarget::Global | SpellTarget::Battlefield => None,
        }
    }
}

/// On a cast in progress: the mana still owed. A cast in progress is an instance of this kind,
/// targeted at its caster, until it is paid and resolved; it is no enchantment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owed(pub i32);

/// On an entity (a unit, city, wizard or feature) that has an instance on it. A quick look for
/// "nothing is on this one" that spares a query over the world, which is the most common
/// question asked (every stat of a unit asks it). Put on by [`spawn_instance`], taken off when the
/// last instance goes; one left behind by mistake only costs the query it was meant to spare.
#[derive(Debug, Clone, Copy)]
struct Enchanted;

/// On a wizard that keeps a spell going on the whole world: the same quick look for the globals.
#[derive(Debug, Clone, Copy)]
struct KeepsGlobal;

/// Does the entity carry the marker? A missing entity answers yes, so the query decides.
fn marked<M: hecs::Component>(world: &hecs::World, entity: hecs::Entity) -> bool {
    world.satisfies::<&M>(entity).unwrap_or(true)
}

/// A new instance in `world`, with the markers that go with it.
pub(crate) fn spawn_instance(
    world: &mut hecs::World,
    def: SpellId,
    caster: hecs::Entity,
    target: SpellTarget,
    serial: Serial,
) -> hecs::Entity {
    match target {
        SpellTarget::Global => {
            let _ = world.insert_one(caster, KeepsGlobal);
        }
        _ => {
            if let Some(e) = target.entity() {
                let _ = world.insert_one(e, Enchanted);
            }
        }
    }
    world.spawn((SpellCast { def }, Caster(caster), target, serial))
}

/// Remove one instance, and the markers it was the last reason for.
fn despawn_instance(world: &mut hecs::World, instance: hecs::Entity) {
    let target = world.get::<&SpellTarget>(instance).ok().map(|t| *t);
    let caster = world.get::<&Caster>(instance).ok().map(|c| c.0);
    let _ = world.despawn(instance);
    let spells = Spells::new(world);
    let (lone_target, lone_global) = match (target, caster) {
        (Some(SpellTarget::Global), Some(c)) => {
            (None, spells.globals_of(c).is_empty().then_some(c))
        }
        (Some(t), _) => (t.entity().filter(|_| spells.on(t).is_empty()), None),
        _ => (None, None),
    };
    if let Some(e) = lone_target {
        let _ = world.remove_one::<Enchanted>(e);
    }
    if let Some(c) = lone_global {
        let _ = world.remove_one::<KeepsGlobal>(c);
    }
}

/// A read handle on the spell instances of a world.
#[derive(Clone, Copy)]
pub struct Spells<'a> {
    world: &'a hecs::World,
}

impl<'a> Spells<'a> {
    pub(crate) fn new(world: &'a hecs::World) -> Self {
        Self { world }
    }

    /// Every spell instance, oldest first.
    pub fn all(&self) -> Vec<hecs::Entity> {
        self.matching(|_, _, _| true)
    }

    /// The instances on a target, oldest first.
    pub fn on(&self, target: SpellTarget) -> Vec<hecs::Entity> {
        if target
            .entity()
            .is_some_and(|e| !marked::<Enchanted>(self.world, e))
        {
            return Vec::new();
        }
        self.matching(|_, _, t| t == target)
    }

    /// The instances a wizard cast, oldest first.
    pub fn of_caster(&self, caster: hecs::Entity) -> Vec<hecs::Entity> {
        self.matching(|_, c, _| c == caster)
    }

    /// The enchantments on a city with the player who cast each, oldest first.
    pub fn city_enchantments(&self, city: hecs::Entity) -> Vec<(SpellId, PlayerId)> {
        self.enchantments_on(SpellTarget::City(city))
    }

    /// The enchantments on a unit with the player who cast each, oldest first.
    pub fn unit_enchantments(&self, unit: hecs::Entity) -> Vec<(SpellId, PlayerId)> {
        self.enchantments_on(SpellTarget::Unit(unit))
    }

    /// Is the spell on the unit, whoever cast it?
    pub fn unit_has(&self, unit: hecs::Entity, spell: SpellId) -> bool {
        self.has(SpellTarget::Unit(unit), spell)
    }

    /// Like [`Spells::city_enchantments`], with the creation number of each.
    pub fn city_instances(&self, city: hecs::Entity) -> Vec<(SpellId, PlayerId, Serial)> {
        self.instances_on(SpellTarget::City(city))
    }

    /// Like [`Spells::unit_enchantments`], with the creation number of each.
    pub fn unit_instances(&self, unit: hecs::Entity) -> Vec<(SpellId, PlayerId, Serial)> {
        self.instances_on(SpellTarget::Unit(unit))
    }

    pub fn enchantments_on(&self, target: SpellTarget) -> Vec<(SpellId, PlayerId)> {
        self.on(target)
            .into_iter()
            .filter_map(|i| Some((self.def(i)?, self.caster_player(i)?)))
            .collect()
    }

    fn instances_on(&self, target: SpellTarget) -> Vec<(SpellId, PlayerId, Serial)> {
        self.on(target)
            .into_iter()
            .filter_map(|i| {
                let serial = *self.world.get::<&Serial>(i).ok()?;
                Some((self.def(i)?, self.caster_player(i)?, serial))
            })
            .collect()
    }

    /// The player behind the wizard that cast an instance.
    pub fn caster_player(&self, instance: hecs::Entity) -> Option<PlayerId> {
        let wizard = self.caster(instance)?;
        let player = self.world.get::<&PlayerLink>(wizard).ok()?.0;
        crate::player::id_of(self.world, player)
    }

    /// The cast a wizard is paying for, if any.
    pub fn cast_of(&self, caster: hecs::Entity) -> Option<hecs::Entity> {
        self.world
            .query::<(&Owed, &Caster)>()
            .iter()
            .find(|(_, (_, c))| c.0 == caster)
            .map(|(e, _)| e)
    }

    /// The lasting spells a wizard keeps going on the whole world, oldest first.
    pub fn globals_of(&self, caster: hecs::Entity) -> Vec<hecs::Entity> {
        if !marked::<KeepsGlobal>(self.world, caster) {
            return Vec::new();
        }
        self.matching(|_, c, t| c == caster && t == SpellTarget::Global)
    }

    /// Does the wizard keep a global spell going for which `wanted` holds? (No list built, no sort.)
    pub fn any_global_of(&self, caster: hecs::Entity, wanted: impl Fn(SpellId) -> bool) -> bool {
        if !marked::<KeepsGlobal>(self.world, caster) {
            return false;
        }
        self.world
            .query::<(&SpellCast, &Caster, &SpellTarget)>()
            .iter()
            .any(|(_, (cast, c, t))| c.0 == caster && *t == SpellTarget::Global && wanted(cast.def))
    }

    /// The instances of one spell, oldest first.
    pub fn of_def(&self, def: SpellId) -> Vec<hecs::Entity> {
        self.matching(|d, _, _| d == def)
    }

    /// Is the spell on the target (whoever cast it)?
    pub fn has(&self, target: SpellTarget, def: SpellId) -> bool {
        if target
            .entity()
            .is_some_and(|e| !marked::<Enchanted>(self.world, e))
        {
            return false;
        }
        self.world
            .query::<(&SpellCast, &SpellTarget)>()
            .iter()
            .any(|(_, (cast, t))| cast.def == def && *t == target)
    }

    /// The spell an instance is of.
    pub fn def(&self, instance: hecs::Entity) -> Option<SpellId> {
        Some(self.world.get::<&SpellCast>(instance).ok()?.def)
    }

    /// The wizard that cast an instance.
    pub fn caster(&self, instance: hecs::Entity) -> Option<hecs::Entity> {
        Some(self.world.get::<&Caster>(instance).ok()?.0)
    }

    /// What an instance is on.
    pub fn target(&self, instance: hecs::Entity) -> Option<SpellTarget> {
        self.world.get::<&SpellTarget>(instance).ok().map(|t| *t)
    }

    fn matching(
        &self,
        keep: impl Fn(SpellId, hecs::Entity, SpellTarget) -> bool,
    ) -> Vec<hecs::Entity> {
        let mut found: Vec<(Serial, hecs::Entity)> = self
            .world
            .query::<(&SpellCast, &Caster, &SpellTarget, &Serial)>()
            .iter()
            .filter(|(_, (cast, caster, target, _))| keep(cast.def, caster.0, **target))
            .map(|(e, (.., serial))| (*serial, e))
            .collect();
        found.sort();
        found.into_iter().map(|(_, e)| e).collect()
    }
}

/// Remove from `world` the instances on `target` that `gone` picks, by spell and by the player who
/// cast it. For code that has the world and not the game.
pub(crate) fn remove_on(
    world: &mut hecs::World,
    target: SpellTarget,
    gone: impl Fn(SpellId, PlayerId) -> bool,
) {
    let spells = Spells::new(world);
    let doomed: Vec<hecs::Entity> = spells
        .on(target)
        .into_iter()
        .filter(|&i| {
            matches!((spells.def(i), spells.caster_player(i)), (Some(s), Some(c)) if gone(s, c))
        })
        .collect();
    for instance in doomed {
        despawn_instance(world, instance);
    }
}

impl Game {
    /// The spell instances of the game.
    pub fn spells(&self) -> Spells<'_> {
        Spells::new(&self.world)
    }

    /// Cast a lasting spell: a new instance on `target`, paid for by the wizard `caster`.
    pub(crate) fn spawn_spell(
        &mut self,
        def: SpellId,
        caster: hecs::Entity,
        target: SpellTarget,
    ) -> hecs::Entity {
        let serial = self.take_serial();
        spawn_instance(&mut self.world, def, caster, target, serial)
    }

    /// A lasting spell of `caster` is put on a city; a wizard has each spell on a city once.
    pub(crate) fn add_city_enchantment(
        &mut self,
        city: hecs::Entity,
        spell: SpellId,
        caster: PlayerId,
    ) {
        self.add_enchantment(SpellTarget::City(city), spell, caster);
    }

    /// `caster`'s `spell` on a city ends (turned off, dispelled, or not paid for).
    pub(crate) fn remove_city_enchantment(
        &mut self,
        city: hecs::Entity,
        spell: SpellId,
        caster: PlayerId,
    ) {
        self.remove_enchantment(SpellTarget::City(city), spell, caster);
    }

    /// A lasting spell of `caster` is put on a unit; a wizard has each spell on a unit once.
    pub(crate) fn add_unit_enchantment(
        &mut self,
        unit: hecs::Entity,
        spell: SpellId,
        caster: PlayerId,
    ) {
        self.add_enchantment(SpellTarget::Unit(unit), spell, caster);
    }

    /// `caster`'s `spell` on a unit ends (turned off, dispelled, or not paid for).
    pub(crate) fn remove_unit_enchantment(
        &mut self,
        unit: hecs::Entity,
        spell: SpellId,
        caster: PlayerId,
    ) {
        self.remove_enchantment(SpellTarget::Unit(unit), spell, caster);
    }

    pub(crate) fn add_enchantment(
        &mut self,
        target: SpellTarget,
        spell: SpellId,
        caster: PlayerId,
    ) {
        let Some(wizard) = self.wizard(caster).map(|w| w.entity()) else {
            return;
        };
        if self
            .spells()
            .enchantments_on(target)
            .contains(&(spell, caster))
        {
            return;
        }
        self.spawn_spell(spell, wizard, target);
    }

    pub(crate) fn remove_enchantment(
        &mut self,
        target: SpellTarget,
        spell: SpellId,
        caster: PlayerId,
    ) {
        remove_on(&mut self.world, target, |s, c| s == spell && c == caster);
    }

    /// A wizard turns off one of their own spells on a target. False if it is not there.
    pub(crate) fn turn_off_enchantment(
        &mut self,
        target: SpellTarget,
        player: PlayerId,
        spell: SpellId,
    ) -> bool {
        if !self
            .spells()
            .enchantments_on(target)
            .contains(&(spell, player))
        {
            return false;
        }
        self.remove_enchantment(target, spell, player);
        true
    }

    /// A city loses all its enchantments, for a test that starts again.
    #[cfg(test)]
    pub(crate) fn clear_city_enchantments(&mut self, city: hecs::Entity) {
        for instance in self.spells().on(SpellTarget::City(city)) {
            self.despawn_spell(instance);
        }
    }

    /// A wizard begins paying `owed` mana for `spell` over turns.
    pub(crate) fn begin_casting(&mut self, player: PlayerId, spell: SpellId, owed: i32) {
        let Some(wizard) = self.wizard(player).map(|w| w.entity()) else {
            return;
        };
        let cast = self.spawn_spell(spell, wizard, SpellTarget::Wizard(wizard));
        let _ = self.world.insert_one(cast, Owed(owed));
    }

    /// Remove one instance (dispelled, cancelled, or its effect is over).
    pub(crate) fn despawn_spell(&mut self, instance: hecs::Entity) {
        despawn_instance(&mut self.world, instance);
    }

    /// Remove an entity, and with it every spell instance that is on it. Every unit, city and
    /// feature goes through here, so that no instance has a target that is gone.
    pub(crate) fn despawn_with_spells(&mut self, entity: hecs::Entity) {
        self.despawn_spells_on(entity);
        let _ = self.world.despawn(entity);
    }

    /// Remove every spell instance that is on `entity` (a hero's body that died keeps the hero's
    /// entity, but not the spells that were on the body).
    pub(crate) fn despawn_spells_on(&mut self, entity: hecs::Entity) {
        for instance in self.spells().all() {
            if self.spells().target(instance).and_then(SpellTarget::entity) == Some(entity) {
                self.despawn_spell(instance);
            }
        }
    }

    /// Remove every instance a wizard cast (the wizard is defeated).
    pub(crate) fn despawn_spells_of(&mut self, caster: hecs::Entity) {
        for instance in self.spells().of_caster(caster) {
            self.despawn_spell(instance);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_magic;
    use crate::map::{Plane, Position};
    use crate::player::PlayerId;

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn game() -> Game {
        let mut g = Game::new(5);
        g.content = content_with_magic();
        g.add_player(P0, "A");
        g.add_player(P1, "B");
        g
    }

    fn wizard(g: &Game, id: PlayerId) -> hecs::Entity {
        g.wizard(id).expect("wizard succeeds").entity()
    }

    fn two_spells(g: &Game) -> (SpellId, SpellId) {
        let mut ids = g.content.rules.spells.iter().map(|(id, _)| id);
        (
            ids.next().expect("next succeeds"),
            ids.next().expect("next succeeds"),
        )
    }

    #[test]
    fn instances_are_found_by_target_caster_and_spell_oldest_first() {
        let mut g = game();
        let (a, b) = two_spells(&g);
        let (w0, w1) = (wizard(&g, P0), wizard(&g, P1));
        let first = g.spawn_spell(a, w1, SpellTarget::Global);
        let second = g.spawn_spell(b, w0, SpellTarget::Global);
        let third = g.spawn_spell(b, w1, SpellTarget::Wizard(w0));
        assert_eq!(g.spells().all(), vec![first, second, third]);
        assert_eq!(g.spells().on(SpellTarget::Global), vec![first, second]);
        assert_eq!(g.spells().of_caster(w1), vec![first, third]);
        assert_eq!(g.spells().of_def(b), vec![second, third]);
        assert!(g.spells().has(SpellTarget::Wizard(w0), b));
        assert!(!g.spells().has(SpellTarget::Wizard(w0), a));
    }

    #[test]
    fn the_order_follows_creation_not_the_entity_ids() {
        let mut g = game();
        let (a, _) = two_spells(&g);
        let w0 = wizard(&g, P0);
        let old = g.spawn_spell(a, w0, SpellTarget::Global);
        let gone = g.spawn_spell(a, w0, SpellTarget::Global);
        g.despawn_spell(gone);
        // The freed entity id is used again, by the newest instance.
        let newest = g.spawn_spell(a, w0, SpellTarget::Global);
        assert_eq!(g.spells().all(), vec![old, newest]);
    }

    #[test]
    fn a_spell_goes_with_its_target() {
        let mut g = game();
        let (a, _) = two_spells(&g);
        let w0 = wizard(&g, P0);
        let pos = Position {
            plane: Plane::Arcanus,
            x: 1,
            y: 1,
        };
        let city = g.spawn_city(P0, pos, "C".into(), None, 3);
        let other = g.spawn_city(P0, Position { x: 5, ..pos }, "D".into(), None, 3);
        let kept = g.spawn_spell(a, w0, SpellTarget::City(other));
        g.spawn_spell(a, w0, SpellTarget::City(city));
        g.spawn_spell(a, w0, SpellTarget::City(city));
        g.despawn_with_spells(city);
        assert!(!g.world.contains(city));
        assert_eq!(g.spells().all(), vec![kept]);
    }

    #[test]
    fn a_defeated_wizards_spells_all_go() {
        let mut g = game();
        let (a, b) = two_spells(&g);
        let (w0, w1) = (wizard(&g, P0), wizard(&g, P1));
        g.spawn_spell(a, w0, SpellTarget::Global);
        g.spawn_spell(b, w0, SpellTarget::Wizard(w1));
        let kept = g.spawn_spell(a, w1, SpellTarget::Global);
        g.despawn_spells_of(w0);
        assert_eq!(g.spells().all(), vec![kept]);
    }
}
