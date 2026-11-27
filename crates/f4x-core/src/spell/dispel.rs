//! Dispelling: the one formula, the spells that use it, and Spell Blast.
//!
//! A dispel of strength `s` against a spell of difficulty `d` works with chance `s / (s + d)`
//! (the original rolls 1 to 250 against `250 s / (s + d)`). A spell's difficulty is its cost,
//! doubled again for each of its caster's masteries that applies: the mastery of the spell's
//! realm (nature, sorcery, chaos) and Archmage each add one more times the cost. A spell's
//! dispel strength is the mana put into it: overland its base cost and the extra mana the caster adds (`CastSpell::extra`), in a battle whatever the
//! wizard puts in (`battle/spells.rs`); the "True" spells count every point as three.
//!
//! Users: Dispel Magic (units), Disenchant Area and True (a map square), Disjunction and True
//! (an enemy wizard's global enchantment), Spell Binding (the same, but the caster takes the
//! spell), Suppress Magic (a standing strength-500 dispel against enemy overland spells),
//! Counter Magic (a battlefield reserve, `battle::Battle::counter_magic_stops`) and
//! nightshade (`city/nightshade.rs`). Spell Blast cancels the spell another wizard is casting.
//!
//! Tests: in spell/dispel/tests.rs.

use crate::world::rng::GameRng;
use rand::RngCore;

use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::exec::Target;
use crate::rules::SpellId;
use crate::spell::instance::SpellTarget;
use crate::spell::target_error::TargetError;
use crate::wizard::magic::MagicError;
use crate::world::{DispelSite, Event, World};
pub use f4x_battle::resistance::dispel_roll;
pub use f4x_def::rules::counter_effect::{CounterEffect, COUNTER_EFFECT};

/// Enchantments with the wizard who cast each.
pub(crate) type Spells = Vec<(SpellId, PlayerId)>;

impl World {
    /// A wizard turns off one of their own spells on a city (the city screen's list of
    /// enchantments): it ends, and so does its upkeep.
    pub fn turn_off_city_spell(
        &mut self,
        player: PlayerId,
        city: hecs::Entity,
        spell: SpellId,
    ) -> Result<Vec<Event>, crate::wizard::magic::MagicError> {
        if !self.turn_off_enchantment(SpellTarget::City(city), player, spell) {
            return Err(MagicError::Target(TargetError::SpellNotThere));
        }
        if crate::spell::city::CityEffect::SpellWard.is_in(self.content.rules.spells.get(spell)) {
            self.strip::<crate::spell::city::SpellWards>(city);
        }
        Ok(vec![Event::EnchantmentLapsed { player, spell }])
    }

    /// How many land units would drown if `spell` were turned off on `unit`: the question the
    /// sheet asks before it warns the wizard. Nothing is changed.
    pub fn drowned_if_off(&mut self, unit: hecs::Entity, spell: SpellId) -> usize {
        // The spell is taken off for a moment: the unit's list of instances goes without it.
        let held: Vec<hecs::Entity> = self
            .spells()
            .on(SpellTarget::Unit(unit))
            .into_iter()
            .filter(|&i| self.spells().def(i) == Some(spell))
            .collect();
        crate::spell::instance::set_listed(&mut self.ecs, unit, &held, false);
        let lost = self.economy().stranded().len();
        crate::spell::instance::set_listed(&mut self.ecs, unit, &held, true);
        lost
    }

    /// Turn off one of the wizard's own spells on a unit (`UV_Remove_Unit_Enchantment`). The
    /// unit may be left at sea without what kept it afloat; it drowns at once, and with it
    /// any other land unit of the square that no longer has what it needs.
    pub fn turn_off_unit_spell(
        &mut self,
        player: PlayerId,
        unit: hecs::Entity,
        spell: SpellId,
        rng: &mut GameRng,
    ) -> Result<Vec<Event>, crate::wizard::magic::MagicError> {
        if !self.turn_off_enchantment(SpellTarget::Unit(unit), player, spell) {
            return Err(MagicError::Target(TargetError::SpellNotThere));
        }
        let mut events = vec![Event::EnchantmentLapsed { player, spell }];
        events.extend(self.drown_stranded(None, rng));
        Ok(events)
    }

