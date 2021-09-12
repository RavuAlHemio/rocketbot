use std::collections::HashSet;

use ::datamatrix::{DataMatrix, SymbolList};

use crate::BarcodeError;
use crate::bitmap::BarcodeBitmap;


pub fn datamatrix_string_to_bitmap(string: &str) -> Result<BarcodeBitmap, BarcodeError> {
    let barcode = DataMatrix::encode_str(&string, SymbolList::default())
        .map_err(|e| BarcodeError::DataMatrixEncoding(e))?;
    let barcode_bitmap = barcode.bitmap();

    let black_squares: HashSet<(usize, usize)> = barcode_bitmap.pixels().collect();
    let mut bitmap_pixels = Vec::with_capacity(barcode_bitmap.width() * barcode_bitmap.height());

    for y in 0..barcode_bitmap.height() {
        for x in 0..barcode_bitmap.width() {
            bitmap_pixels.push(black_squares.contains(&(x, y)));
        }
    }

    Ok(BarcodeBitmap::new(
        barcode_bitmap.width(),
        barcode_bitmap.height(),
        bitmap_pixels,
    ).expect("invalid Data Matrix bitmap"))
}
