//! Casting in a battle: which spells a wizard can cast there, what they cost, and what they do.
//!
//! The rules follow the original's `Combat_Cast_Spell` and `Combat_Cast_Apply_Spell_Effect`:
//! - The wizard casts one spell on each of their turns, from the spells they know that can be
//!   cast in battle. A spell costs its price (before the overland five-fold), less the usual
//!   reductions, out of the casting skill they have for the whole battle, and the reserve pays
//!   that times the range multiplier (`casting::range_percent`).
//! - A bolt, a dispel, Counter Magic and Banish take extra mana: the more put in, the stronger the
//!   spell (one more strength per `mana_per_point` mana for a bolt; a dispel's strength is the
//!   mana; Counter Magic's reserve is the mana).
//! - The enemy's Counter Magic reserve then tries to stop the spell: a dispel of the reserve's
//!   strength against the mana put in. The attempt drains the reserve either way, and a stopped
//!   spell still costs its mana and the wizard's turn.
//! - Lasting spells on units last until the battle ends; a dispel can take them off. A curse
//!   (`SpellDef::save`) first gives its target a resistance check.
//! - Battlefield enchantments (`field.rs`) stay up for the battle.
//!
//! Not here: dispelling battlefield enchantments, the spells whose effects are not in the game,
//! the node counter and the heroes' own casting.

use crate::rules::damage_type::DamageType;
use crate::rules::damage_type::DamageTypes;
use crate::rules::unit_flag::UnitFlag;
use crate::rules::Ability;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

