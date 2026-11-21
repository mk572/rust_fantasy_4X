//! What a spell does to a battle: the damage it deals, what it would do to a unit, the chance a unit
//! resists it, and the effect of a cast that needs nothing outside the battle
//! ([`Battle::apply_spell`]). Casting a spell (the wizard, the mana, the events) and the effects that
//! reach the overland units (Animate and Raise Dead, Chaos Channels, Dispel) are in `f4x-core`'s
//! `battle/spell.rs`.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::id::PlayerId;
use f4x_def::rules::attack_nature::AttackNature;
use f4x_def::rules::damage_type::DamageType;
use f4x_def::rules::damage_type::DamageTypes;
use f4x_def::rules::defs::SpellSave;
use f4x_def::rules::effects::DamageTraits;
use f4x_def::rules::immunity::{effective_resistance, Immunities};
use f4x_def::rules::realm::Realm;
use f4x_def::rules::unit_effect::UnitEffect;
use f4x_def::rules::unit_flag::UnitFlag;
use f4x_def::rules::{Ability, Modifier, SpellDef, SpellId};
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

use super::aim::save_types;
use super::caster::SpellAim;
use super::spell_type::{slay_allows, undead_penalty, warp_mods, CityWall, DamageOdds, SpellType};
use crate::battle::{Battle, BattleEvent, WarpKind};
use crate::side::Side;
use crate::unit::{BattleUnit, Status};

/// One cast, as the battle needs it: what is cast, by whom, at what, with how much mana.
pub struct Cast<'a> {
    pub spell: SpellId,
    pub def: &'a SpellDef,
    pub kind: SpellType,
    pub side: Side,
    pub player: PlayerId,
    pub aim: SpellAim,
    /// The mana put in (the strength a dispel or Counter Magic works with).
    pub pool: i32,
}

