//! The seven buttons of the main screen's menu bar (Game, Spells, Armies, Cities, Magic, Info,
//! Plane) without their labels: the marbled gold of the original's pictures with the lettering
//! taken out (`scripts/button_text_free.py`, files `assets/button/<name>.png`). The label is a text key, drawn over the picture by the font engine, so the
//! menu bar can be translated.

use macroquad::prelude::*;
use macroquad_bitmap_font_engine as bm;
use std::cell::RefCell;
use std::rc::Rc;

use crate::font::{rgb, Style};

const PICTURES: &[&[u8]] = &[
    include_bytes!("../assets/button/game.png"),
    include_bytes!("../assets/button/spells.png"),
    include_bytes!("../assets/button/armies.png"),
    include_bytes!("../assets/button/cities.png"),
    include_bytes!("../assets/button/magic.png"),
    include_bytes!("../assets/button/info.png"),
    include_bytes!("../assets/button/plane.png"),
];

/// The text keys of the seven buttons, in the order of the bar.
pub(crate) const KEYS: &[&str] = &[
    "ui.menu_game",
    "ui.menu_spells",
    "ui.menu_armies",
    "ui.menu_cities",
    "ui.menu_magic",
    "ui.menu_info",
    "ui.menu_plane",
];

thread_local! {
    static TEXTURES: RefCell<Vec<Option<Texture2D>>> = const { RefCell::new(Vec::new()) };
}

/// The picture of button `index` (0 to 6), up.
pub(crate) fn texture(index: usize) -> Texture2D {
    TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.is_empty() {
            cache.resize(PICTURES.len(), None);
        }
        cache[index]
            .get_or_insert_with(|| {
                let tex = Texture2D::from_file_with_format(PICTURES[index], Some(ImageFormat::Png));
                tex.set_filter(FilterMode::Nearest);
                tex
            })
            .clone()
    })
}

/// Draw a button picture with its top left at `p`, `size` big on screen. Pressed, the original's
/// picture is the up one a pixel (`scale` screen pixels) right and down under a dark top row and
/// left column. `tint` colours the picture (its alpha fades it).
pub(crate) fn draw_button(
    tex: &Texture2D,
    p: Vec2,
    size: Vec2,
    scale: f32,
    pressed: bool,
    tint: Color,
) {
    if !pressed {
        draw_texture_ex(
            tex,
            p.x,
            p.y,
            tint,
            DrawTextureParams {
                dest_size: Some(size),
                ..Default::default()
            },
        );
        return;
    }
    draw_rectangle(
        p.x,
        p.y,
        size.x,
        size.y,
        Color::new(8.0 / 255.0, 4.0 / 255.0, 4.0 / 255.0, tint.a),
    );
    let (w, h) = (tex.width(), tex.height());
    draw_texture_ex(
        tex,
        p.x + scale,
        p.y + scale,
        tint,
        DrawTextureParams {
            dest_size: Some(size - vec2(scale, scale)),
            source: Some(Rect::new(0.0, 0.0, w - 1.0, h - 1.0)),
            ..Default::default()
        },
    );
}

/// The font of the labels (the menu bar and Next Turn): style 4 with the pairs the original's pictures set tighter than
/// its print routine does (`assets/button/kerning.fnt`). The original draws its own text without
/// kerning (`Print_Character`: width plus spacing), so the rest of the game keeps the plain font;
/// only these pictures' lettering was tighter, and this face reproduces it.
pub(crate) fn label_font() -> Rc<bm::BmFont> {
    thread_local! {
        static FACE: RefCell<Option<Rc<bm::BmFont>>> = const { RefCell::new(None) };
    }
    FACE.with(|face| {
        face.borrow_mut()
            .get_or_insert_with(|| {
                let fnt = format!(
                    "{}\n{}",
                    include_str!("../assets/fonts/style4.fnt"),
                    include_str!("../assets/button/kerning.fnt")
                );
                Rc::new(
                    bm::BmFont::from_bytes(&fnt, include_bytes!("../assets/fonts/style4.png"))
                        .expect("the menu bar's font loads"),
                )
            })
            .clone()
    })
}

/// How the label looks: the original's blackletter (style 4) in its black colour set, with the
/// lighter gold copy a pixel to the lower right that its picture carries.
pub(crate) fn label_style() -> Style {
    Style::set(4, 1).shadow().edge(rgb(255, 174, 12)).center()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_kerning_file_counts_its_pairs_and_names_letters_the_font_has() {
        let file = include_str!("../assets/button/kerning.fnt");
        let font = include_str!("../assets/fonts/style4.fnt");
        let pairs: Vec<Vec<u32>> = file
            .lines()
            .filter(|l| l.starts_with("kerning "))
            .map(|l| {
                l.split_whitespace()
                    .skip(1)
                    .map(|kv| kv.split('=').nth(1).unwrap().parse::<i32>().unwrap() as u32)
                    .collect()
            })
            .collect();
        let count: usize = file
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("kernings count="))
            .and_then(|n| n.parse().ok())
            .expect("the first line gives the count");
        assert_eq!(count, pairs.len());
        for p in &pairs {
            for id in &p[..2] {
                assert!(font.contains(&format!("char id={id} ")), "no glyph {id}");
            }
        }
    }
}
