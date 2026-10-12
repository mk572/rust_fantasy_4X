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
//! Not yet: dispelling battlefield enchantments, the spells whose effects are not in the game
//! (task 159), the node counter and the heroes' own casting.

use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

use super::casting::{BattleCaster, SpellAim, SpellError, MAX_SIDE_UNITS};
use super::field::{Field, BATTLEFIELD, CALL_CHAOS};
use super::immunity::{AttackNature, Immunities, Realm};
use super::{Battle, BattleError, BattleEvent, Side};
use crate::city::{PlayerId, Position};
use crate::rules::defs::{CastContext, Duration, SpellDef, SpellSave, SpellTarget};
use crate::rules::effects::{Condition, DamageTraits, Effect, SlayOnly, TargetSel, Trigger};
use crate::rules::stats::{Modifier, Source};
use crate::rules::SpellId;
use crate::units::UnitEnchantments;
use crate::{Event, Game};

/// The hook of the Animate Dead spell.
pub const ANIMATE_DEAD: &str = "animate_dead";

/// What a spell does in a battle, from its effects.
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Damage {
        strength: i32,
        immune: Vec<String>,
        realm: Option<Realm>,
        traits: DamageTraits,
        /// Every enemy unit, not one chosen.
        all: bool,
    },
    Heal {
        points: i32,
        /// Every unit of the caster's, not one chosen.
        all: bool,
        /// Creatures of this realm are not healed (Mass Healing does not help the dead).
        except: Option<Realm>,
    },
    Summon(String),
    /// A lasting spell on one unit; a curse has a save.
    Enchant {
        mods: Vec<Modifier>,
        grants: Vec<String>,
        save: Option<SpellSave>,
    },
    /// A dispel of this strength for each mana put in.
    Dispel(u32),
    CounterMagic,
    /// Figures that fail a resistance check are destroyed.
    Slay {
        realm: Realm,
        modifier: i32,
        immune: Vec<String>,
        only: SlayOnly,
        below: Option<i32>,
        mana_per_step: u32,
        all: bool,
    },
    /// A battlefield enchantment.
    Field(Field),
    CallChaos,
    /// Raise one of the wizard's own dead units as undead (Animate Dead).
    Animate,
}

