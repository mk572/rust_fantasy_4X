//! Casting in a battle: which spells a wizard can cast there, what they cost, and what they do.
//!
//! The rules follow the original's `Combat_Cast_Spell` and `Combat_Cast_Apply_Spell_Effect`:
//! - The wizard casts one spell on each of their turns, from the spells they know that can be
//!   cast in battle. A spell costs its price (before the overland five-fold), less the usual
//!   reductions, out of the casting skill they have for the whole battle, and the reserve pays
//!   that times the range multiplier (`casting::range_percent`).
//! - A bolt, a dispel and Counter Magic take extra mana: the more put in, the stronger the
//!   spell (one more strength per `mana_per_point` mana for a bolt; a dispel's strength is the
//!   mana; Counter Magic's reserve is the mana).
//! - The enemy's Counter Magic reserve then tries to stop the spell: a dispel of the reserve's
//!   strength against the mana put in. The attempt drains the reserve either way, and a stopped
//!   spell still costs its mana and the wizard's turn.
//! - Lasting spells on units last until the battle ends; a dispel can take them off.
//!
//! Not yet: spells on the whole battlefield (task 44), the spells whose effects are not in the
//! game, the node counter and the heroes' own casting.

use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

use super::casting::{BattleCaster, SpellAim, SpellError, MAX_SIDE_UNITS};
use super::immunity::{AttackNature, Immunities, Realm};
use super::{Battle, BattleError, BattleEvent, Side};
use crate::city::{PlayerId, Position};
use crate::rules::defs::{CastContext, Duration, SpellDef, SpellTarget};
use crate::rules::effects::{Condition, DamageTraits, Effect, Trigger};
use crate::rules::stats::{Modifier, Source};
use crate::rules::SpellId;
use crate::units::UnitEnchantments;
use crate::{Event, Game};

/// What a spell does in a battle, from its effects.
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Damage {
        strength: i32,
        immune: Vec<String>,
        realm: Option<Realm>,
        traits: DamageTraits,
    },
    Heal(i32),
    Summon(String),
    /// A lasting spell on one unit.
    Enchant {
        mods: Vec<Modifier>,
        grants: Vec<String>,
    },
    /// A dispel of this strength for each mana put in.
    Dispel(u32),
    CounterMagic,
}

impl Kind {
    /// Whether extra mana makes the spell stronger.
    fn takes_extra_mana(&self) -> bool {
        match self {
            Kind::Damage { traits, .. } => traits.mana_per_point > 0,
            Kind::Dispel(_) | Kind::CounterMagic => true,
            _ => false,
        }
    }
}

/// A spell the wizard could cast in the battle, for a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleSpell {
    pub spell: SpellId,
    /// Casting skill it costs at least.
    pub cost: i32,
    /// More mana can be put in to make it stronger.
    pub takes_extra_mana: bool,
    /// What it must be aimed at.
    pub target: SpellTarget,
    /// Whether the wizard can cast it now (skill, reserve, and the one spell a turn).
    pub castable: bool,
}

/// Creatures a spell may only be thrown at, by realm: Star Fires burns chaos and death
/// creatures and nothing else.
fn only_against(spell_id: &str) -> Option<&'static [Realm]> {
    match spell_id {
        "star_fires" => Some(&[Realm::Chaos, Realm::Death]),
        _ => None,
    }
}

impl Game {
    /// What the spell does in a battle, or `None` if it cannot be cast there (yet).
    fn battle_kind(&self, spell: SpellId, def: &SpellDef) -> Option<Kind> {
        if !def.contexts.contains(&CastContext::Battle) {
            return None;
        }
        // A lasting spell on one unit: every rule is a plain modifier or granted ability.
        if matches!(def.target, SpellTarget::OwnUnit | SpellTarget::EnemyUnit)
            && def.duration != Duration::Instant
        {
            let mut mods = Vec::new();
            let mut grants = Vec::new();
            for r in &def.effects {
                if r.when != Trigger::Passive || r.condition != Condition::Always {
                    return None;
                }
                match &r.effect {
                    Effect::Modify { stat, op, value } => {
                        mods.push(Modifier::new(*stat, *op, *value, Source::Spell(spell)));
                    }
                    Effect::GrantAbility(a) => grants.push(a.clone()),
                    _ => return None,
                }
            }
            if mods.is_empty() && grants.is_empty() {
                return None;
            }
            return Some(Kind::Enchant { mods, grants });
        }
        let [rule] = def.effects.as_slice() else {
            return None;
        };
        if rule.when != Trigger::OnCast {
            return None;
        }
        match &rule.effect {
            Effect::Damage {
                amount,
                immune,
                realm,
                traits,
                ..
            } => Some(Kind::Damage {
                strength: *amount,
                immune: immune.clone(),
                realm: realm.as_deref().and_then(Realm::from_id),
                traits: *traits,
            }),
            Effect::Heal(n) => Some(Kind::Heal(*n)),
            Effect::Summon { unit, .. } => Some(Kind::Summon(unit.clone())),
            Effect::Dispel { strength } => Some(Kind::Dispel(*strength / def.cost.max(1))),
            Effect::Custom(h) if h == crate::dispel::COUNTER_MAGIC => Some(Kind::CounterMagic),
            _ => None,
        }
    }

