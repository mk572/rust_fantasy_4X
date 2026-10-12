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

use super::immunity::{
    effective_defense, effective_resistance, resistance_failure, AttackNature, Immunities, Realm,
};
use super::special::{Gaze, SpecialAttacks};

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
    /// Poison, touches, life steal and gazes (`special.rs`).
    pub special: SpecialAttacks,
    /// The attacker sees through illusions, so Blur does not trouble it.
    pub sees_through: bool,
}

/// What a large shield adds to the defence against ranged, thrown and breath attacks.
pub const LARGE_SHIELD: i32 = 2;

/// What a defending unit contributes to the rolls, and the damage it has taken.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DefenderState {
    /// Hit points of one figure.
    pub hits: i32,
    pub defense: i32,
    /// What the defender is immune to, which changes the defence it rolls (`immunity.rs`).
    pub immunities: Immunities,
    /// Its resistance and the lasting spells that add to it, for the resistance checks of
    /// special attacks.
    pub resistance: i32,
    pub enchantments: Vec<String>,
    /// The realm of the creature, for dispel evil (chaos and death creatures); the undead.
    pub realm: Option<Realm>,
    pub undead: bool,
    /// Figures it started with: healing can bring dead figures back up to this.
    pub figures_max: u32,
    /// Pluses to block (each is ten percentage points).
    pub to_block: i32,
    /// Figures still alive.
    pub figures: u32,
    /// Damage already taken by the front figure.
    pub front_damage: i32,
    /// Blur is on it: each point of damage may miss.
    pub blur: bool,
    /// It carries a large shield: +2 defence against every attack but a melee blow.
    pub large_shield: bool,
}

impl DefenderState {
    /// The defence it rolls before immunities against an attack of this `mode`.
    fn defense_against(&self, mode: Mode) -> i32 {
        self.defense
            + if self.large_shield && mode != Mode::Melee {
                LARGE_SHIELD
            } else {
                0
            }
    }

    /// Hit points left over all figures.
    pub fn hit_points(&self) -> i32 {
        if self.figures == 0 {
            return 0;
        }
        self.figures as i32 * self.hits - self.front_damage
    }

    /// How far a resistance check against `realm` fails, with `modifier` added to the
    /// resistance: 0 means the unit resisted.
    pub fn resistance_failure(
        &self,
        realm: Option<Realm>,
        modifier: i32,
        rng: &mut dyn RngCore,
    ) -> i32 {
        let ids: Vec<&str> = self.enchantments.iter().map(String::as_str).collect();
        let resistance = effective_resistance(self.resistance, &self.immunities, realm, &ids);
        resistance_failure(resistance, modifier, rng)
    }

    /// Heal `amount` hit points: the damaged figure first, then bring dead figures back.
    /// Nothing happens to a unit with no figures left.
    pub fn heal(&mut self, amount: i32) {
        if self.figures == 0 || amount <= 0 {
            return;
        }
        let front = self.front_damage - amount;
        if front >= 0 {
            self.front_damage = front;
            return;
        }
        self.front_damage = 0;
        let mut balance = front;
        while balance < 0 && self.figures < self.figures_max {
            self.figures += 1;
            balance += self.hits;
        }
        if balance > 0 {
            self.front_damage = balance;
        }
    }
}

/// Damage that no defence blocks, `points` of it, landing on the front figure and passing on
/// to the next as figures die.
pub fn unblockable(def: &mut DefenderState, points: i32) -> Damage {
    let mut total = Damage::default();
    let mut left = points;
    while left > 0 && def.figures > 0 {
        let need = def.hits - def.front_damage;
        if left >= need {
            total.points += need;
            total.kills += 1;
            def.figures -= 1;
            def.front_damage = 0;
            left -= need;
        } else {
            def.front_damage += left;
            total.points += left;
            left = 0;
        }
    }
    total
}

