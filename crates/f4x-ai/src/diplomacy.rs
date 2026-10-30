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
        // A threat has its own rule, a demand is the human's to answer.
        ProposalKind::Threat => (
            0,
            Some(if answer_threat(game, judge, asker) {
                Verdict::Accepted
            } else {
                Verdict::Refused
            }),
        ),
        ProposalKind::Demand { .. } => (0, Some(Verdict::Refused)),
    };
    if let Some(v) = early {
        return v;
    }
    let stance = game
        .diplomacy
        .stance(asker, judge)
        .cloned()
        .unwrap_or_default();
    let relations = stance.treaty_patience
        + stance.hidden
        + relation
        + crate::objective::temper(game.personality(judge), game.difficulty()).modifier();
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
        let fights = !w.is_banished()
            && crate::objective::picks_fights(
                w.objective(),
                crate::objective::temper(w.personality(), view.difficulty()),
            );
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

/// A relation at which a wizard declares war on the human whatever the treaty (`DIPL_HumanWarOrPeace`).
const WAR_RELATION: i32 = -75;
/// A relation under which a wizard breaks a pact or an alliance it has with the human.
const BREAK_RELATION: i32 = -50;
/// The chance, in a hundred, of a turn in which a friendly wizard offers the human a better treaty.
const OFFER_PERCENT: u32 = 12;

/// What an AI wizard says to the human at the start of the human's turn, as commands a human
/// could send: it offers peace when at war and in the mood (`DIPL_HumanWarOrPeace`, one turn in
/// eight), declares war on a hated one, breaks a treaty with a hated partner, offers a pact or an
/// alliance to a friend, and asks an ally to join its war (`NPC_Required_Alliances`, one turn in
/// ten). The human answers the proposals in a box; they lapse when the turn ends. The numbers are
/// the original's where `docs/diplomacy.md` section 8 gives them, and a plain guess elsewhere.
pub(crate) fn approach_human(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let Some(human) = game.human().filter(|&h| h != me) else {
        return;
    };
    // Nobody makes overtures to a banished human (task 341) or to one who is out of the game.
    if !game.have_met(me, human)
        || game.player(human).is_none_or(|p| p.is_defeated())
        || game.wizard(human).is_none_or(|w| w.is_banished())
    {
        return;
    }
    let (can_talk, third_parties) = {
        let view = PlayerView::new(game, me);
        let thirds: Vec<PlayerId> = game
            .player_ids()
            .into_iter()
            .filter(|&p| p != me && p != human && view.is_rival_wizard(p))
            .filter(|&p| game.have_met(me, p) && game.have_met(human, p))
            .collect();
        (
            view.wizard().is_some_and(|w| !w.is_banished()) && !view.is_defeated(),
            thirds,
        )
    };
    if !can_talk {
        return;
    }
    let relation = game.relation(me, human);
    let mood = crate::objective::temper(game.personality(me), game.difficulty()).modifier();
    let stance = game
        .diplomacy
        .stance(me, human)
        .cloned()
        .unwrap_or_default();
    let propose = |game: &mut Game, events: &mut Vec<Event>, kind| {
        ai.send(
            game,
            Command::Diplomacy(DiplomacyCommand::Propose {
                from: me,
                to: human,
                kind,
            }),
            events,
        )
    };
    match game.treaty(me, human) {
        Treaty::War => {
            let score = stance.hidden + relation + mood + stance.peace_patience;
            if game.roll(8) == 1 && 30 + game.roll(100) as i32 <= score {
                propose(game, events, ProposalKind::Peace);
            }
        }
        treaty => {
            if relation <= WAR_RELATION && treaty == Treaty::None {
                ai.send(
                    game,
                    Command::Diplomacy(DiplomacyCommand::DeclareWar {
                        player: me,
                        on: human,
                    }),
                    events,
                );
                return;
            }
            if relation <= BREAK_RELATION && treaty != Treaty::None {
                ai.send(
                    game,
                    Command::Diplomacy(DiplomacyCommand::BreakTreaty {
                        player: me,
                        with: human,
                    }),
                    events,
                );
                return;
            }
            if quarrel_with_human(ai, game, events, human, &stance, relation, mood, treaty) {
                return;
            }
            // An ally at war asks the human to join it.
            if treaty == Treaty::Alliance && game.roll(10) == 1 {
                let enemy = third_parties.iter().copied().find(|&t| {
                    game.treaty(me, t) == Treaty::War && game.treaty(human, t) != Treaty::War
                });
                if let Some(enemy) = enemy {
                    propose(game, events, ProposalKind::WarOn(enemy));
                    return;
                }
            }
            let better = match treaty {
                Treaty::None if relation > PACT_RELATION + 10 => Some(ProposalKind::Pact),
                Treaty::Pact if relation > ALLIANCE_RELATION + 10 => Some(ProposalKind::Alliance),
                _ => None,
            };
            if let Some(kind) = better {
                if game.roll(100)
                    <= OFFER_PERCENT + relation.max(0) as u32 / 10 + (mood / 10) as u32
                {
                    propose(game, events, kind);
                    return;
                }
            }
            if need_for_war(ai, game, events, human, relation, mood, treaty) {
                return;
            }
            demand(ai, game, events, human, relation, treaty);
            gift(ai, game, events, human, relation, treaty);
        }
    }
}

