//! Diplomacy system B, part 3 (task 779, `docs/ai-diplomacy-b.md`): how a wizard rates what it is asked,
//! and what it says to the human at the start of the human's turn. No dice: every answer is a rule of the
//! three values of `diplo_b::Regard` and of `diplo_b_stance::values`, with its numbers in `diplo_b::Knobs`.
//!
//! * A **pact** or an **alliance** is accepted when the friend value (and the personality's mood) reaches
//!   the mark and the friendship is above the floor of the rules.
//! * **War on a third wizard** is accepted when the friendship with the asker and the enemy value of the
//!   third reach a mark, never against one much stronger; **breaking an alliance** when the asker is
//!   liked much more than the ally.
//! * **A threat or a demand** is yielded to by a wizard that fears the asker; a much weaker asker of a threat
//!   is answered by war. A trade is by worth (`trade.rs`).
//! * **To the human** it offers peace by the peace score, courts with pacts, alliances and gifts, demands
//!   tribute of a weaker player it dislikes, warns and breaks a treaty when the friendship falls and
//!   declares war on someone it hates.
//!
//! Tests: below.

use f4x_core::diplomacy::{army_strength, DiplomacyCommand, ProposalKind, Remark, Treaty};
use f4x_core::game::{Command, Event, Game};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;

use crate::diplo_b::{regard, Knobs, Regard};
use crate::diplo_b_stance::{court, declare_on, values};
use crate::diplomacy::Verdict;
use crate::memory::{Did, Memory};
use crate::statecraft::{can_attack, propose, remark, Scene};
use crate::Ai;

/// The personality's mood, in friendship points.
fn mood(game: &Game, me: PlayerId, k: &Knobs) -> i32 {
    crate::objective::temper(game.personality(me), game.difficulty()).modifier() / k.mood_div
}

/// How `judge` rates a proposal that `asker` makes to it.
pub(crate) fn rate(
    game: &mut Game,
    asker: PlayerId,
    judge: PlayerId,
    kind: ProposalKind,
    memory: &Memory,
) -> Verdict {
    let k = Knobs::current();
    let scene = Scene::new(game, judge);
    let r = regard(game, judge, asker, &scene, memory);
    let rules = &game.content().rules.config.diplomacy;
    let (pact_floor, ally_floor) = (rules.pact_relation, rules.alliance_relation);
    let yes = |b: bool| {
        if b {
            Verdict::Accepted
        } else {
            Verdict::Refused
        }
    };
    let friend = |game: &Game| {
        values(game, judge, asker, &scene, memory, false).friend + mood(game, judge, &k)
    };
    match kind {
        ProposalKind::Peace => yes(crate::diplo_b::accepts(game, judge, asker, memory)),
        ProposalKind::Pact => yes(r.friendship > pact_floor && friend(game) >= k.pact_accept),
        ProposalKind::Alliance => yes(r.friendship > ally_floor && friend(game) >= k.ally_accept),
        ProposalKind::WarOn(third) => {
            let sk = crate::statecraft::Knobs::current();
            let (m, t) = (
                scene.power(game, judge).score(&sk).max(1),
                scene.power(game, third).score(&sk),
            );
            if t * 100 > m * k.too_strong_pct && game.treaty(judge, third) != Treaty::War {
                return Verdict::Refused;
            }
            let reach = can_attack(game, judge, &scene, third);
            let enemy = values(game, judge, third, &scene, memory, reach).enemy;
            yes(game.treaty(judge, third) == Treaty::War
                || r.friendship + mood(game, judge, &k) + enemy >= k.war_accept)
        }
        ProposalKind::BreakAllianceWith(third) => {
            let other = regard(game, judge, third, &scene, memory);
            yes(r.friendship - other.friendship >= k.break_accept)
        }
        ProposalKind::Trade { give, get } => crate::trade::verdict(game, judge, asker, give, get),
        ProposalKind::Threat => yes(yields_to_threat(game, judge, asker, &r, &k)),
        // The tribute is paid by one who fears the asker; the game pays it only if the gold is there.
        ProposalKind::Demand { .. } => yes(r.fear >= k.yield_fear),
    }
}

