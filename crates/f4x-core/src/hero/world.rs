//! The game's side of the heroes: making the game's 35 hero entities when it
//! starts, bringing a hero into a wizard's service as a unit, and the one place every unit that
//! leaves the game goes through so that a hero's entity follows it (`remove_unit`).
//!
//! The rules themselves are in [`kind`](crate::hero::kind) and [`hero`](crate::hero); hiring
//! with gold, summoning, offers and resurrection call what is here.
//!
//! A hero is an individual, so there is one entity of each of the 35 types in the whole game, from
//! its start: `Serial`, `Hero`, `HeroState` and `Experience`. It is free (any wizard may hire it, or
//! be offered it), serving one wizard (then it also has `Owner` and the unit components, and no one
//! else can hire it), dead (any wizard may raise it) or gone for good. When the body goes the unit
//! components and the owner are removed and the entity stays (`release_body`).
//!
//! Tests: `tests/it/c9_hero.rs`.

use crate::hero::kind::{self, Class, Standing};
use crate::hero::Hero;
use crate::hero::HeroError;
use crate::hero::{Fate, HeroState};
use crate::map::Position;
use crate::player::{Owner, PlayerId};
use crate::unit::{Experience, Hurt, MovesLeft, Unit, UnitView};
use crate::world::rng::GameRng;
use crate::world::serial::Serial;

use crate::rules::RealmId;
use crate::world::{Event, World};

impl UnitView<'_> {
    /// Every hero of the game, in the order of the hero types.
    pub fn all_heroes(&self) -> Vec<hecs::Entity> {
        let mut out: Vec<hecs::Entity> = self.ecs.query::<&Hero>().iter().map(|(e, _)| e).collect();
        out.sort_by_key(|&e| self.ecs.get::<&Hero>(e).map_or(usize::MAX, |h| h.kind));
        out
    }

    /// The heroes in the wizard's service, in the order of the hero types.
    pub fn heroes(&self, player: PlayerId) -> Vec<hecs::Entity> {
        let Some(player) = crate::player::find_entity(self.ecs, player) else {
            return Vec::new();
        };
        self.all_heroes()
            .into_iter()
            .filter(|&e| self.ecs.get::<&Owner>(e).is_ok_and(|o| o.0 == player))
            .collect()
    }

    /// The hero of this type.
    pub fn hero_entity(&self, kind: usize) -> Option<hecs::Entity> {
        self.all_heroes()
            .into_iter()
            .find(|&e| self.ecs.get::<&Hero>(e).is_ok_and(|h| h.kind == kind))
    }

    /// Where a hero is.
    pub fn hero_state(&self, hero: hecs::Entity) -> Option<HeroState> {
        self.ecs.get::<&HeroState>(hero).ok().map(|s| *s)
    }

    /// The state of each hero type, in order.
    pub fn hero_states(&self) -> Vec<HeroState> {
        let mut states = vec![HeroState::Gone; self.content.rules.hero_templates.len()];
        for e in self.all_heroes() {
            if let (Ok(h), Some(state)) = (self.ecs.get::<&Hero>(e), self.hero_state(e)) {
                if let Some(slot) = states.get_mut(h.kind) {
                    *slot = state;
                }
            }
        }
        states
    }

    /// The experience a hero keeps while it is free or dead.
    pub fn hero_xp(&self, hero: hecs::Entity) -> u32 {
        self.ecs.get::<&Experience>(hero).map_or(0, |x| x.0)
    }

    /// The name of the hero of this type.
    pub fn hero_type_name(&self, kind: usize) -> Option<String> {
        self.hero_name(self.hero_entity(kind)?)
    }

    /// The wizard's hero units: the heroes in its service, which all have a body.
    pub fn hero_units(&self, player: PlayerId) -> Vec<hecs::Entity> {
        self.heroes(player)
            .into_iter()
            .filter(|&h| self.ecs.get::<&Unit>(h).is_ok())
            .collect()
    }

    /// A hero's mana in battle (`COMBINIT.c`): `(casting skill + 1) * (level + 1) * hero_mana_percent / 100`, and
    /// the spell skill of its items when it has any mana. 0 for a hero with no casting skill
    /// and for a unit that is no hero.
    pub fn hero_mana(&self, unit: hecs::Entity) -> i32 {
        let Ok(hero) = self.ecs.get::<&Hero>(unit) else {
            return 0;
        };
        if hero.casting_skill == 0 {
            return 0;
        }
        let x = self.unit_level(unit) as i32 + 1;
        let base = (i32::from(hero.casting_skill) + 1)
            * x
            * self.content.rules.config.wizard.hero_mana_percent
            / 100;
        let items: i32 = self
            .items()
            .carried(unit)
            .iter()
            .map(|(_, i)| i.bonus.spell_skill)
            .sum();
        base + items
    }

    /// A hero's own name (what the wizard called it).
    pub fn hero_name(&self, unit: hecs::Entity) -> Option<String> {
        self.ecs.get::<&Hero>(unit).ok().map(|h| h.name.clone())
    }

    /// The gender of a hero, from its type (`heroes.ron`).
    pub fn hero_gender(&self, unit: hecs::Entity) -> f4x_def::text::Gender {
        self.hero_kind(unit)
            .and_then(|k| self.content.rules.hero_templates.get(k))
            .map_or_else(Default::default, |t| t.gender)
    }

    /// The hero type of a hero entity.
    pub fn hero_kind(&self, hero: hecs::Entity) -> Option<usize> {
        self.ecs.get::<&Hero>(hero).ok().map(|h| h.kind)
    }

    /// What `kind::pick` asks of the wizard: fame and the books that unlock some heroes.
    pub fn hero_standing(&self, player: PlayerId) -> Standing {
        let magic = self.wizard(player);
        Standing {
            fame: self.fame(player),
            life_books: magic
                .as_ref()
                .map_or(0, |m| m.books_in(RealmId::new(3) /* life */)),
            death_books: magic
                .as_ref()
                .map_or(0, |m| m.books_in(RealmId::new(4) /* death */)),
        }
    }

    /// What the wizard pays to hire this hero type now: the cost plus a quarter of it for
    /// each level the hero kept from earlier service, less for a Charismatic wizard (`HireCostPercent`).
    pub fn hero_hire_price(&self, player: PlayerId, kind: usize) -> Option<i32> {
        let rules = &self.content.rules;
        let hero = self.hero_entity(kind)?;
        let unit = kind::unit_of(rules, kind)?;
        let cost = rules.units.get(unit).hero.as_ref()?.hire_cost as i32;
        let level = kind::level_of(rules, unit, self.hero_xp(hero));
        let percent = self.wizard(player).map_or(100, |m| {
            m.stat(rules, crate::rules::Stat::HireCostPercent, 100)
        });
        Some(kind::hire_cost(cost, level, percent))
    }

    /// The level a hero type would have if hired now.
    pub fn hero_kept_level(&self, kind: usize) -> usize {
        let rules = &self.content.rules;
        match (self.hero_entity(kind), kind::unit_of(rules, kind)) {
            (Some(hero), Some(unit)) => kind::level_of(rules, unit, self.hero_xp(hero)),
            _ => 0,
        }
    }
}

