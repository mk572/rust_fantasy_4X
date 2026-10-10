//! Joining battles to the game: building one from overland units, running it through
//! commands, and writing its results back.

use crate::city::{PlayerId, Position};
use crate::rules::{MovementType, Stat};
use crate::units::Hurt;
use crate::{Event, Game};

use super::map::{deployment, BattleMap};
use super::{
    Battle, BattleCommand, BattleError, BattleEvent, BattleUnit, RangedKind, Side, Status,
};

/// A battle in progress and the overland units its fighters were copied from.
pub struct ActiveBattle {
    pub battle: Battle,
    /// `BattleUnit::source` indexes this list.
    pub sources: Vec<hecs::Entity>,
    /// The lair being assaulted, if this is a fight against a site's guards.
    pub site: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleStartError {
    /// A battle is already being fought.
    InProgress,
    /// A side is empty, or its units are not one player's on one square.
    BadSides,
}

/// What kind of ranged attack the unit's abilities give it.
pub fn ranged_kind_of(abilities: &[String]) -> RangedKind {
    let has = |names: &[&str]| abilities.iter().any(|a| names.contains(&a.as_str()));
    if has(&["bow", "sling", "rock_throwing"]) {
        RangedKind::Missile
    } else if has(&["thrown", "fire_breath", "lightning_breath"]) {
        RangedKind::Short
    } else if has(&[
        "fireball_attack",
        "lightning_bolt",
        "ice_bolt",
        "nature_bolt",
        "sorcery_bolt",
        "sprite_bolt",
        "drow_bolt",
        "shaman_bolt",
        "death_bolt",
    ]) {
        RangedKind::Magic
    } else {
        RangedKind::None
    }
}

impl Game {
    /// Is a battle being fought?
    pub fn in_battle(&self) -> bool {
        self.battle.is_some()
    }

    /// The battle in progress.
    pub fn battle(&self) -> Option<&Battle> {
        self.battle.as_ref().map(|b| &b.battle)
    }

    /// Copy an overland unit into battle form.
    fn battle_unit_from(&self, unit: hecs::Entity, side: Side, index: usize) -> Option<BattleUnit> {
        let def = self.unit_def(unit)?;
        let hits = self.unit_stat(unit, Stat::MaxHp).max(1);
        let figures = self.unit_stat(unit, Stat::Figures).max(1) as u32;
        let total = hits * figures as i32;
        let hurt = self
            .world
            .get::<&Hurt>(unit)
            .map_or(0, |h| h.0 as i32)
            .min(total - 1);
        let current = total - hurt;
        let alive = ((current + hits - 1) / hits) as u32;
        let flies =
            def.movement == MovementType::Flying || def.abilities.iter().any(|a| a == "flying");
        let has = |a: &str| def.abilities.iter().any(|x| x == a);
        Some(BattleUnit {
            id: 0,
            side,
            source: Some(index),
            unit_id: def.id.clone(),
            name: def.name.clone(),
            cell: (0, 0),
            melee: self.unit_stat(unit, Stat::Melee),
            ranged: self.unit_stat(unit, Stat::Ranged),
            defense: self.unit_stat(unit, Stat::Defense),
            resistance: self.unit_stat(unit, Stat::Resistance),
            to_hit: self.unit_stat(unit, Stat::ToHit) / 10,
            to_block: 0,
            hits,
            figures_start: figures,
            figures: alive,
            front_damage: alive as i32 * hits - current,
            moves: self.unit_max_moves(unit),
            moves_left: self.unit_max_moves(unit),
            ammo: self.unit_stat(unit, Stat::Ammo),
            ranged_kind: ranged_kind_of(&def.abilities),
            flies,
            first_strike: has("first_strike"),
            negate_first_strike: has("negate_first_strike"),
            long_range: has("long_range"),
            abilities: def.abilities.clone(),
            status: Status::Active,
            attacked: false,
        })
    }

