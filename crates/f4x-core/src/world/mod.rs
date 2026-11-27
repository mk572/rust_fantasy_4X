//! The game itself: [`World`] owns the world and every domain's state and turns a [`Command`]
//! into [`Event`]s. Around it are the parts that keep a game going: setting one up, saving,
//! replaying and undoing.
//!
//! Tests: in world/tests.rs, and in the tests of the modules that add methods to `World`.

pub(crate) mod access;
pub mod actor;
pub mod cheat;
pub mod command;
pub mod endgame;
pub mod event;
pub mod rng;
pub mod save;
pub mod serial;
pub mod setting;
pub mod stable_ref;
#[cfg(any(test, feature = "testutil"))]
pub mod testutil;
pub mod view;

use crate::world::rng::GameRng;
use crate::{battle, city, diplomacy, map, player, threat, unit, wizard, world, world_event};
use f4x_def::content;
use f4x_def::half_int::HalfInt;

pub use command::Command;
pub use event::{DispelSite, Event, LossCause, Rejection, Shortage};

fn reject(r: Result<(), wizard::magic::MagicError>) -> Vec<Event> {
    match r {
        Ok(()) => Vec::new(),
        Err(e) => vec![Event::Rejected(Rejection::Magic(e))],
    }
}

fn events_or_reject(r: Result<Vec<Event>, wizard::magic::MagicError>) -> Vec<Event> {
    r.unwrap_or_else(|e| vec![Event::Rejected(Rejection::Magic(e))])
}

/// See [`World::net_gold_memo`].
#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum NetGoldMemo {
    #[default]
    Off,
    Empty,
    Known(player::PlayerId, f4x_def::half_int::HalfInt),
}

/// How many map locations a player can save.
pub const MAP_LOCATIONS: u8 = 8;

pub struct World {
    pub(crate) ecs: hecs::World,
    pub(crate) map: Option<map::Map>,
    pub(crate) content: std::sync::Arc<content::Content>,
    /// The name lists the game draws city, hero and rival names from (`NameBundle`). They are not
    /// saved: the names drawn are in the state, and whoever loads a game hands the lists in again
    /// with [`World::set_names`] (without them a new city is `<race id>_city_00`, and so on).
    pub(crate) names: f4x_def::names::NameBundle,
    /// The one neutral player that owns the monsters (`player::MONSTERS`); it has no
    /// data, so it is in no list of players.
    pub(crate) monsters: hecs::Entity,
    /// The player entity of each [`player::PlayerId`].
    pub(crate) player_index: std::collections::HashMap<player::PlayerId, hecs::Entity>,
    pub(crate) fog: map::fog::Fog,
    /// The vision of the computer wizard that is playing, kept for its turn (`map::vision`).
    pub(crate) vision: std::cell::RefCell<map::vision::VisionCache>,
    /// The player's net gold, worked out once while a player's cities build (`turn.rs`), for the
    /// auto-builder, which asks for it in every city. Cleared when something is completed, since
    /// that changes the upkeep. `None` outside that pass: nothing is kept.
    pub(crate) net_gold_memo: std::cell::Cell<NetGoldMemo>,
    /// The squares where a unit was put since the last command ended (`unit/sanity.rs`): the only
    /// squares that command can have pushed over the sanity limit.
    pub(crate) tile_watch: Vec<map::Position>,
    /// The figures the budget is made of while a player's cities build, patched after each
    /// completed item instead of worked out afresh (`World::kept_budget`).
    pub(crate) budget_parts: std::cell::RefCell<Option<player::finance::BudgetParts>>,
    pub(crate) next_serial: u64,
    /// What the game is played with (difficulty, node strength, ...). Chosen when the game is
    /// made and fixed after that; read through [`World::settings`].
    pub(crate) settings: world::setting::GameSettings,
    /// The battle being fought, if any. A save taken in a battle holds it too (`battle/save.rs`).
    pub battle: Option<battle::world::ActiveBattle>,
    /// Who has completed the Spell of Mastery. The game decides what that ends (`f4x-master`).
    pub(crate) mastery: Option<player::PlayerId>,
    /// Contact, relations, treaties and proposals between wizards.
    pub diplomacy: diplomacy::Diplomacy,
    /// The random events that are going on.
    pub(crate) events: world_event::Events,
    /// The clocks of the raiders and the roaming monsters.
    pub(crate) threat: threat::Threat,
    /// The map locations each player saved to come back to, by slot (see [`World::locations`]).
    locations: Vec<(player::PlayerId, u8, map::Position)>,
    /// Items that fell to wizards from slain heroes since the last command ended: the wizard,
    /// the item's name and whether the armory had room for it.
    pub(crate) item_awards: Vec<(hecs::Entity, String, bool)>,
    /// Every override (cheat) command that changed the game, oldest first.
    pub(crate) override_log: Vec<String>,
    /// The prefab items given out so far, by id (`Make_Item` gives each once a game).
    pub(crate) prefab_taken: Vec<crate::rules::ItemId>,
    /// Players of battles that play for some wizards (a learned one; see `battle::world`).
    pub(crate) battle_brains: Vec<(hecs::Entity, Box<dyn battle::world::BattleBrain>)>,
    /// The orders of auto-resolved battles, for replays.
    pub(crate) auto_orders: battle::world::AutoOrders,
    /// Set while a command the caster confirmed runs (`Command::Anyway`): a spell that would do
    /// nothing is cast anyway.
    pub(crate) confirmed: bool,
}

