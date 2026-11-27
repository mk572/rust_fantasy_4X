//! On another player's turn a player may only react to what happened to it
//! (`docs/turn-structure.md` 10.4).

use std::cell::Cell;
use std::rc::Rc;

use f4x_battle::battle::BattleCommand;
use f4x_battle::side::Side;
use f4x_core::diplomacy::{DiplomacyCommand, ProposalKind, Treaty};
use f4x_core::item::armory::{ItemSpot, Placed, Spot};
use f4x_core::item::Item;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::offer::Offer;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::world::save::ContentCheck;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::{UnitId, UnitKind};
use f4x_master::{Controller, Game, Master, Reaction, SaveFile, Seat, Status, Until};

use crate::support::{content, games, text};

/// Plays until `done` is set, then ends its turn once.
struct Sitter {
    done: Rc<Cell<bool>>,
}

impl Controller for Sitter {
    fn play(&mut self, _: &mut Game, _: PlayerId, _: &mut Vec<Event>) -> Status {
        if self.done.replace(false) {
            Status::Done
        } else {
            Status::Waiting
        }
    }
}

/// A game in the turn of the wizard `a`, with the wizard `b` looking on.
struct Table {
    game: Game,
    master: Master<'static>,
    done: Rc<Cell<bool>>,
    a: PlayerId,
    b: PlayerId,
}

impl Table {
    fn new() -> Self {
        let (_, mut game) = games().remove(0);
        let wizards: Vec<PlayerId> = game
            .turn_players()
            .iter()
            .copied()
            .filter(|&p| game.is_wizard(p))
            .collect();
        let (a, b) = (wizards[0], wizards[1]);
        let done = Rc::new(Cell::new(false));
        let seats = wizards
            .iter()
            .map(|&p| Seat::new(p, Box::new(Sitter { done: done.clone() })))
            .collect();
        let mut master = Master::new(seats);
        master.run(&mut game, Until::Waiting);
        assert_eq!(game.turn(), Some(a));
        Self {
            game,
            master,
            done,
            a,
            b,
        }
    }

    /// End the turn being played and go on to the next one a wizard plays.
    fn next_turn(&mut self) -> Option<PlayerId> {
        self.done.set(true);
        self.master.run(&mut self.game, Until::Waiting);
        self.game.turn()
    }

    /// `command` of `player` is run.
    #[track_caller]
    fn takes(&mut self, player: PlayerId, command: Command) -> Vec<Event> {
        let events = self.game.apply(player, command.clone());
        let refused: Vec<&Event> = events
            .iter()
            .filter(|e| matches!(e, Event::Rejected(_)))
            .collect();
        assert!(refused.is_empty(), "{command:?}: {refused:?}");
        events
    }

    /// `command` of `player` is turned down because it is not their turn, and nothing changes.
    #[track_caller]
    fn waits(&mut self, player: PlayerId, command: Command) {
        let before = text(self.game.world());
        let events = self.game.apply(player, command.clone());
        assert_eq!(
            events,
            [Event::Rejected(Rejection::NotYourTurn)],
            "{command:?}"
        );
        assert_eq!(text(self.game.world()), before, "{command:?}");
    }

    /// Two empty squares of grassland side by side, away from every city and unit.
    fn ground(&self) -> (Position, Position) {
        let map = self.game.map().expect("a map");
        let free = |p: Position| {
            self.game.units().units_at(p).is_empty()
                && self.game.economy().terrain_class_at(p) == Some(TerrainClass::Grassland)
                && self.game.economy().city_at(p).is_none()
        };
        let plane = place_of(&self.game, self.a).plane;
        (1..map.height() - 1)
            .flat_map(|y| (1..map.width() - 1).map(move |x| Position { x, y, plane }))
            .map(|p| (p, Position { x: p.x + 1, ..p }))
            .find(|&(p, q)| free(p) && free(q))
            .expect("two squares of grassland")
    }

    /// A unit of a race with a melee attack, for `owner` at `pos`.
    fn soldier(&mut self, owner: PlayerId, pos: Position) -> hecs::Entity {
        let unit = a_unit(&self.game);
        self.game.world_mut().spawn_unit(owner, pos, unit)
    }

    /// A city of `b` with nobody in it, and a soldier of `a` beside it.
    fn undefended_city(&mut self) -> (hecs::Entity, hecs::Entity, Position) {
        let (beside, site) = self.ground();
        let (own, ..) = self.game.economy().cities_of(self.b)[0];
        let race = self.game.economy().city_race(own);
        let city = self
            .game
            .world_mut()
            .scenario_found_city(self.b, site, "Open", race, 3);
        let soldier = self.soldier(self.a, beside);
        (city, soldier, site)
    }

    fn saved_and_loaded(&self) -> Game {
        let text = self.game.save_file(Vec::new()).to_text().expect("a save");
        SaveFile::from_text(&text)
            .and_then(|s| s.restore(content(), ContentCheck::Strict))
            .expect("the save loads")
            .game
    }
}

fn place_of(w: &World, player: PlayerId) -> Position {
    let unit = w.units().player_units(player)[0];
    w.units().unit_position(unit).expect("a position")
}

