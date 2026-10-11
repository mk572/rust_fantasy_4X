//! What the battle agent sees (`docs/learning-ai.md` 7.2), as plain `f32` buffers.
//!
//! A battle is shown to the side to play as: a picture of the field (channels over the 21 x 22
//! squares), one token for each unit (the stats of the unit and a multi-hot of its abilities, so
//! a unit nobody has seen is read from its numbers), one token for each spell the wizard can
//! cast, and a few numbers for the rest (casting skill, mana, the turn, the brief).
//! Everything shown is on the battle screen or in the console's `battle` query.

use f4x_core::battle::immunity::Realm;
use f4x_core::battle::map::{in_city_area, GRID_H, GRID_W};
use f4x_core::battle::{Battle, RangedKind, Side};
use f4x_core::content::Content;
use f4x_core::rules::defs::SpellTarget;
use f4x_core::Game;

/// Channels of the field picture.
pub const GRID_CHANNELS: usize = 6;
/// Most unit tokens (two stacks of nine and what is summoned).
pub const MAX_UNITS: usize = 32;
/// Most spell tokens.
pub const MAX_SPELLS: usize = 24;
/// Numbers of the global vector.
pub const GLOBALS: usize = 17;
/// Features of a unit before its abilities.
pub const UNIT_BASE: usize = 28;
/// The place of "what fleeing now would score" among the globals.
pub const FLEE_VALUE: usize = 16;
/// Features of a spell token.
pub const SPELL_FEATURES: usize = 16;

/// The brief the strategic AI gives (task 171); zeros mean "no brief".
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Brief {
    pub mana_budget: Option<u32>,
    pub stakes: f32,
    pub may_flee: bool,
}

/// What the agent sees now.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    /// `GRID_CHANNELS` x `GRID_H` x `GRID_W`, channel first.
    pub grid: Vec<f32>,
    /// `MAX_UNITS` x `unit_features`.
    pub units: Vec<f32>,
    /// Which unit tokens are real.
    pub unit_mask: Vec<bool>,
    /// The battle unit number of each token.
    pub unit_ids: Vec<usize>,
    /// `MAX_SPELLS` x `SPELL_FEATURES`.
    pub spells: Vec<f32>,
    pub spell_mask: Vec<bool>,
    /// The spell of each token.
    pub spell_ids: Vec<f4x_core::rules::SpellId>,
    pub globals: Vec<f32>,
}

/// How to read a battle: the ability vocabulary of the content.
#[derive(Debug, Clone)]
pub struct ObsSpec {
    abilities: Vec<String>,
}

impl ObsSpec {
    pub fn new(content: &Content) -> Self {
        Self {
            abilities: content
                .rules
                .abilities
                .iter()
                .map(|(_, a)| a.id.clone())
                .collect(),
        }
    }

    pub fn unit_features(&self) -> usize {
        UNIT_BASE + self.abilities.len()
    }

    fn realm_slot(r: Option<Realm>) -> usize {
        match r {
            None => 0,
            Some(Realm::Nature) => 1,
            Some(Realm::Sorcery) => 2,
            Some(Realm::Chaos) => 3,
            Some(Realm::Life) => 4,
            Some(Realm::Death) => 5,
            Some(Realm::Arcane) => 6,
        }
    }

