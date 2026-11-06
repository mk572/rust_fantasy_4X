//! The gold-brown plank buttons of the load/save and settings screens without their labels. The
//! blank picture is the original's own (`button_load_inactive_00`), widened by
//! `scripts/button_text_free.py` to `assets/button/plank_<width>.png`; the label is a text key
//! drawn over it by the font engine in style 4, in the original's dark brown. Widths 38, 39, 40 and 58 serve the
//! Load, Save, Quit, Ok and Settings buttons; the label is kerned as the original's lettering is.

use macroquad::prelude::*;
use std::cell::RefCell;

use crate::font::{rgb, Style};
use crate::frame::Layout;
use crate::menu_bar_art::draw_button;
use crate::menu_bar_art::label_font;
use crate::view::App;

/// The plank pictures by width.
const PLANKS: [(u32, &[u8]); 4] = [
    (38, include_bytes!("../assets/button/plank_38.png")),
    (39, include_bytes!("../assets/button/plank_39.png")),
    (40, include_bytes!("../assets/button/plank_40.png")),
    (58, include_bytes!("../assets/button/plank_58.png")),
];

thread_local! {
    static TEXTURES: RefCell<Vec<(u32, Texture2D)>> = const { RefCell::new(Vec::new()) };
}

fn texture(width: u32) -> Option<Texture2D> {
    TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, t)) = cache.iter().find(|(w, _)| *w == width) {
            return Some(t.clone());
        }
        let bytes = PLANKS.iter().find(|(w, _)| *w == width)?.1;
        let tex = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
        tex.set_filter(FilterMode::Nearest);
        cache.push((width, tex.clone()));
        Some(tex)
    })
}

impl App {
    /// Draw the plank `width` wide with the label `key` (a text key) with its top left at `at`, in
    /// the original's pixels. `pressed` is the look every button of the client has while held
    /// (`menu_bar_art::draw_button`: the picture and the label a pixel right and down); `fade`
    /// (1 = opaque) dims a disabled button. False when there is no picture of that width.
    pub(crate) fn draw_plank(
        &mut self,
        lay: &Layout,
        width: u32,
        key: &str,
        at: (f32, f32),
        pressed: bool,
        fade: f32,
    ) -> bool {
        let Some(tex) = texture(width) else {
            return false;
        };
        let s = lay.scale;
        let size = vec2(tex.width(), tex.height());
        let p = lay.point(at.0, at.1);
        draw_button(
            &tex,
            p,
            size * s,
            s,
            pressed,
            Color::new(1.0, 1.0, 1.0, fade),
        );
        let shift = if pressed { 1.0 } else { 0.0 };
        let label = self.text(key);
        let ink = rgb(97, 56, 16);
        let style = Style::flat(4, Color::new(ink.r, ink.g, ink.b, fade))
            .shadow()
            .edge(Color::from_rgba(255, 220, 110, (26.0 * fade) as u8))
            .center();
        self.draw_styled_in(
            &label_font(),
            (lay.origin.x, lay.origin.y, s),
            &style,
            &label,
            (at.0 + size.x / 2.0 - 1.0 + shift, at.1 + 2.0 + shift),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::PLANKS;

    /// Each picture is as wide as its name says and 13 high (the PNG header holds the size at
    /// bytes 16 to 24).
    #[test]
    fn each_plank_has_the_width_it_is_filed_under() {
        for (width, bytes) in PLANKS {
            let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            assert_eq!((w, h), (width, 13));
        }
    }
}
