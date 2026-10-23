//! The AI's side of the talk. The core keeps the relations and treaties and only checks that a
//! proposal is legal; whether to accept it, and whether to start a war, is decided here, with the
//! original's rating of a proposal (`Diplomacy_Test`, docs/diplomacy.md section 4) as the
//! starting logic. The core wears down the asker's patience when a proposal is answered.

use f4x_core::diplomacy::{
    army_strength, DiplomacyCommand, ProposalKind, Treaty, ALLIANCE_RELATION, PACT_RELATION,
};
use f4x_core::game::{Command, Event, Game};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;

use crate::Ai;

/// Answer every proposal other wizards have made to this one. A proposal nobody answers lapses
/// as refused when the turn ends.
pub(crate) fn answer(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let pending = PlayerView::new(game, ai.player).proposals_to_me();
    for proposal in pending {
        let accept = rate(game, proposal.from, proposal.to, proposal.kind) == Verdict::Accepted;
        ai.send(
            game,
            Command::Diplomacy(DiplomacyCommand::Answer {
                player: ai.player,
                from: proposal.from,
                accept,
            }),
            events,
        );
    }
}

/// How the AI rates a proposal (`Diplomacy_Test`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Refused,
    /// A counter-offer would be due; the original has none.
    Counter,
    Maybe,
    Accepted,
}

/// How a threat weighs: the asker's army against the judge's, as the original rates it.
pub(crate) fn threat_modifier(game: &Game, asker: PlayerId, judge: PlayerId) -> i32 {
    let (mine, theirs) = (army_strength(game, asker), army_strength(game, judge));
    if theirs < mine {
        if theirs > 0 {
            -50 + mine * 100 / theirs
        } else {
            50
        }
    } else if mine > 0 {
        -50 + theirs * 100 / mine
    } else {
        -50
    }
}

/// The original's `Diplomacy_Test` of a proposal that `asker` makes to `judge`, the judge being
/// the AI: its relation to the asker, its patience, its temper, the threat of the asker's army
/// and a roll of the game's dice. A doubtful answer is a yes half the time (the original has no
/// counter-offers).
pub(crate) fn rate(
    game: &mut Game,
    asker: PlayerId,
    judge: PlayerId,
    kind: ProposalKind,
) -> Verdict {
    let relation = game.relation(asker, judge);
    let (type_modifier, early) = match kind {
        ProposalKind::Pact => (75, (relation <= PACT_RELATION).then_some(Verdict::Refused)),
        ProposalKind::Alliance => (
            125,
            (relation <= ALLIANCE_RELATION).then_some(Verdict::Refused),
        ),
        ProposalKind::Peace => (60, None),
        ProposalKind::WarOn(third) => {
            let allied_with_both = game.treaty(asker, judge) == Treaty::Alliance
                && game.treaty(asker, third) == Treaty::Alliance;
            let early = allied_with_both.then(|| {
                if game.roll(8) < 5 {
                    Verdict::Accepted
                } else {
                    Verdict::Refused
                }
            });
            (150 + game.relation(judge, third), early)
        }
        ProposalKind::BreakAllianceWith(third) => (
            175 + game.relation(judge, third),
            (relation <= 24).then_some(Verdict::Maybe),
        ),
        // The original swaps spells without a test: both must have one the other lacks.
        ProposalKind::Exchange { .. } => (0, Some(Verdict::Accepted)),
    };
    if let Some(v) = early {
        return v;
    }
    let stance = game
        .diplomacy
        .stance(asker, judge)
        .cloned()
        .unwrap_or_default();
    let relations =
        stance.treaty_patience + stance.hidden + relation + game.personality(judge).modifier();
    let mut final_value =
        relations + game.roll(100) as i32 - type_modifier + threat_modifier(game, asker, judge);
    if game.treaty(asker, judge) == Treaty::Alliance {
        final_value += 40;
    }
    let verdict = match final_value {
        v if v < -75 => Verdict::Refused,
        v if v < -50 => Verdict::Counter,
        v if v < 0 => Verdict::Maybe,
        _ => Verdict::Accepted,
    };
    match verdict {
        Verdict::Counter | Verdict::Maybe if game.roll(2) == 1 => Verdict::Accepted,
        v => v,
    }
}

