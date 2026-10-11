//! Casting overland spells: targets, the effect host over the ECS, and lasting spells.

use rand::{Error, RngCore};

use crate::city::{PlayerId, Position};
use crate::content::TerrainClass;
use crate::economy::Enchantments;
use crate::magic::MagicError;
use crate::rules::effects::{DamageKind, Trigger};
use crate::rules::exec::{run_rules, Context, EffectHost, Facts, Outcome, Target};
use crate::rules::{Duration, Modifier, Source, SpellId, SpellTarget, Stat};
use crate::units::{Hurt, UnitEnchantments};
use crate::{Event, Game};

/// Something a wizard pays mana for every turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpkeepKind {
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

/// Counts how many values the game's generator hands out, so the save can resume it.
struct CountingRng {
    inner: rand_pcg::Pcg64Mcg,
    draws: u64,
}

impl RngCore for CountingRng {
    fn next_u32(&mut self) -> u32 {
        self.draws += 1;
        self.inner.next_u32()
    }
    fn next_u64(&mut self) -> u64 {
        self.draws += 1;
        self.inner.next_u64()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.draws += 1;
        self.inner.fill_bytes(dest)
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        self.draws += 1;
        self.inner.try_fill_bytes(dest)
    }
}

/// The real world, as the effect executor sees it.
struct GameHost<'a> {
    game: &'a mut Game,
    /// What hooks and other changes report back to the event log.
    events: Vec<Event>,
}

