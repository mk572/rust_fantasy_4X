//! Class 9, the wizard's magic: income and its split, the astrologer's table and the history of power.

use crate::support::worlds;
use f4x_core::game::Game;
use f4x_core::player::PlayerId;
use f4x_core::wizard::astrology::HISTORY_TURNS;
use f4x_def::half_int::HalfInt;

fn wizards(g: &Game) -> Vec<PlayerId> {
    g.wizards()
}

#[test]
fn every_round_adds_one_point_to_each_wizards_history_and_the_history_has_a_limit() {
    for (name, mut g) in worlds() {
        let w = wizards(&g);
        g.end_round();
        g.end_round();
        for &p in &w {
            assert_eq!(g.player(p).expect("a player").history().len(), 2, "{name}");
        }
        for _ in 0..HISTORY_TURNS + 5 {
            g.end_round();
        }
        for &p in &w {
            assert_eq!(
                g.player(p).expect("a player").history().len(),
                HISTORY_TURNS,
                "{name}: the history keeps the last turns only"
            );
        }
    }
}

#[test]
fn the_astrologers_table_has_a_row_for_each_wizard_and_cuts_each_column_to_its_limit() {
    for (name, g) in worlds() {
        let table = g.astrology_table();
        assert_eq!(table.len(), wizards(&g).len(), "{name}");
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
    for (name, g) in worlds() {
        for p in wizards(&g) {
            let income = g.income(p);
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
                g.mana_upkeep(p),
                "{name}: the two ways of asking disagree"
            );
        }
    }
}

#[test]
fn another_city_never_lowers_a_wizards_power() {
    for (name, mut g) in worlds() {
        let p = wizards(&g)[0];
        let before = g.income(p).power_base;
        assert!(
            crate::support::another_city(&mut g, p, 5).is_some(),
            "{name}: no room for a city"
        );
        assert!(g.income(p).power_base >= before, "{name}");
    }
}

#[test]
fn a_wizard_casting_the_spell_of_return_has_no_income() {
    let mut tried = 0;
    for (name, mut g) in worlds() {
        let w = wizards(&g);
        let (a, b) = (w[0], w[1]);
        let need = g.content().rules.config.wizard.resign_below_skill;
        g.scenario_set_casting_skill(b, need);
        g.treasury_mut(b).expect("a treasury").mana = HalfInt::from_int(1000);
        let capital = g.economy().fortress_city(b).expect("a fortress city");
        // A second city, so that the wizard is banished and not defeated.
        crate::support::another_city(&mut g, b, 4);
        assert!(
            g.income(b).power_base > HalfInt::ZERO,
            "{name}: no income to lose"
        );
        g.capture_city(capital, a);
        if !g.wizard(b).expect("a wizard").is_banished() {
            continue;
        }
        tried += 1;
        let income = g.income(b);
        assert_eq!(income.power_base, HalfInt::ZERO, "{name}");
        assert_eq!(income.split.mana, HalfInt::ZERO, "{name}");
        assert_eq!(income.research, 0, "{name}");
    }
    assert!(tried > 0, "no wizard was banished");
}
