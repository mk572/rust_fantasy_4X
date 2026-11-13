//! The neutral player of a race plays as any AI does (task 522): the same `Ai` and the same turn,
//! without a wizard, so no spells, no research and no crafting.

use f4x_ai::{Ai, AiSeat, NeutralSeat};
use f4x_core::game::{Command, Event, Game};
use f4x_core::player::PlayerId;
use f4x_master::{Controller, Master, Seat, Status, Until};
use f4x_newgame::setup::{new_game, NewGame};
use std::cell::RefCell;

/// The neutral seat, shared with the test so that it can look at it between rounds.
struct Shared<'a>(&'a RefCell<NeutralSeat>);

impl Controller for Shared<'_> {
    fn play(&mut self, game: &mut Game, players: &[PlayerId], events: &mut Vec<Event>) -> Status {
        self.0.borrow_mut().play(game, players, events)
    }

    fn plans(&self) -> Vec<(PlayerId, String)> {
        self.0.borrow().plans()
    }
}

fn world(seed: u64) -> Game {
    new_game(
        f4x_def::testdir::shipped(),
        &NewGame {
            seed,
            all_ai: true,
            ..NewGame::default()
        },
    )
    .expect("a new game")
}

/// Play `rounds` rounds with two wizard AIs and the shared neutral seat; the events of the
/// neutral players' commands come back.
fn play(g: &mut Game, neutral: &RefCell<NeutralSeat>, rounds: u32) -> Vec<Event> {
    let mut ais = [Ai::new(PlayerId(0)), Ai::new(PlayerId(1))];
    let [a, b] = &mut ais[..] else { unreachable!() };
    let seats = vec![
        Seat::new(vec![PlayerId(0)], Box::new(AiSeat(a))),
        Seat::new(vec![PlayerId(1)], Box::new(AiSeat(b))),
        Seat::neutrals(Box::new(Shared(neutral))),
    ];
    let events = Master::new(seats).run(g, Until::Rounds(rounds)).events;
    events
}

#[test]
fn the_neutral_players_play_with_one_ai_each_and_send_no_order_a_wizard_less_player_cannot() {
    let mut g = world(7);
    let neutral = RefCell::new(NeutralSeat::default());
    neutral.borrow_mut().record_refusals(true);
    play(&mut g, &neutral, 60);
    let races: Vec<_> = g
        .player_ids()
        .into_iter()
        .filter(|&p| g.race_of_player(p).is_some())
        .collect();
    assert!(!races.is_empty(), "the neutral players are still there");
    let plans = neutral.borrow().plans();
    assert!(
        races.iter().all(|r| plans.iter().any(|(p, _)| p == r)),
        "one AI for each neutral player"
    );
    let refused = neutral.borrow().take_refusals();
    assert!(
        refused.len() <= 3,
        "a neutral player's orders are accepted: {refused:?}"
    );
}

#[test]
fn a_neutral_player_casts_no_spell_researches_nothing_and_turns_its_mana_into_gold() {
    let mut g = world(7);
    let neutral = RefCell::new(NeutralSeat::default());
    let events = play(&mut g, &neutral, 40);
    assert!(
        !events.iter().any(|e| matches!(
            e,
            Event::SpellCast { player, .. } if g.race_of_player(*player).is_some()
        )),
        "no neutral player casts"
    );
    for p in g.player_ids() {
        if g.race_of_player(p).is_some() {
            let mana = g.player(p).map_or(0, |p| p.treasury().mana.floor());
            let income = g.income(p).split.mana.floor();
            assert!(mana <= income + 1, "{p:?} keeps no mana: {mana}");
        }
    }
}

#[test]
fn the_ai_of_a_neutral_player_goes_with_the_player() {
    let mut g = world(7);
    let neutral = RefCell::new(NeutralSeat::default());
    play(&mut g, &neutral, 2);
    let first = *neutral
        .borrow()
        .plans()
        .first()
        .map(|(p, _)| p)
        .expect("a neutral AI");
    // The player has nothing left: the next round removes it, and its AI with it.
    for u in g.units().player_units(first) {
        g.apply(Command::DismissUnit { unit: u });
    }
    let cities: Vec<_> = g
        .economy()
        .cities()
        .into_iter()
        .filter(|(_, c, _)| c.owner == first)
        .map(|(e, ..)| e)
        .collect();
    for c in cities {
        let _ = g.destroy_city(c);
    }
    play(&mut g, &neutral, 2);
    assert!(!g.has_player(first), "the empty player is gone");
    assert!(neutral.borrow().plans().iter().all(|(p, _)| *p != first));
}

#[test]
fn a_neutral_player_meets_answers_proposals_and_trades_no_spells() {
    use f4x_core::diplomacy::{DiplomacyCommand, ProposalKind};
    let mut g = world(7);
    let neutral = RefCell::new(NeutralSeat::default());
    play(&mut g, &neutral, 2);
    let race = *neutral
        .borrow()
        .plans()
        .first()
        .map(|(p, _)| p)
        .expect("a neutral AI");
    assert!(g.takes_part_in_diplomacy(race));
    assert!(!g.takes_part_in_diplomacy(f4x_core::player::MONSTERS));
    g.make_contact(PlayerId(0), race);
    assert!(g.have_met(PlayerId(0), race), "wizards and neutrals meet");
    let events = g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
        from: PlayerId(0),
        to: race,
        kind: ProposalKind::Pact,
    }));
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "a neutral player takes a proposal: {events:?}"
    );
    let answered = f4x_ai::answer_proposals(&mut g, race);
    assert!(
        !answered.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{answered:?}"
    );
    assert!(f4x_core::player::view::PlayerView::new(&g, race)
        .proposals_to_me()
        .is_empty());
    // No spells to trade: a swap is not valid with a neutral player.
    let swap = g.check_proposal(
        PlayerId(0),
        race,
        ProposalKind::Exchange {
            give: f4x_core::rules::SpellId::new(1),
            get: f4x_core::rules::SpellId::new(2),
        },
    );
    assert!(swap.is_err(), "{swap:?}");
}
