use std::f64::consts::PI;
use std::io::Write;


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Pixel {
    Background,
    ZeroLine,
    AngleLine,
    Fill,
}
impl Default for Pixel {
    fn default() -> Self { Self::Background }
}


pub struct SliceImage {
    width: usize,
    pixels: Vec<Pixel>,
}
impl SliceImage {
    pub fn new(width: usize, height: usize) -> Self {
        let pixels = vec![Pixel::default(); width * height];
        Self {
            width,
            pixels,
        }
    }

    pub fn width(&self) -> usize { self.width }
    pub fn height(&self) -> usize { self.pixels.len() / self.width }

    #[inline]
    fn get_pixel(&self, x: usize, y: usize) -> Pixel {
        self.pixels[y * self.width + x]
    }

    #[inline]
    fn set_pixel(&mut self, x: usize, y: usize, value: Pixel) {
        self.pixels[y * self.width + x] = value;
    }

    pub fn draw_angle(&mut self, radians: f64) {
        if !radians.is_finite() {
            return;
        }

        // start by drawing the zero line
        let y0 = self.height() / 2;
        for x0 in self.width()/2..=self.width() {
            self.set_pixel(x0, y0, Pixel::ZeroLine);
        }

        // calculate ascension
        let (delta_y, delta_x) = radians.sin_cos();

        let width_f64 = self.width() as f64;
        let height_f64 = self.height() as f64;

        let mut x_f64 = width_f64 / 2.0;
        let mut y_f64 = height_f64 / 2.0;
        loop {
            let x_rounded = x_f64.round();
            if x_rounded < 0.0 || x_rounded >= width_f64 {
                break;
            }
            let y_rounded = y_f64.round();
            if y_rounded < 0.0 || y_rounded >= height_f64 {
                break;
            }

            let x = x_rounded as usize;
            let y = y_rounded as usize;
            self.set_pixel(x, y, Pixel::AngleLine);

            x_f64 += delta_x;
            // GUI coordinate system: +Y is down, not up
            y_f64 -= delta_y;
        }

        // floodfill?
        // check rightmost pixel above zero line; if it's also a line, the angle is too small to fill
        // exception: if the angle is negative, check rightmost pixel below zero line
        let start_pixel_y = if radians >= 0.0 {
            self.height() / 2 - 1
        } else {
            self.height() / 2 + 1
        };
        if self.get_pixel(self.width() - 1, start_pixel_y) == Pixel::Background {
            // good, we can floodfill, starting there
            let mut fill_stack = Vec::new();
            fill_stack.push((self.width() - 1, start_pixel_y));
            while let Some((x, y)) = fill_stack.pop() {
                if self.get_pixel(x, y) != Pixel::Background {
                    continue;
                }

                self.set_pixel(x, y, Pixel::Fill);

                // left
                if x > 0 {
                    fill_stack.push((x - 1, y));
                }

                // right
                if x < self.width() - 1 {
                    fill_stack.push((x + 1, y));
                }

                // up
                if y > 0 {
                    fill_stack.push((x, y - 1));
                }

                // down
                if y < self.height() - 1 {
                    fill_stack.push((x, y + 1));
                }
            }
        }
    }

    pub fn draw_angle_deg(&mut self, degrees: f64) {
        self.draw_angle(degrees * PI / 180.0)
    }

    pub fn to_png<W: Write>(&self, w: W) {
        let mut encoder = png::Encoder::new(
            w,
            self.width().try_into().unwrap(),
            self.height().try_into().unwrap(),
        );
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()
            .expect("encoding error");

        let image_data: Vec<u8> = self.pixels.iter()
            .flat_map(|px| match px {
                Pixel::Background => [255, 255, 255],
                Pixel::ZeroLine => [0, 0, 0],
                Pixel::AngleLine => [0, 0, 0],
                Pixel::Fill => [0, 255, 0],
            })
            .collect();
        writer.write_image_data(&image_data)
            .expect("image data writing error");
    }
}