/// Give `entity` a component. Hecs refuses only when the entity is gone, which for a caller that
/// has just used the entity is a bug that would otherwise vanish: dev and test builds stop on it
/// (`debug_assert!`), a release build goes on, as it did before the helper (task 554).
#[track_caller]
pub(crate) fn put_in<C: hecs::Component>(
    ecs: &mut hecs::World,
    entity: hecs::Entity,
    component: C,
) {
    let put = ecs.insert_one(entity, component);
    debug_assert!(
        put.is_ok(),
        "no such entity to give a component to: {put:?}"
    );
}

/// Take a component off `entity`. A component that is not there is fine (that is what this is
/// for: "make sure it is off"); an entity that is gone is a bug, as for [`put_in`].
#[track_caller]
pub(crate) fn strip_in<C: hecs::Component>(ecs: &mut hecs::World, entity: hecs::Entity) {
    let taken = ecs.remove_one::<C>(entity);
    debug_assert!(
        !matches!(taken, Err(hecs::ComponentError::NoSuchEntity)),
        "no such entity to take a component off"
    );
}

/// Give `entity` a bundle of components; see [`put_in`].
#[track_caller]
pub(crate) fn put_bundle_in(
    ecs: &mut hecs::World,
    entity: hecs::Entity,
    bundle: impl hecs::DynamicBundle,
) {
    let put = ecs.insert(entity, bundle);
    debug_assert!(put.is_ok(), "no such entity to give components to: {put:?}");
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Give `entity` a component (or replace it); see [`put_in`].
    #[track_caller]
    pub(crate) fn put<C: hecs::Component>(&mut self, entity: hecs::Entity, component: C) {
        // A unit given a square is a unit put on it: the sanity limit looks at that square.
        if let Some(&at) = (&component as &dyn std::any::Any).downcast_ref::<map::Position>() {
            self.watch_tile(at);
        }
        put_in(&mut self.ecs, entity, component);
    }

    /// Take a component off `entity` if it has it; see [`strip_in`].
    #[track_caller]
    pub(crate) fn strip<C: hecs::Component>(&mut self, entity: hecs::Entity) {
        strip_in::<C>(&mut self.ecs, entity);
    }

    /// Give `entity` a bundle of components; see [`put_in`].
    #[track_caller]
    pub(crate) fn put_bundle(&mut self, entity: hecs::Entity, bundle: impl hecs::DynamicBundle) {
        put_bundle_in(&mut self.ecs, entity, bundle);
    }

    pub fn new() -> Self {
        let mut ecs = hecs::World::new();
        let monsters = ecs.spawn((
            player::MONSTERS,
            player::Neutral,
            unit::ledger::ArmyLedger::default(),
        ));
        Self {
            ecs,
            monsters,
            map: None,
            content: std::sync::Arc::default(),
            names: Default::default(),
            player_index: std::collections::HashMap::new(),
            fog: map::fog::Fog::default(),
            vision: Default::default(),
            net_gold_memo: Default::default(),
            tile_watch: Vec::new(),
            budget_parts: Default::default(),
            next_serial: 1,
            // The Average level of the shipped rules; a game made for play gets its level from
            // `f4x_newgame::setup::new_game`.
            settings: world::setting::GameSettings::default(),
            battle: None,
            mastery: None,
            diplomacy: diplomacy::Diplomacy::default(),
            events: world_event::Events::default(),
            threat: threat::Threat::default(),
            locations: Vec::new(),
            item_awards: Vec::new(),
            override_log: Vec::new(),
            prefab_taken: Vec::new(),
            battle_brains: Vec::new(),
            auto_orders: Default::default(),
            confirmed: false,
        }
    }

    /// The map locations a player saved, as (slot, square) in the order of the slots. A square
    /// that is not on the map any more (a save of another map) is left out.
    pub fn locations(&self, player: player::PlayerId) -> Vec<(u8, map::Position)> {
        let mut found: Vec<_> = self
            .locations
            .iter()
            .filter(|(who, _, at)| *who == player && self.on_map(*at))
            .map(|&(_, slot, at)| (slot, at))
            .collect();
        found.sort_by_key(|&(slot, _)| slot);
        found
    }

    /// Take over the saved map locations of `other`, the same game at another moment: an undo
    /// takes back orders, not the bookmarks of the view.
    pub fn keep_locations_of(&mut self, other: &World) {
        self.locations.clone_from(&other.locations);
    }

    /// Save a map location in a slot (0 to [`MAP_LOCATIONS`] - 1) of a player, replacing what
    /// the slot held. Returns false, and changes nothing, for a slot out of range or a square
    /// that is not on the map.
    pub fn save_location(&mut self, player: player::PlayerId, slot: u8, at: map::Position) -> bool {
        if slot >= MAP_LOCATIONS || !self.on_map(at) {
            return false;
        }
        self.locations
            .retain(|&(who, s, _)| !(who == player && s == slot));
        self.locations.push((player, slot, at));
        true
    }

    fn on_map(&self, at: map::Position) -> bool {
        self.map.as_ref().is_some_and(|m| {
            let plane = m.plane(at.plane);
            at.x < plane.width() && at.y < plane.height()
        })
    }

    /// The settings the game is played with.
    pub fn settings(&self) -> &world::setting::GameSettings {
        &self.settings
    }

    /// Whether a city could be founded at `pos` right now.
    pub fn can_found_city(&self, pos: map::Position) -> Result<(), city::FoundCityError> {
        // The cities are read only when the ground and the features allow a city: the AI asks
        // about many squares, most of them water or blocked, and a query costs more than the
        // check.
        city::check_site(&self.economy(), pos, || {
            self.ecs
                .query::<(&city::City, &map::Position)>()
                .iter()
                .map(|(_, (_, at))| *at)
                .collect()
        })
    }

    /// Let `player` see the tiles within `sight` (squares) of `at`.
    pub fn explore(&mut self, player: player::PlayerId, at: map::Position, sight: HalfInt) {
        if let (Some(map), true) = (self.map.as_ref(), self.fog.is_empty()) {
            self.fog = map::fog::Fog::new(map.width(), map.height());
        }
        self.fog
            .explore(player, at, sight, self.settings.basic.diagonal_extra_cost);
        self.contact_near(player, at, map::fog::squares_seen(sight));
    }

    /// Whether `id` is the monsters' owner or an independent city's player: not a wizard.
    pub fn is_neutral_player(&self, id: player::PlayerId) -> bool {
        self.player_entity(id)
            .is_some_and(|e| self.ecs.satisfies::<&player::Neutral>(e).unwrap_or(false))
    }

    /// Make the player entity for `id`, and with it a wizard called `name`.
    pub fn add_player(&mut self, id: player::PlayerId, name: impl Into<String>) -> hecs::Entity {
        debug_assert!(
            self.player_entity(id).is_none(),
            "player {id:?} is made twice"
        );
        let e = self.ecs.spawn(player::bundle(
            id,
            self.content.rules.config.wizard.start_tax_rate,
        ));
        let _ = self.ecs.insert_one(e, player::Control::Ai);
        let w = self.ecs.spawn(wizard::entity::bundle(id, name.into(), e));
        let _ = self.ecs.insert_one(e, player::WizardLink(w));
        self.player_index.insert(id, e);
        e
    }

    /// Make the player of one independent city: it has a treasury and pays its way like a wizard,
    /// but is no wizard (no magic, no victory, no AI).
    pub fn add_neutral_player(&mut self, id: player::PlayerId) -> hecs::Entity {
        debug_assert!(
            self.player_entity(id).is_none(),
            "player {id:?} is made twice"
        );
        let e = self.ecs.spawn(player::bundle(
            id,
            self.content.rules.config.wizard.start_tax_rate,
        ));
        let _ = self.ecs.insert_one(e, player::Neutral);
        self.player_index.insert(id, e);
        e
    }

    /// The read handle on a player (the monsters' owner too).
    pub fn player(&self, id: player::PlayerId) -> Option<player::entity::Player<'_>> {
        Some(player::entity::Player::new(
            &self.ecs,
            self.player_entity(id)?,
            id,
        ))
    }

    /// The wizard of a player, if it has one.
    pub fn wizard(&self, id: player::PlayerId) -> Option<wizard::entity::Wizard<'_>> {
        let w = wizard::entity::of_player(&self.ecs, self.player_entity(id)?)?;
        Some(wizard::entity::Wizard::new(&self.ecs, w))
    }

    /// A write handle on the wizard of a player, if it has one.
    pub fn wizard_mut(&mut self, id: player::PlayerId) -> Option<wizard::sheet::WizardMut<'_>> {
        let w = wizard::entity::of_player(&self.ecs, self.player_entity(id)?)?;
        Some(wizard::sheet::WizardMut::new(&mut self.ecs, w))
    }

    /// A write handle on the wizard of a player, with the rules it plays by.
    pub(crate) fn wizard_mut_with_rules(
        &mut self,
        id: player::PlayerId,
    ) -> Option<(wizard::sheet::WizardMut<'_>, &crate::rules::Rules)> {
        let w = wizard::entity::of_player(&self.ecs, self.player_entity(id)?)?;
        Some((
            wizard::sheet::WizardMut::new(&mut self.ecs, w),
            &self.content.rules,
        ))
    }

    /// Change a wizard's fame by `change`; it stays above zero.
    pub fn add_fame(&mut self, id: player::PlayerId, change: i32) {
        if let Some(mut w) = self.wizard_mut(id) {
            let mut fame = w.get_mut::<wizard::sheet::Fame>();
            fame.0 = (fame.0 + change).max(0);
        }
    }

    /// Set a wizard's fame.
    pub fn set_fame(&mut self, id: player::PlayerId, fame: i32) {
        if let Some(mut w) = self.wizard_mut(id) {
            *w.get_mut::<wizard::sheet::Fame>() = wizard::sheet::Fame(fame);
        }
    }

    /// How a player is called: its wizard's name, or its city's.
    /// The banner colour of a player; the neutral banner if it has no wizard.
    /// The wizard picture of a player.
    /// Change the wizard's components.
    pub(crate) fn wizard_set<C: hecs::Component>(&mut self, id: player::PlayerId, value: C) {
        if let Some(w) = self
            .player_entity(id)
            .and_then(|p| wizard::entity::of_player(&self.ecs, p))
        {
            let _ = self.ecs.insert_one(w, value);
        }
    }

    /// Hand a unit or city to a player: its [`player::Owner`] becomes the player's entity.
    pub(crate) fn set_owner(&mut self, thing: hecs::Entity, player: player::PlayerId) {
        if self.ecs.satisfies::<&unit::Unit>(thing) == Ok(true) {
            self.change_sides(thing, player, unit::ledger::Departure::ChangedSides);
        } else if let Some(owner) = self.player_entity(player) {
            let _ = self.ecs.insert_one(thing, player::Owner(owner));
        }
    }

    /// Give a unit to `player`; its old owner's ledger records it as having left as `how` says.
    pub(crate) fn change_sides(
        &mut self,
        unit: hecs::Entity,
        player: player::PlayerId,
        how: unit::ledger::Departure,
    ) {
        let Some(owner) = self.player_entity(player) else {
            return;
        };
        if self.units().unit_owner_entity(unit) == Some(owner) {
            return;
        }
        unit::ledger::record(&self.ecs, unit, how);
        unit::ledger::transfer(&mut self.ecs, unit, owner);
    }

    /// Whether `id` is a player of the game: a wizard's or an independent city's (not the
    /// monsters' owner).
    pub fn has_player(&self, id: player::PlayerId) -> bool {
        self.player_index.contains_key(&id)
    }

    /// The entity of a player.
    pub fn player_entity(&self, id: player::PlayerId) -> Option<hecs::Entity> {
        if id == player::MONSTERS {
            return Some(self.monsters);
        }
        self.player_index.get(&id).copied()
    }

    /// A component of a player (`Tax`, `Bonus`, `History`, `Offers`, `Defeat`, ...).
    pub fn player_component<C: hecs::Component>(
        &self,
        id: player::PlayerId,
    ) -> Option<hecs::Ref<'_, C>> {
        self.ecs.get::<&C>(self.player_entity(id)?).ok()
    }

    /// A component of a player, to change.
    pub fn player_component_mut<C: hecs::Component>(
        &mut self,
        id: player::PlayerId,
    ) -> Option<&mut C> {
        let e = self.player_entity(id)?;
        self.ecs.query_one_mut::<&mut C>(e).ok()
    }

    /// A player's gold and mana.
    /// A player's gold and mana, to change.
    pub fn treasury_mut(&mut self, id: player::PlayerId) -> Option<&mut player::Treasury> {
        self.player_component_mut(id)
    }

    /// The human wizard, whose defeat ends the game at once.
    pub fn human(&self) -> Option<player::PlayerId> {
        self.ecs
            .query::<(&player::PlayerId, &player::Control)>()
            .iter()
            .find(|(_, (_, &c))| c == player::Control::Human)
            .map(|(_, (&id, _))| id)
    }

    /// Make this player the human wizard and every other one a computer wizard (`None`: all
    /// computer wizards).
    pub fn set_human(&mut self, human: Option<player::PlayerId>) {
        for (_, (&id, control)) in self
            .ecs
            .query_mut::<(&player::PlayerId, &mut player::Control)>()
        {
            *control = if Some(id) == human {
                player::Control::Human
            } else {
                player::Control::Ai
            };
        }
    }

    /// Make this player the human wizard when the game has none yet.
    pub fn ensure_human(&mut self, id: player::PlayerId) {
        if self.human().is_none() {
            self.set_human(Some(id));
        }
    }

    /// The wizard's power at each turn, for the History screen.
    /// Does the player's Grand Vizier choose what its cities build next?
    pub(crate) fn set_grand_vizier(&mut self, id: player::PlayerId, on: bool) {
        let Some(e) = self.player_entity(id) else {
            return;
        };
        if on {
            let _ = self.ecs.insert_one(e, player::GrandVizier);
        } else {
            let _ = self.ecs.remove_one::<player::GrandVizier>(e);
        }
    }

    /// Who took the last city of a defeated player.
    /// Has the player lost everything?
    #[cfg(test)]
    pub(crate) fn tr(&self, i: usize) -> player::Treasury {
        self.player(self.player_ids()[i])
            .expect("player succeeds")
            .treasury()
    }

    #[cfg(test)]
    pub(crate) fn tr_mut(&mut self, i: usize) -> &mut player::Treasury {
        let id = self.player_ids()[i];
        self.treasury_mut(id).expect("treasury_mut succeeds")
    }

    #[cfg(test)]
    pub(crate) fn wiz_mut(&mut self, i: usize) -> wizard::sheet::WizardMut<'_> {
        let id = self.player_ids()[i];
        self.wizard_mut(id).expect("wizard_mut succeeds")
    }

    /// Every player's id: wizards and independent cities' players, ascending.
    pub fn player_ids(&self) -> Vec<player::PlayerId> {
        let mut ids: Vec<_> = self.player_index.keys().copied().collect();
        ids.sort();
        ids
    }

    /// Run a command. Whether the game still takes commands is the game's to say (`f4x-master`).
    pub fn apply(&mut self, command: Command, rng: &mut GameRng) -> Vec<Event> {
        self.execute(command, rng)
    }

    /// Everything the players have explored, summed: it only ever grows.
    pub fn explored_total(&self) -> usize {
        self.player_ids()
            .into_iter()
            .map(|id| {
                map::Plane::all()
                    .map(|plane| self.fog.explored_count(id, plane))
                    .sum::<usize>()
            })
            .sum()
    }

    fn execute(&mut self, cmd: Command, rng: &mut GameRng) -> Vec<Event> {
        // A command for a player the game does not have is refused before anything runs.
        if self
            .command_actor(&cmd)
            .is_some_and(|p| self.player_entity(p).is_none())
        {
            return vec![Event::Rejected(Rejection::NoSuchPlayer)];
        }
        // So is one that names a square off the map.
        let squares: Vec<map::Position> = match &cmd {
            Command::CreateUnit { pos, .. }
            | Command::FoundCity { pos, .. }
            | Command::PlaceFortress { pos, .. } => vec![*pos],
            Command::MoveStack { to, .. }
            | Command::GoTo { to, .. }
            | Command::EnterSite { to, .. }
            | Command::EarthGate { to, .. } => vec![*to],
            Command::BuildRoad { path, .. } => path.clone(),
            _ => Vec::new(),
        };
        if squares.iter().any(|&at| !self.on_map(at)) {
            return vec![Event::Rejected(Rejection::Move(
                unit::movement::MoveError::OffMap,
            ))];
        }
        let vision = map::vision::VisionEffect::of(&cmd);
        let _paused = unit::kept::pause();
        let mut events = self.execute_command(cmd, rng);
        self.settle(vision, &mut events);
        events
    }

    /// Advance the world by what the turn order does to it: `steps` calls one or more of the
    /// turn operations ([`World::start_round`] and the others), and the world is tidied up once
    /// afterwards, as after a command. The order of the steps is the caller's (`f4x-master`).
    pub fn advance(&mut self, steps: impl FnOnce(&mut Self) -> Vec<Event>) -> Vec<Event> {
        let _paused = unit::kept::pause();
        let mut events = steps(self);
        self.settle(map::vision::VisionEffect::Recount, &mut events);
        events
    }

    /// What follows every change of the world: the dead and the misplaced units go, and what the
    /// change raised is told.
    fn settle(&mut self, vision: map::vision::VisionEffect, events: &mut Vec<Event>) {
        events.extend(self.remove_units_without_hit_points());
        events.extend(self.enforce_tile_sanity());
        self.end_dormant_relations();
        // What hooks in battles, spells and the turn raised between wizards comes last.
        events.extend(self.drain_diplomacy_events());
        events.extend(self.drain_item_awards());
        self.end_patrols_on_sight();
        self.end_orders_on_sight(events);
        self.vision_after(vision, events);
        #[cfg(any(test, feature = "testutil"))]
        debug_assert_eq!(
            crate::check::army_problems(self),
            Vec::<String>::new(),
            "a unit changed hands past the ledger"
        );
    }

    fn execute_command(&mut self, cmd: Command, rng: &mut GameRng) -> Vec<Event> {
        match cmd {
            Command::CreateUnit { owner, pos, unit } => {
                let unit = self.spawn_unit(owner, pos, unit);
                vec![Event::UnitCreated { unit, pos }]
            }
            Command::MoveStack { units, to } => {
                // Moving ends a unit's standby, but a move the game refuses changes nothing.
                let standing: Vec<_> = units
                    .iter()
                    .filter_map(|&u| Some((u, self.units().unit_standby(u)?)))
                    .collect();
                // And its standing order: the player has taken it over.
                let ordered: Vec<_> = units
                    .iter()
                    .filter_map(|&u| {
                        let order = unit::ledger::order_of(&self.ecs, u)?;
                        Some((u, order, unit::ledger::seen_of(&self.ecs, u)))
                    })
                    .collect();
                for &u in &units {
                    let _ = self.ecs.remove_one::<unit::Standby>(u);
                }
                self.end_orders_of_moved(&units);
                self.start_from_tower_on(&units, to);
                let events = self.do_move(&units, to, rng);
                if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
                    for (u, standby) in standing {
                        let _ = self.ecs.insert_one(u, standby);
                    }
                    for (u, order, seen) in ordered {
                        unit::ledger::give_order(&self.ecs, u, order, &seen);
                    }
                }
                events
            }
            Command::SetStandby { units, standby } => {
                let seen = if standby == Some(unit::Standby::Patrol) {
                    self.in_sight_of(&units, |_, _| true)
                } else {
                    Vec::new()
                };
                for u in units {
                    match standby {
                        Some(s) => {
                            // Done is for this turn only: a patrol stays one (see ReMoM
                            // `MainScr.c`, `Stack_Action`).
                            if s == unit::Standby::Done
                                && self.units().unit_standby(u) == Some(unit::Standby::Patrol)
                            {
                                continue;
                            }
                            // A patrol takes the place of an order: the unit stays where it is.
                            if s == unit::Standby::Patrol {
                                unit::ledger::end_order(&self.ecs, u);
                                let _ = self.ecs.insert_one(u, unit::PatrolSeen(seen.clone()));
                            }
                            let _ = self.ecs.insert_one(u, s);
                        }
                        None => {
                            let _ = self.ecs.remove_one::<unit::Standby>(u);
                        }
                    }
                }
                Vec::new()
            }
            Command::GoTo { units, to } => self.go_to(&units, to, rng),
            Command::GiveOrder { units, errand } => self
                .give_errand(&units, errand, rng)
                .unwrap_or_else(|why| vec![Event::Rejected(why)]),
            Command::ClearOrders { units } => {
                for u in units {
                    unit::ledger::end_order(&self.ecs, u);
                }
                Vec::new()
            }
            Command::BeginBattle {
                attackers,
                defenders,
            } => self
                .start_battle(&attackers, &defenders, rng)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::BattleStart(e))]),
            Command::AutoResolve {
                attackers,
                defenders,
                mana_budget,
            } => self.auto_resolve_with(&attackers, &defenders, mana_budget, rng),
            Command::Defend {
                attackers,
                fighters,
                auto: true,
                mana,
            } => {
                let defender = fighters.first().and_then(|&u| self.units().unit_owner(u));
                let budget = defender.zip(mana);
                self.auto_resolve_with(&attackers, &fighters, budget, rng)
            }
            Command::Defend {
                attackers,
                fighters,
                auto: false,
                ..
            } => self
                .start_battle(&attackers, &fighters, rng)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::BattleStart(e))]),
            Command::EnterSite { units, to } => self
                .enter_site(&units, to, rng)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Site(e))]),
            Command::Anyway(inner) => {
                self.confirmed = true;
                let events = self.execute_command(*inner, rng);
                self.confirmed = false;
                events
            }
            Command::Battle(command) => self
                .battle_command(command, rng)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Battle(e))]),
            Command::Purify { units } => self
                .purify(&units)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Purify(e))]),
            Command::BuildRoad { units, path } => self
                .build_road(&units, path)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Road(e))]),
            Command::Meld { units } => self
                .meld(&units, rng)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Meld(e))]),
            Command::ShiftPlane { units } => {
                let from = units.first().and_then(|&u| self.units().unit_position(u));
                match (self.shift_plane(&units), from) {
                    (Ok(to), Some(from)) => {
                        self.end_orders_of_moved(&units);
                        vec![Event::UnitsMoved { units, from, to }]
                    }
                    (Err(e), _) => vec![Event::Rejected(Rejection::Move(e))],
                    (Ok(_), None) => Vec::new(),
                }
            }
            Command::EarthGate { units, to } => self
                .use_earth_gate(&units, to)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Move(e))]),
            Command::FoundCity {
                owner,
                pos,
                name,
                race,
                population,
            } => match self.can_found_city(pos) {
                Ok(()) => {
                    let city = self.spawn_city(owner, pos, name, race, population);
                    vec![Event::CityFounded { city, pos }]
                }
                Err(e) => vec![Event::Rejected(Rejection::FoundCity(e))],
            },
            Command::PlaceFortress {
                player,
                pos,
                name,
                race,
                population,
            } => self
                .place_fortress(player, pos, name, race, population)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Fortress(e))]),
            Command::Resign { player } => self
                .resign(player, rng)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Fortress(e))]),
            Command::DismissUnit { unit } => self.dismiss_unit(unit),
            Command::SetBridge { unit, on } => match self.set_bridge(unit, on) {
                Ok(()) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Move(e))],
            },
            Command::MoveItem {
                player,
                from,
                to,
                pay,
            } => match self.move_item(player, from, to, pay) {
                Ok(()) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Item(e))],
            },
            Command::CraftItem {
                player,
                craft,
                design,
            } => match self.craft_item(player, craft, &design, rng) {
                Ok(events) => events,
                Err(e) => vec![Event::Rejected(Rejection::Craft(e))],
            },
            Command::ScrapItem { player, from } => match self.scrap_item(player, from) {
                Ok((item, mana)) => vec![Event::ItemScrapped { player, item, mana }],
                Err(e) => vec![Event::Rejected(Rejection::Item(e))],
            },
            Command::AcceptOffer {
                player,
                index,
                name,
                at,
            } => self
                .accept_offer(player, index, name, at)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Offer(e))]),
            Command::RejectOffer { player, index } => match self.reject_offer(player, index) {
                Ok(events) => events,
                Err(e) => vec![Event::Rejected(Rejection::Offer(e))],
            },
            Command::SellItem { player, from } => self
                .sell_item(player, from)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Item(e))]),
            Command::SettleOutpost { unit, name } => self.settle_outpost(unit, name),
            Command::SetProduction { city, item } => match self.set_production(city, item) {
                Ok(()) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::SetAutoBuild { city, plan } => match self.set_autobuild(city, plan, rng) {
                Ok(()) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::BuyProduction { city } => match self.buy_production(city) {
                Ok(cost) => match self.economy().city_owner(city) {
                    Some(player) => vec![Event::ProductionBought {
                        player,
                        city,
                        cost: cost.floor().max(0) as u32,
                    }],
                    None => Vec::new(),
                },
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::RazeCity { player, city } => self.raze_city(player, city, rng),
            Command::RenameCity { city, name } => {
                let name: String = name.trim().chars().take(city::NAME_LEN).collect();
                if !name.is_empty() {
                    if let Ok(mut c) = self.ecs.get::<&mut city::City>(city) {
                        c.name = name;
                    }
                }
                Vec::new()
            }
            Command::SellBuilding { city, building } => match self.sell_building(city, building) {
                Ok(_) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::SetFarmers { city, farmers } => {
                let figures = self
                    .ecs
                    .get::<&city::economy::Population>(city)
                    .map_or(0, |p| p.figures);
                let min = self.economy().city_min_farmers(city);
                let _ = self
                    .ecs
                    .insert_one(city, city::economy::Farmers(farmers.clamp(min, figures)));
                Vec::new()
            }
            Command::SetTasks { player, tasks } => match self.player_entity(player) {
                Some(owner) => {
                    unit::ledger::set_tasks(&self.ecs, owner, Some(tasks));
                    Vec::new()
                }
                None => vec![Event::Rejected(Rejection::NoSuchPlayer)],
            },
            Command::BalanceFarmers { player } => {
                self.balance_farmers(player);
                Vec::new()
            }
            Command::SetTaxRate { player, rate } => {
                let max = self
                    .content
                    .rules
                    .config
                    .tax_unrest_percent
                    .len()
                    .saturating_sub(1) as u8;
                if let Some(t) = self.player_component_mut::<player::Tax>(player) {
                    t.0 = rate.min(max);
                }
                Vec::new()
            }
            Command::SetGrandVizier { player, on } => {
                self.set_grand_vizier(player, on);
                if on {
                    self.vizier_fills_idle_cities(player, rng);
                }
                Vec::new()
            }
            Command::SetPowerSplit {
                player,
                mana,
                research,
                skill,
            } => reject(match self.wizard_mut(player) {
                Some(mut w) => w.set_split(mana, research, skill),
                None => Err(wizard::magic::MagicError::UnknownPlayer),
            }),
            Command::ReleaseUpkeep { player, items } => {
                events_or_reject(self.release_upkeep(player, &items))
            }
            Command::ReleaseTroops {
                player,
                units,
                scrap,
            } => self
                .release_troops(player, &units, &scrap)
                .unwrap_or_else(|r| vec![Event::Rejected(r)]),
            Command::SetResearch { player, spell } => reject(self.set_research(player, spell)),
            Command::CancelCast { player } => {
                let returning = self.casting_return(player);
                reject(match self.wizard_mut(player) {
                    Some(mut w) => wizard::magic_command::cancel_cast(&mut w, returning),
                    None => Err(wizard::magic::MagicError::UnknownPlayer),
                })
            }
            Command::TurnOffSpell { player, spell } => events_or_reject(
                self.turn_off(player, spell)
                    .map(|_| vec![Event::EnchantmentLapsed { player, spell }]),
            ),
            Command::TurnOffCitySpell {
                player,
                city,
                spell,
            } => events_or_reject(self.turn_off_city_spell(player, city, spell)),
            Command::TurnOffUnitSpell {
                player,
                unit,
                spell,
            } => events_or_reject(self.turn_off_unit_spell(player, unit, spell, rng)),
            Command::Alchemy {
                player,
                amount,
                gold_to_mana,
            } => {
                let percent = self.wizard(player).map_or(50, |w| {
                    w.stat(&self.content.rules, crate::rules::Stat::AlchemyPercent, 50)
                });
                reject(match self.treasury_mut(player) {
                    Some(t) => wizard::magic_command::alchemy(percent, t, amount, gold_to_mana),
                    None => Err(wizard::magic::MagicError::UnknownPlayer),
                })
            }
            Command::CastSpell {
                player,
                spell,
                target,
                extra,
            } => events_or_reject(self.start_cast_with(player, spell, target, extra, rng)),
            Command::ResolveCast { player, target } => {
                events_or_reject(self.finish_cast_with_target(player, target, rng))
            }
            Command::Diplomacy(command) => self.diplomacy_command(command, rng),
        }
    }

    pub(crate) fn settle_outpost(&mut self, unit: hecs::Entity, name: String) -> Vec<Event> {
        let (Some(pos), Some(owner)) = (
            self.units().unit_position(unit),
            self.units().unit_owner(unit),
        ) else {
            return vec![Event::Rejected(Rejection::Move(
                unit::movement::MoveError::BadSelection,
            ))];
        };
        if !self.units().unit_is_settler(unit) {
            return vec![Event::Rejected(Rejection::Production(
                city::production::ProductionError::WrongRace,
            ))];
        }
        if self.time_stopper().is_some_and(|keeper| keeper != owner) {
            return vec![Event::Rejected(Rejection::FoundCity(
                city::FoundCityError::TimeStopped,
            ))];
        }
        if let Err(e) = self.can_found_city(pos) {
            return vec![Event::Rejected(Rejection::FoundCity(e))];
        }
        let race = self
            .units()
            .unit_def(unit)
            .and_then(|d| d.race.as_ref())
            .copied();
        unit::ledger::record(&self.ecs, unit, unit::ledger::Departure::UsedUp);
        self.despawn_with_spells(unit);
        let city = self.spawn_city(owner, pos, name, race, 0);
        vec![Event::CityFounded { city, pos }]
    }

    fn do_move(
        &mut self,
        units: &[hecs::Entity],
        to: map::Position,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        let from = units.first().and_then(|&u| self.units().unit_position(u));
        match (self.move_stack(units, to), from) {
            (Ok(unit::movement::Entry::Free), Some(from)) => {
                let mut events = vec![Event::UnitsMoved {
                    units: units.to_vec(),
                    from,
                    to,
                }];
                events.extend(self.collect_site(units, to, rng));
                events
            }
            (Ok(unit::movement::Entry::Site(lair)), Some(from)) => vec![Event::SiteEncounter {
                units: units.to_vec(),
                from,
                to,
                lair,
            }],
            (Ok(unit::movement::Entry::Hostile(defenders)), Some(from)) => {
                self.hostile_entry(units, from, to, defenders, rng)
            }
            (Err(e), _) => vec![Event::Rejected(Rejection::Move(e))],
            (Ok(_), None) => Vec::new(),
        }
    }

    /// A stack moved into hostile units or a hostile city. Against units it asks for a battle;
    /// an enemy city with no unit in it is simply walked into and taken.
    pub(crate) fn hostile_entry(
        &mut self,
        units: &[hecs::Entity],
        from: map::Position,
        to: map::Position,
        defenders: Vec<hecs::Entity>,
        rng: &mut GameRng,
    ) -> Vec<Event> {
        if !defenders.is_empty() {
            return vec![Event::AttackRequested {
                attackers: units.to_vec(),
                from,
                to,
                defenders,
            }];
        }
        let Some(owner) = units.first().and_then(|&u| self.units().unit_owner(u)) else {
            return Vec::new();
        };
        // Monsters do not hold a city: they wreck it and stay outside.
        if owner == player::MONSTERS {
            if let Some(city) = self.economy().city_at(to) {
                for &u in units {
                    let _ = self.ecs.insert_one(u, unit::MovesLeft(HalfInt::ZERO));
                }
                return self.rampage_city(city, 0, units, rng);
            }
        }
        for &u in units {
            let _ = self.ecs.insert_one(u, to);
            self.watch_tile(to);
            let _ = self.ecs.insert_one(u, unit::MovesLeft(HalfInt::ZERO));
        }
        self.explore(owner, to, map::fog::sight_for(2));
        let mut events = vec![Event::UnitsMoved {
            units: units.to_vec(),
            from,
            to,
        }];
        if let Some(city) = self.economy().city_at(to) {
            events.extend(self.conquer_city(city, owner, 0, rng));
        }
        events
    }
}
