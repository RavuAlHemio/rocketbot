use std::env::args;
use std::fs::File;
use std::io::{Cursor, Write};

use png;
use rocketbot_render_text::{DEFAULT_FONT_DATA, DEFAULT_SIZE_PX, map_to_dimensions, TextRenderer};


fn main() {
    let args: Vec<String> = args().collect();

    if args.len() != 3 || args[1] == "--help" {
        eprintln!("Usage: text_to_png TEXT OUTFILE");
        return;
    }

    let renderer = TextRenderer::new(DEFAULT_FONT_DATA, DEFAULT_SIZE_PX)
        .expect("failed to load default font");

    let text = renderer.render_text(&args[1]);
    let (width, height) = map_to_dimensions(&text);
    let width_usize: usize = width.try_into().unwrap();
    let height_usize: usize = height.try_into().unwrap();
    let byte_count = width_usize * height_usize;

    let mut png_buf = Vec::new();
    {
        let png_cur = Cursor::new(&mut png_buf);
        let mut encoder = png::Encoder::new(png_cur, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()
            .expect("failed to write PNG header");

        let mut image_data: Vec<u8> = vec![0; byte_count];
        for y in 0..height {
            let y_usize: usize = y.try_into().unwrap();
            for x in 0..width {
                let x_usize: usize = x.try_into().unwrap();
                let b = text.get(&(x, y)).map(|b| *b).unwrap_or(0);
                image_data[y_usize * width_usize + x_usize] = b;
            }
        }
        writer.write_image_data(&image_data)
            .expect("failed to write image data");
    }

    {
        let mut f = File::create(&args[2])
            .expect("failed to open output PNG file");
        f.write_all(&png_buf)
            .expect("failed to write output PNG file");
    }
}
