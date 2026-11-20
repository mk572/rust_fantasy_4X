//! The numbers a unit fights with, worked out from what it is, never stored. A unit holds its
//! sheet (what the game handed over), the lasting spells cast on it in the battle, what the
//! battlefield spells ask of it, the ids of the wizards' global enchantments and retorts, and what
//! it has lost, spent and used. Everything else (its level, melee, defence, hit points, figures,
//! moves left, mana) is a function of those and is worked out when asked: the unit keeps the
//! answer only until one of them changes, and the answer is never saved. A spell that goes, a
//! level that rises or a wizard that loses an enchantment changes the inputs; nothing is taken
//! off again.
//!
//! The order is: the definition's numbers with the unit's own modifiers, its level's, a hero's and
//! its wizard's global enchantments' in one sum; then the lasting spells of the battle one after
//! another; the life steal's extra hit points; the battlefield spells' request.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use std::sync::{Arc, OnceLock};

use f4x_def::content::Content;
use f4x_def::half_int::HalfInt;
use f4x_def::rules::damage_type::{DamageTypes, RangedKind};
use f4x_def::rules::effects::{Effect, Trigger};
use f4x_def::rules::facts::{eval, Facts};
use f4x_def::rules::global_effect::GlobalEffect;
use f4x_def::rules::hook_name::HEROISM;
use f4x_def::rules::retort_stat::stat_with;
use f4x_def::rules::stats::{
    compute, compute_half, passive_modifiers, ModOp, Modifier, Source, Stat,
};
use f4x_def::rules::unit_flag::{UnitFlag, UnitFlags};
use f4x_def::rules::{Ability, MovementType, Rules, SpellId, UnitDef, UnitKind};

use super::sheet::WizardSheet;
use super::BattleUnit;
use crate::weapon::{caster_mana_of, ranged_kind_of, weapon_shot_of};

/// What a unit fights with now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub melee: i32,
    pub ranged: i32,
    pub defense: i32,
    pub resistance: i32,
    /// Pluses to hit (each ten percentage points).
    pub to_hit: i32,
    /// Pluses to hit that only the melee attack gets, and only the ranged or thrown attack.
    pub melee_plus: i32,
    pub ranged_plus: i32,
    /// Of the dice of the melee and of the ranged attack, the ones made of damage types.
    pub melee_parts: Vec<(i32, DamageTypes)>,
    pub ranged_parts: Vec<(i32, DamageTypes)>,
    pub to_block: i32,
    /// Hit points of one figure, with the extra ones life steal gave.
    pub hits: i32,
    pub moves: HalfInt,
    pub special_level: i32,
    pub ranged_kind: RangedKind,
    pub flags: UnitFlags,
    pub abilities: Vec<Ability>,
    /// Of those, the ones an item or a lasting spell gives.
    pub granted: Vec<Ability>,
    /// The ids of its lasting spells: overland and of this battle.
    pub enchantments: Vec<SpellId>,
    /// A hero's Leadership and Prayermaster at its level, 0 for anything else.
    pub leadership: i32,
    pub prayermaster: i32,
    /// Figures alive and the damage on the front one, from what the unit has lost.
    pub figures: u32,
    pub front_damage: i32,
    pub moves_left: HalfInt,
    /// Shots left: a spell caster's shots follow its mana.
    pub ammo: i32,
    pub mana: i32,
}

impl Stats {
    /// One lasting spell's modifiers and abilities on the numbers worked out so far.
    pub fn apply(&mut self, mods: &[Modifier], grants: &[Ability]) {
        let comp = |stat: Stat, base: i32| compute(stat, base, mods);
        self.melee = comp(Stat::Melee, self.melee);
        self.ranged = comp(Stat::Ranged, self.ranged);
        self.defense = comp(Stat::Defense, self.defense);
        self.resistance = comp(Stat::Resistance, self.resistance);
        self.to_hit = comp(Stat::ToHit, self.to_hit);
        self.hits = comp(Stat::MaxHp, self.hits).max(1);
        self.moves = compute_half(Stat::Moves, self.moves, mods).max(HalfInt::ZERO);
        for g in grants {
            if !self.abilities.contains(g) {
                self.abilities.push(*g);
            }
        }
    }
}

/// The unit's content handle and its answer, neither of which is saved or counts in its equality.
#[derive(Clone, Default)]
pub(super) struct Memo {
    pub(super) rules: Option<Arc<Content>>,
    stats: OnceLock<Stats>,
}

