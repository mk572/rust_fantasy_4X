//! The game's side of the heroes (tasks 219 and 326): making the game's 35 hero entities when it
//! starts, bringing a hero into a wizard's service as a unit, and the one place every unit that
//! leaves the game goes through so that a hero's entity follows it (`remove_unit`).
//!
//! The rules themselves are in [`kind`](crate::hero::kind) and [`hero`](crate::hero); hiring
//! with gold, summoning, offers and resurrection (tasks 48 and 220) call what is here.
//!
//! A hero is an individual, so there is one entity of each of the 35 types in the whole game, from
//! its start: `Serial`, `Hero`, `HeroState` and `Experience`. It is free (any wizard may hire it, or
//! be offered it), serving one wizard (then it also has `Owner` and the unit components, and no one
//! else can hire it), dead (any wizard may raise it) or gone for good. When the body goes the unit
//! components and the owner are removed and the entity stays (`release_body`).

use crate::game::serial::Serial;
use crate::hero::kind::{self, Class, Standing, MAX_HEROES};
use crate::hero::Hero;
use crate::hero::HeroError;
use crate::hero::{Fate, HeroState};
use crate::map::Position;
use crate::player::{Owner, PlayerId};
use crate::unit::{Experience, Hurt, MovesLeft, Unit, UnitView};

use crate::game::{Event, Game};
use crate::wizard::magic::Retort;

