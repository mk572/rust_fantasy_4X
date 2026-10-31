//! The overworld: map, units, fog, selection and orders. The city screen is in
//! `city_screen.rs`.

use std::collections::HashMap;
use std::path::Path;

use f4x_core::content::Content;
use f4x_core::game::setup::{self, NewGame};
use f4x_core::map::mapgen::GenParams;
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;
use f4x_core::text::Args;

use crate::font::Style;
use crate::input::Input;
use f4x_core::game::{Command, Event, Game};
use f4x_core::wizard::WizardBuild;
use macroquad::prelude::*;

use crate::art::{Art, TILE_H, TILE_W};
use crate::frame::spot;
use crate::modal::{keep, Modal};
use crate::options_screen::OptionsState;
use crate::preferences::Pref;
use crate::ui::{self, GOLD};

pub(crate) const HUMAN: PlayerId = PlayerId(0);

/// Where F10 / Ctrl-L save and load, and where each turn is saved automatically.
pub const QUICKSAVE: &str = "saves/quick.ron";
pub const AUTOSAVE: &str = "saves/autosave.ron";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Map,
    City(hecs::Entity),
    Magic,
    /// One wizard's sheet (the original's Mirror).
    Wizard,
    /// Talking to another wizard.
    Diplomacy,
    /// The spell book the wizard casts from.
    Spellbook,
    /// The Apprentice's library: every spell the wizard can learn.
    Apprentice,
    /// Choosing what to research next.
    Research,
    /// A tactical battle is being fought.
    Battle,
    /// Creating the human wizard before the game begins.
    Create,
    /// The new-game options (difficulty, opponents, land size, magic power).
    Options,
    /// The player's preferences: the original's Settings screen, opened from the save slots.
    Settings,
    /// The settings of the game being played (read-only), or the options for the next game when
    /// there is none; opened from the Settings screen's Game button.
    GameSettings,
    /// The world map of one plane, zoomed out (F2).
    Cartographer,
    /// "History Of Wizards Power" (F4).
    History,
    /// "Current Status Of Wizards" (F5).
    Astrologer,
    /// The game has ended: result and score.
    GameOver,
    /// The text console (backquote key).
    Console,
    /// The start menu: Continue, Load Game, New Game, Hall of Fame, Quit.
    MainMenu,
    /// The original's opening movie (`intro_screen.rs`).
    Intro,
    /// The save slots: load, save, and leave the game.
    Slots,
    /// One unit's statistics sheet (`unit_screen.rs`).
    UnitSheet,
    /// The list of the wizard's stacks (`army_screen.rs`).
    ArmyList,
    /// The list of the wizard's cities (`city_list_screen.rs`).
    CityList,
    /// An outpost (`outpost_screen.rs`).
    Outpost,
    /// Another wizard's city (`enemy_city_screen.rs`).
    EnemyCity,
    /// The table of best games (`hall_of_fame.rs`).
    HallOfFame,
    /// The heroes' items and the vault (`item_screen.rs`).
    Items,
    /// Designing an item for Enchant Item or Create Artifact (`craft_screen.rs`).
    Craft,
}

pub struct App {
    pub(crate) game: Game,
    pub(crate) art: Art,
    params: GenParams,
    /// The new-game options chosen before the wizard is made.
    pub(crate) options: OptionsState,
    /// The player's preferences (the Settings screen), shared by every game.
    pub(crate) prefs: crate::preferences::Preferences,
    /// The sounds and the music (`audio.rs`).
    pub(crate) audio: crate::audio::Audio,
    /// The levels the human's heroes had at the last turn.
    pub(crate) hero_levels: std::collections::HashMap<hecs::Entity, usize>,
    /// The turn the heroes' levels were last looked at (`u32::MAX`: not yet).
    pub(crate) levels_turn: u32,
    /// The news of the last turn change, for the Chancellor to show again.
    pub(crate) last_news: Vec<crate::chancellor::Section>,
    pub(crate) seed: u64,
    /// The seed was chosen with `--seed`: every new game starts from it.
    pub(crate) seed_fixed: bool,
    pub(crate) plane: Plane,
    /// Top-left visible tile (fractional while panning).
    pub(crate) cam: Vec2,
    zoom: f32,
    /// Index into `content.rules.races` the human plays.
    pub(crate) race: usize,
    pub(crate) screen: Screen,
    pub(crate) selection: Vec<hecs::Entity>,
    /// Which nine units of a crowded square the unit window shows: the units from `9 * page` on.
    pub(crate) stack_page: usize,
    /// The first row of the city screen's garrison list that is shown (five units to a row).
    pub(crate) garrison_row: usize,
    pub(crate) log: Vec<String>,
    /// The production menu is open on the city screen.
    pub(crate) choosing: bool,
    /// The question or refusal when a building of the city picture is clicked.
    /// The diplomacy screen's talk.
    /// Wizards met whose greeting waits for the map: the screen opens for each in turn.
    pub(crate) greetings: std::collections::VecDeque<PlayerId>,
    /// Wizards met since the turn began: their greeting comes with the next turn.
    pub(crate) greetings_due: Vec<PlayerId>,
    /// A Summon Hero or Summon Champion cast whose scene waits for the hero it brought (its
    /// gender picks the figure): the caster and the spell.
    pub(crate) hero_summon_due: Option<(PlayerId, f4x_core::rules::SpellId)>,
    /// The box that shows the offers made at the turn change.
    pub(crate) offer_ui: crate::offer_prompt::OfferUi,
    /// The rival wizards, who play their turns when the human ends theirs.
    pub(crate) ais: Vec<f4x_ai::Ai>,
    /// Attacks on the human this turn, waiting to be fought one after the other.
    pub(crate) defences: std::collections::VecDeque<crate::attack_prompt::PendingAttack>,
    /// The turn is half over: the rivals have moved and the human is defending before it ends.
    pub(crate) turn_events: Option<Vec<Event>>,
    /// What the small spell book keeps between openings.
    pub(crate) book_prefs: crate::spellbook::BookPrefs,
    /// A spell to research once the research under way is done (set from the spell book).
    pub(crate) queued_research: Option<f4x_core::rules::SpellId>,
    /// The help topics (local only), which describe the spells.
    pub(crate) help: f4x_core::help::Help,
    /// The messages of the random events (`event_texts.ron`).
    pub(crate) event_texts: Vec<f4x_core::world_event::EventText>,
    /// The original's lines of the wizards' talk by situation (`diplomsg.ron`, local only).
    pub(crate) diplomsg: std::collections::HashMap<u16, Vec<String>>,
    /// Heroes died and their items were awarded: the warning box comes before the item screen.
    pub(crate) item_pool_warning: bool,
    /// The full-screen scenes waiting to be played, oldest first (`end_scene.rs`), and whether the
    /// scene of the game's end is among them already.
    pub(crate) end_scenes: std::collections::VecDeque<crate::end_scene::Scene>,
    pub(crate) end_scene_queued: bool,
    /// The building just announced, which the city screen shows fading in (`built_popup.rs`).
    pub(crate) build_in: Option<crate::built_popup::BuildIn>,
    /// A spell waiting for a target to be clicked on the map.
    pub(crate) targeting: Option<crate::spellbook::Targeting>,
    /// The turn is ending: 2 until one frame with the hourglass has been drawn, then 1 while
    /// the computer plays (`pointer.rs`).
    pub(crate) turn_pending: u8,
    /// Whether a unit of the human waited for orders in the last frame; with End of Turn Wait
    /// off, the turn ends when the last one is given orders.
    pub(crate) units_waited: bool,
    /// Whether the selected stack can walk to the square under the pointer, for the boot.
    pub(crate) pointer_walk: Option<((Position, Vec<hecs::Entity>), bool)>,
    /// A road being planned.
    pub(crate) road_plan: Option<crate::road_plan::RoadPlan>,
    /// The Surveyor is describing the square under the mouse (F1).
    pub(crate) surveyor: bool,
    /// For screenshots: the square the Surveyor describes instead of the one under the mouse.
    pub(crate) survey_probe: Option<Position>,
    /// The human wizard chosen on the creation screen; new maps reuse it.
    pub(crate) human_build: Option<f4x_core::wizard::WizardBuild>,
    /// The battle screen's state.
    /// Steps the units on the map are still sliding through.
    pub(crate) slides: Vec<crate::map_slide::Slide>,
    /// Items have just been awarded to the human wizard: the item screen opens when the boxes
    /// before it are closed (`Process_Item_Pool`: a lair's treasure, a gift, the merchant's item
    /// bought, an item made).
    pub(crate) item_pool_pending: bool,
    /// The test battle is fought at sea (`--battle-field sea`).
    pub(crate) test_sea: bool,
    /// The screens converted to [`Page`](crate::page::Page): one page per kind of screen, kept
    /// while another screen (a unit sheet, the console) is up over it.
    pub(crate) pages: crate::page::Pages,
    /// The player chose Quit on the start menu; the frame returns false.
    pub(crate) quit: bool,
    /// Where the selected units' lines were drawn last frame: a right-click on one opens the
    /// unit's sheet instead of sending the stack anywhere.
    /// The player has seen the game-over screen and chose to look at the map.
    pub(crate) game_over_seen: bool,
    pub(crate) menu: crate::menu_screen::MenuState,
    pub(crate) help_ui: crate::help_popup::HelpUi,
    /// The keys of the overland map (`content/hotkeys.ron`).
    pub(crate) hotkeys: crate::hotkeys::Hotkeys,
    /// The text console's state (backquote key).
    pub(crate) console: crate::console_screen::ConsoleUi,
    /// The scroll that tells how a battle went, once it is over.
    pub(crate) combat_results: Option<crate::combat_result::CombatResults>,
    /// The battle last begun, for the scroll's paper.
    pub(crate) battle_site: Option<crate::combat_result::BattleSite>,
    /// The questions and notices over the screen below (`modal.rs`).
    pub(crate) modals: crate::modal::ModalStack,
    /// The screen is being drawn under a modal: a battle only shows itself and does not go on.
    pub(crate) under_modal: bool,
}

/// A seed for a map nobody has asked a number for: the clock, mixed.
pub fn fresh_seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    // Mix it so that two games a moment apart do not get near numbers; kept to six digits.
    (nanos ^ (nanos >> 29)).wrapping_mul(0x9E37_79B9_7F4A_7C15) % 1_000_000
}

/// How dark the screen below a modal gets (`Pref::DimBehindBoxes`).
const DIM: f32 = 0.45;

/// The fortress has fallen: resign, or cast the Spell of Return.
pub(crate) struct Resign;

impl Modal for Resign {
    fn kind(&self) -> &'static str {
        "resign"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.resign_prompt_frame(input))
    }
}

