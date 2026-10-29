//! The notice that units were lost on the way out of a battle the human wizard fled: the green
//! `Notify2` strip over the map with the count of the lost units, closed by a click or a key.

use crate::input::Input;
use f4x_core::text::Args;

use crate::frame::Layout;
use crate::modal::{keep, Modal};
use crate::view::App;

/// The top of the box, as the other `Notify2` boxes.
const TOP: f32 = 60.0;

/// Units of the human wizard's army that did not get away.
pub(crate) struct FledNotice {
    units: u32,
}

impl Modal for FledNotice {
    fn kind(&self) -> &'static str {
        "fled_notice"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.fled_notice_frame(*self, input))
    }
}

impl App {
    /// Announce the units lost while fleeing.
    pub(crate) fn announce_fled_losses(&mut self, units: u32) {
        self.modals.enqueue(FledNotice { units });
    }

    /// The box until it is closed.
    fn fled_notice_frame(&mut self, notice: FledNotice, input: &Input) -> Option<FledNotice> {
        let lay = Layout::new();
        let text = self.text_with("battle.fled_losses", &Args::new().count("n", notice.units));
        let closed = self.notify2_box(
            &lay,
            TOP,
            crate::event_popup::GREEN,
            &text,
            |_, _, _| {},
            input,
        );
        (!closed).then_some(notice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modal::kind;
    use crate::view::HUMAN;
    use f4x_core::game::Event;
    use f4x_core::player::PlayerId;

    fn lost(player: PlayerId, units: u32) -> Vec<Event> {
        vec![Event::UnitsLostFleeing { player, units }]
    }

    #[test]
    fn the_human_who_loses_units_fleeing_gets_the_notice() {
        let mut app = App::for_test();
        app.note(lost(HUMAN, 2));
        assert!(app.modals.has("fled_notice"));
        assert!(app.modals.blocks_turn());
        assert!(!app.modals.has(kind::EVENT));
    }

    #[test]
    fn another_player_losing_units_fleeing_gets_none() {
        let mut app = App::for_test();
        app.note(lost(PlayerId(1), 2));
        assert!(app.modals.is_empty());
    }
}
