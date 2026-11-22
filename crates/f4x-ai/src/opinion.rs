//! What a wizard feels about each of the others (task 780, `docs/diplomacy.md` section 5).
//!
//! The AI owns its opinions: the core only says what happened (events) and the AI changes its own
//! rating by the table of `Change_Relations`, with no dice (every `Random(n)` of the original is its
//! average). The opinion of the human is the AI's feeling for the human; what the human is shown.
//!
//! Until task 775 removes the core's table, a first opinion is seeded from it (which carries an old save
//! over); a game that has no pair yet starts from the books of the two wizards, as the original does.

use f4x_core::diplomacy::{Action, Treaty};
use f4x_core::diplomacy::{DiplomacyEvent, ProposalKind};
use f4x_core::game::{Event, Game};
use f4x_core::player::PlayerId;
use f4x_core::rules::{RealmId, Stat};
use f4x_core::wizard::objective::Personality;
use serde::{Deserialize, Serialize};

use crate::memory::Memory;

/// What one wizard feels about another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Opinion {
    pub with: PlayerId,
    /// How friendly it feels, -100..=100: what the human is shown.
    pub friendship: i32,
    /// What `friendship` drifts back to.
    pub default: i32,
    /// The lasting part: oaths broken, spells given. Not shown.
    pub hidden: i32,
    /// This one broke a treaty with it.
    pub broken: bool,
    /// The two are at war (the opinion was last moved by a war).
    pub war: bool,
}

/// How a book of realm `x` stands to a book of realm `y` on the colour wheel (kin, next to it, opposed).
fn wheel_affinity(x: RealmId, y: RealmId) -> i32 {
    // The realms' numbers in wheel order: chaos, nature, life, sorcery, death.
    const WHEEL: [u16; 5] = [2, 0, 3, 1, 4];
    let place = |r: RealmId| WHEEL.iter().position(|&n| n == r.num());
    let (Some(i), Some(j)) = (place(x), place(y)) else {
        return 0;
    };
    match (i + 5 - j) % 5 {
        0 => 2,
        1 | 4 => 1,
        _ => -3,
    }
}

/// The feeling two wizards start with, from their books (`Init_Diplomatic_Relations`), or from the
/// colour wheel when the setting `color_wheel_relations` is on.
pub(crate) fn start_feeling(game: &Game, a: PlayerId, b: PlayerId) -> i32 {
    let books = |p: PlayerId, r: RealmId| -> i32 { game.wizard(p).map_or(0, |m| m.books_in(r)) };
    if game.settings().rules.color_wheel_relations {
        let realms = [2u16, 0, 3, 1, 4].map(RealmId::new);
        let score: i32 = realms
            .iter()
            .flat_map(|&x| realms.iter().map(move |&y| (x, y)))
            .map(|(x, y)| books(a, x).min(books(b, y)) * wheel_affinity(x, y))
            .sum();
        return score.max(-90);
    }
    let (death, life, chaos) = (RealmId::new(4), RealmId::new(3), RealmId::new(2));
    let max_death = books(a, death).max(books(b, death));
    let sum_life = books(a, life) + books(b, life);
    let max_chaos = books(a, chaos).max(books(b, chaos));
    let mut score = if sum_life > 0 && max_death > 0 {
        -(sum_life + max_death) * 5
    } else {
        sum_life * 2 - max_death * 3
    };
    score -= max_chaos * 2;
    for realm in [RealmId::new(1), chaos, RealmId::new(0), life] {
        score += books(a, realm).min(books(b, realm)) * 2;
    }
    score.max(-90)
}

impl Memory {
    /// What `me` feels about `other`: the opinion it has kept, or the one it would start with.
    pub(crate) fn opinion(&self, game: &Game, me: PlayerId, other: PlayerId) -> Opinion {
        self.opinions
            .iter()
            .find(|o| o.with == other)
            .copied()
            .unwrap_or_else(|| first_opinion(game, me, other))
    }

    /// How friendly `me` feels about `other`, -100..=100.
    pub(crate) fn friendship(&self, game: &Game, me: PlayerId, other: PlayerId) -> i32 {
        self.opinion(game, me, other).friendship
    }

    fn slot(&mut self, game: &Game, me: PlayerId, other: PlayerId) -> &mut Opinion {
        let i = match self.opinions.iter().position(|o| o.with == other) {
            Some(i) => i,
            None => {
                self.opinions.push(first_opinion(game, me, other));
                self.opinions.len() - 1
            }
        };
        &mut self.opinions[i]
    }

