//! Class 4: the derived values are wrong. The fixed-point numbers, the stat arithmetic, the bit
//! sets and the verdict of a forecast are checked against what they are defined to be, by property
//! over many values (a fixed seed, so a failure repeats) and at the edges of what they hold.
//!
//! The oracle is the definition (`HalfInt` is twice its value in an `i32`, a stat is its base plus
//! adds, times the percent, then the bounds), worked out in `i128` or in plain loops, not a copy
//! of the implementation.

use f4x_def::half_int::HalfInt;
use f4x_def::quarter_int::QuarterInt;
use f4x_def::rules::damage_type::{DamageType, DamageTypes};
use f4x_def::rules::forecast::Forecast;
use f4x_def::rules::stats::{compute, compute_half, ModOp, Modifier, Source, Stat};
use f4x_def::rules::unit_effect::{UnitEffect, UnitEffects};
use f4x_def::rules::unit_flag::{UnitFlag, UnitFlags};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

/// A small deterministic generator (xorshift), so no test depends on chance.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// A number in `lo..=hi`.
    fn between(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

/// Raw halves that a game can hold with room to add and scale (no overflow in the test itself).
const ROOM: i64 = 1 << 24;

fn halves(rng: &mut Rng) -> HalfInt {
    HalfInt::from_raw(rng.between(-ROOM, ROOM) as i32)
}

/// The edges of the raw range, plus the small values around zero.
fn edge_raws() -> Vec<i32> {
    let mut v = vec![i32::MIN, i32::MIN + 1, i32::MAX - 1, i32::MAX];
    v.extend(-9..=9);
    v
}

// ---- HalfInt ----

#[test]
fn a_half_int_is_twice_its_value() {
    for n in -1000..=1000 {
        assert_eq!(HalfInt::from_int(n).raw(), 2 * n);
        assert!(HalfInt::from_int(n).is_whole());
        assert!(!HalfInt::from_raw(2 * n + 1).is_whole());
    }
    assert_eq!(HalfInt::HALF.raw(), 1);
    assert_eq!(HalfInt::ZERO.raw(), 0);
    assert_eq!(HalfInt::default(), HalfInt::ZERO);
}

#[test]
fn from_int_saturates_instead_of_wrapping() {
    assert_eq!(HalfInt::from_int(i32::MAX), HalfInt::from_raw(i32::MAX));
    assert_eq!(HalfInt::from_int(i32::MIN), HalfInt::from_raw(i32::MIN));
}

#[test]
fn floor_and_ceil_bracket_the_value_and_meet_on_wholes() {
    let mut rng = Rng::new(1);
    for _ in 0..5000 {
        let v = halves(&mut rng);
        let (lo, hi) = (i64::from(v.floor()), i64::from(v.ceil()));
        let raw = i64::from(v.raw());
        assert!(2 * lo <= raw && raw <= 2 * hi, "{v}: {lo}..{hi}");
        assert_eq!(hi - lo, i64::from(!v.is_whole()), "{v}");
    }
}

#[test]
fn floor_rounds_towards_negative_infinity() {
    assert_eq!(HalfInt::from_raw(-1).floor(), -1);
    assert_eq!(HalfInt::from_raw(-3).floor(), -2);
    assert_eq!(HalfInt::from_raw(1).floor(), 0);
    assert_eq!(HalfInt::from_raw(-1).ceil(), 0);
}

#[test]
fn half_is_half_rounded_up_to_the_next_half() {
    let mut rng = Rng::new(2);
    for _ in 0..5000 {
        let v = halves(&mut rng);
        let h = i64::from(v.half().raw());
        let raw = i64::from(v.raw());
        // Twice the result is the value or the value plus one step: never less, never more.
        assert!((0..=1).contains(&(2 * h - raw)), "{v}: {}", v.half());
    }
}

#[test]
fn scaled_is_the_exact_product_over_the_divisor_rounded_down() {
    let mut rng = Rng::new(3);
    for _ in 0..5000 {
        let v = halves(&mut rng);
        let num = rng.between(-100_000, 100_000);
        let den = rng.between(1, 1000);
        let exact = i128::from(v.raw()) * i128::from(num);
        // The exact result, or the nearest value the type holds when it is out of range.
        let expect = exact
            .div_euclid(i128::from(den))
            .clamp(i128::from(i32::MIN), i128::from(i32::MAX));
        assert_eq!(
            i128::from(v.scaled(num, den).raw()),
            expect,
            "{v} * {num}/{den}"
        );
    }
}

#[test]
fn percent_of_a_hundred_changes_nothing_and_of_zero_is_zero() {
    let mut rng = Rng::new(4);
    for _ in 0..2000 {
        let v = halves(&mut rng);
        assert_eq!(v.percent(100), v);
        assert_eq!(v.percent(0), HalfInt::ZERO);
        assert_eq!(v.percent_nearest(100), v);
        assert_eq!(v.percent_nearest(0), HalfInt::ZERO);
    }
}

#[test]
fn percent_never_decreases_as_the_percent_rises_for_a_positive_value() {
    let mut rng = Rng::new(5);
    for _ in 0..2000 {
        let v = HalfInt::from_raw(rng.between(0, ROOM) as i32);
        let (a, b) = (rng.between(0, 500) as i32, rng.between(0, 500) as i32);
        let (lo, hi) = (a.min(b), a.max(b));
        assert!(v.percent(lo) <= v.percent(hi));
        assert!(v.percent_nearest(lo) <= v.percent_nearest(hi));
    }
}

#[test]
fn percent_nearest_is_within_half_a_step_of_the_exact_value() {
    let mut rng = Rng::new(6);
    for _ in 0..5000 {
        let v = halves(&mut rng);
        let p = rng.between(-300, 300) as i32;
        let exact = i128::from(v.raw()) * i128::from(p); // in hundredths of a step
        let got = i128::from(v.percent_nearest(p).raw()) * 100;
        assert!((got - exact).abs() <= 50, "{v} at {p}%");
        let down = i128::from(v.percent(p).raw()) * 100;
        assert!(down <= exact && exact - down < 100, "{v} at {p}%");
    }
}

#[test]
fn addition_is_exact_commutative_and_undone_by_subtraction() {
    let mut rng = Rng::new(7);
    for _ in 0..5000 {
        let (a, b) = (halves(&mut rng), halves(&mut rng));
        assert_eq!((a + b).raw(), a.raw() + b.raw());
        assert_eq!(a + b, b + a);
        assert_eq!(a + b - b, a);
        assert_eq!(-(-a), a);
        assert_eq!(a - b, a + -b);
        let mut c = a;
        c += b;
        c -= b;
        assert_eq!(c, a);
    }
}

#[test]
fn whole_number_operands_count_as_that_many_whole_units() {
    let mut rng = Rng::new(8);
    for _ in 0..2000 {
        let v = halves(&mut rng);
        let n = rng.between(-5000, 5000) as i32;
        assert_eq!(v + n, v + HalfInt::from_int(n));
        assert_eq!(v - n, v - HalfInt::from_int(n));
        assert_eq!(v == n, v == HalfInt::from_int(n));
        assert_eq!(v.partial_cmp(&n), v.partial_cmp(&HalfInt::from_int(n)));
        let mut w = v;
        w += n;
        w -= n;
        assert_eq!(w, v);
    }
}

#[test]
fn multiplying_and_dividing_by_a_whole_number() {
    let mut rng = Rng::new(9);
    for _ in 0..3000 {
        let v = HalfInt::from_raw(rng.between(-100_000, 100_000) as i32);
        let k = rng.between(1, 1000) as i32;
        assert_eq!((v * k).raw(), v.raw() * k);
        // Division is by whole units and rounds towards zero.
        assert_eq!((v / k).raw(), v.raw() / k);
        assert!(((v / k).raw().abs()) <= v.raw().abs());
    }
}

#[test]
fn the_sum_of_a_list_is_the_sum_of_its_raws() {
    let mut rng = Rng::new(10);
    let list: Vec<HalfInt> = (0..200).map(|_| halves(&mut rng)).collect();
    let total: i64 = list.iter().map(|v| i64::from(v.raw())).sum();
    assert_eq!(
        i64::from(list.iter().copied().sum::<HalfInt>().raw()),
        total
    );
    assert_eq!(
        Vec::<HalfInt>::new().into_iter().sum::<HalfInt>(),
        HalfInt::ZERO
    );
}

#[test]
fn saturating_sub_never_goes_below_zero() {
    let mut rng = Rng::new(11);
    for _ in 0..3000 {
        let (a, b) = (halves(&mut rng), halves(&mut rng));
        let got = a.saturating_sub(b);
        assert!(got >= HalfInt::ZERO);
        assert_eq!(got, if a > b { a - b } else { HalfInt::ZERO });
    }
}

#[test]
fn abs_and_the_float_form_agree_with_the_value() {
    let mut rng = Rng::new(12);
    for _ in 0..2000 {
        let v = HalfInt::from_raw(rng.between(-100_000, 100_000) as i32);
        assert_eq!(v.abs().raw(), v.raw().abs());
        assert_eq!(f64::from(v.as_f32()), f64::from(v.raw()) / 2.0);
    }
}

#[test]
fn the_order_is_the_order_of_the_values() {
    let mut rng = Rng::new(13);
    for _ in 0..3000 {
        let (a, b) = (halves(&mut rng), halves(&mut rng));
        assert_eq!(a.cmp(&b), a.raw().cmp(&b.raw()));
        assert_eq!(a.max(b).raw(), a.raw().max(b.raw()));
    }
}

#[test]
fn display_shows_wholes_and_halves_and_agrees_with_the_value() {
    let mut rng = Rng::new(14);
    for _ in 0..3000 {
        let v = HalfInt::from_raw(rng.between(-100_000, 100_000) as i32);
        let shown: f64 = v.to_string().parse().expect("a number");
        assert_eq!(shown, f64::from(v.raw()) / 2.0, "{v}");
        assert_eq!(v.to_string().contains('.'), !v.is_whole(), "{v}");
        if v.raw() >= 0 {
            assert!(format!("{v:+}").starts_with('+'));
        }
    }
    for raw in edge_raws() {
        // Even the edges are shown, without a panic.
        let _ = HalfInt::from_raw(raw).to_string();
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Holder(HalfInt);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct RawHolder(#[serde(with = "f4x_def::half_int::raw")] HalfInt);

#[test]
fn a_half_int_is_written_as_its_amount_and_read_back() {
    let mut rng = Rng::new(15);
    let mut raws: Vec<i32> = (0..3000).map(|_| rng.between(-ROOM, ROOM) as i32).collect();
    raws.extend(edge_raws());
    for raw in raws {
        let v = HalfInt::from_raw(raw);
        let text = ron::to_string(&Holder(v)).expect("written");
        let back: Holder = ron::from_str(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(back.0, v, "{text}");
        let text = ron::to_string(&RawHolder(v)).expect("written");
        assert_eq!(ron::from_str::<RawHolder>(&text).expect("read").0, v);
    }
}

#[test]
fn a_file_may_spell_an_amount_in_any_of_its_forms() {
    let read = |s: &str| ron::from_str::<HalfInt>(s);
    assert_eq!(read("3").expect("whole"), HalfInt::from_int(3));
    assert_eq!(read("3.0").expect("whole as a float"), HalfInt::from_int(3));
    assert_eq!(read("-2.5").expect("negative half"), HalfInt::from_raw(-5));
    assert_eq!(read("0.5").expect("half"), HalfInt::HALF);
    assert_eq!(read("0").expect("zero"), HalfInt::ZERO);
    assert_eq!(read("-0.0").expect("negative zero"), HalfInt::ZERO);
}

#[test]
fn an_amount_that_is_not_a_whole_or_a_half_is_refused() {
    for text in [
        "0.3",
        "0.25",
        "1.75",
        "-0.1",
        "1e-9",
        "0.5000001",
        "NaN",
        "inf",
        "-inf",
        "1e10",
        "-1e10",
        "1.1e9",
        "99999999999999999999",
        "-99999999999999999999",
    ] {
        assert!(
            ron::from_str::<HalfInt>(text).is_err(),
            "{text} was accepted"
        );
    }
    for text in ["\"3\"", "true", "()", "[1]", "None", "Some(1)", "'a'", ""] {
        assert!(
            ron::from_str::<HalfInt>(text).is_err(),
            "{text} was accepted"
        );
    }
}

#[test]
fn the_largest_amount_that_fits_is_read_and_one_more_is_refused() {
    let top = i64::from(i32::MAX) / 2; // whole units that fit when doubled
    assert!(ron::from_str::<HalfInt>(&top.to_string()).is_ok());
    assert!(ron::from_str::<HalfInt>(&(top + 1).to_string()).is_err());
    assert!(ron::from_str::<HalfInt>(&(-top).to_string()).is_ok());
    assert!(ron::from_str::<HalfInt>(&(-top - 2).to_string()).is_err());
}

// ---- QuarterInt ----

fn quarters(rng: &mut Rng) -> QuarterInt {
    QuarterInt::from_raw(rng.between(-ROOM, ROOM) as i32)
}

#[test]
fn a_quarter_int_is_four_times_its_value() {
    for n in -1000..=1000 {
        assert_eq!(QuarterInt::from_int(n).raw(), 4 * n);
        assert!(QuarterInt::from_int(n).is_whole());
        assert!(!QuarterInt::from_raw(4 * n + 1).is_whole());
    }
    assert_eq!(QuarterInt::QUARTER.raw(), 1);
    assert_eq!(QuarterInt::from_int(i32::MAX).raw(), i32::MAX);
    assert_eq!(QuarterInt::from_int(i32::MIN).raw(), i32::MIN);
}

#[test]
fn a_half_int_becomes_the_same_amount_in_quarters_and_half_of_it_is_exact() {
    let mut rng = Rng::new(20);
    for _ in 0..3000 {
        let h = halves(&mut rng);
        assert_eq!(QuarterInt::from_half(h).raw(), 2 * h.raw());
        assert_eq!(QuarterInt::from(h), QuarterInt::from_half(h));
        // Half of an amount, twice, is the amount.
        assert_eq!(
            QuarterInt::half_of(h) + QuarterInt::half_of(h),
            QuarterInt::from_half(h)
        );
        assert_eq!(QuarterInt::from_half(h).floor_half(), h);
    }
}

#[test]
fn quarter_floor_and_ceil_bracket_the_value() {
    let mut rng = Rng::new(21);
    for _ in 0..5000 {
        let q = quarters(&mut rng);
        let (lo, hi) = (i64::from(q.floor()), i64::from(q.ceil()));
        let raw = i64::from(q.raw());
        assert!(4 * lo <= raw && raw <= 4 * hi, "{q}");
        assert_eq!(hi - lo, i64::from(!q.is_whole()), "{q}");
        let fh = i64::from(q.floor_half().raw());
        assert!(2 * fh <= raw && raw - 2 * fh < 2, "{q}");
    }
}

#[test]
fn quarter_percent_is_exact_and_rounds_down() {
    let mut rng = Rng::new(22);
    for _ in 0..5000 {
        let q = QuarterInt::from_raw(rng.between(-100_000, 100_000) as i32);
        let p = rng.between(-300, 300) as i32;
        let exact = i128::from(q.raw()) * i128::from(p);
        assert_eq!(i128::from(q.percent(p).raw()), exact.div_euclid(100));
        assert_eq!(q.percent(100), q);
    }
}

#[test]
fn quarter_arithmetic_is_exact_and_summed() {
    let mut rng = Rng::new(23);
    let list: Vec<QuarterInt> = (0..100).map(|_| quarters(&mut rng)).collect();
    for w in list.windows(2) {
        assert_eq!((w[0] + w[1]).raw(), w[0].raw() + w[1].raw());
        assert_eq!(w[0] + w[1] - w[1], w[0]);
        assert_eq!(-(-w[0]), w[0]);
        let mut c = w[0];
        c += w[1];
        c -= w[1];
        assert_eq!(c, w[0]);
    }
    let total: i64 = list.iter().map(|q| i64::from(q.raw())).sum();
    assert_eq!(
        i64::from(list.iter().copied().sum::<QuarterInt>().raw()),
        total
    );
    let h = HalfInt::from_raw(3);
    assert_eq!(QuarterInt::from_int(1) + h, QuarterInt::from_raw(4 + 6));
}

#[test]
fn a_quarter_is_shown_and_written_as_its_amount_and_read_back() {
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Q(QuarterInt);
    let mut rng = Rng::new(24);
    let mut raws: Vec<i32> = (0..3000).map(|_| rng.between(-ROOM, ROOM) as i32).collect();
    raws.extend(edge_raws());
    for raw in raws {
        let q = QuarterInt::from_raw(raw);
        let text = ron::to_string(&Q(q)).expect("written");
        assert_eq!(
            ron::from_str::<Q>(&text)
                .unwrap_or_else(|e| panic!("{text}: {e}"))
                .0,
            q
        );
        let shown: f64 = q.to_string().parse().expect("a number");
        assert_eq!(shown, f64::from(raw) / 4.0, "{q}");
    }
}

#[test]
fn an_amount_that_is_not_a_multiple_of_a_quarter_is_refused() {
    for text in [
        "0.1", "0.125", "NaN", "inf", "1e10", "-1e10", "\"1\"", "()", "",
    ] {
        assert!(
            ron::from_str::<QuarterInt>(text).is_err(),
            "{text} was accepted"
        );
    }
    for (text, raw) in [("0.25", 1), ("-0.75", -3), ("2", 8), ("1.5", 6)] {
        assert_eq!(ron::from_str::<QuarterInt>(text).expect(text).raw(), raw);
    }
}

// ---- stats ----

fn modifier(stat: Stat, op: ModOp, value: i32) -> Modifier {
    Modifier::new(stat, op, value, Source::Other)
}

/// The stat as its definition says: base plus adds, times (100 plus the percents) over 100 rounded
/// down, then the lowest Max and the highest Min, then the lowest Set, then the stat's floor.
/// A Min on the moves raises the base before the adds.
fn model(stat: Stat, base: i64, mods: &[Modifier]) -> i64 {
    let of = |op: ModOp| -> Vec<i64> {
        mods.iter()
            .filter(|m| m.stat == stat && m.op == op)
            .map(|m| i64::from(m.value.floor()))
            .collect()
    };
    let min = of(ModOp::Min).into_iter().max();
    let max = of(ModOp::Max).into_iter().min();
    let set = of(ModOp::Set).into_iter().min();
    let mut base = base;
    if let (Stat::Moves, Some(m)) = (stat, min) {
        base = base.max(m);
    }
    let total = base + of(ModOp::Add).iter().sum::<i64>();
    let factor = (100 + of(ModOp::Percent).iter().sum::<i64>()).max(0);
    let mut v = (total * factor).div_euclid(100);
    if let (Some(m), false) = (min, stat == Stat::Moves) {
        v = v.max(m);
    }
    if let Some(m) = max {
        v = v.min(m);
    }
    if let Some(s) = set {
        v = s;
    }
    match stat.floor() {
        Some(f) => v.max(i64::from(f)),
        None => v,
    }
}

fn random_mods(rng: &mut Rng, stat: Stat) -> Vec<Modifier> {
    let ops = [
        ModOp::Add,
        ModOp::Percent,
        ModOp::Min,
        ModOp::Max,
        ModOp::Set,
    ];
    (0..rng.between(0, 8))
        .map(|_| {
            let other = Stat::iter()
                .nth(rng.between(0, 5) as usize)
                .expect("a stat");
            let s = if rng.between(0, 3) == 0 { other } else { stat };
            modifier(
                s,
                ops[rng.between(0, 4) as usize],
                rng.between(-300, 300) as i32,
            )
        })
        .collect()
}

#[test]
fn a_stat_is_computed_as_its_definition_says() {
    let mut rng = Rng::new(30);
    for stat in Stat::iter() {
        for _ in 0..300 {
            let base = rng.between(-200, 200);
            let mods = random_mods(&mut rng, stat);
            assert_eq!(
                i64::from(compute(stat, base as i32, &mods)),
                model(stat, base, &mods),
                "{stat:?} base {base} {mods:?}"
            );
        }
    }
}

#[test]
fn the_order_the_modifiers_come_in_does_not_matter() {
    let mut rng = Rng::new(31);
    for stat in Stat::iter() {
        for _ in 0..50 {
            let base = rng.between(-100, 100) as i32;
            let mut mods = random_mods(&mut rng, stat);
            let before = compute(stat, base, &mods);
            let before_half = compute_half(stat, HalfInt::from_int(base), &mods);
            for _ in 0..5 {
                for i in (1..mods.len()).rev() {
                    mods.swap(i, rng.between(0, i as i64) as usize);
                }
                assert_eq!(compute(stat, base, &mods), before, "{stat:?}");
                assert_eq!(
                    compute_half(stat, HalfInt::from_int(base), &mods),
                    before_half
                );
            }
        }
    }
}

#[test]
fn modifiers_of_other_stats_change_nothing() {
    let mut rng = Rng::new(32);
    for stat in Stat::iter() {
        let base = rng.between(0, 50) as i32;
        let others: Vec<Modifier> = Stat::iter()
            .filter(|s| *s != stat)
            .map(|s| modifier(s, ModOp::Add, 7))
            .collect();
        assert_eq!(
            compute(stat, base, &others),
            compute(stat, base, &[]),
            "{stat:?}"
        );
    }
}

#[test]
fn a_stat_with_a_floor_never_goes_below_it_and_one_without_may() {
    for stat in Stat::iter() {
        let v = compute(stat, 0, &[modifier(stat, ModOp::Add, -1000)]);
        match stat.floor() {
            Some(floor) => assert_eq!(v, floor, "{stat:?}"),
            None => assert_eq!(v, -1000, "{stat:?}"),
        }
    }
}

#[test]
fn a_percent_below_minus_a_hundred_counts_as_nothing_not_as_a_negative_stat() {
    assert_eq!(
        compute(
            Stat::Melee,
            10,
            &[modifier(Stat::Melee, ModOp::Percent, -500)]
        ),
        0
    );
}

#[test]
fn the_lowest_set_wins_and_set_beats_every_other_op() {
    let m = |op, v| modifier(Stat::Defense, op, v);
    let mods = [
        m(ModOp::Add, 50),
        m(ModOp::Set, 9),
        m(ModOp::Set, 4),
        m(ModOp::Min, 30),
    ];
    assert_eq!(compute(Stat::Defense, 1, &mods), 4);
}

#[test]
fn compute_does_not_overflow_at_the_edges_of_the_base_and_of_a_modifier() {
    for base in [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX] {
        for value in [i32::MIN, -1, 0, 1, i32::MAX] {
            for op in [
                ModOp::Add,
                ModOp::Percent,
                ModOp::Min,
                ModOp::Max,
                ModOp::Set,
            ] {
                let mods = [
                    modifier(Stat::CityUnrest, op, value),
                    modifier(Stat::CityUnrest, ModOp::Add, value),
                ];
                let _ = compute(Stat::CityUnrest, base, &mods);
                let _ = compute_half(Stat::CityUnrest, HalfInt::from_raw(base), &mods);
            }
        }
    }
}

#[test]
fn the_half_form_agrees_with_the_whole_form_on_whole_amounts() {
    let mut rng = Rng::new(33);
    for stat in Stat::iter() {
        for _ in 0..100 {
            let base = rng.between(0, 60) as i32;
            let mods = random_mods(&mut rng, stat);
            // Only adds, bounds and sets that are whole: percent of a whole may leave a half.
            let mods: Vec<Modifier> = mods
                .into_iter()
                .filter(|m| m.op != ModOp::Percent)
                .collect();
            assert_eq!(
                compute_half(stat, HalfInt::from_int(base), &mods),
                HalfInt::from_int(compute(stat, base, &mods)),
                "{stat:?}"
            );
        }
    }
}

#[test]
fn half_amounts_are_added_exactly_and_the_percent_rounds_down_to_a_half() {
    let m = |op, v: HalfInt| Modifier::amount(Stat::CityFood, op, v, Source::Other);
    let adds = [
        m(ModOp::Add, HalfInt::HALF),
        m(ModOp::Add, HalfInt::from_int(2)),
    ];
    assert_eq!(
        compute_half(Stat::CityFood, HalfInt::from_int(1), &adds),
        HalfInt::from_raw(7)
    );
    let mut rng = Rng::new(34);
    for _ in 0..2000 {
        let base = HalfInt::from_raw(rng.between(0, 5000) as i32);
        let p = rng.between(-100, 400) as i32;
        let got = compute_half(
            Stat::CityFood,
            base,
            &[m(ModOp::Percent, HalfInt::from_int(p))],
        );
        assert_eq!(
            got,
            base.scaled(i64::from((100 + p).max(0)), 100),
            "{base} at {p}"
        );
    }
}

// ---- bit sets ----

#[test]
fn a_bit_set_holds_what_was_put_in_and_nothing_else() {
    for a in DamageType::iter() {
        let set = DamageTypes::of(&[a]);
        for b in DamageType::iter() {
            assert_eq!(set.contains(b), a == b);
        }
        assert!(!set.without(a).contains(a) && set.without(a).is_empty());
    }
    for a in UnitFlag::iter() {
        let set = UnitFlags::of(&[a]);
        assert!(UnitFlag::iter().all(|b| set.contains(b) == (a == b)));
        assert!(!set.without(a).contains(a));
    }
    for a in UnitEffect::NAMES
        .iter()
        .filter_map(|n| UnitEffect::named(n))
    {
        let mut set = UnitEffects::NONE;
        set.insert(a);
        assert!(UnitEffect::NAMES
            .iter()
            .filter_map(|n| UnitEffect::named(n))
            .all(|b| set.contains(b) == (a == b)));
    }
}

#[test]
fn every_variant_of_a_bit_set_has_a_bit_of_its_own_that_fits() {
    // A new variant that does not fit the width of its set would shift out of range.
    assert!(DamageType::iter().count() <= 32);
    assert!(UnitFlag::iter().count() <= 16);
    assert!(UnitEffect::NAMES.len() <= 32);
    let all = DamageType::iter().fold(DamageTypes::NONE, DamageTypes::with);
    assert_eq!(all.iter().count(), DamageType::iter().count());
    let all = UnitFlag::iter().fold(UnitFlags::NONE, UnitFlags::with);
    assert!(UnitFlag::iter().all(|f| all.contains(f)));
}

#[test]
fn set_algebra_follows_the_sets() {
    let types: Vec<DamageType> = DamageType::iter().collect();
    let mut rng = Rng::new(40);
    let pick = |rng: &mut Rng| {
        let v: Vec<DamageType> = types
            .iter()
            .copied()
            .filter(|_| rng.between(0, 1) == 1)
            .collect();
        (DamageTypes::of(&v), v)
    };
    for _ in 0..500 {
        let ((a, av), (b, bv)) = (pick(&mut rng), pick(&mut rng));
        for t in &types {
            assert_eq!(a.both(b).contains(*t), av.contains(t) && bv.contains(t));
            assert_eq!(a.minus(b).contains(*t), av.contains(t) && !bv.contains(t));
            let mut u = a;
            u.union(b);
            assert_eq!(u.contains(*t), av.contains(t) || bv.contains(t));
        }
        assert_eq!(a.intersects(b), av.iter().any(|t| bv.contains(t)));
        assert_eq!(a.contains_all(b), bv.iter().all(|t| av.contains(t)));
        assert_eq!(a.is_empty(), av.is_empty());
        let listed: Vec<DamageType> = a.iter().collect();
        assert_eq!(listed, av, "iteration is in the order of the types");
    }
}

#[test]
fn a_damage_type_set_is_written_as_its_list_and_read_back() {
    let all = DamageType::iter().fold(DamageTypes::NONE, DamageTypes::with);
    for set in [
        DamageTypes::NONE,
        all,
        DamageTypes::of(&[DamageType::Fire, DamageType::Magic]),
    ] {
        let text = ron::to_string(&set).expect("written");
        assert_eq!(ron::from_str::<DamageTypes>(&text).expect("read"), set);
    }
    // A repeated type in a file is one type.
    let twice: DamageTypes = ron::from_str("[Fire, Fire]").expect("read");
    assert_eq!(twice, DamageTypes::of(&[DamageType::Fire]));
    assert!(ron::from_str::<DamageTypes>("[NoSuchType]").is_err());
}

#[test]
fn the_realm_damage_types_and_the_realm_numbers_map_both_ways() {
    for t in DamageType::iter() {
        if let Some(n) = t.realm_num() {
            assert_eq!(DamageType::of_realm(n), Some(t));
        }
    }
    for n in 0..=u16::MAX {
        if let Some(t) = DamageType::of_realm(n) {
            assert_eq!(t.realm_num(), Some(n));
        }
    }
    assert_eq!(
        DamageType::iter()
            .filter(|t| t.realm_num().is_some())
            .count(),
        (0..=u16::MAX).filter_map(DamageType::of_realm).count()
    );
}

// ---- forecast ----

#[test]
fn a_forecast_follows_the_ratio_of_the_worths() {
    let bands = [20, 50, 80];
    assert_eq!(Forecast::of_worths(0.0, 0.0, bands), None);
    assert_eq!(Forecast::of_worths(-1.0, 0.0, bands), None);
    let mut rng = Rng::new(50);
    let mut last: Option<Forecast> = None;
    let enemy = 100.0;
    // As our worth rises past the enemy's, the verdict never gets worse.
    for own in (0..=1000).map(|n| n as f32) {
        let f = Forecast::of_worths(own, enemy, bands).expect("a verdict");
        if let Some(l) = last {
            assert!(f >= l, "{own}: {f:?} after {l:?}");
        }
        last = Some(f);
    }
    assert_eq!(
        Forecast::of_worths(100.0, 100.0, bands),
        Some(Forecast::Even)
    );
    for _ in 0..2000 {
        let (a, b) = (rng.between(1, 10_000) as f32, rng.between(1, 10_000) as f32);
        let ab = Forecast::of_worths(a, b, bands).expect("a verdict");
        let ba = Forecast::of_worths(b, a, bands).expect("a verdict");
        // Swapping the sides mirrors the verdict (the list is ordered worst to best, mirrored).
        let mirror = |f: Forecast| match f {
            Forecast::CompletelyOutmatched => Forecast::OtherOutmatched,
            Forecast::MuchWeaker => Forecast::MuchStronger,
            Forecast::SlightlyWeaker => Forecast::SlightlyStronger,
            Forecast::Even => Forecast::Even,
            Forecast::SlightlyStronger => Forecast::SlightlyWeaker,
            Forecast::MuchStronger => Forecast::MuchWeaker,
            Forecast::OtherOutmatched => Forecast::CompletelyOutmatched,
        };
        assert_eq!(mirror(ab), ba, "{a} against {b}");
    }
}

#[test]
fn a_side_worth_nothing_against_a_side_that_is_worth_something_is_outmatched() {
    assert_eq!(
        Forecast::of_worths(0.0, 5.0, [20, 50, 80]),
        Some(Forecast::CompletelyOutmatched)
    );
    assert_eq!(
        Forecast::of_worths(5.0, 0.0, [20, 50, 80]),
        Some(Forecast::OtherOutmatched)
    );
}

#[test]
fn every_forecast_has_its_own_message_key() {
    let all = [
        Forecast::CompletelyOutmatched,
        Forecast::MuchWeaker,
        Forecast::SlightlyWeaker,
        Forecast::Even,
        Forecast::SlightlyStronger,
        Forecast::MuchStronger,
        Forecast::OtherOutmatched,
    ];
    let keys: std::collections::BTreeSet<_> = all.iter().map(|f| f.text_key()).collect();
    assert_eq!(keys.len(), all.len());
}

// ---- the edges of the types ----

#[test]
fn the_pure_functions_of_a_half_int_hold_at_the_edges_of_the_type() {
    for raw in edge_raws() {
        let v = HalfInt::from_raw(raw);
        let r = i128::from(raw);
        assert_eq!(i128::from(v.floor()), r.div_euclid(2));
        assert_eq!(i128::from(v.ceil()), (r + 1).div_euclid(2));
        assert_eq!(i128::from(v.half().raw()), (r + 1).div_euclid(2));
        assert_eq!(
            i128::from(v.plus_half().raw()),
            (r + 1).min(i128::from(i32::MAX))
        );
        assert_eq!(i128::from(v.abs().raw()), r.abs().min(i128::from(i32::MAX)));
        assert_eq!(v.is_whole(), r % 2 == 0);
        let _ = (v.as_f32(), v.to_string(), format!("{v:+}"));
        for other in edge_raws() {
            let w = HalfInt::from_raw(other);
            let diff = i128::from(raw) - i128::from(other);
            assert_eq!(
                i128::from(v.saturating_sub(w).raw()),
                diff.clamp(0, i128::from(i32::MAX))
            );
        }
        for num in [i64::MIN, -1, 0, 1, 2, 100, i64::from(i32::MAX), i64::MAX] {
            for den in [i64::MIN, -100, -1, 1, 2, 100, i64::MAX] {
                let exact = (r * i128::from(num)).div_euclid(i128::from(den));
                let want = exact.clamp(i128::from(i32::MIN), i128::from(i32::MAX));
                assert_eq!(
                    i128::from(v.scaled(num, den).raw()),
                    want,
                    "{raw} * {num} / {den}"
                );
            }
        }
        for p in [i32::MIN, -100, -1, 0, 1, 50, 100, 101, 1000, i32::MAX] {
            let down = (r * i128::from(p))
                .div_euclid(100)
                .clamp(i128::from(i32::MIN), i128::from(i32::MAX));
            let near = (r * i128::from(p) + 50)
                .div_euclid(100)
                .clamp(i128::from(i32::MIN), i128::from(i32::MAX));
            assert_eq!(i128::from(v.percent(p).raw()), down, "{raw} at {p}%");
            assert_eq!(
                i128::from(v.percent_nearest(p).raw()),
                near,
                "{raw} at {p}%"
            );
        }
    }
}

#[test]
fn the_largest_half_int_has_a_ceiling_and_a_half_for_a_search_that_starts_from_it() {
    assert_eq!(HalfInt::MAX.raw(), i32::MAX);
    assert!(HalfInt::MAX > HalfInt::from_int(1_000_000));
    assert_eq!(HalfInt::MAX.plus_half(), HalfInt::MAX);
    let _ = (HalfInt::MAX.ceil(), HalfInt::MAX.half());
}

#[test]
fn the_pure_functions_of_a_quarter_int_hold_at_the_edges_of_the_type() {
    for raw in edge_raws() {
        let q = QuarterInt::from_raw(raw);
        let r = i128::from(raw);
        assert_eq!(i128::from(q.floor()), r.div_euclid(4));
        assert_eq!(i128::from(q.ceil()), (r + 3).div_euclid(4));
        assert_eq!(i128::from(q.floor_half().raw()), r.div_euclid(2));
        assert_eq!(q.is_whole(), r % 4 == 0);
        let _ = (q.as_f32(), q.to_string(), format!("{q:+}"));
        for p in [i32::MIN, -100, -1, 0, 1, 50, 100, 101, 1000, i32::MAX] {
            let want = (r * i128::from(p))
                .div_euclid(100)
                .clamp(i128::from(i32::MIN), i128::from(i32::MAX));
            assert_eq!(i128::from(q.percent(p).raw()), want, "{raw} at {p}%");
        }
        let h = HalfInt::from_raw(raw);
        assert_eq!(
            i128::from(QuarterInt::from_half(h).raw()),
            (r * 2).clamp(i128::from(i32::MIN), i128::from(i32::MAX))
        );
        assert_eq!(QuarterInt::half_of(h).raw(), raw);
    }
}

#[test]
fn the_smallest_and_largest_amounts_are_read_back_and_one_beyond_is_refused() {
    for (text, ok) in [
        ("-1073741824", true),
        ("-1073741824.5", false),
        ("1073741823.5", true),
        ("1073741824", false),
        ("-1073741825", false),
    ] {
        assert_eq!(ron::from_str::<HalfInt>(text).is_ok(), ok, "{text}");
    }
    for (text, ok) in [
        ("-536870912", true),
        ("-536870912.25", false),
        ("536870911.75", true),
        ("536870912", false),
    ] {
        assert_eq!(ron::from_str::<QuarterInt>(text).is_ok(), ok, "{text}");
    }
}

// ---- the settings arithmetic ----

#[test]
fn counts_scaled_to_a_map_do_not_overflow_for_any_size() {
    use f4x_def::settings::{scaled_count, BasicSettings, GameSettings};
    for base in [0, 1, 6, u32::MAX] {
        for w in [0, 1, 90, u32::from(u16::MAX), u32::MAX] {
            for h in [0, 1, 60, u32::from(u16::MAX), u32::MAX] {
                let exact =
                    (u128::from(base) * u128::from(w) * u128::from(h) + 1200).div_euclid(2400);
                assert_eq!(
                    u128::from(scaled_count(base, w, h)),
                    exact.min(u128::from(u32::MAX)),
                    "{base} {w} {h}"
                );
            }
        }
    }
    for (w, h, opponents) in [
        (0, 0, 0),
        (u16::MAX, u16::MAX, 0),
        (u16::MAX, u16::MAX, u8::MAX),
        (1, 1, u8::MAX),
    ] {
        let b = BasicSettings {
            map_width: w,
            map_height: h,
            opponents,
            ..BasicSettings::default()
        };
        let _ = (
            b.tower_count(),
            b.neutral_city_count(),
            b.squares_per_player(),
            b.crowding(),
        );
        let g = GameSettings {
            basic: b,
            ..GameSettings::default()
        };
        let _ = (
            g.node_count(0),
            g.node_count(1),
            g.strong_site_count(),
            g.weak_site_count(),
        );
    }
}

/// The default game as the text of a file, to change one number of at a time.
fn default_game_text() -> String {
    ron::ser::to_string_pretty(
        &f4x_def::settings::GameSettings::default(),
        ron::ser::PrettyConfig::default(),
    )
    .expect("written")
}

#[test]
fn whatever_numbers_a_settings_record_holds_it_is_brought_into_range_and_only_then() {
    use f4x_def::settings::GameSettings;
    let text = default_game_text();
    let spots = crate::support::numbers_in(&text);
    assert!(spots.len() > 30, "the settings have numbers");
    let mut values: Vec<String> = (0..=300).map(|n| n.to_string()).collect();
    values.extend(["1000", "65535", "65536", "4294967295", "-1", "0.5", "1.5"].map(String::from));
    let (mut parsed, mut was_invalid) = (0, 0);
    let mut unmended = std::collections::BTreeSet::new();
    for (a, b) in spots {
        for v in &values {
            let changed = format!("{}{v}{}", &text[..a], &text[b..]);
            let Ok(g) = ron::from_str::<GameSettings>(&changed) else {
                continue;
            };
            parsed += 1;
            let n = g.normalised();
            if !n.validate().is_empty() {
                unmended.insert(format!(
                    "{} (a number {} made {v}): {:?}",
                    text[a.saturating_sub(30)..a].replace('\n', " "),
                    &text[a..b],
                    n.validate()
                ));
            }
            assert_eq!(
                n.normalised(),
                n,
                "normalising twice changes it: {} -> {v}",
                &text[a..b]
            );
            if g.validate().is_empty() {
                assert_eq!(n, g, "a valid record was changed: {} -> {v}", &text[a..b]);
            } else {
                was_invalid += 1;
            }
        }
    }
    // The settings that are unset by default are not in the text: set them.
    for v in 0..=u8::MAX {
        let mut g = GameSettings::default();
        g.world.nodes_arcanus = Some(v);
        g.world.nodes_myrror = Some(v);
        g.world.strong_sites = Some(v);
        g.world.weak_sites = Some(v);
        g.basic.towers = Some(v);
        g.basic.neutral_cities = Some(v);
        assert_eq!(
            g.normalised().validate(),
            Vec::<f4x_def::issue::ContentIssue>::new(),
            "{v}"
        );
    }
    assert!(
        unmended.is_empty(),
        "values that normalising leaves out of range:\n{}",
        unmended.iter().cloned().collect::<Vec<_>>().join("\n")
    );
    assert!(
        parsed > 500 && was_invalid > 50,
        "the attack reached too little ({parsed}, {was_invalid})"
    );
}
