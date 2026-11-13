//! What a unit's stats are made of, kept for the length of a computer wizard's turn.
//!
//! A stat asks for every modifier of the unit (abilities, level, hero, spells, items, temporary
//! effects, global spells), and the AI asks for several stats of the same units again and again.
//! None of it changes between two commands the wizard sends, so the modifiers and the abilities of
//! a unit are worked out once and shared until a command that may change them.
//!
//! The cache is armed by [`crate::game::Game::keep_for_turn`] for the world of that game only and
//! is dropped after the turn; a `UnitView` over any other world (a battle, a copy) never sees it.
//! It is emptied by [`crate::game::Game`]'s `execute` after every command except the ones named
//! harmless in `map::vision::VisionEffect` (a walk changes where a unit stands, and nothing a
//! unit's modifiers or abilities are made of depends on that). `F4X_CHECK_UNITS=1` works every hit
//! out again and panics when the kept one differs.

use crate::fast::{FastMap, FastSet};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::Ability;
use crate::unit::pathing::grid::{CachedGrid, GridKey};
use f4x_def::rules::stats::Modifier;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct Kept {
    /// The address of the world the cache is for.
    world: usize,
    modifiers: FastMap<hecs::Entity, Rc<Vec<Modifier>>>,
    abilities: FastMap<hecs::Entity, Rc<Vec<Ability>>>,
    /// The player whose turn it is, and the squares where something hostile to it stands
    /// (`Economy::hostile_squares`): its own commands are the only changes, and walking its own
    /// units does not change them.
    player: Option<PlayerId>,
    hostile: Option<Rc<FastSet<Position>>>,
    /// The grids of stacks (`Game::stack_grid`): roads, terrain, cities and the ships' room are
    /// what they are made of, and none of them changes by a walk except the room, which each
    /// entry remembers.
    grids: FastMap<GridKey, Rc<CachedGrid>>,
}

thread_local! {
    static KEPT: RefCell<Option<Kept>> = const { RefCell::new(None) };
}

fn address(world: &hecs::World) -> usize {
    world as *const hecs::World as usize
}

/// Start keeping for this world.
pub(crate) fn arm(world: &hecs::World, player: PlayerId) {
    KEPT.with(|k| {
        *k.borrow_mut() = Some(Kept {
            world: address(world),
            player: Some(player),
            ..Kept::default()
        })
    });
}

/// Stop keeping.
pub(crate) fn disarm() {
    KEPT.with(|k| *k.borrow_mut() = None);
}

/// Forget what is kept (the world may have changed), and go on keeping.
pub(crate) fn clear() {
    KEPT.with(|k| {
        if let Some(kept) = k.borrow_mut().as_mut() {
            kept.modifiers.clear();
            kept.abilities.clear();
            kept.hostile = None;
            kept.grids.clear();
        }
    });
}

pub(crate) fn modifiers(world: &hecs::World, unit: hecs::Entity) -> Option<Rc<Vec<Modifier>>> {
    KEPT.with(|k| {
        let k = k.borrow();
        let kept = k.as_ref().filter(|kept| kept.world == address(world))?;
        kept.modifiers.get(&unit).cloned()
    })
}

pub(crate) fn keep_modifiers(world: &hecs::World, unit: hecs::Entity, mods: &Rc<Vec<Modifier>>) {
    KEPT.with(|k| {
        if let Some(kept) = k
            .borrow_mut()
            .as_mut()
            .filter(|kept| kept.world == address(world))
        {
            kept.modifiers.insert(unit, mods.clone());
        }
    });
}

pub(crate) fn abilities(world: &hecs::World, unit: hecs::Entity) -> Option<Rc<Vec<Ability>>> {
    KEPT.with(|k| {
        let k = k.borrow();
        let kept = k.as_ref().filter(|kept| kept.world == address(world))?;
        kept.abilities.get(&unit).cloned()
    })
}

pub(crate) fn keep_abilities(world: &hecs::World, unit: hecs::Entity, list: &Rc<Vec<Ability>>) {
    KEPT.with(|k| {
        if let Some(kept) = k
            .borrow_mut()
            .as_mut()
            .filter(|kept| kept.world == address(world))
        {
            kept.abilities.insert(unit, list.clone());
        }
    });
}

/// Whether the turn of `player` is the one being kept.
pub(crate) fn keeping(world: &hecs::World, player: PlayerId) -> bool {
    KEPT.with(|k| {
        k.borrow()
            .as_ref()
            .is_some_and(|kept| kept.world == address(world) && kept.player == Some(player))
    })
}

/// The kept hostile squares of `player`, if it is the player whose turn is kept.
pub(crate) fn hostile(world: &hecs::World, player: PlayerId) -> Option<Rc<FastSet<Position>>> {
    KEPT.with(|k| {
        let k = k.borrow();
        let kept = k.as_ref().filter(|kept| kept.world == address(world))?;
        if kept.player != Some(player) {
            return None;
        }
        kept.hostile.clone()
    })
}

pub(crate) fn keep_hostile(world: &hecs::World, player: PlayerId, set: &Rc<FastSet<Position>>) {
    KEPT.with(|k| {
        if let Some(kept) = k
            .borrow_mut()
            .as_mut()
            .filter(|kept| kept.world == address(world) && kept.player == Some(player))
        {
            kept.hostile = Some(set.clone());
        }
    });
}

pub(crate) fn grid(world: &hecs::World, key: &GridKey) -> Option<Rc<CachedGrid>> {
    KEPT.with(|k| {
        let k = k.borrow();
        let kept = k.as_ref().filter(|kept| kept.world == address(world))?;
        kept.grids.get(key).cloned()
    })
}

pub(crate) fn keep_grid(world: &hecs::World, key: GridKey, grid: &Rc<CachedGrid>) {
    KEPT.with(|k| {
        if let Some(kept) = k
            .borrow_mut()
            .as_mut()
            .filter(|kept| kept.world == address(world))
        {
            kept.grids.insert(key, grid.clone());
        }
    });
}

/// Set `F4X_CHECK_UNITS` to work every kept answer out again and compare (slow).
pub(crate) fn checking() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("F4X_CHECK_UNITS").is_some())
}
