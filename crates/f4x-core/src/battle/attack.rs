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
//!
//! Tests: in battle/attack/tests.rs.

use crate::rules::damage_type::{DamageType, DamageTypes};
use crate::rules::BattleRules;
use crate::rules::SpellId;
use crate::spell::unit_effect::UnitEffect;
use rand::{Rng, RngCore};

use super::immunity::{
    delivery, effective_defense, effective_resistance, resistance_failure, AttackNature,
    Immunities, Realm,
};
use super::special::SpecialAttacks;

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
    /// How the attack is delivered and what it is made of: what immunities it meets.
    pub mode: Mode,
    pub nature: AttackNature,
    /// Of `strength`, the dice made of damage types (a Flame Blade's, a Flaming weapon's).
    pub parts: Vec<(i32, DamageTypes)>,
    /// Poison, touches, life steal and gazes (`special.rs`).
    pub special: SpecialAttacks,
    /// Eldritch Weapon: the defender blocks with one plus fewer.
    pub eldritch: bool,
    /// Immolation: every melee blow also burns the defender (strength 4, fire).
    pub immolation: bool,
    /// Cause fear (a Cloak of Fear too): figures of the defender that fail a death resistance
    /// check cannot strike back.
    pub fear: bool,
}

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
    pub enchantments: Vec<SpellId>,
    pub effects: crate::spell::unit_effect::UnitEffects,
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
    /// A Lucky hero: whoever attacks it has one plus less to hit.
    pub lucky: bool,
}

#[cfg(test)]
impl DefenderState {
    /// For tests: put a lasting spell on the defender, with what the shipped rules say it does.
    pub(crate) fn put_enchantment(&mut self, spell: SpellId) {
        let rules = crate::game::testutil::shipped().rules;
        self.enchantments.push(spell);
        self.immunities.add_spell(spell, &rules);
        self.effects
            .union(crate::spell::unit_effect::UnitEffects::of(
                rules.spells.get(spell),
            ));
    }
}

impl DefenderState {
    /// Whether the blow is doom damage to this defender: from its source (a doom gaze, a Chaos
    /// weapon) or because the defender turns it into doom (Black Sleep). Every die hits and no
    /// defence is rolled.
    fn takes_doom(&self, attacker: &Combatant) -> bool {
        self.immunities
            .convert(attacker.types())
            .contains(DamageType::Doom)
    }

    /// The defence it rolls against one of `attacker`'s attacks: its own plus what its abilities
    /// and lasting spells add against the attack's types (a large shield against a ranged
    /// one, Elemental Armor against chaos and nature), then what its floors make of it. `None`
    /// if it is immune to the attack: nothing gets through.
    fn defense_versus(&self, attacker: &Combatant) -> Option<i32> {
        let types = self.immunities.convert(attacker.types());
        // Berserk: no defence at all, whatever else the unit has (immunities still count).
        let defense = if self.effects.contains(UnitEffect::Berserk) {
            0
        } else {
            self.defense + self.immunities.defence_bonus(types)
        };
        effective_defense(
            defense,
            &self.immunities,
            types,
            attacker.nature.armor_piercing,
        )
    }

