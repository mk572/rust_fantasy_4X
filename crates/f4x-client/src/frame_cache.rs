//! What a frame asks of the game again and again and the game changes only now and then (task 693).
//!
//! Each value is kept with the revision of the game (`revised.rs`) and the player it was made for, and
//! made again only when either differs. The values are derived data: never saved. The cells are
//! `Cell`s so a screen that holds only `&App` can ask.

use std::cell::Cell;

use f4x_core::game::Game;
use f4x_core::player::finance::Budget;
use f4x_core::player::PlayerId;
use f4x_core::wizard::magic_income::Income;

/// A value made for a player at a revision of the game.
#[derive(Clone, Copy)]
struct Stamped<T: Copy> {
    revision: u64,
    player: PlayerId,
    value: T,
}

#[derive(Default)]
pub(crate) struct FrameCache {
    budget: Cell<Option<Stamped<Budget>>>,
    income: Cell<Option<Stamped<Income>>>,
    /// How often each value was made, for the tests.
    #[cfg(test)]
    pub(crate) made: Cell<(u32, u32)>,
}

impl FrameCache {
    /// The player's budget, as `Game::budget` says it for the revision.
    pub(crate) fn budget(&self, game: &super::revised::Revised<Game>, player: PlayerId) -> Budget {
        self.get(
            &self.budget,
            game,
            player,
            |g| g.budget(player),
            |m| m.0 += 1,
        )
    }

    /// The player's income, as `Game::income` says it for the revision.
    pub(crate) fn income(&self, game: &super::revised::Revised<Game>, player: PlayerId) -> Income {
        self.get(
            &self.income,
            game,
            player,
            |g| g.income(player),
            |m| m.1 += 1,
        )
    }

    fn get<T: Copy>(
        &self,
        cell: &Cell<Option<Stamped<T>>>,
        game: &super::revised::Revised<Game>,
        player: PlayerId,
        make: impl Fn(&Game) -> T,
        _count: impl Fn(&mut (u32, u32)),
    ) -> T {
        let revision = game.revision();
        if let Some(s) = cell.get() {
            if s.revision == revision && s.player == player {
                return s.value;
            }
        }
        let value = make(game);
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
    use f4x_core::game::Command;

    fn same_as_fresh(app: &App) {
        assert_eq!(app.cache.budget(&app.game, HUMAN), app.game.budget(HUMAN));
        assert_eq!(app.cache.income(&app.game, HUMAN), app.game.income(HUMAN));
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
            app.game.apply(Command::EndTurn);
            same_as_fresh(&app);
        }
        assert_ne!(app.cache.made.get(), made, "a change makes them again");

        // A direct call the client makes: a new unit costs upkeep.
        let (_, at) = app.game.economy().cities_of(HUMAN)[0];
        let num = {
            let units = app.game.units();
            let first = *units
                .player_units(HUMAN)
                .first()
                .expect("the human has a unit");
            units.unit_def(first).expect("a unit has its rules").num
        };
        let kind = app
            .game
            .content()
            .rules
            .units
            .id_numbered(num)
            .expect("the rules have it");
        let before = app.cache.budget(&app.game, HUMAN);
        app.game.spawn_unit(HUMAN, at, kind);
        same_as_fresh(&app);
        assert_ne!(
            app.cache.budget(&app.game, HUMAN),
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
