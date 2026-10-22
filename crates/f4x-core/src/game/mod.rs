//! The game itself: [`Game`] owns the world and every domain's state and turns a [`Command`]
//! into [`Event`]s. Around it are the parts that keep a game going: setting one up, saving,
//! replaying and undoing.

pub(crate) mod access;
pub mod cheat;
pub mod command;
pub mod endgame;
pub mod event;
pub mod replay;
pub mod rng;
pub mod save;
pub mod serial;
pub mod setting;
pub mod setup;
#[cfg(test)]
pub(crate) mod testutil;
pub mod undo;
pub mod view;

#[cfg(test)]
mod tests;

use crate::{battle, city, content, diplomacy, game, item, map, player, unit, wizard, world_event};
use rand::RngCore;

pub use command::Command;
pub use event::{Event, LossCause, Rejection, Shortage};

fn reject(r: Result<(), wizard::magic::MagicError>) -> Vec<Event> {
    match r {
        Ok(()) => Vec::new(),
        Err(e) => vec![Event::Rejected(Rejection::Magic(e))],
    }
}

fn events_or_reject(r: Result<Vec<Event>, wizard::magic::MagicError>) -> Vec<Event> {
    r.unwrap_or_else(|e| vec![Event::Rejected(Rejection::Magic(e))])
}

