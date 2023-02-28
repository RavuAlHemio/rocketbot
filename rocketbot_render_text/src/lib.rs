use std::collections::HashMap;

use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::shape::ShapeContext;
use swash::zeno::Vector;


const FONT: &[u8] = include_bytes!("../data/texgyreheros-regular.otf");
const FONT_SIZE_PX: f32 = 16.0;


/// Renders the given text using the built-in font and returns a map of coordinates to pixel
/// intensity values, where higher values are more intense.
///
/// Any pixels not contained in the map can be assumed to be blank (equivalent to intensity value
/// 0).
pub fn render_text(text: &str) -> HashMap<(u32, u32), u8> {
    // load font
    let font = FontRef::from_index(FONT, 0)
        .expect("failed to load font");
    let metrics = font.metrics(&[]);
    let ascender_px_f32 = metrics.ascent * FONT_SIZE_PX / f32::from(metrics.units_per_em);
    let ascender_px: i32 = ascender_px_f32.ceil() as i32;

    // shape text
    let mut shape_ctx = ShapeContext::new();
    let mut shaper = shape_ctx.builder(font)
        .size(FONT_SIZE_PX)
        .build();
    shaper.add_str(text);
    let mut glyphs = Vec::new();
    shaper.shape_with(|cluster| {
        for glyph in cluster.glyphs {
            glyphs.push(*glyph);
        }
    });

    // render text
    let mut context = ScaleContext::new();
    let mut scaler = context.builder(font)
        .size(FONT_SIZE_PX)
        .hint(false)
        .build();
    let mut renderer = Render::new(&[
        Source::ColorOutline(0),
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::Outline,
    ]);
    let mut pixel_values: HashMap<(u32, u32), u8> = HashMap::new();
    let mut pos_x: f32 = 0.0;
    for glyph in &glyphs {
        let pos_x_int: u32 = pos_x.trunc() as u32;
        let pos_x_frac = pos_x.fract();
        renderer.offset(Vector::new(pos_x_frac, 0.0));
        let img = renderer.render(&mut scaler, glyph.id)
            .expect("failed to render glyph");

        for y in 0..img.placement.height {
            for x in 0..img.placement.width {
                let i: usize = (y * img.placement.width + x).try_into().unwrap();
                let b = img.data[i];
                if b == 0 {
                    continue;
                }
                let actual_x: u32 = match (img.placement.left + i32::try_from(pos_x_int + x).unwrap()).try_into() {
                    Ok(ax) => ax,
                    Err(_) => continue,
                };
                let actual_y: u32 = match (ascender_px - img.placement.top + i32::try_from(y).unwrap()).try_into() {
                    Ok(ay) => ay,
                    Err(_) => continue,
                };
                let pixel_ref = pixel_values
                    .entry((actual_x, actual_y))
                    .or_insert(0);
                *pixel_ref = pixel_ref.saturating_add(b);
            }
        }

        pos_x += glyph.advance;
    }

    pixel_values
}

/// Obtains the minimum dimensions of the image described by the given pixel value map.
///
/// The return value is a tuple `(width, height)`.
pub fn map_to_dimensions(pixel_values: &HashMap<(u32, u32), u8>) -> (u32, u32) {
    let image_width = pixel_values.keys()
        .map(|(x, _y)| *x + 1)
        .max()
        .unwrap_or(0);
    let image_height = pixel_values.keys()
        .map(|(_x, y)| *y + 1)
        .max()
        .unwrap_or(0);
    (image_width, image_height)
}

/// Obtains the line height, in pixels, of the font being used.
pub fn font_line_height() -> u32 {
    // ascender + descender + additional leading
    let font = FontRef::from_index(FONT, 0)
        .expect("failed to load font");
    let metrics = font.metrics(&[]);
    let line_height_font_units = metrics.ascent + metrics.descent + metrics.leading;
    let line_height_px_f32 = line_height_font_units * FONT_SIZE_PX / f32::from(metrics.units_per_em);
    line_height_px_f32.ceil() as u32
}

/// Writes the given intensity map to a PNG file.
#[cfg(feature = "png")]
pub fn map_to_png<W: std::io::Write>(
    writer: W,
    map: &HashMap<(u32, u32), u8>,
    top_margin: u32,
    bottom_margin: u32,
    left_margin: u32,
    right_margin: u32,
) -> Result<(), png::EncodingError> {
    let (mut width, mut height) = map_to_dimensions(&map);
    width += left_margin + right_margin;
    height += top_margin + bottom_margin;
    let width_usize = usize::try_from(width).unwrap();
    let top_margin_usize = usize::try_from(top_margin).unwrap();
    let left_margin_usize = usize::try_from(left_margin).unwrap();
    let pixel_count = width_usize * usize::try_from(height).unwrap();

    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;

    let mut pixel_buf = vec![0u8; pixel_count];
    for y in 0..height {
        let y_usize = usize::try_from(y).unwrap();
        for x in 0..width {
            let x_usize = usize::try_from(x).unwrap();

            if let Some(b) = map.get(&(x, y)) {
                pixel_buf[(top_margin_usize + y_usize) * width_usize + (left_margin_usize + x_usize)] = *b;
            }
        }
    }

    writer.write_image_data(&pixel_buf)
}
