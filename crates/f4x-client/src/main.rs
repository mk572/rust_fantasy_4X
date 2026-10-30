//! The desktop client: reads the core's state and draws it, never mutating the
//! world except by sending commands.

mod advisor;
mod advisor_screen;
mod army_screen;
mod art;
mod attack_prompt;
mod audio;
mod autobuild_dialog;
mod battle_anim;
mod battle_ground;
mod battle_info;
mod battle_screen;
mod bigbook;
mod built_popup;
mod chancellor;
mod city_list_screen;
mod city_screen;
mod city_scroll;
mod cityscape;
mod combat_result;
mod console_screen;
mod craft_screen;
mod create_screen;
mod cycle;
mod diplomacy_prompt;
mod diplomacy_screen;
mod diplomacy_text;
mod end_scene;
mod enemy_city_screen;
mod event_popup;
mod fled_notice;
mod font;
mod frame;
mod game_over_screen;
mod global_spell;
mod hall_of_fame;
mod help_popup;
mod hotkeys;
mod input;
mod intro_screen;
mod item_screen;
mod learn_spell;
mod level_up;
mod magic_screen;
mod map_draw;
mod map_slide;
mod menu_screen;
mod modal;
mod newgame_flow;
mod newgame_picks;
mod offer_prompt;
mod options_screen;
mod outpost_screen;
mod page;
mod page_turn;
mod pointer;
mod preferences;
mod production_screen;
mod resurrect_prompt;
mod river_art;
mod road_plan;
mod sell_prompt;
mod settings_screen;
mod site_prompt;
mod spell_fx;
mod spell_sight;
mod spellbook;
mod square_anim;
mod summon_scene;
mod surveyor;
mod temp_effect_view;
mod terrain_pick;
#[cfg(test)]
mod test_app;
mod ui;
mod unit_list_screen;
mod unit_screen;
mod unit_sheet;
mod unit_window;
mod upkeep_prompt;
mod view;
mod wizard_anim;
mod wizard_screen;

use std::path::{Path, PathBuf};

use input::Input;
use macroquad::prelude::*;

use view::App;

