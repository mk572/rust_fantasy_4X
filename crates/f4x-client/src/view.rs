//! The overworld: map, units, fog, selection and orders. The city screen is in
//! `city_screen.rs`.

use std::collections::HashMap;
use std::path::Path;

use f4x_core::city::{PlayerId, Position};
use f4x_core::content::{Content, TerrainClass};
use f4x_core::map::Plane;
use f4x_core::mapgen::GenParams;
use f4x_core::setup::{self, NewGame};
use f4x_core::wizard::WizardBuild;
use f4x_core::{Command, Event, Game};
use macroquad::prelude::*;

use crate::art::{terrain_type as tt, Art, TILE_H, TILE_W};
use crate::frame::Align;
use crate::options_screen::OptionsState;
use crate::ui::{Lines, GOLD};

pub(crate) const HUMAN: PlayerId = PlayerId(0);

/// Where F5 / F9 save and load, and where each turn is saved automatically.
pub const QUICKSAVE: &str = "saves/quick.ron";
pub const AUTOSAVE: &str = "saves/autosave.ron";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Map,
    City(hecs::Entity),
    Magic,
    /// A tactical battle is being fought.
    Battle,
    /// Creating the human wizard before the game begins.
    Create,
    /// The new-game options (difficulty, opponents, land size, magic power).
    Options,
    /// The game has ended: result and score.
    GameOver,
    /// The text console (F2).
    Console,
    /// The start menu: Continue, Load Game, New Game, Hall of Fame, Quit.
    MainMenu,
    /// The save slots: load, save, and leave the game.
    Slots,
}

pub struct App {
    pub(crate) game: Game,
    pub(crate) art: Art,
    params: GenParams,
    /// The new-game options chosen before the wizard is made.
    pub(crate) options: OptionsState,
    pub(crate) seed: u64,
    pub(crate) plane: Plane,
    /// Top-left visible tile (fractional while panning).
    pub(crate) cam: Vec2,
    zoom: f32,
    /// Index into `content.rules.races` the human plays.
    pub(crate) race: usize,
    pub(crate) screen: Screen,
    pub(crate) selection: Vec<hecs::Entity>,
    pub(crate) log: Vec<String>,
    /// The production menu is open on the city screen.
    pub(crate) choosing: bool,
    pub(crate) choice_scroll: usize,
    /// Scroll position of the research and spell lists on the magic screen.
    pub(crate) magic_scroll: [usize; 2],
    /// A spell waiting for a target to be clicked on the map.
    pub(crate) targeting: Option<crate::magic_screen::Targeting>,
    /// The key list is shown (F1 shows it while held).
    pub(crate) show_keys: bool,
    /// The wizard creation screen's state.
    pub(crate) create: crate::create_screen::CreateState,
    /// The human wizard chosen on the creation screen; new maps reuse it.
    pub(crate) human_build: Option<f4x_core::wizard::WizardBuild>,
    /// The battle screen's state.
    pub(crate) battle_ui: crate::battle_screen::BattleUi,
    /// The player has seen the game-over screen and chose to look at the map.
    pub(crate) game_over_seen: bool,
    pub(crate) menu: crate::menu_screen::MenuState,
    /// The text console's state (F2).
    pub(crate) console: crate::console_screen::ConsoleUi,
    /// A city just taken, waiting for the player to keep or raze it.
    pub(crate) raze_prompt: Option<hecs::Entity>,
    /// A Spell Ward waiting for its realm: the spell, the city, and whether the spell is already
    /// paid for (`Targeting::resolve`).
    pub(crate) ward_prompt: Option<(f4x_core::rules::SpellId, hecs::Entity, bool)>,
}

pub(crate) fn base_type(class: TerrainClass) -> usize {
    match class {
        TerrainClass::Ocean => tt::OCEAN,
        TerrainClass::Grassland => tt::GRASSLAND,
        TerrainClass::Forest => tt::FOREST,
        TerrainClass::Hills => tt::HILLS,
        TerrainClass::Mountains => tt::MOUNTAINS,
        TerrainClass::Swamp => tt::SWAMP,
        TerrainClass::Desert => tt::DESERT,
        TerrainClass::Tundra => tt::TUNDRA,
    }
}

impl App {
    pub fn new(content: Content, art: Art, seed: u64) -> Result<Self, String> {
        let params = GenParams::default();
        // Start as the first race that has city names, else the first race.
        let race = content
            .rules
            .races
            .defs()
            .iter()
            .position(|r| !r.city_names.is_empty())
            .unwrap_or(0);
        let options = OptionsState::default();
        let game = Self::new_game(seed, &options, &content, None, race)?;
        let mut app = Self {
            game,
            art,
            params,
            options,
            seed,
            plane: Plane::Arcanus,
            cam: Vec2::ZERO,
            zoom: 4.0,
            race,
            screen: Screen::Map,
            selection: Vec::new(),
            log: Vec::new(),
            choosing: false,
            choice_scroll: 0,
            magic_scroll: [0, 0],
            targeting: None,
            show_keys: false,
            create: crate::create_screen::CreateState::empty(),
            human_build: None,
            battle_ui: Default::default(),
            game_over_seen: false,
            menu: Default::default(),
            console: Default::default(),
            raze_prompt: None,
            ward_prompt: None,
        };
        app.start_game();
        Ok(app)
    }

    /// A new game from the options and the human wizard; `race` is an index into the races.
    fn new_game(
        seed: u64,
        options: &OptionsState,
        content: &Content,
        human: Option<&WizardBuild>,
        race: usize,
    ) -> Result<Game, String> {
        let preset = human
            .filter(|b| b.preset)
            .and_then(|b| {
                content
                    .rules
                    .wizards
                    .defs()
                    .iter()
                    .find(|w| w.name == b.name)
            })
            .map(|w| w.id.clone());
        let spec = NewGame {
            seed,
            wizards: options.wizards(),
            preset,
            build: human.cloned(),
            race: content.rules.races.defs().get(race).map(|r| r.id.clone()),
            land_percent: options.land_percent(),
            difficulty: options.difficulty,
            magic_strength: options.magic,
            ..NewGame::default()
        };
        setup::new_game(content, &spec)
    }

