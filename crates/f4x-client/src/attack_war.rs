//! The notice before a human wizard attacks the unit or city of another wizard or of an independent
//! race: the attack is a declaration of war on that player and ends any pact or alliance with them. The game refuses an attack on a partner and never
//! breaks the treaty itself (`docs/diplomacy.md` 9.2), so on Yes this sends `DeclareWar` first and
//! then the move.

use crate::frame::Layout;
use crate::input::Input;
use crate::modal::{keep, kind, Modal};
use crate::text::Args;
use crate::view::{App, HUMAN};
use f4x_core::diplomacy::{DiplomacyCommand, Treaty};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::world::Command;

/// What the player is asked, and what is sent on Yes.
pub(crate) struct AttackWar {
    on: PlayerId,
    then: Command,
}

impl Modal for AttackWar {
    fn kind(&self) -> &'static str {
        kind::ATTACK_WAR
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        let text = app.text_with(
            "diplomacy.attack_is_war",
            &Args::new().text("wizard", app.diplo_name(self.on)),
        );
        match app.confirmation_box(&Layout::new(), &text, input) {
            Some(true) => {
                let mut events = app.game.apply(
                    HUMAN,
                    Command::Diplomacy(DiplomacyCommand::DeclareWar {
                        player: HUMAN,
                        on: self.on,
                    }),
                );
                events.extend(app.game.apply(HUMAN, self.then));
                app.note(events);
                None
            }
            Some(false) => None,
            None => keep(Some(*self)),
        }
    }
}

impl App {
    /// The player whose unit or city is on `to` when attacking it would be a new war: another wizard or
    /// an independent race (they use the full diplomacy rules) the human is not at war with yet. The
    /// monsters take no part in diplomacy and are attacked freely.
    pub(crate) fn war_to_declare(&self, to: Position) -> Option<PlayerId> {
        let defenders = self.game.economy().hostile_at(to, HUMAN)?;
        let owner = defenders
            .first()
            .and_then(|&u| self.game.units().unit_owner(u))
            .or_else(|| {
                let city = self.game.economy().city_at(to)?;
                self.game.economy().city_owner(city)
            })?;
        (self.game.takes_part_in_diplomacy(owner) && self.game.treaty(HUMAN, owner) != Treaty::War)
            .then_some(owner)
    }

    /// Send `command`, an order that ends on `to`; if that is an attack on a wizard the human is not at
    /// war with, ask first.
    pub(crate) fn send_attack_checked(&mut self, command: Command, to: Position) {
        match self.war_to_declare(to) {
            Some(on) => self.modals.open(AttackWar { on, then: command }),
            None => {
                let events = self.game.apply(HUMAN, command);
                self.note(events);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::TestResult;

    #[test]
    fn a_war_is_to_be_declared_only_on_a_wizard_the_human_is_not_at_war_with() -> TestResult {
        let mut app = App::for_test();
        let rival = PlayerId(1);
        let unit = app.game.units().player_units(HUMAN)[0];
        let kind = app.game.units().unit_kind_id(unit).ok_or("a kind")?;
        let at = app.game.units().unit_position(unit).ok_or("a position")?;
        // The rival's unit on the human's own square is not on a free square, so look for one beside.
        let (w, h) = {
            let map = app.game.map().ok_or("a map")?;
            (map.width() as i32, map.height() as i32)
        };
        let squares: Vec<Position> = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
            .filter(|&d| d != (0, 0))
            .filter_map(|(dx, dy)| {
                let y = at.y as i32 + dy;
                (0..h).contains(&y).then_some(Position {
                    plane: at.plane,
                    x: (at.x as i32 + dx).rem_euclid(w) as u32,
                    y: y as u32,
                })
            })
            .collect();
        let to = squares
            .into_iter()
            .find(|&to| {
                app.game
                    .command(Command::CreateUnit {
                        owner: rival,
                        pos: to,
                        unit: kind,
                    })
                    .iter()
                    .all(|e| !matches!(e, f4x_core::world::Event::Rejected(_)))
            })
            .ok_or("no square for a rival's unit")?;
        assert_eq!(app.war_to_declare(to), Some(rival));
        app.game.declare_war(HUMAN, rival);
        assert_eq!(app.war_to_declare(to), None, "already at war");
        assert_eq!(
            app.war_to_declare(at),
            None,
            "nothing hostile on one's own square"
        );
        Ok(())
    }
}
