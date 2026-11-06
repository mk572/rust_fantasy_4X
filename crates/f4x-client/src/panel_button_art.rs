//! The army list's blue buttons (Items and Ok) without their lettering: the original's pictures
//! cleaned by `scripts/button_text_free.py` (`assets/button/panel_<name>.png`, 41 by 15), the label a
//! text key drawn by the font engine in style 4 in the gold set `font_ramp::PANEL_WORD` with a black
//! shadow down and to the right.

use macroquad::prelude::*;
use std::cell::RefCell;

use crate::font::{rgb, Ink, Style};
use crate::font_ramp::PANEL_WORD;
use crate::frame::Layout;
use crate::input::Input;
use crate::menu_bar_art::{draw_button, label_font};
use crate::ui;
use crate::view::App;

const ITEMS: &[u8] = include_bytes!("../assets/button/panel_items.png");
const OK: &[u8] = include_bytes!("../assets/button/panel_ok.png");

/// Which picture the button has.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Panel {
    Items,
    Ok,
}

thread_local! {
    static TEXTURES: RefCell<[Option<Texture2D>; 2]> = const { RefCell::new([None, None]) };
}

fn texture(panel: Panel) -> Texture2D {
    TEXTURES.with(|cache| {
        cache.borrow_mut()[panel as usize]
            .get_or_insert_with(|| {
                let bytes = if panel == Panel::Items { ITEMS } else { OK };
                let tex = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
                tex.set_filter(FilterMode::Nearest);
                tex
            })
            .clone()
    })
}

impl App {
    /// The button with its top left at `at`, labelled by the text key `key`; it moves a pixel while
    /// held. True when clicked.
    pub(crate) fn panel_button(
        &mut self,
        lay: &Layout,
        panel: Panel,
        key: &str,
        at: (f32, f32),
        input: &Input,
    ) -> bool {
        let tex = texture(panel);
        let s = lay.scale;
        let size = vec2(tex.width(), tex.height());
        let state = ui::hit(lay.rect((at.0, at.1, size.x, size.y)), true, input);
        let pressed = state.held();
        draw_button(&tex, lay.point(at.0, at.1), size * s, s, pressed, WHITE);
        let shift = if pressed { 1.0 } else { 0.0 };
        let style = Style::inked(4, Ink::Set(PANEL_WORD))
            .shadow()
            .edge(rgb(8, 4, 4))
            .center();
        let label = self.text(key);
        self.draw_styled_in(
            &label_font(),
            (lay.origin.x, lay.origin.y, s),
            &style,
            &label,
            (at.0 + size.x / 2.0 + shift, at.1 + 3.0 + shift),
        );
        state.clicked()
    }
}

#[cfg(test)]
mod tests {
    use super::{ITEMS, OK};

    /// Both pictures are 41 by 15 (the PNG header holds the size at bytes 16 to 24).
    #[test]
    fn the_panels_are_forty_one_by_fifteen() {
        for bytes in [ITEMS, OK] {
            let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            assert_eq!((w, h), (41, 15));
        }
    }
}