    /// Look at the human wizard's fortress, where the game starts.
    fn start_game(&mut self) {
        let fortress = self
            .game
            .cities()
            .into_iter()
            .find(|(_, c, _)| c.owner == HUMAN)
            .map(|(_, _, pos)| pos);
        if let Some(pos) = fortress {
            self.center_on(pos);
        }
    }

    fn center_on(&mut self, pos: Position) {
        self.plane = pos.plane;
        let (cols, rows) = self.visible_tiles();
        self.cam = vec2(
            pos.x as f32 - cols / 2.0,
            (pos.y as f32 - rows / 2.0).max(0.0),
        );
    }

    /// The banner colour of a player (by number; 255 is the neutral player).
    pub(crate) fn banner_color(&self, owner: u8) -> Color {
        let banner = self
            .game
            .player(PlayerId(owner))
            .map_or(f4x_core::player::NEUTRAL_BANNER, |p| p.banner);
        self.art.banner_color(banner)
    }

    /// The banner number of a player (by number; 255 is the neutral player).
    pub(crate) fn banner_index(&self, owner: u8) -> u8 {
        self.game
            .player(PlayerId(owner))
            .map_or(f4x_core::player::NEUTRAL_BANNER, |p| p.banner)
    }

    pub(crate) fn race_name(&self, race: usize) -> &str {
        &self.game.content.rules.races.defs()[race].name
    }

    fn next_city_name(&self) -> String {
        let race = &self.game.content.rules.races.defs()[self.race];
        let used = self.game.cities().len();
        race.city_names
            .get(used % race.city_names.len().max(1))
            .cloned()
            .unwrap_or_else(|| format!("City {}", used + 1))
    }

    /// Turn events into log lines; returns whether a city was founded.
    pub(crate) fn note(&mut self, events: Vec<Event>) -> bool {
        let mut founded = false;
        let mut follow = Vec::new();
        for event in events {
            let line = match event {
                Event::CityFounded { city, .. } => {
                    founded = true;
                    let name = self.city_name(city);
                    Some(format!("Founded {name}"))
                }
                Event::Rejected(why) => Some(format!("Cannot: {why:?}")),
                Event::ProductionCompleted { city, item } => {
                    let what = self.item_name(item);
                    Some(format!("{}: {what} complete", self.city_name(city)))
                }
                Event::ChaosRiftStrikes {
                    city,
                    units_lost,
                    building,
                } => {
                    let mut line = format!("The chaos rift over {} strikes", self.city_name(city));
                    if units_lost > 0 {
                        line += &format!(": {units_lost} unit(s) destroyed");
                    }
                    if let Some(b) = building {
                        let name = &self.game.content.rules.buildings.get(b).name;
                        line += &format!(", {name} falls");
                    }
                    Some(line)
                }
                Event::CityGrew { city, figures } => {
                    Some(format!("{} grows to {figures}", self.city_name(city)))
                }
                Event::CityShrank { city, figures } => {
                    Some(format!("{} shrinks to {figures}", self.city_name(city)))
                }
                Event::UnitsDisbanded { units, .. } => {
                    Some(format!("{units} unit(s) disbanded: unpaid or unfed"))
                }
                Event::AttackRequested {
                    attackers,
                    defenders,
                    ..
                } => {
                    if attackers
                        .first()
                        .is_some_and(|&u| self.game.unit_owner(u) == Some(HUMAN))
                    {
                        follow.push(Command::BeginBattle {
                            attackers,
                            defenders,
                        });
                    }
                    None
                }
                Event::SiteEncounter { units, to, .. } => {
                    if units
                        .first()
                        .is_some_and(|&u| self.game.unit_owner(u) == Some(HUMAN))
                    {
                        follow.push(Command::EnterSite { units, to });
                    }
                    None
                }
                Event::SiteCleared { loot, .. } => Some(loot_line(&loot)),
                Event::CityLooted {
                    city,
                    gold,
                    citizens_lost,
                    buildings_lost,
                } => {
                    let mine = self.game.city_owner(city) == Some(HUMAN);
                    if mine {
                        self.raze_prompt = Some(city);
                    }
                    Some(format!(
                        "{} {}: {gold} gold looted, {citizens_lost} citizens and {buildings_lost} buildings lost",
                        self.city_name(city),
                        if mine { "is yours" } else { "falls" },
                    ))
                }
                Event::CityRazed { gold, .. } => {
                    Some(format!("The city is razed: {gold} gold looted"))
                }
                Event::CityDestroyed { .. } => Some("A city is destroyed".to_string()),
                Event::WizardBanished { player, .. } if player == HUMAN => Some(
                    "Your fortress has fallen! You are banished and cast the Spell of Return"
                        .to_string(),
                ),
                Event::WizardBanished { .. } => Some("A rival wizard is banished".to_string()),
                Event::WizardReturned { player, .. } if player == HUMAN => {
                    Some("You have returned to your fortress".to_string())
                }
                Event::WizardDefeated { player } => {
                    Some(format!("Wizard {} is defeated", player.0))
                }
                Event::FortressMoved { .. } => Some("The fortress has moved".to_string()),
                Event::UnitsDrowned { units, .. } => {
                    Some(format!("{units} units drown without a ship"))
                }
                Event::RoadBuilt { .. } => Some("A road square is finished".to_string()),
                Event::RoadsEnchanted { .. } => Some("The roads are enchanted".to_string()),
                Event::VolcanoRaised { .. } => Some("A volcano rises".to_string()),
                Event::NodeMelded { .. } => Some("The spirit melds with the node".to_string()),
                Event::MeldFailed { .. } => {
                    Some("The Guardian Spirit in this node kills your spirit".to_string())
                }
                Event::BuildingsCollapsed { count, .. } => {
                    Some(format!("{count} buildings collapse"))
                }
                Event::TerrainChanged { .. } => Some("The land changes".to_string()),
                Event::SummoningCircleMoved { .. } => {
                    Some("The summoning circle has moved".to_string())
                }
                Event::SpellLearned { spell, .. } => Some(format!(
                    "Learned {}: choose new research (M)",
                    self.game.content.rules.spells.get(spell).name
                )),
                Event::SpellCast { spell, .. } => Some(format!(
                    "Cast {}",
                    self.game.content.rules.spells.get(spell).name
                )),
                Event::SpellFizzled { spell, city, .. } => Some(format!(
                    "{} fizzles: nightshade protects {}",
                    self.game.content.rules.spells.get(spell).name,
                    self.city_name(city)
                )),
                Event::SpellSuppressed { spell, by, .. } => Some(format!(
                    "{} is stopped by {}'s Suppress Magic",
                    self.game.content.rules.spells.get(spell).name,
                    self.game.player(by).map_or("a rival", |p| p.name.as_str())
                )),
                Event::SpellBlasted {
                    spell,
                    victim,
                    mana,
                    ..
                } => Some(format!(
                    "Spell Blast ends {}'s {} ({mana} mana)",
                    self.game
                        .player(victim)
                        .map_or("a rival", |p| p.name.as_str()),
                    self.game.content.rules.spells.get(spell).name
                )),
                Event::GlobalSpellRemoved { victim, spell, .. } => Some(format!(
                    "{}'s {} is ended",
                    self.game
                        .player(victim)
                        .map_or("a rival", |p| p.name.as_str()),
                    self.game.content.rules.spells.get(spell).name
                )),
                Event::GlobalSpellTaken { victim, spell, .. } => Some(format!(
                    "Took {}'s {}",
                    self.game
                        .player(victim)
                        .map_or("a rival", |p| p.name.as_str()),
                    self.game.content.rules.spells.get(spell).name
                )),
                Event::DispelFailed { spell, .. } => Some(format!(
                    "The dispel of {} fails",
                    self.game.content.rules.spells.get(spell).name
                )),
                Event::Dispelled { removed, .. } => Some(format!(
                    "{removed} enchantment{} dispelled",
                    if removed == 1 { "" } else { "s" }
                )),
                Event::SpellReady { spell, .. } => Some(format!(
                    "{} is ready: choose its target (M)",
                    self.game.content.rules.spells.get(spell).name
                )),
                Event::SpellHasNoEffectYet { spell } => Some(format!(
                    "{} has no effect yet (not implemented)",
                    self.game.content.rules.spells.get(spell).name
                )),
                Event::EnchantmentExpired { spell, .. } => Some(format!(
                    "{} has run out",
                    self.game.content.rules.spells.get(spell).name
                )),
                Event::EnchantmentLapsed { spell, .. } => Some(format!(
                    "{} ended: not enough mana for its upkeep",
                    self.game.content.rules.spells.get(spell).name
                )),
                _ => None,
            };
            if let Some(line) = line {
                self.log.push(line);
            }
        }
        if self.log.len() > 40 {
            self.log.drain(..self.log.len() - 40);
        }
        for command in follow {
            self.begin_battle(command);
        }
        founded
    }

