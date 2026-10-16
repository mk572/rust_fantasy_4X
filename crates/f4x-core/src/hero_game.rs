//! The game's side of the hero roster (task 219): rolling the roster when a game starts, bringing
//! a hero into service as a unit, and the one place every unit that leaves the game goes through
//! so that a hero's record follows it (`remove_unit`).
//!
//! The rules themselves are in [`roster`](crate::roster) and [`hero`](crate::hero); hiring
//! with gold, summoning, offers and resurrection (tasks 48 and 220) call what is here.

use crate::city::{PlayerId, Position};
use crate::hero::Hero;
use crate::magic::Retort;
use crate::roster::{self, Availability, Class, Fate, Roster, Standing, MAX_HEROES};
use crate::units::{Experience, Owner, Unit};
use crate::Game;

/// Why a hero could not be brought into service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroError {
    /// The wizard has no record of this hero type (no roster, or no such type).
    Unknown,
    /// The hero is serving already, or gone for good.
    NotAvailable,
    /// The wizard has six heroes.
    NoRoom,
}

impl Game {
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

    /// A random hero type the wizard can be offered or summon (`Pick_Random_Hero`).
    pub fn pick_hero(&mut self, player: PlayerId, zero_cost: bool, class: Class) -> Option<usize> {
        let standing = self.hero_standing(player);
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

    /// What the wizard pays to hire this hero type now: the cost plus a quarter of it for
    /// each level the hero kept from earlier service, half for a Charismatic wizard.
    pub fn hero_hire_price(&self, player: PlayerId, kind: usize) -> Option<i32> {
        let rules = &self.content.rules;
        let record = self.player(player)?.roster.get(kind)?;
        let unit = roster::unit_of(rules, kind)?;
        let cost = rules.units.get(unit).hero.as_ref()?.hire_cost as i32;
        let level = roster::level_of(rules, unit, record.xp);
        let charismatic = self
            .magic(player)
            .is_some_and(|m| m.has(Retort::Charismatic));
        Some(roster::hire_cost(cost, level, charismatic))
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
        if self.open_hero_slots(player) == 0 {
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
    /// record follows it (`Fate` says which way). Anything the unit carries goes with it; the
    /// items of a fallen hero are task 221's to pass on, from here. The removals that do not come
    /// through here cannot be heroes: neutral guards (`lairs.rs`), the copy made for a battle
    /// summon (`battle/spells.rs`), settlers founding a city and the engineer that melds with a
    /// node.
    pub(crate) fn remove_unit(&mut self, unit: hecs::Entity, fate: Fate) {
        if let (Some(kind), Some(owner)) = (self.hero_kind(unit), self.units().unit_owner(unit)) {
            let xp = self.world.get::<&Experience>(unit).map_or(0, |x| x.0);
            if let Some(p) = self.player_mut(owner) {
                p.roster.on_dead(kind, xp, fate);
            }
        }
        let _ = self.world.despawn(unit);
    }

    /// The wizard's roster, for the screens that list the heroes that can be hired or raised.
    pub fn roster(&self, player: PlayerId) -> Option<&Roster> {
        self.player(player).map(|p| &p.roster)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hero::Tier;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::rules::{
        HeroDef, HeroKind, HeroTemplate, MovementType, PickType, Registry, UnitKind,
    };
    use crate::testutil::{content_with_magic, unit};

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn at(x: u32, y: u32) -> Position {
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
    fn world() -> Game {
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
        assert_eq!(g.hero_kind(h), Some(2));
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
    fn a_wizard_has_six_heroes_at_most() {
        let mut g = world();
        for k in 0..6 {
            g.spawn_hero(P0, at(1, 1), k).unwrap();
        }
        assert_eq!(g.open_hero_slots(P0), 0);
        assert_eq!(g.hero_units(P0).len(), 6);
        assert_eq!(g.spawn_hero(P0, at(1, 1), 6), Err(HeroError::NoRoom));
        assert_eq!(g.open_hero_slots(P1), 6);
        // Taking one away opens a slot.
        let first = g.hero_units(P0)[0];
        g.remove_unit(first, Fate::Released);
        assert_eq!(g.open_hero_slots(P0), 1);
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
        let cost = g.hero_hire_price(P0, 0).unwrap();
        assert_eq!(cost, 100);
        g.players[0].roster.get_mut(0).unwrap().xp = 10_000;
        let level = crate::roster::level_of(
            &g.content.rules,
            crate::roster::unit_of(&g.content.rules, 0).unwrap(),
            10_000,
        );
        assert_eq!(g.hero_hire_price(P0, 0), Some(100 + level as i32 * 100 / 4));
        g.players[0]
            .magic
            .retorts
            .insert(crate::magic::Retort::Charismatic);
        assert_eq!(
            g.hero_hire_price(P0, 0),
            Some((100 + level as i32 * 100 / 4) / 2)
        );
        assert_eq!(g.hero_hire_price(P0, 99), None);
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
        let back = crate::save::Snapshot::from_text(&text)
            .unwrap()
            .restore(g.content.clone())
            .unwrap();
        assert_eq!(back.players[0].roster, g.players[0].roster);
        assert_eq!(back.players[1].roster, g.players[1].roster);
        let heroes = back.hero_units(P0);
        assert_eq!(heroes.len(), 1);
        assert_eq!(
            *back.world.get::<&Hero>(heroes[0]).unwrap(),
            *g.world.get::<&Hero>(h).unwrap()
        );
        assert_eq!(back.hero_kind(heroes[0]), Some(2));
        assert!(back.units().unit_has_ability(heroes[0], "noble"));
        assert_eq!(back.players[0].roster.dead(), vec![1]);
    }
}
