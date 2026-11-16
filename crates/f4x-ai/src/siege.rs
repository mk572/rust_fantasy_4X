//! Mana as strength at a hard site (task 502). A tower's guards are worth 900 to 3,900; nine
//! soldiers alone do not beat them, but the wizard's battle spells (the battle AI casts from the
//! pool, `battle/cast.rs`) do the work of more soldiers. So the party that goes to a site is judged
//! by its soldiers plus the spells the pool can pay for, and a wizard that has a hard tower in
//! reach and not the mana for it turns surplus gold into mana (`wizardry::alchemy`) until it has.
//!
//! Gathering: soldiers spread over the cities are sent to the city nearest a tower they could take
//! together ([`gather`]) and set off as one party ([`release`], then `site::deploy`). A tower is a
//! target only on a plane where no rival has a known city (the rivals come first).
//!
//! The AI plays by the human's rules: it counts only the spells it knows and the guards it has
//! seen or guessed (`site::expected`).

use f4x_core::player::view::PlayerView;
use f4x_def::rules::CastContext;

use crate::military::average_strength;
use crate::view::{distance, fighting_strength, is_fighter};
use crate::Ai;
use f4x_core::game::Game;
use f4x_core::map::{Plane, Position};

/// Mana that does the work of one average soldier in a fight (a bolt or a blessing: 15 to 25).
pub(crate) const MANA_PER_SOLDIER: f32 = 20.0;
/// Most soldiers' worth the spells are counted for: a battle lasts a few rounds.
pub(crate) const MOST_SOLDIERS: f32 = 6.0;
/// Turns a muster waits for its soldiers before it sets off with the ones that came.
const MUSTER_TURNS: u32 = 20;

/// The margin the party and its spells need over the guards the wizard expects: the raids' edge.
fn siege_edge() -> f32 {
    crate::site::edge()
}

/// Does the wizard know a spell it can cast in a battle?
fn has_battle_spell(view: &PlayerView) -> bool {
    view.wizard().is_some_and(|w| {
        w.known()
            .iter()
            .any(|&s| view.spell(s).contexts.contains(&CastContext::Battle))
    })
}

/// The mana a fight can use: the pool, up to the casting skill (a wizard casts only so much in
/// one battle).
fn usable_mana(view: &PlayerView) -> f32 {
    let Some(w) = view.wizard() else {
        return 0.0;
    };
    let skill = w.casting_skill(&view.content().rules).max(0) as f32;
    (view.treasury().mana.floor().max(0) as f32).min(skill * 3.0)
}

/// The strength the wizard's spells add to a party in a site fight: nothing without a battle
/// spell, else the soldiers the usable mana is worth, at most [`MOST_SOLDIERS`].
pub(crate) fn spell_power(view: &PlayerView) -> f32 {
    if !has_battle_spell(view) {
        return 0.0;
    }
    (usable_mana(view) / MANA_PER_SOLDIER).min(MOST_SOLDIERS) * average_strength(view)
}

/// The mana the wizard wants in the pool for the hardest intact tower in reach that its nine best
/// soldiers cannot take alone, but soldiers and spells together could (0 when there is none, or
/// when the wizard has no battle spell).
pub(crate) fn mana_goal(view: &PlayerView, blocked: &[Plane]) -> i32 {
    if !has_battle_spell(view) || crate::phase::phase(view) == crate::phase::Phase::Build {
        return 0;
    }
    let fighters: Vec<_> = view
        .my_units()
        .into_iter()
        .filter(|&u| is_fighter(view, u) && !crate::recover::needs_rest(view, u))
        .collect();
    let best_nine = fighting_strength(view, &fighters);
    let each = average_strength(view).max(1.0);
    let reachable = best_nine + MOST_SOLDIERS * each;
    let Some(start) = view.my_cities().first().map(|&(_, p)| p) else {
        return 0;
    };
    view.towers()
        .into_iter()
        .filter(|&(p, intact)| {
            intact
                && !blocked.contains(&p.plane)
                && p.plane == start.plane
                && crate::view::distance(view, start, p).is_some()
        })
        .map(|(p, _)| crate::site::expected(view, p) * siege_edge())
        .filter(|&need| need > best_nine && need <= reachable)
        .map(|need| ((need - best_nine) / each * MANA_PER_SOLDIER).ceil() as i32)
        .min()
        .unwrap_or(0)
}

