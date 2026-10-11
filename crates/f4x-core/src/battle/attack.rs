//! Attack resolution: dice, blocks, figures, and the order of a melee exchange.
//!
//! These are pure functions over the numbers in [`Combatant`] and [`DefenderState`] and a
//! seeded random generator, so every rule can be tested without a game.
//!
//! The rules follow the original's `Battle_Unit_Process_Attack` and
//! `Battle_Unit_Attack_Target`:
//! - Each attacking figure rolls `strength` ten-sided dice. A die hits on 8, 9 or 10 (30
//!   percent), each plus to hit lowers the number needed by one, and a 10 always hits.
//! - The defender rolls `defense` dice against that figure's hits, each blocking on the same
//!   terms; only the hits left over damage the defender.
//! - Damage lands on the front figure first. A figure that dies passes the rest to the next
//!   one, which rolls its defense again against the overflow.
//! - In a melee exchange the attacker's thrown or breath attack comes first, then a first
//!   striker's blows, then the defender hits back with the figures it has left, and last the
//!   attacker's ordinary blows with the figures it has left.

use rand::{Rng, RngCore};

use super::immunity::{effective_defense, AttackNature, Immunities};

/// How an attack is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Melee,
    /// A thrown weapon or breath, used just before melee.
    ThrownOrBreath,
    /// Arrows, boulders or spell shots from a distance.
    Ranged,
}

/// The numbers an attacker brings to one attack.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Combatant {
    /// Dice per attacking figure.
    pub strength: i32,
    /// Pluses to hit (each is ten percentage points); may be negative.
    pub to_hit: i32,
    /// Attackers ignore defense altogether (doom damage).
    pub ignores_defense: bool,
    /// How the attack is delivered and what it is made of: what immunities it meets.
    pub mode: Mode,
    pub nature: AttackNature,
}

/// What a defending unit contributes to the rolls, and the damage it has taken.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DefenderState {
    /// Hit points of one figure.
    pub hits: i32,
    pub defense: i32,
    /// What the defender is immune to, which changes the defence it rolls (`immunity.rs`).
    pub immunities: Immunities,
    /// Pluses to block (each is ten percentage points).
    pub to_block: i32,
    /// Figures still alive.
    pub figures: u32,
    /// Damage already taken by the front figure.
    pub front_damage: i32,
}

impl DefenderState {
    /// Hit points left over all figures.
    pub fn hit_points(&self) -> i32 {
        if self.figures == 0 {
            return 0;
        }
        self.figures as i32 * self.hits - self.front_damage
    }
}

/// A ten-sided die: one to ten.
fn d10(rng: &mut dyn RngCore) -> i32 {
    rng.gen_range(1..=10)
}

/// Dice that hit: each succeeds on `8 - pluses` or better, and a ten always succeeds.
pub fn roll_successes(dice: i32, pluses: i32, rng: &mut dyn RngCore) -> i32 {
    (0..dice.max(0))
        .filter(|_| {
            let die = d10(rng);
            die >= 8 - pluses || die == 10
        })
        .count() as i32
}

/// What one attack did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Damage {
    /// Hit points of damage dealt, after blocks.
    pub points: i32,
    /// Figures killed.
    pub kills: u32,
}

impl std::ops::AddAssign for Damage {
    fn add_assign(&mut self, other: Damage) {
        self.points += other.points;
        self.kills += other.kills;
    }
}

/// Resolve one attack by `figures` attacking figures against `def`, which is updated.
pub fn process_attack(
    attacker: &Combatant,
    figures: u32,
    def: &mut DefenderState,
    rng: &mut dyn RngCore,
) -> Damage {
    let mut total = Damage::default();
    if attacker.strength <= 0 || def.figures == 0 {
        return total;
    }
    let defense = effective_defense(
        def.defense,
        &def.immunities,
        &attacker.nature,
        attacker.mode,
    );
    for _ in 0..figures {
        if def.figures == 0 {
            break;
        }
        let mut dmg = if attacker.ignores_defense {
            attacker.strength
        } else {
            let hits = roll_successes(attacker.strength, attacker.to_hit, rng);
            (hits - roll_successes(defense, def.to_block, rng)).max(0)
        };
        while dmg > 0 && def.figures > 0 {
            let need = def.hits - def.front_damage;
            if dmg >= need {
                total.points += need;
                total.kills += 1;
                def.figures -= 1;
                def.front_damage = 0;
                dmg -= need;
                // The overflow meets the next figure's defense.
                if dmg > 0 && !attacker.ignores_defense {
                    dmg = (dmg - roll_successes(defense, def.to_block, rng)).max(0);
                }
            } else {
                def.front_damage += dmg;
                total.points += dmg;
                dmg = 0;
            }
        }
    }
    total
}

