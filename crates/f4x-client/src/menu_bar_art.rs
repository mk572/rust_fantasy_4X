//! The seven buttons of the main screen's menu bar (Game, Spells, Armies, Cities, Magic, Info,
//! Plane) without their labels: the marbled gold of the original's pictures with the lettering
//! taken out (`scripts/button_text_free.py`, files `assets/button/<name>_<frame>.png`, frame 1 is
//! the pressed button). The label is a text key, drawn over the picture by the font engine, so the
//! menu bar can be translated.

use macroquad::prelude::*;
use macroquad_bitmap_font_engine as bm;
use std::cell::RefCell;
use std::rc::Rc;

use crate::font::{rgb, Style};

const NAMES: [[&[u8]; 2]; 7] = [
    [
        include_bytes!("../assets/button/game_0.png"),
        include_bytes!("../assets/button/game_1.png"),
    ],
    [
        include_bytes!("../assets/button/spells_0.png"),
        include_bytes!("../assets/button/spells_1.png"),
    ],
    [
        include_bytes!("../assets/button/armies_0.png"),
        include_bytes!("../assets/button/armies_1.png"),
    ],
    [
        include_bytes!("../assets/button/cities_0.png"),
        include_bytes!("../assets/button/cities_1.png"),
    ],
    [
        include_bytes!("../assets/button/magic_0.png"),
        include_bytes!("../assets/button/magic_1.png"),
    ],
    [
        include_bytes!("../assets/button/info_0.png"),
        include_bytes!("../assets/button/info_1.png"),
    ],
    [
        include_bytes!("../assets/button/plane_0.png"),
        include_bytes!("../assets/button/plane_1.png"),
    ],
];

/// The text keys of the seven buttons, in the order of the bar.
pub(crate) const KEYS: [&str; 7] = [
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

/// The picture of button `index` (0 to 6) in `frame` (0 up, 1 pressed).
pub(crate) fn texture(index: usize, frame: usize) -> Texture2D {
    TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.is_empty() {
            cache.resize(14, None);
        }
        let slot = index * 2 + frame.min(1);
        cache[slot]
            .get_or_insert_with(|| {
                let tex = Texture2D::from_file_with_format(
                    NAMES[index][frame.min(1)],
                    Some(ImageFormat::Png),
                );
                tex.set_filter(FilterMode::Nearest);
                tex
            })
            .clone()
    })
}

/// The font of the labels: style 4 with the pairs the original's pictures set a pixel tighter than
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