    pub(crate) fn city_name(&self, city: hecs::Entity) -> String {
        self.game
            .world
            .get::<&f4x_core::city::City>(city)
            .map_or_else(|_| "?".to_string(), |c| c.name.clone())
    }

    pub(crate) fn item_name(&self, item: f4x_core::production::ProductionItem) -> String {
        use f4x_core::production::ProductionItem::*;
        let rules = &self.game.content.rules;
        match item {
            Building(b) => rules.buildings.get(b).name.clone(),
            Unit(u) => rules.units.get(u).name.clone(),
            TradeGoods => "Trade Goods".into(),
            Housing => "Housing".into(),
        }
    }

    pub fn open_first_city(&mut self) {
        if let Some((city, ..)) = self.game.cities().first() {
            self.screen = Screen::City(*city);
        }
    }

    /// Write the game to `path`, reporting the outcome in the message log.
    pub fn save_to(&mut self, path: &Path) {
        match f4x_data::save::save_game(path, &self.game) {
            Ok(()) => self.log.push(format!(
                "Saved turn {} to {}",
                self.game.turn,
                path.display()
            )),
            Err(e) => self.log.push(format!("Save failed: {e}")),
        }
    }

    /// Replace the game with the one saved at `path`. On failure the current game is kept.
    pub fn load_from(&mut self, path: &Path) -> Result<(), String> {
        let content = self.game.content.clone();
        let game = f4x_data::save::load_game(path, content).map_err(|e| e.to_string())?;
        self.seed = game.seed();
        // Keep playing the same race: the human player's capital race.
        if let Some(r) = game.player(HUMAN).and_then(|p| p.capital_race) {
            self.race = r.index();
        }
        self.game = game;
        self.game.human.get_or_insert(HUMAN);
        self.game.set_undo(true);
        self.game_over_seen = false;
        self.screen = Screen::Map;
        self.choosing = false;
        self.selection.clear();
        let home = self
            .game
            .cities()
            .into_iter()
            .find(|(_, c, _)| c.owner == HUMAN)
            .map(|(_, _, p)| p);
        if let Some(pos) = home {
            self.center_on(pos);
        }
        self.log.push(format!(
            "Loaded turn {} from {}",
            self.game.turn,
            path.display()
        ));
        Ok(())
    }