use super::casting::{BattleCaster, SpellAim, SpellError};
use super::field::Field;
use super::immunity::Realm;
use super::WarpKind;
use super::{Battle, BattleError, BattleEvent, Side};
use crate::game::{DispelSite, Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::defs::{CastContext, SpellDef, SpellSave, SpellTarget};
use crate::rules::stats::Modifier;
use crate::rules::SpellId;
use crate::spell::target_error::TargetError;
use crate::spell::unit_effect::UnitEffect;
use f4x_battle::battlemagic::aim::{save_types, AimWorld};
pub use f4x_battle::battlemagic::spell_type::{
    slay_allows, BattleEffect, BattleSpell, CityWall, DamageOdds, LegalCast, SpellType,
    BATTLE_EFFECT,
};
use f4x_battle::battlemagic::spell_type::{undead_penalty, warp_mods};
use f4x_def::half_int::HalfInt;

mod unit_cast;
pub use unit_cast::{SpellSource, UnitSpell};

impl Game {
    /// The wizard of `player` in the battle, if one is on.
    fn battle_caster_of(&self, player: PlayerId) -> Option<BattleCaster> {
        let b = self.battle()?;
        b.caster(b.side_of(player)?).copied()
    }

    /// The spells `player` knows that can be cast in a battle, in the order of the spell list.
    /// While a battle is on, `castable` says whether the wizard can cast each now.
    pub fn battle_spells(&self, player: PlayerId) -> Vec<BattleSpell> {
        let Some(magic) = self.wizard(player) else {
            return Vec::new();
        };
        let caster = self.battle_caster_of(player);
        magic
            .known()
            .iter()
            .filter_map(|&spell| {
                let def = self.content.rules.spells.get(spell);
                let kind = SpellType::of(spell, def)?;
                let cost = self.casting_cost(player, spell, true);
                Some(BattleSpell {
                    spell,
                    cost,
                    takes_extra_mana: kind.takes_extra_mana(),
                    target: def.target,
                    castable: caster.is_some_and(|c| !c.cast_this_turn && cost <= c.affordable())
                        && !self.battle().is_some_and(|b| {
                            b.side_of(player)
                                .is_some_and(|side| b.ward_stops(side, Some(def.realm)))
                        }),
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
        let kind = SpellType::of(spell, def).ok_or(SpellError::Unsupported)?;
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
        let cut = self.wizard(player).map_or(0, |m| {
            m.cost_reduction(&self.content.rules, self.content.rules.spells.get(spell))
        });
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
            if !self.is_wizard(player) {
                continue;
            }
            let Some(magic) = self.wizard(player) else {
                continue;
            };
            // A wizard on the way home from banishment has nothing to cast with.
            let skill = if self.casting_return(player) {
                0
            } else {
                self.wizard(player)
                    .map_or(0, |m| m.casting_skill(&self.content.rules))
            };
            let distance = magic
                .fortress()
                .filter(|f| f.plane == site.plane)
                .and_then(|f| {
                    let map = self.map.as_ref()?;
                    Some(map.plane(f.plane).chebyshev((f.x, f.y), (site.x, site.y)))
                });
            let setting = &self.content.rules.config.casting_range;
            let mut range = super::casting::range_percent(setting, distance, |d| {
                magic.stat(&self.content.rules, crate::rules::Stat::CastingDistance, d)
            });
            // On corrupted ground a wizard who keeps Great Wasting going pays no range penalty.
            if self.is_corrupted(site)
                && self.has_global_effect(
                    player,
                    crate::spell::global_enchantment::GlobalEffect::GreatWasting,
                )
            {
                range = range.min(setting.base);
            }
            battle.set_caster(
                side,
                BattleCaster::with_mana(
                    skill,
                    self.player(player)
                        .map_or(HalfInt::default(), |p| p.treasury().mana),
                    range,
                ),
            );
        }
    }

    /// Each wizard's reserve in the game is what the battle says it is: spells and Mana Leak
    /// draw on it.
    pub(super) fn sync_battle_mana(&mut self) {
        let Some(active) = self.battle.as_ref() else {
            return;
        };
        let reserves: Vec<(PlayerId, HalfInt)> = [Side::Attacker, Side::Defender]
            .into_iter()
            .filter_map(|s| Some((active.battle.player_of(s), active.battle.caster(s)?.mana)))
            .collect();
        for (player, mana) in reserves {
            if let Some(t) = self.treasury_mut(player) {
                t.mana = mana;
            }
        }
    }

    /// Every check a cast must pass before it is paid for: the wizard, the spell, its price,
    /// the ward and the aim. `battle_cast` and the list of legal casts both use it, so they
    /// cannot disagree. Gives the spell's definition, what it does, the skill it costs and the
    /// strength it works with.
    fn check_cast(
        &self,
        spell: SpellId,
        aim: SpellAim,
        mana: u32,
    ) -> Result<(SpellDef, SpellType, i32, i32), BattleError> {
        let (def, kind, skill, pool) = self.check_spell(spell, mana)?;
        let battle = &self.battle.as_ref().ok_or(BattleError::Over)?.battle;
        battle
            .check_aim(self, battle.active, spell, &def, &kind, aim)
            .map_err(BattleError::Spell)?;
        Ok((def, kind, skill, pool))
    }

    /// Everything in a cast that does not depend on the aim: the caster, the spell, its price and
    /// the wards.
    fn check_spell(
        &self,
        spell: SpellId,
        mana: u32,
    ) -> Result<(SpellDef, SpellType, i32, i32), BattleError> {
        let err = BattleError::Spell;
        let battle = &self.battle.as_ref().ok_or(BattleError::Over)?.battle;
        let caster = battle.check_caster()?;
        let side = battle.active;
        let player = battle.player_of(side);
        let def = self.content.rules.spells.get(spell).clone();
        if !self.wizard(player).is_some_and(|m| m.knows(spell)) {
            return Err(err(SpellError::NotKnown));
        }
        if !def.contexts.contains(&CastContext::Battle) {
            return Err(err(SpellError::NotForBattle));
        }
        let kind = SpellType::of(spell, &def).ok_or(err(SpellError::Unsupported))?;
        let (skill, pool) = self.battle_spell_price(player, spell, mana).map_err(err)?;
        if skill > caster.affordable() {
            return Err(err(SpellError::TooExpensive));
        }
        if battle.ward_stops(side, Some(def.realm)) {
            return Err(err(SpellError::Warded));
        }
        Ok((def, kind, skill, pool))
    }

    /// The casts the wizard of the side to play may make now, for each spell they could cast:
    /// every aim the spell accepts and the least and most mana that can be put in. Empty when
    /// the wizard has cast this turn or there is no wizard.
    pub fn legal_battle_casts(&self) -> Vec<LegalCast> {
        let Some(battle) = self.battle() else {
            return Vec::new();
        };
        let player = battle.player_of(battle.active);
        let mut out = Vec::new();
        for s in self.battle_spells(player) {
            if !s.castable {
                continue;
            }
            let cost = self.content.rules.spells.get(s.spell).cost;
            let most = self.battle_spell_most_mana(player, s.spell).max(0) as u32;
            let (least_mana, most_mana) = if s.takes_extra_mana {
                (cost, most.max(cost))
            } else {
                (cost, cost)
            };
            let mut candidates = vec![SpellAim::None];
            candidates.extend(battle.units.iter().map(|u| SpellAim::Unit(u.id)));
            for y in 0..super::map::GRID_H {
                for x in 0..super::map::GRID_W {
                    if battle.map.is_playable((x, y)) {
                        candidates.push(SpellAim::Tile((x, y)));
                    }
                }
            }
            // The price and the wards are the same for every aim: asked once.
            let Ok((def, kind, _, _)) = self.check_spell(s.spell, least_mana) else {
                continue;
            };
            let aims: Vec<SpellAim> = candidates
                .into_iter()
                .filter(|&aim| {
                    battle
                        .check_aim(self, battle.active, s.spell, &def, &kind, aim)
                        .is_ok()
                })
                .collect();
            if !aims.is_empty() {
                out.push(LegalCast {
                    spell: s.spell,
                    aims,
                    least_mana,
                    most_mana,
                });
            }
        }
        out
    }

    /// Whether a spell cast by `player` on `side` is stopped before it takes effect: by the magic
    /// node the battle is fought on (not for a caster with Node Mastery), or by the enemy's
    /// Counter Magic. A stopped spell still costs what it cost.
    pub(super) fn battle_spell_stopped(
        &mut self,
        side: Side,
        player: PlayerId,
        realm: Option<crate::rules::RealmId>,
        difficulty: u32,
    ) -> bool {
        let mastery = self
            .wizard(player)
            .is_some_and(|m| m.stat(&self.content.rules, crate::rules::Stat::NodeImmunity, 0) > 0);
        let Some(active) = self.battle.as_mut() else {
            return false;
        };
        if !mastery
            && active
                .battle
                .node_stops(realm.and_then(Realm::of), difficulty)
        {
            return true;
        }
        active.battle.counter_magic_stops(side, difficulty)
    }

    /// The wizard whose turn it is casts a spell.
    pub(super) fn battle_cast(
        &mut self,
        spell: SpellId,
        aim: SpellAim,
        mana: u32,
    ) -> Result<Vec<Event>, BattleError> {
        let (def, kind, skill, pool) = self.check_cast(spell, aim, mana)?;
        let battle = &self.battle.as_ref().ok_or(BattleError::Over)?.battle;
        let side = battle.active;
        let player = battle.player_of(side);

        // Pay: the skill, and the mana from the reserve.
        let paid = self
            .battle
            .as_mut()
            .map_or(HalfInt::default(), |a| a.battle.pay_for_spell(skill));
        self.sync_battle_mana();
        let mut events = vec![BattleEvent::SpellCast {
            side,
            spell,
            skill,
            mana: paid,
        }];

        // The node under the battle, then the enemy's Counter Magic reserve, try to stop it.
        let difficulty = pool as u32 * self.dispel_multiplier(Some(def.realm), Some(player));
        let stopped = self.battle_spell_stopped(side, player, Some(def.realm), difficulty);
        if stopped {
            events.push(BattleEvent::SpellCountered { side, spell });
        } else {
            events.extend(self.apply_battle_spell(spell, &def, kind, side, player, aim, pool));
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

    /// The damage types a curse's target may be immune to (the spell's `save`).
    pub(super) fn save_types(&self, save: &SpellSave) -> Vec<DamageType> {
        save_types(save, self.settings.rules.illusion_immunity_protects)
    }

    /// What a lasting spell with no rules of its own gives a unit of the battle: Heroism raises it
    /// to the level the spell gives (the unit was made without it).
    fn battle_bonuses_by_name(&self, spell: SpellId, target: usize) -> Vec<Modifier> {
        if !self
            .content
            .rules
            .spells
            .get(spell)
            .has_hook(crate::unit::HEROISM)
        {
            return Vec::new();
        }
        let Some(active) = self.battle.as_ref() else {
            return Vec::new();
        };
        active.battle.units[target]
            .source
            .and_then(|i| self.battle_source(i))
            .map(|e| self.units().heroism_modifiers(e))
            .unwrap_or_default()
    }

    /// What takes a spell's bonuses back off a unit that has a spell it gives way to, so that
    /// the two are not cumulative: Iron Skin cast on a unit with Stone Skin leaves +5, not +6.
    /// Only added bonuses are taken back.
    fn superseded_bonuses(&self, spell: SpellId, present: &[SpellId]) -> Vec<Modifier> {
        let spells = &self.content.rules.spells;
        present
            .iter()
            .flat_map(|&other| spells.get(other).bonuses_given_up_for(spell))
            .collect()
    }

    /// Raise Dead: the dead unit returns on a free square of its side's half of the field,
    /// ready to act, with half its figures (rounded up) and none of its enchantments.
    fn raise_dead(&mut self, side: Side, id: usize) -> Vec<BattleEvent> {
        let Some(active) = self.battle.as_ref() else {
            return Vec::new();
        };
        let Some(cell) = active.battle.first_summon_square(side) else {
            return Vec::new();
        };
        let Some(source) = active.battle.unit(id).and_then(|u| u.source) else {
            return Vec::new();
        };
        let Some(entity) = self.battle_source(source) else {
            return Vec::new();
        };
        self.world.remove_one::<crate::unit::Hurt>(entity).ok();
        crate::spell::instance::remove_on(
            &mut self.world,
            crate::spell::instance::SpellTarget::Unit(entity),
            |_, _| true,
        );
        let Some(mut unit) = self.battle_unit_from(entity, side, source) else {
            return Vec::new();
        };
        let Some(active) = self.battle.as_mut() else {
            return Vec::new();
        };
        unit.id = id;
        unit.cell = cell;
        active.battle.put_unit(id, unit);
        let half = active.battle.units[id].figures_start.div_ceil(2).max(1);
        active.battle.units[id].set_figures(half);
        vec![BattleEvent::Raised { unit: id }]
    }

    /// The overland unit the battle unit `id` was copied from.
    fn battle_source_of(&self, id: usize) -> Option<hecs::Entity> {
        let index = self.battle.as_ref()?.battle.units.get(id)?.source?;
        self.battle_source(index)
    }

    /// The overland unit a battle unit was copied from, by its index in the battle's list.
    pub(super) fn battle_source(&self, index: usize) -> Option<hecs::Entity> {
        self.battle.as_ref()?.sources.get(index).copied()
    }

    /// Animate Dead: the dead unit rises as an undead unit of full strength on a free square
    /// of its side's half of the field, ready to act. Overland it is undead for good (its
    /// lasting spells fall away).
    fn animate_dead(&mut self, side: Side, id: usize) -> Vec<BattleEvent> {
        let Some(active) = self.battle.as_ref() else {
            return Vec::new();
        };
        let Some(cell) = active.battle.first_summon_square(side) else {
            return Vec::new();
        };
        let Some(source) = active.battle.unit(id).and_then(|u| u.source) else {
            return Vec::new();
        };
        let Some(entity) = self.battle_source(source) else {
            return Vec::new();
        };
        self.make_undead(entity);
        self.world.remove_one::<crate::unit::Hurt>(entity).ok();
        let Some(mut unit) = self.battle_unit_from(entity, side, source) else {
            return Vec::new();
        };
        let Some(active) = self.battle.as_mut() else {
            return Vec::new();
        };
        unit.id = id;
        unit.cell = cell;
        active.battle.put_unit(id, unit);
        vec![BattleEvent::Raised { unit: id }]
    }

    /// The active enemy units of a side, by id.
    fn enemies_of(&self, side: Side) -> Vec<usize> {
        self.battle().map_or_else(Vec::new, |b| {
            b.active_units(side.other()).map(|u| u.id).collect()
        })
    }

    /// What a damage spell would do to a unit of the running battle (`Battle::preview_spell_damage`).
    pub fn preview_spell_damage(
        &self,
        spell: SpellId,
        target: usize,
        extra_mana: u32,
    ) -> Option<DamageOdds> {
        self.battle()?
            .preview_spell_damage(spell, target, extra_mana)
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_battle_spell(
        &mut self,
        spell: SpellId,
        def: &SpellDef,
        kind: SpellType,
        side: Side,
        player: PlayerId,
        aim: SpellAim,
        pool: i32,
    ) -> Vec<BattleEvent> {
        match (kind, aim) {
            (
                SpellType::Damage {
                    strength,
                    types,
                    traits,
                    all,
                },
                aim,
            ) => {
                let extra = if traits.mana_per_point > 0 {
                    (pool - def.cost as i32) / traits.mana_per_point as i32
                } else {
                    0
                };
                let targets = match (all, aim) {
                    (true, _) => self.enemies_of(side),
                    (false, SpellAim::Unit(t)) => vec![t],
                    _ => return Vec::new(),
                };
                self.battle.as_mut().map_or_else(Vec::new, |a| {
                    a.battle.strike(&targets, strength + extra, types, &traits)
                })
            }
            (
                SpellType::Heal {
                    points,
                    all,
                    except,
                },
                aim,
            ) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                let targets: Vec<usize> = match (all, aim) {
                    (true, _) => battle.active_units(side).map(|u| u.id).collect(),
                    (false, SpellAim::Unit(t)) => vec![t],
                    _ => return Vec::new(),
                };
                let mut events = Vec::new();
                for t in targets {
                    if except.is_some() && battle.units[t].realm == except {
                        continue;
                    }
                    let healed = battle.spell_heal(t, points);
                    events.push(BattleEvent::SpellHealed { unit: t, healed });
                }
                events
            }
            (SpellType::CounterMagic, _) => {
                if let Some(active) = self.battle.as_mut() {
                    active.battle.cast_counter_magic(side, pool as u32);
                }
                vec![BattleEvent::CounterMagicRaised {
                    side,
                    strength: pool as u32,
                }]
            }
            (SpellType::Field(field), _) => {
                if let Some(active) = self.battle.as_mut() {
                    active.battle.raise_field(side, field, spell);
                }
                vec![BattleEvent::FieldRaised { side, spell }]
            }
            (SpellType::CallChaos, _) => self.call_chaos(side, player),
            (SpellType::Vortex, SpellAim::Tile(cell)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                vec![active.battle.make_vortex(side, cell)]
            }
            (SpellType::EarthToMud, SpellAim::Tile(cell)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                active.battle.spread_mud(cell);
                vec![BattleEvent::MudMade { cell }]
            }
            (SpellType::CityWall(wall), _) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                match wall {
                    CityWall::Stone => {
                        battle.map.raise_walls();
                    }
                    CityWall::Fire => battle.wall_of_fire = true,
                    CityWall::Darkness => battle.wall_of_darkness = true,
                }
                vec![BattleEvent::WallRaised { side, spell }]
            }
            (SpellType::Disrupt, SpellAim::Tile(cell)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                active.battle.map.break_wall(cell);
                vec![BattleEvent::WallDestroyed { cell }]
            }
            (SpellType::CracksCall, SpellAim::Tile(cell)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                let mut events = vec![BattleEvent::FissureOpened { cell }];
                // A walker or crawler over the fissure falls in one time in four, whatever its
                // defence; fliers, the bodiless and merging units (which pass through the ground)
                // stand clear.
                let over = battle
                    .unit_at(cell)
                    .filter(|u| {
                        !u.flag(UnitFlag::Flies)
                            && !battle.spares(u, spell)
                            && !u.flag(UnitFlag::NonCorporeal)
                            && !u.has(Ability::new(43 /* merging */))
                    })
                    .map(|u| u.id);
                if let Some(id) = over {
                    if battle.roll(4) == 1 {
                        let dmg = battle.destroy_unit(id);
                        events.extend(battle.spell_events_for_damage(
                            id,
                            Some(dmg),
                            DamageTypes::NONE,
                        ));
                    }
                }
                if battle.map.has_wall(cell) {
                    battle.map.break_wall(cell);
                    events.push(BattleEvent::WallDestroyed { cell });
                }
                events
            }
            (SpellType::LifeDrain, SpellAim::Unit(target)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                let extra = ((pool - def.cost as i32) / 5).max(0);
                let dmg = battle.drain_life(target, -extra, spell);
                let mut events = battle.spell_events_for_damage(
                    target,
                    dmg,
                    DamageTypes::of(&[DamageType::Death]),
                );
                if let Some(d) = dmg.filter(|d| d.points > 0) {
                    events.push(BattleEvent::LifeDrained {
                        unit: target,
                        points: d.points,
                    });
                    match battle.spell_caster {
                        // A hero or creature that cast it takes the life as hit points.
                        Some(c) => {
                            battle.units[c].heal_points(d.points, true);
                        }
                        // A wizard turns it into mana.
                        None => {
                            if let Some(caster) = battle.casters[side.index()].as_mut() {
                                caster.mana += d.points * 3;
                            }
                        }
                    }
                }
                self.sync_battle_mana();
                events
            }
            (SpellType::Recall { .. }, SpellAim::Unit(target)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                active.battle.units[target].status = super::Status::Recalled;
                vec![BattleEvent::HeroRecalled { unit: target }]
            }
            (SpellType::WarpWood, SpellAim::Unit(target)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                active.battle.units[target].set_ammo(0);
                vec![BattleEvent::SpellEnchanted {
                    unit: target,
                    spell,
                }]
            }
            (SpellType::Turn { bind, save }, SpellAim::Unit(target)) => {
                let immune_types = save
                    .as_ref()
                    .map(|s| self.save_types(s))
                    .unwrap_or_default();
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                if let Some(save) = save {
                    let unit = &battle.units[target];
                    if unit.is_immune_to(&immune_types, &battle.content.rules) {
                        return vec![BattleEvent::SpellImmune {
                            unit: target,
                            by: unit.immune_type(&immune_types, &battle.content.rules),
                        }];
                    }
                    if !battle.resist_fails(target, Realm::of(def.realm), save.modifier) {
                        return vec![BattleEvent::SpellResisted { unit: target }];
                    }
                }
                if bind {
                    battle.enchant_unit(target, spell, player, &[], &[]);
                }
                battle.change_sides(target, side);
                vec![BattleEvent::SpellEnchanted {
                    unit: target,
                    spell,
                }]
            }
            (SpellType::Channel, SpellAim::Unit(target)) => {
                if self.channel_battle_unit(target) {
                    vec![BattleEvent::SpellEnchanted {
                        unit: target,
                        spell,
                    }]
                } else {
                    Vec::new()
                }
            }
            (SpellType::Warp { save }, SpellAim::Unit(target)) => {
                self.warp_creature(spell, def, target, player, save.as_ref())
            }
            (SpellType::Animate, SpellAim::Unit(target)) => self.animate_dead(side, target),
            (SpellType::Raise, SpellAim::Unit(target)) => self.raise_dead(side, target),
            (SpellType::Enchant { mods, grants, save }, SpellAim::Unit(target)) => {
                let present = self
                    .battle
                    .as_ref()
                    .map(|a| a.battle.units[target].enchantments.clone())
                    .unwrap_or_default();
                let mut mods = mods;
                mods.extend(self.superseded_bonuses(spell, &present));
                mods.extend(self.battle_bonuses_by_name(spell, target));
                let immune_types = save
                    .as_ref()
                    .map(|s| self.save_types(s))
                    .unwrap_or_default();
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                if let Some(save) = save {
                    let unit = &battle.units[target];
                    if unit.is_immune_to(&immune_types, &battle.content.rules) {
                        return vec![BattleEvent::SpellImmune {
                            unit: target,
                            by: unit.immune_type(&immune_types, &battle.content.rules),
                        }];
                    }
                    let realm = Realm::of(def.realm);
                    if !battle.resist_fails(target, realm, save.modifier) {
                        return vec![BattleEvent::SpellResisted { unit: target }];
                    }
                }
                battle.enchant_unit(target, spell, player, &mods, &grants);
                vec![BattleEvent::SpellEnchanted {
                    unit: target,
                    spell,
                }]
            }
            (
                SpellType::Slay {
                    realm,
                    modifier,
                    types,
                    only,
                    below,
                    mana_per_step,
                    all,
                },
                aim,
            ) => {
                let modifier = if mana_per_step > 0 {
                    modifier - (pool - def.cost as i32) / mana_per_step as i32
                } else {
                    modifier
                };
                let targets = match (all, aim) {
                    (true, _) => self.enemies_of(side),
                    (false, SpellAim::Unit(t)) => vec![t],
                    _ => return Vec::new(),
                };
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                // Word of Death has `death_immunity_exception`: with the rule on, nothing stops it.
                let immune_types =
                    if def.death_immunity_exception && battle.death_immunity_exceptions {
                        Vec::new()
                    } else {
                        types
                    };
                let mut events = Vec::new();
                for t in targets {
                    // Spell Lock keeps a unit from the spells that sever or burn it.
                    let locked = battle.units[t]
                        .effects(&self.content.rules)
                        .contains(UnitEffect::SpellLock);
                    if !slay_allows(only, &battle.units[t]) || locked {
                        continue;
                    }
                    let modifier = if battle.units[t].undead {
                        modifier - undead_penalty(spell)
                    } else {
                        modifier
                    };
                    match battle.spell_slay(t, realm, modifier, &immune_types, below) {
                        None => events.push(BattleEvent::SpellImmune {
                            unit: t,
                            by: battle.units[t].immune_type(&immune_types, &battle.content.rules),
                        }),
                        Some(d) => {
                            events.push(BattleEvent::SpellSlew {
                                unit: t,
                                kills: d.kills,
                            });
                            if !battle.units[t].is_active() {
                                events.push(BattleEvent::UnitDied { unit: t });
                            }
                        }
                    }
                }
                events
            }
            (SpellType::Summon(unit_id), SpellAim::Tile(cell)) => {
                // The creature exists only in this battle: it is made from its definition.
                let unit = super::BattleUnit::from_def(&self.content.rules, unit_id, side);
                let id = self
                    .battle
                    .as_mut()
                    .map_or(0, |a| a.battle.add_summoned(side, unit, cell));
                vec![BattleEvent::Summoned { unit: id }]
            }
            (SpellType::Dispel(per_mana), SpellAim::Unit(target)) => {
                let strength = pool as u32 * per_mana * self.dispel_power(player);
                let mut events = self.dispel_battle_unit(target, strength, player);
                events.extend(self.dispel_battle_fields(strength, player));
                events
            }
            _ => Vec::new(),
        }
    }

    /// Warp Creature on a unit: after its resistance check (`save`), one of three lasting harms.
    fn warp_creature(
        &mut self,
        spell: SpellId,
        def: &SpellDef,
        target: usize,
        player: PlayerId,
        save: Option<&SpellSave>,
    ) -> Vec<BattleEvent> {
        let immune_types = save.map(|s| self.save_types(s)).unwrap_or_default();
        let Some(active) = self.battle.as_mut() else {
            return Vec::new();
        };
        let battle = &mut active.battle;
        if let Some(save) = save {
            let unit = &battle.units[target];
            if unit.is_immune_to(&immune_types, &battle.content.rules) {
                return vec![BattleEvent::SpellImmune {
                    unit: target,
                    by: unit.immune_type(&immune_types, &battle.content.rules),
                }];
            }
            if !battle.resist_fails(target, Realm::of(def.realm), save.modifier) {
                return vec![BattleEvent::SpellResisted { unit: target }];
            }
        }
        let kind = match battle.roll(3) {
            1 => WarpKind::Attack,
            2 => WarpKind::Defense,
            _ => WarpKind::Resistance,
        };
        battle.enchant_unit(target, spell, player, &warp_mods(kind, spell), &[]);
        battle.units[target].warp = Some(kind);
        vec![BattleEvent::SpellEnchanted {
            unit: target,
            spell,
        }]
    }

    /// Call Chaos: every enemy unit gets one of eight effects at random: nothing, five hit
    /// points of healing (not the dead), Chaos Channels, Warp Creature, a Fire Bolt of strength
    /// 15, Warp Lightning, a Doom Bolt, or Disintegrate. Chaos Channels and Warp Creature work
    /// as the spells do, on a normal unit that has not got the change already.
    fn call_chaos(&mut self, side: Side, player: PlayerId) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        for t in self.enemies_of(side) {
            let Some(active) = self.battle.as_mut() else {
                break;
            };
            let roll = active.battle.roll(8);
            events.push(BattleEvent::ChaosCalled {
                unit: t,
                outcome: (roll - 1) as u8,
            });
            if roll == 3 {
                // Chaos Channels, on a normal unit that is not mutated or undead.
                let channelable = self
                    .battle_source_of(t)
                    .is_some_and(|e| self.check_mutate(e).is_ok());
                if channelable && self.channel_battle_unit(t) {
                    events.push(BattleEvent::SpellEnchanted {
                        unit: t,
                        spell: UnitEffect::ChaosChannels
                            .spell_in(&self.content.rules)
                            .unwrap_or(SpellId::new(0)),
                    });
                }
                continue;
            }
            match roll {
                4 => {
                    let u = &active.battle.units[t];
                    let warped = u.warp.is_some();
                    if let Some(spell) = self.content.rules.spells.id_numbered(89)
                    /* warp_creature */
                    {
                        if !u.is_fantastic() && !warped {
                            let def = self.content.rules.spells.get(spell).clone();
                            events.extend(self.warp_creature(
                                spell,
                                &def,
                                t,
                                player,
                                def.save.as_ref(),
                            ));
                        }
                    }
                }
                2 => {
                    let b = &mut active.battle;
                    if b.units[t].realm != Some(Realm::Death) {
                        events.push(BattleEvent::SpellHealed {
                            unit: t,
                            healed: b.spell_heal(t, 5),
                        });
                    }
                }
                5..=7 => {
                    let (id, strength) = match roll {
                        5 => (SpellId::new(83) /* fire_bolt */, Some(15)),
                        6 => (SpellId::new(101) /* warp_lightning */, None),
                        _ => (SpellId::new(104) /* doom_bolt */, None),
                    };
                    if let Some(active) = self.battle.as_mut() {
                        let b = &mut active.battle;
                        if let Some((base, types, traits)) = b.damage_of(id) {
                            events.extend(b.strike(&[t], strength.unwrap_or(base), types, &traits));
                        }
                    }
                }
                8 => {
                    let b = &mut active.battle;
                    if let Some(d) = b.spell_slay(t, Realm::Chaos, 0, &[], Some(10)) {
                        events.push(BattleEvent::SpellSlew {
                            unit: t,
                            kills: d.kills,
                        });
                        if !b.units[t].is_active() {
                            events.push(BattleEvent::UnitDied { unit: t });
                        }
                    }
                }
                _ => {}
            }
        }
        events
    }

    /// The events of a cast in a battle, and after each dispel of lasting spells or of a
    /// battlefield spell the world's `EnchantmentDispelled` for the wizard who lost it.
    pub(super) fn battle_event_list(&self, events: Vec<BattleEvent>) -> Vec<Event> {
        let mut out = Vec::new();
        for event in events {
            let notices: Vec<Event> = match &event {
                BattleEvent::SpellDispelled { spells, .. } => spells
                    .iter()
                    .map(|&(spell, victim)| Event::EnchantmentDispelled {
                        victim,
                        spell,
                        site: DispelSite::BattleUnit,
                    })
                    .collect(),
                BattleEvent::FieldDispelled { side, spell } => {
                    let victim = self.battle.as_ref().map(|a| a.battle.player_of(*side));
                    victim
                        .map(|victim| Event::EnchantmentDispelled {
                            victim,
                            spell: *spell,
                            site: DispelSite::Field,
                        })
                        .into_iter()
                        .collect()
                }
                _ => Vec::new(),
            };
            out.push(Event::Battle(event));
            out.extend(notices);
        }
        out
    }

    /// A dispel of `strength` by `by` on the battlefield enchantments the other side has up
    /// (`Combat_Cast_Dispel`): each is tried at its spell's difficulty, with the retorts of the
    /// wizard that cast it, and a success takes it down.
    fn dispel_battle_fields(&mut self, strength: u32, by: PlayerId) -> Vec<BattleEvent> {
        let Some(active) = self.battle.as_ref() else {
            return Vec::new();
        };
        let Some(mine) = active.battle.side_of(by) else {
            return Vec::new();
        };
        let theirs = mine.other();
        let victim = active.battle.player_of(theirs);
        let up: Vec<(Field, SpellId)> = active.battle.fields[theirs.index()].clone();
        let mut rng = Pcg64Mcg::seed_from_u64(self.next_u64());
        let mut events = Vec::new();
        for (field, spell) in up {
            let difficulty = self.dispel_difficulty(spell, Some(victim));
            if !crate::spell::dispel::dispel_roll(strength, difficulty, &mut rng) {
                continue;
            }
            if let Some(active) = self.battle.as_mut() {
                if let Some(spell) = active.battle.take_down_field(theirs, field) {
                    events.push(BattleEvent::FieldDispelled {
                        side: theirs,
                        spell,
                    });
                }
            }
        }
        // The defender's Wall of Fire and Wall of Darkness, cast in the battle or on the city
        // before it, are battlefield spells too, as hard to dispel as the spells themselves (1.50
        // note 742).
        if theirs == Side::Defender {
            for (number, wall) in [
                (87 /* wall_of_fire */, true),
                (178 /* wall_of_darkness */, false),
            ] {
                let up = self.battle.as_ref().is_some_and(|a| {
                    if wall {
                        a.battle.wall_of_fire
                    } else {
                        a.battle.wall_of_darkness
                    }
                });
                let Some(spell) = self.content.rules.spells.id_numbered(number) else {
                    continue;
                };
                if !up {
                    continue;
                }
                let difficulty = self.dispel_difficulty(spell, Some(victim));
                if !crate::spell::dispel::dispel_roll(strength, difficulty, &mut rng) {
                    continue;
                }
                if let Some(active) = self.battle.as_mut() {
                    if wall {
                        active.battle.wall_of_fire = false;
                    } else {
                        active.battle.wall_of_darkness = false;
                    }
                    events.push(BattleEvent::FieldDispelled {
                        side: theirs,
                        spell,
                    });
                }
            }
        }
        events
    }

    /// A dispel of `strength` by `by` on a unit of the battle: the spells cast on it in the
    /// battle, and the lasting spells on the overland unit it was copied from, if another
    /// wizard cast them.
    pub(super) fn dispel_battle_unit(
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

        let (kept, gone) = self.dispel_list(unit.combat_spells(), strength, by, true, &mut rng);
        let mut removed = gone.len() as u32;
        let mut overland_changed = false;
        let mut overland_gone = Vec::new();
        if let Some(e) = entity {
            let list = self.spells().unit_enchantments(e);
            let (_, gone) = self.dispel_list(list, strength, by, true, &mut rng);
            if !gone.is_empty() {
                for &(spell, victim) in &gone {
                    self.remove_unit_enchantment(e, spell, victim);
                }
                overland_changed = true;
                removed += gone.len() as u32;
                overland_gone = gone;
            }
        }
        let mut spells: Vec<(SpellId, PlayerId)> = gone.clone();
        spells.extend(overland_gone);
        let event = BattleEvent::SpellDispelled {
            unit: target,
            removed,
            spells,
        };
        if removed == 0 {
            return vec![event];
        }
        // Rebuild the unit: from the overland unit as it is now, or from its saved numbers.
        let base = match entity {
            Some(e) if overland_changed => self
                .battle_unit_from(e, unit.side, unit.source.unwrap_or(0))
                .map(|made| made.sheet().clone()),
            _ => Some(unit.sheet().clone()),
        };
        let Some(base) = base else {
            return vec![event];
        };
        let lasting = self.lasting_spells(&unit, &kept, &base, target);
        if let Some(active) = self.battle.as_mut() {
            let warped = kept
                .iter()
                .any(|&(s, _)| UnitEffect::WarpCreature.is_in(self.content.rules.spells.get(s)));
            let bound = lasting
                .iter()
                .any(|l| UnitEffect::CreatureBinding.is_in(self.content.rules.spells.get(l.spell)));
            active.battle.rebuild_unit(target, base, lasting);
            active.battle.units[target].warp = unit.warp.filter(|_| warped);
            if unit
                .effects(&self.content.rules)
                .contains(UnitEffect::CreatureBinding)
                && !bound
            {
                active.battle.return_unit(target);
            }
            active.battle.refresh_fields();
        }
        let mut events = vec![event];
        // Taking off hit points can leave a unit with none.
        if self
            .battle
            .as_ref()
            .is_some_and(|a| !a.battle.units[target].is_active())
        {
            events.push(BattleEvent::UnitDied { unit: target });
        }
        events
    }

    /// The lasting spells a battle unit keeps on top of `base`, for `rebuild_unit`, from the spells
    /// `kept` on it, with what each does to the numbers. `unit` is the unit as it was before.
    fn lasting_spells(
        &self,
        unit: &super::BattleUnit,
        kept: &[(SpellId, PlayerId)],
        base: &super::Sheet,
        target: usize,
    ) -> Vec<super::Lasting> {
        let mut lasting: Vec<super::Lasting> = kept
            .iter()
            .filter_map(|&(spell, caster)| {
                let def = self.content.rules.spells.get(spell);
                let (mods, grants) = match SpellType::of(spell, def)? {
                    SpellType::Enchant { mods, grants, .. } => (mods, grants),
                    SpellType::Turn { bind: true, .. } => (Vec::new(), Vec::new()),
                    // The warp it got is kept among the unit's enchantment ids.
                    SpellType::Warp { .. } => (warp_mods(unit.warp?, spell), Vec::new()),
                    _ => return None,
                };
                Some(super::Lasting {
                    spell,
                    caster,
                    mods,
                    grants,
                })
            })
            .collect();
        // A spell that gives way to another on the unit stays out of the numbers, as in the cast.
        let mut present = base.enchantments.clone();
        present.extend(lasting.iter().map(|l| l.spell));
        for l in &mut lasting {
            let extra = self.superseded_bonuses(l.spell, &present);
            l.mods.extend(extra);
            let named = self.battle_bonuses_by_name(l.spell, target);
            l.mods.extend(named);
        }
        lasting
    }

    /// Chaos Channels on a unit of the battle: its overland unit is mutated for good, and the
    /// battle unit is rebuilt from it with the spells it keeps. Whether it was mutated.
    pub(super) fn channel_battle_unit(&mut self, target: usize) -> bool {
        let Some(active) = self.battle.as_ref() else {
            return false;
        };
        let unit = active.battle.units[target].clone();
        let Some(index) = unit.source else {
            return false;
        };
        let Some(entity) = active.sources.get(index).copied() else {
            return false;
        };
        let mut rng = std::mem::replace(&mut self.rng, crate::game::rng::GameRng::lent());
        let done = self.mutate(entity, &mut rng).is_some();
        self.rng = rng;
        if !done {
            return false;
        }
        let Some(made) = self.battle_unit_from(entity, unit.side, index) else {
            return false;
        };
        let base = made.sheet().clone();
        let lasting = self.lasting_spells(&unit, &unit.combat_spells(), &base, target);
        let realm = made.realm;
        if let Some(active) = self.battle.as_mut() {
            active.battle.rebuild_unit(target, base, lasting);
            active.battle.units[target].realm = realm;
            active.battle.refresh_fields();
        }
        true
    }
}

impl AimWorld for Game {
    fn source_kind(&self, unit: &super::BattleUnit) -> Option<(crate::rules::UnitKind, bool)> {
        let entity = unit.source.and_then(|i| self.battle_source(i))?;
        let units = self.units();
        let kind = units.unit_def(entity)?.kind;
        Some((kind, units.unit_counts_as_undead(entity)))
    }

    fn mutation_refusal(&self, unit: &super::BattleUnit) -> Option<TargetError> {
        match unit.source.and_then(|i| self.battle_source(i)) {
            Some(entity) => self.check_mutate(entity).err(),
            None => Some(TargetError::NotNormalUnit),
        }
    }

    fn summon_point(&self, player: PlayerId) -> Option<Position> {
        Game::summon_point(self, player)
    }

    fn caster_has_global(&self, player: PlayerId, spell: SpellId) -> bool {
        self.has_global(player, spell)
    }

    fn confirmed(&self) -> bool {
        self.confirmed
    }

    fn illusion_immunity_protects(&self) -> bool {
        self.settings.rules.illusion_immunity_protects
    }
}

#[cfg(test)]
mod tests {
    use super::undead_penalty;
    use crate::rules::SpellId;

    #[test]
    fn only_dispel_evil_and_holy_word_are_harder_on_the_undead() {
        assert_eq!(undead_penalty(SpellId::new(134) /* dispel_evil */), 5);
        assert_eq!(undead_penalty(SpellId::new(150) /* holy_word */), 5);
        assert_eq!(undead_penalty(SpellId::new(192) /* word_of_death */), 0);
    }
}
