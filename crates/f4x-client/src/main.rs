//! The desktop client: reads the core's state and draws it, never mutating the
//! world except by sending commands.

mod art;
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
            eprintln!("{e}\nusage: f4x-client [--content DIR] [--assets DIR] [--seed N] [--shot FILE [--city]]");
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
    if options.city {
        app.open_first_city();
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
