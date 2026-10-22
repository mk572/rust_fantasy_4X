//! Casting overland spells: targets, the effect host over the ECS, and lasting spells.

use rand::RngCore;

use crate::battle::immunity::Realm;
use crate::city::economy::Enchantments;
use crate::content::TerrainClass;
use crate::game::rng::GameRng;
use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::rules::effects::{DamageKind, Trigger};
use crate::rules::exec::{run_rules, Context, EffectHost, Facts, Outcome, Target};
use crate::rules::{Duration, Modifier, Source, SpellId, SpellTarget, Stat};
use crate::unit::{Hurt, UnitEnchantments};
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
}

impl GameHost<'_> {
    /// Drain Power, Cruel Unminding, Great Unsummoning, Death Wish and Earthquake
    /// (`spell/wizard.rs`); `None` for any other hook. Subversion is carried out after the cast.
    fn wizard_hook(&mut self, hook: &str, who: Target, ctx: &Context) -> Option<Vec<Event>> {
        use crate::spell::wizard::{
            BLACK_WIND, CRUEL_UNMINDING, DEATH_WISH, DRAIN_POWER, EARTHQUAKE, GREAT_UNSUMMONING,
            LYCANTHROPY, SUBVERSION,
        };
        let player = ctx.owner;
        let spell = match ctx.source {
            Source::Spell(s) => s,
            _ => return None,
        };
        Some(match (hook, who) {
            (DRAIN_POWER, Target::Player(victim)) => {
                let amount = self.game.drain_power(victim, &mut self.rng);
                vec![Event::PowerDrained {
                    player,
                    victim,
                    amount,
                }]
            }
            (CRUEL_UNMINDING, Target::Player(victim)) => {
                let amount = self.game.cruel_unminding(victim, &mut self.rng);
                vec![Event::SkillDestroyed {
                    player,
                    victim,
                    amount,
                }]
            }
            (GREAT_UNSUMMONING, _) => {
                let count = self.game.great_unsummoning(&mut self.rng);
                vec![Event::UnitsSlain {
                    player,
                    spell,
                    count,
                }]
            }
            (DEATH_WISH, _) => {
                let count = self.game.death_wish(player, &mut self.rng);
                vec![Event::UnitsSlain {
                    player,
                    spell,
                    count,
                }]
            }
            (BLACK_WIND, Target::Unit(unit)) => {
                if self.game.black_wind_unit(unit, &mut self.rng) {
                    self.slain += 1;
                }
                Vec::new()
            }
            (LYCANTHROPY, Target::Unit(unit)) => match self.game.lycanthropy(unit) {
                Some((from, to)) => vec![Event::UnitTransformed { unit, from, to }],
                None => Vec::new(),
            },
            (crate::unit::mutation::CHAOS_CHANNELS, Target::Unit(unit)) => {
                match self.game.mutate(unit, &mut self.rng) {
                    Some(mutation) => vec![Event::UnitMutated { unit, mutation }],
                    None => Vec::new(),
                }
            }
            (crate::unit::stasis::STASIS, Target::Unit(unit)) => {
                self.game.put_in_stasis(unit, player);
                Vec::new()
            }
            (EARTHQUAKE, Target::City(city)) => vec![self.game.earthquake(city, &mut self.rng)],
            (crate::spell::world::CALL_THE_VOID, Target::City(city)) => {
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
            (
                DRAIN_POWER
                | CRUEL_UNMINDING
                | EARTHQUAKE
                | SUBVERSION
                | BLACK_WIND
                | LYCANTHROPY
                | crate::spell::world::CALL_THE_VOID
                | crate::unit::mutation::CHAOS_CHANNELS
                | crate::unit::stasis::STASIS,
                _,
            ) => Vec::new(),
            _ => return None,
        })
    }

    /// Disjunction, Spell Binding and Spell Blast; `None` for any other hook.
    fn dispel_hook(&mut self, hook: &str, who: Target, ctx: &Context) -> Option<Vec<Event>> {
        use crate::spell::dispel::{
            global_attack_events, DISJUNCTION, DISJUNCTION_TRUE, SPELL_BINDING, SPELL_BLAST,
        };
        let by = ctx.owner;
        let cost = match ctx.source {
            Source::Spell(s) => self.game.content.rules.spells.get(s).cost,
            _ => 0,
        };
        let (factor, steal) = match hook {
            DISJUNCTION => (1, false),
            DISJUNCTION_TRUE => (3, false),
            SPELL_BINDING => (1, true),
            SPELL_BLAST => {
                let Target::Player(victim) = who else {
                    return Some(Vec::new());
                };
                return Some(match self.game.spell_blast(by, victim) {
                    Some((spell, mana)) => vec![Event::SpellBlasted {
                        player: by,
                        victim,
                        spell,
                        mana,
                    }],
                    None => Vec::new(),
                });
            }
            _ => return None,
        };
        let Target::Global(victim, spell) = who else {
            return Some(Vec::new());
        };
        let result =
            self.game
                .attack_global(by, victim, spell, cost * factor, steal, &mut self.rng);
        Some(global_attack_events(by, victim, spell, result, steal))
    }
}