    /// How many times its cost a spell cast by `caster` resists: one, plus one for the
    /// caster's mastery of its realm and one for Archmage (`DispelMultiplier`).
    pub fn dispel_multiplier(
        &self,
        realm: Option<crate::rules::RealmId>,
        caster: Option<PlayerId>,
    ) -> u32 {
        let Some(m) = caster.and_then(|c| self.wizard(c)) else {
            return 1;
        };
        let facts = crate::rules::exec::Facts {
            realm,
            ..Default::default()
        };
        let times = crate::wizard::retort::stat_with(
            &self.content.rules,
            &m.retorts(),
            crate::rules::Stat::DispelMultiplier,
            1,
            &facts,
        );
        u32::try_from(times).unwrap_or(1)
    }

    /// How many times as strong the dispels `player` casts are (Runemaster: twice). The same for
    /// every wizard, human or AI, overland and in a battle; it is for Dispel Magic, Disenchant
    /// Area and Disjunction and their True versions, which count it on top of their own factor
    /// (the wiki: a Runemaster's Disenchant True works as six times the mana).
    pub fn dispel_power(&self, player: PlayerId) -> u32 {
        let Some(m) = self.wizard(player) else {
            return 1;
        };
        let times = crate::wizard::retort::stat_with(
            &self.content.rules,
            &m.retorts(),
            crate::rules::Stat::DispelPower,
            1,
            &crate::rules::exec::Facts::default(),
        );
        u32::try_from(times).unwrap_or(1).max(1)
    }

    /// The difficulty of dispelling `spell` cast by `caster` (`None` for no caster's retorts).
    pub fn dispel_difficulty(&self, spell: SpellId, caster: Option<PlayerId>) -> u32 {
        let def = self.content.rules.spells.get(spell);
        let base = if def.dispel_difficulty > 0 {
            def.dispel_difficulty
        } else {
            def.cost
        };
        base * self.dispel_multiplier(Some(def.realm), caster)
    }

    /// Try each enchantment in `list` cast by someone other than `by`; returns what is left
    /// and what was removed. A unit under another wizard's Spell Lock can only have the
    /// lock attacked, at difficulty 150.
    pub(crate) fn dispel_list(
        &self,
        list: Vec<(SpellId, PlayerId)>,
        strength: u32,
        by: PlayerId,
        guarded_by_lock: bool,
        rng: &mut dyn RngCore,
    ) -> (Spells, Spells) {
        let lock = self.content.rules.spells.id_numbered(54) /* spell_lock */;
        if guarded_by_lock {
            if let Some(&(_, caster)) = list.iter().find(|&&(s, c)| Some(s) == lock && c != by) {
                let difficulty = self
                    .content
                    .rules
                    .config
                    .spell_effects
                    .spell_lock_difficulty
                    * self.dispel_multiplier(
                        Some(crate::rules::RealmId::new(1) /* sorcery */),
                        Some(caster),
                    );
                if dispel_roll(&self.content.rules.battle, strength, difficulty, rng) {
                    let (gone, kept) = list.into_iter().partition(|&(s, _)| Some(s) == lock);
                    return (kept, gone);
                }
                return (list, Vec::new());
            }
        }
        let mut kept = Vec::new();
        let mut removed = Vec::new();
        for (spell, caster) in list {
            let difficulty = self.dispel_difficulty(spell, Some(caster));
            if caster != by && dispel_roll(&self.content.rules.battle, strength, difficulty, rng) {
                removed.push((spell, caster));
            } else {
                kept.push((spell, caster));
            }
        }
        (kept, removed)
    }

    /// A dispel of `strength` by wizard `by` on a unit, a city or everything on a map square
    /// (the city, the units, a warped node). Only enchantments cast by other wizards are
    /// attacked. Returns how many were removed, and notes each spell that went in `gone`.
    pub(crate) fn dispel_target(
        &mut self,
        who: Target,
        strength: u32,
        by: PlayerId,
        rng: &mut dyn RngCore,
        gone: &mut Vec<Event>,
    ) -> u32 {
        match who {
            // Spell Lock guards a unit's spells, not a city's.
            Target::Unit(u) => {
                self.dispel_enchantments(SpellTarget::Unit(u), strength, by, rng, gone)
            }
            Target::City(c) => {
                self.dispel_enchantments(SpellTarget::City(c), strength, by, rng, gone)
            }
            Target::Tile(at) => {
                let mut removed = self.dispel_node(at, strength, rng);
                removed += self.dispel_road(at, strength, rng);
                if let Some(c) = self.economy().city_at(at) {
                    removed += self.dispel_target(Target::City(c), strength, by, rng, gone);
                }
                for u in self.units().units_at(at) {
                    removed += self.dispel_target(Target::Unit(u), strength, by, rng, gone);
                }
                removed
            }
            _ => 0,
        }
    }

