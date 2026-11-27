//! The neutral player of a race plays as any AI does (task 522): the same `Ai` and the same turn,
//! without a wizard, so no spells, no research and no crafting.

use f4x_ai::{Ai, AiSeat, NeutralAi};
use f4x_core::player::PlayerId;
use f4x_core::world::{Command, Event};
use f4x_master::{Controller, Game, Master, Seat, Status, Until};
use f4x_newgame::setup::{new_game, NewGame};
use std::cell::RefCell;

/// The neutral players' computer play, shared with the test so that it can look at it between
/// rounds.
struct Shared<'a>(&'a RefCell<NeutralAi>);

impl Controller for Shared<'_> {
    fn play(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) -> Status {
        self.0.borrow_mut().play(game, player, events)
    }

    fn plans(&self) -> Vec<(PlayerId, String)> {
        self.0.borrow().plans()
    }
}

fn world(seed: u64) -> Game {
    Game::new(
        new_game(
            f4x_def::testdir::shipped(),
            &NewGame {
                seed,
                all_ai: true,
                ..NewGame::default()
            },
        )
        .expect("a new game"),
    )
}

/// Play `rounds` rounds with two wizard AIs and the shared neutral players' AI; the events of the
/// neutral players' commands come back.
fn play(w: &mut Game, neutral: &RefCell<NeutralAi>, rounds: u32) -> Vec<Event> {
    let mut ais = [Ai::new(PlayerId(0)), Ai::new(PlayerId(1))];
    let [a, b] = &mut ais[..] else { unreachable!() };
    let seats = vec![
        Seat::new(PlayerId(0), Box::new(AiSeat(a))),
        Seat::new(PlayerId(1), Box::new(AiSeat(b))),
        Seat::non_wizards(Box::new(Shared(neutral))),
    ];
    let events = Master::new(seats).run(w, Until::Rounds(rounds)).events;
    events
}

#[test]
fn the_neutral_players_play_with_one_ai_each_and_send_no_order_a_wizard_less_player_cannot() {
    let mut w = world(7);
    let neutral = RefCell::new(NeutralAi::default());
    neutral.borrow_mut().record_refusals(true);
    play(&mut w, &neutral, 60);
    let races: Vec<_> = w
        .player_ids()
        .into_iter()
        .filter(|&p| w.race_of_player(p).is_some())
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
    let mut w = world(7);
    let neutral = RefCell::new(NeutralAi::default());
    let events = play(&mut w, &neutral, 40);
    assert!(
        !events.iter().any(|e| matches!(
            e,
            Event::SpellCast { player, .. } if w.race_of_player(*player).is_some()
        )),
        "no neutral player casts"
    );
    for p in w.player_ids() {
        if w.race_of_player(p).is_some() {
            let mana = w.player(p).map_or(0, |p| p.treasury().mana.floor());
            let income = w.income(p).split.mana.floor();
            assert!(mana <= income + 1, "{p:?} keeps no mana: {mana}");
        }
    }
}

#[test]
fn the_ai_of_a_neutral_player_goes_with_the_player() {
    let mut w = world(7);
    let neutral = RefCell::new(NeutralAi::default());
    play(&mut w, &neutral, 2);
    let first = *neutral
        .borrow()
        .plans()
        .first()
        .map(|(p, _)| p)
        .expect("a neutral AI");
    // The player has nothing left: the next round removes it, and its AI with it.
    for u in w.units().player_units(first) {
        w.apply(Command::DismissUnit { unit: u });
    }
    let cities: Vec<_> = w
        .economy()
        .cities()
        .into_iter()
        .filter(|(_, c, _)| c.owner == first)
        .map(|(e, ..)| e)
        .collect();
    for c in cities {
        let _ = w.destroy_city(c);
    }
    play(&mut w, &neutral, 2);
    assert!(!w.has_player(first), "the empty player is gone");
    assert!(neutral.borrow().plans().iter().all(|(p, _)| *p != first));
}

#[test]
fn a_neutral_player_meets_answers_proposals_and_trades_no_spells() {
    use f4x_core::diplomacy::{DiplomacyCommand, ProposalKind};
    let mut w = world(7);
    let neutral = RefCell::new(NeutralAi::default());
    play(&mut w, &neutral, 2);
    let race = *neutral
        .borrow()
        .plans()
        .first()
        .map(|(p, _)| p)
        .expect("a neutral AI");
    assert!(w.takes_part_in_diplomacy(race));
    assert!(!w.takes_part_in_diplomacy(f4x_core::player::MONSTERS));
    w.make_contact(PlayerId(0), race);
    assert!(w.have_met(PlayerId(0), race), "wizards and neutrals meet");
    let events = w.apply(Command::Diplomacy(DiplomacyCommand::Propose {
        from: PlayerId(0),
        to: race,
        kind: ProposalKind::Pact,
    }));
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "a neutral player takes a proposal: {events:?}"
    );
    let answered = f4x_ai::answer_proposals(&mut w, race, &f4x_ai::Diplomats::new());
    assert!(
        !answered.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{answered:?}"
    );
    assert!(f4x_core::player::view::PlayerView::new(&w, race)
        .proposals_to_me()
        .is_empty());
    // No spells to trade: a swap is not valid with a neutral player.
    let swap = w.check_proposal(
        PlayerId(0),
        race,
        ProposalKind::Trade {
            give: f4x_core::diplomacy::Goods::spell(f4x_core::rules::SpellId::new(1)),
            get: f4x_core::diplomacy::Goods::spell(f4x_core::rules::SpellId::new(2)),
        },
    );
    assert!(swap.is_err(), "{swap:?}");
}
