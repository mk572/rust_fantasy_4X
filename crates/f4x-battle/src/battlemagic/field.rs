//! The battlefield enchantments: spells that act on the whole field, each cast by one side. What they
//! do to a battle is in `f4x-core`'s `battle/field.rs`.
//!
//! Tests: in `f4x-core`'s battle/tests.rs, on a whole battle.

use f4x_def::half_int::HalfInt;
use f4x_def::rules::attack_nature::AttackNature;
use f4x_def::rules::damage_type::{DamageType, RangedKind};
use f4x_def::rules::effects::DamageTraits;
use f4x_def::rules::realm::Realm;
use f4x_def::rules::unit_flag::UnitFlag;
use f4x_def::rules::{Ability, SpellDef, SpellId};
use rand::{Rng, RngCore};

use crate::attack::{unblockable, Damage};
use crate::battle::{Battle, BattleEvent};
use crate::map::{self, Pos};
use crate::side::Side;
use crate::unit::UnitNow;
use crate::unit::{BattleUnit, Confusion, FieldBonus};

/// Hooks the spell data names.
pub const BATTLEFIELD: &str = "battlefield";
pub const DISINTEGRATE: &str = "disintegrate";

/// The battlefield enchantments.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, strum::EnumIter,
)]
pub enum Field {
    TrueLight,
    Darkness,
    WarpReality,
    BlackPrayer,
    Wrack,
    MetalFires,
    Prayer,
    HighPrayer,
    Terror,
    CallLightning,
    MassInvisibility,
    Entangle,
    ManaLeak,
    Blur,
}

impl Field {
    /// The argument of the `battlefield` hook for each enchantment, in the enum's order.
    pub const NAMES: &[&'static str] = &[
        "true_light",
        "darkness",
        "warp_reality",
        "black_prayer",
        "wrack",
        "metal_fires",
        "prayer",
        "high_prayer",
        "terror",
        "call_lightning",
        "mass_invisibility",
        "entangle",
        "mana_leak",
        "blur",
    ];

    /// The argument of the `battlefield` hook for this enchantment.
    pub fn name(self) -> &'static str {
        match self {
            Field::TrueLight => "true_light",
            Field::Darkness => "darkness",
            Field::WarpReality => "warp_reality",
            Field::BlackPrayer => "black_prayer",
            Field::Wrack => "wrack",
            Field::MetalFires => "metal_fires",
            Field::Prayer => "prayer",
            Field::HighPrayer => "high_prayer",
            Field::Terror => "terror",
            Field::CallLightning => "call_lightning",
            Field::MassInvisibility => "mass_invisibility",
            Field::Entangle => "entangle",
            Field::ManaLeak => "mana_leak",
            Field::Blur => "blur",
        }
    }

    /// The enchantment named by a hook argument.
    pub fn named(name: &str) -> Option<Field> {
        match name {
            "true_light" => Some(Field::TrueLight),
            "darkness" => Some(Field::Darkness),
            "warp_reality" => Some(Field::WarpReality),
            "black_prayer" => Some(Field::BlackPrayer),
            "wrack" => Some(Field::Wrack),
            "metal_fires" => Some(Field::MetalFires),
            "prayer" => Some(Field::Prayer),
            "high_prayer" => Some(Field::HighPrayer),
            "terror" => Some(Field::Terror),
            "call_lightning" => Some(Field::CallLightning),
            "mass_invisibility" => Some(Field::MassInvisibility),
            "entangle" => Some(Field::Entangle),
            "mana_leak" => Some(Field::ManaLeak),
            "blur" => Some(Field::Blur),
            _ => None,
        }
    }

    /// The enchantment a spell puts up: the argument of its `battlefield` hook.
    pub fn of(def: &SpellDef) -> Option<Field> {
        Field::named(def.hook_args(BATTLEFIELD)?.first()?)
    }
}

impl Battle {
    /// Whether `side` has put up the enchantment.
    pub fn has_field(&self, side: Side, field: Field) -> bool {
        self.fields[side.index()].iter().any(|&(f, _)| f == field)
    }