struct Options {
    content: PathBuf,
    assets: PathBuf,
    seed: u64,
    /// `--seed` was given: every new game uses it. Without it each game gets a seed of its own.
    seed_given: bool,
    /// Save a screenshot after a few frames and quit (for automated checks).
    shot: Option<PathBuf>,
    /// With `--shot`, open the city screen first.
    city: bool,
    /// With `--shot --city`, open the auto-builder dialog over it.
    autobuild: bool,
    /// With `--shot --city`, open the question to sell the city's first building.
    sell: bool,
    /// With `--shot --city`, open the list to choose what to build.
    choose: bool,
    /// With `--shot`, open the magic screen first.
    magic: bool,
    /// With `--shot`, open this wizard's sheet first (0 is you; a rival's shows as if met).
    wizard: Option<u8>,
    /// With `--shot`, open the diplomacy screen with the first rival (`--talk MENU`, `--mood N`).
    talk: Option<String>,
    /// With `--talk`, make the relation about this much (-100 to 100).
    mood: i32,
    /// With `--shot`, open the spell book, the Apprentice or the research choice first.
    spells: bool,
    /// With `--spells`, the filter of the book (all, known, overland or battle) and its sort (name or cost).
    spells_view: Option<String>,
    apprentice: bool,
    research: bool,
    /// With `--shot`, warp the magic nodes and show the first one.
    warp_nodes: bool,
    /// With `--shot`, show this scene: win, lose, conquest or return.
    scene: Option<String>,
    /// With `--shot`, take it this many seconds after the start instead of at the third frame.
    shot_after: Option<f64>,
    /// With `--shot`, explore the whole map first.
    reveal: bool,
    /// With `--shot`, put the top-left of the map view at this tile.
    at: Option<(f32, f32)>,
    /// Open the wizard creation screen at once, skipping the start menu.
    create: bool,
    create_step: Option<String>,
    /// With `--shot`, show the start menu.
    menu: bool,
    /// With `--shot`, show the new-game options.
    options: bool,
    /// With `--shot`, show the save slots (as if opened from the start menu).
    slots: bool,
    /// With `--shot`, show the Settings screen.
    settings: bool,
    /// With `--shot`, open an advisor: 0 to 8 for the advisors, 9 for the list.
    advisor: Option<usize>,
    /// With `--shot`, ask how much mana an automatic fight may spend.
    auto_attack: bool,
    attack_box: bool,
    /// With `--shot`, open the help for this point of the screen (320 by 200 coordinates).
    help_at: Option<(i32, i32)>,
    /// With `--shot --city`, open the description of this building (an id) in a help pop-up.
    help_building: Option<String>,
    /// With `--shot`, show this scene of the opening movie (0 to 12), at full brightness.
    intro: Option<usize>,
    /// With `--advisor 0`: describe the square at this column and row.
    survey_at: Option<(u32, u32)>,
    /// With `--shot`, show the Chancellor's scroll with made-up news (`--long` for a tall one).
    scroll: Option<bool>,
    /// Skip the creation screen and play the default wizard.
    quick: bool,
    /// With `--shot`, fight a test battle first.
    battle: bool,
    /// With `--shot`, select the starting stack first.
    select: bool,
    /// With `--shot`, open the army list first.
    armies: bool,
    cities: bool,
    outpost: Option<bool>,
    capital_name: bool,
    unit_list: bool,
    site: Option<String>,
    battle_site: Option<String>,
    battle_field: Option<String>,
    enemy_city: bool,
    score: bool,
    items: bool,
    hall: bool,
    /// With `--shot`, run the human out of gold and mana and open the upkeep prompt.
    upkeep: bool,
    /// With `--shot`, make the human wizard this offer first: `hero`, `mercenaries` or `item`.
    offer: Option<String>,
    event: Option<String>,
    /// With `--shot`, announce this finished building (an id).
    built: Option<String>,
    /// With `--shot --city`, dress the city: spells, `ward:REALM`, `sold`, `built:ID`.
    city_fx: Option<String>,
    /// Announce this spell (an id) as just learned; `--learn-at` starts the animation that many seconds in.
    learn: Option<String>,
    /// With `--spells` or `--apprentice`: show a page turn (`forward` or `back`) `--turn-at` seconds in.
    turn: Option<String>,
    turn_at: f64,
    /// Show this global spell (an id) being cast, by `--global-by` (a player number, 0 you) and `--global-at` seconds in.
    global: Option<String>,
    /// Show the summon scene of this spell (an id), by `--global-by` and `--global-at`.
    summon: Option<String>,
    /// Show the city-spell scroll of this spell (an id) on the first city, by `--global-by` and `--global-at`.
    city_scroll: Option<String>,
    /// Show the square animation of this spell (an id) on the first city, `--global-at` seconds in.
    square: Option<String>,
    /// Show the animation of this spell (an id) on wizard `--global-by`, `--global-at` seconds in.
    wizard_spell: Option<String>,
    global_by: u8,
    global_at: f64,
    learn_at: f64,
    /// With `--shot`, show the Spell of Mastery announcement of `--global-by`, `--global-at` seconds in.
    mastery: bool,
    /// With `--shot`, show the box of a spell that fizzled.
    fizzle: bool,
    /// With `--shot`, show the box that announces a hero's new level.
    level_up: bool,
    /// With `--shot`, show the notices of a dispel: `city`, `unit`, `battle` or `field`.
    dispelled: Option<String>,
    /// With `--shot`, show the combat results scroll: `won`, `lost`, `fled`, `exhausted`, `rampage`, `ruins`, `raiders`, `city` or `raze`.
    results: Option<String>,
    /// With `--shot`, open the sheet of this unit of the stack (or of the test battle).
    sheet: Option<usize>,
    /// With `--sheet`, first put this lasting spell (an id) on the unit, cast by you.
    sheet_spell: Option<String>,
    /// With `--sheet-spell`, also click the spell's line so the question is open.
    sheet_ask: bool,
    /// With `--shot`, end this many turns first.
    turns: u32,
    /// Where the original's sounds are (`f4x-tools extract-audio`); none are played with `--shot`.
    audio: PathBuf,
    /// Play no sound.
    mute: bool,
    /// Play one sound at start-up and quit after three seconds: `--hear soundfx:2`.
    hear: Option<(audio::Archive, u16)>,
    /// Start from a saved game.
    load: Option<PathBuf>,
    /// Save the game (after any `--turns`) before carrying on.
    save: Option<PathBuf>,
}

