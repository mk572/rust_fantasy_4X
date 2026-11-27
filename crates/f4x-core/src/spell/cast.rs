//! Casting overland spells: targets, the effect host over the ECS, and lasting spells.
//!
//! Tests: in rules/exec/tests.rs, wizard/magic_world/tests.rs and spell/wizard/tests.rs.

use crate::rules::Ability;
use f4x_battle::resistance::resistance_failure;
use rand::RngCore;

use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::damage_type::DamageTypes;
use crate::rules::effects::Trigger;
use crate::rules::exec::{run_rules, Context, EffectHost, Facts, Outcome, Target};
use crate::rules::{Duration, Modifier, Source, SpellId, SpellTarget, Stat, UnitId};
use crate::spell::instance::SpellTarget as Placed;
use crate::spell::target_error::TargetError;
use crate::unit::Hurt;
use crate::wizard::magic::MagicError;
use crate::world::rng::GameRng;
use crate::world::{Event, World};
use f4x_def::rules::realm::Realm;

/// Something a wizard pays mana for every turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UpkeepKind {
    Global(SpellId),
    City(hecs::Entity, SpellId),
    UnitSpell(hecs::Entity, SpellId),
    Unit(hecs::Entity),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UpkeepItem {
    pub kind: UpkeepKind,
    pub mana: i32,
    /// What it takes to replace it: the spell's mana to cast, the unit's production cost (of
    /// two items that cost the same upkeep, the cheaper to replace goes first).
    pub cost: i32,
}

/// The real world, as the effect executor sees it.
struct WorldHost<'a> {
    world: &'a mut World,
    /// The game's generator, for the executor and the hooks, which draw from one stream.
    rng: &'a mut GameRng,
    /// What hooks and other changes report back to the event log.
    events: Vec<Event>,
    /// Units destroyed by the spell's strikes (`strike`), reported once the spell is done.
    slain: u32,
    /// Mana put into the spell beyond its cost: it adds to a dispel's strength.
    extra: u32,
    /// The strength those mana add to a dispel: one a mana, three for a True spell.
    boost: u32,
    /// The summoning circle Word of Recall sends its unit to, when the wizard chose one.
    recall_to: Option<Position>,
}

impl WorldHost<'_> {
    /// What a spell with the `terrain_effect` hook does to the square.
    fn terrain_effect(
        &mut self,
        effect: crate::spell::terraform::TerrainEffect,
        at: Position,
        owner: PlayerId,
    ) {
        use crate::spell::terraform::TerrainEffect as T;
        let events = match effect {
            T::RaiseVolcano => self.world.raise_volcano(owner, at, &mut *self.rng),
            T::ChangeTerrain => self.world.change_terrain_spell(at),
            T::EnchantRoad => self.world.enchant_roads(at),
            T::Corruption => self.world.corrupt(at),
            T::WarpNode => self.world.warp_node(at),
            T::Transmute => self.world.transmute(at),
            T::FloatingIsland => {
                self.world.floating_island(owner, at);
                return;
            }
            T::EarthLore => {
                self.world.earth_lore(owner, at);
                return;
            }
        };
        self.events.extend(events);
    }
}

