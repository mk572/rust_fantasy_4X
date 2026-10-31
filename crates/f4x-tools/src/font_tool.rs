//! Font tools: which characters a font has, which characters the game's text uses, and a
//! TrueType or OpenType font rasterised at one size into an AngelCode text BMFont (`.fnt` and
//! `.png`, the format `macroquad_bitmap_font_engine` draws). See `docs/fonts.md`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fontdue::{Font, FontSettings, Metrics};

use crate::bdf::{self, BdfFont};

const COLUMNS: usize = 16;
/// Up to this coverage (0 to 255) a pixel counts as covered when the font is not smoothed.
const THRESHOLD: u8 = 128;

pub const USAGE: &str = "  f4x-tools font-chars [text-dir]             the characters the game's text uses, on one line
                                              (default: content/text)
  f4x-tools font-coverage <font> [--chars FILE]
                                              which of the wanted characters the font lacks (default:
                                              Basic Latin, Latin-1 and Latin Extended-A; FILE adds its characters)
  f4x-tools font-bmfont <font> [<px>] [--out DIR] [--name NAME] [--chars FILE] [--smooth]
                                              rasterise the font at <px> (a .bdf bitmap font has its own size)
                                              into DIR/NAME.fnt and .png
                                              (default out: assets-local/fonts; --smooth keeps the
                                              anti-aliasing, otherwise pixels are on or off)";

pub fn run(command: &str, args: &[String]) -> Result<(), String> {
    match command {
        "font-chars" => {
            let dir = args.first().map_or("content/text", String::as_str);
            let chars = used_characters(Path::new(dir))?;
            println!("{}", chars.iter().collect::<String>());
            Ok(())
        }
        "font-coverage" => coverage(args),
        "font-bmfont" => bmfont_command(args),
        _ => Err(USAGE.to_string()),
    }
}

/// Basic Latin, Latin-1 Supplement and Latin Extended-A, without controls.
fn default_charset() -> BTreeSet<char> {
    (32u32..=126)
        .chain(160..=383)
        .filter_map(char::from_u32)
        .collect()
}

/// The characters of a text, without control characters.
fn characters_of(text: &str) -> BTreeSet<char> {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// Every character in the `.ron` files under `dir` (any language folder).
fn used_characters(dir: &Path) -> Result<BTreeSet<char>, String> {
    let mut chars = BTreeSet::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let entries =
            std::fs::read_dir(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "ron") {
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                chars.extend(characters_of(&text));
            }
        }
    }
    Ok(chars)
}

/// A font to read: an outline font (TrueType, OpenType) drawn at a size, or a BDF bitmap font
/// that has only its own size.
enum Source {
    Outline(Font),
    Bitmap(BdfFont),
}

impl Source {
    fn load(path: &str) -> Result<Self, String> {
        if path.to_lowercase().ends_with(".bdf") {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            return bdf::parse(&text)
                .map(Self::Bitmap)
                .map_err(|e| format!("{path}: {e}"));
        }
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        Font::from_bytes(bytes, FontSettings::default())
            .map(Self::Outline)
            .map_err(|e| format!("{path}: {e}"))
    }

    fn has(&self, c: char) -> bool {
        match self {
            Self::Outline(font) => font.lookup_glyph_index(c) != 0,
            Self::Bitmap(font) => font.glyphs.contains_key(&c),
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            Self::Outline(font) => font.name(),
            Self::Bitmap(font) => Some(font.name.as_str()).filter(|n| !n.is_empty()),
        }
    }

    /// The size a bitmap font was drawn for: its ascent plus descent.
    fn own_size(&self) -> Option<f32> {
        match self {
            Self::Outline(_) => None,
            Self::Bitmap(font) => Some((font.ascent + font.descent) as f32),
        }
    }

    /// `(line height, base)` in whole pixels at `px`.
    fn line(&self, px: f32) -> Option<(i32, i32)> {
        match self {
            Self::Outline(font) => font
                .horizontal_line_metrics(px)
                .map(|l| (l.new_line_size.round() as i32, l.ascent.round() as i32)),
            Self::Bitmap(font) => Some((font.ascent + font.descent, font.ascent)),
        }
    }
}

/// The value after `--flag`, if given.
fn option<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == flag)?;
    args.get(at + 1).map(String::as_str)
}