/// How often (one in this) a wizard that is stronger and does not like the human demands tribute.
const DEMAND_ONE_IN: u32 = 14;
/// How often (one in this) a close friend gives the human a gift.
const GIFT_ONE_IN: u32 = 30;
/// The turn before which no wizard picks a war out of need (`G_DIPL_NeedForWar`).
const NEED_FOR_WAR_FROM_TURN: u32 = 100;

/// The warnings before a war (`DIPL_HumanWarOrPeace`): a wizard whose patience with the human is
/// gone (`hidden + relation + mood - Random(100) + patience <= -100`) warns a partner once; after
/// that, or with no treaty at all, it declares war with a chance of the dislike less half the
/// mood, else it breaks the treaty it has, else it warns again. Returns whether it did something.
#[allow(clippy::too_many_arguments)]
fn quarrel_with_human(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    human: PlayerId,
    stance: &f4x_core::diplomacy::Stance,
    relation: i32,
    mood: i32,
    treaty: Treaty,
) -> bool {
    let me = ai.player;
    let score = stance.hidden + relation + mood - game.roll(100) as i32 + stance.treaty_patience;
    if score > -100 {
        return false;
    }
    let warn = |game: &mut Game, events: &mut Vec<Event>| {
        ai.send(
            game,
            Command::Diplomacy(DiplomacyCommand::Warn {
                from: me,
                to: human,
            }),
            events,
        );
    };
    if treaty != Treaty::None && stance.warnings == 0 {
        warn(game, events);
        return true;
    }
    let dislike = (-relation).max(0);
    if (game.roll(100) as i32) < dislike - mood / 2 {
        ai.send(
            game,
            Command::Diplomacy(DiplomacyCommand::DeclareWar {
                player: me,
                on: human,
            }),
            events,
        );
    } else if treaty != Treaty::None {
        ai.send(
            game,
            Command::Diplomacy(DiplomacyCommand::BreakTreaty {
                player: me,
                with: human,
            }),
            events,
        );
    } else {
        warn(game, events);
    }
    true
}

/// A war picked out of need (`G_DIPL_NeedForWar`), against a human it has no treaty with and after
/// turn 100: a Chaotic wizard now and then; a wizard that is stronger and dislikes the human
/// (a d20 roll of 1, then `relation + mood + hidden - superiority <= -150`); one whose ally is at war
/// with the human (one turn in ten); and, from difficulty 2, one that dislikes the human while the
/// human is at war with few (`Random(15) <= -relation / 10`).
fn need_for_war(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    human: PlayerId,
    relation: i32,
    mood: i32,
    treaty: Treaty,
) -> bool {
    let me = ai.player;
    if treaty != Treaty::None || game.turn < NEED_FOR_WAR_FROM_TURN {
        return false;
    }
    let difficulty = u32::from(game.difficulty());
    let hidden = game.diplomacy.stance(me, human).map_or(0, |s| s.hidden);
    let (mine, theirs) = (army_strength(game, me), army_strength(game, human));
    let superiority = (mine - theirs) * 100 / (mine + theirs).max(1);
    let at_war_with_human = |game: &Game, w: PlayerId| game.treaty(w, human) == Treaty::War;
    let allies_at_war = game.player_ids().into_iter().any(|w| {
        w != me
            && w != human
            && game.treaty(me, w) == Treaty::Alliance
            && at_war_with_human(game, w)
    });
    let human_wars = game
        .player_ids()
        .into_iter()
        .filter(|&w| w != human && PlayerView::new(game, me).is_rival_wizard(w))
        .filter(|&w| at_war_with_human(game, w))
        .count() as u32;
    let war = (game.personality(me) == f4x_core::wizard::objective::Personality::Chaotic
        && game.roll(300) <= difficulty)
        || (game.roll(20) == 1 && relation + mood + hidden - superiority <= -150)
        || (allies_at_war && game.roll(10) == 1)
        || (difficulty >= 2
            && human_wars < difficulty
            && relation < -30
            && game.roll(15) - 1 <= (-relation / 10) as u32);
    if war {
        ai.send(
            game,
            Command::Diplomacy(DiplomacyCommand::DeclareWar {
                player: me,
                on: human,
            }),
            events,
        );
    }
    war
}

