//! The question before a spell is cast that would do nothing: the target has its effect already.
//! The game refuses such a cast (`MagicError::is_pointless`); the player may go on anyway, which
//! sends the same command wrapped in `Command::Anyway`. The AI never does.

use crate::frame::Layout;
use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::spellbook::Targeting;
use crate::view::App;
use f4x_core::game::{Command, Event, Rejection};
use f4x_def::text::Args;

/// The spell cast on a target that has its effect: the command waits for the answer.
pub(crate) struct CastAnyway {
    command: Command,
    spell_name: String,
    /// The targeting to go back to when the answer is No.
    again: Option<Targeting>,
}

impl Modal for CastAnyway {
    fn kind(&self) -> &'static str {
        kind::CAST_ANYWAY
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        let text = app.text_with(
            "spell.cast_anyway",
            &Args::new().text("spell", &self.spell_name),
        );
        let answer = app.confirmation_box(&Layout::new(), &text, input);
        match answer {
            Some(true) => {
                let events = app.game.apply(Command::Anyway(Box::new(self.command)));
                app.note(events);
                None
            }
            Some(false) => {
                app.targeting = self.again;
                None
            }
            None => keep(Some(*self)),
        }
    }
}

impl App {
    /// Whether the events refuse a cast only because it would do nothing.
    pub(crate) fn refused_as_pointless(events: &[Event]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, Event::Rejected(Rejection::Magic(why)) if why.is_pointless()))
    }

    /// Ask whether to cast anyway; `again` is the targeting to resume on No.
    pub(crate) fn ask_cast_anyway(
        &mut self,
        command: Command,
        spell: f4x_def::rules::SpellId,
        again: Option<Targeting>,
    ) {
        let spell_name = self
            .names
            .spell(self.game.content().rules.spells.get(spell).num)
            .to_string();
        self.modals.open(CastAnyway {
            command,
            spell_name,
            again,
        });
    }
}