/// The default characters plus those of the `--chars` file.
fn wanted_characters(args: &[String]) -> Result<BTreeSet<char>, String> {
    let mut chars = default_charset();
    if let Some(file) = option(args, "--chars") {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
        chars.extend(characters_of(&text));
    }
    Ok(chars)
}

/// The characters the font has no glyph for.
fn missing(font: &Source, chars: &BTreeSet<char>) -> Vec<char> {
    chars.iter().copied().filter(|&c| !font.has(c)).collect()
}

fn coverage(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or(USAGE)?;
    let font = Source::load(path)?;
    let wanted = wanted_characters(args)?;
    let lacking = missing(&font, &wanted);
    println!(
        "{path}: {} of {} wanted characters, {} missing",
        wanted.len() - lacking.len(),
        wanted.len(),
        lacking.len()
    );
    if !lacking.is_empty() {
        println!("missing: {}", lacking.iter().collect::<String>());
    }
    Ok(())
}

/// One glyph of the font at the chosen size.
struct Raster {
    ch: char,
    metrics: Metrics,
    /// Coverage 0 to 255, `metrics.width` by `metrics.height`.
    alpha: Vec<u8>,
}

/// Every wanted character the font has, drawn at `px` (a bitmap font ignores it); unsmoothed
/// outline glyphs are on or off.
fn rasterise(font: &Source, chars: &BTreeSet<char>, px: f32, smooth: bool) -> Vec<Raster> {
    chars
        .iter()
        .filter(|&&c| font.has(c))
        .map(|&ch| match font {
            Source::Outline(outline) => {
                let (metrics, mut alpha) = outline.rasterize(ch, px);
                if !smooth {
                    threshold(&mut alpha);
                }
                Raster { ch, metrics, alpha }
            }
            Source::Bitmap(bitmap) => {
                let g = &bitmap.glyphs[&ch];
                let metrics = Metrics {
                    xmin: g.x_offset,
                    ymin: g.y_offset,
                    width: g.width,
                    height: g.height,
                    advance_width: g.advance as f32,
                    ..Default::default()
                };
                Raster {
                    ch,
                    metrics,
                    alpha: g.alpha.clone(),
                }
            }
        })
        .collect()
}

fn threshold(alpha: &mut [u8]) {
    for a in alpha {
        *a = if *a >= THRESHOLD { 255 } else { 0 };
    }
}

/// `yoffset` of a glyph: the distance from the top of the line to the top of its bitmap, given
/// the baseline `base` pixels below the top (`ymin` is the bitmap's bottom above the baseline).
fn y_offset(metrics: &Metrics, base: i32) -> i32 {
    base - (metrics.ymin + metrics.height as i32)
}

/// The atlas `(width, height, RGBA)` and the `.fnt` text for the glyphs.
fn bmfont(
    face: &str,
    page: &str,
    px: f32,
    (line_height, base): (i32, i32),
    rasters: &[Raster],
    kernings: &[(char, char, i32)],
) -> ((usize, usize, Vec<u8>), String) {
    let cell_w = rasters.iter().map(|r| r.metrics.width).max().unwrap_or(0) + 1;
    let cell_h = rasters.iter().map(|r| r.metrics.height).max().unwrap_or(0) + 1;
    let rows = rasters.len().div_ceil(COLUMNS).max(1);
    let (aw, ah) = (cell_w * COLUMNS, cell_h * rows);
    let mut rgba = vec![0u8; aw * ah * 4];
    let mut fnt = format!(
        "info face=\"{face}\" size={px} padding=0,0,0,0 spacing=1,1\n\
         common lineHeight={line_height} base={base} scaleW={aw} scaleH={ah} pages=1 packed=0\n\
         page id=0 file=\"{page}\"\nchars count={}\n",
        rasters.len()
    );
    for (n, r) in rasters.iter().enumerate() {
        let (cx, cy) = (n % COLUMNS * cell_w, n / COLUMNS * cell_h);
        let m = &r.metrics;
        for row in 0..m.height {
            for col in 0..m.width {
                let a = r.alpha[row * m.width + col];
                let at = ((cy + row) * aw + cx + col) * 4;
                rgba[at..at + 4].copy_from_slice(&[255, 255, 255, a]);
            }
        }
        fnt.push_str(&format!(
            "char id={} x={cx} y={cy} width={} height={} xoffset={} yoffset={} xadvance={} page=0 chnl=15\n",
            r.ch as u32,
            m.width,
            m.height,
            m.xmin,
            y_offset(m, base),
            m.advance_width.round() as i32
        ));
    }
    if !kernings.is_empty() {
        fnt.push_str(&format!("kernings count={}\n", kernings.len()));
        for (a, b, amount) in kernings {
            fnt.push_str(&format!(
                "kerning first={} second={} amount={amount}\n",
                *a as u32, *b as u32
            ));
        }
    }
    ((aw, ah, rgba), fnt)
}