    /// Take back the last order (Ctrl or Cmd with Z). Units are rebuilt by an undo, so the
    /// selection is found again by creation number.
    fn undo_last(&mut self) {
        let serials: Vec<_> = self
            .selection
            .iter()
            .map(|&u| self.game.serial_of(u))
            .collect();
        match self.game.undo() {
            Ok(label) => {
                self.selection = serials
                    .into_iter()
                    .filter_map(|s| self.game.entity_of_serial(s))
                    .collect();
                self.targeting = None;
                self.log.push(format!("Undid {label}"));
            }
            Err(_) => self.log.push(
                "Nothing to undo (ending a turn, discoveries and dice cannot be undone)".into(),
            ),
        }
    }

    /// End the turn. First put more citizens on farms if the army would otherwise go hungry
    /// (the city screen lets you set farmers by hand; this just protects the troops).
    fn end_turn(&mut self) {
        let t = self.game.treasury(HUMAN);
        if t.food_surplus < t.food_upkeep {
            self.game.balance_farmers(HUMAN);
        }
        let events = self.game.apply(Command::EndTurn);
        let turn = self.game.turn;
        self.note(events);
        self.log.push(format!("Turn {turn}"));
    }

    /// Pass turns without playing them (for screenshots and smoke tests).
    pub fn end_turns(&mut self, turns: u32) {
        for _ in 0..turns {
            self.end_turn();
        }
    }

    pub fn select_first_stack(&mut self) {
        if let Some(&u) = self.game.player_units(HUMAN).first() {
            if let Some(at) = self.game.unit_position(u) {
                self.select_at(at);
            }
        }
    }

    fn tile_size(&self) -> Vec2 {
        vec2(TILE_W * self.zoom, TILE_H * self.zoom)
    }

    pub(crate) fn map_rect(&self) -> Rect {
        crate::frame::Layout::new().rect(crate::frame::spot::MAP)
    }

    pub(crate) fn visible_tiles(&self) -> (f32, f32) {
        let r = self.map_rect();
        let t = self.tile_size();
        (r.w / t.x, r.h / t.y)
    }

    fn tile_at(&self, screen: Vec2) -> Option<Position> {
        let r = self.map_rect();
        if !r.contains(screen) {
            return None;
        }
        let t = self.tile_size();
        let x = ((screen.x - r.x) / t.x + self.cam.x).floor() as i32;
        let y = ((screen.y - r.y) / t.y + self.cam.y).floor() as i32;
        let grid = self.game.map.as_ref()?.plane(self.plane);
        (y >= 0 && y < grid.height() as i32).then(|| Position {
            plane: self.plane,
            x: grid.wrap_x(x),
            y: y as u32,
        })
    }

    fn clamp_camera(&mut self) {
        let (_, rows) = self.visible_tiles();
        let h = self.params.height as f32;
        self.cam.y = self.cam.y.clamp(0.0, (h - rows).max(0.0));
        self.cam.x = self.cam.x.rem_euclid(self.params.width as f32);
    }

    pub(crate) fn new_map(&mut self, seed: u64) {
        let content = self.game.content.clone();
        match Self::new_game(
            seed,
            &self.options,
            &content,
            self.human_build.as_ref(),
            self.race,
        ) {
            Ok(game) => {
                self.game = game;
                self.seed = seed;
                self.game_over_seen = false;
                self.screen = Screen::Map;
                self.selection.clear();
                self.log.clear();
                self.start_game();
                self.log.push(format!("New map, seed {seed}"));
            }
            Err(e) => self.log.push(format!("Map generation failed: {e}")),
        }
    }

    /// Explore every square on both planes (for screenshots and tests).
    pub fn reveal_all(&mut self) {
        let Some(map) = self.game.map.as_ref() else {
            return;
        };
        let (w, h) = (map.width(), map.height());
        for plane in Plane::ALL {
            self.game.fog.explore(
                HUMAN,
                Position {
                    plane,
                    x: w / 2,
                    y: h / 2,
                },
                w.max(h),
            );
        }
    }

    /// Put the top-left of the map view on a tile.
    pub fn set_camera(&mut self, x: f32, y: f32) {
        self.cam = vec2(x, y);
    }

    /// Handle input and draw one frame. Returns false when the player quits.
    pub fn frame(&mut self) -> bool {
        self.selection
            .retain(|&u| self.game.unit_position(u).is_some());
        if self.game.in_battle() {
            if !self.battle_ui.ready {
                self.init_battle_ui();
            }
            if !matches!(self.screen, Screen::Battle | Screen::Console) {
                self.screen = Screen::Battle;
            }
        } else {
            self.battle_ui.ready = false;
        }
        if is_key_pressed(KeyCode::F2) {
            self.toggle_console();
        }
        if let Some(city) = self.raze_prompt {
            if !self.game.in_battle() && !matches!(self.screen, Screen::Console) {
                return self.raze_prompt_frame(city);
            }
        }
        if let Some(prompt) = self.ward_prompt {
            if !self.game.in_battle() && !matches!(self.screen, Screen::Console) {
                return self.ward_prompt_frame(prompt);
            }
        }
        if self.game.game_over.is_some()
            && !self.game_over_seen
            && !self.game.in_battle()
            && !matches!(self.screen, Screen::GameOver | Screen::Console)
        {
            self.screen = Screen::GameOver;
        }
        match self.screen {
            Screen::GameOver => return self.game_over_screen(),
            Screen::Console => self.console_screen(),
            Screen::City(city) => {
                self.city_screen(city);
            }
            Screen::Magic => self.magic_screen(),
            Screen::Battle => self.battle_screen(),
            Screen::Create => self.create_screen(),
            Screen::Options => self.options_screen(),
            Screen::MainMenu => return self.main_menu_screen(),
            Screen::Slots => self.slots_screen(),
            Screen::Map => {
                if is_key_pressed(KeyCode::Escape) {
                    if self.targeting.take().is_some() {
                        self.log.push("Cast cancelled".into());
                    } else {
                        self.open_slots(false);
                    }
                }
                self.overworld_input();
                clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
                self.draw_map();
                self.draw_bars();
            }
        }
        true
    }