impl UnitView<'_> {
    /// Every hero of the game, in the order of the hero types.
    pub fn all_heroes(&self) -> Vec<hecs::Entity> {
        let mut out: Vec<hecs::Entity> =
            self.world.query::<&Hero>().iter().map(|(e, _)| e).collect();
        out.sort_by_key(|&e| self.world.get::<&Hero>(e).map_or(usize::MAX, |h| h.kind));
        out
    }

    /// The heroes in the wizard's service, in the order of the hero types.
    pub fn heroes(&self, player: PlayerId) -> Vec<hecs::Entity> {
        let Some(player) = crate::player::find_entity(self.world, player) else {
            return Vec::new();
        };
        self.all_heroes()
            .into_iter()
            .filter(|&e| self.world.get::<&Owner>(e).is_ok_and(|o| o.0 == player))
            .collect()
    }

    /// The hero of this type.
    pub fn hero_entity(&self, kind: usize) -> Option<hecs::Entity> {
        self.all_heroes()
            .into_iter()
            .find(|&e| self.world.get::<&Hero>(e).is_ok_and(|h| h.kind == kind))
    }

    /// Where a hero is.
    pub fn hero_state(&self, hero: hecs::Entity) -> Option<HeroState> {
        self.world.get::<&HeroState>(hero).ok().map(|s| *s)
    }

    /// The state of each hero type, in order.
    pub fn hero_states(&self) -> Vec<HeroState> {
        let mut states = vec![HeroState::Gone; self.content.rules.hero_templates.len()];
        for e in self.all_heroes() {
            if let (Ok(h), Some(state)) = (self.world.get::<&Hero>(e), self.hero_state(e)) {
                if let Some(slot) = states.get_mut(h.kind) {
                    *slot = state;
                }
            }
        }
        states
    }

    /// The experience a hero keeps while it is free or dead.
    pub fn hero_xp(&self, hero: hecs::Entity) -> u32 {
        self.world.get::<&Experience>(hero).map_or(0, |x| x.0)
    }

    /// The name of the hero of this type.
    pub fn hero_type_name(&self, kind: usize) -> Option<String> {
        self.hero_name(self.hero_entity(kind)?)
    }

    /// The wizard's hero units: the heroes in its service, which all have a body.
    pub fn hero_units(&self, player: PlayerId) -> Vec<hecs::Entity> {
        self.heroes(player)
            .into_iter()
            .filter(|&h| self.world.get::<&Unit>(h).is_ok())
            .collect()
    }

    /// A hero's mana in battle (`COMBINIT.c`): `(casting skill + 1) * (level + 1) * 5 / 2`, and
    /// the spell skill of its items when it has any mana. 0 for a hero with no casting skill
    /// and for a unit that is no hero.
    pub fn hero_mana(&self, unit: hecs::Entity) -> i32 {
        let Ok(hero) = self.world.get::<&Hero>(unit) else {
            return 0;
        };
        if hero.casting_skill == 0 {
            return 0;
        }
        let x = self.unit_level(unit) as i32 + 1;
        let base = (i32::from(hero.casting_skill) + 1) * x * 5 / 2;
        let items: i32 = self
            .world
            .get::<&crate::item::HeroItems>(unit)
            .map_or(0, |i| i.iter().map(|i| i.bonus.spell_skill).sum());
        base + items
    }

    /// A hero's own name (what the wizard called it).
    pub fn hero_name(&self, unit: hecs::Entity) -> Option<String> {
        self.world.get::<&Hero>(unit).ok().map(|h| h.name.clone())
    }

    /// How many more heroes the wizard may have (`Hero_Slot_Open`).
    pub fn open_hero_slots(&self, player: PlayerId) -> usize {
        MAX_HEROES.saturating_sub(self.hero_units(player).len())
    }

    /// The hero type of a hero entity.
    pub fn hero_kind(&self, hero: hecs::Entity) -> Option<usize> {
        self.world.get::<&Hero>(hero).ok().map(|h| h.kind)
    }

    /// What `kind::pick` asks of the wizard: fame and the books that unlock some heroes.
    pub fn hero_standing(&self, player: PlayerId) -> Standing {
        let magic = self.wizard(player);
        Standing {
            fame: self.fame(player),
            life_books: magic.as_ref().map_or(0, |m| m.books_in("life")),
            death_books: magic.as_ref().map_or(0, |m| m.books_in("death")),
        }
    }

    /// What the wizard pays to hire this hero type now: the cost plus a quarter of it for
    /// each level the hero kept from earlier service, half for a Charismatic wizard.
    pub fn hero_hire_price(&self, player: PlayerId, kind: usize) -> Option<i32> {
        let rules = &self.content.rules;
        let hero = self.hero_entity(kind)?;
        let unit = kind::unit_of(rules, kind)?;
        let cost = rules.units.get(unit).hero.as_ref()?.hire_cost as i32;
        let level = kind::level_of(rules, unit, self.hero_xp(hero));
        let charismatic = self
            .wizard(player)
            .is_some_and(|m| m.has(Retort::Charismatic));
        Some(kind::hire_cost(cost, level, charismatic))
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

impl Game {
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

    /// Roll the game's heroes (`Init_Heroes`) and make their entities. Called once when a game is
    /// made, after everything else is placed, so the map and the starting cities of a seed stay
    /// as they were.
    pub(crate) fn init_heroes(&mut self) {
        for hero in kind::roll(&mut self.rng, &self.content.rules) {
            self.make_hero(hero, HeroState::Free, 0);
        }
    }

    /// A new hero entity, in nobody's service.
    pub(crate) fn make_hero(&mut self, hero: Hero, state: HeroState, xp: u32) -> hecs::Entity {
        let serial = self.take_serial();
        self.make_hero_with(serial, hero, state, xp)
    }

    /// Like [`Game::make_hero`], with the serial it was saved with.
    pub(crate) fn make_hero_with(
        &mut self,
        serial: Serial,
        hero: Hero,
        state: HeroState,
        xp: u32,
    ) -> hecs::Entity {
        self.world.spawn((serial, hero, state, Experience(xp)))
    }

    /// A random hero type the wizard can be offered or summon (`Pick_Random_Hero`).
    pub fn pick_hero(&mut self, player: PlayerId, zero_cost: bool, class: Class) -> Option<usize> {
        let standing = self.units().hero_standing(player);
        let states = self.units().hero_states();
        kind::pick(
            &mut self.rng,
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
        if self.units().open_hero_slots(player) == 0 {
            return Err(HeroError::NoRoom);
        }
        let unit = kind::unit_of(&self.content.rules, kind).ok_or(HeroError::Unknown)?;
        let owner = self.player_entity(player).ok_or(HeroError::Unknown)?;
        let _ = self
            .world
            .insert(entity, (Owner(owner), Unit(unit), pos, Hurt(0)));
        let moves = self.units().unit_max_moves(entity);
        let _ = self.world.insert_one(entity, MovesLeft(moves));
        let _ = self.world.insert_one(entity, HeroState::Serving);
        let sight = self.units().unit_sight(entity) as u32;
        self.explore(player, pos, sight);
        // Nobody else is offered this hero now.
        for (_, offers) in self.world.query::<&mut crate::player::Offers>().iter() {
            offers.0.retain(
                |o| !matches!(o, crate::player::offer::Offer::Hero { kind: k, .. } if *k == kind),
            );
        }
        Ok(entity)
    }

    /// The hero's body is gone: the entity keeps only what is the hero's own (serial, what was
    /// rolled, experience) and gets the state the way it left gives. It is no one's now.
    fn release_body(&mut self, hero: hecs::Entity, state: HeroState) {
        let kept = (
            self.world.get::<&Serial>(hero).map(|s| *s),
            self.world.get::<&Hero>(hero).map(|h| (*h).clone()),
            self.world.get::<&Experience>(hero).map(|x| *x),
        );
        let (Ok(serial), Ok(data), Ok(xp)) = kept else {
            return;
        };
        let _ = self.world.despawn(hero);
        self.world.spawn_at(hero, (serial, data, state, xp));
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

    /// Like [`Game::remove_unit`], with the wizard that takes the items of a slain hero: the
    /// winner of the battle it fell in. A neutral winner takes nothing, so the items are
    /// destroyed; without an heir they go to the hero's own wizard.
    pub(crate) fn remove_unit_to(
        &mut self,
        unit: hecs::Entity,
        fate: Fate,
        heir: Option<PlayerId>,
    ) {
        let owner = self.units().unit_owner(unit);
        if self.world.get::<&Hero>(unit).is_err() {
            self.despawn_with_spells(unit);
            return;
        }
        if let (Fate::Slain, Some(owner)) = (fate, owner) {
            let items: Vec<_> = self
                .world
                .get::<&crate::item::HeroItems>(unit)
                .map(|h| h.iter().cloned().collect())
                .unwrap_or_default();
            let takes = heir.unwrap_or(owner);
            if !self.is_neutral_player(takes) {
                let winner = self.player_entity(takes);
                for item in items {
                    let name = item.name.clone();
                    let placed = self.give_item(takes, item);
                    let kept = placed != crate::item::armory::Placed::Lost;
                    self.item_awards
                        .extend(winner.map(|winner| (winner, name, kept)));
                }
            }
        }
        self.despawn_spells_on(unit);
        self.release_body(unit, fate.state());
    }

    /// The items awarded to wizards since the last call, one event for each wizard: the names
    /// and how many did not fit the armory. The client opens the item screen for the human.
    pub(crate) fn drain_item_awards(&mut self) -> Vec<Event> {
        let mut events: Vec<Event> = Vec::new();
        for (player, name, kept) in std::mem::take(&mut self.item_awards) {
            let Some(player) = crate::player::id_of(&self.world, player) else {
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::game::testutil::{content_with_magic, unit};
    use crate::hero::Tier;
    use crate::map::{Grid, Map, Plane};
    use crate::rules::{
        HeroDef, HeroKind, HeroTemplate, MovementType, PickType, Registry, UnitKind,
    };

    pub(crate) const P0: PlayerId = PlayerId(0);
    pub(crate) const P1: PlayerId = PlayerId(1);

    pub(crate) fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    fn template(unit: &str, picks: u32, pick_type: PickType, abilities: &[&str]) -> HeroTemplate {
        HeroTemplate {
            unit: unit.into(),
            picks,
            pick_type,
            abilities: abilities.iter().map(|a| a.to_string()).collect(),
            casting_skill: 1,
            spells: vec!["healing".into()],
            names: vec!["Ann".into(), "Bob".into()],
        }
    }

    /// Seven heroes (the last costs 600 and needs a death book), one wizard-less neutral and
    /// two wizards on a small map. Heroes cost 100 to 160 gold, upkeep 2.
    pub(crate) fn world() -> Game {
        let mut c = content_with_magic();
        let mut units = c.rules.units.defs().to_vec();
        let mut templates = Vec::new();
        for i in 0..7 {
            let id = format!("hero{i}");
            let mut u = unit(&id, MovementType::Walking, 2, &[]);
            u.kind = UnitKind::Hero;
            u.cost = if i == 6 { 600 } else { 100 + 10 * i };
            u.upkeep.gold = 2;
            u.hero = Some(HeroDef {
                kind: HeroKind::Warrior,
                portrait: 0,
                hire_cost: u.cost,
            });
            units.push(u);
            templates.push(template(&id, 0, PickType::Warrior, &[]));
        }
        // The third hero is a noble one.
        templates[2].abilities = vec!["noble".into(), "super_might".into()];
        // The last is the necromancer's stand-in: it needs a death book.
        units.last_mut().unwrap().id = "necromancer".into();
        templates[6].unit = "necromancer".into();
        c.rules.units = Registry::new(units);
        c.rules.hero_templates = templates;
        let grass = c.terrains.id_of("grassland").unwrap();
        let mut g = Game::new(5);
        g.content = c;
        g.map = Some(Map::new(Grid::new(14, 8, grass), Grid::new(14, 8, grass)));
        g.add_player(P0, "A");
        g.add_player(P1, "B");
        g.add_neutral_player(PlayerId(9));
        g.init_heroes();
        g
    }

    /// Where the hero of this type is.
    pub(crate) fn state(g: &Game, kind: usize) -> HeroState {
        let hero = g.units().hero_entity(kind).unwrap();
        g.units().hero_state(hero).unwrap()
    }

    /// The experience the hero of this type keeps.
    pub(crate) fn set_xp(g: &mut Game, kind: usize, xp: u32) {
        let hero = g.units().hero_entity(kind).unwrap();
        g.world.insert_one(hero, Experience(xp)).unwrap();
    }

    /// Put the hero of this type in a state it free of any wizard (dead or gone), as if it had
    /// got there in play.
    pub(crate) fn set_state(g: &mut Game, kind: usize, state: HeroState) {
        let hero = g.units().hero_entity(kind).unwrap();
        g.world.insert_one(hero, state).unwrap();
    }

    #[test]
    fn the_game_has_one_hero_of_each_type_and_no_wizard_owns_any_at_the_start() {
        let g = world();
        // One of each, not one table for each wizard.
        let kinds: Vec<_> = g
            .units()
            .all_heroes()
            .into_iter()
            .map(|h| g.units().hero_kind(h).unwrap())
            .collect();
        assert_eq!(kinds, (0..7).collect::<Vec<_>>());
        assert_eq!(g.units().hero_type_name(0).as_deref(), Some("Ann"));
        assert!((0..7).all(|k| state(&g, k) == HeroState::Free));
        for p in [P0, P1, PlayerId(9)] {
            assert!(g.units().heroes(p).is_empty());
        }
        assert!(g.units().all_units().is_empty());
    }

    #[test]
    fn a_hired_hero_carries_its_record() {
        let mut g = world();
        let h = g.spawn_hero(P0, at(1, 1), 2).unwrap();
        let hero: Hero = (*g.world.get::<&Hero>(h).unwrap()).clone();
        assert_eq!(hero.name, "Ann");
        assert_eq!(hero.abilities.might, Tier::Super);
        assert_eq!(hero.spells, vec!["healing".to_string()]);
        assert_eq!(g.units().hero_kind(h), Some(2));
        assert_eq!(state(&g, 2), HeroState::Serving);
        // Its abilities are the unit's, and a noble hero costs nothing to keep.
        assert!(g.units().unit_has_ability(h, "noble"));
        assert!(g.units().unit_has_ability(h, "super_might"));
        assert_eq!(g.units().unit_gold_upkeep(h), 0);
        assert_eq!(g.budget(P0).noble_gold, 10);
        let plain = g.spawn_hero(P0, at(1, 1), 1).unwrap();
        assert_eq!(g.units().unit_gold_upkeep(plain), 2);
        // The same hero cannot serve twice, and another wizard cannot hire the one who serves.
        assert_eq!(g.spawn_hero(P0, at(1, 1), 2), Err(HeroError::NotAvailable));
        assert_eq!(g.spawn_hero(P1, at(1, 1), 2), Err(HeroError::NotAvailable));
        assert_eq!(g.units().heroes(P0).len(), 2);
        assert!(g.units().heroes(P1).is_empty());
        assert_eq!(g.spawn_hero(P0, at(1, 1), 40), Err(HeroError::Unknown));
    }

    #[test]
    fn a_hero_has_mana_from_skill_level_and_items_and_half_of_it_helps_at_the_fortress() {
        use crate::item::{HeroItems, Item, ItemBonus};
        use crate::rules::ItemKind;
        let mut g = world();
        g.wiz_mut(0)
            .put(Some(crate::wizard::fortress::Fortress(at(3, 3))));
        let h = g.spawn_hero(P0, at(3, 3), 0).unwrap();
        // Casting skill 1 at level 0: (1 + 1) * 1 * 5 / 2.
        assert_eq!(g.units().hero_mana(h), 5);
        assert_eq!(g.hero_casting_skill(P0), 2);
        // Level 3 (the fourth): (1 + 1) * 4 * 5 / 2.
        let _ = g.world.insert_one(h, Experience(120));
        let level = g.units().unit_level(h) as i32 + 1;
        assert_eq!(g.units().hero_mana(h), 2 * level * 5 / 2);
        // An item's spell skill adds to it.
        let before = g.units().hero_mana(h);
        let staff = Item {
            name: "Staff".into(),
            kind: ItemKind::Staff,
            cost: 100,
            icon: 0,
            bonus: ItemBonus {
                spell_skill: 7,
                ..ItemBonus::default()
            },
            powers: Vec::new(),
            charges: None,
            prefab: None,
        };
        let _ = g.world.insert_one(h, HeroItems([Some(staff), None, None]));
        assert_eq!(g.units().hero_mana(h), before + 7);
        // Away from the fortress the hero adds nothing; another wizard's hero never does.
        assert_eq!(g.hero_casting_skill(P0), (before + 7) / 2);
        let away = g.spawn_hero(P0, at(5, 5), 1).unwrap();
        assert_eq!(g.units().hero_mana(away), 5);
        assert_eq!(g.hero_casting_skill(P0), (before + 7) / 2);
        assert_eq!(g.hero_casting_skill(P1), 0);
        // A hero with no casting skill has no mana, whatever its items.
        g.world.get::<&mut Hero>(away).unwrap().casting_skill = 0;
        let _ = g.world.insert_one(away, HeroItems::default());
        assert_eq!(g.units().hero_mana(away), 0);
    }

    #[test]
    fn sage_heroes_add_research_and_fortress_heroes_add_to_the_skill_that_pays_over_turns() {
        let mut g = world();
        g.wiz_mut(0)
            .put(Some(crate::wizard::fortress::Fortress(at(3, 3))));
        let sage = g.spawn_hero(P0, at(3, 3), 0).unwrap();
        g.world.get::<&mut Hero>(sage).unwrap().abilities.sage = Tier::Normal;
        let _ = g.world.insert_one(sage, Experience(120));
        let level = g.units().unit_level(sage);
        assert!(level > 0);
        assert_eq!(
            g.units().sage_research(P0),
            crate::hero::sage(Tier::Normal, level)
        );
        assert_eq!(g.units().sage_research(P1), 0);
        // A spell paid over turns gets the heroes' skill on top of the wizard's own.
        g.init_wizard(P0, &[("life", 2)], &[], None).unwrap();
        let spell = g.content.rules.spells.id_of("healing").unwrap();
        let hero_skill = g.hero_casting_skill(P0);
        assert!(hero_skill > 0);
        let nominal = g.wizard(P0).unwrap().casting_skill();
        // Healing needs a unit to aim at, and the wizard keeps one after the hero is gone.
        let soldier = g.content.rules.units.defs()[0].id.clone();
        let soldier = g.content.rules.units.id_of(&soldier).unwrap();
        g.spawn_unit(P0, at(3, 3), soldier);
        g.scenario_casting(P0, spell, 1000);
        g.tr_mut(0).mana = 5000;
        let _ = g.process_magic();
        let left = g.wizard(P0).unwrap().casting().unwrap().remaining;
        assert_eq!(1000 - left, nominal + hero_skill);
        // The heroes' skill counts for that turn only: the next payment is the wizard's alone.
        let _ = g.process_magic();
        let after = g.wizard(P0).unwrap().casting().unwrap().remaining;
        assert_eq!(
            left - after,
            nominal + hero_skill,
            "the hero is still there"
        );
        g.remove_unit(sage, Fate::Released);
        let _ = g.process_magic();
        let last = g.wizard(P0).unwrap().casting().unwrap().remaining;
        assert_eq!(after - last, nominal);
    }

    #[test]
    fn a_wizard_has_six_heroes_at_most() {
        let mut g = world();
        for k in 0..6 {
            g.spawn_hero(P0, at(1, 1), k).unwrap();
        }
        assert_eq!(g.units().open_hero_slots(P0), 0);
        assert_eq!(g.units().hero_units(P0).len(), 6);
        assert_eq!(g.spawn_hero(P0, at(1, 1), 6), Err(HeroError::NoRoom));
        assert_eq!(g.units().open_hero_slots(P1), 6);
        // Taking one away opens a slot.
        let first = g.units().hero_units(P0)[0];
        g.remove_unit(first, Fate::Released);
        assert_eq!(g.units().open_hero_slots(P0), 1);
    }

    #[test]
    fn a_hero_that_leaves_keeps_its_experience_or_is_gone() {
        let mut g = world();
        let a = g.spawn_hero(P0, at(1, 1), 0).unwrap();
        let b = g.spawn_hero(P0, at(1, 1), 1).unwrap();
        let c = g.spawn_hero(P0, at(1, 1), 3).unwrap();
        for (h, xp) in [(a, 40), (b, 70), (c, 90)] {
            let _ = g.world.insert_one(h, Experience(xp));
        }
        g.remove_unit(a, Fate::Slain);
        g.remove_unit(b, Fate::Released);
        g.remove_unit(c, Fate::Gone);
        let xp = |g: &Game, k| g.units().hero_xp(g.units().hero_entity(k).unwrap());
        assert_eq!((state(&g, 0), xp(&g, 0)), (HeroState::Dead, 40));
        assert_eq!((state(&g, 1), xp(&g, 1)), (HeroState::Free, 70));
        assert_eq!(state(&g, 3), HeroState::Gone);
        assert_eq!(g.units().raisable_heroes(), vec![0]);
        // The body is gone, the hero stays: the same entity, without the unit parts, and
        // without an owner.
        assert_eq!(g.units().hero_entity(0), Some(a));
        assert!(g.world.get::<&Unit>(a).is_err());
        assert!(g.world.get::<&Owner>(a).is_err());
        assert!(g.units().hero_units(P0).is_empty());
        // A dead hero can be raised again, with the experience it had, by any wizard; a gone
        // one cannot.
        let again = g.spawn_hero(P1, at(2, 2), 0).unwrap();
        assert_eq!(g.world.get::<&Experience>(again).unwrap().0, 40);
        assert_eq!(g.spawn_hero(P0, at(2, 2), 3), Err(HeroError::NotAvailable));
        assert_eq!(g.units().hero_units(P1), vec![again]);
    }

    #[test]
    fn the_places_that_remove_units_tell_the_hero() {
        let mut g = world();
        // Unpaid upkeep lets a hero go: it can be hired again.
        let h = g.spawn_hero(P0, at(1, 1), 0).unwrap();
        g.tr_mut(0).gold = 0;
        let _ = g.settle_treasury(P0);
        assert!(g.world.get::<&Unit>(h).is_err());
        assert_eq!(state(&g, 0), HeroState::Free);
        // The cheat that removes a unit does too.
        let h = g.spawn_hero(P0, at(1, 1), 1).unwrap();
        assert!(g.cheat_remove_unit(h));
        assert_eq!(state(&g, 1), HeroState::Free);
        // A wizard who is defeated lets their heroes go: they are free for the others to hire.
        let h = g.spawn_hero(P1, at(3, 3), 4).unwrap();
        g.aftermath(P1, P0);
        assert!(g.world.get::<&Unit>(h).is_err());
        assert_eq!(state(&g, 4), HeroState::Free);
        assert!(g.units().heroes(P1).is_empty());
    }

    #[test]
    fn a_hero_let_go_can_be_hired_by_anyone_with_the_experience_it_has() {
        let mut g = world();
        let h = g.spawn_hero(P0, at(1, 1), 0).unwrap();
        set_xp(&mut g, 0, 10_000);
        // While it serves, the other wizard cannot hire it and is not offered it.
        g.player_component_mut::<crate::player::Offers>(P1)
            .unwrap()
            .0
            .push(crate::player::offer::Offer::Hero {
                kind: 1,
                price: 110,
            });
        assert_eq!(g.spawn_hero(P1, at(2, 2), 0), Err(HeroError::NotAvailable));
        // Hiring a hero takes it off every offer; an offer for another hero stays.
        g.spawn_hero(P0, at(1, 1), 1).unwrap();
        assert!(g.offers(P1).is_empty());
        // Let go, it is free again with its experience, and the price shows it.
        g.remove_unit(h, Fate::Released);
        assert_eq!(state(&g, 0), HeroState::Free);
        assert!(g.units().hero_kept_level(0) > 0);
        assert!(g.units().hero_hire_price(P1, 0).unwrap() > 100);
        let again = g.spawn_hero(P1, at(2, 2), 0).unwrap();
        assert_eq!(again, h);
        assert_eq!(g.units().hero_units(P1), vec![h]);
        assert_eq!(g.world.get::<&Experience>(h).unwrap().0, 10_000);
    }

    #[test]
    fn the_price_follows_the_kept_level_and_the_retort() {
        let mut g = world();
        let cost = g.units().hero_hire_price(P0, 0).unwrap();
        assert_eq!(cost, 100);
        set_xp(&mut g, 0, 10_000);
        let level = crate::hero::kind::level_of(
            &g.content.rules,
            crate::hero::kind::unit_of(&g.content.rules, 0).unwrap(),
            10_000,
        );
        assert_eq!(
            g.units().hero_hire_price(P0, 0),
            Some(100 + level as i32 * 100 / 4)
        );
        g.wizard_mut(crate::player::PlayerId(0))
            .unwrap()
            .get_mut::<crate::wizard::sheet::Retorts>()
            .0
            .insert(crate::wizard::magic::Retort::Charismatic);
        assert_eq!(
            g.units().hero_hire_price(P0, 0),
            Some((100 + level as i32 * 100 / 4) / 2)
        );
        assert_eq!(g.units().hero_hire_price(P0, 99), None);
    }

    #[test]
    fn fame_and_books_decide_who_is_offered() {
        let mut g = world();
        // Fame 0: only heroes costing less than 110 gold qualify, and never the necromancer.
        for _ in 0..50 {
            let k = g.pick_hero(P0, false, Class::Any);
            assert!(matches!(k, None | Some(0)), "{k:?}");
        }
        g.set_fame(crate::player::PlayerId(0), 50);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..400 {
            if let Some(k) = g.pick_hero(P0, false, Class::Any) {
                seen.insert(k);
            }
        }
        assert!(seen.len() >= 4 && !seen.contains(&6), "{seen:?}");
        // A summoning ignores the fame; the death book unlocks the necromancer.
        g.wizard_mut(crate::player::PlayerId(0))
            .unwrap()
            .get_mut::<crate::wizard::sheet::Books>()
            .0
            .insert("death".into(), 1);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..800 {
            if let Some(k) = g.pick_hero(P0, true, Class::Any) {
                seen.insert(k);
            }
        }
        assert!(seen.contains(&6), "{seen:?}");
        // Those who serve, are dead or are gone are not offered.
        for k in 0..6 {
            set_state(&mut g, k, HeroState::Dead);
        }
        for _ in 0..100 {
            let k = g.pick_hero(P0, true, Class::Any);
            assert!(matches!(k, None | Some(6)), "{k:?}");
        }
    }

    #[test]
    fn the_heroes_survive_a_save() {
        let mut g = world();
        let h = g.spawn_hero(P0, at(1, 1), 2).unwrap();
        let _ = g.world.insert_one(h, Experience(33));
        let dead = g.spawn_hero(P0, at(1, 1), 1).unwrap();
        g.remove_unit(dead, Fate::Slain);
        let text = ron::to_string(&g.snapshot()).unwrap();
        let back = crate::game::save::Snapshot::from_text(&text)
            .unwrap()
            .restore(g.content.clone())
            .unwrap();
        {
            let all = |g: &Game| -> Vec<_> {
                g.units()
                    .all_heroes()
                    .into_iter()
                    .map(|e| {
                        (
                            (*g.world.get::<&Hero>(e).unwrap()).clone(),
                            g.units().hero_state(e),
                            g.units().hero_xp(e),
                            g.serial_of(e),
                            g.units().hero_units(P0).contains(&e),
                        )
                    })
                    .collect()
            };
            assert_eq!(all(&back), all(&g));
        }
        let heroes = back.units().hero_units(P0);
        assert_eq!(heroes.len(), 1);
        assert_eq!(
            *back.world.get::<&Hero>(heroes[0]).unwrap(),
            *g.world.get::<&Hero>(h).unwrap()
        );
        assert_eq!(back.units().hero_kind(heroes[0]), Some(2));
        assert!(back.units().unit_has_ability(heroes[0], "noble"));
        assert_eq!(back.units().raisable_heroes(), vec![1]);
    }

    fn sword() -> crate::item::Item {
        crate::item::Item {
            name: "Sword".into(),
            kind: crate::rules::ItemKind::Sword,
            cost: 100,
            icon: 0,
            bonus: crate::item::ItemBonus::default(),
            powers: Vec::new(),
            charges: None,
            prefab: None,
        }
    }

    fn armed_hero(g: &mut Game, owner: PlayerId) -> hecs::Entity {
        let h = g.spawn_hero(owner, at(1, 1), 0).unwrap();
        let mut items = crate::item::HeroItems::default();
        items.0[0] = Some(sword());
        g.world.insert_one(h, items).unwrap();
        h
    }

    fn in_armory(g: &Game, player: PlayerId) -> usize {
        let a = &g
            .player_component::<crate::item::armory::Armory>(player)
            .unwrap();
        a.vault.iter().flatten().count() + a.awards.len()
    }

    #[test]
    fn the_items_of_a_slain_hero_go_to_its_wizard_and_those_of_a_gone_one_are_destroyed() {
        let mut g = world();
        let slain = armed_hero(&mut g, P0);
        g.remove_unit(slain, Fate::Slain);
        assert_eq!(in_armory(&g, P0), 1);
        let events = g.drain_item_awards();
        assert!(matches!(
            events.as_slice(),
            [Event::ItemsAwarded { player, items, lost: 0 }] if *player == P0 && items == &["Sword"]
        ));
        let gone = armed_hero(&mut g, P0);
        g.remove_unit(gone, Fate::Gone);
        assert_eq!(in_armory(&g, P0), 1, "nothing more");
        assert!(g.drain_item_awards().is_empty());
    }

    #[test]
    fn the_victor_takes_the_items_of_a_hero_that_fell_in_a_battle_unless_it_is_neutral() {
        let mut g = world();
        let loser = armed_hero(&mut g, P0);
        g.remove_unit_to(loser, Fate::Slain, Some(P1));
        assert_eq!((in_armory(&g, P0), in_armory(&g, P1)), (0, 1));
        let neutral = g.new_city_player();
        let other = armed_hero(&mut g, P0);
        g.remove_unit_to(other, Fate::Slain, Some(neutral));
        assert_eq!(
            in_armory(&g, P0) + in_armory(&g, P1),
            1,
            "a neutral winner takes nothing"
        );
        assert_eq!(in_armory(&g, neutral), 0);
    }

    #[test]
    fn an_armory_that_is_full_loses_the_rest_and_the_wizard_is_told_how_many() {
        let mut g = world();
        let room = crate::item::armory::VAULT_SLOTS + crate::item::armory::MAX_AWARDS;
        for _ in 0..room {
            g.give_item(P1, sword());
        }
        let hero = armed_hero(&mut g, P0);
        g.remove_unit_to(hero, Fate::Slain, Some(P1));
        let events = g.drain_item_awards();
        assert!(matches!(
            events.as_slice(),
            [Event::ItemsAwarded { lost: 1, .. }]
        ));
        assert_eq!(in_armory(&g, P1), room);
    }

    #[test]
    fn a_hero_killed_in_a_battle_leaves_its_items_to_the_winner() {
        let mut g = world();
        let hero = armed_hero(&mut g, P0);
        let walker = g.content.rules.units.id_of("walker").unwrap();
        let attackers: Vec<_> = (0..8).map(|_| g.spawn_unit(P1, at(2, 1), walker)).collect();
        g.auto_resolve(&attackers, &[hero]);
        assert!(g.world.get::<&Unit>(hero).is_err(), "the hero fell");
        assert_eq!((in_armory(&g, P0), in_armory(&g, P1)), (0, 1));
        let told = g.drain_item_awards();
        assert!(matches!(
            told.as_slice(),
            [Event::ItemsAwarded { player, .. }] if *player == P1
        ));
    }
}
