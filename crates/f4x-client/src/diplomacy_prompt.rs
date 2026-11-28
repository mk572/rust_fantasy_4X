//! What the rival wizards propose to the human at the end of their turns (`f4x-ai`): a pact, an
//! alliance, peace, a war on a third wizard, a trade, a demand. Each waits in a yes-or-no box over
//! the map until it is answered, where a treaty or a trade may also be countered; one that is left
//! lapses when the turn ends. An ally's call to arms asks to join or to end the alliance, and the
//! human may first name a price; an ally's price for joining the human's war is paid or not. And the
//! box at the start of the human's turn that asks what to do about a partner whose units are near the
//! cities after the last warning.

use crate::input::Input;
use crate::text::Args;
use f4x_core::diplomacy::{Deal, DiplomacyCommand, Proposal, ProposalKind};
use f4x_core::player::view::PlayerView;
use f4x_core::world::{Command, Event};
use macroquad::prelude::*;

use crate::action_button_art::Slab;
use crate::modal::{keep, kind, Modal};
use crate::sell_prompt::{Pick, Third, BREAK_OR_OVERLOOK, JOIN_OR_BREAK, YES_NO, YES_OR_NO_ONLY};
use crate::view::{App, Screen, HUMAN};

/// The box that asks about the first proposal waiting in the game; the proposals themselves
/// are the game's state, so the box holds nothing.
pub(crate) struct ProposalAsk;

impl Modal for ProposalAsk {
    fn kind(&self) -> &'static str {
        kind::PROPOSAL
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.proposal_frame(input).then_some(ProposalAsk))
    }
}

/// The box that asks what to do about the first partner left to choose about.
pub(crate) struct TrespassAsk;

impl Modal for TrespassAsk {
    fn kind(&self) -> &'static str {
        kind::TRESPASS
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.trespass_frame(input).then_some(TrespassAsk))
    }
}

/// The key of the question a proposal asks.
pub(crate) fn question_key(kind: ProposalKind) -> &'static str {
    match kind {
        ProposalKind::Pact => "diplomacy.ai_proposes_pact",
        ProposalKind::Alliance => "diplomacy.ai_proposes_alliance",
        ProposalKind::Peace => "diplomacy.ai_proposes_peace",
        ProposalKind::WarOn(_) => "diplomacy.ai_proposes_war",
        ProposalKind::BreakAllianceWith(_) => "diplomacy.ai_proposes_break",
        ProposalKind::Trade { .. } => "diplomacy.ai_proposes_trade",
        ProposalKind::Demand { .. } => "diplomacy.ai_demands",
        // A threat is only ever made by the human.
        ProposalKind::Threat => "diplomacy.refused_threat",
        ProposalKind::WarPrice { .. } => "diplomacy.ai_names_war_price",
        ProposalKind::MapExchange => "diplomacy.ai_proposes_maps",
        ProposalKind::TradePact => "diplomacy.ai_proposes_trade_pact",
        ProposalKind::ResearchPact => "diplomacy.ai_proposes_research_pact",
        ProposalKind::Priced { deal, .. } => match deal {
            Deal::Pact => "diplomacy.ai_proposes_priced_pact",
            Deal::Alliance => "diplomacy.ai_proposes_priced_alliance",
            Deal::Peace => "diplomacy.ai_proposes_priced_peace",
        },
    }
}

impl App {
    /// The first proposal a rival has made to the human that waits for an answer.
    fn pending_proposal(&self) -> Option<Proposal> {
        PlayerView::new(&self.game, HUMAN)
            .proposals_to_me()
            .into_iter()
            .next()
    }

    /// Open the box for a proposal that waits, over the map.
    pub(crate) fn queue_proposal(&mut self) {
        if !self.game.in_battle()
            && matches!(self.screen, Screen::Map)
            && !self.modals.has(kind::PROPOSAL)
            && self.pending_proposal().is_some()
        {
            self.modals.enqueue(ProposalAsk);
        }
    }

    /// The question a proposal made to the human asks.
    pub(crate) fn proposal_question(&self, proposal: Proposal) -> String {
        let mut args = Args::new().text("wizard", self.diplo_name(proposal.from));
        match proposal.kind {
            ProposalKind::WarOn(third) | ProposalKind::BreakAllianceWith(third) => {
                args = args.text("third", self.diplo_name(third));
            }
            ProposalKind::Trade { give, get } | ProposalKind::Priced { give, get, .. } => {
                args = args
                    .text("give", self.goods_text(give))
                    .text("get", self.goods_text(get));
            }
            ProposalKind::Demand { gold } => args = args.count("gold", gold),
            ProposalKind::WarPrice { third, price } => {
                args = args
                    .text("third", self.diplo_name(third))
                    .text("get", self.goods_text(price));
            }
            _ => {}
        }
        let key = if !self.game.is_call_to_arms(proposal) {
            question_key(proposal.kind)
        } else if proposal.round == 0 {
            "diplomacy.call_to_arms"
        } else {
            "diplomacy.call_to_arms_again"
        };
        self.text_with(key, &args)
    }