impl EffectHost for GameHost<'_> {
    fn facts(&self, who: Target) -> Facts {
        let g = &*self.game;
        let mut facts = Facts::default();
        let pos = self.position(who);
        if let Some(p) = pos {
            facts.plane = Some(p.plane);
            facts.terrain = g.terrain_class_at(p);
            facts.in_city = g.city_at(p).is_some();
        }
        if let Target::Unit(u) = who {
            if let Some(def) = g.unit_def(u) {
                facts.abilities = def.abilities.clone();
                facts.race = def.race.clone();
                facts.realm = def.realm.clone();
                facts.kind = Some(def.kind);
            }
        }
        facts
    }

    fn position(&self, who: Target) -> Option<Position> {
        match who {
            Target::Unit(u) => self.game.unit_position(u),
            Target::City(c) => self.game.world.get::<&Position>(c).ok().map(|p| *p),
            Target::Tile(p) => Some(p),
            // A wizard is "at" their summoning circle.
            Target::Player(p) => self.game.summon_point(p),
        }
    }

    fn owner_of(&self, who: Target) -> Option<PlayerId> {
        match who {
            Target::Unit(u) => self.game.unit_owner(u),
            Target::City(c) => self
                .game
                .world
                .get::<&crate::city::City>(c)
                .ok()
                .map(|c| c.owner),
            Target::Player(p) => Some(p),
            Target::Tile(_) => None,
        }
    }

    fn units(&self) -> Vec<hecs::Entity> {
        self.game.all_units()
    }

    fn city_at(&self, pos: Position) -> Option<hecs::Entity> {
        self.game.city_at(pos)
    }

    fn stat(&self, who: Target, stat: Stat) -> i32 {
        match who {
            Target::Unit(u) => self.game.unit_stat(u, stat),
            _ => 0,
        }
    }

    /// Lasting modifiers come from enchantments (see `apply_enchantment`), so a bare
    /// modifier from an instant spell has nowhere to live and is dropped.
    fn add_modifier(&mut self, _who: Target, _modifier: Modifier) {}

    fn grant_ability(&mut self, _who: Target, _ability: &str) {}

    fn remove_ability(&mut self, _who: Target, _ability: &str) {}

    fn damage(&mut self, who: Target, amount: i32, _kind: DamageKind) -> i32 {
        let Target::Unit(u) = who else {
            return 0;
        };
        let (left, _) = self.game.unit_hits(u);
        let dealt = amount.max(0).min(left as i32);
        let hurt = self.game.world.get::<&Hurt>(u).map_or(0, |h| h.0);
        let _ = self.game.world.insert_one(u, Hurt(hurt + dealt as u32));
        if dealt as u32 >= left {
            let _ = self.game.world.despawn(u);
        }
        dealt
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
        if let Some(p) = self.game.player_mut(owner) {
            p.gold += gold;
            p.mana += mana;
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

    /// Each enchantment on the target is removed with chance `strength / (strength +
    /// difficulty)`.
    fn dispel(&mut self, who: Target, strength: u32, rng: &mut dyn RngCore) -> u32 {
        let list: Vec<(SpellId, PlayerId)> = match who {
            Target::City(c) => self
                .game
                .world
                .get::<&Enchantments>(c)
                .map(|e| e.0.clone())
                .unwrap_or_default(),
            Target::Unit(u) => self
                .game
                .world
                .get::<&UnitEnchantments>(u)
                .map(|e| e.0.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let mut kept = Vec::new();
        let mut removed = 0;
        for (spell, caster) in list {
            let difficulty = self.game.content.rules.spells.get(spell).dispel_difficulty;
            let chance = u64::from(strength) * 100 / u64::from((strength + difficulty).max(1));
            if rng.next_u64() % 100 < chance {
                removed += 1;
            } else {
                kept.push((spell, caster));
            }
        }
        match who {
            Target::City(c) => {
                let _ = self.game.world.insert_one(c, Enchantments(kept));
            }
            Target::Unit(u) => {
                let _ = self.game.world.insert_one(u, UnitEnchantments(kept));
            }
            _ => {}
        }
        removed
    }

    fn custom(&mut self, hook: &str, who: Target, ctx: &Context, rng: &mut dyn RngCore) {
        if hook == crate::globals::GLOBAL_EFFECT || hook == crate::gates::CITY_GATE {
            // Carried out by the engine wherever the wizard's global spells are consulted.
            return;
        }
        if hook == crate::endgame::SPELL_OF_MASTERY {
            let events = self.game.win_by_mastery(ctx.owner);
            self.events.extend(events);
            return;
        }
        if let Target::Tile(at) = who {
            let events = match hook {
                crate::terraform::RAISE_VOLCANO => self.game.raise_volcano(ctx.owner, at, rng),
                crate::terraform::CHANGE_TERRAIN => self.game.change_terrain_spell(at),
                crate::roads::ENCHANT_ROAD => self.game.enchant_roads(at),
                crate::corruption::CORRUPT_LAND => self.game.corrupt(at),
                crate::corruption::WARP_NODE => self.game.warp_node(at),
                _ => return,
            };
            self.events.extend(events);
            return;
        }
        let Target::City(city) = who else {
            return;
        };
        use crate::fortress::{MOVE_CIRCLE, MOVE_FORTRESS, RETURN};
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
                .push(Event::Rejected(crate::Rejection::Fortress(e))),
        }
    }
}

impl Game {
    pub fn terrain_class_at(&self, at: Position) -> Option<TerrainClass> {
        let map = self.map.as_ref()?;
        let id = map.plane(at.plane).get(at.x as i32, at.y as i32)?;
        self.content.terrains.get(id).map(|d| d.class)
    }

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
            crate::fortress::MOVE_FORTRESS,
            crate::fortress::MOVE_CIRCLE,
            crate::fortress::RETURN,
        ]
        .into_iter()
        .find(|h| self.spell_has_hook(spell, h));
        if let Some(hook) = fortress_hook {
            let p = self.player(player).ok_or(MagicError::UnknownPlayer)?;
            let Some(Target::City(c)) = target else {
                return Err(MagicError::NeedsTarget);
            };
            if self.city_owner(c) != Some(player) {
                return Err(MagicError::NoSuchTarget);
            }
            let here = self.world.get::<&Position>(c).ok().map(|x| *x);
            let ok = match hook {
                crate::fortress::RETURN => p.banished,
                crate::fortress::MOVE_FORTRESS => p.fortress.is_some() && here != p.fortress,
                _ => here != p.summon_at,
            };
            return ok.then_some(()).ok_or(MagicError::NoSuchTarget);
        }
        if self.spell_has_hook(spell, crate::terraform::RAISE_VOLCANO) {
            return match target {
                Some(Target::Tile(p)) if self.can_raise_volcano(p) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::corruption::CORRUPT_LAND) {
            return match target {
                Some(Target::Tile(p)) if self.can_corrupt(p) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::corruption::WARP_NODE) {
            return match target {
                Some(Target::Tile(p)) if self.can_warp_node(p, player) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::roads::ENCHANT_ROAD) {
            return match target {
                Some(Target::Tile(p)) if self.can_enchant_road(p) => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        if self.spell_has_hook(spell, crate::terraform::CHANGE_TERRAIN) {
            return match target {
                Some(Target::Tile(p)) if self.change_terrain_result(p).is_some() => Ok(()),
                Some(_) => Err(MagicError::NoSuchTarget),
                None => Err(MagicError::NeedsTarget),
            };
        }
        let ok = match (wanted, target) {
            (SpellTarget::None | SpellTarget::Wizard | SpellTarget::Battlefield, _) => true,
            (_, None) => return Err(MagicError::NeedsTarget),
            (SpellTarget::OwnUnit, Some(Target::Unit(u))) => mine(self.unit_owner(u)),
            (SpellTarget::EnemyUnit, Some(Target::Unit(u))) => {
                self.unit_owner(u).is_some_and(|o| o != player)
            }
            (SpellTarget::AnyUnit, Some(Target::Unit(u))) => self.unit_owner(u).is_some(),
            (SpellTarget::OwnCity, Some(Target::City(c))) => mine(self.city_owner(c)),
            (SpellTarget::EnemyCity, Some(Target::City(c))) => {
                self.city_owner(c).is_some_and(|o| o != player)
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
    /// wizard-wide one is kept on the wizard.
    pub(crate) fn attach_enchantment(&mut self, who: Target, spell: SpellId, caster: PlayerId) {
        if let Duration::Turns(n) = self.content.rules.spells.get(spell).duration {
            self.start_timer(who, spell, caster, n);
        }
        match who {
            Target::City(c) => {
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
            Target::Tile(_) => {}
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
        if let Some(Target::City(city)) = target {
            if self.nightshade_fizzles(player, spell, city) {
                return vec![Event::SpellFizzled {
                    player,
                    spell,
                    city,
                }];
            }
        }
        let def = self.content.rules.spells.get(spell).clone();
        let this = target.unwrap_or(Target::Player(player));
        let ctx = Context {
            this,
            owner: player,
            // A spell with no target acts on the wizard.
            chosen: target.or(Some(this)),
            source: Source::Spell(spell),
        };
        let mut rng = CountingRng {
            inner: self.rng.clone(),
            draws: 0,
        };
        let mut host = GameHost {
            game: self,
            events: Vec::new(),
        };
        let outcomes = run_rules(&def.effects, Trigger::OnCast, &ctx, &mut host, &mut rng);
        let hook_events = std::mem::take(&mut host.events);
        self.rng = rng.inner;
        self.draws += rng.draws;

        let mut events = vec![Event::SpellCast { player, spell }];
        events.extend(hook_events);
        if def.duration != Duration::Instant {
            self.attach_enchantment(this, spell, player);
        }
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
        if let Some(m) = self.magic(player) {
            for &s in &m.global {
                items.push(UpkeepItem {
                    kind: UpkeepKind::Global(s),
                    mana: spells.get(s).upkeep as i32,
                });
            }
        }
        for (city, c, _) in self.cities() {
            if c.owner != player {
                continue;
            }
            if let Ok(list) = self.world.get::<&Enchantments>(city) {
                for &(s, caster) in &list.0 {
                    if caster == player {
                        items.push(UpkeepItem {
                            kind: UpkeepKind::City(city, s),
                            mana: spells.get(s).upkeep as i32,
                        });
                    }
                }
            }
        }
        for unit in self.all_units() {
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
        for unit in self.player_units(player) {
            let mana = self.unit_def(unit).map_or(0, |d| d.upkeep.mana as i32);
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
                let _ = self.world.despawn(u);
                vec![Event::UnitsDisbanded { player, units: 1 }]
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub(crate) fn host(game: &mut Game) -> impl EffectHost + '_ {
        GameHost {
            game,
            events: Vec::new(),
        }
    }
}
