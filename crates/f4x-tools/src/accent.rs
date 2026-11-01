//! Letters the original's ASCII fonts lack, drawn from the ones they have: accented letters
//! (acute, grave, circumflex, caron, tilde, diaeresis, ring, breve, dot, macron, double acute,
//! cedilla, ogonek, a tick for the Czech and Slovak ď ť ľ), strokes (ł ø đ), ligatures (æ œ),
//! the eszett, the dotless i, inverted marks, guillemets, typographic quotes and dashes. They
//! cover French, Spanish, Portuguese, Italian, German, Dutch, Danish, Norwegian, Swedish,
//! Finnish, Czech, Slovak, Polish, Hungarian, Romanian, Croatian and Turkish; Icelandic þ and ð
//! are not drawn. Every glyph is made from the style's own letters, so it keeps the style's
//! stroke and colours. See `docs/extract-art.md` section 5.

/// A glyph of the BMFont export: colour indexes (0 is clear), `width` by `height`, drawn
/// `y_offset` rows below the top of the line (negative: above it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BmGlyph {
    pub(crate) ch: char,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) y_offset: i32,
    pub(crate) advance: usize,
    pub(crate) px: Vec<u8>,
}

impl BmGlyph {
    fn at(&self, row: usize, col: usize) -> u8 {
        if row < self.height && col < self.width {
            self.px[row * self.width + col]
        } else {
            0
        }
    }

    fn row_is_empty(&self, row: usize) -> bool {
        (0..self.width).all(|c| self.at(row, c) == 0)
    }

    /// The first and last row with a pixel.
    fn extent(&self) -> Option<(usize, usize)> {
        let first = (0..self.height).find(|&r| !self.row_is_empty(r))?;
        let last = (0..self.height).rev().find(|&r| !self.row_is_empty(r))?;
        Some((first, last))
    }

    /// The colour index most of the glyph has.
    fn colour(&self) -> u8 {
        let mut counts = [0usize; 16];
        for &c in &self.px {
            counts[usize::from(c & 0x0F)] += 1;
        }
        (1..16u8)
            .max_by_key(|&c| counts[usize::from(c)])
            .unwrap_or(1)
    }

    fn renamed(&self, ch: char) -> BmGlyph {
        BmGlyph { ch, ..self.clone() }
    }
}

/// A grid to draw on, grown from a glyph: the glyph is put `left` columns and `top` rows in, and
/// `grow_right` and `grow_down` more are added after it.
struct Canvas {
    width: usize,
    height: usize,
    px: Vec<u8>,
}

impl Canvas {
    fn from(base: &BmGlyph, (left, top): (usize, usize), (right, down): (usize, usize)) -> Canvas {
        let (width, height) = (left + base.width + right, top + base.height + down);
        let mut px = vec![0u8; width * height];
        for row in 0..base.height {
            for col in 0..base.width {
                px[(top + row) * width + left + col] = base.px[row * base.width + col];
            }
        }
        Canvas { width, height, px }
    }

    fn blank(width: usize, height: usize) -> Canvas {
        Canvas {
            width,
            height,
            px: vec![0u8; width * height],
        }
    }

    fn set(&mut self, row: usize, col: usize, colour: u8) {
        if row < self.height && col < self.width {
            self.px[row * self.width + col] = colour;
        }
    }

    /// A pattern (rows of `#` and `.`) with every pixel `scale` by `scale`, top left at `(row, col)`.
    fn stamp(&mut self, pattern: &[&str], scale: usize, (row, col): (usize, usize), colour: u8) {
        for (r, line) in pattern.iter().enumerate() {
            for c in line
                .chars()
                .enumerate()
                .filter(|&(_, ch)| ch == '#')
                .map(|(c, _)| c)
            {
                for dr in 0..scale {
                    for dc in 0..scale {
                        self.set(row + r * scale + dr, col + c * scale + dc, colour);
                    }
                }
            }
        }
    }

