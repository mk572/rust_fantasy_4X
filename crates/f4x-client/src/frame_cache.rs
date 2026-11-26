//! What a frame asks of the game again and again and the game changes only now and then (task 693).
//!
//! Each value is kept with the revision of the game (`revised.rs`) and the player it was made for, and
//! made again only when either differs. The values are derived data: never saved. The cells are
//! `Cell`s so a screen that holds only `&App` can ask.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use f4x_core::map::Plane;
use f4x_core::player::finance::Budget;
use f4x_core::player::PlayerId;
use f4x_core::wizard::magic_income::Income;
use f4x_core::world::World;

/// A value made for a player at a revision of the game.
#[derive(Clone, Copy)]
struct Stamped<T> {
    revision: u64,
    player: PlayerId,
    value: T,
}

/// One plane's flags, shared by the readers of a frame.
type Layer = Rc<Vec<bool>>;

#[derive(Default)]
pub(crate) struct FrameCache {
    budget: Cell<Option<Stamped<Budget>>>,
    income: Cell<Option<Stamped<Income>>>,
    /// The squares the human sees, a layer for each plane (task 694).
    vision: RefCell<[Option<Stamped<Layer>>; 2]>,
    /// How often each value was made, for the tests.
    #[cfg(test)]
    pub(crate) made: Cell<(u32, u32)>,
    #[cfg(test)]
    pub(crate) vision_made: Cell<u32>,
}

impl FrameCache {
    /// The player's budget, as `World::budget` says it for the revision.
    pub(crate) fn budget(
        &self,
        world: &super::revised::Revised<World>,
        player: PlayerId,
    ) -> Budget {
        self.get(
            &self.budget,
            world,
            player,
            |w| w.budget(player),
            |m| m.0 += 1,
        )
    }

    /// The player's income, as `World::income` says it for the revision.
    pub(crate) fn income(
        &self,
        world: &super::revised::Revised<World>,
        player: PlayerId,
    ) -> Income {
        self.get(
            &self.income,
            world,
            player,
            |w| w.income(player),
            |m| m.1 += 1,
        )
    }

    /// The squares `player` sees on `plane`, as `Economy::visibility` says them for the revision: row
    /// major, `width * height` flags, shared by every reader of the frame.
    pub(crate) fn visibility(
        &self,
        world: &super::revised::Revised<World>,
        player: PlayerId,
        plane: Plane,
    ) -> Layer {
        let layer = plane as usize;
        let revision = world.revision();
        let mut cache = self.vision.borrow_mut();
        if let Some(s) = &cache[layer] {
            if s.revision == revision && s.player == player {
                return Rc::clone(&s.value);
            }
        }
        let value = Rc::new(world.economy().visibility(player, plane));
        cache[layer] = Some(Stamped {
            revision,
            player,
            value: Rc::clone(&value),
        });
        #[cfg(test)]
        self.vision_made.set(self.vision_made.get() + 1);
        value
    }

    fn get<T: Copy>(
        &self,
        cell: &Cell<Option<Stamped<T>>>,
        world: &super::revised::Revised<World>,
        player: PlayerId,
        make: impl Fn(&World) -> T,
        _count: impl Fn(&mut (u32, u32)),
    ) -> T {
        let revision = world.revision();
        if let Some(s) = cell.get() {
            if s.revision == revision && s.player == player {
                return s.value;
            }
        }
        let value = make(world);
        cell.set(Some(Stamped {
            revision,
            player,
            value,
        }));
        #[cfg(test)]
        {
            let mut m = self.made.get();
            _count(&mut m);
            self.made.set(m);
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use crate::view::{App, HUMAN};
    use f4x_core::world::Command;

    fn same_as_fresh(app: &App) {
        use f4x_core::map::Plane;
        for plane in [Plane::Arcanus, Plane::Myrror] {
            assert_eq!(
                *app.cache.visibility(&app.world, HUMAN, plane),
                app.world.economy().visibility(HUMAN, plane)
            );
        }
        assert_eq!(app.cache.budget(&app.world, HUMAN), app.world.budget(HUMAN));
        assert_eq!(app.cache.income(&app.world, HUMAN), app.world.income(HUMAN));
    }

    #[test]
    fn a_cached_budget_and_income_follow_commands_direct_calls_and_a_load() {
        let mut app = App::for_test();
        same_as_fresh(&app);
        let made = app.cache.made.get();
        for _ in 0..5 {
            same_as_fresh(&app);
        }
        assert_eq!(app.cache.made.get(), made, "unchanged game: not made again");

        // A command that changes the figures: the turn change pays and eats.
        for _ in 0..3 {
            app.world.apply(Command::EndTurn);
            same_as_fresh(&app);
        }
        assert_ne!(app.cache.made.get(), made, "a change makes them again");
        let seen = app.cache.vision_made.get();
        app.cache
            .visibility(&app.world, HUMAN, f4x_core::map::Plane::Arcanus);
        assert_eq!(
            app.cache.vision_made.get(),
            seen,
            "asked again: not made again"
        );

        // A direct call the client makes: a new unit costs upkeep.
        let (_, at) = app.world.economy().cities_of(HUMAN)[0];
        let num = {
            let units = app.world.units();
            let first = *units
                .player_units(HUMAN)
                .first()
                .expect("the human has a unit");
            units.unit_def(first).expect("a unit has its rules").num
        };
        let kind = app
            .world
            .content()
            .rules
            .units
            .id_numbered(num)
            .expect("the rules have it");
        let before = app.cache.budget(&app.world, HUMAN);
        app.world.spawn_unit(HUMAN, at, kind);
        same_as_fresh(&app);
        assert_ne!(
            app.cache.budget(&app.world, HUMAN),
            before,
            "the new unit's upkeep"
        );

        // A load.
        let path = std::env::temp_dir().join(format!("f4x-cache-{}.save", std::process::id()));
        app.save_to(&path);
        app.load_from(&path).expect("the save loads");
        let _ = std::fs::remove_file(&path);
        same_as_fresh(&app);
    }
}
