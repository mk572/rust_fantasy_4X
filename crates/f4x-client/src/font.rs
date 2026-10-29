//! The one way the client draws text in the original's bitmap fonts: a [`Style`] (font,
//! ink, effect, alignment) and [`App::draw_styled`], which draws it, and one word wrapper,
//! [`App::bm_wrap`]. The named entry points of `frame.rs` (`bm_text`, `bm_text_set`,
//! `bm_text_shadowed`, `bm_text_outlined`) only build a style.

use macroquad::prelude::*;

use crate::frame::Align;
use crate::view::App;

/// Where the colour of the letters comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Ink {
    /// The font's colour set that is closest to this colour (falls back to the plain font
    /// when the original fonts were not extracted).
    Nearest(Color),
    /// One of the font's sixteen colour sets by number, as the original's `Set_Font_Style`.
    Set(usize),
    /// One flat colour: the lightest colour set, tinted.
    Flat(Color),
}

impl Ink {
    /// The same kind of ink in black, for the shadow and the outline.
    fn black(self) -> Ink {
        let black = Color::new(0.0, 0.0, 0.0, 1.0);
        match self {
            Ink::Flat(_) => Ink::Flat(black),
            Ink::Nearest(_) | Ink::Set(_) => Ink::Nearest(black),
        }
    }
}

/// What is drawn around the letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    Plain,
    /// One black copy a pixel down and to the right.
    Shadow,
    /// Black copies on four sides and the lower right, as the original's gold, mana and
    /// income figures.
    Outline,
}

impl Effect {
    /// The offsets, in the original's pixels, of the black copies drawn first.
    pub(crate) fn behind(self) -> &'static [(f32, f32)] {
        match self {
            Effect::Plain => &[],
            Effect::Shadow => &[(1.0, 1.0)],
            Effect::Outline => &[(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.0)],
        }
    }
}

/// How one line of text looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Style {
    pub(crate) font: usize,
    pub(crate) ink: Ink,
    pub(crate) effect: Effect,
    pub(crate) align: Align,
}

impl Style {
    /// Left-aligned and plain.
    pub(crate) fn new(font: usize, ink: Ink) -> Style {
        Style {
            font,
            ink,
            effect: Effect::Plain,
            align: Align::Left,
        }
    }

    pub(crate) fn effect(self, effect: Effect) -> Style {
        Style { effect, ..self }
    }

    pub(crate) fn align(self, align: Align) -> Style {
        Style { align, ..self }
    }
}

/// The left edge of a line of `width` that is placed at `x` by `align`.
pub(crate) fn aligned(x: f32, width: f32, align: Align) -> f32 {
    match align {
        Align::Left => x,
        Align::Center => x - width / 2.0,
        Align::Right => x - width,
    }
}

/// Break `text` into lines no wider than `width`, measuring each candidate line with `measure`.
/// A word wider than `width` stands on a line of its own.
pub(crate) fn wrap_lines(text: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let attempt = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && measure(&attempt) > width {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = attempt;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

impl App {
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
        for &(dx, dy) in style.effect.behind() {
            self.draw_line_of_text(view, style, style.ink.black(), text, x + dx, y + dy);
        }
        self.draw_line_of_text(view, style, style.ink, text, x, y)
    }

    fn draw_line_of_text(
        &mut self,
        (ox, oy, s): (f32, f32, f32),
        style: &Style,
        ink: Ink,
        text: &str,
        x: f32,
        y: f32,
    ) -> f32 {
        let font = style.font;
        if let (Ink::Nearest(color), false) = (ink, self.art.has_fonts()) {
            // No bitmap fonts extracted: the plain font, in the colour asked for.
            let size = (self.art.text_height(font) + 1.0) * s;
            let w = measure_text(text, None, size as u16, 1.0).width / s;
            let left = aligned(x, w, style.align);
            let baseline = oy + (y + self.art.text_height(font)) * s;
            draw_text(text, ox + left * s, baseline, size, color);
            return w;
        }
        let w = self.art.text_width(font, text);
        let (px, py) = (ox + aligned(x, w, style.align) * s, oy + y * s);
        match ink {
            Ink::Nearest(color) => {
                let set = self.art.nearest_font_set(font, color);
                self.art.draw_bitmap_text(font, set, text, px, py, s);
            }
            Ink::Set(set) => {
                self.art.draw_bitmap_text(font, set, text, px, py, s);
            }
            Ink::Flat(color) => self.art.draw_flat_text(font, text, (px, py), s, color),
        }
        w
    }

    /// Words wrapped to `width` pixels in one of the original's bitmap fonts.
    pub(crate) fn bm_wrap(&self, font: usize, text: &str, width: f32) -> Vec<String> {
        wrap_lines(text, width, |line| self.art.text_width(font, line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_placed_by_its_alignment() {
        assert_eq!(aligned(100.0, 40.0, Align::Left), 100.0);
        assert_eq!(aligned(100.0, 40.0, Align::Center), 80.0);
        assert_eq!(aligned(100.0, 40.0, Align::Right), 60.0);
    }

    #[test]
    fn a_shadow_is_one_copy_and_an_outline_five_before_the_letters() {
        assert!(Effect::Plain.behind().is_empty());
        assert_eq!(Effect::Shadow.behind(), &[(1.0, 1.0)]);
        assert_eq!(Effect::Outline.behind().len(), 5);
        assert!(Effect::Outline.behind().contains(&(1.0, 1.0)));
    }

    #[test]
    fn the_shadow_of_an_ink_keeps_its_kind() {
        let red = Color::new(1.0, 0.0, 0.0, 1.0);
        assert!(matches!(Ink::Flat(red).black(), Ink::Flat(c) if c.r == 0.0));
        assert!(matches!(Ink::Nearest(red).black(), Ink::Nearest(c) if c.r == 0.0));
        assert!(matches!(Ink::Set(3).black(), Ink::Nearest(c) if c.r == 0.0));
    }

    #[test]
    fn a_style_is_left_aligned_and_plain_until_told_otherwise() {
        let style = Style::new(2, Ink::Set(1));
        assert_eq!((style.effect, style.align), (Effect::Plain, Align::Left));
        let style = style.effect(Effect::Shadow).align(Align::Right);
        assert_eq!((style.effect, style.align), (Effect::Shadow, Align::Right));
    }

    #[test]
    fn words_wrap_at_the_width_and_a_long_word_stands_alone() {
        let chars = |s: &str| s.len() as f32;
        assert_eq!(wrap_lines("aa bb cc dd", 5.0, chars), ["aa bb", "cc dd"]);
        assert_eq!(
            wrap_lines("a verylongword b", 5.0, chars),
            ["a", "verylongword", "b"]
        );
        assert!(wrap_lines("   ", 5.0, chars).is_empty());
        assert_eq!(wrap_lines("one", 1.0, chars), ["one"]);
    }
}