    fn into_glyph(self, base: &BmGlyph, ch: char, y_offset: i32) -> BmGlyph {
        BmGlyph {
            ch,
            width: self.width,
            height: self.height,
            y_offset,
            advance: base.advance + self.width.saturating_sub(base.width),
            px: self.px,
        }
    }
}

/// What is drawn on or under a letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    Acute,
    Grave,
    Circumflex,
    Caron,
    Breve,
    Tilde,
    Diaeresis,
    Ring,
    Dot,
    Macron,
    DoubleAcute,
    Cedilla,
    Ogonek,
    /// The tick to the right of the Czech and Slovak ď ť ľ and their capitals.
    Tick,
}

impl Mark {
    /// The pattern of a mark at the font's own scale; `Diaeresis` is spread over the letter.
    fn pattern(self) -> &'static [&'static str] {
        match self {
            Mark::Acute => &[".#", "#."],
            Mark::Grave => &["#.", ".#"],
            Mark::Circumflex => &[".#.", "#.#"],
            Mark::Caron => &["#.#", ".#."],
            Mark::Breve => &["#..#", ".##."],
            Mark::Tilde => &[".#.#", "#.#."],
            Mark::Ring => &[".#.", "#.#", ".#."],
            Mark::Dot => &["#"],
            Mark::Macron => &["###"],
            Mark::DoubleAcute => &[".#.#", "#.#."],
            Mark::Cedilla => &[".#", "#."],
            Mark::Ogonek => &["#.", ".#"],
            Mark::Tick => &["#", "#"],
            Mark::Diaeresis => &["#"],
        }
    }
}

/// `base` with `mark` on it, drawn `scale` pixels per pixel of the pattern. A mark above a letter
/// leaves one clear row; one that does not fit grows the glyph upward (drawn above the line, a
/// negative `y_offset`), under a letter downward, and a mark wider than the letter widens it.
fn with_mark(base: &BmGlyph, ch: char, mark: Mark, scale: usize) -> BmGlyph {
    let Some((top, bottom)) = base.extent() else {
        return base.renamed(ch);
    };
    let colour = base.colour();
    let pattern = mark.pattern();
    let (ph, pw) = (
        pattern.len() * scale,
        pattern.iter().map(|r| r.len()).max().unwrap_or(1) * scale,
    );
    let w = base.width;
    match mark {
        Mark::Cedilla | Mark::Ogonek => {
            let width = w.max(pw);
            let left = (width - w) / 2;
            let down = (bottom + 1 + ph).saturating_sub(base.height);
            let mut canvas = Canvas::from(base, (left, 0), (width - w - left, down));
            let x = if mark == Mark::Ogonek {
                width - pw
            } else {
                (width - pw) / 2
            };
            canvas.stamp(pattern, scale, (bottom + 1, x), colour);
            canvas.into_glyph(base, ch, base.y_offset)
        }
        Mark::Tick => {
            let width = w + 1 + scale;
            let mut canvas = Canvas::from(base, (0, 0), (width - w, 0));
            canvas.stamp(pattern, scale, (top, w + 1), colour);
            canvas.into_glyph(base, ch, base.y_offset)
        }
        _ => {
            let width = w.max(pw);
            let left = (width - w) / 2;
            let extra = (ph + 1).saturating_sub(top);
            let mut canvas = Canvas::from(base, (left, extra), (width - w - left, 0));
            let row = top + extra - 1 - ph;
            if mark == Mark::Diaeresis {
                // Two dots over the letter, spread: a quarter in from each side.
                let first = w.saturating_sub(scale) / 4;
                let second = w.saturating_sub(scale + first);
                for x in [first, second] {
                    canvas.stamp(pattern, scale, (row + ph - scale, left + x), colour);
                }
            } else {
                canvas.stamp(pattern, scale, (row, (width - pw) / 2), colour);
            }
            canvas.into_glyph(base, ch, base.y_offset - extra as i32)
        }
    }
}