/// A demand for tribute from a stronger wizard that does not like the human: a quarter of the
/// human's gold, at most `25 * (2..9)`, in whole 25s.
fn demand(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    human: PlayerId,
    relation: i32,
    treaty: Treaty,
) {
    let me = ai.player;
    let gold = game.player(human).map_or(0, |p| p.treasury().gold);
    if treaty == Treaty::Alliance
        || relation >= -20
        || gold < 100
        || army_strength(game, me) <= army_strength(game, human)
        || game.roll(DEMAND_ONE_IN) != 1
    {
        return;
    }
    let asked = (25 * (2 + game.roll(8) as i32)).min(gold / 4 / 25 * 25);
    if asked >= 25 {
        ai.send(
            game,
            Command::Diplomacy(DiplomacyCommand::Propose {
                from: me,
                to: human,
                kind: ProposalKind::Demand { gold: asked },
            }),
            events,
        );
    }
}

/// A gift from a close friend: a spell the human lacks, or gold from a full treasury.
fn gift(
    ai: &Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    human: PlayerId,
    relation: i32,
    treaty: Treaty,
) {
    let me = ai.player;
    if treaty == Treaty::None || relation < 60 || game.roll(GIFT_ONE_IN) != 1 {
        return;
    }
    if game.roll(3) == 1 {
        if let Some(ProposalKind::Exchange { give, .. }) = spell_swap(game, me, human) {
            ai.send(
                game,
                Command::Diplomacy(DiplomacyCommand::GiveSpell {
                    from: me,
                    to: human,
                    spell: give,
                }),
                events,
            );
            return;
        }
    }
    let gold = game.player(me).map_or(0, |p| p.treasury().gold);
    if gold >= 200 {
        let amount = 25 * (1 + game.roll(4) as i32);
        ai.send(
            game,
            Command::Diplomacy(DiplomacyCommand::GiveGold {
                from: me,
                to: human,
                amount,
            }),
            events,
        );
    }
}

/// How a wizard answers a threat of the human (`Threaten to Attack`, `docs/diplomacy.md` section 4):
/// `hidden + relation + mood + threat + Random(100) + Random(100)`; below 0 it declares war; 0 to
/// 89 it refuses; 90 to 199 it pays `25 * (2..9)` gold (at most what it has) and yields; 200 or
/// more it gives a spell instead and yields. Returns whether the wizard yields.
fn answer_threat(game: &mut Game, judge: PlayerId, asker: PlayerId) -> bool {
    let stance = game
        .diplomacy
        .stance(asker, judge)
        .cloned()
        .unwrap_or_default();
    let relation = game.relation(asker, judge);
    let mood = crate::objective::temper(game.personality(judge), game.difficulty()).modifier();
    let (mine, theirs) = (army_strength(game, judge), army_strength(game, asker));
    // The human's side of the scale: a hopeless army either way is worth 100.
    let threat = if mine == 0 {
        100
    } else if theirs == 0 {
        -100
    } else {
        ((theirs - mine) * 100 / mine.max(theirs)).clamp(-100, 100)
    };
    let score =
        stance.hidden + relation + mood + threat + game.roll(100) as i32 + game.roll(100) as i32;
    if score < 0 {
        game.apply(Command::Diplomacy(DiplomacyCommand::DeclareWar {
            player: judge,
            on: asker,
        }));
        false
    } else if score < 90 {
        false
    } else if score < 200 {
        let gold = game.player(judge).map_or(0, |p| p.treasury().gold);
        let pay = (25 * (2 + game.roll(8) as i32)).min(gold / 25 * 25);
        if pay > 0 {
            game.apply(Command::Diplomacy(DiplomacyCommand::GiveGold {
                from: judge,
                to: asker,
                amount: pay,
            }));
        }
        true
    } else {
        let spell = spell_swap(game, judge, asker).and_then(|k| match k {
            ProposalKind::Exchange { give, .. } => Some(give),
            _ => None,
        });
        if let Some(spell) = spell {
            game.apply(Command::Diplomacy(DiplomacyCommand::GiveSpell {
                from: judge,
                to: asker,
                spell,
            }));
        }
        true
    }
}

