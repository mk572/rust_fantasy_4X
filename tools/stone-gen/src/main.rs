use macroquad::{prelude::*, ui::Ui};
use std::{fs::File, io::BufWriter};

const PREVIEW_X: f32 = 360.0;
const PREVIEW_Y: f32 = 24.0;
const PREVIEW_W: f32 = 800.0;
const PREVIEW_H: f32 = 680.0;

#[derive(Clone, Copy, PartialEq)]
enum StoneType { Marble, Granite, Sandstone, Slate }

impl StoneType {
    fn label(self) -> &'static str {
        match self {
            Self::Marble => "Marble",
            Self::Granite => "Granite",
            Self::Sandstone => "Sandstone",
            Self::Slate => "Slate",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Marble => Self::Granite,
            Self::Granite => Self::Sandstone,
            Self::Sandstone => Self::Slate,
            Self::Slate => Self::Marble,
        }
    }
}

#[derive(Clone)]
struct Settings {
    seed: u64,
    stone: StoneType,
    main: [u8; 3],
    light: [u8; 3],
    dark: [u8; 3],
    granularity: f32,
    angle: f32,
    width: u32,
    height: u32,
    palette_size: usize,
    tiled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            seed: 1994,
            stone: StoneType::Marble,
            main: [49, 119, 112],
            light: [112, 166, 151],
            dark: [22, 60, 60],
            granularity: 0.48,
            angle: 35.0,
            width: 256,
            height: 192,
            palette_size: 12,
            tiled: true,
        }
    }
}

fn hash2(x: i32, y: i32, seed: u64) -> f32 {
    let mut n = (x as u64).wrapping_mul(0x9E3779B185EBCA87)
        ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F)
        ^ seed;
    n ^= n >> 30;
    n = n.wrapping_mul(0xBF58476D1CE4E5B9);
    n ^= n >> 27;
    n = n.wrapping_mul(0x94D049BB133111EB);
    n ^= n >> 31;
    (n as u32) as f32 / u32::MAX as f32
}

fn smooth(t: f32) -> f32 { t * t * (3.0 - 2.0 * t) }

// Periodic value noise. Integer cell counts make the field repeat exactly at x/y = 0/1.
fn periodic_noise(x: f32, y: f32, cells_x: i32, cells_y: i32, seed: u64) -> f32 {
    let gx = x.rem_euclid(1.0) * cells_x as f32;
    let gy = y.rem_euclid(1.0) * cells_y as f32;
    let ix = gx.floor() as i32;
    let iy = gy.floor() as i32;
    let tx = smooth(gx.fract());
    let ty = smooth(gy.fract());
    let wrap_x = |v: i32| v.rem_euclid(cells_x);
    let wrap_y = |v: i32| v.rem_euclid(cells_y);
    let a = hash2(wrap_x(ix), wrap_y(iy), seed);
    let b = hash2(wrap_x(ix + 1), wrap_y(iy), seed);
    let c = hash2(wrap_x(ix), wrap_y(iy + 1), seed);
    let d = hash2(wrap_x(ix + 1), wrap_y(iy + 1), seed);
    let top = a + (b - a) * tx;
    let bottom = c + (d - c) * tx;
    top + (bottom - top) * ty
}

fn fbm(x: f32, y: f32, seed: u64, base: i32, octaves: usize) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut norm = 0.0;
    for octave in 0..octaves {
        let f = base << octave;
        sum += periodic_noise(x, y, f, f, seed.wrapping_add(octave as u64 * 7919)) * amp;
        norm += amp;
        amp *= 0.5;
    }
    sum / norm.max(0.0001)
}

fn palette(settings: &Settings) -> Vec<[u8; 3]> {
    let count = settings.palette_size.clamp(2, 16);
    (0..count).map(|i| {
        let t = i as f32 / (count - 1) as f32;
        let (a, b, local_t) = if t < 0.5 {
            (settings.dark, settings.main, t * 2.0)
        } else {
            (settings.main, settings.light, (t - 0.5) * 2.0)
        };
        let ease = local_t * local_t * (3.0 - 2.0 * local_t);
        [
            (a[0] as f32 + (b[0] as f32 - a[0] as f32) * ease).round() as u8,
            (a[1] as f32 + (b[1] as f32 - a[1] as f32) * ease).round() as u8,
            (a[2] as f32 + (b[2] as f32 - a[2] as f32) * ease).round() as u8,
        ]
    }).collect()
}

fn nearest_palette(value: f32, colors: &[[u8; 3]]) -> [u8; 3] {
    colors[((value.clamp(0.0, 0.9999) * colors.len() as f32).floor() as usize).min(colors.len() - 1)]
}

