//! What the AI does when another wizard starts casting the Spell of Mastery
//! (`docs/ai-design.md` section 7): it races for the spell when it can finish first, and
//! otherwise declares war on the caster and sends its armies after that wizard's cities.

use f4x_core::diplomacy::{DiplomacyCommand, Treaty};
use f4x_core::game::{Command, Event, Game};
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::rules::SpellId;

use crate::Ai;

/// The race is run only when the AI would finish this many times sooner than the rival.
const LEAD: f32 = 1.25;

/// What the AI has seen of one rival's casting: the turn and the mana left to pay.
pub(crate) type Sighting = (u32, i32);

/// Turns the AI itself needs to have the spell cast, or `None` when it cannot get it: it
/// neither knows it nor is offered it in research.
fn own_turns(view: &PlayerView, mastery: SpellId) -> Option<f32> {
    let magic = view.wizard()?;
    let mana = view.treasury().mana;
    let income = view.income();
    let skill = magic.casting_skill().max(1) as f32;
    let mana_rate = income.split.mana.max(1) as f32;
    let (left, research) = match magic.casting() {
        Some(c) if c.spell == mastery => (c.remaining, 0.0),
        _ if magic.knows(mastery) => (view.casting_cost(mastery), 0.0),
        _ if view.research_choices().contains(&mastery) => {
            let cost = view.research_cost(mastery) as f32;
            (
                view.casting_cost(mastery),
                cost / income.research.max(1) as f32,
            )
        }
        _ => return None,
    };
    let left = left.max(0) as f32;
    let by_skill = left / skill;
    let by_mana = (left - mana as f32).max(0.0) / mana_rate;
    Some(research + by_skill.max(by_mana))
}

/// Turns a rival needs: its mana left over the rate it has been seen to pay. A rival seen for
/// the first time is taken to pay as fast as the AI itself could.
fn rival_turns(left: i32, seen: Option<Sighting>, now: u32, own_rate: f32) -> f32 {
    let rate = match seen {
        Some((then, was)) if now > then && was > left => (was - left) as f32 / (now - then) as f32,
        _ => own_rate,
    };
    left.max(0) as f32 / rate.max(1.0)
}

