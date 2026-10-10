//! The overworld: map, units, fog, selection and orders. The city screen is in
//! `city_screen.rs`.

use std::collections::HashMap;
use std::path::Path;

use f4x_core::city::{PlayerId, Position};
use f4x_core::content::{Content, FeatureKind, TerrainClass};
use f4x_core::magic::Retort;
use f4x_core::map::{Grid, Plane};
use f4x_core::mapgen::{generate, GenParams};
use f4x_core::player::Player;
use f4x_core::wizard::WizardBuild;
use f4x_core::{Command, Event, Game};
use macroquad::prelude::*;

use crate::art::{terrain_type as tt, Art, TILE_H, TILE_W};
use crate::ui::{player_color, Lines, GOLD, PANEL};

pub(crate) const BAR_H: f32 = 28.0;
pub(crate) const SIDE_W: f32 = 300.0;
pub(crate) const HUMAN: PlayerId = PlayerId(0);
const RIVAL: PlayerId = PlayerId(1);

/// Where F5 / F9 save and load, and where each turn is saved automatically.
pub const QUICKSAVE: &str = "saves/quick.ron";
pub const AUTOSAVE: &str = "saves/autosave.ron";

pub enum Screen {
    Map,
    City(hecs::Entity),
    Magic,
    /// Creating the human wizard before the game begins.
    Create,
}

pub struct App {
    pub(crate) game: Game,
    pub(crate) art: Art,
    params: GenParams,
    pub(crate) seed: u64,
    plane: Plane,
    /// Top-left visible tile (fractional while panning).
    cam: Vec2,
    zoom: f32,
    /// Index into `content.rules.races` the human plays.
    pub(crate) race: usize,
    pub(crate) screen: Screen,
    selection: Vec<hecs::Entity>,
    pub(crate) log: Vec<String>,
    /// The production menu is open on the city screen.
    pub(crate) choosing: bool,
    pub(crate) choice_scroll: usize,
    /// Scroll position of the research and spell lists on the magic screen.
    pub(crate) magic_scroll: [usize; 2],
    /// A spell waiting for a target to be clicked on the map.
    pub(crate) targeting: Option<crate::magic_screen::Targeting>,
    /// The wizard creation screen's state.
    pub(crate) create: crate::create_screen::CreateState,
    /// The human wizard chosen on the creation screen; new maps reuse it.
    pub(crate) human_build: Option<f4x_core::wizard::WizardBuild>,
}

fn flat_color(class: TerrainClass) -> Color {
    match class {
        TerrainClass::Ocean => Color::new(0.1, 0.2, 0.55, 1.0),
        TerrainClass::Grassland => Color::new(0.25, 0.55, 0.2, 1.0),
        TerrainClass::Forest => Color::new(0.1, 0.35, 0.15, 1.0),
        TerrainClass::Hills => Color::new(0.5, 0.45, 0.25, 1.0),
        TerrainClass::Mountains => Color::new(0.5, 0.5, 0.5, 1.0),
        TerrainClass::Swamp => Color::new(0.25, 0.3, 0.2, 1.0),
        TerrainClass::Desert => Color::new(0.8, 0.7, 0.4, 1.0),
        TerrainClass::Tundra => Color::new(0.8, 0.85, 0.9, 1.0),
    }
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

/// Land neighbours of an ocean tile as the original's shore mask: NW 1, N 2, NE 4, E 8, SE 16,
/// S 32, SW 64, W 128. Off the poles counts as water.
fn shore_mask(grid: &Grid, content: &Content, x: i32, y: i32) -> usize {
    const AROUND: [(i32, i32); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
    ];
    AROUND
        .iter()
        .enumerate()
        .filter(|&(_, &(dx, dy))| {
            grid.get(x + dx, y + dy)
                .and_then(|t| content.terrains.get(t))
                .is_some_and(|d| d.class != TerrainClass::Ocean)
        })
        .fold(0, |mask, (bit, _)| mask | (1 << bit))
}

/// A river as blue strokes from the tile centre towards each river neighbour (and the sea).
fn draw_river(grid: &Grid, content: &Content, x: i32, y: i32, sx: f32, sy: f32, t: Vec2) {
    let blue = Color::new(0.2, 0.45, 0.85, 1.0);
    let (cx, cy) = (sx + t.x * 0.5, sy + t.y * 0.5);
    let w = (t.x * 0.14).max(2.0);
    draw_circle(cx, cy, w * 0.9, blue);
    for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
        let sea = grid
            .get(x + dx, y + dy)
            .and_then(|t| content.terrains.get(t))
            .is_some_and(|d| d.class == TerrainClass::Ocean);
        if sea || grid.has_river(x + dx, y + dy) {
            draw_line(
                cx,
                cy,
                cx + dx as f32 * t.x * 0.5,
                cy + dy as f32 * t.y * 0.5,
                w,
                blue,
            );
        }
    }
}