impl Memo {
    pub(super) fn clear(&mut self) {
        self.stats = OnceLock::new();
    }
}

impl std::fmt::Debug for Memo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Memo")
    }
}

impl PartialEq for Memo {
    fn eq(&self, _other: &Memo) -> bool {
        true
    }
}

impl Eq for Memo {}

impl std::ops::Deref for BattleUnit {
    type Target = Stats;

    fn deref(&self) -> &Stats {
        self.memo.stats.get_or_init(|| self.derive())
    }
}

impl BattleUnit {
    /// Give the unit the content it reads definitions from.
    pub fn bind(&mut self, content: &Arc<Content>) {
        self.memo.rules = Some(Arc::clone(content));
        self.memo.clear();
    }

    fn rules(&self) -> Option<&Rules> {
        self.memo.rules.as_deref().map(|c| &c.rules)
    }

    fn def<'a>(&self, rules: &'a Rules) -> Option<&'a UnitDef> {
        self.unit_id.map(|id| rules.units.get(id))
    }

    /// The unit's experience level: what its experience gives, lifted to Elite by Heroism, then by
    /// one each for its wizard's retorts (Warlord) and Crusade, which leave the undead and
    /// creatures alone, and held to the last level of its path. 0 for a unit without a path.
    pub fn level(&self) -> usize {
        self.rules().map_or(0, |rules| self.level_in(rules))
    }

    fn level_in(&self, rules: &Rules) -> usize {
        let Some(def) = self.def(rules) else {
            return 0;
        };
        let Some(path) = def.level_path.map(|p| rules.level_paths.get(p)) else {
            return 0;
        };
        let mut level = path
            .levels
            .iter()
            .rposition(|l| l.xp <= self.sheet.xp)
            .unwrap_or(0);
        let lifted = rules.config.renown.heroism_level;
        if level < lifted
            && self
                .sheet
                .enchantments
                .iter()
                .any(|&s| rules.spells.get(s).has_hook(HEROISM))
        {
            level = lifted;
        }
        let owner = self.owner();
        level += usize::try_from(stat_with(
            rules,
            owner.retorts.iter().copied(),
            Stat::UnitLevels,
            0,
            &Facts::default(),
        ))
        .unwrap_or(0);
        let normal = matches!(def.kind, UnitKind::Normal | UnitKind::Hero);
        if normal
            && !self.undead
            && owner
                .globals
                .iter()
                .any(|&s| GlobalEffect::Crusade.is_in(rules.spells.get(s)))
        {
            level += 1;
        }
        level.min(path.level_count().saturating_sub(1))
    }

    /// The wizard the unit belongs to.
    fn owner(&self) -> &WizardSheet {
        &self.wizards[self.sheet.home.index()]
    }

    /// The modifiers of the unit's level path for these levels. A level's attack plus only counts
    /// for a unit that has the attack.
    fn level_modifiers(
        def: &UnitDef,
        rules: &Rules,
        levels: std::ops::Range<usize>,
    ) -> Vec<Modifier> {
        let Some(path) = def.level_path.map(|p| rules.level_paths.get(p)) else {
            return Vec::new();
        };
        let mut mods = Vec::new();
        for i in levels {
            if let Some(effects) = path.level(i) {
                mods.extend(
                    passive_modifiers(effects, Source::Level(i as u8))
                        .into_iter()
                        .filter(|m| {
                            !matches!(m.stat, Stat::Melee | Stat::Ranged)
                                || def.stats.get(&m.stat).map_or(0, |v| v.floor()) > 0
                        }),
                );
            }
        }
        mods
    }

    /// What the owner's global enchantments do to the unit: the passive rules whose condition it
    /// meets, and Charm of Life's hit points.
    fn global_modifiers(&self, rules: &Rules, def: Option<&UnitDef>) -> Vec<Modifier> {
        let owner = self.owner();
        if owner.globals.is_empty() {
            return Vec::new();
        }
        let facts = Facts {
            abilities: self.sheet.abilities.clone(),
            realm: self.realm.map(|r| f4x_def::rules::RealmId::new(r.num())),
            // The undead are not normal units: Holy Arms and Crusade pass them by.
            kind: def.map(|d| {
                if self.undead {
                    UnitKind::Summoned
                } else {
                    d.kind
                }
            }),
            ..Facts::default()
        };
        let mut mods = Vec::new();
        for &spell in &owner.globals {
            let spell_def = rules.spells.get(spell);
            for rule in &spell_def.effects {
                if rule.when != Trigger::Passive || !eval(&rule.condition, &facts) {
                    continue;
                }
                if let Effect::Modify {
                    stat,
                    op,
                    value,
                    types,
                } = rule.effect
                {
                    mods.push(
                        Modifier::amount(stat, op, value, Source::Spell(spell)).of_types(types),
                    );
                }
            }
            if spell_def.id == "charm_of_life" {
                let hp = def.map_or(0, |d| d.stats.get(&Stat::MaxHp).map_or(0, |v| v.floor()));
                if hp > 0 {
                    mods.push(Modifier::new(
                        Stat::MaxHp,
                        ModOp::Add,
                        (hp / 4).max(1),
                        Source::Spell(spell),
                    ));
                }
            }
        }
        mods
    }

    /// Whether a global enchantment of the owner has the effect.
    fn owner_has(&self, rules: &Rules, effect: GlobalEffect) -> bool {
        self.owner()
            .globals
            .iter()
            .any(|&s| effect.is_in(rules.spells.get(s)))
    }

    /// Wind Mastery (our rule): ships and fliers of a wizard who keeps it go half as far again;
    /// those of a wizard without it, while anyone else keeps it, go half as far.
    fn wind_moves(&self, rules: &Rules, moves: HalfInt) -> HalfInt {
        if self.owner_has(rules, GlobalEffect::WindMastery) {
            return moves.percent(150);
        }
        let anyone = self.wizards.iter().any(|w| {
            w.globals
                .iter()
                .any(|&s| GlobalEffect::WindMastery.is_in(rules.spells.get(s)))
        });
        if anyone {
            moves.percent(50)
        } else {
            moves
        }
    }

    fn derive(&self) -> Stats {
        let sheet = &self.sheet;
        let rules = self.rules();
        let def = rules.and_then(|r| self.def(r));
        let mut mods = sheet.modifiers.clone();
        let mut level = 0;
        if let (Some(rules), Some(def)) = (rules, def) {
            level = self.level_in(rules);
            if let Some(hero) = &sheet.hero {
                let magic = ranged_kind_of(&sheet.abilities, rules) == RangedKind::Magic;
                mods.extend(hero.abilities.modifiers(rules, level, magic));
            }
            mods.extend(Self::level_modifiers(def, rules, 0..level + 1));
        }
        if let Some(rules) = rules {
            mods.extend(self.global_modifiers(rules, def));
        }
        let stat = |s: Stat| compute(s, sheet.number(s), &mods);
        let typed = |s: Stat| {
            mods.iter()
                .filter(|m| m.stat == s && m.op == ModOp::Add && !m.types.is_empty())
                .map(|m| (m.value.floor(), m.types))
                .collect::<Vec<_>>()
        };
        let mut s = Stats::default();
        // Mithril adds one and adamantium two to the attacks and the defence.
        let quality = self.weapon_quality;
        let extra = i32::from(quality.max(1)) - 1;
        let physical_shot = rules.is_some_and(|r| weapon_shot_of(&sheet.abilities, r).is_some());
        let lucky = i32::from(sheet.abilities.contains(&Ability::new(39 /* lucky */)));
        let (melee, ranged) = (stat(Stat::Melee), stat(Stat::Ranged));
        s.melee = if melee > 0 { melee + extra } else { melee };
        s.ranged = if ranged > 0 && physical_shot {
            ranged + extra
        } else {
            ranged
        };
        s.defense = stat(Stat::Defense) + if quality > 0 { extra } else { 0 };
        s.resistance = stat(Stat::Resistance) + lucky;
        s.to_hit = stat(Stat::ToHit) + lucky;
        s.to_block = lucky;
        s.melee_plus = stat(Stat::MeleeToHit);
        s.ranged_plus = stat(Stat::RangedToHit);
        s.melee_parts = typed(Stat::Melee);
        // Only the arrows and stones carry such dice, not a spell shot (it gets no bonus).
        s.ranged_parts = if physical_shot {
            typed(Stat::Ranged)
        } else {
            Vec::new()
        };
        s.hits = stat(Stat::MaxHp).max(1);
        s.special_level = stat(Stat::SpecialLevel);
        s.ranged_kind = sheet.ranged_kind;
        s.abilities = sheet.abilities.clone();
        s.granted = sheet.granted.clone();
        s.enchantments = sheet.enchantments.clone();
        s.flags = sheet.flags;
        if let Some(rules) = rules {
            s.flags = sync_flags(s.flags, &s.abilities, rules);
            if !self.undead && self.owner_has(rules, GlobalEffect::HolyArms) {
                s.flags.insert(UnitFlag::MagicalWeapon);
            }
        }
        let mut moves = compute_half(
            Stat::Moves,
            sheet
                .base
                .get(&Stat::Moves)
                .copied()
                .unwrap_or(HalfInt::ZERO),
            &mods,
        )
        .max(HalfInt::ZERO);
        if let (Some(rules), Some(def)) = (rules, def) {
            let ship_or_flier =
                matches!(def.movement, MovementType::Sailing | MovementType::Flying)
                    || stat(Stat::Transport) > 0
                    || s.flags.contains(UnitFlag::Flies);
            if ship_or_flier {
                moves = self.wind_moves(rules, moves);
            }
        }
        s.moves = moves;
        if let (Some(rules), Some(hero)) = (rules, &sheet.hero) {
            s.leadership = rules.hero_value("leadership", hero.abilities.leadership, level);
            s.prayermaster = rules.hero_value("prayermaster", hero.abilities.prayermaster, level);
        }
        // What the pools can hold, before the battle's own spells change the unit.
        let mana_max = match (rules, &sheet.hero) {
            (Some(rules), Some(hero)) if hero.casting_skill > 0 => {
                (i32::from(hero.casting_skill) + 1)
                    * (level as i32 + 1)
                    * rules.config.wizard.hero_mana_percent
                    / 100
                    + hero.item_spell_skill
            }
            (Some(rules), _) => caster_mana_of(&sheet.abilities, rules),
            _ => 0,
        };
        let shot_mana = rules.map_or(1, |r| r.battle.shot_mana).max(1);
        let mut ammo_max = stat(Stat::Ammo);
        if sheet.ranged_kind == RangedKind::Magic && mana_max > 0 {
            ammo_max = mana_max / shot_mana;
        }
        // The battle's lasting spells, one after another.
        for l in &self.lasting {
            s.enchantments.push(l.spell);
            s.apply(&l.mods, &l.grants);
        }
        let wanted = self.field;
        s.melee = (s.melee + wanted.melee).max(0);
        s.ranged = (s.ranged + wanted.ranged).max(0);
        s.defense = (s.defense + wanted.defense).max(0);
        s.resistance = (s.resistance + wanted.resistance).max(0);
        s.to_hit += wanted.to_hit;
        s.to_block += wanted.to_block;
        if wanted.magical {
            s.flags.insert(UnitFlag::MagicalWeapon);
        }
        if let Some(rules) = rules {
            s.flags = sync_flags(s.flags, &s.abilities, rules);
        }
        // What the unit has lost, spent and used, on what it can hold now.
        // Life steal's extra hits sit on top of each figure's own, for this battle only; they are
        // not among the unit's hits. What the damage taken leaves is what it can hold now.
        let per_figure = s.hits + self.extra_hits;
        let hit_points = (self.figures_start as i32 * per_figure - self.lost).max(0);
        if hit_points == 0 || per_figure == 0 {
            s.figures = 0;
            s.front_damage = 0;
        } else {
            s.figures =
                (((hit_points + per_figure - 1) / per_figure) as u32).min(self.figures_start);
            s.front_damage = s.figures as i32 * per_figure - hit_points;
        }
        s.moves_left = (s.moves - self.moves_spent).max(HalfInt::ZERO);
        s.mana = mana_max - self.mana_used;
        let shoots_with_mana = s.ranged_kind == RangedKind::Magic
            && (sheet.hero.is_some_and(|h| h.casting_skill > 0)
                || rules.is_some_and(|r| caster_mana_of(&s.abilities, r) > 0));
        s.ammo = ammo_max - self.ammo_used;
        if shoots_with_mana {
            s.ammo = s.ammo.min(s.mana / shot_mana);
        }
        s.ammo = s.ammo.max(0);
        s
    }
}

/// The flags the battle reads, brought in line with the abilities. A web or sleep grounds a flyer.
fn sync_flags(mut flags: UnitFlags, abilities: &[Ability], rules: &Rules) -> UnitFlags {
    for &a in abilities {
        for &f in &rules.abilities.get(a).flags {
            flags.insert(f);
        }
    }
    if flags.contains(UnitFlag::Grounds) {
        flags = flags.without(UnitFlag::Flies);
    }
    flags
}
