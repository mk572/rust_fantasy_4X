//! A city of the human's that has finished its production has nothing in hand (design.md,
//! "A finished item ends the production"). At the start of the turn its screen opens, one city
//! after the other, with the message "<city> has finished <item>" over it, and the player
//! chooses the next thing to build there. (The original shows the building's box over the map
//! first and opens the city screen after it; this game shows the message over the city screen.)
//!
//! A city that has an item again (the auto-builder picked one) does not open: a finished
//! building is announced over the map, a finished unit only in the log.

use crate::built_popup::FinishedNotice;
use crate::preferences::Pref;
use crate::view::{App, Screen, HUMAN};
use f4x_core::city::production::ProductionItem;

impl App {
    /// Remember a finished item of a city of the human's, to be announced at the start of the turn.
    pub(crate) fn note_finished(&mut self, city: hecs::Entity, item: ProductionItem) {
        if self.world.economy().city_owner(city) == Some(HUMAN) {
            self.finished_cities.push_back((city, item));
        }
    }

    /// Announce the next finished item and, for a city with nothing in hand, open its screen
    /// with the message over it. A city that has an item again (the auto-builder picked one)
    /// only gets the message of a building over the map, as before. True when it did something.
    pub(crate) fn finished_city_frame(&mut self) -> bool {
        if self.finished_cities.is_empty()
            || self.world.in_battle()
            || !matches!(self.screen, Screen::Map)
            || self.turn_events.is_some()
            || !self.modals.is_empty()
            || !self.world.offers(HUMAN).is_empty()
        {
            return false;
        }
        if !self.prefs.get(Pref::OpenFinishedCities) {
            self.finished_cities.clear();
            return false;
        }
        while let Some((city, item)) = self.finished_cities.pop_front() {
            let economy = self.world.economy();
            if economy.city_owner(city) != Some(HUMAN) {
                continue;
            }
            if economy.city_production(city).item.is_none() {
                self.open_city_screen(city);
                match item {
                    ProductionItem::Building(b) => self.announce_built(city, b),
                    _ => self.modals.enqueue(FinishedNotice { city, item }),
                }
                return true;
            }
            if let ProductionItem::Building(b) = item {
                self.announce_built(city, b);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::view::{App, Screen, HUMAN};
    use f4x_core::city::production::ProductionItem;
    use f4x_core::player::PlayerId;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn city_of(app: &App, player: PlayerId) -> Result<hecs::Entity, &'static str> {
        app.world
            .economy()
            .cities_of(player)
            .first()
            .map(|(e, _)| *e)
            .ok_or("the wizard has no city")
    }

    #[test]
    fn a_city_that_finished_opens_its_screen_on_the_map() -> TestResult {
        let mut app = App::for_test();
        let city = city_of(&app, HUMAN)?;
        app.world.scenario_clear_production(city);
        app.note_finished(city, ProductionItem::Housing);
        assert!(app.finished_city_frame());
        assert!(matches!(app.screen, Screen::City(c) if c == city));
        assert!(
            app.modals.has(crate::modal::kind::BUILT),
            "the message is over the city screen"
        );
        assert!(app.finished_cities.is_empty(), "the queue is empty now");
        Ok(())
    }

    #[test]
    fn nothing_opens_for_a_city_of_another_player_or_one_that_has_an_item_again() -> TestResult {
        let mut app = App::for_test();
        let rival = (0..4)
            .map(PlayerId)
            .filter(|&p| p != HUMAN)
            .find_map(|p| city_of(&app, p).ok())
            .ok_or("no rival city")?;
        app.world.scenario_clear_production(rival);
        app.note_finished(rival, ProductionItem::Housing);
        assert!(
            app.finished_cities.is_empty(),
            "a rival's city is not noted"
        );
        let own = city_of(&app, HUMAN)?;
        app.world
            .set_production(own, ProductionItem::TradeGoods)
            .map_err(|e| format!("{e:?}"))?;
        app.note_finished(own, ProductionItem::Housing);
        assert!(!app.finished_city_frame(), "it has an item again");
        assert!(matches!(app.screen, Screen::Map));
        Ok(())
    }

    #[test]
    fn with_the_switch_off_no_city_opens_and_the_queue_is_dropped() -> TestResult {
        let mut app = App::for_test();
        let city = city_of(&app, HUMAN)?;
        app.world.scenario_clear_production(city);
        app.note_finished(city, ProductionItem::Housing);
        app.prefs
            .set(crate::preferences::Pref::OpenFinishedCities, false);
        assert!(!app.finished_city_frame());
        assert!(matches!(app.screen, Screen::Map));
        assert!(app.finished_cities.is_empty());
        Ok(())
    }

    #[test]
    fn the_screen_waits_while_the_turn_changes() -> TestResult {
        let mut app = App::for_test();
        let city = city_of(&app, HUMAN)?;
        app.world.scenario_clear_production(city);
        app.note_finished(city, ProductionItem::Housing);
        app.turn_events = Some(Vec::new());
        assert!(!app.finished_city_frame());
        app.turn_events = None;
        assert!(app.finished_city_frame());
        Ok(())
    }
}