/// Which realm a Spell Ward guards a city against.
pub(crate) struct Ward {
    pub(crate) spell: f4x_core::rules::SpellId,
    pub(crate) city: hecs::Entity,
    pub(crate) resolve: bool,
}

impl Modal for Ward {
    fn kind(&self) -> &'static str {
        "ward"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.ward_prompt_frame(*self, input))
    }
}

/// The cast has nothing to aim at: keep the spell or give it up.
pub(crate) struct NoTarget(pub(crate) f4x_core::rules::SpellId);

impl Modal for NoTarget {
    fn kind(&self) -> &'static str {
        "no_target"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.no_target_frame(*self, input))
    }
}

/// The rival's global enchantment to dispel, the row picked at `row`.
pub(crate) struct Global {
    pub(crate) spell: f4x_core::rules::SpellId,
    pub(crate) resolve: bool,
    pub(crate) row: usize,
}

impl Modal for Global {
    fn kind(&self) -> &'static str {
        "global"
    }
    fn frame(self: Box<Self>, app: &mut App, input: &Input) -> Option<Box<dyn Modal>> {
        keep(app.global_prompt_frame(*self, input))
    }
}

impl App {
    /// The name of a retort.
    pub(crate) fn retort_name(&self, retort: f4x_core::wizard::magic::Retort) -> String {
        self.text(f4x_core::wizard::retort_key(retort))
    }

    /// All the messages, for the code that picks among several.
    pub(crate) fn texts(&self) -> &f4x_core::text::Texts {
        &self.game.content().texts
    }

    /// A message without values (`content/text/`).
    pub(crate) fn text(&self, key: &str) -> String {
        self.text_with(key, &Args::new())
    }

