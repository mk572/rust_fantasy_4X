//! Deterministic game simulation. No I/O, no rendering.
//!
//! All state changes go through [`Game::apply`] as [`Command`]s, which yield
//! [`Event`]s. The command log plus the seed fully reproduces a game.

pub mod battle;
pub mod cast;
pub mod cheat;
pub mod city;
pub mod city_spells;
pub mod conquest;
pub mod content;
pub mod corruption;
pub mod dispel;
pub mod economy;
pub mod endgame;
pub mod featuregen;
pub mod fog;
pub mod fortress;
pub mod gates;
pub mod globals;
pub mod help;
pub mod lairs;
pub mod magic;
pub mod map;
pub mod mapgen;
pub mod movement;
pub mod neutral_cities;
pub mod nightshade;
pub mod nodes;
pub mod pathing;
pub mod player;
pub mod production;
pub mod roads;
pub mod rules;
pub mod save;
pub mod serial;
pub mod setup;
pub mod temp_effects;
pub mod terraform;
#[cfg(test)]
mod testutil;
pub mod timers;
pub mod transport;
pub mod turn;
pub mod undo;
pub mod units;
pub mod vision;
pub mod wizard;

use rand::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    EndTurn,
    /// Create a unit (summoning, production and scenario setup go through this).
    CreateUnit {
        owner: city::PlayerId,
        pos: city::Position,
        unit: rules::UnitId,
    },
    /// Move a stack one tile.
    MoveStack {
        units: Vec<hecs::Entity>,
        to: city::Position,
    },
    /// Give a stack a standing order to walk to a tile.
    GoTo {
        units: Vec<hecs::Entity>,
        to: city::Position,
    },
    /// Cancel standing orders.
    ClearOrders {
        units: Vec<hecs::Entity>,
    },
    /// Begin a battle between two stacks of units.
    BeginBattle {
        attackers: Vec<hecs::Entity>,
        defenders: Vec<hecs::Entity>,
    },
    /// An order in the battle being fought.
    Battle(battle::BattleCommand),
    /// Fight a battle between two stacks to the end without the tactical map.
    AutoResolve {
        attackers: Vec<hecs::Entity>,
        defenders: Vec<hecs::Entity>,
    },
    /// A stack next to a guarded site (tower, node or encounter site) assaults its guards.
    EnterSite {
        units: Vec<hecs::Entity>,
        to: city::Position,
    },
    /// A stack with a purifying unit cleanses the corrupted square it stands on.
    Purify {
        units: Vec<hecs::Entity>,
    },
    /// Engineers build a road along `path` (starting where they stand).
    BuildRoad {
        units: Vec<hecs::Entity>,
        path: Vec<city::Position>,
    },
    /// Meld a spirit of the stack into the magic node it stands on.
    Meld {
        units: Vec<hecs::Entity>,
    },
    /// Move a stack to the same tile on the other plane.
    ShiftPlane {
        units: Vec<hecs::Entity>,
    },
    /// Teleport a stack from a city with an Earth Gate to another such city of the same wizard.
    EarthGate {
        units: Vec<hecs::Entity>,
        to: city::Position,
    },
    /// Found a city. No population makes an outpost; race decides what it can build.
    FoundCity {
        owner: city::PlayerId,
        pos: city::Position,
        name: String,
        race: Option<rules::RaceId>,
        population: u32,
    },
    /// A settler founds an outpost where it stands and is used up.
    SettleOutpost {
        unit: hecs::Entity,
        name: String,
    },
    SetProduction {
        city: hecs::Entity,
        item: production::ProductionItem,
    },
    /// Pay gold to finish the current item at the next turn change.
    BuyProduction {
        city: hecs::Entity,
    },
    SetFarmers {
        city: hecs::Entity,
        farmers: u32,
    },
    /// Found the wizard's first city with the fortress and summoning circle in it.
    PlaceFortress {
        player: city::PlayerId,
        pos: city::Position,
        name: String,
        race: Option<rules::RaceId>,
        population: u32,
    },
    SellBuilding {
        city: hecs::Entity,
        building: rules::BuildingId,
    },
    /// Raze a city just taken, instead of keeping it (until the end of the turn).
    RazeCity {
        player: city::PlayerId,
        city: hecs::Entity,
    },
    SetTaxRate {
        player: city::PlayerId,
        rate: u8,
    },
    /// Let the Grand Vizier choose what the player's cities build (`production::autobuild`).
    SetGrandVizier {
        player: city::PlayerId,
        on: bool,
    },
    /// Split the power base between mana, research and casting skill (percent, sum 100).
    SetPowerSplit {
        player: city::PlayerId,
        mana: u8,
        research: u8,
        skill: u8,
    },
    SetResearch {
        player: city::PlayerId,
        spell: rules::SpellId,
    },
    /// Start casting a spell overland; the target is needed only if it is paid in full now.
    CastSpell {
        player: city::PlayerId,
        spell: rules::SpellId,
        target: Option<rules::exec::Target>,
    },
    /// Aim a spell that has been paid for over several turns.
    ResolveCast {
        player: city::PlayerId,
        target: rules::exec::Target,
    },
    CancelCast {
        player: city::PlayerId,
    },
    /// Trade gold and mana (two for one, one for one with the alchemy retort).
    Alchemy {
        player: city::PlayerId,
        amount: i32,
        gold_to_mana: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    TurnStarted {
        turn: u32,
    },
    CityFounded {
        city: hecs::Entity,
        pos: city::Position,
    },
    ProductionCompleted {
        city: hecs::Entity,
        item: production::ProductionItem,
    },
    /// A Chaos Rift struck the city: units it destroyed, and a building that fell.
    ChaosRiftStrikes {
        city: hecs::Entity,
        units_lost: u32,
        building: Option<rules::BuildingId>,
    },
    CityGrew {
        city: hecs::Entity,
        figures: u32,
    },
    CityShrank {
        city: hecs::Entity,
        figures: u32,
    },
    /// Units dismissed because their owner could not feed or pay them.
    UnitsDisbanded {
        player: city::PlayerId,
        units: u32,
    },
    FortressPlaced {
        player: city::PlayerId,
        pos: city::Position,
    },
    FortressMoved {
        player: city::PlayerId,
        pos: city::Position,
    },
    SummoningCircleMoved {
        player: city::PlayerId,
        pos: city::Position,
    },
    /// The fortress city fell; the wizard now casts the Spell of Return.
    WizardBanished {
        player: city::PlayerId,
        by: city::PlayerId,
    },
    WizardReturned {
        player: city::PlayerId,
        pos: city::Position,
    },
    /// The wizard lost their last city.
    WizardDefeated {
        player: city::PlayerId,
    },
    /// The game is over: `winner` won, by conquest or by the Spell of Mastery.
    GameOver {
        winner: city::PlayerId,
        kind: endgame::EndKind,
        turn: u32,
    },
    CityCaptured {
        city: hecs::Entity,
        from: city::PlayerId,
        to: city::PlayerId,
    },
    CityDestroyed {
        pos: city::Position,
    },
    /// A city fell: what its taker looted, and the citizens and buildings the fighting cost it.
    CityLooted {
        city: hecs::Entity,
        gold: i32,
        citizens_lost: u32,
        buildings_lost: u32,
    },
    /// The taker razed the city they had just won.
    CityRazed {
        player: city::PlayerId,
        gold: i32,
    },
    VolcanoRaised {
        player: city::PlayerId,
        pos: city::Position,
    },
    /// Buildings lost when a volcano rose under a city.
    BuildingsCollapsed {
        city: hecs::Entity,
        count: u32,
    },
    TerrainChanged {
        pos: city::Position,
    },
    SpellLearned {
        player: city::PlayerId,
        spell: rules::SpellId,
    },
    SpellCast {
        player: city::PlayerId,
        spell: rules::SpellId,
    },
    /// A dispel took `removed` enchantments off its target.
    Dispelled {
        player: city::PlayerId,
        removed: u32,
    },
    /// A dispel against the global enchantment `spell` failed.
    DispelFailed {
        player: city::PlayerId,
        spell: rules::SpellId,
    },
    /// Disjunction ended `victim`'s global enchantment.
    GlobalSpellRemoved {
        player: city::PlayerId,
        victim: city::PlayerId,
        spell: rules::SpellId,
    },
    /// Spell Binding took `victim`'s global enchantment.
    GlobalSpellTaken {
        player: city::PlayerId,
        victim: city::PlayerId,
        spell: rules::SpellId,
    },
    /// Spell Blast ended the spell `victim` was casting, for `mana` of the caster's own.
    SpellBlasted {
        player: city::PlayerId,
        victim: city::PlayerId,
        spell: rules::SpellId,
        mana: i32,
    },
    /// Another wizard's Suppress Magic stopped the spell.
    SpellSuppressed {
        player: city::PlayerId,
        spell: rules::SpellId,
        by: city::PlayerId,
    },
    /// A spell aimed at a city was turned away by the city's nightshade.
    SpellFizzled {
        player: city::PlayerId,
        spell: rules::SpellId,
        city: hecs::Entity,
    },
    /// A spell has been paid for and waits for a target.
    SpellReady {
        player: city::PlayerId,
        spell: rules::SpellId,
    },
    /// The spell was cast but part of what it does is not implemented.
    SpellHasNoEffectYet {
        spell: rules::SpellId,
    },
    /// A lasting spell ended because its upkeep could not be paid.
    EnchantmentLapsed {
        player: city::PlayerId,
        spell: rules::SpellId,
    },
    BattleStarted {
        attacker: city::PlayerId,
        defender: city::PlayerId,
        site: city::Position,
    },
    /// Something happened in the battle being fought.
    Battle(battle::BattleEvent),
    /// Units of a side that fled a battle were lost on the way.
    UnitsLostFleeing {
        player: city::PlayerId,
        units: u32,
    },
    BattleEnded {
        winner: city::PlayerId,
        loser: city::PlayerId,
    },
    LandCorrupted {
        pos: city::Position,
    },
    LandPurified {
        pos: city::Position,
    },
    NodeWarped {
        pos: city::Position,
    },
    RoadBuilt {
        pos: city::Position,
    },
    RoadsEnchanted {
        pos: city::Position,
    },
    /// Land units lost at sea without a ship.
    UnitsDrowned {
        player: city::PlayerId,
        units: u32,
    },
    NodeMelded {
        player: city::PlayerId,
        pos: city::Position,
    },
    /// A spirit died against a guardian: the node stays as it was.
    MeldFailed {
        player: city::PlayerId,
        pos: city::Position,
    },
    /// A timed spell ran out.
    EnchantmentExpired {
        player: city::PlayerId,
        spell: rules::SpellId,
    },
    UnitCreated {
        unit: hecs::Entity,
        pos: city::Position,
    },
    UnitsMoved {
        units: Vec<hecs::Entity>,
        from: city::Position,
        to: city::Position,
    },
    /// Moving into hostile units or a hostile city: the attack is for the combat layer.
    AttackRequested {
        attackers: Vec<hecs::Entity>,
        from: city::Position,
        to: city::Position,
        defenders: Vec<hecs::Entity>,
    },
    /// Moving onto a guarded site: its guards must be fought first (`Command::EnterSite`).
    SiteEncounter {
        units: Vec<hecs::Entity>,
        from: city::Position,
        to: city::Position,
        lair: usize,
    },
    /// A site was cleared and its treasure taken.
    SiteCleared {
        player: city::PlayerId,
        pos: city::Position,
        loot: lairs::Loot,
    },
    Rejected(Rejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    Site(lairs::SiteError),
    FoundCity(city::FoundCityError),
    Move(movement::MoveError),
    Production(production::ProductionError),
    Magic(magic::MagicError),
    Fortress(fortress::FortressError),
    Meld(nodes::MeldError),
    Road(roads::RoadError),
    Purify(corruption::PurifyError),
    BattleStart(battle::game::BattleStartError),
    Raze(conquest::RazeError),
    Battle(battle::BattleError),
    /// The game has ended; nothing more can be done.
    GameOver,
}

fn reject(r: Result<(), magic::MagicError>) -> Vec<Event> {
    match r {
        Ok(()) => Vec::new(),
        Err(e) => vec![Event::Rejected(Rejection::Magic(e))],
    }
}

fn events_or_reject(r: Result<Vec<Event>, magic::MagicError>) -> Vec<Event> {
    r.unwrap_or_else(|e| vec![Event::Rejected(Rejection::Magic(e))])
}

pub struct Game {
    pub world: hecs::World,
    pub turn: u32,
    pub map: Option<map::Map>,
    pub content: content::Content,
    pub players: Vec<player::Player>,
    pub fog: fog::Fog,
    seed: u64,
    /// Values drawn from the RNG so far; a save replays this many to resume the stream.
    draws: u64,
    pub(crate) next_serial: u64,
    rng: Pcg64Mcg,
    /// Countdowns for timed spells.
    pub(crate) timers: Vec<timers::Timer>,
    /// The game's node setting: 0 weak, 1 normal, 2 powerful.
    pub magic_strength: u8,
    /// The game's difficulty: 0 intro, 1 easy, 2 normal, 3 hard, 4 impossible.
    pub difficulty: u8,
    /// The battle being fought, if any. Saves do not hold a battle in progress.
    pub battle: Option<battle::game::ActiveBattle>,
    /// Set once the game has ended; every later command is refused.
    pub game_over: Option<endgame::GameOver>,
    /// The human wizard, whose defeat ends the game at once.
    pub human: Option<city::PlayerId>,
    /// Every override (cheat) command that changed the game, oldest first.
    pub(crate) override_log: Vec<String>,
    undo_on: bool,
    undo_stack: Vec<undo::UndoEntry>,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        Self {
            world: hecs::World::new(),
            turn: 1,
            map: None,
            content: content::Content::default(),
            players: Vec::new(),
            fog: fog::Fog::default(),
            seed,
            draws: 0,
            next_serial: 1,
            rng: Pcg64Mcg::seed_from_u64(seed),
            timers: Vec::new(),
            magic_strength: 1,
            difficulty: 2,
            battle: None,
            game_over: None,
            human: None,
            override_log: Vec::new(),
            undo_on: false,
            undo_stack: Vec::new(),
        }
    }

    /// Start a game on a freshly generated map.
    pub fn with_generated_map(
        seed: u64,
        params: &mapgen::GenParams,
        content: &content::Content,
    ) -> Result<Self, mapgen::GenError> {
        let mut game = Self::new(seed);
        game.magic_strength = params.magic_strength.min(2);
        game.difficulty = params.difficulty.min(4);
        game.map = Some(mapgen::generate(seed, params, content)?);
        game.content = content.clone();
        Ok(game)
    }

    /// All cities with their positions.
    pub fn cities(&self) -> Vec<(hecs::Entity, city::City, city::Position)> {
        let mut out: Vec<_> = self
            .world
            .query::<(&city::City, &city::Position)>()
            .iter()
            .map(|(e, (c, p))| (e, c.clone(), *p))
            .collect();
        out.sort_by_key(|(e, ..)| (self.serial_of(*e), e.id()));
        out
    }

    pub fn city_at(&self, pos: city::Position) -> Option<hecs::Entity> {
        self.cities()
            .into_iter()
            .find(|(_, _, p)| *p == pos)
            .map(|(e, ..)| e)
    }

    /// Whether a city could be founded at `pos` right now.
    pub fn can_found_city(&self, pos: city::Position) -> Result<(), city::FoundCityError> {
        let existing = self.cities().into_iter().map(|(_, _, p)| p);
        city::check_site(self.map.as_ref(), &self.content, pos, existing)
    }

    /// Let `player` see the tiles within `radius` of `at`.
    pub fn explore(&mut self, player: city::PlayerId, at: city::Position, radius: u32) {
        if let (Some(map), true) = (self.map.as_ref(), self.fog.is_empty()) {
            self.fog = fog::Fog::new(map.width(), map.height());
        }
        self.fog.explore(player, at, radius);
    }

    /// The seed this game started from.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Whether `id` is the monsters' owner or an independent city's player: not a wizard.
    pub fn is_neutral_player(&self, id: city::PlayerId) -> bool {
        id == units::NEUTRAL || self.player(id).is_some_and(|p| p.neutral)
    }

    pub fn player(&self, id: city::PlayerId) -> Option<&player::Player> {
        self.players.iter().find(|p| p.id == id)
    }

    pub fn player_mut(&mut self, id: city::PlayerId) -> Option<&mut player::Player> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    fn execute(&mut self, cmd: Command) -> Vec<Event> {
        match cmd {
            Command::EndTurn => self.end_turn(),
            Command::CreateUnit { owner, pos, unit } => {
                let unit = self.spawn_unit(owner, pos, unit);
                vec![Event::UnitCreated { unit, pos }]
            }
            Command::MoveStack { units, to } => self.do_move(&units, to),
            Command::GoTo { units, to } => match self.find_path(&units, to) {
                Some(path) => {
                    for &u in &units {
                        let _ = self.world.insert_one(u, units::Order::GoTo(path.clone()));
                    }
                    self.follow_orders(&units)
                }
                None => vec![Event::Rejected(Rejection::Move(
                    movement::MoveError::Impassable,
                ))],
            },
            Command::ClearOrders { units } => {
                for u in units {
                    let _ = self.world.remove_one::<units::Order>(u);
                    let _ = self.world.remove_one::<roads::RoadWork>(u);
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
                let from = units.first().and_then(|&u| self.unit_position(u));
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
            Command::SettleOutpost { unit, name } => self.settle_outpost(unit, name),
            Command::SetProduction { city, item } => match self.set_production(city, item) {
                Ok(()) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::BuyProduction { city } => match self.buy_production(city) {
                Ok(_) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::RazeCity { player, city } => self.raze_city(player, city),
            Command::SellBuilding { city, building } => match self.sell_building(city, building) {
                Ok(_) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::SetFarmers { city, farmers } => {
                let figures = self
                    .world
                    .get::<&economy::Population>(city)
                    .map_or(0, |p| p.figures);
                let min = self.city_min_farmers(city);
                let _ = self
                    .world
                    .insert_one(city, economy::Farmers(farmers.clamp(min, figures)));
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
            } => reject(self.set_power_split(player, mana, research, skill)),
            Command::SetResearch { player, spell } => reject(self.set_research(player, spell)),
            Command::CancelCast { player } => reject(self.cancel_cast(player)),
            Command::Alchemy {
                player,
                amount,
                gold_to_mana,
            } => reject(self.alchemy(player, amount, gold_to_mana)),
            Command::CastSpell {
                player,
                spell,
                target,
            } => events_or_reject(self.start_cast(player, spell, target)),
            Command::ResolveCast { player, target } => {
                events_or_reject(self.finish_cast_with_target(player, target))
            }
        }
    }

    fn settle_outpost(&mut self, unit: hecs::Entity, name: String) -> Vec<Event> {
        let (Some(pos), Some(owner)) = (self.unit_position(unit), self.unit_owner(unit)) else {
            return vec![Event::Rejected(Rejection::Move(
                movement::MoveError::BadSelection,
            ))];
        };
        if !self.unit_has_ability(unit, "create_outpost") {
            return vec![Event::Rejected(Rejection::Production(
                production::ProductionError::WrongRace,
            ))];
        }
        if let Err(e) = self.can_found_city(pos) {
            return vec![Event::Rejected(Rejection::FoundCity(e))];
        }
        let race = self
            .unit_def(unit)
            .and_then(|d| d.race.as_deref())
            .and_then(|r| self.content.rules.races.id_of(r));
        let _ = self.world.despawn(unit);
        let city = self.spawn_city(owner, pos, name, race, 0);
        vec![Event::CityFounded { city, pos }]
    }

    fn do_move(&mut self, units: &[hecs::Entity], to: city::Position) -> Vec<Event> {
        let from = units.first().and_then(|&u| self.unit_position(u));
        match (self.move_stack(units, to), from) {
            (Ok(movement::Entry::Free), Some(from)) => {
                let mut events = vec![Event::UnitsMoved {
                    units: units.to_vec(),
                    from,
                    to,
                }];
                events.extend(self.collect_site(units, to));
                events
            }
            (Ok(movement::Entry::Site(lair)), Some(from)) => vec![Event::SiteEncounter {
                units: units.to_vec(),
                from,
                to,
                lair,
            }],
            (Ok(movement::Entry::Hostile(defenders)), Some(from)) => {
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
            let Some(pos) = self.unit_position(u) else {
                continue;
            };
            let order = self
                .world
                .get::<&units::Order>(u)
                .ok()
                .map(|o| (*o).clone());
            let Some(order) = order else { continue };
            match groups.iter_mut().find(|g| {
                let f = g[0];
                self.unit_position(f) == Some(pos)
                    && self
                        .world
                        .get::<&units::Order>(f)
                        .ok()
                        .map(|o| (*o).clone())
                        == Some(order.clone())
            }) {
                Some(g) => g.push(u),
                None => groups.push(vec![u]),
            }
        }
        for group in groups {
            let units::Order::GoTo(mut path) = self
                .world
                .get::<&units::Order>(group[0])
                .map(|o| (*o).clone())
                .expect("grouped by order");
            while let Some(&next) = path.first() {
                let from = self.unit_position(group[0]).expect("on map");
                match self.move_stack(&group, next) {
                    Ok(movement::Entry::Free) => {
                        path.remove(0);
                        events.push(Event::UnitsMoved {
                            units: group.clone(),
                            from,
                            to: next,
                        });
                        events.extend(self.collect_site(&group, next));
                    }
                    Ok(movement::Entry::Site(lair)) => {
                        events.push(Event::SiteEncounter {
                            units: group.clone(),
                            from,
                            to: next,
                            lair,
                        });
                        path.clear();
                    }
                    Ok(movement::Entry::Hostile(defenders)) => {
                        events.extend(self.hostile_entry(&group, from, next, defenders));
                        path.clear();
                    }
                    Err(movement::MoveError::NoMovesLeft) => break,
                    Err(_) => {
                        path.clear();
                    }
                }
            }
            for &u in &group {
                if path.is_empty() {
                    let _ = self.world.remove_one::<units::Order>(u);
                } else {
                    let _ = self.world.insert_one(u, units::Order::GoTo(path.clone()));
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
        from: city::Position,
        to: city::Position,
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
        let Some(owner) = units.first().and_then(|&u| self.unit_owner(u)) else {
            return Vec::new();
        };
        for &u in units {
            let _ = self.world.insert_one(u, to);
            let _ = self.world.insert_one(u, units::MovesLeft(0));
        }
        self.explore(owner, to, 2);
        let mut events = vec![Event::UnitsMoved {
            units: units.to_vec(),
            from,
            to,
        }];
        if let Some(city) = self.city_at(to) {
            events.extend(self.conquer_city(city, owner, 0));
        }
        events
    }

    fn end_turn(&mut self) -> Vec<Event> {
        self.close_raze_windows();
        let mut corruption = self.process_land_corruption();
        self.turn += 1;
        for unit in self.all_units() {
            let moves = self.unit_max_moves(unit);
            let _ = self.world.insert_one(unit, units::MovesLeft(moves));
        }
        let mut events = vec![Event::TurnStarted { turn: self.turn }];
        events.append(&mut corruption);
        events.extend(self.process_turn());
        events.extend(self.process_purify());
        events.extend(self.drown_stranded());
        events.extend(self.process_road_work());
        let with_orders: Vec<_> = self
            .world
            .query::<&units::Order>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        let mut with_orders = with_orders;
        self.sort_by_serial(&mut with_orders);
        events.extend(self.follow_orders(&with_orders));
        events
    }

    /// Next value from the game's seeded RNG (all randomness must come from here).
    pub fn next_u64(&mut self) -> u64 {
        self.draws += 1;
        self.rng.next_u64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_turn_advances_turn() {
        let mut g = Game::new(1);
        assert_eq!(
            g.apply(Command::EndTurn),
            vec![Event::TurnStarted { turn: 2 }]
        );
    }

    #[test]
    fn same_seed_same_rng_stream() {
        let (mut a, mut b) = (Game::new(42), Game::new(42));
        assert_eq!((a.next_u64(), a.next_u64()), (b.next_u64(), b.next_u64()));
    }
}