fn a_unit(w: &World) -> UnitId {
    let defs = w.content().rules.units.defs();
    let normal = defs
        .iter()
        .find(|d| d.kind == UnitKind::Normal && d.race.is_some());
    UnitId::new(normal.expect("a unit of a race").num)
}

fn talk(command: DiplomacyCommand) -> Command {
    Command::Diplomacy(command)
}

fn war(player: PlayerId, on: PlayerId) -> Command {
    talk(DiplomacyCommand::DeclareWar { player, on })
}

fn gift(from: PlayerId, to: PlayerId) -> Command {
    talk(DiplomacyCommand::GiveGold {
        from,
        to,
        amount: 25,
    })
}

/// A pact the first wizard offers the second, who has gold to pay with.
fn asked() -> Table {
    let mut t = Table::new();
    let (a, b) = (t.a, t.b);
    t.game.world_mut().make_contact(a, b);
    t.game.treasury_mut(b).expect("a treasury").gold = HalfInt::from_int(1000);
    let kind = ProposalKind::Pact;
    t.takes(
        a,
        talk(DiplomacyCommand::Propose {
            from: a,
            to: b,
            kind,
        }),
    );
    t
}

#[test]
fn another_players_turn_takes_no_ordinary_order() {
    let mut t = Table::new();
    let (a, b) = (t.a, t.b);
    t.game.world_mut().make_contact(a, b);
    let unit = t.game.units().player_units(b)[0];
    let tax = |player| Command::SetTaxRate { player, rate: 1 };
    t.waits(b, tax(b));
    t.waits(b, Command::DismissUnit { unit });
    t.waits(b, Command::Resign { player: b });
    t.waits(b, war(b, a));
    let kind = ProposalKind::Pact;
    t.waits(
        b,
        talk(DiplomacyCommand::Propose {
            from: b,
            to: a,
            kind,
        }),
    );
    t.takes(a, tax(a));
    assert_eq!(t.next_turn(), Some(b));
    t.takes(b, tax(b));
    t.waits(a, tax(a));
}

#[test]
fn whoever_is_asked_may_pay_on_the_askers_turn_until_it_has_answered() {
    let mut t = asked();
    let (a, b) = (t.a, t.b);
    t.takes(b, gift(b, a));
    t.takes(
        b,
        talk(DiplomacyCommand::Answer {
            player: b,
            from: a,
            accept: false,
        }),
    );
    t.waits(b, gift(b, a));
    t.waits(b, war(b, a));
}

#[test]
fn whoever_is_asked_may_declare_war_on_the_asker() {
    let mut t = asked();
    let (a, b) = (t.a, t.b);
    t.takes(b, war(b, a));
    assert_eq!(t.game.treaty(a, b), Treaty::War);
}

#[test]
fn whoever_loses_a_city_may_declare_war_on_the_taker_until_the_turn_ends() {
    let mut t = Table::new();
    let (a, b) = (t.a, t.b);
    t.game.world_mut().make_contact(a, b);
    let (city, soldier, site) = t.undefended_city();
    t.waits(b, war(b, a));
    t.takes(
        a,
        Command::MoveStack {
            units: vec![soldier],
            to: site,
        },
    );
    assert_eq!(t.game.economy().city_owner(city), Some(a), "taken");
    let lost = Reaction::CityTaken { player: b, by: a };
    assert!(
        t.game.reactions().contains(&lost),
        "{:?}",
        t.game.reactions()
    );

    // It is saved with the game.
    let mut loaded = t.saved_and_loaded();
    assert_eq!(loaded.reactions(), t.game.reactions());
    let events = loaded.apply(b, war(b, a));
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );

    // And lapses with the turn.
    t.next_turn();
    assert!(!t.game.reactions().contains(&lost));
}

#[test]
fn a_banished_wizard_may_resign_on_the_turn_that_banished_it() {
    let mut t = Table::new();
    let (a, b) = (t.a, t.b);
    // A wizard with another city and the means to come back is banished, not defeated.
    let (beside, site) = t.ground();
    let fortress = t.game.economy().fortress_city(b).expect("a fortress city");
    let race = t.game.economy().city_race(fortress);
    t.game
        .world_mut()
        .scenario_found_city(b, site, "Spare", race, 4);
    let skill = t.game.content().rules.config.wizard.resign_below_skill;
    t.game.scenario_set_casting_skill(b, skill);
    t.game.treasury_mut(b).expect("a treasury").mana = HalfInt::from_int(1000);
    let cities = t.game.economy().cities();
    let found = cities.iter().find(|(city, ..)| *city == fortress);
    let gate = found.expect("the fortress city").2;
    for unit in t.game.units().units_at(gate) {
        t.game.world_mut().scenario_remove_unit(unit);
    }
    let soldier = t.soldier(a, beside);
    let beside_gate = Position {
        x: gate.x + 1,
        ..gate
    };
    t.game.world_mut().cheat_move_unit(soldier, beside_gate);
    let resign = Command::Resign { player: b };
    t.waits(b, resign.clone());
    t.takes(
        a,
        Command::MoveStack {
            units: vec![soldier],
            to: gate,
        },
    );
    assert!(t.game.wizard(b).expect("a wizard").is_banished());
    t.takes(b, resign);
}

