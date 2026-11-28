//! Diplomacy system B, part 3 (task 779, `docs/ai-diplomacy-b.md`): how a wizard rates what it is asked,
//! what it does at the last warning of a trespass, and what it says to the others at the end of its turn,
//! the same for the human and for a computer player. No dice: every answer is a rule of the three values
//! of `diplo_b::Regard` and of `diplo_b_stance::values`, with its numbers in the `diplomacy_b` levers.
//!
//! * A **pact** or an **alliance** is accepted when the friend value (and the personality's mood) reaches
//!   the mark and the friendship is above the floor of the rules.
//! * **War on a third wizard** is accepted when the friendship with the asker and the enemy value of the
//!   third reach a mark, never against one much stronger; **breaking an alliance** when the asker is
//!   liked much more than the ally.
//! * **A threat or a demand** is yielded to by a wizard that fears the asker; a much weaker asker of a threat
//!   is answered by war. A trade is by worth (`trade.rs`).
//! * **To the others** it offers peace by the peace score, courts with pacts, alliances and gifts, demands
//!   tribute of a weaker player it dislikes, offers trades, warns and breaks a treaty when the friendship
//!   falls and declares war on someone it hates. At most one proposal or gift to each in a turn.
//!
//! Tests: below.

use crate::lever::Levered;
use f4x_core::diplomacy::{
    army_strength, Deal, DiplomacyCommand, Proposal, ProposalKind, Remark, Treaty,
};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::world::{Command, Event, World};
use f4x_def::rules::ai_strategy::DiplomacyBLevers;

use crate::diplo_b::{regard, Regard};
use crate::diplo_b_stance::{court, declare_on, values};
use crate::diplomacy::Verdict;
use crate::memory::{Did, Diplomats};
use crate::statecraft::{can_attack, propose, remark, Scene};
use crate::trade::Reply;
use crate::Ai;
use f4x_master::Game;

/// The personality's mood, in friendship points.
fn mood(world: &World, me: PlayerId, k: &DiplomacyBLevers) -> i32 {
    crate::objective::temper(world.personality(me), world.difficulty()).modifier() / k.mood_div
}

/// How much speaks for `judge` joining `asker` in a war on `third`: its friendship for the asker,
/// its temper and what makes the third an enemy to it (at `war_accept` it joins). A war it is in
/// already speaks for itself; none against one much stronger than it is.
pub(crate) fn war_support(
    game: &Game,
    judge: PlayerId,
    asker: PlayerId,
    third: PlayerId,
    minds: &Diplomats,
) -> Option<i32> {
    let k = game.levers().diplomacy_b;
    if game.treaty(judge, third) == Treaty::War {
        return Some(k.war_accept);
    }
    let memory = &minds.of(judge);
    let scene = Scene::new(game, judge);
    let weights = game.levers().diplomacy;
    let (m, t) = (
        scene.power(game, judge).score(&weights).max(1),
        scene.power(game, third).score(&weights),
    );
    if t * 100 > m * k.too_strong_pct {
        return None;
    }
    let friendship = regard(game, judge, asker, &scene, memory, game.round()).friendship;
    let reach = can_attack(game, judge, &scene, third);
    let enemy = values(game, judge, third, &scene, memory, reach, game.round()).enemy;
    Some(friendship + mood(game, judge, &k) + enemy)
}