impl App {
    pub fn new(content: Content, art: Art, seed: u64) -> Result<Self, String> {
        let params = GenParams::default();
        let game = Self::new_game(seed, &params, &content, None)?;
        // Start as the first race that has city names, else the first race.
        let race = content
            .rules
            .races
            .defs()
            .iter()
            .position(|r| !r.city_names.is_empty())
            .unwrap_or(0);
        let mut app = Self {
            game,
            art,
            params,
            seed,
            plane: Plane::Arcanus,
            cam: Vec2::ZERO,
            zoom: 3.0,
            race,
            screen: Screen::Map,
            selection: Vec::new(),
            log: Vec::new(),
            choosing: false,
            choice_scroll: 0,
            magic_scroll: [0, 0],
            targeting: None,
            create: crate::create_screen::CreateState::empty(),
            human_build: None,
        };
        app.start_game();
        Ok(app)
    }

    fn new_game(
        seed: u64,
        params: &GenParams,
        content: &Content,
        human: Option<&WizardBuild>,
    ) -> Result<Game, String> {
        let map = generate(seed, params, content).map_err(|e| e.to_string())?;
        let mut game = Game::new(seed);
        game.map = Some(map);
        game.content = content.clone();
        game.players.push(Player::new(HUMAN, "You"));
        game.players.push(Player::new(RIVAL, "Rival"));
        // Without a creation screen's choice, fixed picks: five books each.
        let fixed = |books: &[(&str, u8)], retort| WizardBuild {
            name: String::new(),
            books: books.iter().map(|&(r, n)| (r.to_string(), n)).collect(),
            retorts: [retort].into_iter().collect(),
            spells: Vec::new(),
            preset: true,
        };
        let human_build = human
            .cloned()
            .unwrap_or_else(|| fixed(&[("sorcery", 3), ("nature", 2)], Retort::Conjurer));
        // The rival is one of the ready-made wizards other than the human's, by seed.
        let rival_build = {
            let defs = content.rules.wizards.defs();
            let others: Vec<&str> = defs
                .iter()
                .filter(|w| w.name != human_build.name)
                .map(|w| w.id.as_str())
                .collect();
            others
                .get(seed as usize % others.len().max(1))
                .and_then(|id| game.preset_build(id))
                .unwrap_or_else(|| fixed(&[("chaos", 3), ("death", 2)], Retort::Warlord))
        };
        for (player, build) in [(HUMAN, human_build), (RIVAL, rival_build)] {
            game.create_wizard(player, &build)
                .map_err(|e| format!("{e:?}"))?;
        }
        Ok(game)
    }