    /// The observation for `side` (the side to play, usually).
    pub fn observe(&self, game: &Game, side: Side, brief: Brief) -> Option<Observation> {
        let b = game.battle()?;
        let uf = self.unit_features();
        let mut grid = vec![0.0f32; GRID_CHANNELS * (GRID_H * GRID_W) as usize];
        let at = |c: usize, (x, y): (i32, i32)| {
            c * (GRID_H * GRID_W) as usize + (y * GRID_W + x) as usize
        };
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                let p = (x, y);
                grid[at(0, p)] = f32::from(in_city_area(p));
                grid[at(1, p)] = f32::from(b.map.has_wall(p));
                grid[at(2, p)] = f32::from(b.map.enter_cost(p, false).is_none());
            }
        }
        let mut units = vec![0.0f32; MAX_UNITS * uf];
        let mut unit_mask = vec![false; MAX_UNITS];
        let mut unit_ids = vec![0usize; MAX_UNITS];
        for (token, u) in b
            .units
            .iter()
            .filter(|u| u.is_active())
            .take(MAX_UNITS)
            .enumerate()
        {
            let own = u.side == side;
            let frac = u.figures as f32 / u.figures_start.max(1) as f32;
            grid[at(if own { 3 } else { 4 }, u.cell)] = frac;
            if own {
                for cell in b.reachable(u.id).into_keys() {
                    grid[at(5, cell)] = 1.0;
                }
            }
            let row = &mut units[token * uf..(token + 1) * uf];
            let kind = match u.ranged_kind {
                RangedKind::None => 0,
                RangedKind::Missile => 1,
                RangedKind::Magic => 2,
                RangedKind::Short => 3,
            };
            let realm = Self::realm_slot(u.realm);
            let f = [
                f32::from(own),
                f32::from(u.side == b.active),
                u.cell.0 as f32 / GRID_W as f32,
                u.cell.1 as f32 / GRID_H as f32,
                u.melee as f32 / 20.0,
                u.ranged as f32 / 20.0,
                u.defense as f32 / 20.0,
                u.resistance as f32 / 20.0,
                u.hits as f32 / 20.0,
                u.figures as f32 / 10.0,
                u.figures_start as f32 / 10.0,
                u.front_damage as f32 / u.hits.max(1) as f32,
                u.moves_left as f32 / 12.0,
                u.moves as f32 / 12.0,
                u.ammo as f32 / 10.0,
                f32::from(u.attacked),
                f32::from(u.flies),
                f32::from(u.undead),
                f32::from(u.is_fantastic()),
                f32::from(u.first_strike),
                u.special_level as f32 / 5.0,
                f32::from(u.weapon_quality) / 3.0,
                u.enchantments.len() as f32 / 4.0,
                f32::from(kind == 1),
                f32::from(kind == 2),
                f32::from(kind == 3),
                realm as f32 / 6.0,
                f32::from(u.long_range),
            ];
            row[..f.len()].copy_from_slice(&f);
            for (i, a) in self.abilities.iter().enumerate() {
                if u.has(a) {
                    row[UNIT_BASE + i] = 1.0;
                }
            }
            unit_mask[token] = true;
            unit_ids[token] = u.id;
        }
        // The spells of the wizard.
        let player = b.player_of(side);
        let mut spells = vec![0.0f32; MAX_SPELLS * SPELL_FEATURES];
        let mut spell_mask = vec![false; MAX_SPELLS];
        let mut spell_ids = Vec::new();
        for (i, s) in game
            .battle_spells(player)
            .into_iter()
            .take(MAX_SPELLS)
            .enumerate()
        {
            let d = game.content.rules.spells.get(s.spell);
            let target = match s.target {
                SpellTarget::None => 0,
                SpellTarget::Battlefield => 1,
                SpellTarget::Tile => 2,
                SpellTarget::OwnUnit => 3,
                SpellTarget::EnemyUnit => 4,
                _ => 5,
            };
            let realm = ["nature", "sorcery", "chaos", "life", "death", "arcane"]
                .iter()
                .position(|r| *r == d.realm);
            let row = &mut spells[i * SPELL_FEATURES..(i + 1) * SPELL_FEATURES];
            row[0] = 1.0;
            row[1] = s.cost as f32 / 100.0;
            row[2] = f32::from(s.takes_extra_mana);
            row[3] = f32::from(s.castable);
            row[4 + target] = 1.0;
            if let Some(r) = realm {
                row[10 + r] = 1.0;
            }
            spell_mask[i] = true;
            spell_ids.push(s.spell);
        }
        let caster = b.caster(side);
        // What fleeing now would score (`reward.rs`): the units of both sides count.
        let flee_value = {
            let count = |s: Side, f: &dyn Fn(&f4x_core::battle::BattleUnit) -> bool| {
                b.units.iter().filter(|u| u.side == s && f(u)).count() as f32
            };
            let mine = count(side, &|_| true);
            let theirs = count(side.other(), &|_| true);
            let saved = count(side, &|u| u.is_active());
            let killed = count(side.other(), &|u| !u.is_active());
            -1.0 + ((saved + killed) / (mine + theirs).max(1.0)).clamp(0.0, 1.0)
        };
        let globals = [
            caster.map_or(0.0, |c| c.skill_left as f32 / 50.0),
            caster.map_or(0.0, |c| (c.mana as f32 / 500.0).min(4.0)),
            caster.map_or(0.0, |c| c.range as f32 / 30.0),
            caster.map_or(0.0, |c| f32::from(c.cast_this_turn)),
            b.turn as f32 / 50.0,
            f32::from(b.active == side),
            b.counter_magic[side.index()] as f32 / 30.0,
            b.counter_magic[side.other().index()] as f32 / 30.0,
            b.city_damage as f32 / 10.0,
            f32::from(b.wall_of_fire),
            f32::from(b.wall_of_darkness),
            f32::from(b.map.is_walled()),
            // The mana still allowed for the wizard in this battle.
            caster
                .and_then(|c| c.budget)
                .map_or(1.0, |m| (m as f32 / 500.0).min(4.0)),
            f32::from(caster.is_some_and(|c| c.budget.is_some())),
            brief.stakes,
            f32::from(brief.may_flee),
            flee_value,
        ];
        debug_assert_eq!(globals.len(), GLOBALS);
        Some(Observation {
            grid,
            units,
            unit_mask,
            unit_ids,
            spells,
            spell_mask,
            spell_ids,
            globals: globals.to_vec(),
        })
    }
}

/// The battle's own view for tests: how many tokens an observation fills.
pub fn tokens(b: &Battle) -> usize {
    b.units
        .iter()
        .filter(|u| u.is_active())
        .count()
        .min(MAX_UNITS)
}
