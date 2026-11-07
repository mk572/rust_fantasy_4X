//! The text-free blue and stone buttons: the army list's Items and Ok, the item and magic
//! screens' Alchemy and Ok. The pictures are the originals cleaned by `scripts/button_text_free.py`
//! (one blank per family and width). The army list's gold labels are style 4 in the colour set
//! `font_ramp::PANEL_WORD` with a black shadow. The lavender ones have a metallic sheen (diagonal
//! bands across the whole word) and an emboss, which a font cannot do: the text is drawn in white off
//! screen for its shape, and each pixel of it takes its colour from a sheen picture; the pixels
//! beside it form the light and dark edges. That picture is kept per text.

use macroquad::prelude::*;
use std::cell::RefCell;

use crate::font::{rgb, Ink, Style};
use crate::font_ramp::PANEL_WORD;
use crate::frame::Layout;
use crate::input::Input;
use crate::menu_bar_art::{draw_button, label_font};
use crate::ui;
use crate::view::App;

/// Which picture the button has.
#[derive(Clone, Copy, PartialEq, Eq, strum::EnumIter)]
pub(crate) enum Panel {
    /// The army list's Items and Ok (dark blue with a gold border, gold lettering).
    Items,
    Ok,
    /// The item screen's Alchemy and Ok (brown stone, lavender lettering).
    StoneAlchemy,
    StoneOk,
    /// The magic screen's Alchemy and Ok (blue stone, lavender lettering).
    BlueAlchemy,
    BlueOk,
}

impl Panel {
    /// One blank per family and width, made from the family's Ok (`scripts/button_text_free.py`).
    fn picture(self) -> &'static [u8] {
        match self {
            Panel::Items | Panel::Ok => include_bytes!("../assets/button/panel_41.png"),
            Panel::StoneAlchemy | Panel::StoneOk => include_bytes!("../assets/button/stone_59.png"),
            Panel::BlueAlchemy => include_bytes!("../assets/button/blue_58.png"),
            Panel::BlueOk => include_bytes!("../assets/button/blue_26.png"),
        }
    }

    /// The metallic sheen of the lettering (`scripts/button_text_free.py`): the colour of every
    /// pixel of the button, in diagonal bands. The gold labels have none (a colour set does it).
    fn sheen(self) -> Option<&'static [u8]> {
        match self {
            Panel::Items | Panel::Ok => None,
            Panel::StoneAlchemy => Some(include_bytes!("../assets/button/sheen_stone_alchemy.png")),
            Panel::StoneOk => Some(include_bytes!("../assets/button/sheen_stone_ok.png")),
            Panel::BlueAlchemy => Some(include_bytes!("../assets/button/sheen_blue_alchemy.png")),
            Panel::BlueOk => Some(include_bytes!("../assets/button/sheen_blue_ok.png")),
        }
    }

    /// The style of a gold label (a colour set, a black shadow); the lavender ones are composed
    /// from their sheen instead.
    fn style(self) -> Style {
        Style::inked(4, Ink::Set(PANEL_WORD))
            .shadow()
            .edge(rgb(8, 4, 4))
            .center()
    }

    /// Where the label's middle and top are, from the picture's top left.
    fn label_at(self, width: f32) -> (f32, f32) {
        match self {
            Panel::StoneOk => (width / 2.0, 4.0),
            Panel::BlueOk => (width / 2.0 + 1.0, 3.0),
            Panel::StoneAlchemy | Panel::BlueAlchemy => (width / 2.0, 2.0),
            _ => (width / 2.0, 3.0),
        }
    }
}

thread_local! {
    static TEXTURES: RefCell<[Option<Texture2D>; 6]> = const { RefCell::new([None, None, None, None, None, None]) };
}

fn texture(panel: Panel) -> Texture2D {
    TEXTURES.with(|cache| {
        cache.borrow_mut()[panel as usize]
            .get_or_insert_with(|| {
                let tex = Texture2D::from_file_with_format(panel.picture(), Some(ImageFormat::Png));
                tex.set_filter(FilterMode::Nearest);
                tex
            })
            .clone()
    })
}

thread_local! {
    static LABELS: RefCell<Vec<(Panel, String, Texture2D)>> = const { RefCell::new(Vec::new()) };
}