    /// The wizard of `player` in the battle, if one is on.
    fn battle_caster_of(&self, player: PlayerId) -> Option<BattleCaster> {
        let b = self.battle()?;
        b.caster(b.side_of(player)?).copied()
    }

    /// The spells `player` knows that can be cast in a battle, in the order of the spell list.
    /// While a battle is on, `castable` says whether the wizard can cast each now.
    pub fn battle_spells(&self, player: PlayerId) -> Vec<BattleSpell> {
        let Some(magic) = self.magic(player) else {
            return Vec::new();
        };
        let caster = self.battle_caster_of(player);
        magic
            .known
            .iter()
            .filter_map(|&spell| {
                let def = self.content.rules.spells.get(spell);
                let kind = self.battle_kind(spell, def)?;
                let cost = self.casting_cost(player, spell, true);
                Some(BattleSpell {
                    spell,
                    cost,
                    takes_extra_mana: kind.takes_extra_mana(),
                    target: def.target,
                    castable: caster.is_some_and(|c| !c.cast_this_turn && cost <= c.affordable()),
                })
            })
            .collect()
    }

    /// What casting `spell` with `mana` put into it takes: the casting skill, and the strength
    /// the spell works with (the mana). `mana` 0 asks for the least. A spell that takes no extra
    /// mana costs the same whatever is asked.
    pub fn battle_spell_price(
        &self,
        player: PlayerId,
        spell: SpellId,
        mana: u32,
    ) -> Result<(i32, i32), SpellError> {
        let def = self.content.rules.spells.get(spell);
        let kind = self
            .battle_kind(spell, def)
            .ok_or(SpellError::Unsupported)?;
        let least = self.casting_cost(player, spell, true);
        if !kind.takes_extra_mana() {
            return Ok((least, def.cost as i32));
        }
        let pool = if mana == 0 {
            def.cost as i32
        } else {
            mana as i32
        };
        if pool < def.cost as i32 {
            return Err(SpellError::TooLittle);
        }
        let cut = self.casting_cost_reduction(player, spell);
        Ok((least + (pool - def.cost as i32) * (100 - cut) / 100, pool))
    }

    /// The most mana the wizard can put into the spell now (its cost, if it takes no extra).
    pub fn battle_spell_most_mana(&self, player: PlayerId, spell: SpellId) -> i32 {
        let def = self.content.rules.spells.get(spell);
        let Some(caster) = self.battle_caster_of(player) else {
            return 0;
        };
        let mut best = def.cost as i32;
        while let Ok((skill, pool)) = self.battle_spell_price(player, spell, best as u32 + 1) {
            if skill > caster.affordable() || pool <= best {
                break;
            }
            best = pool;
        }
        best
    }

    /// Set up the wizards of a battle: each side's wizard (if it has one) with their casting
    /// skill, reserve and the multiplier for the battle's distance from their fortress.
    pub(super) fn battle_casters(&self, battle: &mut Battle, site: Position) {
        for side in [Side::Attacker, Side::Defender] {
            let player = battle.player_of(side);
            if self.is_neutral_player(player) {
                continue;
            }
            let (Some(p), Some(magic)) = (self.player(player), self.magic(player)) else {
                continue;
            };
            // A wizard on the way home from banishment has nothing to cast with.
            let skill = if self.casting_return(player) {
                0
            } else {
                self.casting_skill(player)
            };
            let distance = p.fortress.filter(|f| f.plane == site.plane).and_then(|f| {
                let map = self.map.as_ref()?;
                Some(map.plane(f.plane).chebyshev((f.x, f.y), (site.x, site.y)))
            });
            let range =
                super::casting::range_percent(distance, magic.has(crate::magic::Retort::Channeler));
            battle.set_caster(side, BattleCaster::new(skill, p.mana, range));
        }
    }

