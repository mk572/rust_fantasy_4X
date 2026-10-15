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
    defense_bonus, effective_defense, effective_resistance, resistance_failure, righteous_ward,
    AttackNature, Immunities, Realm, IMMUNE_DEFENSE,
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
    /// Eldritch Weapon: the defender blocks with one plus fewer.
    pub eldritch: bool,
    /// Immolation: every melee blow also burns the defender (strength 4, fire).
    pub immolation: bool,
    /// Cause fear (a Cloak of Fear too): figures of the defender that fail a death resistance
    /// check cannot strike back.
    pub fear: bool,
}

/// The strength of the fire attack Immolation adds to a melee blow.
const IMMOLATION_STRENGTH: i32 = 4;

/// What a large shield adds to the defence against ranged, thrown and breath attacks.
pub const LARGE_SHIELD: i32 = 2;
/// What Invulnerability takes off every hit.
const INVULNERABILITY_BLOCK: i32 = 2;

/// What a defending unit contributes to the rolls, and the damage it has taken.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DefenderState {
    /// Hit points of one figure, with the extra ones life steal has added in this battle.
    pub hits: i32,
    /// Of `hits`, what overhealing added during the battle (it goes when the battle ends).
    pub extra_hits: i32,
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

    /// Under Black Sleep: every blow against it does its full strength.
    fn asleep(&self) -> bool {
        self.enchantments.iter().any(|e| e == "black_sleep")
    }

    /// The defence it rolls against one of `attacker`'s attacks: its own, the bonuses its
    /// lasting spells give against the attack's realm, then what its immunities make of it.
    /// Righteousness turns a ranged chaos or death attack away outright.
    fn defense_versus(&self, attacker: &Combatant) -> i32 {
        let ids: Vec<&str> = self.enchantments.iter().map(String::as_str).collect();
        let (realm, mode) = (attacker.nature.realm, attacker.mode);
        if righteous_ward(realm, mode, &ids) {
            return IMMUNE_DEFENSE;
        }
        // Berserk: no defence at all, whatever else the unit has (immunities still count).
        let defense = if ids.contains(&"berserk") {
            0
        } else {
            self.defense_against(mode) + defense_bonus(realm, mode, &ids)
        };
        effective_defense(defense, &self.immunities, &attacker.nature, mode)
    }

    /// The blows one roll of `defense` stops: Invulnerability stops two more.
    fn blocks(&self, defense: i32, eldritch: bool, rng: &mut dyn RngCore) -> i32 {
        let invulnerable = self.enchantments.iter().any(|e| e == "invulnerability");
        roll_successes(defense, self.to_block - i32::from(eldritch), rng)
            + if invulnerable {
                INVULNERABILITY_BLOCK
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
        self.heal_by(amount, false);
    }

    /// Heal as life steal does: what is left once every figure is back and whole is spread
    /// over the figures as extra hit points, one more for each figure's worth.
    pub fn heal_overflow(&mut self, amount: i32) {
        self.heal_by(amount, true);
    }

    fn heal_by(&mut self, amount: i32, overflow: bool) {
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
        } else if overflow && self.figures_max > 0 {
            let extra = (-balance) / self.figures_max as i32;
            self.hits += extra;
            self.extra_hits += extra;
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
    if attacker.immolation && attacker.mode == Mode::Melee && touchable && def.figures > 0 {
        let fire = Combatant {
            strength: IMMOLATION_STRENGTH,
            mode: Mode::Ranged,
            nature: AttackNature {
                stopped_by: Immunities {
                    fire: true,
                    magic: true,
                    ..Immunities::default()
                },
                realm: Some(Realm::Chaos),
                armor_piercing: false,
            },
            ..Combatant::default()
        };
        total += spell_attack(&fire, Volley::Area, def, rng);
    }
    if attacker.mode == Mode::ThrownOrBreath && sp.gaze != Gaze::None && touchable {
        let gazed = def.figures;
        if matches!(sp.gaze, Gaze::Stoning | Gaze::Multi) && !def.immunities.stoning {
            for _ in 0..gazed {
                if def.resistance_failure(Some(Realm::Nature), -sp.penalty, rng) > 0 {
                    total += kill_figure(def).all_gone();
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
    let defense = def.defense_versus(attacker);
    for _ in 0..figures {
        if def.figures == 0 {
            break;
        }
        if touchable {
            if sp.dispel_evil && matches!(def.realm, Some(Realm::Chaos | Realm::Death)) {
                let modifier = if def.undead { -9 } else { -4 };
                if def.resistance_failure(Some(Realm::Life), modifier, rng) > 0 {
                    total += kill_figure(def).all_gone();
                }
            }
            if sp.stoning_touch
                && !def.immunities.stoning
                && def.resistance_failure(Some(Realm::Nature), -sp.penalty, rng) > 0
            {
                total += kill_figure(def).all_gone();
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
        let mut dmg = if attacker.ignores_defense || def.asleep() {
            attacker.strength
        } else {
            let hits = blurred(
                roll_successes(attacker.strength, attacker.to_hit, rng),
                attacker,
                def,
                rng,
            );
            (hits - def.blocks(defense, attacker.eldritch, rng)).max(0)
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
                if dmg > 0 && !attacker.ignores_defense && !def.asleep() {
                    dmg = (dmg - def.blocks(defense, attacker.eldritch, rng)).max(0);
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
    let defense = def.defense_versus(attacker);
    let blow = |strength: i32, rng: &mut dyn RngCore| {
        // A sleeper takes the full strength of every blow.
        if attacker.ignores_defense || def.asleep() {
            strength
        } else {
            let hits = roll_successes(strength, attacker.to_hit, rng);
            (hits - def.blocks(defense, attacker.eldritch, rng)).max(0)
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
        att.state.heal_overflow(d.healed);
        out.dealt += d;
    }
    // A defender with a gaze gazes back before the blows, with the figures it has left.
    if let Some(gaze) = def.thrown.as_ref().filter(|t| t.special.gaze != Gaze::None) {
        if def.state.figures > 0 && att.state.figures > 0 {
            let d = process_attack(gaze, def.state.figures, &mut att.state, rng);
            def.state.heal_overflow(d.healed);
            out.taken += d;
        }
    }
    let first_strike = att.first_strike && !def.negate_first_strike;
    if first_strike && def.state.figures > 0 {
        let figures = att
            .state
            .figures
            .saturating_sub(feared_figures(&def.melee, &att.state, rng));
        let d = process_attack(&att.melee, figures, &mut def.state, rng);
        att.state.heal_overflow(d.healed);
        out.dealt += d;
    }
    // The defender strikes back with the figures it has left, less those the attacker frightens.
    if def.state.figures > 0 && att.state.figures > 0 {
        let figures = def
            .state
            .figures
            .saturating_sub(feared_figures(&att.melee, &def.state, rng));
        let d = process_attack(&def.melee, figures, &mut att.state, rng);
        def.state.heal_overflow(d.healed);
        out.taken += d;
    }
    // An attacker without first strike now swings with the figures that survived.
    if !first_strike && att.state.figures > 0 && def.state.figures > 0 {
        let figures = att
            .state
            .figures
            .saturating_sub(feared_figures(&def.melee, &att.state, rng));
        let d = process_attack(&att.melee, figures, &mut def.state, rng);
        att.state.heal_overflow(d.healed);
        out.dealt += d;
    }
    out
}

/// How many of `def`'s figures `attacker` frightens: each rolls a death resistance check, and
/// the ones that fail cannot fight this round (attack or counter-attack, whoever began). Nothing for an attacker that causes no fear or a
/// defender immune to death.
fn feared_figures(attacker: &Combatant, def: &DefenderState, rng: &mut dyn RngCore) -> u32 {
    if !attacker.fear || def.immunities.death {
        return 0;
    }
    (0..def.figures)
        .filter(|_| def.resistance_failure(Some(Realm::Death), 0, rng) > 0)
        .count() as u32
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
mod tests;