impl Kind {
    /// Whether extra mana makes the spell stronger.
    fn takes_extra_mana(&self) -> bool {
        match self {
            Kind::Damage { traits, .. } => traits.mana_per_point > 0,
            Kind::Slay { mana_per_step, .. } => *mana_per_step > 0,
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

/// A spell the wizard may cast now, with where it may be aimed and how much mana may go in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalCast {
    pub spell: SpellId,
    /// Every aim the spell accepts: `None` for a spell with no target, a unit, or a square.
    pub aims: Vec<SpellAim>,
    /// The mana the spell takes at least (its cost) and at most (what the wizard can afford).
    /// They are equal for a spell that takes no extra mana. Any amount between is legal; `0`
    /// asks for the least.
    pub least_mana: u32,
    pub most_mana: u32,
}

/// Creatures a spell may only be thrown at, by realm: Star Fires burns chaos and death
/// creatures and nothing else.
fn only_against(spell_id: &str) -> Option<&'static [Realm]> {
    match spell_id {
        "star_fires" => Some(&[Realm::Chaos, Realm::Death]),
        _ => None,
    }
}

/// How much harder the undead find it to resist a spell, on top of its own modifier: Dispel
/// Evil and Holy Word (the help's "much more difficult time"; the original's Holy Word code
/// has it the wrong way round).
fn undead_penalty(spell_id: &str) -> i32 {
    match spell_id {
        "dispel_evil" | "holy_word" => 5,
        _ => 0,
    }
}

fn slay_allows(only: SlayOnly, unit: &super::BattleUnit) -> bool {
    match only {
        SlayOnly::Any => true,
        SlayOnly::Fantastic => unit.is_fantastic(),
        SlayOnly::ChaosOrDeath => matches!(unit.realm, Some(Realm::Chaos | Realm::Death)),
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
            return Some(Kind::Enchant {
                mods,
                grants,
                save: def.save.clone(),
            });
        }
        let [rule] = def.effects.as_slice() else {
            return None;
        };
        if rule.when != Trigger::OnCast {
            return None;
        }
        let all_enemies = rule.target == TargetSel::AllEnemyUnits;
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
                all: all_enemies,
            }),
            Effect::Heal(n) => Some(Kind::Heal {
                points: *n,
                all: rule.target == TargetSel::AllOwnUnits,
                except: match &rule.condition {
                    Condition::Not(c) => match &**c {
                        Condition::RealmIs(r) => Realm::from_id(r),
                        _ => None,
                    },
                    _ => None,
                },
            }),
            Effect::Summon { unit, .. } => Some(Kind::Summon(unit.clone())),
            Effect::Dispel { strength } => Some(Kind::Dispel(*strength / def.cost.max(1))),
            Effect::Slay {
                realm,
                modifier,
                immune,
                only,
                below,
                mana_per_step,
            } => Some(Kind::Slay {
                realm: Realm::from_id(realm)?,
                modifier: *modifier,
                immune: immune.clone(),
                only: *only,
                below: *below,
                mana_per_step: *mana_per_step,
                all: all_enemies,
            }),
            Effect::Custom(h) if h == crate::dispel::COUNTER_MAGIC => Some(Kind::CounterMagic),
            Effect::Custom(h) if h == BATTLEFIELD => Field::of(&def.id).map(Kind::Field),
            Effect::Custom(h) if h == CALL_CHAOS => Some(Kind::CallChaos),
            Effect::Custom(h) if h == ANIMATE_DEAD => Some(Kind::Animate),
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
                    castable: caster.is_some_and(|c| !c.cast_this_turn && cost <= c.affordable())
                        && !self.battle().is_some_and(|b| {
                            b.side_of(player)
                                .is_some_and(|side| b.ward_stops(side, &def.realm))
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

    /// Each wizard's reserve in the game is what the battle says it is: spells and Mana Leak
    /// draw on it.
    pub(super) fn sync_battle_mana(&mut self) {
        let Some(active) = self.battle.as_ref() else {
            return;
        };
        let reserves: Vec<(PlayerId, i32)> = [Side::Attacker, Side::Defender]
            .into_iter()
            .filter_map(|s| Some((active.battle.player_of(s), active.battle.caster(s)?.mana)))
            .collect();
        for (player, mana) in reserves {
            if let Some(p) = self.player_mut(player) {
                p.mana = mana;
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
    ) -> Result<(SpellDef, Kind, i32, i32), BattleError> {
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
        if battle.ward_stops(side, &def.realm) {
            return Err(err(SpellError::Warded));
        }
        self.check_aim(battle, side, &def, &kind, aim)
            .map_err(err)?;
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
                    candidates.push(SpellAim::Tile((x, y)));
                }
            }
            let aims: Vec<SpellAim> = candidates
                .into_iter()
                .filter(|&aim| self.check_cast(s.spell, aim, least_mana).is_ok())
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
            .map_or(0, |a| a.battle.pay_for_spell(skill));
        self.sync_battle_mana();
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
        if let Some(active) = self.battle.as_mut() {
            active.battle.refresh_fields();
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
            (SpellTarget::None | SpellTarget::Battlefield, SpellAim::None) => {
                if let Kind::Field(f) = kind {
                    if battle.has_field(side, *f) {
                        return Err(SpellError::AlreadyUp);
                    }
                }
                Ok(())
            }
            (SpellTarget::Tile, SpellAim::Tile(cell)) => {
                if battle.active_units(side).count() >= MAX_SIDE_UNITS
                    || !battle.is_summon_square(side, cell)
                {
                    return Err(SpellError::NoRoom);
                }
                Ok(())
            }
            (SpellTarget::AnyUnit, SpellAim::Unit(id)) if matches!(kind, Kind::Animate) => {
                if battle.active_units(side).count() >= MAX_SIDE_UNITS
                    || battle.first_summon_square(side).is_none()
                {
                    return Err(SpellError::NoRoom);
                }
                if self.can_animate(battle, side, id) {
                    Ok(())
                } else {
                    Err(SpellError::BadTarget)
                }
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
                // Death creatures, born so or undead, cannot be healed.
                if matches!(kind, Kind::Heal { all: false, .. }) && u.realm == Some(Realm::Death) {
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
                if let Kind::Slay {
                    only, all: false, ..
                } = kind
                {
                    if !slay_allows(*only, u) {
                        return Err(SpellError::BadTarget);
                    }
                }
                Ok(())
            }
            _ => Err(SpellError::BadTarget),
        }
    }

    /// Whether `id` is a dead unit of `side` that Animate Dead can raise: a unit of the wizard's
    /// army (not one summoned in the battle, and not a hero) that was not destroyed outright.
    fn can_animate(&self, battle: &Battle, side: Side, id: usize) -> bool {
        use crate::rules::UnitKind;
        let Some(u) = battle.unit(id) else {
            return false;
        };
        let Some(entity) = u.source.and_then(|i| self.battle_source(i)) else {
            return false;
        };
        u.side == side
            && u.status == super::Status::Dead
            && u.fate() != super::Fate::Gone
            && self
                .unit_def(entity)
                .is_some_and(|d| d.kind != UnitKind::Hero)
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
        self.world.remove_one::<crate::units::Hurt>(entity).ok();
        let Some(mut unit) = self.battle_unit_from(entity, side, source) else {
            return Vec::new();
        };
        let Some(active) = self.battle.as_mut() else {
            return Vec::new();
        };
        unit.id = id;
        unit.cell = cell;
        active.battle.units[id] = unit;
        vec![BattleEvent::Raised { unit: id }]
    }

    /// The active enemy units of a side, by id.
    fn enemies_of(&self, side: Side) -> Vec<usize> {
        self.battle().map_or_else(Vec::new, |b| {
            b.active_units(side.other()).map(|u| u.id).collect()
        })
    }

    /// The damage a spell does, by its id: strength, what stops it, its realm and traits.
    fn damage_of(&self, id: &str) -> Option<(i32, Vec<String>, Option<Realm>, DamageTraits)> {
        let spell = self.content.rules.spells.id_of(id)?;
        match self.battle_kind(spell, self.content.rules.spells.get(spell))? {
            Kind::Damage {
                strength,
                immune,
                realm,
                traits,
                ..
            } => Some((strength, immune, realm, traits)),
            _ => None,
        }
    }

    /// Hit units with a damage spell; each gets its own events.
    fn strike(
        &mut self,
        targets: &[usize],
        strength: i32,
        immune: &[String],
        realm: Option<Realm>,
        traits: &DamageTraits,
    ) -> Vec<BattleEvent> {
        let nature = AttackNature {
            stopped_by: Immunities::from_names(immune.iter().map(String::as_str)),
            realm,
            armor_piercing: false,
        };
        let Some(active) = self.battle.as_mut() else {
            return Vec::new();
        };
        let battle = &mut active.battle;
        let mut events = Vec::new();
        for &t in targets {
            let dmg = battle.spell_damage(t, strength, traits, nature);
            events.extend(battle.spell_events_for_damage(t, dmg));
        }
        events
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
                self.strike(&targets, strength + extra, &immune, realm, &traits)
            }
            (
                Kind::Heal {
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
            (Kind::CounterMagic, _) => {
                if let Some(active) = self.battle.as_mut() {
                    active.battle.cast_counter_magic(side, pool as u32);
                }
                vec![BattleEvent::CounterMagicRaised {
                    side,
                    strength: pool as u32,
                }]
            }
            (Kind::Field(field), _) => {
                if let Some(active) = self.battle.as_mut() {
                    active.battle.raise_field(side, field, spell);
                }
                vec![BattleEvent::FieldRaised { side, spell }]
            }
            (Kind::CallChaos, _) => self.call_chaos(side),
            (Kind::Animate, SpellAim::Unit(target)) => self.animate_dead(side, target),
            (Kind::Enchant { mods, grants, save }, SpellAim::Unit(target)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                if let Some(save) = save {
                    let unit = &battle.units[target];
                    if save.immune.iter().any(|i| unit.has(i)) {
                        return vec![BattleEvent::SpellImmune { unit: target }];
                    }
                    let realm = Realm::from_id(&def.realm);
                    if !battle.resist_fails(target, realm, save.modifier) {
                        return vec![BattleEvent::SpellResisted { unit: target }];
                    }
                }
                battle.enchant_unit(target, spell, &def.id, player, &mods, &grants);
                vec![BattleEvent::SpellEnchanted {
                    unit: target,
                    spell,
                }]
            }
            (
                Kind::Slay {
                    realm,
                    modifier,
                    immune,
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
                let mut events = Vec::new();
                for t in targets {
                    // Spell Lock keeps a unit from the spells that sever or burn it.
                    let locked = battle.units[t]
                        .enchantments
                        .iter()
                        .any(|e| e == "spell_lock");
                    if !slay_allows(only, &battle.units[t]) || locked {
                        continue;
                    }
                    let modifier = if battle.units[t].undead {
                        modifier - undead_penalty(&def.id)
                    } else {
                        modifier
                    };
                    match battle.spell_slay(t, realm, modifier, &immune, below) {
                        None => events.push(BattleEvent::SpellImmune { unit: t }),
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

    /// Call Chaos: every enemy unit gets one of eight effects at random: nothing, five hit
    /// points of healing (not the dead), Chaos Channels, Warp Creature, a Fire Bolt of strength
    /// 15, Warp Lightning, a Doom Bolt, or Disintegrate. Chaos Channels and Warp Creature are
    /// not in the game yet (task 159) and do nothing.
    fn call_chaos(&mut self, side: Side) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        for t in self.enemies_of(side) {
            let Some(active) = self.battle.as_mut() else {
                break;
            };
            let roll = active.battle.roll(8);
            match roll {
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
                        5 => ("fire_bolt", Some(15)),
                        6 => ("warp_lightning", None),
                        _ => ("doom_bolt", None),
                    };
                    if let Some((base, immune, realm, traits)) = self.damage_of(id) {
                        events.extend(self.strike(
                            &[t],
                            strength.unwrap_or(base),
                            &immune,
                            realm,
                            &traits,
                        ));
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
                    Kind::Enchant { mods, grants, .. } => Some((def.id.clone(), mods, grants)),
                    _ => None,
                }
            })
            .collect();
        if let Some(active) = self.battle.as_mut() {
            active.battle.rebuild_unit(target, base, kept, &lasting);
            active.battle.refresh_fields();
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

    /// A resistance check of one unit against a realm: whether it *fails*.
    pub fn resist_fails(&mut self, target: usize, realm: Option<Realm>, modifier: i32) -> bool {
        let state = self.units[target].defender_state();
        let rng: &mut dyn rand::RngCore = &mut self.rng;
        state.resistance_failure(realm, modifier, rng) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::undead_penalty;

    #[test]
    fn only_dispel_evil_and_holy_word_are_harder_on_the_undead() {
        assert_eq!(undead_penalty("dispel_evil"), 5);
        assert_eq!(undead_penalty("holy_word"), 5);
        assert_eq!(undead_penalty("word_of_death"), 0);
    }
}
