use std::convert::TryInto;
use std::fmt;
use std::num::TryFromIntError;


#[derive(Debug)]
pub enum BitmapError {
    IncorrectPixelCount { expected: usize, obtained: usize },
    DimensionConversion { dimension: &'static str, value: usize, target_type: &'static str, error: TryFromIntError },
    PngEncoding(png::EncodingError),
}
impl fmt::Display for BitmapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncorrectPixelCount { expected, obtained }
                => write!(f, "incorrect pixel count; expected {}, obtained {}", expected, obtained),
            Self::DimensionConversion { dimension, value, target_type, error }
                => write!(f, "failed to convert {} ({}) to {}: {}", dimension, value, target_type, error),
            Self::PngEncoding(e)
                => write!(f, "PNG encoding failed: {}", e),
        }
    }
}
impl std::error::Error for BitmapError {
}


pub struct BitmapRenderOptions {
    quiet_top_pixels: usize,
    quiet_left_pixels: usize,
    quiet_right_pixels: usize,
    quiet_bottom_pixels: usize,
    point_width_pixels: usize,
    point_height_pixels: usize,
}
impl BitmapRenderOptions {
    pub fn new() -> Self {
        Self {
            quiet_top_pixels: 16,
            quiet_left_pixels: 16,
            quiet_right_pixels: 16,
            quiet_bottom_pixels: 16,
            point_width_pixels: 4,
            point_height_pixels: 4,
        }
    }

    pub fn set_quiet_all(&mut self, new_quiet_pixels: usize) {
        self.quiet_top_pixels = new_quiet_pixels;
        self.quiet_left_pixels = new_quiet_pixels;
        self.quiet_right_pixels = new_quiet_pixels;
        self.quiet_bottom_pixels = new_quiet_pixels;
    }

    pub fn set_quiet(
        &mut self,
        quiet_top_pixels: usize,
        quiet_left_pixels: usize,
        quiet_right_pixels: usize,
        quiet_bottom_pixels: usize,
    ) {
        self.quiet_top_pixels = quiet_top_pixels;
        self.quiet_left_pixels = quiet_left_pixels;
        self.quiet_right_pixels = quiet_right_pixels;
        self.quiet_bottom_pixels = quiet_bottom_pixels;
    }

    pub fn set_point_dimensions_all(&mut self, new_dim_pixels: usize) {
        self.point_width_pixels = new_dim_pixels;
        self.point_height_pixels = new_dim_pixels;
    }

