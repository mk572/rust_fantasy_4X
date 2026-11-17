//! The spell helper (task 488): the AI decides what it wants to spend mana on, an intent (the
//! `Kind` of spell: an army, a buff, a blow at an enemy city), and the helper rates the spells the
//! wizard knows for that intent and returns them best first. The rating uses the mana cost, the
//! upkeep, what the spell brings (the strength of the creatures it summons) and how much the
//! wizard's books favour its realm. The AI never names a spell itself. The weights are numbers
//! in one place, so a run can tune them (`AIK_RATE_*`) and a later fit can learn them.

use crate::military::average_strength;
use crate::view::{fighting_strength, is_fighter};
use f4x_core::map::Plane;
use f4x_core::player::view::PlayerView;
use f4x_def::rules::effects::Effect;
use f4x_def::rules::{CastContext, SpellDef, SpellId};

use crate::wizardry::Kind;

/// How much a spell of a realm the wizard has many books in gains (1.0 is a wizard with all its
/// books in that realm).
fn affinity_weight() -> f32 {
    crate::knob("RATE_AFFINITY", 0.5)
}

/// The weight of a summoned creature's strength against the mana it takes to keep.
fn upkeep_weight() -> f32 {
    crate::knob("RATE_UPKEEP", 12.0)
}

/// What the creatures a spell summons are worth in battle, in the AI's own measure.
fn summoned_strength(view: &PlayerView, def: &SpellDef) -> f32 {
    def.effects
        .iter()
        .filter_map(|r| match r.effect {
            Effect::Summon { unit, count } => Some((unit, count)),
            _ => None,
        })
        .map(|(unit, count)| {
            f4x_core::battle::power::def_power(view.content().rules.units.get(unit))
                * f32::from(count)
        })
        .sum()
}

/// How good the spell is for its intent: higher is better. Summons are rated by the strength they
/// bring for each point of mana they cost (the casting cost and the upkeep over twenty turns);
/// every other spell by its cost, a dear spell being the stronger, less its upkeep.
pub fn rate(view: &PlayerView, spell: SpellId, kind: Kind) -> f32 {
    let def = view.spell(spell);
    let cost = view.casting_cost(spell).max(1) as f32;
    let upkeep = def.upkeep as f32;
    let base = match kind {
        Kind::Summon => summoned_strength(view, def) / (cost + upkeep * upkeep_weight()).max(1.0),
        // The strength the werewolf adds over the unit it replaces for the spell's mana and the
        // unit given up, one for one: a summon of the same strength and cost is better, as it
        // costs no unit.
        Kind::Lycanthropy => crate::wizardry::lycanthropy_target(view, spell).map_or(0.0, |t| t.2),
        _ => cost / (1.0 + upkeep),
    };
    let affinity = view.wizard().map_or(1.0, |w| {
        let total = w.total_books().max(1) as f32;
        let books = w.books_in(def.realm) as f32 / total;
        1.0 + affinity_weight() * books
    });
    base * affinity
}

// The mana a wizard's spells add to a fight, in soldiers (task 502): a tower's guards are worth
// 900 to 3,900, nine soldiers alone do not beat them, but the battle spells the pool can pay for
// do the work of more soldiers.

/// Mana that does the work of one average soldier in a fight (a bolt or a blessing: 15 to 25).
pub(crate) const MANA_PER_SOLDIER: f32 = 20.0;
/// Most soldiers' worth the spells are counted for: a battle lasts a few rounds.
pub(crate) const MOST_SOLDIERS: f32 = 6.0;

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
        .map(|(p, _)| crate::site::expected(view, p) * crate::site::edge())
        .filter(|&need| need > best_nine && need <= reachable)
        .map(|need| ((need - best_nine) / each * MANA_PER_SOLDIER).ceil() as i32)
        .min()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    use crate::testworld::{
        at, fresh_game, game, game_rival_across, learn_a_battle_spell, set_mana, spawn, COAST_CITY,
        P0, P1,
    };
    use crate::wizardry::kind;

    #[test]
    fn a_summon_is_rated_by_the_strength_it_brings_for_its_mana(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let g = fresh_game()?;
        let view = PlayerView::new(&g, P1);
        let mut rated: Vec<(f32, i32, String)> = view
            .content()
            .rules
            .spells
            .iter()
            .filter(|(_, d)| kind(d) == Kind::Summon)
            .map(|(id, d)| {
                (
                    rate(&view, id, Kind::Summon),
                    view.casting_cost(id),
                    d.id.clone(),
                )
            })
            .collect();
        assert!(rated.len() >= 5, "the rules have summons");
        rated.sort_by(|a, b| b.0.total_cmp(&a.0));
        // The ranking is not the ranking by cost: a cheap strong summon beats a dear weak one.
        let by_cost = {
            let mut c = rated.clone();
            c.sort_by_key(|r| std::cmp::Reverse(r.1));
            c
        };
        assert_ne!(
            rated.iter().map(|r| &r.2).collect::<Vec<_>>(),
            by_cost.iter().map(|r| &r.2).collect::<Vec<_>>()
        );
        assert!(rated.iter().all(|r| r.0 > 0.0), "{rated:?}");
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
}