/// The AI wizards among themselves (`NPC_To_NPC_Treaty_Negotiations`): for each wizard it has
/// met, with a chance of one in `15 - 2 * difficulty`, the wizard sizes the other up (relation,
/// temper, the difficulty and a roll) and offers an alliance or a pact, a swap of spells, or
/// peace; an ally joins the wars of its friends. The other wizard answers at once with its own
/// rating of the offer (`rate`).
pub(crate) fn negotiate(ai: &Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let human = game.human();
    let others: Vec<PlayerId> = {
        let view = PlayerView::new(game, me);
        if view.wizard().is_none_or(|w| w.is_banished()) || view.is_defeated() {
            return;
        }
        game.player_ids()
            .into_iter()
            .filter(|&p| p != me && Some(p) != human && view.is_rival_wizard(p))
            .filter(|&p| game.have_met(me, p))
            .filter(|&p| game.player(p).is_some_and(|q| !q.is_defeated()))
            .collect()
    };
    let difficulty = u32::from(game.difficulty());
    for other in others {
        if game.roll(15 - 2 * difficulty.min(4)) != 1 {
            continue;
        }
        let treaty = game.treaty(me, other);
        let relation = game.relation(me, other);
        let mood = crate::objective::temper(game.personality(me), game.difficulty()).modifier();
        let stance = game
            .diplomacy
            .stance(me, other)
            .cloned()
            .unwrap_or_default();
        let score = stance.hidden
            + relation
            + mood
            + game.roll(100) as i32
            + (3 + game.roll(5) as i32) * difficulty as i32;
        let kind = match treaty {
            Treaty::War => {
                (stance.peace_patience + game.roll(100) as i32 > 100).then_some(ProposalKind::Peace)
            }
            Treaty::None if score > 150 && relation > ALLIANCE_RELATION => {
                Some(ProposalKind::Alliance)
            }
            Treaty::None if score > 110 && relation > PACT_RELATION => Some(ProposalKind::Pact),
            Treaty::Pact if score > 150 && relation > ALLIANCE_RELATION => {
                Some(ProposalKind::Alliance)
            }
            _ => None,
        };
        let kind = kind.or_else(|| {
            (treaty != Treaty::War && stance.exchange_patience + 80 < score)
                .then(|| spell_swap(game, me, other))
                .flatten()
        });
        if let Some(kind) = kind {
            let made = ai.send(
                game,
                Command::Diplomacy(DiplomacyCommand::Propose {
                    from: me,
                    to: other,
                    kind,
                }),
                events,
            );
            if made {
                events.extend(crate::answer_proposals(game, other));
            }
        }
        // An ally joins the wars of its friend.
        if game.treaty(me, other) == Treaty::Alliance && game.roll(10) == 1 {
            let enemy = game.player_ids().into_iter().find(|&t| {
                t != me
                    && t != other
                    && game.treaty(other, t) == Treaty::War
                    && game.treaty(me, t) != Treaty::War
                    && game.treaty(me, t) != Treaty::Alliance
                    && PlayerView::new(game, me).is_rival_wizard(t)
                    && game.have_met(me, t)
            });
            if let Some(enemy) = enemy {
                ai.send(
                    game,
                    Command::Diplomacy(DiplomacyCommand::DeclareWar {
                        player: me,
                        on: enemy,
                    }),
                    events,
                );
            }
        }
    }
}