    /// Answer the proposal `from` has waiting for the human. What that brings back to it (a call to
    /// arms once more, when its price is not paid) it answers at once. Returns what happened.
    pub(crate) fn answer_offer(
        &mut self,
        from: f4x_core::player::PlayerId,
        accept: bool,
    ) -> Vec<Event> {
        let mut events = self.game.apply(
            HUMAN,
            Command::Diplomacy(DiplomacyCommand::Answer {
                player: HUMAN,
                from,
                accept,
            }),
        );
        let proposals = self.game.diplomacy.proposals();
        if proposals.iter().any(|p| p.from == HUMAN && p.to == from) {
            events.extend(f4x_ai::answer_proposals(
                &mut self.game,
                from,
                &self.diplomats,
            ));
        }
        events
    }

    /// The first partner the human has to choose about, if the human can act on it.
    fn trespass_to_judge(&self) -> Option<f4x_core::player::PlayerId> {
        if self.game.wizard(HUMAN).is_some_and(|w| w.is_banished()) {
            return None;
        }
        self.game.trespass_choices(HUMAN).first().copied()
    }

    /// Open the box for a trespass that waits for the human's choice, over the map.
    pub(crate) fn queue_trespass(&mut self) {
        if !self.game.in_battle()
            && matches!(self.screen, Screen::Map)
            && !self.modals.has(kind::TRESPASS)
            && self.trespass_to_judge().is_some()
        {
            self.modals.enqueue(TrespassAsk);
        }
    }

    /// Ask the human about the first trespass to judge; false once none waits.
    fn trespass_frame(&mut self, input: &Input) -> bool {
        let Some(intruder) = self.trespass_to_judge() else {
            return false;
        };
        let lay = crate::frame::Layout::new();
        let text = self.text_with(
            "diplomacy.trespass_choice",
            &Args::new().text("wizard", self.diplo_name(intruder)),
        );
        let war = Third {
            button: (Slab::No, "diplomacy.button_war"),
            key: KeyCode::W,
        };
        if let Some(pick) = self.choice_box(&lay, &text, &BREAK_OR_OVERLOOK, Some(war), input) {
            self.judge_trespass(intruder, pick);
        }
        self.trespass_to_judge().is_some()
    }

    /// Send what the button of the trespass box stands for.
    fn judge_trespass(&mut self, intruder: f4x_core::player::PlayerId, pick: Pick) {
        let command = match pick {
            Pick::Right => DiplomacyCommand::BreakTreaty {
                player: HUMAN,
                with: intruder,
            },
            Pick::Left => DiplomacyCommand::Overlook {
                player: HUMAN,
                with: intruder,
            },
            Pick::Third => DiplomacyCommand::DeclareWar {
                player: HUMAN,
                on: intruder,
            },
        };
        let events = self.game.apply(HUMAN, Command::Diplomacy(command));
        self.note(events);
    }