#[test]
fn wizards_who_meet_may_greet_each_other() {
    let mut t = Table::new();
    let (a, b) = (t.a, t.b);
    let greet = talk(DiplomacyCommand::Greeted { player: b, with: a });
    t.waits(b, greet.clone());
    t.game.world_mut().make_contact(a, b);
    // The meeting is told with the events of the next command.
    t.takes(a, Command::SetTaxRate { player: a, rate: 1 });
    t.takes(b, greet);
}

#[test]
fn items_won_in_another_players_turn_may_be_sold_until_it_ends() {
    let mut t = Table::new();
    let (a, b) = (t.a, t.b);
    let (here, there) = t.ground();
    let kinds = t.game.content().rules.hero_templates.len();
    let world = t.game.world_mut();
    let hero = (0..kinds)
        .find_map(|k| world.spawn_hero(a, here, k).ok())
        .expect("a hero");
    let def = t.game.units().unit_def(hero).expect("a hero's unit");
    let slot = def.hero.as_ref().expect("a hero").kind.slots()[0];
    let defs = t.game.content().rules.items.defs().to_vec();
    let carried = defs.iter().map(Item::from_def).find(|i| slot.fits(i.kind));
    let carried = carried.expect("an item for the hero's first slot");
    let Placed::Vault(from) = t.game.world_mut().give_item(a, carried) else {
        panic!("the vault of {a:?} is full");
    };
    // Carrying it that far costs mana.
    t.game.treasury_mut(a).expect("a treasury").mana = HalfInt::from_int(10_000);
    t.takes(
        a,
        Command::MoveItem {
            player: a,
            from: ItemSpot::Vault(from),
            to: ItemSpot::Hero(hero, 0),
            pay: true,
        },
    );
    let Placed::Vault(own) = t.game.world_mut().give_item(b, Item::from_def(&defs[0])) else {
        panic!("the vault of {b:?} is full");
    };
    let sell = Command::SellItem {
        player: b,
        from: Spot::Vault(own),
    };
    t.waits(b, sell.clone());
    let guard = t.soldier(b, there);
    t.takes(
        a,
        Command::BeginBattle {
            attackers: vec![hero],
            defenders: vec![guard],
        },
    );
    t.game.scenario_edit_battle(|battle| {
        for u in battle.units.iter_mut().filter(|u| u.side == Side::Attacker) {
            u.kill();
            u.status = f4x_battle::unit::Status::Dead;
        }
    });
    let battle = t.game.battle().expect("a battle");
    let to_play = battle.player_of(battle.active);
    t.takes(to_play, Command::Battle(BattleCommand::EndTurn));
    assert!(t.game.reactions().contains(&Reaction::Items { player: b }));
    t.takes(b, sell.clone());
    t.next_turn();
    assert!(!t.game.reactions().contains(&Reaction::Items { player: b }));
}

#[test]
fn whoever_is_offered_a_sold_item_may_answer() {
    let mut t = Table::new();
    let (a, b) = (t.a, t.b);
    t.game.world_mut().make_contact(a, b);
    for p in t.game.wizards() {
        t.game.treasury_mut(p).expect("a treasury").gold = HalfInt::from_int(30_000);
    }
    let def = t.game.content().rules.items.defs()[0].clone();
    let Placed::Vault(slot) = t.game.world_mut().give_item(a, Item::from_def(&def)) else {
        panic!("the vault of {a:?} is full");
    };
    t.waits(
        b,
        Command::RejectOffer {
            player: b,
            index: 0,
        },
    );
    t.takes(
        a,
        Command::SellItem {
            player: a,
            from: Spot::Vault(slot),
        },
    );
    let offers = PlayerView::new(&t.game, b).offers();
    let index = offers
        .iter()
        .position(|o| matches!(o, Offer::Resale { .. }))
        .expect("the item is offered on");
    t.takes(b, Command::RejectOffer { player: b, index });
}

#[test]
fn what_opens_or_answers_a_reaction_cannot_be_taken_back() {
    let mut t = Table::new();
    let (a, b) = (t.a, t.b);
    t.game.set_undo(true);
    let tax = |rate| Command::SetTaxRate { player: a, rate };
    t.takes(a, tax(1));
    assert!(t.game.can_undo(), "an ordinary order");
    // A meeting, told with the next order.
    t.game.world_mut().make_contact(a, b);
    t.takes(a, tax(2));
    assert!(!t.game.can_undo(), "a first contact");
    t.takes(a, tax(1));
    assert!(t.game.can_undo());
    let kind = ProposalKind::Pact;
    t.takes(
        a,
        talk(DiplomacyCommand::Propose {
            from: a,
            to: b,
            kind,
        }),
    );
    assert!(!t.game.can_undo(), "a proposal");
    t.takes(a, tax(2));
    assert!(t.game.can_undo());
    t.takes(b, talk(DiplomacyCommand::Greeted { player: b, with: a }));
    assert!(!t.game.can_undo(), "a reaction");
}
