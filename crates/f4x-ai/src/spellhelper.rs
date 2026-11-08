//! The spell helper (task 488): the AI decides what it wants to spend mana on, an intent (the
//! `Kind` of spell: an army, a buff, a blow at an enemy city), and the helper rates the spells the
//! wizard knows for that intent and returns them best first. The rating uses the mana cost, the
//! upkeep, what the spell brings (the strength of the creatures it summons) and how much the
//! wizard's books favour its realm. The AI never names a spell itself. The weights are numbers
//! in one place, so a run can tune them (`AIK_RATE_*`) and a later fit can learn them.

use f4x_core::battle::strength;
use f4x_core::player::view::PlayerView;
use f4x_def::rules::effects::Effect;
use f4x_def::rules::{SpellDef, SpellId, Stat};

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
            let stats = &view.content().rules.units.get(unit).stats;
            let stat = |s: Stat| stats.get(&s).map_or(0, |v| v.floor());
            let attack = stat(Stat::Melee).max(stat(Stat::Ranged));
            let hits = stat(Stat::MaxHp) * stat(Stat::Figures).max(1);
            strength(attack, stat(Stat::Defense), hits) * f32::from(count)
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
        _ => cost / (1.0 + upkeep),
    };
    let affinity = view.wizard().map_or(1.0, |w| {
        let total = w.total_books().max(1) as f32;
        let books = w.books_in(def.realm) as f32 / total;
        1.0 + affinity_weight() * books
    });
    base * affinity
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{fresh_game, P1};
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
}
