//! The alchemy window's picture without its title, and its stone Cancel and OK without their
//! words (`scripts/button_text_free.py`: `assets/button/alchemy_frame.png` 166 by 67 and
//! `alchemy_stone.png` 49 by 16). The font engine draws the title and the labels from text keys.

use macroquad::prelude::*;
use std::cell::RefCell;

use crate::font::{rgb, Effect, Style};
use crate::frame::Layout;
use crate::input::Input;
use crate::ui;
use crate::view::App;

/// The space between the words of the title, in the original's pixels.
const WORD_GAP: f32 = 3.0;

fn load(bytes: &[u8]) -> Texture2D {
    let tex = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
    tex.set_filter(FilterMode::Nearest);
    tex
}

fn frame_texture() -> Texture2D {
    thread_local! {
        static TEX: RefCell<Option<Texture2D>> = const { RefCell::new(None) };
    }
    TEX.with(|t| {
        t.borrow_mut()
            .get_or_insert_with(|| load(include_bytes!("../assets/button/alchemy_frame.png")))
            .clone()
    })
}

fn stone_texture() -> Texture2D {
    thread_local! {
        static TEX: RefCell<Option<Texture2D>> = const { RefCell::new(None) };
    }
    TEX.with(|t| {
        t.borrow_mut()
            .get_or_insert_with(|| load(include_bytes!("../assets/button/alchemy_stone.png")))
            .clone()
    })
}

impl App {
    /// The window's picture at `at` and its title, from the text `title_key`.
    pub(crate) fn draw_alchemy_frame(&mut self, lay: &Layout, title_key: &str, at: (f32, f32)) {
        let tex = frame_texture();
        let p = lay.point(at.0, at.1);
        draw_texture_ex(
            &tex,
            p.x,
            p.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(tex.width(), tex.height()) * lay.scale),
                ..Default::default()
            },
        );
        // Embossed as the original: a gold shadow down and to the right, a brown one up and to the
        // left. The original's words are set closer than the font's space.
        let title = self.text(title_key);
        let font = crate::menu_bar_art::label_font();
        let words: Vec<&str> = title.split(' ').collect();
        let widths: Vec<f32> = words
            .iter()
            .map(|w| font.measure_text(w, 1.0).width)
            .collect();
        let total = widths.iter().sum::<f32>() + WORD_GAP * (words.len() - 1) as f32;
        for style in [
            Style::flat(4, rgb(8, 4, 4))
                .effect(Effect::Shadow)
                .edge(Color::from_rgba(255, 182, 44, 128)),
            Style::flat(4, rgb(8, 4, 4))
                .effect(Effect::ShadowUp)
                .edge(Color::from_rgba(121, 85, 36, 128)),
        ] {
            let mut x = at.0 + 84.0 - total / 2.0;
            for (word, width) in words.iter().zip(&widths) {
                self.draw_styled_in(
                    &font,
                    (lay.origin.x, lay.origin.y, lay.scale),
                    &style,
                    word,
                    (x, at.1 + 11.0),
                );
                x += width + WORD_GAP;
            }
        }
    }

    /// A stone button at `at` with its label from the text `key`; true when clicked.
    pub(crate) fn alchemy_stone_button(
        &mut self,
        lay: &Layout,
        key: &str,
        at: (f32, f32),
        input: &Input,
    ) -> bool {
        let s = lay.scale;
        let tex = stone_texture();
        let size = vec2(tex.width(), tex.height());
        let p = lay.point(at.0, at.1);
        let state = ui::hit(Rect::new(p.x, p.y, size.x * s, size.y * s), true, input);
        crate::menu_bar_art::draw_button(&tex, p, size * s, s, state.held(), WHITE);
        let push = f32::from(u8::from(state.held()));
        let label = self.text(key);
        self.draw_styled(
            (lay.origin.x, lay.origin.y, s),
            &Style::flat(4, rgb(52, 40, 28))
                .shadow()
                .edge(Color::from_rgba(243, 235, 231, 200))
                .center(),
            &label,
            at.0 + size.x / 2.0 + push,
            at.1 + 3.0 + push,
        );
        state.clicked()
    }
}
