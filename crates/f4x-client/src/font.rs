//! The one way the client draws text in the original's bitmap fonts: a [`Style`] (font,
//! ink, effect, alignment) and [`App::draw_styled`], which draws it, and one word wrapper,
//! [`App::bm_wrap`]. A screen builds a style (`Style::nearest(font, colour).center()`) and
//! hands it to `draw_styled`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use macroquad::prelude::*;
use macroquad_bitmap_font_engine as bm;

use crate::frame::Align;
use crate::view::App;

/// Where the colour of the letters comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Ink {
    /// The font's colour set that is closest to this colour.
    Nearest(Color),
    /// One of the font's sixteen colour sets by number, as the original's `Set_Font_Style`.
    Set(usize),
    /// One flat colour: the lightest colour set, tinted.
    Flat(Color),
}

/// An 8-bit colour, for the palette entries the original names by number.
pub(crate) const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

/// The shadow of the surveyor and the road planner: `Set_Outline_Color(148)`.
pub(crate) const SURVEY_EDGE: Color = rgb(0x5D, 0x5D, 0x79);

/// The colour of the shadow and the outline unless a screen sets another
/// (`Set_Outline_Color(0)`, palette entry 0).
pub(crate) const EDGE_BLACK: Color = rgb(0, 0, 0);

/// What is drawn around the letters: the original's `Set_Font_Style_Shadow_Down`, `_Shadow_Up`,
/// `_Shadow_Heavy`, `_Outline` and `_Outline_Heavy` (`Print_Display` in `Fonts.c`). The engine
/// draws the copies, in the style's edge colour (`Set_Outline_Color`).
pub(crate) use bm::Effect;

/// How one line of text looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Style {
    pub(crate) font: usize,
    pub(crate) ink: Ink,
    pub(crate) effect: Effect,
    /// The colour of the shadow or outline copies (`Set_Outline_Color`).
    pub(crate) edge: Color,
    /// The light-source reflection on the top and left edges of the letters, if any.
    pub(crate) highlight: Option<Color>,
    /// The rest of the top and left edges (`None`: as `highlight`).
    pub(crate) highlight_edge: Option<Color>,
    pub(crate) align: Align,
}

impl Style {
    fn new(font: usize, ink: Ink) -> Style {
        Style {
            font,
            ink,
            effect: Effect::Plain,
            edge: EDGE_BLACK,
            highlight: None,
            highlight_edge: None,
            align: Align::Left,
        }
    }

    /// The font's colour set closest to `color`; left-aligned and plain.
    pub(crate) fn nearest(font: usize, color: Color) -> Style {
        Style::new(font, Ink::Nearest(color))
    }

    /// [`Style::nearest`] with the original's `Set_Font_Style_Shadow_Down`: a black copy a pixel
    /// down and to the right.
    pub(crate) fn sh_nearest(font: usize, color: Color) -> Style {
        Style::nearest(font, color).shadow()
    }

    /// [`Style::set`] with the shadow.
    pub(crate) fn sh_set(font: usize, set: usize) -> Style {
        Style::set(font, set).shadow()
    }

    /// White with the shadow.
    pub(crate) fn shadowed(font: usize) -> Style {
        Style::sh_nearest(font, WHITE)
    }

    /// One of the font's colour sets by number.
    pub(crate) fn set(font: usize, set: usize) -> Style {
        Style::new(font, Ink::Set(set))
    }

    /// The text of the army and city lists: the ramp of `Set_Font_Colors_15(1, 236 129 129 ...)`
    /// with `Shadow_Down` and `Set_Outline_Color(231)`, an entry of the lists' own palette.
    pub(crate) fn list_text(font: usize) -> Style {
        Style::inked(font, Ink::Set(crate::font_ramp::LIST_TEXT))
            .shadow()
            .edge(rgb(0x10, 0x0C, 0x20))
    }

    /// The text of the city list: like [`Style::list_text`], with the ramp that city list's code leaves.
    pub(crate) fn city_list_text(font: usize) -> Style {
        Style::inked(font, Ink::Set(crate::font_ramp::CITY_LIST_TEXT))
            .shadow()
            .edge(rgb(0x10, 0x0C, 0x20))
    }