impl EffectHost for WorldHost<'_> {
    fn facts(&self, who: Target) -> Facts {
        let w = &*self.world;
        let mut facts = Facts::default();
        if let Target::Unit(u) = who {
            if let Some(def) = w.units().unit_def(u) {
                facts.abilities = w.units().unit_abilities(u);
                facts.realm = w.units().unit_realm(u);
                facts.kind = Some(if w.units().unit_is_undead(u) {
                    crate::rules::UnitKind::Summoned
                } else {
                    def.kind
                });
            }
        }
        facts
    }

    fn position(&self, who: Target) -> Option<Position> {
        match who {
            Target::Unit(u) | Target::UnitAt(u, _) => self.world.units().unit_position(u),
            Target::City(c) | Target::CityRealm(c, _) => {
                self.world.ecs.get::<&Position>(c).ok().map(|p| *p)
            }
            Target::Tile(p) => Some(p),
            // A wizard is "at" their summoning circle.
            Target::Player(p) => self.world.summon_point(p),
            Target::Spell(i) => self
                .world
                .spells()
                .caster_player(i)
                .and_then(|p| self.world.summon_point(p)),
            Target::Hero(_) | Target::HeroAt(..) => None,
        }
    }

    fn owner_of(&self, who: Target) -> Option<PlayerId> {
        match who {
            Target::Unit(u) | Target::UnitAt(u, _) => self.world.units().unit_owner(u),
            Target::City(c) | Target::CityRealm(c, _) => self.world.economy().city_owner(c),
            Target::Player(p) => Some(p),
            Target::Spell(i) => self.world.spells().caster_player(i),
            Target::Tile(_) | Target::Hero(_) | Target::HeroAt(..) => None,
        }
    }

    fn units(&self) -> Vec<hecs::Entity> {
        self.world.units().all_units()
    }

    fn city_at(&self, pos: Position) -> Option<hecs::Entity> {
        self.world.economy().city_at(pos)
    }

    fn stat(&self, who: Target, stat: Stat) -> i32 {
        match who {
            Target::Unit(u) => self.world.units().unit_stat(u, stat),
            _ => 0,
        }
    }

    fn resistance_against(&self, who: Target, realm: Option<Realm>) -> i32 {
        match who {
            Target::Unit(u) => self.world.units().unit_resistance_against(u, realm),
            _ => 0,
        }
    }

    fn protected_from(&self, who: Target, realm: Option<Realm>) -> bool {
        match who {
            Target::Unit(u) => self.world.units().unit_protected_from(u, realm),
            _ => false,
        }
    }

    /// Bare modifiers and abilities from instant spells and rules go to the bearer's
    /// temporary effects (`spell/temp_effect.rs`); lasting spells use enchantments instead.
    fn add_modifier(&mut self, who: Target, modifier: Modifier) {
        self.world.add_temp_modifier(who, modifier);
    }

    fn immune_to(&self, who: Target, types: DamageTypes) -> bool {
        let Target::Unit(u) = who else {
            return false;
        };
        f4x_def::rules::immunity::Immunities::of(
            &self.world.units().unit_abilities(u),
            &self.world.units().unit_enchantment_ids(u),
            &self.world.content.rules,
        )
        .stops(types)
    }

    fn grant_ability(&mut self, who: Target, ability: Ability) {
        self.world.grant_temp_ability(who, ability);
    }

    fn damage(&mut self, who: Target, amount: i32, _types: DamageTypes) -> i32 {
        let Target::Unit(u) = who else {
            return 0;
        };
        let (left, _) = self.world.units().unit_hits(u);
        let dealt = amount.max(0).min(left as i32);
        let hurt = self.world.ecs.get::<&Hurt>(u).map_or(0, |h| h.0);
        self.world.put(u, Hurt(hurt + dealt as u32));
        if dealt as u32 >= left {
            self.world.remove_unit(u, crate::hero::Fate::Slain);
        }
        dealt
    }

    fn strike(
        &mut self,
        who: Target,
        strength: i32,
        types: DamageTypes,
        _realm: Option<f4x_def::rules::realm::Realm>,
        traits: &crate::rules::effects::DamageTraits,
    ) -> i32 {
        let Target::Unit(u) = who else {
            return 0;
        };
        if !traits.area {
            return self.damage(who, strength, types);
        }
        let nature = f4x_def::rules::attack_nature::AttackNature {
            types,
            armor_piercing: traits.armor_piercing,
        };
        let before = self.world.units().unit_hits(u).0 as i32;
        if self
            .world
            .spell_hit_overland_unit(u, strength, traits, nature, &mut *self.rng)
        {
            self.slain += 1;
            return before;
        }
        before - self.world.units().unit_hits(u).0 as i32
    }

    fn heal(&mut self, who: Target, amount: i32) -> i32 {
        let Target::Unit(u) = who else {
            return 0;
        };
        let hurt = self.world.ecs.get::<&Hurt>(u).map_or(0, |h| h.0);
        let healed = amount.max(0).min(hurt as i32);
        self.world.put(u, Hurt(hurt - healed as u32));
        healed
    }

    fn summon(&mut self, owner: PlayerId, at: Position, id: UnitId, count: u8) {
        // Summoned creatures appear at the summoning circle (the one aimed at, when there are several);
        // without one, where aimed.
        let at = if self.world.summon_circles(owner).contains(&at) {
            at
        } else {
            self.world.summon_point(owner).unwrap_or(at)
        };
        for _ in 0..count {
            self.world.spawn_unit(owner, at, id);
        }
    }

    fn resistance_failure(&mut self, resistance: i32, modifier: i32) -> i32 {
        resistance_failure(
            &self.world.content.rules.battle,
            resistance,
            modifier,
            &mut *self.rng,
        )
    }

    fn rng(&mut self) -> &mut dyn RngCore {
        &mut *self.rng
    }

    fn custom(&mut self, hook: &str, args: &[String], who: Target, ctx: &Context) {
        // The hooks several spells share. Most are carried out by the engine wherever the
        // wizard's global spells are consulted.
        if hook == crate::item::crafting::CRAFT_ITEM {
            let events = self.world.complete_crafting(ctx.owner);
            self.events.extend(events);
        }
        if hook == crate::spell::special::RECALL
            && args
                .iter()
                .any(|a| a == crate::spell::special::RECALL_ANYONE)
        {
            if let Target::Unit(unit) = who {
                // `resolve_cast` checked the circle just before: a failure here is a bug.
                let sent = self.world.word_of_recall(ctx.owner, unit, self.recall_to);
                debug_assert!(
                    sent.is_ok(),
                    "Word of Recall failed after its check: {sent:?}"
                );
            }
        }
        if hook == crate::spell::special::PLANE_SHIFT {
            if let Target::Unit(unit) = who {
                let shifted = self.world.plane_shift(ctx.owner, unit);
                self.events.extend(shifted);
            }
        }
        if hook == crate::spell::special::MOVE_BUILDING {
            if let Target::City(city) = who {
                let result = if args
                    .iter()
                    .any(|a| a == crate::spell::special::MOVE_FORTRESS)
                {
                    self.world.move_fortress(ctx.owner, city)
                } else {
                    self.world.move_summoning_circle(ctx.owner, city)
                };
                self.building_moved(result);
            }
        }
        if hook == crate::spell::special::SPELL_OF_MASTERY {
            self.world.complete_mastery(ctx.owner);
        }
        if hook == crate::spell::special::SPELL_OF_RETURN {
            if let Target::City(city) = who {
                let result = self.world.return_to_fortress(ctx.owner, city);
                self.building_moved(result);
            }
        }
        if hook == crate::spell::wizard::CITY_STRIKE || hook == crate::spell::wizard::WORLD_KILL {
            if let Source::Spell(spell) = ctx.source {
                self.strike(hook, args, spell, who, ctx.owner);
            }
        }
        if hook == crate::spell::dispel::COUNTER_EFFECT {
            if let Source::Spell(spell) = ctx.source {
                self.counter_effect(spell, who, ctx.owner);
            }
        }
        if hook == crate::spell::hero::SUMMON_HERO {
            if let Source::Spell(spell) = ctx.source {
                if let Some(events) = self.world.hero_hook(spell, ctx.owner, who, &mut *self.rng) {
                    self.events.extend(events);
                }
            }
        }
        if hook == crate::spell::unit_effect::UNIT_EFFECT {
            for effect in args
                .iter()
                .filter_map(|a| crate::spell::unit_effect::UnitEffect::named(a))
            {
                self.unit_effect(effect, who, ctx.owner);
            }
        }
        if hook == crate::spell::wizard::WIZARD_EFFECT {
            if let (Some(effect), Target::Player(victim)) = (
                args.first()
                    .and_then(|a| crate::spell::wizard::WizardEffect::named(a)),
                who,
            ) {
                self.wizard_effect(effect, victim, ctx.owner);
            }
        }
        if hook == crate::hook::WALL && args.iter().any(|a| a == "stone") {
            if let Target::City(city) = who {
                let events = self.world.wall_of_stone(city);
                self.events.extend(events);
            }
        }
        if hook == crate::spell::terraform::TERRAIN_EFFECT {
            if let (Some(effect), Target::Tile(at)) = (
                args.first()
                    .and_then(|a| crate::spell::terraform::TerrainEffect::named(a)),
                who,
            ) {
                self.terrain_effect(effect, at, ctx.owner);
            }
        }
    }
}

