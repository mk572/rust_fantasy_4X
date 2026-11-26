//! Class 9, the wizard's magic: income and its split, the astrologer's table and the history of power.

use crate::support::worlds;
use f4x_core::player::PlayerId;
use f4x_core::wizard::astrology::HISTORY_TURNS;
use f4x_core::world::World;
use f4x_def::half_int::HalfInt;

fn wizards(w: &World) -> Vec<PlayerId> {
    w.wizards()
}

#[test]
fn every_round_adds_one_point_to_each_wizards_history_and_the_history_has_a_limit() {
    for (name, mut w) in worlds() {
        let ids = wizards(&w);
        w.end_round();
        w.end_round();
        for &p in &ids {
            assert_eq!(w.player(p).expect("a player").history().len(), 2, "{name}");
        }
        for _ in 0..HISTORY_TURNS + 5 {
            w.end_round();
        }
        for &p in &ids {
            assert_eq!(
                w.player(p).expect("a player").history().len(),
                HISTORY_TURNS,
                "{name}: the history keeps the last turns only"
            );
        }
    }
}

#[test]
fn the_astrologers_table_has_a_row_for_each_wizard_and_cuts_each_column_to_its_limit() {
    for (name, w) in worlds() {
        let table = w.astrology_table();
        assert_eq!(table.len(), wizards(&w).len(), "{name}");
        for (p, a, power) in &table {
            assert!(
                a.power <= 200 && a.strength <= 200 && a.research <= 200 && *power <= 160,
                "{name}: {p:?}: {a:?} {power}"
            );
            assert!(a.power >= 0 && a.strength >= 0 && a.research >= 0, "{name}");
        }
    }
}

#[test]
fn the_income_is_never_negative_and_the_shares_do_not_make_power_out_of_nothing() {
    for (name, w) in worlds() {
        for p in wizards(&w) {
            let income = w.income(p);
            assert!(income.power_base >= HalfInt::ZERO, "{name}: {p:?}");
            let s = income.split;
            assert!(
                s.mana >= HalfInt::ZERO && s.skill >= 0,
                "{name}: {p:?}: {s:?}"
            );
            // What goes to mana and to skill comes out of the power base.
            assert!(
                s.mana + HalfInt::from_int(s.skill) <= income.power_base + HalfInt::from_int(1),
                "{name}: {p:?}: {income:?}"
            );
            assert_eq!(
                income.mana_upkeep,
                w.mana_upkeep(p),
                "{name}: the two ways of asking disagree"
            );
        }
    }
}

#[test]
fn another_city_never_lowers_a_wizards_power() {
    for (name, mut w) in worlds() {
        let p = wizards(&w)[0];
        let before = w.income(p).power_base;
        assert!(
            crate::support::another_city(&mut w, p, 5).is_some(),
            "{name}: no room for a city"
        );
        assert!(w.income(p).power_base >= before, "{name}");
    }
}

#[test]
fn a_wizard_casting_the_spell_of_return_has_no_income() {
    let mut tried = 0;
    for (name, mut w) in worlds() {
        let ids = wizards(&w);
        let (a, b) = (ids[0], ids[1]);
        let need = w.content().rules.config.wizard.resign_below_skill;
        w.scenario_set_casting_skill(b, need);
        w.treasury_mut(b).expect("a treasury").mana = HalfInt::from_int(1000);
        let capital = w.economy().fortress_city(b).expect("a fortress city");
        // A second city, so that the wizard is banished and not defeated.
        crate::support::another_city(&mut w, b, 4);
        assert!(
            w.income(b).power_base > HalfInt::ZERO,
            "{name}: no income to lose"
        );
        w.capture_city(capital, a);
        if !w.wizard(b).expect("a wizard").is_banished() {
            continue;
        }
        tried += 1;
        let income = w.income(b);
        assert_eq!(income.power_base, HalfInt::ZERO, "{name}");
        assert_eq!(income.split.mana, HalfInt::ZERO, "{name}");
        assert_eq!(income.research, 0, "{name}");
    }
    assert!(tried > 0, "no wizard was banished");
}