/// Weigh the race and answer it, once a turn before the wizard's own casting.
pub(crate) fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let now = game.turn;
    let casters = PlayerView::new(game, me).rival_mastery_casters();
    // Forget wizards that stopped (or finished) casting.
    ai.mastery_seen
        .retain(|p, _| casters.iter().any(|&(c, _)| c == *p));
    if casters.is_empty() {
        ai.racing = false;
        ai.war_on = None;
        return;
    }
    let Some(mastery) = game
        .content()
        .rules
        .spells
        .id_of(f4x_core::game::endgame::SPELL_OF_MASTERY)
    else {
        return;
    };
    let (mine, own_rate, casting_other) = {
        let view = PlayerView::new(game, me);
        let rate = view
            .wizard()
            .map_or(1.0, |m| m.casting_skill().max(1) as f32);
        (
            own_turns(&view, mastery),
            rate,
            view.wizard()
                .is_some_and(|m| m.casting().is_some_and(|c| c.spell != mastery)),
        )
    };
    // The rival that will finish first is the one to beat or to stop.
    let threat = casters
        .iter()
        .map(|&(p, left)| {
            (
                p,
                rival_turns(left, ai.mastery_seen.get(&p).copied(), now, own_rate),
            )
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0 .0.cmp(&b.0 .0)));
    for &(p, left) in &casters {
        ai.mastery_seen.insert(p, (now, left));
    }
    let Some((rival, rival_eta)) = threat else {
        return;
    };
    ai.racing = mine.is_some_and(|m| m * LEAD < rival_eta);
    if ai.racing {
        ai.war_on = None;
        // Drop whatever else is being cast: the next step casts the Spell of Mastery.
        let knows = PlayerView::new(game, me)
            .wizard()
            .is_some_and(|m| m.knows(mastery));
        if casting_other && knows {
            ai.send(game, Command::CancelCast { player: me }, events);
        }
        return;
    }
    ai.war_on = Some(rival);
    if game.treaty(me, rival) != Treaty::War && game.have_met(me, rival) {
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

/// The wizard whose cities the armies should go for, if any.
pub(crate) fn war_target(ai: &Ai) -> Option<PlayerId> {
    ai.war_on.or(ai.quarrel)
}

/// The wizard whose cities the armies go for: the one the plan names, else a wizard it is at
/// war with (whoever declared the war) that has a city it knows.
pub(crate) fn enemy(ai: &Ai, view: &PlayerView) -> Option<PlayerId> {
    war_target(ai).or_else(|| {
        view.known_cities()
            .into_iter()
            .map(|(_, owner, _)| owner)
            .filter(|&o| {
                view.is_rival_wizard(o) && view.treaty_with(o) == f4x_core::diplomacy::Treaty::War
            })
            .min_by_key(|o| o.0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{game, COAST_CITY, P0, P1};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn world() -> Result<(Game, SpellId), Box<dyn std::error::Error>> {
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let mastery = g
            .content()
            .rules
            .spells
            .id_of("spell_of_mastery")
            .ok_or("spell_of_mastery is shipped")?;
        Ok((g, mastery))
    }

    fn casts(g: &mut Game, who: PlayerId, spell: SpellId, left: i32) {
        g.scenario_casting(who, spell, left);
    }

    #[test]
    fn a_wizard_that_cannot_win_the_race_declares_war_and_aims_at_the_caster() -> TestResult {
        let (mut g, mastery) = world()?;
        casts(&mut g, P1, mastery, 5000);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut g, &mut events);
        assert!(!ai.racing);
        assert_eq!(war_target(&ai), Some(P1));
        assert_eq!(g.treaty(P0, P1), Treaty::War);
        Ok(())
    }

    #[test]
    fn a_wizard_that_knows_the_spell_races_and_drops_its_other_casting() -> TestResult {
        let (mut g, mastery) = world()?;
        let other = g
            .content()
            .rules
            .spells
            .iter()
            .map(|(id, _)| id)
            .find(|&id| id != mastery)
            .ok_or("the content has a second spell")?;
        {
            let mut w = g.wizard_mut(P0).ok_or("player 0 is a wizard")?;
            let mut spells = w.get_mut::<f4x_core::wizard::sheet::KnownSpells>();
            spells.known.insert(mastery);
            spells.known.insert(other);
            drop(spells);
            w.get_mut::<f4x_core::wizard::sheet::Skill>().points = 20_000;
            g.treasury_mut(P0).ok_or("player 0 has a treasury")?.mana = 20_000;
        }
        casts(&mut g, P0, other, 300);
        casts(&mut g, P1, mastery, 40_000);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut g, &mut events);
        assert!(ai.racing);
        assert_eq!(war_target(&ai), None);
        assert_ne!(g.treaty(P0, P1), Treaty::War);
        assert!(
            g.wizard(P0)
                .ok_or("player 0 is a wizard")?
                .casting()
                .is_none(),
            "the other spell is dropped"
        );
        Ok(())
    }

    #[test]
    fn nothing_changes_while_nobody_casts_it() -> TestResult {
        let (mut g, _) = world()?;
        let mut ai = Ai::new(P0);
        ai.war_on = Some(P1);
        run(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(war_target(&ai), None);
        assert!(!ai.racing);
        Ok(())
    }

    #[test]
    fn a_rival_seen_paying_is_timed_by_its_pace() {
        // 1000 left, 200 a turn: five turns.
        assert_eq!(rival_turns(1000, Some((10, 1400)), 12, 50.0), 5.0);
    }

    #[test]
    fn a_rival_seen_once_is_taken_to_pay_as_fast_as_the_ai() {
        assert_eq!(rival_turns(1000, None, 12, 50.0), 20.0);
        // No progress seen since: the same.
        assert_eq!(rival_turns(1000, Some((10, 1000)), 12, 50.0), 20.0);
    }
}