/// The planes where a rival's city stands in the way of the towers: a known city of a rival
/// wizard that the army can walk to (not one in `unreachable`, which a ship has to reach) or that a
/// landing is set on (`naval_target`). The rivals come before the towers; a rival across the sea
/// that no landing is bound for does not hold the towers back.
pub(crate) fn blocked_planes(
    view: &PlayerView,
    unreachable: &[Position],
    naval_target: Option<Position>,
) -> Vec<Plane> {
    let mut planes: Vec<Plane> = Vec::new();
    for (_, owner, p) in view.known_cities() {
        if view.is_rival_wizard(owner)
            && (!unreachable.contains(&p) || naval_target == Some(p))
            && !planes.contains(&p.plane)
        {
            planes.push(p.plane);
        }
    }
    planes
}

/// End a muster whose soldiers have come (or waited long enough): they are free soldiers again,
/// and `site::deploy` sends the best nine of the square they stand on.
pub(crate) fn release(ai: &mut Ai, game: &Game) {
    if !ai.roles.any(crate::roles::Role::Muster) {
        return;
    }
    let view = PlayerView::new(game, ai.player);
    let alive: Vec<_> = view
        .my_units()
        .into_iter()
        .filter(|&u| ai.roles.is(view.serial_of(u), crate::roles::Role::Muster))
        .collect();
    let arrived = ai.muster_at.is_some_and(|at| {
        alive.iter().all(|&u| {
            view.unit_position(u)
                .is_some_and(|p| distance(&view, p, at).is_some_and(|d| d <= 1))
        })
    });
    if alive.is_empty() || arrived || game.turn.saturating_sub(ai.muster_since) > MUSTER_TURNS {
        ai.roles.release_role(crate::roles::Role::Muster);
        ai.muster_at = None;
    }
}