    /// Found the first city near the middle and hand out a small starting force.
    fn start_game(&mut self) {
        let (cx, cy) = (self.params.width / 2, self.params.height / 2);
        let mut spots: Vec<(u32, u32)> = (0..self.params.height)
            .flat_map(|y| (0..self.params.width).map(move |x| (x, y)))
            .collect();
        spots.sort_by_key(|&(x, y)| x.abs_diff(cx).pow(2) + y.abs_diff(cy).pow(2));
        for (x, y) in spots {
            let pos = Position {
                plane: Plane::Arcanus,
                x,
                y,
            };
            if self.game.can_found_city(pos).is_ok() && self.found_city(pos, 3, true) {
                self.give_starting_units(pos);
                self.center_on(pos);
                return;
            }
        }
    }

    fn give_starting_units(&mut self, at: Position) {
        let rules = &self.game.content.rules;
        let race = &rules.races.defs()[self.race];
        let (mut army, mut settlers) = (Vec::new(), None);
        for name in &race.units {
            let Some(id) = rules.units.id_of(name) else {
                continue;
            };
            let def = rules.units.get(id);
            if def.abilities.iter().any(|a| a == "create_outpost") {
                settlers.get_or_insert(id);
            } else if army.len() < 2 && def.cost > 0 {
                army.push(id);
            }
        }
        for unit in army.into_iter().chain(settlers) {
            self.game.apply(Command::CreateUnit {
                owner: HUMAN,
                pos: at,
                unit,
            });
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

    /// Found a city; the first one holds the wizard's fortress.
    fn found_city(&mut self, pos: Position, population: u32, fortress: bool) -> bool {
        let name = self.next_city_name();
        let rules = &self.game.content.rules;
        let race = rules.races.id_of(&rules.races.defs()[self.race].id);
        let cmd = if fortress {
            Command::PlaceFortress {
                player: HUMAN,
                pos,
                name,
                race,
                population,
            }
        } else {
            Command::FoundCity {
                owner: HUMAN,
                pos,
                name,
                race,
                population,
            }
        };
        let events = self.game.apply(cmd);
        self.note(events)
    }

    /// Turn events into log lines; returns whether a city was founded.
    pub(crate) fn note(&mut self, events: Vec<Event>) -> bool {
        let mut founded = false;
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
                Event::CityGrew { city, figures } => {
                    Some(format!("{} grows to {figures}", self.city_name(city)))
                }
                Event::CityShrank { city, figures } => {
                    Some(format!("{} shrinks to {figures}", self.city_name(city)))
                }
                Event::UnitsDisbanded { units, .. } => {
                    Some(format!("{units} unit(s) disbanded: unpaid or unfed"))
                }
                Event::AttackRequested { .. } => Some("Combat is not implemented yet".to_string()),
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
                Event::VolcanoRaised { .. } => Some("A volcano rises".to_string()),
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

    fn map_rect(&self) -> Rect {
        Rect::new(
            0.0,
            BAR_H,
            screen_width() - SIDE_W,
            screen_height() - 2.0 * BAR_H,
        )
    }

    fn visible_tiles(&self) -> (f32, f32) {
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
        match Self::new_game(seed, &self.params, &content, self.human_build.as_ref()) {
            Ok(game) => {
                self.game = game;
                self.seed = seed;
                self.screen = Screen::Map;
                self.selection.clear();
                self.log.clear();
                self.start_game();
                self.log.push(format!("New map, seed {seed}"));
            }
            Err(e) => self.log.push(format!("Map generation failed: {e}")),
        }
    }

    /// Handle input and draw one frame. Returns false when the player quits.
    pub fn frame(&mut self) -> bool {
        self.selection
            .retain(|&u| self.game.unit_position(u).is_some());
        match self.screen {
            Screen::City(city) => {
                self.city_screen(city);
            }
            Screen::Magic => self.magic_screen(),
            Screen::Create => self.create_screen(),
            Screen::Map => {
                if is_key_pressed(KeyCode::Escape) {
                    if self.targeting.take().is_some() {
                        self.log.push("Cast cancelled".into());
                    } else {
                        return false;
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
        if is_key_pressed(KeyCode::F5) {
            self.save_to(Path::new(QUICKSAVE));
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
        let plane_offset = if self.plane == Plane::Myrror {
            tt::MYRROR_OFFSET
        } else {
            0
        };
        let width = grid.width();
        let visible = self.game.visibility(HUMAN, self.plane);
        let content = &self.game.content;
        let cities = self.game.cities();

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
                let Some(def) = grid
                    .get(wx as i32, y)
                    .and_then(|id| content.terrains.get(id))
                else {
                    continue;
                };
                let feature = grid
                    .feature(wx as i32, y)
                    .and_then(|f| content.features.get(f));
                let mut type_no = base_type(def.class);
                if def.class == TerrainClass::Ocean {
                    type_no = self.art.shore_type(shore_mask(grid, content, wx as i32, y));
                }
                if let Some(f) = feature.filter(|f| f.kind == FeatureKind::Node) {
                    type_no = match f.id.as_str() {
                        "sorcery_node" => tt::SORCERY_NODE,
                        "nature_node" => tt::NATURE_NODE,
                        _ => tt::CHAOS_NODE,
                    };
                }
                match self.art.terrain(type_no + plane_offset) {
                    Some(tex) => draw_texture_ex(
                        tex,
                        sx,
                        sy,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(t),
                            ..Default::default()
                        },
                    ),
                    None => draw_rectangle(sx, sy, t.x, t.y, flat_color(def.class)),
                }
                if let Some(owner) = grid.volcano(wx as i32, y) {
                    let c = player_color(owner);
                    draw_triangle(
                        vec2(sx + t.x * 0.5, sy + t.y * 0.15),
                        vec2(sx + t.x * 0.2, sy + t.y * 0.8),
                        vec2(sx + t.x * 0.8, sy + t.y * 0.8),
                        Color::new(0.85, 0.2, 0.05, 1.0),
                    );
                    draw_circle(sx + t.x * 0.5, sy + t.y * 0.2, t.x * 0.1, c);
                }
                if grid.has_river(wx as i32, y) {
                    draw_river(grid, content, wx as i32, y, sx, sy, t);
                }
                if grid.has_road(wx as i32, y) {
                    draw_rectangle(
                        sx + t.x * 0.4,
                        sy + t.y * 0.4,
                        t.x * 0.2,
                        t.y * 0.2,
                        Color::new(0.45, 0.3, 0.15, 1.0),
                    );
                }
                if let Some(f) = feature {
                    draw_feature_marker(f.kind, sx, sy, t);
                }
                if !visible[(y as u32 * width + wx) as usize] {
                    draw_rectangle(sx, sy, t.x, t.y, Color::new(0.0, 0.0, 0.0, 0.5));
                }
            }
        }
        for (_, city, pos) in &cities {
            if pos.plane != self.plane
                || !self.game.fog.is_explored(HUMAN, self.plane, pos.x, pos.y)
            {
                continue;
            }
            let at = screen_of(pos.x, pos.y);
            draw_city_marker(at.x, at.y, t, player_color(city.owner.0));
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
            let size = 14.0 + self.zoom * 2.0;
            draw_text(&city.name, at.x - 2.0, at.y - 3.0, size, BLACK);
            draw_text(&city.name, at.x - 3.0, at.y - 4.0, size, WHITE);
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
            if !mine && !visible[(at.y * width + at.x) as usize] {
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
                    player_color(owner),
                ),
            }
            draw_rectangle_lines(
                at.x + 1.0,
                at.y + 1.0,
                t.x - 2.0,
                t.y - 2.0,
                2.0,
                player_color(owner),
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
        let w = screen_width();
        let h = screen_height();
        let treasury = self.game.treasury(HUMAN);
        let gold = self.game.player(HUMAN).map_or(0, |p| p.gold);
        draw_rectangle(0.0, 0.0, w, BAR_H, PANEL);
        let plane = if self.plane == Plane::Arcanus {
            "Arcanus"
        } else {
            "Myrror"
        };
        draw_text(
            format!(
                "Turn {}   {}   Gold {} ({:+})   Mana {} ({:+})   Food +{} (army {})   zoom {}x   seed {}",
                self.game.turn,
                plane,
                gold,
                treasury.net_gold(),
                self.game.player(HUMAN).map_or(0, |p| p.mana),
                self.game.power_split(HUMAN).mana - self.game.mana_upkeep(HUMAN),
                treasury.food_surplus,
                treasury.food_upkeep,
                self.zoom,
                self.seed
            ),
            10.0,
            19.0,
            22.0,
            GOLD,
        );
        draw_rectangle(0.0, h - BAR_H, w, BAR_H, PANEL);
        draw_text(
            "click select  right-click go  B settle  C city  P plane-shift  F stop  T farmers  M magic  Tab view  Enter end turn  F5 save  F9 load  N map  R race  Esc",
            10.0,
            h - 9.0,
            18.0,
            LIGHTGRAY,
        );

        // Side panel.
        let px = w - SIDE_W;
        draw_rectangle(px, BAR_H, SIDE_W, h - 2.0 * BAR_H, PANEL);
        let mut lines = Lines::new(px + 12.0, BAR_H + 24.0);
        lines.line(&format!("You: {}", self.race_name(self.race)), GOLD);
        lines.line(
            &format!(
                "Cities {}   Units {}",
                self.game
                    .cities()
                    .iter()
                    .filter(|(_, c, _)| c.owner == HUMAN)
                    .count(),
                self.game.player_units(HUMAN).len()
            ),
            WHITE,
        );
        if !self.art.available() {
            lines.line("(no art: run extract-art)", RED);
        }
        lines.gap(6.0);
        self.draw_selection(&mut lines);
        lines.gap(6.0);
        self.draw_hover(&mut lines);
        // Message log at the bottom.
        let mut y = h - BAR_H - 10.0;
        for line in self.log.iter().rev().take(7) {
            draw_text(line, px + 12.0, y, 17.0, Color::new(0.9, 0.85, 0.6, 1.0));
            y -= 19.0;
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

fn draw_feature_marker(kind: FeatureKind, sx: f32, sy: f32, t: Vec2) {
    let (cx, cy) = (sx + t.x / 2.0, sy + t.y / 2.0);
    match kind {
        FeatureKind::Mineral => draw_circle(cx, cy, t.y * 0.18, GOLD),
        FeatureKind::Encounter => {
            draw_poly(cx, cy, 4, t.y * 0.3, 45.0, Color::new(0.85, 0.2, 0.2, 1.0))
        }
        FeatureKind::Tower => draw_rectangle(
            cx - t.x * 0.18,
            sy + t.y * 0.1,
            t.x * 0.36,
            t.y * 0.8,
            LIGHTGRAY,
        ),
        FeatureKind::Node => {}
    }
}

fn draw_city_marker(sx: f32, sy: f32, t: Vec2, flag: Color) {
    let (w, h) = (t.x * 0.8, t.y * 0.6);
    let (x, y) = (sx + t.x * 0.1, sy + t.y * 0.3);
    draw_rectangle(x, y, w, h, Color::new(0.75, 0.6, 0.4, 1.0));
    draw_triangle(
        vec2(x - 2.0, y),
        vec2(x + w + 2.0, y),
        vec2(x + w / 2.0, y - t.y * 0.3),
        Color::new(0.7, 0.2, 0.15, 1.0),
    );
    draw_rectangle_lines(x, y, w, h, 1.5, BLACK);
    draw_rectangle(x + w - 3.0, y - t.y * 0.45, 3.0, t.y * 0.45, BLACK);
    draw_rectangle(x + w, y - t.y * 0.45, t.x * 0.2, t.y * 0.2, flag);
}