    pub fn set_point_dimensions(&mut self, point_width_pixels: usize, point_height_pixels: usize) {
        self.point_width_pixels = point_width_pixels;
        self.point_height_pixels = point_height_pixels;
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BarcodeBitmap {
    width: usize,
    height: usize,
    bits: Vec<bool>,
}
impl BarcodeBitmap {
    pub fn new(
        width: usize,
        height: usize,
        bits: Vec<bool>,
    ) -> Result<Self, BitmapError> {
        if width * height != bits.len() {
            return Err(BitmapError::IncorrectPixelCount { expected: width * height, obtained: bits.len() });
        }
        Ok(Self {
            width,
            height,
            bits,
        })
    }
    pub fn width(&self) -> usize { self.width }
    pub fn height(&self) -> usize { self.height }
    pub fn bits(&self) -> &[bool] { self.bits.as_slice() }

    /// Converts the bitmap from bits to bytes. Bits within a byte are ordered depending on the
    /// `most_significant_first` argument: if `true`, they are ordered from most significant to
    /// least significant (as expected e.g. by PNG); if `false`, they are ordered from least to most
    /// significant. If the total number of bits is not divisible by 8, the remaining bits in the
    /// last byte are set to 0. This padding either happens once per row (if `pad_by_row` is `true`,
    /// as expected by PNG) or at the end of the whole image (if it is `false`).
    pub fn to_bytes(&self, most_significant_first: bool, pad_by_row: bool) -> Vec<u8> {
        let mut ret = Vec::with_capacity(self.bits.len() / 8 + 1);
        let mut cur_byte = 0u8;

        let mut bit_index = 0;
        for (i, b) in self.bits.iter().enumerate() {
            if *b {
                if most_significant_first {
                    cur_byte |= 1 << (7 - bit_index);
                } else {
                    cur_byte |= 1 << bit_index;
                }
            }
            if bit_index == 7 {
                ret.push(cur_byte);
                cur_byte = 0x00;
                bit_index = 0;
            } else if pad_by_row && i % self.width == (self.width - 1) {
                // last byte in the row => pad and flush
                ret.push(cur_byte);
                cur_byte = 0x00;
                bit_index = 0;
            } else {
                bit_index += 1;
            }
        }

        if !pad_by_row && self.bits.len() % 8 != 0 {
            // non-full byte at the end
            ret.push(cur_byte);
        }

        ret
    }

    pub fn render(&self, bitmap_opts: &BitmapRenderOptions) -> BarcodeBitmap {
        let new_width = bitmap_opts.quiet_left_pixels + bitmap_opts.point_width_pixels * self.width + bitmap_opts.quiet_right_pixels;
        let new_height = bitmap_opts.quiet_top_pixels + bitmap_opts.point_height_pixels * self.height + bitmap_opts.quiet_bottom_pixels;

        let quiet_row = vec![false; new_width];
        let mut rendered_pixels = Vec::with_capacity(new_width * new_height);

        // quiet rows at the top
        for _ in 0..bitmap_opts.quiet_top_pixels {
            rendered_pixels.extend_from_slice(&quiet_row);
        }

        // for each actual row
        for y in 0..self.height {
            let row_start = y * self.width;

            let mut this_row = Vec::with_capacity(new_width);

            // left-side quiet pixels
            for _ in 0..bitmap_opts.quiet_left_pixels {
                this_row.push(false);
            }

            // actual barcode pixels
            for x in 0..self.width {
                for _ in 0..bitmap_opts.point_width_pixels {
                    this_row.push(self.bits[row_start + x]);
                }
            }

            // right-side quiet pixels
            for _ in 0..bitmap_opts.quiet_right_pixels {
                this_row.push(false);
            }

            // now, output this row as many times as needed (point height)
            for _ in 0..bitmap_opts.point_height_pixels {
                rendered_pixels.extend_from_slice(&this_row);
            }
        }

        // quiet rows at the bottom
        for _ in 0..bitmap_opts.quiet_bottom_pixels {
            rendered_pixels.extend_from_slice(&quiet_row);
        }

        BarcodeBitmap::new(
            new_width,
            new_height,
            rendered_pixels,
        ).expect("rendered bitmap is invalid?!")
    }

    pub fn to_png(&self) -> Result<Vec<u8>, BitmapError> {
        // encode as PNG
        let mut png = Vec::new();

        let width_u32 = self.width.try_into()
            .map_err(|e| BitmapError::DimensionConversion { dimension: "width", value: self.width, target_type: "u32", error: e })?;
        let height_u32 = self.height.try_into()
            .map_err(|e| BitmapError::DimensionConversion { dimension: "height", value: self.height, target_type: "u32", error: e })?;

        let bitmap_bytes = self.to_bytes(true, true);

        {
            let mut png_encoder = png::Encoder::new(&mut png, width_u32, height_u32);
            png_encoder.set_color(png::ColorType::Indexed);
            png_encoder.set_depth(png::BitDepth::One);
            png_encoder.set_palette(vec![0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00]);
            let mut writer = png_encoder.write_header()
                .map_err(|e| BitmapError::PngEncoding(e))?;
            writer.write_image_data(&bitmap_bytes)
                .map_err(|e| BitmapError::PngEncoding(e))?;
        }

        Ok(png)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_bytes_1x1() {
        let bmp = BarcodeBitmap::new(1, 1, vec![true]).unwrap();

        let bs_msb = bmp.to_bytes(true, false);
        assert_eq!(vec![0x80], bs_msb);

        let bs_lsb = bmp.to_bytes(false, false);
        assert_eq!(vec![0x01], bs_lsb);
    }

    #[test]
    fn test_to_bytes_3x3() {
        let bmp = BarcodeBitmap::new(
            3, 3, vec![
                true, false, true,
                false, true, false,
                true, false, true,
            ],
        ).unwrap();

        let bs_msb = bmp.to_bytes(true, false);
        assert_eq!(vec![0xaa, 0x80], bs_msb);

        let bs_lsb = bmp.to_bytes(false, false);
        assert_eq!(vec![0x55, 0x01], bs_lsb);
    }

    #[test]
    fn test_to_bytes_3x3_row_pad() {
        let bmp = BarcodeBitmap::new(
            3, 3, vec![
                true, false, true,
                false, true, false,
                true, false, true,
            ],
        ).unwrap();

        let bs_msb = bmp.to_bytes(true, true);
        assert_eq!(vec![0xa0, 0x40, 0xa0], bs_msb);

        let bs_lsb = bmp.to_bytes(false, true);
        assert_eq!(vec![0x05, 0x02, 0x05], bs_lsb);
    }
}
