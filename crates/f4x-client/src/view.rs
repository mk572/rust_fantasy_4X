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

use f4x_core::game::{Command, Event, Game};
use f4x_core::wizard::WizardBuild;
use macroquad::prelude::*;

use crate::art::{Art, TILE_H, TILE_W};
use crate::frame::{spot, Align};
use crate::options_screen::OptionsState;
use crate::preferences::Pref;
use crate::ui::GOLD;

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
    /// The player's preferences: the original's Settings screen, opened from the save slots
    /// (task 89).
    Settings,
    /// The settings of the game being played (read-only), or the options for the next game when
    /// there is none; opened from the Settings screen's Game button (task 94).
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
    /// The save slots: load, save, and leave the game.
    Slots,
    /// One unit's statistics sheet (`unit_screen.rs`).
    UnitSheet,
    /// The list of the wizard's stacks (`army_screen.rs`).
    ArmyList,
    /// The list of the wizard's cities (`city_list_screen.rs`).
    CityList,
    /// An outpost (`outpost_screen.rs`).
    Outpost(hecs::Entity),
    /// Another wizard's city (`enemy_city_screen.rs`).
    EnemyCity(hecs::Entity),
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
    /// The Chancellor's scroll, up after a turn change (task 272).
    pub(crate) chancellor: Option<crate::chancellor::Chancellor>,
    /// The player's preferences (the Settings screen), shared by every game.
    pub(crate) prefs: crate::preferences::Preferences,
    /// A list box over the map: the advisors, the tax rates (task 71).
    pub(crate) list_box: Option<crate::advisor::SelectionBox>,
    /// The question before the Grand Vizier is switched on.
    pub(crate) vizier_ask: bool,
    /// The plane the Cartographer shows.
    pub(crate) cart_plane: Plane,
    /// The news of the last turn change, for the Chancellor to show again.
    pub(crate) last_news: Vec<crate::chancellor::Section>,
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
    /// The question or refusal when a building of the city picture is clicked.
    /// The first enchantment shown on the city screen's list.
    pub(crate) city_spell_first: usize,
    /// Whether the city screen's map shows each square's food and production.
    pub(crate) city_area_figures: bool,
    /// The name typed so far while the city screen's rename box is up.
    pub(crate) city_naming: Option<String>,
    pub(crate) sell_prompt: Option<crate::sell_prompt::SellPrompt>,
    /// The auto-builder dialog over the city screen, while it is open.
    pub(crate) autobuild_dialog: Option<crate::autobuild_dialog::AutoDialog>,
    pub(crate) choice_scroll: usize,
    /// The magic screen's staves, locks and windows.
    pub(crate) magic_ui: crate::magic_screen::MagicUi,
    pub(crate) wizard_ui: crate::wizard_screen::WizardUi,
    /// The diplomacy screen's talk.
    pub(crate) diplo_ui: crate::diplomacy_screen::DiploUi,
    /// Wizards met whose greeting waits for the map: the screen opens for each in turn.
    pub(crate) greetings: std::collections::VecDeque<PlayerId>,
    /// Wizards met since the turn began: their greeting comes with the next turn.
    pub(crate) greetings_due: Vec<PlayerId>,
    /// The box that shows the offers made at the turn change (task 220).
    pub(crate) offer_ui: crate::offer_prompt::OfferUi,
    /// An attack the human started and has yet to confirm.
    pub(crate) attack_prompt: Option<crate::attack_prompt::PendingAttack>,
    /// The rival wizards, who play their turns when the human ends theirs (task 135).
    pub(crate) ais: Vec<f4x_ai::Ai>,
    /// Attacks on the human this turn, waiting to be fought one after the other.
    pub(crate) defences: std::collections::VecDeque<crate::attack_prompt::PendingAttack>,
    /// The turn is half over: the rivals have moved and the human is defending before it ends.
    pub(crate) turn_events: Option<Vec<Event>>,
    /// The small spell book's pages.
    pub(crate) book_ui: crate::spellbook::BookUi,
    /// The big spell book: the library, the research choice, spells just learned.
    pub(crate) big_ui: crate::bigbook::BigBookUi,
    /// A page turn of the spell books in progress (`page_turn.rs`).
    pub(crate) book_turn: Option<crate::page_turn::BookTurn>,
    /// The help topics (local only), which describe the spells.
    pub(crate) help: f4x_core::help::Help,
    /// The messages of the random events (`event_texts.ron`).
    pub(crate) event_texts: Vec<f4x_core::world_event::EventText>,
    /// Heroes died and their items were awarded: the warning box comes before the item screen.
    pub(crate) item_pool_warning: bool,
    /// The events the player has yet to be told of, oldest first.
    pub(crate) event_notices: std::collections::VecDeque<f4x_core::world_event::EventNotice>,
    /// The buildings the human's cities finished, to be announced, oldest first.
    /// The spells that fizzled or were dispelled, to be announced.
    /// Global enchantments cast and not yet shown in the mirror (`global_spell.rs`).
    pub(crate) global_casts: crate::global_spell::GlobalCasts,
    pub(crate) spell_notices: std::collections::VecDeque<crate::event_popup::SpellNotice>,
    pub(crate) built_notices:
        std::collections::VecDeque<(hecs::Entity, f4x_core::rules::BuildingId)>,
    /// A spell waiting for a target to be clicked on the map.
    pub(crate) targeting: Option<crate::spellbook::Targeting>,
    /// The turn is ending: 2 until one frame with the hourglass has been drawn, then 1 while
    /// the computer plays (`pointer.rs`).
    pub(crate) turn_pending: u8,
    /// Whether the selected stack can walk to the square under the pointer, for the boot.
    pub(crate) pointer_walk: Option<((Position, Vec<hecs::Entity>), bool)>,
    /// A road being planned (task 102).
    pub(crate) road_plan: Option<crate::road_plan::RoadPlan>,
    /// The Surveyor is describing the square under the mouse (F1, task 71).
    pub(crate) surveyor: bool,
    /// For screenshots: the square the Surveyor describes instead of the one under the mouse.
    pub(crate) survey_probe: Option<Position>,
    /// The key list is shown (F1 shows it while held).
    pub(crate) show_keys: bool,
    /// The wizard creation screen's state.
    pub(crate) create: crate::create_screen::CreateState,
    /// The human wizard chosen on the creation screen; new maps reuse it.
    pub(crate) human_build: Option<f4x_core::wizard::WizardBuild>,
    /// The battle screen's state.
    pub(crate) battle_ui: crate::battle_screen::BattleUi,
    /// The unit statistics sheet that is open, if any.
    pub(crate) sheet_ui: crate::unit_screen::SheetUi,
    /// The army list's scroll and the unit under the mouse.
    pub(crate) army_ui: crate::army_screen::ArmyUi,
    pub(crate) city_list_ui: crate::city_list_screen::CityListUi,
    pub(crate) outpost_ui: crate::outpost_screen::OutpostUi,
    /// The capital and the name typed so far, while the first-turn naming box is up.
    /// The units of the foreign stack whose list window is up.
    pub(crate) unit_list: Vec<hecs::Entity>,
    /// The site boxes waiting to be shown.
    pub(crate) site_notices: std::collections::VecDeque<crate::site_prompt::SiteNotice>,
    /// Items have just been awarded to the human wizard: the item screen opens when the boxes
    /// before it are closed (`Process_Item_Pool`: a lair's treasure, a gift, the merchant's item
    /// bought, an item made).
    pub(crate) item_pool_pending: bool,
    pub(crate) capital_naming: Option<(hecs::Entity, String)>,
    /// The test battle is fought at sea (`--battle-field sea`).
    pub(crate) test_sea: bool,
    pub(crate) hall: Vec<crate::hall_of_fame::Entry>,
    pub(crate) item_ui: crate::item_screen::ItemUi,
    pub(crate) craft_ui: Option<crate::craft_screen::CraftUi>,
    /// Where the selected units' lines were drawn last frame: a right-click on one opens the
    /// unit's sheet instead of sending the stack anywhere.
    /// The player has seen the game-over screen and chose to look at the map.
    pub(crate) game_over_seen: bool,
    pub(crate) menu: crate::menu_screen::MenuState,
    /// The text console's state (backquote key).
    pub(crate) console: crate::console_screen::ConsoleUi,
    /// The scroll that tells how a battle went, once it is over.
    pub(crate) combat_results: Option<crate::combat_result::CombatResults>,
    /// The battle last begun, for the scroll's paper.
    pub(crate) battle_site: Option<crate::combat_result::BattleSite>,
    /// The question after the human's fortress falls: resign, or cast the Spell of Return.
    pub(crate) resign_prompt: bool,
    /// The question of what to give up when the mana reserve cannot pay the upkeep.
    pub(crate) upkeep_prompt: Option<crate::upkeep_prompt::UpkeepPrompt>,
    /// A Spell Ward waiting for its realm: the spell, the city, and whether the spell is already
    /// paid for (`Targeting::resolve`).
    pub(crate) ward_prompt: Option<(f4x_core::rules::SpellId, hecs::Entity, bool)>,
    /// Disjunction or Spell Binding being aimed: the spell, whether it is already paid for, and
    /// the row of the rival's global enchantment chosen so far.
    /// The spell being cast has nothing to aim at: ask whether to stop casting it.
    pub(crate) no_target_prompt: Option<f4x_core::rules::SpellId>,
    pub(crate) global_prompt: Option<(f4x_core::rules::SpellId, bool, usize)>,
    /// Resurrection's list of the dead heroes: the spell, whether it is paid already, the row picked.
    pub(crate) resurrect_prompt: Option<(f4x_core::rules::SpellId, bool, usize)>,
    /// Wizards who started casting the Spell of Mastery, announced full screen one by one.
    pub(crate) mastery_notice: std::collections::VecDeque<f4x_core::player::PlayerId>,
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
            chancellor: None,
            list_box: None,
            vizier_ask: false,
            cart_plane: Plane::Arcanus,
            last_news: Vec::new(),
            seed,
            plane: Plane::Arcanus,
            cam: Vec2::ZERO,
            zoom: 4.0,
            race,
            screen: Screen::Map,
            selection: Vec::new(),
            log: Vec::new(),
            choosing: false,
            city_spell_first: 0,
            city_area_figures: false,
            city_naming: None,
            sell_prompt: None,
            autobuild_dialog: None,
            choice_scroll: 0,
            magic_ui: Default::default(),
            wizard_ui: Default::default(),
            diplo_ui: Default::default(),
            greetings: Default::default(),
            greetings_due: Vec::new(),
            offer_ui: Default::default(),
            attack_prompt: None,
            ais: Vec::new(),
            defences: Default::default(),
            turn_events: None,
            book_ui: Default::default(),
            book_turn: None,
            big_ui: Default::default(),
            help: Default::default(),
            event_texts: Vec::new(),
            item_pool_warning: false,
            event_notices: Default::default(),
            built_notices: Default::default(),
            global_casts: Default::default(),
            spell_notices: Default::default(),
            targeting: None,
            turn_pending: 0,
            pointer_walk: None,
            road_plan: None,
            surveyor: false,
            survey_probe: None,
            show_keys: false,
            create: crate::create_screen::CreateState::empty(),
            human_build: None,
            battle_ui: Default::default(),
            sheet_ui: Default::default(),
            army_ui: Default::default(),
            city_list_ui: Default::default(),
            outpost_ui: Default::default(),
            capital_naming: None,
            test_sea: false,
            site_notices: Default::default(),
            item_pool_pending: false,
            unit_list: Vec::new(),
            hall: Vec::new(),
            item_ui: Default::default(),
            craft_ui: None,
            game_over_seen: false,
            menu: Default::default(),
            console: Default::default(),
            combat_results: None,
            battle_site: None,
            resign_prompt: false,
            upkeep_prompt: None,
            ward_prompt: None,
            global_prompt: None,
            resurrect_prompt: None,
            no_target_prompt: None,
            mastery_notice: std::collections::VecDeque::new(),
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

    pub(crate) fn race_name(&self, race: usize) -> &str {
        &self.game.content().rules.races.defs()[race].name
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
            let line = match event {
                Event::EnchantmentDispelled {
                    victim,
                    spell,
                    on_unit,
                } => {
                    if victim == HUMAN {
                        self.announce_dispelled(spell, on_unit, &mut dispelled);
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
                    self.event_notices.push_back(notice);
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
                        self.attack_prompt = Some(crate::attack_prompt::PendingAttack {
                            attackers,
                            defenders,
                            defending: false,
                        });
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
                Event::WizardBanished { player, .. } if player == HUMAN => {
                    self.resign_prompt = true;
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
                        self.big_ui.learned.push_back(spell);
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
                    self.mastery_notice.push_back(caster);
                    let args = self.wizard_arg(Args::new(), "wizard", caster);
                    Some(self.text_with("log.mastery_started", &args))
                }
                Event::StasisEnded { .. } => Some(self.text("log.stasis_ended")),
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
                        self.no_target_prompt = Some(spell);
                    }
                    None
                }
                Event::SpellReady { player, spell } => {
                    if player == HUMAN {
                        self.aim_paid_spell(spell);
                    }
                    Some(self.text_with("log.spell_ready", &self.spell_arg(Args::new(), spell)))
                }
                Event::HeroJoined { kind, .. } => {
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
                Event::HeroSpellFailed { spell, .. } => Some(
                    self.text_with("log.hero_spell_failed", &self.spell_arg(Args::new(), spell)),
                ),
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
            self.city_spell_first = 0;
            self.screen = Screen::City(city);
        }
    }

    pub fn open_first_city(&mut self) {
        if let Some((city, ..)) = self.game.economy().cities().first() {
            self.city_spell_first = 0;
            self.screen = Screen::City(*city);
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
        self.seed = game.seed();
        // Keep playing the same race: the human player's capital race.
        if let Some(r) = game.wizard(HUMAN).and_then(|w| w.capital_race()) {
            self.race = r.index();
        }
        self.game = game;
        self.game.ensure_human(HUMAN);
        self.game.set_undo(true);
        self.ais = self.rival_wizards();
        self.defences.clear();
        // A game saved in a battle comes back in it: the frame loop opens the battle screen.
        self.battle_ui = crate::battle_screen::BattleUi::default();
        self.turn_events = None;
        self.game_over_seen = false;
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
        let mut events = Vec::new();
        for ai in &mut self.ais {
            events.extend(ai.play_turn(&mut self.game));
            for attack in ai.take_attacks() {
                self.defences
                    .push_back(crate::attack_prompt::PendingAttack {
                        attackers: attack.attackers,
                        defenders: attack.defenders,
                        defending: true,
                    });
            }
        }
        // The independent cities run themselves through their own AI (farmers, taxes, builds).
        events.extend(f4x_ai::play_neutrals(&mut self.game));
        self.turn_events = Some(events);
        self.resume_turn();
    }

    /// Go on with the turn change once no attack on the human waits: end the turn for everyone
    /// and report what the rivals and the turn did.
    pub(crate) fn resume_turn(&mut self) {
        if self.attack_prompt.is_some() || self.game.in_battle() {
            return;
        }
        // The next attack that is still real (a unit may have died to an earlier one).
        while let Some(attack) = self.defences.pop_front() {
            let alive = |u: &hecs::Entity| self.game.units().unit_owner(*u).is_some();
            let mine = attack
                .defenders
                .first()
                .is_some_and(|&u| self.game.units().unit_owner(u) == Some(HUMAN));
            if mine && attack.attackers.iter().all(alive) && attack.defenders.iter().all(alive) {
                self.attack_prompt = Some(attack);
                return;
            }
        }
        let Some(mut events) = self.turn_events.take() else {
            return;
        };
        if self.game.game_over.is_none() {
            events.extend(self.game.apply(Command::EndTurn));
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
                if let Some(attack) = self.attack_prompt.take() {
                    let _ = self.game.apply(Command::AutoResolve {
                        attackers: attack.attackers,
                        defenders: attack.defenders,
                    });
                }
                self.resume_turn();
            }
        }
        self.chancellor = None;
    }

    pub fn select_first_stack(&mut self) {
        if let Some(&u) = self.game.units().player_units(HUMAN).first() {
            if let Some(at) = self.game.units().unit_position(u) {
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
                self.seed = seed;
                self.game_over_seen = false;
                self.screen = Screen::Map;
                self.selection.clear();
                self.log.clear();
                self.start_game();
                self.offer_capital_naming();
                self.log
                    .push(self.text_with("log.new_map", &Args::new().count("seed", seed)));
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

    /// Handle input and draw one frame. Returns false when the player quits.
    pub fn frame(&mut self) -> bool {
        if self.turn_pending > 0 {
            self.run_pending_turn();
        }
        self.selection
            .retain(|&u| self.game.units().unit_position(u).is_some());
        if self.game.in_battle() {
            if !self.battle_ui.ready {
                self.init_battle_ui();
            }
            if !matches!(
                self.screen,
                Screen::Battle | Screen::Console | Screen::UnitSheet
            ) {
                self.screen = Screen::Battle;
            }
        } else {
            self.battle_ui.ready = false;
        }
        if is_key_pressed(KeyCode::GraveAccent) {
            self.toggle_console();
        }
        if self.combat_results.is_some()
            && !self.game.in_battle()
            && !matches!(self.screen, Screen::Console)
            && self.combat_results_frame()
        {
            return true;
        }
        if self.resign_prompt && !self.game.in_battle() && !matches!(self.screen, Screen::Console) {
            return self.resign_prompt_frame();
        }
        if self.upkeep_prompt.is_some()
            && !self.game.in_battle()
            && !matches!(self.screen, Screen::Console)
            && self.upkeep_prompt_frame()
        {
            return true;
        }
        if let Some(prompt) = self.ward_prompt {
            if !self.game.in_battle() && !matches!(self.screen, Screen::Console) {
                return self.ward_prompt_frame(prompt);
            }
        }
        if let Some(spell) = self.no_target_prompt {
            if !self.game.in_battle() && !matches!(self.screen, Screen::Console) {
                return self.no_target_frame(spell);
            }
        }
        if let Some(prompt) = self.resurrect_prompt {
            if !self.game.in_battle() && !matches!(self.screen, Screen::Console) {
                return self.resurrect_prompt_frame(prompt);
            }
        }
        if let Some(prompt) = self.global_prompt {
            if !self.game.in_battle() && !matches!(self.screen, Screen::Console) {
                return self.global_prompt_frame(prompt);
            }
        }
        if self.site_notice_frame() {
            return true;
        }
        if !self.unit_list.is_empty()
            && !self.game.in_battle()
            && matches!(self.screen, Screen::Map)
            && self.unit_list_frame()
        {
            return true;
        }
        if self.capital_naming.is_some()
            && !self.game.in_battle()
            && matches!(self.screen, Screen::Map)
            && self.capital_name_frame()
        {
            return true;
        }
        // A wizard has started casting the Spell of Mastery: announced over the whole screen.
        if !self.mastery_notice.is_empty()
            && !self.game.in_battle()
            && matches!(self.screen, Screen::Map)
            && self.mastery_notice_frame()
        {
            return true;
        }
        // A spell just learned is announced over the big book, and then the next research asked.
        if !self.big_ui.learned.is_empty()
            && !self.game.in_battle()
            && matches!(self.screen, Screen::Map)
            && self.learned_notice()
        {
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
            && self.attack_prompt_frame()
        {
            return true;
        }
        if matches!(self.screen, Screen::Map) && !self.game.in_battle() && self.event_popup_frame()
        {
            return true;
        }
        if matches!(self.screen, Screen::Map) && !self.game.in_battle() && self.global_cast_frame()
        {
            return true;
        }
        if matches!(self.screen, Screen::Map) && !self.game.in_battle() && self.spell_notice_frame()
        {
            return true;
        }
        // The offers made at the turn change wait over the map until they are answered.
        if matches!(self.screen, Screen::Map) && !self.game.in_battle() && self.offer_prompt_frame()
        {
            return true;
        }
        if matches!(self.screen, Screen::Map) && !self.game.in_battle() && self.chancellor_frame() {
            return true;
        }
        if matches!(self.screen, Screen::Map) && !self.game.in_battle() && self.built_popup_frame()
        {
            return true;
        }
        if matches!(self.screen, Screen::Map)
            && !self.game.in_battle()
            && (self.selection_frame() || self.vizier_ask_frame())
        {
            return true;
        }
        self.maybe_ask_for_research();
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
            Screen::Wizard => self.wizard_screen(),
            Screen::UnitSheet => self.unit_sheet_screen(),
            Screen::ArmyList => self.army_screen(),
            Screen::CityList => self.city_list_screen(),
            Screen::Outpost(city) => self.outpost_screen(city),
            Screen::EnemyCity(city) => self.enemy_city_screen(city),
            Screen::HallOfFame => self.hall_of_fame_screen(),
            Screen::Items => self.item_screen(),
            Screen::Craft => self.craft_screen(),
            Screen::Diplomacy => self.diplomacy_screen(),
            Screen::Spellbook => self.spellbook_screen(),
            Screen::Apprentice => self.apprentice_screen(),
            Screen::Research => self.research_screen(),
            Screen::Battle => self.battle_screen(),
            Screen::Create => self.create_screen(),
            Screen::Options => self.options_screen(),
            Screen::Cartographer => self.cartographer_screen(),
            Screen::History => self.history_screen(),
            Screen::Astrologer => self.astrologer_screen(),
            Screen::Settings => self.settings_screen(),
            Screen::GameSettings => self.game_settings_screen(),
            Screen::MainMenu => return self.main_menu_screen(),
            Screen::Slots => self.slots_screen(),
            Screen::Map => {
                if is_key_pressed(KeyCode::Escape) {
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
                self.overworld_input();
                clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
                self.draw_map();
                self.draw_bars();
                self.road_plan_bar();
                self.surveyor_bar();
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
            .filter(|(_, r)| !self.game.economy().city_warded_against(city, r))
            .collect();
        if is_key_pressed(KeyCode::Escape) || open.is_empty() {
            self.ward_prompt = None;
            self.log.push(self.text("log.cast_cancelled"));
        } else if let Some(&(_, realm)) = open.iter().find(|(k, _)| is_key_pressed(*k)) {
            self.ward_prompt = None;
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
        true
    }

    /// The cast has nothing to aim at: "There are no targets for X. Do you wish to continue
    /// casting it?" Yes keeps the spell (it is not paid for until a target exists), No gives it
    /// up.
    fn no_target_frame(&mut self, spell: f4x_core::rules::SpellId) -> bool {
        let casting = self.game.wizard(HUMAN).and_then(|m| m.casting());
        if casting.is_none_or(|c| c.spell != spell) {
            self.no_target_prompt = None;
            return true;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
        let lay = crate::frame::Layout::new();
        let name = self.game.content().rules.spells.get(spell).name.clone();
        let text = self.text_with("spell.no_target", &Args::new().text("spell", &name));
        match self.confirmation_box(&lay, &text) {
            Some(true) => self.no_target_prompt = None,
            Some(false) => {
                self.no_target_prompt = None;
                let events = self.game.apply(Command::CancelCast { player: HUMAN });
                self.note(events);
            }
            None => {}
        }
        true
    }

    /// The list of the rivals' global enchantments a Disjunction or Spell Binding can aim at,
    /// by wizard, with the spell's name and cost: Up and Down move, Enter or a click casts, and
    /// Escape gives up.
    fn global_prompt_frame(
        &mut self,
        (spell, resolve, row): (f4x_core::rules::SpellId, bool, usize),
    ) -> bool {
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
        let clicked = if is_mouse_button_pressed(MouseButton::Left) {
            let (mx, my) = mouse_position();
            (0..lines.len()).find(|&n| {
                let y = top + step * n as f32;
                (w / 2.0 - 260.0..w / 2.0 + 260.0).contains(&mx)
                    && (y - 20.0..y + 8.0).contains(&my)
            })
        } else {
            None
        };
        let mut pick = clicked.unwrap_or(row);
        if is_key_pressed(KeyCode::Up) {
            pick = pick.saturating_sub(1);
        }
        if is_key_pressed(KeyCode::Down) {
            pick = (pick + 1).min(lines.len().saturating_sub(1));
        }
        if is_key_pressed(KeyCode::Escape) || rows.is_empty() {
            self.global_prompt = None;
            self.log.push(self.text("log.cast_cancelled"));
        } else if clicked.is_some() || is_key_pressed(KeyCode::Enter) {
            self.global_prompt = None;
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
            self.global_prompt = Some((spell, resolve, pick));
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
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
        true
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

    /// The question after the human's fortress falls (`Banish_Wizard`): resign (Y) or cast the
    /// Spell of Return (N, Enter), shown over the map.
    fn resign_prompt_frame(&mut self) -> bool {
        let casting = self.game.wizard(HUMAN).and_then(|m| m.casting());
        let Some(casting) = casting.filter(|_| self.game.casting_return(HUMAN)) else {
            self.resign_prompt = false;
            return true;
        };
        if is_key_pressed(KeyCode::Y) {
            self.resign_prompt = false;
            let events = self.game.apply(Command::Resign { player: HUMAN });
            self.note(events);
        } else if is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::N)
            || is_key_pressed(KeyCode::Escape)
        {
            self.resign_prompt = false;
        }
        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));
        self.draw_map();
        self.draw_bars();
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
        true
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
    fn stack_keys(&mut self) {
        let Some(from) = self.selection_position() else {
            return;
        };
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let mut keys = vec![
            (KeyCode::Kp1, (-1, 1)),
            (KeyCode::Kp2, (0, 1)),
            (KeyCode::Kp3, (1, 1)),
            (KeyCode::Kp4, (-1, 0)),
            (KeyCode::Kp6, (1, 0)),
            (KeyCode::Kp7, (-1, -1)),
            (KeyCode::Kp8, (0, -1)),
            (KeyCode::Kp9, (1, -1)),
        ];
        if !shift {
            keys.extend([
                (KeyCode::Left, (-1, 0)),
                (KeyCode::Right, (1, 0)),
                (KeyCode::Up, (0, -1)),
                (KeyCode::Down, (0, 1)),
            ]);
        }
        let Some(&(_, (dx, dy))) = keys.iter().find(|(k, _)| is_key_pressed(*k)) else {
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
        self.selection = mine;
    }

    pub(crate) fn selection_position(&self) -> Option<Position> {
        self.selection
            .first()
            .and_then(|&u| self.game.units().unit_position(u))
    }

    fn overworld_input(&mut self) {
        let step = 12.0 * get_frame_time();
        let mut pan = Vec2::ZERO;
        // The arrow keys move the selected stack, as in the original; they scroll the map when
        // nothing is selected, or with Shift held.
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let scrolls = self.selection.is_empty() || shift;
        if scrolls {
            for (key, dir) in [
                (KeyCode::Left, vec2(-1.0, 0.0)),
                (KeyCode::Right, vec2(1.0, 0.0)),
                (KeyCode::Up, vec2(0.0, -1.0)),
                (KeyCode::Down, vec2(0.0, 1.0)),
            ] {
                if is_key_down(key) {
                    pan += dir;
                }
            }
        }
        self.cam += pan * step;
        if is_key_pressed(KeyCode::Tab) {
            self.toggle_plane();
        }
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            self.zoom = (self.zoom + 1.0).min(6.0);
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            self.zoom = (self.zoom - 1.0).max(1.0);
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::N) {
            self.request_end_turn();
        }
        let command_key = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::LeftSuper);
        if command_key && is_key_pressed(KeyCode::Z) {
            self.undo_last();
        }
        if is_key_pressed(KeyCode::F10) {
            self.save_to(Path::new(QUICKSAVE));
        }
        if command_key && is_key_pressed(KeyCode::L) {
            if let Err(e) = self.load_from(Path::new(QUICKSAVE)) {
                self.log.push(
                    self.text_with("log.load_failed", &Args::new().text("why", e.to_string())),
                );
            }
        }
        if !command_key && is_key_pressed(KeyCode::M) {
            self.open_magic();
        }
        if is_key_pressed(KeyCode::S) {
            self.open_spellbook();
        }
        if !command_key && is_key_pressed(KeyCode::A) {
            self.open_army_list();
        }
        if is_key_pressed(KeyCode::G) {
            self.open_slots(false);
        }
        for (key, advisor) in [
            (KeyCode::F1, 0),
            (KeyCode::F2, 1),
            (KeyCode::F3, 2),
            (KeyCode::F4, 3),
            (KeyCode::F5, 4),
            (KeyCode::F6, 5),
            (KeyCode::F7, 6),
            (KeyCode::F8, 7),
            (KeyCode::F9, 8),
        ] {
            if is_key_pressed(key) {
                self.open_advisor(advisor);
            }
        }
        if is_key_pressed(KeyCode::I) && !command_key {
            self.open_advisor_menu();
        }
        if command_key && is_key_pressed(KeyCode::N) {
            self.new_map(self.seed + 1);
        }
        if command_key && is_key_pressed(KeyCode::R) {
            self.race = (self.race + 1) % self.game.content().rules.races.len().max(1);
            self.new_map(self.seed);
            let name = self.race_name(self.race).to_string();
            self.log
                .push(self.text_with("log.playing_as", &Args::new().text("race", name)));
        }
        self.road_plan_keys();
        if is_key_pressed(KeyCode::B) && self.road_plan.is_some() {
            self.road_plan_confirm();
        } else if is_key_pressed(KeyCode::B) {
            self.special_button();
        }
        if is_key_pressed(KeyCode::Q) || is_key_pressed(KeyCode::U) {
            self.selection.clear();
        }
        if is_key_pressed(KeyCode::P) {
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
        self.stack_keys();
        if is_key_pressed(KeyCode::T) {
            self.game.balance_farmers(HUMAN);
            self.log.push(self.text("log.farmers_balanced"));
        }
        if is_key_pressed(KeyCode::W) && !self.selection.is_empty() {
            self.select_next_idle(true);
        }
        if is_key_pressed(KeyCode::D) && !self.selection.is_empty() {
            self.game.apply(Command::ClearOrders {
                units: self.selection.clone(),
            });
        }
        if is_key_pressed(KeyCode::C) {
            if let Some(at) = self.selection_position() {
                self.center_on(at);
            }
        }
        self.clamp_camera();

        let hovered = self.tile_at(mouse_position().into());
        if let Some(pos) = hovered {
            if is_mouse_button_pressed(MouseButton::Left) {
                self.click(pos);
            }
            if is_mouse_button_pressed(MouseButton::Right) && self.surveyor {
                self.stop_surveyor();
            } else if is_mouse_button_pressed(MouseButton::Right) && self.road_plan.is_some() {
                self.road_plan_cancel();
            } else if is_mouse_button_pressed(MouseButton::Right)
                && self.game.is_explored(HUMAN, pos)
                && self.foreign_stack_at(pos)
                && self.open_unit_list(pos)
            {
            } else if is_mouse_button_pressed(MouseButton::Right)
                && !self.selection.is_empty()
                && !self.prefs.get(Pref::LeftClickMoves)
            {
                self.go_to(pos);
            }
        }
    }

    /// Send the selected units to `pos` (the click that moves units is the preference
    /// `Left Click Moves Units`).
    fn go_to(&mut self, pos: Position) {
        let events = self.game.apply(Command::GoTo {
            units: self.selection.clone(),
            to: pos,
        });
        self.note(events);
    }

    fn click(&mut self, pos: Position) {
        if let Some(t) = self.targeting.take() {
            self.aim_at(t, pos);
            return;
        }
        if self.road_plan.is_some() {
            if !self.road_plan_on_button() {
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
                    self.prefs.get(crate::preferences::Pref::ShowNodeOwners),
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
        for unit in self.game.units().all_units() {
            let Some(at) = self.game.units().unit_position(unit) else {
                continue;
            };
            if at.plane != self.plane {
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
            let top = units
                .iter()
                .copied()
                .find(|u| self.selection.contains(u))
                .unwrap_or(units[0]);
            let owner = self.game.units().unit_owner(top).map_or(u8::MAX, |o| o.0);
            let art = self.game.units().unit_def(top).and_then(|d| d.art.clone());
            let at = screen_of(x, y);
            // The stack's top unit on the background of its owner's banner (`mapback` 14 to 19:
            // blue, green, purple, red, yellow, neutral), the figure one pixel in.
            let banner = self.banner_index(owner).min(5);
            let k = t.x / TILE_W;
            if let Some(tex) = self.art.picture("mapback", 14 + usize::from(banner), 0) {
                draw_texture_ex(
                    tex,
                    at.x,
                    at.y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(tex.size() * k),
                        ..Default::default()
                    },
                );
            }
            let icon =
                art.and_then(|(lbx, entry)| self.art.unit_picture(&lbx, entry as usize, banner));
            match icon {
                Some(tex) => draw_texture_ex(
                    tex,
                    at.x + k,
                    at.y + k,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(tex.size() * k),
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
                self.bm_text_outlined(view, 1, &count, 19.0, 11.0, Align::Right);
            }
        }
    }

    pub(crate) fn draw_bars(&mut self) {
        let click = self.draw_frame();
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
        if click.next_turn {
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
                .text("plane", plane)
                .text("race", self.race_name(self.race)),
        );
        self.overlay_line(view, &mut y, &turn, GOLD);
        if !self.art.available() {
            self.overlay_line(view, &mut y, &self.text("ui.no_art"), RED);
        }
        self.draw_selection(view, &mut y);
        self.draw_hover(view, &mut y);
        // Messages along the bottom of the map, newest last.
        let line = self.art.text_height(1) + 2.0;
        let bottom = spot::MAP.1 + spot::MAP.3 - 2.0;
        let messages = Color::new(0.95, 0.9, 0.65, 1.0);
        let shown: Vec<String> = self.log.iter().rev().take(5).cloned().collect();
        for (i, text) in shown.iter().enumerate() {
            let at = bottom - line * (i + 1) as f32;
            self.bm_text_shadowed(view, 1, text, spot::MAP.0 + 4.0, at, messages, Align::Left);
        }
        let hint = self.text("ui.keys_hint");
        let hint_x = spot::MAP.0 + spot::MAP.2 - 4.0;
        self.bm_text_shadowed(
            view,
            1,
            &hint,
            hint_x,
            bottom - line,
            LIGHTGRAY,
            Align::Right,
        );
        if self.show_keys || is_key_down(KeyCode::F1) {
            let keys = [
                self.text("ui.keys_1"),
                self.text("ui.keys_2"),
                self.text("ui.keys_3"),
                self.text("ui.keys_4"),
            ];
            let width = spot::MAP.2 * 0.9 - 8.0;
            let rows: Vec<String> = keys
                .iter()
                .flat_map(|k| self.bm_wrap(1, k, width))
                .collect();
            let (left, top) = (spot::MAP.0 + spot::MAP.2 * 0.05, spot::MAP.1 + 50.0);
            let r = lay.rect((left, top, spot::MAP.2 * 0.9, line * rows.len() as f32 + 6.0));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.8));
            for (i, k) in rows.iter().enumerate() {
                let at = top + 3.0 + line * i as f32;
                self.bm_text(view, 1, WHITE, k, left + 4.0, at, Align::Left);
            }
        }
    }

    /// One line of the overlay on the map, in the original's small font, and the next row.
    fn overlay_line(&mut self, view: (f32, f32, f32), y: &mut f32, text: &str, color: Color) {
        let at = (spot::MAP.0 + 4.0, spot::MAP.1 + *y);
        self.bm_text_shadowed(view, 1, text, at.0, at.1, color, Align::Left);
        *y += self.art.text_height(1) + 2.0;
    }

    fn draw_selection(&mut self, view: (f32, f32, f32), y: &mut f32) {
        if self.selection.is_empty() {
            self.overlay_line(view, y, &self.text("ui.no_units_selected"), GRAY);
        }
    }

    /// What is under the mouse, a line each.
    fn hover_lines(&self) -> Vec<(String, Color)> {
        let Some(pos) = self.tile_at(mouse_position().into()) else {
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

    fn draw_hover(&mut self, view: (f32, f32, f32), y: &mut f32) {
        for (text, color) in self.hover_lines() {
            self.overlay_line(view, y, &text, color);
        }
    }
}