fn generate(settings: &Settings) -> Vec<u8> {
    let w = settings.width.clamp(16, 1024) as usize;
    let h = settings.height.clamp(16, 1024) as usize;
    let colors = palette(settings);
    let angle = settings.angle.to_radians();
    // Integer direction vector: its phase repeats at both horizontal and vertical edges.
    let mut kx = (angle.cos() * 8.0).round() as i32;
    let mut ky = (angle.sin() * 8.0).round() as i32;
    if kx == 0 && ky == 0 { kx = 1; }
    let grain = settings.granularity.clamp(0.05, 1.0);
    let mut rgba = Vec::with_capacity(w * h * 4);

    for py in 0..h {
        for px in 0..w {
            let x = px as f32 / w as f32;
            let y = py as f32 / h as f32;
            let warp_a = fbm(x, y, settings.seed ^ 0xA53A, 3, 3) - 0.5;
            let warp_b = fbm(x, y, settings.seed ^ 0xB71D, 3, 3) - 0.5;
            let warped_phase = (kx as f32 * x + ky as f32 * y)
                + warp_a * (0.08 + grain * 0.18)
                + warp_b * 0.035;
            let coarse = fbm(x, y, settings.seed, 3, 4);
            let fine = fbm(x, y, settings.seed ^ 0xDEADBEEF, 24, 2);
            let fleck = periodic_noise(x, y, 48, 48, settings.seed ^ 0xF1EC);

            let value = match settings.stone {
                StoneType::Marble => {
                    let bands = (warped_phase * (7.0 + grain * 18.0) * std::f32::consts::TAU).sin();
                    let veins = (bands * 0.5 + 0.5).powf(1.8);
                    (0.18 + veins * 0.54 + coarse * 0.22 + (fine - 0.5) * 0.12)
                }
                StoneType::Granite => {
                    let mineral = (coarse * 0.55 + fine * 0.25 + fleck * 0.20);
                    let specks = if fleck > 0.83 { 0.16 } else if fleck < 0.12 { -0.12 } else { 0.0 };
                    mineral + specks
                }
                StoneType::Sandstone => {
                    let strata = (warped_phase * (5.0 + grain * 10.0) * std::f32::consts::TAU).sin();
                    0.22 + coarse * 0.36 + (strata * 0.5 + 0.5) * 0.32 + (fine - 0.5) * 0.12
                }
                StoneType::Slate => {
                    let cleavage = (warped_phase * (13.0 + grain * 20.0) * std::f32::consts::TAU).sin();
                    0.20 + coarse * 0.42 + (cleavage * 0.5 + 0.5) * 0.24 + (fine - 0.5) * 0.14
                }
            };
            let rgb = nearest_palette(value, &colors);
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    rgba
}

fn save_png(path: &str, width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(pixels).map_err(|e| e.to_string())
}

fn color_controls(ui: &mut Ui, title: &str, color: &mut [u8; 3], id: u64) {
    ui.label(None, title);
    for (i, channel) in ["R", "G", "B"].iter().enumerate() {
        let mut value = color[i] as f32;
        ui.slider(id + i as u64, channel, 0.0..255.0, &mut value);
        color[i] = value.round().clamp(0.0, 255.0) as u8;
    }
}

#[macroquad::main("Stone Gen - VGA Texture Lab")]
async fn main() {
    let mut settings = Settings::default();
    let mut seed_text = settings.seed.to_string();
    let mut zoom = 2.0_f32;
    let mut texture: Option<Texture2D> = None;
    let mut pixels = Vec::new();
    let mut status = "Press Generate to create a texture".to_string();
    let mut regenerate = true;

    loop {
        if regenerate {
            pixels = generate(&settings);
            let image = Image {
                bytes: pixels.clone(),
                width: settings.width as u16,
                height: settings.height as u16,
            };
            let generated_texture = Texture2D::from_image(&image);
            generated_texture.set_filter(FilterMode::Nearest);
            texture = Some(generated_texture);
            status = format!("Generated {} x {} · {} colours", settings.width, settings.height, settings.palette_size);
            regenerate = false;
        }

        clear_background(Color::from_rgba(24, 27, 31, 255));

        // Preview canvas
        draw_rectangle(PREVIEW_X, PREVIEW_Y, PREVIEW_W, PREVIEW_H, Color::from_rgba(12, 14, 17, 255));
        if let Some(tex) = &texture {
            let tw = settings.width as f32 * zoom;
            let th = settings.height as f32 * zoom;
            if settings.tiled {
                let start_x = PREVIEW_X + 8.0;
                let start_y = PREVIEW_Y + 8.0;
                let cols = ((PREVIEW_W - 16.0) / tw).ceil().max(1.0) as i32;
                let rows = ((PREVIEW_H - 16.0) / th).ceil().max(1.0) as i32;
                for ty in 0..rows {
                    for tx in 0..cols {
                        draw_texture_ex(tex, start_x + tx as f32 * tw, start_y + ty as f32 * th, WHITE,
                            DrawTextureParams { dest_size: Some(vec2(tw, th)), ..Default::default() });
                    }
                }
            } else {
                let scale = (PREVIEW_W - 24.0).min(PREVIEW_H - 24.0) / settings.width.max(settings.height) as f32;
                let draw_w = settings.width as f32 * scale.min(zoom.max(0.1));
                let draw_h = settings.height as f32 * scale.min(zoom.max(0.1));
                draw_texture_ex(tex, PREVIEW_X + (PREVIEW_W - draw_w) * 0.5, PREVIEW_Y + (PREVIEW_H - draw_h) * 0.5, WHITE,
                    DrawTextureParams { dest_size: Some(vec2(draw_w, draw_h)), ..Default::default() });
            }
        }

        let ui = root_ui();
        ui.window(hash!(), vec2(12.0, 12.0), vec2(330.0, screen_height() - 24.0), |ui| {
            ui.label(None, "STONE GEN / VGA TEXTURE LAB");
            ui.separator();

            ui.label(None, &format!("Material: {}", settings.stone.label()));
            if ui.button(None, "Change stone type") { settings.stone = settings.stone.next(); regenerate = true; }

            ui.label(None, "Seed (integer)");
            ui.editbox(hash!(), vec2(180.0, 24.0), &mut seed_text);
            if let Ok(seed) = seed_text.trim().parse::<u64>() {
                if seed != settings.seed { settings.seed = seed; }
            }

            ui.label(None, &format!("Output width: {}", settings.width));
            let mut width = settings.width as f32;
            ui.slider(hash!(), "Width", 32.0..512.0, &mut width);
            settings.width = (width.round() as u32 / 8 * 8).clamp(32, 512);
            ui.label(None, &format!("Output height: {}", settings.height));
            let mut height = settings.height as f32;
            ui.slider(hash!(), "Height", 32.0..512.0, &mut height);
            settings.height = (height.round() as u32 / 8 * 8).clamp(32, 512);

            ui.label(None, &format!("Granularity: {:.2}", settings.granularity));
            ui.slider(hash!(), "Granularity", 0.05..1.0, &mut settings.granularity);
            ui.label(None, &format!("Angle: {:.0} degrees", settings.angle));
            ui.slider(hash!(), "Angle", 0.0..180.0, &mut settings.angle);

            let mut palette_count = settings.palette_size as f32;
            ui.label(None, &format!("Palette: {} / 16", settings.palette_size));
            ui.slider(hash!(), "Colours", 2.0..16.0, &mut palette_count);
            settings.palette_size = palette_count.round() as usize;

            color_controls(ui, "Main colour RGB", &mut settings.main, hash!(100));
            color_controls(ui, "Lightest colour RGB", &mut settings.light, hash!(200));
            color_controls(ui, "Darkest colour RGB", &mut settings.dark, hash!(300));

            ui.checkbox(hash!(), "Tile preview", &mut settings.tiled);
            ui.label(None, &format!("Preview zoom: {:.2}x", zoom));
            ui.slider(hash!(), "View size", 0.25..4.0, &mut zoom);

            if ui.button(None, "Generate") { regenerate = true; }
            if ui.button(None, "Save PNG: stone_texture.png") {
                if pixels.is_empty() {
                    status = "Generate a texture first".to_string();
                } else {
                    let export_pixels = generate(&settings);
                    match save_png("stone_texture.png", settings.width, settings.height, &export_pixels) {
                        Ok(()) => status = "Saved stone_texture.png".to_string(),
                        Err(error) => status = format!("PNG save failed: {error}"),
                    }
                }
            }
            ui.separator();
            ui.label(None, &status);
            ui.label(None, "Tiling is built into the periodic noise.");
            ui.label(None, "Export size is independent of preview zoom.");
        });

        // Settings changed by sliders should refresh on the next frame.
        // The preview zoom and tiling toggle do not require regeneration.
        if let Ok(seed) = seed_text.trim().parse::<u64>() {
            if seed != settings.seed { settings.seed = seed; regenerate = true; }
        }
        // Rebuild when parameter values differ from the last generated snapshot.
        // A compact signature avoids retaining duplicate settings structs.
        let signature = format!("{:?}-{}-{}-{:?}-{:?}-{:?}-{}-{}-{}-{}",
            settings.stone as u8, settings.seed, settings.palette_size, settings.main,
            settings.light, settings.dark, settings.granularity, settings.angle,
            settings.width, settings.height);
        if LAST_SIGNATURE.with(|last| {
            let changed = last.borrow().as_deref() != Some(signature.as_str());
            if changed { *last.borrow_mut() = Some(signature); }
            changed
        }) { regenerate = true; }

        next_frame().await;
    }
}

thread_local! {
    static LAST_SIGNATURE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
