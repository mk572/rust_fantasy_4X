//! Spells cast by units on their own, the original's `Combat_Cast_Spell` with a
//! battle unit as the caster. A hero casts its four innate spells with its own mana
//! (`BattleUnit::mana`) and the spell of an item with the item's charges; a creature caster (20
//! or 40 mana) casts the spells of its realm; a unit with a `casts_...` ability casts that
//! spell once a battle. Each costs the spell's plain cost: the wizard's reductions and the
//! range multiplier do not apply, and the wizard's own cast of the turn is left alone. A cast
//! uses up the unit's turn (the original sets its movement to 0). A unit with Creature Summoning
//! calls a demon.
//!
//! Where the original is wrong this follows its evident intent: any item's spell can be cast
//! (the original reads only the first slot, bug H22) and the charges stay on the item after the
//! battle (`f4x-core`'s `battle/world.rs` writes them back).
//!
//! What is left to the game (`f4x-core`'s `battle/spell/unit_cast.rs`): the enemy's Counter Magic
//! and the node, the wizard's retorts, and the effect of the spell on the world. Evil Omens is a
//! fact of the world and comes in as an argument.
//!
//! Tests: in `f4x-core`'s battle/tests.rs (`a_hero_casts_an_innate_spell_...`,
//! `the_legal_unit_casts_...` and the demon tests), on whole battles.

use f4x_def::rules::realm::Realm;
use f4x_def::rules::{SpellDef, SpellId, SpellTarget, UnitId};

use super::aim::AimWorld;
use super::caster::SpellAim;
use super::error::SpellError;
use super::spell_type::{LegalCast, SpellType};
use crate::battle::{Battle, BattleError, BattleEvent};
use crate::map::{Pos, GRID_H, GRID_W};
use crate::side::Side;
use crate::unit::{BattleUnit, UnitCasting};

/// Where a unit's cast comes from, which decides what it uses up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellSource {
    /// A charge of this item.
    Item(hecs::Entity),
    /// A spell its ability casts: once a battle.
    Ability,
    /// Its mana.
    Mana,
}

/// A spell a unit could cast in the battle, for a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitSpell {
    pub spell: SpellId,
    pub source: SpellSource,
    /// What it costs the unit's mana at least (0 for an item's charge or an ability).
    pub cost: i32,
    pub takes_extra_mana: bool,
    pub target: SpellTarget,
    /// Whether the unit can cast it now (mana, moves, a ward).
    pub castable: bool,
}

impl Battle {
    /// The spells a unit may cast, each once, with the cheapest source: an item's charge, then
    /// an ability, then its mana (a hero's innate spells, a creature caster's realm).
    fn unit_spell_sources(&self, unit: &BattleUnit) -> Vec<(SpellId, SpellSource)> {
        let UnitCasting {
            innate,
            item_charges,
            abilities,
            ..
        } = &unit.casting;
        let mut out: Vec<(SpellId, SpellSource)> = Vec::new();
        let mut add = |spell: SpellId, source: SpellSource| {
            if !out.iter().any(|(s, _)| *s == spell) {
                out.push((spell, source));
            }
        };
        for c in item_charges {
            if c.charges > 0 {
                add(c.spell, SpellSource::Item(c.item));
            }
        }
        for &spell in abilities {
            add(spell, SpellSource::Ability);
        }
        for &spell in innate {
            add(spell, SpellSource::Mana);
        }
        if !unit.hero_caster && unit.is_caster(&self.content.rules) {
            if let Some(realm) = unit.realm {
                let spells = &self.content.rules.spells;
                for def in spells.defs() {
                    if Realm::of(def.realm) == Some(realm) {
                        add(SpellId::new(def.num), SpellSource::Mana);
                    }
                }
            }
        }
        out
    }