/// How `judge` rates a proposal that `asker` makes to it.
pub(crate) fn rate(
    game: &mut Game,
    asker: PlayerId,
    judge: PlayerId,
    kind: ProposalKind,
    minds: &Diplomats,
) -> Verdict {
    let memory = &minds.of(judge);
    let k = game.levers().diplomacy_b;
    let scene = Scene::new(game, judge);
    let r = regard(game, judge, asker, &scene, memory, game.round());
    let yes = |b: bool| {
        if b {
            Verdict::Accepted
        } else {
            Verdict::Refused
        }
    };
    match kind {
        // A treaty, with a price or without, is weighed in points (`shortfall`, `bargain.rs`).
        ProposalKind::Pact
        | ProposalKind::Alliance
        | ProposalKind::Peace
        | ProposalKind::Priced { .. } => {
            let proposal = Proposal {
                from: asker,
                to: judge,
                kind,
                round: 0,
            };
            yes(Deal::of(kind).is_some_and(|deal| {
                crate::bargain::respond(game, minds, proposal, deal) == Reply::Accept
            }))
        }
        ProposalKind::WarOn(third) => yes(war_support(game, judge, asker, third, minds)
            .is_some_and(|support| support >= k.war_accept)),
        ProposalKind::WarPrice { price, .. } => {
            yes(crate::war_call::pays(game, judge, asker, price))
        }
        ProposalKind::MapExchange => yes(crate::map_swap::wanted(game, judge, asker)),
        // Both sides gain.
        ProposalKind::TradePact | ProposalKind::ResearchPact => Verdict::Accepted,
        ProposalKind::BreakAllianceWith(third) => {
            let other = regard(game, judge, third, &scene, memory, game.round());
            yes(r.friendship - other.friendship >= k.break_accept)
        }
        ProposalKind::Trade { give, get } => {
            crate::trade::verdict(game, minds, judge, asker, give, get)
        }
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
    k: &DiplomacyBLevers,
) -> bool {
    if r.fear >= k.yield_fear {
        let gold = game.player(judge).map_or(0, |p| p.treasury().gold.floor());
        let pay = (25 * (2 + r.fear / 25)).min(gold / 25 * 25);
        return pay <= 0
            || !game
                .apply(
                    judge,
                    Command::Diplomacy(DiplomacyCommand::GiveGold {
                        from: judge,
                        to: asker,
                        amount: pay,
                    }),
                )
                .iter()
                .any(|e| matches!(e, Event::Rejected(_)));
    }
    if army_strength(game, judge) * 80 > army_strength(game, asker) * 100 {
        game.apply(
            judge,
            Command::Diplomacy(DiplomacyCommand::DeclareWar {
                player: judge,
                on: asker,
            }),
        );
    }
    false
}

/// How many points `judge` is short of accepting the treaty `deal` from `asker` without a price;
/// 0 or less: it accepts it, with that many to spare.
pub(crate) fn shortfall(
    game: &Game,
    asker: PlayerId,
    judge: PlayerId,
    deal: Deal,
    minds: &Diplomats,
) -> i32 {
    let memory = &minds.of(judge);
    let k = game.levers().diplomacy_b;
    let rules = &game.content().rules.config.diplomacy;
    let (floor, mark) = match deal {
        Deal::Pact => (rules.pact_relation, k.pact_accept),
        Deal::Alliance => (rules.alliance_relation, k.ally_accept),
        Deal::Peace if game.treaty(judge, asker) != Treaty::War => return 0,
        Deal::Peace => {
            return k.accept - crate::diplo_b::peace_wish(game, judge, asker, memory, game.round())
        }
    };
    let scene = Scene::new(game, judge);
    let friendship = regard(game, judge, asker, &scene, memory, game.round()).friendship;
    let friend = values(game, judge, asker, &scene, memory, false, game.round()).friend
        + mood(game, judge, &k);
    // The friendship must be above the floor of the rules, and the friend value at the mark.
    (floor + 1 - friendship).max(mark - friend)
}

/// At a treaty partner's last warning for trespass the wizard chooses by its friendship: war, a
/// broken treaty, or letting it pass.
pub(crate) fn judge_trespass(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let k = game.levers().diplomacy_b;
    for other in game.trespass_choices(me) {
        let friendship = {
            let scene = Scene::new(game, me);
            let memory = ai.memory.get();
            regard(game, me, other, &scene, &memory, game.round()).friendship
        };
        let banished = game.wizard(me).is_some_and(|w| w.is_banished());
        if !banished && friendship <= k.trespass_war_below && game.round() >= k.from_turn {
            declare_on(ai, game, events, other);
            continue;
        }
        let command = if !banished && friendship <= k.trespass_break_below {
            DiplomacyCommand::BreakTreaty {
                player: me,
                with: other,
            }
        } else {
            DiplomacyCommand::Overlook {
                player: me,
                with: other,
            }
        };
        ai.order(game, Command::Diplomacy(command), events);
    }
}

/// What a computer player does about each of the others at the end of its own turn, as commands a
/// human could send: peace, war, a treaty broken, a warning, courting, demands, gifts and trades.
/// The human and a computer player are dealt with alike; only the answer to a proposal comes from
/// elsewhere (the turn waits for the human's, `f4x-master`; a computer player answers at once,
/// `statecraft::propose`).
pub(crate) fn approach(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    if PlayerView::new(game, me)
        .wizard()
        .is_some_and(|w| w.is_banished())
    {
        return;
    }
    for other in Scene::new(game, me).others {
        if game.wizard(other).is_none_or(|w| !w.is_banished()) {
            approach_one(ai, game, events, other);
        }
    }
}

/// [`approach`] to one other player: war and peace first, then at most one proposal or gift.
fn approach_one(ai: &Ai, game: &mut Game, events: &mut Vec<Event>, other: PlayerId) {
    let me = ai.player;
    let k = game.levers().diplomacy_b;
    let treaty = game.treaty(me, other);
    if treaty == Treaty::War {
        if crate::diplo_b::offer_due(ai, game, other, game.round())
            && propose(ai, game, events, other, ProposalKind::Peace)
        {
            ai.memory.put().offered_peace(other, game.round());
            remark(ai, game, events, other, Remark::WarWeary);
        }
        return;
    }
    let friendship = {
        let scene = Scene::new(game, me);
        let memory = ai.memory.get();
        regard(game, me, other, &scene, &memory, game.round()).friendship
    };
    if treaty == Treaty::None && friendship <= k.war_below && game.round() >= k.from_turn {
        declare_on(ai, game, events, other);
        return;
    }
    if treaty != Treaty::None {
        if friendship <= k.break_below {
            ai.order(
                game,
                Command::Diplomacy(DiplomacyCommand::BreakTreaty {
                    player: me,
                    with: other,
                }),
                events,
            );
            return;
        }
        let warn_due = ai
            .memory
            .get()
            .due(other, Did::Warn, game.round(), k.warn_cool);
        if friendship <= k.warn_below && warn_due {
            ai.order(
                game,
                Command::Diplomacy(DiplomacyCommand::Warn {
                    from: me,
                    to: other,
                }),
                events,
            );
            ai.memory.put().did(other, Did::Warn, game.round());
            return;
        }
    }
    // The chosen enemy is not courted.
    let enemy = ai.memory.get().enemy.is_some_and(|(e, _)| e == other);
    if (treaty == Treaty::Alliance && crate::war_call::ask(ai, game, events, other))
        || (!enemy && court(ai, game, events, other))
        || demand(ai, game, events, other, friendship, treaty, &k)
        || gift(ai, game, events, other, friendship, treaty, &k)
        || crate::map_swap::offer(ai, game, events, other)
        || crate::econ_pact::offer(ai, game, events, other)
    {
        return;
    }
    trade_offer(ai, game, events, other, &k);
}

/// A demand for tribute from a stronger wizard that does not like the other: a quarter of the
/// other's gold, at most a hundred, in whole 25s. Whether it was made.
fn demand(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    other: PlayerId,
    friendship: i32,
    treaty: Treaty,
    k: &DiplomacyBLevers,
) -> bool {
    let me = ai.player;
    let gold = game.player(other).map_or(0, |p| p.treasury().gold.floor());
    let due = ai
        .memory
        .get()
        .due(other, Did::Demand, game.round(), k.demand_cool);
    if !due
        || treaty == Treaty::Alliance
        || friendship >= k.warn_below
        || gold < 100
        || army_strength(game, me) <= army_strength(game, other)
    {
        return false;
    }
    let asked = (gold / 4 / 25 * 25).min(100);
    let made = asked >= 25
        && propose(
            ai,
            game,
            events,
            other,
            ProposalKind::Demand { gold: asked },
        );
    if made {
        ai.memory.put().did(other, Did::Demand, game.round());
    }
    made
}

/// A gift from a close friend with a treaty: a spell the other lacks, else gold from a full
/// treasury. Whether one was given.
fn gift(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    other: PlayerId,
    friendship: i32,
    treaty: Treaty,
    k: &DiplomacyBLevers,
) -> bool {
    let me = ai.player;
    let due = ai
        .memory
        .get()
        .due(other, Did::Gift, game.round(), k.gift_cool);
    if !due || treaty == Treaty::None || friendship < k.gift_friend {
        return false;
    }
    let spell = game
        .round()
        .is_multiple_of(3)
        .then(|| crate::trade::spell_to_give(game, me, other))
        .flatten();
    let gold = game.player(me).map_or(0, |p| p.treasury().gold.floor());
    let levers = game.levers().diplomacy;
    let command = match spell {
        Some(spell) => DiplomacyCommand::GiveSpell {
            from: me,
            to: other,
            spell,
        },
        None if gold >= levers.friend_gift_least_gold => DiplomacyCommand::GiveGold {
            from: me,
            to: other,
            amount: levers.gift_gold,
        },
        None => return false,
    };
    let given = ai.send(game, Command::Diplomacy(command), events).is_ok();
    if given {
        ai.memory.put().did(other, Did::Gift, game.round());
    }
    given
}

/// A trade offered now and then.
fn trade_offer(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    other: PlayerId,
    k: &DiplomacyBLevers,
) {
    if !ai
        .memory
        .get()
        .due(other, Did::Trade, game.round(), k.trade_cool)
    {
        return;
    }
    ai.memory.put().did(other, Did::Trade, game.round());
    if let Some(kind) = crate::trade::offer(game, &ai.minds(), ai.player, other) {
        propose(ai, game, events, other, kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplo_b_stance::tests::pair;
    use crate::testworld::{at, levers, spawn, P0, P1};
    use f4x_core::diplomacy::{DiplomacyEvent, Goods};
    use f4x_def::half_int::HalfInt;
    use f4x_master::Game;

    /// What `P0` feels about `P1`, set outright in the memory of the `Ai` the test holds for P0.
    fn feel(w: &mut Game, ai: &Ai, value: i32) {
        ai.memory.put().set_friendship(w, P0, P1, value);
    }

    fn verdict(w: &mut Game, ai: &Ai, kind: ProposalKind) -> Verdict {
        rate(w, P1, P0, kind, &ai.minds())
    }

    #[test]
    fn a_friendly_wizard_accepts_a_pact_and_a_hostile_one_does_not() {
        let ai = Ai::new(P0);
        let mut w = pair(8, 8, 100);
        assert_eq!(
            verdict(&mut w, &ai, ProposalKind::Pact),
            Verdict::Refused,
            "a stranger"
        );
        feel(&mut w, &ai, 80);
        assert_eq!(
            verdict(&mut w, &ai, ProposalKind::Pact),
            Verdict::Accepted,
            "a friend"
        );
        feel(&mut w, &ai, -200);
        assert_eq!(
            verdict(&mut w, &ai, ProposalKind::Pact),
            Verdict::Refused,
            "a foe"
        );
        assert_eq!(
            verdict(&mut w, &ai, ProposalKind::Alliance),
            Verdict::Refused
        );
    }

    #[test]
    fn an_alliance_needs_more_than_a_pact() {
        let ai = Ai::new(P0);
        let mut w = pair(8, 8, 100);
        feel(&mut w, &ai, 50);
        let pact = verdict(&mut w, &ai, ProposalKind::Pact);
        let alliance = verdict(&mut w, &ai, ProposalKind::Alliance);
        assert!(
            !(alliance == Verdict::Accepted && pact == Verdict::Refused),
            "{pact:?} {alliance:?}"
        );
    }

    #[test]
    fn the_answer_is_the_same_every_time_no_dice() {
        let ai = Ai::new(P0);
        let mut w = pair(8, 8, 100);
        feel(&mut w, &ai, 30);
        let first = verdict(&mut w, &ai, ProposalKind::Pact);
        for _ in 0..20 {
            assert_eq!(verdict(&mut w, &ai, ProposalKind::Pact), first);
        }
    }

    #[test]
    fn a_wizard_that_fears_the_asker_yields_to_a_demand_and_one_that_does_not_refuses() {
        let ai = Ai::new(P0);
        let mut w = pair(2, 40, 100);
        assert_eq!(
            verdict(&mut w, &ai, ProposalKind::Demand { gold: 50 }),
            Verdict::Accepted,
            "outclassed and near"
        );
        let mut w = pair(40, 2, 100);
        assert_eq!(
            verdict(&mut w, &ai, ProposalKind::Demand { gold: 50 }),
            Verdict::Refused,
            "the stronger one"
        );
    }

    #[test]
    fn a_threat_from_a_much_weaker_asker_is_answered_by_war() {
        let ai = Ai::new(P0);
        let mut w = pair(40, 2, 100);
        assert_eq!(verdict(&mut w, &ai, ProposalKind::Threat), Verdict::Refused);
        assert_eq!(
            w.treaty(P0, P1),
            Treaty::War,
            "it declared war on the threatener"
        );
    }

    /// The two seats `P1` may be: a computer player or the human. What `P0` does is the same.
    fn seats() -> [Option<PlayerId>; 2] {
        [None, Some(P1)]
    }

    fn seated(mut w: Game, human: Option<PlayerId>) -> Game {
        w.set_human(human);
        w
    }

    #[test]
    fn a_wizard_that_hates_another_declares_war_and_a_stranger_does_nothing() {
        for human in seats() {
            let ai = Ai::new(P0);
            let mut w = seated(pair(8, 8, 100), human);
            let mut events = Vec::new();
            approach(&ai, &mut w, &mut events);
            assert_ne!(w.treaty(P0, P1), Treaty::War, "{human:?}: a stranger");
            feel(&mut w, &ai, -200);
            approach(&ai, &mut w, &mut events);
            assert_eq!(w.treaty(P0, P1), Treaty::War, "{human:?}");
        }
    }

    #[test]
    fn a_close_friend_with_a_treaty_gives_gold_once_in_a_while() {
        for human in seats() {
            let ai = Ai::new(P0);
            let mut w = seated(pair(8, 8, 100), human);
            w.start_treaty(P0, P1, Treaty::Pact);
            feel(&mut w, &ai, 100);
            let rich = w.levers().diplomacy.friend_gift_least_gold;
            w.treasury_mut(P0).expect("a treasury").gold =
                f4x_def::half_int::HalfInt::from_int(2 * rich);
            let gifts = |events: &[Event]| {
                events
                    .iter()
                    .filter(|e| {
                        matches!(
                            e,
                            Event::Diplomacy(DiplomacyEvent::GoldGiven { from: P0, .. })
                        )
                    })
                    .count()
            };
            let mut events = Vec::new();
            approach(&ai, &mut w, &mut events);
            assert_eq!(gifts(&events), 1, "{human:?}: no gift");
            approach(&ai, &mut w, &mut events);
            assert_eq!(gifts(&events), 1, "{human:?}: a second gift at once");
        }
    }

    #[test]
    fn a_wizard_whose_friendship_fell_breaks_the_pact_with_either_kind_of_player() {
        for human in seats() {
            let ai = Ai::new(P0);
            let mut w = seated(pair(8, 8, 100), human);
            w.start_treaty(P0, P1, Treaty::Pact);
            feel(&mut w, &ai, -150);
            approach(&ai, &mut w, &mut Vec::new());
            assert_eq!(w.treaty(P0, P1), Treaty::None, "{human:?}: the pact stands");
        }
    }

    #[test]
    fn a_wizard_warns_a_partner_it_is_tiring_of_and_not_again_at_once() {
        for human in seats() {
            let ai = Ai::new(P0);
            let mut w = seated(pair(8, 8, 100), human);
            w.start_treaty(P0, P1, Treaty::Pact);
            // Down to between the marks for a warning and for breaking the treaty.
            let k = levers().diplomacy_b;
            let friendship = |w: &Game| {
                let scene = Scene::new(w, P0);
                regard(w, P0, P1, &scene, &ai.memory.get(), w.round()).friendship
            };
            feel(&mut w, &ai, k.warn_below - 1);
            assert!(
                friendship(&w) > k.break_below,
                "went past the mark for breaking"
            );
            let mut events = Vec::new();
            let warnings = |events: &[Event]| {
                events
                    .iter()
                    .filter(|e| matches!(e, Event::Diplomacy(DiplomacyEvent::Warned { .. })))
                    .count()
            };
            approach(&ai, &mut w, &mut events);
            assert_eq!(warnings(&events), 1, "{human:?}: no warning");
            approach(&ai, &mut w, &mut events);
            assert_eq!(warnings(&events), 1, "{human:?}: warned twice in a turn");
            w.set_round(w.round() + k.warn_cool);
            approach(&ai, &mut w, &mut events);
            assert_eq!(
                warnings(&events),
                2,
                "{human:?}: no second warning after the cool-down"
            );
        }
    }

    #[test]
    fn a_wizard_says_what_it_has_to_say_in_its_own_turn() {
        for human in seats() {
            let mut ai = Ai::new(P0);
            let mut w = seated(pair(8, 8, 100), human);
            w.start_treaty(P0, P1, Treaty::Pact);
            // Half way between the marks, so that what the turn does to the feeling stays between
            // them.
            let k = levers().diplomacy_b;
            feel(&mut w, &ai, (k.warn_below + k.break_below) / 2);
            let events = ai.play_turn(&mut w);
            let warned = events
                .iter()
                .any(|e| matches!(e, Event::Diplomacy(DiplomacyEvent::Warned { .. })));
            assert!(warned, "{human:?}: no warning in the turn: {events:?}");
        }
    }

    #[test]
    fn a_courted_player_is_asked_alike_and_only_the_human_is_waited_for() {
        let kinds: Vec<Vec<ProposalKind>> = seats()
            .into_iter()
            .map(|human| {
                let ai = Ai::new(P0);
                let mut w = seated(pair(8, 8, 100), human);
                feel(&mut w, &ai, 100);
                let mut events = Vec::new();
                approach(&ai, &mut w, &mut events);
                let waits = w.diplomacy.proposals().iter().any(|p| p.to == P1);
                assert_eq!(waits, human.is_some(), "{human:?}");
                events
                    .iter()
                    .filter_map(|e| match e {
                        Event::Diplomacy(DiplomacyEvent::ProposalMade(p)) if p.from == P0 => {
                            Some(p.kind)
                        }
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        assert_eq!(kinds[0], [ProposalKind::Pact]);
        assert_eq!(kinds[0], kinds[1]);
    }

    /// `P0` has had `P1`'s units next to its city up to the last warning, and judges.
    fn judged(friendship: i32) -> Game {
        let ai = Ai::new(P0);
        let mut w = pair(8, 8, 100);
        w.start_treaty(P0, P1, Treaty::Pact);
        spawn(&mut w, P1, at(11, 10), 104 /* high_men_spearmen */, 2);
        w.explore(P0, at(11, 10), HalfInt::from_raw(3));
        for _ in 0..w.content().rules.config.diplomacy.trespass_warnings {
            w.start_player_turn(P0);
        }
        assert_eq!(w.trespass_choices(P0), [P1], "no choice to make");
        feel(&mut w, &ai, friendship);
        judge_trespass(&ai, &mut w, &mut Vec::new());
        assert_eq!(w.trespass_choices(P0), [], "the choice is made");
        w
    }

    #[test]
    fn at_the_last_warning_a_wizard_overlooks_breaks_or_fights_by_its_friendship() {
        let k = levers().diplomacy_b;
        // The hidden regard is 0 in this world: the friendship is what is set.
        assert_eq!(
            judged(k.trespass_break_below + 20).treaty(P0, P1),
            Treaty::Pact
        );
        assert_eq!(
            judged(k.trespass_break_below - 1).treaty(P0, P1),
            Treaty::None
        );
        assert_eq!(judged(k.trespass_war_below - 1).treaty(P0, P1), Treaty::War);
    }

    /// `P1` proposes a pact to `P0`, which feels `friendship`, and `P0` answers.
    fn answered(friendship: i32) -> (Game, Vec<Event>) {
        let ai = Ai::new(P0);
        let mut w = pair(8, 8, 100);
        w.set_human(Some(P1));
        w.treasury_mut(P1).expect("a treasury").gold = HalfInt::from_int(1000);
        feel(&mut w, &ai, friendship);
        w.command(Command::Diplomacy(DiplomacyCommand::Propose {
            from: P1,
            to: P0,
            kind: ProposalKind::Pact,
        }));
        let events = crate::answer_proposals(&mut w, P0, &ai.minds());
        (w, events)
    }

    #[test]
    fn a_wizard_a_little_short_of_a_pact_names_its_price_and_takes_it() {
        // The friendships from a foe to a friend: refused, then priced, then accepted.
        let mut priced = 0;
        let mut seen = Vec::new();
        for friendship in (-100..=100).step_by(5) {
            let (mut w, _) = answered(friendship);
            let counter = w.diplomacy.proposals().first().copied();
            let outcome = match counter {
                Some(Proposal {
                    kind:
                        ProposalKind::Priced {
                            deal: Deal::Pact,
                            give,
                            get,
                        },
                    ..
                }) => {
                    assert!(give.is_empty() && !get.is_empty(), "{give:?} {get:?}");
                    let events = w.command(Command::Diplomacy(DiplomacyCommand::Answer {
                        player: P1,
                        from: P0,
                        accept: true,
                    }));
                    assert!(
                        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
                        "{events:?}"
                    );
                    assert_eq!(w.treaty(P0, P1), Treaty::Pact, "the price is paid");
                    priced += 1;
                    1
                }
                Some(other) => panic!("an odd counter: {other:?}"),
                None if w.treaty(P0, P1) == Treaty::Pact => 2,
                None => 0,
            };
            seen.push(outcome);
        }
        for outcome in [0, 1, 2] {
            assert!(seen.contains(&outcome), "never outcome {outcome}: {seen:?}");
        }
        assert!(priced > 0, "never a price: {seen:?}");
        assert!(
            seen.windows(2).all(|w| w[0] <= w[1]),
            "a better friend is never harder to win: {seen:?}"
        );
    }

    #[test]
    fn a_wizard_takes_a_price_that_makes_up_what_it_is_short_of() {
        let ai = Ai::new(P0);
        let k = levers().diplomacy_b;
        // The first friendship at which the plain pact is within reach but not accepted.
        let (friendship, short) = (-100..=100)
            .find_map(|f| {
                let mut w = pair(8, 8, 100);
                feel(&mut w, &ai, f);
                let short = shortfall(&w, P1, P0, Deal::Pact, &ai.minds());
                (short > 0 && short <= k.counter_reach).then_some((f, short))
            })
            .expect("a friendship a little short");
        let mut w = pair(8, 8, 100);
        feel(&mut w, &ai, friendship);
        let offer = |gold: i32| ProposalKind::Priced {
            deal: Deal::Pact,
            give: Goods::gold(gold),
            get: Goods::NONE,
        };
        let enough = short * k.gold_per_point;
        assert_eq!(
            rate(&mut w, P1, P0, offer(enough), &ai.minds()),
            Verdict::Accepted
        );
        assert_eq!(
            rate(&mut w, P1, P0, offer(enough - 1), &ai.minds()),
            Verdict::Refused
        );
    }
}
