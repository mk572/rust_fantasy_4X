//! AI players: they look at the game the way a player does and answer with `Command`s sent
//! through `Game::apply`. Nothing here reaches into the game's internals.
//!
//! One [`Ai`] plays one wizard. [`Ai::play_turn`] does that wizard's whole turn (research,
//! spells, city production, settlers, the army) and leaves ending the turn to the caller,
//! because a turn ends for every wizard at once; [`play_round`] does that for a list of AIs.
//! The first version is deliberately simple: it expands, builds, defends and attacks cities it
//! has found, and is meant to be good enough to play whole games, not to play them well
//! (tasks 64 to 67 add personalities, spell use and real campaign planning).

mod diplomacy;
mod economy;
mod expand;
mod military;
mod view;

use std::collections::HashMap;

use f4x_core::city::{PlayerId, Position};
use f4x_core::serial::Serial;
use f4x_core::{Command, Event, Game};

/// One computer-played wizard.
#[derive(Debug, Clone)]
pub struct Ai {
    pub player: PlayerId,
    /// Where each settler is heading to found a city.
    settler_targets: HashMap<Serial, Position>,
    /// The enemy city the army is gathering to attack, once it has chosen one.
    campaign: Option<Position>,
    /// Where each settler stood last turn, to notice one that is stuck.
    settler_last: HashMap<Serial, Position>,
    /// Sites the AI gave up on.
    bad_sites: Vec<Position>,
}

impl Ai {
    pub fn new(player: PlayerId) -> Self {
        Self {
            player,
            settler_targets: HashMap::new(),
            campaign: None,
            settler_last: HashMap::new(),
            bad_sites: Vec::new(),
        }
    }

    /// The enemy city this AI is campaigning against, if any (for tools that watch it).
    pub fn campaign(&self) -> Option<Position> {
        self.campaign
    }

    /// Play this wizard's turn; returns the events the AI's own commands caused.
    pub fn play_turn(&mut self, game: &mut Game) -> Vec<Event> {
        let mut events = Vec::new();
        if game.game_over.is_some() || game.player(self.player).is_none_or(|p| p.defeated) {
            return events;
        }
        diplomacy::answer(self, game, &mut events);
        game.balance_farmers(self.player);
        economy::taxes(self, game, &mut events);
        economy::research(self, game, &mut events);
        economy::cast(self, game, &mut events);
        expand::settlers(self, game, &mut events);
        military::army(self, game, &mut events);
        economy::cities(self, game, &mut events);
        events
    }

    /// Send a command and keep its events.
    fn send(&self, game: &mut Game, command: Command, events: &mut Vec<Event>) -> bool {
        let out = game.apply(command);
        let ok = !out.iter().any(|e| matches!(e, Event::Rejected(_)));
        events.extend(out);
        ok
    }
}

/// The wizard answers the proposals made to it now (a human who has put a proposal waits for
/// the answer on the diplomacy screen instead of until the turn ends). Returns the events.
pub fn answer_proposals(game: &mut Game, player: PlayerId) -> Vec<Event> {
    let mut events = Vec::new();
    diplomacy::answer(&Ai::new(player), game, &mut events);
    events
}

/// Let every AI play its turn, then end the turn for everyone. Returns all the events.
pub fn play_round(game: &mut Game, ais: &mut [Ai]) -> Vec<Event> {
    let mut events = Vec::new();
    for ai in ais.iter_mut() {
        events.extend(ai.play_turn(game));
    }
    if game.game_over.is_none() {
        events.extend(game.apply(Command::EndTurn));
    }
    events
}