    /// A dispel attacks the enchantments on a unit or a city, cast by other wizards.
    fn dispel_enchantments(
        &mut self,
        target: SpellTarget,
        strength: u32,
        by: PlayerId,
        rng: &mut dyn RngCore,
        gone: &mut Vec<Event>,
    ) -> u32 {
        let on_unit = matches!(target, SpellTarget::Unit(_));
        let site = if on_unit {
            DispelSite::Unit
        } else {
            DispelSite::City
        };
        let list = self.spells().enchantments_on(target);
        let (_, removed) = self.dispel_list(list, strength, by, on_unit, rng);
        for &(spell, victim) in &removed {
            self.remove_enchantment(target, spell, victim);
        }
        gone.extend(
            removed
                .iter()
                .map(|&(spell, victim)| Event::EnchantmentDispelled {
                    victim,
                    spell,
                    site,
                }),
        );
        removed.len() as u32
    }

    /// Dispel an enchanted road on the square, against the difficulty of Enchant Road. The
    /// road keeps no caster, so any dispel may attack it (the original's cannot be dispelled
    /// at all; divergence 27). Myrror's roads are magic by the plane and stay so.
    fn dispel_road(&mut self, at: Position, strength: u32, rng: &mut dyn RngCore) -> u32 {
        let enchanted = self.map.as_ref().is_some_and(|m| {
            m.plane(at.plane)
                .has_enchanted_road(at.x as i32, at.y as i32)
        });
        if !enchanted {
            return 0;
        }
        let difficulty = self
            .content
            .rules
            .spells
            .id_numbered(55) /* enchant_road */
            .map_or(100, |s| self.dispel_difficulty(s, None));
        if !dispel_roll(&self.content.rules.battle, strength, difficulty, rng) {
            return 0;
        }
        if let Some(m) = self.map.as_mut() {
            m.plane_mut(at.plane).set_enchanted_road(at.x, at.y, false);
        }
        1
    }

    /// Dispel a warped node on the square, against the cost of Warp Node.
    fn dispel_node(&mut self, at: Position, strength: u32, rng: &mut dyn RngCore) -> u32 {
        let warp = self
            .content
            .rules
            .spells
            .iter()
            .find(|(_, d)| crate::spell::terraform::TerrainEffect::WarpNode.is_in(d))
            .map_or(300, |(_, s)| s.cost);
        let Some(node) = self.economy().node_at(at) else {
            return 0;
        };
        if node.warped && dispel_roll(&self.content.rules.battle, strength, warp, rng) {
            if let Ok(mut n) = self.ecs.get::<&mut crate::feature::node::Node>(node.entity) {
                n.warped = false;
            }
            return 1;
        }
        0
    }

    /// Disjunction and Spell Binding: attack the global enchantment `instance` of another
    /// wizard with a dispel of `strength`. When `steal` is set, a success hands the spell to `by`
    /// (the instance gets a new caster) instead of ending it. Returns whether it worked, or
    /// `None` if the instance is no global enchantment of another wizard.
    pub(crate) fn attack_global(
        &mut self,
        by: PlayerId,
        instance: hecs::Entity,
        strength: u32,
        steal: bool,
        rng: &mut dyn RngCore,
    ) -> Option<bool> {
        let spells = self.spells();
        if spells.target(instance)? != SpellTarget::Global {
            return None;
        }
        let (spell, victim) = (spells.def(instance)?, spells.caster_player(instance)?);
        if victim == by {
            return None;
        }
        let difficulty = self.dispel_difficulty(spell, Some(victim));
        // Spell Binding never fails when the setting says so (patch 1.50, note 269).
        let certain = steal && self.settings.rules.spell_binding_always_succeeds;
        if !certain && !dispel_roll(&self.content.rules.battle, strength, difficulty, rng) {
            return Some(false);
        }
        self.despawn_spell(instance);
        if steal {
            self.add_global(by, spell);
            self.reveal_for_globals(by);
        }
        Some(true)
    }

    /// Spell Blast: the mana `victim` has put into the spell being cast, if any.
    pub fn mana_spent_casting(&self, victim: PlayerId) -> Option<i32> {
        let c = self.wizard(victim)?.casting()?;
        Some((self.casting_cost(victim, c.spell, false) - c.remaining).max(0))
    }

