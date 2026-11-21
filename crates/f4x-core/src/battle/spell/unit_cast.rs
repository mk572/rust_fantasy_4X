//! The game's side of the casts of battle units (`f4x_battle::battlemagic::unit_cast` has the
//! rules: what a unit may cast, what it costs, the demon): Evil Omens, the enemy's Counter Magic
//! and the node, the wizard's retorts and the spell's effect on the world.
//!
//! Tests: in battle/tests.rs (`a_hero_casts_an_innate_spell_...`, `the_legal_unit_casts_...` and the demon tests).

use super::*;
pub use f4x_battle::battlemagic::unit_cast::{SpellSource, UnitSpell};

impl Game {
    /// The spells a battle unit can cast, in the order of its sources. Those the battle cannot
    /// play are left out. While it is the unit's side to play, `castable` says whether it can
    /// cast each now.
    pub fn battle_unit_spells(&self, unit: usize) -> Vec<UnitSpell> {
        self.battle()
            .map_or_else(Vec::new, |b| b.unit_spells(unit, self.evil_omens_active()))
    }

    /// The casts the units of the side to play may make now: for each unit and spell it could
    /// cast, every aim the spell accepts and the mana that may go in. Like
    /// [`Game::legal_battle_casts`], for the AI and the console.
    pub fn legal_unit_casts(&self) -> Vec<(usize, LegalCast)> {
        self.battle().map_or_else(Vec::new, |b| {
            b.legal_unit_casts(self, self.evil_omens_active())
        })
    }

    /// Whether a unit can summon a demon now (for the AI and the menus).
    pub fn can_summon_demon(&self, unit: usize) -> bool {
        self.battle().is_some_and(|b| b.demon_squares(unit).is_ok())
    }

    /// A unit with Creature Summoning calls a demon (`Battle::summon_demon`).
    pub(in crate::battle) fn battle_summon_demon(
        &mut self,
        unit: usize,
    ) -> Result<Vec<Event>, BattleError> {
        let active = self.battle.as_mut().ok_or(BattleError::Over)?;
        let event = active.battle.summon_demon(unit)?;
        Ok(vec![Event::Battle(event)])
    }

    /// A unit casts a spell of its own.
    pub(in crate::battle) fn battle_unit_cast(
        &mut self,
        unit: usize,
        spell: SpellId,
        aim: SpellAim,
        mana: u32,
    ) -> Result<Vec<Event>, BattleError> {
        let battle = &self.battle.as_ref().ok_or(BattleError::Over)?.battle;
        let (def, kind, source, paid, pool) =
            battle.check_unit_cast(self, self.evil_omens_active(), unit, spell, aim, mana)?;
        let battle = &mut self.battle.as_mut().ok_or(BattleError::Over)?.battle;
        let side = battle.active;
        let player = battle.player_of(side);

        // Pay: the unit's mana, a charge or the ability, and its turn.
        let spell_save = battle.pay_unit_cast(unit, spell, source, paid);
        let mut events = vec![BattleEvent::UnitSpellCast {
            unit,
            spell,
            mana: paid,
        }];

        // The enemy's Counter Magic tries to stop it (not the Web an ability casts).
        let exempt = source == SpellSource::Ability
            && crate::spell::unit_effect::UnitEffect::Web.is_in(&def);
        let difficulty = pool as u32 * self.dispel_multiplier(Some(def.realm), Some(player));
        let stopped =
            !exempt && self.battle_spell_stopped(side, player, Some(def.realm), difficulty);
        if stopped {
            events.push(BattleEvent::SpellCountered { side, spell });
        } else {
            if let Some(a) = self.battle.as_mut() {
                a.battle.save_penalty = spell_save;
                a.battle.spell_caster = Some(unit);
            }
            events.extend(self.apply_battle_spell(spell, &def, kind, side, player, aim, pool));
            if let Some(a) = self.battle.as_mut() {
                a.battle.save_penalty = 0;
                a.battle.spell_caster = None;
            }
        }
        if let Some(active) = self.battle.as_mut() {
            active.battle.refresh_fields();
        }
        let mut out = self.battle_event_list(events);
        let ended = self
            .battle
            .as_mut()
            .map(|a| a.battle.check_winner())
            .unwrap_or_default();
        out.extend(ended.into_iter().map(Event::Battle));
        self.finish_if_over(&mut out);
        Ok(out)
    }
}