    /// Begin a battle between two stacks. The defenders' square decides the terrain.
    pub fn start_battle(
        &mut self,
        attackers: &[hecs::Entity],
        defenders: &[hecs::Entity],
    ) -> Result<Vec<Event>, BattleStartError> {
        if self.battle.is_some() {
            return Err(BattleStartError::InProgress);
        }
        let side_info = |game: &Game, units: &[hecs::Entity]| -> Option<(PlayerId, Position)> {
            let first = *units.first()?;
            let pos = game.unit_position(first)?;
            let owner = game.unit_owner(first)?;
            units
                .iter()
                .all(|&u| game.unit_position(u) == Some(pos) && game.unit_owner(u) == Some(owner))
                .then_some((owner, pos))
        };
        let (a_owner, _) = side_info(self, attackers).ok_or(BattleStartError::BadSides)?;
        let (d_owner, site) = side_info(self, defenders).ok_or(BattleStartError::BadSides)?;
        if a_owner == d_owner {
            return Err(BattleStartError::BadSides);
        }
        let mut units = Vec::new();
        let mut sources = Vec::new();
        for (side, group) in [(Side::Attacker, attackers), (Side::Defender, defenders)] {
            let cells = deployment(side == Side::Attacker, group.len());
            for (&unit, cell) in group.iter().zip(cells) {
                let index = sources.len();
                let mut bu = self
                    .battle_unit_from(unit, side, index)
                    .ok_or(BattleStartError::BadSides)?;
                bu.cell = cell;
                sources.push(unit);
                units.push(bu);
            }
        }
        let seed = self.next_u64();
        let mut map_rng = <rand_pcg::Pcg64Mcg as rand::SeedableRng>::seed_from_u64(seed ^ 0xB477);
        let map = BattleMap::generate(self.terrain_class_at(site), &mut map_rng);
        let mut battle = Battle::new(a_owner, d_owner, map, units, seed);
        battle.site = Some(site);
        self.battle = Some(ActiveBattle {
            battle,
            sources,
            site: None,
        });
        Ok(vec![Event::BattleStarted {
            attacker: a_owner,
            defender: d_owner,
            site,
        }])
    }

    /// Carry out a battle command; when it ends the battle, apply the results.
    pub fn battle_command(&mut self, command: BattleCommand) -> Result<Vec<Event>, BattleError> {
        let active = self.battle.as_mut().ok_or(BattleError::Over)?;
        let events = active.battle.apply(command)?;
        let mut out: Vec<Event> = events.iter().cloned().map(Event::Battle).collect();
        if events
            .iter()
            .any(|e| matches!(e, BattleEvent::Ended { .. }))
        {
            out.extend(self.finish_battle());
        }
        Ok(out)
    }

    /// Write a finished battle back: wounds, deaths, and who won.
    fn finish_battle(&mut self) -> Vec<Event> {
        let Some(active) = self.battle.take() else {
            return Vec::new();
        };
        let ActiveBattle {
            battle,
            sources,
            site,
        } = active;
        let mut events = Vec::new();
        for u in &battle.units {
            let Some(entity) = u.source.and_then(|i| sources.get(i).copied()) else {
                continue;
            };
            if u.status == Status::Dead {
                let _ = self.world.despawn(entity);
            } else {
                let hurt = (u.max_hit_points() - u.hit_points()).max(0) as u32;
                let _ = self.world.insert_one(entity, Hurt(hurt));
            }
        }
        if let Some(side) = battle.winner {
            events.push(Event::BattleEnded {
                winner: battle.player_of(side),
                loser: battle.player_of(side.other()),
            });
        }
        if let Some(index) = site {
            let attackers: Vec<hecs::Entity> = battle
                .units
                .iter()
                .filter(|u| u.side == Side::Attacker)
                .filter_map(|u| u.source.and_then(|i| sources.get(i).copied()))
                .collect();
            let winner = battle
                .winner
                .filter(|&s| s == Side::Attacker)
                .map(|s| battle.player_of(s));
            events.extend(self.finish_site_battle(index, winner, &attackers));
        }
        events
    }
}