    /// The question after aiming a Spell Ward at a city: which realm to ward it against, by the
    /// first letter of the realm, shown over the map. Realms already warded are not offered.
    fn ward_prompt_frame(
        &mut self,
        (spell, city, resolve): (f4x_core::rules::SpellId, hecs::Entity, bool),
    ) -> bool {
        const KEYS: [(KeyCode, &str); 5] = [
            (KeyCode::N, "nature"),
            (KeyCode::S, "sorcery"),
            (KeyCode::C, "chaos"),
            (KeyCode::L, "life"),
            (KeyCode::D, "death"),
        ];
        let open: Vec<(KeyCode, &str)> = KEYS
            .into_iter()
            .filter(|(_, r)| !self.game.city_warded_against(city, r))
            .collect();
        if is_key_pressed(KeyCode::Escape) || open.is_empty() {
            self.ward_prompt = None;
            self.log.push("Cast cancelled".into());
        } else if let Some(&(_, realm)) = open.iter().find(|(k, _)| is_key_pressed(*k)) {
            self.ward_prompt = None;
            if let Some(realm) = self.game.content.rules.realms.id_of(realm) {
                let target = f4x_core::rules::exec::Target::CityRealm(city, realm);
                let cmd = if resolve {
                    Command::ResolveCast {
                        player: HUMAN,
                        target,
                    }
                } else {
                    Command::CastSpell {
                        player: HUMAN,
                        spell,
                        target: Some(target),
                    }
                };
                let events = self.game.apply(cmd);
                self.note(events);
            }
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(
            w / 2.0 - 260.0,
            h / 2.0 - 70.0,
            520.0,
            140.0,
            Color::new(0.08, 0.06, 0.04, 0.95),
        );
        draw_rectangle_lines(w / 2.0 - 260.0, h / 2.0 - 70.0, 520.0, 140.0, 3.0, GOLD);
        let name = self.city_name(city);
        crate::frame::text_centered(
            &format!("Select a Spell Ward for {name}"),
            w / 2.0,
            h / 2.0 - 25.0,
            22.0,
            WHITE,
        );
        let choices: Vec<String> = open
            .iter()
            .map(|(_, r)| {
                let mut c = r.chars();
                let first = c.next().map(|f| f.to_ascii_uppercase()).unwrap_or(' ');
                format!("{first}: {r}")
            })
            .collect();
        crate::frame::text_centered(&choices.join("   "), w / 2.0, h / 2.0 + 15.0, 22.0, GOLD);
        crate::frame::text_centered("Esc: cancel", w / 2.0, h / 2.0 + 50.0, 18.0, WHITE);
        true
    }

    /// The question after taking a city: keep it (Enter) or raze it (R), shown over the map.
    fn raze_prompt_frame(&mut self, city: hecs::Entity) -> bool {
        let gone = self.game.city_owner(city) != Some(HUMAN)
            || self
                .game
                .world
                .get::<&f4x_core::conquest::Conquered>(city)
                .is_err();
        if gone {
            self.raze_prompt = None;
            return true;
        }
        if is_key_pressed(KeyCode::R) {
            self.raze_prompt = None;
            let events = self.game.apply(Command::RazeCity {
                player: HUMAN,
                city,
            });
            self.note(events);
        } else if is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::N)
            || is_key_pressed(KeyCode::Escape)
        {
            self.raze_prompt = None;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(
            w / 2.0 - 260.0,
            h / 2.0 - 60.0,
            520.0,
            120.0,
            Color::new(0.08, 0.06, 0.04, 0.95),
        );
        draw_rectangle_lines(w / 2.0 - 260.0, h / 2.0 - 60.0, 520.0, 120.0, 3.0, GOLD);
        let name = self.city_name(city);
        crate::frame::text_centered(
            &format!("{name} is yours. Do you wish to completely destroy this city?"),
            w / 2.0,
            h / 2.0 - 15.0,
            22.0,
            WHITE,
        );
        crate::frame::text_centered(
            "R: raze it      Enter: keep it",
            w / 2.0,
            h / 2.0 + 30.0,
            22.0,
            GOLD,
        );
        true
    }

    fn select_at(&mut self, pos: Position) {
        let mine: Vec<_> = self
            .game
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.game.unit_owner(u) == Some(HUMAN))
            .collect();
        self.selection = mine;
    }

    fn selection_position(&self) -> Option<Position> {
        self.selection
            .first()
            .and_then(|&u| self.game.unit_position(u))
    }