    /// The text of the unit view (the unit window, the production panel, the hire window): the
    /// original's `Set_Font_Style_Outline` with a ramp by font and `Set_Outline_Color(251)`, an entry
    /// of the unit view's own palette.
    pub(crate) fn unit_view(font: usize) -> Style {
        let ramp = match font {
            4 => crate::font_ramp::UV_TITLE,
            _ => crate::font_ramp::UV_STATS,
        };
        Style::inked(font, Ink::Set(ramp))
            .effect(Effect::Outline)
            .edge(rgb(0x14, 0x45, 0x45))
    }

    /// The unit sheet's abilities list (`USW_DrawListPage`): style 2 in the `UV_LIST` ramp, outlined.
    pub(crate) fn unit_list() -> Style {
        Style::inked(2, Ink::Set(crate::font_ramp::UV_LIST))
            .effect(Effect::Outline)
            .edge(rgb(0x14, 0x45, 0x45))
    }

    /// The race and population line, the turns to build: style 1 in the ramp of
    /// `Set_Font_Colors_15(0, 190 179)` with `Shadow_Down` and `Set_Outline_Color(19)`.
    pub(crate) fn city_figures() -> Style {
        Style::inked(1, Ink::Set(crate::font_ramp::CITY_FIG))
            .shadow()
            .edge(rgb(0x51, 0x3C, 0x30))
    }

    /// The surveyor's and road planner's text in its ramp (`Set_Font_Colors_15(1, 8 161 161)`,
    /// `Shadow_Down`, `Set_Outline_Color(148)`, a grey-blue of the base palette).
    pub(crate) fn survey_ramp() -> Style {
        Style::inked(1, Ink::Set(crate::font_ramp::SURVEY))
            .shadow()
            .edge(SURVEY_EDGE)
    }

    /// The surveyor's white text (`Shadow_Down(1, 0)`) with the same edge.
    pub(crate) fn survey_white(font: usize) -> Style {
        Style::sh_set(font, 0).edge(SURVEY_EDGE)
    }

    /// The gold text of the wizard screen: `Shadow_Down` in the ramp `colors[i] = 177 + i`.
    pub(crate) fn gold_shadowed(font: usize) -> Style {
        Style::inked(font, Ink::Set(crate::font_ramp::GOLD_TEXT)).shadow()
    }

    /// A style with the ink given (a colour set number as a ramp of `font_ramp.rs`).
    pub(crate) fn inked(font: usize, ink: Ink) -> Style {
        Style::new(font, ink)
    }

    /// One flat colour.
    pub(crate) fn flat(font: usize, color: Color) -> Style {
        Style::new(font, Ink::Flat(color))
    }

    /// White with a black outline, as the original's gold, mana and income figures.
    pub(crate) fn outlined(font: usize) -> Style {
        Style::nearest(font, WHITE).effect(Effect::Outline)
    }

    pub(crate) fn shadow(self) -> Style {
        self.effect(Effect::Shadow)
    }

    pub(crate) fn effect(self, effect: Effect) -> Style {
        Style { effect, ..self }
    }

    /// The colour of the shadow or outline, from the original's `Set_Outline_Color`.
    pub(crate) fn edge(self, edge: Color) -> Style {
        Style { edge, ..self }
    }

    /// The top-left corner pixels of the letters painted in `corner` over the ink, and the rest
    /// of their top and left edges in `edge`.
    pub(crate) fn highlight(mut self, corner: Color, edge: Color) -> Style {
        self.highlight = Some(corner);
        self.highlight_edge = Some(edge);
        self
    }

    pub(crate) fn center(self) -> Style {
        self.align(Align::Center)
    }

    pub(crate) fn right(self) -> Style {
        self.align(Align::Right)
    }

    pub(crate) fn align(self, align: Align) -> Style {
        Style { align, ..self }
    }
}

/// The engine's alignment for the frame's.
fn engine_align(align: Align) -> bm::Align {
    match align {
        Align::Left => bm::Align::Left,
        Align::Center => bm::Align::Center,
        Align::Right => bm::Align::Right,
    }
}