    /// The blows one roll of `defense` stops: Invulnerability stops two more.
    fn blocks(
        &self,
        defense: i32,
        eldritch: bool,
        rules: &BattleRules,
        rng: &mut dyn RngCore,
    ) -> i32 {
        let invulnerable = self.effects.contains(UnitEffect::Invulnerability);
        roll_successes(defense, self.to_block - i32::from(eldritch), rng)
            + if invulnerable {
                rules.invulnerability_block
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
        let resistance = effective_resistance(self.resistance, &self.immunities, realm);
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

impl Combatant {
    /// What the attack is made of, with how it is delivered.
    pub fn types(&self) -> DamageTypes {
        self.nature.types.with(delivery(self.mode))
    }

    /// The dice that count against a defender with these immunities: the dice of a part whose
    /// types the defender is immune to do nothing to it.
    pub fn strength_against(&self, immunities: &Immunities) -> i32 {
        let lost: i32 = self
            .parts
            .iter()
            .filter(|(_, types)| immunities.stops(immunities.convert(*types)))
            .map(|(dice, _)| dice)
            .sum();
        self.strength - lost.clamp(0, self.strength)
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
    /// Hit points the attacker heals from what it stole (life steal).
    pub healed: i32,
    /// Of `points`, the damage of a life steal or a create-undead blow: a unit that dies of
    /// it can be raised as undead (`Fate`).
    pub drained: i32,
    /// Of `points`, the damage of a destruction or of a spell that severs or burns: a unit
    /// that dies of it is gone for good.
    pub gone: i32,
    /// Mana the target is to lose (Power Drain).
    pub mana_lost: i32,
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
        self.mana_lost += other.mana_lost;
    }
}

/// Resolve one attack by `figures` attacking figures against `def`, which is updated.
///
/// Order, as in the original: a gaze, once, against each of the target's figures; then for
/// each attacking figure the touches (dispel evil, stoning, death, life steal, destruction),
/// the blows themselves, and the poison.
pub fn process_attack(
    rules: &BattleRules,
    attacker: &Combatant,
    figures: u32,
    def: &mut DefenderState,
    rng: &mut dyn RngCore,
) -> Damage {
    let mut total = Damage::default();
    let sp = &attacker.special;
    // Magic immunity switches off every melee touch and gaze, as in the original (task 711): the
    // exceptions are the thrown attack, the poison touch and doom damage, which never asked.
    let touchable = !def.immunities.has(DamageType::Magic);
    if attacker.immolation && attacker.mode == Mode::Melee && touchable && def.figures > 0 {
        let fire = Combatant {
            strength: rules.immolation_strength,
            mode: Mode::Ranged,
            nature: AttackNature::magical(Realm::Chaos, &[DamageType::Fire]),
            ..Combatant::default()
        };
        total += spell_attack(rules, &fire, Volley::Area, def, rng);
    }
    if attacker.mode == Mode::ThrownOrBreath && sp.has_gaze() && touchable {
        let gazed = def.figures;
        if sp.stoning_gaze && !def.immunities.has(DamageType::Stoning) {
            for _ in 0..gazed {
                if def.resistance_failure(Some(Realm::Nature), -sp.penalty, rng) > 0 {
                    total += kill_figure(def).all_gone();
                }
            }
        }
        if sp.death_gaze && !def.immunities.has(DamageType::Death) {
            for _ in 0..gazed {
                if def.resistance_failure(Some(Realm::Death), -sp.penalty, rng) > 0 {
                    total += kill_figure(def);
                }
            }
        }
    }
    let strength = attacker.strength_against(&def.immunities);
    if strength <= 0 || def.figures == 0 {
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
                && !def.immunities.has(DamageType::Stoning)
                && def.resistance_failure(Some(Realm::Nature), -sp.penalty - sp.stoning_extra, rng)
                    > 0
            {
                total += kill_figure(def).all_gone();
            }
            if sp.death_touch
                && !def.immunities.has(DamageType::Death)
                && def.resistance_failure(Some(Realm::Death), -sp.penalty - sp.death_extra, rng) > 0
            {
                total += kill_figure(def);
            }
            if sp.life_steal && !def.immunities.has(DamageType::Death) {
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
        // An attack the defender is immune to does nothing, whatever else is true of it.
        let mut dmg = if let Some(defense) = defense {
            if def.takes_doom(attacker) {
                strength
            } else {
                let hits = roll_successes(strength, attacker.to_hit - i32::from(def.lucky), rng);
                (hits - def.blocks(defense, attacker.eldritch, rules, rng)).max(0)
            }
        } else {
            0
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
                if dmg > 0 && !def.takes_doom(attacker) {
                    let blocked = def.blocks(defense.unwrap_or(0), attacker.eldritch, rules, rng);
                    dmg = (dmg - blocked).max(0);
                }
            } else {
                def.front_damage += dmg;
                total.points += dmg;
                dmg = 0;
            }
        }
        if sp.poison > 0 && !def.immunities.has(DamageType::Poison) {
            for _ in 0..sp.poison {
                if def.figures > 0 && def.resistance_failure(None, 0, rng) > 0 {
                    total += unblockable(def, 1);
                }
            }
        }
    }
    // Power Drain, once for the attack that hurt: the target resists, or loses 2 to 20 mana.
    if sp.power_drain
        && touchable
        && total.points > 0
        && def.resistance_failure(Some(Realm::Death), 0, rng) > 0
    {
        total.mana_lost = rng.gen_range(rules.power_drain_min..=rules.power_drain_max);
    }
    // What a creator of undead deals, apart from what the touches destroy, can raise the dead.
    if sp.create_undead && !def.immunities.has(DamageType::Magic) {
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
    rules: &BattleRules,
    attacker: &Combatant,
    volley: Volley,
    def: &mut DefenderState,
    rng: &mut dyn RngCore,
) -> Damage {
    if attacker.strength <= 0 || def.figures == 0 {
        return Damage::default();
    }
    let Some(defense) = def.defense_versus(attacker) else {
        return Damage::default();
    };
    let blow = |strength: i32, rng: &mut dyn RngCore| {
        // A sleeper takes the full strength of every blow.
        if def.takes_doom(attacker) {
            strength
        } else {
            let hits = roll_successes(strength, attacker.to_hit - i32::from(def.lucky), rng);
            (hits - def.blocks(defense, attacker.eldritch, rules, rng)).max(0)
        }
    };
    match volley {
        Volley::Bolt => process_attack(
            rules,
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
    /// A second swing in the same exchange (Haste). Only the attacker's counts: a hasted
    /// defender still strikes back once.
    pub extra_swing: bool,
    pub state: DefenderState,
}

/// The damage each side took.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Exchange {
    pub dealt: Damage,
    pub taken: Damage,
}

/// A melee attack and the defender's reply. Both fighters' states are updated. Life stolen heals
/// the thief at once. The order of the blows (thrown or breath, first strike, the defender's
/// counter-attack, the attacker's swing) and Haste's extra swing in the same exchange: see ReMoM
/// `Combat.c`, `Battle_Unit_Attack`.
pub fn melee_exchange(
    rules: &BattleRules,
    att: &mut Fighter,
    def: &mut Fighter,
    rng: &mut dyn RngCore,
) -> Exchange {
    let mut out = Exchange::default();
    // Thrown weapons and breath go first, with every figure the attacker has.
    if let Some(thrown) = &att.thrown {
        let d = process_attack(rules, thrown, att.state.figures, &mut def.state, rng);
        att.state.heal_overflow(d.healed);
        out.dealt += d;
    }
    // A defender with a gaze gazes back before the blows, with the figures it has left.
    if let Some(gaze) = def.thrown.as_ref().filter(|t| t.special.has_gaze()) {
        if def.state.figures > 0 && att.state.figures > 0 {
            let d = process_attack(rules, gaze, def.state.figures, &mut att.state, rng);
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
        let d = process_attack(rules, &att.melee, figures, &mut def.state, rng);
        att.state.heal_overflow(d.healed);
        out.dealt += d;
    }
    // The defender strikes back with the figures it has left, less those the attacker frightens.
    if def.state.figures > 0 && att.state.figures > 0 {
        let figures = def
            .state
            .figures
            .saturating_sub(feared_figures(&att.melee, &def.state, rng));
        let d = process_attack(rules, &def.melee, figures, &mut att.state, rng);
        def.state.heal_overflow(d.healed);
        out.taken += d;
    }
    // An attacker without first strike now swings with the figures that survived.
    if !first_strike && att.state.figures > 0 && def.state.figures > 0 {
        let figures = att
            .state
            .figures
            .saturating_sub(feared_figures(&def.melee, &att.state, rng));
        let d = process_attack(rules, &att.melee, figures, &mut def.state, rng);
        att.state.heal_overflow(d.healed);
        out.dealt += d;
    }
    // Haste: one more swing, with the figures that are left (ReMoM `Combat.c`, `Battle_Unit_Attack`).
    if att.extra_swing && att.state.figures > 0 && def.state.figures > 0 {
        let figures = att
            .state
            .figures
            .saturating_sub(feared_figures(&def.melee, &att.state, rng));
        let d = process_attack(rules, &att.melee, figures, &mut def.state, rng);
        att.state.heal_overflow(d.healed);
        out.dealt += d;
    }
    out
}

/// How many of `def`'s figures `attacker` frightens: each rolls a death resistance check, and
/// the ones that fail cannot fight this round (attack or counter-attack, whoever began). Nothing for an attacker that causes no fear or a
/// defender immune to death.
fn feared_figures(attacker: &Combatant, def: &DefenderState, rng: &mut dyn RngCore) -> u32 {
    if !attacker.fear || def.immunities.has(DamageType::Death) {
        return 0;
    }
    (0..def.figures)
        .filter(|_| def.resistance_failure(Some(Realm::Death), 0, rng) > 0)
        .count() as u32
}

/// A ranged attack: the target fires nothing back.
pub fn ranged_attack(
    rules: &BattleRules,
    attacker: &Combatant,
    figures: u32,
    def: &mut DefenderState,
    rng: &mut dyn RngCore,
) -> Damage {
    process_attack(rules, attacker, figures, def, rng)
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