    fn overworld_input(&mut self) {
        let step = 12.0 * get_frame_time();
        let mut pan = Vec2::ZERO;
        for (key, dir) in [
            (KeyCode::Left, vec2(-1.0, 0.0)),
            (KeyCode::A, vec2(-1.0, 0.0)),
            (KeyCode::Right, vec2(1.0, 0.0)),
            (KeyCode::D, vec2(1.0, 0.0)),
            (KeyCode::Up, vec2(0.0, -1.0)),
            (KeyCode::W, vec2(0.0, -1.0)),
            (KeyCode::Down, vec2(0.0, 1.0)),
            (KeyCode::S, vec2(0.0, 1.0)),
        ] {
            if is_key_down(key) {
                pan += dir;
            }
        }
        self.cam += pan * step;
        if is_key_pressed(KeyCode::Tab) {
            self.plane = if self.plane == Plane::Arcanus {
                Plane::Myrror
            } else {
                Plane::Arcanus
            };
        }
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            self.zoom = (self.zoom + 1.0).min(6.0);
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            self.zoom = (self.zoom - 1.0).max(1.0);
        }
        if is_key_pressed(KeyCode::Enter) {
            self.end_turn();
            self.save_to(Path::new(AUTOSAVE));
        }
        let command_key = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::LeftSuper);
        if command_key && is_key_pressed(KeyCode::Z) {
            self.undo_last();
        }
        if is_key_pressed(KeyCode::F5) {
            self.save_to(Path::new(QUICKSAVE));
        }
        if is_key_pressed(KeyCode::F8) {
            let on = !self.game.player(HUMAN).is_some_and(|p| p.grand_vizier);
            self.game
                .apply(Command::SetGrandVizier { player: HUMAN, on });
            self.log.push(
                if on {
                    "Grand Vizier on: your cities choose their next building"
                } else {
                    "Grand Vizier off"
                }
                .into(),
            );
        }
        if is_key_pressed(KeyCode::F9) {
            if let Err(e) = self.load_from(Path::new(QUICKSAVE)) {
                self.log.push(format!("Load failed: {e}"));
            }
        }
        if is_key_pressed(KeyCode::M) {
            self.screen = Screen::Magic;
        }
        if is_key_pressed(KeyCode::N) {
            self.new_map(self.seed + 1);
        }
        if is_key_pressed(KeyCode::R) {
            self.race = (self.race + 1) % self.game.content.rules.races.len().max(1);
            self.new_map(self.seed);
            let name = self.race_name(self.race).to_string();
            self.log.push(format!("Playing as {name}"));
        }
        if is_key_pressed(KeyCode::B) {
            self.settle();
        }
        if is_key_pressed(KeyCode::U) && !self.selection.is_empty() {
            let events = self.game.apply(Command::Meld {
                units: self.selection.clone(),
            });
            self.note(events);
        }
        if is_key_pressed(KeyCode::P) && !self.selection.is_empty() {
            let events = self.game.apply(Command::ShiftPlane {
                units: self.selection.clone(),
            });
            if let Some(at) = self.selection_position() {
                self.plane = at.plane;
            }
            self.note(events);
        }
        if is_key_pressed(KeyCode::T) {
            self.game.balance_farmers(HUMAN);
            self.log.push("Farmers balanced to feed the army".into());
        }
        if is_key_pressed(KeyCode::F) && !self.selection.is_empty() {
            self.game.apply(Command::ClearOrders {
                units: self.selection.clone(),
            });
        }
        if is_key_pressed(KeyCode::C) {
            let at = self
                .selection_position()
                .or_else(|| self.tile_at(mouse_position().into()));
            if let Some(city) = at.and_then(|p| self.game.city_at(p)) {
                self.screen = Screen::City(city);
            }
        }
        self.clamp_camera();

        let hovered = self.tile_at(mouse_position().into());
        if let Some(pos) = hovered {
            if is_mouse_button_pressed(MouseButton::Left) {
                self.click(pos);
            }
            if is_mouse_button_pressed(MouseButton::Right) && !self.selection.is_empty() {
                let events = self.game.apply(Command::GoTo {
                    units: self.selection.clone(),
                    to: pos,
                });
                self.note(events);
            }
        }
    }

    fn click(&mut self, pos: Position) {
        if let Some(t) = self.targeting.take() {
            self.aim_at(t, pos);
            return;
        }
        let has_mine = self
            .game
            .units_at(pos)
            .iter()
            .any(|&u| self.game.unit_owner(u) == Some(HUMAN));
        if has_mine {
            self.select_at(pos);
        } else if let Some(city) = self.game.city_at(pos) {
            self.selection.clear();
            if self.game.city_owner(city) == Some(HUMAN) {
                self.screen = Screen::City(city);
            }
        } else {
            self.selection.clear();
        }
    }

    fn settle(&mut self) {
        // Engineers build a road where they stand; settlers found an outpost.
        let builders: Vec<_> = self
            .selection
            .iter()
            .copied()
            .filter(|&u| self.game.unit_stat(u, f4x_core::rules::Stat::Construction) > 0)
            .collect();
        let has_settlers = self
            .selection
            .iter()
            .any(|&u| self.game.unit_has_ability(u, "create_outpost"));
        if !builders.is_empty() && !has_settlers {
            let events = self.game.apply(Command::BuildRoad {
                units: self.selection.clone(),
                path: Vec::new(),
            });
            self.note(events);
            return;
        }
        let settler = self
            .selection
            .iter()
            .copied()
            .find(|&u| self.game.unit_has_ability(u, "create_outpost"));
        let Some(unit) = settler else {
            self.log.push("Select settlers first".into());
            return;
        };
        let name = self.next_city_name();
        let events = self.game.apply(Command::SettleOutpost { unit, name });
        self.note(events);
    }

    fn draw_map(&mut self) {
        let Some(map) = self.game.map.as_ref() else {
            return;
        };
        let grid = map.plane(self.plane);
        let r = self.map_rect();
        let t = self.tile_size();
        let (cols, rows) = self.visible_tiles();
        let (x0, y0) = (self.cam.x.floor() as i32, self.cam.y.floor() as i32);
        let off = vec2(
            (self.cam.x - x0 as f32) * t.x,
            (self.cam.y - y0 as f32) * t.y,
        );
        let width = grid.width();
        let visible = self.game.visibility(HUMAN, self.plane);
        let cities = self.game.cities();
        let anim = (get_time() * 2.5) as usize;

        // Clip the map to its viewport.
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        let dpi = screen_dpi_scale();
        gl.quad_gl.scissor(Some((
            (r.x * dpi) as i32,
            ((screen_height() - r.y - r.h) * dpi) as i32,
            (r.w * dpi) as i32,
            (r.h * dpi) as i32,
        )));
        let screen_of = |x: u32, y: u32| -> Vec2 {
            let mut dx = x as f32 - x0 as f32;
            if dx < -1.0 {
                dx += width as f32;
            }
            vec2(
                r.x + dx * t.x - off.x,
                r.y + (y as f32 - y0 as f32) * t.y - off.y,
            )
        };
        for j in 0..=(rows.ceil() as i32) {
            let y = y0 + j;
            if y < 0 || y >= grid.height() as i32 {
                continue;
            }
            for i in 0..=(cols.ceil() as i32) {
                let wx = grid.wrap_x(x0 + i);
                let (sx, sy) = (r.x + i as f32 * t.x - off.x, r.y + j as f32 * t.y - off.y);
                if !self.game.fog.is_explored(HUMAN, self.plane, wx, y as u32) {
                    draw_rectangle(sx, sy, t.x, t.y, BLACK);
                    continue;
                }
                crate::map_draw::draw_square(
                    &self.game,
                    &mut self.art,
                    self.plane,
                    wx,
                    y as u32,
                    sx,
                    sy,
                    t,
                    anim,
                );
            }
        }
        for (entity, city, pos) in &cities {
            if pos.plane != self.plane
                || !self.game.fog.is_explored(HUMAN, self.plane, pos.x, pos.y)
            {
                continue;
            }
            let at = screen_of(pos.x, pos.y);
            crate::map_draw::draw_city(&self.game, &mut self.art, *entity, at.x, at.y, t);
            let owner = city.owner;
            if self.game.player(owner).and_then(|p| p.fortress) == Some(*pos) {
                // A gold star: the wizard's fortress.
                draw_poly(
                    at.x + t.x * 0.15,
                    at.y + t.y * 0.25,
                    5,
                    t.x * 0.14,
                    0.0,
                    GOLD,
                );
            }
            if self.game.player(owner).and_then(|p| p.summon_at) == Some(*pos) {
                draw_circle_lines(at.x + t.x * 0.85, at.y + t.y * 0.85, t.x * 0.12, 2.0, GOLD);
            }
            // The name under the city, in the original's small font.
            let k = t.x / TILE_W;
            let name = city.name.clone();
            let (cx, cy) = ((at.x + t.x / 2.0) / k, (at.y + t.y) / k - 2.0);
            self.bm_text_outlined((0.0, 0.0, k), 1, &name, cx, cy, Align::Center);
        }
        self.draw_units(&screen_of, &visible, width, t);
        gl.flush();
        gl.quad_gl.scissor(None);
    }

    /// One icon per occupied tile (the selected stack's first unit, else the first unit).
    fn draw_units(
        &mut self,
        screen_of: &dyn Fn(u32, u32) -> Vec2,
        visible: &[bool],
        width: u32,
        t: Vec2,
    ) {
        let mut stacks: HashMap<(u32, u32), Vec<hecs::Entity>> = HashMap::new();
        for unit in self.game.all_units() {
            let Some(at) = self.game.unit_position(unit) else {
                continue;
            };
            if at.plane != self.plane {
                continue;
            }
            let mine = self.game.unit_owner(unit) == Some(HUMAN);
            if !mine
                && (!visible[(at.y * width + at.x) as usize]
                    || !self.game.unit_visible_to(unit, HUMAN))
            {
                continue;
            }
            stacks.entry((at.x, at.y)).or_default().push(unit);
        }
        let blink = (get_time() * 3.0) as i64 % 2 == 0;
        for ((x, y), units) in stacks {
            let top = units
                .iter()
                .copied()
                .find(|u| self.selection.contains(u))
                .unwrap_or(units[0]);
            let owner = self.game.unit_owner(top).map_or(u8::MAX, |o| o.0);
            let art = self.game.unit_def(top).and_then(|d| d.art.clone());
            let at = screen_of(x, y);
            let inset = t * 0.08;
            draw_rectangle(
                at.x + inset.x,
                at.y + inset.y,
                t.x - 2.0 * inset.x,
                t.y - 2.0 * inset.y,
                Color::new(0.0, 0.0, 0.0, 0.35),
            );
            let icon = art.and_then(|(lbx, entry)| self.art.picture(&lbx, entry as usize, 0));
            match icon {
                Some(tex) => draw_texture_ex(
                    tex,
                    at.x + inset.x,
                    at.y + inset.y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(t - 2.0 * inset),
                        ..Default::default()
                    },
                ),
                None => draw_rectangle(
                    at.x + t.x * 0.25,
                    at.y + t.y * 0.25,
                    t.x * 0.5,
                    t.y * 0.5,
                    self.banner_color(owner),
                ),
            }
            draw_rectangle_lines(
                at.x + 1.0,
                at.y + 1.0,
                t.x - 2.0,
                t.y - 2.0,
                2.0,
                self.banner_color(owner),
            );
            if units.iter().any(|u| self.selection.contains(u)) && blink {
                draw_rectangle_lines(at.x, at.y, t.x, t.y, 3.0, GOLD);
            }
            if units.len() > 1 {
                draw_text(
                    units.len().to_string(),
                    at.x + t.x - 14.0,
                    at.y + t.y - 4.0,
                    18.0,
                    WHITE,
                );
            }
        }
    }

    fn draw_bars(&mut self) {
        let click = self.draw_frame();
        // Menu: Game, Spells, Armies, Cities, Magic, Info, Plane.
        match click.menu {
            Some(0) => self.open_slots(false),
            Some(1) | Some(4) => self.open_magic(),
            Some(3) => self.open_first_city(),
            Some(6) => {
                self.plane = if self.plane == Plane::Arcanus {
                    Plane::Myrror
                } else {
                    Plane::Arcanus
                };
            }
            Some(_) => self.log.push("Not in the game yet".into()),
            None => {}
        }
        if click.next_turn {
            self.end_turn();
            self.save_to(Path::new(AUTOSAVE));
        }

        // Information laid over the map: turn, selection, what is under the mouse, messages.
        let lay = crate::frame::Layout::new();
        let m = lay.rect(crate::frame::spot::MAP);
        let plane = if self.plane == Plane::Arcanus {
            "Arcanus"
        } else {
            "Myrror"
        };
        let size = (lay.scale * 4.2).max(14.0);
        let mut lines = Lines::new(m.x + 10.0, m.y + size + 6.0);
        lines.size = size;
        lines.line(
            &format!(
                "Turn {}   {}   {}",
                self.game.turn,
                plane,
                self.race_name(self.race)
            ),
            GOLD,
        );
        if !self.art.available() {
            lines.line("(no art: run extract-art)", RED);
        }
        self.draw_selection(&mut lines);
        self.draw_hover(&mut lines);
        // Messages along the bottom of the map.
        let mut y = m.y + m.h - 8.0;
        for line in self.log.iter().rev().take(5) {
            draw_text(
                line,
                m.x + 10.0,
                y,
                size * 0.8,
                Color::new(0.95, 0.9, 0.65, 1.0),
            );
            y -= size;
        }
        draw_text(
            "F1 keys",
            m.x + m.w - 8.0 * lay.scale * 2.2,
            m.y + m.h - 8.0,
            size * 0.7,
            LIGHTGRAY,
        );
        if self.show_keys || is_key_down(KeyCode::F1) {
            let keys = [
                "click select      right-click go      B settle / build road      U meld spirit",
                "C city      P plane-shift      F stop      T balance farmers      M magic",
                "Tab other plane      +/- zoom      arrows pan      Enter end turn",
                "Ctrl-Z undo      F5 quick-save      F9 quick-load      F8 Grand Vizier      N new map      R next race      Esc game menu",
            ];
            let box_w = m.w * 0.9;
            draw_rectangle(
                m.x + m.w * 0.05,
                m.y + m.h * 0.3,
                box_w,
                size * 5.5,
                Color::new(0.0, 0.0, 0.0, 0.8),
            );
            for (i, k) in keys.iter().enumerate() {
                draw_text(
                    k,
                    m.x + m.w * 0.06,
                    m.y + m.h * 0.3 + size * (1.3 + i as f32 * 1.1),
                    size * 0.75,
                    WHITE,
                );
            }
        }
    }

    fn draw_selection(&mut self, lines: &mut Lines) {
        if self.selection.is_empty() {
            lines.line("No units selected", GRAY);
            return;
        }
        lines.line("Selected stack", GOLD);
        for &unit in &self.selection {
            let Some(def) = self.game.unit_def(unit) else {
                continue;
            };
            let (hp, max) = self.game.unit_hits(unit);
            let moves = self.game.unit_moves_left(unit) as f32 / 2.0;
            let order = if self.game.world.get::<&f4x_core::units::Order>(unit).is_ok() {
                " ->"
            } else {
                ""
            };
            lines.line(
                &format!("{}  hp {hp}/{max}  mv {moves}{order}", def.name),
                WHITE,
            );
        }
    }

    fn draw_hover(&mut self, lines: &mut Lines) {
        let Some(pos) = self.tile_at(mouse_position().into()) else {
            return;
        };
        let content = &self.game.content;
        let Some(map) = self.game.map.as_ref() else {
            return;
        };
        let grid = map.plane(self.plane);
        if !self.game.fog.is_explored(HUMAN, self.plane, pos.x, pos.y) {
            lines.line("Unexplored", GRAY);
            return;
        }
        lines.line(&format!("Tile {}, {}", pos.x, pos.y), GOLD);
        if let Some(def) = grid
            .get(pos.x as i32, pos.y as i32)
            .and_then(|id| content.terrains.get(id))
        {
            lines.line(&format!("{}  (move {})", def.name, def.move_cost), WHITE);
        }
        if let Some(f) = grid
            .feature(pos.x as i32, pos.y as i32)
            .and_then(|f| content.features.get(f))
        {
            lines.line(&f.name, Color::new(0.6, 0.9, 1.0, 1.0));
        }
        if let Some(city) = self.game.city_at(pos) {
            let name = self.city_name(city);
            let pop = self
                .game
                .world
                .get::<&f4x_core::economy::Population>(city)
                .map_or(0, |p| p.figures);
            lines.line(&format!("{name} (size {pop})"), GOLD);
        }
        let others: Vec<_> = self
            .game
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.game.unit_owner(u) != Some(HUMAN))
            .collect();
        for u in others.into_iter().take(4) {
            if let Some(def) = self.game.unit_def(u) {
                lines.line(&format!("Foreign {}", def.name), RED);
            }
        }
    }
}

/// One log line for the treasure a cleared site gave.
fn loot_line(loot: &f4x_core::lairs::Loot) -> String {
    let mut parts = Vec::new();
    if loot.gold > 0 {
        parts.push(format!("{} gold", loot.gold));
    }
    if loot.mana > 0 {
        parts.push(format!("{} mana", loot.mana));
    }
    if !loot.items.is_empty() {
        parts.push(format!("{} magic item(s)", loot.items.len()));
    }
    if loot.prisoner {
        parts.push("a prisoner".to_string());
    }
    if loot.spell.is_some() {
        parts.push("a spell".to_string());
    }
    for book in &loot.books {
        parts.push(format!("a {book} book"));
    }
    for _ in &loot.retorts {
        parts.push("a retort".to_string());
    }
    if parts.is_empty() {
        "The site is cleared".to_string()
    } else {
        format!("The site is cleared: {}", parts.join(", "))
    }
}
