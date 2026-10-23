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
use super::{Battle, BattleError, BattleEvent, RangedKind, Side};
use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::defs::{CastContext, Duration, SpellDef, SpellSave, SpellTarget};
use crate::rules::effects::{Condition, DamageTraits, Effect, SlayOnly, TargetSel, Trigger};
use crate::rules::stats::{ModOp, Modifier, Source, Stat};
use crate::rules::SpellId;

mod unit_cast;
pub use unit_cast::{SpellSource, UnitSpell};

/// The hook of the Animate Dead spell.
pub const ANIMATE_DEAD: &str = "animate_dead";
/// The hook of Creature Binding: a fantastic creature fights for the caster until the battle
/// ends or the spell is dispelled.
pub const CREATURE_BINDING: &str = "creature_binding";
/// The hook of Possession: a normal unit fights for the caster until the battle ends.
pub const POSSESSION: &str = "possession";
/// The hook of Raise Dead: a fallen normal unit returns with half its figures.
pub const RAISE_DEAD: &str = "raise_dead";
pub const RECALL_HERO: &str = "recall_hero";
/// The hook of Word of Recall in a battle: any of the wizard's units goes to the summoning circle.
pub const WORD_OF_RECALL: &str = crate::spell::special::WORD_OF_RECALL;
/// The hook of Earth to Mud: a 5 by 5 area of open ground turns to mud.
pub const EARTH_TO_MUD: &str = "earth_to_mud";
/// The hook of Cracks Call: a fissure on one square.
pub const CRACKS_CALL: &str = "cracks_call";
/// The hook of Disrupt: one section of city wall is destroyed.
pub const DISRUPT: &str = "disrupt";
/// The hook of Life Drain: the target's life goes to the caster.
pub const LIFE_DRAIN: &str = "life_drain";
/// The hook of Warp Wood: a missile unit's ammunition is destroyed.
pub const WARP_WOOD: &str = "warp_wood";
/// The hook of Warp Creature.
pub const WARP_CREATURE: &str = "warp_creature";
/// What Warp Creature did to a unit, as the id kept among its lasting spells: half its attacks,
/// half its defence, or no resistance.
const WARPS: [&str; 3] = ["warp_attack", "warp_defense", "warp_resistance"];

/// The changes of one of Warp Creature's three results.
fn warp_mods(variant: usize, spell: SpellId) -> Vec<Modifier> {
    let source = Source::Spell(spell);
    match variant {
        0 => vec![
            Modifier::new(Stat::Melee, ModOp::Percent, -50, source),
            Modifier::new(Stat::Ranged, ModOp::Percent, -50, source),
        ],
        1 => vec![Modifier::new(Stat::Defense, ModOp::Percent, -50, source)],
        _ => vec![Modifier::new(Stat::Resistance, ModOp::Set, 0, source)],
    }
}

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
    /// Bring a fallen normal unit of the wizard's back with half its figures (Raise Dead).
    Raise,
    /// Destroy a missile unit's ammunition (Warp Wood).
    WarpWood,
    /// Take one of the wizard's units off the field to the summoning circle: only a hero for
    /// Recall Hero, any unit for Word of Recall.
    Recall {
        anyone: bool,
    },
    /// A Magic Vortex on a free square.
    Vortex,
    /// Mud on a 5 by 5 area (Earth to Mud).
    EarthToMud,
    /// A fissure on a square (Cracks Call).
    CracksCall,
    /// A section of the city wall is destroyed (Disrupt).
    Disrupt,
    /// Life from the target to the caster (Life Drain); extra mana makes it harder to resist.
    LifeDrain,
    /// A unit changes sides after a resistance check: for good until a dispel (Creature
    /// Binding, fantastic creatures) or just for the battle (Possession, normal units).
    Turn {
        bind: bool,
        save: Option<SpellSave>,
    },
    /// After a resistance check, one of three lasting harms to a normal unit (Warp Creature).
    Warp {
        save: Option<SpellSave>,
    },
}