impl WorldHost<'_> {
    /// Other wizards' enchantments on the target are removed with chance
    /// `strength / (strength + difficulty)` (see `spell/dispel.rs`).
    fn dispel(&mut self, who: Target, strength: u32, by: PlayerId) -> u32 {
        let strength = (strength + self.boost) * self.world.dispel_power(by);
        let mut gone = Vec::new();
        let removed = self
            .world
            .dispel_target(who, strength, by, &mut *self.rng, &mut gone);
        self.events.extend(gone);
        removed
    }

    /// What a spell with the `city_strike` or `world_kill` hook does.
    fn strike(
        &mut self,
        hook: &str,
        args: &[String],
        spell: SpellId,
        who: Target,
        player: PlayerId,
    ) {
        use crate::spell::wizard::{CityStrike, WorldKill, CITY_STRIKE};
        let name = args.first().map(String::as_str).unwrap_or_default();
        let events = if hook == CITY_STRIKE {
            let Target::City(city) = who else {
                return;
            };
            match CityStrike::named(name) {
                Some(CityStrike::Earthquake) => {
                    // `who` was checked to be a city when the spell was chosen.
                    self.world
                        .earthquake(city, &mut *self.rng)
                        .into_iter()
                        .collect()
                }
                Some(CityStrike::CallTheVoid) => {
                    let (units_lost, buildings_lost, population_lost) =
                        self.world.call_the_void(city, &mut *self.rng);
                    vec![Event::WorldSpellStruck {
                        spell,
                        victim: self.world.economy().city_owner(city),
                        units_lost,
                        buildings_lost,
                        population_lost,
                    }]
                }
                None => Vec::new(),
            }
        } else {
            let count = match WorldKill::named(name) {
                Some(WorldKill::GreatUnsummoning) => self.world.great_unsummoning(&mut *self.rng),
                Some(WorldKill::DeathWish) => self.world.death_wish(player, &mut *self.rng),
                None => return,
            };
            vec![Event::UnitsSlain {
                player,
                spell,
                count,
            }]
        };
        self.events.extend(events);
    }

    /// What a spell with the `counter_effect` hook does against the spell, unit or square aimed
    /// at. Counter Magic is a battle spell and is carried out there.
    fn counter_effect(&mut self, spell: SpellId, who: Target, by: PlayerId) {
        use crate::spell::dispel::{global_attack_events, CounterEffect as C};
        let def = self.world.content.rules.spells.get(spell);
        let Some((effect, number)) = C::of(def) else {
            return;
        };
        let cost = def.cost + self.extra;
        match effect {
            C::Dispel => {
                let removed = self.dispel(who, number, by);
                self.events.push(Event::Dispelled {
                    player: by,
                    removed,
                });
            }
            C::Disjunction | C::SpellBinding => {
                let steal = effect == C::SpellBinding;
                let Target::Spell(instance) = who else {
                    return;
                };
                let spells = self.world.spells();
                let (Some(victim), Some(attacked)) =
                    (spells.caster_player(instance), spells.def(instance))
                else {
                    return;
                };
                // Disjunction (not Spell Binding) is a Runemaster's dispel, twice as strong.
                let power = if steal {
                    1
                } else {
                    self.world.dispel_power(by)
                };
                let result = self.world.attack_global(
                    by,
                    instance,
                    cost * number.max(1) * power,
                    steal,
                    &mut *self.rng,
                );
                self.events
                    .extend(global_attack_events(by, victim, attacked, result, steal));
            }
            C::CounterMagic => {}
        }
    }

    /// What a spell with the `unit_effect` hook does to the unit it is aimed at. The effects that
    /// are only read from a lasting spell do nothing here.
    fn unit_effect(
        &mut self,
        effect: crate::spell::unit_effect::UnitEffect,
        who: Target,
        player: PlayerId,
    ) {
        use crate::spell::unit_effect::UnitEffect as U;
        let Target::Unit(unit) = who else {
            return;
        };
        match effect {
            U::NaturesCures => self.world.natures_cures(unit),
            U::BlackWind => {
                if self.world.black_wind_unit(unit, &mut *self.rng) {
                    self.slain += 1;
                }
            }
            U::Lycanthropy => {
                if let Some((from, to)) = self.world.lycanthropy(unit) {
                    self.events.push(Event::UnitTransformed { unit, from, to });
                }
            }
            U::ChaosChannels => {
                if let Some(mutation) = self.world.mutate(unit, &mut *self.rng) {
                    self.events.push(Event::UnitMutated { unit, mutation });
                }
            }
            U::Stasis => {
                self.world.put_in_stasis(unit, player);
            }
            _ => {}
        }
    }

    /// What a spell with the `wizard_effect` hook does to `victim`. Subversion is carried out
    /// after the cast.
    fn wizard_effect(
        &mut self,
        effect: crate::spell::wizard::WizardEffect,
        victim: PlayerId,
        player: PlayerId,
    ) {
        use crate::spell::wizard::WizardEffect as W;
        let events = match effect {
            W::DrainPower => {
                let amount = self.world.drain_power(victim, &mut *self.rng);
                vec![Event::PowerDrained {
                    player,
                    victim,
                    amount,
                }]
            }
            W::CruelUnminding => {
                let amount = self.world.cruel_unminding(victim, &mut *self.rng);
                vec![Event::SkillDestroyed {
                    player,
                    victim,
                    amount,
                }]
            }
            W::SpellBlast => match self.world.spell_blast(player, victim) {
                Some((spell, mana)) => vec![Event::SpellBlasted {
                    player,
                    victim,
                    spell,
                    mana,
                }],
                None => Vec::new(),
            },
            W::Subversion => Vec::new(),
        };
        self.events.extend(events);
    }

    /// Report what moving a building did: its events, or the rejection.
    fn building_moved(
        &mut self,
        result: Result<Vec<Event>, crate::wizard::fortress::FortressError>,
    ) {
        match result {
            Ok(events) => self.events.extend(events),
            Err(e) => self
                .events
                .push(Event::Rejected(crate::world::Rejection::Fortress(e))),
        }
    }
}