/// A wizard declares war on a rival who is casting the Spell of Mastery when a roll of 1 to 100
/// comes out below this, each turn it is cast (the original rolls it when the rival's relation
/// drops).
const MASTERY_WAR_BELOW: u32 = 11;

/// A wizard that has met a rival casting the Spell of Mastery may declare war on it, on its own
/// account, whatever the race (`race::run` makes the war that the race itself calls for).
pub(crate) fn mastery_scare(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    if PlayerView::new(game, me)
        .wizard()
        .is_none_or(|w| w.is_banished())
    {
        return;
    }
    let casters: Vec<PlayerId> = PlayerView::new(game, me)
        .rival_mastery_casters()
        .into_iter()
        .map(|(p, _)| p)
        .filter(|&p| game.have_met(me, p) && game.treaty(me, p) != Treaty::War)
        .collect();
    for rival in casters {
        if game.roll(100) < MASTERY_WAR_BELOW {
            ai.send(
                game,
                Command::Diplomacy(DiplomacyCommand::DeclareWar {
                    player: me,
                    on: rival,
                }),
                events,
            );
        }
    }
}

/// The chance, in a turn, that a fierce militarist or expansionist starts a war (one in this).
const QUARREL_ONE_IN: u32 = 20;

/// A wizard whose objective and temper pick fights declares war, now and then, on the weakest
/// rival it has met and has no treaty with, if that rival holds fewer cities than it does and
/// the AI knows one of them to march on (`DIPLOMAC.c`). The armies then go for that wizard's
/// cities first (`race::war_target`).
pub(crate) fn quarrel(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let (fights, mine, rivals) = {
        let view = PlayerView::new(game, me);
        let Some(w) = view.wizard() else {
            return;
        };
        // A banished wizard starts no talk and no war.
        let fights =
            !w.is_banished() && crate::objective::picks_fights(w.objective(), w.personality());
        let known = view.known_cities();
        let rivals: Vec<(PlayerId, usize)> = game
            .player_ids()
            .into_iter()
            .filter(|&p| p != me && view.is_rival_wizard(p))
            .filter(|&p| game.have_met(me, p) && game.treaty(me, p) == Treaty::None)
            .map(|p| (p, known.iter().filter(|(_, o, _)| *o == p).count()))
            .filter(|&(_, n)| n > 0)
            .collect();
        (fights, view.my_cities().len(), rivals)
    };
    if ai.quarrel.is_some_and(|p| {
        game.player(p).is_none_or(|q| q.is_defeated()) || game.treaty(me, p) != Treaty::War
    }) {
        ai.quarrel = None;
    }
    if !fights || ai.quarrel.is_some() || game.roll(QUARREL_ONE_IN) != 1 {
        return;
    }
    let Some(&(target, _)) = rivals
        .iter()
        .filter(|&&(_, n)| n < mine)
        .min_by_key(|&&(p, n)| (n, p.0))
    else {
        return;
    };
    if ai.send(
        game,
        Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: me,
            on: target,
        }),
        events,
    ) {
        ai.quarrel = Some(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::diplomacy::{DiplomacyEvent, ProposalKind};
    use f4x_core::player::PlayerId;
    use f4x_core::wizard::objective::{Objective, Personality};

    const HUMAN: PlayerId = PlayerId(0);
    const AI: PlayerId = PlayerId(1);

    fn game() -> Game {
        let mut g = Game::new(5);
        for id in [HUMAN, AI] {
            g.add_player(id, "W");
        }
        g.set_human(Some(HUMAN));
        g.make_contact(HUMAN, AI);
        g
    }

    #[test]
    fn a_proposal_made_to_the_ai_is_answered() {
        let mut g = game();
        g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
            from: HUMAN,
            to: AI,
            kind: ProposalKind::Pact,
        }));
        let mut events = Vec::new();
        answer(&Ai::new(AI), &mut g, &mut events);
        assert!(g.diplomacy.proposals().is_empty());
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Diplomacy(DiplomacyEvent::ProposalAnswered { lapsed: false, .. })
        )));
    }

    /// The west wizard (player 0) with two cities has seen the one city of the east wizard
    /// (player 1), met it, and has the given objective and temper.
    fn rivals(objective: Objective, temper: Personality) -> Game {
        use crate::testworld::{at, game, COAST_CITY, ENEMY_CITY, INLAND_CITY, P0, P1};
        let mut g = game(COAST_CITY, "high_men");
        let race = g.content().rules.races.id_of("high_men");
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(INLAND_CITY.0, INLAND_CITY.1),
            name: "Inland".into(),
            race,
            population: 5,
        });
        g.make_contact(P0, P1);
        g.explore(P0, at(ENEMY_CITY.0, ENEMY_CITY.1), 3);
        let mut w = g.wizard_mut(P0).unwrap();
        w.put(Some(objective));
        w.put(Some(temper));
        g
    }

    /// Whether the west wizard declared war in `turns` tries.
    fn declares_war(g: &mut Game, turns: u32) -> bool {
        use crate::testworld::{P0, P1};
        let mut ai = Ai::new(P0);
        for _ in 0..turns {
            quarrel(&mut ai, g, &mut Vec::new());
        }
        g.treaty(P0, P1) == Treaty::War && ai.quarrel == Some(P1)
    }

    #[test]
    fn a_fierce_militarist_declares_war_on_a_weaker_neighbour_in_time() {
        let mut g = rivals(Objective::Militarist, Personality::Aggressive);
        assert!(declares_war(&mut g, 400));
    }

    #[test]
    fn a_lawful_or_peaceful_wizard_never_starts_a_war() {
        for temper in [Personality::Lawful, Personality::Peaceful] {
            let mut g = rivals(Objective::Militarist, temper);
            assert!(!declares_war(&mut g, 400), "{temper:?}");
        }
    }

    #[test]
    fn a_theurgist_never_starts_a_war() {
        let mut g = rivals(Objective::Theurgist, Personality::Maniacal);
        assert!(!declares_war(&mut g, 400));
    }

    #[test]
    fn nobody_attacks_a_neighbour_that_holds_as_many_cities() {
        use crate::testworld::{at, ENEMY_CITY, P1};
        let mut g = rivals(Objective::Expansionist, Personality::Ruthless);
        let race = g.content().rules.races.id_of("high_men");
        let events = g.apply(Command::FoundCity {
            owner: P1,
            pos: at(ENEMY_CITY.0 + 4, ENEMY_CITY.1 + 3),
            name: "Eastern".into(),
            race,
            population: 5,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        g.explore(
            crate::testworld::P0,
            at(ENEMY_CITY.0 + 4, ENEMY_CITY.1 + 3),
            3,
        );
        assert!(!declares_war(&mut g, 400));
    }

    #[test]
    fn a_pact_is_refused_out_of_hand_below_the_threshold_and_the_rating_needs_no_patience() {
        let mut g = game();
        assert_eq!(
            rate(&mut g, HUMAN, AI, ProposalKind::Pact),
            Verdict::Refused,
            "a stranger at relation 0 does not sign a pact"
        );
        assert_eq!(
            rate(&mut g, HUMAN, AI, ProposalKind::Alliance),
            Verdict::Refused
        );
        // A spell swap is granted unrated.
        let spell = g.content().rules.spells.iter().next().map(|(id, _)| id);
        if let Some(spell) = spell {
            assert_eq!(
                rate(
                    &mut g,
                    HUMAN,
                    AI,
                    ProposalKind::Exchange {
                        give: spell,
                        get: spell
                    }
                ),
                Verdict::Accepted
            );
        }
    }

    #[test]
    fn a_bigger_army_makes_a_wizard_more_pliant() {
        use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
        let mut g = game(COAST_CITY, "high_men");
        let cost = g
            .content()
            .rules
            .units
            .get(g.content().rules.units.id_of("high_men_spearmen").unwrap())
            .cost as i32;
        let base = threat_modifier(&g, P0, P1);
        spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            "high_men_spearmen",
            4,
        );
        let stronger = threat_modifier(&g, P0, P1);
        assert!(stronger > base, "{base} -> {stronger}");
        spawn(
            &mut g,
            P1,
            at(ENEMY_CITY.0, ENEMY_CITY.1),
            "high_men_spearmen",
            1,
        );
        assert_eq!(threat_modifier(&g, P0, P1), -50 + 4 * cost * 100 / cost);
    }

    #[test]
    fn a_wizard_that_has_met_a_mastery_caster_declares_war_on_it_in_time() {
        use crate::testworld::{game, COAST_CITY, P0, P1};
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let mastery = g.content().rules.spells.id_of("spell_of_mastery").unwrap();
        let ai = Ai::new(P0);
        // Nobody casts it: nothing happens, however often it is rolled.
        for _ in 0..300 {
            mastery_scare(&ai, &mut g, &mut Vec::new());
        }
        assert_eq!(g.treaty(P0, P1), Treaty::None);
        g.scenario_casting(P1, mastery, 5000);
        for _ in 0..300 {
            mastery_scare(&ai, &mut g, &mut Vec::new());
        }
        assert_eq!(g.treaty(P0, P1), Treaty::War);
    }

    #[test]
    fn proposals_to_others_are_left_alone() {
        let mut g = game();
        g.add_player(PlayerId(2), "W");
        g.make_contact(HUMAN, PlayerId(2));
        g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
            from: HUMAN,
            to: PlayerId(2),
            kind: ProposalKind::Pact,
        }));
        let mut events = Vec::new();
        answer(&Ai::new(AI), &mut g, &mut events);
        assert_eq!(g.diplomacy.proposals().len(), 1);
        assert!(events.is_empty());
    }

    #[test]
    fn a_friendly_wizard_accepts_a_pact_and_a_hostile_one_does_not() {
        let mut yes = 0;
        let mut no = 0;
        for seed in 0..40 {
            let mut g = Game::new(seed);
            for id in [HUMAN, AI] {
                g.add_player(id, "W");
            }
            g.set_human(Some(HUMAN));
            g.make_contact(HUMAN, AI);
            // Friends: the human has done well by the AI, and the AI is easy-going.
            for _ in 0..8 {
                g.change_relations(8, HUMAN, AI, f4x_core::diplomacy::Action::Friendship, None);
            }
            g.wizard_mut(AI)
                .unwrap()
                .put(Some(f4x_core::wizard::objective::Personality::Peaceful));
            if rate(&mut g, HUMAN, AI, ProposalKind::Pact) == Verdict::Accepted {
                yes += 1;
            }
            let mut h = Game::new(seed);
            for id in [HUMAN, AI] {
                h.add_player(id, "W");
            }
            h.set_human(Some(HUMAN));
            h.make_contact(HUMAN, AI);
            for _ in 0..8 {
                h.change_relations(
                    -8,
                    HUMAN,
                    AI,
                    f4x_core::diplomacy::Action::AttackedCity,
                    None,
                );
            }
            if rate(&mut h, HUMAN, AI, ProposalKind::Pact) == Verdict::Accepted {
                no += 1;
            }
        }
        assert_eq!(
            no, 0,
            "a hostile wizard refuses (relation is below the pact threshold)"
        );
        assert!(yes > 0, "a friendly one sometimes agrees");
    }
}
