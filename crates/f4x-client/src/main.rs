//! The desktop client: reads the core's state and draws it, never mutating the
//! world except by sending commands.

mod advisor_screens;
mod advisors;
mod army_screen;
mod art;
mod attack_prompt;
mod battle_anim;
mod battle_ground;
mod battle_screen;
mod bigbook;
mod chancellor;
mod city_list_screen;
mod city_screen;
mod cityscape;
mod console_screen;
mod create_screen;
mod diplomacy_screen;
mod diplomacy_text;
mod enemy_city_screen;
mod frame;
mod game_over_screen;
mod hall_of_fame;
mod item_screen;
mod magic_screen;
mod map_draw;
mod menu_screen;
mod newgame_flow;
mod newgame_picks;
mod offer_prompt;
mod options_screen;
mod outpost_screen;
mod preferences;
mod road_plan;
mod settings_screen;
mod site_prompt;
mod spellbook;
mod surveyor;
mod terrain_pick;
mod ui;
mod unit_list_screen;
mod unit_screen;
mod unit_sheet;
mod view;
mod wizard_screen;

use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use view::App;

struct Options {
    content: PathBuf,
    assets: PathBuf,
    seed: u64,
    /// Save a screenshot after a few frames and quit (for automated checks).
    shot: Option<PathBuf>,
    /// With `--shot`, open the city screen first.
    city: bool,
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
    apprentice: bool,
    research: bool,
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
    enemy_city: bool,
    score: bool,
    items: bool,
    hall: bool,
    /// With `--shot`, make the human wizard this offer first: `hero`, `mercenaries` or `item`.
    offer: Option<String>,
    /// With `--shot`, open the sheet of this unit of the stack (or of the test battle).
    sheet: Option<usize>,
    /// With `--shot`, end this many turns first.
    turns: u32,
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
        seed: 1,
        shot: None,
        city: false,
        magic: false,
        wizard: None,
        talk: None,
        mood: 0,
        spells: false,
        apprentice: false,
        research: false,
        create: false,
        create_step: None,
        menu: false,
        options: false,
        slots: false,
        settings: false,
        advisor: None,
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
        armies: false,
        cities: false,
        outpost: None,
        capital_name: false,
        unit_list: false,
        site: None,
        enemy_city: false,
        score: false,
        items: false,
        hall: false,
        offer: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--content" => o.content = value("--content")?.into(),
            "--assets" => o.assets = value("--assets")?.into(),
            "--seed" => {
                o.seed = value("--seed")?
                    .parse()
                    .map_err(|_| "--seed must be a number")?
            }
            "--shot" => o.shot = Some(value("--shot")?.into()),
            "--city" => o.city = true,
            "--select" => o.select = true,
            "--armies" => o.armies = true,
            "--cities" => o.cities = true,
            "--score" => o.score = true,
            "--items" => o.items = true,
            "--hall" => o.hall = true,
            "--enemy-city" => o.enemy_city = true,
            "--capital-name" => o.capital_name = true,
            "--unit-list" => o.unit_list = true,
            "--site" => o.site = Some(value("--site")?),
            "--outpost" => o.outpost = Some(false),
            "--outpost-new" => o.outpost = Some(true),
            "--offer" => o.offer = Some(value("--offer")?),
            "--sheet" => {
                o.sheet = Some(
                    value("--sheet")?
                        .parse()
                        .map_err(|_| "--sheet must be a unit number")?,
                )
            }
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
            eprintln!("{e}\nusage: f4x-client [--content DIR] [--assets DIR] [--seed N] [--load FILE] [--save FILE] [--shot FILE [--city] [--magic] [--wizard N] [--talk MENU [--mood N]] [--spells] [--apprentice] [--research] [--create] [--create-step wizard|portrait|name|picks|spells|race|banner|custom-NAME] [--options] [--menu] [--slots] [--settings] [--advisor N [--survey-at X,Y]] [--scroll|--scroll-long] [--quick] [--select] [--armies] [--cities] [--outpost|--outpost-new] [--capital-name] [--unit-list] [--site ask|found|treasure] [--enemy-city] [--score] [--items] [--hall] [--offer KIND] [--sheet N] [--turns N]]");
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
    let help = f4x_data::load_help(&options.content).unwrap_or_default();
    let mut app = match App::new(content, art::Art::load(options.assets), options.seed) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("could not start a game: {e}");
            std::process::exit(1);
        }
    };
    app.help = help;
    app.prefs = preferences::Preferences::load(Path::new(preferences::FILE));
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
    }
    if options.reveal {
        app.reveal_all();
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
        app.open_spellbook();
    }
    if options.apprentice {
        app.open_apprentice();
    }
    if options.research {
        app.open_research_choice();
    }
    if options.battle {
        app.open_test_battle();
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
    if let Some(kind) = &options.offer {
        app.open_test_offer(kind);
    }
    if let Some(n) = options.sheet {
        app.open_test_sheet(n);
    }
    if let Some((x, y)) = options.survey_at {
        app.survey_probe = if x == u32::MAX {
            app.game.economy().cities().first().map(|c| c.2)
        } else {
            Some(f4x_core::city::Position {
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
        app.open_creation();
        if let Some(step) = &options.create_step {
            app.test_create_step(step);
        }
    } else if options.options {
        app.open_options();
    } else if options.slots {
        app.open_slots(true);
    } else if options.settings {
        app.screen = view::Screen::Settings;
    } else if options.menu || (options.shot.is_none() && options.load.is_none() && !options.quick) {
        app.open_main_menu();
    }
    let mut frames = 0;
    loop {
        if !app.frame() {
            break;
        }
        frames += 1;
        if let Some(path) = &options.shot {
            if frames == 3 {
                get_screen_data().export_png(&path.to_string_lossy());
                break;
            }
        }
        next_frame().await;
    }
}