impl World {
    /// Does the spell run the named hook?
    pub fn spell_has_hook(&self, spell: SpellId, hook: &str) -> bool {
        self.content.rules.spells.get(spell).has_hook(hook)
    }

    pub fn spell_summons(&self, spell: SpellId) -> bool {
        self.content
            .rules
            .spells
            .get(spell)
            .effects
            .iter()
            .any(|r| match &r.effect {
                crate::rules::effects::Effect::Summon { .. } => true,
                crate::rules::effects::Effect::Custom { name, .. } => {
                    name == crate::spell::hero::SUMMON_HERO
                }
                _ => false,
            })
    }

    /// Does casting `spell` ask the wizard for a target? Summoning spells do not when the
    /// wizard has a summoning circle: the creatures appear there.
    pub fn spell_needs_target(&self, player: PlayerId, spell: SpellId) -> bool {
        if crate::spell::hero::HeroSummon::Resurrection.is_in(self.content.rules.spells.get(spell))
        {
            // The dead hero to raise is chosen when the cast is finished.
            return true;
        }
        if self.spell_summons(spell) && self.summon_point(player).is_some() {
            // Only with several circles (`multiple_summoning_circles`) is the city chosen.
            return self.has_several_circles(player);
        }
        !matches!(
            self.content.rules.spells.get(spell).target,
            SpellTarget::None | SpellTarget::Wizard | SpellTarget::Battlefield
        )
    }