/// One side of a melee exchange.
#[derive(Debug, Clone)]
pub struct Fighter {
    /// Melee dice per figure.
    pub melee: Combatant,
    /// Thrown or breath attack used before melee, if any.
    pub thrown: Option<Combatant>,
    pub first_strike: bool,
    pub negate_first_strike: bool,
    pub state: DefenderState,
}

/// The damage each side took.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Exchange {
    pub dealt: Damage,
    pub taken: Damage,
}

/// A melee attack and the defender's reply, in the original's order. Both fighters' states
/// are updated.
pub fn melee_exchange(att: &mut Fighter, def: &mut Fighter, rng: &mut dyn RngCore) -> Exchange {
    let mut out = Exchange::default();
    // Thrown weapons and breath go first, with every figure the attacker has.
    if let Some(thrown) = &att.thrown {
        out.dealt += process_attack(thrown, att.state.figures, &mut def.state, rng);
    }
    let first_strike = att.first_strike && !def.negate_first_strike;
    if first_strike && def.state.figures > 0 {
        out.dealt += process_attack(&att.melee, att.state.figures, &mut def.state, rng);
    }
    // The defender strikes back with the figures it has left.
    if def.state.figures > 0 {
        out.taken += process_attack(&def.melee, def.state.figures, &mut att.state, rng);
    }
    // An attacker without first strike now swings with the figures that survived.
    if !first_strike && att.state.figures > 0 && def.state.figures > 0 {
        out.dealt += process_attack(&att.melee, att.state.figures, &mut def.state, rng);
    }
    out
}

/// A ranged attack: the target fires nothing back.
pub fn ranged_attack(
    attacker: &Combatant,
    figures: u32,
    def: &mut DefenderState,
    rng: &mut dyn RngCore,
) -> Damage {
    process_attack(attacker, figures, def, rng)
}