/// The dotless i: the `i` without the rows of its dot.
fn dotless_i(i: &BmGlyph) -> BmGlyph {
    let Some((first, _)) = i.extent() else {
        return i.renamed('ı');
    };
    // The dot ends at the first clear row under the first run of rows.
    let Some(gap) = (first..i.height).find(|&r| i.row_is_empty(r)) else {
        return i.renamed('ı');
    };
    let mut glyph = i.renamed('ı');
    for row in 0..=gap {
        for col in 0..i.width {
            glyph.px[row * i.width + col] = 0;
        }
    }
    glyph
}

/// An eszett drawn to the shape of the letter: a straight stem on the left, an upper half-circle
/// and a slightly larger lower one on its right, the lower one's end left unjoined to the stem
/// along the bottom. The height is the ascender height of the style's `b`; the stem is as thick
/// as `b`'s stem.
fn eszett(small_b: &BmGlyph) -> BmGlyph {
    let Some((top, bottom)) = small_b.extent() else {
        return small_b.renamed('ß');
    };
    let colour = small_b.colour();
    let rows = bottom - top + 1;
    // The stem is the first run of the top row (an ascender is the stem alone).
    let stem = (0..small_b.width)
        .skip_while(|&c| small_b.at(top, c) == 0)
        .take_while(|&c| small_b.at(top, c) != 0)
        .count()
        .max(1);
    let width = small_b.width.max(stem + 3);
    // The two bowls share the middle bar; the lower one is the larger (it gets the rows over).
    let middle = (rows - 1) * 9 / 20;
    let (upper_right, lower_right) = (width - 2, width - 1);
    let corner = stem.div_ceil(2);
    let mut canvas = Canvas::blank(width, small_b.height);
    let draw = |canvas: &mut Canvas, (r0, r1): (usize, usize), right: usize| {
        for r in r0..=r1 {
            for c in 0..=right {
                let on_bar = r < r0 + stem || r + stem > r1;
                let on_side = c < stem || c + stem > right;
                let clipped = (r < r0 + corner || r + corner > r1) && c + corner > right;
                if (on_bar || on_side) && !clipped {
                    canvas.set(top + r, c, colour);
                }
            }
        }
    };
    draw(&mut canvas, (0, middle), upper_right);
    draw(&mut canvas, (middle, rows - 1), lower_right);
    // The stem runs straight to the bottom; the lower bowl's bar starts after a gap.
    for r in 0..rows {
        for c in 0..stem {
            canvas.set(top + r, c, colour);
        }
    }
    for r in rows.saturating_sub(stem)..rows {
        for c in stem..(stem + 1).min(lower_right) {
            canvas.px[(top + r) * canvas.width + c] = 0;
        }
    }
    canvas.into_glyph(small_b, 'ß', small_b.y_offset)
}

/// Two glyphs side by side, `gap` columns apart (negative: overlapping), the second drawn over.
fn join(first: &BmGlyph, second: &BmGlyph, gap: i32, ch: char) -> BmGlyph {
    let second_at = (first.width as i32 + gap).max(0) as usize;
    let width = second_at + second.width;
    let height = first.height.max(second.height);
    let mut px = vec![0u8; width * height];
    for row in 0..first.height {
        for col in 0..first.width {
            px[row * width + col] = first.px[row * first.width + col];
        }
    }
    for row in 0..second.height {
        for col in 0..second.width {
            let c = second.px[row * second.width + col];
            if c != 0 {
                px[row * width + second_at + col] = c;
            }
        }
    }
    BmGlyph {
        ch,
        width,
        height,
        y_offset: first.y_offset,
        advance: width + first.advance.saturating_sub(first.width),
        px,
    }
}

/// A line through a letter: the row (counted from the letter's top) and the columns it covers.
fn with_bar(base: &BmGlyph, ch: char, row_from_top: usize, columns: (usize, usize)) -> BmGlyph {
    let Some((top, _)) = base.extent() else {
        return base.renamed(ch);
    };
    let colour = base.colour();
    let width = base.width.max(columns.1);
    let mut canvas = Canvas::from(base, (0, 0), (width - base.width, 0));
    for c in columns.0..columns.1.min(width) {
        canvas.set(top + row_from_top, c, colour);
    }
    canvas.into_glyph(base, ch, base.y_offset)
}

