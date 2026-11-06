//! The unit window's action buttons (Done, Patrol, Wait and Build, Meld or Purify) without their
//! labels: 33 by 9 pictures of marbled gold (`scripts/button_text_free.py`, files
//! `assets/button/<name>_<frame>.png`, frame 1 is the pressed button, and `<name>_locked.png` the
//! dimmed one). The label is a text key drawn over the picture by the font engine in the 5 px
//! font (style 0), so the buttons can be translated.

use macroquad::prelude::*;
use macroquad_bitmap_font_engine as bm;
use std::cell::RefCell;
use std::rc::Rc;

use crate::font::{rgb, Style};
use crate::frame::Layout;
use crate::input::Input;
use crate::ui;
use crate::view::App;

/// The buttons of the unit window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Done,
    Patrol,
    Wait,
    Build,
    Meld,
    Purify,
}

impl Action {
    const ALL: [Action; 6] = [
        Action::Done,
        Action::Patrol,
        Action::Wait,
        Action::Build,
        Action::Meld,
        Action::Purify,
    ];

    /// The text key of the label.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Action::Done => "ui.button_done",
            Action::Patrol => "ui.button_patrol",
            Action::Wait => "ui.button_wait",
            Action::Build => "ui.button_build",
            Action::Meld => "ui.button_meld",
            Action::Purify => "ui.button_purify",
        }
    }

    fn index(self) -> usize {
        Action::ALL.iter().position(|&a| a == self).unwrap_or(0)
    }
}

/// How a button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Look {
    Up,
    Pressed,
    /// Dimmed: there is nothing the button can do now.
    Locked,
}

const UP: [&[u8]; 6] = [
    include_bytes!("../assets/button/done.png"),
    include_bytes!("../assets/button/patrol.png"),
    include_bytes!("../assets/button/wait.png"),
    include_bytes!("../assets/button/build.png"),
    include_bytes!("../assets/button/meld.png"),
    include_bytes!("../assets/button/purify.png"),
];

/// The locked pictures: Done, Patrol, Wait and Build (the original has no locked Meld).
const LOCKED: [&[u8]; 4] = [
    include_bytes!("../assets/button/done_locked.png"),
    include_bytes!("../assets/button/patrol_locked.png"),
    include_bytes!("../assets/button/wait_locked.png"),
    include_bytes!("../assets/button/build_locked.png"),
];

thread_local! {
    static TEXTURES: RefCell<Vec<Option<Texture2D>>> = const { RefCell::new(Vec::new()) };
}

fn load(bytes: &[u8]) -> Texture2D {
    let tex = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
    tex.set_filter(FilterMode::Nearest);
    tex
}

/// The picture of `action` in `look`; a button without a locked picture shows its up one.
pub(crate) fn texture(action: Action, look: Look) -> Texture2D {
    TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.is_empty() {
            cache.resize(Action::ALL.len() * 2, None);
        }
        let i = action.index();
        let (slot, bytes) = match look {
            Look::Locked if i < LOCKED.len() => (i * 2 + 1, LOCKED[i]),
            _ => (i * 2, UP[i]),
        };
        cache[slot].get_or_insert_with(|| load(bytes)).clone()
    })
}

/// The Next Turn button's picture (67 by 16, grey stone), without its lettering.
pub(crate) fn next_turn_texture() -> Texture2D {
    thread_local! {
        static TEX: RefCell<Option<Texture2D>> = const { RefCell::new(None) };
    }
    TEX.with(|t| {
        t.borrow_mut()
            .get_or_insert_with(|| load(include_bytes!("../assets/button/next_turn.png")))
            .clone()
    })
}

/// The Next Turn label in two passes, the second over the first: the gold lettering with a dark
/// brown shadow down and to the right, then the lettering again with a light brown shadow up and
/// to the left over it.
pub(crate) fn next_turn_styles() -> [Style; 2] {
    let gold = rgb(255, 182, 44);
    [
        Style::flat(4, gold)
            .effect(crate::font::Effect::Shadow)
            .edge(rgb(69, 36, 4))
            .center(),
        Style::flat(4, gold)
            .effect(crate::font::Effect::ShadowUp)
            .edge(Color::from_rgba(190, 125, 20, 200))
            .highlight(
                Color::from_rgba(255, 236, 130, 200),
                Color::from_rgba(255, 236, 130, 90),
            )
            .center(),
    ]
}

/// The font of the labels: style 0, with the serif "I" of the Build picture's lettering for Build
/// (`scripts/button_label_font.py`).
fn label_font(app: &App, action: Action) -> Rc<bm::BmFont> {
    thread_local! {
        static SERIF: RefCell<Option<Rc<bm::BmFont>>> = const { RefCell::new(None) };
    }
    if action != Action::Build {
        return app.art.bm_font(0);
    }
    SERIF.with(|face| {
        face.borrow_mut()
            .get_or_insert_with(|| {
                Rc::new(
                    bm::BmFont::from_bytes(
                        include_str!("../assets/button/label.fnt"),
                        include_bytes!("../assets/button/label.png"),
                    )
                    .expect("the Build label's font loads"),
                )
            })
            .clone()
    })
}

/// The ink of the label: the original's black, or a dim brown on a locked button, with the
/// lighter gold copy a pixel to the lower right that its picture carries.
fn label_style(look: Look) -> Style {
    match look {
        Look::Locked => Style::flat(0, rgb(121, 85, 36)),
        _ => Style::set(0, 1),
    }
    .shadow()
    .edge(Color::from_rgba(255, 196, 56, 100))
    .center()
}

impl App {
    /// Draw `action`'s button with its top left at `at` in the original's pixels: the picture, then
    /// the label. A pressed button's lettering sits a pixel right and down with the picture.
    /// Returns the state of the pointer on it (`Hit::None` while `locked`).
    pub(crate) fn draw_action_button(
        &mut self,
        lay: &Layout,
        action: Action,
        at: (f32, f32),
        locked: bool,
        input: &Input,
    ) -> ui::Hit {
        let s = lay.scale;
        let up = texture(action, Look::Up);
        let size = vec2(up.width(), up.height());
        let p = lay.point(at.0, at.1);
        let state = if locked {
            ui::Hit::None
        } else {
            ui::hit(Rect::new(p.x, p.y, size.x * s, size.y * s), true, input)
        };
        let look = if locked {
            Look::Locked
        } else if state.held() {
            Look::Pressed
        } else {
            Look::Up
        };
        crate::menu_bar_art::draw_button(
            &texture(action, look),
            p,
            size * s,
            s,
            look == Look::Pressed,
            WHITE,
        );
        let push = f32::from(u8::from(look == Look::Pressed));
        let label = self.text(action.key());
        let font = label_font(self, action);
        self.draw_styled_in(
            &font,
            (lay.origin.x, lay.origin.y, s),
            &label_style(look),
            &label,
            (at.0 + size.x / 2.0 + 0.5 + push, at.1 + 2.0 + push),
        );
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pictures_are_33_by_9_and_the_build_font_has_the_serif_i() {
        for bytes in UP.iter().chain(LOCKED.iter()) {
            // A PNG's header holds the size at bytes 16 to 24.
            let size = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
            assert_eq!((size(16), size(20)), (33, 9));
        }
        let fnt = include_str!("../assets/button/label.fnt");
        let i = fnt.lines().find(|l| l.starts_with("char id=73 ")).unwrap();
        assert!(i.contains("width=3") && i.contains("xadvance=4"), "{i}");
    }
}
