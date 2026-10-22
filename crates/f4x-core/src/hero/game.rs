//! The game's side of the hero roster (task 219): rolling the roster when a game starts, bringing
//! a hero into service as a unit, and the one place every unit that leaves the game goes through
//! so that a hero's record follows it (`remove_unit`).
//!
//! The rules themselves are in [`roster`](crate::hero::roster) and [`hero`](crate::hero); hiring
//! with gold, summoning, offers and resurrection (tasks 48 and 220) call what is here.

use crate::hero::roster::{self, Availability, Class, Fate, Roster, Standing, MAX_HEROES};
use crate::hero::Hero;
use crate::hero::HeroError;
use crate::map::Position;
use crate::player::{Owner, PlayerId};
use crate::unit::{Experience, Unit, UnitView};

use crate::game::{Event, Game};
use crate::wizard::magic::Retort;

impl UnitView<'_> {
    /// The wizard's hero units.
    pub fn hero_units(&self, player: PlayerId) -> Vec<hecs::Entity> {
        let mut out: Vec<hecs::Entity> = self
            .world
            .query::<(&Hero, &Owner)>()
            .iter()
            .filter(|(_, (_, o))| o.0 == player)
            .map(|(e, _)| e)
            .collect();
        self.sort_by_serial(&mut out);
        out
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

    /// The hero type of a hero unit: its index in the roster.
    pub fn hero_kind(&self, unit: hecs::Entity) -> Option<usize> {
        let id = self.world.get::<&Unit>(unit).ok()?.0;
        self.world.get::<&Hero>(unit).ok()?;
        roster::kind_of(&self.content.rules, id)
    }

    /// What `roster::pick` asks of the wizard: fame and the books that unlock some heroes.
    pub fn hero_standing(&self, player: PlayerId) -> Standing {
        let magic = self.magic(player);
        Standing {
            fame: self.fame(player),
            life_books: magic.map_or(0, |m| m.books_in("life")),
            death_books: magic.map_or(0, |m| m.books_in("death")),
        }
    }

    /// What the wizard pays to hire this hero type now: the cost plus a quarter of it for
    /// each level the hero kept from earlier service, half for a Charismatic wizard.
    pub fn hero_hire_price(&self, player: PlayerId, kind: usize) -> Option<i32> {
        let rules = &self.content.rules;
        let record = self
            .players
            .iter()
            .find(|p| p.id == player)?
            .roster
            .get(kind)?;
        let unit = roster::unit_of(rules, kind)?;
        let cost = rules.units.get(unit).hero.as_ref()?.hire_cost as i32;
        let level = roster::level_of(rules, unit, record.xp);
        let charismatic = self
            .magic(player)
            .is_some_and(|m| m.has(Retort::Charismatic));
        Some(roster::hire_cost(cost, level, charismatic))
    }
}

impl Game {
    /// Casting skill the wizard's heroes add (`Player_Hero_Casting_Skill`): half the mana of each
    /// hero standing on the fortress square. It pays for a spell over turns, on top of the
    /// wizard's own skill.
    pub fn hero_casting_skill(&self, player: PlayerId) -> i32 {
        let Some(fortress) = self.player(player).and_then(|p| p.fortress) else {
            return 0;
        };
        self.units()
            .hero_units(player)
            .into_iter()
            .filter(|&h| self.units().unit_position(h) == Some(fortress))
            .map(|h| self.units().hero_mana(h) / 2)
            .sum()
    }

    /// Roll every wizard's hero roster, in player order (`Init_Heroes`). Independent cities'
    /// players have none. Called once when a game is made, after everything else is placed, so
    /// the map and the starting cities of a seed stay as they were.
    pub(crate) fn init_heroes(&mut self) {
        let mut rosters = Vec::new();
        for (k, id) in self
            .players
            .iter()
            .filter(|p| !p.neutral)
            .map(|p| p.id)
            .enumerate()
        {
            rosters.push((id, roster::roll(&mut self.rng, &self.content.rules, k)));
        }
        for (id, roster) in rosters {
            if let Some(p) = self.player_mut(id) {
                p.roster = roster;
            }
        }
    }