    /// The spells a battle unit can cast, in the order of its sources. Those the battle cannot
    /// play are left out. While it is the unit's side to play, `castable` says whether it can
    /// cast each now. `evil_omens`: whether Evil Omens is in play.
    pub fn unit_spells(&self, unit: usize, evil_omens: bool) -> Vec<UnitSpell> {
        let Some(u) = self.unit(unit) else {
            return Vec::new();
        };
        let ready = u.is_active() && u.side == self.active && u.moves_left > 0;
        self.unit_spell_sources(u)
            .into_iter()
            .filter_map(|(spell, source)| {
                let def = self.content.rules.spells.get(spell);
                let kind = SpellType::of(spell, def)?;
                let cost = if source == SpellSource::Mana {
                    unit_cast_cost(def, evil_omens)
                } else {
                    0
                };
                Some(UnitSpell {
                    spell,
                    source,
                    cost,
                    takes_extra_mana: kind.takes_extra_mana(),
                    target: def.target,
                    castable: ready && cost <= u.mana && !self.ward_stops(u.side, Some(def.realm)),
                })
            })
            .collect()
    }

    /// Every check a unit's cast must pass before it is paid for. Gives the spell's
    /// definition, what it does, where it comes from, the mana it uses up and the strength it
    /// works with (the spell's cost, or the mana put into a spell that takes extra).
    pub fn check_unit_cast(
        &self,
        world: &dyn AimWorld,
        evil_omens: bool,
        unit: usize,
        spell: SpellId,
        aim: SpellAim,
        mana: u32,
    ) -> Result<(SpellDef, SpellType, SpellSource, i32, i32), BattleError> {
        let err = BattleError::Spell;
        if self.winner.is_some() {
            return Err(BattleError::Over);
        }
        let u = self.unit(unit).ok_or(BattleError::NoSuchUnit)?;
        if !u.is_active() || u.side != self.active {
            return Err(BattleError::NotYours);
        }
        if u.moves_left == 0 {
            return Err(BattleError::NoMovesLeft);
        }
        let (_, source) = self
            .unit_spell_sources(u)
            .into_iter()
            .find(|(s, _)| *s == spell)
            .ok_or(err(SpellError::NotKnown))?;
        let def = self.content.rules.spells.get(spell).clone();
        let kind = SpellType::of(spell, &def).ok_or(err(SpellError::Unsupported))?;
        let least = unit_cast_cost(&def, evil_omens);
        let (paid, pool) = match source {
            SpellSource::Mana if kind.takes_extra_mana() => {
                let pool = if mana == 0 { least } else { mana as i32 };
                if pool < least {
                    return Err(err(SpellError::TooLittle));
                }
                (pool, pool)
            }
            // Evil Omens dears the mana, not the strength a counter has to beat (1.50 note 720).
            SpellSource::Mana => (least, def.cost as i32),
            _ => (0, def.cost as i32),
        };
        if paid > u.mana {
            return Err(err(SpellError::TooExpensive));
        }
        if self.ward_stops(u.side, Some(def.realm)) {
            return Err(err(SpellError::Warded));
        }
        self.check_aim(world, u.side, spell, &def, &kind, aim)
            .map_err(err)?;
        Ok((def, kind, source, paid, pool))
    }

