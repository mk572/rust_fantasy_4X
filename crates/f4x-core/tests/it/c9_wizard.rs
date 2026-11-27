//! Class 9, the wizard: what is researched and learned, the split of the power base.

use crate::support::{content, worlds};
use f4x_core::player::PlayerId;
use f4x_core::wizard::magic::MagicError;
use f4x_core::world::rng::GameRng;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection, World};

fn wizards(w: &World) -> Vec<PlayerId> {
    w.player_ids()
        .into_iter()
        .filter(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
        .collect()
}

fn magic_refusal(events: &[Event]) -> Option<MagicError> {
    events.iter().find_map(|e| match e {
        Event::Rejected(Rejection::Magic(m)) => Some(*m),
        _ => None,
    })
}

fn split(w: &World, p: PlayerId) -> (u8, u8, u8) {
    let s = w.wizard(p).expect("a wizard").power_split();
    (s.mana, s.research, s.skill)
}

#[test]
fn the_power_split_must_add_up_to_a_hundred_and_a_refusal_keeps_the_old_split() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        for p in wizards(&w) {
            let set = |w: &mut Table, m, r, s| {
                magic_refusal(&w.apply(Command::SetPowerSplit {
                    player: p,
                    mana: m,
                    research: r,
                    skill: s,
                }))
            };
            assert_eq!(set(&mut w, 10, 20, 70), None, "{name}");
            assert_eq!(split(&w, p), (10, 20, 70), "{name}");
            for (m, r, s) in [
                (0, 0, 0),
                (50, 50, 50),
                (99, 0, 0),
                (100, 1, 0),
                (255, 255, 255),
            ] {
                assert_eq!(
                    set(&mut w, m, r, s),
                    Some(MagicError::BadSplit),
                    "{name}: {m}+{r}+{s}"
                );
                assert_eq!(split(&w, p), (10, 20, 70), "{name}: {m}+{r}+{s} changed it");
            }
            assert_eq!(set(&mut w, 0, 100, 0), None, "{name}: all in one");
            assert_eq!(split(&w, p), (0, 100, 0), "{name}");
        }
    }
}

#[test]
fn research_takes_a_spell_on_offer_once_and_costs_what_the_rules_list() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        for p in wizards(&w) {
            let wizard = w.wizard(p).expect("a wizard");
            assert!(
                wizard.researching().is_none(),
                "{name}: a new wizard researches nothing"
            );
            let offer = wizard.candidates();
            assert!(!offer.is_empty(), "{name}: nothing on offer");
            let known = wizard.known();
            for s in &offer {
                assert!(!known.contains(s), "{name}: a known spell is on offer");
                assert!(
                    wizard.can_research(*s),
                    "{name}: not researchable but on offer"
                );
            }
            // A spell that is not on offer is refused, one that is on offer is taken.
            let off = (0..w.content().rules.spells.defs().len())
                .map(|i| f4x_core::rules::SpellId::new(w.content().rules.spells.defs()[i].num))
                .find(|s| !offer.contains(s))
                .expect("a spell off the offer");
            let events = w.apply(Command::SetResearch {
                player: p,
                spell: off,
            });
            assert_eq!(
                magic_refusal(&events),
                Some(MagicError::NotAvailable),
                "{name}"
            );
            assert!(w.wizard(p).expect("a wizard").researching().is_none());
            let pick = offer[0];
            let events = w.apply(Command::SetResearch {
                player: p,
                spell: pick,
            });
            assert_eq!(magic_refusal(&events), None, "{name}");
            let wizard = w.wizard(p).expect("a wizard");
            assert_eq!(wizard.researching(), Some(pick), "{name}");
            assert_eq!(
                wizard.research_remaining(),
                wizard.research_cost(&content(), pick),
                "{name}"
            );
            // One at a time, until it is learned.
            let other = offer.get(1).copied().unwrap_or(pick);
            let events = w.apply(Command::SetResearch {
                player: p,
                spell: other,
            });
            assert_eq!(
                magic_refusal(&events),
                Some(MagicError::AlreadyResearching),
                "{name}"
            );
            assert_eq!(w.wizard(p).expect("a wizard").researching(), Some(pick));
        }
    }
}

#[test]
fn research_with_the_cost_nearly_paid_learns_the_spell_in_the_turn_and_frees_the_choice() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        w.apply(Command::SetPowerSplit {
            player: p,
            mana: 0,
            research: 100,
            skill: 0,
        });
        let pick = w.wizard(p).expect("a wizard").candidates()[0];
        w.apply(Command::SetResearch {
            player: p,
            spell: pick,
        });
        // Nearly done: one point is left.
        let mut snap = w.snapshot();
        let save = snap
            .wizards
            .iter_mut()
            .find(|s| s.player == p.0)
            .expect("a save");
        save.research_remaining = 1;
        let dice = GameRng::resume(w.seed(), w.draws());
        w = Table::new(snap.restore(content()).expect("restores"), dice);
        assert!(!w.wizard(p).expect("a wizard").knows(pick));
        // The points come in at the turn's end, the spell is learned at the next one's start.
        w.end_player_turn(p);
        w.start_player_turn(p, 2);
        let wizard = w.wizard(p).expect("a wizard");
        assert!(wizard.knows(pick), "{name}: not learned after a turn");
        assert!(
            wizard.researching().is_none(),
            "{name}: still researching it"
        );
        assert!(
            !wizard.candidates().contains(&pick),
            "{name}: a known spell is still on offer"
        );
        assert!(
            !wizard.candidates().is_empty(),
            "{name}: the offer was not refilled"
        );
    }
}

#[test]
fn a_known_spell_cannot_be_researched_and_the_neutral_player_researches_nothing() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        let known = w.wizard(p).expect("a wizard").known();
        let spell = *known.iter().next().expect("a known spell");
        let events = w.apply(Command::SetResearch { player: p, spell });
        assert_eq!(
            magic_refusal(&events),
            Some(MagicError::NotAvailable),
            "{name}: a known spell"
        );
        // The neutral player is no wizard.
        let neutral = w.player_ids().into_iter().find(|&q| w.is_neutral_player(q));
        if let Some(n) = neutral {
            let events = w.apply(Command::SetResearch { player: n, spell });
            assert!(
                events.iter().any(|e| matches!(e, Event::Rejected(_))),
                "{name}"
            );
        }
    }
}

/// A banished wizard is always casting the Spell of Return: when it is ended (a Spell Blast takes
/// it off the wizard), the turn's end begins it again, whoever the wizard is.
#[test]
fn a_banished_wizard_whose_return_was_ended_begins_it_again_at_the_turns_end() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let p = wizards(&w)[0];
        let mut snap = w.snapshot();
        let save = snap
            .wizards
            .iter_mut()
            .find(|s| s.player == p.0)
            .expect("a save");
        save.banished = true;
        save.fortress = None;
        save.summon_at = None;
        save.more_circles.clear();
        let dice = GameRng::resume(w.seed(), w.draws());
        w = Table::new(snap.restore(content()).expect("restores"), dice);
        assert!(w.wizard(p).expect("a wizard").is_banished(), "{name}");
        assert!(w.wizard(p).expect("a wizard").casting().is_none(), "{name}");
        w.end_player_turn(p);
        let casting = w.wizard(p).expect("a wizard").casting();
        assert!(
            casting.is_some_and(|c| w.spell_has_hook(c.spell, "spell_of_return")),
            "{name}: not casting the Spell of Return: {casting:?}"
        );
    }
}
