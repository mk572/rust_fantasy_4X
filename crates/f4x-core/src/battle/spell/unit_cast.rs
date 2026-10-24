//! Spells cast by units on their own (task 265), the original's `Combat_Cast_Spell` with a
//! battle unit as the caster. A hero casts its four innate spells with its own mana
//! (`BattleUnit::mana`) and the spell of an item with the item's charges; a creature caster (20
//! or 40 mana) casts the spells of its realm; a unit with a `casts_...` ability casts that
//! spell once a battle. Each costs the spell's plain cost: the wizard's reductions and the
//! range multiplier do not apply, and the wizard's own cast of the turn is left alone. A cast
//! uses up the unit's turn (the original sets its movement to 0).
//!
//! Where the original is wrong this follows its evident intent: any item's spell can be cast
//! (the original reads only the first slot, bug H22) and the charges stay on the item after the
//! battle (`game.rs` writes them back).

use super::*;
use crate::battle::{BattleUnit, UnitCasting};

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

impl Game {
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
        if !unit.hero_caster && (unit.has("spellcaster") || unit.has("spellcaster_major")) {
            if let Some(realm) = unit.realm {
                let spells = &self.content.rules.spells;
                for def in spells.defs() {
                    if Realm::from_id(&def.realm) == Some(realm) {
                        if let Some(id) = spells.id_of(&def.id) {
                            add(id, SpellSource::Mana);
                        }
                    }
                }
            }
        }
        out
    }

    /// The spells a battle unit can cast, in the order of its sources. Those the battle cannot
    /// play are left out. While it is the unit's side to play, `castable` says whether it can
    /// cast each now.
    pub fn battle_unit_spells(&self, unit: usize) -> Vec<UnitSpell> {
        let Some(battle) = self.battle() else {
            return Vec::new();
        };
        let Some(u) = battle.unit(unit) else {
            return Vec::new();
        };
        let ready = u.is_active() && u.side == battle.active && u.moves_left > 0;
        self.unit_spell_sources(u)
            .into_iter()
            .filter_map(|(spell, source)| {
                let def = self.content.rules.spells.get(spell);
                let kind = self.battle_kind(spell, def)?;
                let cost = if source == SpellSource::Mana {
                    def.cost as i32
                } else {
                    0
                };
                Some(UnitSpell {
                    spell,
                    source,
                    cost,
                    takes_extra_mana: kind.takes_extra_mana(),
                    target: def.target,
                    castable: ready && cost <= u.mana && !battle.ward_stops(u.side, &def.realm),
                })
            })
            .collect()
    }

    /// Every check a unit's cast must pass before it is paid for. Gives the spell's
    /// definition, what it does, where it comes from, the mana it uses up and the strength it
    /// works with (the spell's cost, or the mana put into a spell that takes extra).
    fn check_unit_cast(
        &self,
        unit: usize,
        spell: SpellId,
        aim: SpellAim,
        mana: u32,
    ) -> Result<(SpellDef, Kind, SpellSource, i32, i32), BattleError> {
        let err = BattleError::Spell;
        let battle = &self.battle.as_ref().ok_or(BattleError::Over)?.battle;
        if battle.winner.is_some() {
            return Err(BattleError::Over);
        }
        let u = battle.unit(unit).ok_or(BattleError::NoSuchUnit)?;
        if !u.is_active() || u.side != battle.active {
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
        let kind = self
            .battle_kind(spell, &def)
            .ok_or(err(SpellError::Unsupported))?;
        let least = def.cost as i32;
        let (paid, pool) = match source {
            SpellSource::Mana if kind.takes_extra_mana() => {
                let pool = if mana == 0 { least } else { mana as i32 };
                if pool < least {
                    return Err(err(SpellError::TooLittle));
                }
                (pool, pool)
            }
            SpellSource::Mana => (least, least),
            _ => (0, least),
        };
        if paid > u.mana {
            return Err(err(SpellError::TooExpensive));
        }
        if battle.ward_stops(u.side, &def.realm) {
            return Err(err(SpellError::Warded));
        }
        self.check_aim(battle, u.side, &def, &kind, aim)
            .map_err(err)?;
        Ok((def, kind, source, paid, pool))
    }

    /// The casts the units of the side to play may make now: for each unit and spell it could
    /// cast, every aim the spell accepts and the mana that may go in. Like
    /// [`Game::legal_battle_casts`], for the AI and the console.
    pub fn legal_unit_casts(&self) -> Vec<(usize, LegalCast)> {
        let Some(battle) = self.battle() else {
            return Vec::new();
        };
        let mut candidates = vec![SpellAim::None];
        candidates.extend(battle.units.iter().map(|u| SpellAim::Unit(u.id)));
        for y in 0..super::super::map::GRID_H {
            for x in 0..super::super::map::GRID_W {
                if battle.map.is_playable((x, y)) {
                    candidates.push(SpellAim::Tile((x, y)));
                }
            }
        }
        let mut out = Vec::new();
        let ids: Vec<usize> = battle.active_units(battle.active).map(|u| u.id).collect();
        for id in ids {
            let mana = battle.unit(id).map_or(0, |u| u.mana);
            for s in self.battle_unit_spells(id) {
                if !s.castable {
                    continue;
                }
                let cost = self.content.rules.spells.get(s.spell).cost;
                let most = if s.takes_extra_mana && s.source == SpellSource::Mana {
                    (mana.max(0) as u32).max(cost)
                } else {
                    cost
                };
                let aims: Vec<SpellAim> = candidates
                    .iter()
                    .copied()
                    .filter(|&aim| self.check_unit_cast(id, s.spell, aim, cost).is_ok())
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

    /// Whether the unit could summon a demon now, and where one could appear.
    fn demon_squares(&self, unit: usize) -> Result<Vec<super::super::Pos>, BattleError> {
        let battle = &self.battle.as_ref().ok_or(BattleError::Over)?.battle;
        if battle.winner.is_some() {
            return Err(BattleError::Over);
        }
        let u = battle.unit(unit).ok_or(BattleError::NoSuchUnit)?;
        if !u.is_active() || u.side != battle.active {
            return Err(BattleError::NotYours);
        }
        if u.moves_left == 0 {
            return Err(BattleError::NoMovesLeft);
        }
        if u.casting.demons == 0 {
            return Err(BattleError::Spell(SpellError::NotKnown));
        }
        let side = u.side;
        let mut squares: Vec<super::super::Pos> = (0..super::super::map::GRID_H)
            .flat_map(|y| (0..super::super::map::GRID_W).map(move |x| (x, y)))
            .filter(|&c| battle.is_summon_square(side, c))
            .collect();
        if battle.active_units(side).count() >= MAX_SIDE_UNITS || squares.is_empty() {
            return Err(BattleError::Spell(SpellError::NoRoom));
        }
        // The nine squares nearest the side's edge, in the middle of the field.
        squares.sort_by_key(|&(x, y)| {
            let edge = match side {
                Side::Attacker => x,
                Side::Defender => super::super::map::GRID_W - 1 - x,
            };
            (edge, (y - super::super::map::GRID_H / 2).abs(), y)
        });
        squares.truncate(9);
        Ok(squares)
    }

    /// Whether a unit can summon a demon now (for the AI and the menus).
    pub fn can_summon_demon(&self, unit: usize) -> bool {
        self.demon_squares(unit).is_ok()
    }

    /// A unit with Creature Summoning calls a demon (the original's `Summon_Demon`): one of the
    /// unit's summons is used up, the demon appears on one of the nine squares nearest the
    /// side's edge, and the unit's turn is over. Counter Magic does not meet it: it is no spell.
    pub(in crate::battle) fn battle_summon_demon(
        &mut self,
        unit: usize,
    ) -> Result<Vec<Event>, BattleError> {
        let squares = self.demon_squares(unit)?;
        let Some(demon) = self.content.rules.units.id_of("demon") else {
            return Err(BattleError::Spell(SpellError::Unsupported));
        };
        let battle = &mut self.battle.as_mut().ok_or(BattleError::Over)?.battle;
        let (side, player) = (battle.active, battle.player_of(battle.active));
        let cell = squares[battle.roll(squares.len() as u32) as usize - 1];
        // Make the demon as the game would, copy it, and let the original go.
        let site = self
            .battle()
            .and_then(|b| b.site)
            .or_else(|| self.summon_point(player));
        let Some(site) = site else {
            return Err(BattleError::Spell(SpellError::NoRoom));
        };
        let entity = self.spawn_unit(player, site, demon);
        let copy = self.battle_unit_from(entity, side, 0);
        let _ = self.world.despawn(entity);
        let Some(copy) = copy else {
            return Err(BattleError::Spell(SpellError::Unsupported));
        };
        let Some(active) = self.battle.as_mut() else {
            return Err(BattleError::Over);
        };
        let u = &mut active.battle.units[unit];
        u.casting.demons -= 1;
        u.moves_left = 0;
        u.attacked = true;
        let id = active.battle.add_summoned(side, copy, cell);
        Ok(vec![Event::Battle(BattleEvent::Summoned { unit: id })])
    }

    /// A unit casts a spell of its own.
    pub(in crate::battle) fn battle_unit_cast(
        &mut self,
        unit: usize,
        spell: SpellId,
        aim: SpellAim,
        mana: u32,
    ) -> Result<Vec<Event>, BattleError> {
        let (def, kind, source, paid, pool) = self.check_unit_cast(unit, spell, aim, mana)?;
        let battle = &mut self.battle.as_mut().ok_or(BattleError::Over)?.battle;
        let side = battle.active;
        let player = battle.player_of(side);

        // Pay: the unit's mana, a charge or the ability, and its turn.
        let u = &mut battle.units[unit];
        u.moves_left = 0;
        u.attacked = true;
        u.revealed = true;
        let spell_save = u.casting.spell_save;
        match source {
            SpellSource::Mana => {
                u.mana -= paid;
                if u.shoots_with_mana() {
                    u.ammo = u.ammo.min(u.mana / super::super::SHOT_MANA);
                }
            }
            SpellSource::Item(item) => {
                if let Some(i) = u.casting.item_charges.iter().position(|c| c.item == item) {
                    u.casting.item_charges[i].charges -= 1;
                }
            }
            SpellSource::Ability => u.casting.abilities.retain(|s| *s != spell),
        }
        let mut events = vec![BattleEvent::UnitSpellCast {
            unit,
            spell,
            mana: paid,
        }];

        // The enemy's Counter Magic tries to stop it (not the Web an ability casts).
        let exempt = source == SpellSource::Ability && def.id == "web";
        let difficulty = pool as u32 * self.dispel_multiplier(&def.realm, Some(player));
        let stopped = !exempt && self.battle_spell_stopped(side, player, &def.realm, difficulty);
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
        let mut out: Vec<Event> = events.into_iter().map(Event::Battle).collect();
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