    /// A random hero type the wizard can be offered or summon (`Pick_Random_Hero`).
    pub fn pick_hero(&mut self, player: PlayerId, zero_cost: bool, class: Class) -> Option<usize> {
        let standing = self.units().hero_standing(player);
        let roster = &self.players.iter().find(|p| p.id == player)?.roster;
        roster::pick(
            &mut self.rng,
            &self.content.rules,
            roster,
            &standing,
            zero_cost,
            class,
        )
    }

    /// Bring a hero into the wizard's army at `pos`: a free hero (hired, summoned) or a dead one
    /// (resurrected), with the abilities, spells, name and experience of its roster record. No
    /// gold or mana is taken here; whoever calls this has charged for it.
    pub fn spawn_hero(
        &mut self,
        player: PlayerId,
        pos: Position,
        kind: usize,
    ) -> Result<hecs::Entity, HeroError> {
        let record = self
            .player(player)
            .and_then(|p| p.roster.get(kind))
            .ok_or(HeroError::Unknown)?
            .clone();
        if !matches!(record.state, Availability::Free | Availability::Dead) {
            return Err(HeroError::NotAvailable);
        }
        if self.units().open_hero_slots(player) == 0 {
            return Err(HeroError::NoRoom);
        }
        let unit = roster::unit_of(&self.content.rules, kind).ok_or(HeroError::Unknown)?;
        let entity = self.spawn_unit(player, pos, unit);
        let _ = self.world.insert_one(entity, Experience(record.xp));
        let _ = self.world.insert_one(
            entity,
            Hero {
                name: record.name,
                abilities: record.abilities,
                casting_skill: record.casting_skill,
                spells: record.spells,
            },
        );
        if let Some(p) = self.player_mut(player) {
            p.roster.on_hired(kind);
        }
        Ok(entity)
    }

