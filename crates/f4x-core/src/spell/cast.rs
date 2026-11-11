//! Casting overland spells: targets, the effect host over the ECS, and lasting spells.
//!
//! Tests: in rules/exec/tests.rs, wizard/magic_game/tests.rs and spell/wizard/tests.rs.

use crate::rules::Ability;
use rand::RngCore;

use crate::battle::immunity::Realm;
use crate::game::rng::GameRng;
use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::effects::{DamageKind, Trigger};
use crate::rules::exec::{run_rules, Context, EffectHost, Facts, Outcome, Target};
use crate::rules::{Duration, Modifier, Source, SpellId, SpellTarget, Stat, UnitId};
use crate::spell::instance::SpellTarget as Placed;
use crate::spell::target_error::TargetError;
use crate::unit::Hurt;
use crate::wizard::magic::MagicError;

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
struct GameHost<'a> {
    game: &'a mut Game,
    /// The game's generator, lent to the host for the resolution (`Game::resolve_cast`), so the
    /// executor and the hooks draw from one stream.
    rng: GameRng,
    /// What hooks and other changes report back to the event log.
    events: Vec<Event>,
    /// Units destroyed by the spell's strikes (`strike`), reported once the spell is done.
    slain: u32,
    /// Mana put into the spell beyond its cost: it adds to a dispel's strength.
    extra: u32,
    /// The strength those mana add to a dispel: one a mana, three for a True spell.
    boost: u32,
}

impl GameHost<'_> {
    /// What a spell with the `terrain_effect` hook does to the square.
    fn terrain_effect(
        &mut self,
        effect: crate::spell::terraform::TerrainEffect,
        at: Position,
        owner: PlayerId,
    ) {
        use crate::spell::terraform::TerrainEffect as T;
        let events = match effect {
            T::RaiseVolcano => self.game.raise_volcano(owner, at, &mut self.rng),
            T::ChangeTerrain => self.game.change_terrain_spell(at),
            T::EnchantRoad => self.game.enchant_roads(at),
            T::Corruption => self.game.corrupt(at),
            T::WarpNode => self.game.warp_node(at),
            T::Transmute => self.game.transmute(at),
            T::FloatingIsland => {
                self.game.floating_island(owner, at);
                return;
            }
            T::EarthLore => {
                self.game.earth_lore(owner, at);
                return;
            }
        };
        self.events.extend(events);
    }
}