/// What a cast asks of the world outside the battle.
#[derive(Debug, Clone, Default)]
pub struct CastWorld {
    /// The rule setting: immunity to illusion protects against curses with a save.
    pub illusion_immunity_protects: bool,
    /// For an Enchant: what the unit's overland state adds to the spell's bonuses (what spells it
    /// already has give way to, Heroism's level), worked out by the game.
    pub extra_mods: Vec<Modifier>,
}

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
                self.field_bonus_for(unit),
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
        let state = unit.defender_state(&self.content.rules, self.field_bonus_for(unit));
        let resistance = effective_resistance(state.resistance, &state.immunities, realm);
        let needed = resistance + modifier - self.save_penalty;
        (10 - needed.clamp(0, 10)) as f32 / 10.0
    }

    /// A resistance check of one unit against a realm: whether it *fails*.
    pub fn resist_fails(&mut self, target: usize, realm: Option<Realm>, modifier: i32) -> bool {
        let state = self.units[target].defender_state(
            &self.content.rules,
            self.field_bonus_for(&self.units[target]),
        );
        let rng: &mut dyn rand::RngCore = &mut self.rng;
        state.resistance_failure(realm, modifier - self.save_penalty, rng) > 0
    }

    /// The active units of the side that is not `side`, by id.
    fn enemies_of(&self, side: Side) -> Vec<usize> {
        self.active_units(side.other()).map(|u| u.id).collect()
    }

    /// What a cast does to the battle, for every spell whose effect needs nothing but the battle
    /// and the rules. The effects that reach the overland units (Animate and Raise Dead, Chaos
    /// Channels, Call Chaos, Dispel) are the game's and give no events here. The events come in
    /// the order the effect made them; the random rolls are the battle's own stream.
    pub fn apply_spell(&mut self, cast: &Cast, world: &CastWorld) -> Vec<BattleEvent> {
        let Cast {
            spell,
            def,
            side,
            player,
            aim,
            pool,
            ..
        } = *cast;
        let extra_mana = pool - def.cost as i32;
        match (&cast.kind, aim) {
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
                    extra_mana / traits.mana_per_point as i32
                } else {
                    0
                };
                let targets = match (all, aim) {
                    (true, _) => self.enemies_of(side),
                    (false, SpellAim::Unit(t)) => vec![t],
                    _ => return Vec::new(),
                };
                self.strike(&targets, strength + extra, *types, traits)
            }
            (
                SpellType::Heal {
                    points,
                    all,
                    except,
                },
                aim,
            ) => {
                let targets: Vec<usize> = match (all, aim) {
                    (true, _) => self.active_units(side).map(|u| u.id).collect(),
                    (false, SpellAim::Unit(t)) => vec![t],
                    _ => return Vec::new(),
                };
                let mut events = Vec::new();
                for t in targets {
                    if except.is_some() && self.units[t].realm == *except {
                        continue;
                    }
                    let healed = self.spell_heal(t, *points);
                    events.push(BattleEvent::SpellHealed { unit: t, healed });
                }
                events
            }
            (SpellType::CounterMagic, _) => {
                self.cast_counter_magic(side, pool as u32);
                vec![BattleEvent::CounterMagicRaised {
                    side,
                    strength: pool as u32,
                }]
            }
            (SpellType::Field(field), _) => {
                self.raise_field(side, *field, spell);
                vec![BattleEvent::FieldRaised { side, spell }]
            }
            (SpellType::Vortex, SpellAim::Tile(cell)) => vec![self.make_vortex(side, cell)],
            (SpellType::EarthToMud, SpellAim::Tile(cell)) => {
                self.spread_mud(cell);
                vec![BattleEvent::MudMade { cell }]
            }
            (SpellType::CityWall(wall), _) => {
                match wall {
                    CityWall::Stone => {
                        self.map.raise_walls();
                    }
                    CityWall::Fire => self.wall_of_fire = true,
                    CityWall::Darkness => self.wall_of_darkness = true,
                }
                vec![BattleEvent::WallRaised { side, spell }]
            }
            (SpellType::Disrupt, SpellAim::Tile(cell)) => {
                self.map.break_wall(cell);
                vec![BattleEvent::WallDestroyed { cell }]
            }
            (SpellType::CracksCall, SpellAim::Tile(cell)) => self.open_fissure(spell, cell),
            (SpellType::LifeDrain, SpellAim::Unit(target)) => {
                let extra = (extra_mana / 5).max(0);
                let dmg = self.drain_life(target, -extra, spell);
                let mut events = self.spell_events_for_damage(
                    target,
                    dmg,
                    DamageTypes::of(&[DamageType::Death]),
                );
                if let Some(d) = dmg.filter(|d| d.points > 0) {
                    events.push(BattleEvent::LifeDrained {
                        unit: target,
                        points: d.points,
                    });
                    match self.spell_caster {
                        // A hero or creature that cast it takes the life as hit points.
                        Some(c) => {
                            self.units[c].heal_points(d.points, true);
                        }
                        // A wizard turns it into mana.
                        None => {
                            if let Some(caster) = self.casters[side.index()].as_mut() {
                                caster.mana += d.points * 3;
                            }
                        }
                    }
                }
                events
            }
            (SpellType::Recall { .. }, SpellAim::Unit(target)) => {
                self.units[target].status = Status::Recalled;
                vec![BattleEvent::HeroRecalled { unit: target }]
            }
            (SpellType::WarpWood, SpellAim::Unit(target)) => {
                self.units[target].set_ammo(0);
                vec![BattleEvent::SpellEnchanted {
                    unit: target,
                    spell,
                }]
            }
            (SpellType::Turn { bind, save }, SpellAim::Unit(target)) => {
                if let Some(refused) = self.curse_refusal(def, save.as_ref(), target, world) {
                    return refused;
                }
                if *bind {
                    self.enchant_unit(target, spell, player, &[], &[]);
                }
                self.change_sides(target, side);
                vec![BattleEvent::SpellEnchanted {
                    unit: target,
                    spell,
                }]
            }
            (SpellType::Warp { save }, SpellAim::Unit(target)) => {
                self.warp_creature(spell, def, target, player, save.as_ref(), world)
            }
            (SpellType::Enchant { mods, grants, save }, SpellAim::Unit(target)) => {
                let mut mods = mods.clone();
                mods.extend(world.extra_mods.iter().cloned());
                if let Some(refused) = self.curse_refusal(def, save.as_ref(), target, world) {
                    return refused;
                }
                self.enchant_unit(target, spell, player, &mods, grants);
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
                let modifier = if *mana_per_step > 0 {
                    modifier - extra_mana / *mana_per_step as i32
                } else {
                    *modifier
                };
                let targets = match (all, aim) {
                    (true, _) => self.enemies_of(side),
                    (false, SpellAim::Unit(t)) => vec![t],
                    _ => return Vec::new(),
                };
                // Word of Death has `death_immunity_exception`: with the rule on, nothing stops it.
                let immune_types = if def.death_immunity_exception && self.death_immunity_exceptions
                {
                    Vec::new()
                } else {
                    types.clone()
                };
                let mut events = Vec::new();
                for t in targets {
                    // Spell Lock keeps a unit from the spells that sever or burn it.
                    let locked = self.units[t]
                        .effects(&self.content.rules)
                        .contains(UnitEffect::SpellLock);
                    if !slay_allows(*only, &self.units[t]) || locked {
                        continue;
                    }
                    let modifier = if self.units[t].undead {
                        modifier - undead_penalty(spell)
                    } else {
                        modifier
                    };
                    match self.spell_slay(t, *realm, modifier, &immune_types, *below) {
                        None => events.push(BattleEvent::SpellImmune {
                            unit: t,
                            by: self.units[t].immune_type(&immune_types, &self.content.rules),
                        }),
                        Some(d) => {
                            events.push(BattleEvent::SpellSlew {
                                unit: t,
                                kills: d.kills,
                            });
                            if !self.units[t].is_active() {
                                events.push(BattleEvent::UnitDied { unit: t });
                            }
                        }
                    }
                }
                events
            }
            (SpellType::Summon(unit_id), SpellAim::Tile(cell)) => {
                // The creature exists only in this battle: it is made from its definition.
                let unit = BattleUnit::from_def(&self.content.rules, *unit_id, side);
                let id = self.add_summoned(side, unit, cell);
                vec![BattleEvent::Summoned { unit: id }]
            }
            _ => Vec::new(),
        }
    }

    /// The fissure a Cracks Call opens at `cell`: a walker or crawler over it falls in one time in
    /// four, whatever its defence; fliers, the bodiless and merging units (which pass through the
    /// ground) stand clear. A wall on the square breaks.
    fn open_fissure(&mut self, spell: SpellId, cell: crate::map::Pos) -> Vec<BattleEvent> {
        let mut events = vec![BattleEvent::FissureOpened { cell }];
        let over = self
            .unit_at(cell)
            .filter(|u| {
                !u.flag(UnitFlag::Flies)
                    && !self.spares(u, spell)
                    && !u.flag(UnitFlag::NonCorporeal)
                    && !u.has(Ability::new(43 /* merging */))
            })
            .map(|u| u.id);
        if let Some(id) = over {
            if self.roll(4) == 1 {
                let dmg = self.destroy_unit(id);
                events.extend(self.spell_events_for_damage(id, Some(dmg), DamageTypes::NONE));
            }
        }
        if self.map.has_wall(cell) {
            self.map.break_wall(cell);
            events.push(BattleEvent::WallDestroyed { cell });
        }
        events
    }

    /// The resistance check of a spell with a `save` (a curse): the events of a refusal when the
    /// target is immune or resists, `None` when the spell goes on to work (or has no save).
    fn curse_refusal(
        &mut self,
        def: &SpellDef,
        save: Option<&SpellSave>,
        target: usize,
        world: &CastWorld,
    ) -> Option<Vec<BattleEvent>> {
        let save = save?;
        let immune_types = save_types(save, world.illusion_immunity_protects);
        let unit = &self.units[target];
        if unit.is_immune_to(&immune_types, &self.content.rules) {
            return Some(vec![BattleEvent::SpellImmune {
                unit: target,
                by: unit.immune_type(&immune_types, &self.content.rules),
            }]);
        }
        if !self.resist_fails(target, Realm::of(def.realm), save.modifier) {
            return Some(vec![BattleEvent::SpellResisted { unit: target }]);
        }
        None
    }

    /// Warp Creature on a unit: after its resistance check (`save`), one of three lasting harms.
    pub fn warp_creature(
        &mut self,
        spell: SpellId,
        def: &SpellDef,
        target: usize,
        player: PlayerId,
        save: Option<&SpellSave>,
        world: &CastWorld,
    ) -> Vec<BattleEvent> {
        if let Some(refused) = self.curse_refusal(def, save, target, world) {
            return refused;
        }
        let kind = match self.roll(3) {
            1 => WarpKind::Attack,
            2 => WarpKind::Defense,
            _ => WarpKind::Resistance,
        };
        self.enchant_unit(target, spell, player, &warp_mods(kind, spell), &[]);
        self.units[target].warp = Some(kind);
        vec![BattleEvent::SpellEnchanted {
            unit: target,
            spell,
        }]
    }
}