    /// Ask the human about the first waiting proposal; false once none waits.
    fn proposal_frame(&mut self, input: &Input) -> bool {
        let Some(proposal) = self.pending_proposal() else {
            return false;
        };
        let lay = crate::frame::Layout::new();
        let text = self.proposal_question(proposal);
        let call = self.game.is_call_to_arms(proposal);
        let buttons = if call {
            &JOIN_OR_BREAK
        } else if matches!(proposal.kind, ProposalKind::WarPrice { .. }) {
            &YES_OR_NO_ONLY
        } else {
            &YES_NO
        };
        let label = if call {
            "diplomacy.button_price"
        } else {
            "diplomacy.button_counter"
        };
        let counter = self.counterable(proposal).then_some(Third {
            button: (Slab::Yes, label),
            key: KeyCode::C,
        });
        match self.choice_box(&lay, &text, buttons, counter, input) {
            // The counter is put together on the diplomacy screen; the box comes back if it is
            // left unsent.
            Some(Pick::Third) if self.open_counter(proposal) => return false,
            Some(Pick::Third) | None => {}
            Some(pick) => {
                let events = self.answer_offer(proposal.from, pick == Pick::Right);
                self.note(events);
            }
        }
        self.pending_proposal().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_app::shipped;
    use f4x_core::diplomacy::Treaty;
    use f4x_core::player::PlayerId;
    use f4x_core::rules::RealmId;
    use f4x_def::testdir::TestResult;

    #[test]
    fn every_kind_of_proposal_asks_a_question_that_the_texts_have() -> TestResult {
        let spell = shipped::content()
            .rules
            .spells
            .id_numbered(83)
            .ok_or("fire_bolt is not in the content")?;
        let priced = |deal| ProposalKind::Priced {
            deal,
            give: f4x_core::diplomacy::Goods::NONE,
            get: f4x_core::diplomacy::Goods::gold(50),
        };
        let kinds = [
            priced(Deal::Pact),
            priced(Deal::Alliance),
            priced(Deal::Peace),
            ProposalKind::Pact,
            ProposalKind::Alliance,
            ProposalKind::Peace,
            ProposalKind::WarOn(PlayerId(2)),
            ProposalKind::BreakAllianceWith(PlayerId(2)),
            ProposalKind::Trade {
                give: f4x_core::diplomacy::Goods::spell(spell),
                get: f4x_core::diplomacy::Goods::spell(spell),
            },
            ProposalKind::Demand { gold: 50 },
            ProposalKind::Threat,
            ProposalKind::MapExchange,
            ProposalKind::TradePact,
            ProposalKind::ResearchPact,
            ProposalKind::WarPrice {
                third: PlayerId(2),
                price: f4x_core::diplomacy::Goods::gold(50),
            },
        ];
        for kind in kinds {
            let key = question_key(kind);
            assert!(
                shipped::texts().has(key),
                "{kind:?} asks {key}, which is missing"
            );
        }
        Ok(())
    }

    /// A new game in which a rival, bound to the human by a pact, has had a fighter next to a
    /// city of the human up to the last warning.
    fn trespassed() -> Result<(App, PlayerId), Box<dyn std::error::Error>> {
        let mut app = App::for_test();
        let rival = app.ais.first().ok_or("no rival")?.player;
        app.game.make_contact(rival, HUMAN);
        app.game.start_treaty(rival, HUMAN, Treaty::Pact);
        let (_, city) = *app
            .game
            .economy()
            .cities_of(HUMAN)
            .first()
            .ok_or("the human has no city")?;
        let unit = app
            .game
            .content()
            .rules
            .units
            .id_numbered(104 /* high_men_spearmen */)
            .ok_or("high_men_spearmen is not in the content")?;
        let arrived = f4x_core::world::testutil::sweep::around(&app.game, city)
            .into_iter()
            .any(|pos| {
                app.game.command(Command::CreateUnit {
                    owner: rival,
                    pos,
                    unit,
                });
                app.game.trespassers(HUMAN).contains(&rival)
            });
        if !arrived {
            return Err("no room next to the city".into());
        }
        for _ in 0..app.game.content().rules.config.diplomacy.trespass_warnings {
            app.game.start_player_turn(HUMAN);
        }
        Ok((app, rival))
    }

    #[test]
    fn after_the_last_warning_the_human_is_asked_and_each_button_does_what_it_says() -> TestResult {
        for (pick, treaty) in [
            (Pick::Right, Treaty::None),
            (Pick::Left, Treaty::Pact),
            (Pick::Third, Treaty::War),
        ] {
            let (mut app, rival) = trespassed()?;
            app.queue_trespass();
            assert!(app.modals.has(kind::TRESPASS), "{pick:?}: not asked");
            app.judge_trespass(rival, pick);
            assert_eq!(app.game.treaty(HUMAN, rival), treaty, "{pick:?}");
            assert_eq!(app.trespass_to_judge(), None, "{pick:?}: asked again");
        }
        Ok(())
    }

    /// A new game in which `caller`, the human or a rival, allied to the other of the two and at
    /// war with a second rival, has called that ally to arms. Returns the rival and the call.
    fn called(by_human: bool) -> Result<(App, PlayerId, Proposal), Box<dyn std::error::Error>> {
        let mut app = App::for_test();
        let rival = app.ais.first().ok_or("no rival")?.player;
        // The game of a test has one rival: a third wizard joins it.
        let third = (2..u8::MAX)
            .map(PlayerId)
            .find(|&p| app.game.player(p).is_none())
            .ok_or("no free seat")?;
        app.game.add_player(third, "North");
        app.game
            .init_wizard(third, &[(RealmId::new(3 /* life */), 3)], &[], None)
            .map_err(|e| format!("{e:?}"))?;
        for (a, b) in [(HUMAN, rival), (HUMAN, third), (rival, third)] {
            app.game.make_contact(a, b);
        }
        app.game.start_treaty(HUMAN, rival, Treaty::Alliance);
        let (from, to) = if by_human {
            (HUMAN, rival)
        } else {
            (rival, HUMAN)
        };
        app.game.declare_war(from, third);
        app.game
            .command(Command::Diplomacy(DiplomacyCommand::Propose {
                from,
                to,
                kind: ProposalKind::WarOn(third),
            }));
        let call = *app.game.diplomacy.proposals().first().ok_or("no call")?;
        Ok((app, rival, call))
    }

    #[test]
    fn a_call_to_arms_says_that_the_alliance_is_at_stake_and_takes_a_price_once() -> TestResult {
        let (app, _, call) = called(false)?;
        assert!(app.game.is_call_to_arms(call));
        assert!(app.counterable(call), "a price may be named");
        let asked = app.proposal_question(call);
        let again = Proposal { round: 1, ..call };
        assert!(!app.counterable(again), "one price to a call");
        assert_ne!(app.proposal_question(again), asked);
        Ok(())
    }

    #[test]
    fn the_human_joins_the_war_or_ends_the_alliance() -> TestResult {
        for accept in [true, false] {
            let (mut app, rival, call) = called(false)?;
            let ProposalKind::WarOn(third) = call.kind else {
                return Err("not a war".into());
            };
            app.answer_offer(rival, accept);
            let joined = app.game.treaty(HUMAN, third) == Treaty::War;
            let allied = app.game.treaty(HUMAN, rival) == Treaty::Alliance;
            assert_eq!((joined, allied), (accept, accept));
        }
        Ok(())
    }

    #[test]
    fn an_ally_whose_price_the_human_turns_down_answers_at_once() -> TestResult {
        let (mut app, rival, call) = called(true)?;
        let ProposalKind::WarOn(third) = call.kind else {
            return Err("not a war".into());
        };
        app.game
            .command(Command::Diplomacy(DiplomacyCommand::Counter {
                player: rival,
                from: HUMAN,
                kind: ProposalKind::WarPrice {
                    third,
                    price: f4x_core::diplomacy::Goods::gold(1),
                },
            }));
        app.answer_offer(rival, false);
        assert_eq!(app.game.diplomacy.proposals(), [], "nothing left waiting");
        let joined = app.game.treaty(rival, third) == Treaty::War;
        let allied = app.game.treaty(HUMAN, rival) == Treaty::Alliance;
        assert_eq!(joined, allied, "joined, or the alliance is over");
        Ok(())
    }

    #[test]
    fn a_rivals_pact_is_countered_until_the_rounds_of_the_rules_are_used_up() -> TestResult {
        let mut app = App::for_test();
        let rival = app.ais.first().ok_or("no rival")?.player;
        app.game.make_contact(rival, HUMAN);
        app.game.apply(
            rival,
            Command::Diplomacy(DiplomacyCommand::Propose {
                from: rival,
                to: HUMAN,
                kind: ProposalKind::Pact,
            }),
        );
        let proposal = app.pending_proposal().ok_or("no proposal")?;
        assert!(app.counterable(proposal));
        let last = Proposal {
            round: app.game.content().rules.config.diplomacy.max_rounds - 1,
            ..proposal
        };
        assert!(!app.counterable(last), "no round left");
        Ok(())
    }

    #[test]
    fn a_rivals_proposal_is_asked_in_the_middle_of_the_round_and_holds_the_rivals_turn(
    ) -> TestResult {
        let mut app = App::for_test();
        let rival = app.ais.first().ok_or("no rival")?.player;
        let round = app.game.round();
        app.game.make_contact(rival, HUMAN);
        app.game.apply(
            rival,
            Command::Diplomacy(DiplomacyCommand::Propose {
                from: rival,
                to: HUMAN,
                kind: ProposalKind::Pact,
            }),
        );
        // The rival has played and its turn is about to end.
        let held = f4x_master::Phase::TurnEnd(rival);
        app.master_state = f4x_master::State { phase: held };
        app.turn_events = Some(Vec::new());
        app.resume_turn();
        assert_eq!(
            app.master_state.phase, held,
            "the turn waits for the answer"
        );
        assert!(app.turn_events.is_some(), "the round goes on later");
        app.queue_proposal();
        assert!(app.modals.has(kind::PROPOSAL), "the human is asked at once");

        app.game.apply(
            HUMAN,
            Command::Diplomacy(DiplomacyCommand::Answer {
                player: HUMAN,
                from: rival,
                accept: false,
            }),
        );
        app.modals.remove(kind::PROPOSAL);
        app.resume_turn();
        assert_ne!(app.master_state.phase, held, "the turn has ended");
        assert!(app.game.round() >= round);
        Ok(())
    }
}
