use std::collections::HashMap;

use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::scale::image::Image;
use swash::shape::ShapeContext;
use swash::zeno::Vector;


const FONT: &[u8] = include_bytes!("../data/texgyreheros-regular.otf");


/// Renders the given text using the built-in font and returns a map of coordinates to pixel
/// intensity values, where higher values are more intense.
///
/// Any pixels not contained in the map can be assumed to be blank (equivalent to intensity value
/// 0).
pub fn render_text(text: &str) -> HashMap<(u32, u32), u8> {
    // load font
    let font = FontRef::from_index(FONT, 0)
        .expect("failed to load font");

    // shape text
    let mut shape_ctx = ShapeContext::new();
    let mut shaper = shape_ctx.builder(font)
        .size(12.0)
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
        .size(12.0)
        .hint(false)
        .build();
    let mut renderer = Render::new(&[
        Source::ColorOutline(0),
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::Outline,
    ]);
    let mut pixel_values: HashMap<(u32, u32), u8> = HashMap::new();
    let mut pos_x: f32 = 0.0;
    let mut images: Vec<(u32, Image)> = Vec::new();
    for glyph in &glyphs {
        let pos_x_int: u32 = pos_x.trunc() as u32;
        let pos_x_frac = pos_x.fract();
        renderer.offset(Vector::new(pos_x_frac, 0.0));
        let img = renderer.render(&mut scaler, glyph.id)
            .expect("failed to render glyph");
        images.push((pos_x_int, img));
        pos_x += glyph.advance;
    }
    let max_top = images.iter()
        .map(|(_pos, m)| m.placement.top)
        .max()
        .unwrap_or(0);
    for (pos_x_int, img) in images {
        for y in 0..img.placement.height {
            for x in 0..img.placement.width {
                let i: usize = (y * img.placement.width + x).try_into().unwrap();
                let b = img.data[i];
                let actual_x: u32 = match (img.placement.left + i32::try_from(pos_x_int + x).unwrap()).try_into() {
                    Ok(ax) => ax,
                    Err(_) => continue,
                };
                let actual_y: u32 = match (max_top - img.placement.top + i32::try_from(y).unwrap()).try_into() {
                    Ok(ay) => ay,
                    Err(_) => continue,
                };
                let pixel_ref = pixel_values
                    .entry((actual_x, actual_y))
                    .or_insert(0);
                *pixel_ref = pixel_ref.saturating_add(b);
            }
        }
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