    /// Is `target` one `player` may aim `spell` at?
    pub fn check_target(
        &self,
        player: PlayerId,
        spell: SpellId,
        target: Option<Target>,
    ) -> Result<(), MagicError> {
        if crate::spell::hero::HeroSummon::Resurrection.is_in(self.content.rules.spells.get(spell))
        {
            let raisable = self.units().raisable_heroes();
            let circles = self.summon_circles(player);
            return match target {
                // With several circles the wizard says which one the hero comes back to.
                Some(Target::Hero(kind)) if raisable.contains(&kind) && circles.len() < 2 => Ok(()),
                Some(Target::Hero(kind)) if raisable.contains(&kind) => {
                    Err(MagicError::NeedsTarget)
                }
                Some(Target::HeroAt(kind, at))
                    if raisable.contains(&kind) && circles.contains(&at) =>
                {
                    Ok(())
                }
                _ => Err(MagicError::Target(TargetError::WrongTarget)),
            };
        }
        // Summon Hero, Summon Champion and Incarnation: no circle to appear in, or nobody left to
        // bring, is refused here as the cast itself refuses it (`Resurrection` returned above).
        if crate::spell::hero::HeroSummon::of(self.content.rules.spells.get(spell)).is_some()
            && self.units().hero_spell_blocked(player, spell).is_some()
        {
            return Err(MagicError::NotAvailable);
        }
        let wanted = self.content.rules.spells.get(spell).target;
        let mine = |o: Option<PlayerId>| o == Some(player);
        let wrong = MagicError::Target(TargetError::WrongTarget);
        let own_city = |c: hecs::Entity| {
            if mine(self.economy().city_owner(c)) {
                Ok(())
            } else {
                Err(MagicError::Target(TargetError::NotYourCity))
            }
        };
        if !self.spell_needs_target(player, spell) {
            return Ok(());
        }
        // A summoning with several circles: the city the creature appears in.
        if self.spell_summons(spell) && self.has_several_circles(player) {
            return match target {
                None => Err(MagicError::NeedsTarget),
                Some(Target::City(c)) => match self.circle_in(player, c) {
                    Some(_) => Ok(()),
                    None if mine(self.economy().city_owner(c)) => Err(wrong),
                    None => Err(MagicError::Target(TargetError::NotYourCity)),
                },
                Some(_) => Err(wrong),
            };
        }
        // Move Fortress, Summoning Circle and Return to Fortress.
        let building = self
            .content
            .rules
            .spells
            .get(spell)
            .hook_args(crate::spell::special::MOVE_BUILDING)
            .and_then(|a| a.first())
            .map(String::as_str);
        let returns = self
            .content
            .rules
            .spells
            .get(spell)
            .has_hook(crate::spell::special::SPELL_OF_RETURN);
        if building.is_some() || returns {
            let w = self.wizard(player).ok_or(MagicError::UnknownPlayer)?;
            let Some(Target::City(c)) = target else {
                return Err(MagicError::NeedsTarget);
            };
            if self.economy().city_owner(c) != Some(player) {
                return Err(MagicError::Target(TargetError::NotYourCity));
            }
            let here = self.ecs.get::<&Position>(c).ok().map(|x| *x);
            let refused = match building {
                None /* spell_of_return */ => {
                    (!w.is_banished()).then_some(TargetError::NotBanished)
                }
                Some(crate::spell::special::MOVE_FORTRESS) => match w.fortress() {
                    None => Some(TargetError::NoFortress),
                    Some(f) if Some(f) == here => Some(TargetError::FortressHere),
                    Some(_) => None,
                },
                Some(_) => (here == w.summon_at()).then_some(TargetError::CircleHere),
            };
            return refused.map_or(Ok(()), |e| Err(MagicError::Target(e)));
        }
        if crate::spell::city::CityEffect::SpellWard.is_in(self.content.rules.spells.get(spell)) {
            return match target {
                Some(Target::CityRealm(c, r)) => {
                    own_city(c)?;
                    self.check_ward(c, r).map_err(MagicError::Target)
                }
                Some(_) => Err(wrong),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if spell == crate::rules::SpellId::new(183)
        /* evil_presence */
        {
            if let Some(Target::City(c)) = target {
                if self.economy().evil_presence_is_wasted_on(c) {
                    return Err(MagicError::Target(TargetError::EvilPresenceWasted));
                }
            }
        }
        if self
            .content
            .rules
            .spells
            .get(spell)
            .has_hook_arg(crate::hook::WALL, "stone")
        {
            return match target {
                Some(Target::City(c)) => {
                    own_city(c)?;
                    self.check_raise_wall_of_stone(c)
                        .map_err(MagicError::Target)
                }
                Some(_) => Err(wrong),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if let Some(effect) =
            crate::spell::terraform::TerrainEffect::of(self.content.rules.spells.get(spell))
                .filter(|e| *e != crate::spell::terraform::TerrainEffect::EarthLore)
        {
            use crate::spell::terraform::TerrainEffect as T;
            return match target {
                Some(Target::Tile(p)) => match effect {
                    T::RaiseVolcano => self.check_raise_volcano(p),
                    T::Corruption => self.check_corrupt(p),
                    T::WarpNode => self.check_warp_node(p, player),
                    T::EnchantRoad => self.check_enchant_road(p),
                    T::ChangeTerrain => self.change_terrain_result(p).map(|_| ()),
                    T::Transmute => self.transmute_result(p).map(|_| ()),
                    T::FloatingIsland => self.check_floating_island(player, p),
                    T::EarthLore => Ok(()),
                }
                .map_err(MagicError::Target),
                Some(_) => Err(wrong),
                None => Err(MagicError::NeedsTarget),
            };
        }
        let own_unit_at = |u: hecs::Entity| match target {
            Some(Target::UnitAt(t, _)) if t == u && mine(self.units().unit_owner(u)) => Ok(()),
            _ => Err(MagicError::Target(TargetError::NotYourUnit)),
        };
        let own_unit = |u: hecs::Entity| match target {
            Some(Target::Unit(t)) if t == u && mine(self.units().unit_owner(u)) => Ok(()),
            _ => Err(MagicError::Target(TargetError::NotYourUnit)),
        };
        if self.content.rules.spells.get(spell).has_hook_arg(
            crate::spell::special::RECALL,
            crate::spell::special::RECALL_ANYONE,
        ) {
            return match target {
                // With several circles the wizard says which one the unit goes to.
                Some(Target::Unit(u)) => {
                    own_unit(u)?;
                    self.recall_destination(player, None)
                        .map(|_| ())
                        .map_err(MagicError::Target)
                }
                Some(Target::UnitAt(u, at)) => {
                    own_unit_at(u)?;
                    self.recall_destination(player, Some(at))
                        .map(|_| ())
                        .map_err(MagicError::Target)
                }
                Some(_) => Err(wrong),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self
            .content
            .rules
            .spells
            .get(spell)
            .has_hook(crate::spell::special::PLANE_SHIFT)
        {
            return match target {
                Some(Target::Unit(u)) => {
                    own_unit(u)?;
                    self.plane_shift_destination(u)
                        .map(|_| ())
                        .map_err(MagicError::Target)
                }
                Some(_) => Err(wrong),
                None => Err(MagicError::NeedsTarget),
            };
        }
        let unit_effect =
            crate::spell::unit_effect::UnitEffect::of(self.content.rules.spells.get(spell));
        if unit_effect == Some(crate::spell::unit_effect::UnitEffect::ChaosChannels) {
            return match target {
                Some(Target::Unit(u)) => {
                    own_unit(u)?;
                    self.check_mutate(u).map_err(MagicError::Target)
                }
                Some(_) => Err(wrong),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if unit_effect == Some(crate::spell::unit_effect::UnitEffect::Lycanthropy) {
            return match target {
                Some(Target::Unit(u)) => {
                    own_unit(u)?;
                    self.check_become_werewolf(u).map_err(MagicError::Target)
                }
                Some(_) => Err(wrong),
                None => Err(MagicError::NeedsTarget),
            };
        }
        let disenchants = matches!(
            crate::spell::dispel::CounterEffect::of(self.content.rules.spells.get(spell)),
            Some((crate::spell::dispel::CounterEffect::Dispel, _))
        );
        if disenchants && wanted == SpellTarget::Tile {
            if let Some(Target::Tile(at)) = target {
                self.check_dispel_area(at, player)
                    .map_err(MagicError::Target)?;
            }
        }
        if self.is_square_spell(spell) {
            return match target {
                Some(Target::Tile(at)) => {
                    let enemy_there = self.units().units_at(at).iter().any(|&u| {
                        !mine(self.units().unit_owner(u))
                            && !(unit_effect == Some(crate::spell::unit_effect::UnitEffect::Stasis)
                                && self.unit_in_stasis(u))
                    });
                    if enemy_there {
                        Ok(())
                    } else {
                        Err(MagicError::Target(TargetError::NoEnemyHere))
                    }
                }
                Some(_) => Err(wrong),
                None => Err(MagicError::NeedsTarget),
            };
        }
        let refused = match (wanted, target) {
            (SpellTarget::None | SpellTarget::Wizard | SpellTarget::Battlefield, _) => None,
            (_, None) => return Err(MagicError::NeedsTarget),
            (SpellTarget::OwnUnit, Some(Target::Unit(u))) => {
                (!mine(self.units().unit_owner(u))).then_some(TargetError::NotYourUnit)
            }
            (SpellTarget::EnemyUnit, Some(Target::Unit(u))) => {
                (!self.units().unit_owner(u).is_some_and(|o| o != player))
                    .then_some(TargetError::NotEnemyUnit)
            }
            (SpellTarget::AnyUnit, Some(Target::Unit(u))) => self
                .units()
                .unit_owner(u)
                .is_none()
                .then_some(TargetError::WrongTarget),
            (SpellTarget::OwnCity, Some(Target::City(c))) => {
                (!mine(self.economy().city_owner(c))).then_some(TargetError::NotYourCity)
            }
            (SpellTarget::EnemyCity, Some(Target::City(c))) => {
                (!self.economy().city_owner(c).is_some_and(|o| o != player))
                    .then_some(TargetError::NotEnemyCity)
            }
            (SpellTarget::EnemyWizard, Some(Target::Player(p))) => {
                let blast = crate::spell::wizard::WizardEffect::SpellBlast
                    .is_in(self.content.rules.spells.get(spell));
                if p == player || self.player(p).is_some_and(|p| p.is_defeated()) {
                    Some(TargetError::NotEnemyWizard)
                } else if blast && self.mana_spent_casting(p).is_none() {
                    Some(TargetError::NotCasting)
                } else if !self.have_met(player, p) {
                    Some(TargetError::NoContact)
                } else if blast
                    && self.player(player).is_some_and(|me| {
                        me.treasury().mana < self.mana_spent_casting(p).unwrap_or(0)
                    })
                {
                    Some(TargetError::CannotMatchMana)
                } else {
                    None
                }
            }
            (SpellTarget::GlobalEnchantment, Some(Target::Spell(i))) => {
                let other_wizards = self.spells().target(i)
                    == Some(crate::spell::instance::SpellTarget::Global)
                    && self.spells().caster_player(i).is_some_and(|o| o != player);
                (!other_wizards).then_some(TargetError::NotGlobalSpell)
            }
            (SpellTarget::Tile, Some(Target::Tile(p))) => {
                let on_map = self
                    .map
                    .as_ref()
                    .is_some_and(|m| p.x < m.width() && p.y < m.height());
                (!on_map).then_some(TargetError::OffMap)
            }
            _ => Some(TargetError::WrongTarget),
        };
        if let Some(why) = refused {
            return Err(MagicError::Target(why));
        }
        self.check_stacking(player, spell, target)
    }

    /// Put a lasting spell on its target: a unit or city carries it with its caster, a
    /// wizard-wide one is kept on the wizard. No cost, no checks (the screenshot options use it).
    pub fn attach_enchantment(&mut self, who: Target, spell: SpellId, caster: PlayerId) {
        match who {
            Target::City(c) | Target::CityRealm(c, _) => {
                self.add_city_enchantment(c, spell, caster)
            }
            Target::Unit(u) => self.add_unit_enchantment(u, spell, caster),
            Target::Player(p) => {
                self.add_global(p, spell);
                self.reveal_for_globals(p);
            }
            Target::Tile(_)
            | Target::Spell(_)
            | Target::Hero(_)
            | Target::HeroAt(..)
            | Target::UnitAt(..) => {}
        }
    }

    /// Run a spell whose cost has been paid. Instant effects run now; a lasting spell is
    /// attached to its target (or to the wizard when it has none). The cast in progress, with
    /// the item or hero it carries, ends with it.
    pub(crate) fn resolve_cast(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        target: Option<Target>,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let extra = self.cast_extra(player);
        self.resolve_cast_with(player, spell, target, extra, rng)
    }

    /// `resolve_cast` for a spell with `extra` mana put into it.
    pub(crate) fn resolve_cast_with(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        target: Option<Target>,
        extra: i32,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let events = self.run_cast(player, spell, target, extra, rng);
        if let Some(mut w) = self.wizard_mut(player) {
            w.clear_casting();
        }
        events
    }

    fn run_cast(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        target: Option<Target>,
        extra: i32,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        // A crafting spell is resisted at the mana its item costs (patch 1.50, note 270).
        let crafted = crate::item::entity::crafting_of(&self.ecs, player)
            .filter(|_| self.spell_has_hook(spell, crate::item::crafting::CRAFT_ITEM))
            .map(|(_, item)| self.crafting_price(player, spell, item.cost).max(0) as u32);
        // A spell that takes extra mana is resisted at its cost and the extra, as a counter in a
        // battle is (the cost reductions do not make it easier).
        let crafted = crafted.or_else(|| {
            (extra > 0).then(|| self.content.rules.spells.get(spell).cost + extra as u32)
        });
        if let Some((by, counter)) = self.suppressed_at_cost(player, spell, crafted, rng) {
            return vec![Event::SpellSuppressed {
                player,
                spell,
                by,
                counter,
            }];
        }
        // Plane Shift and Word of Recall are checked again when it comes to it: the square or
        // the summoning circle they lead to can have been taken since the spell was chosen.
        // Then the spell fails and its mana comes back.
        // Word of Recall names its circle with its unit when the wizard has several: the rest of the
        // cast sees the unit.
        let (target, recall_to) = match target {
            Some(Target::UnitAt(unit, at)) => (Some(Target::Unit(unit)), Some(at)),
            other => (other, None),
        };
        if let Some(Target::Unit(unit)) = target {
            let rules = self.content.rules.spells.get(spell);
            let late = if rules.has_hook(crate::spell::special::PLANE_SHIFT) {
                self.plane_shift_destination(unit).err()
            } else if rules.has_hook_arg(
                crate::spell::special::RECALL,
                crate::spell::special::RECALL_ANYONE,
            ) {
                self.recall_destination(player, recall_to).err()
            } else {
                None
            };
            if let Some(reason) = late {
                let refunded = self.casting_cost_with_extra(player, spell, extra);
                if let Some(t) = self.treasury_mut(player) {
                    t.mana += refunded;
                }
                return vec![Event::SpellFailed {
                    player,
                    spell,
                    reason,
                    refunded,
                }];
            }
        }
        if let Some(city) = self.ward_turns_away(player, spell, target) {
            return vec![Event::SpellFizzled {
                player,
                spell,
                city,
            }];
        }
        if let Some(Target::City(city)) = target {
            if self.nightshade_fizzles(player, spell, city, rng) {
                return vec![Event::SpellFizzled {
                    player,
                    spell,
                    city,
                }];
            }
        }
        // A Spell Ward is set up as a spell on the city; the realm it guards is kept beside it.
        let target = match target {
            Some(Target::CityRealm(city, realm)) => {
                self.add_spell_ward(city, realm);
                Some(Target::City(city))
            }
            other => other,
        };
        // Where the spell lands, for the client's scene (read before the spell can move or remove it).
        let aimed_at = match target {
            Some(Target::City(c)) => self.city_position(c),
            Some(Target::Unit(u)) => self.units().unit_position(u),
            Some(Target::Tile(pos)) => Some(pos),
            _ => None,
        };
        let def = self.content.rules.spells.get(spell).clone();
        let this = target.unwrap_or(Target::Player(player));
        let boost = match crate::spell::dispel::CounterEffect::of(&def) {
            Some((crate::spell::dispel::CounterEffect::Dispel, strength)) if def.cost > 0 => {
                extra.max(0) as u32 * strength / def.cost
            }
            _ => 0,
        };
        let ctx = Context {
            this,
            owner: player,
            // A spell with no target acts on the wizard.
            chosen: target.or(Some(this)),
            source: Source::Spell(spell),
        };
        let mut host = WorldHost {
            world: self,
            rng,
            events: Vec::new(),
            slain: 0,
            extra: extra.max(0) as u32,
            boost,
            recall_to,
        };
        let outcomes = run_rules(&def.effects, Trigger::OnCast, &ctx, &mut host);
        let WorldHost {
            events: hook_events,
            slain,
            ..
        } = host;

        let mut events = vec![Event::SpellCast { player, spell }];
        if let Some(at) = aimed_at {
            events.push(Event::SpellCastAt { player, spell, at });
        }
        if let Some(Target::Player(victim)) = target {
            if victim != player {
                events.push(Event::SpellCastOnWizard {
                    player,
                    spell,
                    victim,
                });
            }
        }
        let victim = match target {
            Some(Target::City(c)) => self.economy().city_owner(c).filter(|&o| o != player),
            _ => None,
        };
        self.cast_relations(player, spell, victim);
        if let Some(Target::Player(wizard)) = target {
            if crate::spell::wizard::WizardEffect::Subversion
                .is_in(self.content.rules.spells.get(spell))
            {
                self.subvert(player, wizard);
            }
        }
        events.extend(hook_events);
        if slain > 0 {
            events.push(Event::UnitsSlain {
                player,
                spell,
                count: slain,
            });
        }
        if def.duration != Duration::Instant {
            self.attach_enchantment(this, spell, player);
        }
        events.extend(self.natures_wrath_strikes(player, spell, rng));
        if outcomes
            .iter()
            .any(|o| matches!(o, Outcome::Custom { hook } if hook == "unimplemented"))
        {
            events.push(Event::SpellHasNoEffectYet { spell });
        }
        events
    }

    // ---- upkeep ------------------------------------------------------------------

    /// What the wizard pays mana for each turn, oldest first.
    pub(crate) fn upkeep_items(&self, player: PlayerId) -> Vec<UpkeepItem> {
        let spells = &self.content.rules.spells;
        let mut items = Vec::new();
        // Time Stop's wizard pays for that spell alone.
        let only_time_stop = self.time_stopper() == Some(player);
        // What the wizard cast and keeps up, oldest first: on the world, on any city (their own
        // and somebody else's) and on any unit.
        if let Some(wizard) = self.wizard(player).map(|w| w.entity()) {
            let lasting = self.spells();
            for instance in lasting.of_caster(wizard) {
                let (Some(s), Some(target)) = (lasting.def(instance), lasting.target(instance))
                else {
                    continue;
                };
                if only_time_stop
                    && !(target == Placed::Global && s == crate::rules::SpellId::new(80)/* time_stop */)
                {
                    continue;
                }
                let upkeep = spells.get(s).upkeep as i32;
                let (kind, mana) = match target {
                    Placed::Global => (UpkeepKind::Global(s), upkeep),
                    Placed::City(c) => (UpkeepKind::City(c, s), upkeep * self.upkeep_count(c, s)),
                    Placed::Unit(u) => (UpkeepKind::UnitSpell(u, s), upkeep),
                    // The cast in progress is paid when it is cast.
                    Placed::Wizard(_) | Placed::Feature(_) | Placed::Battlefield => {
                        continue;
                    }
                };
                items.push(UpkeepItem {
                    kind,
                    mana,
                    cost: spells.get(s).cost as i32,
                });
            }
        }
        if only_time_stop {
            return items;
        }
        // A Conjurer pays three quarters of a unit's mana upkeep, a fraction of a mana rounded up
        // (`Unit_Mana_Upkeep`: `(upkeep * 3 + 3) / 4`); the unit's own enchantments cost in full.
        let unit_percent = self
            .wizard(player)
            .map_or(100, |w| w.unit_upkeep_percent(&self.content.rules));
        for unit in self.units().player_units(player) {
            let mut mana = self
                .units()
                .unit_def(unit)
                .map_or(0, |d| d.upkeep.mana as i32);
            // A fantastic creature made undead costs half as much again.
            if self.units().unit_is_undead(unit)
                && self.units().unit_def(unit).is_some_and(|d| {
                    matches!(
                        d.kind,
                        crate::rules::UnitKind::Summoned | crate::rules::UnitKind::Fantastic
                    )
                })
            {
                mana = mana * 3 / 2;
            }
            mana = (mana * unit_percent + 99) / 100;
            if mana > 0 {
                items.push(UpkeepItem {
                    kind: UpkeepKind::Unit(unit),
                    mana,
                    cost: self.units().unit_def(unit).map_or(0, |d| d.cost as i32),
                });
            }
        }
        items.retain(|i| i.mana > 0);
        items
    }

    /// A spell of the wizard's ends because it cannot be paid for.
    fn lapse(&mut self, target: Placed, player: PlayerId, spell: SpellId) -> Vec<Event> {
        self.remove_enchantment(target, spell, player);
        vec![Event::EnchantmentLapsed { player, spell }]
    }

    /// Give up something the wizard can no longer pay for.
    pub(crate) fn drop_upkeep_item(&mut self, player: PlayerId, item: UpkeepItem) -> Vec<Event> {
        match item.kind {
            UpkeepKind::Global(s) => self.lapse(Placed::Global, player, s),
            UpkeepKind::City(c, s) => self.lapse(Placed::City(c), player, s),
            UpkeepKind::UnitSpell(u, s) => self.lapse(Placed::Unit(u), player, s),
            UpkeepKind::Unit(u) => {
                let lost = self.units().unit_kind_id(u).map(|unit| Event::UnitLost {
                    player,
                    unit,
                    cause: crate::world::LossCause::Mana,
                });
                self.remove_unit(u, crate::hero::Fate::Released);
                lost.into_iter()
                    .chain([Event::UnitsDisbanded { player, units: 1 }])
                    .collect()
            }
        }
    }
}