/// How a wizard answers a threat: one that fears the asker pays (what it can of two to nine times
/// 25 gold, more the more it fears) and yields; one far stronger than the asker declares war; else it
/// refuses.
fn yields_to_threat(
    game: &mut Game,
    judge: PlayerId,
    asker: PlayerId,
    r: &Regard,
    k: &Knobs,
) -> bool {
    if r.fear >= k.yield_fear {
        let gold = game.player(judge).map_or(0, |p| p.treasury().gold.floor());
        let pay = (25 * (2 + r.fear / 25)).min(gold / 25 * 25);
        return pay <= 0
            || !game
                .apply(Command::Diplomacy(DiplomacyCommand::GiveGold {
                    from: judge,
                    to: asker,
                    amount: pay,
                }))
                .iter()
                .any(|e| matches!(e, Event::Rejected(_)));
    }
    if army_strength(game, judge) * 80 > army_strength(game, asker) * 100 {
        game.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: judge,
            on: asker,
        }));
    }
    false
}

/// What an AI wizard says to the human at the start of the human's turn, as commands a human could send:
/// peace, courting, gifts, demands, warnings, a treaty broken, war. The proposals wait for the human's
/// answer in that turn. The client calls it for each rival after the turn has changed.
pub(crate) fn approach_human(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let Some(human) = game.human().filter(|&h| h != me) else {
        return;
    };
    if game.player(human).is_none_or(|p| p.is_defeated())
        || game.wizard(human).is_none_or(|w| w.is_banished())
    {
        return;
    }
    let scene = Scene::new(game, me);
    let banished = PlayerView::new(game, me)
        .wizard()
        .is_some_and(|w| w.is_banished());
    if banished || !scene.others.contains(&human) {
        return;
    }
    let k = Knobs::current();
    let treaty = game.treaty(me, human);
    if treaty == Treaty::War {
        if crate::diplo_b::offer_due(ai, game, human)
            && propose(ai, game, events, human, ProposalKind::Peace)
        {
            ai.memory.put().offered_peace(human, game.turn);
            remark(ai, game, events, human, Remark::WarWeary);
        }
        return;
    }
    let friendship = {
        let memory = ai.memory.get();
        regard(game, me, human, &scene, &memory).friendship
    };
    if treaty == Treaty::None && friendship <= k.war_below && game.turn >= k.from_turn {
        declare_on(ai, game, events, human);
        return;
    }
    if treaty != Treaty::None {
        if friendship <= k.break_below {
            ai.order(
                game,
                Command::Diplomacy(DiplomacyCommand::BreakTreaty {
                    player: me,
                    with: human,
                }),
                events,
            );
            return;
        }
        let warn_due = ai
            .memory
            .get()
            .due(human, Did::Warn, game.turn, k.warn_cool);
        if friendship <= k.warn_below && warn_due {
            ai.order(
                game,
                Command::Diplomacy(DiplomacyCommand::Warn {
                    from: me,
                    to: human,
                }),
                events,
            );
            ai.memory.put().did(human, Did::Warn, game.turn);
            return;
        }
    }
    let reachable = can_attack(game, me, &scene, human);
    court(ai, game, events, human, reachable);
    demand(ai, game, events, human, friendship, treaty, &k);
    gift(ai, game, events, human, friendship, treaty, &k);
}

/// A demand for tribute from a stronger wizard that does not like the human: a quarter of the human's
/// gold, at most a hundred, in whole 25s.
fn demand(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    human: PlayerId,
    friendship: i32,
    treaty: Treaty,
    k: &Knobs,
) {
    let me = ai.player;
    let gold = game.player(human).map_or(0, |p| p.treasury().gold.floor());
    let due = ai
        .memory
        .get()
        .due(human, Did::Demand, game.turn, k.demand_cool);
    if !due
        || treaty == Treaty::Alliance
        || friendship >= k.warn_below
        || gold < 100
        || army_strength(game, me) <= army_strength(game, human)
    {
        return;
    }
    let asked = (gold / 4 / 25 * 25).min(100);
    if asked >= 25 {
        ai.order(
            game,
            Command::Diplomacy(DiplomacyCommand::Propose {
                from: me,
                to: human,
                kind: ProposalKind::Demand { gold: asked },
            }),
            events,
        );
        ai.memory.put().did(human, Did::Demand, game.turn);
    }
}

