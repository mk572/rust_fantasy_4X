//! What a spell does to a battle in numbers: the damage it deals, what it would do to a unit, the
//! chance a unit resists it. Casting a spell (the wizard, the mana, the events) is in
//! `f4x-core`'s `battle/spell.rs`.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::id::PlayerId;
use f4x_def::rules::attack_nature::AttackNature;
use f4x_def::rules::damage_type::DamageTypes;
use f4x_def::rules::effects::DamageTraits;
use f4x_def::rules::immunity::{effective_resistance, Immunities};
use f4x_def::rules::realm::Realm;
use f4x_def::rules::{SpellDef, SpellId};
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

use super::spell_type::{DamageOdds, SpellType};
use crate::battle::{Battle, BattleEvent};
use crate::side::Side;

impl Battle {
    /// The damage a spell does, by its id: strength, its types and traits.
    pub fn damage_of(&self, spell: SpellId) -> Option<(i32, DamageTypes, DamageTraits)> {
        match SpellType::of(spell, self.content.rules.spells.get(spell))? {
            SpellType::Damage {
                strength,
                types,
                traits,
                ..
            } => Some((strength, types, traits)),
            _ => None,
        }
    }

    /// What a damage spell would do to one unit of the battle, tried on copies of the unit with
    /// the game's own damage code (defence, immunities, realm, area or chain): the average hit
    /// points taken and the share of the tries that destroy the unit. `extra_mana` is mana beyond
    /// the cost, for a spell that turns it into strength. `None` if the spell does not damage, or
    /// the unit is immune to it.
    pub fn preview_spell_damage(
        &self,
        spell: SpellId,
        target: usize,
        extra_mana: u32,
    ) -> Option<DamageOdds> {
        const TRIES: u32 = 24;
        let (strength, types, traits) = self.damage_of(spell)?;
        let unit = self.unit(target)?;
        if Immunities::of(&unit.abilities, &unit.enchantments, &self.content.rules).stops(types) {
            return None;
        }
        let extra = extra_mana
            .checked_div(traits.mana_per_point)
            .map_or(0, |points| points as i32);
        let nature = AttackNature {
            types,
            armor_piercing: false,
        };
        let (mut points, mut kills) = (0.0, 0);
        for seed in 0..TRIES {
            let mut copy = unit.clone();
            let mut rng = Pcg64Mcg::seed_from_u64(u64::from(seed) + 1);
            let dealt = super::casting::damage_unit_with_spell(
                &self.content.rules,
                &mut copy,
                strength + extra,
                &traits,
                nature,
                &mut rng,
            )?;
            points += dealt.points as f32;
            kills += u32::from(copy.figures == 0);
        }
        Some(DamageOdds {
            mean: points / TRIES as f32,
            kill_chance: kills as f32 / TRIES as f32,
        })
    }

    /// Hit units with a damage spell; each gets its own events.
    pub fn strike(
        &mut self,
        targets: &[usize],
        strength: i32,
        types: DamageTypes,
        traits: &DamageTraits,
    ) -> Vec<BattleEvent> {
        let nature = AttackNature {
            types,
            armor_piercing: false,
        };
        let mut events = Vec::new();
        for &t in targets {
            let dmg = self.spell_damage(t, strength, traits, nature);
            events.extend(self.spell_events_for_damage(t, dmg, nature.types));
        }
        events
    }

    /// The side `player` fights on.
    pub fn side_of(&self, player: PlayerId) -> Option<Side> {
        [Side::Attacker, Side::Defender]
            .into_iter()
            .find(|&s| self.player_of(s) == player)
    }

    /// The chance that a unit of the battle fails the resistance check of a spell with a save
    /// (a curse, Possession, a banishment): 1 for a spell with no save or no such unit. The
    /// check is the one `resist_fails` rolls, a ten-sided die over the resistance.
    pub fn save_failure_chance(&self, spell: &SpellDef, target: usize) -> f32 {
        match &spell.save {
            Some(save) => self.failure_chance(target, Realm::of(spell.realm), save.modifier),
            None => 1.0,
        }
    }

    /// The chance that a unit of the battle fails a resistance check of `realm` with `modifier`
    /// (negative for a spell that is hard to resist); 1 for a unit that is not there.
    pub fn failure_chance(&self, target: usize, realm: Option<Realm>, modifier: i32) -> f32 {
        let Some(unit) = self.units.get(target) else {
            return 1.0;
        };
        let state = unit.defender_state(&self.content.rules);
        let resistance = effective_resistance(state.resistance, &state.immunities, realm);
        let needed = resistance + modifier - self.save_penalty;
        (10 - needed.clamp(0, 10)) as f32 / 10.0
    }

    /// A resistance check of one unit against a realm: whether it *fails*.
    pub fn resist_fails(&mut self, target: usize, realm: Option<Realm>, modifier: i32) -> bool {
        let state = self.units[target].defender_state(&self.content.rules);
        let rng: &mut dyn rand::RngCore = &mut self.rng;
        state.resistance_failure(realm, modifier - self.save_penalty, rng) > 0
    }
}
