//! Time Stop over whole rounds: nobody's time moves but the keeper's, who earns no gold. What it
//! does to one operation of the world is tested in core (`c9_turn.rs`).

use f4x_core::player::PlayerId;
use f4x_core::world::World;
use f4x_def::half_int::HalfInt;
use f4x_master::Game;

use crate::support::{games, gold, idle_cities, mana, wizards};

/// The population of every city: (figures, tens of growth), by owner.
fn people(w: &World, p: PlayerId) -> Vec<(u32, u32)> {
    w.economy()
        .cities_of(p)
        .into_iter()
        .map(|(c, _)| {
            let pop = w.city_population(c);
            (pop.figures, pop.tens)
        })
        .collect()
}

/// Wizard 0 keeps Time Stop (spell 80, `time_stop`) with mana to pay for it.
fn stopped() -> Vec<(String, Game, Vec<PlayerId>)> {
    games()
        .into_iter()
        .map(|(name, mut w)| {
            let ids = wizards(&w);
            for &p in &ids {
                let t = w.treasury_mut(p).expect("a treasury");
                t.gold = HalfInt::from_int(500);
                t.mana = HalfInt::from_int(2000);
            }
            let spell = w.content().rules.spells.id_numbered(80).expect("time_stop");
            w.scenario_add_global(ids[0], spell);
            idle_cities(&mut w);
            (name, w, ids)
        })
        .collect()
}

#[test]
fn time_stop_halts_growth_gold_and_mana_for_the_others_and_gold_for_the_keeper() {
    for (name, mut w, ids) in stopped() {
        assert_eq!(w.time_stopper(), Some(ids[0]), "{name}");
        let (keeper, others) = (ids[0], &ids[1..]);
        let before: Vec<_> = ids
            .iter()
            .map(|&p| (gold(&w, p), mana(&w, p), people(&w, p)))
            .collect();
        for _ in 0..3 {
            w.play_round();
        }
        assert_eq!(
            gold(&w, keeper),
            before[0].0,
            "{name}: the keeper earned or paid gold"
        );
        assert!(
            mana(&w, keeper) < before[0].1,
            "{name}: Time Stop cost no mana"
        );
        for (i, &o) in others.iter().enumerate() {
            let at = &before[i + 1];
            assert_eq!(
                (gold(&w, o), mana(&w, o), people(&w, o)),
                (at.0, at.1, at.2.clone()),
                "{name}: time moved for {o:?}"
            );
        }
    }
}
