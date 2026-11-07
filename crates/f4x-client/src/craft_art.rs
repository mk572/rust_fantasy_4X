//! The item crafting screen's buttons without their lettering (`scripts/button_text_free.py`):
//! one blank 31 by 13 tab for the ten kinds (`assets/button/tab.png`) and the OK (`craft_ok.png`). The font engine draws the labels from text keys.

use macroquad::prelude::*;
use std::cell::RefCell;

use crate::font::{rgb, Style};
use crate::frame::Layout;
use crate::input::Input;
use crate::ui;
use crate::view::App;

const TAB: &[u8] = include_bytes!("../assets/button/tab.png");
const OK: &[u8] = include_bytes!("../assets/button/craft_ok.png");

/// The text keys of the ten tabs, in the order of `craft_screen::KINDS`.
pub(crate) const TAB_KEYS: &[&str] = &[
    "craft.tab_sword",
    "craft.tab_mace",
    "craft.tab_axe",
    "craft.tab_bow",
    "craft.tab_staff",
    "craft.tab_wand",
    "craft.tab_misc",
    "craft.tab_shield",
    "craft.tab_chain",
    "craft.tab_plate",
];

thread_local! {
    static TEXTURES: RefCell<Vec<(usize, Texture2D)>> = const { RefCell::new(Vec::new()) };
}

/// The texture for `bytes`, loaded once; `slot` tells the pictures apart.
fn texture(slot: usize, bytes: &[u8]) -> Texture2D {
    TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, tex)) = cache.iter().find(|(s, _)| *s == slot) {
            return tex.clone();
        }
        let tex = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
        tex.set_filter(FilterMode::Nearest);
        cache.push((slot, tex.clone()));
        tex
    })
}

impl App {
    /// A picture of `slot` at `at` with a label (none when `key` is empty), pressed while the
    /// pointer holds it and `enabled`. Returns the picture's rectangle.
    #[allow(clippy::too_many_arguments)]
    fn craft_blank(
        &mut self,
        lay: &Layout,
        (slot, bytes): (usize, &[u8]),
        key: &str,
        (style, ink): (usize, Color),
        at: (f32, f32),
        label_dy: f32,
        (enabled, input): (bool, &Input),
    ) -> Rect {
        let s = lay.scale;
        let tex = texture(slot, bytes);
        let size = vec2(tex.width(), tex.height());
        let p = lay.point(at.0, at.1);
        let rect = Rect::new(p.x, p.y, size.x * s, size.y * s);
        let held = enabled && ui::hit(rect, true, input).held();
        crate::menu_bar_art::draw_button(&tex, p, size * s, s, held, WHITE);
        if !key.is_empty() {
            let push = f32::from(u8::from(held));
            let label = self.text(key);
            self.draw_styled(
                (lay.origin.x, lay.origin.y, s),
                &Style::flat(style, ink)
                    .shadow()
                    .edge(Color::from_rgba(52, 40, 28, 255))
                    .center(),
                &label,
                at.0 + size.x / 2.0 + push,
                at.1 + label_dy + push,
            );
        }
        rect
    }

    /// One of the ten tabs, gold when `chosen`.
    pub(crate) fn craft_tab(
        &mut self,
        lay: &Layout,
        key: &str,
        chosen: bool,
        at: (f32, f32),
        (enabled, input): (bool, &Input),
    ) -> Rect {
        let ink = if chosen {
            rgb(255, 182, 44)
        } else {
            rgb(166, 134, 105)
        };
        self.craft_blank(lay, (0, TAB), key, (1, ink), at, 3.0, (enabled, input))
    }

    /// The OK button.
    pub(crate) fn craft_ok(&mut self, lay: &Layout, at: (f32, f32), input: (bool, &Input)) -> Rect {
        self.craft_blank(
            lay,
            (1, OK),
            "craft.ok",
            (3, rgb(166, 134, 105)),
            at,
            4.0,
            input,
        )
    }
}