/// The kerning pairs, in whole pixels, between the rasterised glyphs (those that round to 0 are left out).
fn kerning_pairs(font: &Source, rasters: &[Raster], px: f32) -> Vec<(char, char, i32)> {
    // A bitmap font has no kerning table.
    let Source::Outline(font) = font else {
        return Vec::new();
    };
    let mut pairs = Vec::new();
    for a in rasters {
        for b in rasters {
            let amount = font
                .horizontal_kern(a.ch, b.ch, px)
                .map_or(0, |k| k.round() as i32);
            if amount != 0 {
                pairs.push((a.ch, b.ch, amount));
            }
        }
    }
    pairs
}

/// A file name for a font and size: `Jersey10-Regular.ttf` at 10 px is `jersey10_regular_10`.
fn default_name(path: &str, px: f32) -> String {
    let stem = Path::new(path)
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().to_lowercase());
    let stem: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{}_{px}", stem.trim_matches('_'))
}

fn bmfont_command(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or(USAGE)?;
    let font = Source::load(path)?;
    // An outline font needs a size; a bitmap font has its own, and a number only checks it.
    let given: Option<f32> = args.get(1).and_then(|s| s.parse().ok());
    let px = match (given, font.own_size()) {
        (Some(px), None) if px >= 1.0 => px,
        (None, Some(own)) => own,
        (Some(px), Some(own)) if px == own => own,
        (Some(_), Some(own)) => {
            return Err(format!(
                "{path} is a bitmap font of size {own}; leave the size out"
            ))
        }
        _ => return Err("font-bmfont needs a size in pixels, 1 or more".to_string()),
    };
    let heights = font
        .line(px)
        .ok_or_else(|| format!("{path} has no horizontal line metrics"))?;
    let name = option(args, "--name").map_or_else(|| default_name(path, px), str::to_string);
    let out = PathBuf::from(option(args, "--out").unwrap_or("assets-local/fonts"));
    let wanted = wanted_characters(args)?;
    let rasters = rasterise(&font, &wanted, px, args.iter().any(|a| a == "--smooth"));
    if rasters.is_empty() {
        return Err(format!("{path} has none of the wanted characters"));
    }
    let kernings = kerning_pairs(&font, &rasters, px);
    let face = font.name().unwrap_or(&name).to_string();
    let ((w, h, rgba), fnt) = bmfont(
        &face,
        &format!("{name}.png"),
        px,
        heights,
        &rasters,
        &kernings,
    );
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    crate::art::write_png(&out.join(format!("{name}.png")), w, h, &rgba)?;
    let fnt_path = out.join(format!("{name}.fnt"));
    std::fs::write(&fnt_path, fnt).map_err(|e| format!("{}: {e}", fnt_path.display()))?;
    let lacking = wanted.len() - rasters.len();
    println!(
        "wrote {} ({} glyphs, {} kerning pairs, line height {}, base {}){}",
        fnt_path.display(),
        rasters.len(),
        kernings.len(),
        heights.0,
        heights.1,
        if lacking > 0 {
            format!("; {lacking} wanted characters are missing from the font")
        } else {
            String::new()
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(xmin: i32, ymin: i32, width: usize, height: usize, advance: f32) -> Metrics {
        Metrics {
            xmin,
            ymin,
            width,
            height,
            advance_width: advance,
            ..Default::default()
        }
    }

    #[test]
    fn unsmoothed_glyphs_are_on_or_off() {
        let mut alpha = [0u8, 127, 128, 255];
        threshold(&mut alpha);
        assert_eq!(alpha, [0, 0, 255, 255]);
    }

    #[test]
    fn a_glyph_sits_above_the_baseline_by_its_ymin() {
        // Base 8 below the line top: a 5 high glyph resting on the baseline starts 3 down; one
        // whose bottom hangs 2 below the baseline (a descender) starts 5 down.
        assert_eq!(y_offset(&metrics(0, 0, 4, 5, 5.0), 8), 3);
        assert_eq!(y_offset(&metrics(0, -2, 4, 5, 5.0), 8), 5);
    }

    #[test]
    fn the_fnt_lists_each_glyph_on_a_grid_with_its_offsets_and_kerning() {
        let rasters = [
            Raster {
                ch: 'A',
                metrics: metrics(1, 0, 2, 2, 4.0),
                alpha: vec![255, 0, 0, 255],
            },
            Raster {
                ch: 'V',
                metrics: metrics(0, 0, 3, 2, 4.4),
                alpha: vec![255; 6],
            },
        ];
        let ((aw, ah, rgba), fnt) =
            bmfont("Test", "t.png", 8.0, (10, 8), &rasters, &[('A', 'V', -1)]);
        assert_eq!((aw, ah), (4 * COLUMNS, 3));
        assert!(fnt.contains("common lineHeight=10 base=8 scaleW=64 scaleH=3"));
        assert!(fnt.contains("char id=65 x=0 y=0 width=2 height=2 xoffset=1 yoffset=6 xadvance=4"));
        assert!(fnt.contains("char id=86 x=4 y=0 width=3 height=2 xoffset=0 yoffset=6 xadvance=4"));
        assert!(fnt.contains("kerning first=65 second=86 amount=-1"));
        // 'A' draws its top left and bottom right pixels only, in white.
        let pixel = |x: usize, y: usize| rgba[(y * aw + x) * 4..(y * aw + x) * 4 + 4].to_vec();
        assert_eq!(pixel(0, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(1, 0), [255, 255, 255, 0]);
    }

    #[test]
    fn the_default_characters_cover_the_european_accents() {
        let chars = default_charset();
        for c in "Aaé ÄÖÜß ñçø Łž".chars() {
            assert!(chars.contains(&c), "{c}");
        }
        assert!(!chars.contains(&'\n'));
    }

    #[test]
    fn names_come_from_the_file_and_size() {
        assert_eq!(
            default_name("a/Jersey10-Regular.ttf", 10.0),
            "jersey10_regular_10"
        );
    }

    #[test]
    fn a_shipped_font_is_rasterised_when_the_reference_fonts_are_there() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../reference/fonts/jersey10/Jersey10-Regular.ttf");
        let Some(path) = path.to_str().filter(|p| Path::new(p).exists()) else {
            return; // /reference is local only
        };
        let font = Source::load(path).unwrap();
        let wanted: BTreeSet<char> = "Aé?".chars().collect();
        assert!(missing(&font, &wanted).is_empty());
        let rasters = rasterise(&font, &wanted, 10.0, false);
        assert_eq!(rasters.len(), 3);
        assert!(rasters
            .iter()
            .all(|r| r.alpha.iter().all(|&a| a == 0 || a == 255)));
        assert!(rasters[0].alpha.contains(&255));
        let missing = missing(&font, &"\u{4e2d}".chars().collect());
        assert_eq!(missing, vec!['\u{4e2d}']);
    }

    #[test]
    fn a_bitmap_font_keeps_its_pixels_and_gives_its_own_line() {
        let text = "STARTFONT 2.1\nFONT_ASCENT 6\nFONT_DESCENT 2\nSTARTCHAR A\nENCODING 65\nDWIDTH 4 0\nBBX 3 2 1 -1\nBITMAP\nA0\n40\nENDCHAR\nENDFONT\n";
        let font = Source::Bitmap(bdf::parse(text).unwrap());
        assert_eq!(font.own_size(), Some(8.0));
        assert_eq!(font.line(99.0), Some((8, 6)));
        let wanted: BTreeSet<char> = "AB".chars().collect();
        assert_eq!(missing(&font, &wanted), vec!['B']);
        let rasters = rasterise(&font, &wanted, 99.0, false);
        assert_eq!(rasters.len(), 1);
        let m = &rasters[0].metrics;
        assert_eq!(
            (m.xmin, m.ymin, m.width, m.height, m.advance_width),
            (1, -1, 3, 2, 4.0)
        );
        // Base 6: a 2 high glyph whose bottom is 1 below the baseline starts 5 down.
        assert_eq!(y_offset(m, 6), 5);
        assert_eq!(rasters[0].alpha, vec![255, 0, 255, 0, 255, 0]);
    }
}