fn parse_args() -> Result<Options, String> {
    let mut o = Options {
        content: if Path::new("content-local").exists() {
            "content-local".into()
        } else {
            "content".into()
        },
        assets: "assets-local/art".into(),
        audio: "assets-local/audio".into(),
        mute: false,
        hear: None,
        seed: 1,
        seed_given: false,
        shot: None,
        city: false,
        autobuild: false,
        sell: false,
        choose: false,
        warp_nodes: false,
        scene: None,
        shot_after: None,
        magic: false,
        wizard: None,
        talk: None,
        mood: 0,
        spells: false,
        spells_view: None,
        apprentice: false,
        research: false,
        create: false,
        create_step: None,
        menu: false,
        options: false,
        slots: false,
        settings: false,
        advisor: None,
        intro: None,
        auto_attack: false,
        attack_box: false,
        help_at: None,
        help_building: None,
        survey_at: None,
        scroll: None,
        reveal: false,
        at: None,
        quick: false,
        select: false,
        battle: false,
        turns: 0,
        load: None,
        save: None,
        sheet: None,
        sheet_spell: None,
        sheet_ask: false,
        armies: false,
        cities: false,
        outpost: None,
        capital_name: false,
        unit_list: false,
        site: None,
        battle_site: None,
        battle_field: None,
        enemy_city: false,
        score: false,
        items: false,
        hall: false,
        offer: None,
        event: None,
        built: None,
        city_fx: None,
        learn: None,
        turn: None,
        turn_at: 0.0,
        global: None,
        summon: None,
        city_scroll: None,
        square: None,
        wizard_spell: None,
        global_by: 0,
        global_at: 0.0,
        learn_at: 0.0,
        mastery: false,
        fizzle: false,
        level_up: false,
        dispelled: None,
        results: None,
        upkeep: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--content" => o.content = value("--content")?.into(),
            "--assets" => o.assets = value("--assets")?.into(),
            "--audio" => o.audio = value("--audio")?.into(),
            "--mute" => o.mute = true,
            "--hear" => {
                let v = value("--hear")?;
                o.hear = v
                    .split_once(':')
                    .and_then(|(a, n)| Some((audio::Archive::named(a)?, n.parse().ok()?)));
                if o.hear.is_none() {
                    return Err("--hear takes ARCHIVE:ENTRY, e.g. soundfx:2".into());
                }
            }
            "--seed" => {
                o.seed_given = true;
                o.seed = value("--seed")?
                    .parse()
                    .map_err(|_| "--seed must be a number")?
            }
            "--shot" => o.shot = Some(value("--shot")?.into()),
            "--city" => o.city = true,
            "--autobuild" => o.autobuild = true,
            "--sell" => o.sell = true,
            "--choose" => o.choose = true,
            "--warp-nodes" => o.warp_nodes = true,
            "--scene" => o.scene = Some(value("--scene")?),
            "--shot-after" => {
                o.shot_after = Some(
                    value("--shot-after")?
                        .parse()
                        .map_err(|_| "--shot-after must be a number of seconds")?,
                )
            }
            "--select" => o.select = true,
            "--armies" => o.armies = true,
            "--cities" => o.cities = true,
            "--score" => o.score = true,
            "--items" => o.items = true,
            "--hall" => o.hall = true,
            "--enemy-city" => o.enemy_city = true,
            "--capital-name" => o.capital_name = true,
            "--unit-list" => o.unit_list = true,
            "--battle-site" => o.battle_site = Some(value("--battle-site")?),
            "--battle-field" => o.battle_field = Some(value("--battle-field")?),
            "--site" => o.site = Some(value("--site")?),
            "--outpost" => o.outpost = Some(false),
            "--outpost-new" => o.outpost = Some(true),
            "--upkeep" => o.upkeep = true,
            "--offer" => o.offer = Some(value("--offer")?),
            "--event" => o.event = Some(value("--event")?),
            "--dispelled" => o.dispelled = Some(value("--dispelled")?),
            "--fizzle" => o.fizzle = true,
            "--mastery" => o.mastery = true,
            "--level-up" => o.level_up = true,
            "--built" => o.built = Some(value("--built")?),
            "--city-fx" => o.city_fx = Some(value("--city-fx")?),
            "--learn" => o.learn = Some(value("--learn")?),
            "--turn" => o.turn = Some(value("--turn")?),
            "--turn-at" => {
                o.turn_at = value("--turn-at")?
                    .parse()
                    .map_err(|_| "--turn-at must be a number of seconds")?
            }
            "--global" => o.global = Some(value("--global")?),
            "--summon" => o.summon = Some(value("--summon")?),
            "--square" => o.square = Some(value("--square")?),
            "--wizard-spell" => o.wizard_spell = Some(value("--wizard-spell")?),
            "--city-scroll" => o.city_scroll = Some(value("--city-scroll")?),
            "--global-by" => {
                o.global_by = value("--global-by")?
                    .parse()
                    .map_err(|_| "--global-by must be a player number")?
            }
            "--global-at" => {
                o.global_at = value("--global-at")?
                    .parse()
                    .map_err(|_| "--global-at must be a number of seconds")?
            }
            "--learn-at" => {
                o.learn_at = value("--learn-at")?
                    .parse()
                    .map_err(|_| "--learn-at must be a number of seconds")?
            }
            "--results" => o.results = Some(value("--results")?),
            "--sheet" => {
                o.sheet = Some(
                    value("--sheet")?
                        .parse()
                        .map_err(|_| "--sheet must be a unit number")?,
                )
            }
            "--sheet-spell" => o.sheet_spell = Some(value("--sheet-spell")?),
            "--sheet-ask" => o.sheet_ask = true,
            "--battle" => o.battle = true,
            "--magic" => o.magic = true,
            "--wizard" => {
                o.wizard = Some(
                    value("--wizard")?
                        .parse()
                        .map_err(|_| "--wizard must be a player number")?,
                )
            }
            "--talk" => o.talk = Some(value("--talk")?),
            "--mood" => {
                o.mood = value("--mood")?
                    .parse()
                    .map_err(|_| "--mood must be a number")?
            }
            "--spells" => o.spells = true,
            "--spells-view" => o.spells_view = Some(value("--spells-view")?),
            "--apprentice" => o.apprentice = true,
            "--research" => o.research = true,
            "--create" => o.create = true,
            "--create-step" => {
                o.create = true;
                o.create_step = Some(value("--create-step")?);
            }
            "--menu" => o.menu = true,
            "--options" => o.options = true,
            "--slots" => o.slots = true,
            "--settings" => o.settings = true,
            "--survey-at" => {
                let v = value("--survey-at")?;
                // "city" is the first city's square.
                o.survey_at = if v == "city" {
                    Some((u32::MAX, 0))
                } else {
                    v.split_once(',')
                        .and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))
                };
                if o.survey_at.is_none() {
                    return Err("--survey-at takes X,Y or city".into());
                }
            }
            "--advisor" => {
                o.advisor = Some(
                    value("--advisor")?
                        .parse()
                        .map_err(|_| "--advisor must be a number, 0 to 9")?,
                )
            }
            "--auto-attack" => o.auto_attack = true,
            "--attack-box" => o.attack_box = true,
            "--help-at" => {
                let v = value("--help-at")?;
                let (x, y) = v.split_once(',').ok_or("--help-at takes X,Y")?;
                o.help_at = Some((
                    x.parse().map_err(|_| "--help-at takes X,Y")?,
                    y.parse().map_err(|_| "--help-at takes X,Y")?,
                ));
            }
            "--help-building" => o.help_building = Some(value("--help-building")?),
            "--intro" => {
                o.intro = Some(
                    value("--intro")?
                        .parse()
                        .map_err(|_| "--intro must be a scene number, 0 to 12")?,
                )
            }
            "--scroll" => o.scroll = Some(false),
            "--scroll-long" => o.scroll = Some(true),
            "--reveal" => o.reveal = true,
            "--at" => {
                let v = value("--at")?;
                let (x, y) = v.split_once(',').ok_or("--at wants X,Y")?;
                o.at = Some((
                    x.parse().map_err(|_| "--at X is a number")?,
                    y.parse().map_err(|_| "--at Y is a number")?,
                ));
            }
            "--quick" => o.quick = true,
            "--load" => o.load = Some(value("--load")?.into()),
            "--save" => o.save = Some(value("--save")?.into()),
            "--turns" => {
                o.turns = value("--turns")?
                    .parse()
                    .map_err(|_| "--turns must be a number")?
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(o)
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Fantasy 4X".to_string(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let options = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}\nusage: f4x-client [--content DIR] [--assets DIR] [--audio DIR] [--mute] [--hear ARCHIVE:ENTRY] [--seed N] [--load FILE] [--save FILE] [--shot FILE [--city [--autobuild|--sell|--choose] [--city-fx LIST]] [--warp-nodes] [--scene win|lose|conquest|return|parade] [--shot-after SECONDS] [--magic] [--wizard N] [--talk MENU [--mood N]] [--spells [--spells-view all|known|overland|battle[,cost]]] [--apprentice] [--research] [--create] [--create-step wizard|portrait|name|picks|spells|race|banner|custom-NAME] [--options] [--menu] [--slots] [--settings] [--advisor N [--survey-at X,Y]] [--scroll|--scroll-long] [--quick] [--select] [--armies] [--cities] [--outpost|--outpost-new] [--capital-name] [--unit-list] [--site ask|found|treasure] [--battle-site FEATURE] [--enemy-city] [--score] [--items] [--hall] [--offer KIND] [--event KIND] [--fizzle] [--mastery [--global-by N] [--global-at SECONDS]] [--level-up] [--dispelled city|unit|battle|field] [--built ID] [--learn SPELL [--learn-at SECONDS]] [--turn forward|back [--turn-at SECONDS]] [--global SPELL [--global-by N] [--global-at SECONDS]] [--summon SPELL [--global-by N] [--global-at SECONDS]] [--square SPELL [--global-at SECONDS]] [--wizard-spell SPELL [--global-by N] [--global-at SECONDS]] [--city-scroll SPELL [--global-by N] [--global-at SECONDS]] [--results won|lost|fled|exhausted|rampage|ruins|raiders|city|raze] [--upkeep] [--attack-box] [--sheet N [--sheet-spell ID [--sheet-ask]]] [--turns N]]");
            std::process::exit(2);
        }
    };
    let content = match f4x_data::load_content_layers(&[&options.content, Path::new("content")]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "could not load content from {}: {e}",
                options.content.display()
            );
            std::process::exit(1);
        }
    };
    let help = f4x_data::load_help(Path::new("content")).unwrap_or_default();
    let spell_art = match f4x_data::spell_art::load_spell_art(
        &[&options.content, Path::new("content")],
        &content.rules,
    ) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("could not load the spell art table: {e}");
            std::process::exit(1);
        }
    };
    let sounds =
        f4x_data::sound::load_sound(&[&options.content, Path::new("content")], &content.rules)
            .unwrap_or_else(|e| {
                eprintln!("could not load the sound table: {e}");
                std::process::exit(1);
            });
    let mut app = match App::new(
        content,
        art::Art::load(options.assets, spell_art),
        if options.seed_given {
            options.seed
        } else {
            view::fresh_seed()
        },
    ) {
        Ok(mut app) => {
            app.seed_fixed = options.seed_given;
            app
        }
        Err(e) => {
            eprintln!("could not start a game: {e}");
            std::process::exit(1);
        }
    };
    app.help = help;
    match hotkeys::Hotkeys::load(&[&options.content, Path::new("saves")]) {
        Ok(keys) => app.hotkeys = keys,
        Err(e) => eprintln!("{e}; the shipped keys are used"),
    }
    if !options.mute && options.shot.is_none() {
        app.audio = audio::Audio::scan(&options.audio, sounds);
        if !app.audio.has_files() {
            eprintln!(
                "no sound files in {}: the game is silent (f4x-tools extract-audio)",
                options.audio.display()
            );
        }
    }
    app.event_texts = f4x_data::load_event_texts(&options.content).unwrap_or_default();
    app.diplomsg = f4x_data::load_diplomacy_lines(&options.content).unwrap_or_default();
    app.prefs = preferences::Preferences::load(Path::new(preferences::FILE), app.prefs.clone());
    if let Some(path) = &options.load {
        if let Err(e) = app.load_from(path) {
            eprintln!("could not load {}: {e}", path.display());
            std::process::exit(1);
        }
    }
    app.end_turns(options.turns);
    if let Some(path) = &options.save {
        app.save_to(path);
    }
    if options.select {
        app.select_first_stack();
    }
    if options.city {
        app.open_first_city();
        if options.autobuild {
            app.open_test_autobuild();
        }
        if options.sell {
            app.open_test_sale();
        }
        if options.choose {
            app.open_test_production();
        }
        if let Some(list) = &options.city_fx {
            app.open_test_city_fx(list);
        }
    }
    if options.reveal {
        app.reveal_all();
    }
    if options.warp_nodes {
        app.open_test_warped_nodes();
    }
    if let Some(name) = &options.scene {
        app.open_test_scene(name);
    }
    if let Some((x, y)) = options.at {
        app.set_camera(x, y);
    }
    if options.magic {
        app.open_magic();
    }
    if let Some(who) = options.wizard {
        app.open_test_wizard(who);
    }
    if let Some(menu) = &options.talk {
        app.open_test_talk(menu, options.mood);
    }
    if options.spells {
        if let Some(view) = &options.spells_view {
            app.test_book_view(view);
        }
        app.open_spellbook();
    }
    if options.apprentice {
        app.open_apprentice();
    }
    if options.research {
        app.open_research_choice();
    }
    if let Some(way) = &options.turn {
        app.open_test_page_turn(way == "forward", options.turn_at);
    }
    if let Some(id) = &options.battle_site {
        app.set_test_feature(id);
    }
    if let Some(kind) = &options.battle_field {
        app.set_test_field(kind);
    }
    if options.battle {
        app.open_test_battle();
    }
    if let Some(kind) = &options.battle_field {
        app.set_test_effects(kind);
    }
    if options.armies {
        app.open_army_list();
    }
    if options.items {
        app.open_item_screen();
    }
    if options.score {
        app.open_test_game_over();
    }
    if options.hall {
        app.open_hall_of_fame();
    }
    if options.enemy_city {
        app.open_first_enemy_city();
    }
    if let Some(new) = options.outpost {
        app.open_first_outpost(new);
    }
    if let Some(kind) = &options.site {
        app.open_test_site(kind);
    }
    if options.unit_list {
        app.open_first_unit_list();
    }
    if options.capital_name {
        app.offer_capital_naming();
    }
    if options.cities {
        app.open_city_list();
    }
    if options.upkeep {
        app.open_test_upkeep();
    }
    if let Some(kind) = &options.offer {
        app.open_test_offer(kind);
    }
    if let Some(kind) = &options.event {
        app.open_test_event(kind);
    }
    if let Some(kind) = &options.dispelled {
        app.open_test_dispelled(match kind.as_str() {
            "unit" => f4x_core::game::DispelSite::Unit,
            "battle" => f4x_core::game::DispelSite::BattleUnit,
            "field" => f4x_core::game::DispelSite::Field,
            _ => f4x_core::game::DispelSite::City,
        });
    }
    if options.mastery {
        app.open_test_mastery(options.global_by, options.global_at);
    }
    if options.fizzle {
        app.open_test_fizzle();
    }
    if options.level_up {
        app.open_test_level_up();
    }
    if let Some(kind) = &options.results {
        app.open_test_results(kind);
    }
    if let Some(id) = &options.global {
        app.open_test_global(id, options.global_by, options.global_at);
    }
    if let Some(id) = &options.wizard_spell {
        app.open_test_wizard_anim(id, options.global_by, options.global_at);
    }
    if let Some(id) = &options.square {
        app.open_test_square(id, options.global_at);
    }
    if let Some(id) = &options.city_scroll {
        app.open_test_city_scroll(id, options.global_by, options.global_at);
    }
    if let Some(id) = &options.summon {
        app.open_test_summon(id, options.global_by, options.global_at);
    }
    if let Some(id) = &options.learn {
        app.open_test_learn(id, options.learn_at);
    }
    if let Some(id) = &options.built {
        app.open_test_built(id);
    }
    if let Some(n) = options.sheet {
        app.open_test_sheet(n, options.sheet_spell.as_deref(), options.sheet_ask);
    }
    if let Some((x, y)) = options.survey_at {
        app.survey_probe = if x == u32::MAX {
            app.game.economy().cities().first().map(|c| c.2)
        } else {
            Some(f4x_core::map::Position {
                plane: f4x_core::map::Plane::Arcanus,
                x,
                y,
            })
        };
    }
    if let Some(n) = options.advisor {
        if n > 8 {
            app.open_advisor_menu();
        } else {
            app.open_advisor(n);
        }
    }
    if let Some(long) = options.scroll {
        app.open_test_chancellor(long);
    }
    if options.create {
        match &options.create_step {
            Some(step) => app.open_creation_at(step),
            None => app.open_creation(),
        }
    } else if options.options {
        app.open_options();
    } else if options.slots {
        app.open_slots(true);
    } else if options.settings {
        app.open_settings();
    } else if options.auto_attack || options.attack_box {
        app.open_test_auto_attack(options.attack_box);
    } else if let Some(scene) = options.intro {
        app.open_intro_at(scene);
    } else if options.menu {
        app.open_main_menu();
    } else if options.shot.is_none() && options.load.is_none() && !options.quick {
        app.open_intro();
    }
    app.help_ui.probe = options.help_at;
    app.help_ui.probe_building = options.help_building.clone();
    let mut frames = 0;
    if let Some(key) = options.hear {
        if app.audio.can_play(key) {
            println!("playing {key:?}");
        } else {
            eprintln!("{key:?}: no such sound file, or it does not decode");
        }
    }
    loop {
        let input = Input::read();
        if !app.frame(&input) {
            break;
        }
        app.open_help_after_frame();
        if let Some(key) = options.hear {
            if frames == 5 {
                app.audio.effects_on = true;
                app.audio.effect(Some(key));
            }
            if get_time() > 3.0 {
                break;
            }
        }
        frames += 1;
        if let Some(path) = &options.shot {
            let due = match options.shot_after {
                Some(seconds) => get_time() >= seconds,
                None => frames == 3,
            };
            if due {
                get_screen_data().export_png(&path.to_string_lossy());
                break;
            }
        }
        app.draw_pointer(&input);
        next_frame().await;
    }
}