pub struct Game {
    pub(crate) world: hecs::World,
    pub turn: u32,
    pub(crate) map: Option<map::Map>,
    pub(crate) content: content::Content,
    /// The one neutral player that owns the monsters (`player::MONSTERS`); it has no [`player::Player`]
    /// data, so it is in no list of players.
    pub(crate) monsters: hecs::Entity,
    /// The player entity of each [`player::PlayerId`].
    pub(crate) player_index: std::collections::HashMap<player::PlayerId, hecs::Entity>,
    pub(crate) fog: map::fog::Fog,
    pub(crate) next_serial: u64,
    /// The seeded generator all randomness comes from; it counts its draws (see `game/rng.rs`).
    pub(crate) rng: game::rng::GameRng,
    /// What the game is played with (difficulty, node strength, ...). Chosen when the game is
    /// made and fixed after that; read through [`Game::settings`].
    pub(crate) settings: game::setting::GameSettings,
    /// The battle being fought, if any. Saves do not hold a battle in progress.
    pub battle: Option<battle::game::ActiveBattle>,
    /// Set once the game has ended; every later command is refused.
    pub game_over: Option<game::endgame::GameOver>,
    /// The human wizard, whose defeat ends the game at once.
    pub human: Option<player::PlayerId>,
    /// Contact, relations, treaties and proposals between wizards.
    pub diplomacy: diplomacy::Diplomacy,
    /// The random events that are going on.
    pub(crate) events: world_event::Events,
    /// Items that fell to wizards from slain heroes since the last command ended: the wizard,
    /// the item's name and whether the armory had room for it.
    pub(crate) item_awards: Vec<(hecs::Entity, String, bool)>,
    /// Every override (cheat) command that changed the game, oldest first.
    pub(crate) override_log: Vec<String>,
    /// The prefab items given out so far, by id (`Make_Item` gives each once a game).
    pub(crate) prefab_taken: Vec<String>,
    /// Players of battles that play for some wizards (a learned one; see `battle::game`).
    pub(crate) battle_brains: Vec<(hecs::Entity, Box<dyn battle::game::BattleBrain>)>,
    pub(crate) recording: Option<game::replay::Recording>,
    undo_on: bool,
    undo_stack: Vec<game::undo::UndoEntry>,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let mut world = hecs::World::new();
        let monsters = world.spawn((player::MONSTERS, player::Neutral));
        Self {
            world,
            monsters,
            turn: 1,
            map: None,
            content: content::Content::default(),
            player_index: std::collections::HashMap::new(),
            fog: map::fog::Fog::default(),
            next_serial: 1,
            rng: game::rng::GameRng::new(seed),
            // The symmetric game, as `game::setting::GameSettings::flat` says; a game made for
            // play gets its level from `game::setup::new_game`.
            settings: game::setting::GameSettings::flat(),
            battle: None,
            game_over: None,
            human: None,
            diplomacy: diplomacy::Diplomacy::default(),
            events: world_event::Events::default(),
            item_awards: Vec::new(),
            override_log: Vec::new(),
            prefab_taken: Vec::new(),
            battle_brains: Vec::new(),
            recording: None,
            undo_on: false,
            undo_stack: Vec::new(),
        }
    }

    /// The settings the game is played with.
    pub fn settings(&self) -> &game::setting::GameSettings {
        &self.settings
    }

    /// Start a game on a freshly generated map.
    pub fn with_generated_map(
        seed: u64,
        params: &map::mapgen::GenParams,
        content: &content::Content,
    ) -> Result<Self, map::mapgen::GenError> {
        let mut game = Self::new(seed);
        game.settings.basic.magic_strength = params.magic_strength.min(2);
        game.settings.basic.difficulty = params.difficulty.min(4);
        game.map = Some(map::mapgen::generate(seed, params, content)?);
        game.content = content.clone();
        Ok(game)
    }

    /// Whether a city could be founded at `pos` right now.
    pub fn can_found_city(&self, pos: map::Position) -> Result<(), city::FoundCityError> {
        let existing = self.economy().cities().into_iter().map(|(_, _, p)| p);
        city::check_site(self.map.as_ref(), &self.content, pos, existing)
    }

    /// Let `player` see the tiles within `radius` of `at`.
    pub fn explore(&mut self, player: player::PlayerId, at: map::Position, radius: u32) {
        if let (Some(map), true) = (self.map.as_ref(), self.fog.is_empty()) {
            self.fog = map::fog::Fog::new(map.width(), map.height());
        }
        self.fog.explore(player, at, radius);
        self.contact_near(player, at, radius);
    }

    /// The seed this game started from.
    pub fn seed(&self) -> u64 {
        self.rng.seed()
    }

    /// Whether `id` is the monsters' owner or an independent city's player: not a wizard.
    pub fn is_neutral_player(&self, id: player::PlayerId) -> bool {
        self.player_entity(id)
            .is_some_and(|e| self.world.satisfies::<&player::Neutral>(e).unwrap_or(false))
    }

    /// Make the player entity for `id`, and with it a wizard called `name`.
    pub fn add_player(
        &mut self,
        id: player::PlayerId,
        name: impl Into<String>,
        player: player::Player,
    ) -> hecs::Entity {
        debug_assert!(
            self.player_entity(id).is_none(),
            "player {id:?} is made twice"
        );
        let e = self.world.spawn((id, player));
        let w = self.world.spawn(wizard::entity::bundle(id, name.into(), e));
        let _ = self.world.insert_one(e, player::WizardLink(w));
        self.player_index.insert(id, e);
        e
    }

    /// Make the player of one independent city: it has a treasury and pays its way like a wizard,
    /// but is no wizard (no magic, no victory, no AI).
    pub fn add_neutral_player(
        &mut self,
        id: player::PlayerId,
        player: player::Player,
    ) -> hecs::Entity {
        let e = self.world.spawn((id, player, player::Neutral));
        self.player_index.insert(id, e);
        e
    }

    /// The wizard of a player, if it has one.
    pub fn wizard(&self, id: player::PlayerId) -> Option<wizard::entity::Wizard<'_>> {
        let w = wizard::entity::of_player(&self.world, self.player_entity(id)?)?;
        Some(wizard::entity::Wizard::new(&self.world, w))
    }

    /// How a player is called: its wizard's name, or its city's.
    pub fn player_name(&self, id: player::PlayerId) -> String {
        wizard::entity::player_name(&self.world, id)
    }

    /// The banner colour of a player; the neutral banner if it has no wizard.
    pub fn banner(&self, id: player::PlayerId) -> u8 {
        self.wizard(id)
            .map_or(player::NEUTRAL_BANNER, |w| w.banner())
    }

    /// The wizard picture of a player.
    pub fn portrait(&self, id: player::PlayerId) -> u8 {
        self.wizard(id).map_or(0, |w| w.portrait())
    }

    /// Change the wizard's components.
    pub(crate) fn wizard_set<C: hecs::Component>(&mut self, id: player::PlayerId, value: C) {
        if let Some(w) = self
            .player_entity(id)
            .and_then(|p| wizard::entity::of_player(&self.world, p))
        {
            let _ = self.world.insert_one(w, value);
        }
    }

    /// An independent city's player goes with its city; wizards' players are never removed.
    pub(crate) fn remove_player(&mut self, id: player::PlayerId) {
        if let Some(e) = self.player_index.remove(&id) {
            if let Some(w) = wizard::entity::of_player(&self.world, e) {
                let _ = self.world.despawn(w);
            }
            let _ = self.world.despawn(e);
        }
    }

    /// Hand a unit or city to a player: its [`player::Owner`] becomes the player's entity.
    pub(crate) fn set_owner(&mut self, thing: hecs::Entity, player: player::PlayerId) {
        if let Some(e) = self.player_entity(player) {
            let _ = self.world.insert_one(thing, player::Owner(e));
        }
    }

    /// The entity of a player.
    pub fn player_entity(&self, id: player::PlayerId) -> Option<hecs::Entity> {
        if id == player::MONSTERS {
            return Some(self.monsters);
        }
        self.player_index.get(&id).copied()
    }

    /// Read a player's data.
    pub fn player(&self, id: player::PlayerId) -> Option<player::PlayerRef<'_>> {
        self.world
            .get::<&player::Player>(self.player_entity(id)?)
            .ok()
    }

    pub fn player_mut(&mut self, id: player::PlayerId) -> Option<&mut player::Player> {
        let e = self.player_entity(id)?;
        self.world.query_one_mut::<&mut player::Player>(e).ok()
    }

    /// Every player with its id: wizards and independent cities' players, ascending.
    pub fn players(&self) -> Vec<(player::PlayerId, player::PlayerRef<'_>)> {
        player::all(&self.world)
    }

    /// The player that was added `i`th (tests index players the way `Vec<Player>` did).
    #[cfg(test)]
    pub(crate) fn pl(&self, i: usize) -> player::PlayerRef<'_> {
        self.player(self.player_ids()[i]).unwrap()
    }

    #[cfg(test)]
    pub(crate) fn pl_mut(&mut self, i: usize) -> &mut player::Player {
        let id = self.player_ids()[i];
        self.player_mut(id).unwrap()
    }

    /// Every player's id: wizards and independent cities' players, ascending.
    pub fn player_ids(&self) -> Vec<player::PlayerId> {
        let mut ids: Vec<_> = self.player_index.keys().copied().collect();
        ids.sort();
        ids
    }

    fn execute(&mut self, cmd: Command) -> Vec<Event> {
        let mut events = self.execute_command(cmd);
        // What hooks in battles, spells and the turn raised between wizards comes last.
        events.extend(self.drain_diplomacy_events());
        events.extend(self.drain_item_awards());
        events
    }

    fn execute_command(&mut self, cmd: Command) -> Vec<Event> {
        match cmd {
            Command::EndTurn => self.end_turn(),
            Command::CreateUnit { owner, pos, unit } => {
                let unit = self.spawn_unit(owner, pos, unit);
                vec![Event::UnitCreated { unit, pos }]
            }
            Command::MoveStack { units, to } => {
                for &u in &units {
                    let _ = self.world.remove_one::<unit::Standby>(u);
                }
                self.do_move(&units, to)
            }
            Command::SetStandby { units, standby } => {
                for u in units {
                    match standby {
                        Some(s) => {
                            let _ = self.world.insert_one(u, s);
                        }
                        None => {
                            let _ = self.world.remove_one::<unit::Standby>(u);
                        }
                    }
                }
                Vec::new()
            }
            Command::GoTo { units, to } => match self.find_path(&units, to) {
                Some(path) => {
                    for &u in &units {
                        let _ = self.world.insert_one(u, unit::Order::GoTo(path.clone()));
                    }
                    self.follow_orders(&units)
                }
                None => vec![Event::Rejected(Rejection::Move(
                    unit::movement::MoveError::Impassable,
                ))],
            },
            Command::ClearOrders { units } => {
                for u in units {
                    let _ = self.world.remove_one::<unit::Order>(u);
                    let _ = self.world.remove_one::<map::road::RoadWork>(u);
                }
                Vec::new()
            }
            Command::BeginBattle {
                attackers,
                defenders,
            } => self
                .start_battle(&attackers, &defenders)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::BattleStart(e))]),
            Command::AutoResolve {
                attackers,
                defenders,
            } => self.auto_resolve(&attackers, &defenders),
            Command::EnterSite { units, to } => self
                .enter_site(&units, to)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Site(e))]),
            Command::Battle(command) => self
                .battle_command(command)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Battle(e))]),
            Command::Purify { units } => self
                .purify(&units)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Purify(e))]),
            Command::BuildRoad { units, path } => self
                .build_road(&units, path)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Road(e))]),
            Command::Meld { units } => self
                .meld(&units)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Meld(e))]),
            Command::ShiftPlane { units } => {
                let from = units.first().and_then(|&u| self.units().unit_position(u));
                match (self.shift_plane(&units), from) {
                    (Ok(to), Some(from)) => vec![Event::UnitsMoved { units, from, to }],
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
                .resign(player)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Fortress(e))]),
            Command::DismissUnit { unit } => self.dismiss_unit(unit),
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
            } => match self.craft_item(player, craft, &design) {
                Ok(events) => events,
                Err(e) => vec![Event::Rejected(Rejection::Craft(e))],
            },
            Command::ResurrectHero { player, kind } => self
                .resurrect_hero(player, kind)
                .unwrap_or_else(|r| vec![Event::Rejected(r)]),
            Command::ScrapItem { player, from } => match self.scrap_item(player, from) {
                Some(_) => Vec::new(),
                None => vec![Event::Rejected(Rejection::Item(
                    item::armory::ItemError::Empty,
                ))],
            },
            Command::AcceptOffer {
                player,
                index,
                name,
            } => self
                .accept_offer(player, index, name)
                .unwrap_or_else(|e| vec![Event::Rejected(Rejection::Offer(e))]),
            Command::RejectOffer { player, index } => match self.reject_offer(player, index) {
                Ok(()) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Offer(e))],
            },
            Command::SettleOutpost { unit, name } => self.settle_outpost(unit, name),
            Command::SetProduction { city, item } => match self.set_production(city, item) {
                Ok(()) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::SetAutoBuild { city, plan } => match self.set_autobuild(city, plan) {
                Ok(()) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::BuyProduction { city } => match self.buy_production(city) {
                Ok(_) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::RazeCity { player, city } => self.raze_city(player, city),
            Command::RenameCity { city, name } => {
                let name: String = name.trim().chars().take(city::NAME_LEN).collect();
                if !name.is_empty() {
                    if let Ok(mut c) = self.world.get::<&mut city::City>(city) {
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
                    .world
                    .get::<&city::economy::Population>(city)
                    .map_or(0, |p| p.figures);
                let min = self.economy().city_min_farmers(city);
                let _ = self
                    .world
                    .insert_one(city, city::economy::Farmers(farmers.clamp(min, figures)));
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
                if let Some(p) = self.player_mut(player) {
                    p.tax_rate = rate.min(max);
                }
                Vec::new()
            }
            Command::SetGrandVizier { player, on } => {
                if let Some(p) = self.player_mut(player) {
                    p.grand_vizier = on;
                }
                if on {
                    self.vizier_fills_idle_cities(player);
                }
                Vec::new()
            }
            Command::SetPowerSplit {
                player,
                mana,
                research,
                skill,
            } => reject(match self.magic_mut(player) {
                Some(m) => m.set_split(mana, research, skill),
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
                reject(match self.player_mut(player) {
                    Some(p) => wizard::magic_commands::cancel_cast(p, returning),
                    None => Err(wizard::magic::MagicError::UnknownPlayer),
                })
            }
            Command::TurnOffSpell { player, spell } => {
                events_or_reject(match self.magic_mut(player) {
                    Some(m) => m
                        .turn_off(spell)
                        .map(|_| vec![Event::EnchantmentLapsed { player, spell }]),
                    None => Err(wizard::magic::MagicError::UnknownPlayer),
                })
            }
            Command::TurnOffCitySpell {
                player,
                city,
                spell,
            } => events_or_reject(self.turn_off_city_spell(player, city, spell)),
            Command::TurnOffUnitSpell {
                player,
                unit,
                spell,
            } => events_or_reject(self.turn_off_unit_spell(player, unit, spell)),
            Command::Alchemy {
                player,
                amount,
                gold_to_mana,
            } => reject(match self.player_mut(player) {
                Some(p) => wizard::magic_commands::alchemy(p, amount, gold_to_mana),
                None => Err(wizard::magic::MagicError::UnknownPlayer),
            }),
            Command::CastSpell {
                player,
                spell,
                target,
            } => events_or_reject(self.start_cast(player, spell, target)),
            Command::ResolveCast { player, target } => {
                events_or_reject(self.finish_cast_with_target(player, target))
            }
            Command::Diplomacy(command) => self.diplomacy_command(command),
        }
    }

    fn settle_outpost(&mut self, unit: hecs::Entity, name: String) -> Vec<Event> {
        let (Some(pos), Some(owner)) = (
            self.units().unit_position(unit),
            self.units().unit_owner(unit),
        ) else {
            return vec![Event::Rejected(Rejection::Move(
                unit::movement::MoveError::BadSelection,
            ))];
        };
        if !self.units().unit_has_ability(unit, "create_outpost") {
            return vec![Event::Rejected(Rejection::Production(
                city::production::ProductionError::WrongRace,
            ))];
        }
        if let Err(e) = self.can_found_city(pos) {
            return vec![Event::Rejected(Rejection::FoundCity(e))];
        }
        let race = self
            .units()
            .unit_def(unit)
            .and_then(|d| d.race.as_deref())
            .and_then(|r| self.content.rules.races.id_of(r));
        let _ = self.world.despawn(unit);
        let city = self.spawn_city(owner, pos, name, race, 0);
        vec![Event::CityFounded { city, pos }]
    }

    fn do_move(&mut self, units: &[hecs::Entity], to: map::Position) -> Vec<Event> {
        let from = units.first().and_then(|&u| self.units().unit_position(u));
        match (self.move_stack(units, to), from) {
            (Ok(unit::movement::Entry::Free), Some(from)) => {
                let mut events = vec![Event::UnitsMoved {
                    units: units.to_vec(),
                    from,
                    to,
                }];
                events.extend(self.collect_site(units, to));
                events
            }
            (Ok(unit::movement::Entry::Site(lair)), Some(from)) => vec![Event::SiteEncounter {
                units: units.to_vec(),
                from,
                to,
                lair,
            }],
            (Ok(unit::movement::Entry::Hostile(defenders)), Some(from)) => {
                self.hostile_entry(units, from, to, defenders)
            }
            (Err(e), _) => vec![Event::Rejected(Rejection::Move(e))],
            (Ok(_), None) => Vec::new(),
        }
    }

    /// Advance each listed unit along its standing order as far as its moves allow.
    fn follow_orders(&mut self, units: &[hecs::Entity]) -> Vec<Event> {
        let mut events = Vec::new();
        // Units sharing a tile and an order walk together.
        let mut groups: Vec<Vec<hecs::Entity>> = Vec::new();
        for &u in units {
            let Some(pos) = self.units().unit_position(u) else {
                continue;
            };
            let order = self.world.get::<&unit::Order>(u).ok().map(|o| (*o).clone());
            let Some(order) = order else { continue };
            match groups.iter_mut().find(|g| {
                let f = g[0];
                self.units().unit_position(f) == Some(pos)
                    && self.world.get::<&unit::Order>(f).ok().map(|o| (*o).clone())
                        == Some(order.clone())
            }) {
                Some(g) => g.push(u),
                None => groups.push(vec![u]),
            }
        }
        for group in groups {
            let unit::Order::GoTo(mut path) = self
                .world
                .get::<&unit::Order>(group[0])
                .map(|o| (*o).clone())
                .expect("grouped by order");
            while let Some(&next) = path.first() {
                let from = self.units().unit_position(group[0]).expect("on map");
                match self.move_stack(&group, next) {
                    Ok(unit::movement::Entry::Free) => {
                        path.remove(0);
                        events.push(Event::UnitsMoved {
                            units: group.clone(),
                            from,
                            to: next,
                        });
                        events.extend(self.collect_site(&group, next));
                    }
                    Ok(unit::movement::Entry::Site(lair)) => {
                        events.push(Event::SiteEncounter {
                            units: group.clone(),
                            from,
                            to: next,
                            lair,
                        });
                        path.clear();
                    }
                    Ok(unit::movement::Entry::Hostile(defenders)) => {
                        events.extend(self.hostile_entry(&group, from, next, defenders));
                        path.clear();
                    }
                    Err(unit::movement::MoveError::NoMovesLeft) => break,
                    Err(_) => {
                        path.clear();
                    }
                }
            }
            for &u in &group {
                if path.is_empty() {
                    let _ = self.world.remove_one::<unit::Order>(u);
                } else {
                    let _ = self.world.insert_one(u, unit::Order::GoTo(path.clone()));
                }
            }
        }
        events
    }

    /// A stack moved into hostile units or a hostile city. Against units it asks for a battle;
    /// an enemy city with no unit in it is simply walked into and taken.
    fn hostile_entry(
        &mut self,
        units: &[hecs::Entity],
        from: map::Position,
        to: map::Position,
        defenders: Vec<hecs::Entity>,
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
        for &u in units {
            let _ = self.world.insert_one(u, to);
            let _ = self.world.insert_one(u, unit::MovesLeft(0));
        }
        self.explore(owner, to, 2);
        let mut events = vec![Event::UnitsMoved {
            units: units.to_vec(),
            from,
            to,
        }];
        if let Some(city) = self.economy().city_at(to) {
            events.extend(self.conquer_city(city, owner, 0));
        }
        events
    }

    fn end_turn(&mut self) -> Vec<Event> {
        self.close_raze_windows();
        self.reopen_sales();
        let mut corruption = self.process_land_corruption();
        self.turn += 1;
        let mut freed = self.process_stasis();
        let stopper = self.time_stopper();
        for unit in self.units().all_units() {
            // A unit in stasis cannot move, nor can any unit but the Time Stop wizard's.
            let moves = if self.unit_in_stasis(unit)
                || stopper.is_some_and(|s| self.units().unit_owner(unit) != Some(s))
            {
                0
            } else {
                self.units().unit_max_moves(unit)
            };
            let _ = self.world.insert_one(unit, unit::MovesLeft(moves));
            if self.units().unit_standby(unit) == Some(unit::Standby::Done) {
                let _ = self.world.remove_one::<unit::Standby>(unit);
            }
        }
        let mut events = vec![Event::TurnStarted { turn: self.turn }];
        events.append(&mut corruption);
        events.append(&mut freed);
        events.extend(self.process_turn());
        self.record_history();
        self.process_diplomacy();
        events.extend(self.process_purify());
        events.extend(self.sail_marooned());
        events.extend(self.drown_stranded());
        events.extend(self.process_road_work());
        let with_orders: Vec<_> = self
            .world
            .query::<&unit::Order>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        let mut with_orders = with_orders;
        self.sort_by_serial(&mut with_orders);
        events.extend(self.follow_orders(&with_orders));
        events
    }

    /// Uniform random integer in `1..=n`.
    pub fn roll(&mut self, n: u32) -> u32 {
        self.rng.roll(n)
    }

    /// Next value from the game's seeded RNG (all randomness must come from here).
    pub fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }
}