thread_local! {
    static FONTS: RefCell<HashMap<usize, Rc<bm::BmFont>>> = RefCell::new(HashMap::new());
}

/// A font style as a BMFont of the embedded files, made on first use and shared by every screen.
pub(crate) fn shared_font(style: usize) -> Rc<bm::BmFont> {
    FONTS.with(|fonts| {
        fonts
            .borrow_mut()
            .entry(style)
            .or_insert_with(|| Rc::new(crate::art::load_font(style)))
            .clone()
    })
}

/// The font style of the text that is not in the original's layout (the options screen, the
/// prompts of the battle and the road planner, the plain buttons).
const PLAIN_STYLE: usize = 5;

/// The scale of the plain font that comes closest to a text `size` in window pixels.
fn plain_scale(font: &bm::BmFont, size: f32) -> f32 {
    (size / font.line_height).round().max(1.0)
}

/// Draw `text` in the plain font, as `macroquad::draw_text` did: `(x, baseline)` in window
/// pixels, `size` the height of a line, `color` the ink.
pub(crate) fn plain_text(text: impl AsRef<str>, x: f32, baseline: f32, size: f32, color: Color) {
    plain_text_in(PLAIN_STYLE, text, x, baseline, size, color);
}

/// [`plain_text`] in font style `style` (5 is the headline face; the console reads better in 4).
pub(crate) fn plain_text_in(
    style: usize,
    text: impl AsRef<str>,
    x: f32,
    baseline: f32,
    size: f32,
    color: Color,
) {
    let font = shared_font(style);
    let params = bm::TextParams {
        scale: plain_scale(&font, size),
        ink: bm::Ink::Tint(color),
        ..Default::default()
    };
    font.draw_text_ex(text.as_ref(), x, baseline, &params);
}

/// Width and height of `text` in the plain font, in window pixels.
pub(crate) fn plain_size(text: &str, size: f32) -> (f32, f32) {
    plain_size_in(PLAIN_STYLE, text, size)
}

/// [`plain_size`] in font style `style`.
pub(crate) fn plain_size_in(style: usize, text: &str, size: f32) -> (f32, f32) {
    let font = shared_font(style);
    let scale = plain_scale(&font, size);
    (font.measure_text(text, scale).width, font.base * scale)
}

impl App {
    /// The ink of text in a wizard's banner colour: the colour set of the palette colour `table`
    /// names for that banner (each screen has its own list: `banner_colors[]` in the original), or
    /// the banner's colour itself for a banner the list lacks.
    pub(crate) fn banner_ink(&self, owner: u8, table: &[u8]) -> Ink {
        self.banner_set_ink(self.banner_index(owner), table)
    }

    /// [`App::banner_ink`] for a banner number.
    pub(crate) fn banner_set_ink(&self, banner: u8, table: &[u8]) -> Ink {
        match table.get(usize::from(banner)) {
            Some(&index) => Ink::Set(crate::font_ramp::flat(index)),
            None => Ink::Flat(self.art.banner_color(banner)),
        }
    }

    /// The ink of a city enchantment's name: the colour of its caster's banner
    /// (`Print_City_Enchantment_List`), grey-brown for no wizard.
    pub(crate) fn city_ench_ink(&self, owner: u8) -> Ink {
        if self
            .game
            .wizard(f4x_core::player::PlayerId(owner))
            .is_none()
        {
            return Ink::Set(crate::font_ramp::flat(6));
        }
        self.banner_ink(owner, &[221, 218, 125, 201, 211, 50])
    }