    /// The wizard whose turn it is casts a spell.
    pub(super) fn battle_cast(
        &mut self,
        spell: SpellId,
        aim: SpellAim,
        mana: u32,
    ) -> Result<Vec<Event>, BattleError> {
        let err = BattleError::Spell;
        let battle = &self.battle.as_ref().ok_or(BattleError::Over)?.battle;
        let caster = battle.check_caster()?;
        let side = battle.active;
        let player = battle.player_of(side);
        let def = self.content.rules.spells.get(spell).clone();
        if !self.magic(player).is_some_and(|m| m.known.contains(&spell)) {
            return Err(err(SpellError::NotKnown));
        }
        if !def.contexts.contains(&CastContext::Battle) {
            return Err(err(SpellError::NotForBattle));
        }
        let kind = self
            .battle_kind(spell, &def)
            .ok_or(err(SpellError::Unsupported))?;
        let (skill, pool) = self.battle_spell_price(player, spell, mana).map_err(err)?;
        if skill > caster.affordable() {
            return Err(err(SpellError::TooExpensive));
        }
        self.check_aim(battle, side, &def, &kind, aim)
            .map_err(err)?;

        // Pay: the skill, and the mana from the reserve.
        let paid = self
            .battle
            .as_mut()
            .map_or(0, |a| a.battle.pay_for_spell(skill));
        if let Some(p) = self.player_mut(player) {
            p.mana = (p.mana - paid).max(0);
        }
        let mut events = vec![BattleEvent::SpellCast {
            side,
            spell,
            skill,
            mana: paid,
        }];

        // The enemy's Counter Magic reserve tries to stop it.
        let difficulty = pool as u32 * self.dispel_multiplier(&def.realm, Some(player));
        let stopped = self
            .battle
            .as_mut()
            .is_some_and(|a| a.battle.counter_magic_stops(side, difficulty));
        if stopped {
            events.push(BattleEvent::SpellCountered { side, spell });
        } else {
            events.extend(self.apply_battle_spell(spell, &def, kind, side, player, aim, pool));
        }
        let mut out: Vec<Event> = events.into_iter().map(Event::Battle).collect();
        let ended = self.battle.as_mut().and_then(|a| a.battle.check_winner());
        if let Some(end) = ended {
            out.push(Event::Battle(end));
            out.extend(self.finish_battle());
        }
        Ok(out)
    }