impl EffectHost for GameHost<'_> {
    fn facts(&self, who: Target) -> Facts {
        let g = &*self.game;
        let mut facts = Facts::default();
        if let Target::Unit(u) = who {
            if let Some(def) = g.units().unit_def(u) {
                facts.abilities = g.units().unit_abilities(u);
                facts.realm = g.units().unit_realm(u);
                facts.kind = Some(if g.units().unit_is_undead(u) {
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
            Target::Unit(u) => self.game.units().unit_position(u),
            Target::City(c) | Target::CityRealm(c, _) => {
                self.game.world.get::<&Position>(c).ok().map(|p| *p)
            }
            Target::Tile(p) => Some(p),
            // A wizard is "at" their summoning circle.
            Target::Player(p) => self.game.summon_point(p),
            Target::Spell(i) => self
                .game
                .spells()
                .caster_player(i)
                .and_then(|p| self.game.summon_point(p)),
            Target::Hero(_) => None,
        }
    }

    fn owner_of(&self, who: Target) -> Option<PlayerId> {
        match who {
            Target::Unit(u) => self.game.units().unit_owner(u),
            Target::City(c) | Target::CityRealm(c, _) => self.game.economy().city_owner(c),
            Target::Player(p) => Some(p),
            Target::Spell(i) => self.game.spells().caster_player(i),
            Target::Tile(_) | Target::Hero(_) => None,
        }
    }

    fn units(&self) -> Vec<hecs::Entity> {
        self.game.units().all_units()
    }

    fn city_at(&self, pos: Position) -> Option<hecs::Entity> {
        self.game.economy().city_at(pos)
    }

    fn stat(&self, who: Target, stat: Stat) -> i32 {
        match who {
            Target::Unit(u) => self.game.units().unit_stat(u, stat),
            _ => 0,
        }
    }

    fn resistance_against(&self, who: Target, realm: Option<Realm>) -> i32 {
        match who {
            Target::Unit(u) => self.game.units().unit_resistance_against(u, realm),
            _ => 0,
        }
    }

    fn protected_from(&self, who: Target, realm: Option<Realm>) -> bool {
        match who {
            Target::Unit(u) => self.game.units().unit_protected_from(u, realm),
            _ => false,
        }
    }

    /// Bare modifiers and abilities from instant spells and rules go to the bearer's
    /// temporary effects (`spell/temp_effect.rs`); lasting spells use enchantments instead.
    fn add_modifier(&mut self, who: Target, modifier: Modifier) {
        self.game.add_temp_modifier(who, modifier, None);
    }

    fn grant_ability(&mut self, who: Target, ability: Ability) {
        self.game.grant_temp_ability(who, ability, None);
    }

    fn damage(&mut self, who: Target, amount: i32, _kind: DamageKind) -> i32 {
        let Target::Unit(u) = who else {
            return 0;
        };
        let (left, _) = self.game.units().unit_hits(u);
        let dealt = amount.max(0).min(left as i32);
        let hurt = self.game.world.get::<&Hurt>(u).map_or(0, |h| h.0);
        let _ = self.game.world.insert_one(u, Hurt(hurt + dealt as u32));
        if dealt as u32 >= left {
            self.game.remove_unit(u, crate::hero::Fate::Slain);
        }
        dealt
    }

    fn strike(
        &mut self,
        who: Target,
        strength: i32,
        kind: DamageKind,
        realm: Option<crate::battle::immunity::Realm>,
        traits: &crate::rules::effects::DamageTraits,
    ) -> i32 {
        let Target::Unit(u) = who else {
            return 0;
        };
        if !traits.area {
            return self.damage(who, strength, kind);
        }
        let nature = crate::battle::immunity::AttackNature {
            stopped_by: Default::default(),
            realm,
            armor_piercing: traits.armor_piercing,
        };
        let before = self.game.units().unit_hits(u).0 as i32;
        if self
            .game
            .spell_hit_overland_unit(u, strength, traits, nature, &mut self.rng)
        {
            self.slain += 1;
            return before;
        }
        before - self.game.units().unit_hits(u).0 as i32
    }

    fn heal(&mut self, who: Target, amount: i32) -> i32 {
        let Target::Unit(u) = who else {
            return 0;
        };
        let hurt = self.game.world.get::<&Hurt>(u).map_or(0, |h| h.0);
        let healed = amount.max(0).min(hurt as i32);
        let _ = self.game.world.insert_one(u, Hurt(hurt - healed as u32));
        healed
    }

    fn summon(&mut self, owner: PlayerId, at: Position, id: UnitId, count: u8) {
        // Summoned creatures appear at the summoning circle; without one, where aimed.
        let at = self.game.summon_point(owner).unwrap_or(at);
        for _ in 0..count {
            self.game.spawn_unit(owner, at, id);
        }
    }

    fn rng(&mut self) -> &mut dyn RngCore {
        &mut self.rng
    }

    fn custom(&mut self, hook: &str, args: &[String], who: Target, ctx: &Context) {
        // The hooks several spells share. Most are carried out by the engine wherever the
        // wizard's global spells are consulted.
        if hook == crate::item::crafting::CRAFT_ITEM {
            let events = self.game.complete_crafting(ctx.owner);
            self.events.extend(events);
        }
        if hook == crate::spell::special::RECALL
            && args
                .iter()
                .any(|a| a == crate::spell::special::RECALL_ANYONE)
        {
            if let Target::Unit(unit) = who {
                self.game.word_of_recall(ctx.owner, unit);
            }
        }
        if hook == crate::spell::special::PLANE_SHIFT {
            if let Target::Unit(unit) = who {
                let shifted = self.game.plane_shift(ctx.owner, unit);
                self.events.extend(shifted);
            }
        }
        if hook == crate::spell::special::MOVE_BUILDING {
            if let Target::City(city) = who {
                let result = if args
                    .iter()
                    .any(|a| a == crate::spell::special::MOVE_FORTRESS)
                {
                    self.game.move_fortress(ctx.owner, city)
                } else {
                    self.game.move_summoning_circle(ctx.owner, city)
                };
                self.building_moved(result);
            }
        }
        if hook == crate::spell::special::SPELL_OF_MASTERY {
            let events = self.game.win_by_mastery(ctx.owner);
            self.events.extend(events);
        }
        if hook == crate::spell::special::SPELL_OF_RETURN {
            if let Target::City(city) = who {
                let result = self.game.return_to_fortress(ctx.owner, city);
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
                if let Some(events) = self.game.hero_hook(spell, ctx.owner, who, &mut self.rng) {
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
                let events = self.game.wall_of_stone(city);
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

impl GameHost<'_> {
    /// Other wizards' enchantments on the target are removed with chance
    /// `strength / (strength + difficulty)` (see `spell/dispel.rs`).
    fn dispel(&mut self, who: Target, strength: u32, by: PlayerId) -> u32 {
        let strength = strength + self.boost;
        let mut gone = Vec::new();
        let removed = self
            .game
            .dispel_target(who, strength, by, &mut self.rng, &mut gone);
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
                    vec![self.game.earthquake(city, &mut self.rng)]
                }
                Some(CityStrike::CallTheVoid) => {
                    let (units_lost, buildings_lost, population_lost) =
                        self.game.call_the_void(city, &mut self.rng);
                    vec![Event::WorldSpellStruck {
                        spell,
                        victim: self.game.economy().city_owner(city),
                        units_lost,
                        buildings_lost,
                        population_lost,
                    }]
                }
                None => Vec::new(),
            }
        } else {
            let count = match WorldKill::named(name) {
                Some(WorldKill::GreatUnsummoning) => self.game.great_unsummoning(&mut self.rng),
                Some(WorldKill::DeathWish) => self.game.death_wish(player, &mut self.rng),
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
        let def = self.game.content.rules.spells.get(spell);
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
                let spells = self.game.spells();
                let (Some(victim), Some(attacked)) =
                    (spells.caster_player(instance), spells.def(instance))
                else {
                    return;
                };
                let result = self.game.attack_global(
                    by,
                    instance,
                    cost * number.max(1),
                    steal,
                    &mut self.rng,
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
            U::NaturesCures => self.game.natures_cures(unit),
            U::BlackWind => {
                if self.game.black_wind_unit(unit, &mut self.rng) {
                    self.slain += 1;
                }
            }
            U::Lycanthropy => {
                if let Some((from, to)) = self.game.lycanthropy(unit) {
                    self.events.push(Event::UnitTransformed { unit, from, to });
                }
            }
            U::ChaosChannels => {
                if let Some(mutation) = self.game.mutate(unit, &mut self.rng) {
                    self.events.push(Event::UnitMutated { unit, mutation });
                }
            }
            U::Stasis => {
                self.game.put_in_stasis(unit, player);
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
                let amount = self.game.drain_power(victim, &mut self.rng);
                vec![Event::PowerDrained {
                    player,
                    victim,
                    amount,
                }]
            }
            W::CruelUnminding => {
                let amount = self.game.cruel_unminding(victim, &mut self.rng);
                vec![Event::SkillDestroyed {
                    player,
                    victim,
                    amount,
                }]
            }
            W::SpellBlast => match self.game.spell_blast(player, victim) {
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
                .push(Event::Rejected(crate::game::Rejection::Fortress(e))),
        }
    }
}

impl Game {
    /// Does the spell run the named hook?
    pub fn spell_has_hook(&self, spell: SpellId, hook: &str) -> bool {
        self.content.rules.spells.get(spell).has_hook(hook)
    }

    fn summons(&self, spell: SpellId) -> bool {
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
        if self.summons(spell) && self.summon_point(player).is_some() {
            return false;
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
            return match target {
                Some(Target::Hero(kind)) if self.units().raisable_heroes().contains(&kind) => {
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
        // A summoning needs room on the square the creatures appear on.
        if self.summons(spell) {
            let at = match target {
                Some(Target::Tile(p)) => self.summon_point(player).or(Some(p)),
                _ => self.summon_point(player),
            };
            if at.is_some_and(|p| self.units().units_at(p).len() >= self.max_tile()) {
                return Err(MagicError::Target(TargetError::SquareFull));
            }
        }
        if !self.spell_needs_target(player, spell) {
            return Ok(());
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
            let here = self.world.get::<&Position>(c).ok().map(|x| *x);
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
        let own_unit = |u: hecs::Entity| match target {
            Some(Target::Unit(t)) if t == u && mine(self.units().unit_owner(u)) => Ok(()),
            _ => Err(MagicError::Target(TargetError::NotYourUnit)),
        };
        if self.content.rules.spells.get(spell).has_hook_arg(
            crate::spell::special::RECALL,
            crate::spell::special::RECALL_ANYONE,
        ) {
            return match target {
                Some(Target::Unit(u)) => {
                    own_unit(u)?;
                    self.recall_destination(player, u)
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
            Target::Tile(_) | Target::Spell(_) | Target::Hero(_) => {}
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
    ) -> Vec<Event> {
        let extra = self.cast_extra(player);
        self.resolve_cast_with(player, spell, target, extra)
    }

    /// `resolve_cast` for a spell with `extra` mana put into it.
    pub(crate) fn resolve_cast_with(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        target: Option<Target>,
        extra: i32,
    ) -> Vec<Event> {
        let events = self.run_cast(player, spell, target, extra);
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
    ) -> Vec<Event> {
        if let Some((by, counter)) = self.suppressed_by(player, spell) {
            return vec![Event::SpellSuppressed {
                player,
                spell,
                by,
                counter,
            }];
        }
        // Plane Shift is checked again when it comes to it: the square it leads to can have
        // been taken since the spell was chosen. Then it fails and its mana comes back.
        if let (true, Some(Target::Unit(unit))) = (
            self.content
                .rules
                .spells
                .get(spell)
                .has_hook(crate::spell::special::PLANE_SHIFT),
            target,
        ) {
            if let Err(reason) = self.plane_shift_destination(unit) {
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
            if self.nightshade_fizzles(player, spell, city) {
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
        // The host needs the game and the generator at once, so the generator is lent to it for
        // the resolution (`GameRng::lent` stands in the game meanwhile) and taken back after.
        let rng = std::mem::replace(&mut self.rng, GameRng::lent());
        let mut host = GameHost {
            game: self,
            rng,
            events: Vec::new(),
            slain: 0,
            extra: extra.max(0) as u32,
            boost,
        };
        let outcomes = run_rules(&def.effects, Trigger::OnCast, &ctx, &mut host);
        let GameHost {
            rng,
            events: hook_events,
            slain,
            ..
        } = host;
        self.rng = rng;

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
                self.subvert(wizard);
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
        events.extend(self.natures_wrath_strikes(player, spell));
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
                    cause: crate::game::LossCause::Mana,
                });
                self.remove_unit(u, crate::hero::Fate::Released);
                lost.into_iter()
                    .chain([Event::UnitsDisbanded { player, units: 1 }])
                    .collect()
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    /// A dispel of `strength` by `by` on `who`, as the `dispel` counter effect does it: how many
    /// enchantments came off.
    pub(crate) fn dispel(game: &mut Game, who: Target, strength: u32, by: PlayerId) -> u32 {
        let rng = game.rng.clone();
        let mut host = GameHost {
            game,
            rng,
            events: Vec::new(),
            slain: 0,
            extra: 0,
            boost: 0,
        };
        host.dispel(who, strength, by)
    }

    pub(crate) fn host(game: &mut Game) -> impl EffectHost + '_ {
        // A copy of the generator: tests of single effects need no stream of their own.
        let rng = game.rng.clone();
        GameHost {
            game,
            rng,
            events: Vec::new(),
            slain: 0,
            extra: 0,
            boost: 0,
        }
    }
}