    /// A title the original draws as a stack of copies in four colour sets, centred on `x`: the
    /// farthest ramp two pixels to the lower right (three copies), the middle one pixel right and
    /// below, the nearest one pixel to the lower right, and the top ramp on the spot
    /// (`ArmyList_Screen_Draw`, `CityList_Screen_Draw`). `ramps` is far, middle, near, top.
    pub(crate) fn draw_title_stack(
        &mut self,
        view: (f32, f32, f32),
        font: usize,
        ramps: [usize; 4],
        text: &str,
        (x, y): (f32, f32),
    ) {
        let copies: [(usize, &[(f32, f32)]); 3] = [
            (ramps[0], &[(2.0, 2.0), (1.0, 2.0), (2.0, 1.0)]),
            (ramps[1], &[(1.0, 0.0), (0.0, 1.0)]),
            (ramps[2], &[(1.0, 1.0)]),
        ];
        for (ramp, offsets) in copies {
            let style = Style::inked(font, Ink::Set(ramp)).center();
            for &(dx, dy) in offsets {
                self.draw_styled(view, &style, text, x + dx, y + dy);
            }
        }
        let top = Style::inked(font, Ink::Set(ramps[3])).center();
        self.draw_styled(view, &top, text, x, y);
    }

    /// Draw one line in `style` at `(x, y)` in the original's pixels under a view of
    /// `(origin x, origin y, scale)`. Returns the width in the original's pixels.
    pub(crate) fn draw_styled(
        &mut self,
        view: (f32, f32, f32),
        style: &Style,
        text: &str,
        x: f32,
        y: f32,
    ) -> f32 {
        let font = self.art.bm_font(style.font);
        draw_line_in(&font, view, style, style.ink, text, (x, y))
    }

    /// [`App::draw_styled`] with `font` instead of the style's own BMFont (the same atlas with
    /// other metrics, such as the kerned face of the menu bar's labels).
    pub(crate) fn draw_styled_in(
        &mut self,
        font: &bm::BmFont,
        view: (f32, f32, f32),
        style: &Style,
        text: &str,
        (x, y): (f32, f32),
    ) -> f32 {
        draw_line_in(font, view, style, style.ink, text, (x, y))
    }

    /// Words wrapped to `width` pixels in one of the original's bitmap fonts.
    pub(crate) fn bm_wrap(&self, font: usize, text: &str, width: f32) -> Vec<String> {
        bm::wrap_lines(text, width, |line| self.art.text_width(font, line))
    }
}

/// One line drawn by the font engine (`macroquad_bitmap_font_engine`) from `font`, in the ink
/// asked for. Returns the width in the original's pixels.
fn draw_line_in(
    font: &bm::BmFont,
    (ox, oy, s): (f32, f32, f32),
    style: &Style,
    ink: Ink,
    text: &str,
    (x, y): (f32, f32),
) -> f32 {
    let w = font.measure_text(text, 1.0).width;
    let engine_ink = match ink {
        // A font of one colour set (a free font) cannot pick one: it is tinted instead.
        Ink::Nearest(color) if font.colour_sets() == 1 => bm::Ink::Tint(color),
        Ink::Nearest(color) => bm::Ink::Nearest(color),
        Ink::Set(set) => bm::Ink::Set(set),
        Ink::Flat(color) => bm::Ink::Tint(color),
    };
    let params = bm::TextParams {
        scale: s,
        ink: engine_ink,
        effect: style.effect,
        edge: style.edge,
        highlight: style.highlight,
        highlight_edge: style.highlight_edge,
        ..Default::default()
    };
    let left = ox + engine_align(style.align).anchor(x, w) * s;
    font.draw_text_ex(text, left, oy + (y + font.base) * s, &params);
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_has_a_black_edge_until_told_otherwise() {
        assert_eq!(Style::set(1, 2).edge, EDGE_BLACK);
        assert_eq!(Style::set(1, 2).edge(rgb(16, 12, 32)).edge, rgb(16, 12, 32));
    }

    #[test]
    fn a_style_is_left_aligned_and_plain_until_told_otherwise() {
        let style = Style::set(2, 1);
        assert_eq!((style.effect, style.align), (Effect::Plain, Align::Left));
        let style = style.shadow().right();
        assert_eq!((style.effect, style.align), (Effect::Shadow, Align::Right));
        assert_eq!(style.center().align, Align::Center);
        assert_eq!(Style::outlined(3).effect, Effect::Outline);
        assert_eq!(Style::flat(1, WHITE).ink, Ink::Flat(WHITE));
    }
}