/// A gift from a close friend with a treaty: a spell the human lacks, else gold from a full treasury.
fn gift(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    human: PlayerId,
    friendship: i32,
    treaty: Treaty,
    k: &Knobs,
) {
    let me = ai.player;
    let due = ai
        .memory
        .get()
        .due(human, Did::Gift, game.turn, k.gift_cool);
    if !due || treaty == Treaty::None || friendship < k.gift_friend {
        return;
    }
    if game.turn.is_multiple_of(3) {
        if let Some(spell) = crate::trade::spell_to_give(game, me, human) {
            ai.order(
                game,
                Command::Diplomacy(DiplomacyCommand::GiveSpell {
                    from: me,
                    to: human,
                    spell,
                }),
                events,
            );
            ai.memory.put().did(human, Did::Gift, game.turn);
            return;
        }
    }
    let gold = game.player(me).map_or(0, |p| p.treasury().gold.floor());
    if gold >= 200 {
        ai.order(
            game,
            Command::Diplomacy(DiplomacyCommand::GiveGold {
                from: me,
                to: human,
                amount: 50,
            }),
            events,
        );
        ai.memory.put().did(human, Did::Gift, game.turn);
    }
}

/// The trades the wizard offers the other wizards, now and then (the human is asked by
/// `approach_human`'s courting and by the human's own offers).
pub(crate) fn negotiate(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let k = Knobs::current();
    let scene = Scene::new(game, me);
    let banished = PlayerView::new(game, me)
        .wizard()
        .is_some_and(|w| w.is_banished());
    if banished {
        return;
    }
    for &other in &scene.others {
        if game.human() == Some(other)
            || game.treaty(me, other) == Treaty::War
            || !ai
                .memory
                .get()
                .due(other, Did::Trade, game.turn, k.trade_cool)
        {
            continue;
        }
        ai.memory.put().did(other, Did::Trade, game.turn);
        if let Some(kind) = crate::trade::offer(game, me, other) {
            if propose(ai, game, events, other, kind) {
                // `propose` has the other answer at once.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplo_b_stance::tests::pair;
    use crate::testworld::{P0, P1};
    use f4x_core::diplomacy::Action;

    /// What `P0` feels about `P1`, moved by `value` (P1 did it).
    fn feel(g: &mut Game, value: i32) {
        g.change_relations(value, P1, P0, Action::Friendship, None);
    }

    fn verdict(g: &mut Game, kind: ProposalKind) -> Verdict {
        let ai = Ai::new(P0);
        let memory = ai.memory.get().clone();
        rate(g, P1, P0, kind, &memory)
    }

    #[test]
    fn a_friendly_wizard_accepts_a_pact_and_a_hostile_one_does_not() {
        let mut g = pair(8, 8, 100);
        assert_eq!(
            verdict(&mut g, ProposalKind::Pact),
            Verdict::Refused,
            "a stranger"
        );
        feel(&mut g, 80);
        assert_eq!(
            verdict(&mut g, ProposalKind::Pact),
            Verdict::Accepted,
            "a friend"
        );
        feel(&mut g, -200);
        assert_eq!(
            verdict(&mut g, ProposalKind::Pact),
            Verdict::Refused,
            "a foe"
        );
        assert_eq!(verdict(&mut g, ProposalKind::Alliance), Verdict::Refused);
    }

    #[test]
    fn an_alliance_needs_more_than_a_pact() {
        let mut g = pair(8, 8, 100);
        feel(&mut g, 50);
        let pact = verdict(&mut g, ProposalKind::Pact);
        let alliance = verdict(&mut g, ProposalKind::Alliance);
        assert!(
            !(alliance == Verdict::Accepted && pact == Verdict::Refused),
            "{pact:?} {alliance:?}"
        );
    }

    #[test]
    fn the_answer_is_the_same_every_time_no_dice() {
        let mut g = pair(8, 8, 100);
        feel(&mut g, 30);
        let first = verdict(&mut g, ProposalKind::Pact);
        for _ in 0..20 {
            assert_eq!(verdict(&mut g, ProposalKind::Pact), first);
        }
    }

    #[test]
    fn a_wizard_that_fears_the_asker_yields_to_a_demand_and_one_that_does_not_refuses() {
        let mut g = pair(2, 40, 100);
        assert_eq!(
            verdict(&mut g, ProposalKind::Demand { gold: 50 }),
            Verdict::Accepted,
            "outclassed and near"
        );
        let mut g = pair(40, 2, 100);
        assert_eq!(
            verdict(&mut g, ProposalKind::Demand { gold: 50 }),
            Verdict::Refused,
            "the stronger one"
        );
    }

    #[test]
    fn a_threat_from_a_much_weaker_asker_is_answered_by_war() {
        let mut g = pair(40, 2, 100);
        assert_eq!(verdict(&mut g, ProposalKind::Threat), Verdict::Refused);
        assert_eq!(
            g.treaty(P0, P1),
            Treaty::War,
            "it declared war on the threatener"
        );
    }

    /// The human is P1 and P0 the AI.
    fn with_human(mut g: Game) -> Game {
        g.set_human(Some(P1));
        g
    }

    #[test]
    fn a_wizard_that_hates_the_human_declares_war_and_a_stranger_does_nothing() {
        let mut g = with_human(pair(8, 8, 100));
        let ai = Ai::new(P0);
        let mut events = Vec::new();
        approach_human(&ai, &mut g, &mut events);
        assert_ne!(g.treaty(P0, P1), Treaty::War, "a stranger is left alone");
        feel(&mut g, -200);
        approach_human(&ai, &mut g, &mut events);
        assert_eq!(g.treaty(P0, P1), Treaty::War);
    }

    #[test]
    fn a_close_friend_with_a_treaty_gives_the_human_gold_and_one_whose_friendship_fell_breaks_the_pact(
    ) {
        let mut g = with_human(pair(8, 8, 100));
        g.start_treaty(P0, P1, Treaty::Pact);
        feel(&mut g, 100);
        g.treasury_mut(P0).expect("a treasury").gold = f4x_def::half_int::HalfInt::from_int(500);
        let before = g.player(P1).expect("a player").treasury().gold;
        let ai = Ai::new(P0);
        let mut events = Vec::new();
        approach_human(&ai, &mut g, &mut events);
        assert!(
            g.player(P1).expect("a player").treasury().gold > before,
            "no gift"
        );
        let after = g.player(P1).expect("a player").treasury().gold;
        approach_human(&ai, &mut g, &mut events);
        assert_eq!(
            g.player(P1).expect("a player").treasury().gold,
            after,
            "a second gift at once"
        );
        let mut g = with_human(pair(8, 8, 100));
        g.start_treaty(P0, P1, Treaty::Pact);
        feel(&mut g, -150);
        let mut events = Vec::new();
        approach_human(&ai, &mut g, &mut events);
        assert_eq!(g.treaty(P0, P1), Treaty::None, "the pact stands");
    }

    #[test]
    fn a_wizard_warns_a_partner_it_is_tiring_of_and_not_again_at_once() {
        let mut g = with_human(pair(8, 8, 100));
        g.start_treaty(P0, P1, Treaty::Pact);
        // Down to between the marks for a warning and for breaking the treaty.
        let k = Knobs::current();
        let ai = Ai::new(P0);
        let friendship = |g: &Game| {
            let scene = Scene::new(g, P0);
            regard(g, P0, P1, &scene, &ai.memory.get()).friendship
        };
        while friendship(&g) > k.warn_below {
            feel(&mut g, -3);
        }
        assert!(
            friendship(&g) > k.break_below,
            "went past the mark for breaking"
        );
        let mut events = Vec::new();
        let warnings = |g: &Game| g.diplomacy.stance(P0, P1).map_or(0, |s| s.warnings);
        approach_human(&ai, &mut g, &mut events);
        assert_eq!(warnings(&g), 1, "no warning");
        approach_human(&ai, &mut g, &mut events);
        assert_eq!(warnings(&g), 1, "warned twice in a turn");
        g.turn += Knobs::current().warn_cool;
        approach_human(&ai, &mut g, &mut events);
        assert_eq!(warnings(&g), 2, "no second warning after the cool-down");
    }
}