/// A slash from the lower left to the upper right through a letter (ø, Ø).
fn with_slash(base: &BmGlyph, ch: char) -> BmGlyph {
    let Some((top, bottom)) = base.extent() else {
        return base.renamed(ch);
    };
    let colour = base.colour();
    let mut glyph = base.renamed(ch);
    let span = (bottom - top).max(1);
    for row in top..=bottom {
        let col = (bottom - row) * (base.width - 1) / span;
        glyph.px[row * base.width + col] = colour;
    }
    glyph
}

/// The glyph turned upside down within its own rows (¿ ¡).
fn flipped(base: &BmGlyph, ch: char) -> BmGlyph {
    let Some((top, bottom)) = base.extent() else {
        return base.renamed(ch);
    };
    let mut glyph = base.renamed(ch);
    for row in top..=bottom {
        let from = bottom - (row - top);
        for col in 0..base.width {
            glyph.px[row * base.width + col] = base.px[from * base.width + col];
        }
    }
    glyph
}

/// The glyph made wider by repeating its last column (dashes).
fn stretched(base: &BmGlyph, ch: char, extra: usize) -> BmGlyph {
    let mut canvas = Canvas::from(base, (0, 0), (extra, 0));
    for row in 0..base.height {
        let last = base.at(row, base.width - 1);
        for col in base.width..base.width + extra {
            canvas.set(row, col, last);
        }
    }
    canvas.into_glyph(base, ch, base.y_offset)
}

/// « or »: two chevrons the height of a small `o`, one apart.
fn guillemet(small_o: &BmGlyph, ch: char) -> BmGlyph {
    let Some((top, bottom)) = small_o.extent() else {
        return small_o.renamed(ch);
    };
    let colour = small_o.colour();
    let rows = bottom - top + 1;
    let mid = (rows - 1) / 2;
    let reach = mid.max(rows - 1 - mid);
    let width = reach + 1 + 2;
    let mut px = vec![0u8; width * small_o.height];
    for i in 0..rows {
        let depth = mid.abs_diff(i);
        for chevron in [0, 2] {
            let col = if ch == '«' { depth } else { reach - depth } + chevron;
            px[(top + i) * width + col] = colour;
        }
    }
    BmGlyph {
        ch,
        width,
        height: small_o.height,
        y_offset: small_o.y_offset,
        advance: small_o.advance + width.saturating_sub(small_o.width),
        px,
    }
}

/// The letters with a mark: (accented letters, their base letters, the mark). A small `i` is
/// not a base here: under a mark it loses its dot, see `marked_glyphs`.
const MARKED: &[(&str, &str, Mark)] = &[
    ("ÁÉÍÓÚÝĆĹŃŔŚŹ", "AEIOUYCLNRSZ", Mark::Acute),
    ("áéóúýćĺńŕśź", "aeouyclnrsz", Mark::Acute),
    ("ÀÈÌÒÙ", "AEIOU", Mark::Grave),
    ("àèòù", "aeou", Mark::Grave),
    ("ÂÊÎÔÛĈĜĤĴŜŴŶ", "AEIOUCGHJSWY", Mark::Circumflex),
    ("âêôûĉĝĥŝŵŷ", "aeoucghswy", Mark::Circumflex),
    ("ČĚŇŘŠŽ", "CENRSZ", Mark::Caron),
    ("čěňřšž", "cenrsz", Mark::Caron),
    ("ĂĞŬ", "AGU", Mark::Breve),
    ("ăğŭ", "agu", Mark::Breve),
    ("ÃÑÕĨŨ", "ANOIU", Mark::Tilde),
    ("ãñõũ", "anou", Mark::Tilde),
    ("ÄËÏÖÜŸ", "AEIOUY", Mark::Diaeresis),
    ("äëöüÿ", "aeouy", Mark::Diaeresis),
    ("ÅŮ", "AU", Mark::Ring),
    ("åů", "au", Mark::Ring),
    ("ĊĖĠŻİ", "CEGZI", Mark::Dot),
    ("ċėġż", "cegz", Mark::Dot),
    ("ĀĒĪŌŪ", "AEIOU", Mark::Macron),
    ("āēōū", "aeou", Mark::Macron),
    ("ŐŰ", "OU", Mark::DoubleAcute),
    ("őű", "ou", Mark::DoubleAcute),
    ("ÇŞŢȘȚĢĶĻŅŖ", "CSTSTGKLNR", Mark::Cedilla),
    ("çşţșțģķļņŗ", "cststgklnr", Mark::Cedilla),
    ("ĄĘĮŲ", "AEIU", Mark::Ogonek),
    ("ąęų", "aeu", Mark::Ogonek),
    ("ĽĎŤ", "LDT", Mark::Tick),
    ("ľďť", "ldt", Mark::Tick),
];