    /// A message with its values put in.
    pub(crate) fn text_with(&self, key: &str, args: &Args) -> String {
        self.game.content().texts.get(key, args)
    }

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
        let options = OptionsState::from_settings(&content.settings);
        let prefs = crate::preferences::Preferences::from_defaults(&content.settings.preferences)?;
        let game = Self::new_game(seed, &options, &content, None, race, true)?;
        let mut app = Self {
            game,
            art,
            params,
            options,
            prefs,
            audio: crate::audio::Audio::silent(),
            hero_levels: Default::default(),
            levels_turn: u32::MAX,
            last_news: Vec::new(),
            seed,
            seed_fixed: false,
            plane: Plane::Arcanus,
            cam: Vec2::ZERO,
            zoom: 4.0,
            race,
            screen: Screen::Map,
            selection: Vec::new(),
            stack_page: 0,
            garrison_row: 0,
            log: Vec::new(),
            choosing: false,
            greetings: Default::default(),
            greetings_due: Vec::new(),
            hero_summon_due: None,
            offer_ui: Default::default(),
            ais: Vec::new(),
            defences: Default::default(),
            turn_events: None,
            book_prefs: Default::default(),
            queued_research: None,
            help: Default::default(),
            event_texts: Vec::new(),
            diplomsg: Default::default(),
            item_pool_warning: false,
            build_in: None,
            end_scenes: Default::default(),
            end_scene_queued: false,
            targeting: None,
            turn_pending: 0,
            units_waited: false,
            pointer_walk: None,
            road_plan: None,
            surveyor: false,
            survey_probe: None,
            human_build: None,
            slides: Vec::new(),
            test_sea: false,
            item_pool_pending: false,
            pages: Default::default(),
            quit: false,
            game_over_seen: false,
            menu: Default::default(),
            help_ui: Default::default(),
            hotkeys: Default::default(),
            console: Default::default(),
            combat_results: None,
            battle_site: None,
            modals: Default::default(),
            under_modal: false,
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
        random_events: bool,
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
            preset,
            build: human.cloned(),
            race: content.rules.races.defs().get(race).map(|r| r.id.clone()),
            settings: {
                let mut s = f4x_core::game::setting::GameSettings::for_difficulty(
                    &content.settings,
                    options.difficulty,
                );
                s.basic.opponents = options.opponents.clamp(1, content.settings.max_opponents);
                s.basic.land_percent = options.land_percent(&content.settings);
                s.basic.magic_strength = options.magic;
                (s.basic.map_width, s.basic.map_height) = options.map_size(&content.settings);
                s.basic.original_map_generator = options.original_generator;
                // The Random Events preference is the default for a new game; a level that
                // plays without events (Intro) keeps them off.
                s.events.random_events &= random_events;
                s
            },
            ..NewGame::default()
        };
        setup::new_game(content, &spec)
    }

    /// Look at the human wizard's fortress, where the game starts.
    fn start_game(&mut self) {
        self.ais = self.rival_wizards();
        self.defences.clear();
        self.turn_events = None;
        let fortress = self
            .game
            .economy()
            .cities()
            .into_iter()
            .find(|(_, c, _)| c.owner == HUMAN)
            .map(|(_, _, pos)| pos);
        if let Some(pos) = fortress {
            self.center_on(pos);
        }
    }

    /// One computer player for each rival wizard; the human fights battles against them in
    /// person.
    fn rival_wizards(&self) -> Vec<f4x_ai::Ai> {
        self.game
            .player_ids()
            .into_iter()
            .filter(|id| *id != HUMAN && !self.game.is_neutral_player(*id))
            .map(|id| {
                let mut ai = f4x_ai::Ai::new(id);
                ai.leave_defence_to(&[HUMAN]);
                ai
            })
            .collect()
    }

    pub(crate) fn center_on(&mut self, pos: Position) {
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
            .wizard(PlayerId(owner))
            .map_or(f4x_core::player::NEUTRAL_BANNER, |w| w.banner());
        self.art.banner_color(banner)
    }

    /// The banner number of a player (by number; 255 is the neutral player).
    pub(crate) fn banner_index(&self, owner: u8) -> u8 {
        self.game
            .wizard(PlayerId(owner))
            .map_or(f4x_core::player::NEUTRAL_BANNER, |w| w.banner())
    }

    fn next_city_name(&self) -> String {
        let race = &self.game.content().rules.races.defs()[self.race];
        let used = self.game.economy().cities().len();
        race.city_names
            .get(used % race.city_names.len().max(1))
            .cloned()
            .unwrap_or_else(|| self.text_with("log.city_number", &Args::new().count("n", used + 1)))
    }

    /// Turn events into log lines; returns whether a city was founded.
    pub(crate) fn note(&mut self, events: Vec<Event>) -> bool {
        let mut founded = false;
        let mut dispelled = 0;
        for event in events {
            self.start_slide(&event);
            if let Event::SpellCast { player, spell } = &event {
                if *player == HUMAN || self.prefs.get(crate::preferences::Pref::EnemySpells) {
                    let id = &self.game.content().rules.spells.get(*spell).id;
                    let key = self
                        .audio
                        .table
                        .spells
                        .get(id)
                        .and_then(|&n| crate::audio::spell_sound(n));
                    self.audio.effect(key);
                }
            }
            if let Event::SpellCastAt { player, spell, at } = &event {
                self.announce_city_spell(*player, *spell, *at);
                self.announce_square_spell(*player, *spell, *at);
            }
            if let Event::SpellCastOnWizard {
                player,
                spell,
                victim,
            } = &event
            {
                self.announce_wizard_spell(*player, *spell, *victim);
            }
            match &event {
                Event::WizardBanished { player, by } => {
                    self.queue_conquest_scene(*player, Some(*by), true)
                }
                Event::WizardDefeated { player } => {
                    let by = self.game.player(*player).and_then(|p| p.defeated_by());
                    self.queue_conquest_scene(*player, by, false);
                }
                _ => {}
            }
            let line = match event {
                Event::EnchantmentDispelled {
                    victim,
                    spell,
                    site,
                } => {
                    if victim == HUMAN {
                        self.announce_dispelled(spell, site, &mut dispelled);
                    }
                    None
                }
                Event::CityFounded { city, .. } => {
                    founded = true;
                    Some(self.text_with(
                        "log.founded",
                        &Args::new().text("city", self.city_name(city)),
                    ))
                }
                Event::Rejected(why) => Some(self.text(why.key())),
                Event::ItemMade { player, .. } => {
                    self.item_pool_pending |= player == HUMAN;
                    None
                }
                Event::ItemsAwarded {
                    player,
                    items,
                    lost,
                } => {
                    if player == HUMAN {
                        self.item_pool_pending = true;
                        self.item_pool_warning = true;
                    }
                    let args = Args::new().list("items", items).count("lost", lost);
                    Some(self.text_with("log.items_awarded", &args))
                }
                Event::RandomEvent(notice) => {
                    if notice.kind == f4x_core::world_event::EventKind::Gift
                        && notice.voice == f4x_core::world_event::Voice::Started
                    {
                        self.item_pool_pending = true;
                    }
                    let line = f4x_core::world_event::compose(
                        &self.event_texts,
                        &self.game.content().texts,
                        &notice,
                    );
                    self.modals.enqueue(crate::event_popup::EventBox(notice));
                    line
                }
                Event::ProductionCompleted { city, item } => {
                    if let f4x_core::city::production::ProductionItem::Building(b) = item {
                        self.announce_built(city, b);
                    }
                    let args = Args::new()
                        .text("city", self.city_name(city))
                        .text("item", self.item_name(item));
                    Some(self.text_with("log.production_complete", &args))
                }
                Event::ProductionBlocked { city, item } => {
                    let args = Args::new()
                        .text("city", self.city_name(city))
                        .text("item", self.item_name(item));
                    Some(self.text_with("log.production_blocked", &args))
                }
                Event::ProductionImpossible { city, item } => {
                    let args = Args::new()
                        .text("city", self.city_name(city))
                        .text("item", self.item_name(item));
                    Some(self.text_with("log.production_impossible", &args))
                }
                Event::ChaosRiftStrikes {
                    city,
                    units_lost,
                    building,
                } => {
                    let building =
                        building.map(|b| self.game.content().rules.buildings.get(b).name.clone());
                    let args = Args::new()
                        .text("city", self.city_name(city))
                        .count("units", units_lost)
                        .text("building", building.clone().unwrap_or_default());
                    Some(self.text_with(
                        match (units_lost > 0, building.is_some()) {
                            (false, false) => "log.rift",
                            (true, false) => "log.rift_units",
                            (false, true) => "log.rift_building",
                            (true, true) => "log.rift_units_building",
                        },
                        &args,
                    ))
                }
                Event::CityGrew { city, figures } => {
                    let args = Args::new()
                        .text("city", self.city_name(city))
                        .count("figures", figures);
                    Some(self.text_with("log.city_grew", &args))
                }
                Event::CityShrank { city, figures } => {
                    let args = Args::new()
                        .text("city", self.city_name(city))
                        .count("figures", figures);
                    Some(self.text_with("log.city_shrank", &args))
                }
                Event::UnitsDisbanded { units, .. } => {
                    Some(self.text_with("log.units_disbanded", &Args::new().count("n", units)))
                }
                Event::AttackRequested {
                    attackers,
                    defenders,
                    ..
                } => {
                    if attackers
                        .first()
                        .is_some_and(|&u| self.game.units().unit_owner(u) == Some(HUMAN))
                    {
                        // Asked first: fight, leave it to the computer, or call it off.
                        self.modals.open(crate::attack_prompt::AttackAsk::new(
                            crate::attack_prompt::PendingAttack {
                                attackers,
                                defenders,
                                defending: false,
                                treaty_asked: false,
                            },
                        ));
                    }
                    None
                }
                Event::SiteEncounter { units, to, .. } => {
                    if units
                        .first()
                        .is_some_and(|&u| self.game.units().unit_owner(u) == Some(HUMAN))
                    {
                        self.site_encounter(units, to);
                    }
                    None
                }
                Event::SiteCleared {
                    player,
                    site,
                    guarded,
                    loot,
                    ..
                } => {
                    if player == HUMAN {
                        self.site_cleared(&site, guarded, &loot);
                    }
                    Some(self.loot_line(&loot))
                }
                Event::CityLooted {
                    city,
                    gold,
                    citizens_lost,
                    buildings_lost,
                    ..
                } => {
                    let mine = self.game.economy().city_owner(city) == Some(HUMAN);
                    let args = Args::new()
                        .text("city", self.city_name(city))
                        .count("gold", gold)
                        .count("citizens", citizens_lost)
                        .count("buildings", buildings_lost.len());
                    Some(self.text_with(
                        if mine {
                            "log.city_looted_yours"
                        } else {
                            "log.city_looted"
                        },
                        &args,
                    ))
                }
                Event::BattleStarted {
                    attacker,
                    defender,
                    site,
                } => {
                    self.note_battle_start(attacker, defender, site);
                    None
                }
                Event::CityRazed { gold, .. } => {
                    Some(self.text_with("log.city_razed", &Args::new().count("gold", gold)))
                }
                Event::CityDestroyed { .. } => Some(self.text("log.city_destroyed")),
                // The raiders and the monsters are news where the human has been.
                Event::RaidersRaised { city, count, .. } => {
                    let seen = self
                        .game
                        .economy()
                        .cities()
                        .into_iter()
                        .find(|(e, ..)| *e == city)
                        .is_some_and(|(.., p)| self.game.is_explored(HUMAN, p));
                    seen.then(|| {
                        let args = Args::new()
                            .text("city", self.city_name(city))
                            .count("n", count);
                        self.text_with("log.raiders", &args)
                    })
                }
                Event::MonstersSettled {
                    pos,
                    site,
                    count,
                    evicted,
                } => self.game.is_explored(HUMAN, pos).then(|| {
                    let name = self
                        .game
                        .content()
                        .features
                        .id_of(&site)
                        .and_then(|f| self.game.content().features.get(f))
                        .map_or(site, |def| def.name.clone());
                    let args = Args::new().text("site", name).count("n", count);
                    let key = if evicted > 0 {
                        "log.monsters_took_over"
                    } else {
                        "log.monsters_settled"
                    };
                    self.text_with(key, &args)
                }),
                Event::CityRampaged {
                    name,
                    owner,
                    ruined,
                    ..
                } => {
                    let key = match (owner == HUMAN, ruined) {
                        (true, true) => "log.city_ruined_yours",
                        (true, false) => "log.city_rampaged_yours",
                        (false, true) => "log.city_ruined",
                        (false, false) => "log.city_rampaged",
                    };
                    Some(self.text_with(key, &Args::new().text("city", name)))
                }
                Event::WizardBanished { player, .. } if player == HUMAN => {
                    self.modals.open(Resign);
                    Some(self.text("log.banished_you"))
                }
                Event::WizardBanished { .. } => Some(self.text("log.banished_rival")),
                Event::WizardReturned { player, .. } if player == HUMAN => {
                    Some(self.text("log.returned"))
                }
                Event::WizardDefeated { player } => {
                    let args = self.wizard_arg(Args::new(), "wizard", player);
                    Some(self.text_with("log.wizard_defeated", &args))
                }
                Event::ConquestSpells { winner, spells, .. } => {
                    if winner == HUMAN {
                        self.announce_conquest_spells(&spells);
                    }
                    None
                }
                Event::FortressMoved { .. } => Some(self.text("log.fortress_moved")),
                Event::UnitsLostFleeing { player, units } if player == HUMAN => {
                    self.announce_fled_losses(units);
                    Some(self.text_with("battle.fled_losses", &Args::new().count("n", units)))
                }
                Event::UnitsDrowned { units, .. } => {
                    Some(self.text_with("log.units_drowned", &Args::new().count("n", units)))
                }
                Event::RoadBuilt { .. } => Some(self.text("log.road_built")),
                Event::RoadsEnchanted { .. } => Some(self.text("log.roads_enchanted")),
                Event::VolcanoRaised { .. } => Some(self.text("log.volcano_raised")),
                Event::NodeMelded { .. } => Some(self.text("log.node_melded")),
                Event::MeldFailed { .. } => Some(self.text("log.meld_failed")),
                Event::BuildingsCollapsed { count, .. } => {
                    Some(self.text_with("log.buildings_collapsed", &Args::new().count("n", count)))
                }
                Event::TerrainChanged { .. } => Some(self.text("log.terrain_changed")),
                Event::SummoningCircleMoved { .. } => Some(self.text("log.circle_moved")),
                Event::SpellLearned { player, spell } => {
                    if player == HUMAN {
                        self.modals.enqueue(crate::learn_spell::Learned::new(spell));
                    }
                    Some(self.text_with("log.learned", &self.spell_arg(Args::new(), spell)))
                }
                Event::Diplomacy(d) => self.diplomacy_line(d),
                Event::TurnStarted { .. } => {
                    let due = std::mem::take(&mut self.greetings_due);
                    self.greetings.extend(due);
                    None
                }
                Event::SpellCast { player, spell } => {
                    self.announce_global_cast(player, spell);
                    self.announce_summon(player, spell);
                    Some(self.text_with("log.cast", &self.spell_arg(Args::new(), spell)))
                }
                Event::SpellFizzled { spell, city, .. } => {
                    let args = self
                        .spell_arg(Args::new(), spell)
                        .text("city", self.city_name(city));
                    Some(self.text_with("log.fizzled_nightshade", &args))
                }
                Event::SpellSuppressed { spell, by, player } => {
                    self.announce_fizzle(player, by, spell, "spell.suppress_magic");
                    let args = self.wizard_arg(self.spell_arg(Args::new(), spell), "wizard", by);
                    Some(self.text_with("log.suppressed", &args))
                }
                Event::PowerDrained { victim, amount, .. } => {
                    let args = self
                        .wizard_arg(Args::new(), "wizard", victim)
                        .count("amount", amount);
                    Some(self.text_with("log.power_drained", &args))
                }
                Event::SkillDestroyed { victim, amount, .. } => {
                    let args = self
                        .wizard_arg(Args::new(), "wizard", victim)
                        .count("amount", amount);
                    Some(self.text_with("log.skill_destroyed", &args))
                }
                Event::UnitsSlain { spell, count, .. } => {
                    let args = self.spell_arg(Args::new(), spell).count("n", count);
                    Some(self.text_with("log.units_slain", &args))
                }
                Event::UnitTransformed { from, to, .. } => {
                    let units = &self.game.content().rules.units;
                    let args = Args::new()
                        .text("from", &units.get(from).name)
                        .text("to", &units.get(to).name);
                    Some(self.text_with("log.unit_transformed", &args))
                }
                Event::EarthquakeStruck {
                    units_lost,
                    buildings_lost,
                    ..
                } => {
                    let args = Args::new()
                        .count("units", units_lost)
                        .count("buildings", buildings_lost);
                    Some(self.text_with("log.earthquake", &args))
                }
                Event::MasteryStarted { caster } => {
                    self.modals
                        .enqueue(crate::bigbook::MasteryNotice::new(caster));
                    let args = self.wizard_arg(Args::new(), "wizard", caster);
                    Some(self.text_with("log.mastery_started", &args))
                }
                Event::StasisEnded { unit } => {
                    // The box of a dispelled lasting spell, for a unit of the human's (message 20).
                    let stasis = self.game.content().rules.spells.id_of("stasis");
                    if let Some(spell) = stasis {
                        if self.game.units().unit_owner(unit) == Some(HUMAN) {
                            self.announce_dispelled(
                                spell,
                                f4x_core::game::DispelSite::Unit,
                                &mut dispelled,
                            );
                        }
                    }
                    Some(self.text("log.stasis_ended"))
                }
                Event::UnitMutated { .. } => Some(self.text("log.unit_mutated")),
                Event::WorldSpellStruck {
                    spell,
                    units_lost,
                    buildings_lost,
                    population_lost,
                    ..
                } => {
                    let args = self
                        .spell_arg(Args::new(), spell)
                        .count("units", units_lost)
                        .count("citizens", population_lost)
                        .count("buildings", buildings_lost);
                    Some(self.text_with("log.world_spell_struck", &args))
                }
                Event::SpellBlasted {
                    spell,
                    victim,
                    mana,
                    player,
                } => {
                    self.announce_fizzle(victim, player, spell, "spell.spell_blast");
                    let args = self
                        .wizard_arg(self.spell_arg(Args::new(), spell), "wizard", victim)
                        .count("mana", mana);
                    Some(self.text_with("log.spell_blasted", &args))
                }
                Event::GlobalSpellRemoved {
                    victim,
                    spell,
                    player,
                } => {
                    self.announce_fizzle(victim, player, spell, "spell.disjunction");
                    let args =
                        self.wizard_arg(self.spell_arg(Args::new(), spell), "wizard", victim);
                    Some(self.text_with("log.global_removed", &args))
                }
                Event::GlobalSpellTaken { victim, spell, .. } => {
                    let args =
                        self.wizard_arg(self.spell_arg(Args::new(), spell), "wizard", victim);
                    Some(self.text_with("log.global_taken", &args))
                }
                Event::DispelFailed { spell, .. } => {
                    Some(self.text_with("log.dispel_failed", &self.spell_arg(Args::new(), spell)))
                }
                Event::Dispelled { removed, .. } => {
                    Some(self.text_with("log.dispelled", &Args::new().count("n", removed)))
                }
                Event::NoTargetForCast { player, spell } => {
                    if player == HUMAN {
                        self.modals.open(NoTarget(spell));
                    }
                    None
                }
                Event::SpellReady { player, spell } => {
                    if player == HUMAN {
                        self.aim_paid_spell(spell);
                    }
                    Some(self.text_with("log.spell_ready", &self.spell_arg(Args::new(), spell)))
                }
                Event::HeroJoined { player, kind, .. } => {
                    self.announce_hero_summon(player, kind);
                    let hero = self.game.units().hero_entity(kind);
                    let args = match hero.and_then(|h| self.game.units().hero_name(h)) {
                        Some(name) => Args::new().named(
                            "hero",
                            name,
                            hero.map_or_else(Default::default, |h| {
                                self.game.units().hero_gender(h)
                            }),
                        ),
                        None => Args::new().text("hero", self.text("log.a_hero")),
                    };
                    Some(self.text_with("log.hero_joined", &args))
                }
                Event::HeroSpellFailed { spell, .. } => Some({
                    self.hero_summon_due = None;
                    self.text_with("log.hero_spell_failed", &self.spell_arg(Args::new(), spell))
                }),
                Event::SpellHasNoEffectYet { spell } => {
                    Some(self.text_with("log.no_effect_yet", &self.spell_arg(Args::new(), spell)))
                }
                Event::EnchantmentLapsed { spell, .. } => {
                    Some(self.text_with("log.lapsed", &self.spell_arg(Args::new(), spell)))
                }
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

    /// `args` with the wizard's name, which carries the wizard's gender; a rival the game has no
    /// record of is "a rival".
    pub(crate) fn wizard_arg(&self, args: Args, name: &'static str, player: PlayerId) -> Args {
        match self.game.wizard(player) {
            Some(w) => args.named(name, w.name(), w.gender()),
            None => args.text(name, self.text("log.a_rival")),
        }
    }

    /// `args` with the name of a spell.
    pub(crate) fn spell_arg(&self, args: Args, spell: f4x_core::rules::SpellId) -> Args {
        args.text("spell", &self.game.content().rules.spells.get(spell).name)
    }

    /// One log line for the treasure a cleared site gave.
    fn loot_line(&self, loot: &f4x_core::feature::lair::Loot) -> String {
        let none = Args::new();
        let mut parts = Vec::new();
        if loot.gold > 0 {
            parts.push(self.text_with("log.loot_gold", &Args::new().count("n", loot.gold)));
        }
        if loot.mana > 0 {
            parts.push(self.text_with("log.loot_mana", &Args::new().count("n", loot.mana)));
        }
        if !loot.items.is_empty() {
            parts.push(self.text_with("log.loot_items", &Args::new().count("n", loot.items.len())));
        }
        if loot.prisoner {
            parts.push(self.text_with("log.loot_prisoner", &none));
        }
        if loot.spell.is_some() {
            parts.push(self.text_with("log.loot_spell", &none));
        }
        for book in &loot.books {
            parts.push(self.text_with("log.loot_book", &Args::new().text("realm", book)));
        }
        for _ in &loot.retorts {
            parts.push(self.text_with("log.loot_retort", &none));
        }
        if parts.is_empty() {
            self.text_with("log.site_cleared_empty", &none)
        } else {
            self.text_with("log.site_cleared", &Args::new().list("things", parts))
        }
    }

    pub(crate) fn city_name(&self, city: hecs::Entity) -> String {
        self.game.city_name(city).unwrap_or_else(|| "?".to_string())
    }

    pub(crate) fn item_name(&self, item: f4x_core::city::production::ProductionItem) -> String {
        use f4x_core::city::production::ProductionItem::*;
        let rules = &self.game.content().rules;
        match item {
            Building(b) => rules.buildings.get(b).name.clone(),
            Unit(u) => rules.units.get(u).name.clone(),
            TradeGoods => self.text("city.trade_goods"),
            Housing => self.text("city.housing"),
        }
    }

    /// A city's screen, or an outpost's (a city with no figures yet).
    pub(crate) fn open_city_or_outpost(&mut self, city: hecs::Entity) {
        let outpost = self
            .game
            .economy()
            .city_economy(city)
            .is_some_and(|e| e.figures == 0);
        if outpost {
            self.open_outpost(city, false);
        } else {
            self.open_city_screen(city);
        }
    }

    pub fn open_first_city(&mut self) {
        if let Some((city, ..)) = self.game.economy().cities().first() {
            self.open_city_screen(*city);
        }
    }

    /// Write the game to `path`, reporting the outcome in the message log.
    pub fn save_to(&mut self, path: &Path) {
        match f4x_data::save::save_game(path, &self.game) {
            Ok(()) => {
                let args = Args::new()
                    .count("turn", self.game.turn)
                    .text("path", path.display().to_string());
                self.log.push(self.text_with("log.saved", &args));
            }
            Err(e) => {
                let args = Args::new().text("why", e.to_string());
                self.log.push(self.text_with("log.save_failed", &args));
            }
        }
    }

    /// Replace the game with the one saved at `path`. On failure the current game is kept.
    pub fn load_from(&mut self, path: &Path) -> Result<(), String> {
        let content = self.game.content().clone();
        let (game, integrity) =
            f4x_data::save::load_game(path, content).map_err(|e| e.to_string())?;
        // The next new game keeps its own seed; a loaded game does not set it.
        // Keep playing the same race: the human player's capital race.
        if let Some(r) = game.wizard(HUMAN).and_then(|w| w.capital_race()) {
            self.race = r.num() as usize;
        }
        self.game = game;
        self.game.ensure_human(HUMAN);
        self.game.set_undo(true);
        self.ais = self.rival_wizards();
        self.defences.clear();
        // A game saved in a battle comes back in it: the frame loop opens the battle screen.
        self.close_page(&Screen::Battle);
        self.turn_events = None;
        self.game_over_seen = false;
        self.end_scenes.clear();
        self.end_scene_queued = false;
        self.screen = Screen::Map;
        self.choosing = false;
        self.selection.clear();
        let home = self
            .game
            .economy()
            .cities()
            .into_iter()
            .find(|(_, c, _)| c.owner == HUMAN)
            .map(|(_, _, p)| p);
        if let Some(pos) = home {
            self.center_on(pos);
        }
        let args = Args::new()
            .count("turn", self.game.turn)
            .text("path", path.display().to_string());
        self.log.push(self.text_with("log.loaded", &args));
        if integrity != f4x_core::game::save::Integrity::Intact {
            self.log.push(self.text("log.save_edited"));
        }
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
                    .filter_map(|s| self.game.units().entity_of_serial(s))
                    .collect();
                self.targeting = None;
                self.log
                    .push(self.text_with("log.undid", &Args::new().text("what", label)));
            }
            Err(_) => self.log.push(self.text("log.nothing_to_undo")),
        }
    }

    /// End the turn. First put more citizens on farms if the army would otherwise go hungry
    /// (the city screen lets you set farmers by hand; this just protects the troops).
    pub(crate) fn end_turn(&mut self) {
        if self.turn_events.is_some() {
            return;
        }
        let t = self.game.budget(HUMAN);
        if t.food_surplus < t.food_upkeep {
            self.game.balance_farmers(HUMAN);
        }
        // The rival wizards play first; an attack on the human is fought before the turn ends.
        // Their commands are not taken back, and undo would snapshot the whole game for each.
        let undo = self.game.undo_enabled();
        self.game.set_undo(false);
        let mut events = Vec::new();
        for ai in &mut self.ais {
            events.extend(ai.play_turn(&mut self.game));
            for attack in ai.take_attacks() {
                self.defences
                    .push_back(crate::attack_prompt::PendingAttack {
                        attackers: attack.attackers,
                        defenders: attack.defenders,
                        defending: true,
                        treaty_asked: true,
                    });
            }
        }
        // The independent cities and the roaming monsters run themselves through their own AI
        // (farmers, taxes, builds, raids); the human fights the attacks on them in person.
        let (neutral_events, attacks) = f4x_ai::play_neutrals_for(&mut self.game, &[HUMAN]);
        events.extend(neutral_events);
        self.game.set_undo(undo);
        for attack in attacks {
            self.defences
                .push_back(crate::attack_prompt::PendingAttack {
                    attackers: attack.attackers,
                    defenders: attack.defenders,
                    defending: true,
                    treaty_asked: true,
                });
        }
        self.turn_events = Some(events);
        self.resume_turn();
    }

    /// Go on with the turn change once no attack on the human waits: end the turn for everyone
    /// and report what the rivals and the turn did.
    pub(crate) fn resume_turn(&mut self) {
        if self.modals.has(crate::modal::kind::ATTACK) || self.game.in_battle() {
            return;
        }
        if let Some(attack) = self.next_defence() {
            self.modals
                .open(crate::attack_prompt::AttackAsk::new(attack));
            return;
        }
        let Some(mut events) = self.turn_events.take() else {
            return;
        };
        if self.game.game_over.is_none() {
            events.extend(self.game.apply(Command::EndTurn));
            // The rivals now say what they have to say to the human: offers, requests, war.
            let undo = self.game.undo_enabled();
            self.game.set_undo(false);
            for ai in &mut self.ais {
                events.extend(ai.approach_human(&mut self.game));
            }
            self.game.set_undo(undo);
        }
        // What the rivals did out of sight is not shown: only what touches the human.
        let events: Vec<Event> = events
            .into_iter()
            .filter(|e| self.concerns_human(e))
            .collect();
        let turn = self.game.turn;
        let news = self.collect_report(&events);
        self.last_news = news.clone();
        self.note(events);
        self.open_chancellor(news);
        self.log
            .push(self.text_with("log.turn", &Args::new().count("turn", turn)));
    }

    /// The next attack on the human that is still real (a unit may have died to an earlier one).
    fn next_defence(&mut self) -> Option<crate::attack_prompt::PendingAttack> {
        while let Some(attack) = self.defences.pop_front() {
            let alive = |u: &hecs::Entity| self.game.units().unit_owner(*u).is_some();
            let mine = attack
                .defenders
                .first()
                .is_some_and(|&u| self.game.units().unit_owner(u) == Some(HUMAN));
            if mine && attack.attackers.iter().all(alive) && attack.defenders.iter().all(alive) {
                return Some(attack);
            }
        }
        None
    }

    /// Whether the human sees this event of a rival's turn: their refusals, the founding of a
    /// city out of sight and the like stay hidden.
    fn concerns_human(&self, event: &Event) -> bool {
        match event {
            Event::Rejected(_) | Event::Battle(_) | Event::BattleStarted { .. } => false,
            Event::CityFounded { city, .. } => self.game.economy().city_owner(*city) == Some(HUMAN),
            _ => true,
        }
    }

    /// Pass turns without playing them (for screenshots and smoke tests).
    pub fn end_turns(&mut self, turns: u32) {
        for _ in 0..turns {
            self.end_turn();
            // No one defends in person here: the computer fights the attacks on the human.
            while self.turn_events.is_some() {
                while let Some(attack) = self.next_defence() {
                    let _ = self.game.apply(Command::AutoResolve {
                        mana_budget: None,
                        attackers: attack.attackers,
                        defenders: attack.defenders,
                    });
                }
                self.resume_turn();
            }
        }
        self.modals.remove(crate::modal::kind::CHANCELLOR);
    }

    pub fn select_first_stack(&mut self) {
        if let Some(&u) = self.game.units().player_units(HUMAN).first() {
            if let Some(at) = self.game.units().unit_position(u) {
                self.select_at(at);
            }
        }
    }

    pub(crate) fn tile_size(&self) -> Vec2 {
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

    pub(crate) fn tile_at(&self, screen: Vec2) -> Option<Position> {
        let r = self.map_rect();
        if !r.contains(screen) {
            return None;
        }
        let t = self.tile_size();
        let x = ((screen.x - r.x) / t.x + self.cam.x).floor() as i32;
        let y = ((screen.y - r.y) / t.y + self.cam.y).floor() as i32;
        let grid = self.game.map()?.plane(self.plane);
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
        let content = self.game.content().clone();
        match Self::new_game(
            seed,
            &self.options,
            &content,
            self.human_build.as_ref(),
            self.race,
            self.prefs.get(crate::preferences::Pref::RandomEvents),
        ) {
            Ok(game) => {
                self.game = game;
                // The next game gets another seed, unless `--seed` fixed it.
                self.seed = if self.seed_fixed { seed } else { fresh_seed() };
                self.game_over_seen = false;
                self.end_scenes.clear();
                self.end_scene_queued = false;
                self.screen = Screen::Map;
                self.selection.clear();
                self.log.clear();
                self.start_game();
                self.offer_capital_naming();
                self.log
                    .push(self.text_with("log.new_map", &Args::new().count("seed", seed)));
                if let Some(each) = self.game.settings().basic.crowding() {
                    self.log
                        .push(self.text_with("log.crowded", &Args::new().count("n", each)));
                }
            }
            Err(e) => {
                let args = Args::new().text("why", e.to_string());
                self.log.push(self.text_with("log.map_failed", &args));
            }
        }
    }

    /// Explore every square on both planes (for screenshots and tests).
    pub fn reveal_all(&mut self) {
        self.game.reveal_map(HUMAN);
    }

    /// Put the top-left of the map view on a tile.
    pub fn set_camera(&mut self, x: f32, y: f32) {
        self.cam = vec2(x, y);
    }

    /// For screenshots: warp every magic node, explore the map and show the first node.
    pub fn open_test_warped_nodes(&mut self) {
        let nodes = self.game.economy().nodes();
        for node in &nodes {
            self.game.warp_node(node.pos);
        }
        self.game.reveal_map(HUMAN);
        if let Some(node) = nodes.first() {
            self.set_camera(node.pos.x as f32 - 4.0, node.pos.y as f32 - 3.0);
        }
    }

    /// With End of Turn Wait off: the last unit that waited for orders has just been given them
    /// (a turn that starts with every unit already settled does not end by itself).
    fn last_order_given(&mut self) -> bool {
        let waits = !self.all_units_moved();
        let was = std::mem::replace(&mut self.units_waited, waits);
        was && !waits
            && !self.prefs.get(Pref::EndOfTurnWait)
            && matches!(self.screen, Screen::Map)
            && self.turn_pending == 0
            && self.turn_events.is_none()
            && !self.modals.blocks_turn()
            && self.defences.is_empty()
            && !self.game.in_battle()
    }

    /// Handle input and draw one frame. Returns false when the player quits.
    pub fn frame(&mut self, input: &Input) -> bool {
        self.update_audio(input);
        if self.turn_pending > 0 {
            self.run_pending_turn();
        }
        self.selection
            .retain(|&u| self.game.units().unit_position(u).is_some());
        if self.game.in_battle() {
            if !self.has_page(&Screen::Battle) {
                self.open_battle_screen();
            } else if !matches!(
                self.screen,
                Screen::Battle | Screen::Console | Screen::UnitSheet
            ) {
                self.screen = Screen::Battle;
            }
        } else if !matches!(self.screen, Screen::Battle) {
            self.close_page(&Screen::Battle);
        }
        if self.hotkeys.pressed("console", input) {
            self.toggle_console();
        }
        // The result scroll waits until the last blow of the battle has been played.
        let still_playing = matches!(self.screen, Screen::Battle)
            && self
                .pages
                .get(&Screen::Battle)
                .is_some_and(|p| p.animating());
        if !self.game.in_battle() && !still_playing {
            if let Some(results) = self.combat_results.take() {
                self.modals.open(results);
            }
        }
        self.queue_proposal();
        self.queue_offer();
        if !self.modals.is_empty() && !matches!(self.screen, Screen::Console) {
            return self.modal_frame(input);
        }
        if self.item_pool_frame(input) {
            return true;
        }
        // A wizard just met greets the player on the next turn, on the diplomacy screen.
        if matches!(self.screen, Screen::Map) && !self.game.in_battle() {
            if let Some(w) = self.greetings.pop_front() {
                self.open_diplomacy(w);
                return true;
            }
        }
        if self.turn_events.is_some() && matches!(self.screen, Screen::Map) {
            self.resume_turn();
            if self.turn_events.is_none() {
                self.save_to(std::path::Path::new(AUTOSAVE));
            }
        }
        if matches!(self.screen, Screen::Map)
            && !self.game.in_battle()
            && self.turn_events.is_none()
        {
            self.check_hero_levels();
        }
        self.maybe_ask_for_research();
        if self.game.game_over.is_some()
            && !self.game_over_seen
            && !self.game.in_battle()
            && !matches!(self.screen, Screen::GameOver | Screen::Console)
        {
            // The victory or the defeat is played before the score.
            if !self.end_scene_queued {
                self.end_scene_queued = true;
                self.queue_game_over_scene();
            }
            if self.end_scenes.is_empty() {
                self.open_page(Screen::GameOver, crate::game_over_screen::GameOverPage);
            }
        }
        if !matches!(self.screen, Screen::Console | Screen::MainMenu) && self.end_scene_frame(input)
        {
            return true;
        }
        self.watch_for_help(input);
        self.screen_frame(input)
    }

    /// One frame of the screen that is open (`self.screen`): its input, then its drawing. A modal
    /// draws it as the base with an empty `Input`, so it shows no hover and takes no click.
    fn screen_frame(&mut self, input: &Input) -> bool {
        match self.screen {
            Screen::GameOver => self.page_frame(input),
            Screen::Console => self.page_frame(input),
            Screen::City(_) => self.page_frame(input),
            Screen::Magic => self.page_frame(input),
            Screen::Wizard => self.page_frame(input),
            Screen::UnitSheet => self.page_frame(input),
            Screen::ArmyList => self.page_frame(input),
            Screen::CityList => self.page_frame(input),
            Screen::Outpost => self.page_frame(input),
            Screen::EnemyCity => self.page_frame(input),
            Screen::HallOfFame => self.page_frame(input),
            Screen::Items => self.page_frame(input),
            Screen::Craft => self.page_frame(input),
            Screen::Diplomacy => self.page_frame(input),
            Screen::Spellbook => self.page_frame(input),
            Screen::Apprentice => self.page_frame(input),
            Screen::Research => self.page_frame(input),
            Screen::Battle => self.page_frame(input),
            Screen::Create => self.page_frame(input),
            Screen::Options => self.page_frame(input),
            Screen::Cartographer => self.page_frame(input),
            Screen::History => self.page_frame(input),
            Screen::Astrologer => self.page_frame(input),
            Screen::Settings => self.page_frame(input),
            Screen::GameSettings => self.page_frame(input),
            Screen::MainMenu => {
                self.page_frame(input);
                return !self.quit;
            }
            Screen::Intro => self.page_frame(input),
            Screen::Slots => self.page_frame(input),
            Screen::Map => {
                if input.key_pressed(KeyCode::Escape) {
                    if self.targeting.take().is_some() {
                        self.log.push(self.text("log.cast_cancelled"));
                    } else if self.road_plan.is_some() {
                        self.road_plan_cancel();
                    } else if self.surveyor {
                        self.stop_surveyor();
                    } else {
                        self.open_slots(false);
                    }
                }
                self.overworld_input(input);
                clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
                self.draw_map();
                self.draw_bars(input);
                self.road_plan_bar(input);
                self.surveyor_bar(input);
            }
        }
        true
    }

    /// One frame of the top modal, over the screen that is open, dimmed if the player wants it.
    fn modal_frame(&mut self, input: &Input) -> bool {
        let Some((at, modal)) = self.modals.take_top() else {
            return false;
        };
        if !modal.covers_screen() {
            self.under_modal = true;
            self.screen_frame(&Input::default());
            self.under_modal = false;
        }
        if !modal.covers_screen() && self.prefs.get(crate::preferences::Pref::DimBehindBoxes) {
            draw_rectangle(
                0.0,
                0.0,
                screen_width(),
                screen_height(),
                Color::new(0.0, 0.0, 0.0, DIM),
            );
        }
        let left = modal.frame(self, input);
        self.modals.restore(at, left);
        true
    }

    /// The question after aiming a Spell Ward at a city: which realm to ward it against, by the
    /// first letter of the realm, shown over the map. Realms already warded are not offered.
    fn ward_prompt_frame(&mut self, ward: Ward, input: &Input) -> Option<Ward> {
        let Ward {
            spell,
            city,
            resolve,
        } = ward;
        const KEYS: [(KeyCode, &str); 5] = [
            (KeyCode::N, "nature"),
            (KeyCode::S, "sorcery"),
            (KeyCode::C, "chaos"),
            (KeyCode::L, "life"),
            (KeyCode::D, "death"),
        ];
        let open: Vec<(KeyCode, &str)> = KEYS
            .into_iter()
            .filter(|(_, r)| !self.game.economy().city_warded_against(city, r))
            .collect();
        let mut left = Some(Ward {
            spell,
            city,
            resolve,
        });
        if input.key_pressed(KeyCode::Escape) || open.is_empty() {
            left = None;
            self.log.push(self.text("log.cast_cancelled"));
        } else if let Some(&(_, realm)) = open.iter().find(|(k, _)| input.key_pressed(*k)) {
            left = None;
            if let Some(realm) = self.game.content().rules.realms.id_of(realm) {
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
            &self.text_with("spell.ward_select", &Args::new().text("city", &name)),
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
        crate::frame::text_centered(
            &self.text("ui.esc_cancel"),
            w / 2.0,
            h / 2.0 + 50.0,
            18.0,
            WHITE,
        );
        left
    }

    /// The cast has nothing to aim at: "There are no targets for X. Do you wish to continue
    /// casting it?" Yes keeps the spell (it is not paid for until a target exists), No gives it
    /// up.
    fn no_target_frame(&mut self, no_target: NoTarget, input: &Input) -> Option<NoTarget> {
        let NoTarget(spell) = no_target;
        let casting = self.game.wizard(HUMAN).and_then(|m| m.casting());
        if casting.is_none_or(|c| c.spell != spell) {
            return None;
        }
        let lay = crate::frame::Layout::new();
        let name = self.game.content().rules.spells.get(spell).name.clone();
        let text = self.text_with("spell.no_target", &Args::new().text("spell", &name));
        match self.confirmation_box(&lay, &text, input) {
            Some(true) => None,
            Some(false) => {
                let events = self.game.apply(Command::CancelCast { player: HUMAN });
                self.note(events);
                None
            }
            None => Some(NoTarget(spell)),
        }
    }

    /// The list of the rivals' global enchantments a Disjunction or Spell Binding can aim at,
    /// by wizard, with the spell's name and cost: Up and Down move, Enter or a click casts, and
    /// Escape gives up.
    fn global_prompt_frame(&mut self, global: Global, input: &Input) -> Option<Global> {
        let Global {
            spell,
            resolve,
            row,
        } = global;
        let rows = self.game.rival_globals(HUMAN);
        let row = row.min(rows.len().saturating_sub(1));
        let spells = self.game.spells();
        let lines: Vec<String> = rows
            .iter()
            .filter_map(|&i| {
                let def = self.game.content().rules.spells.get(spells.def(i)?);
                let who = self.game.player(spells.caster_player(i)?)?.name();
                let args = Args::new()
                    .text("wizard", who)
                    .text("spell", &def.name)
                    .count("cost", def.cost);
                Some(self.text_with("spell.global_row", &args))
            })
            .collect();
        let (w, h) = (screen_width(), screen_height());
        let (top, step) = (h / 2.0 - 20.0, 30.0);
        let clicked = (0..lines.len()).find(|&n| {
            let y = top + step * n as f32;
            ui::hit(
                Rect::new(w / 2.0 - 260.0, y - 20.0, 520.0, 28.0),
                true,
                input,
            )
            .clicked()
        });
        let mut pick = clicked.unwrap_or(row);
        if input.key_pressed(KeyCode::Up) {
            pick = pick.saturating_sub(1);
        }
        if input.key_pressed(KeyCode::Down) {
            pick = (pick + 1).min(lines.len().saturating_sub(1));
        }
        let mut left = None;
        if input.key_pressed(KeyCode::Escape) || rows.is_empty() {
            self.log.push(self.text("log.cast_cancelled"));
        } else if clicked.is_some() || input.key_pressed(KeyCode::Enter) {
            let target = f4x_core::rules::exec::Target::Spell(rows[pick]);
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
        } else {
            left = Some(Global {
                spell,
                resolve,
                row: pick,
            });
        }
        let height = 110.0 + step * lines.len() as f32;
        draw_rectangle(
            w / 2.0 - 280.0,
            h / 2.0 - 80.0,
            560.0,
            height,
            Color::new(0.08, 0.06, 0.04, 0.95),
        );
        draw_rectangle_lines(w / 2.0 - 280.0, h / 2.0 - 80.0, 560.0, height, 3.0, GOLD);
        let name = self.game.content().rules.spells.get(spell).name.clone();
        crate::frame::text_centered(
            &self.text_with("spell.choose_global", &Args::new().text("spell", &name)),
            w / 2.0,
            h / 2.0 - 45.0,
            22.0,
            WHITE,
        );
        for (n, line) in lines.iter().enumerate() {
            let color = if n == pick { GOLD } else { WHITE };
            crate::frame::text_centered(line, w / 2.0, top + step * n as f32, 22.0, color);
        }
        crate::frame::text_centered(
            &self.text("spell.global_keys"),
            w / 2.0,
            top + step * lines.len() as f32 + 10.0,
            18.0,
            WHITE,
        );
        left
    }

    /// How many turns the Spell of Return still needs at the human's casting skill.
    fn turns_to_return(&self, remaining: i32) -> i32 {
        let skill = self
            .game
            .wizard(HUMAN)
            .map_or(1, |m| m.casting_skill())
            .max(1);
        (remaining + skill - 1) / skill
    }

    /// A banished human may not throw spells: say so, with the turns left until the return
    /// (`MagicScr.c`). Returns whether the human is banished.
    pub(crate) fn refuse_banished_spells(&mut self) -> bool {
        let Some(wizard) = self.game.wizard(HUMAN).filter(|w| w.is_banished()) else {
            return false;
        };
        let remaining = wizard.casting().map_or(0, |c| c.remaining);
        let turns = self.turns_to_return(remaining);
        let line = self.text_with("log.banished_spells", &Args::new().count("turns", turns));
        self.log.push(line);
        true
    }

    /// What the keys answer to the resign question; whether it stays open.
    pub(crate) fn resign_answer(&mut self, input: &Input) -> bool {
        if input.key_pressed(KeyCode::Y) {
            let events = self.game.apply(Command::Resign { player: HUMAN });
            self.note(events);
            return false;
        }
        !(input.key_pressed(KeyCode::Enter)
            || input.key_pressed(KeyCode::N)
            || input.key_pressed(KeyCode::Escape))
    }

    /// The question after the human's fortress falls (`Banish_Wizard`): resign (Y) or cast the
    /// Spell of Return (N, Enter), shown over the map.
    fn resign_prompt_frame(&mut self, input: &Input) -> Option<Resign> {
        let casting = self.game.wizard(HUMAN).and_then(|m| m.casting());
        let casting = casting.filter(|_| self.game.casting_return(HUMAN))?;
        let left = self.resign_answer(input).then_some(Resign);
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(
            w / 2.0 - 300.0,
            h / 2.0 - 60.0,
            600.0,
            120.0,
            Color::new(0.08, 0.06, 0.04, 0.95),
        );
        draw_rectangle_lines(w / 2.0 - 300.0, h / 2.0 - 60.0, 600.0, 120.0, 3.0, GOLD);
        let turns = self.turns_to_return(casting.remaining);
        crate::frame::text_centered(
            &self.text_with("log.resign_question", &Args::new().count("turns", turns)),
            w / 2.0,
            h / 2.0 - 15.0,
            20.0,
            WHITE,
        );
        crate::frame::text_centered(
            &self.text("log.resign_keys"),
            w / 2.0,
            h / 2.0 + 30.0,
            22.0,
            GOLD,
        );
        left
    }

    fn toggle_plane(&mut self) {
        self.plane = if self.plane == Plane::Arcanus {
            Plane::Myrror
        } else {
            Plane::Arcanus
        };
    }

    /// The numpad digits (and the arrow keys, while they do not scroll) move the selected stack
    /// one square.
    fn stack_keys(&mut self, input: &Input) {
        let Some(from) = self.selection_position() else {
            return;
        };
        let shift = input.key_down(KeyCode::LeftShift) || input.key_down(KeyCode::RightShift);
        let Some(&(_, (dx, dy))) = [
            ("move_down_left", (-1, 1)),
            ("move_down", (0, 1)),
            ("move_down_right", (1, 1)),
            ("move_left", (-1, 0)),
            ("move_right", (1, 0)),
            ("move_up_left", (-1, -1)),
            ("move_up", (0, -1)),
            ("move_up_right", (1, -1)),
        ]
        .iter()
        // With Shift held, the keys that scroll the map scroll it.
        .find(|(action, _)| self.hotkeys.pressed_moving(action, shift, input)) else {
            return;
        };
        let Some(map) = self.game.map() else {
            return;
        };
        let (w, h) = (map.width() as i32, map.height() as i32);
        let y = from.y as i32 + dy;
        if !(0..h).contains(&y) {
            return;
        }
        let to = Position {
            plane: from.plane,
            x: (from.x as i32 + dx).rem_euclid(w) as u32,
            y: y as u32,
        };
        let events = self.game.apply(Command::MoveStack {
            units: self.selection.clone(),
            to,
        });
        self.note(events);
        // A stack with nothing left to do hands over to the next one waiting for orders.
        let spent = self
            .selection
            .iter()
            .all(|&u| self.game.units().unit_moves_left(u) == 0);
        if spent {
            self.select_next_idle(false);
        }
    }

    pub(crate) fn select_at(&mut self, pos: Position) {
        let mine: Vec<_> = self
            .game
            .units()
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.game.units().unit_owner(u) == Some(HUMAN))
            .collect();
        self.stack_page = 0;
        self.selection = mine;
    }

    /// The human's units on a square, all of them (a square holds more than an army does).
    pub(crate) fn own_units_at(&self, pos: Position) -> Vec<hecs::Entity> {
        self.game
            .units()
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.game.units().unit_owner(u) == Some(HUMAN))
            .collect()
    }

    /// Show the next nine units of a crowded square in the unit window (all of them stay
    /// selected as they were).
    fn next_stack_page(&mut self) {
        let Some(at) = self.selection_position() else {
            return;
        };
        let units = self.own_units_at(at);
        let pages = units.len().div_ceil(f4x_core::unit::MAX_STACK);
        if pages > 1 {
            self.stack_page = (self.stack_page + 1) % pages;
        }
    }

    pub(crate) fn selection_position(&self) -> Option<Position> {
        self.selection
            .first()
            .and_then(|&u| self.game.units().unit_position(u))
    }

    fn overworld_input(&mut self, input: &Input) {
        let step = 12.0 * get_frame_time();
        let mut pan = Vec2::ZERO;
        // The arrow keys move the selected stack, as in the original; they scroll the map when
        // nothing is selected, or with Shift held.
        let shift = input.key_down(KeyCode::LeftShift) || input.key_down(KeyCode::RightShift);
        let scrolls = self.selection.is_empty() || shift;
        if scrolls {
            for (action, dir) in [
                ("scroll_left", vec2(-1.0, 0.0)),
                ("scroll_right", vec2(1.0, 0.0)),
                ("scroll_up", vec2(0.0, -1.0)),
                ("scroll_down", vec2(0.0, 1.0)),
            ] {
                if self.hotkeys.down(action, input) {
                    pan += dir;
                }
            }
        }
        self.cam += pan * step;
        if self.hotkeys.pressed("next_page", input) {
            self.next_stack_page();
        }
        if self.hotkeys.pressed("other_plane", input) {
            self.toggle_plane();
        }
        if self.hotkeys.pressed("zoom_in", input) {
            self.zoom = (self.zoom + 1.0).min(6.0);
        }
        if self.hotkeys.pressed("zoom_out", input) {
            self.zoom = (self.zoom - 1.0).max(1.0);
        }
        if self.hotkeys.pressed("end_turn", input) {
            self.request_end_turn();
        }
        if self.hotkeys.pressed("undo", input) {
            self.undo_last();
        }
        if self.hotkeys.pressed("quick_save", input) {
            self.save_to(Path::new(QUICKSAVE));
        }
        if self.hotkeys.pressed("quick_load", input) {
            if let Err(e) = self.load_from(Path::new(QUICKSAVE)) {
                self.log.push(
                    self.text_with("log.load_failed", &Args::new().text("why", e.to_string())),
                );
            }
        }
        if self.hotkeys.pressed("magic", input) {
            self.open_magic();
        }
        if self.hotkeys.pressed("spellbook", input) {
            self.open_spellbook();
        }
        if self.hotkeys.pressed("army_list", input) {
            self.open_army_list();
        }
        if self.hotkeys.pressed("game_menu", input) {
            self.open_slots(false);
        }
        for (action, advisor) in [
            ("surveyor", 0),
            ("cartographer", 1),
            ("apprentice", 2),
            ("history", 3),
            ("astrologer", 4),
            ("chancellor", 5),
            ("tax_collector", 6),
            ("grand_vizier", 7),
            ("mirror", 8),
        ] {
            if self.hotkeys.pressed(action, input) {
                self.open_advisor(advisor);
            }
        }
        if self.hotkeys.pressed("advisors", input) {
            self.open_advisor_menu();
        }
        self.road_plan_keys(input);
        if self.hotkeys.pressed("special", input) && self.road_plan.is_some() {
            self.road_plan_confirm();
        } else if self.hotkeys.pressed("special", input) {
            self.special_button();
        }
        if self.hotkeys.pressed("unselect", input) {
            self.selection.clear();
        }
        if self.hotkeys.pressed("plane_shift", input) {
            // The plane button: the selected units change plane if they can, else the view does.
            let shifted = !self.selection.is_empty() && {
                let events = self.game.apply(Command::ShiftPlane {
                    units: self.selection.clone(),
                });
                let ok = !events
                    .iter()
                    .any(|e| matches!(e, f4x_core::game::Event::Rejected(_)));
                self.note(events);
                ok
            };
            if shifted {
                if let Some(at) = self.selection_position() {
                    self.plane = at.plane;
                }
            } else {
                self.toggle_plane();
            }
        }
        self.stack_keys(input);
        if self.hotkeys.pressed("balance_farmers", input) {
            self.game.balance_farmers(HUMAN);
            self.log.push(self.text("log.farmers_balanced"));
        }
        if self.hotkeys.pressed("wait", input) && !self.selection.is_empty() {
            self.select_next_idle(true);
        }
        if self.hotkeys.pressed("done", input) && !self.selection.is_empty() {
            self.game.apply(Command::ClearOrders {
                units: self.selection.clone(),
            });
        }
        if self.hotkeys.pressed("centre", input) {
            if let Some(at) = self.selection_position() {
                self.center_on(at);
            }
        }
        self.clamp_camera();

        let hovered = self.tile_at(input.mouse_position().into());
        if let Some(pos) = hovered {
            if input.mouse_pressed(MouseButton::Left) {
                self.click(pos, input);
            }
            if input.mouse_pressed(MouseButton::Right) && self.surveyor {
                self.stop_surveyor();
            } else if input.mouse_pressed(MouseButton::Right) && self.road_plan.is_some() {
                self.road_plan_cancel();
            } else if input.mouse_pressed(MouseButton::Right)
                && self.game.is_explored(HUMAN, pos)
                && self.foreign_stack_at(pos)
                && self.open_unit_list(pos)
            {
            } else if input.mouse_pressed(MouseButton::Right)
                && !self.selection.is_empty()
                && !self.prefs.get(Pref::LeftClickMoves)
                && self.selection_position() != Some(pos)
            {
                self.go_to(pos);
            } else if input.mouse_pressed(MouseButton::Right) {
                // A right-click on one of your stacks shows the sheet of its first unit; the
                // arrows there step through the others.
                let stack = self.own_stack_at(pos);
                if let Some(&first) = stack.first() {
                    self.open_unit_sheet_of(first, stack);
                }
            }
        }
    }

    /// Send the selected units to `pos` (the click that moves units is the preference
    /// `Left Click Moves Units`).
    fn go_to(&mut self, pos: Position) {
        // A stack in a city with an Earth Gate that is sent to another city with one is
        // teleported there at once, for one move (`MainScr.c`, the Earth Gate branch).
        if self.game.can_use_earth_gate(&self.selection, pos).is_ok() {
            let events = self.game.apply(Command::EarthGate {
                units: self.selection.clone(),
                to: pos,
            });
            self.note(events);
            if let Some(at) = self.selection_position() {
                self.center_on(at);
            }
            return;
        }
        let events = self.game.apply(Command::GoTo {
            units: self.selection.clone(),
            to: pos,
        });
        self.note(events);
    }

    fn click(&mut self, pos: Position, input: &Input) {
        if let Some(t) = self.targeting.take() {
            self.aim_at(t, pos);
            return;
        }
        if self.road_plan.is_some() {
            if !self.road_plan_on_button(input) {
                self.road_plan_click(pos);
            }
            return;
        }
        if self.surveyor {
            // A click takes the map there.
            let (cols, rows) = self.visible_tiles();
            self.cam = vec2(pos.x as f32 - cols / 2.0, pos.y as f32 - rows / 2.0);
            self.clamp_camera();
            return;
        }
        let has_mine = self
            .game
            .units()
            .units_at(pos)
            .iter()
            .any(|&u| self.game.units().unit_owner(u) == Some(HUMAN));
        if has_mine {
            self.select_at(pos);
        } else if !self.selection.is_empty() && self.prefs.get(Pref::LeftClickMoves) {
            self.go_to(pos);
        } else if let Some(city) = self.game.economy().city_at(pos) {
            self.selection.clear();
            if self.game.economy().city_owner(city) == Some(HUMAN) {
                self.open_city_or_outpost(city);
            } else if self.game.is_explored(HUMAN, pos) {
                self.open_enemy_city(city);
            }
        } else {
            self.selection.clear();
        }
    }

    /// Whether the square holds units of another wizard or of the neutral player.
    /// The human's units on a square, as many as a stack holds.
    fn own_stack_at(&self, pos: Position) -> Vec<hecs::Entity> {
        self.game
            .units()
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.game.units().unit_owner(u) == Some(HUMAN))
            .take(f4x_core::unit::MAX_STACK)
            .collect()
    }

    fn foreign_stack_at(&self, pos: Position) -> bool {
        self.game
            .units()
            .units_at(pos)
            .iter()
            .any(|&u| self.game.units().unit_owner(u) != Some(HUMAN))
    }

    /// The special button of the original (Build, Meld or Purify), by what the selection can do.
    pub(crate) fn special_button(&mut self) {
        use crate::unit_window::Special;
        match self.special_action() {
            Special::Build => self.settle(),
            Special::Meld => {
                let events = self.game.apply(Command::Meld {
                    units: self.selection.clone(),
                });
                self.note(events);
            }
            Special::Purify => {
                let events = self.game.apply(Command::Purify {
                    units: self.selection.clone(),
                });
                self.note(events);
            }
            Special::None if self.selection.is_empty() => self.settle(),
            Special::None => {}
        }
    }

    fn settle(&mut self) {
        // Engineers plan a road (road_plan.rs); settlers found an outpost.
        let builders: Vec<_> = self
            .selection
            .iter()
            .copied()
            .filter(|&u| {
                self.game
                    .units()
                    .unit_stat(u, f4x_core::rules::Stat::Construction)
                    > 0
            })
            .collect();
        let has_settlers = self
            .selection
            .iter()
            .any(|&u| self.game.units().unit_has_ability(u, "create_outpost"));
        if !builders.is_empty() && !has_settlers {
            self.start_road_plan();
            return;
        }
        let settler = self
            .selection
            .iter()
            .copied()
            .find(|&u| self.game.units().unit_has_ability(u, "create_outpost"));
        let Some(unit) = settler else {
            self.log.push(self.text("log.select_settlers"));
            return;
        };
        let name = self.next_city_name();
        let events = self.game.apply(Command::SettleOutpost { unit, name });
        let founded = events.iter().find_map(|e| match e {
            Event::CityFounded { city, .. } => Some(*city),
            _ => None,
        });
        self.note(events);
        if let Some(city) = founded {
            self.open_outpost(city, true);
        }
    }

    pub(crate) fn draw_map(&mut self) {
        let Some(map) = self.game.map() else {
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
        let visible = self.game.economy().visibility(HUMAN, self.plane);
        let cities = self.game.economy().cities();
        let anim = (get_time() * 2.5) as usize;

        // Clip the map to its viewport.
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        let dpi = screen_dpi_scale();
        gl.quad_gl.scissor(Some((
            (r.x * dpi) as i32,
            (r.y * dpi) as i32,
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
                let corners = self.game.explored_corners(
                    HUMAN,
                    Position {
                        plane: self.plane,
                        x: wx,
                        y: y as u32,
                    },
                );
                if corners == 0 {
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
                if corners != 15 {
                    crate::map_draw::draw_unexplored_edge(
                        &mut self.art,
                        corners,
                        sx,
                        sy,
                        t.x / TILE_W,
                    );
                }
            }
        }
        for (entity, city, pos) in &cities {
            if pos.plane != self.plane
                || !self.game.is_explored(
                    HUMAN,
                    Position {
                        plane: self.plane,
                        x: pos.x,
                        y: pos.y,
                    },
                )
            {
                continue;
            }
            let at = screen_of(pos.x, pos.y);
            crate::map_draw::draw_city(&self.game, &mut self.art, *entity, at.x, at.y, t);
            let owner = city.owner;
            if self.game.wizard(owner).and_then(|w| w.fortress()) == Some(*pos) {
                // A gold star: the wizard's fortress.
                crate::map_draw::draw_star(
                    vec2(at.x + t.x * 0.15, at.y + t.y * 0.25),
                    t.x * 0.17,
                    GOLD,
                );
            }
            if self.game.wizard(owner).and_then(|w| w.summon_at()) == Some(*pos) {
                draw_circle_lines(at.x + t.x * 0.85, at.y + t.y * 0.85, t.x * 0.12, 2.0, GOLD);
            }
            // The name under the city, in the original's small font.
            let k = t.x / TILE_W;
            let name = city.name.clone();
            let (cx, cy) = ((at.x + t.x / 2.0) / k, (at.y + t.y) / k - 2.0);
            self.draw_styled((0.0, 0.0, k), &Style::outlined(1).center(), &name, cx, cy);
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
        self.finish_slides();
        let mut stacks: HashMap<(u32, u32), Vec<hecs::Entity>> = HashMap::new();
        for unit in self.game.units().all_units() {
            let Some(at) = self.game.units().unit_position(unit) else {
                continue;
            };
            if at.plane != self.plane || self.is_sliding(unit) {
                continue;
            }
            let mine = self.game.units().unit_owner(unit) == Some(HUMAN);
            if !mine
                && (!visible[(at.y * width + at.x) as usize]
                    || !self.game.economy().unit_visible_to(unit, HUMAN))
            {
                continue;
            }
            stacks.entry((at.x, at.y)).or_default().push(unit);
        }
        self.draw_road_plan(&screen_of, t);
        let blink = (get_time() * 3.0) as i64 % 2 == 0;
        for ((x, y), units) in stacks {
            let at = screen_of(x, y);
            self.draw_stack_icon(at, t, &units, blink);
        }
        // The stacks on their way from one square to the next.
        let sliding: Vec<(Vec2, Vec<hecs::Entity>)> = self
            .sliding(self.plane)
            .into_iter()
            .map(|(square, s)| (square, s.units.clone()))
            .collect();
        for (square, units) in sliding {
            let from = screen_of(square.x.floor() as u32, square.y.floor() as u32);
            let at = from + vec2(square.x.fract() * t.x, square.y.fract() * t.y);
            self.draw_stack_icon(at, t, &units, blink);
        }
    }

    /// One stack as one icon: its top unit (a selected one first) on the background of the
    /// owner's banner, the count when several stand together, and the blinking frame of the
    /// selection.
    fn draw_stack_icon(&mut self, at: Vec2, t: Vec2, units: &[hecs::Entity], blink: bool) {
        let top = units
            .iter()
            .copied()
            .find(|u| self.selection.contains(u))
            .unwrap_or(units[0]);
        let owner = self.game.units().unit_owner(top).map_or(u8::MAX, |o| o.0);
        let art = self.game.units().unit_def(top).and_then(|d| d.art.clone());
        // The stack's top unit on the background of its owner's banner
        // (`art::background_entry`), the figure one pixel in.
        let banner = self
            .banner_index(owner)
            .min(f4x_core::player::NEUTRAL_BANNER);
        let k = t.x / TILE_W;
        if let Some(tex) = self
            .art
            .picture("mapback", crate::art::background_entry(banner), 0)
        {
            draw_texture_ex(
                tex,
                at.x,
                at.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(crate::resolution::logical_size(tex) * k),
                    ..Default::default()
                },
            );
        }
        // An enchanted unit glows in its realm's colours; one of yours on patrol is greyed.
        let glow = self.unit_glow_realm(top);
        let patrolling = owner == HUMAN.0
            && self.game.units().unit_standby(top) == Some(f4x_core::unit::Standby::Patrol);
        let tint = if self.game.unit_in_stasis(top) {
            // A unit held in stasis is drawn pale and bluish.
            Color::new(0.65, 0.8, 1.0, 0.75)
        } else if patrolling {
            Color::new(0.6, 0.6, 0.6, 1.0)
        } else {
            WHITE
        };
        let icon = art.and_then(|(lbx, entry)| match glow {
            Some(realm) => self.art.unit_glow(
                &lbx,
                entry as usize,
                banner,
                realm,
                crate::map_slide::glow_step(),
            ),
            None => self.art.unit_picture(&lbx, entry as usize, banner),
        });
        match icon {
            Some(tex) => draw_texture_ex(
                tex,
                at.x + k,
                at.y + k,
                tint,
                DrawTextureParams {
                    dest_size: Some(crate::resolution::logical_size(tex) * k),
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
        if units.iter().any(|u| self.selection.contains(u)) && blink {
            draw_rectangle_lines(at.x, at.y, t.x, t.y, 3.0, GOLD);
        }
        if units.len() > 1 {
            let view = (at.x, at.y, k);
            let count = units.len().to_string();
            self.draw_styled(view, &Style::outlined(1).right(), &count, 19.0, 11.0);
        }
    }

    pub(crate) fn draw_bars(&mut self, input: &Input) {
        let click = self.draw_frame(input);
        self.unit_window_click(click);
        // Menu: Game, Spells, Armies, Cities, Magic, Info, Plane.
        match click.menu {
            Some(0) => self.open_slots(false),
            Some(1) => self.open_spellbook(),
            Some(2) => self.open_army_list(),
            Some(4) => self.open_magic(),
            Some(5) => self.open_advisor_menu(),
            Some(3) => self.open_city_list(),
            Some(6) => {
                self.plane = if self.plane == Plane::Arcanus {
                    Plane::Myrror
                } else {
                    Plane::Arcanus
                };
            }
            Some(_) => self.log.push(self.text("log.not_in_game")),
            None => {}
        }
        if click.next_turn || self.last_order_given() {
            self.request_end_turn();
        }

        // Information laid over the map: turn, selection, what is under the mouse, messages.
        let lay = crate::frame::Layout::new();
        let plane = self.text(if self.plane == Plane::Arcanus {
            "ui.arcanus"
        } else {
            "ui.myrror"
        });
        let view = (lay.origin.x, lay.origin.y, lay.scale);
        let mut y = 2.0;
        let turn = self.text_with(
            "ui.status_line",
            &Args::new()
                .count("turn", self.game.turn)
                .text("plane", plane),
        );
        self.overlay_line(view, &mut y, &turn, GOLD);
        if !self.art.available() {
            self.overlay_line(view, &mut y, &self.text("ui.no_art"), RED);
        }
        self.draw_selection(view, &mut y);
        self.draw_hover(view, &mut y, input);
        // Messages along the bottom of the map, newest last.
        let line = self.art.text_height(1) + 2.0;
        let bottom = spot::MAP.1 + spot::MAP.3 - 2.0;
        let messages = Color::new(0.95, 0.9, 0.65, 1.0);
        let shown: Vec<String> = self.log.iter().rev().take(5).cloned().collect();
        for (i, text) in shown.iter().enumerate() {
            let at = bottom - line * (i + 1) as f32;
            self.draw_styled(
                view,
                &Style::flat(1, messages).shadow(),
                text,
                spot::MAP.0 + 4.0,
                at,
            );
        }
    }

    /// One line of the overlay on the map, in the original's small font, and the next row.
    fn overlay_line(&mut self, view: (f32, f32, f32), y: &mut f32, text: &str, color: Color) {
        let at = (spot::MAP.0 + 4.0, spot::MAP.1 + *y);
        self.draw_styled(view, &Style::flat(1, color).shadow(), text, at.0, at.1);
        *y += self.art.text_height(1) + 2.0;
    }

    fn draw_selection(&mut self, view: (f32, f32, f32), y: &mut f32) {
        if self.selection.is_empty() {
            self.overlay_line(view, y, &self.text("ui.no_units_selected"), GRAY);
        }
    }

    /// What is under the mouse, a line each.
    fn hover_lines(&self, input: &Input) -> Vec<(String, Color)> {
        let Some(pos) = self.tile_at(input.mouse_position().into()) else {
            return Vec::new();
        };
        let content = self.game.content();
        let Some(map) = self.game.map() else {
            return Vec::new();
        };
        let grid = map.plane(self.plane);
        let at = Position {
            plane: self.plane,
            x: pos.x,
            y: pos.y,
        };
        if !self.game.is_explored(HUMAN, at) {
            return vec![(self.text("ui.unexplored"), GRAY)];
        }
        let tile = Args::new().count("x", pos.x).count("y", pos.y);
        let mut lines = vec![(self.text_with("ui.tile", &tile), GOLD)];
        if let Some(def) = grid
            .get(pos.x as i32, pos.y as i32)
            .and_then(|id| content.terrains.get(id))
        {
            let args = Args::new()
                .text("terrain", &def.name)
                .count("cost", def.move_cost);
            lines.push((self.text_with("ui.terrain_move", &args), WHITE));
        }
        if let Some(f) = self.game.economy().feature_at(pos) {
            lines.push((f.name.clone(), Color::new(0.6, 0.9, 1.0, 1.0)));
        }
        // What guards a site you have fought for once.
        if let Some(guards) = self.game.economy().lair_guards_known_to(at, HUMAN) {
            for (id, n) in guards {
                if let Some(u) = content.rules.units.by_name(&id) {
                    let args = Args::new().text("unit", &u.name).count("n", n);
                    lines.push((self.text_with("ui.lair_guard", &args), RED));
                }
            }
        }
        if let Some(city) = self.game.economy().city_at(pos) {
            let name = self.city_name(city);
            let pop = self.game.city_population(city).figures;
            let args = Args::new().text("city", name).count("size", pop);
            lines.push((self.text_with("ui.city_size", &args), GOLD));
        }
        let foreign = self
            .game
            .units()
            .units_at(pos)
            .into_iter()
            .filter(|&u| self.game.units().unit_owner(u) != Some(HUMAN));
        for u in foreign.take(4) {
            if let Some(def) = self.game.units().unit_def(u) {
                let args = Args::new().text("unit", &def.name);
                lines.push((self.text_with("ui.foreign_unit", &args), RED));
            }
        }
        lines
    }

    fn draw_hover(&mut self, view: (f32, f32, f32), y: &mut f32, input: &Input) {
        for (text, color) in self.hover_lines(input) {
            self.overlay_line(view, y, &text, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_without_a_wizard_has_the_neutral_banner() {
        let app = App::for_test();
        assert_eq!(app.banner_index(255), f4x_core::player::NEUTRAL_BANNER);
        let human = app.game.wizard(HUMAN).map(|w| w.banner());
        assert_eq!(Some(app.banner_index(HUMAN.0)), human);
        assert_ne!(app.banner_color(HUMAN.0), app.banner_color(255));
    }

    #[test]
    fn centring_on_a_square_puts_it_in_the_middle_of_the_map_and_never_above_the_top() {
        let mut app = App::for_test();
        let at = Position {
            plane: f4x_core::map::Plane::Myrror,
            x: 20,
            y: 1,
        };
        app.center_on(at);
        let (cols, rows) = app.visible_tiles();
        assert_eq!(app.plane, at.plane);
        assert_eq!(app.cam.x, 20.0 - cols / 2.0);
        assert_eq!(app.cam.y, (1.0 - rows / 2.0).max(0.0));
        assert!(app.cam.y >= 0.0);
    }

    #[test]
    fn the_first_cities_of_a_race_take_its_names_in_order() {
        let app = App::for_test();
        let race = &app.game.content().rules.races.defs()[app.race];
        let used = app.game.economy().cities().len();
        let names = &race.city_names;
        assert!(!names.is_empty());
        assert_eq!(app.next_city_name(), names[used % names.len()]);
    }
}
