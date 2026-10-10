//! The desktop client: reads the core's state and draws it, never mutating the
//! world except by sending commands.

mod art;
mod battle_screen;
mod city_screen;
mod cityscape;
mod create_screen;
mod frame;
mod magic_screen;
mod map_draw;
mod terrain_pick;
mod ui;
mod view;

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
    /// With `--shot`, explore the whole map first.
    reveal: bool,
    /// With `--shot`, put the top-left of the map view at this tile.
    at: Option<(f32, f32)>,
    /// Open the wizard creation screen (the default unless loading, shooting or `--quick`).
    create: bool,
    /// Skip the creation screen and play the default wizard.
    quick: bool,
    /// With `--shot`, fight a test battle first.
    battle: bool,
    /// With `--shot`, select the starting stack first.
    select: bool,
    /// With `--shot`, end this many turns first.
    turns: u32,
    /// Start from a saved game.
    load: Option<PathBuf>,
    /// Save the game (after any `--turns`) before carrying on.
    save: Option<PathBuf>,
}

fn parse_args() -> Result<Options, String> {
    let mut o = Options {
        content: if Path::new("content-local/rules.ron").exists() {
            "content-local".into()
        } else {
            "content".into()
        },
        assets: "assets-local/art".into(),
        seed: 1,
        shot: None,
        city: false,
        magic: false,
        create: false,
        reveal: false,
        at: None,
        quick: false,
        select: false,
        battle: false,
        turns: 0,
        load: None,
        save: None,
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
            "--battle" => o.battle = true,
            "--magic" => o.magic = true,
            "--create" => o.create = true,
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
            eprintln!("{e}\nusage: f4x-client [--content DIR] [--assets DIR] [--seed N] [--load FILE] [--save FILE] [--shot FILE [--city] [--magic] [--create] [--quick] [--select] [--turns N]]");
            std::process::exit(2);
        }
    };
    let content = match f4x_data::load_content(&options.content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "could not load content from {}: {e}",
                options.content.display()
            );
            std::process::exit(1);
        }
    };
    let mut app = match App::new(content, art::Art::load(options.assets), options.seed) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("could not start a game: {e}");
            std::process::exit(1);
        }
    };
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
    if options.battle {
        app.open_test_battle();
    }
    if options.create || (options.shot.is_none() && options.load.is_none() && !options.quick) {
        app.open_creation();
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
