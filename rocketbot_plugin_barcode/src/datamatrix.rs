use std::collections::HashSet;
use std::convert::TryInto;

use ::datamatrix::{DataMatrix, SymbolList};

use crate::BarcodeError;


const DATAGRID_SQUARE_SIDE_PIXELS: usize = 4;
const DATAGRID_QUIET_ZONE_PIXELS: usize = 16;


fn bools_to_bytes(bools: &[bool]) -> Vec<u8> {
    let mut ret = Vec::with_capacity(bools.len() / 8 + 1);
    let mut cur_byte = 0u8;

    for (i, b) in bools.iter().enumerate() {
        if *b {
            cur_byte |= 1 << (7 - (i % 8));
        }
        if i % 8 == 7 {
            ret.push(cur_byte);
            cur_byte = 0x00;
        }
    }

    if bools.len() % 8 != 0 {
        // non-full byte at the end
        ret.push(cur_byte);
    }

    ret
}


pub fn datamatrix_string_to_png(string: &str) -> Result<Vec<u8>, BarcodeError> {
    let barcode = DataMatrix::encode_str(&string, SymbolList::default())
        .map_err(|e| BarcodeError::DataMatrixEncoding(e))?;
    let barcode_bitmap = barcode.bitmap();

    // calculate PNG dimensions
    let width = 2 * DATAGRID_QUIET_ZONE_PIXELS + barcode_bitmap.width() * DATAGRID_SQUARE_SIDE_PIXELS;
    let height = 2 * DATAGRID_QUIET_ZONE_PIXELS + barcode_bitmap.height() * DATAGRID_SQUARE_SIDE_PIXELS;

    let width_u32: u32 = width.try_into()
        .map_err(|e| BarcodeError::SizeConversion("width", width, "u32", e))?;
    let height_u32: u32 = height.try_into()
        .map_err(|e| BarcodeError::SizeConversion("height", height, "u32", e))?;

    // encode as an image-like bitmap
    let black_squares: HashSet<(usize, usize)> = barcode_bitmap.pixels().collect();
    let mut bitmap = Vec::new();

    let mut quiet_zone_row = Vec::new();
    for _ in 0..width {
        quiet_zone_row.push(false);
    }

    // start with rows of quiet zone
    for _ in 0..DATAGRID_QUIET_ZONE_PIXELS {
        bitmap.extend_from_slice(&quiet_zone_row);
    }

    for y in 0..barcode_bitmap.height() {
        // assemble the row
        let mut row = Vec::new();

        // pixels of quiet zone
        for _ in 0..DATAGRID_QUIET_ZONE_PIXELS {
            row.push(false);
        }

        for x in 0..barcode_bitmap.width() {
            if black_squares.contains(&(x, y)) {
                for _ in 0..DATAGRID_SQUARE_SIDE_PIXELS {
                    row.push(true);
                }
            } else {
                for _ in 0..DATAGRID_SQUARE_SIDE_PIXELS {
                    row.push(false);
                }
            }
        }

        // pixels of quiet zone
        for _ in 0..DATAGRID_QUIET_ZONE_PIXELS {
            row.push(false);
        }

        // append the row to match the height
        for _ in 0..DATAGRID_SQUARE_SIDE_PIXELS {
            bitmap.extend_from_slice(&row);
        }
    }

    // end with rows of quiet zone
    for _ in 0..DATAGRID_QUIET_ZONE_PIXELS {
        bitmap.extend_from_slice(&quiet_zone_row);
    }

    // pack into bytes
    let bitmap_bytes = bools_to_bytes(&bitmap);

    // encode as PNG
    let mut png = Vec::new();

    {
        let mut png_encoder = png::Encoder::new(&mut png, width_u32, height_u32);
        png_encoder.set_color(png::ColorType::Indexed);
        png_encoder.set_depth(png::BitDepth::One);
        png_encoder.set_palette(vec![0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00]);
        let mut writer = png_encoder.write_header()
            .map_err(|e| BarcodeError::PngEncoding(e))?;
        writer.write_image_data(&bitmap_bytes)
            .map_err(|e| BarcodeError::PngEncoding(e))?;
    }

    Ok(png)
}
