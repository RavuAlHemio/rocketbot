use std::collections::BTreeMap;
use std::sync::LazyLock;

use png;


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ChartColor {
    Background,
    Border,
    Tick,
    TimeSubdivision,
    Text,
    Data(u8),
}
impl ChartColor {
    #[inline]
    pub fn palette_index(&self) -> u8 {
        match self {
            Self::Background => 0,
            Self::Border => 1,
            Self::Tick => 2,
            Self::TimeSubdivision => 3,
            Self::Text => 4,
            Self::Data(d) => d.checked_add(5).unwrap(),
        }
    }
}

pub struct LineGraph {
    width: usize,
    thicken: usize,
    pixels: Vec<ChartColor>,
}
impl LineGraph {
    pub fn width(&self) -> usize { self.width }
    pub fn height(&self) -> usize {
        debug_assert_eq!(self.pixels.len() % self.width, 0);
        self.pixels.len() / self.width
    }

    #[allow(unused)]
    pub fn thicken(&self) -> usize { self.thicken }

    #[allow(unused)]
    pub fn pixels(&self) -> &[ChartColor] { self.pixels.as_slice() }

    fn data_height_with_headroom(max_y_value: usize) -> usize {
        if max_y_value % 100 > 75 {
            // 80 -> 200
            ((max_y_value / 100) + 2) * 100
        } else {
            // 50 -> 100
            ((max_y_value / 100) + 1) * 100
        }
    }

    fn calculate_image_size(x_positions: usize, max_y_value: usize) -> (usize, usize) {
        // 2 = frame width on both edges
        let width = 2 + x_positions;
        let height = 2 + Self::data_height_with_headroom(max_y_value);

        // crash early if the dimensions are too large
        u32::try_from(width).expect("width too large");
        u32::try_from(height).expect("height too large");

        (width, height)
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, color: ChartColor) {
        self.pixels[y * self.width + x] = color;
    }

    pub fn set_pixel_if_in_range(&mut self, x: usize, y: usize, color: ChartColor) {
        if x >= self.width() || y >= self.height() {
            return;
        }
        self.set_pixel(x, y, color);
    }

    pub fn new_for_ranges(x_positions: usize, max_y_value: usize, thicken: usize) -> Self {
        let (width, height) = Self::calculate_image_size(x_positions, max_y_value);
        let pixels = vec![ChartColor::Background; width * height];
        let mut image = Self {
            width,
            thicken,
            pixels,
        };

        // draw ticks
        const HORIZONTAL_TICK_STEP: usize = 100;
        const VERTICAL_TICK_STEP: usize = 100;
        for graph_y in (0..height).step_by(VERTICAL_TICK_STEP) {
            let y = height - (1 + graph_y);
            for x in 1..(width-1) {
                image.set_pixel(x, y, ChartColor::Tick);
            }
        }
        for graph_x in (0..width).step_by(HORIZONTAL_TICK_STEP) {
            let x = 1 + graph_x;
            for y in 1..(height-1) {
                image.set_pixel(x, y, ChartColor::Tick);
            }
        }

        // draw frame
        for y in 0..height {
            image.set_pixel(0, y, ChartColor::Border);
            image.set_pixel(width - 1, y, ChartColor::Border);
        }
        for x in 0..width {
            image.set_pixel(x, 0, ChartColor::Border);
            image.set_pixel(x, height - 1, ChartColor::Border);
        }

        image
    }

    pub fn draw_data_point(&mut self, graph_x: usize, value: usize, color: u8) {
        let x = 1 + graph_x;
        let y = self.height() - (1 + value);
        let pixel_value = ChartColor::Data(color);

        self.set_pixel(x, y, pixel_value);

        for graph_thicker_y in 0..self.thicken {
            let thicker_y_down = y + 1 + graph_thicker_y;
            if thicker_y_down < self.height() {
                self.set_pixel(x, thicker_y_down, pixel_value);
            }

            if let Some(thicker_y_up) = y.checked_sub(1 + graph_thicker_y) {
                self.set_pixel(x, thicker_y_up, pixel_value);
            }
        }
    }

    pub fn draw_time_subdivision(&mut self, graph_x: usize) {
        let x = 1 + graph_x;
        for y in 1..(self.height()-1) {
            self.set_pixel(x, y, ChartColor::TimeSubdivision);
        }
    }

    pub fn draw_string(&mut self, mut x: usize, y: usize, text: &str) {
        for c in text.chars() {
            let pixel_slice = DIGIT_FONT
                .get(&c).unwrap_or(&DIGIT_FONT_REPLACEMENT_CHARACTER);
            for pixel in *pixel_slice {
                if x >= self.width() {
                    // enough
                    break;
                }

                for y_offset in 0..8 {
                    if *pixel & (1 << y_offset) != 0 {
                        self.set_pixel_if_in_range(x, y + y_offset, ChartColor::Text);
                    }
                }

                x += 1;
            }
        }
    }