/// The free soldiers of the empire cannot take a tower from any one square, but the best nine of
/// them with the wizard's spells could: send those nine to the own city nearest the tower. They
/// are kept apart from raids while they walk (`Ai::muster`).
pub(crate) fn gather(
    ai: &mut Ai,
    game: &mut Game,
    free: &[hecs::Entity],
    events: &mut Vec<f4x_core::game::Event>,
) {
    use f4x_core::game::Command;
    let me = ai.player;
    if ai.roles.any(crate::roles::Role::Muster) {
        return;
    }
    let (rally, orders) = {
        let view = PlayerView::new(game, me);
        if crate::phase::phase(&view) == crate::phase::Phase::Build {
            return;
        }
        let mut best: Vec<_> = free
            .iter()
            .copied()
            .filter(|&u| {
                is_fighter(&view, u)
                    && !crate::recover::needs_rest(&view, u)
                    && !ai.roles.is(view.serial_of(u), crate::roles::Role::Raid)
                    && !ai.scouts.contains(&view.serial_of(u))
                    && view.unit_position(u).is_some()
            })
            .collect();
        best.sort_by(|&a, &b| {
            crate::view::unit_strength(&view, b)
                .total_cmp(&crate::view::unit_strength(&view, a))
                .then(view.serial_of(a).cmp(&view.serial_of(b)))
        });
        best.truncate(view.max_battle_side());
        if best.len() < 3 {
            return;
        }
        let strength = crate::view::stack_strength(&view, &best) + spell_power(&view);
        // The easiest tower the nine could take, on a plane with no rival's city.
        let Some(tower) = view
            .towers()
            .into_iter()
            .filter(|&(p, intact)| intact && !ai.tower_blocked.contains(&p.plane))
            .map(|(p, _)| p)
            .filter(|&p| strength >= crate::site::expected(&view, p) * siege_edge())
            .min_by(|&a, &b| {
                crate::site::expected(&view, a)
                    .total_cmp(&crate::site::expected(&view, b))
                    .then((a.y, a.x).cmp(&(b.y, b.x)))
            })
        else {
            return;
        };
        let Some(rally) = view
            .my_cities()
            .into_iter()
            .map(|(_, p)| p)
            .filter(|p| p.plane == tower.plane)
            .filter_map(|p| distance(&view, p, tower).map(|d| (d, p)))
            .min_by_key(|&(d, p)| (d, p.y, p.x))
            .map(|(_, p)| p)
        else {
            return;
        };
        let mut reached = crate::view::ReachCache::default();
        let orders: Vec<hecs::Entity> = best
            .into_iter()
            .filter(|&u| {
                view.unit_position(u).is_some_and(|p| {
                    distance(&view, p, rally).is_none_or(|d| d > 1) && !view.unit_has_order(u)
                })
            })
            .filter(|&u| reached.can_reach(&view, &[u], rally))
            .collect();
        (rally, orders)
    };
    if orders.is_empty() {
        return;
    }
    let serials: Vec<_> = {
        let view = PlayerView::new(game, me);
        orders.iter().map(|&u| view.serial_of(u)).collect()
    };
    for u in orders {
        if Ai::room_at(game, rally, 1) {
            let _ = ai.send(
                game,
                Command::GoTo {
                    units: vec![u],
                    to: rally,
                },
                events,
            );
        }
    }
    ai.roles
        .assign(crate::roles::Role::Muster, serials, game.turn);
    ai.muster_at = Some(rally);
    ai.muster_since = game.turn;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, game_rival_across, spawn, COAST_CITY, P0};
    use f4x_core::game::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Teach the wizard a spell it can cast in a battle (`fire_bolt`, 15 mana).
    fn learn_a_battle_spell(g: &mut Game) -> TestResult {
        let id = g
            .content()
            .rules
            .spells
            .id_numbered(5)
            .ok_or("a battle spell is in the content")?;
        let spell = g.content().rules.spells.get(id);
        assert!(spell.contexts.contains(&CastContext::Battle));
        g.cheat_learn_spell(P0, id);
        g.wizard_mut(P0)
            .ok_or("player 0 is a wizard")?
            .put(Some(f4x_core::wizard::sheet::Skill {
                points: 400,
                left: 10,
            }));
        Ok(())
    }

    fn set_mana(g: &mut Game, mana: i32) -> TestResult {
        g.treasury_mut(P0).ok_or("a treasury")?.mana = f4x_def::half_int::HalfInt::from_int(mana);
        Ok(())
    }

    #[test]
    fn spells_add_strength_only_with_a_battle_spell_and_mana_to_cast_it() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut g, P0, at(5, 5), 113 /* high_men_paladins */, 3);
        set_mana(&mut g, 200)?;
        assert_eq!(spell_power(&PlayerView::new(&g, P0)), 0.0, "no spell known");
        learn_a_battle_spell(&mut g)?;
        let rich = spell_power(&PlayerView::new(&g, P0));
        assert!(rich > 0.0);
        set_mana(&mut g, 0)?;
        assert_eq!(spell_power(&PlayerView::new(&g, P0)), 0.0, "no mana");
        Ok(())
    }

    #[test]
    fn a_tower_the_soldiers_cannot_take_alone_is_a_goal_for_mana() -> TestResult {
        let mut g = game_rival_across(COAST_CITY, 8 /* high_men */);
        g.turn = 400; // the end of the game: not a build-up
        spawn(&mut g, P0, at(10, 6), 113 /* high_men_paladins */, 1);
        crate::testworld::spawn_tower(&mut g, at(33, 9), true)?;
        g.reveal_map(P0);
        assert_eq!(
            mana_goal(&PlayerView::new(&g, P0), &[]),
            0,
            "no battle spell"
        );
        learn_a_battle_spell(&mut g)?;
        let goal = mana_goal(&PlayerView::new(&g, P0), &[]);
        assert!(
            goal > 0,
            "one soldier is short of a tower's guess of guards"
        );
        // The goal is spells for the shortfall only: a tenth soldier more and it shrinks.
        spawn(&mut g, P0, at(10, 6), 113 /* high_men_paladins */, 4);
        assert!(mana_goal(&PlayerView::new(&g, P0), &[]) < goal);
        Ok(())
    }

    #[test]
    fn a_party_with_mana_for_spells_is_enough_for_a_tower_it_could_not_take_without() -> TestResult
    {
        let mut g = game_rival_across(COAST_CITY, 8 /* high_men */);
        let paladins = spawn(&mut g, P0, at(10, 6), 113 /* high_men_paladins */, 1);
        crate::testworld::spawn_tower(&mut g, at(33, 9), true)?;
        g.reveal_map(P0);
        learn_a_battle_spell(&mut g)?;
        set_mana(&mut g, 200)?;
        let view = PlayerView::new(&g, P0);
        let soldiers = crate::view::stack_strength(&view, &paladins);
        let with = soldiers + spell_power(&view);
        let near = |s| crate::site::sites_near(&view, at(10, 6), s, &[]);
        assert!(near(soldiers).is_empty());
        assert_eq!(near(with), vec![at(33, 9)]);
        Ok(())
    }

    #[test]
    fn soldiers_in_the_field_are_sent_to_the_city_nearest_a_tower_they_could_take_together(
    ) -> TestResult {
        let mut g = game_rival_across(COAST_CITY, 8 /* high_men */);
        g.turn = 400;
        let party = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 5);
        crate::testworld::spawn_tower(&mut g, at(3, 4), true)?;
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        gather(&mut ai, &mut g, &party, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        assert_eq!(
            ai.roles.count(crate::roles::Role::Muster),
            5,
            "all five are mustered"
        );
        assert_eq!(ai.muster_at, Some(at(COAST_CITY.0, COAST_CITY.1)));
        assert!(party.iter().all(|&u| view.unit_has_order(u)));
        // A second call while they walk changes nothing.
        let before = ai.roles.members(crate::roles::Role::Muster);
        gather(&mut ai, &mut g, &party, &mut Vec::new());
        assert_eq!(ai.roles.members(crate::roles::Role::Muster), before);
        Ok(())
    }

    #[test]
    fn no_muster_while_the_soldiers_could_not_take_any_tower_and_none_while_a_rival_is_on_the_plane(
    ) -> TestResult {
        // One soldier: too few for a tower, with no spell to help.
        let mut g = game_rival_across(COAST_CITY, 8 /* high_men */);
        g.turn = 400;
        let few = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 2);
        crate::testworld::spawn_tower(&mut g, at(3, 4), true)?;
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        gather(&mut ai, &mut g, &few, &mut Vec::new());
        assert!(!ai.roles.any(crate::roles::Role::Muster));
        // Strong enough, but a rival has a city on the tower's plane.
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.turn = 400;
        let many = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 6);
        crate::testworld::spawn_tower(&mut g, at(3, 4), true)?;
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        ai.tower_blocked = blocked_planes(&PlayerView::new(&g, P0), &[], None);
        gather(&mut ai, &mut g, &many, &mut Vec::new());
        assert!(!ai.roles.any(crate::roles::Role::Muster));
        Ok(())
    }

    #[test]
    fn a_muster_ends_when_its_soldiers_have_come_or_have_waited_too_long() -> TestResult {
        let mut g = game_rival_across(COAST_CITY, 8 /* high_men */);
        g.turn = 400;
        let party = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 5);
        crate::testworld::spawn_tower(&mut g, at(3, 4), true)?;
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        gather(&mut ai, &mut g, &party, &mut Vec::new());
        release(&mut ai, &g);
        assert_eq!(
            ai.roles.count(crate::roles::Role::Muster),
            5,
            "still on the way"
        );
        g.turn += MUSTER_TURNS + 1;
        release(&mut ai, &g);
        assert!(!ai.roles.any(crate::roles::Role::Muster), "gave up waiting");
        // Or they arrive.
        let mut ai = Ai::new(P0);
        gather(&mut ai, &mut g, &party, &mut Vec::new());
        for &u in &party {
            g.cheat_move_unit(u, at(COAST_CITY.0, COAST_CITY.1));
        }
        release(&mut ai, &g);
        assert!(!ai.roles.any(crate::roles::Role::Muster), "arrived");
        Ok(())
    }
}