/// A swap of spells `me` could offer `other`: the cheapest spell each has and the other lacks.
fn spell_swap(game: &Game, me: PlayerId, other: PlayerId) -> Option<ProposalKind> {
    let (mine, theirs) = (game.wizard(me)?, game.wizard(other)?);
    let cheapest = |from: &dyn Fn(f4x_core::rules::SpellId) -> bool,
                    to: &dyn Fn(f4x_core::rules::SpellId) -> bool| {
        game.content()
            .rules
            .spells
            .iter()
            .filter(|&(id, _)| from(id) && !to(id))
            .min_by_key(|&(_, def)| (def.research_cost, def.name.clone()))
            .map(|(id, _)| id)
    };
    let give = cheapest(&|id| mine.knows(id), &|id| theirs.knows(id))?;
    let get = cheapest(&|id| theirs.knows(id), &|id| mine.knows(id))?;
    Some(ProposalKind::Exchange { give, get })
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::diplomacy::{DiplomacyEvent, ProposalKind};
    use f4x_core::player::PlayerId;
    use f4x_core::wizard::objective::{Objective, Personality};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const HUMAN: PlayerId = PlayerId(0);
    const AI: PlayerId = PlayerId(1);

    impl Ai {
        fn send_approach(&self, g: &mut Game) {
            approach_human(self, g, &mut Vec::new());
        }
    }

    /// A human and an AI wizard who have met, on the shipped content.
    fn game_seeded(seed: u64) -> Game {
        let mut g = Game::new(seed);
        g.set_content(crate::testworld::content());
        for id in [HUMAN, AI] {
            g.add_player(id, "W");
        }
        g.set_human(Some(HUMAN));
        g.make_contact(HUMAN, AI);
        g
    }

    fn game() -> Game {
        game_seeded(5)
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
    fn rivals(
        objective: Objective,
        temper: Personality,
    ) -> Result<Game, Box<dyn std::error::Error>> {
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
        let mut w = g.wizard_mut(P0).ok_or("the west wizard exists")?;
        w.put(Some(objective));
        w.put(Some(temper));
        Ok(g)
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
    fn a_fierce_militarist_declares_war_on_a_weaker_neighbour_in_time() -> TestResult {
        let mut g = rivals(Objective::Militarist, Personality::Aggressive)?;
        assert!(declares_war(&mut g, 400));
        Ok(())
    }

    #[test]
    fn a_lawful_or_peaceful_wizard_never_starts_a_war() -> TestResult {
        for temper in [Personality::Lawful, Personality::Peaceful] {
            let mut g = rivals(Objective::Militarist, temper)?;
            assert!(!declares_war(&mut g, 400), "{temper:?}");
        }
        Ok(())
    }

    #[test]
    fn a_theurgist_never_starts_a_war() -> TestResult {
        let mut g = rivals(Objective::Theurgist, Personality::Maniacal)?;
        assert!(!declares_war(&mut g, 400));
        Ok(())
    }

    #[test]
    fn nobody_attacks_a_neighbour_that_holds_as_many_cities() -> TestResult {
        use crate::testworld::{at, ENEMY_CITY, P1};
        let mut g = rivals(Objective::Expansionist, Personality::Ruthless)?;
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
        Ok(())
    }

    #[test]
    fn a_pact_is_refused_out_of_hand_below_the_threshold_and_the_rating_needs_no_patience(
    ) -> TestResult {
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
        let (spell, _) = g
            .content()
            .rules
            .spells
            .iter()
            .next()
            .ok_or("the shipped content has spells")?;
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
        Ok(())
    }

    #[test]
    fn a_bigger_army_makes_a_wizard_more_pliant() -> TestResult {
        use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
        let mut g = game(COAST_CITY, "high_men");
        let units = &g.content().rules.units;
        let spearmen = units
            .id_of("high_men_spearmen")
            .ok_or("high_men_spearmen is shipped")?;
        let cost = units.get(spearmen).cost as i32;
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
        Ok(())
    }

    #[test]
    fn a_wizard_that_has_met_a_mastery_caster_declares_war_on_it_in_time() -> TestResult {
        use crate::testworld::{game, COAST_CITY, P0, P1};
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let mastery = g
            .content()
            .rules
            .spells
            .id_of("spell_of_mastery")
            .ok_or("spell_of_mastery is shipped")?;
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
        Ok(())
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
    fn a_friendly_wizard_accepts_a_pact_and_a_hostile_one_does_not() -> TestResult {
        let mut yes = 0;
        let mut no = 0;
        for seed in 0..40 {
            let mut g = game_seeded(seed);
            // Friends: the human has done well by the AI, and the AI is easy-going.
            for _ in 0..8 {
                g.change_relations(8, HUMAN, AI, f4x_core::diplomacy::Action::Friendship, None);
            }
            g.wizard_mut(AI)
                .ok_or("the AI wizard exists")?
                .put(Some(f4x_core::wizard::objective::Personality::Peaceful));
            if rate(&mut g, HUMAN, AI, ProposalKind::Pact) == Verdict::Accepted {
                yes += 1;
            }
            let mut h = game_seeded(seed);
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
        Ok(())
    }

    /// The wizard `AI` likes (or hates) the human by `value`, in steps the original's damping
    /// lets through.
    fn feeling(g: &mut Game, value: i32) {
        use f4x_core::diplomacy::Action;
        for _ in 0..40 {
            g.change_relations(value, HUMAN, AI, Action::Friendship, None);
        }
    }

    #[test]
    fn a_friendly_wizard_offers_the_human_a_pact_in_time() -> TestResult {
        let mut g = game();
        feeling(&mut g, 30);
        assert!(g.relation(HUMAN, AI) > PACT_RELATION + 10);
        let ai = Ai::new(AI);
        let mut offered = None;
        for _ in 0..300 {
            ai.send_approach(&mut g);
            offered = PlayerView::new(&g, HUMAN)
                .proposals_to_me()
                .first()
                .copied();
            if offered.is_some() {
                break;
            }
        }
        let offered = offered.ok_or("the friendly wizard made an offer")?;
        assert_eq!((offered.from, offered.kind), (AI, ProposalKind::Pact));
        Ok(())
    }

    #[test]
    fn a_wizard_that_hates_the_human_declares_war_and_a_stranger_does_nothing() {
        let mut g = game();
        let ai = Ai::new(AI);
        for _ in 0..100 {
            ai.send_approach(&mut g);
        }
        assert_eq!(g.treaty(HUMAN, AI), Treaty::None);
        assert!(g.diplomacy.proposals().is_empty());
        feeling(&mut g, -30);
        assert!(g.relation(HUMAN, AI) <= WAR_RELATION);
        ai.send_approach(&mut g);
        assert_eq!(g.treaty(HUMAN, AI), Treaty::War);
    }

    #[test]
    fn a_wizard_whose_patience_is_gone_warns_or_goes_to_war() -> TestResult {
        let mut g = game();
        g.start_treaty(HUMAN, AI, Treaty::Pact);
        // Breaking a pact leaves every patience at -200.
        g.break_treaties(HUMAN, AI);
        let ai = Ai::new(AI);
        for _ in 0..50 {
            ai.send_approach(&mut g);
            if g.treaty(HUMAN, AI) == Treaty::War {
                return Ok(());
            }
        }
        let stance = g.diplomacy.stance(AI, HUMAN).ok_or("the AI has a stance")?;
        assert!(stance.warnings > 0);
        Ok(())
    }

    #[test]
    fn a_threat_is_answered_by_war_a_refusal_or_a_payment() -> TestResult {
        let mut outcomes = std::collections::BTreeSet::new();
        for seed in 0..80u64 {
            let mut g = game_seeded(seed);
            g.treasury_mut(AI).ok_or("the AI has a treasury")?.gold = 500;
            g.apply(Command::Diplomacy(DiplomacyCommand::Propose {
                from: HUMAN,
                to: AI,
                kind: ProposalKind::Threat,
            }));
            crate::answer_proposals(&mut g, AI);
            let paid = g.player(AI).ok_or("the AI exists")?.treasury().gold < 500;
            outcomes.insert((g.treaty(HUMAN, AI) == Treaty::War, paid));
        }
        assert!(outcomes.len() >= 2, "{outcomes:?}");
        Ok(())
    }

    #[test]
    fn a_wizard_without_an_army_demands_nothing() -> TestResult {
        let mut g = game();
        feeling(&mut g, -3);
        g.treasury_mut(HUMAN)
            .ok_or("the human has a treasury")?
            .gold = 400;
        let ai = Ai::new(AI);
        let relation = g.relation(AI, HUMAN);
        for _ in 0..200 {
            demand(&ai, &mut g, &mut Vec::new(), HUMAN, relation, Treaty::None);
        }
        assert!(PlayerView::new(&g, HUMAN).proposals_to_me().is_empty());
        Ok(())
    }

    #[test]
    fn ai_wizards_that_like_each_other_make_a_treaty_in_time() {
        use crate::testworld::{game, COAST_CITY, P0, P1};
        use f4x_core::diplomacy::Action;
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        for _ in 0..40 {
            g.change_relations(30, P0, P1, Action::Friendship, None);
        }
        assert!(g.relation(P0, P1) > ALLIANCE_RELATION);
        let ai = Ai::new(P0);
        for _ in 0..600 {
            negotiate(&ai, &mut g, &mut Vec::new());
            if g.treaty(P0, P1) != Treaty::None {
                break;
            }
        }
        assert!(matches!(g.treaty(P0, P1), Treaty::Pact | Treaty::Alliance));
        assert!(
            g.diplomacy.proposals().is_empty(),
            "the other wizard answered at once"
        );
    }
}