    /// Whether the unit is spared by the spell: it is immune to a type the spell harms with
    /// (`harm_types`), less the types the spell is an exception for while the rule for it is on
    /// (`death_immunity_exceptions`, `magic_immunity_exceptions`).
    pub fn spares(&self, u: &BattleUnit, spell: SpellId) -> bool {
        let types = self.content.rules.spells.get(spell).harmful_types(
            self.death_immunity_exceptions,
            self.magic_immunity_exceptions,
        );
        u.is_immune_to_set(types, &self.content.rules)
    }

    /// Whether the unit is spared by the battlefield enchantment `field` (the spell that raises it).
    pub fn field_spares(&self, u: &BattleUnit, field: Field) -> bool {
        self.content
            .rules
            .spells
            .iter()
            .filter(|(_, def)| Field::of(def) == Some(field))
            .any(|(id, _)| self.spares(u, id))
    }

    /// Whether either side has put it up.
    pub fn has_field_either(&self, field: Field) -> bool {
        self.has_field(Side::Attacker, field) || self.has_field(Side::Defender, field)
    }

    /// The enchantments on the field, with who cast each and the spell that did.
    pub fn fields(&self) -> impl Iterator<Item = (Side, Field, SpellId)> + '_ {
        [Side::Attacker, Side::Defender]
            .into_iter()
            .flat_map(|side| {
                self.fields[side.index()]
                    .iter()
                    .map(move |&(f, s)| (side, f, s))
            })
    }

    /// Put an enchantment up for `side`; false if it already has it.
    pub fn raise_field(&mut self, side: Side, field: Field, spell: SpellId) -> bool {
        if self.has_field(side, field) {
            return false;
        }
        self.fields[side.index()].push((field, spell));
        self.note_invisibility();
        true
    }

    /// Take an enchantment of `side` down, and its changes off the units. Returns the spell that
    /// had put it up, or `None` if `side` had not.
    pub fn take_down_field(&mut self, side: Side, field: Field) -> Option<SpellId> {
        let list = &mut self.fields[side.index()];
        let at = list.iter().position(|&(f, _)| f == field)?;
        let (_, spell) = list.remove(at);
        self.note_invisibility();
        Some(spell)
    }

    /// What the battlefield spells do to this unit's numbers now.
    pub fn field_bonus_for(&self, u: &BattleUnit) -> FieldBonus {
        let mine = u.side;
        let fields = &self.rules().fields;
        let own = |f| self.has_field(mine, f);
        let theirs = |f| self.has_field(mine.other(), f);
        let mut b = FieldBonus::default();
        // Light and darkness: life and death creatures gain and lose by turns.
        let light = i32::from(
            self.has_field_either(Field::TrueLight) && !self.field_spares(u, Field::TrueLight),
        );
        let dark = i32::from(
            self.has_field_either(Field::Darkness) && !self.field_spares(u, Field::Darkness),
        );
        let swing = match u.realm {
            Some(Realm::Life) => light - dark,
            Some(Realm::Death) => dark - light,
            _ => 0,
        };
        for _ in 0..swing.abs() {
            let step = swing.signum() * fields.light_and_darkness;
            // A gain only helps an attack the creature has; a loss is felt anyway.
            b.melee += if step > 0 && u.own_melee <= 0 {
                0
            } else {
                step
            };
            b.ranged += if step > 0 && u.own_ranged <= 0 {
                0
            } else {
                step
            };
            b.defense += step;
            b.resistance += step;
        }
        if self.has_field_either(Field::WarpReality)
            && u.realm != Some(Realm::Chaos)
            && !self.field_spares(u, Field::WarpReality)
        {
            b.add(fields.warp_reality, true, true);
        }
        if theirs(Field::BlackPrayer) && !self.field_spares(u, Field::BlackPrayer) {
            b.add(fields.black_prayer, true, true);
        }
        // A gain only helps an attack the creature has.
        if own(Field::HighPrayer) {
            b.add(fields.high_prayer, u.own_melee > 0, u.own_ranged > 0);
        } else if own(Field::Prayer) {
            b.add(fields.prayer, u.own_melee > 0, u.own_ranged > 0);
        }
        if own(Field::MetalFires)
            && !u.is_fantastic()
            && !u.enchantments.contains(&SpellId::new(94) /* flame_blade */)
        {
            let shoots = u.weapon_shot(&self.content.rules).is_some();
            b.add(fields.metal_fires, u.own_melee > 0, shoots);
            b.magical = true;
        }
        // Holy Bonus and Resistance to All: the highest level among the side's standing units,
        // never added up. Holy Bonus gives attack (melee), defence and resistance, and counts
        // towards resistance to all (`Update_Battlefield_Bonuses`, `COMBINIT.c`).
        let level = |ability: Ability| {
            self.units
                .iter()
                .filter(|x| x.side == mine && x.is_active() && x.has(ability))
                .map(|x| x.special_level)
                .max()
                .unwrap_or(0)
        };
        let holy = level(Ability::new(28 /* holy_bonus */));
        let best = |f: fn(&BattleUnit) -> i32| {
            self.units
                .iter()
                .filter(|x| x.side == mine && x.is_active())
                .map(f)
                .max()
                .unwrap_or(0)
        };
        // A hero's Prayermaster counts as Resistance to All; the better of the two applies.
        let resist_all = level(Ability::new(56 /* resist_all */)).max(best(|x| x.prayermaster));
        if holy > 0 && u.own_melee > 0 {
            b.melee += holy;
        }
        // Leadership: the best hero's bonus to the melee (and a part of it to the ranged, but not
        // magic) attack of every normal unit, not the fantastic ones or the undead.
        let lead = best(|x| x.leadership);
        if lead > 0 && !u.is_fantastic() && !u.undead {
            if u.own_melee > 0 {
                b.melee += lead;
            }
            if u.own_ranged > 0 && u.ranged_kind != RangedKind::Magic {
                b.ranged += lead * fields.leadership_ranged_percent / 100;
            }
        }
        b.defense += holy;
        b.resistance += holy + resist_all;
        // A node's aura: its own realm's creatures fight harder, on either side.
        if u.realm.is_some() && u.realm == self.node_aura {
            b.melee += self.rules().node_aura_bonus;
            if u.own_ranged > 0 {
                b.ranged += self.rules().node_aura_bonus;
            }
            b.defense += self.rules().node_aura_bonus;
            b.resistance += self.rules().node_aura_bonus;
        }
        b.invisible = own(Field::MassInvisibility);
        b.blur = own(Field::Blur);
        b
    }

    /// A unit as it stands in this battle now: its own numbers with what the battlefield adds.
    pub fn now(&self, id: usize) -> Option<UnitNow<'_>> {
        Some(self.now_of(self.unit(id)?))
    }

    /// [`Battle::now`] for a unit of this battle that is at hand.
    pub fn now_of<'a>(&'a self, unit: &'a BattleUnit) -> UnitNow<'a> {
        UnitNow::new(unit, self.field_bonus_for(unit))
    }

    /// Invisibility that begins now (a field or a spell) hides the unit at once, whatever it
    /// showed before: call it after anything that can make a unit invisible.
    pub fn note_invisibility(&mut self) {
        for i in 0..self.units.len() {
            let invisible = self.now_of(&self.units[i]).is_invisible();
            let u = &mut self.units[i];
            if invisible && !u.was_invisible {
                u.revealed = false;
            }
            u.was_invisible = invisible;
        }
    }

    /// What happens when a new turn begins (the attacker's start): Mana Leak, Call Lightning and
    /// Wrack, for each side that has put them up.
    pub fn turn_effects(&mut self) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        for caster in [Side::Attacker, Side::Defender] {
            if self.has_field(caster, Field::ManaLeak) {
                events.extend(self.mana_leak(caster.other()));
            }
        }
        for caster in [Side::Attacker, Side::Defender] {
            if self.has_field(caster, Field::CallLightning) {
                events.extend(self.call_lightning(caster));
            }
        }
        for caster in [Side::Attacker, Side::Defender] {
            if self.has_field(caster, Field::Wrack) {
                events.extend(self.wrack(caster));
            }
        }
        events.extend(self.vortex_round());
        events
    }

    /// The side's wizard loses mana and its units with spell shots lose a shot.
    pub fn mana_leak(&mut self, victim: Side) -> Vec<BattleEvent> {
        let drain = self.rules().mana_leak_drain;
        if let Some(c) = self.casters[victim.index()].as_mut() {
            c.mana = (c.mana - drain).max(HalfInt::default());
        }
        for u in self
            .units
            .iter_mut()
            .filter(|u| u.side == victim && u.is_active())
        {
            u.leak_mana(drain, &self.content.rules);
        }
        vec![BattleEvent::ManaLeaked { side: victim }]
    }

    /// Bolts of lightning strike random enemy units: how many, and the odds that a try uses the
    /// unit it lands on, are `fields.call_lightning` of `battle.ron`.
    pub fn call_lightning(&mut self, caster: Side) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        let rules = self.rules().fields.call_lightning;
        let mut bolts = self
            .rng
            .gen_range(rules.least_bolts as i32..=rules.most_bolts as i32);
        let mut tries = 0;
        let traits = DamageTraits {
            armor_piercing: true,
            ..Default::default()
        };
        let nature = AttackNature::magical(Realm::Nature, &[DamageType::Lightning]);
        while bolts > 0 && tries < rules.tries && !self.units.is_empty() {
            tries += 1;
            let id = self.rng.gen_range(0..self.units.len());
            let u = &self.units[id];
            if u.side == caster
                || !u.is_active()
                || self.rng.gen_range(1..=rules.one_in.max(1) as i32) != 1
            {
                continue;
            }
            let dmg = self.spell_damage(id, self.rules().call_lightning_strength, &traits, nature);
            events.push(BattleEvent::LightningStruck { unit: id });
            events.extend(self.spell_events_for_damage(id, dmg, nature.types));
            bolts -= 1;
            tries = 0;
        }
        events
    }

    /// Every figure of every enemy unit without death immunity must resist (`wrack_modifier`) or
    /// lose hit points (`wrack_damage`). Wraith Form gives no shelter: the original's code skips such units and marks that
    /// as a bug, and Call Lightning's identical skip is fixed by the Unofficial Patch.
    pub fn wrack(&mut self, caster: Side) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        for id in 0..self.units.len() {
            let u = &self.units[id];
            if u.side == caster || !u.is_active() || self.field_spares(u, Field::Wrack) {
                continue;
            }
            let mut state = u.defender_state(&self.content.rules, self.field_bonus_for(u));
            let rng: &mut dyn RngCore = &mut self.rng;
            let battle = &self.content.rules.battle;
            let mut lost = Damage::default();
            for _ in 0..state.figures {
                let modifier = battle.fields.wrack_modifier;
                if state.resistance_failure(battle, Some(Realm::Death), modifier, rng) > 0 {
                    lost += unblockable(&mut state, battle.fields.wrack_damage);
                }
            }
            if lost.points > 0 {
                self.units[id].set_from(&state);
                self.units[id].record(lost);
                events.push(BattleEvent::FieldStruck {
                    unit: id,
                    dealt: lost.points,
                    kills: lost.kills,
                });
                if !self.units[id].is_active() {
                    events.push(BattleEvent::UnitDied { unit: id });
                }
            }
        }
        events
    }

    /// What the side's enchantments from the other side do as its turn begins: Terror and
    /// Entangle.
    pub fn side_turn_effects(&mut self, side: Side) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        let terror = self.has_field(side.other(), Field::Terror);
        let entangle = self.has_field(side.other(), Field::Entangle);
        for id in 0..self.units.len() {
            if self.units[id].side != side || !self.units[id].is_active() {
                continue;
            }
            // Death immunity "negates all spells which cause death and fear" (the help; the
            // original's Terror ignores it, and `death_immunity_exceptions` brings that back).
            if terror && !self.field_spares(&self.units[id], Field::Terror) {
                let state = self.units[id]
                    .defender_state(&self.content.rules, self.field_bonus_for(&self.units[id]));
                let rng: &mut dyn RngCore = &mut self.rng;
                let battle = &self.content.rules.battle;
                let modifier = battle.fields.terror_modifier;
                if state.resistance_failure(battle, Some(Realm::Death), modifier, rng) > 0 {
                    let u = &mut self.units[id];
                    u.end_moves();
                    u.attacked = true;
                    events.push(BattleEvent::Terrified { unit: id });
                }
            }
            // Entangle holds only walkers with a body: the help says so.
            if entangle
                && !self.field_spares(&self.units[id], Field::Entangle)
                && !self.units[id].flag(UnitFlag::Flies)
                && !self.units[id].flag(UnitFlag::NonCorporeal)
            {
                let moves = self.content.rules.battle.fields.entangle_moves;
                self.units[id].spend_moves(moves);
            }
            // A unit in a web spends its whole turn striking at it.
            if self.units[id].held_by_web(&self.content.rules) {
                let strength = self
                    .now(id)
                    .map_or(0, |u| u.web_strength(&self.content.rules));
                let u = &mut self.units[id];
                let dealt = strength.min(u.web_hits);
                u.web_hits -= dealt;
                u.end_moves();
                u.attacked = true;
                events.push(BattleEvent::WebStruck {
                    unit: id,
                    dealt,
                    broken: u.web_hits == 0,
                });
            }
            events.extend(self.regenerate(id));
            if self.units[id]
                .enchantments
                .contains(&SpellId::new(165) /* black_sleep */)
            {
                self.units[id].attacked = true;
            }
            events.extend(self.confusion(id));
        }
        // The enemy's units that confusion has put in this side's hands (they go back as the
        // next turn begins).
        for u in &mut self.units {
            if u.seized && u.side == side.other() && u.is_active() {
                u.seized = false;
                u.turned_from = Some(u.side);
                u.side = side;
                u.refill_moves();
                u.attacked = false;
            }
        }
        events
    }

    /// A confused unit does one of four things each turn, each as likely: nothing, a move to a
    /// random free square, the enemy's orders, or its own business as usual.
    pub fn confusion(&mut self, id: usize) -> Option<BattleEvent> {
        if !self.units[id]
            .enchantments
            .contains(&SpellId::new(46) /* confusion */)
        {
            return None;
        }
        let act = match self.rng.gen_range(0..4) {
            0 => Confusion::Idle,
            1 => Confusion::Wandered,
            2 => Confusion::Seized,
            _ => return None,
        };
        if act == Confusion::Wandered {
            let free: Vec<Pos> = (0..map::GRID_H)
                .flat_map(|y| (0..map::GRID_W).map(move |x| (x, y)))
                .filter(|&p| {
                    self.map.enter_cost(p, false).is_some()
                        && !self.map.is_shut_tower(p)
                        && self.unit_at(p).is_none()
                })
                .collect();
            if !free.is_empty() {
                let to = free[self.rng.gen_range(0..free.len())];
                self.units[id].cell = to;
            }
        }
        let u = &mut self.units[id];
        u.end_moves();
        u.attacked = true;
        u.seized = act == Confusion::Seized;
        Some(BattleEvent::Confused { unit: id, act })
    }

    /// A regenerating unit (the ability, or the Regeneration spell) heals as its side's turn
    /// begins (`regeneration_heals`).
    pub fn regenerate(&mut self, id: usize) -> Option<BattleEvent> {
        let heals = self.content.rules.battle.regeneration_heals;
        let u = &mut self.units[id];
        if !u.regenerates() {
            return None;
        }
        let healed = u.heal_points(heals, false);
        (healed > 0).then_some(BattleEvent::SpellHealed { unit: id, healed })
    }
}

#[cfg(test)]
mod name_tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    pub fn the_names_the_hook_table_lists_are_those_of_the_variants_in_order() {
        let names: Vec<_> = Field::iter().map(Field::name).collect();
        assert_eq!(names, Field::NAMES);
        for field in Field::iter() {
            assert_eq!(Field::named(field.name()), Some(field));
        }
        assert_eq!(Field::named("no such field"), None);
    }
}
