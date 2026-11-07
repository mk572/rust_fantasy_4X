//! The unit window's action buttons (Done, Patrol, Wait and Build, Meld or Purify) without their
//! labels: 33 by 9 pictures of marbled gold (`scripts/button_text_free.py`, files
//! `assets/button/<name>_<frame>.png`, frame 1 is the pressed button, and `<name>_locked.png` the
//! dimmed one). The label is a text key drawn over the picture by the font engine in the 5 px
//! font (style 0), so the buttons can be translated.

use macroquad::prelude::*;
use macroquad_bitmap_font_engine as bm;
use std::cell::RefCell;
use std::rc::Rc;
use strum::IntoEnumIterator;

use crate::font::{rgb, Style};
use crate::frame::Layout;
use crate::input::Input;
use crate::ui;
use crate::view::App;

/// The buttons of the unit window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumIter)]
pub(crate) enum Action {
    Done,
    Patrol,
    Wait,
    Build,
    Meld,
    Purify,
    /// OK and Cancel of the road and spell panels: the Done's blank picture.
    Ok,
    Cancel,
}

impl Action {
    /// The text key of the label.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Action::Done => "ui.button_done",
            Action::Patrol => "ui.button_patrol",
            Action::Wait => "ui.button_wait",
            Action::Build => "ui.button_build",
            Action::Meld => "ui.button_meld",
            Action::Purify => "ui.button_purify",
            Action::Ok => "ui.button_ok",
            Action::Cancel => "ui.button_cancel",
        }
    }

    /// The place of the button's pictures in the cache. OK and Cancel have no pictures of their
    /// own: they show the Done button's blank one, so they share its place.
    fn index(self) -> usize {
        match self {
            Action::Done | Action::Ok | Action::Cancel => 0,
            Action::Patrol => 1,
            Action::Wait => 2,
            Action::Build => 3,
            Action::Meld => 4,
            Action::Purify => 5,
        }
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

const UP: &[&[u8]] = &[
    include_bytes!("../assets/button/done.png"),
    include_bytes!("../assets/button/patrol.png"),
    include_bytes!("../assets/button/wait.png"),
    include_bytes!("../assets/button/build.png"),
    include_bytes!("../assets/button/meld.png"),
    include_bytes!("../assets/button/purify.png"),
];

/// The locked pictures: Done, Patrol, Wait and Build (the original has no locked Meld).
const LOCKED: &[&[u8]] = &[
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
            cache.resize(
                Action::iter().map(Action::index).max().map_or(0, |m| m + 1) * 2,
                None,
            );
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

/// The battle screen's gold buttons (26 by 10): one blank picture for all six, and its dimmed copy.
fn battle_texture(locked: bool) -> Texture2D {
    thread_local! {
        static TEX: RefCell<[Option<Texture2D>; 2]> = const { RefCell::new([None, None]) };
    }
    TEX.with(|t| {
        t.borrow_mut()[usize::from(locked)]
            .get_or_insert_with(|| {
                load(if locked {
                    include_bytes!("../assets/button/battle_locked.png")
                } else {
                    include_bytes!("../assets/button/battle.png")
                })
            })
            .clone()
    })
}

/// The stone slabs of the question boxes (66 by 17): Yes with green lettering, No with red.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slab {
    Yes,
    No,
}

fn slab_texture(slab: Slab) -> Texture2D {
    thread_local! {
        static TEX: RefCell<[Option<Texture2D>; 2]> = const { RefCell::new([None, None]) };
    }
    TEX.with(|t| {
        t.borrow_mut()[slab as usize]
            .get_or_insert_with(|| {
                load(match slab {
                    Slab::Yes => include_bytes!("../assets/button/yes.png"),
                    Slab::No => include_bytes!("../assets/button/no.png"),
                })
            })
            .clone()
    })
}

impl App {
    /// A slab of a question box at `at`, its label from the text key; the state of the pointer.
    pub(crate) fn slab_button(
        &mut self,
        lay: &Layout,
        (slab, key): (Slab, &str),
        at: (f32, f32),
        input: &Input,
    ) -> ui::Hit {
        let s = lay.scale;
        let tex = slab_texture(slab);
        let size = vec2(tex.width(), tex.height());
        let p = lay.point(at.0, at.1);
        let state = ui::hit(Rect::new(p.x, p.y, size.x * s, size.y * s), true, input);
        crate::menu_bar_art::draw_button(&tex, p, size * s, s, state.held(), WHITE);
        let push = f32::from(u8::from(state.held()));
        let label = self.text(key);
        let ink = match slab {
            Slab::Yes => rgb(4, 69, 36),
            Slab::No => rgb(93, 16, 16),
        };
        let font = self.art.bm_font(4);
        self.draw_styled_in(
            &font,
            (lay.origin.x, lay.origin.y, s),
            &Style::flat(4, ink).center(),
            &label,
            (at.0 + size.x / 2.0 + push, at.1 + 3.0 + push),
        );
        state
    }
}

/// The font of the battle buttons' labels: style 0 with the original's own I, W, N, S, F and O
/// (`scripts/button_label_font.py`).
fn battle_font() -> Rc<bm::BmFont> {
    thread_local! {
        static FONT: RefCell<Option<Rc<bm::BmFont>>> = const { RefCell::new(None) };
    }
    FONT.with(|font| {
        font.borrow_mut()
            .get_or_insert_with(|| {
                Rc::new(
                    bm::BmFont::from_bytes(
                        include_str!("../assets/button/battle_label.fnt"),
                        include_bytes!("../assets/button/battle_label.png"),
                    )
                    .expect("the battle buttons' font loads"),
                )
            })
            .clone()
    })
}

impl App {
    /// A battle panel button at `at` in the original's pixels, its label from the text `key`.
    /// True on the frame it is clicked.
    pub(crate) fn battle_button(
        &mut self,
        lay: &Layout,
        key: &str,
        at: (f32, f32),
        enabled: bool,
        input: &Input,
    ) -> bool {
        let s = lay.scale;
        let tex = battle_texture(!enabled);
        let size = vec2(tex.width(), tex.height());
        let p = lay.point(at.0, at.1);
        let state = if enabled {
            ui::hit(Rect::new(p.x, p.y, size.x * s, size.y * s), true, input)
        } else {
            ui::Hit::None
        };
        let look = match (enabled, state.held()) {
            (false, _) => Look::Locked,
            (_, true) => Look::Pressed,
            _ => Look::Up,
        };
        crate::menu_bar_art::draw_button(&tex, p, size * s, s, look == Look::Pressed, WHITE);
        let push = f32::from(u8::from(look == Look::Pressed));
        let label = self.text(key);
        let font = battle_font();
        self.draw_styled_in(
            &font,
            (lay.origin.x, lay.origin.y, s),
            &Style::flat(
                0,
                match look {
                    Look::Locked => rgb(142, 97, 36),
                    _ => rgb(52, 40, 28),
                },
            )
            .shadow()
            .edge(Color::from_rgba(255, 196, 56, 100))
            .center(),
            &label,
            (at.0 + size.x / 2.0 + push, at.1 + 1.0 + push),
        );
        state.clicked()
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
        for bytes in [
            include_bytes!("../assets/button/battle.png").as_slice(),
            include_bytes!("../assets/button/battle_locked.png").as_slice(),
        ] {
            let size = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
            assert_eq!((size(16), size(20)), (26, 10));
        }
        let fnt = include_str!("../assets/button/label.fnt");
        let i = fnt.lines().find(|l| l.starts_with("char id=73 ")).unwrap();
        assert!(i.contains("width=3") && i.contains("xadvance=4"), "{i}");
    }
}