    /// Whether the spell may be aimed here.
    fn check_aim(
        &self,
        battle: &Battle,
        side: Side,
        def: &SpellDef,
        kind: &Kind,
        aim: SpellAim,
    ) -> Result<(), SpellError> {
        match (def.target, aim) {
            (SpellTarget::None | SpellTarget::Battlefield, SpellAim::None) => Ok(()),
            (SpellTarget::Tile, SpellAim::Tile(cell)) => {
                if battle.active_units(side).count() >= MAX_SIDE_UNITS
                    || !battle.is_summon_square(side, cell)
                {
                    return Err(SpellError::NoRoom);
                }
                Ok(())
            }
            (
                target @ (SpellTarget::OwnUnit | SpellTarget::EnemyUnit | SpellTarget::AnyUnit),
                SpellAim::Unit(id),
            ) => {
                let u = battle
                    .unit(id)
                    .filter(|u| u.is_active())
                    .ok_or(SpellError::BadTarget)?;
                let own = u.side == side;
                if (target == SpellTarget::OwnUnit && !own)
                    || (target == SpellTarget::EnemyUnit && own)
                {
                    return Err(SpellError::BadTarget);
                }
                if matches!(kind, Kind::Enchant { .. }) && u.enchantments.contains(&def.id) {
                    return Err(SpellError::BadTarget);
                }
                if let Some(realms) = only_against(&def.id) {
                    if !u.realm.is_some_and(|r| realms.contains(&r)) {
                        return Err(SpellError::BadTarget);
                    }
                }
                Ok(())
            }
            _ => Err(SpellError::BadTarget),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_battle_spell(
        &mut self,
        spell: SpellId,
        def: &SpellDef,
        kind: Kind,
        side: Side,
        player: PlayerId,
        aim: SpellAim,
        pool: i32,
    ) -> Vec<BattleEvent> {
        match (kind, aim) {
            (
                Kind::Damage {
                    strength,
                    immune,
                    realm,
                    traits,
                },
                SpellAim::Unit(target),
            ) => {
                let extra = if traits.mana_per_point > 0 {
                    (pool - def.cost as i32) / traits.mana_per_point as i32
                } else {
                    0
                };
                let nature = AttackNature {
                    stopped_by: Immunities::from_names(immune.iter().map(String::as_str)),
                    realm,
                    armor_piercing: false,
                };
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                let dmg = battle.spell_damage(target, strength + extra, &traits, nature);
                battle.spell_events_for_damage(target, dmg)
            }
            (Kind::Heal(points), SpellAim::Unit(target)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let healed = active.battle.spell_heal(target, points);
                vec![BattleEvent::SpellHealed {
                    unit: target,
                    healed,
                }]
            }
            (Kind::CounterMagic, _) => {
                if let Some(active) = self.battle.as_mut() {
                    active.battle.cast_counter_magic(side, pool as u32);
                }
                vec![BattleEvent::CounterMagicRaised {
                    side,
                    strength: pool as u32,
                }]
            }
            (Kind::Enchant { mods, grants }, SpellAim::Unit(target)) => {
                if let Some(active) = self.battle.as_mut() {
                    active
                        .battle
                        .enchant_unit(target, spell, &def.id, player, &mods, &grants);
                }
                vec![BattleEvent::SpellEnchanted {
                    unit: target,
                    spell,
                }]
            }
            (Kind::Summon(name), SpellAim::Tile(cell)) => {
                let Some(unit_id) = self.content.rules.units.id_of(&name) else {
                    return Vec::new();
                };
                // Make the creature as the game would, copy it, and let the original go: the
                // copy vanishes with the battle.
                let site = self
                    .battle()
                    .and_then(|b| b.site)
                    .or_else(|| self.summon_point(player));
                let Some(site) = site else {
                    return Vec::new();
                };
                let entity = self.spawn_unit(player, site, unit_id);
                let copy = self.battle_unit_from(entity, side, 0);
                let _ = self.world.despawn(entity);
                let Some(unit) = copy else {
                    return Vec::new();
                };
                let id = self
                    .battle
                    .as_mut()
                    .map_or(0, |a| a.battle.add_summoned(side, unit, cell));
                vec![BattleEvent::Summoned { unit: id }]
            }
            (Kind::Dispel(per_mana), SpellAim::Unit(target)) => {
                self.dispel_battle_unit(target, pool as u32 * per_mana, player)
            }
            _ => Vec::new(),
        }
    }

    /// A dispel of `strength` by `by` on a unit of the battle: the spells cast on it in the
    /// battle, and the lasting spells on the overland unit it was copied from, if another
    /// wizard cast them.
    fn dispel_battle_unit(
        &mut self,
        target: usize,
        strength: u32,
        by: PlayerId,
    ) -> Vec<BattleEvent> {
        let Some(active) = self.battle.as_ref() else {
            return Vec::new();
        };
        let unit = active.battle.units[target].clone();
        let entity = unit.source.and_then(|i| active.sources.get(i).copied());
        let mut rng = Pcg64Mcg::seed_from_u64(self.next_u64());

        let (kept, mut removed) =
            self.dispel_list(unit.combat_spells.clone(), strength, by, true, &mut rng);
        let mut overland_changed = false;
        if let Some(e) = entity {
            let list = self
                .world
                .get::<&UnitEnchantments>(e)
                .map(|l| l.0.clone())
                .unwrap_or_default();
            let (left, gone) = self.dispel_list(list, strength, by, true, &mut rng);
            if gone > 0 {
                let _ = self.world.insert_one(e, UnitEnchantments(left));
                overland_changed = true;
                removed += gone;
            }
        }
        let event = BattleEvent::SpellDispelled {
            unit: target,
            removed,
        };
        if removed == 0 {
            return vec![event];
        }
        // Rebuild the unit: from the overland unit as it is now, or from its saved numbers.
        let base = match entity {
            Some(e) if overland_changed || unit.base.is_none() => {
                self.battle_unit_from(e, unit.side, unit.source.unwrap_or(0))
            }
            _ => unit.base.as_deref().cloned(),
        };
        let Some(base) = base else {
            return vec![event];
        };
        let lasting: Vec<_> = kept
            .iter()
            .filter_map(|&(s, _)| {
                let def = self.content.rules.spells.get(s);
                match self.battle_kind(s, def)? {
                    Kind::Enchant { mods, grants } => Some((def.id.clone(), mods, grants)),
                    _ => None,
                }
            })
            .collect();
        if let Some(active) = self.battle.as_mut() {
            active.battle.rebuild_unit(target, base, kept, &lasting);
        }
        vec![event]
    }
}

impl Battle {
    /// The side `player` fights on.
    pub fn side_of(&self, player: PlayerId) -> Option<Side> {
        [Side::Attacker, Side::Defender]
            .into_iter()
            .find(|&s| self.player_of(s) == player)
    }
}