/// A whole figure's worth of damage no defence blocks (a failed gaze or touch).
pub fn kill_figure(def: &mut DefenderState) -> Damage {
    unblockable(def, def.hits)
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

/// Blur: each point of damage has a one in ten chance to miss (the original's loop counts up
/// as the damage comes down, so it can take off at most about half). Not against an attacker
/// that sees through illusions.
fn blurred(hits: i32, attacker: &Combatant, def: &DefenderState, rng: &mut dyn RngCore) -> i32 {
    if !def.blur || attacker.sees_through {
        return hits;
    }
    let mut left = hits;
    let mut rolls = 0;
    while rolls < left {
        if d10(rng) == 10 {
            left -= 1;
        }
        rolls += 1;
    }
    left
}

/// What one attack did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Damage {
    /// Hit points of damage dealt, after blocks.
    pub points: i32,
    /// Figures killed.
    pub kills: u32,
    /// Hit points the attacker heals from what it stole (life steal).
    pub healed: i32,
    /// Of `points`, the damage of a life steal or a create-undead blow: a unit that dies of
    /// it can be raised as undead (`Fate`).
    pub drained: i32,
    /// Of `points`, the damage of a destruction or of a spell that severs or burns: a unit
    /// that dies of it is gone for good.
    pub gone: i32,
}

impl Damage {
    /// All of this damage counted as `gone`.
    pub fn all_gone(self) -> Damage {
        Damage {
            gone: self.points,
            drained: 0,
            ..self
        }
    }

    /// All of this damage counted as `drained`.
    pub fn all_drained(self) -> Damage {
        Damage {
            drained: self.points,
            gone: 0,
            ..self
        }
    }
}

impl std::ops::AddAssign for Damage {
    fn add_assign(&mut self, other: Damage) {
        self.points += other.points;
        self.kills += other.kills;
        self.healed += other.healed;
        self.drained += other.drained;
        self.gone += other.gone;
    }
}

/// Resolve one attack by `figures` attacking figures against `def`, which is updated.
///
/// Order, as in the original: a gaze, once, against each of the target's figures; then for
/// each attacking figure the touches (dispel evil, stoning, death, life steal, destruction),
/// the blows themselves, and the poison.
pub fn process_attack(
    attacker: &Combatant,
    figures: u32,
    def: &mut DefenderState,
    rng: &mut dyn RngCore,
) -> Damage {
    let mut total = Damage::default();
    let sp = &attacker.special;
    let touchable = !def.immunities.magic;
    if attacker.mode == Mode::ThrownOrBreath && sp.gaze != Gaze::None && touchable {
        let gazed = def.figures;
        if matches!(sp.gaze, Gaze::Stoning | Gaze::Multi) && !def.immunities.stoning {
            for _ in 0..gazed {
                if def.resistance_failure(Some(Realm::Nature), -sp.penalty, rng) > 0 {
                    total += kill_figure(def);
                }
            }
        }
        if matches!(sp.gaze, Gaze::Death | Gaze::Multi) && !def.immunities.death {
            for _ in 0..gazed {
                if def.resistance_failure(Some(Realm::Death), -sp.penalty, rng) > 0 {
                    total += kill_figure(def);
                }
            }
        }
    }
    if attacker.strength <= 0 || def.figures == 0 {
        return total;
    }
    let defense = effective_defense(
        def.defense_against(attacker.mode),
        &def.immunities,
        &attacker.nature,
        attacker.mode,
    );
    for _ in 0..figures {
        if def.figures == 0 {
            break;
        }
        if touchable {
            if sp.dispel_evil && matches!(def.realm, Some(Realm::Chaos | Realm::Death)) {
                let modifier = if def.undead { -9 } else { -4 };
                if def.resistance_failure(Some(Realm::Life), modifier, rng) > 0 {
                    total += kill_figure(def);
                }
            }
            if sp.stoning_touch
                && !def.immunities.stoning
                && def.resistance_failure(Some(Realm::Nature), -sp.penalty, rng) > 0
            {
                total += kill_figure(def);
            }
            if sp.death_touch
                && !def.immunities.death
                && def.resistance_failure(Some(Realm::Death), -sp.penalty, rng) > 0
            {
                total += kill_figure(def);
            }
            if sp.life_steal && !def.immunities.death {
                let stolen = def.resistance_failure(Some(Realm::Death), -sp.penalty, rng);
                total += unblockable(def, stolen).all_drained();
                total.healed += stolen;
            }
            if sp.destruction && def.resistance_failure(Some(Realm::Chaos), 0, rng) > 0 {
                total += kill_figure(def).all_gone();
            }
        }
        if def.figures == 0 {
            break;
        }
        let mut dmg = if attacker.ignores_defense {
            attacker.strength
        } else {
            let hits = blurred(
                roll_successes(attacker.strength, attacker.to_hit, rng),
                attacker,
                def,
                rng,
            );
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
        if sp.poison > 0 && !def.immunities.poison {
            for _ in 0..sp.poison {
                if def.figures > 0 && def.resistance_failure(None, 0, rng) > 0 {
                    total += unblockable(def, 1);
                }
            }
        }
    }
    // What a creator of undead deals, apart from what the touches destroy, can raise the dead.
    if sp.create_undead && !def.immunities.magic {
        total.drained = total.points - total.gone;
    }
    total
}

/// How a damage spell lands on its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Volley {
    /// One attack of `strength` dice, as from a single figure.
    Bolt,
    /// Every figure of the target is hit on its own (Fireball); none loses more than one figure
    /// and the damage short of a kill pools over the unit.
    Area,
    /// A chain of attacks, each with one die less than the last (Warp Lightning); the damage
    /// from all of them pools over the unit.
    Chain,
}