impl World {
    /// Casting skill the wizard's heroes add (`Player_Hero_Casting_Skill`): half the mana of each
    /// hero standing on the fortress square. It pays for a spell over turns, on top of the
    /// wizard's own skill.
    pub fn hero_casting_skill(&self, player: PlayerId) -> i32 {
        let Some(fortress) = self.wizard(player).and_then(|w| w.fortress()) else {
            return 0;
        };
        self.units()
            .hero_units(player)
            .into_iter()
            .filter(|&h| self.units().unit_position(h) == Some(fortress))
            .map(|h| self.units().hero_mana(h) / 2)
            .sum()
    }

    /// Roll one hero of the template numbered `kind` (`Init_Heroes` for one hero type, see
    /// [`kind::roll_one`]) and make its entity, in nobody's service. `None` when there is no such
    /// template. A new game makes one of each (`f4x_newgame::setup`), after everything else is
    /// placed, so the map and the starting cities of a seed stay as they were; a test makes the
    /// ones it needs.
    pub fn add_hero(&mut self, kind: usize, rng: &mut GameRng) -> Option<hecs::Entity> {
        let template = self.content.rules.hero_templates.get(kind)?;
        let hero = kind::roll_one(rng, &self.content.rules, &self.names, kind, template);
        Some(self.make_hero(hero, HeroState::Free, 0))
    }