/// The small `i` with a mark: it is drawn without its dot.
const ON_I: &[(char, Mark)] = &[
    ('í', Mark::Acute),
    ('ì', Mark::Grave),
    ('î', Mark::Circumflex),
    ('ï', Mark::Diaeresis),
    ('ĩ', Mark::Tilde),
    ('ī', Mark::Macron),
    ('į', Mark::Ogonek),
];

fn marked_glyphs(ascii: &[BmGlyph], scale: usize) -> Vec<BmGlyph> {
    let find = |c: char| ascii.iter().find(|g| g.ch == c);
    let mut out = Vec::new();
    for &(accented, bases, mark) in MARKED {
        for (ch, base) in accented.chars().zip(bases.chars()) {
            if let Some(glyph) = find(base) {
                out.push(with_mark(glyph, ch, mark, scale));
            }
        }
    }
    if let Some(i) = find('i').map(dotless_i) {
        for &(ch, mark) in ON_I {
            out.push(with_mark(&i, ch, mark, scale));
        }
        out.push(i);
    }
    out
}

/// Every extra glyph for the style, from its ASCII glyphs; `scale` is 1 for a small font and 2 for
/// the 16 pixel one.
pub(crate) fn extra_glyphs(ascii: &[BmGlyph], scale: usize) -> Vec<BmGlyph> {
    let find = |c: char| ascii.iter().find(|g| g.ch == c);
    let mut out = marked_glyphs(ascii, scale);
    if let Some(b) = find('b') {
        out.push(eszett(b));
    }
    for (ligature, a, b) in [
        ('æ', 'a', 'e'),
        ('œ', 'o', 'e'),
        ('Æ', 'A', 'E'),
        ('Œ', 'O', 'E'),
    ] {
        if let (Some(a), Some(b)) = (find(a), find(b)) {
            out.push(join(a, b, -(scale as i32), ligature));
        }
    }
    if let Some(l) = find('l') {
        out.push(with_bar(
            l,
            'ł',
            l.extent().map_or(0, |(t, b)| (b - t) / 2),
            (0, 3 * scale),
        ));
    }
    if let Some(l) = find('L') {
        out.push(with_bar(
            l,
            'Ł',
            l.extent().map_or(0, |(t, b)| (b - t) / 2),
            (0, 3 * scale),
        ));
    }
    for (ch, base) in [('ø', 'o'), ('Ø', 'O')] {
        if let Some(glyph) = find(base) {
            out.push(with_slash(glyph, ch));
        }
    }
    if let Some(d) = find('d') {
        let width = d.width;
        out.push(with_bar(
            d,
            'đ',
            scale,
            (width.saturating_sub(3 * scale), width),
        ));
    }
    if let Some(d) = find('D') {
        out.push(with_bar(
            d,
            'Đ',
            d.extent().map_or(0, |(t, b)| (b - t) / 2),
            (0, 3 * scale),
        ));
        out.push(with_bar(
            d,
            'Ð',
            d.extent().map_or(0, |(t, b)| (b - t) / 2),
            (0, 3 * scale),
        ));
    }
    for (ch, base) in [('¿', '?'), ('¡', '!')] {
        if let Some(glyph) = find(base) {
            out.push(flipped(glyph, ch));
        }
    }
    if let Some(o) = find('o') {
        out.push(guillemet(o, '«'));
        out.push(guillemet(o, '»'));
    }
    for (ch, base) in [('‘', '\''), ('’', '\''), ('“', '"'), ('”', '"'), ('‚', ',')] {
        if let Some(glyph) = find(base) {
            out.push(glyph.renamed(ch));
        }
    }
    if let Some(comma) = find(',') {
        out.push(join(comma, comma, 1, '„'));
    }
    if let Some(dash) = find('-') {
        out.push(stretched(dash, '–', 2 * scale));
        out.push(stretched(dash, '—', 4 * scale));
    }
    if let Some(dot) = find('.') {
        let two = join(dot, dot, 1, '…');
        out.push(join(&two, dot, 1, '…'));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A glyph from rows of colour digits, `.` for clear.
    fn glyph(ch: char, rows: &[&str]) -> BmGlyph {
        let width = rows[0].len();
        BmGlyph {
            ch,
            width,
            height: rows.len(),
            y_offset: 0,
            advance: width + 1,
            px: rows
                .iter()
                .flat_map(|r| r.chars().map(|c| c.to_digit(10).map_or(0, |d| d as u8)))
                .collect(),
        }
    }

    fn rows(g: &BmGlyph) -> Vec<String> {
        g.px.chunks(g.width)
            .map(|r| r.iter().map(|&c| if c == 0 { '.' } else { '#' }).collect())
            .collect()
    }

    #[test]
    fn dots_go_over_a_letter_with_room_and_the_glyph_grows_when_there_is_none() {
        let a = glyph('a', &[".....", ".....", ".111.", "1...1"]);
        let umlaut = with_mark(&a, 'ä', Mark::Diaeresis, 1);
        assert_eq!((umlaut.height, umlaut.y_offset), (4, 0));
        assert_eq!(rows(&umlaut), [".#.#.", ".....", ".###.", "#...#"]);
        let cap = glyph('A', &[".1.", "1.1", "111"]);
        let umlaut = with_mark(&cap, 'Ä', Mark::Diaeresis, 1);
        assert_eq!((umlaut.height, umlaut.y_offset), (5, -2));
        assert_eq!(rows(&umlaut), ["#.#", "...", ".#.", "#.#", "###"]);
    }

    #[test]
    fn a_mark_wider_than_the_letter_widens_it_and_centres_the_letter() {
        let i = glyph('ı', &[".", ".", "1", "1"]);
        let circumflex = with_mark(&i, 'î', Mark::Circumflex, 1);
        assert_eq!(circumflex.width, 3);
        assert_eq!(circumflex.advance, i.advance + 2);
        assert_eq!(rows(&circumflex), [".#.", "#.#", "...", ".#.", ".#."]);
    }

    #[test]
    fn marks_of_the_big_font_are_two_pixels_per_pixel() {
        let o = glyph(
            'o',
            &[
                "........", "........", "........", "........", ".111111.", ".111111.",
            ],
        );
        let umlaut = with_mark(&o, 'ö', Mark::Diaeresis, 2);
        let r = rows(&umlaut);
        assert_eq!(r[1], ".##..##.");
        assert_eq!(r[2], r[1]);
        assert!(r[3].chars().all(|c| c == '.'));
    }

    #[test]
    fn a_mark_below_extends_the_glyph_downward() {
        let c = glyph('c', &[".11", "1..", ".11"]);
        let cedilla = with_mark(&c, 'ç', Mark::Cedilla, 1);
        assert_eq!(cedilla.height, 5);
        assert_eq!(rows(&cedilla)[3].matches('#').count(), 1);
        assert_eq!(rows(&cedilla)[4].matches('#').count(), 1);
    }

    #[test]
    fn the_eszett_is_a_stem_with_two_bowls_the_lower_larger_and_a_gap_at_the_bottom() {
        let b = glyph(
            'b',
            &[
                "1...", "1...", "1...", "1...", "1...", "1...", "1...", "1...",
            ],
        );
        let eszett = eszett(&b);
        assert_eq!(eszett.ch, 'ß');
        let r = rows(&eszett);
        assert!(
            r.iter().all(|row| row.starts_with('#')),
            "a straight stem on the left: {r:?}"
        );
        assert_eq!(
            r[0], "##..",
            "the upper bowl is narrower than the lower one"
        );
        let widest = |from: usize, to: usize| {
            (from..to)
                .map(|i| r[i].rfind('#').unwrap_or(0))
                .max()
                .unwrap_or(0)
        };
        assert!(
            widest(4, 8) > widest(0, 3),
            "the lower bowl reaches further right: {r:?}"
        );
        let last = r.last().unwrap();
        assert_eq!(
            &last[..2],
            "#.",
            "the bottom bar is not joined to the stem: {last}"
        );
        assert!(last[2..].contains('#'));
    }

    #[test]
    fn the_dotless_i_loses_its_dot_rows() {
        let i = glyph('i', &["1", ".", "1", "1"]);
        let dotless = dotless_i(&i);
        assert_eq!(dotless.ch, 'ı');
        assert_eq!(dotless.px, vec![0, 0, 1, 1]);
    }

    #[test]
    fn a_ligature_overlaps_its_letters_by_a_column() {
        let a = glyph('a', &["11.", "1.1"]);
        let e = glyph('e', &["111", "1.."]);
        let ae = join(&a, &e, -1, 'æ');
        assert_eq!(ae.width, 5);
        assert_eq!(rows(&ae), ["#####", "#.#.."]);
    }

    #[test]
    fn an_upside_down_mark_is_the_mark_turned_within_its_rows() {
        let q = glyph('?', &["11.", "..1", ".1.", "..."]);
        assert_eq!(rows(&flipped(&q, '¿')), [".#.", "..#", "##.", "..."]);
    }

    #[test]
    fn guillemets_point_left_and_right() {
        let o = glyph('o', &[".11.", "1..1", ".11."]);
        let left = guillemet(&o, '«');
        let right = guillemet(&o, '»');
        assert_eq!(rows(&left)[1].find('#'), Some(0));
        assert_eq!(rows(&right)[1].rfind('#'), Some(left.width - 1));
    }

    #[test]
    fn the_table_gives_every_listed_letter_a_base_letter() {
        for &(accented, bases, _) in MARKED {
            assert_eq!(
                accented.chars().count(),
                bases.chars().count(),
                "{accented}"
            );
        }
    }

    #[test]
    fn a_font_gets_the_french_czech_danish_and_german_letters() {
        let ascii: Vec<BmGlyph> = (32u8..127)
            .map(|c| {
                let ch = char::from(c);
                let mut g = glyph(ch, &["...", ".1.", "1.1", "111", "1.1"]);
                g.ch = ch;
                g
            })
            .collect();
        let extra = extra_glyphs(&ascii, 1);
        let letters: String = extra.iter().map(|g| g.ch).collect();
        for ch in "ÄÖÜäöüßàâçéèêëîïôûùÿœæÀÉČĎĚŇŘŠŤŽčďěňřšťžůýåøÅØÆąęłńóśźżŁőű¿¡«»„“”–—…ı".chars()
        {
            assert!(letters.contains(ch), "missing {ch}");
        }
        let mut seen = std::collections::HashSet::new();
        for g in &extra {
            assert!(seen.insert(g.ch), "{} twice", g.ch);
            assert_eq!(g.px.len(), g.width * g.height, "{}", g.ch);
        }
    }
}