impl Kind {
    /// Whether extra mana makes the spell stronger.
    fn takes_extra_mana(&self) -> bool {
        match self {
            Kind::Damage { traits, .. } => traits.mana_per_point > 0,
            Kind::Slay { mana_per_step, .. } => *mana_per_step > 0,
            Kind::Dispel(_) | Kind::CounterMagic | Kind::LifeDrain => true,
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
        if let [r] = def.effects.as_slice() {
            let bind = matches!(&r.effect, Effect::Custom(h) if h == CREATURE_BINDING);
            if bind || matches!(&r.effect, Effect::Custom(h) if h == POSSESSION) {
                return Some(Kind::Turn {
                    bind,
                    save: def.save.clone(),
                });
            }
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
            Effect::Custom(h) if h == crate::spell::dispel::COUNTER_MAGIC => {
                Some(Kind::CounterMagic)
            }
            Effect::Custom(h) if h == BATTLEFIELD => Field::of(&def.id).map(Kind::Field),
            Effect::Custom(h) if h == CALL_CHAOS => Some(Kind::CallChaos),
            Effect::Custom(h) if h == ANIMATE_DEAD => Some(Kind::Animate),
            Effect::Custom(h) if h == RAISE_DEAD => Some(Kind::Raise),
            Effect::Custom(h) if h == WARP_WOOD => Some(Kind::WarpWood),
            Effect::Custom(h) if h == RECALL_HERO => Some(Kind::Recall { anyone: false }),
            Effect::Custom(h) if h == WORD_OF_RECALL => Some(Kind::Recall { anyone: true }),
            Effect::Custom(h) if h == EARTH_TO_MUD => Some(Kind::EarthToMud),
            Effect::Custom(h) if h == CRACKS_CALL => Some(Kind::CracksCall),
            Effect::Custom(h) if h == super::vortex::MAGIC_VORTEX => Some(Kind::Vortex),
            Effect::Custom(h) if h == DISRUPT => Some(Kind::Disrupt),
            Effect::Custom(h) if h == LIFE_DRAIN => Some(Kind::LifeDrain),
            Effect::Custom(h) if h == WARP_CREATURE => Some(Kind::Warp {
                save: def.save.clone(),
            }),
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
        let Some(magic) = self.wizard(player) else {
            return Vec::new();
        };
        let caster = self.battle_caster_of(player);
        magic
            .known()
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
        let cut = self.wizard(player).map_or(0, |m| {
            m.cost_reduction(self.content.rules.spells.get(spell))
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
            if self.is_neutral_player(player) {
                continue;
            }
            let Some(magic) = self.wizard(player) else {
                continue;
            };
            // A wizard on the way home from banishment has nothing to cast with.
            let skill = if self.casting_return(player) {
                0
            } else {
                self.wizard(player).map_or(0, |m| m.casting_skill())
            };
            let distance = magic
                .fortress()
                .filter(|f| f.plane == site.plane)
                .and_then(|f| {
                    let map = self.map.as_ref()?;
                    Some(map.plane(f.plane).chebyshev((f.x, f.y), (site.x, site.y)))
                });
            let mut range = super::casting::range_percent(
                distance,
                magic.has(crate::wizard::magic::Retort::Channeler),
            );
            // On corrupted ground a wizard who keeps Great Wasting going pays no range penalty.
            if self.is_corrupted(site) && self.has_global(player, "great_wasting") {
                range = range.min(10);
            }
            battle.set_caster(
                side,
                BattleCaster::new(skill, self.treasury(player).map_or(0, |t| t.mana), range),
            );
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
    ) -> Result<(SpellDef, Kind, i32, i32), BattleError> {
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
                    if battle.map.is_playable((x, y)) {
                        candidates.push(SpellAim::Tile((x, y)));
                    }
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

    /// Whether a spell cast by `player` on `side` is stopped before it takes effect: by the magic
    /// node the battle is fought on (not for a caster with Node Mastery), or by the enemy's
    /// Counter Magic. A stopped spell still costs what it cost.
    pub(super) fn battle_spell_stopped(
        &mut self,
        side: Side,
        player: PlayerId,
        realm: &str,
        difficulty: u32,
    ) -> bool {
        let mastery = self
            .wizard(player)
            .is_some_and(|m| m.has(crate::wizard::magic::Retort::NodeMastery));
        let Some(active) = self.battle.as_mut() else {
            return false;
        };
        if !mastery && active.battle.node_stops(Realm::from_id(realm), difficulty) {
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
            .map_or(0, |a| a.battle.pay_for_spell(skill));
        self.sync_battle_mana();
        let mut events = vec![BattleEvent::SpellCast {
            side,
            spell,
            skill,
            mana: paid,
        }];

        // The node under the battle, then the enemy's Counter Magic reserve, try to stop it.
        let difficulty = pool as u32 * self.dispel_multiplier(&def.realm, Some(player));
        let stopped = self.battle_spell_stopped(side, player, &def.realm, difficulty);
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
            (SpellTarget::Tile, SpellAim::Tile(cell)) if matches!(kind, Kind::Disrupt) => {
                if battle.map.has_wall(cell) {
                    Ok(())
                } else {
                    Err(SpellError::BadTarget)
                }
            }
            (SpellTarget::Tile, SpellAim::Tile(cell)) if matches!(kind, Kind::Vortex) => {
                if battle.can_make_vortex(cell) {
                    Ok(())
                } else {
                    Err(SpellError::BadTarget)
                }
            }
            (SpellTarget::Tile, SpellAim::Tile(cell))
                if matches!(kind, Kind::EarthToMud | Kind::CracksCall) =>
            {
                if battle.map.is_playable(cell) {
                    Ok(())
                } else {
                    Err(SpellError::BadTarget)
                }
            }
            (SpellTarget::Tile, SpellAim::Tile(cell)) => {
                if battle.active_units(side).count() >= MAX_SIDE_UNITS
                    || !battle.is_summon_square(side, cell)
                {
                    return Err(SpellError::NoRoom);
                }
                Ok(())
            }
            (SpellTarget::AnyUnit, SpellAim::Unit(id)) if matches!(kind, Kind::Raise) => {
                if battle.active_units(side).count() >= MAX_SIDE_UNITS
                    || battle.first_summon_square(side).is_none()
                {
                    return Err(SpellError::NoRoom);
                }
                if self.can_raise(battle, side, id) {
                    Ok(())
                } else {
                    Err(SpellError::BadTarget)
                }
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
                // An invisible enemy that cannot be seen cannot be picked.
                if !battle.seen_by(side, id) {
                    return Err(SpellError::BadTarget);
                }
                // Death creatures, born so or undead, cannot be healed.
                if matches!(kind, Kind::Heal { all: false, .. }) && u.realm == Some(Realm::Death) {
                    return Err(SpellError::BadTarget);
                }
                if let Kind::Turn { bind, .. } = kind {
                    // Binding takes fantastic creatures, Possession normal units, not heroes.
                    let fits = if *bind {
                        u.is_fantastic()
                    } else {
                        !u.is_fantastic() && !self.battle_unit_is_hero(u)
                    };
                    if !fits
                        || u.bound_from.is_some()
                        || battle.active_units(side).count() >= MAX_SIDE_UNITS
                    {
                        return Err(SpellError::BadTarget);
                    }
                }
                // Recall Hero takes one of the wizard's own heroes, and needs a place to send it.
                if let Kind::Recall { anyone } = kind {
                    if !own
                        || (!anyone && !self.battle_unit_is_hero(u))
                        || self.summon_point(battle.player_of(side)).is_none()
                    {
                        return Err(SpellError::BadTarget);
                    }
                }
                if matches!(kind, Kind::LifeDrain)
                    && (u.realm == Some(Realm::Death) || u.has("death_immunity"))
                {
                    return Err(SpellError::BadTarget);
                }
                if matches!(kind, Kind::Enchant { .. }) && u.enchantments.contains(&def.id) {
                    return Err(SpellError::BadTarget);
                }
                // Warp Wood needs a missile unit; it and Warp Creature take normal units only,
                // and a unit is warped once.
                if matches!(kind, Kind::WarpWood)
                    && (u.ranged_kind != RangedKind::Missile || u.ammo <= 0 || u.is_fantastic())
                {
                    return Err(SpellError::BadTarget);
                }
                if matches!(kind, Kind::Warp { .. })
                    && (u.is_fantastic()
                        || u.enchantments.iter().any(|e| WARPS.contains(&e.as_str())))
                {
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

    /// Whether the unit of the battle is a hero, from the overland unit it was copied from.
    fn battle_unit_is_hero(&self, u: &super::BattleUnit) -> bool {
        let Some(active) = self.battle.as_ref() else {
            return false;
        };
        u.source
            .and_then(|i| active.sources.get(i).copied())
            .and_then(|e| self.units().unit_def(e))
            .is_some_and(|d| d.kind == crate::rules::UnitKind::Hero)
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
                .units()
                .unit_def(entity)
                .is_some_and(|d| d.kind != UnitKind::Hero)
    }

    /// Whether `id` is a dead normal unit of `side` that Raise Dead can bring back: from the
    /// wizard's army, not a hero, a fantastic creature or one destroyed outright.
    fn can_raise(&self, battle: &Battle, side: Side, id: usize) -> bool {
        battle
            .unit(id)
            .is_some_and(|u| !u.is_fantastic() && !u.undead)
            && self.can_animate(battle, side, id)
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
        unit.figures = unit.figures_start.div_ceil(2).max(1);
        active.battle.units[id] = unit;
        vec![BattleEvent::Raised { unit: id }]
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
            (Kind::CallChaos, _) => self.call_chaos(side, player),
            (Kind::Vortex, SpellAim::Tile(cell)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                vec![active.battle.make_vortex(side, cell)]
            }
            (Kind::EarthToMud, SpellAim::Tile(cell)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                active.battle.spread_mud(cell);
                vec![BattleEvent::MudMade { cell }]
            }
            (Kind::Disrupt, SpellAim::Tile(cell)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                active.battle.map.break_wall(cell);
                vec![BattleEvent::WallDestroyed { cell }]
            }
            (Kind::CracksCall, SpellAim::Tile(cell)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                let mut events = vec![BattleEvent::FissureOpened { cell }];
                // A walker or crawler over the fissure falls in one time in four, whatever its
                // defence; fliers and the bodiless stand clear.
                let over = battle
                    .unit_at(cell)
                    .filter(|u| !u.flies && !u.has("non_corporeal"))
                    .map(|u| u.id);
                if let Some(id) = over {
                    if battle.roll(4) == 1 {
                        let dmg = battle.destroy_unit(id);
                        events.extend(battle.spell_events_for_damage(id, Some(dmg)));
                    }
                }
                if battle.map.has_wall(cell) {
                    battle.map.break_wall(cell);
                    events.push(BattleEvent::WallDestroyed { cell });
                }
                events
            }
            (Kind::LifeDrain, SpellAim::Unit(target)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                let extra = ((pool - def.cost as i32) / 5).max(0);
                let dmg = battle.drain_life(target, -extra);
                let mut events = battle.spell_events_for_damage(target, dmg);
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
            (Kind::Recall { .. }, SpellAim::Unit(target)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                active.battle.units[target].status = super::Status::Recalled;
                vec![BattleEvent::HeroRecalled { unit: target }]
            }
            (Kind::WarpWood, SpellAim::Unit(target)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                active.battle.units[target].ammo = 0;
                vec![BattleEvent::SpellEnchanted {
                    unit: target,
                    spell,
                }]
            }
            (Kind::Turn { bind, save }, SpellAim::Unit(target)) => {
                let Some(active) = self.battle.as_mut() else {
                    return Vec::new();
                };
                let battle = &mut active.battle;
                if let Some(save) = save {
                    let unit = &battle.units[target];
                    if save.immune.iter().any(|i| unit.has(i)) {
                        return vec![BattleEvent::SpellImmune { unit: target }];
                    }
                    if !battle.resist_fails(target, Realm::from_id(&def.realm), save.modifier) {
                        return vec![BattleEvent::SpellResisted { unit: target }];
                    }
                }
                if bind {
                    battle.enchant_unit(target, spell, &def.id, player, &[], &[]);
                }
                battle.change_sides(target, side);
                vec![BattleEvent::SpellEnchanted {
                    unit: target,
                    spell,
                }]
            }
            (Kind::Warp { save }, SpellAim::Unit(target)) => {
                self.warp_creature(spell, def, target, player, save.as_ref())
            }
            (Kind::Animate, SpellAim::Unit(target)) => self.animate_dead(side, target),
            (Kind::Raise, SpellAim::Unit(target)) => self.raise_dead(side, target),
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
                let strength = pool as u32 * per_mana;
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
        let Some(active) = self.battle.as_mut() else {
            return Vec::new();
        };
        let battle = &mut active.battle;
        if let Some(save) = save {
            let unit = &battle.units[target];
            if save.immune.iter().any(|i| unit.has(i)) {
                return vec![BattleEvent::SpellImmune { unit: target }];
            }
            if !battle.resist_fails(target, Realm::from_id(&def.realm), save.modifier) {
                return vec![BattleEvent::SpellResisted { unit: target }];
            }
        }
        let variant = battle.roll(3) as usize - 1;
        battle.enchant_unit(
            target,
            spell,
            WARPS[variant],
            player,
            &warp_mods(variant, spell),
            &[],
        );
        vec![BattleEvent::SpellEnchanted {
            unit: target,
            spell,
        }]
    }

    /// Call Chaos: every enemy unit gets one of eight effects at random: nothing, five hit
    /// points of healing (not the dead), Chaos Channels, Warp Creature, a Fire Bolt of strength
    /// 15, Warp Lightning, a Doom Bolt, or Disintegrate. Chaos Channels is not in the game yet
    /// (task 159) and does nothing; Warp Creature works as the spell does, on a normal unit not
    /// warped already.
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
            match roll {
                4 => {
                    let u = &active.battle.units[t];
                    let warped = u.enchantments.iter().any(|e| WARPS.contains(&e.as_str()));
                    if let Some(spell) = self.content.rules.spells.id_of("warp_creature") {
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

        let (kept, gone) =
            self.dispel_list(unit.combat_spells.clone(), strength, by, true, &mut rng);
        let mut removed = gone.len() as u32;
        let mut overland_changed = false;
        if let Some(e) = entity {
            let list = self.spells().unit_enchantments(e);
            let (_, gone) = self.dispel_list(list, strength, by, true, &mut rng);
            if !gone.is_empty() {
                for &(spell, victim) in &gone {
                    self.remove_unit_enchantment(e, spell, victim);
                }
                overland_changed = true;
                removed += gone.len() as u32;
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
                    Kind::Turn { bind: true, .. } => Some((def.id.clone(), Vec::new(), Vec::new())),
                    // The warp it got is kept among the unit's enchantment ids.
                    Kind::Warp { .. } => {
                        let variant = WARPS
                            .iter()
                            .position(|w| unit.enchantments.iter().any(|e| e == w))?;
                        Some((
                            WARPS[variant].to_string(),
                            warp_mods(variant, s),
                            Vec::new(),
                        ))
                    }
                    _ => None,
                }
            })
            .collect();
        if let Some(active) = self.battle.as_mut() {
            active.battle.rebuild_unit(target, base, kept, &lasting);
            if unit.enchantments.iter().any(|e| e == CREATURE_BINDING)
                && !lasting.iter().any(|(id, _, _)| id == CREATURE_BINDING)
            {
                active.battle.return_unit(target);
            }
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
        state.resistance_failure(realm, modifier - self.save_penalty, rng) > 0
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