/// Penalty to hit with arrows and boulders at a distance: one per three squares, at most one
/// for long-range shooters.
pub fn range_penalty(distance: u32, long_range: bool) -> i32 {
    let p = (distance / 3) as i32;
    if long_range && p > 0 {
        1
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_pcg::Pcg64Mcg;

    fn rng(seed: u64) -> Pcg64Mcg {
        Pcg64Mcg::seed_from_u64(seed)
    }

    fn def(figures: u32, hits: i32, defense: i32) -> DefenderState {
        DefenderState {
            hits,
            defense,
            to_block: 0,
            figures,
            front_damage: 0,
            ..DefenderState::default()
        }
    }

    fn combatant(strength: i32, to_hit: i32) -> Combatant {
        Combatant {
            strength,
            to_hit,
            ..Combatant::default()
        }
    }

    #[test]
    fn a_die_hits_thirty_percent_and_each_plus_adds_ten() {
        let mut r = rng(1);
        let n = 20_000;
        for (pluses, expected) in [(0, 30), (1, 40), (3, 60), (-1, 20), (-5, 10), (9, 100)] {
            let hits = roll_successes(n, pluses, &mut r);
            let pct = f64::from(hits) / f64::from(n) * 100.0;
            assert!(
                (pct - f64::from(expected)).abs() < 1.5,
                "{pluses} pluses: {pct:.1}% vs {expected}%"
            );
        }
        assert_eq!(roll_successes(0, 0, &mut r), 0);
        assert_eq!(roll_successes(-3, 0, &mut r), 0);
    }

    #[test]
    fn defense_blocks_hits() {
        // Strong attackers against a strong defense average less than against none.
        let mut r = rng(2);
        let mut open = 0;
        let mut armoured = 0;
        for _ in 0..2_000 {
            let mut a = def(100, 100, 0);
            let mut b = def(100, 100, 6);
            open += process_attack(&combatant(5, 0), 4, &mut a, &mut r).points;
            armoured += process_attack(&combatant(5, 0), 4, &mut b, &mut r).points;
        }
        assert!(open > armoured * 2, "{open} vs {armoured}");
        // Four figures with five dice at 30 percent hit six times each on average.
        let per_attack = f64::from(open) / 2_000.0;
        assert!((per_attack - 6.0).abs() < 0.5, "{per_attack}");
    }

    #[test]
    fn damage_goes_to_the_front_figure_and_overflows_to_the_next() {
        let mut r = rng(3);
        // Unblockable, certain hits (strength 10 at +7 always rolls 1+ ... use doom damage).
        let doom = Combatant {
            strength: 5,
            ignores_defense: true,
            ..Combatant::default()
        };
        let mut d = def(3, 2, 9);
        let dmg = process_attack(&doom, 1, &mut d, &mut r);
        // Five damage on 2-hit figures: two figures dead, one point on the third.
        assert_eq!(
            dmg,
            Damage {
                points: 5,
                kills: 2
            }
        );
        assert_eq!((d.figures, d.front_damage), (1, 1));
        assert_eq!(d.hit_points(), 1);
        // The wounded front figure needs only one more.
        let dmg = process_attack(&doom, 1, &mut d, &mut r);
        assert_eq!(
            dmg,
            Damage {
                points: 1,
                kills: 1
            }
        );
        assert_eq!(d.figures, 0);
        assert_eq!(d.hit_points(), 0);
        // Nothing left to hit.
        assert_eq!(process_attack(&doom, 3, &mut d, &mut r), Damage::default());
    }

    #[test]
    fn damage_never_exceeds_what_the_defender_has() {
        let mut r = rng(4);
        for seed in 0..200 {
            let mut r2 = rng(seed);
            let mut d = def(3, 4, 2);
            let before = d.hit_points();
            let dmg = process_attack(&combatant(9, 3), 6, &mut d, &mut r2);
            assert_eq!(before - dmg.points, d.hit_points(), "seed {seed}");
            assert!(d.hit_points() >= 0);
            assert!(dmg.kills <= 3);
        }
        let _ = &mut r;
    }

    #[test]
    fn the_same_seed_gives_the_same_battle() {
        let fight = |seed| {
            let mut r = rng(seed);
            let mut a = Fighter {
                melee: combatant(4, 0),
                thrown: None,
                first_strike: false,
                negate_first_strike: false,
                state: def(4, 3, 3),
            };
            let mut b = a.clone();
            let ex = melee_exchange(&mut a, &mut b, &mut r);
            (ex, a.state, b.state)
        };
        assert_eq!(fight(9), fight(9));
        let outcomes: std::collections::HashSet<_> =
            (0..40).map(|s| fight(s).0.dealt.points).collect();
        assert!(outcomes.len() > 3, "varied across seeds");
    }

    fn doomed(strength: i32) -> Combatant {
        Combatant {
            strength,
            ignores_defense: true,
            ..Combatant::default()
        }
    }

    fn fighter(figures: u32, hits: i32, melee: Combatant) -> Fighter {
        Fighter {
            melee,
            thrown: None,
            first_strike: false,
            negate_first_strike: false,
            state: def(figures, hits, 0),
        }
    }

    #[test]
    fn the_defender_strikes_back_before_an_ordinary_attacker_swings() {
        let mut r = rng(5);
        // 3 attackers (1 hp each, melee 1 doom), 3 defenders (1 hp each, melee 1 doom).
        // The defender's counter kills all three attackers first, so nobody swings back.
        let mut att = fighter(3, 1, doomed(1));
        let mut def_ = fighter(3, 1, doomed(1));
        let ex = melee_exchange(&mut att, &mut def_, &mut r);
        assert_eq!(att.state.figures, 0);
        assert_eq!(ex.taken.kills, 3);
        assert_eq!(
            ex.dealt.kills, 0,
            "the attacker lost its figures before swinging"
        );
        assert_eq!(def_.state.figures, 3);
    }

    #[test]
    fn first_strike_lets_the_attacker_hit_before_the_reply() {
        let mut r = rng(6);
        let mut att = fighter(3, 1, doomed(1));
        att.first_strike = true;
        let mut def_ = fighter(3, 1, doomed(1));
        let ex = melee_exchange(&mut att, &mut def_, &mut r);
        assert_eq!(
            def_.state.figures, 0,
            "the defenders fell before they could strike"
        );
        assert_eq!(ex.dealt.kills, 3);
        assert_eq!(ex.taken.kills, 0);
        assert_eq!(att.state.figures, 3);
        // Negate first strike takes the advantage away.
        let mut att = fighter(3, 1, doomed(1));
        att.first_strike = true;
        let mut def_ = fighter(3, 1, doomed(1));
        def_.negate_first_strike = true;
        melee_exchange(&mut att, &mut def_, &mut r);
        assert_eq!(
            att.state.figures, 0,
            "ordinary order again: the defender hit first"
        );
    }

    #[test]
    fn thrown_attacks_open_the_exchange() {
        let mut r = rng(7);
        let mut att = fighter(2, 1, doomed(0));
        att.thrown = Some(doomed(2));
        let mut def_ = fighter(4, 1, doomed(1));
        let ex = melee_exchange(&mut att, &mut def_, &mut r);
        // Two figures throw two points each: all four defenders fall before they can answer.
        assert_eq!(ex.dealt.kills, 4);
        assert_eq!(att.state.figures, 2);
        assert_eq!(ex.taken, Damage::default());
    }

    #[test]
    fn shooting_draws_no_reply_and_range_costs_accuracy() {
        let mut r = rng(8);
        let mut d = def(4, 2, 0);
        let shot = ranged_attack(&doomed(3), 2, &mut d, &mut r);
        assert_eq!(shot.points, 6);
        assert_eq!(range_penalty(0, false), 0);
        assert_eq!(range_penalty(2, false), 0);
        assert_eq!(range_penalty(3, false), 1);
        assert_eq!(range_penalty(9, false), 3);
        assert_eq!(range_penalty(9, true), 1, "long range caps the penalty");
        assert_eq!(range_penalty(2, true), 0);
    }

    #[test]
    fn a_defender_rolls_the_defense_its_immunities_give_it() {
        use crate::battle::immunity::{AttackNature, Immunities};
        let golem = Immunities {
            weapon: true,
            ..Immunities::default()
        };
        let armoured = |immunities| DefenderState {
            immunities,
            ..def(100, 100, 0)
        };
        // Normal swords: strength 6, no defense of its own, but weapon immunity gives it ten.
        let sword = Combatant {
            strength: 6,
            mode: Mode::Melee,
            nature: AttackNature::of_unit_attack(Mode::Melee, &[], false),
            ..Combatant::default()
        };
        let magic_sword = Combatant {
            nature: AttackNature::of_unit_attack(Mode::Melee, &[], true),
            ..sword.clone()
        };
        let (mut plain, mut stone, mut stone_vs_magic) = (0, 0, 0);
        let mut r = rng(11);
        for _ in 0..1_000 {
            plain += process_attack(&sword, 4, &mut armoured(Immunities::default()), &mut r).points;
            stone += process_attack(&sword, 4, &mut armoured(golem), &mut r).points;
            stone_vs_magic += process_attack(&magic_sword, 4, &mut armoured(golem), &mut r).points;
        }
        assert!(stone * 4 < plain, "{stone} vs {plain}");
        assert!((f64::from(stone_vs_magic) / f64::from(plain) - 1.0).abs() < 0.1);
        // Fifty dice of defense stop all but a die in a thousand: arrows against missile
        // immunity do no damage at all over a thousand volleys.
        let arrows = Combatant {
            strength: 6,
            mode: Mode::Ranged,
            nature: AttackNature::of_unit_attack(Mode::Ranged, &["bow".to_string()], false),
            ..Combatant::default()
        };
        let archer_proof = Immunities {
            missiles: true,
            ..Immunities::default()
        };
        let mut through = 0;
        for _ in 0..1_000 {
            through += process_attack(&arrows, 4, &mut armoured(archer_proof), &mut r).points;
        }
        assert_eq!(through, 0);
        // Doom damage ignores even that.
        let doom = Combatant {
            ignores_defense: true,
            ..arrows
        };
        assert_eq!(
            process_attack(&doom, 1, &mut armoured(archer_proof), &mut r).points,
            6
        );
    }
}