    /// Spell Blast by `by` on `victim`: pays as much mana as the victim has spent on the spell
    /// being cast and ends it. Returns the spell and the mana paid, or `None` if the victim
    /// is casting nothing or `by` cannot pay.
    pub(crate) fn spell_blast(&mut self, by: PlayerId, victim: PlayerId) -> Option<(SpellId, i32)> {
        if victim == by {
            return None;
        }
        let spent = self.mana_spent_casting(victim)?;
        if self.player(by)?.treasury().mana < spent {
            return None;
        }
        let spell = self.wizard(victim)?.casting()?.spell;
        self.treasury_mut(by)?.mana -= spent;
        self.wizard_mut(victim)?.clear_casting();
        Some((spell, spent))
    }

    /// Disenchant Area needs something of another wizard's to attack on the square: an
    /// enchantment on the city or a unit there, a warped node or an enchanted road.
    pub fn check_dispel_area(&self, at: Position, by: PlayerId) -> Result<(), TargetError> {
        let theirs = |t: SpellTarget| {
            self.spells()
                .enchantments_on(t)
                .iter()
                .any(|&(_, caster)| caster != by)
        };
        let node = self.economy().node_at(at).is_some_and(|n| n.warped);
        let road = self.map.as_ref().is_some_and(|m| {
            m.plane(at.plane)
                .has_enchanted_road(at.x as i32, at.y as i32)
        });
        let city = self
            .economy()
            .city_at(at)
            .is_some_and(|c| theirs(SpellTarget::City(c)));
        let unit = self
            .units()
            .units_at(at)
            .into_iter()
            .any(|u| theirs(SpellTarget::Unit(u)));
        if node || road || city || unit {
            Ok(())
        } else {
            Err(TargetError::NothingToDispel)
        }
    }

    /// Another wizard's standing dispel of strength 500 against every overland spell cast:
    /// Suppress Magic against all spells, Life Force against death spells, Tranquility against
    /// chaos spells. Returns the wizard whose enchantment stopped `spell`, and that enchantment,
    /// if any. The dispel difficulty is read from `cost` when given: a crafting spell is
    /// resisted at the mana the item costs, not at the spell's own small cost (patch 1.50, note
    /// 270); `None`: the spell's own difficulty.
    pub(crate) fn suppressed_at_cost(
        &mut self,
        caster: PlayerId,
        spell: SpellId,
        cost: Option<u32>,
        rng: &mut GameRng,
    ) -> Option<(PlayerId, SpellId)> {
        if self.settings.rules.suppress_spares_return && spell == SpellId::new(214) {
            return None; // spell_of_return (patch IW114)
        }
        let realm = self.content.rules.spells.get(spell).realm.num();
        let guards: Vec<SpellId> = [
            SpellId::new(79),  /* suppress_magic */
            SpellId::new(156), /* life_force */
            SpellId::new(157), /* tranquility */
        ]
        .into_iter()
        .filter(|&g| match g.num() {
                156 /* life_force */ => realm == 4 /* death */,
                157 /* tranquility */ => realm == 2 /* chaos */,
                _ => true,
            })
        .collect();
        let holders: Vec<(PlayerId, SpellId)> = self
            .player_ids()
            .into_iter()
            .filter(|&p| p != caster)
            .flat_map(|p| {
                let has = &guards;
                has.iter()
                    .filter(|&&g| self.has_global(p, g))
                    .map(move |&g| (p, g))
                    .collect::<Vec<_>>()
            })
            .collect();
        let difficulty = match cost {
            Some(cost) => {
                cost * self.dispel_multiplier(
                    Some(self.content.rules.spells.get(spell).realm),
                    Some(caster),
                )
            }
            None => self.dispel_difficulty(spell, Some(caster)),
        };
        for (holder, guard) in holders {
            let chance =
                250 * u64::from(
                    self.content
                        .rules
                        .config
                        .spell_effects
                        .suppress_magic_strength,
                ) / u64::from(
                    self.content
                        .rules
                        .config
                        .spell_effects
                        .suppress_magic_strength
                        + difficulty,
                );
            if u64::from(rng.roll(250)) <= chance {
                return Some((holder, guard));
            }
        }
        None
    }
}

/// What a hook reports about the attack on a global enchantment.
pub(crate) fn global_attack_events(
    by: PlayerId,
    victim: PlayerId,
    spell: SpellId,
    result: Option<bool>,
    steal: bool,
) -> Vec<Event> {
    match result {
        Some(true) if steal => vec![Event::GlobalSpellTaken {
            player: by,
            victim,
            spell,
        }],
        Some(true) => vec![Event::GlobalSpellRemoved {
            player: by,
            victim,
            spell,
        }],
        Some(false) => vec![Event::DispelFailed { player: by, spell }],
        None => Vec::new(),
    }
}