    pub fn to_png(&self) -> Vec<u8> {
        let palette: Vec<u8> = GRAPH_BACKGROUND_COLOR.into_iter()
            .chain(GRAPH_BORDER_COLOR.into_iter())
            .chain(GRAPH_TICK_COLOR.into_iter())
            .chain(GRAPH_TIME_SUBDIVISION_COLOR.into_iter())
            .chain(GRAPH_TEXT_COLOR.into_iter())
            .chain(GRAPH_COLORS.into_iter().flat_map(|cs| cs))
            .collect();
        let mut png_bytes: Vec<u8> = Vec::new();

        let width_u32 = self.width().try_into().unwrap();
        let height_u32 = self.height().try_into().unwrap();

        {
            let mut png_encoder = png::Encoder::new(&mut png_bytes, width_u32, height_u32);
            png_encoder.set_color(png::ColorType::Indexed);
            png_encoder.set_palette(palette);

            let mut png_writer = png_encoder.write_header().expect("failed to write PNG header");
            let mut png_data = Vec::with_capacity(self.pixels.len());
            png_data.extend(self.pixels.iter().map(|p| p.palette_index()));
            png_writer.write_image_data(&png_data).expect("failed to write image data");
        }

        png_bytes
    }
}


pub(crate) const GRAPH_COLORS: [[u8; 3]; 30] = [
    // DawnBringer DB32 palette without black and white
    [0x63, 0x9b, 0xff], // #639bff
    [0xac, 0x32, 0x32], // #ac3232
    [0xdf, 0x71, 0x26], // #df7126
    [0xfb, 0xf2, 0x36], // #fbf236
    [0x99, 0xe5, 0x50], // #99e550
    [0x76, 0x42, 0x8a], // #76428a

    [0x5b, 0x6e, 0xe1], // #5b6ee1
    [0xd9, 0x57, 0x63], // #d95763
    [0xd9, 0xa0, 0x66], // #d9a066
    [0x8f, 0x97, 0x4a], // #8f974a
    [0x6a, 0xbe, 0x30], // #6abe30
    [0x3f, 0x3f, 0x74], // #3f3f74

    [0x30, 0x60, 0x82], // #306082
    [0x8f, 0x56, 0x3b], // #8f563b
    [0xee, 0xc3, 0x9a], // #eec39a
    [0x8a, 0x6f, 0x30], // #8a6f30
    [0x37, 0x94, 0x6e], // #37946e
    [0xd7, 0x7b, 0xba], // #d77bba

    [0x5f, 0xcd, 0xe4], // #5fcde4
    [0x66, 0x39, 0x31], // #663931
    [0x52, 0x4b, 0x24], // #524b24
    [0xcb, 0xdb, 0xfc], // #cbdbfc
    [0x4b, 0x69, 0x2f], // #4b692f
    [0x45, 0x28, 0x3c], // #45283c

    [0x22, 0x20, 0x34], // #222034
    [0x59, 0x56, 0x52], // #595652
    [0x84, 0x7e, 0x87], // #847e87
    [0x9b, 0xad, 0xb7], // #9badb7
    [0x32, 0x3c, 0x39], // #323c39
    [0x69, 0x6a, 0x6a], // #696a6a
];
const GRAPH_BORDER_COLOR: [u8; 3] = [0, 0, 0]; // #000000
const GRAPH_BACKGROUND_COLOR: [u8; 3] = [255, 255, 255]; // #ffffff
const GRAPH_TICK_COLOR: [u8; 3] = [221, 221, 221]; // #dddddd
const GRAPH_TIME_SUBDIVISION_COLOR: [u8; 3] = [136, 136, 136]; // #888888
const GRAPH_TEXT_COLOR: [u8; 3] = [136, 136, 136]; // #888888


static DIGIT_FONT: LazyLock<BTreeMap<char, &'static [u8]>> = LazyLock::new(|| {
    let mut font: BTreeMap<char, &'static [u8]> = BTreeMap::new();

    // encoding is column by column; each byte represents one column
    // LSB is the topmost pixel, LSB-but-one is the pixel below it, etc.
    // if a bit is 1, the font has a pixel there; if it is 0, there is none

    font.insert('1', &[0b00010, 0b11111, 0b00000]);
    font.insert('2', &[0b11101, 0b10101, 0b10111, 0b00000]);
    font.insert('3', &[0b10001, 0b10101, 0b11111, 0b00000]);
    font.insert('4', &[0b00111, 0b00100, 0b11111, 0b00000]);
    font.insert('5', &[0b10111, 0b10101, 0b11101, 0b00000]);
    font.insert('6', &[0b11111, 0b10101, 0b11101, 0b00000]);
    font.insert('7', &[0b00001, 0b11001, 0b00111, 0b00000]);
    font.insert('8', &[0b11111, 0b10101, 0b11111, 0b00000]);
    font.insert('9', &[0b10111, 0b10101, 0b11111, 0b00000]);
    font.insert('0', &[0b11111, 0b10001, 0b11111, 0b00000]);
    font.insert('/', &[0b10000, 0b01100, 0b00010, 0b00001, 0b00000]);

    font
});
const DIGIT_FONT_REPLACEMENT_CHARACTER: &'static [u8] = &[0b11111, 0b01010, 0b11000, 0b11111, 0b00000];