impl EffectHost for GameHost<'_> {
    fn facts(&self, who: Target) -> Facts {
        let g = &*self.game;
        let mut facts = Facts::default();
        let pos = self.position(who);
        if let Some(p) = pos {
            facts.plane = Some(p.plane);
            facts.terrain = g.economy().terrain_class_at(p);
            facts.in_city = g.economy().city_at(p).is_some();
        }
        if let Target::Unit(u) = who {
            if let Some(def) = g.units().unit_def(u) {
                facts.abilities = g.units().unit_abilities(u);
                facts.race = def.race.clone();
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
            Target::Global(owner, _) => self.game.summon_point(owner),
        }
    }

    fn owner_of(&self, who: Target) -> Option<PlayerId> {
        match who {
            Target::Unit(u) => self.game.units().unit_owner(u),
            Target::City(c) | Target::CityRealm(c, _) => self.game.economy().city_owner(c),
            Target::Player(p) | Target::Global(p, _) => Some(p),
            Target::Tile(_) => None,
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
    /// temporary effects (`spell/temp_effects.rs`); lasting spells use enchantments instead.
    fn add_modifier(&mut self, who: Target, modifier: Modifier) {
        self.game.add_temp_modifier(who, modifier, None);
    }

    fn grant_ability(&mut self, who: Target, ability: &str) {
        self.game.grant_temp_ability(who, ability, None);
    }

    fn remove_ability(&mut self, who: Target, ability: &str) {
        self.game.remove_temp_ability(who, ability, None);
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
            self.game.remove_unit(u, crate::hero::roster::Fate::Slain);
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

    fn add_resources(&mut self, who: Target, gold: i32, _food: i32, mana: i32, research: i32) {
        let Some(owner) = self.owner_of(who) else {
            return;
        };
        if let Some((p, t)) = self.game.player_treasury_mut(owner) {
            t.gold += gold;
            t.mana += mana;
            if p.magic.researching.is_some() {
                p.magic.research_remaining = (p.magic.research_remaining - research).max(0);
            }
        }
    }

    fn summon(&mut self, owner: PlayerId, at: Position, unit: &str, count: u8) {
        let Some(id) = self.game.content.rules.units.id_of(unit) else {
            return;
        };
        // Summoned creatures appear at the summoning circle; without one, where aimed.
        let at = self.game.summon_point(owner).unwrap_or(at);
        for _ in 0..count {
            self.game.spawn_unit(owner, at, id);
        }
    }

    fn change_terrain(&mut self, at: Position, class: TerrainClass) {
        let Some(id) = self.game.content.terrains.first_of_class(class, at.plane) else {
            return;
        };
        if let Some(map) = self.game.map.as_mut() {
            map.plane_mut(at.plane).set(at.x, at.y, id);
        }
    }

    fn set_feature(&mut self, at: Position, feature: &str) {
        let Some(id) = self.game.content.features.id_of(feature) else {
            return;
        };
        if let Some(map) = self.game.map.as_mut() {
            map.plane_mut(at.plane).set_feature(at.x, at.y, Some(id));
        }
    }

    fn apply_enchantment(&mut self, who: Target, spell: &str, caster: PlayerId) {
        let Some(id) = self.game.content.rules.spells.id_of(spell) else {
            return;
        };
        self.game.attach_enchantment(who, id, caster);
    }

    /// Other wizards' enchantments on the target are removed with chance
    /// `strength / (strength + difficulty)` (see `spell/dispel.rs`).
    fn dispel(&mut self, who: Target, strength: u32, by: PlayerId) -> u32 {
        let mut gone = Vec::new();
        let removed = self
            .game
            .dispel_target(who, strength, by, &mut self.rng, &mut gone);
        self.events.extend(gone);
        removed
    }

    fn rng(&mut self) -> &mut dyn RngCore {
        &mut self.rng
    }

    fn custom(&mut self, hook: &str, who: Target, ctx: &Context) {
        if hook == crate::spell::global_enchantment::GLOBAL_EFFECT
            || hook == crate::city::gates::CITY_GATE
            || hook == crate::spell::city::CITY_FLAG
        {
            // Carried out by the engine wherever the wizard's global spells are consulted.
            return;
        }
        if let Some(events) = self.dispel_hook(hook, who, ctx) {
            self.events.extend(events);
            return;
        }
        if hook == crate::item::crafting::CRAFT_ITEM {
            let events = self.game.complete_crafting(ctx.owner);
            self.events.extend(events);
            return;
        }
        if let Some(events) = self.game.hero_hook(hook, ctx.owner, &mut self.rng) {
            self.events.extend(events);
            return;
        }
        if hook == crate::game::endgame::SPELL_OF_MASTERY {
            let events = self.game.win_by_mastery(ctx.owner);
            self.events.extend(events);
            return;
        }
        if let Some(events) = self.wizard_hook(hook, who, ctx) {
            self.events.extend(events);
            return;
        }
        if let Target::Tile(at) = who {
            let events = match hook {
                crate::spell::terraform::RAISE_VOLCANO => {
                    self.game.raise_volcano(ctx.owner, at, &mut self.rng)
                }
                crate::spell::terraform::CHANGE_TERRAIN => self.game.change_terrain_spell(at),
                crate::map::road::ENCHANT_ROAD => self.game.enchant_roads(at),
                crate::map::corruption::CORRUPT_LAND => self.game.corrupt(at),
                crate::map::corruption::WARP_NODE => self.game.warp_node(at),
                crate::spell::special::TRANSMUTE => self.game.transmute(at),
                crate::spell::special::EARTH_LORE => {
                    self.game.earth_lore(ctx.owner, at);
                    return;
                }
                _ => return,
            };
            self.events.extend(events);
            return;
        }
        if self.game.run_unit_special(hook, ctx.owner, who) {
            return;
        }
        let Target::City(city) = who else {
            return;
        };
        if hook == crate::spell::city::WALL_OF_STONE {
            let events = self.game.wall_of_stone(city);
            self.events.extend(events);
            return;
        }
        use crate::wizard::fortress::{MOVE_CIRCLE, MOVE_FORTRESS, RETURN};
        let result = match hook {
            MOVE_FORTRESS => self.game.move_fortress(ctx.owner, city),
            MOVE_CIRCLE => self.game.move_summoning_circle(ctx.owner, city),
            RETURN => self.game.return_to_fortress(ctx.owner, city),
            _ => return,
        };
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
        self.content
            .rules
            .spells
            .get(spell)
            .effects
            .iter()
            .any(|r| matches!(&r.effect, crate::rules::effects::Effect::Custom(h) if h == hook))
    }

    fn summons(&self, spell: SpellId) -> bool {
        self.content
            .rules
            .spells
            .get(spell)
            .effects
            .iter()
            .any(|r| matches!(r.effect, crate::rules::effects::Effect::Summon { .. }))
    }

    /// Does casting `spell` ask the wizard for a target? Summoning spells do not when the
    /// wizard has a summoning circle: the creatures appear there.
    pub fn spell_needs_target(&self, player: PlayerId, spell: SpellId) -> bool {
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
        let wanted = self.content.rules.spells.get(spell).target;
        let mine = |o: Option<PlayerId>| o == Some(player);
        if !self.spell_needs_target(player, spell) {
            return Ok(());
        }
        let fortress_hook = [
            crate::wizard::fortress::MOVE_FORTRESS,
            crate::wizard::fortress::MOVE_CIRCLE,
            crate::wizard::fortress::RETURN,
        ]
        .into_iter()
        .find(|h| self.spell_has_hook(spell, h));
        if let Some(hook) = fortress_hook {
            let p = self.player(player).ok_or(MagicError::UnknownPlayer)?;
            let Some(Target::City(c)) = target else {
                return Err(MagicError::NeedsTarget);
            };
            if self.economy().city_owner(c) != Some(player) {
                return Err(MagicError::NoSuchTarget);
            }
            let here = self.world.get::<&Position>(c).ok().map(|x| *x);
            let ok = match hook {
                crate::wizard::fortress::RETURN => p.banished,
                crate::wizard::fortress::MOVE_FORTRESS => {
                    p.fortress.is_some() && here != p.fortress
                }
                _ => here != p.summon_at,
            };
            return ok.then_some(()).ok_or(MagicError::NoSuchTarget);
        }
        if self.spell_has_hook(spell, crate::spell::city::SPELL_WARD) {
            return match target {
                Some(Target::CityRealm(c, r))
                    if self.economy().city_owner(c) == Some(player) && self.can_ward(c, r) =>
                {
                    Ok(())
                }
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.content.rules.spells.get(spell).id == "evil_presence" {
            if let Some(Target::City(c)) = target {
                if self.economy().evil_presence_is_wasted_on(c) {
                    return Err(MagicError::NoSuchTarget);
                }
            }
        }
        if self.spell_has_hook(spell, crate::spell::city::WALL_OF_STONE) {
            return match target {
                Some(Target::City(c))
                    if self.economy().city_owner(c) == Some(player)
                        && self.can_raise_wall_of_stone(c) =>
                {
                    Ok(())
                }
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::spell::terraform::RAISE_VOLCANO) {
            return match target {
                Some(Target::Tile(p)) if self.can_raise_volcano(p) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::map::corruption::CORRUPT_LAND) {
            return match target {
                Some(Target::Tile(p)) if self.can_corrupt(p) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::map::corruption::WARP_NODE) {
            return match target {
                Some(Target::Tile(p)) if self.can_warp_node(p, player) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::map::road::ENCHANT_ROAD) {
            return match target {
                Some(Target::Tile(p)) if self.can_enchant_road(p) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::spell::terraform::CHANGE_TERRAIN) {
            return match target {
                Some(Target::Tile(p)) if self.change_terrain_result(p).is_some() => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::spell::special::TRANSMUTE) {
            return match target {
                Some(Target::Tile(p)) if self.transmute_result(p).is_some() => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        let unit_ok = |u: hecs::Entity| match target {
            Some(Target::Unit(t)) if t == u => mine(self.units().unit_owner(u)),
            _ => false,
        };
        if self.spell_has_hook(spell, crate::spell::special::WORD_OF_RECALL) {
            return match target {
                Some(Target::Unit(u))
                    if unit_ok(u) && self.recall_destination(player, u).is_some() =>
                {
                    Ok(())
                }
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::spell::special::PLANE_SHIFT) {
            return match target {
                Some(Target::Unit(u))
                    if unit_ok(u) && self.plane_shift_destination(u).is_some() =>
                {
                    Ok(())
                }
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::unit::mutation::CHAOS_CHANNELS) {
            return match target {
                Some(Target::Unit(u)) if unit_ok(u) && self.can_mutate(u) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::spell::wizard::LYCANTHROPY) {
            return match target {
                Some(Target::Unit(u)) if unit_ok(u) && self.can_become_werewolf(u) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.is_square_spell(spell) {
            return match target {
                Some(Target::Tile(at))
                    if self.units().units_at(at).iter().any(|&u| {
                        !mine(self.units().unit_owner(u))
                            && !(self.spell_has_hook(spell, crate::unit::stasis::STASIS)
                                && self.unit_in_stasis(u))
                    }) =>
                {
                    Ok(())
                }
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        let ok = match (wanted, target) {
            (SpellTarget::None | SpellTarget::Wizard | SpellTarget::Battlefield, _) => true,
            (_, None) => return Err(MagicError::NeedsTarget),
            (SpellTarget::OwnUnit, Some(Target::Unit(u))) => mine(self.units().unit_owner(u)),
            (SpellTarget::EnemyUnit, Some(Target::Unit(u))) => {
                self.units().unit_owner(u).is_some_and(|o| o != player)
            }
            (SpellTarget::AnyUnit, Some(Target::Unit(u))) => self.units().unit_owner(u).is_some(),
            (SpellTarget::OwnCity, Some(Target::City(c))) => mine(self.economy().city_owner(c)),
            (SpellTarget::EnemyCity, Some(Target::City(c))) => {
                self.economy().city_owner(c).is_some_and(|o| o != player)
            }
            (SpellTarget::EnemyWizard, Some(Target::Player(p))) => {
                p != player
                    && !self.is_defeated(p)
                    && (!self.spell_has_hook(spell, crate::spell::dispel::SPELL_BLAST)
                        || self.mana_spent_casting(p).is_some())
            }
            (SpellTarget::GlobalEnchantment, Some(Target::Global(owner, s))) => {
                owner != player && self.magic(owner).is_some_and(|m| m.global.contains(&s))
            }
            (SpellTarget::Tile, Some(Target::Tile(p))) => self
                .map
                .as_ref()
                .is_some_and(|m| p.x < m.width() && p.y < m.height()),
            _ => false,
        };
        ok.then_some(()).ok_or(MagicError::NoSuchTarget)
    }

    /// Put a lasting spell on its target: a unit or city carries it with its caster, a
    /// wizard-wide one is kept on the wizard. No cost, no checks (the screenshot options use it).
    pub fn attach_enchantment(&mut self, who: Target, spell: SpellId, caster: PlayerId) {
        match who {
            Target::City(c) | Target::CityRealm(c, _) => {
                let mut list = self
                    .world
                    .get::<&Enchantments>(c)
                    .map(|e| e.0.clone())
                    .unwrap_or_default();
                if !list.contains(&(spell, caster)) {
                    list.push((spell, caster));
                }
                let _ = self.world.insert_one(c, Enchantments(list));
            }
            Target::Unit(u) => {
                let mut list = self
                    .world
                    .get::<&UnitEnchantments>(u)
                    .map(|e| e.0.clone())
                    .unwrap_or_default();
                if !list.contains(&(spell, caster)) {
                    list.push((spell, caster));
                }
                let _ = self.world.insert_one(u, UnitEnchantments(list));
            }
            Target::Player(p) => {
                if let Some(m) = self.player_mut(p).map(|p| &mut p.magic) {
                    if !m.global.contains(&spell) {
                        m.global.push(spell);
                    }
                }
                self.reveal_for_globals(p);
            }
            Target::Tile(_) | Target::Global(..) => {}
        }
    }

    /// Run a spell whose cost has been paid. Instant effects run now; a lasting spell is
    /// attached to its target (or to the wizard when it has none).
    pub(crate) fn resolve_cast(
        &mut self,
        player: PlayerId,
        spell: SpellId,
        target: Option<Target>,
    ) -> Vec<Event> {
        if let Some(m) = self.player_mut(player).map(|p| &mut p.magic) {
            m.casting = None;
        }
        if let Some(by) = self.suppressed_by(player, spell) {
            return vec![Event::SpellSuppressed { player, spell, by }];
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
        let def = self.content.rules.spells.get(spell).clone();
        let this = target.unwrap_or(Target::Player(player));
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
        let victim = match target {
            Some(Target::City(c)) => self.economy().city_owner(c).filter(|&o| o != player),
            _ => None,
        };
        self.cast_relations(player, spell, victim);
        if let Some(Target::Player(wizard)) = target {
            if self.spell_has_hook(spell, crate::spell::wizard::SUBVERSION) {
                self.subvert(wizard);
            }
        }
        for o in &outcomes {
            if let Outcome::Dispelled { removed, .. } = o {
                events.push(Event::Dispelled {
                    player,
                    removed: *removed,
                });
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
        if let Some(m) = self.magic(player) {
            for &s in &m.global {
                if only_time_stop && spells.get(s).id != "time_stop" {
                    continue;
                }
                items.push(UpkeepItem {
                    kind: UpkeepKind::Global(s),
                    mana: spells.get(s).upkeep as i32,
                });
            }
        }
        if only_time_stop {
            return items;
        }
        // Every city: the caster pays for what they put on somebody else's city too.
        for (city, _, _) in self.economy().cities() {
            if let Ok(list) = self.world.get::<&Enchantments>(city) {
                for &(s, caster) in &list.0 {
                    if caster == player {
                        items.push(UpkeepItem {
                            kind: UpkeepKind::City(city, s),
                            mana: spells.get(s).upkeep as i32 * self.upkeep_count(city, s),
                        });
                    }
                }
            }
        }
        for unit in self.units().all_units() {
            if let Ok(list) = self.world.get::<&UnitEnchantments>(unit) {
                for &(s, caster) in &list.0 {
                    if caster == player {
                        items.push(UpkeepItem {
                            kind: UpkeepKind::UnitSpell(unit, s),
                            mana: spells.get(s).upkeep as i32,
                        });
                    }
                }
            }
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
                });
            }
        }
        items.retain(|i| i.mana > 0);
        items
    }

    /// Give up something the wizard can no longer pay for.
    pub(crate) fn drop_upkeep_item(&mut self, player: PlayerId, item: UpkeepItem) -> Vec<Event> {
        match item.kind {
            UpkeepKind::Global(s) => {
                if let Some(m) = self.player_mut(player).map(|p| &mut p.magic) {
                    m.global.retain(|&x| x != s);
                }
                vec![Event::EnchantmentLapsed { player, spell: s }]
            }
            UpkeepKind::City(c, s) => {
                if let Ok(mut list) = self.world.get::<&mut Enchantments>(c) {
                    list.0.retain(|&(x, p)| !(x == s && p == player));
                }
                vec![Event::EnchantmentLapsed { player, spell: s }]
            }
            UpkeepKind::UnitSpell(u, s) => {
                if let Ok(mut list) = self.world.get::<&mut UnitEnchantments>(u) {
                    list.0.retain(|&(x, p)| !(x == s && p == player));
                }
                vec![Event::EnchantmentLapsed { player, spell: s }]
            }
            UpkeepKind::Unit(u) => {
                let lost = self.units().unit_kind_id(u).map(|unit| Event::UnitLost {
                    player,
                    unit,
                    cause: crate::game::LossCause::Mana,
                });
                self.remove_unit(u, crate::hero::roster::Fate::Released);
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

    pub(crate) fn host(game: &mut Game) -> impl EffectHost + '_ {
        // A copy of the generator: tests of single effects need no stream of their own.
        let rng = game.rng.clone();
        GameHost {
            game,
            rng,
            events: Vec::new(),
            slain: 0,
        }
    }
}