    /// Take a unit out of the game for good: the one way to do it, so that a hero's roster
    /// record follows it (`Fate` says which way). The items of a hero that is slain go back to
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
        if let (Some(kind), Some(owner)) = (self.units().hero_kind(unit), owner) {
            let xp = self.world.get::<&Experience>(unit).map_or(0, |x| x.0);
            if let Some(p) = self.player_mut(owner) {
                p.roster.on_dead(kind, xp, fate);
            }
            if fate == Fate::Slain {
                let items: Vec<_> = self
                    .world
                    .get::<&crate::item::HeroItems>(unit)
                    .map(|h| h.iter().cloned().collect())
                    .unwrap_or_default();
                let takes = heir.unwrap_or(owner);
                if !self.is_neutral_player(takes) {
                    for item in items {
                        let name = item.name.clone();
                        let placed = self.give_item(takes, item);
                        self.item_awards.push((
                            takes,
                            name,
                            placed != crate::item::armory::Placed::Lost,
                        ));
                    }
                }
            }
        }
        let _ = self.world.despawn(unit);
    }

    /// The items awarded to wizards since the last call, one event for each wizard: the names
    /// and how many did not fit the armory. The client opens the item screen for the human.
    pub(crate) fn drain_item_awards(&mut self) -> Vec<Event> {
        let mut events: Vec<Event> = Vec::new();
        for (player, name, kept) in std::mem::take(&mut self.item_awards) {
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

    /// The wizard's roster, for the screens that list the heroes that can be hired or raised.
    pub fn roster(&self, player: PlayerId) -> Option<&Roster> {
        self.player(player).map(|p| &p.roster)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::game::testutil::{content_with_magic, unit};
    use crate::hero::Tier;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
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
        g.players = vec![Player::new(P0, "A"), Player::new(P1, "B")];
        g.players.push(Player::new_neutral(PlayerId(9), "Town"));
        g.init_heroes();
        g
    }

    #[test]
    fn every_wizard_has_a_roster_and_a_neutral_city_has_none() {
        let g = world();
        assert_eq!(g.players[0].roster.0.len(), 7);
        assert_eq!(g.players[1].roster.0.len(), 7);
        assert!(g.players[2].roster.0.is_empty());
        // Names go by the wizard's number.
        assert_eq!(g.players[0].roster.get(0).unwrap().name, "Ann");
        assert_eq!(g.players[1].roster.get(0).unwrap().name, "Bob");
        assert!(g.players[0]
            .roster
            .0
            .iter()
            .all(|r| r.state == Availability::Free));
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
        assert_eq!(
            g.players[0].roster.get(2).unwrap().state,
            Availability::Serving
        );
        // Its abilities are the unit's, and a noble hero costs nothing to keep.
        assert!(g.units().unit_has_ability(h, "noble"));
        assert!(g.units().unit_has_ability(h, "super_might"));
        assert_eq!(g.units().unit_gold_upkeep(h), 0);
        assert_eq!(g.treasury(P0).noble_gold, 10);
        let plain = g.spawn_hero(P0, at(1, 1), 1).unwrap();
        assert_eq!(g.units().unit_gold_upkeep(plain), 2);
        // The same hero cannot serve twice, and the other wizard's record is untouched.
        assert_eq!(g.spawn_hero(P0, at(1, 1), 2), Err(HeroError::NotAvailable));
        assert_eq!(
            g.players[1].roster.get(2).unwrap().state,
            Availability::Free
        );
        assert_eq!(g.spawn_hero(P0, at(1, 1), 40), Err(HeroError::Unknown));
    }

    #[test]
    fn a_hero_has_mana_from_skill_level_and_items_and_half_of_it_helps_at_the_fortress() {
        use crate::item::{HeroItems, Item, ItemBonus};
        use crate::rules::ItemKind;
        let mut g = world();
        g.players[0].fortress = Some(at(3, 3));
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
        use crate::wizard::magic::Casting;
        let mut g = world();
        g.players[0].fortress = Some(at(3, 3));
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
        g.players[0].magic = crate::wizard::magic::Magic::new(&[("life", 2)], &[]);
        let spell = g.content.rules.spells.id_of("healing").unwrap();
        let hero_skill = g.hero_casting_skill(P0);
        assert!(hero_skill > 0);
        let nominal = g.magic(P0).unwrap().casting_skill();
        g.magic_mut(P0).unwrap().casting = Some(Casting {
            spell,
            remaining: 1000,
        });
        g.players[0].mana = 5000;
        let _ = g.process_magic();
        let left = g.magic(P0).unwrap().casting.unwrap().remaining;
        assert_eq!(1000 - left, nominal + hero_skill);
        // The heroes' skill counts for that turn only: the next payment is the wizard's alone.
        let _ = g.process_magic();
        let after = g.magic(P0).unwrap().casting.unwrap().remaining;
        assert_eq!(
            left - after,
            nominal + hero_skill,
            "the hero is still there"
        );
        g.remove_unit(sage, Fate::Released);
        let _ = g.process_magic();
        let last = g.magic(P0).unwrap().casting.unwrap().remaining;
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
        let r = &g.players[0].roster;
        assert_eq!(
            (r.get(0).unwrap().state, r.get(0).unwrap().xp),
            (Availability::Dead, 40)
        );
        assert_eq!(
            (r.get(1).unwrap().state, r.get(1).unwrap().xp),
            (Availability::Free, 70)
        );
        assert_eq!(r.get(3).unwrap().state, Availability::Gone);
        assert_eq!(r.dead(), vec![0]);
        assert!(!g.world.contains(a));
        // A dead hero can be raised again, with the experience it had; a gone one cannot.
        let again = g.spawn_hero(P0, at(2, 2), 0).unwrap();
        assert_eq!(g.world.get::<&Experience>(again).unwrap().0, 40);
        assert_eq!(g.spawn_hero(P0, at(2, 2), 3), Err(HeroError::NotAvailable));
    }

    #[test]
    fn the_places_that_remove_units_tell_the_roster() {
        let mut g = world();
        // Unpaid upkeep lets a hero go: it can be hired again.
        let h = g.spawn_hero(P0, at(1, 1), 0).unwrap();
        g.players[0].gold = 0;
        let _ = g.settle_treasury(P0);
        assert!(!g.world.contains(h));
        assert_eq!(
            g.players[0].roster.get(0).unwrap().state,
            Availability::Free
        );
        // The cheat that removes a unit does too.
        let h = g.spawn_hero(P0, at(1, 1), 1).unwrap();
        assert!(g.cheat_remove_unit(h));
        assert_eq!(
            g.players[0].roster.get(1).unwrap().state,
            Availability::Free
        );
        // A wizard who is defeated loses the heroes for good.
        let h = g.spawn_hero(P1, at(3, 3), 4).unwrap();
        g.aftermath(P1, P0);
        assert!(!g.world.contains(h));
        assert_eq!(
            g.players[1].roster.get(4).unwrap().state,
            Availability::Gone
        );
    }

    #[test]
    fn the_price_follows_the_kept_level_and_the_retort() {
        let mut g = world();
        let cost = g.units().hero_hire_price(P0, 0).unwrap();
        assert_eq!(cost, 100);
        g.players[0].roster.get_mut(0).unwrap().xp = 10_000;
        let level = crate::hero::roster::level_of(
            &g.content.rules,
            crate::hero::roster::unit_of(&g.content.rules, 0).unwrap(),
            10_000,
        );
        assert_eq!(
            g.units().hero_hire_price(P0, 0),
            Some(100 + level as i32 * 100 / 4)
        );
        g.players[0]
            .magic
            .retorts
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
        g.players[0].fame = 50;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..400 {
            if let Some(k) = g.pick_hero(P0, false, Class::Any) {
                seen.insert(k);
            }
        }
        assert!(seen.len() >= 4 && !seen.contains(&6), "{seen:?}");
        // A summoning ignores the fame; the death book unlocks the necromancer.
        g.players[0].magic.books.insert("death".into(), 1);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..800 {
            if let Some(k) = g.pick_hero(P0, true, Class::Any) {
                seen.insert(k);
            }
        }
        assert!(seen.contains(&6), "{seen:?}");
        // Those who serve, are dead or are gone are not offered.
        for k in 0..6 {
            g.players[0].roster.get_mut(k).unwrap().state = Availability::Dead;
        }
        for _ in 0..100 {
            let k = g.pick_hero(P0, true, Class::Any);
            assert!(matches!(k, None | Some(6)), "{k:?}");
        }
    }

    #[test]
    fn the_roster_and_the_hero_survive_a_save() {
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
        assert_eq!(back.players[0].roster, g.players[0].roster);
        assert_eq!(back.players[1].roster, g.players[1].roster);
        let heroes = back.units().hero_units(P0);
        assert_eq!(heroes.len(), 1);
        assert_eq!(
            *back.world.get::<&Hero>(heroes[0]).unwrap(),
            *g.world.get::<&Hero>(h).unwrap()
        );
        assert_eq!(back.units().hero_kind(heroes[0]), Some(2));
        assert!(back.units().unit_has_ability(heroes[0], "noble"));
        assert_eq!(back.players[0].roster.dead(), vec![1]);
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
        let a = &g.player(player).unwrap().armory;
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
        let neutral = g.new_city_player("Free");
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
        assert!(!g.world.contains(hero), "the hero fell");
        assert_eq!((in_armory(&g, P0), in_armory(&g, P1)), (0, 1));
        let told = g.drain_item_awards();
        assert!(matches!(
            told.as_slice(),
            [Event::ItemsAwarded { player, .. }] if *player == P1
        ));
    }
}