    /// A new hero entity, in nobody's service.
    pub(crate) fn make_hero(&mut self, hero: Hero, state: HeroState, xp: u32) -> hecs::Entity {
        let serial = self.take_serial();
        self.make_hero_with(serial, hero, state, xp)
    }

    /// Like [`World::make_hero`], with the serial it was saved with.
    pub(crate) fn make_hero_with(
        &mut self,
        serial: Serial,
        hero: Hero,
        state: HeroState,
        xp: u32,
    ) -> hecs::Entity {
        self.ecs.spawn((serial, hero, state, Experience(xp)))
    }

    /// A random hero type the wizard can be offered or summon (`Pick_Random_Hero`). `None` has
    /// one meaning: the draws (10, or 100 for a free hero) found nobody who is free, within the
    /// wizard's fame and with the books the hero needs. That is a chance outcome of the original's
    /// rule, not a fault: the callers offer nothing (`player/offer.rs`) or leave the prisoner out
    /// (`feature/lair.rs`, where `SiteCleared` says `prisoner: false`).
    pub fn pick_hero(
        &mut self,
        player: PlayerId,
        zero_cost: bool,
        class: Class,
        rng: &mut GameRng,
    ) -> Option<usize> {
        let standing = self.units().hero_standing(player);
        let states = self.units().hero_states();
        kind::pick(
            rng,
            &self.content.rules,
            &states,
            &standing,
            zero_cost,
            class,
        )
    }

    /// Bring a hero into the wizard's army at `pos`: a free hero (hired, summoned) or a dead one
    /// (resurrected) enters the wizard's service and gets its unit body, with the abilities,
    /// spells, name and experience it already has. No one else can hire it while it serves. No gold or mana is taken here; whoever calls this has charged for it.
    pub fn spawn_hero(
        &mut self,
        player: PlayerId,
        pos: Position,
        kind: usize,
    ) -> Result<hecs::Entity, HeroError> {
        let entity = self.units().hero_entity(kind).ok_or(HeroError::Unknown)?;
        if !matches!(
            self.units().hero_state(entity),
            Some(HeroState::Free | HeroState::Dead)
        ) {
            return Err(HeroError::NotAvailable);
        }
        self.serve_hero(entity, player, pos)?;
        // Nobody else is offered this hero now.
        for (_, offers) in self.ecs.query::<&mut crate::player::Offers>().iter() {
            offers.0.retain(
                |o| !matches!(o, crate::player::offer::Offer::Hero { kind: k, .. } if *k == kind),
            );
        }
        Ok(entity)
    }

    /// Scenario setup: a hero rolled elsewhere (`hero::kind::roll_one`) enters `player`'s army at
    /// `pos` as one more body of its type, so that a battle can hold several of the same kind
    /// (the ladder's four against four). Unlike [`World::spawn_hero`] it takes no free hero.
    pub fn scenario_spawn_hero_copy(
        &mut self,
        player: PlayerId,
        pos: Position,
        hero: Hero,
    ) -> Result<hecs::Entity, HeroError> {
        let entity = self.make_hero(hero, HeroState::Free, 0);
        self.serve_hero(entity, player, pos)?;
        Ok(entity)
    }

    /// Give the hero entity its unit body in the army of `player` at `pos`.
    fn serve_hero(
        &mut self,
        entity: hecs::Entity,
        player: PlayerId,
        pos: Position,
    ) -> Result<(), HeroError> {
        let kind = self
            .ecs
            .get::<&Hero>(entity)
            .map_err(|_| HeroError::Unknown)?
            .kind;
        let unit = kind::unit_of(&self.content.rules, kind).ok_or(HeroError::Unknown)?;
        let owner = self.player_entity(player).ok_or(HeroError::Unknown)?;
        self.put_bundle(entity, (Owner(owner), Unit(unit), pos, Hurt(0)));
        let moves = self.units().unit_max_moves(entity);
        self.put(entity, MovesLeft(moves));
        self.put(entity, HeroState::Serving);
        crate::unit::ledger::enlist(&self.ecs, entity);
        let sight = self.units().unit_sight(entity);
        self.explore(player, pos, sight);
        Ok(())
    }

