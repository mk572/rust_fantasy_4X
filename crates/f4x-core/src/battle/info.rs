//! The combat information window: every global and battlefield enchantment in force, split
//! between the two wizards (`Combat_Information_Window`, `Combat_Info_Effects_Base` and
//! `Combat_Info_Effects` in ReMoM `Combat.c`).
//!
//! The first section holds what belongs to the field itself: the node's dispelling and aura,
//! the besieged city's Cloud of Shadow and Heavenly Light, and the two spells that touch every
//! fight, Chaos Surge and Eternal Night. Under it each wizard's own spells follow: Crusade,
//! Holy Arms, Charm of Life and Zombie Mastery, the opponent's on the left and the viewer's on
//! the right.
//!
//! Divergence: the original lists neither Evil Omens nor Spell Wards (`OGBUG: INCONSISTENT`);
//! Evil Omens is shown here, among the common effects, since it changes what every spell
//! costs. The original's drawing loop stops short of the per-side cells (its bound is half the
//! rows); the window here shows all of them.
//!
//! The layout (`InfoItem`, `BattleInfo`, `build`) is in `f4x-battle` (`battle/info.rs`) and is
//! re-exported here at the old path; this file gathers the facts from the game.
//!
//! Tests: in battle/tests.rs, on real battles (the spells of two wizards, a besieged city, a
//! node, an empty window, a monster opponent).

pub use f4x_battle::battle::info::*;

use super::Battle;
use crate::game::Game;
use crate::player::PlayerId;

impl Game {
    /// The combat information window for `viewer`, a wizard in the battle being fought.
    pub fn battle_info(&self, viewer: PlayerId) -> Option<BattleInfo> {
        let battle: &Battle = self.battle()?;
        let opponent = if battle.attacker == viewer {
            battle.defender
        } else {
            battle.attacker
        };
        let wizard = |p: PlayerId| {
            if !self.is_wizard(p) {
                return None;
            }
            let name = self.wizard(p)?.name();
            Some((
                name,
                WizardSpells {
                    crusade: self.has_global_effect(
                        p,
                        crate::spell::global_enchantment::GlobalEffect::Crusade,
                    ),
                    holy_arms: self.has_global_effect(
                        p,
                        crate::spell::global_enchantment::GlobalEffect::HolyArms,
                    ),
                    charm_of_life: self.has_global_effect(
                        p,
                        crate::spell::global_enchantment::GlobalEffect::CharmOfLife,
                    ),
                    zombie_mastery: self.has_global_effect(
                        p,
                        crate::spell::global_enchantment::GlobalEffect::ZombieMastery,
                    ),
                },
            ))
        };
        // The city's own spells are listed only against a city that is not the viewer's: the
        // viewer sees theirs among their own enchantments.
        let city = battle
            .site
            .and_then(|s| self.economy().city_at(s))
            .filter(|&c| self.economy().city_owner(c) != Some(viewer));
        let on_city = |effect: crate::spell::city::CityEffect| {
            city.is_some_and(|c| self.economy().city_has_effect(c, effect))
        };
        Some(build(InfoFacts {
            node_site: battle.node_site,
            node_aura: battle.node_aura,
            cloud_of_shadow: on_city(crate::spell::city::CityEffect::CloudOfShadow),
            heavenly_light: on_city(crate::spell::city::CityEffect::HeavenlyLight),
            chaos_surge: self
                .any_global_effect(crate::spell::global_enchantment::GlobalEffect::ChaosSurge),
            eternal_night: self
                .any_global_effect(crate::spell::global_enchantment::GlobalEffect::EternalNight),
            evil_omens: self
                .any_global_effect(crate::spell::global_enchantment::GlobalEffect::EvilOmens),
            opponent: wizard(opponent),
            viewer: wizard(viewer),
        }))
    }
}