/// A damage spell against `def`: unlike a blow it ignores the target's resistance and
/// brings no special attacks. `attacker.strength` is the spell's strength.
pub fn spell_attack(
    attacker: &Combatant,
    volley: Volley,
    def: &mut DefenderState,
    rng: &mut dyn RngCore,
) -> Damage {
    if attacker.strength <= 0 || def.figures == 0 {
        return Damage::default();
    }
    let defense = effective_defense(
        def.defense_against(attacker.mode),
        &def.immunities,
        &attacker.nature,
        attacker.mode,
    );
    let blow = |strength: i32, rng: &mut dyn RngCore| {
        if attacker.ignores_defense {
            strength
        } else {
            let hits = roll_successes(strength, attacker.to_hit, rng);
            (hits - roll_successes(defense, def.to_block, rng)).max(0)
        }
    };
    match volley {
        Volley::Bolt => process_attack(
            &Combatant {
                special: SpecialAttacks::default(),
                ..attacker.clone()
            },
            1,
            def,
            rng,
        ),
        Volley::Area => {
            let hits = def.hits;
            let pooled: i32 = (0..def.figures)
                .map(|_| {
                    let d = blow(attacker.strength, rng);
                    if d > hits {
                        hits
                    } else {
                        d
                    }
                })
                .sum();
            unblockable(def, pooled)
        }
        Volley::Chain => {
            let pooled: i32 = (1..=attacker.strength)
                .rev()
                .map(|strength| blow(strength, rng))
                .sum();
            unblockable(def, pooled)
        }
    }
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
/// are updated. Life stolen heals the thief at once.
pub fn melee_exchange(att: &mut Fighter, def: &mut Fighter, rng: &mut dyn RngCore) -> Exchange {
    let mut out = Exchange::default();
    // Thrown weapons and breath go first, with every figure the attacker has.
    if let Some(thrown) = &att.thrown {
        let d = process_attack(thrown, att.state.figures, &mut def.state, rng);
        att.state.heal(d.healed);
        out.dealt += d;
    }
    // A defender with a gaze gazes back before the blows, with the figures it has left.
    if let Some(gaze) = def.thrown.as_ref().filter(|t| t.special.gaze != Gaze::None) {
        if def.state.figures > 0 && att.state.figures > 0 {
            let d = process_attack(gaze, def.state.figures, &mut att.state, rng);
            def.state.heal(d.healed);
            out.taken += d;
        }
    }
    let first_strike = att.first_strike && !def.negate_first_strike;
    if first_strike && def.state.figures > 0 {
        let d = process_attack(&att.melee, att.state.figures, &mut def.state, rng);
        att.state.heal(d.healed);
        out.dealt += d;
    }
    // The defender strikes back with the figures it has left.
    if def.state.figures > 0 && att.state.figures > 0 {
        let d = process_attack(&def.melee, def.state.figures, &mut att.state, rng);
        def.state.heal(d.healed);
        out.taken += d;
    }
    // An attacker without first strike now swings with the figures that survived.
    if !first_strike && att.state.figures > 0 && def.state.figures > 0 {
        let d = process_attack(&att.melee, att.state.figures, &mut def.state, rng);
        att.state.heal(d.healed);
        out.dealt += d;
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
                kills: 2,
                healed: 0,
                ..Damage::default()
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
                kills: 1,
                healed: 0,
                ..Damage::default()
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

    /// A defender that nothing blocks (defense 0) with the given resistance.
    fn soft(figures: u32, hits: i32, resistance: i32) -> DefenderState {
        DefenderState {
            resistance,
            figures_max: figures,
            ..def(figures, hits, 0)
        }
    }

    /// A weak attacker (one die) with the given special attacks.
    fn special(special: SpecialAttacks, mode: Mode) -> Combatant {
        Combatant {
            strength: 1,
            mode,
            special,
            ..Combatant::default()
        }
    }

    /// Average kills over many attacks.
    fn mean_kills(
        attacker: &Combatant,
        figures: u32,
        mut target: impl FnMut() -> DefenderState,
    ) -> f64 {
        let mut r = rng(21);
        let n = 3_000;
        let total: u32 = (0..n)
            .map(|_| process_attack(attacker, figures, &mut target(), &mut r).kills)
            .sum();
        f64::from(total) / f64::from(n)
    }

    #[test]
    fn a_touch_that_is_not_resisted_kills_a_figure_for_each_attacker() {
        let touch = |s: SpecialAttacks| special(s, Mode::Melee);
        let stoner = SpecialAttacks {
            stoning_touch: true,
            penalty: 3,
            ..SpecialAttacks::default()
        };
        // Resistance 4 at -3 fails on 2 to 10: nine times in ten; six attackers.
        let k = mean_kills(&touch(stoner), 6, || soft(40, 100, 4));
        assert!((k - 5.4).abs() < 0.3, "{k}");
        // Stoning immunity and magic immunity both stop it.
        let stone_proof = || DefenderState {
            immunities: Immunities {
                stoning: true,
                ..Immunities::default()
            },
            ..soft(40, 100, 4)
        };
        let warded = || DefenderState {
            immunities: Immunities {
                magic: true,
                ..Immunities::default()
            },
            ..soft(40, 100, 4)
        };
        assert!(mean_kills(&touch(stoner), 6, stone_proof) < 1.0);
        assert!(mean_kills(&touch(stoner), 6, warded) < 1.0);
        // A higher resistance resists more often.
        assert!(mean_kills(&touch(stoner), 6, || soft(40, 100, 12)) < k / 3.0);
        let death = SpecialAttacks {
            death_touch: true,
            penalty: 3,
            ..SpecialAttacks::default()
        };
        let d = mean_kills(&touch(death), 6, || soft(40, 100, 4));
        assert!((d - 5.4).abs() < 0.3, "{d}");
        let destroyer = SpecialAttacks {
            destruction: true,
            ..SpecialAttacks::default()
        };
        // Destruction has no penalty: resistance 4 fails six times in ten.
        let x = mean_kills(&touch(destroyer), 6, || soft(40, 100, 4));
        assert!((x - 3.6).abs() < 0.3, "{x}");
    }

    #[test]
    fn dispel_evil_works_on_creatures_of_chaos_and_death_only() {
        let angel = special(
            SpecialAttacks {
                dispel_evil: true,
                ..SpecialAttacks::default()
            },
            Mode::Melee,
        );
        let creature = |realm, undead| {
            move || DefenderState {
                realm,
                undead,
                ..soft(40, 100, 8)
            }
        };
        // -4 on resistance 8 fails six times in ten, -9 always (the die cannot roll under 0).
        let evil = mean_kills(&angel, 6, creature(Some(Realm::Death), false));
        assert!((evil - 3.6).abs() < 0.3, "{evil}");
        let dead = mean_kills(&angel, 6, creature(Some(Realm::Chaos), true));
        assert!((dead - 6.0).abs() < 0.2, "{dead}");
        assert!(mean_kills(&angel, 6, creature(Some(Realm::Life), false)) < 1.0);
        assert!(mean_kills(&angel, 6, creature(None, false)) < 1.0);
    }

    #[test]
    fn poison_makes_each_point_of_strength_resisted_or_hurt() {
        let spider = special(
            SpecialAttacks {
                poison: 6,
                ..SpecialAttacks::default()
            },
            Mode::Melee,
        );
        let armoured = |immunities| DefenderState {
            immunities,
            defense: 50,
            ..soft(200, 100, 4)
        };
        let mut r = rng(5);
        let n = 2_000;
        let hurt: i32 = (0..n)
            .map(|_| {
                process_attack(&spider, 1, &mut armoured(Immunities::default()), &mut r).points
            })
            .sum();
        // Six checks at resistance 4: six in ten fail, so 3.6 points a time.
        let mean = f64::from(hurt) / f64::from(n);
        assert!((mean - 3.6).abs() < 0.2, "{mean}");
        let proof = Immunities {
            poison: true,
            ..Immunities::default()
        };
        let none: i32 = (0..200)
            .map(|_| process_attack(&spider, 1, &mut armoured(proof), &mut r).points)
            .sum();
        assert_eq!(none, 0);
        // Magic immunity does not stop poison.
        let warded = Immunities {
            magic: true,
            ..Immunities::default()
        };
        let still: i32 = (0..200)
            .map(|_| process_attack(&spider, 1, &mut armoured(warded), &mut r).points)
            .sum();
        assert!(still > 400, "{still}");
    }

    #[test]
    fn life_steal_hurts_past_armour_and_heals_the_thief() {
        let wraith = special(
            SpecialAttacks {
                life_steal: true,
                penalty: 3,
                ..SpecialAttacks::default()
            },
            Mode::Melee,
        );
        let target = || DefenderState {
            defense: 50,
            ..soft(200, 100, 4)
        };
        let mut r = rng(8);
        let n = 3_000;
        let (mut dealt, mut healed) = (0, 0);
        for _ in 0..n {
            let d = process_attack(&wraith, 1, &mut target(), &mut r);
            dealt += d.points;
            healed += d.healed;
        }
        // Resistance 1: a die of 2 to 10 fails by 1 to 9, 5 on average: 4.5 a time.
        let mean = f64::from(healed) / f64::from(n);
        assert!((mean - 4.5).abs() < 0.3, "{mean}");
        assert_eq!(dealt, healed, "what it steals it takes from the target");
        // Death immunity protects against it.
        let proof = DefenderState {
            immunities: Immunities {
                death: true,
                ..Immunities::default()
            },
            ..target()
        };
        let d = process_attack(&wraith, 1, &mut proof.clone(), &mut r);
        assert_eq!((d.points, d.healed), (0, 0));
    }

    #[test]
    fn damage_is_sorted_by_what_dealt_it_so_the_dead_know_how_they_fell() {
        let target = || DefenderState {
            defense: 0,
            ..soft(200, 100, 4)
        };
        let mut r = rng(11);
        // Life steal and the blows of a creator of undead drain; destruction destroys.
        let wraith = special(
            SpecialAttacks {
                life_steal: true,
                penalty: 3,
                ..SpecialAttacks::default()
            },
            Mode::Melee,
        );
        let d = process_attack(&wraith, 3, &mut target(), &mut r);
        assert!(d.points > 0 && d.drained > 0 && d.drained <= d.points);
        let ghoul = Combatant {
            strength: 4,
            to_hit: 3,
            ..special(
                SpecialAttacks {
                    create_undead: true,
                    ..SpecialAttacks::default()
                },
                Mode::Melee,
            )
        };
        let d = process_attack(&ghoul, 4, &mut target(), &mut r);
        assert!(d.points > 0, "{d:?}");
        assert_eq!((d.drained, d.gone), (d.points, 0));
        // Magic immunity keeps the victim from being raised.
        let warded = DefenderState {
            immunities: Immunities {
                magic: true,
                ..Immunities::default()
            },
            ..target()
        };
        let d = process_attack(&ghoul, 4, &mut warded.clone(), &mut r);
        assert_eq!(d.drained, 0);
        let destroyer = special(
            SpecialAttacks {
                destruction: true,
                ..SpecialAttacks::default()
            },
            Mode::Melee,
        );
        let d = process_attack(
            &destroyer,
            4,
            &mut DefenderState {
                resistance: 0,
                ..target()
            },
            &mut r,
        );
        assert!(d.gone > 0 && d.drained == 0, "{d:?}");
    }

    #[test]
    fn a_melee_exchange_heals_a_life_stealer() {
        let thief = Fighter {
            melee: special(
                SpecialAttacks {
                    life_steal: true,
                    penalty: 3,
                    ..SpecialAttacks::default()
                },
                Mode::Melee,
            ),
            thrown: None,
            first_strike: false,
            negate_first_strike: false,
            state: DefenderState {
                front_damage: 3,
                ..soft(1, 8, 4)
            },
        };
        let victim = Fighter {
            melee: Combatant::default(),
            thrown: None,
            first_strike: false,
            negate_first_strike: false,
            state: soft(50, 100, 4),
        };
        let mut healed_any = false;
        for seed in 0..40 {
            let (mut a, mut b) = (thief.clone(), victim.clone());
            melee_exchange(&mut a, &mut b, &mut rng(seed));
            assert!(a.state.front_damage <= 3);
            healed_any |= a.state.front_damage < 3;
        }
        assert!(healed_any);
    }

    #[test]
    fn a_gaze_makes_each_figure_resist_in_the_thrown_step_only() {
        let gazer = |gaze, mode| {
            special(
                SpecialAttacks {
                    gaze,
                    penalty: 2,
                    ..SpecialAttacks::default()
                },
                mode,
            )
        };
        // Resistance 4 at -2: fails on 3 to 10, eight in ten of ten figures.
        let k = mean_kills(&gazer(Gaze::Stoning, Mode::ThrownOrBreath), 1, || {
            soft(10, 100, 4)
        });
        assert!((k - 8.0).abs() < 0.4, "{k}");
        // The same unit's melee blows do not gaze.
        assert!(mean_kills(&gazer(Gaze::Stoning, Mode::Melee), 1, || soft(10, 100, 4)) < 1.0);
        // A multiple gaze stones and kills: about 1 - 0.2 * 0.2 of the figures.
        let multi = mean_kills(&gazer(Gaze::Multi, Mode::ThrownOrBreath), 1, || {
            soft(10, 100, 4)
        });
        assert!((multi - 9.6).abs() < 0.4, "{multi}");
        let death = mean_kills(&gazer(Gaze::Death, Mode::ThrownOrBreath), 1, || {
            DefenderState {
                immunities: Immunities {
                    death: true,
                    ..Immunities::default()
                },
                ..soft(10, 100, 4)
            }
        });
        assert!(death < 1.0, "{death}");
    }

    #[test]
    fn healing_mends_the_front_figure_then_brings_the_dead_back() {
        let mut d = DefenderState {
            front_damage: 3,
            figures_max: 4,
            ..def(2, 5, 0)
        };
        d.heal(2);
        assert_eq!((d.figures, d.front_damage), (2, 1));
        // Past the front figure's damage, a dead figure returns, the rest wounding it.
        d.heal(3);
        assert_eq!((d.figures, d.front_damage), (3, 3));
        d.heal(100);
        assert_eq!((d.figures, d.front_damage), (4, 0));
        // The dead cannot be healed, and nothing is healed for nothing.
        let mut gone = DefenderState {
            figures_max: 4,
            ..def(0, 5, 0)
        };
        gone.heal(10);
        assert_eq!(gone.figures, 0);
        d.heal(-4);
        assert_eq!((d.figures, d.front_damage), (4, 0));
    }
}