    /// The hero leaves its body: the unit's parts come off the hero's entry, which stays where it
    /// is with what is the hero's own (serial, what was rolled, experience) and gets the state the
    /// way it left gives. It is no one's now.
    fn release_body(&mut self, hero: hecs::Entity, state: HeroState) {
        crate::unit::ledger::strike(&self.ecs, hero);
        crate::unit::strip_body(&mut self.ecs, hero);
        self.put(hero, state);
        // Serial, Hero, HeroState and Experience: anything more is a part of a unit that
        // `strip_body` does not know.
        debug_assert_eq!(
            self.ecs
                .entity(hero)
                .map(|e| e.component_types().count())
                .ok(),
            Some(4),
            "a released hero keeps a part of its body"
        );
    }

    /// Take a unit out of the game for good: the one way to do it, so that a hero's entity
    /// follows it (`Fate` says which way). The items of a hero that is slain go back to
    /// its own wizard's armory (a spell or an earthquake kills it: `Player_Process_Item_Pool`);
    /// those of one that is released or gone go with it (a released hero's items are taken by
    /// the caller first, `release_one`). The removals that do not come through here cannot be
    /// heroes: neutral guards (`feature/lair.rs`), the copy made for a battle summon
    /// (`battle/spells.rs`), settlers founding a city and the engineer that melds with a node.
    pub(crate) fn remove_unit(&mut self, unit: hecs::Entity, fate: Fate) {
        self.remove_unit_to(unit, fate, None);
    }

    /// Like [`World::remove_unit`], with the wizard that takes the items of a slain hero: the
    /// winner of the battle it fell in. A neutral winner takes nothing, so the items are
    /// destroyed; without an heir they go to the hero's own wizard.
    pub(crate) fn remove_unit_to(
        &mut self,
        unit: hecs::Entity,
        fate: Fate,
        heir: Option<PlayerId>,
    ) {
        let owner = self.units().unit_owner(unit);
        if self.ecs.get::<&Hero>(unit).is_err() {
            crate::unit::ledger::record(&self.ecs, unit, fate.into());
            self.despawn_with_spells(unit);
            return;
        }
        // A hero that is not serving is already out of the army: dead, gone or free. Taking it
        // out again would change how it left (a slain hero made `Gone` can never be brought
        // back), which is the original's repeated death of a unit that is already dead
        // (1.50 note 148).
        if self
            .ecs
            .get::<&HeroState>(unit)
            .is_ok_and(|state| *state != HeroState::Serving)
        {
            return;
        }
        if let (Fate::Slain, Some(owner)) = (fate, owner) {
            let takes = heir.unwrap_or(owner);
            if takes != crate::player::MONSTERS {
                self.pass_items(unit, takes);
            }
        }
        // What the hero still holds (a neutral winner takes nothing) is gone with it.
        self.despawn_items_of(unit);
        self.despawn_spells_on(unit);
        crate::unit::ledger::record(&self.ecs, unit, fate.into());
        self.release_body(unit, fate.state());
    }

    /// The items a hero holds go to the armory of `to` (the vault first); what does not fit is
    /// lost. Each is reported as an award, so the client can show the item screen.
    pub(crate) fn pass_items(&mut self, unit: hecs::Entity, to: PlayerId) {
        let items = self
            .items()
            .held(|h| matches!(h, crate::item::entity::HeldBy::Hero(u, _) if u == unit));
        let winner = self.player_entity(to);
        for (item, data) in items {
            let placed = self.place_item(to, item);
            let kept = placed != crate::item::armory::Placed::Lost;
            self.item_awards
                .extend(winner.map(|winner| (winner, data.name.clone(), kept)));
        }
    }

    /// The items awarded to wizards since the last call, one event for each wizard: the names
    /// and how many did not fit the armory. The client opens the item screen for the human.
    pub(crate) fn drain_item_awards(&mut self) -> Vec<Event> {
        let mut events: Vec<Event> = Vec::new();
        for (player, name, kept) in std::mem::take(&mut self.item_awards) {
            let Some(player) = crate::player::id_of(&self.ecs, player) else {
                continue;
            };
            let found = events.iter_mut().find_map(|e| match e {
                Event::ItemsAwarded {
                    player: p,
                    items,
                    lost,
                } if *p == player => Some((items, lost)),
                _ => None,
            });
            match found {
                Some((items, lost)) => {
                    items.push(name);
                    *lost += u32::from(!kept);
                }
                None => events.push(Event::ItemsAwarded {
                    player,
                    items: vec![name],
                    lost: u32::from(!kept),
                }),
            }
        }
        events
    }
}