/// The lettering of a sheen label as a picture the size of its button: the text drawn in white
/// off screen at the original's pixels gives the shape; each pixel of it takes its colour from
/// the sheen picture, and the pixels diagonally beside it are the emboss: a light one up and to
/// the left, a dark one down and to the right.
fn sheen_label(app: &mut App, panel: Panel, sheen: &[u8], label: &str, size: Vec2) -> Texture2D {
    let (w, h) = (size.x as u32, size.y as u32);
    let target = render_target(w, h);
    target.texture.set_filter(FilterMode::Nearest);
    set_camera(&Camera2D {
        zoom: vec2(2.0 / size.x, 2.0 / size.y),
        target: size / 2.0,
        render_target: Some(target.clone()),
        ..Default::default()
    });
    clear_background(Color::new(0.0, 0.0, 0.0, 0.0));
    let (lx, ly) = panel.label_at(size.x);
    app.draw_styled_in(
        &label_font(),
        (0.0, 0.0, 1.0),
        &Style::flat(4, WHITE).center(),
        label,
        (lx, ly),
    );
    set_default_camera();
    let mask = target.texture.get_texture_data();
    let colours = Image::from_file_with_format(sheen, Some(ImageFormat::Png))
        .expect("the sheen picture loads");
    let mut out = Image::gen_image_color(w as u16, h as u16, Color::new(0.0, 0.0, 0.0, 0.0));
    let ink = |x: i32, y: i32| {
        x >= 0
            && y >= 0
            && (x as u32) < w
            && (y as u32) < h
            && mask.get_pixel(x as u32, y as u32).a > 0.5
    };
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let pixel = if ink(x, y) {
                colours.get_pixel(x as u32, y as u32)
            } else if ink(x - 1, y - 1) {
                EMBOSS_DARK
            } else if ink(x + 1, y + 1) {
                EMBOSS_LIGHT
            } else {
                continue;
            };
            out.set_pixel(x as u32, y as u32, pixel);
        }
    }
    let tex = Texture2D::from_image(&out);
    tex.set_filter(FilterMode::Nearest);
    tex
}

/// The emboss of the sheen labels: a translucent dark edge down and to the right, a light one up
/// and to the left.
const EMBOSS_DARK: Color = Color::new(0.08, 0.06, 0.1, 0.8);
const EMBOSS_LIGHT: Color = Color::new(0.85, 0.85, 0.95, 0.35);

impl App {
    /// The button with its top left at `at`, labelled by the text key `key`; it moves a pixel while
    /// held. True when clicked. The lavender labels are composed once per text and kept.
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
        let label = self.text(key);
        if let Some(sheen) = panel.sheen() {
            let cached = LABELS.with(|c| {
                c.borrow()
                    .iter()
                    .find(|(p, l, _)| *p == panel && *l == label)
                    .map(|(_, _, t)| t.clone())
            });
            let tex = cached.unwrap_or_else(|| {
                let t = sheen_label(self, panel, sheen, &label, size);
                LABELS.with(|c| c.borrow_mut().push((panel, label.clone(), t.clone())));
                t
            });
            let p = lay.point(at.0 + shift, at.1 + shift);
            draw_texture_ex(
                &tex,
                p.x,
                p.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(size * s),
                    ..Default::default()
                },
            );
        } else {
            let (lx, ly) = panel.label_at(size.x);
            self.draw_styled_in(
                &label_font(),
                (lay.origin.x, lay.origin.y, s),
                &panel.style(),
                &label,
                (at.0 + lx + shift, at.1 + ly + shift),
            );
        }
        state.clicked()
    }
}

#[cfg(test)]
mod tests {
    use super::Panel;
    use strum::IntoEnumIterator;

    /// Every picture is 15 high (the PNG header holds the size at bytes 16 to 24); the army list's
    /// are 41 wide, the item screen's 59 and the magic screen's 58 and 26.
    #[test]
    fn the_panels_and_their_sheens_have_the_sizes_of_the_originals() {
        let widths = [41, 41, 59, 59, 58, 26];
        for (panel, width) in Panel::iter().zip(widths) {
            let bytes = panel.picture();
            let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            assert_eq!((w, h), (width, 15));
            if let Some(sheen) = panel.sheen() {
                let sw = u32::from_be_bytes(sheen[16..20].try_into().unwrap());
                let sh = u32::from_be_bytes(sheen[20..24].try_into().unwrap());
                assert_eq!((sw, sh), (width, 15), "the sheen of a {width} wide button");
            }
        }
    }
}