    /// Set the friendship outright (the tests; the client's demo helpers, task 784).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn set_friendship(&mut self, game: &Game, me: PlayerId, other: PlayerId, to: i32) {
        self.slot(game, me, other).friendship = to.clamp(-100, 100);
    }

    /// What `from` did moves `me`'s feeling about it (`Change_Relations`): worse after a better one
    /// (diminishing returns), doubled for a Lawful wizard, and so on. `from` and `me` must have met.
    pub(crate) fn change(
        &mut self,
        game: &Game,
        me: PlayerId,
        from: PlayerId,
        value: i32,
        action: Action,
    ) {
        if value == 0 || from == me || !game.have_met(me, from) {
            return;
        }
        let rel = self.friendship(game, me, from);
        let mut v = value;
        if v > 0 {
            if rel < 0 {
                v *= 2;
                if rel + v > 10 {
                    v = 10 - rel;
                }
            } else {
                v /= rel / 25 + 1;
            }
        } else if rel > 0 {
            v *= 2;
        } else {
            v /= -rel / 25 + 1;
        }
        match game.personality(me) {
            Personality::Lawful if v < 0 => v *= 2,
            Personality::Maniacal if v < 0 => v = v * 3 / 2,
            Personality::Maniacal if v > 0 => v = v * 3 / 4,
            _ => {}
        }
        if let Some(m) = game.wizard(from) {
            let rules = &game.content().rules;
            if v > 0 {
                v = v * m.stat(rules, Stat::DiplomacyGainPercent, 100) / 100;
            } else if v < 0 {
                v = v * m.stat(rules, Stat::DiplomacyLossPercent, 100) / 100;
            }
        }
        let treaty = game.treaty(me, from);
        let o = self.slot(game, me, from);
        // Wars are not made worse by what one does in them.
        if !(treaty == Treaty::War && action != Action::Trespass && v < 0) {
            o.friendship = (o.friendship + v).clamp(-100, 100);
        }
        if treaty == Treaty::War {
            o.friendship = o.friendship.min(-25);
        }
        if treaty != Treaty::Alliance {
            o.friendship = o.friendship.min(65);
        }
    }

    /// What an event does to `me`'s feelings (task 780): the facts the core reports, which the AI weighs by
    /// the table of `docs/diplomacy.md` section 5. (The rest of the table comes with 782 and 783.)
    pub(crate) fn feel(&mut self, game: &Game, me: PlayerId, event: &Event) {
        match *event {
            Event::CityCaptured { from, to, .. } if from == me => {
                self.change(game, me, to, -40, Action::AttackedCity);
            }
            Event::Diplomacy(DiplomacyEvent::WarDeclared { attacker, defender }) => {
                if me == attacker {
                    self.war_began(game, me, defender);
                } else if me == defender {
                    self.war_began(game, me, attacker);
                }
            }
            Event::Diplomacy(DiplomacyEvent::TreatyChanged { a, b, treaty })
                if treaty != Treaty::War =>
            {
                if treaty == Treaty::None && (me == a || me == b) {
                    self.war_ended(game, me, if me == a { b } else { a });
                }
            }
            Event::Diplomacy(DiplomacyEvent::TreatyBroken {
                breaker,
                victim,
                treaty,
            }) if me != breaker => {
                let mut penalty = match treaty {
                    Treaty::Pact => 10,
                    Treaty::Alliance => 20,
                    _ => 0,
                };
                if penalty == 0 {
                    return;
                }
                if game.personality(breaker) == Personality::Lawful {
                    penalty *= 2;
                }
                if me == victim {
                    let o = self.slot(game, me, breaker);
                    o.hidden = (o.hidden - penalty).max(-100);
                    // The original takes `Random(20)` off the relation: its average.
                    o.friendship = (o.friendship - 10).max(-100);
                    if treaty == Treaty::Alliance {
                        o.default = (o.default - penalty).max(-100);
                    }
                    o.broken = true;
                } else {
                    // Every other wizard thinks less of the oath-breaker.
                    let o = self.slot(game, me, breaker);
                    o.hidden = (o.hidden - 5).max(-100);
                }
            }
            Event::Diplomacy(DiplomacyEvent::GoldGiven { from, to, gold }) if to == me => {
                // A larger share of what it holds, the warmer the thanks; the original rolls
                // `Random(3)` (0..2) over ten: one tenth, rounded.
                let reserve = game
                    .player(me)
                    .map_or(0, |p| p.treasury().gold.floor() - gold)
                    .max(1);
                let choice = ((gold * 4 + reserve - 1) / reserve).clamp(1, 4);
                let base = if game.army_strength(me) > 0 {
                    choice * 4
                } else {
                    8
                };
                let o = self.slot(game, me, from);
                o.friendship = (o.friendship + (base + 5) / 10).min(100);
            }
            Event::Diplomacy(DiplomacyEvent::SpellGiven { from, to, .. }) if to == me => {
                let rel = self.friendship(game, me, from);
                let base = if rel < 0 { 20 } else { (100 - rel) / 10 };
                // `base * (Random(8) + Random(8)) / 10`, the two rolls at their average of 3.5; and
                // the hidden regard `2 + Random(8)`.
                let o = self.slot(game, me, from);
                o.friendship = (o.friendship + base * 7 / 10).min(100);
                o.hidden = (o.hidden + 5).min(30);
            }
            Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
                proposal,
                accepted,
                lapsed: false,
            }) => match proposal.kind {
                // Turning a demand down sours the one who made it (`Random(10) + 1`).
                ProposalKind::Demand { .. } if !accepted && proposal.from == me => {
                    self.change(game, me, proposal.to, -6, Action::Threat);
                }
                // A threat costs the threatener the goodwill of the one threatened (`Random(15)`).
                ProposalKind::Threat if proposal.to == me => {
                    self.change(game, me, proposal.from, -7, Action::Threat);
                }
                // A trade that was agreed pleases both (the original: when it is the better deal).
                ProposalKind::Trade { .. }
                    if accepted && (proposal.to == me || proposal.from == me) =>
                {
                    let other = if proposal.to == me {
                        proposal.from
                    } else {
                        proposal.to
                    };
                    self.change(game, me, other, 5, Action::Friendship);
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// A war with `other` began: the feeling falls to the middle of the original's `-75 - Random(25)`.
    pub(crate) fn war_began(&mut self, game: &Game, me: PlayerId, other: PlayerId) {
        let o = self.slot(game, me, other);
        o.friendship = -87;
        o.default = (o.default - 5).max(-100);
        o.war = true;
    }

    /// The war with `other` ended in peace: the feeling is 40 better.
    pub(crate) fn war_ended(&mut self, game: &Game, me: PlayerId, other: PlayerId) {
        let o = self.slot(game, me, other);
        if o.war {
            o.friendship = (o.friendship + 40).min(100);
            o.war = false;
        }
    }
}

/// The first opinion of `other`: what the core's table holds (a game that went on before this task,
/// or one where the core has already moved it), else the start.
fn first_opinion(game: &Game, me: PlayerId, other: PlayerId) -> Opinion {
    let start = start_feeling(game, me, other);
    let (friendship, default) = game
        .diplomacy
        .pair(me, other)
        .map_or((start, start), |p| (p.visible, p.default));
    Opinion {
        with: other,
        friendship,
        default,
        hidden: game.diplomacy.stance(other, me).map_or(0, |s| s.hidden),
        broken: game
            .diplomacy
            .stance(me, other)
            .is_some_and(|s| s.broken.is_some()),
        war: game.treaty(me, other) == Treaty::War,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplo_b_stance::tests::pair;
    use crate::testworld::{P0, P1};
    use f4x_core::game::Command;

    fn mind(g: &Game, friendship: i32) -> Memory {
        let mut m = Memory::default();
        m.set_friendship(g, P0, P1, friendship);
        m
    }

    #[test]
    fn a_first_opinion_is_what_the_core_holds_or_the_start_from_the_books() {
        let g = pair(8, 8, 100);
        let m = Memory::default();
        let o = m.opinion(&g, P0, P1);
        assert_eq!(
            o.friendship,
            g.diplomacy
                .pair(P0, P1)
                .map_or(start_feeling(&g, P0, P1), |p| p.visible)
        );
        assert_eq!((o.hidden, o.broken, o.war), (0, false, false));
    }

    #[test]
    fn a_good_deed_counts_for_more_when_it_is_needed_and_a_bad_one_when_it_is_not() {
        let g = pair(8, 8, 100);
        let mut low = mind(&g, -40);
        low.change(&g, P0, P1, 5, Action::Friendship);
        assert!(
            low.friendship(&g, P0, P1) >= -40 + 10,
            "doubled when it is negative"
        );
        let mut high = mind(&g, 60);
        high.change(&g, P0, P1, -5, Action::AttackedCity);
        assert!(
            high.friendship(&g, P0, P1) <= 60 - 10,
            "doubled when it is positive"
        );
        let mut warm = mind(&g, 60);
        warm.change(&g, P0, P1, 10, Action::Friendship);
        assert!(
            warm.friendship(&g, P0, P1) < 70,
            "less when it is already good"
        );
    }

    #[test]
    fn the_feeling_is_capped_and_a_war_is_not_made_worse() {
        let mut g = pair(8, 8, 100);
        let mut m = mind(&g, 60);
        m.change(&g, P0, P1, 100, Action::Friendship);
        assert!(m.friendship(&g, P0, P1) <= 65, "no treaty: at most 65");
        g.declare_war(P1, P0);
        let mut m = mind(&g, -50);
        m.change(&g, P0, P1, -20, Action::AttackedCity);
        assert_eq!(
            m.friendship(&g, P0, P1),
            -50,
            "a war is not made worse by what is done in it"
        );
        m.change(&g, P0, P1, 30, Action::Friendship);
        assert!(
            m.friendship(&g, P0, P1) <= -25,
            "in a war it stays at -25 or below"
        );
    }

    #[test]
    fn a_city_taken_costs_the_loser_forty_and_the_taker_nothing() {
        let g = pair(8, 8, 100);
        let city = g.economy().cities().into_iter().next().expect("a city").0;
        let event = Event::CityCaptured {
            city,
            from: P0,
            to: P1,
        };
        let mut loser = mind(&g, 0);
        loser.feel(&g, P0, &event);
        assert!(
            loser.friendship(&g, P0, P1) <= -40,
            "{}",
            loser.friendship(&g, P0, P1)
        );
        let mut taker = mind(&g, 0);
        taker.feel(&g, P1, &event);
        assert_eq!(taker.friendship(&g, P0, P1), 0);
    }

    #[test]
    fn a_war_sets_the_feeling_low_and_peace_lifts_it_by_forty() {
        let mut g = pair(8, 8, 100);
        let mut m = mind(&g, 20);
        let events = g.apply(Command::Diplomacy(
            f4x_core::diplomacy::DiplomacyCommand::DeclareWar { player: P1, on: P0 },
        ));
        for e in &events {
            m.feel(&g, P0, e);
        }
        assert_eq!(m.friendship(&g, P0, P1), -87);
        assert!(m.opinion(&g, P0, P1).war);
        let peace = Event::Diplomacy(DiplomacyEvent::TreatyChanged {
            a: P0,
            b: P1,
            treaty: Treaty::None,
        });
        m.feel(&g, P0, &peace);
        assert_eq!(m.friendship(&g, P0, P1), -47);
        m.feel(&g, P0, &peace);
        assert_eq!(
            m.friendship(&g, P0, P1),
            -47,
            "a second peace changes nothing"
        );
    }

    #[test]
    fn an_oath_broken_is_held_against_the_breaker_by_the_victim_more_than_by_the_rest() {
        let g = pair(8, 8, 100);
        let broke = Event::Diplomacy(DiplomacyEvent::TreatyBroken {
            breaker: P1,
            victim: P0,
            treaty: Treaty::Alliance,
        });
        let mut victim = mind(&g, 30);
        victim.feel(&g, P0, &broke);
        let o = victim.opinion(&g, P0, P1);
        assert!(o.broken && o.hidden <= -20 && o.friendship < 30, "{o:?}");
        let mut other = Memory::default();
        other.feel(&g, PlayerId(2), &broke);
        assert_eq!(other.opinion(&g, PlayerId(2), P1).hidden, -5);
        let mut breaker = mind(&g, 30);
        breaker.feel(&g, P1, &broke);
        assert_eq!(
            breaker.friendship(&g, P0, P1),
            30,
            "the breaker does not feel it"
        );
    }

    #[test]
    fn gifts_warm_the_receiver_and_a_spell_leaves_a_lasting_regard() {
        let g = pair(8, 8, 100);
        let spell = f4x_core::rules::SpellId::new(g.content().rules.spells.defs()[0].num);
        let mut m = mind(&g, 0);
        m.feel(
            &g,
            P0,
            &Event::Diplomacy(DiplomacyEvent::SpellGiven {
                from: P1,
                to: P0,
                spell,
            }),
        );
        let o = m.opinion(&g, P0, P1);
        assert!(o.friendship > 0 && o.hidden == 5, "{o:?}");
        let mut other = mind(&g, 0);
        other.feel(
            &g,
            P1,
            &Event::Diplomacy(DiplomacyEvent::SpellGiven {
                from: P1,
                to: P0,
                spell,
            }),
        );
        assert_eq!(
            other.friendship(&g, P0, P1),
            0,
            "only the receiver is moved"
        );
    }

    #[test]
    fn a_refused_demand_sours_the_demander() {
        let g = pair(8, 8, 100);
        let demand = |accepted| {
            Event::Diplomacy(DiplomacyEvent::ProposalAnswered {
                proposal: f4x_core::diplomacy::Proposal {
                    from: P0,
                    to: P1,
                    kind: ProposalKind::Demand { gold: 10 },
                    round: 0,
                },
                accepted,
                lapsed: false,
            })
        };
        let mut m = mind(&g, 0);
        m.feel(&g, P0, &demand(true));
        assert_eq!(m.friendship(&g, P0, P1), 0);
        m.feel(&g, P0, &demand(false));
        assert!(m.friendship(&g, P0, P1) < 0);
    }
}
