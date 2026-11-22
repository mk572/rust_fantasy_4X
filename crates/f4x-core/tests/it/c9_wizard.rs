//! Class 9, the wizard: what is researched and learned, the split of the power base.

use crate::support::{content, worlds};
use f4x_core::game::{Command, Event, Game, Rejection};
use f4x_core::player::PlayerId;
use f4x_core::wizard::magic::MagicError;

fn wizards(g: &Game) -> Vec<PlayerId> {
    g.player_ids()
        .into_iter()
        .filter(|&p| !g.is_neutral_player(p) && g.wizard(p).is_some())
        .collect()
}

fn magic_refusal(events: &[Event]) -> Option<MagicError> {
    events.iter().find_map(|e| match e {
        Event::Rejected(Rejection::Magic(m)) => Some(*m),
        _ => None,
    })
}

fn split(g: &Game, p: PlayerId) -> (u8, u8, u8) {
    let s = g.wizard(p).expect("a wizard").power_split();
    (s.mana, s.research, s.skill)
}

#[test]
fn the_power_split_must_add_up_to_a_hundred_and_a_refusal_keeps_the_old_split() {
    for (name, mut g) in worlds().into_iter().step_by(2) {
        for p in wizards(&g) {
            let set = |g: &mut Game, m, r, s| {
                magic_refusal(&g.apply(Command::SetPowerSplit {
                    player: p,
                    mana: m,
                    research: r,
                    skill: s,
                }))
            };
            assert_eq!(set(&mut g, 10, 20, 70), None, "{name}");
            assert_eq!(split(&g, p), (10, 20, 70), "{name}");
            for (m, r, s) in [
                (0, 0, 0),
                (50, 50, 50),
                (99, 0, 0),
                (100, 1, 0),
                (255, 255, 255),
            ] {
                assert_eq!(
                    set(&mut g, m, r, s),
                    Some(MagicError::BadSplit),
                    "{name}: {m}+{r}+{s}"
                );
                assert_eq!(split(&g, p), (10, 20, 70), "{name}: {m}+{r}+{s} changed it");
            }
            assert_eq!(set(&mut g, 0, 100, 0), None, "{name}: all in one");
            assert_eq!(split(&g, p), (0, 100, 0), "{name}");
        }
    }
}

#[test]
fn research_takes_a_spell_on_offer_once_and_costs_what_the_rules_list() {
    for (name, mut g) in worlds().into_iter().step_by(2) {
        for p in wizards(&g) {
            let w = g.wizard(p).expect("a wizard");
            assert!(
                w.researching().is_none(),
                "{name}: a new wizard researches nothing"
            );
            let offer = w.candidates();
            assert!(!offer.is_empty(), "{name}: nothing on offer");
            let known = w.known();
            for s in &offer {
                assert!(!known.contains(s), "{name}: a known spell is on offer");
                assert!(w.can_research(*s), "{name}: not researchable but on offer");
            }
            // A spell that is not on offer is refused, one that is on offer is taken.
            let off = (0..g.content().rules.spells.defs().len())
                .map(|i| f4x_core::rules::SpellId::new(g.content().rules.spells.defs()[i].num))
                .find(|s| !offer.contains(s))
                .expect("a spell off the offer");
            let events = g.apply(Command::SetResearch {
                player: p,
                spell: off,
            });
            assert_eq!(
                magic_refusal(&events),
                Some(MagicError::NotAvailable),
                "{name}"
            );
            assert!(g.wizard(p).expect("a wizard").researching().is_none());
            let pick = offer[0];
            let events = g.apply(Command::SetResearch {
                player: p,
                spell: pick,
            });
            assert_eq!(magic_refusal(&events), None, "{name}");
            let w = g.wizard(p).expect("a wizard");
            assert_eq!(w.researching(), Some(pick), "{name}");
            assert_eq!(
                w.research_remaining(),
                w.research_cost(&content(), pick),
                "{name}"
            );
            // One at a time, until it is learned.
            let other = offer.get(1).copied().unwrap_or(pick);
            let events = g.apply(Command::SetResearch {
                player: p,
                spell: other,
            });
            assert_eq!(
                magic_refusal(&events),
                Some(MagicError::AlreadyResearching),
                "{name}"
            );
            assert_eq!(g.wizard(p).expect("a wizard").researching(), Some(pick));
        }
    }
}

#[test]
fn research_with_the_cost_nearly_paid_learns_the_spell_in_the_turn_and_frees_the_choice() {
    for (name, mut g) in worlds().into_iter().step_by(2) {
        let p = wizards(&g)[0];
        g.apply(Command::SetPowerSplit {
            player: p,
            mana: 0,
            research: 100,
            skill: 0,
        });
        let pick = g.wizard(p).expect("a wizard").candidates()[0];
        g.apply(Command::SetResearch {
            player: p,
            spell: pick,
        });
        // Nearly done: one point is left.
        let mut snap = g.snapshot();
        let save = snap
            .wizards
            .iter_mut()
            .find(|s| s.player == p.0)
            .expect("a save");
        save.research_remaining = 1;
        g = snap.restore(content()).expect("restores");
        assert!(!g.wizard(p).expect("a wizard").knows(pick));
        g.apply(Command::EndTurn);
        let w = g.wizard(p).expect("a wizard");
        assert!(w.knows(pick), "{name}: not learned after a turn");
        assert!(w.researching().is_none(), "{name}: still researching it");
        assert!(
            !w.candidates().contains(&pick),
            "{name}: a known spell is still on offer"
        );
        assert!(
            !w.candidates().is_empty(),
            "{name}: the offer was not refilled"
        );
    }
}

#[test]
fn a_known_spell_cannot_be_researched_and_the_neutral_player_researches_nothing() {
    for (name, mut g) in worlds().into_iter().step_by(2) {
        let p = wizards(&g)[0];
        let known = g.wizard(p).expect("a wizard").known();
        let spell = *known.iter().next().expect("a known spell");
        let events = g.apply(Command::SetResearch { player: p, spell });
        assert_eq!(
            magic_refusal(&events),
            Some(MagicError::NotAvailable),
            "{name}: a known spell"
        );
        // The neutral player is no wizard.
        let neutral = g.player_ids().into_iter().find(|&q| g.is_neutral_player(q));
        if let Some(n) = neutral {
            let events = g.apply(Command::SetResearch { player: n, spell });
            assert!(
                events.iter().any(|e| matches!(e, Event::Rejected(_))),
                "{name}"
            );
        }
    }
}
