//! The city screen's wooden buttons (Buy, Change and OK) without their labels: 30 or 36 by 10
//! pictures (`scripts/button_text_free.py`, files `assets/button/city_<name>.png`). The label is a
//! text key drawn over the picture by the font engine in the 5 px font (style 1), in the
//! original's lavender grey. The pressed look is the up picture moved a pixel
//! (`menu_bar_art::draw_button`); a disabled button fades as the original's does.

use macroquad::prelude::*;
use macroquad_bitmap_font_engine as bm;
use std::cell::RefCell;
use std::rc::Rc;

use crate::font::{rgb, Style};
use crate::frame::Layout;
use crate::input::Input;
use crate::ui;
use crate::view::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wood {
    Buy,
    Change,
    Ok,
}

const PICTURES: [&[u8]; 3] = [
    include_bytes!("../assets/button/city_buy.png"),
    include_bytes!("../assets/button/city_change.png"),
    include_bytes!("../assets/button/city_ok.png"),
];

impl Wood {
    /// The text key of the label.
    fn key(self) -> &'static str {
        match self {
            Wood::Buy => "ui.button_buy",
            Wood::Change => "ui.button_change",
            Wood::Ok => "ui.button_ok",
        }
    }

    /// Where the label's middle sits against the picture's, in pixels: the original rounds the
    /// start of the lettering down, which the font engine's centring rounds up for OK.
    fn nudge(self) -> f32 {
        match self {
            Wood::Change => 0.0,
            Wood::Buy => 0.5,
            Wood::Ok => -0.5,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

thread_local! {
    static TEXTURES: RefCell<[Option<Texture2D>; 3]> = const { RefCell::new([None, None, None]) };
}

fn texture(which: Wood) -> Texture2D {
    TEXTURES.with(|cache| {
        cache.borrow_mut()[which.index()]
            .get_or_insert_with(|| {
                let tex = Texture2D::from_file_with_format(
                    PICTURES[which.index()],
                    Some(ImageFormat::Png),
                );
                tex.set_filter(FilterMode::Nearest);
                tex
            })
            .clone()
    })
}

/// The font of the labels: style 1 with the original's own letters for BUY, CHANGE and OK, the
/// tailed Y and the darker corner pixels of the A and G (`scripts/button_label_font.py`).
fn label_font() -> Rc<bm::BmFont> {
    thread_local! {
        static FACE: RefCell<Option<Rc<bm::BmFont>>> = const { RefCell::new(None) };
    }
    FACE.with(|face| {
        face.borrow_mut()
            .get_or_insert_with(|| {
                Rc::new(
                    bm::BmFont::from_bytes(
                        include_str!("../assets/button/wood.fnt"),
                        include_bytes!("../assets/button/wood.png"),
                    )
                    .expect("the wooden buttons' font loads"),
                )
            })
            .clone()
    })
}

impl App {
    /// Draw the button with its top left at `at` in the original's pixels; true when clicked. A
    /// disabled button is drawn faded and does not answer the pointer.
    pub(crate) fn draw_wood_button(
        &mut self,
        lay: &Layout,
        which: Wood,
        at: (f32, f32),
        enabled: bool,
        input: &Input,
    ) -> bool {
        let s = lay.scale;
        let tex = texture(which);
        let size = vec2(tex.width(), tex.height());
        let p = lay.point(at.0, at.1);
        let state = ui::hit(Rect::new(p.x, p.y, size.x * s, size.y * s), enabled, input);
        let alpha = if enabled { 1.0 } else { 0.45 };
        let pressed = state.held();
        crate::menu_bar_art::draw_button(
            &tex,
            p,
            size * s,
            s,
            pressed,
            Color::new(1.0, 1.0, 1.0, alpha),
        );
        let push = f32::from(u8::from(pressed));
        let label = self.text(which.key());
        let ink = rgb(178, 178, 195);
        let style = Style::flat(1, Color::new(ink.r, ink.g, ink.b, alpha)).center();
        self.draw_styled_in(
            &label_font(),
            (lay.origin.x, lay.origin.y, s),
            &style,
            &label,
            (
                at.0 + size.x / 2.0 + which.nudge() + push,
                at.1 + 2.0 + push,
            ),
        );
        state.clicked()
    }
}