    /// The casts the units of the side to play may make now: for each unit and spell it could
    /// cast, every aim the spell accepts and the mana that may go in. For the AI and the console.
    pub fn legal_unit_casts(
        &self,
        world: &dyn AimWorld,
        evil_omens: bool,
    ) -> Vec<(usize, LegalCast)> {
        let mut candidates = vec![SpellAim::None];
        candidates.extend(self.units.iter().map(|u| SpellAim::Unit(u.id)));
        // With several summoning circles a recall names the one it goes to.
        let circles = world.summon_circles(self.player_of(self.active));
        if circles.len() > 1 {
            for u in &self.units {
                candidates.extend(circles.iter().map(|&c| SpellAim::UnitTo(u.id, c)));
            }
        }
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                if self.map.is_playable((x, y)) {
                    candidates.push(SpellAim::Tile((x, y)));
                }
            }
        }
        let mut out = Vec::new();
        let ids: Vec<usize> = self.active_units(self.active).map(|u| u.id).collect();
        for id in ids {
            let mana = self.unit(id).map_or(0, |u| u.mana);
            for s in self.unit_spells(id, evil_omens) {
                if !s.castable {
                    continue;
                }
                let cost = unit_cast_cost(self.content.rules.spells.get(s.spell), evil_omens).max(0)
                    as u32;
                let most = if s.takes_extra_mana && s.source == SpellSource::Mana {
                    (mana.max(0) as u32).max(cost)
                } else {
                    cost
                };
                let aims: Vec<SpellAim> = candidates
                    .iter()
                    .copied()
                    .filter(|&aim| {
                        self.check_unit_cast(world, evil_omens, id, s.spell, aim, cost)
                            .is_ok()
                    })
                    .collect();
                if !aims.is_empty() {
                    out.push((
                        id,
                        LegalCast {
                            spell: s.spell,
                            aims,
                            least_mana: cost,
                            most_mana: most,
                        },
                    ));
                }
            }
        }
        out
    }

    /// Pay for a unit's cast: its mana, a charge or the ability, and its turn. Gives the unit's
    /// own bonus to the resistance checks of its spells.
    pub fn pay_unit_cast(
        &mut self,
        unit: usize,
        spell: SpellId,
        source: SpellSource,
        paid: i32,
    ) -> i32 {
        let u = &mut self.units[unit];
        u.end_moves();
        u.attacked = true;
        u.revealed = true;
        match source {
            SpellSource::Mana => {
                u.spend_mana(paid);
            }
            SpellSource::Item(item) => {
                if let Some(i) = u.casting.item_charges.iter().position(|c| c.item == item) {
                    u.casting.item_charges[i].charges -= 1;
                }
            }
            SpellSource::Ability => u.casting.abilities.retain(|s| *s != spell),
        }
        u.casting.spell_save
    }

    /// Whether the unit could summon a demon now, and where one could appear.
    pub fn demon_squares(&self, unit: usize) -> Result<Vec<Pos>, BattleError> {
        if self.winner.is_some() {
            return Err(BattleError::Over);
        }
        let u = self.unit(unit).ok_or(BattleError::NoSuchUnit)?;
        if !u.is_active() || u.side != self.active {
            return Err(BattleError::NotYours);
        }
        if u.moves_left == 0 {
            return Err(BattleError::NoMovesLeft);
        }
        if u.casting.demons == 0 {
            return Err(BattleError::Spell(SpellError::NotKnown));
        }
        let side = u.side;
        let mut squares: Vec<Pos> = (0..GRID_H)
            .flat_map(|y| (0..GRID_W).map(move |x| (x, y)))
            .filter(|&c| self.is_summon_square(side, c))
            .collect();
        if self.active_units(side).count() >= self.max_side || squares.is_empty() {
            return Err(BattleError::Spell(SpellError::NoRoom));
        }
        // The nine squares nearest the side's edge, in the middle of the field.
        squares.sort_by_key(|&(x, y)| {
            let edge = match side {
                Side::Attacker => x,
                Side::Defender => GRID_W - 1 - x,
            };
            (edge, (y - GRID_H / 2).abs(), y)
        });
        squares.truncate(9);
        Ok(squares)
    }

    /// A unit with Creature Summoning calls a demon (the original's `Summon_Demon`): one of the
    /// unit's summons is used up, the demon appears on one of the nine squares nearest the
    /// side's edge, and the unit's turn is over. Counter Magic does not meet it: it is no spell.
    pub fn summon_demon(&mut self, unit: usize) -> Result<BattleEvent, BattleError> {
        let squares = self.demon_squares(unit)?;
        let demon = UnitId::new(169) /* demon */;
        if self.content.rules.units.id_numbered(demon.num()).is_none() {
            return Err(BattleError::Spell(SpellError::Unsupported));
        }
        let side = self.active;
        let cell = squares[self.roll(squares.len() as u32) as usize - 1];
        // The demon exists only in this battle: it is made from its definition.
        let copy = BattleUnit::from_def(&self.content.rules, demon, side);
        let u = &mut self.units[unit];
        u.casting.demons -= 1;
        u.end_moves();
        u.attacked = true;
        let id = self.add_summoned(side, copy, cell);
        Ok(BattleEvent::Summoned { unit: id })
    }
}

/// The mana a unit pays for a spell: its plain cost, and with Evil Omens in play half as
/// much again for a life or nature spell (1.50 note 720: caster units pay it too).
pub fn unit_cast_cost(def: &SpellDef, evil_omens: bool) -> i32 {
    let cost = def.cost as i32;
    if matches!(def.realm.num(), 0 | 3) && evil_omens {
        cost + cost / 2
    } else {
        cost
    }
}
